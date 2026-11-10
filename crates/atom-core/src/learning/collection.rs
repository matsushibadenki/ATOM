//! Train-only reference data collection. Original embedded holdouts are never regenerated.
use super::*;
use crate::{Project, reach};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub start_seed: u32,
    pub count: u32,
    pub horizon: u32,
    pub max_seconds: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mining: Option<String>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            start_seed: 0,
            count: 8,
            horizon: 300,
            max_seconds: 60,
            mining: None,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        if !(1..=16).contains(&self.count)
            || !(1..=500).contains(&self.horizon)
            || !(1..=300).contains(&self.max_seconds)
            || self.start_seed.checked_add(1023).is_none()
            || self
                .mining
                .as_ref()
                .is_some_and(|name| !super::mining::valid_name(name))
        {
            return Err(err("learning_collection_config"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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
pub struct Entry {
    pub seed: u32,
    pub group: String,
    pub episode: Option<String>,
    pub episode_sha256: Option<String>,
    pub accepted: bool,
    pub reasons: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32,
    pub format: String,
    pub config: Config,
    pub source: String,
    pub source_sha256: String,
    pub status: Status,
    pub code: Option<String>,
    pub entries: Vec<Entry>,
    pub attempted: u32,
    pub accepted: usize,
    pub exported: Option<String>,
    pub exported_sha256: Option<String>,
    pub holdout_sha256: String,
    pub groups: BTreeMap<Split, usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Saved {
    pub file: String,
    pub report: Report,
}
#[derive(Clone, Debug, Serialize)]
pub struct Progress {
    pub attempted: u32,
    pub total: u32,
    pub accepted: usize,
    pub seed: u32,
}
fn batch_name(name: &str) -> bool {
    name.ends_with(".batch.json")
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}
fn holdout(data: &Validated) -> Result<String> {
    Ok(hash(
        &serde_json::to_vec(
            &data
                .package
                .datasets
                .iter()
                .filter(|i| i.split != Split::Train)
                .collect::<Vec<_>>(),
        )
        .map_err(|_| err("learning_collection_invalid"))?,
    ))
}
pub(super) fn eligible(
    data: &Validated,
    project: &Project,
    config: &Config,
    seed: u32,
) -> Result<(String, Option<&'static str>)> {
    let first = &data.package.datasets[0].dataset;
    let m = reach::prepare(
        project,
        reach::Config {
            seed,
            robot: first.metadata.robot,
            horizon: config.horizon,
        },
    )
    .map_err(|_| err("learning_rollout_scene"))?;
    if m.model_sha256 != first.metadata.model_sha256
        || !same(&m.channels, &first.metadata.channels)
        || m.initial_action != first.transitions[0].observation.previous_action_rad
        || m.profile != first.metadata.profile
        || m.endpoint != first.metadata.endpoint
    {
        return Err(err("learning_rollout_scene"));
    }
    let group=hash(&serde_json::to_vec(&json!({"version":1,"profile":m.profile,"model":m.model_sha256,"seed":seed,"robot":m.config.robot,"endpoint":m.endpoint,"target":m.target})).unwrap());
    let reason = if data
        .package
        .report
        .request
        .split
        .assign(&group)
        .map_err(|_| err("learning_invalid_batch"))?
        != Split::Train
    {
        Some("collection_holdout")
    } else if data
        .package
        .labels
        .iter()
        .any(|labels| labels.group() == group)
        || data
            .package
            .report
            .entries
            .iter()
            .any(|e| e.quality.as_ref().is_some_and(|q| q.split_group == group))
    {
        Some("collection_existing")
    } else {
        None
    };
    Ok((group, reason))
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
pub(super) fn write_atomic(path: &Path, value: &impl Serialize) -> Result<String> {
    let parent = path.parent().ok_or_else(|| err("learning_io"))?;
    std::fs::create_dir_all(parent).map_err(|_| err("learning_io"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|_| err("learning_io"))?;
    let mut writer = LimitedWriter {
        file: tmp.as_file_mut(),
        hash: Sha256::new(),
        bytes: 0,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| err("learning_size_limit"))?;
    let digest = format!("{:x}", writer.hash.finalize());
    tmp.as_file().sync_all().map_err(|_| err("learning_io"))?;
    if let Err(e) = tmp.persist_noclobber(path)
        && (e.error.kind() != std::io::ErrorKind::AlreadyExists
            || hash(&training::read(path, MAX_BYTES)?) != digest)
    {
        return Err(err("learning_output_conflict"));
    }
    std::fs::File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| err("learning_io"))?;
    Ok(digest)
}
pub fn run(
    root: &Path,
    source: &str,
    project: &Project,
    config: &Config,
    cancel: &AtomicBool,
    progress: impl FnMut(Progress),
) -> Result<Saved> {
    run_checked(root, source, None, project, config, cancel, progress)
}
pub fn run_checked(
    root: &Path,
    source: &str,
    expected_sha256: Option<&str>,
    project: &Project,
    config: &Config,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<Saved> {
    config.validate()?;
    if !batch_name(source) {
        return Err(err("learning_collection_invalid"));
    }
    let deadline = Instant::now() + Duration::from_secs(config.max_seconds.into());
    let data = Validated::load(&root.join("exports").join(source))?;
    if expected_sha256.is_some_and(|expected| expected != data.summary.source_sha256) {
        return Err(err("learning_source_changed"));
    }
    if !data.summary.fit_data_ready
        || data.package.report.entries.len() + data.package.labels.len() + config.count as usize
            > 64
    {
        return Err(err("learning_collection_capacity"));
    }
    // Scene compatibility is checked before creating output or starting a worker.
    eligible(&data, project, config, config.start_seed)?;
    let priority = config
        .mining
        .as_ref()
        .map(|name| super::mining::selection(root, name, &data, config))
        .transpose()?;
    let mut report = Report {
        schema_version: 1,
        format: "atom-train-collection-v1".into(),
        config: config.clone(),
        source: source.into(),
        source_sha256: data.summary.source_sha256.clone(),
        status: Status::Exhausted,
        code: None,
        entries: vec![],
        attempted: 0,
        accepted: 0,
        exported: None,
        exported_sha256: None,
        holdout_sha256: holdout(&data)?,
        groups: data.summary.groups.clone(),
    };
    let mut package = data.package;
    let run_id = crate::Uuid::new_v4();
    // Scan at most 1024 deterministic seeds; skipped holdouts never launch physics.
    for seed in priority.unwrap_or_else(|| (config.start_seed..=config.start_seed + 1023).collect())
    {
        if let Some(status) = boundary(cancel, deadline) {
            report.status = status;
            break;
        }
        // Reuse the immutable source contract; no original Episode files are reopened.
        let context = Validated {
            summary: data.summary.clone(),
            package,
        };
        let (group, reason) = eligible(&context, project, config, seed)?;
        package = context.package;
        let mut entry = Entry {
            seed,
            group,
            episode: None,
            episode_sha256: None,
            accepted: false,
            reasons: vec![],
        };
        if let Some(reason) = reason {
            entry.reasons.push(reason.into());
            report.entries.push(entry);
            continue;
        }
        progress(Progress {
            attempted: report.attempted,
            total: config.count,
            accepted: report.accepted,
            seed,
        });
        if let Some(status) = boundary(cancel, deadline) {
            report.status = status;
            break;
        }
        report.attempted += 1;
        let result = (|| -> Result<()> {
            let per_run = Instant::now() + Duration::from_secs(30);
            let robot = package.datasets[0].dataset.metadata.robot;
            let mut runner = reach::Runner::reset(
                project,
                reach::Config {
                    seed,
                    robot,
                    horizon: config.horizon,
                },
            )
            .map_err(|_| err("learning_rollout_physics"))?;
            while runner.episode.frames.last().unwrap().outcome == Outcome::Running {
                if boundary(cancel, deadline).is_some() {
                    return Err(err("learning_collection_interrupted"));
                }
                if Instant::now() >= per_run {
                    return Err(err("learning_collection_run_budget"));
                }
                runner
                    .step(None)
                    .map_err(|_| err("learning_rollout_physics"))?;
            }
            let quality = runner
                .episode
                .quality()
                .map_err(|_| err("learning_collection_invalid"))?;
            if quality.split_group != entry.group {
                return Err(err("learning_collection_invalid"));
            }
            let name = format!("reach-collection-{run_id}-{seed}.episode.json");
            runner
                .episode
                .save(&root.join(&name))
                .map_err(|_| err("learning_io"))?;
            entry.episode = Some(name.clone());
            entry.episode_sha256 = Some(quality.episode_sha256.clone());
            let mut request = package.report.request.clone();
            request.sources = vec![root.join(&name)];
            let analyzed = request
                .analyze()
                .map_err(|_| err("learning_collection_invalid"))?;
            entry.reasons = analyzed.entries()[0].reasons.clone();
            if quality.outcome != Outcome::Success {
                entry.reasons.push("collection_timeout".into());
            }
            if entry.reasons.is_empty() {
                let audit: AuditEntry =
                    serde_json::from_value(serde_json::to_value(&analyzed.entries()[0]).unwrap())
                        .map_err(|_| err("learning_collection_invalid"))?;
                let dataset = runner
                    .episode
                    .dataset(package.report.request.policy)
                    .map_err(|_| err("learning_collection_invalid"))?;
                let count = package.report.splits.get_mut(&Split::Train).unwrap();
                count.episodes += 1;
                count.groups += 1;
                count.transitions += quality.transitions;
                package.report.request.sources.push(audit.source.clone());
                package.report.entries.push(audit);
                package.report.selected += 1;
                package.datasets.push(Item {
                    split: Split::Train,
                    dataset,
                });
                entry.accepted = true;
                report.accepted += 1;
            }
            Ok(())
        })();
        report.entries.push(entry);
        progress(Progress {
            attempted: report.attempted,
            total: config.count,
            accepted: report.accepted,
            seed,
        });
        if let Err(e) = result {
            report.status = boundary(cancel, deadline).unwrap_or(Status::Failed);
            report
                .entries
                .last_mut()
                .unwrap()
                .reasons
                .push(e.code.clone());
            report.code = Some(e.code);
            break;
        }
        if report.attempted == config.count {
            report.status = Status::Completed;
            break;
        }
    }
    if report.accepted > 0 {
        let name = format!("collection-{run_id}.batch.json");
        let path = root.join("exports").join(&name);
        let digest = write_atomic(&path, &package)?;
        let augmented = Validated::load(&path)?;
        if holdout(&augmented)? != report.holdout_sha256 {
            return Err(err("learning_collection_invalid"));
        }
        report.groups = augmented.summary.groups;
        report.exported = Some(name);
        report.exported_sha256 = Some(digest);
    }
    let bytes = serde_json::to_vec(&report).map_err(|_| err("learning_collection_invalid"))?;
    let file = format!("collection-{}.json", hash(&bytes));
    let path = root.join("collections").join(&file);
    write_atomic(&path, &report)?;
    Ok(Saved { file, report })
}
pub fn valid_name(name: &str) -> bool {
    name.strip_prefix("collection-")
        .and_then(|s| s.strip_suffix(".json"))
        .is_some_and(sha)
}
pub fn inspect(root: &Path, file: &str) -> Result<Saved> {
    if !valid_name(file) {
        return Err(err("learning_collection_invalid"));
    }
    let bytes = training::read(&root.join("collections").join(file), 256 * 1024)?;
    if file != format!("collection-{}.json", hash(&bytes)) {
        return Err(err("learning_collection_invalid"));
    }
    let report: Report =
        serde_json::from_slice(&bytes).map_err(|_| err("learning_collection_invalid"))?;
    report.config.validate()?;
    if report.schema_version != 1
        || report.format != "atom-train-collection-v1"
        || !batch_name(&report.source)
        || report.entries.len() > 1024
        || report.attempted > report.config.count
    {
        return Err(err("learning_collection_invalid"));
    }
    let original = Validated::load(&root.join("exports").join(&report.source))?;
    if original.summary.source_sha256 != report.source_sha256
        || holdout(&original)? != report.holdout_sha256
    {
        return Err(err("learning_source_changed"));
    }
    let skipped = |entry: &Entry| {
        entry
            .reasons
            .iter()
            .any(|r| r == "collection_holdout" || r == "collection_existing")
    };
    let priority = report
        .config
        .mining
        .as_ref()
        .map(|name| super::mining::selection(root, name, &original, &report.config))
        .transpose()?;
    let order_invalid = if let Some(seeds) = priority {
        report.entries.len() > seeds.len()
            || report
                .entries
                .iter()
                .zip(seeds)
                .any(|(entry, seed)| entry.seed != seed)
    } else {
        report
            .entries
            .windows(2)
            .any(|pair| pair[0].seed >= pair[1].seed)
    };
    if order_invalid
        || report.entries.iter().any(|e| {
            !sha(&e.group)
                || e.seed < report.config.start_seed
                || e.seed > report.config.start_seed + 1023
        })
        || report.entries.iter().filter(|e| !skipped(e)).count() != report.attempted as usize
        || (report.status == Status::Completed && report.attempted != report.config.count)
        || (report.accepted == 0 && report.groups != original.summary.groups)
    {
        return Err(err("learning_collection_invalid"));
    }
    let accepted = report
        .entries
        .iter()
        .filter(|e| e.accepted)
        .collect::<Vec<_>>();
    if accepted.len() != report.accepted
        || report.exported.is_some() != (report.accepted > 0)
        || report.exported.is_some() != report.exported_sha256.is_some()
    {
        return Err(err("learning_collection_invalid"));
    }
    if let Some(name) = &report.exported {
        if !batch_name(name) {
            return Err(err("learning_collection_invalid"));
        }
        let augmented = Validated::load(&root.join("exports").join(name))?;
        if Some(&augmented.summary.source_sha256) != report.exported_sha256.as_ref()
            || holdout(&augmented)? != report.holdout_sha256
            || augmented.package.datasets.len() != original.package.datasets.len() + accepted.len()
            || augmented.summary.groups != report.groups
            || !same(
                &original.package.report.request.split,
                &augmented.package.report.request.split,
            )
            || !same(
                &original.package.report.request.limits,
                &augmented.package.report.request.limits,
            )
            || original.package.report.request.policy != augmented.package.report.request.policy
            || augmented.package.report.entries.len()
                != original.package.report.entries.len() + accepted.len()
            || !same(
                &original.package.report.entries,
                &augmented.package.report.entries[..original.package.report.entries.len()],
            )
            || !same(
                &original.package.report.request.sources,
                &augmented.package.report.request.sources
                    [..original.package.report.request.sources.len()],
            )
            || !same(&original.package.labels, &augmented.package.labels)
            || !same(
                &original.package.datasets,
                &augmented.package.datasets[..original.package.datasets.len()],
            )
        {
            return Err(err("learning_collection_invalid"));
        }
        for (item, entry) in augmented.package.datasets[original.package.datasets.len()..]
            .iter()
            .zip(accepted)
        {
            if item.split != Split::Train
                || item.dataset.quality.split_group != entry.group
                || Some(&item.dataset.quality.episode_sha256) != entry.episode_sha256.as_ref()
                || item.dataset.metadata.seed != entry.seed
                || item.dataset.quality.outcome != Outcome::Success
                || !entry.reasons.is_empty()
                || item.dataset.metadata.horizon != report.config.horizon
                || !item.dataset.quality.reference_actions_only
                || original
                    .package
                    .labels
                    .iter()
                    .any(|labels| labels.group() == entry.group)
                || original.package.report.entries.iter().any(|e| {
                    e.quality
                        .as_ref()
                        .is_some_and(|q| q.split_group == entry.group)
                })
            {
                return Err(err("learning_collection_invalid"));
            }
            let name = entry
                .episode
                .as_deref()
                .filter(|name| reach::valid_name(name))
                .ok_or_else(|| err("learning_collection_invalid"))?;
            let episode = reach::Episode::load(&root.join(name))
                .map_err(|_| err("learning_collection_invalid"))?;
            let recorded = episode
                .dataset(original.package.report.request.policy)
                .map_err(|_| err("learning_collection_invalid"))?;
            if !same(&item.dataset, &recorded) {
                return Err(err("learning_collection_invalid"));
            }
        }
    }
    for entry in &report.entries {
        let assigned = original
            .package
            .report
            .request
            .split
            .assign(&entry.group)
            .map_err(|_| err("learning_collection_invalid"))?;
        let existed = original
            .package
            .labels
            .iter()
            .any(|labels| labels.group() == entry.group)
            || original.package.report.entries.iter().any(|e| {
                e.quality
                    .as_ref()
                    .is_some_and(|q| q.split_group == entry.group)
            });
        if (entry.reasons.iter().any(|r| r == "collection_holdout")
            && (assigned == Split::Train || entry.reasons.len() != 1))
            || (entry.reasons.iter().any(|r| r == "collection_existing")
                && (assigned != Split::Train || !existed || entry.reasons.len() != 1))
            || (!skipped(entry) && (assigned != Split::Train || existed))
        {
            return Err(err("learning_collection_invalid"));
        }
        if skipped(entry)
            && (entry.episode.is_some() || entry.episode_sha256.is_some() || entry.accepted)
        {
            return Err(err("learning_collection_invalid"));
        }
        if entry.episode.is_some() != entry.episode_sha256.is_some() {
            return Err(err("learning_collection_invalid"));
        }
        if let Some(name) = &entry.episode {
            if !reach::valid_name(name) {
                return Err(err("learning_collection_invalid"));
            }
            let episode = reach::Episode::load(&root.join(name))
                .map_err(|_| err("learning_collection_invalid"))?;
            if episode.manifest.config.seed != entry.seed {
                return Err(err("learning_collection_invalid"));
            }
            let q = episode
                .quality()
                .map_err(|_| err("learning_collection_invalid"))?;

            if Some(&q.episode_sha256) != entry.episode_sha256.as_ref()
                || q.split_group != entry.group
                || !q.complete
                || !q.reference_actions_only
                || episode.manifest.config.horizon != report.config.horizon
                || (entry.accepted && q.outcome != Outcome::Success)
            {
                return Err(err("learning_collection_invalid"));
            }
        }
    }
    Ok(Saved {
        file: file.into(),
        report,
    })
}
pub fn catalog(root: &Path) -> Result<Vec<String>> {
    let dir = root.join("collections");
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
