//! Bounded, deterministic offline curation. No GUI document or physics worker access.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
pub const MAX_SOURCES: usize = 64;
pub const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitConfig {
    pub salt: String,
    pub validation_bps: u16,
    pub test_bps: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_abs_velocity_rad_s: Option<f64>,
    pub max_tracking_error_rad: Option<f64>,
    pub max_limit_excess_rad: Option<f64>,
    pub max_action_delta_rad: Option<f64>,
    pub max_final_distance_m: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema_version: u32,
    pub policy: ExportPolicy,
    pub split: SplitConfig,
    pub limits: Limits,
    pub sources: Vec<PathBuf>,
}
impl Default for Request {
    fn default() -> Self {
        Self {
            schema_version: 1,
            policy: ExportPolicy::SuccessOnly,
            split: SplitConfig {
                salt: "atom-reach-split-v1".into(),
                validation_bps: 1000,
                test_bps: 1000,
            },
            limits: Limits {
                max_abs_velocity_rad_s: None,
                max_tracking_error_rad: None,
                max_limit_excess_rad: Some(1e-6),
                max_action_delta_rad: None,
                max_final_distance_m: None,
            },
            sources: vec![],
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Split {
    Train,
    Validation,
    Test,
}
impl SplitConfig {
    fn validate(&self) -> Result<()> {
        if self.salt.is_empty()
            || self.salt.len() > 64
            || self.salt.chars().any(char::is_control)
            || u32::from(self.validation_bps) + u32::from(self.test_bps) >= 10000
        {
            return Err(Error::Invalid("Batch split configuration".into()));
        }
        Ok(())
    }
    pub fn assign(&self, group: &str) -> Result<Split> {
        self.validate()?;
        let hash = Sha256::digest(serde_json::to_vec(&serde_json::json!([
            "atom-group-split-v1",
            self.salt,
            group
        ]))?);
        let bucket = u64::from_be_bytes(hash[..8].try_into().unwrap()) % 10000;
        Ok(if bucket < u64::from(self.test_bps) {
            Split::Test
        } else if bucket < u64::from(self.test_bps) + u64::from(self.validation_bps) {
            Split::Validation
        } else {
            Split::Train
        })
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub source: PathBuf,
    pub source_bytes: Option<u64>,
    pub quality: Option<Quality>,
    pub split: Option<Split>,
    pub selected: bool,
    pub reasons: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Count {
    pub episodes: usize,
    pub groups: usize,
    pub transitions: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    schema_version: u32,
    format: &'static str,
    request: Request,
    entries: Vec<Entry>,
    selected: usize,
    rejected: usize,
    splits: BTreeMap<Split, Count>,
    warnings: Vec<String>,
}
fn invalid(what: &str) -> Error {
    Error::Invalid(what.into())
}
impl Request {
    pub(crate) fn validate(&self) -> Result<()> {
        self.split.validate()?;
        if self.schema_version != 1
            || self.sources.is_empty()
            || self.sources.len() > MAX_SOURCES
            || self
                .sources
                .iter()
                .any(|p| p.to_str().is_none_or(|s| s.is_empty() || s.len() > 4096))
        {
            return Err(invalid("Batch sources/version"));
        }
        for v in [
            self.limits.max_abs_velocity_rad_s,
            self.limits.max_tracking_error_rad,
            self.limits.max_limit_excess_rad,
            self.limits.max_action_delta_rad,
            self.limits.max_final_distance_m,
        ]
        .into_iter()
        .flatten()
        {
            if !v.is_finite() || v < 0. {
                return Err(invalid("Batch quality limit"));
            }
        }
        Ok(())
    }
    pub fn analyze(&self) -> Result<Report> {
        self.validate()?;
        let mut request = self.clone();
        request.sources.sort();
        let mut total: u64 = 0;
        let mut entries = vec![];
        for source in &request.sources {
            let mut source_bytes = None;
            let result = (|| {
                let metadata = std::fs::metadata(source)?;
                if !metadata.is_file() {
                    return Err(invalid("Batch source must be a regular file"));
                }
                source_bytes = Some(metadata.len());
                total = total.saturating_add(metadata.len());
                if total > MAX_INPUT_BYTES {
                    return Err(invalid("Batch input budget"));
                }
                Episode::load_bounded(source, metadata.len())?.quality()
            })();
            // Aggregate budget is a request failure; malformed individual files stay visible.
            if total > MAX_INPUT_BYTES {
                return Err(invalid("Batch input budget"));
            }
            let mut entry = Entry {
                source: source.clone(),
                source_bytes,
                quality: None,
                split: None,
                selected: false,
                reasons: vec![],
            };
            match result {
                Err(e) => entry.reasons.push(format!("source_{}", e.code())),
                Ok(q) => {
                    entry.split = Some(request.split.assign(&q.split_group)?);
                    if !q.complete {
                        entry.reasons.push("partial_episode".into());
                    } else if q.transitions == 0 {
                        entry.reasons.push("no_transitions".into());
                    } else if request.policy == ExportPolicy::SuccessOnly
                        && q.outcome != Outcome::Success
                    {
                        entry.reasons.push("outcome_filtered".into());
                    }
                    for (value, limit, code) in [
                        (
                            q.max_abs_velocity_rad_s,
                            request.limits.max_abs_velocity_rad_s,
                            "velocity_limit",
                        ),
                        (
                            q.max_tracking_error_rad,
                            request.limits.max_tracking_error_rad,
                            "tracking_error_limit",
                        ),
                        (
                            q.max_limit_excess_rad,
                            request.limits.max_limit_excess_rad,
                            "joint_limit_excess",
                        ),
                        (
                            q.max_action_delta_rad,
                            request.limits.max_action_delta_rad,
                            "action_delta_limit",
                        ),
                        (
                            q.final_distance_m,
                            request.limits.max_final_distance_m,
                            "final_distance_limit",
                        ),
                    ] {
                        if limit.is_some_and(|m| value > m) {
                            entry.reasons.push(code.into());
                        }
                    }
                    entry.quality = Some(q);
                }
            }
            entries.push(entry);
        }
        // Checkpoints of one run share UUID and group. Prefer the longest eligible
        // version; a UUID reused across different tasks is a conflict for every version.
        let mut identities: BTreeMap<Uuid, BTreeSet<String>> = BTreeMap::new();
        let mut best: BTreeMap<Uuid, (usize, String)> = BTreeMap::new();
        for entry in &entries {
            if let Some(q) = &entry.quality {
                identities
                    .entry(q.episode_id)
                    .or_default()
                    .insert(q.split_group.clone());
                if entry.reasons.is_empty() {
                    let key = (q.transitions, q.episode_sha256.clone());
                    let winner = best.entry(q.episode_id).or_insert_with(|| key.clone());
                    if key > *winner {
                        *winner = key;
                    }
                }
            }
        }
        let mut seen = BTreeSet::new();
        let mut splits: BTreeMap<_, Count> = [Split::Train, Split::Validation, Split::Test]
            .into_iter()
            .map(|s| (s, Count::default()))
            .collect();
        let mut groups: BTreeMap<Split, BTreeSet<String>> = BTreeMap::new();
        for entry in &mut entries {
            if let Some(q) = &entry.quality {
                if identities[&q.episode_id].len() > 1 {
                    entry.reasons.push("episode_id_conflict".into());
                }
                if entry.reasons.is_empty()
                    && best
                        .get(&q.episode_id)
                        .is_some_and(|(_, hash)| hash != &q.episode_sha256)
                {
                    entry.reasons.push("superseded_episode_version".into());
                }
                if !seen.insert(q.episode_sha256.clone()) {
                    entry.reasons.push("duplicate_episode".into());
                }
                entry.selected = entry.reasons.is_empty();
                if entry.selected {
                    let split = entry.split.unwrap();
                    let count = splits.get_mut(&split).unwrap();
                    count.episodes += 1;
                    count.transitions += q.transitions;
                    groups
                        .entry(split)
                        .or_default()
                        .insert(q.split_group.clone());
                }
            }
        }
        let mut warnings = vec![
            "no_physics_safety_or_generalization_certification".into(),
            "hash_split_proportions_not_exact".into(),
            "different_models_or_seeds_may_still_be_correlated".into(),
        ];
        for (split, count) in &mut splits {
            count.groups = groups.get(split).map_or(0, BTreeSet::len);
            if count.episodes == 0 {
                warnings.push(format!(
                    "empty_{}_partition",
                    match split {
                        Split::Train => "train",
                        Split::Validation => "validation",
                        Split::Test => "test",
                    }
                ));
            }
        }
        let selected = entries.iter().filter(|e| e.selected).count();
        Ok(Report {
            schema_version: 1,
            format: "atom-reach-batch-report-v1",
            request,
            rejected: entries.len() - selected,
            selected,
            entries,
            splits,
            warnings,
        })
    }
}
// Each dataset is loaded, hash-checked and serialized individually. No batch-sized
// transition array is held in memory, and publication only happens after all succeed.
struct Selected<'a>(&'a Report);
impl Serialize for Selected<'_> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::{ser::Error, ser::SerializeSeq};
        let mut seq = serializer.serialize_seq(Some(self.0.selected))?;
        for entry in self.0.entries.iter().filter(|e| e.selected) {
            if !std::fs::metadata(&entry.source)
                .map_err(S::Error::custom)?
                .is_file()
            {
                return Err(S::Error::custom("batch_source_changed"));
            }
            let episode = Episode::load_bounded(&entry.source, entry.source_bytes.unwrap())
                .map_err(S::Error::custom)?;
            let quality = episode.quality().map_err(S::Error::custom)?;
            if quality.episode_sha256 != entry.quality.as_ref().unwrap().episode_sha256 {
                return Err(S::Error::custom("batch_source_changed"));
            }
            let dataset = episode
                .dataset(self.0.request.policy)
                .map_err(S::Error::custom)?;
            #[derive(Serialize)]
            struct Item {
                split: Option<Split>,
                dataset: Dataset,
            }
            seq.serialize_element(&Item {
                split: entry.split,
                dataset,
            })?;
        }
        seq.end()
    }
}
#[derive(Debug, Serialize)]
pub struct Receipt {
    pub sha256: String,
    pub bytes: u64,
    pub selected: usize,
}
struct BoundedHash<'a> {
    file: &'a mut std::fs::File,
    hash: Sha256,
    bytes: u64,
}
impl Write for BoundedHash<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.bytes + buf.len() as u64 > MAX_BYTES {
            return Err(std::io::Error::other("Batch output budget"));
        }
        self.file.write_all(buf)?;
        self.hash.update(buf);
        self.bytes += buf.len() as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
impl Report {
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn splits(&self) -> &BTreeMap<Split, Count> {
        &self.splits
    }
    /// A single atomic, no-clobber package. This report cannot be deserialized from untrusted input.
    pub fn save_package(&self, path: &Path) -> Result<Receipt> {
        if self.selected == 0 {
            return Err(Error::EpisodeExportIneligible);
        }
        #[derive(Serialize)]
        struct Package<'a> {
            schema_version: u32,
            format: &'static str,
            report: &'a Report,
            datasets: Selected<'a>,
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
        let mut writer = std::io::BufWriter::new(BoundedHash {
            file: tmp.as_file_mut(),
            hash: Sha256::new(),
            bytes: 0,
        });
        serde_json::to_writer(
            &mut writer,
            &Package {
                schema_version: 1,
                format: "atom-reach-batch-v1",
                report: self,
                datasets: Selected(self),
            },
        )?;
        writer.flush()?;
        let writer = writer.into_inner().map_err(|e| Error::Io(e.into_error()))?;
        let receipt = Receipt {
            sha256: format!("{:x}", writer.hash.finalize()),
            bytes: writer.bytes,
            selected: self.selected,
        };
        tmp.as_file().sync_all()?;
        if path.exists() {
            if !std::fs::metadata(path)?.is_file() {
                return Err(Error::ExternalChange);
            }
            let mut old = std::fs::File::open(path)?.take(MAX_BYTES + 1);
            let mut hash = Sha256::new();
            let bytes = std::io::copy(&mut old, &mut HashSink(&mut hash))?;
            if bytes == receipt.bytes && format!("{:x}", hash.finalize()) == receipt.sha256 {
                return Ok(receipt);
            }
            return Err(Error::ExternalChange);
        }
        tmp.persist_noclobber(path)
            .map_err(|e| Error::Io(e.error))?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(receipt)
    }
}
struct HashSink<'a>(&'a mut Sha256);
impl Write for HashSink<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_limit_stops_before_writing_and_source_counts_are_bounded() {
        let mut file = tempfile::tempfile().unwrap();
        let mut writer = BoundedHash {
            file: &mut file,
            hash: Sha256::new(),
            bytes: MAX_BYTES,
        };
        assert!(writer.write_all(b"x").is_err());
        assert_eq!(file.metadata().unwrap().len(), 0);
        let mut request = Request::default();
        assert!(request.analyze().is_err());
        request.sources = vec![PathBuf::from("/not-read"); MAX_SOURCES + 1];
        assert!(request.analyze().is_err());
    }
}
