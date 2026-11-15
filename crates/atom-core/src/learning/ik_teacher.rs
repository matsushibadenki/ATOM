//! Position IK seeded by the measured joint state, with a reference fallback.
use super::*;
use crate::reach;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointWeight {
    pub id: u32,
    pub axis: usize,
    pub weight: f64,
}

pub(super) fn validate_weights(
    weights: &[JointWeight],
    manifest: Option<&reach::Manifest>,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for joint in weights {
        let in_chain = manifest.is_none_or(|m| {
            if !m
                .channels
                .iter()
                .any(|c| c.id == joint.id && c.axis == joint.axis)
            {
                return false;
            }
            let robot = m
                .project
                .robots
                .iter()
                .find(|r| r.root == m.config.robot)
                .unwrap();
            let mut id = m.endpoint;
            while id != robot.root {
                if id == joint.id {
                    return true;
                }
                let Some(parent) = robot
                    .links
                    .iter()
                    .find(|link| link.object_id == id)
                    .and_then(|link| link.parent)
                else {
                    break;
                };
                id = parent;
            }
            false
        });
        if !in_chain
            || joint.id == 0
            || joint.axis >= 64
            || !seen.insert((joint.id, joint.axis))
            || !joint.weight.is_finite()
            || !(0.01..=1.).contains(&joint.weight)
        {
            return Err(err("learning_ik_weights"));
        }
    }
    if weights.len() > 64 {
        return Err(err("learning_ik_weights"));
    }
    Ok(())
}

pub(super) fn action(
    m: &reach::Manifest,
    o: &reach::Observation,
    weights: &[JointWeight],
) -> (Vec<f64>, bool) {
    let mut pose = m.project.clone();
    let measured = m
        .channels
        .iter()
        .map(|c| o.qpos[c.qpos_index].clamp(f64::from(c.range[0]), f64::from(c.range[1])))
        .collect::<Vec<_>>();
    let mobility = |id, axis| {
        if weights.is_empty() {
            1.
        } else {
            weights
                .iter()
                .find(|j| j.id == id && j.axis == axis)
                .map_or(0., |j| j.weight)
        }
    };
    let fallback = || {
        if weights.is_empty() {
            return (m.reference_action.clone(), true);
        }
        let targets = m
            .channels
            .iter()
            .zip(&measured)
            .zip(&m.reference_action)
            .map(|((c, q), target)| {
                let limit = 0.25 * mobility(c.id, c.axis);
                (q + (target - q).clamp(-limit, limit))
                    .clamp(f64::from(c.range[0]), f64::from(c.range[1]))
            })
            .collect();
        (targets, true)
    };
    for (c, q) in m.channels.iter().zip(&measured) {
        if pose.set_joint_angle(c.id, c.axis, *q as f32).is_err() {
            return fallback();
        }
    }
    let result = pose.solve_ik_weighted(m.endpoint, m.config.robot, m.target, mobility);
    if !matches!(result, Ok(true) | Ok(false)) {
        return fallback();
    }
    let robot = pose
        .robots
        .iter()
        .find(|r| r.root == m.config.robot)
        .unwrap();
    (
        m.channels
            .iter()
            .zip(&measured)
            .map(|(c, q)| {
                // Limit each command's displacement from the observed pose, including idle joints.
                let target = f64::from(
                    robot
                        .links
                        .iter()
                        .find(|l| l.object_id == c.id)
                        .unwrap()
                        .angle(c.axis),
                );
                let limit = 0.25 * mobility(c.id, c.axis);
                (q + (target - q).clamp(-limit, limit))
                    .clamp(f64::from(c.range[0]), f64::from(c.range[1]))
            })
            .collect(),
        false,
    )
}

/// Replays the same applied prefix on both branches, retaining solver history and success counter.
pub fn recovery(root: &Path, mining_id: &str, frame: u32, count: u32) -> Result<serde_json::Value> {
    recovery_many(root, mining_id, &[frame], count, &[])
}

pub fn recovery_many(
    root: &Path,
    mining_id: &str,
    frames: &[u32],
    count: u32,
    weights: &[JointWeight],
) -> Result<serde_json::Value> {
    if !(1..=4).contains(&count)
        || frames.is_empty()
        || frames.len() > 4
        || frames.iter().any(|f| *f >= 500)
        || frames.iter().collect::<BTreeSet<_>>().len() != frames.len()
    {
        return Err(err("learning_recovery_config"));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let check = || {
        if std::time::Instant::now() >= deadline {
            Err(err("learning_recovery_budget"))
        } else {
            Ok(())
        }
    };
    let mined = mining::inspect(root, mining_id)?.report;
    if mined.status != mining::Status::Completed || mined.priorities.len() < count as usize {
        return Err(err("learning_supervision_pool"));
    }
    let mut pairs = vec![];
    for seed in mined.priorities.iter().take(count as usize) {
        let probe = mined.probes.iter().find(|p| p.seed == *seed).unwrap();
        let episode = reach::Episode::load(&root.join(&probe.episode))
            .map_err(|_| err("learning_supervision_invalid"))?;
        if episode
            .quality()
            .map_err(|_| err("learning_supervision_invalid"))?
            .episode_sha256
            != probe.episode_sha256
            || frames.iter().any(|frame| *frame >= probe.steps)
        {
            return Err(err("learning_recovery_config"));
        }
        validate_weights(weights, Some(&episode.manifest))?;
        for &frame in frames {
            let mut branches = vec![];
            for ik in [false, true] {
                check()?;
                let m = &episode.manifest;
                let mut runner = reach::Runner::reset(&m.project, m.config.clone())
                    .map_err(|_| err("learning_rollout_physics"))?;
                for index in 0..=frame as usize {
                    check()?;
                    if index > 0 {
                        runner
                            .step(episode.frames[index].action.clone())
                            .map_err(|_| err("learning_rollout_physics"))?;
                    }
                    if !same(
                        runner.episode.frames.last().unwrap(),
                        &episode.frames[index],
                    ) {
                        return Err(err("learning_recovery_prefix_mismatch"));
                    }
                }
                let mut fallbacks = 0;
                while runner.episode.frames.last().unwrap().outcome == Outcome::Running {
                    check()?;
                    let command = ik.then(|| {
                        let (target, fallback) = action(
                            &runner.episode.manifest,
                            &runner.episode.frames.last().unwrap().observation,
                            weights,
                        );
                        fallbacks += usize::from(fallback);
                        target
                    });
                    runner
                        .step(command)
                        .map_err(|_| err("learning_rollout_physics"))?;
                }
                let last = runner.episode.frames.last().unwrap();
                branches.push(json!({"outcome":last.outcome,"steps":last.index-frame,"final_distance_m":last.distance,"ik_fallback_steps":fallbacks}));
            }
            pairs.push(json!({"seed":seed,"frame":frame,"episode_sha256":probe.episode_sha256,"initial_distance_m":episode.frames[frame as usize].distance,"reference":branches[0],"ik":branches[1]}));
        }
    }
    let score = |key: &str| {
        let successes = pairs
            .iter()
            .filter(|r| r[key]["outcome"] == "success")
            .count();
        json!({"successes":successes,"timeouts":pairs.len()-successes,"success_rate":successes as f64/pairs.len() as f64,"mean_final_distance_m":pairs.iter().map(|r|r[key]["final_distance_m"].as_f64().unwrap()).sum::<f64>()/pairs.len() as f64})
    };
    Ok(
        json!({"format":"atom-ik-recovery-v1","mining":mining_id,"frame":(frames.len()==1).then(||frames[0]),"frames":frames,"joint_weights":weights,"partition":"train","reference":score("reference"),"ik":score("ik"),"pairs":pairs}),
    )
}
