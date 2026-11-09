//! Bounded comparison of frozen registered candidates, using validation physics only.
use super::*;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub policies: Vec<String>,
    pub max_seconds: u32,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        if !(2..=4).contains(&self.policies.len())
            || !(1..=300).contains(&self.max_seconds)
            || self.policies.iter().collect::<BTreeSet<_>>().len() != self.policies.len()
            || self.policies.iter().any(|id| {
                !id.strip_prefix("policy-")
                    .and_then(|s| s.strip_suffix(".json"))
                    .is_some_and(sha)
            })
        {
            return Err(err("learning_campaign_config"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Completed,
    Cancelled,
    Budget,
    Failed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub policy: String,
    pub checkpoint_sha256: String,
    pub source_sha256: String,
    pub job_manifest_sha256: String,
    pub status: Status,
    pub code: Option<String>,
    pub evaluation: Option<String>,
    pub evaluation_manifest_sha256: Option<String>,
    pub score: Option<registry::Score>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32,
    pub format: String,
    pub config: Config,
    pub partition: rollout::Partition,
    pub task_spec_sha256: String,
    pub comparison_key: Option<String>,
    pub status: Status,
    pub candidates: Vec<Candidate>,
    pub ranking: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Saved {
    pub file: String,
    pub report: Report,
}
#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub completed: usize,
    pub total: usize,
    pub active_policy: Option<String>,
}
fn ranking(candidates: &[Candidate], status: Status) -> Vec<String> {
    if status != Status::Completed
        || candidates
            .iter()
            .any(|c| c.status != Status::Completed || c.score.is_none())
    {
        return vec![];
    }
    let mut candidates = candidates.iter().collect::<Vec<_>>();
    candidates.sort_by(|a, b| {
        b.score
            .as_ref()
            .unwrap()
            .successes
            .cmp(&a.score.as_ref().unwrap().successes)
            .then_with(|| {
                a.score
                    .as_ref()
                    .unwrap()
                    .mean_final_distance_m
                    .total_cmp(&b.score.as_ref().unwrap().mean_final_distance_m)
            })
            .then_with(|| a.policy.cmp(&b.policy))
    });
    candidates.iter().map(|c| c.policy.clone()).collect()
}
pub fn run(
    root: &Path,
    project: &crate::Project,
    config: &Config,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<Saved> {
    config.validate()?;
    let deadline = Instant::now() + Duration::from_secs(u64::from(config.max_seconds));
    let mut policies = vec![];
    let mut candidates = vec![];
    let mut spec = None;
    // Verify every candidate and common validation contract before any physics runs.
    for id in &config.policies {
        let policy = registry::select(root, id)?;
        let data = Validated::load(&root.join("exports").join(&policy.batch))?;
        if data.summary.source_sha256 != policy.source_sha256 {
            return Err(err("learning_registry_changed"));
        }
        let identity = rollout::validation_spec(&data, project)?;
        if spec.as_ref().is_some_and(|s| s != &identity) {
            return Err(err("learning_campaign_incompatible"));
        }
        spec = Some(identity);
        candidates.push(Candidate {
            policy: id.clone(),
            checkpoint_sha256: policy.checkpoint_sha256.clone(),
            source_sha256: policy.source_sha256.clone(),
            job_manifest_sha256: policy.job_manifest_sha256.clone(),
            status: Status::Pending,
            code: None,
            evaluation: None,
            evaluation_manifest_sha256: None,
            score: None,
        });
        policies.push(policy);
    }
    let mut report = Report {
        schema_version: 1,
        format: "atom-validation-campaign-v1".into(),
        config: config.clone(),
        partition: rollout::Partition::Validation,
        task_spec_sha256: spec.unwrap(),
        comparison_key: None,
        status: Status::Completed,
        candidates,
        ranking: vec![],
    };
    for (index, policy) in policies.iter().enumerate() {
        progress(Progress {
            completed: index,
            total: policies.len(),
            active_policy: Some(config.policies[index].clone()),
        });
        if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
            report.status = if cancel.load(Ordering::Relaxed) {
                Status::Cancelled
            } else {
                Status::Budget
            };
            report.candidates[index].status = report.status;
            break;
        }
        let result = (|| {
            let current = registry::select(root, &config.policies[index])?;
            if current.job_manifest_sha256 != policy.job_manifest_sha256 {
                return Err(err("learning_registry_changed"));
            }
            let data = Validated::load(&root.join("exports").join(&policy.batch))?;
            if data.summary.source_sha256 != policy.source_sha256 {
                return Err(err("learning_registry_changed"));
            }
            let parent = root.join("rollouts");
            std::fs::create_dir_all(&parent).map_err(|_| err("learning_io"))?;
            let name = format!("rollout-campaign-{}-{index}", crate::Uuid::new_v4());
            rollout::compare_deadline(
                &data,
                &root.join("training").join(&policy.job),
                project,
                &parent.join(&name),
                cancel,
                rollout::Partition::Validation,
                deadline,
            )?;
            let evaluation = registry::attach(root, &config.policies[index], &name)?;
            if evaluation.split != rollout::Partition::Validation
                || report
                    .comparison_key
                    .as_ref()
                    .is_some_and(|k| k != &evaluation.record.comparison_key)
            {
                return Err(err("learning_campaign_incompatible"));
            }
            Ok(evaluation)
        })();
        match result {
            Ok(evaluation) => {
                report.comparison_key = Some(evaluation.record.comparison_key.clone());
                let c = &mut report.candidates[index];
                c.status = Status::Completed;
                c.evaluation = Some(evaluation.id);
                c.evaluation_manifest_sha256 = Some(evaluation.record.manifest_sha256);
                c.score = Some(evaluation.score);
                progress(Progress {
                    completed: index + 1,
                    total: policies.len(),
                    active_policy: None,
                });
            }
            Err(e) => {
                report.status =
                    if cancel.load(Ordering::Relaxed) || e.code == "learning_rollout_cancelled" {
                        Status::Cancelled
                    } else if Instant::now() >= deadline || e.code == "learning_rollout_budget" {
                        Status::Budget
                    } else {
                        Status::Failed
                    };
                report.candidates[index].status = report.status;
                report.candidates[index].code = Some(e.code);
                break;
            }
        }
    }
    if report
        .candidates
        .iter()
        .all(|c| c.status == Status::Completed)
    {
        report.status = Status::Completed;
    }
    report.ranking = ranking(&report.candidates, report.status);
    save(root, report)
}
fn save(root: &Path, report: Report) -> Result<Saved> {
    let bytes = serde_json::to_vec(&report).map_err(|_| err("learning_campaign_invalid"))?;
    let dir = root.join("campaigns");
    std::fs::create_dir_all(&dir).map_err(|_| err("learning_io"))?;
    let file = format!("campaign-{}.json", hash(&bytes));
    let mut temp = tempfile::NamedTempFile::new_in(&dir).map_err(|_| err("learning_io"))?;
    temp.write_all(&bytes)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|_| err("learning_io"))?;
    match temp.persist_noclobber(dir.join(&file)) {
        Ok(_) => {}
        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
            if training::read(&dir.join(&file), 65536)? != bytes {
                return Err(err("learning_campaign_invalid"));
            }
        }
        Err(_) => return Err(err("learning_io")),
    }
    std::fs::File::open(&dir)
        .and_then(|f| f.sync_all())
        .map_err(|_| err("learning_io"))?;
    Ok(Saved { file, report })
}
pub fn valid_name(name: &str) -> bool {
    name.strip_prefix("campaign-")
        .and_then(|s| s.strip_suffix(".json"))
        .is_some_and(sha)
}
pub fn inspect(root: &Path, file: &str) -> Result<Saved> {
    if !valid_name(file) {
        return Err(err("learning_campaign_invalid"));
    }
    let bytes = training::read(&root.join("campaigns").join(file), 65536)?;
    if file != format!("campaign-{}.json", hash(&bytes)) {
        return Err(err("learning_campaign_invalid"));
    }
    let report: Report =
        serde_json::from_slice(&bytes).map_err(|_| err("learning_campaign_invalid"))?;
    report.config.validate()?;
    if report.status == Status::Pending
        || (report.status != Status::Completed
            && report
                .candidates
                .iter()
                .all(|c| c.status == Status::Completed))
    {
        return Err(err("learning_campaign_invalid"));
    }
    if report.schema_version != 1
        || report.format != "atom-validation-campaign-v1"
        || report.partition != rollout::Partition::Validation
        || report.candidates.len() != report.config.policies.len()
        || !sha(&report.task_spec_sha256)
        || report.ranking != ranking(&report.candidates, report.status)
        || (report.status == Status::Completed
            && report
                .candidates
                .iter()
                .any(|c| c.status != Status::Completed))
    {
        return Err(err("learning_campaign_invalid"));
    }
    for (candidate, expected) in report.candidates.iter().zip(&report.config.policies) {
        if candidate.policy != *expected {
            return Err(err("learning_campaign_invalid"));
        }
        let policy = registry::select(root, expected)?;
        if candidate.checkpoint_sha256 != policy.checkpoint_sha256
            || candidate.source_sha256 != policy.source_sha256
            || candidate.job_manifest_sha256 != policy.job_manifest_sha256
        {
            return Err(err("learning_registry_changed"));
        }
        if candidate.status == Status::Completed {
            let (view, _) = registry::evaluation(
                root,
                candidate
                    .evaluation
                    .as_deref()
                    .ok_or_else(|| err("learning_campaign_invalid"))?,
            )?;
            if view.split != rollout::Partition::Validation
                || view.record.policy != candidate.policy
                || Some(&view.record.comparison_key) != report.comparison_key.as_ref()
                || Some(&view.record.manifest_sha256)
                    != candidate.evaluation_manifest_sha256.as_ref()
                || Some(&view.score) != candidate.score.as_ref()
            {
                return Err(err("learning_campaign_invalid"));
            }
        } else if candidate.score.is_some()
            || candidate.evaluation.is_some()
            || candidate.evaluation_manifest_sha256.is_some()
        {
            return Err(err("learning_campaign_invalid"));
        }
    }
    Ok(Saved {
        file: file.into(),
        report,
    })
}
pub fn catalog(root: &Path) -> Result<Vec<String>> {
    let dir = root.join("campaigns");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut names = vec![];
    for entry in std::fs::read_dir(dir).map_err(|_| err("learning_io"))? {
        let entry = entry.map_err(|_| err("learning_io"))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if valid_name(&name) && entry.file_type().is_ok_and(|t| t.is_file()) {
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
    fn complete_denominator_required_and_validation_ranking_is_deterministic() {
        let make = |policy: &str, successes, distance| Candidate {
            policy: policy.into(),
            checkpoint_sha256: "a".repeat(64),
            source_sha256: "b".repeat(64),
            job_manifest_sha256: "c".repeat(64),
            status: Status::Completed,
            code: None,
            evaluation: Some("evaluation".into()),
            evaluation_manifest_sha256: Some("d".repeat(64)),
            score: Some(registry::Score {
                tasks: 3,
                successes,
                timeouts: 3 - successes,
                success_rate: successes as f64 / 3.,
                mean_final_distance_m: distance,
            }),
        };
        let mut candidates = vec![make("b", 2, 0.1), make("a", 2, 0.1), make("c", 1, 0.01)];
        assert_eq!(ranking(&candidates, Status::Completed), ["a", "b", "c"]);
        candidates[2].status = Status::Failed;
        assert!(ranking(&candidates, Status::Completed).is_empty());
        assert!(ranking(&candidates, Status::Cancelled).is_empty());
        let config = Config {
            policies: vec![
                format!("policy-{}.json", "a".repeat(64)),
                format!("policy-{}.json", "b".repeat(64)),
            ],
            max_seconds: 1,
        };
        assert!(config.validate().is_ok());
        let mut bad = config.clone();
        bad.policies[1] = bad.policies[0].clone();
        assert!(bad.validate().is_err());
        bad = config;
        bad.max_seconds = 301;
        assert!(bad.validate().is_err());
    }
}
