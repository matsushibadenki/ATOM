//! Frozen-checkpoint, fixed-test-group closed-loop Reach comparison. Never mutates a scene.
use super::*;
use crate::{Project, reach};
use std::{
    io::{BufRead, BufReader},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Partition {
    Validation,
    #[default]
    Test,
}
impl Partition {
    pub fn split(self) -> Split {
        match self {
            Self::Validation => Split::Validation,
            Self::Test => Split::Test,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Test => "test",
        }
    }
}
pub fn partition(report: &serde_json::Value) -> Result<Partition> {
    serde_json::from_value(report["split"].clone()).map_err(|_| err("learning_rollout_partition"))
}
pub(super) struct Policy {
    child: Child,
    input: ChildStdin,
    replies: mpsc::Receiver<Vec<u8>>,
    _stage: tempfile::TempDir,
    pub(super) runtime: serde_json::Value,
}
impl Drop for Policy {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Policy {
    pub(super) fn start(
        data: &Validated,
        job: &Path,
        cancel: &AtomicBool,
    ) -> Result<(Self, String)> {
        let (report, files) = training::checked_job(job)?;
        let stage = tempfile::tempdir().map_err(|_| err("learning_io"))?;
        let prepared = data.prepare()?;
        let artifact = prepared.save(&stage.path().join("data.json"))?;
        if report["source_sha256"] != data.summary.source_sha256
            || report["artifact_sha256"] != artifact
        {
            return Err(err("learning_checkpoint_config"));
        }
        let mut checkpoint_hash = String::new();
        for (name, bytes) in files {
            if name == "best.pt" {
                checkpoint_hash = hash(&bytes);
                std::fs::write(stage.path().join(name), bytes).map_err(|_| err("learning_io"))?;
            }
        }
        std::fs::write(
            stage.path().join("expected.json"),
            serde_json::to_vec(&json!({
                "artifact_sha256":artifact,"source_sha256":data.summary.source_sha256,
                "contract":prepared.contract,"normalization":prepared.normalization
            }))
            .map_err(|_| err("learning_io"))?,
        )
        .map_err(|_| err("learning_io"))?;
        let mut child = Command::new(python_executable())
            .args(["-I", "-c", include_str!("policy_worker.py")])
            .arg(stage.path())
            .env_remove("ATOM_RUNTIME_TOKEN")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| err("learning_python_unavailable"))?;
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, replies) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut line = vec![];
                match (&mut reader).take(32769).read_until(b'\n', &mut line) {
                    Ok(n) if n > 0 && n <= 32768 && line.last() == Some(&b'\n') => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        });
        let mut policy = Self {
            child,
            input,
            replies,
            _stage: stage,
            runtime: serde_json::Value::Null,
        };
        let ready: serde_json::Value = serde_json::from_slice(&policy.receive(20, cancel)?)
            .map_err(|_| err("learning_policy_protocol"))?;
        if ready["ready"] != true {
            return Err(err("learning_policy_protocol"));
        }
        policy.runtime = ready;
        Ok((policy, checkpoint_hash))
    }
    fn receive(&self, seconds: u64, cancel: &AtomicBool) -> Result<Vec<u8>> {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        loop {
            check_budget(deadline, cancel).map_err(|e| {
                if e.code == "learning_rollout_budget" {
                    err("learning_policy_worker")
                } else {
                    e
                }
            })?;
            match self.replies.recv_timeout(Duration::from_millis(50)) {
                Ok(bytes) => return Ok(bytes),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => return Err(err("learning_policy_worker")),
            }
        }
    }
    pub(super) fn action(
        &mut self,
        runner: &reach::Runner,
        cancel: &AtomicBool,
    ) -> Result<(Vec<f64>, usize)> {
        let features = features(&runner.episode);
        serde_json::to_writer(&mut self.input, &features)
            .map_err(|_| err("learning_policy_worker"))?;
        self.input
            .write_all(b"\n")
            .and_then(|_| self.input.flush())
            .map_err(|_| err("learning_policy_worker"))?;
        let mut action: Vec<f64> = serde_json::from_slice(&self.receive(3, cancel)?)
            .map_err(|_| err("learning_policy_protocol"))?;
        let channels = &runner.episode.manifest.channels;
        if action.len() != channels.len() || !finite(&action) {
            return Err(err("learning_policy_action"));
        }
        let mut clamped = 0;
        for (a, c) in action.iter_mut().zip(channels) {
            let bounded = a.clamp(f64::from(c.range[0]), f64::from(c.range[1]));
            clamped += usize::from(bounded != *a);
            *a = bounded;
        }
        Ok((action, clamped))
    }
}
pub(crate) fn features(episode: &reach::Episode) -> Vec<f64> {
    features_at(episode, episode.frames.len() - 1)
}
pub(crate) fn features_at(episode: &reach::Episode, index: usize) -> Vec<f64> {
    let m = &episode.manifest;
    let f = &episode.frames[index];
    m.channels
        .iter()
        .map(|c| f.observation.qpos[c.qpos_index])
        .chain(m.channels.iter().map(|c| f.observation.qvel[c.qpos_index]))
        .chain(f.observation.endpoint)
        .chain(m.target.map(f64::from))
        .chain(
            f.action
                .as_ref()
                .unwrap_or(&m.initial_action)
                .iter()
                .copied(),
        )
        .collect()
}
fn tasks(
    data: &Validated,
    project: &Project,
    partition: Partition,
) -> Result<Vec<(String, reach::Config)>> {
    let mut seen = BTreeSet::new();
    let mut tasks = vec![];
    for item in &data.package.datasets {
        if item.split != partition.split() {
            continue;
        }
        if !seen.insert(item.dataset.quality.split_group.clone()) {
            let previous = data
                .package
                .datasets
                .iter()
                .find(|candidate| {
                    candidate.split == partition.split()
                        && candidate.dataset.quality.split_group == item.dataset.quality.split_group
                })
                .unwrap();
            if previous.dataset.metadata.horizon != item.dataset.metadata.horizon {
                return Err(err("learning_rollout_ambiguous_task"));
            }
            continue;
        }
        let d = &item.dataset;
        let m = &d.metadata;
        let config = reach::Config {
            seed: m.seed,
            robot: m.robot,
            horizon: m.horizon,
        };
        let generated =
            reach::prepare(project, config.clone()).map_err(|_| err("learning_rollout_scene"))?;
        if generated.model_sha256 != m.model_sha256
            || !same(&generated.channels, &m.channels)
            || generated.target != d.transitions[0].observation.target_world_m
            || generated.initial_action != d.transitions[0].observation.previous_action_rad
            || generated.profile != m.profile
            || generated.endpoint != m.endpoint
        {
            return Err(err("learning_rollout_scene"));
        }
        tasks.push((d.quality.split_group.clone(), config));
    }
    if tasks.is_empty() {
        return Err(err(if partition == Partition::Test {
            "learning_rollout_requires_test"
        } else {
            "learning_rollout_requires_validation"
        }));
    }
    // Stable task order, independent of audit/source file order.
    tasks.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(tasks)
}
fn check_budget(deadline: Instant, cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(err("learning_rollout_cancelled"));
    }
    if Instant::now() >= deadline {
        return Err(err("learning_rollout_budget"));
    }
    Ok(())
}
/// Compare every original test group once, with its original seed and horizon.
/// A failed/cancelled/budget-exhausted run publishes no result. Max wall time 300s plus one pending operation.
/// Output Episodes retain the normal Reach format and are playable without PyTorch.
pub fn compare(
    data: &Validated,
    job: &Path,
    project: &Project,
    out: &Path,
    cancel: &AtomicBool,
) -> Result<serde_json::Value> {
    compare_partition(data, job, project, out, cancel, Partition::Test)
}
/// Execute exactly the selected saved validation or test groups. Empty partitions fail without fallback.
pub fn compare_partition(
    data: &Validated,
    job: &Path,
    project: &Project,
    out: &Path,
    cancel: &AtomicBool,
    partition: Partition,
) -> Result<serde_json::Value> {
    compare_deadline(
        data,
        job,
        project,
        out,
        cancel,
        partition,
        Instant::now() + Duration::from_secs(300),
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn compare_deadline(
    data: &Validated,
    job: &Path,
    project: &Project,
    out: &Path,
    cancel: &AtomicBool,
    partition: Partition,
    deadline: Instant,
) -> Result<serde_json::Value> {
    if out.exists() {
        return Err(err("learning_output_conflict"));
    }
    let tasks = tasks(data, project, partition)?;
    check_budget(deadline, cancel)?;
    let (mut policy, checkpoint_hash) = Policy::start(data, job, cancel)?;
    let parent = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let stage = tempfile::tempdir_in(parent).map_err(|_| err("learning_io"))?;
    let mut entries = vec![];
    let mut files = BTreeMap::new();
    let mut successes = [0usize; 2];
    let mut total_bytes = 0u64;
    for (index, (group, config)) in tasks.iter().enumerate() {
        let mut pair_engine = None;
        let mut initial = None;
        for (which, controller) in ["reference_servo", "bc_policy"].iter().enumerate() {
            check_budget(deadline, cancel)?;
            let run_deadline = Instant::now() + Duration::from_secs(30);
            let mut runner = reach::Runner::reset(project, config.clone())
                .map_err(|_| err("learning_rollout_physics"))?;
            let first = &runner.episode.frames[0].observation;
            let engine = (&first.engine, &first.python, &first.platform);
            let observed =
                serde_json::to_value(first).map_err(|_| err("learning_rollout_physics"))?;
            if let Some(expected) = &initial {
                if expected != &observed {
                    return Err(err("learning_rollout_reset_mismatch"));
                }
            } else {
                initial = Some(observed);
            }
            if let Some(expected) = &pair_engine {
                if expected != &json!(engine) {
                    return Err(err("learning_rollout_engine"));
                }
            } else {
                pair_engine = Some(json!(engine));
            }
            let mut clamped = 0;
            while runner.episode.frames.last().unwrap().outcome == reach::Outcome::Running {
                check_budget(deadline.min(run_deadline), cancel)?;
                let action = if which == 1 {
                    let (action, count) = policy.action(&runner, cancel)?;
                    clamped += count;
                    Some(action)
                } else {
                    None
                };
                runner
                    .step(action)
                    .map_err(|_| err("learning_rollout_physics"))?;
            }
            let episode = &runner.episode;
            let last = episode.frames.last().unwrap();
            successes[which] += usize::from(last.outcome == reach::Outcome::Success);
            let name = format!("{index:02}-{controller}.episode.json");
            let path = stage.path().join(&name);
            episode
                .save(&path)
                .map_err(|_| err("learning_rollout_episode"))?;
            total_bytes += std::fs::metadata(&path)
                .map_err(|_| err("learning_io"))?
                .len();
            if total_bytes > 256 * 1024 * 1024 {
                return Err(err("learning_size_limit"));
            }
            files.insert(
                name.clone(),
                hash(&std::fs::read(path).map_err(|_| err("learning_io"))?),
            );
            entries.push(json!({"group":group,"seed":config.seed,"horizon":config.horizon,"controller":controller,
                "episode":name,"outcome":last.outcome,"steps":last.index,"final_distance_m":last.distance,
                "clamped_action_components":clamped,"engine":pair_engine}));
        }
    }
    check_budget(deadline, cancel)?;
    let report = json!({"schema_version":1,"format":"atom-bc-rollout-v1","split":partition,
        "source_sha256":data.summary.source_sha256,"checkpoint_sha256":checkpoint_hash,
        "policy_runtime":policy.runtime,"architecture":"mlp-64-64-relu-v1","preprocessing":"torch-f32-normalize-v1",
        "task_groups":tasks.len(),"reference_successes":successes[0],"policy_successes":successes[1],
        "reference_success_rate":successes[0] as f64/tasks.len() as f64,"policy_success_rate":successes[1] as f64/tasks.len() as f64,
        "action_policy":"clamp_to_joint_limits","teacher_goals_in_policy_input":false,"entries":entries});
    let bytes = serde_json::to_vec_pretty(&report).map_err(|_| err("learning_io"))?;
    files.insert("report.json".into(), hash(&bytes));
    std::fs::write(stage.path().join("report.json"), bytes).map_err(|_| err("learning_io"))?;
    // Exclusive directory creation; the manifest is the final commit marker.
    std::fs::create_dir(out).map_err(|_| err("learning_output_conflict"))?;
    for name in files.keys() {
        let target = out.join(name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
            .map_err(|_| err("learning_io"))?;
        file.write_all(&std::fs::read(stage.path().join(name)).map_err(|_| err("learning_io"))?)
            .map_err(|_| err("learning_io"))?;
        file.sync_all().map_err(|_| err("learning_io"))?;
    }
    let manifest = json!({"schema_version":1,"format":"atom-bc-rollout-manifest-v1","files":files});
    let mut marker = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out.join("manifest.json"))
        .map_err(|_| err("learning_io"))?;
    marker
        .write_all(&serde_json::to_vec(&manifest).map_err(|_| err("learning_io"))?)
        .map_err(|_| err("learning_io"))?;
    marker.sync_all().map_err(|_| err("learning_io"))?;
    std::fs::File::open(out)
        .and_then(|f| f.sync_all())
        .map_err(|_| err("learning_io"))?;
    std::fs::File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| err("learning_io"))?;
    Ok(report)
}

/// Read only committed comparisons, checking every named file without trusting paths in JSON.
fn checked(directory: &Path, verify_files: bool) -> Result<(serde_json::Value, serde_json::Value)> {
    if !std::fs::symlink_metadata(directory).is_ok_and(|m| m.is_dir()) {
        return Err(err("learning_rollout_invalid"));
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&training::read(&directory.join("manifest.json"), 65536)?)
            .map_err(|_| err("learning_rollout_invalid"))?;
    if manifest["schema_version"] != 1 || manifest["format"] != "atom-bc-rollout-manifest-v1" {
        return Err(err("learning_rollout_invalid"));
    }
    let report = training::read(&directory.join("report.json"), 256 * 1024)?;
    if manifest["files"]["report.json"] != hash(&report) {
        return Err(err("learning_rollout_hash"));
    }
    let report: serde_json::Value =
        serde_json::from_slice(&report).map_err(|_| err("learning_rollout_invalid"))?;
    let entries = report["entries"]
        .as_array()
        .ok_or_else(|| err("learning_rollout_invalid"))?;
    let groups = report["task_groups"]
        .as_u64()
        .filter(|n| (1..=64).contains(n))
        .ok_or_else(|| err("learning_rollout_invalid"))?;
    if report["schema_version"] != 1
        || report["format"] != "atom-bc-rollout-v1"
        || partition(&report).is_err()
        || entries.len() != groups as usize * 2
        || manifest["files"].as_object().map(|v| v.len()) != Some(entries.len() + 1)
        || !report["source_sha256"].as_str().is_some_and(sha)
        || !report["checkpoint_sha256"].as_str().is_some_and(sha)
    {
        return Err(err("learning_rollout_invalid"));
    }
    let mut total = 0u64;
    let mut successes = [0u64; 2];
    let mut seen = BTreeSet::new();
    for (index, entry) in entries.iter().enumerate() {
        let controller = if index % 2 == 0 {
            "reference_servo"
        } else {
            "bc_policy"
        };
        let name = format!("{:02}-{controller}.episode.json", index / 2);
        let outcome = entry["outcome"].as_str().unwrap_or("");
        if entry["controller"] != controller
            || entry["episode"] != name
            || !matches!(outcome, "success" | "timeout")
            || !entry["seed"].as_u64().is_some_and(|n| n <= u32::MAX as u64)
            || !entry["horizon"]
                .as_u64()
                .is_some_and(|n| (1..=500).contains(&n))
            || !entry["steps"]
                .as_u64()
                .is_some_and(|n| n > 0 && n <= entry["horizon"].as_u64().unwrap_or(0))
            || !entry["final_distance_m"]
                .as_f64()
                .is_some_and(|n| n.is_finite() && n >= 0.)
            || entry["clamped_action_components"].as_u64().is_none()
            || !entry["group"].as_str().is_some_and(sha)
            || (index % 2 == 1
                && ["group", "seed", "horizon"]
                    .iter()
                    .any(|k| entry[*k] != entries[index - 1][*k]))
        {
            return Err(err("learning_rollout_invalid"));
        }
        if index % 2 == 0 && !seen.insert(entry["group"].as_str().unwrap()) {
            return Err(err("learning_rollout_invalid"));
        }
        successes[index % 2] += u64::from(outcome == "success");
        if !verify_files {
            continue;
        }
        let path = directory.join(name);
        if !std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_BYTES) {
            return Err(err("learning_rollout_invalid"));
        }
        let mut reader = std::fs::File::open(path)
            .map_err(|_| err("learning_io"))?
            .take(MAX_BYTES + 1);
        let mut digest = Sha256::new();
        let mut buf = [0; 8192];
        loop {
            let count = reader.read(&mut buf).map_err(|_| err("learning_io"))?;
            if count == 0 {
                break;
            }
            total += count as u64;
            if total > 256 * 1024 * 1024 {
                return Err(err("learning_size_limit"));
            }
            digest.update(&buf[..count]);
        }
        if manifest["files"][entry["episode"].as_str().unwrap()]
            != format!("{:x}", digest.finalize())
        {
            return Err(err("learning_rollout_hash"));
        }
    }
    for (index, key) in ["reference", "policy"].iter().enumerate() {
        if report[format!("{key}_successes")] != successes[index]
            || report[format!("{key}_success_rate")].as_f64()
                != Some(successes[index] as f64 / groups as f64)
        {
            return Err(err("learning_rollout_invalid"));
        }
    }
    Ok((report, manifest))
}
pub fn inspect(directory: &Path) -> Result<serde_json::Value> {
    Ok(checked(directory, true)?.0)
}
/// Load a verified saved Episode, protecting against replacement between inspection and load.
pub fn episode(directory: &Path, index: usize) -> Result<reach::Episode> {
    let (report, manifest) = checked(directory, true)?;
    load_verified(directory, index, &report, &manifest)
}
fn load_verified(
    directory: &Path,
    index: usize,
    report: &serde_json::Value,
    manifest: &serde_json::Value,
) -> Result<reach::Episode> {
    let entry = report["entries"]
        .get(index)
        .ok_or_else(|| err("learning_rollout_invalid"))?;
    let name = entry["episode"].as_str().unwrap();
    let bytes = training::read(&directory.join(name), MAX_BYTES)?;
    if manifest["files"][name] != hash(&bytes) {
        return Err(err("learning_rollout_hash"));
    }
    let temp = tempfile::NamedTempFile::new().map_err(|_| err("learning_io"))?;
    std::fs::write(temp.path(), bytes).map_err(|_| err("learning_io"))?;
    let episode = reach::Episode::load(temp.path()).map_err(|_| err("learning_rollout_episode"))?;
    let last = episode.frames.last().unwrap();
    if episode.manifest.config.seed != entry["seed"].as_u64().unwrap() as u32
        || episode.manifest.config.horizon != entry["horizon"].as_u64().unwrap() as u32
        || serde_json::to_value(last.outcome).unwrap() != entry["outcome"]
        || last.index != entry["steps"].as_u64().unwrap() as u32
        || last.distance != entry["final_distance_m"].as_f64().unwrap()
        || episode
            .quality()
            .map_err(|_| err("learning_rollout_episode"))?
            .split_group
            != entry["group"]
    {
        return Err(err("learning_rollout_episode"));
    }
    Ok(episode)
}
pub fn valid_name(name: &str) -> bool {
    name.starts_with("rollout-")
        && name.len() <= 96
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
pub fn catalog(directory: &Path) -> Result<Vec<String>> {
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut names = vec![];
    for entry in std::fs::read_dir(directory).map_err(|_| err("learning_io"))? {
        let entry = entry.map_err(|_| err("learning_io"))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if valid_name(&name)
            && entry.file_type().is_ok_and(|t| t.is_dir())
            && entry.path().join("manifest.json").is_file()
        {
            names.push(name);
        }
    }
    names.sort();
    names.reverse();
    names.truncate(64);
    Ok(names)
}

/// Exact evaluation identity, independent of training data but including reset/task/runtime conditions.
pub fn comparison_key(directory: &Path) -> Result<String> {
    let (report, manifest) = checked(directory, true)?;
    let mut tasks = vec![];
    for index in (0..report["entries"].as_array().unwrap().len()).step_by(2) {
        let episode = load_verified(directory, index, &report, &manifest)?;
        let m = &episode.manifest;
        let initial = &episode.frames[0].observation;
        tasks.push(
            json!({"group":report["entries"][index]["group"],"config":m.config,
            "profile":m.profile,"model":m.model_sha256,"channels":m.channels,"endpoint":m.endpoint,
            "target":m.target,"initial_action":m.initial_action,"physics_dt":m.physics_dt,
            "substeps":m.substeps,"tolerance":m.tolerance,"hold_steps":m.hold_steps,
            "engine":report["entries"][index]["engine"],"qpos":initial.qpos,"qvel":initial.qvel,
            "initial_endpoint":initial.endpoint}),
        );
    }
    tasks.sort_by_key(|v| v["group"].as_str().unwrap().to_string());
    let mut basis = json!({"schema_version":1,"tasks":tasks,"policy_runtime":report["policy_runtime"],
        "architecture":report["architecture"],"preprocessing":report["preprocessing"],"action_policy":report["action_policy"]});
    // Preserve all existing fixed-test registry keys. Validation uses an explicit separate namespace.
    if partition(&report)? == Partition::Validation {
        basis["partition"] = json!("validation");
    }
    Ok(hash(
        &serde_json::to_vec(&basis).map_err(|_| err("learning_io"))?,
    ))
}

/// Fast catalog summary: validates the manifest/report link; full Episode verification occurs on inspect/comparison.
pub fn summary(directory: &Path) -> Result<serde_json::Value> {
    Ok(checked(directory, false)?.0)
}

pub(super) fn visit_policy_episodes(
    directory: &Path,
    mut visit: impl FnMut(&reach::Episode, &serde_json::Value) -> Result<()>,
) -> Result<()> {
    let (report, manifest) = checked(directory, true)?;
    for index in (1..report["entries"].as_array().unwrap().len()).step_by(2) {
        let episode = load_verified(directory, index, &report, &manifest)?;
        visit(&episode, &report["entries"][index])?;
    }
    Ok(())
}

/// The declared evaluation partition must contain exactly its original saved groups, with no borrowing.
pub(super) fn validate_partition(
    data: &Validated,
    report: &serde_json::Value,
) -> Result<Partition> {
    let selected = partition(report)?;
    let mut expected = data
        .package
        .datasets
        .iter()
        .filter(|item| item.split == selected.split())
        .map(|item| item.dataset.quality.split_group.as_str())
        .collect::<BTreeSet<_>>();
    if expected.is_empty() {
        return Err(err(if selected == Partition::Test {
            "learning_rollout_requires_test"
        } else {
            "learning_rollout_requires_validation"
        }));
    }
    let entries = report["entries"]
        .as_array()
        .ok_or_else(|| err("learning_rollout_partition"))?;
    for entry in entries
        .iter()
        .filter(|e| e["controller"] == "reference_servo")
    {
        let group = entry["group"]
            .as_str()
            .ok_or_else(|| err("learning_rollout_partition"))?;
        let item = data
            .package
            .datasets
            .iter()
            .find(|item| {
                item.split == selected.split() && item.dataset.quality.split_group == group
            })
            .ok_or_else(|| err("learning_rollout_partition"))?;
        if !expected.remove(group)
            || entry["seed"] != item.dataset.metadata.seed
            || entry["horizon"] != item.dataset.metadata.horizon
        {
            return Err(err("learning_rollout_partition"));
        }
    }
    if !expected.is_empty() {
        return Err(err("learning_rollout_partition"));
    }
    Ok(selected)
}

/// Preflight identity of the exact validation tasks in the current authored scene, before starting workers.
pub(super) fn validation_spec(data: &Validated, project: &Project) -> Result<String> {
    let tasks = tasks(data, project, Partition::Validation)?;
    let mut basis = vec![];
    for (group, config) in tasks {
        let m = reach::prepare(project, config).map_err(|_| err("learning_rollout_scene"))?;
        basis.push(json!({"group":group,"config":m.config,"model":m.model_sha256,"channels":m.channels,
            "target":m.target,"initial_action":m.initial_action,"physics_dt":m.physics_dt,"substeps":m.substeps,
            "tolerance":m.tolerance,"hold_steps":m.hold_steps}));
    }
    Ok(hash(
        &serde_json::to_vec(&basis).map_err(|_| err("learning_io"))?,
    ))
}
