//! Paired, bounded DAgger rounds initialized from the same registered policy.
use super::*;
use crate::{Project, reach};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intervention {
    pub teacher_bps: u16,
    pub seed: u32,
    pub ik: bool,
    pub joint_weights: Vec<ik_teacher::JointWeight>,
}
impl Intervention {
    pub(super) fn validate(&self, manifest: Option<&reach::Manifest>) -> Result<()> {
        if self.teacher_bps > 10_000 || (self.ik == self.joint_weights.is_empty()) {
            return Err(err("learning_dagger_config"));
        }
        ik_teacher::validate_weights(&self.joint_weights, manifest)
    }

    pub(super) fn teacher_at(&self, episode: u32, frame: u32) -> bool {
        let mut digest = Sha256::new();
        digest.update(b"atom-dagger-mix-v1");
        for part in [self.seed, episode, frame] {
            digest.update(part.to_le_bytes());
        }
        let bytes = digest.finalize();
        u64::from_le_bytes(bytes[..8].try_into().unwrap()) % 10_000 < u64::from(self.teacher_bps)
    }

    pub(super) fn action(
        &self,
        manifest: &reach::Manifest,
        state: &reach::Observation,
    ) -> Vec<f64> {
        if self.ik {
            ik_teacher::action(manifest, state, &self.joint_weights).0
        } else {
            manifest.reference_action.clone()
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub initial_policy: String,
    /// One teacher probability per round, in basis points.
    pub teacher_bps: Vec<u16>,
    pub seed: u32,
    pub start_seed: u32,
    pub episodes_per_round: u32,
    pub horizon: u32,
    pub stride: u32,
    pub joint_weights: Vec<ik_teacher::JointWeight>,
    pub training: training::Config,
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.teacher_bps.is_empty()
            || self.teacher_bps.len() > 4
            || self.teacher_bps.iter().any(|&p| p > 10_000)
            || !(2..=8).contains(&self.episodes_per_round)
            || !(1..=500).contains(&self.horizon)
            || !(1..=50).contains(&self.stride)
            || self.start_seed.checked_add(1023).is_none()
            || self.joint_weights.is_empty()
        {
            return Err(err("learning_dagger_config"));
        }
        self.training.validate()?;
        ik_teacher::validate_weights(&self.joint_weights, None)
    }
}

pub fn run(
    root: &Path,
    project: &Project,
    config: &Config,
    cancel: &AtomicBool,
) -> Result<serde_json::Value> {
    run_with_progress(root, project, config, cancel, |_| {})
}

#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub round: usize,
    pub schedule: Option<usize>,
    pub stage: &'static str,
    pub rounds: Vec<serde_json::Value>,
}

pub fn run_with_progress(
    root: &Path,
    project: &Project,
    config: &Config,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<serde_json::Value> {
    config.validate()?;
    let initial = registry::select(root, &config.initial_policy)?;
    let initial_data = Validated::load(&root.join("exports").join(initial.batch))?;
    if initial_data.summary.source_sha256 != initial.source_sha256
        || !initial_data.summary.fit_data_ready
        || initial_data.package.report.entries.len()
            + initial_data.package.labels.len()
            + config.teacher_bps.len() * config.episodes_per_round as usize
            > 64
    {
        return Err(err("learning_dagger_source"));
    }
    let manifest = reach::prepare(
        project,
        reach::Config {
            seed: config.start_seed,
            robot: initial_data.package.datasets[0].dataset.metadata.robot,
            horizon: config.horizon,
        },
    )
    .map_err(|_| err("learning_rollout_scene"))?;
    ik_teacher::validate_weights(&config.joint_weights, Some(&manifest))?;
    let validation = rollout::validation_spec(&initial_data, project)?;
    let id = crate::Uuid::new_v4();
    let directory = root.join("dagger").join(id.to_string());
    std::fs::create_dir_all(&directory).map_err(|_| err("learning_io"))?;
    let mut policies = [config.initial_policy.clone(), config.initial_policy.clone()];
    let mut rounds = Vec::new();
    let mut comparison_key = None;
    let result = (|| -> Result<()> {
        for (round, &teacher_bps) in config.teacher_bps.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return Err(err("learning_dagger_cancelled"));
            }
            let mut branches = Vec::new();
            let mut groups = None;
            for (branch, policy) in policies.iter_mut().enumerate() {
                let ik = branch == 1;
                let mix = Intervention {
                    teacher_bps,
                    seed: config.seed.wrapping_add(round as u32),
                    ik,
                    joint_weights: if ik {
                        config.joint_weights.clone()
                    } else {
                        vec![]
                    },
                };
                progress(Progress {
                    round: round + 1,
                    schedule: None,
                    stage: if ik {
                        "collect_ik"
                    } else {
                        "collect_reference"
                    },
                    rounds: rounds.clone(),
                });
                let mined = mining::run_intervened(
                    root,
                    project,
                    &mining::Config {
                        policy: policy.clone(),
                        start_seed: config.start_seed,
                        count: config.episodes_per_round,
                        horizon: config.horizon,
                        max_seconds: 300,
                    },
                    None,
                    cancel,
                    |_| {},
                    Some(&mix),
                )?;
                if mined.report.status != mining::Status::Completed {
                    return Err(err(mined
                        .report
                        .code
                        .as_deref()
                        .unwrap_or("learning_dagger_collection")));
                }
                let selected = mined
                    .report
                    .probes
                    .iter()
                    .map(|p| &p.group)
                    .cloned()
                    .collect::<Vec<_>>();
                if groups.as_ref().is_some_and(|g| g != &selected) {
                    return Err(err("learning_dagger_groups"));
                }
                groups = Some(selected);
                let labels = supervision::run(
                    root,
                    &supervision::Config {
                        mining: mined.file.clone(),
                        count: config.episodes_per_round,
                        stride: config.stride,
                        max_seconds: 300,
                        ik,
                        joint_weights: mix.joint_weights.clone(),
                    },
                    None,
                    cancel,
                    |_| {},
                )?;
                let batch = labels
                    .report
                    .exported
                    .as_deref()
                    .ok_or_else(|| err("learning_dagger_labels"))?;
                let data = Validated::load(&root.join("exports").join(batch))?;
                if rollout::validation_spec(&data, project)? != validation {
                    return Err(err("learning_dagger_holdout"));
                }
                let job = format!("bc-dagger-{id}-{round}-{branch}");
                progress(Progress {
                    round: round + 1,
                    schedule: None,
                    stage: if ik { "train_ik" } else { "train_reference" },
                    rounds: rounds.clone(),
                });
                let trained = training::train_validation(
                    &data,
                    &config.training,
                    &root.join("training").join(&job),
                    cancel,
                )?;
                if trained.result["status"] == "cancelled" {
                    return Err(err("learning_dagger_cancelled"));
                }
                if trained.result["status"] != "completed" {
                    return Err(err("learning_dagger_training_budget"));
                }
                let registered = registry::register(root, &job, batch)?;
                branches.push(json!({
                    "teacher": if ik {"weighted_ik"} else {"reference_servo"},
                    "parent_policy": policy, "mining": mined.file, "labels": labels.file,
                    "policy": registered.id, "batch": batch,
                    "teacher_steps": mined.report.probes.iter().map(|p|p.teacher_steps).sum::<usize>(),
                    "total_steps": mined.report.probes.iter().map(|p|p.steps).sum::<u32>(),
                    "training_status": trained.result["status"], "training_steps": trained.result["step"],
                }));
                *policy = registered.id;
            }
            progress(Progress {
                round: round + 1,
                schedule: None,
                stage: "validation",
                rounds: rounds.clone(),
            });
            let compared = campaign::run(
                root,
                project,
                &campaign::Config {
                    policies: policies.to_vec(),
                    max_seconds: 300,
                },
                cancel,
                |_| {},
            )?;
            for (branch, candidate) in branches.iter_mut().zip(&compared.report.candidates) {
                branch["validation"] = json!(candidate.score);
                branch["evaluation"] = json!(candidate.evaluation);
            }
            let complete = compared.report.status == campaign::Status::Completed
                && compared.report.task_spec_sha256 == validation
                && comparison_key
                    .as_ref()
                    .is_none_or(|key| compared.report.comparison_key.as_ref() == Some(key));
            if complete {
                comparison_key.clone_from(&compared.report.comparison_key);
            }
            rounds.push(json!({"round": round+1,"teacher_bps":teacher_bps,"campaign":compared.file,"complete":complete,"branches":branches}));
            collection::write_atomic(
                &directory.join(format!("round-{}.json", round + 1)),
                rounds.last().unwrap(),
            )?;
            if !complete {
                return Err(err("learning_dagger_validation"));
            }
        }
        Ok(())
    })();
    let report = json!({
        "format":"atom-dagger-v1","id":id,"config":config,"validation_spec":validation,
        "comparison_key":comparison_key,"complete":result.is_ok(),"cancelled":cancel.load(Ordering::Relaxed),"error":result.err().map(|e|e.code),"rounds":rounds
    });
    collection::write_atomic(&directory.join("report.json"), &report)?;
    Ok(report)
}

pub fn validate_schedules(config: &Config, schedules: &[Vec<u16>]) -> Result<()> {
    config.validate()?;
    if schedules.len() > 4
        || (!schedules.is_empty()
            && (schedules.len() < 2
                || schedules[0] != config.teacher_bps
                || schedules
                    .iter()
                    .any(|s| s.len() != config.teacher_bps.len() || s.iter().any(|p| *p > 10_000))
                || schedules.iter().collect::<BTreeSet<_>>().len() != schedules.len()))
    {
        return Err(err("learning_dagger_schedules"));
    }
    Ok(())
}

fn append_schedule(
    rounds: &mut Vec<serde_json::Value>,
    source: &[serde_json::Value],
    index: usize,
) {
    rounds.extend(source.iter().cloned().map(|mut round| {
        round["schedule"] = json!(index + 1);
        round
    }));
}

pub fn compare_schedules(
    root: &Path,
    project: &Project,
    config: &Config,
    schedules: &[Vec<u16>],
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<serde_json::Value> {
    validate_schedules(config, schedules)?;
    if schedules.is_empty() {
        return run_with_progress(root, project, config, cancel, progress);
    }
    let mut rounds = Vec::new();
    let mut runs = Vec::new();
    let mut identity = None;
    let mut failure = None;
    for (index, schedule) in schedules.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let candidate = Config {
            teacher_bps: schedule.clone(),
            ..config.clone()
        };
        let result = run_with_progress(root, project, &candidate, cancel, |mut update| {
            let mut visible = rounds.clone();
            append_schedule(&mut visible, &update.rounds, index);
            update.rounds = visible;
            update.schedule = Some(index + 1);
            progress(update);
        });
        let report = match result {
            Ok(report) => report,
            Err(error) if runs.is_empty() => return Err(error),
            Err(error) => {
                failure = Some(error.code);
                break;
            }
        };
        let key = (
            report["validation_spec"].clone(),
            report["comparison_key"].clone(),
        );
        let comparable =
            report["complete"] == true && identity.as_ref().is_none_or(|previous| previous == &key);
        runs.push(json!({"id":report["id"],"comparable":comparable}));
        if comparable {
            identity = Some(key);
        }
        if comparable || report["complete"] != true {
            append_schedule(&mut rounds, report["rounds"].as_array().unwrap(), index);
        }
        if !comparable {
            failure = report["error"].as_str().map(str::to_owned);
            if report["complete"] == true {
                failure = Some("learning_dagger_holdout".into());
            }
            break;
        }
    }
    let id = crate::Uuid::new_v4().to_string();
    let complete =
        runs.len() == schedules.len() && runs.iter().all(|run| run["comparable"] == true);
    let report = json!({"format":"atom-dagger-schedules-v1","id":id,"config":config,
        "schedules":schedules,"runs":runs,"rounds":rounds,"complete":complete,"error":failure,
        "cancelled":cancel.load(Ordering::Relaxed)});
    collection::write_atomic(&root.join("dagger").join(&id).join("report.json"), &report)?;
    Ok(report)
}

pub fn inspect(root: &Path, id: &str) -> Result<serde_json::Value> {
    let uuid = crate::Uuid::parse_str(id).map_err(|_| err("learning_dagger_report"))?;
    if uuid.to_string() != id {
        return Err(err("learning_dagger_report"));
    }
    let dir = root.join("dagger").join(id);
    let bytes = training::read(&dir.join("report.json"), 4 * 65536)?;
    let report: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| err("learning_dagger_report"))?;
    let config: Config = serde_json::from_value(report["config"].clone())
        .map_err(|_| err("learning_dagger_report"))?;
    config.validate()?;
    if report["format"] == "atom-dagger-schedules-v1" {
        if report["id"] != id {
            return Err(err("learning_dagger_report"));
        }
        return inspect_schedules(root, report, &config);
    }
    let rounds = report["rounds"]
        .as_array()
        .ok_or_else(|| err("learning_dagger_report"))?;
    if report["id"] != id
        || report["format"] != "atom-dagger-v1"
        || rounds.len() > config.teacher_bps.len()
    {
        return Err(err("learning_dagger_report"));
    }
    for (index, round) in rounds.iter().enumerate() {
        let bytes = training::read(&dir.join(format!("round-{}.json", index + 1)), 65536)?;
        if serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .as_ref()
            != Some(round)
            || round["round"] != index + 1
            || round["teacher_bps"] != config.teacher_bps[index]
        {
            return Err(err("learning_dagger_report"));
        }
        let verified = campaign::inspect(
            root,
            round["campaign"]
                .as_str()
                .ok_or_else(|| err("learning_dagger_report"))?,
        )?;
        if verified.report.task_spec_sha256 != report["validation_spec"]
            || (round["complete"] == true
                && !report["comparison_key"].is_null()
                && json!(verified.report.comparison_key) != report["comparison_key"])
        {
            return Err(err("learning_dagger_report"));
        }
        let branches = round["branches"]
            .as_array()
            .ok_or_else(|| err("learning_dagger_report"))?;
        if branches.len() != 2 || verified.report.candidates.len() != 2 {
            return Err(err("learning_dagger_report"));
        }
        for (branch, candidate) in branches.iter().zip(verified.report.candidates) {
            if branch["policy"] != candidate.policy
                || branch["validation"] != json!(candidate.score)
            {
                return Err(err("learning_dagger_report"));
            }
        }
    }
    Ok(report)
}

fn inspect_schedules(
    root: &Path,
    report: serde_json::Value,
    config: &Config,
) -> Result<serde_json::Value> {
    let invalid = || err("learning_dagger_report");
    let schedules: Vec<Vec<u16>> =
        serde_json::from_value(report["schedules"].clone()).map_err(|_| invalid())?;
    validate_schedules(config, &schedules)?;
    let runs = report["runs"]
        .as_array()
        .filter(|runs| !schedules.is_empty() && runs.len() <= schedules.len())
        .ok_or_else(invalid)?;
    let mut rounds = Vec::new();
    let mut identity = None;
    for (index, run) in runs.iter().enumerate() {
        let id = run["id"].as_str().ok_or_else(invalid)?;
        let uuid = crate::Uuid::parse_str(id).map_err(|_| invalid())?;
        if uuid.to_string() != id {
            return Err(invalid());
        }
        let bytes = training::read(&root.join("dagger").join(id).join("report.json"), 65536)?;
        let header: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if header["format"] != "atom-dagger-v1" {
            return Err(invalid());
        }
        let child = inspect(root, id)?;
        let expected = Config {
            teacher_bps: schedules[index].clone(),
            ..config.clone()
        };
        let key = (
            child["validation_spec"].clone(),
            child["comparison_key"].clone(),
        );
        let comparable =
            child["complete"] == true && identity.as_ref().is_none_or(|previous| previous == &key);
        if child["config"] != json!(expected)
            || run["comparable"] != comparable
            || (!comparable && index + 1 != runs.len())
        {
            return Err(invalid());
        }
        if comparable {
            identity = Some(key);
        }
        if comparable || child["complete"] != true {
            append_schedule(&mut rounds, child["rounds"].as_array().unwrap(), index);
        }
    }
    let complete =
        runs.len() == schedules.len() && runs.iter().all(|run| run["comparable"] == true);
    if report["rounds"] != json!(rounds) || report["complete"] != complete {
        return Err(invalid());
    }
    Ok(report)
}

pub fn catalog(root: &Path) -> Result<Vec<String>> {
    let dir = root.join("dagger");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|_| err("learning_io"))? {
        let entry = entry.map_err(|_| err("learning_io"))?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if crate::Uuid::parse_str(&id).is_ok_and(|uuid| uuid.to_string() == id)
            && entry.file_type().is_ok_and(|kind| kind.is_dir())
            && entry.path().join("report.json").is_file()
        {
            ids.push(id);
        }
    }
    ids.sort();
    ids.truncate(64);
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intervention_is_paired_and_respects_probability_endpoints() {
        let mut mix = Intervention {
            teacher_bps: 0,
            seed: 7,
            ik: false,
            joint_weights: vec![],
        };
        assert!((0..500).all(|step| !mix.teacher_at(1, step)));
        mix.teacher_bps = 10_000;
        assert!((0..500).all(|step| mix.teacher_at(1, step)));
        mix.teacher_bps = 5000;
        let choices = (0..500)
            .map(|step| mix.teacher_at(1, step))
            .collect::<Vec<_>>();
        assert!(choices.contains(&true) && choices.contains(&false));
        mix.ik = true;
        assert_eq!(
            choices,
            (0..500)
                .map(|step| mix.teacher_at(1, step))
                .collect::<Vec<_>>()
        );
    }
}
