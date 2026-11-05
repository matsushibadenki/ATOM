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
);
struct Session {
    snapshot: Snapshot,
    report: Option<Arc<Report>>,
}
pub struct State {
    session: Mutex<Session>,
    dir: PathBuf,
}
impl State {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
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
                },
                report: None,
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
            Command::ValidateLearning | Command::DiagnoseLearning
        ) && self.snapshot.exported.is_none()
        {
            return Err(error("learning_export_first"));
        }
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
    let (snapshot, report) = session.begin(expected_revision, &command)?;
    let token = snapshot.revision;
    let dir = state.dir.clone();
    let previous = snapshot.exported.clone();
    let expected_hash = snapshot.exported_sha256.clone();
    drop(session);
    notify(&app, &snapshot);
    let is_export = matches!(&command, Command::Export);
    let is_diagnose = matches!(&command, Command::DiagnoseLearning);
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
                    Ok((None, None, Some(view)))
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
                    Ok((None, Some((name, receipt.sha256)), None))
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
        session.snapshot.revision += 1;
        match result {
            Ok((report, name, learning)) => {
                if learning.is_some() {
                    session.snapshot.learning = learning;
                }
                if let Some(report) = report {
                    session.snapshot.report = Some(display(&report));
                    session.report = Some(Arc::new(report));
                }
                if let Some((name, hash)) = name {
                    session.snapshot.exported = Some(name);
                    session.snapshot.exported_sha256 = Some(hash);
                    session.snapshot.learning = None;
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
