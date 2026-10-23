//! One owner/token-scoped joint drag, shared by docked and floating panels.
pub mod drag;
use super::{Shared, UiError, error};
use atom_core::{JointEdit, Snapshot, Uuid};
use serde::Deserialize;
use tauri::Manager;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub phase: super::transform::Phase,
    pub expected_revision: u64,
    pub project_id: Uuid,
    pub id: u32,
    pub axis: usize,
    pub token: Option<u64>,
    pub angle: Option<f32>,
    #[serde(default)]
    pub edit: JointEdit,
    pub range: Option<[f32; 2]>,
}
#[tauri::command]
pub fn joint_command(webview: tauri::Webview, request: Request) -> Result<Snapshot, UiError> {
    if !matches!(webview.label(), "scene" | "viewport") {
        return Err(error("forbidden"));
    }
    let shared = webview.state::<Shared>();
    let mut session = shared.0.lock().map_err(|_| error("internal_error"))?;
    let owner = webview.label();
    let result = match request.phase {
        super::transform::Phase::Begin => {
            if webview
                .state::<super::camera::State>()
                .0
                .lock()
                .map_err(|_| error("internal_error"))?
                .selected
                != Some(request.id)
            {
                return Err(error("no_selection"));
            }
            session.core.begin_joint_edit(
                owner,
                request.expected_revision,
                request.project_id,
                request.id,
                request.axis,
                request.edit,
            )
        }
        super::transform::Phase::Preview => {
            let token = request.token.ok_or_else(|| error("invalid_project"))?;
            if let Some(range) = request.range {
                session.core.preview_joint_range(
                    owner,
                    token,
                    request.expected_revision,
                    request.project_id,
                    range,
                )
            } else {
                session.core.preview_joint(
                    owner,
                    token,
                    request.expected_revision,
                    request.project_id,
                    request.angle.ok_or_else(|| error("invalid_project"))?,
                )
            }
        }
        super::transform::Phase::Commit => session.core.commit_joint(
            owner,
            request.token.ok_or_else(|| error("invalid_project"))?,
            request.expected_revision,
            request.project_id,
        ),
        super::transform::Phase::Cancel => session.core.cancel_joint(
            owner,
            request.token.ok_or_else(|| error("invalid_project"))?,
        ),
    }?;
    drop(session);
    let app = webview.app_handle().clone();
    super::transform::publish(&app, &result);
    let redraw = app.clone();
    let _ = app.run_on_main_thread(move || super::viewport::draw(&redraw));
    Ok(result)
}

pub fn range_view(
    core: &atom_core::AppCore,
    view: &super::camera::View,
    height: f32,
) -> Option<threex_renderer::joint_ranges::JointRanges> {
    let id = view.selected?;
    let range = core.joint_ranges(id)?;
    let object = core.object(id)?;
    let radius = (object.scale.iter().map(|v| v * v).sum::<f32>().sqrt() * 0.7)
        .clamp(0.035, 0.22)
        .max(super::transform::handle_size(view, object, height) * 0.4);
    Some(threex_renderer::joint_ranges::JointRanges {
        origin: range.origin,
        arcs: range
            .axes
            .into_iter()
            .map(|a| threex_renderer::joint_ranges::Arc {
                channel: a.channel,
                axis: a.axis,
                zero: a.zero,
                range: a.range,
                angle: a.angle,
            })
            .collect(),
        radius,
        active_bound: core
            .joint_preview_info()
            .filter(|p| p.object_id == id)
            .and_then(|p| match p.edit {
                JointEdit::Lower => Some((p.axis, 0)),
                JointEdit::Upper => Some((p.axis, 1)),
                JointEdit::Angle => None,
            }),
        active_axis: core
            .joint_preview_info()
            .filter(|p| p.object_id == id)
            .map(|p| p.axis),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pixels(renderer: &threex_renderer::Renderer, texture: &wgpu::Texture) -> Vec<u8> {
        let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 512 * 2048,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
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
        renderer.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        rx.recv().unwrap().unwrap();
        buffer.slice(..).get_mapped_range().to_vec()
    }
    #[test]
    #[ignore = "Requires native GPU; position IK must update rendered geometry and undo exactly"]
    fn native_gpu_position_ik_updates_frame_and_undo() {
        tauri::async_runtime::block_on(async {
            let mut project = atom_core::Project::new("IK GPU").unwrap();
            let (robot, objects, _) =
                atom_core::robots::build(atom_core::RobotTemplate::DualArm, 1).unwrap();
            let base = robot.root;
            project.objects = objects;
            project.robots.push(robot);
            let mut core = atom_core::AppCore::new(project).unwrap();
            let id = core
                .project()
                .objects
                .iter()
                .find(|o| o.label == "left_gripper")
                .unwrap()
                .id;
            let mut goal = core.object(id).unwrap().position;
            goal[0] += 0.18;
            goal[2] += 0.10;
            let view = super::super::camera::View {
                selected: Some(id),
                target: [0., 0.8, 0.],
                distance: 3.5,
                pitch: 0.3,
                ..Default::default()
            };
            let adapter = wgpu::Instance::default()
                .request_adapter(&Default::default())
                .await
                .unwrap();
            let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
            device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut renderer =
                threex_renderer::Renderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
            let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("position IK"),
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
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            let before = pixels(&renderer, &texture);
            let apply = |core: &mut atom_core::AppCore, command| {
                let s = core.snapshot();
                core.apply(atom_core::EditRequest {
                    project_id: s.project_id,
                    expected_revision: s.revision,
                    command,
                })
                .unwrap()
            };
            let solved = apply(
                &mut core,
                atom_core::Command::SolveIk {
                    id,
                    base,
                    target: goal,
                },
            );
            assert_eq!(solved.undo_depth, 1);
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            let after = pixels(&renderer, &texture);
            assert_ne!(before, after);
            apply(&mut core, atom_core::Command::Undo);
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            assert_eq!(before, pixels(&renderer, &texture));
            apply(&mut core, atom_core::Command::Redo);
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            assert_eq!(after, pixels(&renderer, &texture));
            assert!(renderer.device.pop_error_scope().await.is_none());
        });
    }
    #[test]
    #[ignore = "Requires native GPU; exercises direct native arc drag through FK, rendering and history"]
    fn native_gpu_direct_arc_drag_preview_commit_undo_and_cancel() {
        tauri::async_runtime::block_on(async {
            let project: atom_core::Project =
                serde_json::from_str(include_str!("../../../templates/humanoid.atom.json"))
                    .unwrap();
            let mut core = atom_core::AppCore::new(project).unwrap();
            let id = core
                .project()
                .objects
                .iter()
                .find(|o| o.label == "left_upper_arm")
                .unwrap()
                .id;
            let view = super::super::camera::View {
                selected: Some(id),
                target: [0., 0.95, 0.],
                distance: 2.6,
                pitch: 0.15,
                ..Default::default()
            };
            let region = [0, 0, 512, 512];
            let range = range_view(&core, &view, 512.).unwrap();
            let project = |p| {
                super::super::transform::project(&view, region, p)
                    .unwrap()
                    .map(f64::from)
            };
            let phase = [-1.2f32, -0.7, 0.6, 1.2]
                .into_iter()
                .find(|p| {
                    super::drag::hit_test(
                        &core,
                        &view,
                        region,
                        1.,
                        project(threex_renderer::joint_ranges::arc_point(&range, 0, *p).unwrap()),
                    )
                    .is_some_and(|hit| hit.axis == 0)
                })
                .unwrap();
            let press =
                project(threex_renderer::joint_ranges::arc_point(&range, 0, phase).unwrap());
            let target_point =
                project(threex_renderer::joint_ranges::arc_point(&range, 0, phase + 0.3).unwrap());
            let (adapter_device, queue) = {
                let adapter = wgpu::Instance::default()
                    .request_adapter(&Default::default())
                    .await
                    .unwrap();
                adapter.request_device(&Default::default()).await.unwrap()
            };
            adapter_device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut renderer = threex_renderer::Renderer::new(
                adapter_device,
                queue,
                wgpu::TextureFormat::Rgba8Unorm,
            );
            let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("direct native joint drag"),
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
            let original = core.snapshot();
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            let before = pixels(&renderer, &texture);
            let (mut drag, _) =
                super::drag::NativeDrag::begin(&mut core, &view, 0, region, 1., press)
                    .unwrap()
                    .unwrap();
            drag.preview(&mut core, target_point).unwrap();
            let preview = core.snapshot();
            assert_eq!(preview.undo_depth, 0);
            assert_eq!(preview.revision, original.revision);
            assert!(
                (preview.robots[0]
                    .links
                    .iter()
                    .find(|l| l.object_id == id)
                    .unwrap()
                    .angle(0)
                    - 0.3)
                    .abs()
                    < 0.001
            );
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            assert_ne!(before, pixels(&renderer, &texture));
            renderer.set_joint_ranges(range_view(&core, &view, 512.));
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            if let Some(path) = std::env::var_os("ATOM_DIRECT_GUIDE_PPM") {
                let mut ppm = b"P6\n512 512\n255\n".to_vec();
                for p in pixels(&renderer, &texture).as_chunks::<4>().0 {
                    ppm.extend(&p[..3]);
                }
                std::fs::write(path, ppm).unwrap();
            }
            drag.commit(&mut core).unwrap();
            assert_eq!(core.snapshot().undo_depth, 1);
            for command in [atom_core::Command::Undo, atom_core::Command::Redo] {
                let s = core.snapshot();
                core.apply(atom_core::EditRequest {
                    expected_revision: s.revision,
                    project_id: s.project_id,
                    command,
                })
                .unwrap();
            }
            assert_eq!(core.snapshot().objects, preview.objects);
            let (mut drag, _) =
                super::drag::NativeDrag::begin(&mut core, &view, 0, region, 1., press)
                    .unwrap()
                    .unwrap();
            drag.preview(
                &mut core,
                project(threex_renderer::joint_ranges::arc_point(&range, 0, phase + 0.6).unwrap()),
            )
            .unwrap();
            drag.cancel(&mut core).unwrap();
            assert_eq!(core.snapshot().objects, preview.objects);
            assert_eq!(core.snapshot().undo_depth, 1);
            assert!(renderer.device.pop_error_scope().await.is_none());
        });
    }
    #[test]
    #[ignore = "Requires native GPU; verifies draggable range caps, pose clamp and atomic history"]
    fn native_gpu_limit_grip_updates_bounds_pose_and_history() {
        tauri::async_runtime::block_on(async {
            let project: atom_core::Project =
                serde_json::from_str(include_str!("../../../templates/humanoid.atom.json"))
                    .unwrap();
            let mut core = atom_core::AppCore::new(project).unwrap();
            let id = core
                .project()
                .objects
                .iter()
                .find(|o| o.label == "left_upper_arm")
                .unwrap()
                .id;
            let view = super::super::camera::View {
                selected: Some(id),
                target: [0., 0.95, 0.],
                distance: 2.6,
                pitch: 0.15,
                ..Default::default()
            };
            let region = [0, 0, 512, 512];
            let project = |p| {
                super::super::transform::project(&view, region, p)
                    .unwrap()
                    .map(f64::from)
            };
            let range = range_view(&core, &view, 512.).unwrap();
            let [a, b] = threex_renderer::joint_ranges::limit_handle(&range, 0, 1).unwrap();
            let press = project(std::array::from_fn(|j| (a[j] + b[j]) * 0.5));
            let hit = super::drag::hit_test(&core, &view, region, 1., press).unwrap();
            assert_eq!(hit.axis, 0);
            assert_eq!(hit.handle, super::drag::Handle::Upper);
            let adapter = wgpu::Instance::default()
                .request_adapter(&Default::default())
                .await
                .unwrap();
            let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
            device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut renderer =
                threex_renderer::Renderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
            let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("joint limit grips"),
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
            let original = core.snapshot();
            renderer.set_joint_ranges(range_view(&core, &view, 512.));
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            let before = pixels(&renderer, &texture);
            let (mut drag, _) =
                super::drag::NativeDrag::begin(&mut core, &view, 0, region, 1., press)
                    .unwrap()
                    .unwrap();
            drag.preview(
                &mut core,
                project(threex_renderer::joint_ranges::arc_point(&range, 0, -0.2).unwrap()),
            )
            .unwrap();
            let preview = core.snapshot();
            let link = preview.robots[0]
                .links
                .iter()
                .find(|l| l.object_id == id)
                .unwrap();
            assert!((link.range[1] + 0.2).abs() < 0.001);
            assert!((link.angle(0) + 0.2).abs() < 0.001);
            assert_eq!(preview.undo_depth, 0);
            assert_eq!(core.project().robots, original.robots);
            let guides = range_view(&core, &view, 512.).unwrap();
            assert_eq!(guides.active_bound, Some((0, 1)));
            renderer.set_joint_ranges(Some(guides));
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            let after = pixels(&renderer, &texture);
            assert_ne!(before, after);
            if let Some(path) = std::env::var_os("ATOM_RANGE_GUIDE_PPM") {
                let mut ppm = b"P6\n512 512\n255\n".to_vec();
                for pixel in after.as_chunks::<4>().0 {
                    ppm.extend(&pixel[..3]);
                }
                std::fs::write(path, ppm).unwrap();
            }
            drag.commit(&mut core).unwrap();
            assert_eq!(core.snapshot().undo_depth, 1);
            let s = core.snapshot();
            core.apply(atom_core::EditRequest {
                project_id: s.project_id,
                expected_revision: s.revision,
                command: atom_core::Command::Undo,
            })
            .unwrap();
            assert_eq!(core.snapshot().objects, original.objects);
            assert_eq!(core.project().robots, original.robots);
            let s = core.snapshot();
            core.apply(atom_core::EditRequest {
                project_id: s.project_id,
                expected_revision: s.revision,
                command: atom_core::Command::Redo,
            })
            .unwrap();
            assert_eq!(core.project().robots, preview.robots);
            let range = range_view(&core, &view, 512.).unwrap();
            let [a, b] = threex_renderer::joint_ranges::limit_handle(&range, 0, 1).unwrap();
            let press = project(std::array::from_fn(|j| (a[j] + b[j]) * 0.5));
            let (mut drag, _) =
                super::drag::NativeDrag::begin(&mut core, &view, 0, region, 1., press)
                    .unwrap()
                    .unwrap();
            drag.preview(
                &mut core,
                project(threex_renderer::joint_ranges::arc_point(&range, 0, -0.5).unwrap()),
            )
            .unwrap();
            drag.cancel(&mut core).unwrap();
            assert_eq!(core.project().robots, preview.robots);
            assert_eq!(core.snapshot().undo_depth, 1);
            assert!(renderer.device.pop_error_scope().await.is_none());
        });
    }
    #[test]
    #[ignore = "Requires native GPU; verifies live humanoid FK and angular-guide rendering"]
    fn native_gpu_humanoid_live_joint_preview_changes_frame_before_commit() {
        tauri::async_runtime::block_on(async {
            let project: atom_core::Project =
                serde_json::from_str(include_str!("../../../templates/humanoid.atom.json"))
                    .unwrap();
            let mut core = atom_core::AppCore::new(project).unwrap();
            let arm = core
                .project()
                .objects
                .iter()
                .find(|o| o.label == "left_upper_arm")
                .unwrap()
                .id;
            let view = super::super::camera::View {
                selected: Some(arm),
                target: [0., 0.95, 0.],
                distance: 2.6,
                pitch: 0.15,
                ..Default::default()
            };
            let adapter = wgpu::Instance::default()
                .request_adapter(&Default::default())
                .await
                .unwrap();
            let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
            device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut renderer =
                threex_renderer::Renderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
            let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("live humanoid limits"),
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
            let s = core.snapshot();
            let token = core
                .begin_joint("scene", s.revision, s.project_id, arm, 0)
                .unwrap()
                .joint_preview
                .unwrap()
                .token;
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            let body_before = pixels(&renderer, &texture);
            renderer.set_joint_ranges(range_view(&core, &view, 512.));
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            let before = pixels(&renderer, &texture);
            core.preview_joint("scene", token, s.revision, s.project_id, 0.8)
                .unwrap();
            renderer.set_joint_ranges(None);
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            assert_ne!(body_before, pixels(&renderer, &texture));
            renderer.set_joint_ranges(range_view(&core, &view, 512.));
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            let after = pixels(&renderer, &texture);
            assert_ne!(before, after);
            assert_eq!(core.snapshot().undo_depth, 0);
            assert_eq!(core.snapshot().revision, s.revision);
            if let Some(path) = std::env::var_os("ATOM_HUMAN_GUIDE_PPM") {
                let mut ppm = b"P6\n512 512\n255\n".to_vec();
                for p in after.as_chunks::<4>().0 {
                    ppm.extend(&p[..3]);
                }
                std::fs::write(path, ppm).unwrap();
            }
            core.cancel_joint("scene", token).unwrap();
            renderer.set_joint_ranges(range_view(&core, &view, 512.));
            renderer.render(core.scene(), &target, 512, 512, &view.matrix(1.));
            assert_ne!(after, pixels(&renderer, &texture));
            assert!(renderer.device.pop_error_scope().await.is_none());
        });
    }
}
