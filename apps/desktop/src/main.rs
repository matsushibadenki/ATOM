mod assets;
mod batch;
mod camera;
mod camera_input;
mod control;
mod joints;
mod layout;
mod reach;
mod runtime;
mod transform;
mod viewport;
mod window_placement;
mod workspace;
use atom_core::{AppCore, EditRequest, Project, ProjectStore, Snapshot};
use serde::Serialize;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Emitter, Manager};

struct Session {
    core: AppCore,
    store: ProjectStore,
    simulation_cancel: Option<Arc<AtomicBool>>,
    simulation_pause: Arc<AtomicBool>,
}
struct Shared(Arc<Mutex<Session>>);
#[derive(Debug, Serialize)]
struct UiError {
    code: String,
}
impl From<atom_core::Error> for UiError {
    fn from(error: atom_core::Error) -> Self {
        eprintln!("ATOM: {error}");
        Self {
            code: error.code().into(),
        }
    }
}
fn error(code: &str) -> UiError {
    UiError { code: code.into() }
}
fn authorize(webview: &tauri::Webview) -> Result<(), UiError> {
    if matches!(
        webview.label(),
        "toolbar" | "scene" | "workflow" | "viewport"
    ) {
        Ok(())
    } else {
        Err(error("forbidden"))
    }
}
#[tauri::command]
fn snapshot(webview: tauri::Webview, state: tauri::State<Shared>) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    Ok(state
        .0
        .lock()
        .map_err(|_| error("internal_error"))?
        .core
        .snapshot())
}
#[tauri::command]
async fn edit(webview: tauri::Webview, request: EditRequest) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    // IK is bounded CPU work; keep it off the native UI/event thread.
    let worker_app = app.clone();
    let (result, failure_snapshot) = tauri::async_runtime::spawn_blocking(move || {
        let path_edit = matches!(request.command, atom_core::Command::SolvePathIk { .. });
        let state = worker_app.state::<Shared>();
        let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
        let result = session.core.apply(request).map_err(UiError::from);
        let failure_snapshot = (path_edit && result.is_err()).then(|| session.core.snapshot());
        Ok::<_, UiError>((result, failure_snapshot))
    })
    .await
    .map_err(|_| error("internal_error"))??;
    if let Some(snapshot) = failure_snapshot {
        let _ = app.emit("atom://changed", &snapshot);
        let redraw = app.clone();
        let _ = app.run_on_main_thread(move || viewport::draw(&redraw));
    }
    let snapshot = result?;
    camera::reconcile(&app);
    let _ = app.emit("atom://changed", &snapshot);
    let redraw = app.clone();
    let _ = app.run_on_main_thread(move || viewport::draw(&redraw));
    Ok(snapshot)
}
#[tauri::command]
async fn save(webview: tauri::Webview) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Shared>();
        let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
        let Session { core, store, .. } = &mut *session;
        if core.has_preview() {
            return Err(error("edit_busy"));
        }
        store.save(core.project())?;
        core.mark_saved();
        let snapshot = core.snapshot();
        let _ = app.emit("atom://changed", &snapshot);
        Ok(snapshot)
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[tauri::command]
async fn reload(webview: tauri::Webview) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Shared>();
        let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
        if session.core.snapshot().dirty {
            return Err(error("unsaved_changes"));
        }
        let project = session
            .store
            .load()?
            .ok_or_else(|| error("missing_project"))?;
        session.core.reload(project)?;
        let snapshot = session.core.snapshot();
        let _ = app.emit("atom://changed", &snapshot);
        drop(session);
        camera::reconcile(&app);
        let redraw = app.clone();
        let _ = app.run_on_main_thread(move || viewport::draw(&redraw));
        Ok(snapshot)
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[tauri::command]
async fn validate_workflow(
    webview: tauri::Webview,
    expected_revision: u64,
) -> Result<atom_core::workflow::ValidationReport, UiError> {
    authorize(&webview)?;
    let document = {
        let state = webview.state::<Shared>();
        let session = state.0.lock().map_err(|_| error("internal_error"))?;
        if session.core.snapshot().revision != expected_revision {
            return Err(error("revision_conflict"));
        }
        session.core.project().workflow.clone()
    };
    Ok(atom_core::workflow::validate_tasks(&document).await?)
}
#[tauri::command]
fn physics_catalog(webview: tauri::Webview) -> Result<serde_json::Value, UiError> {
    authorize(&webview)?;
    use atom_core::{GravityPreset as G, MaterialPreset as M};
    let gravity = [G::Earth, G::Moon, G::Mars, G::Zero].map(|preset| atom_core::Gravity {
        preset,
        vector: preset.vector().unwrap(),
    });
    let materials = [
        M::Wood,
        M::Steel,
        M::Aluminum,
        M::Rubber,
        M::Plastic,
        M::Glass,
        M::Concrete,
        M::Ice,
    ]
    .map(atom_core::PhysicalMaterial::preset);
    Ok(serde_json::json!({"gravity":gravity,"materials":materials}))
}
#[tauri::command]
async fn simulate(
    webview: tauri::Webview,
    expected_revision: u64,
    project_id: atom_core::Uuid,
    steps: u32,
    continuous: Option<bool>,
) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    let continuous = continuous.unwrap_or(false);
    if !matches!(steps, 1 | 500 | 1000) {
        return Err(error("invalid_project"));
    }
    let app = webview.app_handle().clone();
    let (project, token, cancel, pause) = {
        let state = app.state::<Shared>();
        let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
        let token = if continuous {
            session
                .core
                .begin_continuous_simulation(expected_revision, project_id)?
        } else {
            session
                .core
                .begin_simulation(expected_revision, project_id)?
        };
        let _ = app.emit("atom://changed", session.core.snapshot());
        let cancel = Arc::new(AtomicBool::new(false));
        session.simulation_cancel = Some(cancel.clone());
        session.simulation_pause.store(false, Ordering::Relaxed);
        (
            session.core.project().clone(),
            token,
            cancel,
            session.simulation_pause.clone(),
        )
    };
    let signal = cancel.clone();
    let worker_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        if !continuous {
            return atom_core::mujoco::run_cancellable(&project, steps, &signal).map(Some);
        }
        atom_core::mujoco::run_continuous(&project, &signal, &pause, |frame, steps| {
            let (sequence, time) = {
                let shared = worker_app.state::<Shared>();
                let mut session = shared
                    .0
                    .lock()
                    .map_err(|_| atom_core::Error::PhysicsFailed)?;
                if signal.load(Ordering::Relaxed) || pause.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let time = frame.time;
                session
                    .core
                    .update_continuous_simulation(token, frame, steps)?;
                (session.core.sequence(), time)
            };
            let _ = worker_app.emit("atom://simulation-tick", (sequence, time));
            let redraw = worker_app.clone();
            let _ = worker_app.run_on_main_thread(move || viewport::draw(&redraw));
            Ok(())
        })
        .map(|()| None)
    })
    .await
    .unwrap_or(Err(atom_core::Error::PhysicsFailed));
    let (result, snapshot) = {
        let state = app.state::<Shared>();
        let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
        session.simulation_cancel = None;
        let cancelled = cancel.load(Ordering::Relaxed);
        let result = if continuous || cancelled {
            let snapshot = session.core.reset_simulation();
            if cancelled {
                Ok(snapshot)
            } else {
                result.map(|_| snapshot)
            }
        } else {
            session.core.finish_simulation(
                token,
                result.and_then(|frame| frame.ok_or(atom_core::Error::PhysicsFailed)),
                steps,
            )
        };
        (result, session.core.snapshot())
    };
    transform::publish(&app, &snapshot);
    let redraw = app.clone();
    let _ = app.run_on_main_thread(move || viewport::draw(&redraw));
    Ok(result?)
}
#[tauri::command]
fn pause_simulation(webview: tauri::Webview, paused: bool) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    let shared = webview.state::<Shared>();
    let mut session = shared.0.lock().map_err(|_| error("internal_error"))?;
    let snapshot = session.core.set_simulation_paused(paused)?;
    session.simulation_pause.store(paused, Ordering::Relaxed);
    drop(session);
    let _ = webview.app_handle().emit("atom://changed", &snapshot);
    Ok(snapshot)
}
#[tauri::command]
fn reset_simulation(webview: tauri::Webview) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    let state = webview.state::<Shared>();
    let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
    if let Some(cancel) = &session.simulation_cancel {
        cancel.store(true, Ordering::Relaxed);
        return Ok(session.core.snapshot());
    }
    let snapshot = session.core.reset_simulation();
    drop(session);
    let app = webview.app_handle().clone();
    transform::publish(&app, &snapshot);
    let redraw = app.clone();
    let _ = app.run_on_main_thread(move || viewport::draw(&redraw));
    Ok(snapshot)
}
fn main() {
    let result = tauri::Builder::default()
        .manage(assets::Jobs::default())
        .manage(control::Runtime::default())
        .manage(viewport::Preview::default())
        .manage(camera::State::default())
        .setup(|app| {
            let args: Vec<_> = std::env::args().skip(1).collect();
            let path = if args.len() == 2 && args[0] == "--project" {
                std::path::PathBuf::from(&args[1])
            } else if args.is_empty() {
                let dir = app.path().app_data_dir()?;
                std::fs::create_dir_all(&dir)?;
                dir.join("workspace.atom.json")
            } else {
                return Err("usage: atom-desktop [--project /path/to/project.atom.json]".into());
            };
            let mut store = ProjectStore::open(path)?;
            let project = match store.load()? {
                Some(p) => p,
                None => {
                    let p = Project::new("ATOM Lab")?;
                    store.save(&p)?;
                    p
                }
            };
            let control_path = store.path().to_owned();
            app.manage(Shared(Arc::new(Mutex::new(Session {
                core: AppCore::new(project)?,
                store,
                simulation_cancel: None,
                simulation_pause: Arc::new(AtomicBool::new(false)),
            }))));
            app.manage(batch::State::new(
                control_path.parent().unwrap().join("episodes"),
            ));
            control::start(app.handle(), control_path);
            let path = std::env::var_os("ATOM_LAYOUT_PATH")
                .map(std::path::PathBuf::from)
                .unwrap_or(app.path().app_config_dir()?.join("dock-layout.json"));
            app.manage(window_placement::Store::load(Some(
                path.with_extension("windows.json"),
            )));
            let runtime_path = std::env::var_os("ATOM_RUNTIME_CONFIG_PATH")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| path.with_extension("runtime.json"));
            app.manage(runtime::State::load(runtime_path));
            app.manage(workspace::Workspace::load(path));
            workspace::initialize(app.handle())?;
            #[cfg(target_os = "macos")]
            transform::install_history_menu(app.handle())?;
            app.wry_plugin(camera_input::Bridge {
                app: app.handle().clone(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            runtime::planning_scene,
            runtime::apply_plan,
            runtime::runtime_state,
            runtime::runtime_command,
            control::control_request,
            control::control_diagnose,
            reach::reach_command,
            reach::episode_files,
            reach::episode_dataset,
            batch::batch_state,
            batch::batch_command,
            batch::batch_cancel,
            batch::training_jobs,
            batch::campaign_files,
            batch::collection_files,
            batch::mining_files,
            batch::supervision_files,
            batch::dagger_files,
            batch::rollout_jobs,
            batch::rollout_episode,
            camera::camera_state,
            camera::camera_command,
            transform::transform_command,
            joints::joint_command,
            snapshot,
            assets::asset_catalog,
            assets::import_asset,
            assets::cancel_asset_import,
            physics_catalog,
            simulate,
            reset_simulation,
            pause_simulation,
            edit,
            save,
            reload,
            validate_workflow,
            workspace::workspace_state,
            workspace::workspace_command,
            workspace::split_resize,
            workspace::panel_drag
        ])
        .on_window_event(|window, event| {
            if matches!(
                event,
                tauri::WindowEvent::Resized(_)
                    | tauri::WindowEvent::ScaleFactorChanged { .. }
                    | tauri::WindowEvent::Focused(true)
            ) && window.try_state::<workspace::Workspace>().is_some()
            {
                window.state::<workspace::Workspace>().update_inset(window);
                workspace::resize(window.app_handle());
            }
            if matches!(
                event,
                tauri::WindowEvent::Moved(_)
                    | tauri::WindowEvent::Resized(_)
                    | tauri::WindowEvent::Focused(true)
                    | tauri::WindowEvent::ScaleFactorChanged { .. }
            ) && let Some(store) = window.try_state::<window_placement::Store>()
            {
                store.capture(window);
                window_placement::schedule_save(window.app_handle());
            }
            if let Some(host) = layout::Panel::ALL
                .into_iter()
                .find(|p| p.window() == window.label())
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let state = app.state::<workspace::Workspace>().snapshot();
                    let Some(group) = state.floating_groups.iter().find(|g| g.host == host) else {
                        return;
                    };
                    if let Err(e) = workspace::execute(
                        &app,
                        layout::Command::ReturnGroup {
                            panel: group.active,
                        },
                        state.revision,
                    ) {
                        let _ = app.emit("atom://panel-error", e.code);
                    }
                });
            }
            if window.label() == "main"
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                if window.state::<workspace::Workspace>().busy() {
                    api.prevent_close();
                    return;
                }
                let state = window.state::<Shared>();
                if state
                    .0
                    .lock()
                    .map(|s| s.core.snapshot().dirty || s.core.has_preview())
                    .unwrap_or(true)
                {
                    api.prevent_close();
                    let _ = window.emit("atom://close-blocked", ());
                } else {
                    window.app_handle().exit(0);
                }
            }
        })
        .build(tauri::generate_context!());
    match result {
        Ok(app) => app.run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                app.state::<window_placement::Store>().save();
            }
        }),
        Err(error) => {
            eprintln!("ATOM startup: {error}");
            std::process::exit(1);
        }
    }
}
