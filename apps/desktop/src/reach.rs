//! GUI requests and replay scheduling; the core owns all Episode/physics state.
use super::{Shared, UiError, authorize, error, transform, viewport};
use tauri::Manager;
fn directory(app: &tauri::AppHandle) -> Result<std::path::PathBuf, UiError> {
    let state = app.state::<Shared>();
    let session = state.0.lock().map_err(|_| error("internal_error"))?;
    Ok(session
        .store
        .path()
        .parent()
        .ok_or_else(|| error("io_error"))?
        .join("episodes"))
}
#[tauri::command]
pub async fn reach_command(
    webview: tauri::Webview,
    request: atom_core::reach::Request,
) -> Result<atom_core::Snapshot, UiError> {
    authorize(&webview)?;
    let focus = matches!(
        &request.command,
        atom_core::reach::Command::Reset { .. } | atom_core::reach::Command::Load { .. }
    );
    let start_playback = matches!(&request.command, atom_core::reach::Command::Play { .. });
    let app = webview.app_handle().clone();
    let dir = directory(&app)?;
    let worker_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = worker_app.state::<Shared>();
        let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
        let result = session
            .core
            .reach_command(request, &dir)
            .map_err(UiError::from);
        let snapshot = session.core.snapshot();
        let focus_target = if focus && result.is_ok() {
            snapshot
                .reach
                .as_ref()
                .and_then(|r| crate::camera::focus_target(&session.core, r.robot))
        } else {
            None
        };
        drop(session);
        if let Some(target) = focus_target {
            let state = worker_app.state::<crate::camera::State>();
            if let Ok(mut camera) = state.0.lock() {
                camera.selected = Some(target.id);
                camera.tool = crate::transform::Tool::Camera;
                camera.pan_mode = false;
                camera.focus(&target, viewport::aspect(&worker_app));
                camera.revision += 1;
            }
            crate::camera::notify(&worker_app);
        } else {
            crate::camera::reconcile(&worker_app);
        }
        transform::publish(&worker_app, &snapshot);
        let redraw = worker_app.clone();
        let _ = worker_app.run_on_main_thread(move || viewport::draw(&redraw));
        result
    })
    .await
    .map_err(|_| error("internal_error"))?;
    if start_playback && result.is_ok() {
        let token = app
            .state::<Shared>()
            .0
            .lock()
            .map_err(|_| error("internal_error"))?
            .core
            .reach_play_token();
        if let Some((id, token)) = token {
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    let snapshot = {
                        let state = app.state::<Shared>();
                        let Ok(mut session) = state.0.lock() else {
                            break;
                        };
                        session.core.reach_play_tick(id, token)
                    };
                    match snapshot {
                        Ok(Some(s)) => {
                            let playing = s.reach.as_ref().is_some_and(|r| r.playing);
                            transform::publish(&app, &s);
                            let redraw = app.clone();
                            let _ = app.run_on_main_thread(move || viewport::draw(&redraw));
                            if !playing {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
            });
        }
    }
    result
}
#[tauri::command]
pub async fn episode_files(webview: tauri::Webview) -> Result<Vec<String>, UiError> {
    authorize(&webview)?;
    let dir = directory(webview.app_handle())?;
    tauri::async_runtime::spawn_blocking(move || atom_core::reach::catalog(&dir))
        .await
        .map_err(|_| error("internal_error"))?
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "Requires Metal GPU and MuJoCo Python; verifies recorded replay scene rendering"]
    fn native_gpu_recorded_reach_seek_restores_identical_frames() {
        tauri::async_runtime::block_on(async {
            let project: atom_core::Project =
                serde_json::from_str(include_str!("../../../templates/dual-arm.atom.json"))
                    .unwrap();
            let mut runner = atom_core::reach::Runner::reset(
                &project,
                atom_core::reach::Config {
                    seed: 7,
                    robot: project.robots[0].root,
                    horizon: 300,
                },
            )
            .unwrap();
            runner.run().unwrap();
            let dir = tempfile::tempdir().unwrap();
            runner
                .episode
                .save(&dir.path().join("reach-gpu.episode.json"))
                .unwrap();
            let last = runner.episode.frames.len() - 1;
            let id = runner.episode.id;
            drop(runner);
            let mut core = atom_core::AppCore::new(project).unwrap();
            let command = |core: &mut atom_core::AppCore, command| {
                let s = core.snapshot();
                core.reach_command(
                    atom_core::reach::Request {
                        project_id: s.project_id,
                        expected_revision: s.revision,
                        expected_sequence: s.sequence,
                        command,
                    },
                    dir.path(),
                )
                .unwrap()
            };
            command(
                &mut core,
                atom_core::reach::Command::Load {
                    name: "reach-gpu.episode.json".into(),
                },
            );
            let adapter = wgpu::Instance::default()
                .request_adapter(&Default::default())
                .await
                .unwrap();
            let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
            device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut renderer =
                threex_renderer::Renderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
            let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Reach recorded replay"),
                size: wgpu::Extent3d {
                    width: 512,
                    height: 512,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let target = texture.create_view(&Default::default());
            let view = crate::camera::View {
                target: [0., 0.8, 0.],
                distance: 3.5,
                pitch: 0.3,
                ..Default::default()
            };
            let capture = |renderer: &threex_renderer::Renderer| {
                let b = renderer.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 512 * 2048,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                let mut e = renderer.device.create_command_encoder(&Default::default());
                e.copy_texture_to_buffer(
                    texture.as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &b,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(2048),
                            rows_per_image: None,
                        },
                    },
                    wgpu::Extent3d {
                        width: 512,
                        height: 512,
                        depth_or_array_layers: 1,
                    },
                );
                renderer.queue.submit([e.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                b.slice(..)
                    .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
                renderer
                    .device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .unwrap();
                rx.recv().unwrap().unwrap();
                b.slice(..).get_mapped_range().to_vec()
            };
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            let first = capture(&renderer);
            command(
                &mut core,
                atom_core::reach::Command::Seek { id, index: last },
            );
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            assert_ne!(first, capture(&renderer));
            command(&mut core, atom_core::reach::Command::Seek { id, index: 0 });
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            assert_eq!(first, capture(&renderer));
            assert_eq!(core.snapshot().undo_depth, 0);
            assert!(!core.snapshot().dirty);
            assert!(renderer.device.pop_error_scope().await.is_none());
        });
    }
}

#[tauri::command]
pub async fn episode_dataset(
    webview: tauri::Webview,
    request: atom_core::reach::dataset::Request,
    policy: Option<atom_core::reach::dataset::ExportPolicy>,
) -> Result<atom_core::reach::dataset::Analysis, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (episode, dir) = {
            let state = app.state::<Shared>();
            let session = state.0.try_lock().map_err(|_| error("edit_busy"))?;
            let episode = session
                .core
                .recorded_episode(&request)
                .map_err(UiError::from)?;
            let dir = session
                .store
                .path()
                .parent()
                .ok_or_else(|| error("io_error"))?
                .join("episodes")
                .join("exports");
            (episode, dir)
        };
        let (quality, exported) = if let Some(policy) = policy {
            let dataset = episode.dataset(policy).map_err(UiError::from)?;
            let label = match policy {
                atom_core::reach::dataset::ExportPolicy::SuccessOnly => "success_only",
                atom_core::reach::dataset::ExportPolicy::Completed => "completed",
            };
            let name = format!(
                "reach-{}-{}-{label}.transitions.json",
                episode.id, dataset.quality.transitions
            );
            std::fs::create_dir_all(&dir).map_err(|_| error("io_error"))?;
            dataset.save(&dir.join(&name)).map_err(UiError::from)?;
            (dataset.quality, Some(name))
        } else {
            (episode.quality().map_err(UiError::from)?, None)
        };
        Ok(atom_core::reach::dataset::Analysis {
            basis: request,
            quality,
            exported,
        })
    })
    .await
    .map_err(|_| error("internal_error"))?
}
