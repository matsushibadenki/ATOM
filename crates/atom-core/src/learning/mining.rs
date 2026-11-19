//! Bounded, frozen-policy difficulty probes on NEW train groups only.
use super::*;
use crate::{Project, reach};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub policy: String,
    pub start_seed: u32,
    pub count: u32,
    pub horizon: u32,
    pub max_seconds: u32,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        let policy = self
            .policy
            .strip_prefix("policy-")
            .and_then(|s| s.strip_suffix(".json"))
            .is_some_and(sha);
        if !policy
            || !(2..=16).contains(&self.count)
            || !(1..=500).contains(&self.horizon)
            || !(1..=300).contains(&self.max_seconds)
            || self.start_seed.checked_add(1023).is_none()
        {
            return Err(err("learning_mining_config"));
        }
        Ok(())
    }
    fn collection(&self) -> collection::Config {
        collection::Config {
            start_seed: self.start_seed,
            count: 1,
            horizon: self.horizon,
            max_seconds: self.max_seconds,
            mining: None,
            accepted_target: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Completed,
    Cancelled,
    Budget,
    Failed,
    Exhausted,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Probe {
    pub seed: u32,
    pub group: String,
    pub episode: String,
    pub episode_sha256: String,
    pub outcome: reach::Outcome,
    pub final_distance_m: f64,
    pub steps: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outside_train_rows: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outside_train_components: Option<usize>,
    pub clamped_components: usize,
    #[serde(default, skip_serializing_if = "super::is_zero")]
    pub teacher_steps: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32,
    pub format: String,
    pub config: Config,
    pub source: String,
    pub source_sha256: String,
    pub checkpoint_sha256: String,
    pub job_manifest_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intervention: Option<super::dagger::Intervention>,
    pub inference_runtime: Option<serde_json::Value>,
    pub status: Status,
    pub code: Option<String>,
    pub scanned: u32,
    pub skipped_holdout: u32,
    pub skipped_existing: u32,
    pub probes: Vec<Probe>,
    pub priorities: Vec<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Saved {
    pub file: String,
    pub report: Report,
}
#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub completed: usize,
    pub total: u32,
    pub seed: u32,
}
fn priorities(probes: &[Probe], status: Status, count: u32) -> Vec<u32> {
    if status != Status::Completed || probes.len() != count as usize {
        return vec![];
    }
    let mut sorted = probes.iter().collect::<Vec<_>>();
    sorted.sort_by(|a, b| {
        (b.outcome == reach::Outcome::Timeout)
            .cmp(&(a.outcome == reach::Outcome::Timeout))
            .then_with(|| {
                let a_outside = a.outside_train_rows.unwrap_or_default() as u64;
                let b_outside = b.outside_train_rows.unwrap_or_default() as u64;
                (b_outside * u64::from(a.steps)).cmp(&(a_outside * u64::from(b.steps)))
            })
            .then_with(|| {
                let a_components = a.outside_train_components.unwrap_or_default() as u64;
                let b_components = b.outside_train_components.unwrap_or_default() as u64;
                (b_components * u64::from(a.steps)).cmp(&(a_components * u64::from(b.steps)))
            })
            .then_with(|| b.final_distance_m.total_cmp(&a.final_distance_m))
            .then_with(|| a.seed.cmp(&b.seed))
    });
    sorted.iter().map(|p| p.seed).collect()
}
fn boundary(cancel: &AtomicBool, deadline: Instant) -> Option<Status> {
    if cancel.load(Ordering::Relaxed) {
        Some(Status::Cancelled)
    } else if Instant::now() >= deadline {
        Some(Status::Budget)
    } else {
        None
    }
}
fn checked_policy(
    root: &Path,
    config: &Config,
    expected_source: Option<&str>,
) -> Result<(registry::Policy, Validated)> {
    config.validate()?;
    let policy = registry::select(root, &config.policy)?;
    if expected_source.is_some_and(|s| s != policy.source_sha256) {
        return Err(err("learning_mining_source"));
    }
    let data = Validated::load(&root.join("exports").join(&policy.batch))?;
    if data.summary.source_sha256 != policy.source_sha256 {
        return Err(err("learning_source_changed"));
    }
    Ok((policy, data))
}
pub fn run(
    root: &Path,
    project: &Project,
    config: &Config,
    expected_source: Option<&str>,
    cancel: &AtomicBool,
    progress: impl FnMut(Progress),
) -> Result<Saved> {
    run_intervened(
        root,
        project,
        config,
        expected_source,
        cancel,
        progress,
        None,
    )
}

pub(super) fn run_intervened(
    root: &Path,
    project: &Project,
    config: &Config,
    expected_source: Option<&str>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
    intervention: Option<&super::dagger::Intervention>,
) -> Result<Saved> {
    config.validate()?;
    let deadline = Instant::now() + Duration::from_secs(config.max_seconds.into());
    let (policy, data) = checked_policy(root, config, expected_source)?;
    let support = if intervention.is_none() {
        Some(diagnostics::Support::fit(&data.prepare()?))
    } else {
        None
    };
    let shape = config.collection();
    collection::eligible(&data, project, &shape, config.start_seed)?;
    let mut report = Report {
        schema_version: 1,
        format: if support.is_some() {
            "atom-train-mining-v3"
        } else {
            "atom-train-mining-v1"
        }
        .into(),
        config: config.clone(),
        source: policy.batch.clone(),
        source_sha256: policy.source_sha256.clone(),
        checkpoint_sha256: policy.checkpoint_sha256.clone(),
        job_manifest_sha256: policy.job_manifest_sha256.clone(),
        intervention: intervention.cloned(),
        inference_runtime: None,
        status: Status::Exhausted,
        code: None,
        scanned: 0,
        skipped_holdout: 0,
        skipped_existing: 0,
        probes: vec![],
        priorities: vec![],
    };
    let mut inference = None;
    let mut engine = None;
    let id = crate::Uuid::new_v4();
    for seed in config.start_seed..=config.start_seed + 1023 {
        if let Some(status) = boundary(cancel, deadline) {
            report.status = status;
            break;
        }
        let (group, reason) = collection::eligible(&data, project, &shape, seed)?;
        report.scanned += 1;
        match reason {
            Some("collection_holdout") => {
                report.skipped_holdout += 1;
                continue;
            }
            Some(_) => {
                report.skipped_existing += 1;
                continue;
            }
            None => {}
        }
        progress(Progress {
            completed: report.probes.len(),
            total: config.count,
            seed,
        });
        let result = (|| -> Result<Probe> {
            if boundary(cancel, deadline).is_some() {
                return Err(err("learning_mining_interrupted"));
            }
            if inference.is_none() {
                let (worker, checkpoint) = rollout::Policy::start(
                    &data,
                    &root.join("training").join(&policy.job),
                    cancel,
                )?;
                if checkpoint != policy.checkpoint_sha256 {
                    return Err(err("learning_registry_changed"));
                }
                report.inference_runtime = Some(worker.runtime.clone());
                inference = Some(worker);
            }
            if boundary(cancel, deadline).is_some() {
                return Err(err("learning_mining_interrupted"));
            }
            let per_run = Instant::now() + Duration::from_secs(30);
            let mut runner = reach::Runner::reset(
                project,
                reach::Config {
                    seed,
                    robot: data.package.datasets[0].dataset.metadata.robot,
                    horizon: config.horizon,
                },
            )
            .map_err(|_| err("learning_rollout_physics"))?;
            let first = &runner.episode.frames[0].observation;
            let identity = json!([first.engine, first.python, first.platform]);
            if engine.as_ref().is_some_and(|e| e != &identity) {
                return Err(err("learning_rollout_engine"));
            }
            engine = Some(identity);
            if let Some(mix) = intervention {
                mix.validate(Some(&runner.episode.manifest))?;
            }
            let mut teacher_steps = 0;
            let mut clamped = 0;
            while runner.episode.frames.last().unwrap().outcome == Outcome::Running {
                if boundary(cancel, deadline).is_some() {
                    return Err(err("learning_mining_interrupted"));
                }
                if Instant::now() >= per_run {
                    return Err(err("learning_mining_run_budget"));
                }
                let frame = runner.episode.frames.last().unwrap();
                let action = if let Some(mix) =
                    intervention.filter(|mix| mix.teacher_at(seed, frame.index))
                {
                    teacher_steps += 1;
                    mix.action(&runner.episode.manifest, &frame.observation)
                } else {
                    let (action, n) = inference.as_mut().unwrap().action(&runner, cancel)?;
                    clamped += n;
                    action
                };
                if boundary(cancel, deadline).is_some() {
                    return Err(err("learning_mining_interrupted"));
                }
                runner
                    .step(Some(action))
                    .map_err(|_| err("learning_rollout_physics"))?;
            }
            let q = runner
                .episode
                .quality()
                .map_err(|_| err("learning_mining_invalid"))?;
            if q.split_group != group {
                return Err(err("learning_mining_invalid"));
            }
            let outside_counts = support
                .as_ref()
                .map(|support| support.outside_counts(&runner.episode))
                .transpose()?;
            let episode = format!("reach-mining-{id}-{seed}.episode.json");
            runner
                .episode
                .save(&root.join(&episode))
                .map_err(|_| err("learning_io"))?;
            Ok(Probe {
                seed,
                group,
                episode,
                episode_sha256: q.episode_sha256,
                outcome: q.outcome,
                final_distance_m: q.final_distance_m,
                steps: q.transitions as u32,
                outside_train_rows: outside_counts.map(|counts| counts.0),
                outside_train_components: outside_counts.map(|counts| counts.1),
                clamped_components: clamped,
                teacher_steps,
            })
        })();
        match result {
            Ok(probe) => {
                report.probes.push(probe);
                progress(Progress {
                    completed: report.probes.len(),
                    total: config.count,
                    seed,
                });
            }
            Err(e) => {
                report.status = boundary(cancel, deadline).unwrap_or(Status::Failed);
                if e.code == "learning_rollout_cancelled" {
                    report.status = Status::Cancelled;
                }
                report.code = Some(e.code);
                break;
            }
        }
        if report.probes.len() == config.count as usize {
            report.status = Status::Completed;
            break;
        }
    }
    drop(inference);
    report.priorities = priorities(&report.probes, report.status, config.count);
    save_report(root, report)
}

/// Re-rank the same observed candidates using the original distance ordering.
pub fn distance_baseline(root: &Path, file: &str) -> Result<Saved> {
    let mut report = inspect(root, file)?.report;
    if !matches!(
        report.format.as_str(),
        "atom-train-mining-v2" | "atom-train-mining-v3"
    ) || report.status != Status::Completed
    {
        return Err(err("learning_mining_invalid"));
    }
    report.format = "atom-train-mining-v1".into();
    for probe in &mut report.probes {
        probe.outside_train_rows = None;
        probe.outside_train_components = None;
    }
    report.priorities = priorities(&report.probes, report.status, report.config.count);
    save_report(root, report)
}

fn save_report(root: &Path, report: Report) -> Result<Saved> {
    let bytes = serde_json::to_vec(&report).map_err(|_| err("learning_mining_invalid"))?;
    let file = format!("mining-{}.json", hash(&bytes));
    let dir = root.join("mining");
    std::fs::create_dir_all(&dir).map_err(|_| err("learning_io"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(&dir).map_err(|_| err("learning_io"))?;
    tmp.write_all(&bytes)
        .and_then(|_| tmp.as_file().sync_all())
        .map_err(|_| err("learning_io"))?;
    if let Err(e) = tmp.persist_noclobber(dir.join(&file))
        && (e.error.kind() != std::io::ErrorKind::AlreadyExists
            || training::read(&dir.join(&file), 65536)? != bytes)
    {
        return Err(err("learning_output_conflict"));
    }
    std::fs::File::open(&dir)
        .and_then(|f| f.sync_all())
        .map_err(|_| err("learning_io"))?;
    Ok(Saved { file, report })
}
pub fn valid_name(name: &str) -> bool {
    name.strip_prefix("mining-")
        .and_then(|s| s.strip_suffix(".json"))
        .is_some_and(sha)
}
pub fn inspect(root: &Path, file: &str) -> Result<Saved> {
    if !valid_name(file) {
        return Err(err("learning_mining_invalid"));
    }
    let bytes = training::read(&root.join("mining").join(file), 65536)?;
    if file != format!("mining-{}.json", hash(&bytes)) {
        return Err(err("learning_mining_invalid"));
    }
    let report: Report =
        serde_json::from_slice(&bytes).map_err(|_| err("learning_mining_invalid"))?;
    let (policy, data) = checked_policy(root, &report.config, Some(&report.source_sha256))?;
    let support = match report.format.as_str() {
        "atom-train-mining-v1" => None,
        "atom-train-mining-v2" | "atom-train-mining-v3" if report.intervention.is_none() => {
            Some(diagnostics::Support::fit(&data.prepare()?))
        }
        _ => return Err(err("learning_mining_invalid")),
    };
    if report.schema_version != 1
        || report.source != policy.batch
        || report.checkpoint_sha256 != policy.checkpoint_sha256
        || report.job_manifest_sha256 != policy.job_manifest_sha256
        || report.scanned > 1024
        || report.probes.len() > report.config.count as usize
        || u64::from(report.skipped_holdout)
            + u64::from(report.skipped_existing)
            + report.probes.len() as u64
            > u64::from(report.scanned)
        || report.probes.iter().any(|p| {
            p.outside_train_rows
                .is_some_and(|rows| rows > p.steps as usize)
        })
        || report.probes.iter().any(|p| {
            p.outside_train_components
                .is_some_and(|n| n > 500 * data.summary.feature_dim)
        })
        || report.priorities != priorities(&report.probes, report.status, report.config.count)
        || (report.status == Status::Completed)
            != (report.probes.len() == report.config.count as usize)
    {
        return Err(err("learning_mining_invalid"));
    }
    if (!report.probes.is_empty() && report.inference_runtime.is_none())
        || report.inference_runtime.as_ref().is_some_and(|r| {
            r["ready"] != true
                || r["device"] != "cpu"
                || r["threads"] != 1
                || ["torch", "python", "platform"].iter().any(|key| {
                    r[key]
                        .as_str()
                        .is_none_or(|s| s.is_empty() || s.len() > 200)
                })
        })
    {
        return Err(err("learning_mining_invalid"));
    }
    let mut previous = None;
    let mut engine = None;
    for p in &report.probes {
        if previous.is_some_and(|seed| seed >= p.seed)
            || p.seed < report.config.start_seed
            || p.seed > report.config.start_seed + 1023
            || !reach::valid_name(&p.episode)
        {
            return Err(err("learning_mining_invalid"));
        }
        previous = Some(p.seed);
        let episode = reach::Episode::load(&root.join(&p.episode))
            .map_err(|_| err("learning_mining_invalid"))?;
        let (group, reason) = collection::eligible(
            &data,
            &episode.manifest.project,
            &report.config.collection(),
            p.seed,
        )?;
        let expected_teacher_steps = if let Some(mix) = &report.intervention {
            mix.validate(Some(&episode.manifest))?;
            let mut count = 0;
            for pair in episode.frames.windows(2) {
                if mix.teacher_at(p.seed, pair[0].index) {
                    count += 1;
                    if pair[1].action.as_ref()
                        != Some(&mix.action(&episode.manifest, &pair[0].observation))
                    {
                        return Err(err("learning_mining_invalid"));
                    }
                }
            }
            count
        } else {
            0
        };
        let outside_counts = support
            .as_ref()
            .map(|support| support.outside_counts(&episode))
            .transpose()?;
        if p.teacher_steps != expected_teacher_steps
            || p.outside_train_rows != outside_counts.map(|counts| counts.0)
            || p.outside_train_components
                != outside_counts
                    .filter(|_| report.format == "atom-train-mining-v3")
                    .map(|counts| counts.1)
        {
            return Err(err("learning_mining_invalid"));
        }
        let q = episode
            .quality()
            .map_err(|_| err("learning_mining_invalid"))?;
        let first = &episode.frames[0].observation;
        let identity = json!([first.engine, first.python, first.platform]);
        if reason.is_some()
            || group != p.group
            || q.split_group != p.group
            || episode.manifest.config.seed != p.seed
            || episode.manifest.config.horizon != report.config.horizon
            || q.episode_sha256 != p.episode_sha256
            || q.outcome != p.outcome
            || !q.complete
            || q.final_distance_m != p.final_distance_m
            || q.transitions != p.steps as usize
            || p.clamped_components > q.transitions * episode.manifest.channels.len()
            || engine.as_ref().is_some_and(|e| e != &identity)
        {
            return Err(err("learning_mining_invalid"));
        }
        engine = Some(identity);
    }
    Ok(Saved {
        file: file.into(),
        report,
    })
}
pub(super) fn selection(
    root: &Path,
    file: &str,
    data: &Validated,
    config: &collection::Config,
) -> Result<Vec<u32>> {
    let saved = inspect(root, file)?;
    let r = saved.report;
    if r.source_sha256 != data.summary.source_sha256
        || r.status != Status::Completed
        || r.config.start_seed != config.start_seed
        || r.config.horizon != config.horizon
        || r.priorities.len() < config.count as usize
    {
        return Err(err("learning_mining_selection"));
    }
    Ok(r.priorities
        .into_iter()
        .take(config.count as usize)
        .collect())
}
pub fn catalog(root: &Path) -> Result<Vec<String>> {
    let dir = root.join("mining");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut names = vec![];
    for entry in std::fs::read_dir(dir).map_err(|_| err("learning_io"))? {
        let e = entry.map_err(|_| err("learning_io"))?;
        let name = e.file_name().to_string_lossy().to_string();
        if valid_name(&name) && e.file_type().is_ok_and(|t| t.is_file()) {
            names.push(name);
        }
    }
    names.sort();
    names.truncate(64);
    Ok(names)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn difficulty_requires_a_complete_pool_and_ranks_timeout_then_distance_then_seed() {
        let p = |seed, outcome, distance| Probe {
            seed,
            group: "a".repeat(64),
            episode: "reach-probe.episode.json".into(),
            episode_sha256: "b".repeat(64),
            outcome,
            final_distance_m: distance,
            steps: 5,
            clamped_components: 0,
            outside_train_rows: None,
            outside_train_components: None,
            teacher_steps: 0,
        };
        let mut probes = vec![
            p(1, Outcome::Success, 0.02),
            p(2, Outcome::Timeout, 0.01),
            p(3, Outcome::Timeout, 1.0),
            p(4, Outcome::Timeout, 1.0),
        ];
        assert_eq!(priorities(&probes, Status::Completed, 4), [3, 4, 2, 1]);
        assert!(priorities(&probes, Status::Budget, 4).is_empty());
        assert!(priorities(&probes, Status::Completed, 5).is_empty());
        probes[0].outside_train_rows = Some(5);
        probes[1].outside_train_rows = Some(2);
        probes[2].outside_train_rows = Some(3);
        probes[2].steps = 10;
        probes[3].outside_train_rows = Some(0);
        assert_eq!(priorities(&probes, Status::Completed, 4), [2, 3, 4, 1]);
        probes[1].outside_train_rows = Some(5);
        probes[2].outside_train_rows = Some(10);
        probes[1].outside_train_components = Some(15);
        probes[2].outside_train_components = Some(20);
        assert_eq!(priorities(&probes, Status::Completed, 4), [2, 3, 4, 1]);
    }
}
