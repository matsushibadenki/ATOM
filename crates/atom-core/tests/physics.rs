use atom_core::*;
fn edit(c: &mut AppCore, command: Command) -> Result<Snapshot> {
    let s = c.snapshot();
    c.apply(EditRequest {
        expected_revision: s.revision,
        project_id: s.project_id,
        command,
    })
}
#[test]
fn gravity_material_history_save_restore_and_branching() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = ProjectStore::open(temp.path().join("physics.json")).unwrap();
    let mut c = AppCore::new(Project::new("Physics").unwrap()).unwrap();
    let original = c.project().objects[0].clone();
    edit(
        &mut c,
        Command::SetGravity {
            gravity: Gravity {
                preset: GravityPreset::Moon,
                vector: [0., 0., -1.62],
            },
        },
    )
    .unwrap();
    let custom = PhysicalMaterial {
        preset: MaterialPreset::Custom,
        density: 820.,
        friction: [1.8, 0.02, 0.001],
    };
    edit(
        &mut c,
        Command::SetMaterial {
            id: Some(1),
            material: custom.clone(),
        },
    )
    .unwrap();
    edit(
        &mut c,
        Command::SetMaterial {
            id: None,
            material: PhysicalMaterial::preset(MaterialPreset::Rubber),
        },
    )
    .unwrap();
    store.save(c.project()).unwrap();
    c.mark_saved();
    edit(&mut c, Command::Undo).unwrap();
    assert!(c.snapshot().dirty);
    edit(&mut c, Command::Undo).unwrap();
    assert_eq!(c.project().objects[0], original);
    edit(&mut c, Command::Redo).unwrap();
    edit(&mut c, Command::Redo).unwrap();
    assert!(!c.snapshot().dirty);
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.physics, c.project().physics);
    assert_eq!(loaded.objects[0].material, custom);
    assert_eq!(loaded.objects[0].color, original.color);
    edit(&mut c, Command::Undo).unwrap();
    edit(&mut c, Command::SetFloor { enabled: false }).unwrap();
    assert!(!c.snapshot().can_redo);
}
#[test]
fn invalid_values_and_stale_windows_do_not_change_history() {
    let mut c = AppCore::new(Project::new("Physics").unwrap()).unwrap();
    let old = c.snapshot();
    for vector in [[f64::NAN, 0., 0.], [0., 0., 1001.]] {
        assert!(
            edit(
                &mut c,
                Command::SetGravity {
                    gravity: Gravity {
                        preset: GravityPreset::Custom,
                        vector
                    }
                }
            )
            .is_err()
        );
    }
    assert!(
        edit(
            &mut c,
            Command::SetGravity {
                gravity: Gravity {
                    preset: GravityPreset::Mars,
                    vector: [0.; 3]
                }
            }
        )
        .is_err()
    );
    for material in [
        PhysicalMaterial {
            preset: MaterialPreset::Custom,
            density: -1.,
            friction: [0.; 3],
        },
        PhysicalMaterial {
            preset: MaterialPreset::Custom,
            density: 1.,
            friction: [-1., 0., 0.],
        },
        PhysicalMaterial {
            preset: MaterialPreset::Wood,
            density: 1.,
            friction: [0.; 3],
        },
    ] {
        assert!(
            edit(
                &mut c,
                Command::SetMaterial {
                    id: Some(1),
                    material
                }
            )
            .is_err()
        );
    }
    assert_eq!(c.snapshot().revision, 0);
    assert_eq!(c.snapshot().undo_depth, 0);
    edit(
        &mut c,
        Command::SetGravity {
            gravity: Gravity {
                preset: GravityPreset::Zero,
                vector: [0.; 3],
            },
        },
    )
    .unwrap();
    assert!(matches!(
        c.apply(EditRequest {
            expected_revision: old.revision,
            project_id: old.project_id,
            command: Command::SetFloor { enabled: false }
        }),
        Err(Error::Conflict(_))
    ));
}
#[test]
fn v1_migrates_without_rewriting_until_saved_and_v2_roundtrips() {
    let mut json = serde_json::to_value(Project::new("Old").unwrap()).unwrap();
    json["schema_version"] = 1.into();
    let map = json.as_object_mut().unwrap();
    map.remove("physics");
    map.remove("robots");
    for o in json["objects"].as_array_mut().unwrap() {
        for key in ["label", "material", "fixed"] {
            o.as_object_mut().unwrap().remove(key);
        }
    }
    let bytes = serde_json::to_vec(&json).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("old.json");
    std::fs::write(&path, &bytes).unwrap();
    let mut store = ProjectStore::open(&path).unwrap();
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.schema_version, 3);
    assert_eq!(loaded.physics, PhysicsSpec::default());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    store.save(&loaded).unwrap();
    assert_eq!(store.load().unwrap().unwrap().objects, loaded.objects);
}
#[test]
fn robot_templates_are_atomic_undoable_nonoverlapping_and_persistent() {
    let mut c = AppCore::new(Project::new("Robots").unwrap()).unwrap();
    for template in [RobotTemplate::Humanoid, RobotTemplate::DualArm] {
        edit(&mut c, Command::AddRobot { template }).unwrap();
    }
    assert_eq!(c.project().robots.len(), 2);
    assert_eq!(c.project().objects.len(), 113);
    let first = &c.project().robots[0];
    let second = &c.project().robots[1];
    let right = c
        .project()
        .objects
        .iter()
        .filter(|o| first.links.iter().any(|l| l.object_id == o.id))
        .map(|o| o.position[0] + o.scale[0] / 2.)
        .fold(f32::NEG_INFINITY, f32::max);
    let left = c
        .project()
        .objects
        .iter()
        .filter(|o| second.links.iter().any(|l| l.object_id == o.id))
        .map(|o| o.position[0] - o.scale[0] / 2.)
        .fold(f32::INFINITY, f32::min);
    assert!(left > right);
    let root = second.root;
    assert!(edit(&mut c, Command::RemoveBox { id: root + 1 }).is_err());
    edit(&mut c, Command::RemoveBox { id: root }).unwrap();
    assert_eq!(c.project().robots.len(), 1);
    edit(&mut c, Command::Undo).unwrap();
    assert_eq!(c.project().robots.len(), 2);
    edit(&mut c, Command::Undo).unwrap();
    assert_eq!(c.project().robots.len(), 1);
    edit(&mut c, Command::Redo).unwrap();
    let p: Project = serde_json::from_slice(&serde_json::to_vec(c.project()).unwrap()).unwrap();
    p.validate().unwrap();
    assert_eq!(p.robots, c.project().robots);
    let mut bad = p.clone();
    bad.robots[0].links[1].parent = Some(bad.robots[0].links[1].object_id);
    assert!(bad.validate().is_err());
}
#[test]
fn simulation_is_transient_conflict_checked_and_cannot_overwrite_edits() {
    let mut c = AppCore::new(Project::new("Preview").unwrap()).unwrap();
    let s = c.snapshot();
    let token = c.begin_simulation(s.revision, s.project_id).unwrap();
    assert!(edit(&mut c, Command::SetFloor { enabled: false }).is_err());
    let result = mujoco::SimulationResult {
        engine: "test".into(),
        time: 0.002,
        contacts: 0,
        poses: vec![mujoco::Pose {
            id: 1,
            position: [0., 2., 0.],
            rotation: [0.; 3],
        }],
    };
    c.finish_simulation(token, Ok(result.clone()), 1).unwrap();
    assert_eq!(c.snapshot().objects[0].position, [0., 2., 0.]);
    assert_eq!(c.project().objects[0].position, [0., 1., 0.]);
    assert!(!c.snapshot().dirty);
    assert_eq!(c.snapshot().undo_depth, 0);
    assert!(!c.snapshot().can_undo);
    c.reset_simulation();
    assert_eq!(c.snapshot().objects[0].position, [0., 1., 0.]);
    edit(&mut c, Command::SetFloor { enabled: false }).unwrap();
    c.finish_simulation(token, Ok(result), 1).unwrap();
    assert!(c.snapshot().simulation.is_none());
    let s = c.snapshot();
    let token = c.begin_simulation(s.revision, s.project_id).unwrap();
    assert!(
        c.finish_simulation(token, Err(Error::PhysicsUnavailable), 1)
            .is_err()
    );
    assert!(!c.has_preview());
}
/// Explicit opt-in: no silent pass when the external runtime is absent.
#[test]
#[ignore = "requires Python with MuJoCo; run with --ignored"]
fn live_gravity_zero_and_custom_axes_match_free_fall() {
    for preset in [
        GravityPreset::Earth,
        GravityPreset::Moon,
        GravityPreset::Mars,
        GravityPreset::Zero,
        GravityPreset::Custom,
    ] {
        let mut p = Project::new("Drop").unwrap();
        p.physics.floor_enabled = false;
        p.objects[0].position = [0., 10., 0.];
        p.objects[0].rotation = [0.3, 0.7, -0.2];
        p.physics.gravity = Gravity {
            preset,
            vector: preset.vector().unwrap_or([1., 2., -3.]),
        };
        let r = mujoco::run(&p, 100).unwrap();
        let g = p.physics.gravity.editor_vector();
        for (i, acceleration) in g.iter().enumerate() {
            let expected = p.objects[0].position[i] as f64 + 0.5 * acceleration * 0.2 * 0.2;
            assert!((r.poses[0].position[i] as f64 - expected).abs() < 0.003);
            assert!((r.poses[0].rotation[i] - p.objects[0].rotation[i]).abs() < 1e-5);
        }
    }
}
#[test]
#[ignore = "requires Python with MuJoCo; run with --ignored"]
fn live_both_robot_templates_compile_step_and_keep_fixed_base() {
    for template in [RobotTemplate::Humanoid, RobotTemplate::DualArm] {
        let mut p = Project::new("Robot").unwrap();
        let (robot, objects, _) = robots::build(template, 1).unwrap();
        p.robots = vec![robot];
        p.objects = objects;
        let r = mujoco::run(&p, 1000).unwrap();
        assert_eq!(r.poses.len(), p.objects.len());
        if template == RobotTemplate::DualArm {
            for i in 0..3 {
                assert!(
                    (r.poses.iter().find(|o| o.id == 1).unwrap().position[i]
                        - p.objects[0].position[i])
                        .abs()
                        < 1e-6
                );
            }
        }
    }
}
#[test]
#[ignore = "requires Python with MuJoCo; run with --ignored"]
fn live_contact_friction_changes_sliding_and_density_updates_mass_inertia() {
    let mut p = Project::new("Friction").unwrap();
    p.objects[0].position = [0., 0.5, 0.];
    p.objects[0].rotation = [0.; 3];
    p.physics.gravity = Gravity {
        preset: GravityPreset::Custom,
        vector: [2., 0., -9.80665],
    };
    p.objects[0].material = PhysicalMaterial {
        preset: MaterialPreset::Custom,
        density: 600.,
        friction: [0.; 3],
    };
    let low = mujoco::run(&p, 500).unwrap();
    p.objects[0].material.friction = [1.2, 0., 0.];
    let high = mujoco::run(&p, 500).unwrap();
    assert!(low.poses[0].position[0] > 0.5);
    assert!(high.poses[0].position[0].abs() < 0.03);
    p.objects[0].scale = [2., 3., 4.];
    p.objects[0].material.density = 1000.;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("mass.xml");
    std::fs::write(&path, mujoco::export(&p).unwrap()).unwrap();
    let output=std::process::Command::new(std::env::var_os("ATOM_MUJOCO_PYTHON").unwrap_or_else(||"python3".into())).args(["-I","-c", "import mujoco,sys; m=mujoco.MjModel.from_xml_path(sys.argv[1]); b=m.body('b1'); assert abs(b.mass[0]-24000)<1e-6; assert sorted(b.inertia.tolist())==sorted([24000/12*(9+16),24000/12*(4+16),24000/12*(4+9)])"]).arg(path).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn bundled_robot_projects_and_mjcf_match_the_generator() {
    for (template, json, xml) in [
        (
            RobotTemplate::Humanoid,
            include_str!("../../../templates/humanoid.atom.json"),
            include_str!("../../../templates/humanoid.xml"),
        ),
        (
            RobotTemplate::DualArm,
            include_str!("../../../templates/dual-arm.atom.json"),
            include_str!("../../../templates/dual-arm.xml"),
        ),
    ] {
        let project: Project = serde_json::from_str(json).unwrap();
        project.validate().unwrap();
        let (robot, objects, _) = robots::build(template, 1).unwrap();
        assert_eq!(project.objects, objects);
        assert_eq!(project.robots, vec![robot]);
        assert_eq!(mujoco::export(&project).unwrap(), xml);
    }
}

#[test]
fn detailed_human_has_complete_digits_spine_and_independent_axes() {
    let (robot, objects, next) = robots::build(RobotTemplate::Humanoid, 1).unwrap();
    assert_eq!(next, 100);
    assert_eq!(robot.links.len(), 99);
    assert_eq!(
        robot
            .links
            .iter()
            .skip(1)
            .map(|l| l.axes().count())
            .sum::<usize>(),
        182
    );
    for (region, count) in [("lumbar", 5), ("thoracic", 12), ("cervical", 7)] {
        for i in 1..=count {
            assert!(objects.iter().any(|o| o.label == format!("{region}_{i}")));
        }
    }
    for side in ["left", "right"] {
        for part in ["finger", "toe"] {
            for digit in 1..=5 {
                for segment in 1..=if digit == 1 { 2 } else { 3 } {
                    let object = objects
                        .iter()
                        .find(|o| o.label == format!("{side}_{part}_{digit}_{segment}"))
                        .unwrap();
                    let link = robot
                        .links
                        .iter()
                        .find(|l| l.object_id == object.id)
                        .unwrap();
                    assert_eq!(link.axes().count(), if segment == 1 { 2 } else { 1 });
                }
            }
        }
    }
    let mut bad = robot.clone();
    bad.links[1].extra_axes[0].axis = bad.links[1].axis;
    assert!(bad.validate(&objects).is_err());
    bad = robot.clone();
    bad.links[1].extra_axes[0].range = [f32::NAN, 1.];
    assert!(bad.validate(&objects).is_err());
    let mut project = Project::new("Human").unwrap();
    project.objects = objects;
    project.robots = vec![robot];
    let xml = mujoco::export(&project).unwrap();
    assert_eq!(xml.matches("<joint name=").count(), 182);
    assert_eq!(xml.matches("<position name=").count(), 182);
    assert_eq!(xml.matches("<freejoint/>").count(), 1);
}

#[test]
fn legacy_human_topology_is_not_silently_upgraded() {
    let project: Project =
        serde_json::from_str(include_str!("fixtures/humanoid-legacy.json")).unwrap();
    project.validate().unwrap();
    assert_eq!(project.robots[0].links.len(), 15);
    assert!(
        project.robots[0]
            .links
            .iter()
            .all(|l| l.extra_axes.is_empty())
    );
    let xml = mujoco::export(&project).unwrap();
    assert_eq!(xml.matches("<joint name=").count(), 14);
    let restored: Project = serde_json::from_slice(&serde_json::to_vec(&project).unwrap()).unwrap();
    assert_eq!(restored.robots, project.robots);
}

#[test]
#[ignore = "requires Python with MuJoCo; run with --ignored"]
fn live_detailed_human_compiles_and_drives_every_rotational_axis() {
    let mut project = Project::new("Full body axes").unwrap();
    let (robot, objects, _) = robots::build(RobotTemplate::Humanoid, 1).unwrap();
    project.objects = objects;
    project.robots = vec![robot];
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("human.xml");
    std::fs::write(&path, mujoco::export(&project).unwrap()).unwrap();
    let script = r#"
import mujoco, numpy as np, sys
m=mujoco.MjModel.from_xml_path(sys.argv[1])
assert (m.nbody,m.njnt,m.nq,m.nv,m.nu)==(100,183,189,188,182)
# Isolate drive/kinematic verification from contact and gravity effects.
m.opt.gravity[:]=0
m.opt.disableflags |= int(mujoco.mjtDisableBit.mjDSBL_CONTACT)
d=mujoco.MjData(m)
mujoco.mj_forward(m,d)
initial=d.geom_xpos.copy()
for i in range(m.nu):
    mujoco.mj_resetData(m,d)
    j=m.actuator_trnid[i,0]
    d.ctrl[i]=min(0.08,m.actuator_ctrlrange[i,1]*0.5)
    mujoco.mj_forward(m,d)
    assert d.qacc[m.jnt_dofadr[j]]>0, (i,d.qacc[m.jnt_dofadr[j]])
    # Each authored axis changes its child body's orientation independently.
    before=d.xmat[m.jnt_bodyid[j]].copy()
    d.qpos[m.jnt_qposadr[j]]=0.05
    mujoco.mj_forward(m,d)
    assert np.linalg.norm(d.xmat[m.jnt_bodyid[j]]-before)>0.01
mujoco.mj_resetData(m,d)
d.ctrl[-1]=0.08
for _ in range(500): mujoco.mj_step(m,d)
mujoco.mj_forward(m,d)
assert not any(w.number for w in d.warning)
assert np.isfinite(d.qpos).all()
assert d.qpos[m.jnt_qposadr[m.actuator_trnid[-1,0]]]>0.001
assert np.max(np.linalg.norm(d.geom_xpos-initial,axis=1))>0.00001
"#;
    let output = std::process::Command::new(
        std::env::var_os("ATOM_MUJOCO_PYTHON").unwrap_or_else(|| "python3".into()),
    )
    .args(["-I", "-c", script])
    .arg(path)
    .output()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
