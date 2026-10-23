//! Shared view state; no project edits, GPU readback or window-owned cameras.
use super::{Shared, UiError, error};
use atom_core::SceneObject;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use threex_core::multiply;

#[derive(Clone, Debug, Serialize)]
pub struct View {
    pub revision: u64,
    pub selected: Option<u32>,
    pub tool: super::transform::Tool,
    pub axis: super::transform::Axis,
    pub pan_mode: bool,
    pub show_grid: bool,
    pub show_axes: bool,
    pub target: [f32; 3],
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
}
impl Default for View {
    fn default() -> Self {
        Self {
            revision: 0,
            selected: None,
            tool: Default::default(),
            axis: Default::default(),
            pan_mode: false,
            show_grid: true,
            show_axes: true,
            target: [1.4, 0., 0.],
            distance: 8.,
            yaw: 0.32,
            pitch: 0.4,
        }
    }
}
#[derive(Default)]
pub struct State(pub Mutex<View>);
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
impl View {
    pub fn basis(&self) -> [[f32; 3]; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let back = [cp * sy, sp, cp * cy];
        let right = [cy, 0., -sy];
        [right, cross(back, right), back]
    }
    pub fn eye(&self) -> [f32; 3] {
        let back = self.basis()[2];
        std::array::from_fn(|i| self.target[i] + back[i] * self.distance)
    }
    pub fn matrix(&self, aspect: f32) -> [f32; 16] {
        let [right, up, back] = self.basis();
        let eye = self.eye();
        let view = [
            [right[0], up[0], back[0], 0.],
            [right[1], up[1], back[1], 0.],
            [right[2], up[2], back[2], 0.],
            [-dot(right, eye), -dot(up, eye), -dot(back, eye), 1.],
        ];
        let projection = threex_core::Perspective {
            fov: 50.,
            aspect,
            near: (self.distance / 10000.).max(0.001),
            far: self.distance + 40000.,
            zoom: 1.,
        }
        .matrix()
        .expect("bounded camera");
        let matrix = threex_core::multiply(projection, view);
        std::array::from_fn(|i| matrix[i / 4][i % 4])
    }
    pub fn orbit(&mut self, delta: [f32; 2]) {
        self.yaw = (self.yaw - delta[0] * 0.007).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + delta[1] * 0.007).clamp(-1.55, 1.55);
    }
    pub fn pan(&mut self, delta: [f32; 2], height: f32) {
        let [right, up, _] = self.basis();
        let factor = 2. * self.distance * (25f32.to_radians()).tan() / height.max(1.);
        self.target = std::array::from_fn(|i| {
            (self.target[i] + (-right[i] * delta[0] + up[i] * delta[1]) * factor)
                .clamp(-30000., 30000.)
        });
    }
    pub fn zoom(&mut self, delta: f32) {
        self.distance =
            (self.distance * (delta.clamp(-400., 400.) * 0.002).exp()).clamp(0.05, 100000.);
    }
    pub fn focus(&mut self, object: &SceneObject, aspect: f32) {
        // A bounding sphere covers all rotated/scaled box corners, also in portrait floats.
        let radius = object.scale.iter().map(|s| s * s).sum::<f32>().sqrt() * 0.5;
        let half = (25f32.to_radians()).tan();
        let angle = (half * aspect.clamp(0.001, 1.)).atan();
        self.target = object.position;
        self.distance = (radius / angle.sin() * 1.15).clamp(0.05, 100000.);
    }
    pub fn pick(&self, objects: &[SceneObject], uv: [f32; 2], aspect: f32) -> Option<u32> {
        let [right, up, back] = self.basis();
        let f = (25f32.to_radians()).tan();
        let d: [f32; 3] = std::array::from_fn(|i| {
            right[i] * (uv[0] * 2. - 1.) * aspect * f + up[i] * (1. - uv[1] * 2.) * f - back[i]
        });
        let length = dot(d, d).sqrt();
        let direction = d.map(|v| v / length);
        let eye = self.eye();
        objects
            .iter()
            .filter_map(|o| {
                let model = box_model(o);
                let origin: [f32; 3] = std::array::from_fn(|i| eye[i] - o.position[i]);
                let mut near = 0f32;
                let mut far = f32::INFINITY;
                for column in model.iter().take(3) {
                    let axis: [f32; 3] = column[..3].try_into().unwrap();
                    let sq = dot(axis, axis);
                    if sq <= f32::MIN_POSITIVE {
                        return None;
                    }
                    let p = dot(origin, axis) / sq;
                    let v = dot(direction, axis) / sq;
                    if v.abs() < 1e-8 {
                        if p.abs() > 0.5 {
                            return None;
                        }
                    } else {
                        let a = (-0.5 - p) / v;
                        let b = (0.5 - p) / v;
                        near = near.max(a.min(b));
                        far = far.min(a.max(b));
                        if far < near {
                            return None;
                        }
                    }
                }
                Some((o.id, near))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|v| v.0)
    }
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Select {
        id: Option<u32>,
    },
    PanMode {
        enabled: bool,
    },
    Tool {
        tool: super::transform::Tool,
        axis: super::transform::Axis,
    },
    Guides {
        grid: bool,
        axes: bool,
    },
    Focus,
    Reset,
}
/// Root focus includes the complete robot, including a simulated final pose.
pub fn focus_target(core: &atom_core::AppCore, id: u32) -> Option<SceneObject> {
    let mut target = core.object(id)?.clone();
    if let Some(robot) = core.render_project().robots.iter().find(|r| r.root == id) {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for link in &robot.links {
            let o = core.object(link.object_id)?;
            let radius = o.scale.iter().map(|v| v * v).sum::<f32>().sqrt() / 2.;
            for axis in 0..3 {
                min[axis] = min[axis].min(o.position[axis] - radius);
                max[axis] = max[axis].max(o.position[axis] + radius);
            }
        }
        target.position = std::array::from_fn(|i| (min[i] + max[i]) / 2.);
        target.scale = std::array::from_fn(|i| max[i] - min[i]);
        target.rotation = [0.; 3];
    }
    Some(target)
}
pub fn notify(app: &tauri::AppHandle) {
    let view = app.state::<State>().0.lock().expect("camera").clone();
    let _ = app.emit("atom://camera", view);
}
pub fn reconcile(app: &tauri::AppHandle) {
    let shared = app.state::<Shared>();
    let session = shared.0.lock().expect("session");
    let state = app.state::<State>();
    let mut view = state.0.lock().expect("camera");
    if view
        .selected
        .is_some_and(|id| session.core.object(id).is_none())
    {
        view.selected = None;
        view.revision += 1;
    }
    drop(view);
    drop(session);
    notify(app);
}
#[tauri::command]
pub fn camera_state(webview: tauri::Webview) -> Result<View, UiError> {
    super::authorize(&webview)?;
    Ok(webview
        .state::<State>()
        .0
        .lock()
        .map_err(|_| error("internal_error"))?
        .clone())
}
#[tauri::command]
pub fn camera_command(
    webview: tauri::Webview,
    command: Command,
    expected_revision: u64,
) -> Result<View, UiError> {
    if !matches!(webview.label(), "scene" | "viewport") {
        return Err(error("forbidden"));
    }
    let shared = webview.state::<Shared>();
    let session = shared.0.lock().map_err(|_| error("internal_error"))?;
    if session.core.has_transform_preview() {
        return Err(error("edit_busy"));
    }
    if session.core.snapshot().revision != expected_revision {
        return Err(error("revision_conflict"));
    }
    let state = webview.state::<State>();
    let mut view = state.0.lock().map_err(|_| error("internal_error"))?;
    match command {
        Command::Select { id } => {
            if id.is_some_and(|id| session.core.object(id).is_none()) {
                return Err(error("invalid_project"));
            }
            view.selected = id;
        }
        Command::PanMode { enabled } => view.pan_mode = enabled,
        Command::Tool { tool, axis } => {
            view.tool = tool;
            view.axis = axis;
            view.pan_mode = false;
        }
        Command::Guides { grid, axes } => {
            view.show_grid = grid;
            view.show_axes = axes;
        }
        Command::Focus => {
            let object = view
                .selected
                .and_then(|id| focus_target(&session.core, id))
                .ok_or_else(|| error("no_selection"))?;
            view.focus(&object, super::viewport::aspect(webview.app_handle()));
        }
        Command::Reset => {
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
        }
    }
    view.revision += 1;
    let result = view.clone();
    drop(view);
    drop(session);
    let app = webview.app_handle().clone();
    notify(&app);
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || super::viewport::draw(&handle));
    Ok(result)
}

pub(crate) fn box_model(o: &SceneObject) -> [[f32; 4]; 4] {
    let (sx, cx) = o.rotation[0].sin_cos();
    let (sy, cy) = o.rotation[1].sin_cos();
    let (sz, cz) = o.rotation[2].sin_cos();
    let x = [
        [1., 0., 0., 0.],
        [0., cx, sx, 0.],
        [0., -sx, cx, 0.],
        [0., 0., 0., 1.],
    ];
    let y = [
        [cy, 0., -sy, 0.],
        [0., 1., 0., 0.],
        [sy, 0., cy, 0.],
        [0., 0., 0., 1.],
    ];
    let z = [
        [cz, sz, 0., 0.],
        [-sz, cz, 0., 0.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
    ];
    let mut m = multiply(multiply(x, y), z);
    for (column, scale) in m.iter_mut().take(3).zip(o.scale) {
        for value in column {
            *value *= scale;
        }
    }
    m[3] = [o.position[0], o.position[1], o.position[2], 1.];
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    fn object() -> SceneObject {
        SceneObject {
            id: 1,
            position: [2., 3., -4.],
            rotation: [0.4, 0.7, 0.2],
            scale: [2., 0.5, 3.],
            color: [1.; 3],
            label: String::new(),
            material: Default::default(),
            fixed: false,
            mesh_ref: None,
            mass_override: None,
        }
    }
    #[test]
    fn focus_projects_every_rotated_corner_inside_portrait_and_landscape() {
        for aspect in [0.2, 1., 3.] {
            let mut view = View::default();
            let o = object();
            view.focus(&o, aspect);
            let m = view.matrix(aspect);
            let model = box_model(&o);
            let matrix: [[f32; 4]; 4] =
                std::array::from_fn(|c| std::array::from_fn(|r| m[c * 4 + r]));
            let combined = threex_core::multiply(matrix, model);
            for x in [-0.5, 0.5] {
                for y in [-0.5, 0.5] {
                    for z in [-0.5, 0.5] {
                        let p = [x, y, z, 1.];
                        let clip: [f32; 4] =
                            std::array::from_fn(|r| (0..4).map(|c| combined[c][r] * p[c]).sum());
                        assert!(clip[3] > 0.);
                        assert!(clip[0].abs() < clip[3]);
                        assert!(clip[1].abs() < clip[3]);
                        assert!(clip[2] > 0. && clip[2] < clip[3]);
                    }
                }
            }
        }
    }
    #[test]
    fn pick_nearest_rotated_box_and_empty_background() {
        let mut view = View::default();
        let o = object();
        view.focus(&o, 1.);
        assert_eq!(view.pick(std::slice::from_ref(&o), [0.5, 0.5], 1.), Some(1));
        assert_eq!(view.pick(&[o], [0., 0.], 1.), None);
    }
    #[test]
    fn picking_resolves_occlusion_and_rejects_objects_behind_camera() {
        let mut view = View::default();
        let near = object();
        view.focus(&near, 1.);
        let back = view.basis()[2];
        let mut far = near.clone();
        far.id = 2;
        far.position = std::array::from_fn(|i| near.position[i] - back[i] * 5.);
        assert_eq!(view.pick(&[far, near.clone()], [0.5, 0.5], 1.), Some(1));
        let eye = view.eye();
        let mut behind = near;
        behind.position = std::array::from_fn(|i| eye[i] + back[i] * 5.);
        assert_eq!(view.pick(&[behind], [0.5, 0.5], 1.), None);
    }
    #[test]
    fn camera_limits_and_pan_follow_screen_basis() {
        let mut view = View::default();
        let before = view.matrix(1.);
        view.orbit([80., 20.]);
        assert_ne!(before, view.matrix(1.));
        let target = view.target;
        view.pan([20., 10.], 600.);
        assert_ne!(target, view.target);
        for _ in 0..100 {
            view.zoom(-400.);
            view.orbit([10000., 10000.]);
        }
        assert_eq!(view.distance, 0.05);
        assert!(view.pitch <= 1.55);
        for _ in 0..100 {
            view.zoom(400.);
        }
        assert_eq!(view.distance, 100000.);
        assert!(view.matrix(0.2).iter().all(|v| v.is_finite()));
    }
}
