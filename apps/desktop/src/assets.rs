//! Asset operations exchange only bounded requests and summaries with the WebView.
use super::{Shared, UiError, authorize, error};
use atom_core::{
    Snapshot,
    assets::{Catalog, ImportSource},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Emitter, Manager};
#[derive(Default)]
pub struct Jobs(Mutex<Option<Arc<AtomicBool>>>);
#[tauri::command]
pub async fn asset_catalog(
    webview: tauri::Webview,
    repo: String,
    revision: String,
) -> Result<Catalog, UiError> {
    authorize(&webview)?;
    tauri::async_runtime::spawn_blocking(move || atom_core::assets::catalog(&repo, &revision))
        .await
        .map_err(|_| error("internal_error"))?
        .map_err(Into::into)
}
#[tauri::command]
pub fn cancel_asset_import(webview: tauri::Webview) -> Result<(), UiError> {
    authorize(&webview)?;
    if let Some(flag) = webview
        .state::<Jobs>()
        .0
        .lock()
        .map_err(|_| error("internal_error"))?
        .as_ref()
    {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}
#[tauri::command]
pub async fn import_asset(
    webview: tauri::Webview,
    source: ImportSource,
    expected_revision: u64,
    project_id: atom_core::Uuid,
) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    let flag = Arc::new(AtomicBool::new(false));
    let (cache, token) = {
        let state = app.state::<Shared>();
        let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
        let cache = session
            .store
            .path()
            .parent()
            .ok_or_else(|| error("io_error"))?
            .join(".asset-cache");
        let token = session.core.begin_import(expected_revision, project_id)?;
        *app.state::<Jobs>()
            .0
            .lock()
            .map_err(|_| error("internal_error"))? = Some(flag.clone());
        let _ = app.emit("atom://changed", session.core.snapshot());
        (cache, token)
    };
    let command = tauri::async_runtime::spawn_blocking(move || {
        atom_core::assets::prepare_cancellable(source, &cache, Some(&flag))
    })
    .await
    .unwrap_or_else(|_| Err(atom_core::Error::Asset("usd_failed".into())));
    *app.state::<Jobs>()
        .0
        .lock()
        .map_err(|_| error("internal_error"))? = None;
    let (result, snapshot) = {
        let state = app.state::<Shared>();
        let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
        let result = session.core.finish_import(token, command);
        (result, session.core.snapshot())
    };
    super::camera::reconcile(&app);
    let _ = app.emit("atom://changed", &snapshot);
    let redraw = app.clone();
    let _ = app.run_on_main_thread(move || super::viewport::draw(&redraw));
    Ok(result?)
}
