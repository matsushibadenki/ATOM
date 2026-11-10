//! Offline reference-servo labels on recorded BC-visited train states; applied actions stay intact.
use super::*;
use crate::reach;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
const TEACHER: &str = "reference_goal_servo_v1";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Labels {
    pub teacher: String,
    pub mining: String,
    pub policy: String,
    pub checkpoint_sha256: String,
    pub parent_source_sha256: String,
    pub stride: u32,
    pub episode_sha256: String,
    pub episode: reach::Episode,
}
impl Labels {
    pub(super) fn group(&self) -> String {
        let m = &self.episode.manifest;
        hash(&serde_json::to_vec(&json!({"version":1,"profile":m.profile,"model":m.model_sha256,"seed":m.config.seed,"robot":m.config.robot,"endpoint":m.endpoint,"target":m.target})).unwrap())
    }
    fn rows(&self) -> usize {
        (self.episode.frames.len() - 1).div_ceil(self.stride as usize)
    }
}
pub(super) fn validate_labels(
    package: &Package,
    rows: &mut BTreeMap<Split, usize>,
    groups: &mut BTreeMap<Split, usize>,
) -> Result<usize> {
    if package.datasets.len() + package.labels.len() > 64
        || package.report.entries.len() + package.labels.len() > 64
    {
        return Err(err("learning_supervision_invalid"));
    }
    let first = &package.datasets[0].dataset;
    let mut hashes = package
        .datasets
        .iter()
        .map(|i| i.dataset.quality.episode_sha256.clone())
        .collect::<BTreeSet<_>>();
    let mut ids = package
        .datasets
        .iter()
        .map(|i| i.dataset.quality.episode_id)
        .collect::<BTreeSet<_>>();
    let mut train_groups = package
        .datasets
        .iter()
        .filter(|i| i.split == Split::Train)
        .map(|i| i.dataset.quality.split_group.clone())
        .collect::<BTreeSet<_>>();
    let mut total = 0;
    for label in &package.labels {
        if label.teacher != TEACHER
            || !mining::valid_name(&label.mining)
            || !label
                .policy
                .strip_prefix("policy-")
                .and_then(|s| s.strip_suffix(".json"))
                .is_some_and(sha)
            || !sha(&label.checkpoint_sha256)
            || !sha(&label.parent_source_sha256)
            || !(1..=50).contains(&label.stride)
        {
            return Err(err("learning_supervision_invalid"));
        }
        let q = label
            .episode
            .quality()
            .map_err(|_| err("learning_supervision_invalid"))?;
        let m = &label.episode.manifest;
        if q.episode_sha256 != label.episode_sha256
            || !q.complete
            || q.transitions == 0
            || !hashes.insert(q.episode_sha256)
            || !ids.insert(q.episode_id)
            || m.model_sha256 != first.metadata.model_sha256
            || !same(&m.channels, &first.metadata.channels)
            || m.profile != first.metadata.profile
            || m.endpoint != first.metadata.endpoint
            || m.initial_action != first.transitions[0].observation.previous_action_rad
            || package
                .report
                .request
                .split
                .assign(&q.split_group)
                .map_err(|_| err("learning_supervision_invalid"))?
                != Split::Train
        {
            return Err(err("learning_supervision_invalid"));
        }
        total += label.rows();
        train_groups.insert(q.split_group);
    }
    *rows.get_mut(&Split::Train).unwrap() += total;
    groups.insert(Split::Train, train_groups.len());
    if rows.values().any(|n| *n > 32000) {
        return Err(err("learning_size_limit"));
    }
    Ok(total)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub mining: String,
    pub count: u32,
    pub stride: u32,
    pub max_seconds: u32,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        if !mining::valid_name(&self.mining)
            || !(1..=16).contains(&self.count)
            || !(1..=50).contains(&self.stride)
            || !(1..=300).contains(&self.max_seconds)
        {
            return Err(err("learning_supervision_config"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Completed,
    Cancelled,
    Budget,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub seed: u32,
    pub group: String,
    pub episode_sha256: String,
    pub rows: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32,
    pub format: String,
    pub teacher: String,
    pub config: Config,
    pub source: String,
    pub source_sha256: String,
    pub policy: String,
    pub checkpoint_sha256: String,
    pub status: Status,
    pub entries: Vec<Entry>,
    pub exported: Option<String>,
    pub exported_sha256: Option<String>,
    pub rows: usize,
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
    pub rows: usize,
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
pub fn run(
    root: &Path,
    config: &Config,
    expected_source: Option<&str>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<Saved> {
    config.validate()?;
    let deadline = Instant::now() + Duration::from_secs(config.max_seconds.into());
    let mined = mining::inspect(root, &config.mining)?.report;
    if mined.status != mining::Status::Completed || mined.priorities.len() < config.count as usize {
        return Err(err("learning_supervision_pool"));
    }
    if expected_source.is_some_and(|s| s != mined.source_sha256) {
        return Err(err("learning_mining_source"));
    }
    let source = Validated::load(&root.join("exports").join(&mined.source))?;
    if source.package.report.entries.len() + source.package.labels.len() + config.count as usize
        > 64
    {
        return Err(err("learning_collection_capacity"));
    }
    if source.summary.source_sha256 != mined.source_sha256 {
        return Err(err("learning_source_changed"));
    }
    let previous_teacher_rows = source.summary.teacher_rows;
    let mut report = Report {
        schema_version: 1,
        format: "atom-teacher-supervision-v1".into(),
        teacher: TEACHER.into(),
        config: config.clone(),
        source: mined.source.clone(),
        source_sha256: mined.source_sha256.clone(),
        policy: mined.config.policy.clone(),
        checkpoint_sha256: mined.checkpoint_sha256.clone(),
        status: Status::Completed,
        entries: vec![],
        exported: None,
        exported_sha256: None,
        rows: 0,
    };
    let mut package = source.package;
    for seed in mined.priorities.iter().take(config.count as usize) {
        progress(Progress {
            completed: report.entries.len(),
            total: config.count,
            rows: report.rows,
        });
        if let Some(status) = boundary(cancel, deadline) {
            report.status = status;
            break;
        }
        let probe = mined.probes.iter().find(|p| &p.seed == seed).unwrap();
        let episode = reach::Episode::load(&root.join(&probe.episode))
            .map_err(|_| err("learning_supervision_invalid"))?;
        let q = episode
            .quality()
            .map_err(|_| err("learning_supervision_invalid"))?;
        if q.episode_sha256 != probe.episode_sha256 {
            return Err(err("learning_supervision_invalid"));
        }
        let labels = Labels {
            teacher: TEACHER.into(),
            mining: config.mining.clone(),
            policy: mined.config.policy.clone(),
            checkpoint_sha256: mined.checkpoint_sha256.clone(),
            parent_source_sha256: mined.source_sha256.clone(),
            stride: config.stride,
            episode_sha256: q.episode_sha256.clone(),
            episode,
        };
        let rows = labels.rows();
        report.rows += rows;
        report.entries.push(Entry {
            seed: *seed,
            group: q.split_group,
            episode_sha256: q.episode_sha256,
            rows,
        });
        package.labels.push(labels);
    }
    if report.entries.len() == config.count as usize {
        // Validate a bounded staged package before it becomes a selectable export.
        let parent = root.join("exports");
        std::fs::create_dir_all(&parent).map_err(|_| err("learning_io"))?;
        let stage = tempfile::tempdir_in(&parent).map_err(|_| err("learning_io"))?;
        let staged = stage.path().join("checked.batch.json");
        let digest = collection::write_atomic(&staged, &package)?;
        let checked = Validated::load(&staged)?;
        if checked.summary.teacher_rows != previous_teacher_rows + report.rows {
            return Err(err("learning_supervision_invalid"));
        }
        progress(Progress {
            completed: report.entries.len(),
            total: config.count,
            rows: report.rows,
        });
        if let Some(status) = boundary(cancel, deadline) {
            report.status = status;
        } else {
            let name = format!("supervision-{}.batch.json", crate::Uuid::new_v4());
            std::fs::hard_link(&staged, parent.join(&name))
                .map_err(|_| err("learning_output_conflict"))?;
            std::fs::File::open(parent)
                .and_then(|f| f.sync_all())
                .map_err(|_| err("learning_io"))?;
            report.exported = Some(name);
            report.exported_sha256 = Some(digest);
        }
    }
    let bytes = serde_json::to_vec(&report).map_err(|_| err("learning_supervision_invalid"))?;
    let file = format!("supervision-{}.json", hash(&bytes));
    collection::write_atomic(&root.join("supervision").join(&file), &report)?;
    Ok(Saved { file, report })
}
pub fn valid_name(name: &str) -> bool {
    name.strip_prefix("supervision-")
        .and_then(|s| s.strip_suffix(".json"))
        .is_some_and(sha)
}
pub fn inspect(root: &Path, file: &str) -> Result<Saved> {
    if !valid_name(file) {
        return Err(err("learning_supervision_invalid"));
    }
    let bytes = training::read(&root.join("supervision").join(file), 65536)?;
    if file != format!("supervision-{}.json", hash(&bytes)) {
        return Err(err("learning_supervision_invalid"));
    }
    let report: Report =
        serde_json::from_slice(&bytes).map_err(|_| err("learning_supervision_invalid"))?;
    report.config.validate()?;
    let mined = mining::inspect(root, &report.config.mining)?.report;
    if report.schema_version != 1
        || report.format != "atom-teacher-supervision-v1"
        || report.teacher != TEACHER
        || mined.status != mining::Status::Completed
        || mined.priorities.len() < report.config.count as usize
        || report.source != mined.source
        || report.source_sha256 != mined.source_sha256
        || report.policy != mined.config.policy
        || report.checkpoint_sha256 != mined.checkpoint_sha256
        || report.entries.len() > report.config.count as usize
        || report.entries.iter().any(|e| e.rows > 500)
        || report.rows != report.entries.iter().map(|e| e.rows).sum::<usize>()
        || (report.status != Status::Completed && report.exported.is_some())
        || report.exported.is_some() != report.exported_sha256.is_some()
        || (report.status == Status::Completed)
            != (report.exported.is_some() && report.entries.len() == report.config.count as usize)
    {
        return Err(err("learning_supervision_invalid"));
    }
    for (entry, seed) in report.entries.iter().zip(&mined.priorities) {
        let p = mined.probes.iter().find(|p| &p.seed == seed).unwrap();
        if entry.seed != *seed
            || entry.group != p.group
            || entry.episode_sha256 != p.episode_sha256
            || entry.rows != (p.steps as usize).div_ceil(report.config.stride as usize)
        {
            return Err(err("learning_supervision_invalid"));
        }
    }
    if let Some(name) = &report.exported {
        if name.len() > 128
            || !name.starts_with("supervision-")
            || !name.ends_with(".batch.json")
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'.')
        {
            return Err(err("learning_supervision_invalid"));
        }
        let before = Validated::load(&root.join("exports").join(&report.source))?;
        let after = Validated::load(&root.join("exports").join(name))?;
        if before.summary.source_sha256 != report.source_sha256
            || Some(&after.summary.source_sha256) != report.exported_sha256.as_ref()
            || !same(&before.package.report, &after.package.report)
            || !same(&before.package.datasets, &after.package.datasets)
            || after.package.labels.len() != before.package.labels.len() + report.entries.len()
            || !same(
                &before.package.labels,
                &after.package.labels[..before.package.labels.len()],
            )
        {
            return Err(err("learning_supervision_invalid"));
        }
        for (label, entry) in after.package.labels[before.package.labels.len()..]
            .iter()
            .zip(&report.entries)
        {
            let q = label
                .episode
                .quality()
                .map_err(|_| err("learning_supervision_invalid"))?;
            if q.episode_sha256 != entry.episode_sha256
                || label.mining != report.config.mining
                || label.stride != report.config.stride
                || label.policy != report.policy
                || label.checkpoint_sha256 != report.checkpoint_sha256
                || label.parent_source_sha256 != report.source_sha256
            {
                return Err(err("learning_supervision_invalid"));
            }
        }
    }
    Ok(Saved {
        file: file.into(),
        report,
    })
}
pub fn catalog(root: &Path) -> Result<Vec<String>> {
    let dir = root.join("supervision");
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
