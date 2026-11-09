//! Immutable, content-addressed policy/evaluation links in a project's episodes directory.
use super::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema_version: u32,
    pub job: String,
    pub batch: String,
    pub job_manifest_sha256: String,
    pub checkpoint_sha256: String,
    pub source_sha256: String,
    pub artifact_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evaluation {
    pub schema_version: u32,
    pub policy: String,
    pub run: String,
    pub manifest_sha256: String,
    pub comparison_key: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct PolicyView {
    pub id: String,
    pub record: Policy,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Score {
    pub tasks: usize,
    pub successes: usize,
    pub timeouts: usize,
    pub success_rate: f64,
    pub mean_final_distance_m: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct EvaluationView {
    pub split: rollout::Partition,
    pub id: String,
    pub record: Evaluation,
    pub score: Score,
    pub reference: Score,
}
#[derive(Clone, Debug, Serialize, Default)]
pub struct Listing {
    pub policies: Vec<PolicyView>,
    pub evaluations: Vec<EvaluationView>,
    pub issues: Vec<Issue>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Issue {
    pub id: String,
    pub code: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Comparison {
    pub key: String,
    pub a: EvaluationView,
    pub b: EvaluationView,
    pub rows: Vec<serde_json::Value>,
}
fn policy_shape(p: &Policy) -> bool {
    p.schema_version == 1
        && training::valid_name(&p.job)
        && batch_name(&p.batch)
        && [
            &p.job_manifest_sha256,
            &p.checkpoint_sha256,
            &p.source_sha256,
            &p.artifact_sha256,
        ]
        .iter()
        .all(|s| sha(s))
}
fn batch_name(s: &str) -> bool {
    s.len() <= 128
        && s.ends_with(".batch.json")
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}
fn record_name(s: &str, prefix: &str) -> bool {
    s.strip_prefix(prefix)
        .and_then(|s| s.strip_suffix(".json"))
        .is_some_and(sha)
}
fn save(root: &Path, prefix: &str, value: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| err("learning_registry_invalid"))?;
    let id = format!("{prefix}{}.json", hash(&bytes));
    let dir = root.join("registry");
    std::fs::create_dir_all(&dir).map_err(|_| err("learning_io"))?;
    if !dir.join(&id).exists()
        && std::fs::read_dir(&dir)
            .map_err(|_| err("learning_io"))?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                record_name(&n, "policy-") || record_name(&n, "evaluation-")
            })
            .take(128)
            .count()
            >= 128
    {
        return Err(err("learning_registry_limit"));
    }
    let mut temp = tempfile::NamedTempFile::new_in(&dir).map_err(|_| err("learning_io"))?;
    temp.write_all(&bytes)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|_| err("learning_io"))?;
    match temp.persist_noclobber(dir.join(&id)) {
        Ok(_) => {}
        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
            if training::read(&dir.join(&id), 16384)? != bytes {
                return Err(err("learning_registry_hash"));
            }
        }
        Err(_) => return Err(err("learning_io")),
    }
    std::fs::File::open(&dir)
        .and_then(|f| f.sync_all())
        .map_err(|_| err("learning_io"))?;
    Ok(id)
}
fn read<T: serde::de::DeserializeOwned>(root: &Path, id: &str, prefix: &str) -> Result<T> {
    if !record_name(id, prefix) {
        return Err(err("learning_registry_invalid"));
    }
    let bytes = training::read(&root.join("registry").join(id), 16384)?;
    if id != format!("{prefix}{}.json", hash(&bytes)) {
        return Err(err("learning_registry_hash"));
    }
    serde_json::from_slice(&bytes).map_err(|_| err("learning_registry_invalid"))
}
/// Always verifies the actual batch and complete training job, never just registry metadata.
pub fn select(root: &Path, id: &str) -> Result<Policy> {
    let p: Policy = read(root, id, "policy-")?;
    if !policy_shape(&p) {
        return Err(err("learning_registry_invalid"));
    }
    let job = training::inspect(&root.join("training").join(&p.job))?;
    let batch = root.join("exports").join(&p.batch);
    if !std::fs::symlink_metadata(&batch).is_ok_and(|m| m.is_file()) {
        return Err(err("learning_registry_invalid"));
    }
    let data = Validated::load(&batch)?;
    let checkpoint = hash(&training::read(
        &root.join("training").join(&p.job).join("best.pt"),
        8 * 1024 * 1024,
    )?);
    if job.manifest_sha256 != p.job_manifest_sha256
        || checkpoint != p.checkpoint_sha256
        || data.summary.source_sha256 != p.source_sha256
        || job.result["source_sha256"] != p.source_sha256
        || job.result["artifact_sha256"] != p.artifact_sha256
    {
        return Err(err("learning_registry_changed"));
    }
    Ok(p)
}
/// Registers the exact frozen best checkpoint and immutable source batch reference.
pub fn register(root: &Path, job: &str, batch: &str) -> Result<PolicyView> {
    if !training::valid_name(job) || !batch_name(batch) {
        return Err(err("learning_registry_invalid"));
    }
    let inspected = training::inspect(&root.join("training").join(job))?;
    let batch_path = root.join("exports").join(batch);
    if !std::fs::symlink_metadata(&batch_path).is_ok_and(|m| m.is_file()) {
        return Err(err("learning_registry_invalid"));
    }
    let data = Validated::load(&batch_path)?;
    let stage = tempfile::tempdir().map_err(|_| err("learning_io"))?;
    let artifact = data.prepare()?.save(&stage.path().join("data.json"))?;
    if inspected.result["source_sha256"] != data.summary.source_sha256
        || inspected.result["artifact_sha256"] != artifact
    {
        return Err(err("learning_registry_changed"));
    }
    let p = Policy {
        schema_version: 1,
        job: job.into(),
        batch: batch.into(),
        job_manifest_sha256: inspected.manifest_sha256,
        checkpoint_sha256: hash(&training::read(
            &root.join("training").join(job).join("best.pt"),
            8 * 1024 * 1024,
        )?),
        source_sha256: data.summary.source_sha256.clone(),
        artifact_sha256: artifact,
    };
    let id = save(root, "policy-", &p)?;
    Ok(PolicyView { id, record: p })
}
pub fn attach(root: &Path, policy: &str, run: &str) -> Result<EvaluationView> {
    let p = select(root, policy)?;
    if !rollout::valid_name(run) {
        return Err(err("learning_registry_invalid"));
    }
    let dir = root.join("rollouts").join(run);
    let report = rollout::inspect(&dir)?;
    let data = Validated::load(&root.join("exports").join(&p.batch))?;
    if data.summary().source_sha256 != p.source_sha256 {
        return Err(err("learning_registry_changed"));
    }
    rollout::validate_partition(&data, &report)?;
    if report["checkpoint_sha256"] != p.checkpoint_sha256
        || report["source_sha256"] != p.source_sha256
    {
        return Err(err("learning_registry_changed"));
    }
    let record = Evaluation {
        schema_version: 1,
        policy: policy.into(),
        run: run.into(),
        manifest_sha256: hash(&training::read(&dir.join("manifest.json"), 65536)?),
        comparison_key: rollout::comparison_key(&dir)?,
    };
    let id = save(root, "evaluation-", &record)?;
    Ok(EvaluationView {
        split: rollout::partition(&report)?,
        id,
        record,
        score: score(&report, "bc_policy"),
        reference: score(&report, "reference_servo"),
    })
}
fn score(report: &serde_json::Value, controller: &str) -> Score {
    let entries = report["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["controller"] == controller)
        .collect::<Vec<_>>();
    let tasks = entries.len();
    let successes = entries.iter().filter(|e| e["outcome"] == "success").count();
    Score {
        tasks,
        successes,
        timeouts: tasks - successes,
        success_rate: successes as f64 / tasks as f64,
        mean_final_distance_m: entries
            .iter()
            .map(|e| e["final_distance_m"].as_f64().unwrap())
            .sum::<f64>()
            / tasks as f64,
    }
}
pub(super) fn evaluation(root: &Path, id: &str) -> Result<(EvaluationView, serde_json::Value)> {
    let record: Evaluation = read(root, id, "evaluation-")?;
    if record.schema_version != 1 || !rollout::valid_name(&record.run) {
        return Err(err("learning_registry_invalid"));
    }
    let p = select(root, &record.policy)?;
    let dir = root.join("rollouts").join(&record.run);
    let report = rollout::inspect(&dir)?;
    let data = Validated::load(&root.join("exports").join(&p.batch))?;
    if data.summary().source_sha256 != p.source_sha256 {
        return Err(err("learning_registry_changed"));
    }
    rollout::validate_partition(&data, &report)?;
    if hash(&training::read(&dir.join("manifest.json"), 65536)?) != record.manifest_sha256
        || report["checkpoint_sha256"] != p.checkpoint_sha256
        || report["source_sha256"] != p.source_sha256
        || rollout::comparison_key(&dir)? != record.comparison_key
    {
        return Err(err("learning_registry_changed"));
    }
    Ok((
        EvaluationView {
            split: rollout::partition(&report)?,
            id: id.into(),
            record,
            score: score(&report, "bc_policy"),
            reference: score(&report, "reference_servo"),
        },
        report,
    ))
}
pub fn compare(root: &Path, a: &str, b: &str) -> Result<Comparison> {
    let (a, ar) = evaluation(root, a)?;
    let (b, br) = evaluation(root, b)?;
    if a.split != b.split || a.record.comparison_key != b.record.comparison_key {
        return Err(err("learning_registry_incompatible"));
    }
    let mut rows = vec![];
    for ea in ar["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["controller"] == "bc_policy")
    {
        let eb = br["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["controller"] == "bc_policy" && e["group"] == ea["group"])
            .ok_or_else(|| err("learning_registry_incompatible"))?;
        rows.push(json!({"seed":ea["seed"],"group":ea["group"],"a":ea,"b":eb}));
    }
    Ok(Comparison {
        key: a.record.comparison_key.clone(),
        a,
        b,
        rows,
    })
}
pub fn listing(root: &Path) -> Result<Listing> {
    let mut result = Listing::default();
    let dir = root.join("registry");
    if !dir.exists() {
        return Ok(result);
    }
    let mut names = vec![];
    for item in std::fs::read_dir(dir).map_err(|_| err("learning_io"))? {
        let item = item.map_err(|_| err("learning_io"))?;
        let name = item.file_name().to_string_lossy().to_string();
        if record_name(&name, "policy-") || record_name(&name, "evaluation-") {
            names.push(name);
        }
        if names.len() > 128 {
            return Err(err("learning_registry_limit"));
        }
    }
    names.sort();
    for id in names {
        let check = if id.starts_with("policy-") {
            read::<Policy>(root, &id, "policy-")
                .and_then(|record| {
                    if !policy_shape(&record) {
                        return Err(err("learning_registry_invalid"));
                    }
                    Ok(record)
                })
                .map(|record| {
                    result.policies.push(PolicyView {
                        id: id.clone(),
                        record,
                    })
                })
        } else {
            evaluation_summary(root, &id).map(|e| result.evaluations.push(e))
        };
        if let Err(e) = check {
            result.issues.push(Issue { id, code: e.code });
        }
    }
    Ok(result)
}

fn evaluation_summary(root: &Path, id: &str) -> Result<EvaluationView> {
    let record: Evaluation = read(root, id, "evaluation-")?;
    if record.schema_version != 1
        || !rollout::valid_name(&record.run)
        || !sha(&record.comparison_key)
    {
        return Err(err("learning_registry_invalid"));
    }
    let policy: Policy = read(root, &record.policy, "policy-")?;
    let dir = root.join("rollouts").join(&record.run);
    let report = rollout::summary(&dir)?;
    if hash(&training::read(&dir.join("manifest.json"), 65536)?) != record.manifest_sha256
        || report["checkpoint_sha256"] != policy.checkpoint_sha256
        || report["source_sha256"] != policy.source_sha256
    {
        return Err(err("learning_registry_changed"));
    }
    Ok(EvaluationView {
        split: rollout::partition(&report)?,
        id: id.into(),
        record,
        score: score(&report, "bc_policy"),
        reference: score(&report, "reference_servo"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_registry_records_are_idempotent_and_tampering_is_reported() {
        let root = tempfile::tempdir().unwrap();
        let p = Policy {
            schema_version: 1,
            job: "bc-one".into(),
            batch: "data.batch.json".into(),
            job_manifest_sha256: "1".repeat(64),
            checkpoint_sha256: "2".repeat(64),
            source_sha256: "3".repeat(64),
            artifact_sha256: "4".repeat(64),
        };
        let id = save(root.path(), "policy-", &p).unwrap();
        assert_eq!(id, save(root.path(), "policy-", &p).unwrap());
        assert_eq!(listing(root.path()).unwrap().policies.len(), 1);
        assert!(select(root.path(), &id).is_err()); // Dangling metadata never becomes a selectable policy.
        std::fs::write(root.path().join("registry").join(&id), b"{}").unwrap();
        assert_eq!(
            save(root.path(), "policy-", &p).unwrap_err().code,
            "learning_registry_hash"
        );
        let list = listing(root.path()).unwrap();
        assert!(list.policies.is_empty());
        assert_eq!(list.issues[0].code, "learning_registry_hash");
        assert!(select(root.path(), "../policy.json").is_err());
    }
}
