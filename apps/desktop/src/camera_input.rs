//! ATOM native camera input, following UNGE's host-only Tao plugin boundary.
use super::{
    Shared,
    camera::{State, View},
    workspace::Workspace,
};
use tauri::{EventLoopMessage, Manager};
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
    dirty: bool,
}
struct Gesture {
    origin: [f64; 2],
    last: [f64; 2],
    before: View,
    button: MouseButton,
    pan: bool,
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
            dirty: false,
        }
    }
}
impl Input {
    fn update(&mut self, action: impl FnOnce(&mut View)) {
        let state = self.app.state::<State>();
        let mut camera = state.0.lock().expect("camera");
        action(&mut camera);
        camera.revision += 1;
        self.dirty = true;
    }
    fn focus(&mut self) {
        let shared = self.app.state::<Shared>();
        let session = shared.0.lock().expect("session");
        let state = self.app.state::<State>();
        let mut camera = state.0.lock().expect("camera");
        if let Some(o) = session
            .core
            .project()
            .objects
            .iter()
            .find(|o| Some(o.id) == camera.selected)
        {
            camera.focus(o, super::viewport::aspect(&self.app));
            camera.revision += 1;
            self.dirty = true;
        }
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
            if self
                .gesture
                .as_ref()
                .is_some_and(|g| g.layout != self.app.state::<Workspace>().snapshot().revision)
            {
                self.gesture = None;
                self.position = None;
            }
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
                    self.position = None;
                    self.gesture = None;
                    return false;
                }
                self.position = Some(point);
                if let Some(g) = &mut self.gesture {
                    if g.layout != self.app.state::<Workspace>().snapshot().revision {
                        self.gesture = None;
                        return false;
                    }
                    if !g.moved
                        && ((point[0] - g.origin[0]).hypot(point[1] - g.origin[1]) / scale) < 4.
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
                    let height = region.unwrap()[3] as f32 / scale as f32;
                    self.update(|view| {
                        if pan {
                            view.pan(delta, height)
                        } else {
                            view.orbit(delta)
                        }
                    });
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if !matches!(
                    button,
                    MouseButton::Left | MouseButton::Middle | MouseButton::Right
                ) {
                    return false;
                }
                if *state == ElementState::Pressed && self.gesture.is_none() {
                    if let (Some(point), Some(_)) = (self.position, region) {
                        let before = self.app.state::<State>().0.lock().expect("camera").clone();
                        self.gesture = Some(Gesture {
                            origin: point,
                            last: point,
                            before: before.clone(),
                            button: *button,
                            pan: before.pan_mode
                                || *button != MouseButton::Left
                                || self.modifiers.shift_key(),
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
                        let state = self.app.state::<State>();
                        let mut camera = state.0.lock().expect("camera");
                        camera.selected = camera.pick(
                            &session.core.project().objects,
                            [
                                (p[0] - r[0] as f64) as f32 / r[2] as f32,
                                (p[1] - r[1] as f64) as f32 / r[3] as f32,
                            ],
                            r[2] as f32 / r[3] as f32,
                        );
                        camera.revision += 1;
                        self.dirty = true;
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. }
                if self.position.is_some() && region.is_some() && self.gesture.is_none() =>
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
                    if let Some(g) = self.gesture.take() {
                        self.update(|view| {
                            let revision = view.revision;
                            *view = g.before;
                            view.revision = revision;
                        });
                    }
                } else if self.position.is_some() && region.is_some() && self.gesture.is_none() {
                    match &event.logical_key {
                        Key::Character(c) if c.eq_ignore_ascii_case("f") => self.focus(),
                        Key::Character(c) if *c == "0" => self.update(|view| {
                            let selected = view.selected;
                            let revision = view.revision;
                            *view = View {
                                selected,
                                revision,
                                show_grid: view.show_grid,
                                show_axes: view.show_axes,
                                ..Default::default()
                            };
                        }),
                        _ => {}
                    }
                }
            }
            WindowEvent::Focused(false)
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::Resized(_)
            | WindowEvent::ScaleFactorChanged { .. } => {
                self.gesture = None;
                self.position = None;
                self.modifiers = ModifiersState::default();
            }
            _ => {}
        }
        false
    }
}
