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
    ring: Option<Ring>,
    scale_axis: Option<[f32; 2]>,
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
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

struct Ring {
    normal: [f32; 3],
    u: [f32; 3],
    v: [f32; 3],
}

impl Ring {
    fn new(normal: [f32; 3]) -> Self {
        let reference = if normal[1].abs() > 0.9 {
            [1., 0., 0.]
        } else {
            [0., 1., 0.]
        };
        let u = cross(reference, normal);
        let u = u.map(|value| value / dot(u, u).sqrt());
        Self {
            normal,
            u,
            v: cross(normal, u),
        }
    }

    fn nearest(
        &self,
        view: &View,
        region: [u32; 4],
        center: [f32; 3],
        radius: f32,
        point: [f32; 2],
    ) -> Option<(f32, [f32; 2])> {
        let sample = |i| {
            let angle = i as f32 * std::f32::consts::TAU / 64.;
            let (sin, cos) = angle.sin_cos();
            project(
                view,
                region,
                std::array::from_fn(|k| center[k] + radius * (self.u[k] * cos + self.v[k] * sin)),
            )
        };
        (0..64)
            .filter_map(|i| {
                let a = sample(i)?;
                let b = sample(i + 1)?;
                let d = [b[0] - a[0], b[1] - a[1]];
                let length2 = d[0] * d[0] + d[1] * d[1];
                if length2 < 1e-6 {
                    return None;
                }
                let offset = [point[0] - a[0], point[1] - a[1]];
                let t = ((offset[0] * d[0] + offset[1] * d[1]) / length2).clamp(0., 1.);
                Some(((offset[0] - t * d[0]).hypot(offset[1] - t * d[1]), d))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
    }

    fn phase(
        &self,
        view: &View,
        region: [u32; 4],
        center: [f32; 3],
        point: [f64; 2],
    ) -> Option<f32> {
        let [right, up, back] = view.basis();
        let x = (2. * (point[0] as f32 - region[0] as f32) / region[2] as f32 - 1.)
            * region[2] as f32
            / region[3] as f32;
        let y = 1. - 2. * (point[1] as f32 - region[1] as f32) / region[3] as f32;
        let slope = 25_f32.to_radians().tan();
        let ray = std::array::from_fn(|i| (right[i] * x + up[i] * y) * slope - back[i]);
        let denom = dot(ray, self.normal);
        if denom.abs() < 0.08 {
            return None;
        }
        let eye = view.eye();
        let t = dot(std::array::from_fn(|i| center[i] - eye[i]), self.normal) / denom;
        let offset = std::array::from_fn(|i| eye[i] + t * ray[i] - center[i]);
        let u = dot(offset, self.u);
        let v = dot(offset, self.v);
        (t > 0. && u.hypot(v) > 1e-6).then(|| v.atan2(u))
    }
}
pub fn axes(object: &SceneObject, tool: Tool) -> [[f32; 3]; 3] {
    if tool == Tool::Rotate {
        // For Rx * Ry * Rz, each axis includes the preceding Euler rotations.
        let (sx, cx) = object.rotation[0].sin_cos();
        let (sy, cy) = object.rotation[1].sin_cos();
        return [[1., 0., 0.], [0., cx, sx], [sy, -sx * cy, cx * cy]];
    }
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
            if view.tool == Tool::Rotate {
                let (distance, _) = Ring::new(*d).nearest(
                    view,
                    region,
                    object.position,
                    size,
                    p.map(|v| v as f32),
                )?;
                return (distance < 8. * scale as f32).then_some((i, distance));
            }
            let end = project(
                view,
                region,
                std::array::from_fn(|k| object.position[k] + d[k] * size),
            )?;
            if view.tool == Tool::Scale {
                let dx = p[0] as f32 - end[0];
                let dy = p[1] as f32 - end[1];
                return (dx.abs().max(dy.abs()) < 8. * scale as f32
                    && (end[0] - origin[0]).hypot(end[1] - origin[1]) > 4. * scale as f32)
                    .then_some((i, dx.hypot(dy)));
            }
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
        project_id: Uuid,
        revision: u64,
        layout: u64,
        view: View,
        region: [u32; 4],
        scale: f64,
        origin: [f64; 2],
        axis: Axis,
    ) -> Self {
        let radius = handle_size(&view, object, region[3] as f32 / scale as f32);
        let direction = axis.index().map(|i| axes(object, view.tool)[i]);
        let ring = direction
            .filter(|_| view.tool == Tool::Rotate)
            .map(Ring::new)
            .filter(|ring| {
                ring.nearest(
                    &view,
                    region,
                    object.position,
                    radius,
                    origin.map(|v| v as f32),
                )
                .is_some_and(|(distance, _)| distance < 8. * scale as f32)
            });
        let scale_axis = direction
            .filter(|_| view.tool == Tool::Scale)
            .and_then(|axis| {
                let start = project(&view, region, object.position)?;
                let tip = project(
                    &view,
                    region,
                    std::array::from_fn(|i| object.position[i] + radius * axis[i]),
                )?;
                let vector = [
                    (tip[0] - start[0]) / scale as f32,
                    (tip[1] - start[1]) / scale as f32,
                ];
                (vector[0].hypot(vector[1]) > 4.).then_some(vector)
            });
        Self {
            id: object.id,
            project: project_id,
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
            ring,
            scale_axis,
        }
    }
    pub fn pose(&self, point: [f64; 2], snap: bool) -> Transform {
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
        let quantize = |delta: f32, step: f32| {
            if snap {
                (delta / step).round() * step
            } else {
                delta
            }
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
                pose.position = std::array::from_fn(|i| {
                    (pose.position[i] + quantize(change[i], 0.1)).clamp(-10000., 10000.)
                });
            }
            Tool::Rotate => {
                if let Some(i) = self.axis.index() {
                    let amount = self
                        .ring
                        .as_ref()
                        .and_then(|ring| {
                            let phase = |point| {
                                ring.phase(&self.view, self.region, self.original.position, point)
                            };
                            if let (Some(start), Some(current)) = (phase(self.origin), phase(point))
                            {
                                let angle = current - start;
                                return Some(angle.sin().atan2(angle.cos()));
                            }
                            let radius = world_per_pixel * 72.;
                            let (_, tangent) = ring.nearest(
                                &self.view,
                                self.region,
                                self.original.position,
                                radius,
                                self.origin.map(|v| v as f32),
                            )?;
                            Some(
                                (delta[0] * tangent[0] + delta[1] * tangent[1])
                                    / tangent[0].hypot(tangent[1])
                                    / 72.,
                            )
                        })
                        .unwrap_or((delta[0] - delta[1]) * 0.01);
                    pose.rotation[i] += quantize(amount, 15_f32.to_radians());
                } else {
                    pose.rotation[0] += quantize(delta[1] * 0.01, 15_f32.to_radians());
                    pose.rotation[1] += quantize(delta[0] * 0.01, 15_f32.to_radians());
                }
                for (angle, original) in pose.rotation.iter_mut().zip(self.original.rotation) {
                    if *angle != original {
                        *angle = (*angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                            - std::f32::consts::PI;
                    }
                }
            }
            Tool::Scale => {
                let factor = if let Some([x, y]) = self.scale_axis {
                    1. + (delta[0] * x + delta[1] * y) / (x * x + y * y)
                } else {
                    ((delta[0] - delta[1]) * 0.01).clamp(-12., 12.).exp()
                };
                let factor = if snap {
                    1. + quantize(factor - 1., 0.1)
                } else {
                    factor
                };
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
                let changed = drag.pose([560., 330.], false);
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
                let snapped = drag.pose([560., 330.], true);
                for i in 0..3 {
                    let increments = match tool {
                        Tool::Move => (snapped.position[i] - o.position[i]) / 0.1,
                        Tool::Rotate => (snapped.rotation[i] - o.rotation[i]) / 15_f32.to_radians(),
                        Tool::Scale => (snapped.scale[i] / o.scale[i] - 1.) / 0.1,
                        Tool::Camera => unreachable!(),
                    };
                    assert!((increments - increments.round()).abs() < 1e-4);
                }
                assert_eq!(drag.pose([500., 350.], true), Transform::from(&o));
                let origin = drag.pose([500., 350.], false);
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
    #[test]
    fn dedicated_handles_pick_and_drag_in_their_projected_directions() {
        let mut object = atom_core::Project::new("Handles").unwrap().objects[0].clone();
        object.rotation = [0.; 3];
        let region = [20, 40, 1000, 800];
        let view = View {
            tool: Tool::Rotate,
            yaw: 0.,
            pitch: 0.,
            target: object.position,
            ..Default::default()
        };
        let radius = handle_size(&view, &object, 400.);
        let ring_point = |angle: f32| {
            let (sin, cos) = angle.sin_cos();
            project(
                &view,
                region,
                [
                    object.position[0] + radius * cos,
                    object.position[1] + radius * sin,
                    object.position[2],
                ],
            )
            .unwrap()
            .map(f64::from)
        };
        let start = ring_point(0.6);
        assert_eq!(hit_axis(&view, &object, region, 2., start), Some(Axis::Z));
        let drag = Drag::new(
            &object,
            Uuid::new_v4(),
            0,
            0,
            view.clone(),
            region,
            2.,
            start,
            Axis::Z,
        );
        let turned = drag.pose(ring_point(1.0), false);
        assert!((turned.rotation[2] - object.rotation[2] - 0.4).abs() < 1e-4);
        let snapped = drag.pose(ring_point(1.0), true);
        assert!((snapped.rotation[2] - object.rotation[2] - 30_f32.to_radians()).abs() < 1e-4);
        let tip = ring_point(0.);
        let view = View {
            tool: Tool::Scale,
            ..view
        };
        assert_eq!(hit_axis(&view, &object, region, 2., tip), Some(Axis::X));
        let center = project(&view, region, object.position)
            .unwrap()
            .map(f64::from);
        let drag = Drag::new(
            &object,
            Uuid::new_v4(),
            0,
            0,
            view,
            region,
            2.,
            tip,
            Axis::X,
        );
        let extended = std::array::from_fn(|i| tip[i] + 0.24 * (tip[i] - center[i]));
        let scaled = drag.pose(extended, true);
        assert!((scaled.scale[0] - object.scale[0] * 1.2).abs() < 1e-4);
        assert_eq!(scaled.scale[1..], object.scale[1..]);
    }
}
