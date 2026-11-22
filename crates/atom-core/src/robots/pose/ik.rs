//! Bounded, deterministic position and pose IK with damped least-squares refinement.
use super::ik_collision::{CollisionModel, Environment};
use super::*;
use crate::{Error, Result};

const TOLERANCE: f64 = 0.001; // meters, measured again with the same f32 FK as the renderer
const ANGULAR_TOLERANCE: f64 = 0.5 * std::f64::consts::PI / 180.;
const ORIENTATION_WEIGHT: f64 = 0.25; // meters per radian in the least-squares objective
const COLLISION_WEIGHT: f64 = 16.;
const MAX_UPDATES: usize = 6000;
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
#[derive(Clone, Copy)]
struct Goal {
    id: u32,
    center: [f32; 3],
    rest_rotation: Matrix,
    target: [f32; 3],
    orientation: Option<Matrix>,
}
impl Goal {
    fn residual(self, frame: Rigid) -> [f64; 6] {
        let [x, y, z] = sub(
            self.target.map(f64::from),
            frame.point(self.center).map(f64::from),
        );
        let [rx, ry, rz] = self.orientation.map_or([0.; 3], |target| {
            let current = mul(frame.r, self.rest_rotation);
            let inverse = std::array::from_fn(|i| std::array::from_fn(|j| current[j][i]));
            rotation_error(mul(target, inverse)).map(|v| v * ORIENTATION_WEIGHT)
        });
        [x, y, z, rx, ry, rz]
    }
    fn reached(error: [f64; 6]) -> bool {
        norm(&error[..3]) <= TOLERANCE
            && norm(&error[3..]) <= ANGULAR_TOLERANCE * ORIENTATION_WEIGHT
    }
}
fn norm(values: &[f64]) -> f64 {
    values.iter().map(|v| v * v).sum::<f64>().sqrt()
}
/// Logarithm of the relative rotation, expressed about a world-space axis.
fn rotation_error(r: Matrix) -> [f64; 3] {
    let skew = [r[2][1] - r[1][2], r[0][2] - r[2][0], r[1][0] - r[0][1]];
    let sine = norm(&skew) / 2.;
    let cosine = ((r[0][0] + r[1][1] + r[2][2] - 1.) / 2.).clamp(-1., 1.);
    let angle = sine.atan2(cosine);
    if sine > 1e-5 {
        return skew.map(|v| v * angle / (2. * sine));
    }
    if cosine >= 0. {
        return skew.map(|v| v / 2.);
    }
    // Near pi the skew vanishes. Recover the axis from the symmetric part.
    let i = (0..3).max_by(|&i, &j| r[i][i].total_cmp(&r[j][j])).unwrap();
    let diagonal = ((r[i][i] - cosine) / (1. - cosine)).sqrt();
    let axis = std::array::from_fn(|j| {
        if i == j {
            diagonal
        } else {
            (r[i][j] + r[j][i]) / (2. * (1. - cosine) * diagonal)
        }
    });
    let sign = if dot(axis, skew) < 0. { -1. } else { 1. };
    axis.map(|v| v * angle * sign)
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
    chain: &[(usize, usize, f64)],
    goal: Goal,
    collision: Option<&CollisionModel>,
) {
    let fs = frames(work);
    let end = fs[&goal.id].point(goal.center);
    let error = goal.residual(fs[&goal.id]);
    let initial_score = score(error, collision, &fs);
    let angular_weight = if goal.orientation.is_some() {
        ORIENTATION_WEIGHT
    } else {
        0.
    };
    let columns: Vec<_> = chain
        .iter()
        .map(|&(index, axis_index, weight)| {
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
            let [x, y, z] = cross(
                axis,
                sub(end.map(f64::from), parent.point(link.anchor).map(f64::from)),
            );
            let [rx, ry, rz] = axis.map(|v| v * angular_weight);
            [x, y, z, rx, ry, rz].map(|v| v * weight.sqrt())
        })
        .collect();
    let mut augmented: [[f64; 7]; 6] = std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            if c == 6 {
                error[r]
            } else {
                columns.iter().map(|j| j[r] * j[c]).sum::<f64>() + if r == c { 1e-4 } else { 0. }
            }
        })
    });
    for pivot in 0..6 {
        let row = (pivot..6)
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
    let correction: [f64; 6] = std::array::from_fn(|i| augmented[i][6]);
    let deltas: Vec<f64> = columns
        .iter()
        .map(|j| j.iter().zip(correction).map(|(a, b)| a * b).sum())
        .collect();
    let scale = 0.25 / deltas.iter().map(|v| v.abs()).fold(0.25, f64::max);
    let before = work.robots[robot].clone();
    for attempt in 0..6 {
        work.robots[robot] = before.clone();
        let scale = scale * 0.5f64.powi(attempt);
        for (&(index, axis, weight), delta) in chain.iter().zip(&deltas) {
            let link = &mut work.robots[robot].links[index];
            let range = link.axes().nth(axis).unwrap().range;
            let value = (f64::from(link.angle(axis)) + scale * delta * weight.sqrt())
                .clamp(f64::from(range[0]), f64::from(range[1])) as f32;
            assign(link, axis, value);
        }
        let candidate = frames(work);
        if score(goal.residual(candidate[&goal.id]), collision, &candidate) < initial_score {
            return;
        }
    }
    work.robots[robot] = before;
}
fn score(
    error: [f64; 6],
    collision: Option<&CollisionModel>,
    frames: &BTreeMap<u32, Rigid>,
) -> f64 {
    let task = error.into_iter().map(|v| v * v).sum::<f64>();
    (task + collision.map_or(0., |model| COLLISION_WEIGHT * model.penalty(frames))).sqrt()
}

fn orientation_matrix(orientation: [f32; 3]) -> Result<Matrix> {
    if orientation
        .iter()
        .any(|v| !v.is_finite() || v.abs() > std::f32::consts::TAU)
    {
        return Err(Error::Invalid("IK orientation".into()));
    }
    Ok(from_euler(orientation))
}

/// Sample the straight segment in joint space, including both endpoints.
fn validate_path(
    mut work: Project,
    robot: usize,
    chain: &[(usize, usize, f64)],
    start: &Robot,
    goal: Goal,
    model: &CollisionModel,
    environment: &Environment,
) -> Result<IkPathReport> {
    let changes: Vec<_> = chain
        .iter()
        .map(|&(index, axis, _)| {
            let a = start.links[index].angle(axis);
            let b = work.robots[robot].links[index].angle(axis);
            (index, axis, a, b)
        })
        .collect();
    let travel = changes
        .iter()
        .map(|&(_, _, a, b)| f64::from((b - a).abs()))
        .sum::<f64>();
    let samples = travel.to_degrees().ceil() as usize;
    if samples > 720 {
        return Err(Error::IkPathBudget);
    }
    let mut points = Vec::with_capacity(samples + 1);
    for step in 0..=samples {
        let fraction = if samples == 0 {
            0.
        } else {
            step as f32 / samples as f32
        };
        for &(index, axis, a, b) in &changes {
            assign(
                &mut work.robots[robot].links[index],
                axis,
                a * (1. - fraction) + b * fraction,
            );
        }
        let fs = frames(&work);
        points.push(fs[&goal.id].point(goal.center));
        if let Some((impact, marker)) = model.impact(&fs, environment) {
            return Ok(IkPathReport {
                points,
                impact: Some(impact),
                fraction,
                marker: Some(marker),
            });
        }
    }
    Ok(IkPathReport {
        points,
        impact: None,
        fraction: 1.,
        marker: None,
    })
}

impl Project {
    /// Solve on a private candidate; an unsuccessful search cannot change the document.
    pub(crate) fn solve_ik(&mut self, id: u32, base: u32, target: [f32; 3]) -> Result<bool> {
        self.solve_ik_weighted(id, base, target, |_, _| 1.)
    }

    pub(crate) fn solve_ik_weighted(
        &mut self,
        id: u32,
        base: u32,
        target: [f32; 3],
        weight: impl Fn(u32, usize) -> f64,
    ) -> Result<bool> {
        self.solve_ik_goal(id, base, target, None, false, None, weight)
    }

    pub(crate) fn solve_pose_ik(
        &mut self,
        id: u32,
        base: u32,
        target: [f32; 3],
        orientation: [f32; 3],
    ) -> Result<bool> {
        self.solve_ik_goal(
            id,
            base,
            target,
            Some(orientation_matrix(orientation)?),
            false,
            None,
            |_, _| 1.,
        )
    }

    pub(crate) fn solve_collision_free_ik(
        &mut self,
        id: u32,
        base: u32,
        target: [f32; 3],
        orientation: Option<[f32; 3]>,
    ) -> Result<bool> {
        self.solve_ik_goal(
            id,
            base,
            target,
            orientation.map(orientation_matrix).transpose()?,
            true,
            None,
            |_, _| 1.,
        )
    }

    pub(crate) fn solve_path_ik(
        &mut self,
        id: u32,
        base: u32,
        target: [f32; 3],
        orientation: Option<[f32; 3]>,
        report: &mut Option<IkPathReport>,
    ) -> Result<bool> {
        self.solve_ik_goal(
            id,
            base,
            target,
            orientation.map(orientation_matrix).transpose()?,
            true,
            Some(report),
            |_, _| 1.,
        )
    }

    fn solve_ik_goal(
        &mut self,
        id: u32,
        base: u32,
        target: [f32; 3],
        orientation: Option<Matrix>,
        avoid_collision: bool,
        mut path_report: Option<&mut Option<IkPathReport>>,
        weight: impl Fn(u32, usize) -> f64,
    ) -> Result<bool> {
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
                let mobility = weight(link.object_id, axis);
                if mobility > 0. {
                    chain.push((index, axis, mobility));
                }
            }
        }
        if chain.is_empty() {
            return Err(Error::Invalid("IK base must be a strict ancestor".into()));
        }
        let rest = self
            .objects
            .iter()
            .find(|o| o.id == id)
            .ok_or_else(|| Error::Invalid("IK endpoint".into()))?;
        let goal = Goal {
            id,
            center: rest.position,
            rest_rotation: from_euler(rest.rotation),
            target,
            orientation,
        };
        let collision = avoid_collision
            .then(|| CollisionModel::new(self, robot))
            .transpose()?;
        let initial_frames = frames(self);
        let environment = path_report
            .is_some()
            .then(|| Environment::new(self, robot, &initial_frames));
        if let Some((model, environment)) = collision.as_ref().zip(environment.as_ref())
            && let Some((impact, marker)) = model.impact(&initial_frames, environment)
        {
            *path_report.as_deref_mut().unwrap() = Some(IkPathReport {
                points: vec![initial_frames[&id].point(goal.center)],
                impact: Some(impact),
                fraction: 0.,
                marker: Some(marker),
            });
            return Err(Error::IkPathBlocked);
        }
        let initial_reached = Goal::reached(goal.residual(initial_frames[&id]));
        let mut blocked_goal = initial_reached
            && collision
                .as_ref()
                .is_some_and(|model| model.penalty(&initial_frames) > 0.);
        if initial_reached && !blocked_goal {
            if let Some(report) = path_report {
                *report = Some(IkPathReport {
                    points: vec![initial_frames[&id].point(goal.center)],
                    impact: None,
                    fraction: 1.,
                    marker: None,
                });
            }
            return Ok(false);
        }
        let original = self.robots[robot_index].clone();
        let mut work = self.clone();
        let mut updates = 0;
        let mut path_budget_exceeded = false;
        // Small deterministic restarts escape straight-chain stationary configurations.
        // Each restart starts from the authored pose, never an earlier failed attempt.
        let seeds: &[f32] = if avoid_collision {
            &[0., 0.16, -0.16, 0.55, -0.55, 1.1, -1.1]
        } else {
            &[0., 0.16, -0.16]
        };
        for (restart, &seed) in seeds.iter().enumerate() {
            // First seek an exact goal, then favor lower penetration on later restarts.
            let penalty = collision.as_ref().filter(|_| restart >= 2);
            work.robots[robot_index] = original.clone();
            if seed != 0. {
                for (n, &(index, axis, weight)) in chain.iter().enumerate() {
                    let link = &mut work.robots[robot_index].links[index];
                    let range = link.axes().nth(axis).unwrap().range;
                    let angle = (link.angle(axis)
                        + (if n % 2 == 0 { seed } else { -seed }) * weight as f32)
                        .clamp(range[0], range[1]);
                    assign(link, axis, angle);
                }
            }
            let mut previous = f64::INFINITY;
            let mut stagnant = 0;
            let use_ccd = orientation.is_none() && penalty.is_none();
            let iteration_cost = if !use_ccd { 8 } else { chain.len() + 8 };
            for _ in 0..(MAX_UPDATES / seeds.len() / iteration_cost).min(240) {
                for &(index, axis_index, weight) in chain.iter().filter(|_| use_ccd) {
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
                    let a = sub(fs[&id].point(goal.center).map(f64::from), origin);
                    let b = sub(target.map(f64::from), origin);
                    let a = sub(a, axis.map(|v| v * dot(a, axis)));
                    let b = sub(b, axis.map(|v| v * dot(b, axis)));
                    if dot(a, a) < 1e-16 || dot(b, b) < 1e-16 {
                        continue;
                    }
                    let delta = dot(axis, cross(a, b)).atan2(dot(a, b)).clamp(-0.45, 0.45);
                    let next = (f64::from(link.angle(axis_index)) + delta * weight)
                        .clamp(f64::from(spec.range[0]), f64::from(spec.range[1]))
                        as f32;
                    assign(&mut work.robots[robot_index].links[index], axis_index, next);
                }
                updates += 8; // one Jacobian and at most six line-search FK evaluations
                refine(&mut work, robot_index, &chain, goal, penalty);
                let current_frames = frames(&work);
                let residual = goal.residual(current_frames[&id]);
                if Goal::reached(residual) {
                    if let Some(environment) = &environment {
                        let report = validate_path(
                            work.clone(),
                            robot_index,
                            &chain,
                            &original,
                            goal,
                            collision.as_ref().unwrap(),
                            environment,
                        );
                        match report {
                            Ok(report) if report.impact.is_none() => {
                                *path_report.as_deref_mut().unwrap() = Some(report);
                            }
                            Ok(report) => {
                                let previous = path_report.as_deref_mut().unwrap();
                                if previous
                                    .as_ref()
                                    .is_none_or(|p| p.fraction < report.fraction)
                                {
                                    *previous = Some(report);
                                }
                                break;
                            }
                            Err(Error::IkPathBudget) => {
                                path_budget_exceeded = true;
                                break;
                            }
                            Err(error) => return Err(error),
                        }
                    } else if collision
                        .as_ref()
                        .is_some_and(|model| model.penalty(&current_frames) > 0.)
                    {
                        blocked_goal = true;
                        break;
                    }
                    for link in &mut work.robots[robot_index].links {
                        if link.angles.iter().all(|v| *v == 0.) {
                            link.angles.clear();
                        }
                    }
                    self.robots[robot_index] = work.robots[robot_index].clone();
                    return Ok(true);
                }
                let error = score(residual, penalty, &current_frames);
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
        Err(if path_report.is_some_and(|report| report.is_some()) {
            Error::IkPathBlocked
        } else if path_budget_exceeded {
            Error::IkPathBudget
        } else if blocked_goal {
            Error::IkSelfCollision
        } else {
            Error::IkUnreachable
        })
    }
}
