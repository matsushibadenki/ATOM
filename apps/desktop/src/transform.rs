//! Shared transform transactions and native screen-to-world authoring math.
use super::{Shared, UiError, camera::View, error};
use atom_core::{SceneObject, Snapshot, Transform, Uuid};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
pub const NATIVE_OWNER: &str = "native-transform";
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    #[default]
    Camera,
    Move,
    Rotate,
    Scale,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    #[default]
    Free,
    X,
    Y,
    Z,
}
impl Axis {
    pub fn index(self) -> Option<usize> {
        match self {
            Self::Free => None,
            Self::X => Some(0),
            Self::Y => Some(1),
            Self::Z => Some(2),
        }
    }
}
pub struct Drag {
    pub id: u32,
    pub project: Uuid,
    pub revision: u64,
    pub layout: u64,
    pub tool: Tool,
    pub axis: Axis,
    pub origin: [f64; 2],
    pub scale: f64,
    pub moved: bool,
    original: Transform,
    view: View,
    region: [u32; 4],
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
pub fn project(view: &View, region: [u32; 4], position: [f32; 3]) -> Option<[f32; 2]> {
    let matrix = view.matrix(region[2] as f32 / region[3].max(1) as f32);
    let p = [position[0], position[1], position[2], 1.];
    let clip: [f32; 4] = std::array::from_fn(|r| (0..4).map(|c| matrix[c * 4 + r] * p[c]).sum());
    (clip[3] > 0. && clip[2] >= 0.).then(|| {
        [
            region[0] as f32 + (clip[0] / clip[3] + 1.) * 0.5 * region[2] as f32,
            region[1] as f32 + (1. - clip[1] / clip[3]) * 0.5 * region[3] as f32,
        ]
    })
}
pub fn axes(object: &SceneObject, tool: Tool) -> [[f32; 3]; 3] {
    if tool != Tool::Scale {
        return [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    }
    let model = super::camera::box_model(object);
    std::array::from_fn(|c| std::array::from_fn(|r| model[c][r] / object.scale[c]))
}
pub fn handle_size(view: &View, object: &SceneObject, height: f32) -> f32 {
    let depth = dot(
        std::array::from_fn(|i| view.eye()[i] - object.position[i]),
        view.basis()[2],
    );
    2. * depth.max(0.01) * (25f32.to_radians()).tan() / height.max(1.) * 72.
}
pub fn hit_axis(
    view: &View,
    object: &SceneObject,
    region: [u32; 4],
    scale: f64,
    p: [f64; 2],
) -> Option<Axis> {
    let origin = project(view, region, object.position)?;
    let size = handle_size(view, object, region[3] as f32 / scale as f32);
    axes(object, view.tool)
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let end = project(
                view,
                region,
                std::array::from_fn(|k| object.position[k] + d[k] * size),
            )?;
            let v = [end[0] - origin[0], end[1] - origin[1]];
            let len = v[0] * v[0] + v[1] * v[1];
            if len < 16. * scale as f32 * scale as f32 {
                return None;
            }
            let delta = [p[0] as f32 - origin[0], p[1] as f32 - origin[1]];
            let t = (delta[0] * v[0] + delta[1] * v[1]) / len;
            let distance = (delta[0] - t * v[0]).hypot(delta[1] - t * v[1]);
            (t > 0.2 && t < 1.2 && distance < 8. * scale as f32).then_some((i, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| [Axis::X, Axis::Y, Axis::Z][i])
}
impl Drag {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        object: &SceneObject,
        project: Uuid,
        revision: u64,
        layout: u64,
        view: View,
        region: [u32; 4],
        scale: f64,
        origin: [f64; 2],
        axis: Axis,
    ) -> Self {
        Self {
            id: object.id,
            project,
            revision,
            layout,
            tool: view.tool,
            axis,
            origin,
            scale,
            moved: false,
            original: Transform::from(object),
            view,
            region,
        }
    }
    pub fn pose(&self, point: [f64; 2]) -> Transform {
        let delta = [
            ((point[0] - self.origin[0]) / self.scale) as f32,
            ((point[1] - self.origin[1]) / self.scale) as f32,
        ];
        let mut pose = self.original.clone();
        let [right, up, back] = self.view.basis();
        let depth = dot(
            std::array::from_fn(|i| self.view.eye()[i] - pose.position[i]),
            back,
        )
        .max(0.01);
        let world_per_pixel = 2. * depth * (25f32.to_radians()).tan()
            / (self.region[3] as f32 / self.scale as f32).max(1.);
        let projected_amount = |axis: [f32; 3]| {
            let end = std::array::from_fn(|i| pose.position[i] + axis[i]);
            if let (Some(a), Some(b)) = (
                project(&self.view, self.region, pose.position),
                project(&self.view, self.region, end),
            ) {
                let v = [
                    (b[0] - a[0]) / self.scale as f32,
                    (b[1] - a[1]) / self.scale as f32,
                ];
                let sq = v[0] * v[0] + v[1] * v[1];
                if sq > 0.25 {
                    return (delta[0] * v[0] + delta[1] * v[1]) / sq;
                }
            }
            -delta[1] * world_per_pixel
        };
        match self.tool {
            Tool::Move => {
                let change = if let Some(axis) = self.axis.index() {
                    let direction = std::array::from_fn(|i| if i == axis { 1. } else { 0. });
                    let amount = projected_amount(direction);
                    direction.map(|v| v * amount)
                } else {
                    std::array::from_fn(|i| {
                        (right[i] * delta[0] - up[i] * delta[1]) * world_per_pixel
                    })
                };
                pose.position =
                    std::array::from_fn(|i| (pose.position[i] + change[i]).clamp(-10000., 10000.));
            }
            Tool::Rotate => {
                if let Some(i) = self.axis.index() {
                    pose.rotation[i] += (delta[0] - delta[1]) * 0.01;
                } else {
                    pose.rotation[0] += delta[1] * 0.01;
                    pose.rotation[1] += delta[0] * 0.01;
                }
                pose.rotation = pose.rotation.map(|v| {
                    (v + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI
                });
            }
            Tool::Scale => {
                let factor = ((delta[0] - delta[1]) * 0.01).clamp(-12., 12.).exp();
                for (i, value) in pose.scale.iter_mut().enumerate() {
                    if self.axis.index().is_none_or(|axis| axis == i) {
                        *value = (*value * factor).clamp(0.001, 100.);
                    }
                }
            }
            Tool::Camera => {}
        }
        pose
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Begin,
    Preview,
    Commit,
    Cancel,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub phase: Phase,
    pub expected_revision: u64,
    pub project_id: Uuid,
    pub id: u32,
    pub transform: Option<Transform>,
}
pub fn publish(app: &tauri::AppHandle, snapshot: &Snapshot) {
    super::camera::reconcile(app);
    let _ = app.emit("atom://changed", snapshot);
}
#[tauri::command]
pub fn transform_command(webview: tauri::Webview, request: Request) -> Result<Snapshot, UiError> {
    if !matches!(webview.label(), "scene" | "viewport") {
        return Err(error("forbidden"));
    }
    let shared = webview.state::<Shared>();
    let mut session = shared.0.lock().map_err(|_| error("internal_error"))?;
    let owner = webview.label();
    let result = match request.phase {
        Phase::Begin => {
            if webview
                .state::<super::camera::State>()
                .0
                .lock()
                .map_err(|_| error("internal_error"))?
                .selected
                != Some(request.id)
            {
                return Err(error("no_selection"));
            }
            session.core.begin_transform(
                owner,
                request.expected_revision,
                request.project_id,
                request.id,
            )
        }
        Phase::Preview => session.core.preview_transform(
            owner,
            request.expected_revision,
            request.project_id,
            request.transform.ok_or_else(|| error("invalid_project"))?,
        ),
        Phase::Commit => {
            session
                .core
                .commit_transform(owner, request.expected_revision, request.project_id)
        }
        Phase::Cancel => session.core.cancel_transform(owner),
    }?;
    drop(session);
    let app = webview.app_handle().clone();
    publish(&app, &result);
    let redraw = app.clone();
    let _ = app.run_on_main_thread(move || super::viewport::draw(&redraw));
    Ok(result)
}
// AppKit consumes the predefined Undo selector before Tao sees Cmd+Z on a GPU view.
#[cfg(target_os = "macos")]
pub fn install_history_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, MenuItemKind};
    let menu = Menu::default(app)?;
    for item in menu.items()? {
        if let MenuItemKind::Submenu(edit) = item
            && edit.text()? == "Edit"
        {
            edit.remove_at(0)?;
            edit.remove_at(0)?;
            let undo = MenuItem::with_id(
                app,
                "atom-undo",
                "Undo / 取り消す / 撤销",
                true,
                Some("CmdOrCtrl+Z"),
            )?;
            let redo = MenuItem::with_id(
                app,
                "atom-redo",
                "Redo / やり直す / 重做",
                true,
                Some("CmdOrCtrl+Shift+Z"),
            )?;
            edit.insert_items(&[&undo, &redo], 0)?;
        }
    }
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        let redo = match event.id().as_ref() {
            "atom-undo" => false,
            "atom-redo" => true,
            _ => return,
        };
        let result = {
            let shared = app.state::<Shared>();
            let mut session = shared.0.lock().expect("session");
            let snapshot = session.core.snapshot();
            if snapshot.previewing {
                let owner = [
                    NATIVE_OWNER,
                    super::joints::drag::OWNER,
                    "scene",
                    "viewport",
                ]
                .into_iter()
                .find(|owner| session.core.preview_owned_by(owner));
                owner.map(|owner| session.core.cancel_transform(owner))
            } else if if redo {
                snapshot.can_redo
            } else {
                snapshot.can_undo
            } {
                Some(session.core.apply(atom_core::EditRequest {
                    expected_revision: snapshot.revision,
                    project_id: snapshot.project_id,
                    command: if redo {
                        atom_core::Command::Redo
                    } else {
                        atom_core::Command::Undo
                    },
                }))
            } else {
                None
            }
        };
        if let Some(Ok(snapshot)) = result {
            publish(app, &snapshot);
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || super::viewport::draw(&handle));
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn constrained_move_rotation_and_scale_are_finite_and_reversible_from_origin() {
        let o = atom_core::Project::new("Math").unwrap().objects[0].clone();
        for tool in [Tool::Move, Tool::Rotate, Tool::Scale] {
            for axis in [Axis::Free, Axis::X, Axis::Y, Axis::Z] {
                let view = View {
                    tool,
                    ..Default::default()
                };
                let drag = Drag::new(
                    &o,
                    Uuid::new_v4(),
                    0,
                    0,
                    view,
                    [0, 0, 1000, 700],
                    1.,
                    [500., 350.],
                    axis,
                );
                let changed = drag.pose([560., 330.]);
                assert_ne!(changed, Transform::from(&o));
                if tool == Tool::Move
                    && let Some(i) = axis.index()
                {
                    for k in 0..3 {
                        if k != i {
                            assert_eq!(changed.position[k], o.position[k]);
                        }
                    }
                }
                if tool == Tool::Scale {
                    for value in changed.scale {
                        assert!((0.001..=100.).contains(&value));
                    }
                }
                let origin = drag.pose([500., 350.]);
                for (a, b) in origin
                    .position
                    .iter()
                    .chain(origin.rotation.iter())
                    .chain(origin.scale.iter())
                    .zip(
                        o.position
                            .iter()
                            .chain(o.rotation.iter())
                            .chain(o.scale.iter()),
                    )
                {
                    assert!((a - b).abs() < 1e-5);
                }
            }
        }
    }
    #[test]
    fn axis_handles_share_projection_at_retina_and_float_aspects() {
        let o = atom_core::Project::new("Math").unwrap().objects[0].clone();
        for scale in [1., 2.] {
            let region = [0, 0, (380. * scale) as u32, (480. * scale) as u32];
            let view = View {
                tool: Tool::Move,
                ..Default::default()
            };
            let size = handle_size(&view, &o, 480.);
            let tip = project(
                &view,
                region,
                [o.position[0] + size * 0.9, o.position[1], o.position[2]],
            )
            .unwrap();
            assert_eq!(
                hit_axis(&view, &o, region, scale, [tip[0] as f64, tip[1] as f64]),
                Some(Axis::X)
            );
        }
    }
}
