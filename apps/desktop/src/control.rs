//! IPC handler and WebView entrypoints all read the exact same Shared mutex.
use super::{Shared, UiError, authorize, error};
use atom_control::{
    Request, Response, Service,
    ipc::{Client, Diagnostic, Server},
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::Manager;
#[derive(Default)]
pub struct Runtime {
    server: Mutex<Option<Server>>,
    service: Arc<Mutex<Service>>,
    project: Mutex<Option<PathBuf>>,
    start_error: Mutex<Option<String>>,
}
pub fn start(app: &tauri::AppHandle, path: PathBuf) {
    let runtime = app.state::<Runtime>();
    *runtime.project.lock().expect("control project") = Some(path.clone());
    let shared = app.state::<Shared>().0.clone();
    let service = runtime.service.clone();
    match Server::start(&path, move |request| dispatch(&shared, &service, request)) {
        Ok(server) => *runtime.server.lock().expect("control server") = Some(server),
        Err(e) => {
            eprintln!("ATOM control: {}", e.code);
            *runtime.start_error.lock().expect("control startup") = Some(e.code);
        }
    }
}
fn dispatch(
    shared: &Arc<Mutex<super::Session>>,
    service: &Mutex<Service>,
    request: Request,
) -> Response {
    let id = request.request_id;
    let session = match shared.try_lock() {
        Ok(s) => s,
        Err(std::sync::TryLockError::WouldBlock) => return Response::error(id, "busy"),
        Err(_) => return Response::error(id, "internal_error"),
    };
    match service.lock() {
        Ok(mut service) => service.handle(&session.core, request),
        Err(_) => Response::error(id, "internal_error"),
    }
}
#[tauri::command]
pub async fn control_request(
    webview: tauri::Webview,
    request: Request,
) -> Result<Response, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let shared = app.state::<Shared>();
        let runtime = app.state::<Runtime>();
        dispatch(&shared.0, &runtime.service, request)
    })
    .await
    .map_err(|_| error("internal_error"))
}
#[tauri::command]
pub async fn control_diagnose(webview: tauri::Webview) -> Result<Diagnostic, UiError> {
    authorize(&webview)?;
    let state = webview.state::<Runtime>();
    let project = state
        .project
        .lock()
        .map_err(|_| error("internal_error"))?
        .clone()
        .ok_or_else(|| error("not_running"))?;
    let startup = state
        .start_error
        .lock()
        .map_err(|_| error("internal_error"))?
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut report = Client::new(project).diagnose();
        if let Some(code) = startup {
            report.connected = false;
            report.checks.insert(
                0,
                atom_control::ipc::Check {
                    stage: "server_start".into(),
                    ok: false,
                    code,
                },
            );
        }
        report
    })
    .await
    .map_err(|_| error("internal_error"))
}
