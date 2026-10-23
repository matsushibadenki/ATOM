//! Rigid forward kinematics; rest geometry and world-space joint pivots stay authored.
mod ik;
use super::*;
use crate::{Project, Transform};
use std::collections::BTreeMap;
type Matrix = [[f64; 3]; 3];
const ID: Matrix = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
fn mul(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum()))
}
fn vector(a: Matrix, b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|r| (0..3).map(|k| a[r][k] * b[k]).sum())
}
fn rotation(axis: [f32; 3], angle: f32) -> Matrix {
    let [x, y, z] = axis.map(f64::from);
    let (s, c) = f64::from(angle).sin_cos();
    let t = 1. - c;
    [
        [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
        [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
        [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
    ]
}
fn from_euler(e: [f32; 3]) -> Matrix {
    mul(
        mul(rotation([1., 0., 0.], e[0]), rotation([0., 1., 0.], e[1])),
        rotation([0., 0., 1.], e[2]),
    )
}
fn euler(r: Matrix) -> [f32; 3] {
    let y = r[0][2].clamp(-1., 1.).asin();
    let (x, z) = if y.cos().abs() > 1e-7 {
        ((-r[1][2]).atan2(r[2][2]), (-r[0][1]).atan2(r[0][0]))
    } else {
        (r[2][1].atan2(r[1][1]), 0.)
    };
    [x as f32, y as f32, z as f32]
}
#[derive(Clone, Copy)]
struct Rigid {
    r: Matrix,
    t: [f64; 3],
}
impl Rigid {
    fn point(self, p: [f32; 3]) -> [f32; 3] {
        let v = vector(self.r, p.map(f64::from));
        std::array::from_fn(|i| (v[i] + self.t[i]) as f32)
    }
    fn inverse(self) -> Self {
        let r = std::array::from_fn(|i| std::array::from_fn(|j| self.r[j][i]));
        Self {
            r,
            t: vector(r, self.t).map(|v| -v),
        }
    }
}
fn frames(project: &Project) -> BTreeMap<u32, Rigid> {
    let mut frames = BTreeMap::new();
    for robot in &project.robots {
        for link in &robot.links {
            let parent = link
                .parent
                .and_then(|p| frames.get(&p).copied())
                .unwrap_or(Rigid { r: ID, t: [0.; 3] });
            let mut local = ID;
            if link.parent.is_some() {
                for (i, a) in link.axes().enumerate() {
                    local = mul(local, rotation(a.axis, link.angle(i)));
                }
            }
            let pivot = link.anchor.map(f64::from);
            let rotated = vector(local, pivot);
            let shift = vector(parent.r, std::array::from_fn(|i| pivot[i] - rotated[i]));
            frames.insert(
                link.object_id,
                Rigid {
                    r: mul(parent.r, local),
                    t: std::array::from_fn(|i| parent.t[i] + shift[i]),
                },
            );
        }
    }
    frames
}
impl Project {
    /// Derived world transforms used for rendering, selection and snapshots. Never saved as rest geometry.
    pub fn posed_objects(&self) -> Vec<SceneObject> {
        let frames = frames(self);
        self.objects
            .iter()
            .map(|o| {
                let mut posed = o.clone();
                if let Some(f) = frames.get(&o.id)
                    && (f.r != ID || f.t != [0.; 3])
                {
                    posed.position = f.point(o.position);
                    posed.rotation = euler(mul(f.r, from_euler(o.rotation)));
                }
                posed
            })
            .collect()
    }
    /// Convert an editor world-space geometry edit back into the unposed document frame.
    pub(crate) fn rest_transform(&self, id: u32, world: &Transform) -> Transform {
        let mut rest = world.clone();
        if let Some(f) = frames(self).get(&id)
            && (f.r != ID || f.t != [0.; 3])
        {
            let inverse = f.inverse();
            rest.position = inverse.point(world.position);
            rest.rotation = euler(mul(inverse.r, from_euler(world.rotation)));
        }
        rest
    }
}

#[derive(Clone, Debug)]
pub struct JointRangeAxis {
    pub channel: usize,
    pub axis: [f32; 3],
    pub zero: [f32; 3],
    pub range: [f32; 2],
    pub angle: f32,
}
#[derive(Clone, Debug)]
pub struct JointRange {
    pub origin: [f32; 3],
    pub axes: Vec<JointRangeAxis>,
}
impl Project {
    /// Axis i is expressed after the parent pose and the preceding intrinsic axes, not after itself.
    pub fn joint_ranges(&self, id: u32) -> Option<JointRange> {
        let link = self
            .robots
            .iter()
            .flat_map(|r| &r.links)
            .find(|l| l.object_id == id && l.parent.is_some())?;
        let frames = frames(self);
        let parent = frames.get(&link.parent?)?;
        let origin = parent.point(link.anchor);
        let mut basis = parent.r;
        let mut axes = Vec::new();
        for (i, a) in link.axes().enumerate() {
            let reference = if a.axis[0].abs() < 0.8 {
                [1., 0., 0.]
            } else {
                [0., 1., 0.]
            };
            let dot: f64 = a
                .axis
                .iter()
                .zip(reference)
                .map(|(x, y)| f64::from(*x) * y)
                .sum();
            let zero: [f64; 3] = std::array::from_fn(|j| reference[j] - f64::from(a.axis[j]) * dot);
            let norm = zero.iter().map(|v| v * v).sum::<f64>().sqrt();
            axes.push(JointRangeAxis {
                channel: a
                    .axis
                    .iter()
                    .enumerate()
                    .max_by(|(_, x), (_, y)| x.abs().total_cmp(&y.abs()))
                    .unwrap()
                    .0,
                axis: vector(basis, a.axis.map(f64::from)).map(|v| v as f32),
                zero: vector(basis, zero.map(|v| v / norm)).map(|v| v as f32),
                range: a.range,
                angle: link.angle(i),
            });
            basis = mul(basis, rotation(a.axis, link.angle(i)));
        }
        Some(JointRange { origin, axes })
    }
}
