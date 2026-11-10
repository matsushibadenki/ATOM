//! Strict saved-batch → split-preserving behavior-cloning data contract. Never edits a scene.
use crate::reach::{
    Outcome,
    dataset::{
        self,
        batch::{self, Split},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::Path,
};
pub mod campaign;
pub mod collection;
pub mod diagnostics;
pub mod mining;
pub mod registry;
pub mod rollout;
pub mod supervision;
pub mod training;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
pub const FORMAT: &str = "atom-bc-dataset-v1";
#[derive(Debug, Serialize)]
pub struct Error {
    pub code: String,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.code)
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;
fn err(code: &str) -> Error {
    Error { code: code.into() }
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn sha(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditEntry {
    source: std::path::PathBuf,
    source_bytes: Option<u64>,
    quality: Option<dataset::Quality>,
    split: Option<Split>,
    selected: bool,
    reasons: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Audit {
    schema_version: u32,
    format: String,
    request: batch::Request,
    entries: Vec<AuditEntry>,
    selected: usize,
    rejected: usize,
    splits: BTreeMap<Split, batch::Count>,
    warnings: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    split: Split,
    dataset: dataset::Dataset,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    schema_version: u32,
    format: String,
    report: Audit,
    datasets: Vec<Item>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    labels: Vec<supervision::Labels>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub schema_version: u32,
    pub adapter: &'static str,
    pub source_sha256: String,
    pub feature_dim: usize,
    pub action_dim: usize,
    pub rows: BTreeMap<Split, usize>,
    pub groups: BTreeMap<Split, usize>,
    pub fit_data_ready: bool,
    pub test_data_ready: bool,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "is_zero")]
    pub teacher_rows: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub teacher_episodes: usize,
}
fn is_zero(value: &usize) -> bool {
    *value == 0
}
pub struct Validated {
    summary: Summary,
    package: Package,
}
fn same(a: &(impl Serialize + ?Sized), b: &(impl Serialize + ?Sized)) -> bool {
    match (serde_json::to_value(a), serde_json::to_value(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}
fn finite(v: &[f64]) -> bool {
    v.iter().all(|x| x.is_finite() && x.abs() <= 1e8)
}
fn observation(o: &dataset::PolicyObservation, n: usize) -> bool {
    o.joint_position_rad.len() == n
        && o.joint_velocity_rad_s.len() == n
        && o.previous_action_rad.len() == n
        && finite(&o.joint_position_rad)
        && finite(&o.joint_velocity_rad_s)
        && finite(&o.previous_action_rad)
        && finite(&o.endpoint_world_m)
        && o.target_world_m
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1e8)
}
fn group(d: &dataset::Dataset) -> String {
    hash(&serde_json::to_vec(&json!({"version":1,"profile":d.metadata.profile,"model":d.metadata.model_sha256,"seed":d.metadata.seed,"robot":d.metadata.robot,"endpoint":d.metadata.endpoint,"target":d.transitions[0].observation.target_world_m})).unwrap())
}
fn validate_dataset(d: &dataset::Dataset) -> Result<()> {
    let m = &d.metadata;
    let q = &d.quality;
    let n = m.channels.len();
    if d.schema_version != 1
        || d.format != "atom-reach-transitions-v1"
        || q.schema_version != 1
        || n == 0
        || n > 64
        || d.transitions.is_empty()
        || d.transitions.len() > 500
        || q.transitions != d.transitions.len()
        || q.frames != q.transitions + 1
        || !q.complete
        || !q.completed_exportable
        || q.outcome == Outcome::Running
        || q.success_exportable != (q.outcome == Outcome::Success)
        || d.policy == dataset::ExportPolicy::SuccessOnly && q.outcome != Outcome::Success
        || !sha(&q.episode_sha256)
        || !sha(&m.source_sha256)
        || !sha(&m.model_sha256)
        || !sha(&q.split_group)
    {
        return Err(err("learning_invalid_dataset"));
    }
    if m.robot == 0
        || m.endpoint == 0
        || m.engine.is_empty()
        || m.engine.len() > 80
        || m.python.is_empty()
        || m.python.len() > 40
        || m.platform.is_empty()
        || m.platform.len() > 200
        || m.profile != "atom-reach-dual-arm-zero-g-v1/splitmix64-v1"
        || m.world_up != "Y"
        || m.action_semantics != "absolute_joint_position_servo_target_rad"
        || m.control_dt_seconds != 0.02
        || m.physics_dt_seconds != 0.002
        || m.substeps != 10
        || m.tolerance_m != 0.035
        || m.hold_steps != 5
        || m.horizon == 0
        || m.horizon > 500
        || q.transitions > m.horizon as usize
        || !matches!(
            m.controller_provenance.as_str(),
            "reference_goal_servo" | "recorded_actions_unknown_policy"
        )
        || q.reference_actions_only != (m.controller_provenance == "reference_goal_servo")
    {
        return Err(err("learning_unsupported_semantics"));
    }
    let mut ids = BTreeSet::new();
    let mut indices = BTreeSet::new();
    for c in &m.channels {
        if c.id == 0
            || c.axis > 2
            || !ids.insert((c.id, c.axis))
            || !indices.insert(c.qpos_index)
            || c.qpos_index >= n
            || c.range.iter().any(|v| !v.is_finite())
            || c.range[0] > c.range[1]
        {
            return Err(err("learning_invalid_channels"));
        }
    }
    if d.transitions[0]
        .observation
        .previous_action_rad
        .iter()
        .zip(&m.channels)
        .any(|(v, c)| *v < f64::from(c.range[0]) || *v > f64::from(c.range[1]))
    {
        return Err(err("learning_invalid_transition"));
    }
    let mut hits = 0;
    for (i, t) in d.transitions.iter().enumerate() {
        if t.index != i
            || !t.time_seconds.is_finite()
            || !t.next_time_seconds.is_finite()
            || (t.time_seconds - i as f64 * 0.02).abs() > 1e-7
            || (t.next_time_seconds - (i + 1) as f64 * 0.02).abs() > 1e-7
            || !observation(&t.observation, n)
            || !observation(&t.next_observation, n)
            || t.action_rad.len() != n
            || !finite(&t.action_rad)
            || t.action_rad
                .iter()
                .zip(&m.channels)
                .any(|(a, c)| *a < f64::from(c.range[0]) || *a > f64::from(c.range[1]))
            || t.observation.target_world_m != d.transitions[0].observation.target_world_m
            || t.next_observation.target_world_m != t.observation.target_world_m
            || t.next_observation.previous_action_rad != t.action_rad
            || i > 0 && !same(&d.transitions[i - 1].next_observation, &t.observation)
        {
            return Err(err("learning_invalid_transition"));
        }
        let distance = t
            .next_observation
            .endpoint_world_m
            .iter()
            .zip(t.next_observation.target_world_m)
            .map(|(a, b)| (a - f64::from(b)).powi(2))
            .sum::<f64>()
            .sqrt();
        if !t.next_distance_m.is_finite() || (distance - t.next_distance_m).abs() > 1e-9 {
            return Err(err("learning_invalid_transition"));
        }
        hits = if distance <= 0.035 { hits + 1 } else { 0 };
        let success = hits >= 5;
        let timeout = !success && i + 1 == m.horizon as usize;
        if t.terminated != success
            || t.truncated != timeout
            || (success || timeout) && i + 1 != d.transitions.len()
        {
            return Err(err("learning_invalid_terminal"));
        }
    }
    let last = d.transitions.last().unwrap();
    if (q.outcome == Outcome::Success) != last.terminated
        || (q.outcome == Outcome::Timeout) != last.truncated
        || group(d) != q.split_group
    {
        return Err(err("learning_invalid_group_or_outcome"));
    }
    for v in [
        q.duration_seconds,
        q.initial_distance_m,
        q.final_distance_m,
        q.min_distance_m,
        q.max_abs_velocity_rad_s,
        q.max_tracking_error_rad,
        q.max_limit_excess_rad,
        q.max_action_delta_rad,
    ] {
        if !v.is_finite() || v < 0. {
            return Err(err("learning_invalid_quality"));
        }
    }
    // Recompute observed quality, independently of the serialized selection/audit claims.
    let first = &d.transitions[0].observation;
    let initial = first
        .endpoint_world_m
        .iter()
        .zip(first.target_world_m)
        .map(|(a, b)| (a - f64::from(b)).powi(2))
        .sum::<f64>()
        .sqrt();
    let mut min = initial;
    let mut velocity: f64 = 0.;
    let mut tracking: f64 = 0.;
    let mut excess: f64 = 0.;
    let mut delta: f64 = 0.;
    let mut accumulate = |o: &dataset::PolicyObservation, action: &[f64], previous: &[f64]| {
        for (j, c) in m.channels.iter().enumerate() {
            velocity = velocity.max(o.joint_velocity_rad_s[j].abs());
            tracking = tracking.max((o.joint_position_rad[j] - action[j]).abs());
            excess = excess.max(
                (f64::from(c.range[0]) - o.joint_position_rad[j])
                    .max(o.joint_position_rad[j] - f64::from(c.range[1]))
                    .max(0.),
            );
            delta = delta.max((action[j] - previous[j]).abs());
        }
    };
    accumulate(
        first,
        &first.previous_action_rad,
        &first.previous_action_rad,
    );
    for t in &d.transitions {
        accumulate(
            &t.next_observation,
            &t.action_rad,
            &t.observation.previous_action_rad,
        );
        min = min.min(t.next_distance_m);
    }
    if [
        q.duration_seconds - last.next_time_seconds,
        q.initial_distance_m - initial,
        q.final_distance_m - last.next_distance_m,
        q.min_distance_m - min,
        q.max_abs_velocity_rad_s - velocity,
        q.max_tracking_error_rad - tracking,
        q.max_limit_excess_rad - excess,
        q.max_action_delta_rad - delta,
    ]
    .iter()
    .any(|v| v.abs() > 1e-8)
    {
        return Err(err("learning_quality_mismatch"));
    }
    Ok(())
}
impl Validated {
    pub fn load(path: &Path) -> Result<Self> {
        if !std::fs::metadata(path)
            .map_err(|_| err("learning_io"))?
            .is_file()
        {
            return Err(err("learning_io"));
        }
        let mut bytes = vec![];
        std::fs::File::open(path)
            .map_err(|_| err("learning_io"))?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| err("learning_io"))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(err("learning_size_limit"));
        }
        let package: Package =
            serde_json::from_slice(&bytes).map_err(|_| err("learning_invalid_json"))?;
        let a = &package.report;
        if package.schema_version != 1
            || package.format != "atom-reach-batch-v1"
            || a.schema_version != 1
            || a.format != "atom-reach-batch-report-v1"
            || package.datasets.is_empty()
            || package.datasets.len() > 64
            || a.entries.len() > 64
            || a.selected != package.datasets.len()
            || a.selected + a.rejected != a.entries.len()
            || a.request.sources.len() != a.entries.len()
            || a.warnings.len() > 256
        {
            return Err(err("learning_invalid_batch"));
        }
        a.request
            .validate()
            .map_err(|_| err("learning_invalid_batch"))?;
        let mut selected = BTreeMap::new();
        for (entry, source) in a.entries.iter().zip(&a.request.sources) {
            if &entry.source != source || entry.source_bytes.is_some_and(|v| v > MAX_BYTES) {
                return Err(err("learning_audit_mismatch"));
            }
            if entry.selected {
                let q = entry
                    .quality
                    .as_ref()
                    .ok_or_else(|| err("learning_audit_mismatch"))?;
                if !entry.reasons.is_empty()
                    || entry.source_bytes.is_none()
                    || selected.insert(q.episode_id, entry).is_some()
                {
                    return Err(err("learning_audit_mismatch"));
                }
            } else if entry.reasons.is_empty() {
                return Err(err("learning_audit_mismatch"));
            }
        }
        if selected.len() != a.selected {
            return Err(err("learning_audit_mismatch"));
        }
        let mut seen = BTreeSet::new();
        let mut groups: BTreeMap<String, Split> = BTreeMap::new();
        let mut rows: BTreeMap<Split, usize> =
            [(Split::Train, 0), (Split::Validation, 0), (Split::Test, 0)].into();
        let mut episode_counts = rows.clone();
        let mut grouped: BTreeMap<Split, BTreeSet<String>> = BTreeMap::new();
        let first = &package.datasets[0].dataset.metadata;
        for item in &package.datasets {
            let d = &item.dataset;
            validate_dataset(d)?;
            if !same(&d.metadata.channels, &first.channels)
                || d.metadata.model_sha256 != first.model_sha256
            {
                return Err(err("learning_incompatible_channels_or_model"));
            }
            if !seen.insert(d.quality.episode_sha256.clone()) {
                return Err(err("learning_duplicate_episode"));
            }
            let e = selected
                .remove(&d.quality.episode_id)
                .ok_or_else(|| err("learning_audit_mismatch"))?;
            if !same(e.quality.as_ref().unwrap(), &d.quality)
                || e.split != Some(item.split)
                || d.policy != a.request.policy
            {
                return Err(err("learning_audit_mismatch"));
            }
            if a.request
                .split
                .assign(&d.quality.split_group)
                .map_err(|_| err("learning_invalid_batch"))?
                != item.split
                || groups
                    .insert(d.quality.split_group.clone(), item.split)
                    .is_some_and(|s| s != item.split)
            {
                return Err(err("learning_split_leakage"));
            }
            let q = &d.quality;
            let l = &a.request.limits;
            if [
                (q.max_abs_velocity_rad_s, l.max_abs_velocity_rad_s),
                (q.max_tracking_error_rad, l.max_tracking_error_rad),
                (q.max_limit_excess_rad, l.max_limit_excess_rad),
                (q.max_action_delta_rad, l.max_action_delta_rad),
                (q.final_distance_m, l.max_final_distance_m),
            ]
            .iter()
            .any(|(v, l)| l.is_some_and(|m| *v > m))
            {
                return Err(err("learning_quality_mismatch"));
            }
            *rows.get_mut(&item.split).unwrap() += d.transitions.len();
            *episode_counts.get_mut(&item.split).unwrap() += 1;
            grouped
                .entry(item.split)
                .or_default()
                .insert(q.split_group.clone());
        }
        let mut group_counts = BTreeMap::new();
        let mut warnings = vec![];
        for split in [Split::Train, Split::Validation, Split::Test] {
            let count = a
                .splits
                .get(&split)
                .ok_or_else(|| err("learning_audit_mismatch"))?;
            let groups = grouped.get(&split).map_or(0, BTreeSet::len);
            group_counts.insert(split, groups);
            if count.episodes != episode_counts[&split]
                || count.transitions != rows[&split]
                || count.groups != groups
            {
                return Err(err("learning_audit_mismatch"));
            }
            if rows[&split] == 0 {
                warnings.push(format!(
                    "learning_empty_{}",
                    match split {
                        Split::Train => "train",
                        Split::Validation => "validation",
                        Split::Test => "test",
                    }
                ));
            }
        }
        let teacher_rows = supervision::validate_labels(&package, &mut rows, &mut group_counts)?;
        if teacher_rows > 0 {
            warnings.retain(|warning| warning != "learning_empty_train");
            warnings.push("learning_teacher_supervision".into());
        }
        let summary = Summary {
            schema_version: 1,
            adapter: "pytorch_bc_v1",
            teacher_rows,
            teacher_episodes: package.labels.len(),
            source_sha256: hash(&bytes),
            feature_dim: first.channels.len() * 3 + 6,
            action_dim: first.channels.len(),
            fit_data_ready: rows[&Split::Train] > 0 && rows[&Split::Validation] > 0,
            test_data_ready: rows[&Split::Test] > 0,
            rows,
            groups: group_counts,
            warnings,
        };
        Ok(Self { summary, package })
    }
    pub fn curation_request(&self) -> batch::Request {
        self.package.report.request.clone()
    }
    pub fn summary(&self) -> Summary {
        self.summary.clone()
    }
    pub fn prepare(&self) -> Result<Prepared> {
        if self.summary.rows[&Split::Train] == 0 {
            return Err(err("learning_no_train"));
        }
        let dim = self.summary.feature_dim;
        let mut mean = vec![0.; dim];
        let mut m2 = vec![0.; dim];
        let mut count = 0usize;
        let mut splits: BTreeMap<Split, Vec<Sample>> = [
            (Split::Train, vec![]),
            (Split::Validation, vec![]),
            (Split::Test, vec![]),
        ]
        .into();
        for item in &self.package.datasets {
            for t in &item.dataset.transitions {
                let o = &t.observation;
                let features = o
                    .joint_position_rad
                    .iter()
                    .chain(&o.joint_velocity_rad_s)
                    .chain(&o.endpoint_world_m)
                    .copied()
                    .chain(o.target_world_m.iter().map(|v| f64::from(*v)))
                    .chain(o.previous_action_rad.iter().copied())
                    .collect::<Vec<_>>();
                splits.get_mut(&item.split).unwrap().push(Sample {
                    episode_id: item.dataset.quality.episode_id,
                    group: item.dataset.quality.split_group.clone(),
                    index: t.index,
                    features,
                    action: t.action_rad.clone(),
                });
            }
        }
        for labels in &self.package.labels {
            let group = labels.group();
            for index in (0..labels.episode.frames.len() - 1).step_by(labels.stride as usize) {
                splits.get_mut(&Split::Train).unwrap().push(Sample {
                    episode_id: labels.episode.id,
                    group: group.clone(),
                    index,
                    features: rollout::features_at(&labels.episode, index),
                    action: labels.episode.manifest.reference_action.clone(),
                });
            }
        }
        for sample in &splits[&Split::Train] {
            count += 1;
            for j in 0..dim {
                let delta = sample.features[j] - mean[j];
                mean[j] += delta / count as f64;
                m2[j] += delta * (sample.features[j] - mean[j]);
            }
        }
        let std = m2
            .into_iter()
            .map(|v| {
                let s = (v / count as f64).max(0.).sqrt();
                if s < 1e-8 { 1. } else { s }
            })
            .collect();
        Ok(Prepared {
            schema_version: 1,
            format: FORMAT.into(),
            summary: self.summary(),
            feature_order: vec![
                "joint_position_rad".into(),
                "joint_velocity_rad_s".into(),
                "endpoint_world_m".into(),
                "target_world_m".into(),
                "previous_action_rad".into(),
            ],
            contract: Contract {
                channels: self.package.datasets[0].dataset.metadata.channels.clone(),
                model_sha256: self.package.datasets[0]
                    .dataset
                    .metadata
                    .model_sha256
                    .clone(),
                world_up: "Y".into(),
                action_semantics: "absolute_joint_position_servo_target_rad".into(),
                control_dt_seconds: 0.02,
            },
            normalization: Normalization {
                fit_split: "train".into(),
                count,
                mean,
                std,
            },
            splits,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub episode_id: crate::Uuid,
    pub group: String,
    pub index: usize,
    pub features: Vec<f64>,
    pub action: Vec<f64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Normalization {
    pub fit_split: String,
    pub count: usize,
    pub mean: Vec<f64>,
    pub std: Vec<f64>,
}
#[derive(Serialize)]
pub struct Contract {
    pub channels: Vec<crate::reach::Channel>,
    pub model_sha256: String,
    pub world_up: String,
    pub action_semantics: String,
    pub control_dt_seconds: f64,
}
#[derive(Serialize)]
pub struct Prepared {
    schema_version: u32,
    format: String,
    summary: Summary,
    feature_order: Vec<String>,
    contract: Contract,
    normalization: Normalization,
    splits: BTreeMap<Split, Vec<Sample>>,
}
struct LimitedWriter<'a> {
    file: &'a mut std::fs::File,
    hash: Sha256,
    bytes: u64,
}
impl Write for LimitedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes + bytes.len() as u64 > MAX_BYTES {
            return Err(std::io::Error::other("learning_size_limit"));
        }
        self.file.write_all(bytes)?;
        self.hash.update(bytes);
        self.bytes += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
impl Prepared {
    pub fn save(&self, path: &Path) -> Result<String> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|_| err("learning_io"))?;
        let mut writer = std::io::BufWriter::new(LimitedWriter {
            file: tmp.as_file_mut(),
            hash: Sha256::new(),
            bytes: 0,
        });
        serde_json::to_writer(&mut writer, self).map_err(|_| err("learning_size_limit"))?;
        writer.flush().map_err(|_| err("learning_io"))?;
        let writer = writer.into_inner().map_err(|_| err("learning_io"))?;
        let size = writer.bytes;
        let digest = format!("{:x}", writer.hash.finalize());
        tmp.as_file().sync_all().map_err(|_| err("learning_io"))?;
        if path.exists() {
            if !std::fs::metadata(path)
                .map_err(|_| err("learning_io"))?
                .is_file()
            {
                return Err(err("learning_output_conflict"));
            }
            let mut file = std::fs::File::open(path)
                .map_err(|_| err("learning_io"))?
                .take(MAX_BYTES + 1);
            let mut hash = Sha256::new();
            let mut buffer = [0; 8192];
            let mut bytes = 0;
            loop {
                let n = file.read(&mut buffer).map_err(|_| err("learning_io"))?;
                if n == 0 {
                    break;
                }
                bytes += n as u64;
                hash.update(&buffer[..n]);
            }
            if bytes == size && format!("{:x}", hash.finalize()) == digest {
                return Ok(digest);
            }
            return Err(err("learning_output_conflict"));
        }
        tmp.persist_noclobber(path)
            .map_err(|_| err("learning_output_conflict"))?;
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|_| err("learning_io"))?;
        Ok(digest)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub schema_version: u32,
    pub adapter: String,
    pub connected: bool,
    pub data_loaded: bool,
    pub framework_available: bool,
    pub tensor_batch_verified: bool,
    pub fit_ready: bool,
    pub trained: bool,
    pub artifact_sha256: String,
    pub source_sha256: String,
    pub python: String,
    pub framework_version: Option<String>,
    pub rows: BTreeMap<Split, usize>,
    pub code: String,
}
struct Process(std::process::Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
/// Launch only the bundled loader script. No shell, downloads, optimizer or model startup.
pub fn diagnose(data: &Validated) -> Result<Diagnostic> {
    let python = python_executable();
    diagnose_using(data, &python)
}
pub(crate) fn diagnose_using(data: &Validated, python: &std::ffi::OsStr) -> Result<Diagnostic> {
    diagnose_using_budget(data, python, std::time::Duration::from_secs(20))
}
pub(crate) fn diagnose_using_budget(
    data: &Validated,
    python: &std::ffi::OsStr,
    budget: std::time::Duration,
) -> Result<Diagnostic> {
    use std::{
        process::{Command, Stdio},
        sync::mpsc,
        time::{Duration, Instant},
    };
    let temp = tempfile::tempdir().map_err(|_| err("learning_io"))?;
    let file = temp.path().join("fit.bc.json");
    let digest = data.prepare()?.save(&file)?;
    let mut process = Process(
        Command::new(python)
            .args(["-I", "-c", include_str!("learning_worker.py")])
            .arg(&file)
            .arg(&digest)
            .env_remove("ATOM_RUNTIME_TOKEN")
            .current_dir(temp.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| err("learning_python_unavailable"))?,
    );
    let output = process.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = vec![];
        let result = output.take(16385).read_to_end(&mut bytes).map(|_| bytes);
        let _ = tx.send(result);
    });
    let deadline = Instant::now() + budget;
    let bytes = rx
        .recv_timeout(budget)
        .map_err(|_| err("learning_worker_timeout"))?
        .map_err(|_| err("learning_worker_protocol"))?;
    if bytes.len() > 16384 {
        return Err(err("learning_worker_protocol"));
    }
    loop {
        match process
            .0
            .try_wait()
            .map_err(|_| err("learning_worker_protocol"))?
        {
            Some(status) if status.success() => break,
            Some(_) => return Err(err("learning_worker_protocol")),
            None => {
                if Instant::now() >= deadline {
                    return Err(err("learning_worker_timeout"));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    let reply: Diagnostic =
        serde_json::from_slice(&bytes).map_err(|_| err("learning_worker_protocol"))?;
    if reply.schema_version != 1
        || reply.adapter != "pytorch_bc_v1"
        || !reply.connected
        || !reply.data_loaded
        || reply.trained
        || reply.artifact_sha256 != digest
        || reply.source_sha256 != data.summary.source_sha256
        || reply.rows != data.summary.rows
        || reply.python.is_empty()
        || reply.python.len() > 40
        || reply
            .framework_version
            .as_ref()
            .is_some_and(|v| v.len() > 80)
        || reply.fit_ready
            && (!reply.tensor_batch_verified
                || !reply.framework_available
                || !data.summary.fit_data_ready)
        || reply.tensor_batch_verified && !reply.framework_available
        || !matches!(
            reply.code.as_str(),
            "learning_ready"
                | "learning_missing_validation"
                | "learning_torch_unavailable"
                | "learning_tensor_failed"
        )
    {
        return Err(err("learning_worker_protocol"));
    }
    if (reply.code == "learning_ready") != reply.fit_ready
        || (reply.code == "learning_torch_unavailable" && reply.framework_available)
        || (reply.code == "learning_missing_validation"
            && (!reply.tensor_batch_verified || data.summary.fit_data_ready))
        || (reply.framework_available && reply.framework_version.is_none())
        || (!reply.framework_available && reply.framework_version.is_some())
    {
        return Err(err("learning_worker_protocol"));
    }
    Ok(reply)
}

/// Prefer the explicitly configured interpreter, then this checkout's isolated learning env.
pub fn python_executable() -> std::ffi::OsString {
    if let Some(path) = std::env::var_os("ATOM_LEARNING_PYTHON") {
        return path;
    }
    let local = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv-learning/Scripts/python.exe"
    } else {
        "../../.venv-learning/bin/python"
    });
    if local.is_file() {
        local.into_os_string()
    } else {
        "python3".into()
    }
}
