//! Descriptive, train-only support diagnostics; no fitting, policy ranking or causal attribution.
use super::*;
#[derive(Clone, Debug, Serialize)]
pub struct Feature {
    pub kind: String,
    pub index: usize,
    pub object: Option<u32>,
    pub axis: Option<usize>,
    pub train_min: f64,
    pub train_max: f64,
    pub outside: usize,
    pub z3: usize,
    pub max_abs_z: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Joint {
    pub object: u32,
    pub axis: usize,
    pub actions: usize,
    pub limit_contacts: usize,
    pub max_tracking_error_rad: f64,
    pub max_action_delta_rad: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Task {
    pub seed: u32,
    pub outcome: crate::reach::Outcome,
    pub classification: String,
    pub initial_distance_m: f64,
    pub min_distance_m: f64,
    pub final_distance_m: f64,
    pub max_hold_steps: u32,
    pub required_hold_steps: u32,
    pub observation_rows: usize,
    pub outside_rows: usize,
    pub clamped_components: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub format: String,
    pub algorithm: String,
    pub evaluation: String,
    pub policy: String,
    pub evaluation_manifest_sha256: String,
    pub source_sha256: String,
    pub checkpoint_sha256: String,
    pub comparison_key: String,
    pub split: String,
    pub fit_split: String,
    pub training_rows: usize,
    pub observation_rows: usize,
    pub outside_rows: usize,
    pub z3_rows: usize,
    pub features: Vec<Feature>,
    pub joints: Vec<Joint>,
    pub tasks: Vec<Task>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Saved {
    pub file: String,
    pub report: Report,
}
struct Support {
    min: Vec<f64>,
    max: Vec<f64>,
    mean: Vec<f64>,
    std: Vec<f64>,
}
impl Support {
    fn fit(prepared: &Prepared) -> Self {
        let dim = prepared.summary.feature_dim;
        let mut min = vec![f64::INFINITY; dim];
        let mut max = vec![f64::NEG_INFINITY; dim];
        for sample in &prepared.splits[&Split::Train] {
            for (i, v) in sample.features.iter().enumerate() {
                min[i] = min[i].min(*v);
                max[i] = max[i].max(*v);
            }
        }
        Self {
            min,
            max,
            mean: prepared.normalization.mean.clone(),
            std: prepared.normalization.std.clone(),
        }
    }
    fn observation(&self, features: &[f64], metrics: &mut [Feature]) -> (bool, bool) {
        let mut outside = false;
        let mut z3 = false;
        for (i, v) in features.iter().enumerate() {
            // Scale-aware numerical tolerance; this is an empirical envelope, not a safety boundary.
            let epsilon = 1e-6 * (1. + self.min[i].abs().max(self.max[i].abs()));
            let out = *v < self.min[i] - epsilon || *v > self.max[i] + epsilon;
            let z = ((*v - self.mean[i]) / self.std[i]).abs();
            outside |= out;
            z3 |= z > 3.;
            metrics[i].outside += usize::from(out);
            metrics[i].z3 += usize::from(z > 3.);
            metrics[i].max_abs_z = metrics[i].max_abs_z.max(z);
        }
        (outside, z3)
    }
}
fn classify(
    outcome: crate::reach::Outcome,
    distances: impl Iterator<Item = f64>,
    tolerance: f64,
) -> (String, u32) {
    let mut hold = 0;
    let mut max_hold = 0;
    for distance in distances {
        hold = if distance <= tolerance { hold + 1 } else { 0 };
        max_hold = max_hold.max(hold);
    }
    let code = if outcome == crate::reach::Outcome::Success {
        "success"
    } else if max_hold > 0 {
        "reached_without_hold"
    } else {
        "never_reached"
    };
    (code.into(), max_hold)
}
fn analyze_episode(
    episode: &crate::reach::Episode,
    entry: &serde_json::Value,
    support: &Support,
    report: &mut Report,
) -> Result<()> {
    let m = &episode.manifest;
    let n = m.channels.len();
    if n != report.joints.len() {
        return Err(err("learning_diagnostics_contract"));
    }
    let mut outside_rows = 0;
    for (index, pair) in episode.frames.windows(2).enumerate() {
        let features = rollout::features_at(episode, index);
        if features.len() != report.features.len() {
            return Err(err("learning_diagnostics_contract"));
        }
        let (out, z3) = support.observation(&features, &mut report.features);
        outside_rows += usize::from(out);
        report.outside_rows += usize::from(out);
        report.z3_rows += usize::from(z3);
        report.observation_rows += 1;
        let action = pair[1].action.as_ref().unwrap();
        let previous = pair[0].action.as_ref().unwrap_or(&m.initial_action);
        for (i, c) in m.channels.iter().enumerate() {
            let metric = &mut report.joints[i];
            metric.actions += 1;
            let a = action[i];
            let epsilon = 1e-6;
            metric.limit_contacts += usize::from(
                (a - f64::from(c.range[0])).abs() <= epsilon
                    || (a - f64::from(c.range[1])).abs() <= epsilon,
            );
            metric.max_tracking_error_rad = metric
                .max_tracking_error_rad
                .max((a - pair[1].observation.qpos[c.qpos_index]).abs());
            metric.max_action_delta_rad = metric.max_action_delta_rad.max((a - previous[i]).abs());
        }
    }
    let first = &episode.frames[0];
    let last = episode.frames.last().unwrap();
    let min = episode
        .frames
        .iter()
        .map(|f| f.distance)
        .fold(f64::INFINITY, f64::min);
    let (classification, max_hold) = classify(
        last.outcome,
        episode.frames.iter().skip(1).map(|f| f.distance),
        m.tolerance,
    );
    report.tasks.push(Task {
        seed: m.config.seed,
        outcome: last.outcome,
        classification,
        initial_distance_m: first.distance,
        min_distance_m: min,
        final_distance_m: last.distance,
        max_hold_steps: max_hold,
        required_hold_steps: m.hold_steps,
        observation_rows: episode.frames.len() - 1,
        outside_rows,
        clamped_components: entry["clamped_action_components"]
            .as_u64()
            .ok_or_else(|| err("learning_rollout_invalid"))?,
    });
    Ok(())
}
/// Reads verified original artifacts. Only train observations determine the support envelope and z scale.
pub fn analyze(root: &Path, evaluation: &str) -> Result<Report> {
    let (view, evaluated) = registry::evaluation(root, evaluation)?;
    let policy = registry::select(root, &view.record.policy)?;
    let data = Validated::load(&root.join("exports").join(&policy.batch))?;
    if data.summary.source_sha256 != policy.source_sha256 {
        return Err(err("learning_registry_changed"));
    }
    let selected = rollout::validate_partition(&data, &evaluated)?;
    let prepared = data.prepare()?;
    let support = Support::fit(&prepared);
    let n = prepared.contract.channels.len();
    let mut features = vec![];
    for i in 0..prepared.summary.feature_dim {
        let (kind, index, channel) = if i < n {
            ("joint_position_rad", i, Some(i))
        } else if i < n * 2 {
            ("joint_velocity_rad_s", i - n, Some(i - n))
        } else if i < n * 2 + 3 {
            ("endpoint_world_m", i - n * 2, None)
        } else if i < n * 2 + 6 {
            ("target_world_m", i - n * 2 - 3, None)
        } else {
            ("previous_action_rad", i - n * 2 - 6, Some(i - n * 2 - 6))
        };
        features.push(Feature {
            kind: kind.into(),
            index,
            object: channel.map(|c| prepared.contract.channels[c].id),
            axis: channel.map(|c| prepared.contract.channels[c].axis),
            train_min: support.min[i],
            train_max: support.max[i],
            outside: 0,
            z3: 0,
            max_abs_z: 0.,
        });
    }
    let mut report = Report {
        schema_version: 1,
        format: "atom-bc-diagnostics-v1".into(),
        algorithm: "train-envelope-z3-v1".into(),
        evaluation: evaluation.into(),
        policy: view.record.policy.clone(),
        evaluation_manifest_sha256: view.record.manifest_sha256,
        source_sha256: policy.source_sha256,
        checkpoint_sha256: policy.checkpoint_sha256,
        comparison_key: view.record.comparison_key,
        split: selected.name().into(),
        fit_split: "train".into(),
        training_rows: prepared.normalization.count,
        observation_rows: 0,
        outside_rows: 0,
        z3_rows: 0,
        features,
        joints: prepared
            .contract
            .channels
            .iter()
            .map(|c| Joint {
                object: c.id,
                axis: c.axis,
                actions: 0,
                limit_contacts: 0,
                max_tracking_error_rad: 0.,
                max_action_delta_rad: 0.,
            })
            .collect(),
        tasks: vec![],
    };
    let mut remaining = data
        .package
        .datasets
        .iter()
        .filter(|item| item.split == selected.split())
        .map(|item| item.dataset.quality.split_group.clone())
        .collect::<BTreeSet<_>>();
    rollout::visit_policy_episodes(
        &root.join("rollouts").join(view.record.run),
        |episode, entry| {
            let group = entry["group"].as_str().unwrap();
            let source = data
                .package
                .datasets
                .iter()
                .find(|item| {
                    item.split == selected.split() && item.dataset.quality.split_group == group
                })
                .ok_or_else(|| err("learning_diagnostics_contract"))?;
            if !remaining.remove(group)
                || episode.manifest.model_sha256 != prepared.contract.model_sha256
                || !same(&episode.manifest.channels, &prepared.contract.channels)
                || episode.manifest.config.horizon != source.dataset.metadata.horizon
                || episode.manifest.initial_action
                    != source.dataset.transitions[0]
                        .observation
                        .previous_action_rad
            {
                return Err(err("learning_diagnostics_contract"));
            }
            analyze_episode(episode, entry, &support, &mut report)
        },
    )?;
    if !remaining.is_empty() {
        return Err(err("learning_diagnostics_contract"));
    }
    Ok(report)
}
/// Content-addressed diagnostics are reproducible sidecars, never replacements for the original evaluation.
pub fn save(root: &Path, report: Report) -> Result<Saved> {
    let bytes = serde_json::to_vec(&report).map_err(|_| err("learning_diagnostics_invalid"))?;
    if bytes.len() > 256 * 1024 {
        return Err(err("learning_size_limit"));
    }
    let dir = root.join("diagnostics");
    std::fs::create_dir_all(&dir).map_err(|_| err("learning_io"))?;
    let file = format!("diagnostic-{}.json", hash(&bytes));
    let mut temp = tempfile::NamedTempFile::new_in(&dir).map_err(|_| err("learning_io"))?;
    temp.write_all(&bytes)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|_| err("learning_io"))?;
    match temp.persist_noclobber(dir.join(&file)) {
        Ok(_) => {}
        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
            if training::read(&dir.join(&file), 256 * 1024)? != bytes {
                return Err(err("learning_diagnostics_invalid"));
            }
        }
        Err(_) => return Err(err("learning_io")),
    }
    std::fs::File::open(&dir)
        .and_then(|f| f.sync_all())
        .map_err(|_| err("learning_io"))?;
    Ok(Saved { file, report })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failure_classification_counts_consecutive_post_action_hits() {
        use crate::reach::Outcome;
        assert_eq!(
            classify(Outcome::Timeout, [0.1, 0.035, 0.02, 0.1].into_iter(), 0.035),
            ("reached_without_hold".into(), 2)
        );
        assert_eq!(
            classify(Outcome::Timeout, [0.1, 0.2].into_iter(), 0.035),
            ("never_reached".into(), 0)
        );
        assert_eq!(
            classify(Outcome::Success, [0.01; 5].into_iter(), 0.035),
            ("success".into(), 5)
        );
    }
    #[test]
    fn support_uses_train_only_and_separates_envelope_from_z_heuristic() {
        let summary = Summary {
            schema_version: 1,
            adapter: "pytorch_bc_v1",
            source_sha256: "a".repeat(64),
            feature_dim: 1,
            action_dim: 1,
            rows: [(Split::Train, 2), (Split::Validation, 1), (Split::Test, 1)].into(),
            groups: BTreeMap::new(),
            fit_data_ready: true,
            test_data_ready: true,
            warnings: vec![],
        };
        let sample = |v| Sample {
            episode_id: crate::Uuid::nil(),
            group: "x".into(),
            index: 0,
            features: vec![v],
            action: vec![0.],
        };
        let mut prepared = Prepared {
            schema_version: 1,
            format: FORMAT.into(),
            summary,
            feature_order: vec![],
            contract: Contract {
                channels: vec![],
                model_sha256: "a".repeat(64),
                world_up: "Y".into(),
                action_semantics: "absolute_joint_position_servo_target_rad".into(),
                control_dt_seconds: 0.02,
            },
            normalization: Normalization {
                fit_split: "train".into(),
                count: 2,
                mean: vec![2.],
                std: vec![1.],
            },
            splits: [
                (Split::Train, vec![sample(1.), sample(3.)]),
                (Split::Validation, vec![sample(-100.)]),
                (Split::Test, vec![sample(100.)]),
            ]
            .into(),
        };
        let support = Support::fit(&prepared);
        assert_eq!(support.min, [1.]);
        assert_eq!(support.max, [3.]);
        prepared.splits.get_mut(&Split::Test).unwrap()[0].features[0] = 1e8;
        assert_eq!(support.max, Support::fit(&prepared).max);
        let mut metrics = vec![Feature {
            kind: "target_world_m".into(),
            index: 0,
            object: None,
            axis: None,
            train_min: 1.,
            train_max: 3.,
            outside: 0,
            z3: 0,
            max_abs_z: 0.,
        }];
        assert_eq!(
            support.observation(&[3. + 1e-7], &mut metrics),
            (false, false)
        );
        assert_eq!(support.observation(&[4.], &mut metrics), (true, false));
        assert_eq!(support.observation(&[9.], &mut metrics), (true, true));
        assert_eq!(metrics[0].outside, 2);
        assert_eq!(metrics[0].z3, 1);
        assert_eq!(metrics[0].max_abs_z, 7.);
    }
}
