//! One Renderer/Device/Scene; native surfaces switch without copying GPU resources to JS.
use super::{Shared, layout::Panel, workspace::Workspace};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tauri::{Emitter, Manager};
use threex_renderer::native::{AcquireFailure, SurfaceBackend, WgpuSurface};

#[derive(Default)]
pub struct Preview(Mutex<Option<Target>>);
struct Target {
    backend: WgpuSurface,
    attached: String,
    parked: BTreeMap<String, wgpu::Surface<'static>>,
    size: [u32; 2],
}
impl Target {
    fn activate(&mut self, label: &str, size: [u32; 2]) -> Result<(), String> {
        let switched = self.attached != label;
        if switched {
            let surface = self.parked.remove(label).ok_or("missing GPU surface")?;
            match self.backend.exchange_surface(surface) {
                Ok(previous) => {
                    self.parked.insert(
                        std::mem::replace(&mut self.attached, label.into()),
                        previous,
                    );
                }
                Err(surface) => {
                    self.parked.insert(label.into(), surface);
                    return Err("incompatible GPU surface".into());
                }
            }
        }
        if switched || self.size != size {
            self.backend
                .configure(size[0], size[1])
                .map_err(|e| format!("{e:?}"))?;
            self.size = size;
        }
        Ok(())
    }
}
pub fn initialize(
    app: &tauri::AppHandle,
    home: tauri::Window,
    floating: Vec<tauri::Window>,
) -> Result<(), String> {
    let instance = wgpu::Instance::default();
    let main_surface = instance
        .create_surface(Arc::new(home))
        .map_err(|e| e.to_string())?;
    let mut parked = BTreeMap::new();
    for window in floating {
        let label = window.label().to_owned();
        parked.insert(
            label,
            instance
                .create_surface(Arc::new(window))
                .map_err(|e| e.to_string())?,
        );
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match WgpuSurface::prepare(&instance, main_surface).await {
            Ok(backend) => {
                *app.state::<Preview>().0.lock().expect("GPU state") = Some(Target {
                    backend,
                    attached: "main".into(),
                    parked,
                    size: [0, 0],
                });
                let target = app.clone();
                let _ = app.run_on_main_thread(move || draw(&target));
            }
            Err(e) => {
                eprintln!("ATOM GPU prepare: {e:?}");
                let _ = app.emit("atom://viewport", "gpu_unavailable");
            }
        }
    });
    Ok(())
}
pub fn draw(app: &tauri::AppHandle) {
    let workspace = app.state::<Workspace>();
    let layout = workspace.effective();
    let gpu = app.state::<Preview>();
    let mut gpu = gpu.0.lock().expect("GPU state");
    let Some(target) = gpu.as_mut() else {
        return;
    };
    let result = (|| -> Result<&str, String> {
        let mut status = "ready";
        for label in ["main", "float-scene", "float-workflow", "float-viewport"] {
            if label != "main"
                && !layout
                    .floating_groups
                    .iter()
                    .any(|g| g.host.window() == label)
            {
                continue;
            }
            let Some(window) = app.get_window(label) else {
                continue;
            };
            let dimensions = workspace.size(&window).map_err(|e| e.to_string())?;
            let size = [dimensions.width, dimensions.height];
            if size.contains(&0) || window.is_minimized().unwrap_or(false) {
                continue;
            }
            let scale = window.scale_factor().unwrap_or(1.);
            let region = if label == "main" {
                layout
                    .geometry(size, scale, workspace.inset(label))
                    .groups
                    .iter()
                    .find(|g| g.active == Panel::Viewport)
                    .map(|g| g.content(scale))
            } else {
                let top = (workspace.inset(label) + (96. * scale).round() as u32).min(size[1]);
                layout
                    .floating_groups
                    .iter()
                    .any(|g| g.host.window() == label && g.active == Panel::Viewport)
                    .then_some([0, top, size[0], size[1] - top])
            }
            .filter(|r| r[2] > 0 && r[3] > 0);
            target.activate(label, size)?;
            let view = app
                .state::<super::camera::State>()
                .0
                .lock()
                .expect("camera")
                .clone();
            let camera = region.map(|r| view.matrix(r[2] as f32 / r[3] as f32));
            if let Some(region) = region {
                target.backend.set_region(region);
            }
            let shared = app.state::<Shared>();
            let session = shared.0.lock().map_err(|e| e.to_string())?;
            target.backend.set_imported_meshes(
                session.core.project().objects.iter().filter_map(|o| {
                    let asset = session
                        .core
                        .project()
                        .assets
                        .iter()
                        .find(|a| Some(a.id) == o.mesh_ref)?;
                    Some(threex_renderer::imported::MeshView {
                        instance: session.core.scene().instance_index(o.id)? as u32,
                        key: &asset.mesh_hash,
                        positions: &asset.mesh.positions,
                        colors: &asset.mesh.colors,
                        indices: &asset.mesh.indices,
                    })
                }),
            )?;
            let selected_index = view
                .selected
                .and_then(|id| session.core.scene().instance_index(id));
            let selected = selected_index.map(|index| {
                let model = session.core.scene().instances()[index].model;
                std::array::from_fn(|i| model[i / 4][i % 4])
            });
            target
                .backend
                .set_guides(Some(threex_renderer::guides::GuideView {
                    selected: if view
                        .selected
                        .and_then(|id| session.core.object(id))
                        .is_some_and(|o| o.mesh_ref.is_some())
                    {
                        None
                    } else {
                        selected
                    },
                    selected_index: selected_index.unwrap_or(0) as u32,
                    eye: view.eye(),
                    distance: view.distance,
                    scale: scale as f32,
                    grid: view.show_grid,
                    axes: view.show_axes,
                    gizmo: if view.tool != super::transform::Tool::Camera {
                        view.selected
                            .and_then(|id| session.core.object(id))
                            .map(|object| threex_renderer::guides::Gizmo {
                                origin: object.position,
                                axes: super::transform::axes(object, view.tool),
                                size: super::transform::handle_size(
                                    &view,
                                    object,
                                    region.map_or(1., |r| r[3] as f32 / scale as f32),
                                ),
                                selected_axis: view.axis.index().map(|i| i as u32),
                                mode: match view.tool {
                                    super::transform::Tool::Move => 1,
                                    super::transform::Tool::Rotate => 2,
                                    super::transform::Tool::Scale => 3,
                                    _ => 0,
                                },
                            })
                    } else {
                        None
                    },
                }));
            for attempt in 0..2 {
                let frame = if let Some(camera) = camera {
                    target.backend.draw(session.core.scene(), &camera)
                } else {
                    target.backend.draw_blank()
                };
                match frame {
                    Ok(()) => break,
                    Err(AcquireFailure::Lost | AcquireFailure::Outdated) if attempt == 0 => {
                        target
                            .backend
                            .configure(size[0], size[1])
                            .map_err(|e| format!("{e:?}"))?;
                    }
                    Err(
                        AcquireFailure::Timeout | AcquireFailure::Lost | AcquireFailure::Outdated,
                    ) => {
                        status = "gpu_waiting";
                        break;
                    }
                    Err(e) => return Err(format!("{e:?}")),
                }
            }
        }
        Ok(status)
    })();
    match result {
        Ok(status) => {
            let _ = app.emit("atom://viewport", status);
        }
        Err(e) => {
            eprintln!("ATOM GPU: {e}");
            let _ = app.emit("atom://viewport", "gpu_unavailable");
        }
    }
}

/// Same physical hit region as native rendering. Inactive tabs receive no camera input.
pub fn region(app: &tauri::AppHandle, label: &str, active: bool) -> Option<[u32; 4]> {
    let workspace = app.state::<Workspace>();
    let layout = workspace.effective();
    if layout.host(Panel::Viewport) != label {
        return None;
    }
    let window = app.get_window(label)?;
    let size = workspace.size(&window).ok()?;
    let scale = window.scale_factor().ok()?;
    if label == "main" {
        layout
            .geometry([size.width, size.height], scale, workspace.inset(label))
            .groups
            .iter()
            .find(|g| {
                g.panels.contains(&Panel::Viewport) && (!active || g.active == Panel::Viewport)
            })
            .map(|g| g.content(scale))
    } else {
        let group = layout.floating_group(Panel::Viewport)?;
        if active && group.active != Panel::Viewport {
            return None;
        }
        let top = (workspace.inset(label) + (96. * scale).round() as u32).min(size.height);
        Some([0, top, size.width, size.height - top])
    }
    .filter(|r| r[2] > 0 && r[3] > 0)
}
pub fn aspect(app: &tauri::AppHandle) -> f32 {
    let label = app.state::<Workspace>().snapshot().host(Panel::Viewport);
    region(app, label, false).map_or(1., |r| r[2] as f32 / r[3] as f32)
}
