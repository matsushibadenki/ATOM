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
    p.objects[0].rotation = [0.2, -0.3, 0.4];
    for (override_mass, inertia, expected) in [
        (None, None, "24000"),
        (Some(12.), None, "12"),
        (Some(12.), Some([3., 4., 5.]), "12"),
        (None, Some([3., 4., 5.]), "24000"),
    ] {
        p.objects[0].inertia_override = inertia;
        p.objects[0].mass_override = override_mass;
        std::fs::write(&path, mujoco::export(&p).unwrap()).unwrap();
        let output = std::process::Command::new(
            std::env::var_os("ATOM_MUJOCO_PYTHON").unwrap_or_else(|| "python3".into()),
        )
        .args([
            "-I",
            "-c",
            r#"
import math, mujoco, sys
model = mujoco.MjModel.from_xml_path(sys.argv[1])
mass = float(sys.argv[2])
body = model.body('b1')
assert math.isclose(body.mass[0], mass)
if sys.argv[3] == "explicit":
    assert body.inertia.tolist() == [3., 4., 5.]
    assert abs(body.iquat @ model.geom('o1').quat) > 1 - 1e-10, (body.iquat, model.geom('o1').quat)
else:
    expected = sorted(mass / 12 * value for value in (25, 20, 13))
    assert all(math.isclose(a, b) for a, b in zip(sorted(body.inertia), expected))
"#,
        ])
        .arg(&path)
        .arg(expected)
        .arg(if inertia.is_some() {
            "explicit"
        } else {
            "auto"
        })
        .output()
        .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
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

#[test]
fn explicit_box_mass_survives_resize_material_edit_and_history() {
    let mut core = AppCore::new(Project::new("Mass").unwrap()).unwrap();
    let id = core.project().objects[0].id;
    edit(
        &mut core,
        Command::SetMass {
            id,
            mass: Some(12.),
        },
    )
    .unwrap();
    let mut transform = Transform::from(&core.project().objects[0]);
    transform.scale = [2., 3., 4.];
    edit(&mut core, Command::Transform { id, transform }).unwrap();
    edit(
        &mut core,
        Command::SetMaterial {
            id: Some(id),
            material: PhysicalMaterial::preset(MaterialPreset::Steel),
        },
    )
    .unwrap();
    let xml = mujoco::export(core.project()).unwrap();
    let geom = xml
        .lines()
        .find(|line| line.contains(&format!("name=\"o{id}\"")))
        .unwrap();
    assert!(
        geom.contains("size=\"1 1.5 2\"") && geom.contains("mass=\"12\""),
        "{geom}"
    );
    assert!(edit(&mut core, Command::SetMass { id, mass: Some(0.) }).is_err());
    assert_eq!(core.project().objects[0].mass_override, Some(12.));
    let temp = tempfile::tempdir().unwrap();
    let mut store = ProjectStore::open(temp.path().join("mass.json")).unwrap();
    store.save(core.project()).unwrap();
    assert_eq!(
        store.load().unwrap().unwrap().objects[0].mass_override,
        Some(12.)
    );
    edit(&mut core, Command::SetMass { id, mass: None }).unwrap();
    assert_eq!(core.project().objects[0].mass_override, None);
    edit(&mut core, Command::Undo).unwrap();
    assert_eq!(core.project().objects[0].mass_override, Some(12.));
    edit(&mut core, Command::Redo).unwrap();
    assert_eq!(core.project().objects[0].mass_override, None);
}

#[test]
fn box_inertia_override_roundtrips_and_clears_as_one_edit() {
    let mut core = AppCore::new(Project::new("Inertia").unwrap()).unwrap();
    let id = core.project().objects[0].id;
    let moments = Some([3., 4., 5.]);
    assert!(
        edit(
            &mut core,
            Command::SetInertia {
                id,
                inertia: Some([1., 1., 3.])
            }
        )
        .is_err()
    );
    edit(
        &mut core,
        Command::SetInertia {
            id,
            inertia: moments,
        },
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut store = ProjectStore::open(directory.path().join("inertia.json")).unwrap();
    store.save(core.project()).unwrap();
    assert_eq!(
        store.load().unwrap().unwrap().objects[0].inertia_override,
        moments
    );
    edit(&mut core, Command::SetInertia { id, inertia: None }).unwrap();
    assert!(
        !mujoco::export(core.project())
            .unwrap()
            .contains("<inertial")
    );
    edit(&mut core, Command::Undo).unwrap();
    assert_eq!(core.project().objects[0].inertia_override, moments);
    edit(&mut core, Command::Redo).unwrap();
    assert_eq!(core.project().objects[0].inertia_override, None);
}

#[test]
fn contact_pair_friction_is_symmetric_persistent_and_removed_with_object() {
    let mut core = AppCore::new(Project::new("Pair").unwrap()).unwrap();
    edit(&mut core, Command::AddBox).unwrap();
    let friction = [0.2, 0.03, 0.004];
    edit(
        &mut core,
        Command::SetContactFriction {
            objects: [2, 1],
            friction: Some(friction),
        },
    )
    .unwrap();
    let pair = ContactPair {
        objects: [1, 2],
        friction,
    };
    assert_eq!(
        core.project().physics.contact_pairs,
        std::slice::from_ref(&pair)
    );
    let temp = tempfile::tempdir().unwrap();
    let mut store = ProjectStore::open(temp.path().join("pairs.json")).unwrap();
    store.save(core.project()).unwrap();
    assert_eq!(
        store.load().unwrap().unwrap().physics.contact_pairs,
        std::slice::from_ref(&pair)
    );
    assert!(
        edit(
            &mut core,
            Command::SetContactFriction {
                objects: [1, 99999],
                friction: Some(friction),
            }
        )
        .is_err()
    );
    edit(&mut core, Command::RemoveBox { id: 2 }).unwrap();
    assert!(core.project().physics.contact_pairs.is_empty());
    edit(&mut core, Command::Undo).unwrap();
    assert_eq!(core.project().physics.contact_pairs, [pair]);
    edit(
        &mut core,
        Command::SetContactFriction {
            objects: [1, 2],
            friction: None,
        },
    )
    .unwrap();
    assert!(core.project().physics.contact_pairs.is_empty());
    edit(&mut core, Command::Undo).unwrap();
    assert_eq!(core.project().physics.contact_pairs[0].friction, friction);
    edit(&mut core, Command::Redo).unwrap();
    assert!(core.project().physics.contact_pairs.is_empty());
}

#[test]
#[ignore = "requires Python with MuJoCo"]
fn live_pair_override_changes_only_the_selected_contact() {
    let mut core = AppCore::new(Project::new("Pair contacts").unwrap()).unwrap();
    edit(&mut core, Command::AddBox).unwrap();
    let mut project = core.project().clone();
    project.physics.floor_enabled = false;
    project.objects[0].fixed = true;
    project.objects[0].rotation = [0.; 3];
    project.objects[1].rotation = [0.; 3];
    project.objects[1].position = [0.75, 1., 0.];
    project.objects[1].material = PhysicalMaterial::preset(MaterialPreset::Rubber);
    let automatic = mujoco::export(&project).unwrap();
    project.physics.contact_pairs.push(ContactPair {
        objects: [1, 2],
        friction: [0.2, 0.03, 0.004],
    });
    let explicit = mujoco::export(&project).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let auto = temp.path().join("auto.xml");
    let custom = temp.path().join("custom.xml");
    std::fs::write(&auto, automatic).unwrap();
    std::fs::write(&custom, explicit).unwrap();
    let output = std::process::Command::new(
        std::env::var_os("ATOM_MUJOCO_PYTHON").unwrap_or_else(|| "python3".into()),
    ).args(["-I", "-c", r#"
import mujoco, numpy as np, sys
for path, expected in zip(sys.argv[1:], ([1.2, 1.2, .005, .0001, .0001], [.2, .2, .03, .004, .004])):
    model = mujoco.MjModel.from_xml_path(path)
    data = mujoco.MjData(model)
    mujoco.mj_forward(model, data)
    assert data.ncon > 0
    for contact in data.contact:
        assert set(contact.geom) == {model.geom('o1').id, model.geom('o2').id}
        np.testing.assert_allclose(contact.friction, expected)
"#]).arg(auto).arg(custom).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires Python with MuJoCo"]
fn live_continuous_preview_pauses_resumes_and_restores_authoring() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    use std::time::Duration;
    let mut core = AppCore::new(Project::new("Continuous").unwrap()).unwrap();
    edit(&mut core, Command::AddBox).unwrap();
    edit(&mut core, Command::AddBox).unwrap();
    edit(&mut core, Command::Undo).unwrap();
    let initial = core.snapshot();
    let project = core.project().clone();
    let token = core
        .begin_continuous_simulation(initial.revision, initial.project_id)
        .unwrap();
    let pause = AtomicBool::new(false);
    let cancel = AtomicBool::new(false);
    let (send, receive) = mpsc::channel();
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            mujoco::run_continuous(&project, &cancel, &pause, |result, steps| {
                pause.store(true, Ordering::Relaxed);
                send.send((result, steps)).unwrap();
                Ok(())
            })
        });
        let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (first, steps) = receive.recv_timeout(Duration::from_secs(15)).unwrap();
            assert_eq!(steps, 25);
            core.update_continuous_simulation(token, first, steps)
                .unwrap();
            core.set_simulation_paused(true).unwrap();
            assert!(matches!(
                receive.recv_timeout(Duration::from_millis(150)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ));
            assert!(core.snapshot().simulation_paused);
            core.set_simulation_paused(false).unwrap();
            pause.store(false, Ordering::Relaxed);
            let (second, steps) = receive.recv_timeout(Duration::from_secs(3)).unwrap();
            assert_eq!(steps, 50);
            let expected = mujoco::run(&project, steps).unwrap();
            assert_eq!(
                serde_json::to_value(&second).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
            core.update_continuous_simulation(token, second, steps)
                .unwrap();
            assert_ne!(core.snapshot().objects, initial.objects);
        }));
        cancel.store(true, Ordering::Relaxed);
        worker.join().unwrap().unwrap();
        if let Err(error) = checked {
            std::panic::resume_unwind(error);
        }
    });
    let restored = core.reset_simulation();
    assert_eq!(restored.objects, initial.objects);
    assert_eq!(restored.undo_depth, initial.undo_depth);
    assert_eq!(restored.redo_depth, initial.redo_depth);
    assert_eq!(restored.revision, initial.revision);
    assert!(!restored.previewing);
}
