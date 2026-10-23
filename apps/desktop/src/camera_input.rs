//! Host-only Tao bridge: camera gestures and atomic native transform transactions.
use super::{
    Shared,
    camera::{State, View},
    transform::{self, Axis, Tool},
    workspace::Workspace,
};
use tauri::{Emitter, EventLoopMessage, Manager};
use tauri_runtime_wry::tao::{
    event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoopProxy, EventLoopWindowTarget},
    keyboard::{Key, ModifiersState},
};
use tauri_runtime_wry::{
    Context, EventLoopIterationContext, Message, Plugin, PluginBuilder, WebContextStore,
};
pub struct Bridge {
    pub app: tauri::AppHandle,
}
pub struct Input {
    app: tauri::AppHandle,
    position: Option<[f64; 2]>,
    label: String,
    modifiers: ModifiersState,
    gesture: Option<Gesture>,
    object_drag: Option<transform::Drag>,
    pending: Option<atom_core::Transform>,
    dirty: bool,
}
struct Gesture {
    origin: [f64; 2],
    last: [f64; 2],
    before: View,
    button: MouseButton,
    pan: bool,
    orbit: bool,
    moved: bool,
    layout: u64,
}
impl PluginBuilder<EventLoopMessage> for Bridge {
    type Plugin = Input;
    fn build(self, _: Context<EventLoopMessage>) -> Input {
        Input {
            app: self.app,
            position: None,
            label: String::new(),
            modifiers: ModifiersState::default(),
            gesture: None,
            object_drag: None,
            pending: None,
            dirty: false,
        }
    }
}
impl Input {
    fn update(&mut self, action: impl FnOnce(&mut View)) {
        let state = self.app.state::<State>();
        let mut view = state.0.lock().expect("camera");
        action(&mut view);
        view.revision += 1;
        self.dirty = true;
    }
    fn edited(&mut self, result: atom_core::Result<atom_core::Snapshot>) {
        match result {
            Ok(snapshot) => {
                transform::publish(&self.app, &snapshot);
                self.dirty = true;
            }
            Err(e) => {
                eprintln!("ATOM transform: {e}");
                let _ = self.app.emit("atom://panel-error", e.code());
            }
        }
    }
    fn cancel_object(&mut self) {
        self.pending = None;
        self.object_drag = None;
        let result = {
            let shared = self.app.state::<Shared>();
            let mut session = shared.0.lock().expect("session");
            session
                .core
                .preview_owned_by(transform::NATIVE_OWNER)
                .then(|| session.core.cancel_transform(transform::NATIVE_OWNER))
        };
        if let Some(result) = result {
            self.edited(result);
        }
    }
    fn flush_object(&mut self) {
        let Some(pose) = self.pending.take() else {
            return;
        };
        let Some(d) = &self.object_drag else {
            return;
        };
        let result = {
            let shared = self.app.state::<Shared>();
            let mut session = shared.0.lock().expect("session");
            session
                .core
                .preview_transform(transform::NATIVE_OWNER, d.revision, d.project, pose)
        };
        if result.is_err() {
            self.cancel_object();
        }
        self.edited(result);
    }
    fn commit_object(&mut self) {
        self.flush_object();
        let Some(d) = self.object_drag.take() else {
            return;
        };
        let result = {
            let shared = self.app.state::<Shared>();
            let mut session = shared.0.lock().expect("session");
            session
                .core
                .commit_transform(transform::NATIVE_OWNER, d.revision, d.project)
        };
        if result.is_err() {
            self.cancel_object();
        }
        self.edited(result);
    }
    fn focus(&mut self) {
        let shared = self.app.state::<Shared>();
        let session = shared.0.lock().expect("session");
        let state = self.app.state::<State>();
        let mut view = state.0.lock().expect("camera");
        if let Some(o) = view
            .selected
            .and_then(|id| super::camera::focus_target(&session.core, id))
        {
            view.focus(&o, super::viewport::aspect(&self.app));
            view.revision += 1;
            self.dirty = true;
        }
    }
    fn history(&mut self, redo: bool) {
        if self.object_drag.is_some() {
            self.cancel_object();
            return;
        }
        let result = {
            let shared = self.app.state::<Shared>();
            let mut session = shared.0.lock().expect("session");
            let snapshot = session.core.snapshot();
            if (redo && !snapshot.can_redo) || (!redo && !snapshot.can_undo) {
                return;
            }
            session.core.apply(atom_core::EditRequest {
                expected_revision: snapshot.revision,
                project_id: snapshot.project_id,
                command: if redo {
                    atom_core::Command::Redo
                } else {
                    atom_core::Command::Undo
                },
            })
        };
        self.edited(result);
    }
    fn begin_object(&mut self, view: &View, region: [u32; 4], scale: f64, point: [f64; 2]) -> bool {
        if view.tool == Tool::Camera {
            return false;
        }
        let Some(id) = view.selected else {
            return false;
        };
        let shared = self.app.state::<Shared>();
        let mut session = shared.0.lock().expect("session");
        let Some(object) = session.core.object(id).cloned() else {
            return false;
        };
        let hit = transform::hit_axis(view, &object, region, scale, point);
        let picked = view.pick(
            &session.core.snapshot().objects,
            [
                (point[0] - region[0] as f64) as f32 / region[2] as f32,
                (point[1] - region[1] as f64) as f32 / region[3] as f32,
            ],
            region[2] as f32 / region[3] as f32,
        );
        if hit.is_none() && picked != Some(id) {
            return false;
        }
        let axis = hit.unwrap_or(view.axis);
        let snapshot = session.core.snapshot();
        let result = session.core.begin_transform(
            transform::NATIVE_OWNER,
            snapshot.revision,
            snapshot.project_id,
            id,
        );
        drop(session);
        if result.is_ok() {
            let mut frozen = view.clone();
            frozen.axis = axis;
            self.object_drag = Some(transform::Drag::new(
                &object,
                snapshot.project_id,
                snapshot.revision,
                self.app.state::<Workspace>().snapshot().revision,
                frozen,
                region,
                scale,
                point,
                axis,
            ));
            self.update(|view| view.axis = axis);
        }
        self.edited(result);
        true
    }
}
impl Plugin<EventLoopMessage> for Input {
    fn on_event(
        &mut self,
        event: &Event<Message<EventLoopMessage>>,
        _: &EventLoopWindowTarget<Message<EventLoopMessage>>,
        _: &EventLoopProxy<Message<EventLoopMessage>>,
        _: &mut ControlFlow,
        context: EventLoopIterationContext<'_, EventLoopMessage>,
        _: &WebContextStore,
    ) -> bool {
        if matches!(event, Event::MainEventsCleared) {
            let layout = self.app.state::<Workspace>().snapshot().revision;
            let view = self.app.state::<State>().0.lock().expect("camera").clone();
            let owns_preview = self.object_drag.is_some()
                && self
                    .app
                    .state::<Shared>()
                    .0
                    .lock()
                    .expect("session")
                    .core
                    .preview_owned_by(transform::NATIVE_OWNER);
            if self.object_drag.as_ref().is_some_and(|d| {
                !owns_preview
                    || d.layout != layout
                    || view.selected != Some(d.id)
                    || d.tool != view.tool
                    || d.axis != view.axis
            }) {
                self.cancel_object();
                self.position = None;
            }
            if self.gesture.as_ref().is_some_and(|g| g.layout != layout) {
                self.gesture = None;
                self.position = None;
            }
            self.flush_object();
            if std::mem::take(&mut self.dirty) {
                super::camera::notify(&self.app);
                super::viewport::draw(&self.app);
            }
            return false;
        }
        let Event::WindowEvent {
            window_id, event, ..
        } = event
        else {
            return false;
        };
        let label = context.window_id_map.get(window_id).and_then(|id| {
            context
                .windows
                .0
                .borrow()
                .get(&id)
                .map(|w| w.label().to_owned())
        });
        let Some(label) = label else {
            return false;
        };
        if !matches!(
            label.as_str(),
            "main" | "float-scene" | "float-workflow" | "float-viewport"
        ) {
            return false;
        }
        if label != self.label {
            self.cancel_object();
            self.gesture = None;
            self.position = None;
            self.label = label.clone();
        }
        let Some(window) = self.app.get_window(&label) else {
            return false;
        };
        let scale = window.scale_factor().unwrap_or(1.);
        let region = super::viewport::region(&self.app, &label, true);
        match event {
            WindowEvent::ModifiersChanged(m) => self.modifiers = *m,
            WindowEvent::CursorMoved { position, .. } => {
                let point = [position.x, position.y];
                let inside = region.is_some_and(|r| {
                    point[0] >= r[0] as f64
                        && point[1] >= r[1] as f64
                        && point[0] < (r[0] + r[2]) as f64
                        && point[1] < (r[1] + r[3]) as f64
                });
                if !inside {
                    self.cancel_object();
                    self.position = None;
                    self.gesture = None;
                    return false;
                }
                self.position = Some(point);
                if let Some(d) = &mut self.object_drag {
                    if d.layout != self.app.state::<Workspace>().snapshot().revision {
                        self.cancel_object();
                        return false;
                    }
                    if d.moved
                        || (point[0] - d.origin[0]).hypot(point[1] - d.origin[1]) / d.scale >= 3.
                    {
                        d.moved = true;
                        self.pending = Some(d.pose(point));
                    }
                    return false;
                }
                if let Some(g) = &mut self.gesture {
                    if !g.moved
                        && (point[0] - g.origin[0]).hypot(point[1] - g.origin[1]) / scale < 4.
                    {
                        return false;
                    }
                    g.moved = true;
                    let delta = [
                        ((point[0] - g.last[0]) / scale) as f32,
                        ((point[1] - g.last[1]) / scale) as f32,
                    ];
                    g.last = point;
                    let pan = g.pan;
                    let orbit = g.orbit;
                    let height = region.unwrap()[3] as f32 / scale as f32;
                    if pan || orbit {
                        self.update(|view| {
                            if pan {
                                view.pan(delta, height)
                            } else {
                                view.orbit(delta)
                            }
                        });
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if !matches!(
                    button,
                    MouseButton::Left | MouseButton::Middle | MouseButton::Right
                ) {
                    return false;
                }
                if self.object_drag.is_some() {
                    if *button == MouseButton::Left && *state == ElementState::Released {
                        self.commit_object();
                    }
                    return false;
                }
                if *state == ElementState::Pressed && self.gesture.is_none() {
                    if let (Some(point), Some(r)) = (self.position, region) {
                        if self
                            .app
                            .state::<Shared>()
                            .0
                            .lock()
                            .expect("session")
                            .core
                            .has_transform_preview()
                        {
                            return false;
                        }
                        let before = self.app.state::<State>().0.lock().expect("camera").clone();
                        let pan = before.pan_mode
                            || *button != MouseButton::Left
                            || self.modifiers.shift_key();
                        let alt = self.modifiers.alt_key();
                        if *button == MouseButton::Left
                            && !pan
                            && !alt
                            && self.begin_object(&before, r, scale, point)
                        {
                            return false;
                        }
                        self.gesture = Some(Gesture {
                            origin: point,
                            last: point,
                            before: before.clone(),
                            button: *button,
                            pan,
                            orbit: before.tool == Tool::Camera || alt,
                            moved: false,
                            layout: self.app.state::<Workspace>().snapshot().revision,
                        });
                    }
                } else if *state == ElementState::Released
                    && self.gesture.as_ref().is_some_and(|g| g.button == *button)
                {
                    let g = self.gesture.take().unwrap();
                    if !g.moved
                        && *button == MouseButton::Left
                        && !g.pan
                        && let (Some(p), Some(r)) = (self.position, region)
                    {
                        let shared = self.app.state::<Shared>();
                        let session = shared.0.lock().expect("session");
                        if session.core.has_transform_preview() {
                            return false;
                        }
                        let state = self.app.state::<State>();
                        let mut view = state.0.lock().expect("camera");
                        view.selected = view.pick(
                            &session.core.snapshot().objects,
                            [
                                (p[0] - r[0] as f64) as f32 / r[2] as f32,
                                (p[1] - r[1] as f64) as f32 / r[3] as f32,
                            ],
                            r[2] as f32 / r[3] as f32,
                        );
                        view.revision += 1;
                        self.dirty = true;
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. }
                if self.position.is_some()
                    && region.is_some()
                    && self.gesture.is_none()
                    && self.object_drag.is_none() =>
            {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -*y * 40.,
                    MouseScrollDelta::PixelDelta(p) => -(p.y / scale) as f32,
                    _ => return false,
                };
                self.update(|view| view.zoom(dy));
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if event.logical_key == Key::Escape {
                    self.cancel_object();
                    if let Some(g) = self.gesture.take() {
                        self.update(|view| {
                            let revision = view.revision;
                            *view = g.before;
                            view.revision = revision;
                        });
                    }
                } else if self.position.is_some()
                    && region.is_some()
                    && self.gesture.is_none()
                    && let Key::Character(key) = &event.logical_key
                {
                    let key = key.to_lowercase();
                    if (self.modifiers.super_key() || self.modifiers.control_key()) && key == "z" {
                        self.history(self.modifiers.shift_key());
                        return false;
                    }
                    if self.modifiers.super_key()
                        || self.modifiers.control_key()
                        || self.modifiers.alt_key()
                    {
                        return false;
                    }
                    if matches!(key.as_str(), "v" | "w" | "e" | "r" | "x" | "y" | "z") {
                        self.cancel_object();
                        self.update(|view| match key.as_str() {
                            "v" => {
                                view.tool = Tool::Camera;
                                view.pan_mode = false;
                            }
                            "w" => {
                                view.tool = Tool::Move;
                                view.pan_mode = false;
                            }
                            "e" => {
                                view.tool = Tool::Rotate;
                                view.pan_mode = false;
                            }
                            "r" => {
                                view.tool = Tool::Scale;
                                view.pan_mode = false;
                            }
                            "x" => {
                                view.axis = if view.axis == Axis::X {
                                    Axis::Free
                                } else {
                                    Axis::X
                                }
                            }
                            "y" => {
                                view.axis = if view.axis == Axis::Y {
                                    Axis::Free
                                } else {
                                    Axis::Y
                                }
                            }
                            "z" => {
                                view.axis = if view.axis == Axis::Z {
                                    Axis::Free
                                } else {
                                    Axis::Z
                                }
                            }
                            _ => {}
                        });
                    } else if self.object_drag.is_none() {
                        match key.as_str() {
                            "f" => self.focus(),
                            "0" => self.update(|view| {
                                let selected = view.selected;
                                let revision = view.revision;
                                *view = View {
                                    selected,
                                    revision,
                                    tool: view.tool,
                                    axis: view.axis,
                                    show_grid: view.show_grid,
                                    show_axes: view.show_axes,
                                    ..Default::default()
                                };
                            }),
                            _ => {}
                        }
                    }
                }
            }
            WindowEvent::Focused(false)
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::Resized(_)
            | WindowEvent::ScaleFactorChanged { .. } => {
                self.cancel_object();
                self.gesture = None;
                self.position = None;
                self.modifiers = ModifiersState::default();
            }
            _ => {}
        }
        false
    }
}
