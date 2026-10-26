//! Auditable single-Episode transition export. No teacher reference goals in policy inputs.
use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportPolicy {
    SuccessOnly,
    Completed,
}
#[derive(Clone, Debug, Serialize)]
pub struct Quality {
    pub schema_version: u32,
    pub episode_id: Uuid,
    pub episode_sha256: String,
    /// All checkpoints/reruns of this model/seed task belong to one split group.
    pub split_group: String,
    pub frames: usize,
    pub transitions: usize,
    pub outcome: Outcome,
    pub complete: bool,
    pub success_exportable: bool,
    pub completed_exportable: bool,
    pub duration_seconds: f64,
    pub initial_distance_m: f64,
    pub final_distance_m: f64,
    pub min_distance_m: f64,
    pub max_abs_velocity_rad_s: f64,
    pub max_tracking_error_rad: f64,
    pub max_limit_excess_rad: f64,
    pub max_action_delta_rad: f64,
    pub reference_actions_only: bool,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PolicyObservation {
    /// All joint arrays follow metadata.channels, never raw qpos array order.
    pub joint_position_rad: Vec<f64>,
    pub joint_velocity_rad_s: Vec<f64>,
    pub endpoint_world_m: [f64; 3],
    pub target_world_m: [f32; 3],
    pub previous_action_rad: Vec<f64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Transition {
    pub index: usize,
    pub time_seconds: f64,
    pub next_time_seconds: f64,
    pub observation: PolicyObservation,
    /// Position-servo targets held during [time_seconds, next_time_seconds].
    pub action_rad: Vec<f64>,
    pub next_observation: PolicyObservation,
    pub next_distance_m: f64,
    pub terminated: bool,
    pub truncated: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Metadata {
    pub profile: String,
    pub seed: u32,
    pub horizon: u32,
    pub robot: u32,
    pub endpoint: u32,
    pub source_sha256: String,
    pub source_revision: Option<u64>,
    pub model_sha256: String,
    pub engine: String,
    pub python: String,
    pub platform: String,
    pub channels: Vec<Channel>,
    pub control_dt_seconds: f64,
    pub physics_dt_seconds: f64,
    pub substeps: u32,
    pub tolerance_m: f64,
    pub hold_steps: u32,
    pub world_up: String,
    pub action_semantics: String,
    pub controller_provenance: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Dataset {
    pub schema_version: u32,
    pub format: String,
    pub policy: ExportPolicy,
    pub quality: Quality,
    pub metadata: Metadata,
    pub transitions: Vec<Transition>,
}
impl Episode {
    /// Structural validation runs here; Episode::load additionally verifies file checksums.
    /// Valid records can still be partial or unsuitable as successful demonstrations.
    pub fn quality(&self) -> Result<Quality> {
        self.validate()?;
        let m = &self.manifest;
        let first = &self.frames[0];
        let last = self.frames.last().unwrap();
        let mut tracking: f64 = 0.;
        let mut excess: f64 = 0.;
        let mut delta: f64 = 0.;
        let mut velocity: f64 = 0.;
        let mut reference_only = true;
        for (i, f) in self.frames.iter().enumerate() {
            let action = f.action.as_ref().unwrap_or(&m.initial_action);
            let previous = i.checked_sub(1).map_or(&m.initial_action, |j| {
                self.frames[j].action.as_ref().unwrap_or(&m.initial_action)
            });
            if i > 0 && action != &m.reference_action {
                reference_only = false;
            }
            for (j, c) in m.channels.iter().enumerate() {
                let q = f.observation.qpos[c.qpos_index];
                tracking = tracking.max((q - action[j]).abs());
                velocity = velocity.max(f.observation.qvel[c.qpos_index].abs());
                excess = excess.max(
                    (f64::from(c.range[0]) - q)
                        .max(q - f64::from(c.range[1]))
                        .max(0.),
                );
                delta = delta.max((action[j] - previous[j]).abs());
            }
        }
        let complete = last.outcome != Outcome::Running;
        let has_transitions = self.frames.len() > 1;
        let mut warnings = vec![
            "no_collision_balance_or_task_generalization_check".into(),
            "split_by_group_not_by_transition".into(),
        ];
        if !complete {
            warnings.push("partial_episode".into());
        }
        if !has_transitions {
            warnings.push("no_transitions".into());
        }
        if last.outcome == Outcome::Timeout {
            warnings.push("timeout_not_success_demonstration".into());
        }
        if excess > 1e-6 {
            warnings.push("observed_joint_limit_excess".into());
        }
        if reference_only {
            warnings.push("reference_controller_not_learned_policy".into());
        } else {
            warnings.push("contains_custom_actions".into());
        }
        // Excludes Episode UUID, GUI revision, frame count and horizon: repeated seeds and
        // all checkpoints of a task must not straddle train/eval partitions.
        let split_group = digest(&serde_json::to_vec(&serde_json::json!({
            "version":1,"profile":m.profile,
            "model":m.model_sha256,"seed":m.config.seed,"robot":m.config.robot,
            "endpoint":m.endpoint,"target":m.target
        }))?);
        Ok(Quality {
            schema_version: 1,
            episode_id: self.id,
            episode_sha256: digest(&serde_json::to_vec(self)?),
            split_group,
            frames: self.frames.len(),
            transitions: self.frames.len() - 1,
            outcome: last.outcome,
            complete,
            success_exportable: has_transitions && last.outcome == Outcome::Success,
            completed_exportable: has_transitions && complete,
            duration_seconds: last.observation.time - first.observation.time,
            initial_distance_m: first.distance,
            final_distance_m: last.distance,
            min_distance_m: self
                .frames
                .iter()
                .map(|f| f.distance)
                .fold(f64::INFINITY, f64::min),
            max_abs_velocity_rad_s: velocity,
            max_tracking_error_rad: tracking,
            max_limit_excess_rad: excess,
            max_action_delta_rad: delta,
            reference_actions_only: reference_only,
            warnings,
        })
    }
    pub fn dataset(&self, policy: ExportPolicy) -> Result<Dataset> {
        let quality = self.quality()?;
        if !match policy {
            ExportPolicy::SuccessOnly => quality.success_exportable,
            ExportPolicy::Completed => quality.completed_exportable,
        } {
            return Err(Error::EpisodeExportIneligible);
        }
        let m = &self.manifest;
        let obs = |f: &Frame| PolicyObservation {
            joint_position_rad: m
                .channels
                .iter()
                .map(|c| f.observation.qpos[c.qpos_index])
                .collect(),
            joint_velocity_rad_s: m
                .channels
                .iter()
                .map(|c| f.observation.qvel[c.qpos_index])
                .collect(),
            endpoint_world_m: f.observation.endpoint,
            target_world_m: m.target,
            previous_action_rad: f.action.as_ref().unwrap_or(&m.initial_action).clone(),
        };
        let transitions = self
            .frames
            .windows(2)
            .enumerate()
            .map(|(index, pair)| Transition {
                index,
                time_seconds: pair[0].observation.time,
                next_time_seconds: pair[1].observation.time,
                observation: obs(&pair[0]),
                action_rad: pair[1].action.clone().unwrap(),
                next_observation: obs(&pair[1]),
                next_distance_m: pair[1].distance,
                terminated: pair[1].outcome == Outcome::Success,
                truncated: pair[1].outcome == Outcome::Timeout,
            })
            .collect();
        let o = &self.frames[0].observation;
        Ok(Dataset {
            schema_version: 1,
            format: "atom-reach-transitions-v1".into(),
            policy,
            quality,
            metadata: Metadata {
                profile: m.profile.clone(),
                seed: m.config.seed,
                horizon: m.config.horizon,
                robot: m.config.robot,
                endpoint: m.endpoint,
                source_sha256: m.source_sha256.clone(),
                source_revision: m.source_revision,
                model_sha256: m.model_sha256.clone(),
                engine: o.engine.clone(),
                python: o.python.clone(),
                platform: o.platform.clone(),
                channels: m.channels.clone(),
                control_dt_seconds: DT,
                physics_dt_seconds: m.physics_dt,
                substeps: m.substeps,
                tolerance_m: m.tolerance,
                hold_steps: m.hold_steps,
                world_up: "Y".into(),
                action_semantics: "absolute_joint_position_servo_target_rad".into(),
                controller_provenance: if self
                    .frames
                    .iter()
                    .skip(1)
                    .all(|f| f.action.as_ref() == Some(&m.reference_action))
                {
                    "reference_goal_servo"
                } else {
                    "recorded_actions_unknown_policy"
                }
                .into(),
            },
            transitions,
        })
    }
}
impl Dataset {
    /// Atomic no-clobber publication; identical repeated export is idempotent.
    pub fn save(&self, path: &Path) -> Result<()> {
        let value = serde_json::to_value(self)?;
        let data = serde_json::to_vec(&value)?;
        let envelope = serde_json::json!({"sha256":digest(&data),"dataset":value});
        let bytes = serde_json::to_vec(&envelope)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(Error::Invalid("Dataset size".into()));
        }
        if path.exists() {
            let mut old = vec![];
            std::fs::File::open(path)?
                .take(MAX_BYTES + 1)
                .read_to_end(&mut old)?;
            if old == bytes {
                return Ok(());
            }
            return Err(Error::ExternalChange);
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
        tmp.write_all(&bytes)?;
        tmp.as_file().sync_all()?;
        tmp.persist_noclobber(path)
            .map_err(|e| Error::Io(e.error))?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub project_id: Uuid,
    pub expected_revision: u64,
    pub expected_sequence: u64,
    pub episode_id: Uuid,
}
#[derive(Clone, Debug, Serialize)]
pub struct Analysis {
    pub basis: Request,
    pub quality: Quality,
    /// Filename only; the GUI never receives transition arrays or absolute paths.
    pub exported: Option<String>,
}
