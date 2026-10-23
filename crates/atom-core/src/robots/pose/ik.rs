//! Bounded, deterministic position-only CCD with damped least-squares refinement. No collision or orientation constraint.
use super::*;
use crate::{Error, Result};

const TOLERANCE: f64 = 0.001; // meters, measured again with the same f32 FK as the renderer
const MAX_UPDATES: usize = 6000;
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn distance(a: [f32; 3], b: [f32; 3]) -> f64 {
    let d = sub(a.map(f64::from), b.map(f64::from));
    dot(d, d).sqrt()
}
fn end_point(project: &Project, id: u32, rest: [f32; 3]) -> [f32; 3] {
    frames(project)[&id].point(rest)
}
fn assign(link: &mut RobotLink, axis: usize, angle: f32) {
    if link.angles.is_empty() {
        link.angles = vec![0.; link.axes().count()];
    }
    link.angles[axis] = angle;
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
/// A damped least-squares correction accelerates the slow final part of CCD.
/// Backtracking accepts only an improvement after projecting onto joint limits.
fn refine(
    work: &mut Project,
    robot: usize,
    chain: &[(usize, usize)],
    id: u32,
    rest: [f32; 3],
    target: [f32; 3],
) {
    let fs = frames(work);
    let end = fs[&id].point(rest);
    let error = sub(target.map(f64::from), end.map(f64::from));
    let columns: Vec<_> = chain
        .iter()
        .map(|&(index, axis_index)| {
            let link = &work.robots[robot].links[index];
            let parent = fs[&link.parent.unwrap()];
            let mut basis = parent.r;
            for (i, spec) in link.axes().take(axis_index).enumerate() {
                basis = mul(basis, rotation(spec.axis, link.angle(i)));
            }
            let axis = vector(
                basis,
                link.axes().nth(axis_index).unwrap().axis.map(f64::from),
            );
            cross(
                axis,
                sub(end.map(f64::from), parent.point(link.anchor).map(f64::from)),
            )
        })
        .collect();
    let mut augmented: [[f64; 4]; 3] = std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            if c == 3 {
                error[r]
            } else {
                columns.iter().map(|j| j[r] * j[c]).sum::<f64>() + if r == c { 1e-4 } else { 0. }
            }
        })
    });
    for pivot in 0..3 {
        let row = (pivot..3)
            .max_by(|&a, &b| {
                augmented[a][pivot]
                    .abs()
                    .total_cmp(&augmented[b][pivot].abs())
            })
            .unwrap();
        augmented.swap(pivot, row);
        let divisor = augmented[pivot][pivot];
        for value in &mut augmented[pivot][pivot..] {
            *value /= divisor;
        }
        let pivot_row = augmented[pivot];
        for (r, row) in augmented.iter_mut().enumerate() {
            if r != pivot {
                let factor = row[pivot];
                for (value, source) in row[pivot..].iter_mut().zip(&pivot_row[pivot..]) {
                    *value -= factor * source;
                }
            }
        }
    }
    let correction = std::array::from_fn(|i| augmented[i][3]);
    let deltas: Vec<_> = columns.iter().map(|j| dot(*j, correction)).collect();
    let scale = 0.25 / deltas.iter().map(|v| v.abs()).fold(0.25, f64::max);
    let before = work.robots[robot].clone();
    for attempt in 0..6 {
        work.robots[robot] = before.clone();
        let scale = scale * 0.5f64.powi(attempt);
        for (&(index, axis), delta) in chain.iter().zip(&deltas) {
            let link = &mut work.robots[robot].links[index];
            let range = link.axes().nth(axis).unwrap().range;
            let value = (f64::from(link.angle(axis)) + scale * delta)
                .clamp(f64::from(range[0]), f64::from(range[1])) as f32;
            assign(link, axis, value);
        }
        if distance(end_point(work, id, rest), target) < dot(error, error).sqrt() {
            return;
        }
    }
    work.robots[robot] = before;
}
impl Project {
    /// Solve on a private candidate; an unsuccessful search cannot change the document.
    pub(crate) fn solve_ik(&mut self, id: u32, base: u32, target: [f32; 3]) -> Result<bool> {
        if target.iter().any(|v| !v.is_finite() || v.abs() > 10_000.) {
            return Err(Error::Invalid("IK target".into()));
        }
        let robot_index = self
            .robots
            .iter()
            .position(|r| r.links.iter().any(|l| l.object_id == id))
            .ok_or_else(|| Error::Invalid("IK robot".into()))?;
        let robot = &self.robots[robot_index];
        let mut chain = Vec::new();
        let mut cursor = id;
        // Reverse intrinsic axes as well as links: distal coordinates first.
        while cursor != base {
            let index = robot
                .links
                .iter()
                .position(|l| l.object_id == cursor)
                .ok_or_else(|| Error::Invalid("IK chain".into()))?;
            let link = &robot.links[index];
            cursor = link
                .parent
                .ok_or_else(|| Error::Invalid("IK base must be a strict ancestor".into()))?;
            for axis in (0..link.axes().count()).rev() {
                chain.push((index, axis));
            }
        }
        if chain.is_empty() {
            return Err(Error::Invalid("IK base must be a strict ancestor".into()));
        }
        let rest = self
            .objects
            .iter()
            .find(|o| o.id == id)
            .ok_or_else(|| Error::Invalid("IK endpoint".into()))?
            .position;
        if distance(end_point(self, id, rest), target) <= TOLERANCE {
            return Ok(false);
        }
        let original = self.robots[robot_index].clone();
        let mut work = self.clone();
        let mut updates = 0;
        // Small deterministic restarts escape straight-chain stationary configurations.
        // Each restart starts from the authored pose, never an earlier failed attempt.
        for seed in [0., 0.16, -0.16] {
            work.robots[robot_index] = original.clone();
            if seed != 0. {
                for (n, &(index, axis)) in chain.iter().enumerate() {
                    let link = &mut work.robots[robot_index].links[index];
                    let range = link.axes().nth(axis).unwrap().range;
                    let angle = (link.angle(axis) + if n % 2 == 0 { seed } else { -seed })
                        .clamp(range[0], range[1]);
                    assign(link, axis, angle);
                }
            }
            let mut previous = f64::INFINITY;
            let mut stagnant = 0;
            for _ in 0..(MAX_UPDATES / 3 / (chain.len() + 8)).min(240) {
                for &(index, axis_index) in &chain {
                    updates += 1;
                    if updates > MAX_UPDATES {
                        return Err(Error::IkUnreachable);
                    }
                    let fs = frames(&work);
                    let link = &work.robots[robot_index].links[index];
                    let parent = fs[&link.parent.unwrap()];
                    let origin = parent.point(link.anchor).map(f64::from);
                    let mut basis = parent.r;
                    for (i, spec) in link.axes().take(axis_index).enumerate() {
                        basis = mul(basis, rotation(spec.axis, link.angle(i)));
                    }
                    let spec = link.axes().nth(axis_index).unwrap();
                    let axis = vector(basis, spec.axis.map(f64::from));
                    let a = sub(fs[&id].point(rest).map(f64::from), origin);
                    let b = sub(target.map(f64::from), origin);
                    let a = sub(a, axis.map(|v| v * dot(a, axis)));
                    let b = sub(b, axis.map(|v| v * dot(b, axis)));
                    if dot(a, a) < 1e-16 || dot(b, b) < 1e-16 {
                        continue;
                    }
                    let delta = dot(axis, cross(a, b)).atan2(dot(a, b)).clamp(-0.45, 0.45);
                    let next = (f64::from(link.angle(axis_index)) + delta)
                        .clamp(f64::from(spec.range[0]), f64::from(spec.range[1]))
                        as f32;
                    assign(&mut work.robots[robot_index].links[index], axis_index, next);
                }
                updates += 8; // one Jacobian and at most six line-search FK evaluations
                refine(&mut work, robot_index, &chain, id, rest, target);
                let error = distance(end_point(&work, id, rest), target);
                if error <= TOLERANCE {
                    for link in &mut work.robots[robot_index].links {
                        if link.angles.iter().all(|v| *v == 0.) {
                            link.angles.clear();
                        }
                    }
                    self.robots[robot_index] = work.robots[robot_index].clone();
                    return Ok(true);
                }
                stagnant = if previous - error < 1e-7 {
                    stagnant + 1
                } else {
                    0
                };
                if stagnant >= 4 {
                    break;
                }
                previous = error;
            }
        }
        Err(Error::IkUnreachable)
    }
}
