//! Native angular manipulation in the same world frames and screen region as the GPU guides.
use super::range_view;
use crate::camera::View;
use atom_core::{AppCore, Error, JointEdit, Result, Snapshot, Uuid};
use threex_renderer::joint_ranges::{JointRanges, arc_point, axis_line, limit_handle};
pub const OWNER: &str = "native-joint";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Arc,
    Axis,
    Lower,
    Upper,
}
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub axis: usize,
    pub handle: Handle,
    pub distance: f64,
    phase: f64,
}
#[derive(Clone)]
struct Screen {
    view: View,
    region: [u32; 4],
    matrix: [f64; 16],
    scale: f64,
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
impl Screen {
    fn new(view: &View, region: [u32; 4], scale: f64) -> Option<Self> {
        if region[2] == 0 || region[3] == 0 || !scale.is_finite() || scale <= 0. {
            return None;
        }
        Some(Self {
            view: view.clone(),
            region,
            matrix: view
                .matrix(region[2] as f32 / region[3] as f32)
                .map(f64::from),
            scale,
        })
    }
    fn clip(&self, p: [f32; 3]) -> [f64; 4] {
        let p = [p[0] as f64, p[1] as f64, p[2] as f64, 1.];
        std::array::from_fn(|r| (0..4).map(|c| self.matrix[c * 4 + r] * p[c]).sum())
    }
    fn pixel(&self, p: [f64; 4]) -> Option<[f64; 2]> {
        if !p.iter().all(|v| v.is_finite()) || p[3] <= 1e-8 || p[2] < 0. {
            return None;
        }
        Some([
            self.region[0] as f64 + (p[0] / p[3] + 1.) * 0.5 * self.region[2] as f64,
            self.region[1] as f64 + (1. - p[1] / p[3]) * 0.5 * self.region[3] as f64,
        ])
    }
    fn project(&self, p: [f32; 3]) -> Option<[f64; 2]> {
        self.pixel(self.clip(p))
    }
    fn segment(&self, a: [f32; 3], b: [f32; 3]) -> Option<[[f64; 2]; 2]> {
        let (mut a, mut b) = (self.clip(a), self.clip(b));
        if (a[2] <= 0. && b[2] <= 0.) || (a[3] <= 0. && b[3] <= 0.) {
            return None;
        }
        if a[2] < 0. {
            let t = -a[2] / (b[2] - a[2]);
            a = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
            a[2] = 0.;
        }
        if b[2] < 0. {
            let t = -b[2] / (a[2] - b[2]);
            b = std::array::from_fn(|i| b[i] + (a[i] - b[i]) * t);
            b[2] = 0.;
        }
        Some([self.pixel(a)?, self.pixel(b)?])
    }
    fn inside(&self, p: [f64; 2]) -> bool {
        p.iter().all(|v| v.is_finite())
            && (0..2).all(|i| {
                p[i] >= self.region[i] as f64 && p[i] < (self.region[i] + self.region[i + 2]) as f64
            })
    }
    fn ray(&self, p: [f64; 2]) -> [f64; 3] {
        let uv = [
            (p[0] - self.region[0] as f64) / self.region[2] as f64,
            (p[1] - self.region[1] as f64) / self.region[3] as f64,
        ];
        let [right, up, back] = self.view.basis().map(|a| a.map(f64::from));
        let f = 25f64.to_radians().tan();
        let d = std::array::from_fn(|i| {
            right[i] * (uv[0] * 2. - 1.) * self.region[2] as f64 / self.region[3] as f64 * f
                + up[i] * (1. - uv[1] * 2.) * f
                - back[i]
        });
        let norm = dot(d, d).sqrt();
        d.map(|v| v / norm)
    }
}
fn segment_distance(p: [f64; 2], segment: [[f64; 2]; 2]) -> (f64, f64) {
    let [a, b] = segment;
    let d = [b[0] - a[0], b[1] - a[1]];
    let len = d[0] * d[0] + d[1] * d[1];
    if len < 1e-8 {
        return (f64::INFINITY, 0.);
    }
    let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len).clamp(0., 1.);
    ((p[0] - a[0] - t * d[0]).hypot(p[1] - a[1] - t * d[1]), t)
}
fn pick(screen: &Screen, range: &JointRanges, point: [f64; 2]) -> Option<Hit> {
    if !screen.inside(point) {
        return None;
    }
    let center = screen.project(range.origin)?;
    if (point[0] - center[0]).hypot(point[1] - center[1]) < 4. * screen.scale {
        return None;
    }
    let mut hits = Vec::new();
    for (axis, arc) in range.arcs.iter().take(3).enumerate() {
        let mut add = |a, b, handle, lo: f64, hi: f64| {
            if let Some(segment) = screen.segment(a, b) {
                let (distance, t) = segment_distance(point, segment);
                if distance <= 7. * screen.scale {
                    hits.push(Hit {
                        axis,
                        handle,
                        distance,
                        phase: lo + (hi - lo) * t,
                    });
                }
            }
        };
        for step in 0..96 {
            let phase =
                |s| arc.range[0] as f64 + (arc.range[1] - arc.range[0]) as f64 * s as f64 / 96.;
            let lo = phase(step);
            let hi = phase(step + 1);
            add(
                arc_point(range, axis, lo as f32)?,
                arc_point(range, axis, hi as f32)?,
                Handle::Arc,
                lo,
                hi,
            );
        }
        // The gold current-angle ray is also an angular handle; skip its ambiguous center.
        add(
            range.origin,
            arc_point(range, axis, arc.angle)?,
            Handle::Arc,
            arc.angle as f64,
            arc.angle as f64,
        );
        for bound in 0..2 {
            let [a, b] = limit_handle(range, axis, bound)?;
            add(
                a,
                b,
                if bound == 0 {
                    Handle::Lower
                } else {
                    Handle::Upper
                },
                arc.range[bound] as f64,
                arc.range[bound] as f64,
            );
        }
        let [a, b] = axis_line(range, axis)?;
        add(a, b, Handle::Axis, arc.angle as f64, arc.angle as f64);
    }
    hits.into_iter().min_by(|a, b| {
        let rank = |h: &Hit| {
            h.distance
                + if matches!(h.handle, Handle::Lower | Handle::Upper) {
                    -14. * screen.scale
                } else if h.handle == Handle::Axis {
                    screen.scale * 0.2
                } else {
                    0.
                }
        };
        rank(a).total_cmp(&rank(b))
    })
}
pub fn hit_test(
    core: &AppCore,
    view: &View,
    region: [u32; 4],
    scale: f64,
    point: [f64; 2],
) -> Option<Hit> {
    let screen = Screen::new(view, region, scale)?;
    let ranges = range_view(core, view, region[3] as f32 / scale as f32)?;
    pick(&screen, &ranges, point)
}
struct Plane {
    origin: [f64; 3],
    axis: [f64; 3],
    zero: [f64; 3],
    tangent: [f64; 3],
}
impl Plane {
    fn phase(&self, screen: &Screen, point: [f64; 2]) -> Option<f64> {
        let ray = screen.ray(point);
        let denom = dot(ray, self.axis);
        if denom.abs() < 0.08 {
            return None;
        }
        let eye = screen.view.eye().map(f64::from);
        let t = dot(std::array::from_fn(|i| self.origin[i] - eye[i]), self.axis) / denom;
        if t <= 0. || !t.is_finite() {
            return None;
        }
        let v = std::array::from_fn(|i| eye[i] + ray[i] * t - self.origin[i]);
        let x = dot(v, self.zero);
        let y = dot(v, self.tangent);
        (x.hypot(y) > 1e-7).then(|| y.atan2(x))
    }
}
enum Mapping {
    Plane {
        plane: Plane,
        last: Option<f64>,
    },
    Screen {
        direction: [f64; 2],
        per_radian: f64,
        last: [f64; 2],
    },
}
impl Mapping {
    fn new(screen: &Screen, range: &JointRanges, hit: Hit, point: [f64; 2]) -> Option<Self> {
        let arc = range.arcs.get(hit.axis)?;
        let axis = arc.axis.map(f64::from);
        let zero = arc.zero.map(f64::from);
        let plane = Plane {
            origin: range.origin.map(f64::from),
            axis,
            zero,
            tangent: cross(axis, zero),
        };
        if hit.handle != Handle::Axis
            && dot(screen.ray(point), axis).abs() > 0.2
            && let Some(phase) = plane.phase(screen, point)
        {
            return Some(Self::Plane {
                plane,
                last: Some(phase),
            });
        }
        let center = screen.project(range.origin)?;
        let radius = (0..4)
            .filter_map(|i| {
                screen.project(arc_point(
                    range,
                    hit.axis,
                    i as f32 * std::f32::consts::FRAC_PI_2,
                )?)
            })
            .map(|p| (p[0] - center[0]).hypot(p[1] - center[1]))
            .fold(0., f64::max)
            .max(24. * screen.scale);
        let a = screen.project(arc_point(range, hit.axis, (hit.phase - 0.01) as f32)?)?;
        let b = screen.project(arc_point(range, hit.axis, (hit.phase + 0.01) as f32)?)?;
        let tangent = [(b[0] - a[0]) / 0.02, (b[1] - a[1]) / 0.02];
        let length = tangent[0].hypot(tangent[1]);
        let mut direction = if length > screen.scale * 4. {
            tangent.map(|v| v / length)
        } else {
            [1., 0.]
        };
        let mut per_radian = length.max(radius * 0.6);
        if hit.handle == Handle::Axis
            && let Some([a, b]) = screen.segment(
                axis_line(range, hit.axis)?[0],
                axis_line(range, hit.axis)?[1],
            )
        {
            let d = [b[0] - a[0], b[1] - a[1]];
            let n = d[0].hypot(d[1]);
            if n > 2. * screen.scale {
                let mut perpendicular = [-d[1] / n, d[0] / n];
                if perpendicular[0] * direction[0] + perpendicular[1] * direction[1] < 0. {
                    perpendicular = perpendicular.map(|v| -v);
                }
                direction = perpendicular;
            }
            per_radian = radius;
        }
        Some(Self::Screen {
            direction,
            per_radian,
            last: point,
        })
    }
    fn delta(&mut self, screen: &Screen, point: [f64; 2]) -> Option<f64> {
        match self {
            Self::Plane { plane, last } => {
                let phase = plane.phase(screen, point);
                let old = std::mem::replace(last, phase);
                let d = phase? - old?;
                Some(d.sin().atan2(d.cos()))
            }
            Self::Screen {
                direction,
                per_radian,
                last,
            } => {
                let delta = ((point[0] - last[0]) * direction[0]
                    + (point[1] - last[1]) * direction[1])
                    / *per_radian;
                *last = point;
                Some(delta)
            }
        }
    }
}
pub struct NativeDrag {
    pub id: u32,
    pub layout: u64,
    pub origin: [f64; 2],
    pub scale: f64,
    token: u64,
    revision: u64,
    project: Uuid,
    screen: Screen,
    mapping: Mapping,
    angle: f64,
    range: [f32; 2],
    edit: JointEdit,
    moved: bool,
}
impl NativeDrag {
    pub fn begin(
        core: &mut AppCore,
        view: &View,
        layout: u64,
        region: [u32; 4],
        scale: f64,
        point: [f64; 2],
    ) -> Result<Option<(Self, Snapshot)>> {
        let Some(screen) = Screen::new(view, region, scale) else {
            return Ok(None);
        };
        let Some(ranges) = range_view(core, view, region[3] as f32 / scale as f32) else {
            return Ok(None);
        };
        let Some(hit) = pick(&screen, &ranges, point) else {
            return Ok(None);
        };
        let Some(mapping) = Mapping::new(&screen, &ranges, hit, point) else {
            return Ok(None);
        };
        let id = view.selected.unwrap();
        let arc = &ranges.arcs[hit.axis];
        let s = core.snapshot();
        let edit = match hit.handle {
            Handle::Lower => JointEdit::Lower,
            Handle::Upper => JointEdit::Upper,
            _ => JointEdit::Angle,
        };
        let started = core.begin_joint_edit(OWNER, s.revision, s.project_id, id, hit.axis, edit)?;
        let token = started.joint_preview.as_ref().unwrap().token;
        Ok(Some((
            Self {
                id,
                layout,
                origin: point,
                scale,
                token,
                revision: s.revision,
                project: s.project_id,
                screen,
                mapping,
                angle: match edit {
                    JointEdit::Lower => arc.range[0],
                    JointEdit::Upper => arc.range[1],
                    JointEdit::Angle => arc.angle,
                } as f64,
                edit,
                range: arc.range,
                moved: false,
            },
            started,
        )))
    }
    pub fn active(&self, core: &AppCore) -> bool {
        core.preview_owned_by(OWNER)
            && core
                .joint_preview_info()
                .is_some_and(|p| p.token == self.token)
    }
    pub fn view_matches(&self, view: &View) -> bool {
        view.selected == Some(self.id)
            && view.matrix(self.screen.region[2] as f32 / self.screen.region[3] as f32)
                == self
                    .screen
                    .view
                    .matrix(self.screen.region[2] as f32 / self.screen.region[3] as f32)
            && view.tool == self.screen.view.tool
            && view.pan_mode == self.screen.view.pan_mode
    }
    pub fn preview(&mut self, core: &mut AppCore, point: [f64; 2]) -> Result<Option<Snapshot>> {
        if !self.active(core) {
            return Err(Error::EditBusy);
        }
        if !self.screen.inside(point) {
            return Err(Error::Invalid("joint pointer outside viewport".into()));
        }
        if !self.moved
            && (point[0] - self.origin[0]).hypot(point[1] - self.origin[1]) / self.scale < 3.
        {
            return Ok(None);
        }
        self.moved = true;
        let Some(delta) = self.mapping.delta(&self.screen, point) else {
            return Ok(None);
        };
        match self.edit {
            JointEdit::Angle => {
                self.angle = (self.angle + delta).clamp(self.range[0] as f64, self.range[1] as f64);
                core.preview_joint(
                    OWNER,
                    self.token,
                    self.revision,
                    self.project,
                    self.angle as f32,
                )
                .map(Some)
            }
            JointEdit::Lower | JointEdit::Upper => {
                // Keep a representable strict ordering, including very narrow authored intervals near ±pi.
                let (lo, hi) = if self.edit == JointEdit::Lower {
                    (
                        -std::f32::consts::PI as f64,
                        self.range[1].next_down() as f64,
                    )
                } else {
                    (self.range[0].next_up() as f64, std::f32::consts::PI as f64)
                };
                self.angle = (self.angle + delta).clamp(lo, hi);
                let mut range = self.range;
                range[if self.edit == JointEdit::Lower { 0 } else { 1 }] = self.angle as f32;
                core.preview_joint_range(OWNER, self.token, self.revision, self.project, range)
                    .map(Some)
            }
        }
    }
    pub fn commit(&self, core: &mut AppCore) -> Result<Snapshot> {
        core.commit_joint(OWNER, self.token, self.revision, self.project)
    }
    pub fn cancel(&self, core: &mut AppCore) -> Result<Snapshot> {
        if self.active(core) {
            core.cancel_joint(OWNER, self.token)
        } else {
            Ok(core.snapshot())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(yaw: f32, scale: f64) -> (AppCore, View, [u32; 4], JointRanges, Screen) {
        let project =
            serde_json::from_str(include_str!("../../../../templates/humanoid.atom.json")).unwrap();
        let core = AppCore::new(project).unwrap();
        let id = core
            .project()
            .objects
            .iter()
            .find(|o| o.label == "left_foot")
            .unwrap()
            .id;
        let origin = core.joint_ranges(id).unwrap().origin;
        let view = View {
            selected: Some(id),
            target: origin,
            distance: 1.,
            yaw,
            pitch: 0.,
            ..Default::default()
        };
        let region = [
            (23. * scale) as u32,
            (61. * scale) as u32,
            (800. * scale) as u32,
            (600. * scale) as u32,
        ];
        let range = range_view(&core, &view, 600.).unwrap();
        let screen = Screen::new(&view, region, scale).unwrap();
        (core, view, region, range, screen)
    }
    fn at(screen: &Screen, range: &JointRanges, phase: f32) -> [f64; 2] {
        screen.project(arc_point(range, 0, phase).unwrap()).unwrap()
    }
    fn angle(core: &AppCore, id: u32) -> f32 {
        core.snapshot()
            .robots
            .iter()
            .flat_map(|r| &r.links)
            .find(|l| l.object_id == id)
            .unwrap()
            .angle(0)
    }
    #[test]
    fn arc_drag_updates_before_release_clamps_and_is_one_undo_step() {
        let (mut core, view, region, range, screen) = fixture(0., 1.);
        let id = view.selected.unwrap();
        let original = core.snapshot();
        let press = at(&screen, &range, 0.1);
        let hit = hit_test(&core, &view, region, 1., press).unwrap();
        assert_eq!(hit.axis, 0);
        assert_eq!(hit.handle, Handle::Arc);
        let (mut drag, _) = NativeDrag::begin(&mut core, &view, 5, region, 1., press)
            .unwrap()
            .unwrap();
        drag.preview(&mut core, press).unwrap();
        assert_eq!(angle(&core, id), 0.);
        for phase in [0.2, 0.3, 0.5, 0.8] {
            drag.preview(&mut core, at(&screen, &range, phase)).unwrap();
            assert_eq!(core.snapshot().undo_depth, 0);
        }
        assert_eq!(angle(&core, id), range.arcs[0].range[1]);
        // A limit must not accumulate excess mouse movement that delays reversing direction.
        drag.preview(&mut core, at(&screen, &range, 0.7)).unwrap();
        assert!((angle(&core, id) - 0.25).abs() < 1e-4);
        drag.preview(&mut core, at(&screen, &range, 0.)).unwrap();
        assert_eq!(angle(&core, id), range.arcs[0].range[0]);
        drag.preview(&mut core, at(&screen, &range, 0.1)).unwrap();
        assert!((angle(&core, id) + 0.25).abs() < 1e-4);
        let preview = core.snapshot();
        assert_ne!(preview.objects, original.objects);
        assert_eq!(preview.revision, original.revision);
        assert!(!preview.dirty);
        drag.commit(&mut core).unwrap();
        assert_eq!(core.snapshot().undo_depth, 1);
        assert_eq!(core.snapshot().objects, preview.objects);
        for command in [atom_core::Command::Undo, atom_core::Command::Redo] {
            let s = core.snapshot();
            core.apply(atom_core::EditRequest {
                expected_revision: s.revision,
                project_id: s.project_id,
                command,
            })
            .unwrap();
        }
        assert_eq!(core.snapshot().objects, preview.objects);
    }
    #[test]
    fn shaft_drag_and_edge_on_arc_are_stable_at_retina_scale() {
        for scale in [1., 2.] {
            let (mut core, view, region, range, screen) =
                fixture(std::f32::consts::FRAC_PI_2, scale);
            let [a, b] = axis_line(&range, 0).unwrap();
            let point = screen
                .project(std::array::from_fn(|i| a[i] * 0.1 + b[i] * 0.9))
                .unwrap();
            let hit = hit_test(&core, &view, region, scale, point).unwrap();
            assert_eq!(hit.handle, Handle::Axis);
            let (mut drag, _) = NativeDrag::begin(&mut core, &view, 0, region, scale, point)
                .unwrap()
                .unwrap();
            let Mapping::Screen {
                direction,
                per_radian,
                ..
            } = drag.mapping
            else {
                panic!("shaft needs screen-space mapping");
            };
            let target = [
                point[0] + direction[0] * per_radian * 0.2,
                point[1] + direction[1] * per_radian * 0.2,
            ];
            drag.preview(&mut core, target).unwrap();
            assert!((angle(&core, view.selected.unwrap()) - 0.2).abs() < 1e-4);
            drag.cancel(&mut core).unwrap();
            assert_eq!(core.snapshot().undo_depth, 0);
            assert_eq!(angle(&core, view.selected.unwrap()), 0.);
            let point = at(&screen, &range, 0.25);
            let hit = hit_test(&core, &view, region, scale, point).unwrap();
            assert_eq!(hit.handle, Handle::Arc);
            let (mut drag, _) = NativeDrag::begin(&mut core, &view, 0, region, scale, point)
                .unwrap()
                .unwrap();
            assert!(matches!(drag.mapping, Mapping::Screen { .. }));
            drag.preview(&mut core, [point[0] + 4. * scale, point[1] - 4. * scale])
                .unwrap();
            assert!(angle(&core, view.selected.unwrap()).is_finite());
            drag.cancel(&mut core).unwrap();
        }
    }
    #[test]
    fn no_jump_noop_cancel_and_stale_drag_do_not_touch_a_new_owner() {
        let (mut core, view, region, range, screen) = fixture(0., 1.);
        let original = core.snapshot();
        let point = at(&screen, &range, -0.2);
        let (mut drag, _) = NativeDrag::begin(&mut core, &view, 9, region, 1., point)
            .unwrap()
            .unwrap();
        drag.preview(&mut core, [point[0] + 0.5, point[1]]).unwrap();
        drag.commit(&mut core).unwrap();
        assert_eq!(core.snapshot().undo_depth, 0);
        assert_eq!(core.snapshot().objects, original.objects);
        let (mut drag, _) = NativeDrag::begin(&mut core, &view, 9, region, 1., point)
            .unwrap()
            .unwrap();
        drag.preview(&mut core, at(&screen, &range, 0.1)).unwrap();
        drag.cancel(&mut core).unwrap();
        assert_eq!(core.snapshot().objects, original.objects);
        assert!(!core.snapshot().dirty);
        let newer = core
            .begin_joint(
                "scene",
                original.revision,
                original.project_id,
                view.selected.unwrap(),
                0,
            )
            .unwrap();
        let token = newer.joint_preview.unwrap().token;
        assert!(drag.preview(&mut core, point).is_err());
        assert!(drag.commit(&mut core).is_err());
        drag.cancel(&mut core).unwrap();
        assert_eq!(core.joint_preview_info().unwrap().token, token);
        core.cancel_joint("scene", token).unwrap();
        let (mut drag, _) = NativeDrag::begin(&mut core, &view, 9, region, 1., point)
            .unwrap()
            .unwrap();
        assert!(!drag.view_matches(&View {
            yaw: 0.1,
            ..view.clone()
        }));
        assert!(drag.preview(&mut core, [-1., -1.]).is_err());
        drag.cancel(&mut core).unwrap();
        assert_eq!(core.snapshot().objects, original.objects);
    }
    #[test]
    fn projection_hit_testing_respects_offsets_clipping_and_dpi() {
        for scale in [1., 2.] {
            let (core, view, region, range, screen) = fixture(0., scale);
            let center = screen.project(range.origin).unwrap();
            assert!(hit_test(&core, &view, region, scale, center).is_none());
            assert!(hit_test(&core, &view, region, scale, [-1., -1.]).is_none());
            assert!(hit_test(&core, &view, region, scale, [f64::NAN, 1.]).is_none());
            let point = at(&screen, &range, 0.2);
            assert_eq!(
                hit_test(&core, &view, region, scale, point).unwrap().axis,
                0
            );
            assert!(Screen::new(&view, [0, 0, 0, 600], scale).is_none());
            let behind = View {
                target: std::array::from_fn(|i| view.target[i] - if i == 2 { 2. } else { 0. }),
                ..view.clone()
            };
            assert!(hit_test(&core, &behind, region, scale, point).is_none());
            let near = Screen::new(&view, region, scale).unwrap();
            let eye = view.eye();
            assert!(
                near.segment([eye[0], eye[1], eye[2] + 1.], [eye[0], eye[1], eye[2] + 2.])
                    .is_none()
            );
        }
    }
    #[test]
    fn plane_mapping_unwraps_pi_and_does_not_return_nan_at_a_singularity() {
        let (_, _, _, range, screen) = fixture(0., 1.);
        let arc = &range.arcs[0];
        let plane = Plane {
            origin: range.origin.map(f64::from),
            axis: arc.axis.map(f64::from),
            zero: arc.zero.map(f64::from),
            tangent: cross(arc.axis.map(f64::from), arc.zero.map(f64::from)),
        };
        let point = at(&screen, &range, 3.1);
        let mut mapping = Mapping::Plane {
            last: plane.phase(&screen, point),
            plane,
        };
        let delta = mapping.delta(&screen, at(&screen, &range, -3.1)).unwrap();
        assert!((delta - (std::f64::consts::TAU - 6.2)).abs() < 1e-4);
        assert!(
            mapping
                .delta(&screen, screen.project(range.origin).unwrap())
                .is_none()
        );
        assert!(mapping.delta(&screen, point).is_none());
        assert!(
            mapping
                .delta(&screen, at(&screen, &range, 3.0))
                .unwrap()
                .is_finite()
        );
    }
    #[test]
    fn lower_and_upper_grips_edit_bounds_clamp_pose_and_commit_one_history() {
        for (bound, edit) in [(0, JointEdit::Lower), (1, JointEdit::Upper)] {
            let (mut core, view, region, range, screen) = fixture(0., 1.);
            let id = view.selected.unwrap();
            let original = core.snapshot();
            let [a, b] = limit_handle(&range, 0, bound).unwrap();
            let press = screen
                .project(std::array::from_fn(|j| (a[j] + b[j]) * 0.5))
                .unwrap();
            assert_eq!(
                hit_test(&core, &view, region, 1., press).unwrap().handle,
                if bound == 0 {
                    Handle::Lower
                } else {
                    Handle::Upper
                }
            );
            let (mut drag, _) = NativeDrag::begin(&mut core, &view, 0, region, 1., press)
                .unwrap()
                .unwrap();
            assert_eq!(core.joint_preview_info().unwrap().edit, edit);
            let target = if bound == 0 { 0.1 } else { -0.1 };
            drag.preview(&mut core, at(&screen, &range, target))
                .unwrap();
            let state = core.snapshot();
            let link = state.robots[0]
                .links
                .iter()
                .find(|l| l.object_id == id)
                .unwrap();
            assert!((link.range[bound] - target).abs() < 1e-4);
            assert!((link.angle(0) - target).abs() < 1e-4);
            assert_eq!(link.range[1 - bound], range.arcs[0].range[1 - bound]);
            assert_eq!(state.undo_depth, 0);
            drag.commit(&mut core).unwrap();
            assert_eq!(core.snapshot().undo_depth, 1);
            let saved = core.snapshot();
            let now = core.snapshot();
            core.apply(atom_core::EditRequest {
                expected_revision: now.revision,
                project_id: now.project_id,
                command: atom_core::Command::Undo,
            })
            .unwrap();
            assert_eq!(core.snapshot().objects, original.objects);
            assert_eq!(core.project().robots, original.robots);
            let now = core.snapshot();
            core.apply(atom_core::EditRequest {
                expected_revision: now.revision,
                project_id: now.project_id,
                command: atom_core::Command::Redo,
            })
            .unwrap();
            assert_eq!(core.project().robots, saved.robots);
        }
    }
    #[test]
    fn limit_handles_do_not_cross_and_cancel_or_noop_restores_everything() {
        let (mut core, view, region, range, screen) = fixture(0., 1.);
        let original = core.snapshot();
        let [a, b] = limit_handle(&range, 0, 0).unwrap();
        let press = screen
            .project(std::array::from_fn(|j| (a[j] + b[j]) * 0.5))
            .unwrap();
        let (mut drag, _) = NativeDrag::begin(&mut core, &view, 0, region, 1., press)
            .unwrap()
            .unwrap();
        drag.preview(&mut core, at(&screen, &range, 0.7)).unwrap();
        let link = core.snapshot().robots[0]
            .links
            .iter()
            .find(|l| l.object_id == view.selected.unwrap())
            .unwrap()
            .clone();
        assert!(link.range[0] < link.range[1]);
        assert!((link.range[0] - link.range[1]).abs() < 1e-5);
        drag.preview(&mut core, at(&screen, &range, 0.6)).unwrap();
        let lower = core.snapshot().robots[0]
            .links
            .iter()
            .find(|l| l.object_id == view.selected.unwrap())
            .unwrap()
            .range[0];
        assert!((lower - 0.25).abs() < 1e-4);
        drag.cancel(&mut core).unwrap();
        assert_eq!(core.snapshot().objects, original.objects);
        assert_eq!(core.project().robots, original.robots);
        assert_eq!(core.snapshot().undo_depth, 0);
        let (drag, _) = NativeDrag::begin(&mut core, &view, 0, region, 1., press)
            .unwrap()
            .unwrap();
        drag.commit(&mut core).unwrap();
        assert_eq!(core.snapshot().undo_depth, 0);
        assert!(!core.snapshot().dirty);
    }
    #[test]
    fn full_turn_grips_remain_separate_and_narrow_edge_intervals_do_not_panic() {
        let (mut core, view, region, _, screen) = fixture(0., 1.);
        let id = view.selected.unwrap();
        let s = core.snapshot();
        core.apply(atom_core::EditRequest {
            expected_revision: s.revision,
            project_id: s.project_id,
            command: atom_core::Command::SetJointRange {
                id,
                axis: 0,
                range: [-std::f32::consts::PI, std::f32::consts::PI],
            },
        })
        .unwrap();
        let range = range_view(&core, &view, 600.).unwrap();
        for bound in 0..2 {
            let [a, b] = limit_handle(&range, 0, bound).unwrap();
            let press = screen
                .project(std::array::from_fn(|j| (a[j] + b[j]) * 0.5))
                .unwrap();
            let hit = hit_test(&core, &view, region, 1., press).unwrap();
            assert_eq!(
                hit.handle,
                if bound == 0 {
                    Handle::Lower
                } else {
                    Handle::Upper
                }
            );
        }
        let s = core.snapshot();
        core.apply(atom_core::EditRequest {
            expected_revision: s.revision,
            project_id: s.project_id,
            command: atom_core::Command::SetJointRange {
                id,
                axis: 0,
                range: [std::f32::consts::PI.next_down(), std::f32::consts::PI],
            },
        })
        .unwrap();
        let range = range_view(&core, &view, 600.).unwrap();
        let [a, b] = limit_handle(&range, 0, 1).unwrap();
        let press = screen
            .project(std::array::from_fn(|j| (a[j] + b[j]) * 0.5))
            .unwrap();
        let (mut drag, _) = NativeDrag::begin(&mut core, &view, 0, region, 1., press)
            .unwrap()
            .unwrap();
        drag.preview(&mut core, at(&screen, &range, 2.8)).unwrap();
        core.snapshot().robots[0]
            .validate(&core.project().objects)
            .unwrap();
        drag.cancel(&mut core).unwrap();
    }
}

#[cfg(test)]
mod articulated_tests {
    use super::*;
    #[test]
    fn multi_axis_joint_drag_uses_the_rotated_parent_frame() {
        let project =
            serde_json::from_str(include_str!("../../../../templates/humanoid.atom.json")).unwrap();
        let mut core = AppCore::new(project).unwrap();
        for (label, axis, angle) in [
            ("lumbar_1", 1, 0.08),
            ("left_upper_arm", 0, 0.5),
            ("left_forearm", 1, 0.4),
        ] {
            let id = core
                .project()
                .objects
                .iter()
                .find(|o| o.label == label)
                .unwrap()
                .id;
            let s = core.snapshot();
            core.apply(atom_core::EditRequest {
                expected_revision: s.revision,
                project_id: s.project_id,
                command: atom_core::Command::SetJointAngle { id, axis, angle },
            })
            .unwrap();
        }
        let id = core
            .project()
            .objects
            .iter()
            .find(|o| o.label == "left_palm")
            .unwrap()
            .id;
        let region = [31, 79, 700, 600];
        for axis in 0..2 {
            let initial = core.snapshot();
            let frame = core.joint_ranges(id).unwrap();
            let normal = frame.axes[axis].axis;
            let view = View {
                selected: Some(id),
                target: frame.origin,
                distance: 0.6,
                yaw: normal[0].atan2(normal[2]),
                pitch: normal[1].asin(),
                ..Default::default()
            };
            let ranges = range_view(&core, &view, 600.).unwrap();
            let screen = Screen::new(&view, region, 1.).unwrap();
            let phase = [-0.2f32, 0.1, 0.2, 0.3]
                .into_iter()
                .find(|p| {
                    hit_test(
                        &core,
                        &view,
                        region,
                        1.,
                        screen
                            .project(arc_point(&ranges, axis, *p).unwrap())
                            .unwrap(),
                    )
                    .is_some_and(|h| h.axis == axis)
                })
                .unwrap();
            let point = screen
                .project(arc_point(&ranges, axis, phase).unwrap())
                .unwrap();
            let (mut drag, _) = NativeDrag::begin(&mut core, &view, 0, region, 1., point)
                .unwrap()
                .unwrap();
            drag.preview(
                &mut core,
                screen
                    .project(arc_point(&ranges, axis, phase + 0.1).unwrap())
                    .unwrap(),
            )
            .unwrap();
            let state = core.snapshot();
            let link = state.robots[0]
                .links
                .iter()
                .find(|l| l.object_id == id)
                .unwrap();
            assert!((link.angle(axis) - 0.1).abs() < 1e-4);
            assert_eq!(link.angle(1 - axis), 0.);
            drag.cancel(&mut core).unwrap();
            assert_eq!(core.snapshot().objects, initial.objects);
        }
    }
}
