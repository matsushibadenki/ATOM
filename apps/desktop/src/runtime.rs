//! Rust-owned runtime connection state; GPU/document buffers never enter this gateway.
use super::{Shared, UiError, authorize, error};
use atom_runtime::{
    Config, Gateway, Profile,
    session::{Session, Snapshot},
};
use serde::Deserialize;
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::Mutex,
};
use tauri::{Emitter, Manager};
pub struct State {
    session: Mutex<Session>,
    path: PathBuf,
}
impl State {
    pub fn load(path: PathBuf) -> Self {
        let loaded = (|| -> Option<Config> {
            let mut b = vec![];
            std::fs::File::open(&path)
                .ok()?
                .take(4097)
                .read_to_end(&mut b)
                .ok()?;
            if b.len() > 4096 {
                return None;
            }
            serde_json::from_slice::<Config>(&b).ok()?.normalized().ok()
        })();
        let invalid = loaded.is_none() && path.exists();
        let config = loaded.unwrap_or(Config {
            profile: Profile::VllmApplePreview,
            origin: "http://127.0.0.1:8000".into(),
        });
        let mut session = Session::new(config).expect("validated runtime config");
        if invalid {
            session.settings_warning();
        }
        Self {
            session: Mutex::new(session),
            path,
        }
    }
}
fn ui(e: atom_runtime::Error) -> UiError {
    error(&e.code)
}
fn publish(app: &tauri::AppHandle, s: &Snapshot) {
    let _ = app.emit("atom://runtime", s);
}
#[tauri::command]
pub fn runtime_state(webview: tauri::Webview) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    Ok(webview
        .state::<State>()
        .session
        .lock()
        .map_err(|_| error("internal_error"))?
        .snapshot())
}
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Configure {
        config: Config,
    },
    Probe,
    Smoke {
        model: String,
        max_tokens: u32,
    },
    Plan {
        model: String,
        instruction: String,
        focus: Option<u32>,
        basis: atom_core::planning::Basis,
    },
    ValidatePlan {
        text: String,
        focus: Option<u32>,
        basis: atom_core::planning::Basis,
    },
}
#[tauri::command]
pub fn runtime_command(
    webview: tauri::Webview,
    expected_revision: u64,
    command: Command,
) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    let state = app.state::<State>();
    let mut session = state.session.lock().map_err(|_| error("internal_error"))?;
    if let Command::Configure { config } = command {
        let config = config.normalized().map_err(ui)?;
        // Check the shared revision before publishing settings or touching the file.
        let current = session.snapshot();
        if current.revision != expected_revision {
            return Err(error("runtime_conflict"));
        }
        if current.busy {
            return Err(error("runtime_busy"));
        }
        let parent = state.path.parent().ok_or_else(|| error("io_error"))?;
        std::fs::create_dir_all(parent).map_err(|_| error("io_error"))?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|_| error("io_error"))?;
        tmp.write_all(&serde_json::to_vec(&config).map_err(|_| error("internal_error"))?)
            .map_err(|_| error("io_error"))?;
        tmp.as_file().sync_all().map_err(|_| error("io_error"))?;
        tmp.persist(&state.path).map_err(|_| error("io_error"))?;
        session.configure(expected_revision, config).map_err(ui)?;
        let snapshot = session.snapshot();
        drop(session);
        publish(&app, &snapshot);
        return Ok(snapshot);
    }
    if let Command::Smoke {
        ref model,
        max_tokens,
    } = command
        && (model.is_empty()
            || model.len() > 256
            || !(1..=atom_runtime::MAX_TOKENS).contains(&max_tokens))
    {
        return Err(error("runtime_invalid_budget"));
    }
    match &command {
        Command::Plan {
            model, instruction, ..
        } if model.is_empty()
            || model.len() > 256
            || instruction.trim().is_empty()
            || instruction.len() > 512 =>
        {
            return Err(error("plan_instruction_budget"));
        }
        Command::ValidatePlan { text, .. } if text.len() > atom_core::planning::MAX_PLAN_BYTES => {
            return Err(error("plan_response_budget"));
        }
        _ => {}
    }
    let (job, config) = if matches!(
        &command,
        Command::Plan { .. } | Command::ValidatePlan { .. }
    ) {
        session.begin_planning(expected_revision)
    } else {
        session.begin(expected_revision)
    }
    .map_err(ui)?;
    let snapshot = session.snapshot();
    drop(session);
    publish(&app, &snapshot);
    std::thread::spawn(move || {
        let gateway = || Gateway::new(config, std::env::var("ATOM_RUNTIME_TOKEN").ok());
        let state = app.state::<State>();
        // Neither the runtime-state mutex nor the simulation-core mutex is held during HTTP.
        let update = match command {
            Command::Probe => {
                let result = gateway().map(|g| g.probe());
                let mut s = state.session.lock().unwrap();
                s.finish_probe(job, result);
                s.snapshot()
            }
            Command::Smoke { model, max_tokens } => {
                let result = gateway().and_then(|g| g.smoke(&model, max_tokens));
                let mut s = state.session.lock().unwrap();
                s.finish_smoke(job, result);
                s.snapshot()
            }
            Command::Plan {
                model,
                instruction,
                focus,
                basis,
            } => {
                let result = capture(&app, &basis, focus).and_then(|context| {
                    gateway().and_then(|g| g.generate_plan(&context, &instruction, &model))
                });
                finish_plan(&app, job, result)
            }
            Command::ValidatePlan { text, focus, basis } => {
                let result = capture(&app, &basis, focus)
                    .and_then(|context| atom_runtime::planning::Report::manual(&context, &text))
                    .and_then(|r| r.guard_size());
                finish_plan(&app, job, result)
            }
            Command::Configure { .. } => unreachable!(),
        };
        publish(&app, &update);
    });
    Ok(snapshot)
}

fn capture(
    app: &tauri::AppHandle,
    basis: &atom_core::planning::Basis,
    focus: Option<u32>,
) -> atom_runtime::Result<atom_core::planning::Context> {
    let state = app.state::<Shared>();
    let session = state
        .0
        .try_lock()
        .map_err(|_| atom_runtime::Error::code("plan_scene_busy"))?;
    session.core.planning_context_at(basis, focus).map_err(|e| {
        atom_runtime::Error::code(if matches!(e, atom_core::Error::Conflict(_)) {
            "plan_scene_stale"
        } else {
            e.code()
        })
    })
}
fn finish_plan(
    app: &tauri::AppHandle,
    job: u64,
    mut result: atom_runtime::Result<atom_runtime::planning::Report>,
) -> Snapshot {
    if let Ok(report) = &mut result {
        let shared = app.state::<Shared>();
        report.scene_current = shared
            .0
            .try_lock()
            .is_ok_and(|s| s.core.planning_matches(&report.scene.basis));
    }
    let state = app.state::<State>();
    let mut s = state.session.lock().unwrap();
    s.finish_plan(job, result);
    s.snapshot()
}
#[tauri::command]
pub async fn planning_scene(
    webview: tauri::Webview,
    focus: Option<u32>,
) -> Result<atom_core::planning::SceneSummary, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Shared>();
        let s = state.0.try_lock().map_err(|_| error("plan_scene_busy"))?;
        s.core.planning_summary(focus).map_err(UiError::from)
    })
    .await
    .map_err(|_| error("internal_error"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_restore_and_invalid_files_are_preserved_without_network() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runtime.json");
        let absent = State::load(path.clone());
        assert!(!path.exists());
        assert!(!absent.session.lock().unwrap().snapshot().busy);
        let config = Config {
            profile: Profile::Strata,
            origin: "http://localhost:12345/".into(),
        };
        std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
        let restored = State::load(path.clone());
        let s = restored.session.lock().unwrap().snapshot();
        assert_eq!(s.config.profile, Profile::Strata);
        assert_eq!(s.config.origin, "http://127.0.0.1:12345");
        assert!(s.capabilities.is_none() && s.completion.is_none());
        std::fs::write(&path, b"invalid-config").unwrap();
        let invalid = State::load(path.clone());
        assert_eq!(
            invalid
                .session
                .lock()
                .unwrap()
                .snapshot()
                .error
                .unwrap()
                .code,
            "runtime_invalid_config"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"invalid-config");
    }
}
