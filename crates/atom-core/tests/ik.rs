use atom_core::*;
fn edit(core: &mut AppCore, command: Command) -> Result<Snapshot> {
    core.apply(EditRequest {
        expected_revision: core.snapshot().revision,
        project_id: core.project().id,
        command,
    })
}
fn robot(template: RobotTemplate) -> AppCore {
    let mut p = Project::new("Position IK").unwrap();
    let (r, objects, _) = robots::build(template, 1).unwrap();
    p.robots.push(r);
    p.objects = objects;
    AppCore::new(p).unwrap()
}
fn id(c: &AppCore, label: &str) -> u32 {
    c.project()
        .objects
        .iter()
        .find(|o| o.label == label)
        .unwrap()
        .id
}
fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.into_iter()
        .zip(b)
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}
#[test]
fn ik_reaches_with_limits_one_history_save_restore_and_determinism() {
    let mut c = robot(RobotTemplate::DualArm);
    let base = c.project().robots[0].root;
    let end = id(&c, "left_gripper");
    let other = id(&c, "right_gripper");
    let mut reference = AppCore::new(c.project().clone()).unwrap();
    for (label, angle) in [
        ("left_shoulder", 0.4),
        ("left_upper_arm", 0.65),
        ("left_elbow", -0.5),
        ("left_forearm", -0.7),
    ] {
        let id = id(&c, label);
        edit(
            &mut reference,
            Command::SetJointAngle { id, axis: 0, angle },
        )
        .unwrap();
    }
    let target = reference.object(end).unwrap().position;
    let before = c.snapshot();
    let mut duplicate = AppCore::new(c.project().clone()).unwrap();
    let after = edit(
        &mut c,
        Command::SolveIk {
            id: end,
            base,
            target,
        },
    )
    .unwrap();
    assert!(distance(c.object(end).unwrap().position, target) <= 0.001);
    assert_eq!(after.undo_depth, 1);
    assert_eq!(
        c.object(other).unwrap(),
        before.objects.iter().find(|o| o.id == other).unwrap()
    );
    assert_eq!(
        c.object(base).unwrap(),
        before.objects.iter().find(|o| o.id == base).unwrap()
    );
    assert_eq!(c.project().objects, duplicate.project().objects);
    c.project().validate().unwrap();
    edit(
        &mut duplicate,
        Command::SolveIk {
            id: end,
            base,
            target,
        },
    )
    .unwrap();
    assert_eq!(c.project().robots, duplicate.project().robots);
    let tmp = tempfile::tempdir().unwrap();
    let mut store = ProjectStore::open(tmp.path().join("ik.atom.json")).unwrap();
    store.save(c.project()).unwrap();
    c.mark_saved();
    let restored = AppCore::new(store.load().unwrap().unwrap()).unwrap();
    assert_eq!(restored.snapshot().objects, after.objects);
    edit(&mut c, Command::Undo).unwrap();
    assert_eq!(c.snapshot().objects, before.objects);
    let redo = edit(&mut c, Command::Redo).unwrap();
    assert_eq!(redo.objects, after.objects);
    assert!(!redo.dirty);
    let revision = redo.revision;
    assert_eq!(
        edit(
            &mut c,
            Command::SolveIk {
                id: end,
                base,
                target
            }
        )
        .unwrap()
        .revision,
        revision
    );
}
#[test]
fn ik_failures_are_atomic_and_cannot_change_an_unrelated_chain() {
    let mut c = robot(RobotTemplate::DualArm);
    let base = c.project().robots[0].root;
    let end = id(&c, "left_gripper");
    let before = serde_json::to_value(c.project()).unwrap();
    for (base, target) in [
        (base, [100., 100., 100.]),
        (end, [0.; 3]),
        (id(&c, "right_shoulder"), [0.; 3]),
        (u32::MAX, [0.; 3]),
        (base, [f32::NAN, 0., 0.]),
        (base, [10001., 0., 0.]),
    ] {
        assert!(
            edit(
                &mut c,
                Command::SolveIk {
                    id: end,
                    base,
                    target
                }
            )
            .is_err()
        );
        assert_eq!(serde_json::to_value(c.project()).unwrap(), before);
        assert_eq!(c.snapshot().revision, 0);
        assert_eq!(c.snapshot().undo_depth, 0);
    }
    let target = c.object(end).unwrap().position;
    assert!(
        c.apply(EditRequest {
            project_id: c.project().id,
            expected_revision: 99,
            command: Command::SolveIk {
                id: end,
                base,
                target
            }
        })
        .is_err()
    );
    c.begin_joint("test", 0, c.project().id, end, 0).unwrap();
    assert!(matches!(
        edit(
            &mut c,
            Command::SolveIk {
                id: end,
                base,
                target
            }
        ),
        Err(Error::EditBusy)
    ));
}
#[test]
fn ik_humanoid_multi_axis_chain_keeps_chosen_base_and_ancestors_fixed() {
    let mut c = robot(RobotTemplate::Humanoid);
    let end = id(&c, "left_palm");
    let base = c.project().robots[0]
        .links
        .iter()
        .find(|l| l.object_id == id(&c, "left_upper_arm"))
        .unwrap()
        .parent
        .unwrap();
    let mut reference = AppCore::new(c.project().clone()).unwrap();
    for (label, axis, angle) in [
        ("left_upper_arm", 0, 0.4),
        ("left_upper_arm", 1, 0.3),
        ("left_forearm", 0, -0.7),
    ] {
        let id = id(&c, label);
        edit(&mut reference, Command::SetJointAngle { id, axis, angle }).unwrap();
    }
    let target = reference.object(end).unwrap().position;
    let before = c.project().robots[0].clone();
    edit(
        &mut c,
        Command::SolveIk {
            id: end,
            base,
            target,
        },
    )
    .unwrap();
    assert!(distance(c.object(end).unwrap().position, target) <= 0.001);
    for link in &before.links {
        if !["left_upper_arm", "left_forearm", "left_palm"]
            .iter()
            .any(|label| id(&c, label) == link.object_id)
        {
            assert_eq!(
                link,
                c.project().robots[0]
                    .links
                    .iter()
                    .find(|l| l.object_id == link.object_id)
                    .unwrap()
            );
        }
    }
    c.project().validate().unwrap();
}
#[test]
fn ik_straight_chain_can_bend_towards_a_closer_target() {
    let mut c = robot(RobotTemplate::DualArm);
    let end = id(&c, "left_gripper");
    let base = c.project().robots[0].root;
    let mut target = c.object(end).unwrap().position;
    target[0] += 0.18;
    edit(
        &mut c,
        Command::SolveIk {
            id: end,
            base,
            target,
        },
    )
    .unwrap();
    assert!(distance(c.object(end).unwrap().position, target) <= 0.001);
}

#[test]
fn ik_default_humanoid_root_chain_reaches_without_moving_root() {
    let mut c = robot(RobotTemplate::Humanoid);
    let base = c.project().robots[0].root;
    let end = id(&c, "left_palm");
    let root = c.object(base).unwrap().clone();
    let p = c.object(end).unwrap().position;
    let target = [p[0] - 0.08, p[1] + 0.04, p[2] + 0.08];
    edit(
        &mut c,
        Command::SolveIk {
            id: end,
            base,
            target,
        },
    )
    .unwrap();
    assert!(distance(c.object(end).unwrap().position, target) <= 0.001);
    assert_eq!(c.object(base).unwrap(), &root);
    c.project().validate().unwrap();
}

#[test]
fn ik_uses_world_target_under_a_posed_base_and_respects_edited_limits() {
    let mut c = robot(RobotTemplate::DualArm);
    let base = id(&c, "left_shoulder");
    let end = id(&c, "left_gripper");
    edit(
        &mut c,
        Command::SetJointAngle {
            id: base,
            axis: 0,
            angle: 0.7,
        },
    )
    .unwrap();
    let arm = id(&c, "left_upper_arm");
    edit(
        &mut c,
        Command::SetJointRange {
            id: arm,
            axis: 0,
            range: [0.2, 0.9],
        },
    )
    .unwrap();
    let original = c.snapshot();
    let mut reference = AppCore::new(c.project().clone()).unwrap();
    edit(
        &mut reference,
        Command::SetJointAngle {
            id: arm,
            axis: 0,
            angle: 0.65,
        },
    )
    .unwrap();
    let target = reference.object(end).unwrap().position;
    edit(
        &mut c,
        Command::SolveIk {
            id: end,
            base,
            target,
        },
    )
    .unwrap();
    assert!(distance(c.object(end).unwrap().position, target) <= 0.001);
    assert_eq!(
        c.object(base).unwrap(),
        original.objects.iter().find(|o| o.id == base).unwrap()
    );
    c.project().validate().unwrap();
    let revision = c.snapshot().revision;
    assert!(matches!(
        edit(
            &mut c,
            Command::SolveIk {
                id: arm,
                base,
                target: [50.; 3]
            }
        ),
        Err(Error::IkUnreachable)
    ));
    assert_eq!(c.snapshot().revision, revision);
}

#[test]
#[ignore = "Requires Python with MuJoCo; validates IK authored keyframe"]
fn ik_authored_keyframe_matches_mujoco_endpoint_and_limits() {
    let mut c = robot(RobotTemplate::DualArm);
    let base = c.project().robots[0].root;
    let end = id(&c, "left_gripper");
    let mut target = c.object(end).unwrap().position;
    target[0] += 0.18;
    target[2] += 0.1;
    edit(
        &mut c,
        Command::SolveIk {
            id: end,
            base,
            target,
        },
    )
    .unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let xml = tmp.path().join("ik.xml");
    std::fs::write(&xml, mujoco::export(c.project()).unwrap()).unwrap();
    let output = std::process::Command::new(
        std::env::var_os("ATOM_MUJOCO_PYTHON").unwrap_or_else(|| "python3".into()),
    )
    .args([
        "-I",
        "-c",
        r#"
import mujoco,sys,numpy as np
m=mujoco.MjModel.from_xml_path(sys.argv[1]);d=mujoco.MjData(m)
k=mujoco.mj_name2id(m,mujoco.mjtObj.mjOBJ_KEY,'authored')
assert k>=0;mujoco.mj_resetDataKeyframe(m,d,k);mujoco.mj_forward(m,d)
g=mujoco.mj_name2id(m,mujoco.mjtObj.mjOBJ_GEOM,'o'+sys.argv[2])
assert np.linalg.norm(d.geom_xpos[g]-np.array(list(map(float,sys.argv[3:6]))))<=0.00101
for i in range(m.nu):
    j=m.actuator_trnid[i,0];q=d.qpos[m.jnt_qposadr[j]]
    assert m.jnt_range[j,0]<=q<=m.jnt_range[j,1]
    assert abs(d.ctrl[i]-q)<1e-6
"#,
    ])
    .arg(xml)
    .arg(end.to_string())
    .args(target.map(|v| v.to_string()))
    .output()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let authored = c.snapshot().objects;
    let s = c.snapshot();
    let token = c.begin_simulation(s.revision, s.project_id).unwrap();
    let result = mujoco::run(c.project(), 1).unwrap();
    c.finish_simulation(token, Ok(result), 1).unwrap();
    c.reset_simulation();
    assert_eq!(c.snapshot().objects, authored);
}
