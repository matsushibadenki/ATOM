//! One Rust-owned batch result for all docked/floating WebViews. Disk work runs off-thread.
use super::{UiError, authorize, error};
use atom_core::reach::{
    self,
    dataset::{
        ExportPolicy,
        batch::{Limits, Report, Request, SplitConfig},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub policy: ExportPolicy,
    pub split: SplitConfig,
    pub limits: Limits,
}
impl Default for Settings {
    fn default() -> Self {
        let r = Request::default();
        Self {
            policy: r.policy,
            split: r.split,
            limits: r.limits,
        }
    }
}
#[derive(Clone, Serialize)]
pub struct Snapshot {
    revision: u64,
    busy: bool,
    settings: Settings,
    files: Vec<String>,
    report: Option<serde_json::Value>,
    exported: Option<String>,
    exported_sha256: Option<String>,
    error: Option<String>,
    learning: Option<LearningView>,
    training: Option<TrainingView>,
    training_running: bool,
    rollout_running: bool,
    campaign_running: bool,
    collection_running: bool,
    collection_progress: Option<atom_core::learning::collection::Progress>,
    collection_config: Option<atom_core::learning::collection::Config>,
    collection: Option<atom_core::learning::collection::Saved>,
    campaign_progress: Option<atom_core::learning::campaign::Progress>,
    campaign_config: Option<atom_core::learning::campaign::Config>,
    campaign: Option<atom_core::learning::campaign::Saved>,
    rollout_partition: Option<atom_core::learning::rollout::Partition>,
    diagnostics_running: bool,
    cancel_requested: bool,
    rollout: Option<TrainingView>,
    registry: atom_core::learning::registry::Listing,
    selected_policy: Option<String>,
    comparison: Option<atom_core::learning::registry::Comparison>,
    diagnostics: Option<atom_core::learning::diagnostics::Saved>,
}
#[derive(Clone, Serialize)]
struct LearningView {
    valid: bool,
    summary: Option<atom_core::learning::Summary>,
    diagnostic: Option<atom_core::learning::Diagnostic>,
    code: Option<String>,
}
type JobOutput = (
    Option<Report>,
    Option<(String, String)>,
    Option<LearningView>,
    Option<TrainingView>,
    Option<TrainingView>,
    Option<RegistryUpdate>,
);
struct RegistryUpdate {
    listing: atom_core::learning::registry::Listing,
    selected: Option<atom_core::learning::registry::PolicyView>,
    context: Option<(TrainingView, Settings)>,
    comparison: Option<atom_core::learning::registry::Comparison>,
    diagnostics: Option<atom_core::learning::diagnostics::Saved>,
    campaign: Option<atom_core::learning::campaign::Saved>,
    collection: Option<atom_core::learning::collection::Saved>,
    dataset_settings: Option<Settings>,
}
#[derive(Clone, Serialize)]
struct TrainingView {
    directory: String,
    result: serde_json::Value,
}
struct Session {
    snapshot: Snapshot,
    report: Option<Arc<Report>>,
    last_job: Option<PathBuf>,
}
pub struct State {
    session: Mutex<Session>,
    dir: PathBuf,
    cancel: Mutex<Option<Arc<std::sync::atomic::AtomicBool>>>,
}
impl State {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            cancel: Mutex::new(None),
            session: Mutex::new(Session {
                snapshot: Snapshot {
                    revision: 0,
                    busy: false,
                    settings: Settings::default(),
                    files: vec![],
                    report: None,
                    exported: None,
                    exported_sha256: None,
                    error: None,
                    learning: None,
                    training: None,
                    training_running: false,
                    rollout_running: false,
                    collection_running: false,
                    collection_progress: None,
                    collection_config: None,
                    collection: None,
                    campaign_running: false,
                    campaign_progress: None,
                    campaign_config: None,
                    campaign: None,
                    rollout_partition: None,
                    diagnostics_running: false,
                    cancel_requested: false,
                    rollout: None,
                    registry: Default::default(),
                    selected_policy: None,
                    comparison: None,
                    diagnostics: None,
                },
                report: None,
                last_job: None,
            }),
        }
    }
}
fn notify(app: &tauri::AppHandle, s: &Snapshot) {
    let _ = app.emit("atom://batch", s);
}
#[tauri::command]
pub fn batch_state(webview: tauri::Webview) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    Ok(webview
        .state::<State>()
        .session
        .lock()
        .map_err(|_| error("internal_error"))?
        .snapshot
        .clone())
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Analyze {
        settings: Settings,
        files: Vec<String>,
    },
    Export,
    ValidateLearning,
    DiagnoseLearning,
    Train {
        config: atom_core::learning::training::Config,
        resume: bool,
    },
    Evaluate,
    RegistryRefresh,
    Campaign {
        config: atom_core::learning::campaign::Config,
    },
    Collect {
        config: atom_core::learning::collection::Config,
    },
    LoadCollection {
        name: String,
    },
    UseCollection {
        name: String,
    },
    LoadCampaign {
        name: String,
    },
    DiagnoseHistory {
        evaluation: String,
    },
    RegisterPolicy,
    SelectPolicy {
        id: String,
    },
    CompareHistory {
        a: String,
        b: String,
    },
    Rollout {
        #[serde(default)]
        partition: atom_core::learning::rollout::Partition,
    },
    LoadRollout {
        name: String,
    },
    LoadJob {
        name: String,
    },
}
fn request(
    dir: &std::path::Path,
    settings: Settings,
    files: &[String],
) -> Result<Request, UiError> {
    if files.is_empty() || files.len() > 64 || files.iter().any(|n| !reach::valid_name(n)) {
        return Err(error("batch_invalid_sources"));
    }
    let mut unique = files.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != files.len() {
        return Err(error("batch_invalid_sources"));
    }
    for name in files {
        if std::fs::symlink_metadata(dir.join(name)).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(error("batch_invalid_sources"));
        }
    }
    Ok(Request {
        schema_version: 1,
        policy: settings.policy,
        split: settings.split,
        limits: settings.limits,
        sources: files.iter().map(|n| dir.join(n)).collect(),
    })
}
fn display(report: &Report) -> serde_json::Value {
    let mut value = serde_json::to_value(report).expect("finite validated report");
    // The WebView receives summaries and local filenames, never paths or transitions.
    for entry in value["entries"].as_array_mut().unwrap() {
        entry["source"] = serde_json::json!(
            std::path::Path::new(entry["source"].as_str().unwrap())
                .file_name()
                .unwrap()
                .to_string_lossy()
        );
    }
    value["request"].as_object_mut().unwrap().remove("sources");
    value
}
impl Session {
    fn begin(
        &mut self,
        expected_revision: u64,
        command: &Command,
    ) -> Result<(Snapshot, Option<Arc<Report>>), UiError> {
        if self.snapshot.busy {
            return Err(error("batch_busy"));
        }
        if self.snapshot.revision != expected_revision {
            return Err(error("batch_stale"));
        }
        if matches!(
            command,
            Command::Collect { .. }
                | Command::ValidateLearning
                | Command::DiagnoseLearning
                | Command::Train { .. }
                | Command::Evaluate
                | Command::Rollout { .. }
                | Command::RegisterPolicy
                | Command::LoadJob { .. }
        ) && self.snapshot.exported.is_none()
        {
            return Err(error("learning_export_first"));
        }
        if matches!(
            command,
            Command::Train { resume: true, .. }
                | Command::Evaluate
                | Command::Rollout { .. }
                | Command::RegisterPolicy
        ) && self.last_job.is_none()
        {
            return Err(error("learning_checkpoint_missing"));
        }
        if let Command::Train { config, .. } = command {
            config.validate().map_err(|e| error(&e.code))?;
        }
        if let Command::Campaign { config } = command {
            config.validate().map_err(|e| error(&e.code))?;
            self.snapshot.campaign_config = Some(config.clone());
            self.snapshot.campaign = None;
            self.snapshot.campaign_progress = None;
        }
        if let Command::Collect { config } = command {
            config.validate().map_err(|e| error(&e.code))?;
            self.snapshot.collection_config = Some(config.clone());
            self.snapshot.collection = None;
            self.snapshot.collection_progress = None;
        }
        self.snapshot.collection_running = matches!(command, Command::Collect { .. });
        self.snapshot.campaign_running = matches!(command, Command::Campaign { .. });
        self.snapshot.diagnostics_running = matches!(command, Command::DiagnoseHistory { .. });
        self.snapshot.rollout_running = matches!(command, Command::Rollout { .. });
        if let Command::Rollout { partition } = command {
            self.snapshot.rollout_partition = Some(*partition);
        }
        self.snapshot.cancel_requested = false;
        self.snapshot.training_running =
            matches!(command, Command::Train { .. } | Command::Evaluate);
        let report = match &command {
            Command::Export => Some(
                self.report
                    .clone()
                    .filter(|r| r.selected() > 0)
                    .ok_or_else(|| error("episode_export_ineligible"))?,
            ),
            _ => None,
        };
        if let Command::Analyze { settings, files } = &command {
            // Validate cheap shape before entering a job; all filesystem access stays off-thread.
            if files.is_empty()
                || files.len() > 64
                || files.iter().any(|n| !reach::valid_name(n))
                || files
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != files.len()
            {
                return Err(error("batch_invalid_sources"));
            }
            self.snapshot.settings = settings.clone();
            self.snapshot.files = files.clone();
            self.snapshot.report = None;
            self.report = None;
            self.snapshot.exported = None;
            self.snapshot.exported_sha256 = None;
            self.snapshot.learning = None;
            self.snapshot.training = None;
            self.snapshot.rollout = None;
            self.snapshot.selected_policy = None;
            self.last_job = None;
        }
        if matches!(command, Command::Rollout { .. }) {
            self.snapshot.rollout = None;
        }
        if matches!(command, Command::CompareHistory { .. }) {
            self.snapshot.comparison = None;
        }
        if matches!(command, Command::DiagnoseHistory { .. }) {
            self.snapshot.diagnostics = None;
        }
        self.snapshot.busy = true;
        self.snapshot.error = None;
        self.snapshot.revision += 1;
        Ok((self.snapshot.clone(), report))
    }
}

#[tauri::command]
pub fn batch_command(
    webview: tauri::Webview,
    expected_revision: u64,
    command: Command,
) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    let state = app.state::<State>();
    let mut session = state.session.lock().map_err(|_| error("internal_error"))?;
    let project = if matches!(
        &command,
        Command::Rollout { .. } | Command::Campaign { .. } | Command::Collect { .. }
    ) {
        Some(
            app.state::<super::Shared>()
                .0
                .try_lock()
                .map_err(|_| error("edit_busy"))?
                .core
                .project()
                .clone(),
        )
    } else {
        None
    };
    let (snapshot, report) = session.begin(expected_revision, &command)?;
    let token = snapshot.revision;
    let last_job = session.last_job.clone();
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    if snapshot.training_running
        || snapshot.rollout_running
        || snapshot.campaign_running
        || snapshot.collection_running
    {
        *state.cancel.lock().map_err(|_| error("internal_error"))? = Some(cancel.clone());
    }
    let dir = state.dir.clone();
    let previous = snapshot.exported.clone();
    let expected_hash = snapshot.exported_sha256.clone();
    drop(session);
    notify(&app, &snapshot);
    let is_export = matches!(&command, Command::Export);
    let is_diagnose = matches!(&command, Command::DiagnoseLearning);
    let is_evaluate = matches!(&command, Command::Evaluate);
    let is_use_collection = matches!(&command, Command::UseCollection { .. });
    tauri::async_runtime::spawn_blocking(move || {
        let result = (|| -> Result<JobOutput, UiError> {
            match command {
                Command::Analyze { settings, files } => Ok((
                    Some(
                        request(&dir, settings, &files)?
                            .analyze()
                            .map_err(UiError::from)?,
                    ),
                    None,
                    None,
                    None,
                    None,
                    None,
                )),
                Command::ValidateLearning | Command::DiagnoseLearning => {
                    let path = dir.join("exports").join(previous.unwrap());
                    if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                        return Err(error("batch_invalid_sources"));
                    }
                    let view = match atom_core::learning::Validated::load(&path) {
                        Err(e) => LearningView {
                            valid: false,
                            summary: None,
                            diagnostic: None,
                            code: Some(e.code),
                        },
                        Ok(data)
                            if expected_hash.as_deref()
                                != Some(data.summary().source_sha256.as_str()) =>
                        {
                            LearningView {
                                valid: false,
                                summary: None,
                                diagnostic: None,
                                code: Some("learning_source_changed".into()),
                            }
                        }
                        Ok(data) => {
                            let summary = Some(data.summary());
                            if is_diagnose {
                                match atom_core::learning::diagnose(&data) {
                                    Ok(d) => LearningView {
                                        valid: true,
                                        summary,
                                        code: Some(d.code.clone()),
                                        diagnostic: Some(d),
                                    },
                                    Err(e) => LearningView {
                                        valid: true,
                                        summary,
                                        diagnostic: None,
                                        code: Some(e.code),
                                    },
                                }
                            } else {
                                LearningView {
                                    valid: true,
                                    summary,
                                    diagnostic: None,
                                    code: None,
                                }
                            }
                        }
                    };
                    Ok((None, None, Some(view), None, None, None))
                }
                Command::LoadJob { name } => {
                    if !atom_core::learning::training::valid_name(&name) {
                        return Err(error("learning_checkpoint_invalid"));
                    }
                    let job =
                        atom_core::learning::training::inspect(&dir.join("training").join(&name))
                            .map_err(|e| error(&e.code))?;
                    if job.result["source_sha256"].as_str() != expected_hash.as_deref() {
                        return Err(error("learning_checkpoint_data_mismatch"));
                    }
                    let mut result = job.result;
                    result.as_object_mut().unwrap().remove("history");
                    Ok((
                        None,
                        None,
                        None,
                        Some(TrainingView {
                            directory: name,
                            result,
                        }),
                        None,
                        None,
                    ))
                }
                Command::Train { config, resume } => run_training(
                    &dir,
                    previous.as_deref().unwrap(),
                    expected_hash.as_deref().unwrap(),
                    &config,
                    if resume { last_job.as_deref() } else { None },
                    false,
                    &cancel,
                    token,
                ),
                Command::Evaluate => run_training(
                    &dir,
                    previous.as_deref().unwrap(),
                    expected_hash.as_deref().unwrap(),
                    &atom_core::learning::training::Config::default(),
                    last_job.as_deref(),
                    is_evaluate,
                    &cancel,
                    token,
                ),
                Command::Rollout { partition } => {
                    let path = dir.join("exports").join(previous.unwrap());
                    if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                        return Err(error("batch_invalid_sources"));
                    }
                    let data =
                        atom_core::learning::Validated::load(&path).map_err(|e| error(&e.code))?;
                    if Some(data.summary().source_sha256.as_str()) != expected_hash.as_deref() {
                        return Err(error("learning_source_changed"));
                    }
                    let parent = dir.join("rollouts");
                    std::fs::create_dir_all(&parent).map_err(|_| error("io_error"))?;
                    let name = format!(
                        "rollout-{}-{token}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos()
                    );
                    let job_name = last_job
                        .as_ref()
                        .unwrap()
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap();
                    let policy = atom_core::learning::registry::register(
                        &dir,
                        job_name,
                        path.file_name().unwrap().to_str().unwrap(),
                    )
                    .map_err(|e| error(&e.code))?;
                    let result = atom_core::learning::rollout::compare_partition(
                        &data,
                        last_job.as_deref().unwrap(),
                        &project.unwrap(),
                        &parent.join(&name),
                        &cancel,
                        partition,
                    )
                    .map_err(|e| error(&e.code))?;
                    atom_core::learning::registry::attach(&dir, &policy.id, &name)
                        .map_err(|e| error(&e.code))?;
                    let listing =
                        atom_core::learning::registry::listing(&dir).map_err(|e| error(&e.code))?;
                    Ok((
                        None,
                        None,
                        None,
                        None,
                        Some(TrainingView {
                            directory: name,
                            result,
                        }),
                        Some(RegistryUpdate {
                            listing,
                            selected: Some(policy),
                            context: None,
                            comparison: None,
                            diagnostics: None,
                            campaign: None,
                            collection: None,
                            dataset_settings: None,
                        }),
                    ))
                }
                Command::LoadRollout { name } => {
                    if !atom_core::learning::rollout::valid_name(&name) {
                        return Err(error("learning_rollout_invalid"));
                    }
                    let result =
                        atom_core::learning::rollout::inspect(&dir.join("rollouts").join(&name))
                            .map_err(|e| error(&e.code))?;
                    Ok((
                        None,
                        None,
                        None,
                        None,
                        Some(TrainingView {
                            directory: name,
                            result,
                        }),
                        None,
                    ))
                }
                Command::Collect { config } => {
                    let source = previous.as_deref().unwrap();
                    let saved = atom_core::learning::collection::run_checked(
                        &dir,
                        source,
                        expected_hash.as_deref(),
                        &project.unwrap(),
                        &config,
                        &cancel,
                        |progress| {
                            let state = app.state::<State>();
                            if let Ok(mut session) = state.session.lock()
                                && session.snapshot.revision == token
                                && session.snapshot.collection_running
                            {
                                session.snapshot.collection_progress = Some(progress);
                                notify(&app, &session.snapshot);
                            }
                        },
                    )
                    .map_err(|e| error(&e.code))?;
                    Ok((
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(RegistryUpdate {
                            listing: atom_core::learning::registry::listing(&dir)
                                .map_err(|e| error(&e.code))?,
                            selected: None,
                            context: None,
                            comparison: None,
                            diagnostics: None,
                            campaign: None,
                            collection: Some(saved),
                            dataset_settings: None,
                        }),
                    ))
                }
                Command::LoadCollection { name } | Command::UseCollection { name } => {
                    let saved = atom_core::learning::collection::inspect(&dir, &name)
                        .map_err(|e| error(&e.code))?;
                    let using = is_use_collection;
                    let mut receipt = None;
                    let mut settings = None;
                    if using {
                        let name = saved
                            .report
                            .exported
                            .as_ref()
                            .ok_or_else(|| error("learning_collection_empty"))?;
                        let data =
                            atom_core::learning::Validated::load(&dir.join("exports").join(name))
                                .map_err(|e| error(&e.code))?;
                        let request = data.curation_request();
                        settings = Some(Settings {
                            policy: request.policy,
                            split: request.split,
                            limits: request.limits,
                        });
                        receipt = Some((name.clone(), data.summary().source_sha256));
                    }
                    Ok((
                        None,
                        receipt,
                        None,
                        None,
                        None,
                        Some(RegistryUpdate {
                            listing: atom_core::learning::registry::listing(&dir)
                                .map_err(|e| error(&e.code))?,
                            selected: None,
                            context: None,
                            comparison: None,
                            diagnostics: None,
                            campaign: None,
                            collection: Some(saved),
                            dataset_settings: settings,
                        }),
                    ))
                }
                Command::Campaign { config } => {
                    let saved = atom_core::learning::campaign::run(
                        &dir,
                        &project.unwrap(),
                        &config,
                        &cancel,
                        |progress| {
                            let state = app.state::<State>();
                            if let Ok(mut session) = state.session.lock()
                                && session.snapshot.revision == token
                                && session.snapshot.campaign_running
                            {
                                session.snapshot.campaign_progress = Some(progress);
                                notify(&app, &session.snapshot);
                            }
                        },
                    )
                    .map_err(|e| error(&e.code))?;
                    let listing =
                        atom_core::learning::registry::listing(&dir).map_err(|e| error(&e.code))?;
                    Ok((
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(RegistryUpdate {
                            listing,
                            selected: None,
                            context: None,
                            comparison: None,
                            diagnostics: None,
                            campaign: Some(saved),
                            collection: None,
                            dataset_settings: None,
                        }),
                    ))
                }
                Command::LoadCampaign { name } => {
                    let saved = atom_core::learning::campaign::inspect(&dir, &name)
                        .map_err(|e| error(&e.code))?;
                    let listing =
                        atom_core::learning::registry::listing(&dir).map_err(|e| error(&e.code))?;
                    Ok((
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(RegistryUpdate {
                            listing,
                            selected: None,
                            context: None,
                            comparison: None,
                            diagnostics: None,
                            campaign: Some(saved),
                            collection: None,
                            dataset_settings: None,
                        }),
                    ))
                }
                Command::RegistryRefresh
                | Command::RegisterPolicy
                | Command::SelectPolicy { .. }
                | Command::CompareHistory { .. }
                | Command::DiagnoseHistory { .. } => {
                    use atom_core::learning::{registry, training};
                    let mut selected = None;
                    let mut context = None;
                    let mut comparison = None;
                    let mut diagnostics = None;
                    match command {
                        Command::RegisterPolicy => {
                            let job = last_job
                                .as_ref()
                                .unwrap()
                                .file_name()
                                .unwrap()
                                .to_str()
                                .unwrap();
                            let policy =
                                registry::register(&dir, job, previous.as_deref().unwrap())
                                    .map_err(|e| error(&e.code))?;
                            // Backfill compatible pre-registry comparisons without claiming unrelated runs.
                            for name in atom_core::learning::rollout::catalog(&dir.join("rollouts"))
                                .map_err(|e| error(&e.code))?
                            {
                                if let Ok(report) = atom_core::learning::rollout::summary(
                                    &dir.join("rollouts").join(&name),
                                ) && report["checkpoint_sha256"]
                                    == policy.record.checkpoint_sha256
                                    && report["source_sha256"] == policy.record.source_sha256
                                {
                                    registry::attach(&dir, &policy.id, &name)
                                        .map_err(|e| error(&e.code))?;
                                }
                            }
                            selected = Some(policy);
                        }
                        Command::SelectPolicy { id } => {
                            let record = registry::select(&dir, &id).map_err(|e| error(&e.code))?;
                            let job = training::inspect(&dir.join("training").join(&record.job))
                                .map_err(|e| error(&e.code))?;
                            if job.manifest_sha256 != record.job_manifest_sha256 {
                                return Err(error("learning_registry_changed"));
                            }
                            let mut result = job.result;
                            result.as_object_mut().unwrap().remove("history");
                            let data = atom_core::learning::Validated::load(
                                &dir.join("exports").join(&record.batch),
                            )
                            .map_err(|e| error(&e.code))?;
                            if data.summary().source_sha256 != record.source_sha256 {
                                return Err(error("learning_registry_changed"));
                            }
                            let request = data.curation_request();
                            context = Some((
                                TrainingView {
                                    directory: record.job.clone(),
                                    result,
                                },
                                Settings {
                                    policy: request.policy,
                                    split: request.split,
                                    limits: request.limits,
                                },
                            ));
                            selected = Some(registry::PolicyView { id, record });
                        }
                        Command::DiagnoseHistory { evaluation } => {
                            let report =
                                atom_core::learning::diagnostics::analyze(&dir, &evaluation)
                                    .map_err(|e| error(&e.code))?;
                            diagnostics = Some(
                                atom_core::learning::diagnostics::save(&dir, report)
                                    .map_err(|e| error(&e.code))?,
                            );
                        }
                        Command::CompareHistory { a, b } => {
                            comparison =
                                Some(registry::compare(&dir, &a, &b).map_err(|e| error(&e.code))?);
                        }
                        _ => {}
                    }
                    let listing = registry::listing(&dir).map_err(|e| error(&e.code))?;
                    Ok((
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(RegistryUpdate {
                            listing,
                            selected,
                            context,
                            comparison,
                            diagnostics,
                            campaign: None,
                            collection: None,
                            dataset_settings: None,
                        }),
                    ))
                }
                Command::Export => {
                    let report = report.unwrap();
                    // Reject symlink replacements as well as the core content-hash check.
                    for entry in report.entries().iter().filter(|e| e.selected) {
                        if std::fs::symlink_metadata(&entry.source)
                            .is_ok_and(|m| m.file_type().is_symlink())
                        {
                            return Err(error("batch_invalid_sources"));
                        }
                    }
                    let out = dir.join("exports");
                    std::fs::create_dir_all(&out).map_err(|_| error("io_error"))?;
                    let name = previous.unwrap_or_else(|| {
                        format!(
                            "batch-{}-{token}.batch.json",
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos()
                        )
                    });
                    let receipt = report
                        .save_package(&out.join(&name))
                        .map_err(|_| error("batch_export_failed"))?;
                    Ok((None, Some((name, receipt.sha256)), None, None, None, None))
                }
            }
        })();
        let state = app.state::<State>();
        let Ok(mut session) = state.session.lock() else {
            return;
        };
        if session.snapshot.revision != token {
            return;
        }
        session.snapshot.busy = false;
        session.snapshot.training_running = false;
        session.snapshot.rollout_running = false;
        session.snapshot.campaign_running = false;
        session.snapshot.collection_running = false;
        session.snapshot.diagnostics_running = false;
        *state.cancel.lock().unwrap() = None;
        session.snapshot.revision += 1;
        match result {
            Ok((report, name, learning, training, rollout, registry)) => {
                if let Some(update) = registry {
                    session.snapshot.registry = update.listing;
                    if let Some(saved) = update.collection {
                        session.snapshot.collection = Some(saved);
                    }
                    if let Some(settings) = update.dataset_settings {
                        session.snapshot.settings = settings;
                        session.snapshot.files.clear();
                        session.snapshot.report = None;
                        session.report = None;
                    }
                    if let Some(policy) = update.selected {
                        session.snapshot.selected_policy = Some(policy.id);
                        if let Some((context, settings)) = update.context {
                            session.snapshot.settings = settings;
                            session.snapshot.files.clear();
                            session.report = None;
                            session.snapshot.report = None;
                            session.snapshot.exported = Some(policy.record.batch);
                            session.snapshot.exported_sha256 = Some(policy.record.source_sha256);
                            session.snapshot.learning = None;
                            session.snapshot.rollout = None;
                            session.last_job = Some(dir.join("training").join(&context.directory));
                            session.snapshot.training = Some(context);
                        }
                    }
                    if let Some(campaign) = update.campaign {
                        session.snapshot.campaign = Some(campaign);
                    }
                    if let Some(diagnostics) = update.diagnostics {
                        session.snapshot.diagnostics = Some(diagnostics);
                    }
                    if let Some(comparison) = update.comparison {
                        session.snapshot.comparison = Some(comparison);
                    }
                }
                if let Some(view) = rollout {
                    session.snapshot.rollout = Some(view);
                }
                if let Some(view) = training {
                    session.snapshot.selected_policy = None;
                    session.snapshot.rollout = None;
                    if view.result["kind"] == "training" {
                        session.last_job = Some(dir.join("training").join(&view.directory));
                    }
                    session.snapshot.training = Some(view);
                }
                if learning.is_some() {
                    session.snapshot.learning = learning;
                }
                if let Some(report) = report {
                    session.snapshot.report = Some(display(&report));
                    session.report = Some(Arc::new(report));
                }
                if let Some((name, hash)) = name {
                    session.snapshot.selected_policy = None;
                    session.snapshot.rollout = None;
                    session.snapshot.exported = Some(name);
                    session.snapshot.exported_sha256 = Some(hash);
                    session.snapshot.learning = None;
                    session.snapshot.training = None;
                    session.last_job = None;
                }
            }
            Err(e) => {
                session.snapshot.error = Some(e.code);
                if is_export {
                    session.report = None;
                    session.snapshot.report = None;
                    session.snapshot.exported = None;
                    session.snapshot.exported_sha256 = None;
                    session.snapshot.learning = None;
                }
            }
        }
        let snapshot = session.snapshot.clone();
        drop(session);
        notify(&app, &snapshot);
    });
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collection_requires_a_source_but_no_checkpoint_and_uses_the_shared_job_slot() {
        let state = State::new(PathBuf::from("/not-read"));
        let mut session = state.session.lock().unwrap();
        let command = Command::Collect {
            config: atom_core::learning::collection::Config::default(),
        };
        assert_eq!(
            session.begin(0, &command).err().unwrap().code,
            "learning_export_first"
        );
        session.snapshot.exported = Some("fixture.batch.json".into());
        session.snapshot.exported_sha256 = Some("a".repeat(64));
        let (snapshot, _) = session.begin(0, &command).unwrap();
        assert!(snapshot.busy && snapshot.collection_running && !snapshot.training_running);
        assert!(session.last_job.is_none());
        assert_eq!(
            session
                .begin(snapshot.revision, &Command::RegistryRefresh)
                .err()
                .unwrap()
                .code,
            "batch_busy"
        );
        let restored: Command =
            serde_json::from_str(r#"{"op":"load_collection","name":"collection.json"}"#).unwrap();
        session.snapshot.busy = false;
        assert!(session.begin(snapshot.revision, &restored).is_ok());
    }
    #[test]
    fn shared_batch_revisions_gate_competing_windows_and_unanalyzed_export() {
        let state = State::new(PathBuf::from("/not-read"));
        let mut session = state.session.lock().unwrap();
        let command = Command::Analyze {
            settings: Settings::default(),
            files: vec!["reach-a.episode.json".into()],
        };
        assert!(session.begin(0, &Command::Export).is_err());
        assert!(session.begin(0, &Command::ValidateLearning).is_err());
        assert!(session.begin(0, &Command::DiagnoseLearning).is_err());
        let (started, report) = session.begin(0, &command).unwrap();
        assert!(started.busy && report.is_none());
        assert_eq!(session.begin(1, &command).err().unwrap().code, "batch_busy");
        session.snapshot.busy = false;
        session.snapshot.revision += 1;
        assert_eq!(
            session.begin(0, &command).err().unwrap().code,
            "batch_stale"
        );
        assert!(session.begin(2, &command).is_ok());
    }
    #[test]
    fn rollout_is_shared_cancellable_and_saved_results_need_no_active_batch() {
        let state = State::new(PathBuf::from("/not-read"));
        let mut s = state.session.lock().unwrap();
        assert_eq!(
            s.begin(
                0,
                &Command::Rollout {
                    partition: Default::default()
                }
            )
            .err()
            .unwrap()
            .code,
            "learning_export_first"
        );
        s.snapshot.exported = Some("batch.json".into());
        assert_eq!(
            s.begin(
                0,
                &Command::Rollout {
                    partition: Default::default()
                }
            )
            .err()
            .unwrap()
            .code,
            "learning_checkpoint_missing"
        );
        s.last_job = Some(PathBuf::from("bc-test"));
        let (snapshot, _) = s
            .begin(
                0,
                &Command::Rollout {
                    partition: Default::default(),
                },
            )
            .unwrap();
        assert!(snapshot.busy && snapshot.rollout_running && !snapshot.training_running);
        assert_eq!(
            s.begin(
                1,
                &Command::Rollout {
                    partition: Default::default()
                }
            )
            .err()
            .unwrap()
            .code,
            "batch_busy"
        );
        s.snapshot.busy = false;
        s.snapshot.rollout_running = false;
        s.snapshot.exported = None;
        s.snapshot.revision += 1;
        assert!(
            s.begin(
                2,
                &Command::LoadRollout {
                    name: "rollout-test".into()
                }
            )
            .is_ok()
        );
    }
    #[test]
    fn registry_selection_and_history_work_without_an_export_and_keep_revision_guards() {
        let state = State::new(PathBuf::from("/not-read"));
        let mut session = state.session.lock().unwrap();
        assert_eq!(
            session
                .begin(0, &Command::RegisterPolicy)
                .err()
                .unwrap()
                .code,
            "learning_export_first"
        );
        let (snapshot, _) = session
            .begin(
                0,
                &Command::SelectPolicy {
                    id: "policy-id".into(),
                },
            )
            .unwrap();
        assert!(snapshot.busy && !snapshot.rollout_running && !snapshot.training_running);
        assert_eq!(
            session
                .begin(1, &Command::RegistryRefresh)
                .err()
                .unwrap()
                .code,
            "batch_busy"
        );
        session.snapshot.busy = false;
        session.snapshot.revision += 1;
        assert_eq!(
            session
                .begin(
                    1,
                    &Command::CompareHistory {
                        a: "a".into(),
                        b: "b".into()
                    }
                )
                .err()
                .unwrap()
                .code,
            "batch_stale"
        );
        assert!(
            session
                .begin(
                    2,
                    &Command::CompareHistory {
                        a: "a".into(),
                        b: "b".into()
                    }
                )
                .is_ok()
        );
    }
    #[test]
    fn diagnosis_is_shared_read_only_job_with_revision_and_busy_guards() {
        let state = State::new(PathBuf::from("/not-read"));
        let mut session = state.session.lock().unwrap();
        let command = Command::DiagnoseHistory {
            evaluation: "evaluation-id".into(),
        };
        let (snapshot, _) = session.begin(0, &command).unwrap();
        assert!(snapshot.diagnostics_running && snapshot.busy);
        assert!(!snapshot.training_running && !snapshot.rollout_running);
        assert_eq!(session.begin(1, &command).err().unwrap().code, "batch_busy");
        session.snapshot.busy = false;
        session.snapshot.revision += 1;
        assert_eq!(
            session.begin(1, &command).err().unwrap().code,
            "batch_stale"
        );
    }
    #[test]
    fn validation_start_carries_partition_and_legacy_start_remains_test() {
        use atom_core::learning::rollout::Partition;
        let old: Command = serde_json::from_str(r#"{"op":"rollout"}"#).unwrap();
        assert!(matches!(
            old,
            Command::Rollout {
                partition: Partition::Test
            }
        ));
        assert!(
            serde_json::from_str::<Command>(r#"{"op":"rollout","partition":"train"}"#).is_err()
        );
        let state = State::new(PathBuf::from("/not-read"));
        let mut session = state.session.lock().unwrap();
        session.snapshot.exported = Some("batch.json".into());
        session.last_job = Some(PathBuf::from("bc-job"));
        let (s, _) = session
            .begin(
                0,
                &Command::Rollout {
                    partition: Partition::Validation,
                },
            )
            .unwrap();
        assert!(s.busy && s.rollout_running);
        assert_eq!(s.rollout_partition, Some(Partition::Validation));
        assert_eq!(
            session.begin(s.revision, &old).err().unwrap().code,
            "batch_busy"
        );
    }
    #[test]
    fn campaign_is_cancellable_without_an_active_export_or_training_selection() {
        let state = State::new(PathBuf::from("/not-read"));
        let mut session = state.session.lock().unwrap();
        let config = atom_core::learning::campaign::Config {
            policies: vec![
                format!("policy-{}.json", "a".repeat(64)),
                format!("policy-{}.json", "b".repeat(64)),
            ],
            max_seconds: 60,
        };
        let (snapshot, _) = session
            .begin(
                0,
                &Command::Campaign {
                    config: config.clone(),
                },
            )
            .unwrap();
        assert!(snapshot.campaign_running && snapshot.busy);
        assert!(!snapshot.training_running && !snapshot.rollout_running);
        assert_eq!(snapshot.campaign_config.unwrap().policies, config.policies);
        assert_eq!(
            session
                .begin(snapshot.revision, &Command::Campaign { config })
                .err()
                .unwrap()
                .code,
            "batch_busy"
        );
    }
    #[test]
    fn source_scope_and_ipc_summary_exclude_paths_and_transition_arrays() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            request(
                dir.path(),
                Settings::default(),
                &["../escape.episode.json".into()]
            )
            .is_err()
        );
        let name = "reach-broken.episode.json".to_owned();
        std::fs::write(dir.path().join(&name), b"{}").unwrap();
        assert!(
            request(
                dir.path(),
                Settings::default(),
                &[name.clone(), name.clone()]
            )
            .is_err()
        );
        let report = request(dir.path(), Settings::default(), std::slice::from_ref(&name))
            .unwrap()
            .analyze()
            .unwrap();
        let value = display(&report);
        assert_eq!(value["entries"][0]["source"], name);
        let text = value.to_string();
        assert!(!text.contains(dir.path().to_str().unwrap()));
        assert!(value["request"].get("sources").is_none());
        assert!(value["entries"][0].get("transitions").is_none());
        #[cfg(unix)]
        {
            let link = "reach-link.episode.json".to_owned();
            std::os::unix::fs::symlink(dir.path().join(name), dir.path().join(&link)).unwrap();
            assert!(request(dir.path(), Settings::default(), &[link]).is_err());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_training(
    dir: &std::path::Path,
    filename: &str,
    hash: &str,
    config: &atom_core::learning::training::Config,
    resume: Option<&std::path::Path>,
    evaluate: bool,
    cancel: &std::sync::atomic::AtomicBool,
    token: u64,
) -> Result<JobOutput, UiError> {
    let source = dir.join("exports").join(filename);
    if std::fs::symlink_metadata(&source).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(error("batch_invalid_sources"));
    }
    let data = atom_core::learning::Validated::load(&source).map_err(|e| error(&e.code))?;
    if data.summary().source_sha256 != hash {
        return Err(error("learning_source_changed"));
    }
    let parent = dir.join("training");
    std::fs::create_dir_all(&parent).map_err(|_| error("io_error"))?;
    let name = format!(
        "bc-{}-{token}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let job = if evaluate {
        atom_core::learning::training::evaluate(&data, resume.unwrap(), &parent.join(&name), cancel)
    } else {
        atom_core::learning::training::train(&data, config, &parent.join(&name), resume, cancel)
    }
    .map_err(|e| error(&e.code))?;
    let mut result = job.result;
    result.as_object_mut().unwrap().remove("history");
    Ok((
        None,
        None,
        None,
        Some(TrainingView {
            directory: name,
            result,
        }),
        None,
        None,
    ))
}
#[tauri::command]
pub fn batch_cancel(webview: tauri::Webview, expected_revision: u64) -> Result<(), UiError> {
    authorize(&webview)?;
    let state = webview.state::<State>();
    let mut session = state.session.lock().map_err(|_| error("internal_error"))?;
    if session.snapshot.revision != expected_revision {
        return Err(error("batch_stale"));
    }
    if !session.snapshot.training_running
        && !session.snapshot.rollout_running
        && !session.snapshot.campaign_running
        && !session.snapshot.collection_running
    {
        return Err(error("learning_job_not_running"));
    }
    if let Some(cancel) = state
        .cancel
        .lock()
        .map_err(|_| error("internal_error"))?
        .as_ref()
    {
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    session.snapshot.cancel_requested = true;
    notify(webview.app_handle(), &session.snapshot);
    Ok(())
}

#[tauri::command]
pub async fn training_jobs(webview: tauri::Webview) -> Result<Vec<String>, UiError> {
    authorize(&webview)?;
    let dir = webview.state::<State>().dir.join("training");
    tauri::async_runtime::spawn_blocking(move || {
        atom_core::learning::training::catalog(&dir).map_err(|e| error(&e.code))
    })
    .await
    .map_err(|_| error("internal_error"))?
}

#[tauri::command]
pub async fn rollout_jobs(webview: tauri::Webview) -> Result<Vec<String>, UiError> {
    authorize(&webview)?;
    let dir = webview.state::<State>().dir.join("rollouts");
    tauri::async_runtime::spawn_blocking(move || {
        atom_core::learning::rollout::catalog(&dir).map_err(|e| error(&e.code))
    })
    .await
    .map_err(|_| error("internal_error"))?
}

/// Materialize one verified comparison Episode into the existing Reach player's scope.
/// Only a local filename crosses IPC; the native renderer reads the Rust-owned Episode.
#[tauri::command]
pub async fn rollout_episode(
    webview: tauri::Webview,
    expected_revision: u64,
    directory: String,
    index: usize,
) -> Result<String, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    let dir = {
        let state = app.state::<State>();
        let session = state.session.lock().map_err(|_| error("internal_error"))?;
        if session.snapshot.revision != expected_revision
            || session.snapshot.busy
            || session
                .snapshot
                .rollout
                .as_ref()
                .map(|r| r.directory.as_str())
                != Some(directory.as_str())
            || !atom_core::learning::rollout::valid_name(&directory)
        {
            return Err(error("batch_stale"));
        }
        state.dir.clone()
    };
    tauri::async_runtime::spawn_blocking(move || {
        let episode =
            atom_core::learning::rollout::episode(&dir.join("rollouts").join(directory), index)
                .map_err(|e| error(&e.code))?;
        let name = format!("reach-{}.episode.json", episode.id);
        episode.save(&dir.join(&name)).map_err(UiError::from)?;
        let state = app.state::<State>();
        let session = state.session.lock().map_err(|_| error("internal_error"))?;
        if session.snapshot.revision != expected_revision || session.snapshot.busy {
            return Err(error("batch_stale"));
        }
        Ok(name)
    })
    .await
    .map_err(|_| error("internal_error"))?
}

#[tauri::command]
pub async fn campaign_files(webview: tauri::Webview) -> Result<Vec<String>, UiError> {
    authorize(&webview)?;
    let root = webview.state::<State>().dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        atom_core::learning::campaign::catalog(&root).map_err(|e| error(&e.code))
    })
    .await
    .map_err(|_| error("internal_error"))?
}

#[tauri::command]
pub async fn collection_files(webview: tauri::Webview) -> Result<Vec<String>, UiError> {
    authorize(&webview)?;
    let root = webview.state::<State>().dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        atom_core::learning::collection::catalog(&root).map_err(|e| error(&e.code))
    })
    .await
    .map_err(|_| error("internal_error"))?
}
