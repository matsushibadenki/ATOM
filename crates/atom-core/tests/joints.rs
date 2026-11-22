use atom_core::*;
fn edit(core: &mut AppCore, command: Command) -> Result<Snapshot> {
    let s = core.snapshot();
    core.apply(EditRequest {
        expected_revision: s.revision,
        project_id: s.project_id,
        command,
    })
}
fn human() -> AppCore {
    let mut p = Project::new("Joint authoring").unwrap();
    let (robot, objects, _) = robots::build(RobotTemplate::Humanoid, 1).unwrap();
    p.objects = objects;
    p.robots = vec![robot];
    AppCore::new(p).unwrap()
}
fn id(core: &AppCore, label: &str) -> u32 {
    core.project()
        .objects
        .iter()
        .find(|o| o.label == label)
        .unwrap()
        .id
}
fn close(a: [f32; 3], b: [f32; 3]) {
    for i in 0..3 {
        assert!((a[i] - b[i]).abs() < 2e-5, "{a:?} != {b:?}");
    }
}
#[test]
fn joint_pose_moves_descendants_and_roundtrips_with_history() {
    let mut c = human();
    let arm = id(&c, "left_upper_arm");
    let hand = id(&c, "left_palm");
    let other = id(&c, "right_palm");
    let rest = c.project().objects.clone();
    let original = c.snapshot();
    let posed = edit(
        &mut c,
        Command::SetJointAngle {
            id: arm,
            axis: 0,
            angle: 0.8,
        },
    )
    .unwrap();
    assert_ne!(
        c.object(hand).unwrap().position,
        original.objects[(hand - 1) as usize].position
    );
    assert_eq!(
        c.object(other).unwrap(),
        &original.objects[(other - 1) as usize]
    );
    assert_eq!(c.project().objects, rest);
    assert_eq!(posed.undo_depth, 1);
    assert!(posed.dirty);
    let temp = tempfile::tempdir().unwrap();
    let mut store = ProjectStore::open(temp.path().join("pose.json")).unwrap();
    store.save(c.project()).unwrap();
    c.mark_saved();
    let restored = AppCore::new(store.load().unwrap().unwrap()).unwrap();
    assert_eq!(restored.snapshot().objects, posed.objects);
    assert_eq!(restored.project().robots, c.project().robots);
    edit(&mut c, Command::Undo).unwrap();
    assert_eq!(c.snapshot().objects, original.objects);
    edit(&mut c, Command::Redo).unwrap();
    assert_eq!(c.snapshot().objects, posed.objects);
    assert!(!c.snapshot().dirty);
    let unchanged = c.snapshot();
    edit(
        &mut c,
        Command::SetJointAngle {
            id: arm,
            axis: 0,
            angle: 0.8,
        },
    )
    .unwrap();
    assert_eq!(c.snapshot().revision, unchanged.revision);
    edit(
        &mut c,
        Command::SetJointAngle {
            id: arm,
            axis: 0,
            angle: 0.,
        },
    )
    .unwrap();
    assert_eq!(c.snapshot().objects, original.objects);
}
#[test]
fn joint_pivot_edit_updates_only_the_posed_branch_and_persists() {
    let mut c = human();
    let arm = id(&c, "left_upper_arm");
    let hand = id(&c, "left_palm");
    let other = id(&c, "right_palm");
    let root = c.project().robots[0].root;
    let rest = c.snapshot().objects;
    let anchor = c.joint_ranges(arm).unwrap().origin;
    let moved = [anchor[0], anchor[1] + 0.1, anchor[2]];
    let command = || Command::SetJointAnchor {
        id: arm,
        anchor: moved,
    };
    let original_project = c.project().clone();
    let changed = edit(&mut c, command()).unwrap();
    assert_eq!(c.joint_ranges(arm).unwrap().origin, moved);
    assert_eq!(changed.objects, rest);
    assert_eq!(changed.undo_depth, 1);
    assert_eq!(edit(&mut c, command()).unwrap().revision, changed.revision);
    for (id, anchor) in [(root, moved), (arm, [f32::NAN; 3]), (arm, [10_001.; 3])] {
        assert!(edit(&mut c, Command::SetJointAnchor { id, anchor }).is_err());
        assert_eq!(c.snapshot().revision, changed.revision);
    }
    let pose = || Command::SetJointAngle {
        id: arm,
        axis: 0,
        angle: 0.8,
    };
    edit(&mut c, pose()).unwrap();
    let posed = c.snapshot();
    let mut baseline = AppCore::new(original_project).unwrap();
    edit(&mut baseline, pose()).unwrap();
    assert_ne!(c.object(hand), baseline.object(hand));
    assert_eq!(c.object(other), baseline.object(other));

    let temp = tempfile::tempdir().unwrap();
    let mut store = ProjectStore::open(temp.path().join("pivot.json")).unwrap();
    store.save(c.project()).unwrap();
    let restored = AppCore::new(store.load().unwrap().unwrap()).unwrap();
    assert_eq!(restored.snapshot().objects, posed.objects);
    assert_eq!(restored.project().robots, c.project().robots);
    edit(&mut c, Command::Undo).unwrap();
    edit(&mut c, Command::Undo).unwrap();
    assert_eq!(c.joint_ranges(arm).unwrap().origin, anchor);
    edit(&mut c, Command::Redo).unwrap();
    assert_eq!(c.joint_ranges(arm).unwrap().origin, moved);
    edit(&mut c, Command::Redo).unwrap();
    assert_eq!(c.snapshot().objects, posed.objects);
}

#[test]
fn root_rigid_transform_moves_posed_links_and_pivots_as_one_edit() {
    for template in [RobotTemplate::Humanoid, RobotTemplate::DualArm] {
        let mut project = Project::new("Root transform").unwrap();
        let (robot, objects, _) = robots::build(template, 1).unwrap();
        let root = robot.root;
        let child = robot.links[1].object_id;
        project.robots.push(robot);
        project.objects = objects;
        let mut core = AppCore::new(project).unwrap();
        edit(
            &mut core,
            Command::SetJointAngle {
                id: child,
                axis: 0,
                angle: 0.1,
            },
        )
        .unwrap();
        edit(&mut core, Command::AddBox).unwrap();
        let before = core.snapshot();
        let center = core.object(root).unwrap().position;
        let joint = core.joint_ranges(child).unwrap();
        let mut transform = Transform::from(core.object(root).unwrap());
        transform.position = [2., 3., 4.];
        transform.rotation[2] = std::f32::consts::FRAC_PI_2;
        let moved = |p: [f32; 3]| {
            [
                2. - (p[1] - center[1]),
                3. + (p[0] - center[0]),
                4. + (p[2] - center[2]),
            ]
        };
        core.begin_transform("view", before.revision, before.project_id, root)
            .unwrap();
        let preview = core
            .preview_transform(
                "view",
                before.revision,
                before.project_id,
                transform.clone(),
            )
            .unwrap();
        assert_eq!(preview.undo_depth, before.undo_depth);
        for (old, new) in before.objects.iter().zip(&preview.objects) {
            if before.robots[0]
                .links
                .iter()
                .any(|link| link.object_id == old.id)
            {
                close(new.position, moved(old.position));
                assert_eq!(new.scale, old.scale);
            } else {
                assert_eq!(old, new);
            }
        }
        close(
            core.joint_ranges(child).unwrap().origin,
            moved(joint.origin),
        );
        let axis = joint.axes[0].axis;
        close(
            core.joint_ranges(child).unwrap().axes[0].axis,
            [-axis[1], axis[0], axis[2]],
        );
        assert_eq!(
            core.object(child),
            preview.objects.iter().find(|o| o.id == child)
        );
        assert_eq!(core.project().robots, before.robots);
        assert_eq!(
            core.cancel_transform("view").unwrap().objects,
            before.objects
        );

        core.begin_transform("view", before.revision, before.project_id, root)
            .unwrap();
        core.preview_transform("view", before.revision, before.project_id, transform)
            .unwrap();
        let committed = core
            .commit_transform("view", before.revision, before.project_id)
            .unwrap();
        assert_eq!(committed.objects, preview.objects);
        assert_eq!(committed.robots, preview.robots);
        assert_eq!(committed.undo_depth, before.undo_depth + 1);

        let temp = tempfile::tempdir().unwrap();
        let mut store = ProjectStore::open(temp.path().join("root.json")).unwrap();
        store.save(core.project()).unwrap();
        let restored = AppCore::new(store.load().unwrap().unwrap())
            .unwrap()
            .snapshot();
        assert_eq!(restored.objects, committed.objects);
        assert_eq!(restored.robots, committed.robots);
        let undone = edit(&mut core, Command::Undo).unwrap();
        assert_eq!(undone.objects, before.objects);
        assert_eq!(undone.robots, before.robots);
        assert_eq!(
            edit(&mut core, Command::Redo).unwrap().objects,
            committed.objects
        );

        let mut scale_only = Transform::from(core.object(root).unwrap());
        scale_only.scale[0] *= 1.1;
        let scaled = edit(
            &mut core,
            Command::Transform {
                id: root,
                transform: scale_only,
            },
        )
        .unwrap();
        assert_eq!(scaled.robots, committed.robots);
        assert!(
            scaled
                .objects
                .iter()
                .zip(&committed.objects)
                .all(|(a, b)| a.id == root || a == b)
        );
    }
}

#[test]
fn every_axis_enforces_limits_without_mutating_on_failure() {
    let mut c = human();
    let links = c.project().robots[0].links.clone();
    for link in links.iter().skip(1) {
        for (axis, spec) in link.axes().enumerate() {
            for angle in [spec.range[0], spec.range[1]] {
                edit(
                    &mut c,
                    Command::SetJointAngle {
                        id: link.object_id,
                        axis,
                        angle,
                    },
                )
                .unwrap();
            }
            for angle in [
                spec.range[0] - 0.001,
                spec.range[1] + 0.001,
                f32::NAN,
                f32::INFINITY,
            ] {
                let before = c.snapshot();
                assert!(
                    edit(
                        &mut c,
                        Command::SetJointAngle {
                            id: link.object_id,
                            axis,
                            angle
                        }
                    )
                    .is_err()
                );
                assert_eq!(c.snapshot().revision, before.revision);
                assert_eq!(c.snapshot().objects, before.objects);
                assert_eq!(c.snapshot().undo_depth, before.undo_depth);
            }
        }
    }
    let s = c.snapshot();
    for (object, axis) in [
        (c.project().robots[0].root, 0),
        (9999, 0),
        (links[1].object_id, 3),
    ] {
        assert!(
            edit(
                &mut c,
                Command::SetJointAngle {
                    id: object,
                    axis,
                    angle: 0.
                }
            )
            .is_err()
        );
    }
    assert!(
        c.apply(EditRequest {
            expected_revision: s.revision - 1,
            project_id: s.project_id,
            command: Command::SetJointAngle {
                id: links[1].object_id,
                axis: 0,
                angle: 0.
            }
        })
        .is_err()
    );
    assert_eq!(c.snapshot().revision, s.revision);
    let mut bad = c.project().clone();
    bad.robots[0].links[1].angles = vec![0.];
    assert!(bad.validate().is_err());
    bad = c.project().clone();
    bad.robots[0].links[1].angles[0] = f32::NAN;
    assert!(bad.validate().is_err());
}
#[test]
fn posed_geometry_transforms_and_preview_locks_remain_consistent() {
    let mut c = human();
    let arm = id(&c, "left_upper_arm");
    let hand = id(&c, "left_palm");
    edit(
        &mut c,
        Command::SetJointAngle {
            id: arm,
            axis: 0,
            angle: 0.6,
        },
    )
    .unwrap();
    let unchanged = c.snapshot();
    c.begin_transform("scene", unchanged.revision, unchanged.project_id, hand)
        .unwrap();
    c.commit_transform("scene", unchanged.revision, unchanged.project_id)
        .unwrap();
    assert_eq!(c.snapshot().undo_depth, unchanged.undo_depth);
    assert_eq!(c.snapshot().objects, unchanged.objects);
    let before = c.object(hand).unwrap().clone();
    let mut transform = Transform::from(&before);
    transform.position[2] += 0.05;
    transform.rotation = [0.2, 0.4, -0.3];
    let s = c.snapshot();
    c.begin_transform("scene", s.revision, s.project_id, hand)
        .unwrap();
    assert!(
        edit(
            &mut c,
            Command::SetJointAngle {
                id: arm,
                axis: 0,
                angle: 0.2
            }
        )
        .is_err()
    );
    c.preview_transform("scene", s.revision, s.project_id, transform.clone())
        .unwrap();
    close(c.object(hand).unwrap().position, transform.position);
    c.commit_transform("scene", s.revision, s.project_id)
        .unwrap();
    close(c.object(hand).unwrap().position, transform.position);
    close(c.object(hand).unwrap().rotation, transform.rotation);
    edit(&mut c, Command::Undo).unwrap();
    close(c.object(hand).unwrap().position, before.position);
    close(c.object(hand).unwrap().rotation, before.rotation);
    let s = c.snapshot();
    let token = c.begin_simulation(s.revision, s.project_id).unwrap();
    assert!(
        edit(
            &mut c,
            Command::SetJointAngle {
                id: arm,
                axis: 0,
                angle: 0.2
            }
        )
        .is_err()
    );
    assert!(
        c.finish_simulation(token, Err(Error::PhysicsFailed), 1)
            .is_err()
    );
    assert!(!c.has_preview());
}
#[test]
#[ignore = "requires Python with MuJoCo; run with --ignored"]
fn authored_pose_matches_mujoco_keyframe_and_simulation_is_not_double_posed() {
    let mut c = human();
    let links = c.project().robots[0].links.clone();
    for link in links.iter().skip(1) {
        for (axis, a) in link.axes().enumerate() {
            edit(
                &mut c,
                Command::SetJointAngle {
                    id: link.object_id,
                    axis,
                    angle: a.range[1] * 0.2,
                },
            )
            .unwrap();
        }
    }
    edit(&mut c, Command::AddBox).unwrap();
    edit(
        &mut c,
        Command::AddRobot {
            template: RobotTemplate::DualArm,
        },
    )
    .unwrap();
    let wrist = id(&c, "left_wrist");
    let anchor = c.joint_ranges(wrist).unwrap().origin;
    edit(
        &mut c,
        Command::SetJointAnchor {
            id: wrist,
            anchor: [anchor[0] + 0.05, anchor[1] + 0.03, anchor[2]],
        },
    )
    .unwrap();
    edit(
        &mut c,
        Command::SetJointAngle {
            id: wrist,
            axis: 0,
            angle: 0.4,
        },
    )
    .unwrap();
    let roots: Vec<_> = c.project().robots.iter().map(|robot| robot.root).collect();
    for root in roots {
        let mut transform = Transform::from(c.object(root).unwrap());
        transform.position[0] += 0.7;
        transform.position[1] += 0.3;
        transform.rotation = [0.31, -0.62, 0.27];
        edit(
            &mut c,
            Command::Transform {
                id: root,
                transform,
            },
        )
        .unwrap();
    }
    let temp = tempfile::tempdir().unwrap();
    let xml = temp.path().join("posed.xml");
    let expected = temp.path().join("posed.json");
    std::fs::write(&xml, mujoco::export(c.project()).unwrap()).unwrap();
    std::fs::write(
        &expected,
        serde_json::to_vec(&c.snapshot().objects).unwrap(),
    )
    .unwrap();
    let script = r#"
import mujoco,numpy as np,json,sys,math
m=mujoco.MjModel.from_xml_path(sys.argv[1]);d=mujoco.MjData(m)
k=mujoco.mj_name2id(m,mujoco.mjtObj.mjOBJ_KEY,'authored');assert k>=0
mujoco.mj_resetDataKeyframe(m,d,k);mujoco.mj_forward(m,d)
for o in json.load(open(sys.argv[2])):
    g=mujoco.mj_name2id(m,mujoco.mjtObj.mjOBJ_GEOM,'o'+str(o['id']))
    assert np.max(np.abs(d.geom_xpos[g]-o['position']))<3e-5,(o['label'],d.geom_xpos[g],o['position'])
    x,y,z=o['rotation'];sx,cx,sy,cy,sz,cz=math.sin(x),math.cos(x),math.sin(y),math.cos(y),math.sin(z),math.cos(z)
    r=np.array([[1,0,0],[0,cx,-sx],[0,sx,cx]])@np.array([[cy,0,sy],[0,1,0],[-sy,0,cy]])@np.array([[cz,-sz,0],[sz,cz,0],[0,0,1]])
    assert np.max(np.abs(d.geom_xmat[g].reshape(3,3)-r))<3e-5,o['label']
for i in range(m.nu):
    j=m.actuator_trnid[i,0];assert abs(d.ctrl[i]-d.qpos[m.jnt_qposadr[j]])<1e-6
"#;
    let output = std::process::Command::new(
        std::env::var_os("ATOM_MUJOCO_PYTHON").unwrap_or_else(|| "python3".into()),
    )
    .args(["-I", "-c", script])
    .arg(xml)
    .arg(expected)
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
    c.finish_simulation(token, Ok(result.clone()), 1).unwrap();
    for pose in result.poses {
        close(c.object(pose.id).unwrap().position, pose.position);
    }
    c.reset_simulation();
    assert_eq!(c.snapshot().objects, authored);
}

#[test]
fn live_joint_drag_is_transient_and_commits_exactly_one_history_entry() {
    let mut c = human();
    let arm = id(&c, "left_upper_arm");
    let hand = id(&c, "left_palm");
    let initial = c.snapshot();
    let begin = c
        .begin_joint("scene", initial.revision, initial.project_id, arm, 0)
        .unwrap();
    let token = begin.joint_preview.unwrap().token;
    for n in 1..=30 {
        let p = c
            .preview_joint(
                "scene",
                token,
                initial.revision,
                initial.project_id,
                n as f32 * 0.02,
            )
            .unwrap();
        assert_eq!(p.revision, initial.revision);
        assert_eq!(p.undo_depth, 0);
        assert!(!p.dirty);
        assert!(p.previewing);
        assert_eq!(c.project().robots[0].links[(arm - 1) as usize].angle(0), 0.);
        assert_eq!(
            c.object(hand).unwrap(),
            p.objects.iter().find(|o| o.id == hand).unwrap()
        );
    }
    let preview = c.snapshot();
    assert_ne!(preview.objects, initial.objects);
    assert!(edit(&mut c, Command::Undo).is_err());
    assert!(
        c.begin_simulation(initial.revision, initial.project_id)
            .is_err()
    );
    assert!(
        c.begin_import(initial.revision, initial.project_id)
            .is_err()
    );
    let committed = c
        .commit_joint("scene", token, initial.revision, initial.project_id)
        .unwrap();
    assert_eq!(committed.objects, preview.objects);
    assert_eq!(committed.undo_depth, 1);
    assert_eq!(committed.revision, initial.revision + 1);
    assert!(committed.joint_preview.is_none());
    edit(&mut c, Command::Undo).unwrap();
    assert_eq!(c.snapshot().objects, initial.objects);
    edit(&mut c, Command::Redo).unwrap();
    assert_eq!(c.snapshot().objects, committed.objects);
}
#[test]
fn joint_drag_cancel_noop_and_stale_owner_tokens_cannot_corrupt_other_edits() {
    let mut c = human();
    let arm = id(&c, "left_upper_arm");
    let s = c.snapshot();
    let token = c
        .begin_joint("scene", s.revision, s.project_id, arm, 0)
        .unwrap()
        .joint_preview
        .unwrap()
        .token;
    c.preview_joint("scene", token, s.revision, s.project_id, 0.4)
        .unwrap();
    let valid = c.snapshot();
    assert!(
        c.preview_joint("viewport", token, s.revision, s.project_id, 0.7)
            .is_err()
    );
    assert!(
        c.preview_joint("scene", token, s.revision + 1, s.project_id, 0.7)
            .is_err()
    );
    assert!(
        c.preview_joint("scene", token, s.revision, s.project_id, 99.)
            .is_err()
    );
    assert!(
        c.commit_joint("viewport", token, s.revision, s.project_id)
            .is_err()
    );
    assert!(c.cancel_joint("viewport", token).is_err());
    assert_eq!(c.snapshot().objects, valid.objects);
    c.cancel_joint("scene", token).unwrap();
    assert_eq!(c.snapshot().objects, s.objects);
    assert_eq!(c.snapshot().undo_depth, 0);
    let next = c
        .begin_joint("scene", s.revision, s.project_id, arm, 0)
        .unwrap()
        .joint_preview
        .unwrap()
        .token;
    assert_ne!(next, token);
    assert!(c.cancel_joint("scene", token).is_err());
    assert!(
        c.preview_joint("scene", token, s.revision, s.project_id, 0.3)
            .is_err()
    );
    assert!(
        c.commit_joint("scene", token, s.revision, s.project_id)
            .is_err()
    );
    c.commit_joint("scene", next, s.revision, s.project_id)
        .unwrap();
    assert_eq!(c.snapshot().undo_depth, 0);
    assert!(!c.snapshot().dirty);
    let token = c
        .begin_joint("scene", s.revision, s.project_id, arm, 0)
        .unwrap()
        .joint_preview
        .unwrap()
        .token;
    c.preview_joint("scene", token, s.revision, s.project_id, 0.4)
        .unwrap();
    c.cancel_transform("scene").unwrap();
    assert!(!c.has_preview());
    assert_eq!(c.snapshot().objects, s.objects);
    assert!(
        c.begin_joint("scene", s.revision, s.project_id, arm, 3)
            .is_err()
    );
    assert!(!c.has_preview());
    assert!(
        c.begin_joint(
            "scene",
            s.revision,
            s.project_id,
            c.project().robots[0].root,
            0
        )
        .is_err()
    );
}
#[test]
fn range_frames_follow_parent_and_preceding_intrinsic_axes_during_drag() {
    let mut c = human();
    let arm = id(&c, "left_upper_arm");
    let hand = id(&c, "left_palm");
    let s = c.snapshot();
    let token = c
        .begin_joint("scene", s.revision, s.project_id, arm, 0)
        .unwrap()
        .joint_preview
        .unwrap()
        .token;
    let before = c.joint_ranges(hand).unwrap().origin;
    c.preview_joint("scene", token, s.revision, s.project_id, 0.5)
        .unwrap();
    let range = c.joint_ranges(arm).unwrap();
    close(range.axes[0].axis, [1., 0., 0.]);
    close(range.axes[1].axis, [0., 0.5f32.cos(), 0.5f32.sin()]);
    assert_ne!(c.joint_ranges(hand).unwrap().origin, before);
    for axis in range.axes {
        let dot: f32 = axis.axis.iter().zip(axis.zero).map(|(x, y)| x * y).sum();
        assert!(dot.abs() < 1e-5);
        assert!((axis.zero.iter().map(|v| v * v).sum::<f32>() - 1.).abs() < 1e-5);
    }
    assert!(c.joint_ranges(c.project().robots[0].root).is_none());
    c.cancel_joint("scene", token).unwrap();
    close(c.joint_ranges(hand).unwrap().origin, before);
    let token = c.begin_simulation(s.revision, s.project_id).unwrap();
    assert!(c.joint_ranges(arm).is_none());
    assert!(
        c.finish_simulation(token, Err(Error::PhysicsFailed), 1)
            .is_err()
    );
}

#[test]
fn range_edit_clamps_pose_atomically_and_persists_with_history() {
    let mut c = human();
    let arm = id(&c, "left_upper_arm");
    edit(
        &mut c,
        Command::SetJointAngle {
            id: arm,
            axis: 0,
            angle: 0.8,
        },
    )
    .unwrap();
    let before = c.snapshot();
    let rest = c.project().objects.clone();
    let changed = edit(
        &mut c,
        Command::SetJointRange {
            id: arm,
            axis: 0,
            range: [-0.2, 0.3],
        },
    )
    .unwrap();
    let link = changed.robots[0]
        .links
        .iter()
        .find(|l| l.object_id == arm)
        .unwrap();
    assert_eq!(link.range, [-0.2, 0.3]);
    assert_eq!(link.angle(0), 0.3);
    assert_eq!(changed.undo_depth, before.undo_depth + 1);
    assert_eq!(c.project().objects, rest);
    let temp = tempfile::tempdir().unwrap();
    let mut store = ProjectStore::open(temp.path().join("ranges.json")).unwrap();
    store.save(c.project()).unwrap();
    c.mark_saved();
    let restored = AppCore::new(store.load().unwrap().unwrap()).unwrap();
    assert_eq!(restored.snapshot().objects, changed.objects);
    assert_eq!(restored.project().robots, c.project().robots);
    edit(&mut c, Command::Undo).unwrap();
    assert_eq!(c.snapshot().objects, before.objects);
    assert_eq!(c.project().robots, before.robots);
    edit(&mut c, Command::Redo).unwrap();
    assert_eq!(c.snapshot().objects, changed.objects);
    assert!(!c.snapshot().dirty);
    let s = c.snapshot();
    edit(
        &mut c,
        Command::SetJointRange {
            id: arm,
            axis: 0,
            range: [-0.2, 0.3],
        },
    )
    .unwrap();
    assert_eq!(c.snapshot().revision, s.revision);
    // Additional axes, positive-only and negative-only intervals use the same contract.
    for (axis, range, expected) in [(1, [0.2, 0.5], 0.2), (2, [-0.8, -0.4], -0.4)] {
        let result = edit(
            &mut c,
            Command::SetJointRange {
                id: arm,
                axis,
                range,
            },
        )
        .unwrap();
        let link = result.robots[0]
            .links
            .iter()
            .find(|l| l.object_id == arm)
            .unwrap();
        assert_eq!(link.axes().nth(axis).unwrap().range, range);
        assert_eq!(link.angle(axis), expected);
    }
    c.project().validate().unwrap();
}
#[test]
fn invalid_ranges_and_stale_requests_do_not_change_state() {
    let mut c = human();
    let arm = id(&c, "left_upper_arm");
    let original = c.snapshot();
    for range in [
        [0., 0.],
        [1., -1.],
        [-4., 1.],
        [-1., 4.],
        [f32::NAN, 1.],
        [-1., f32::INFINITY],
    ] {
        assert!(
            edit(
                &mut c,
                Command::SetJointRange {
                    id: arm,
                    axis: 0,
                    range
                }
            )
            .is_err()
        );
        assert_eq!(c.snapshot().objects, original.objects);
        assert_eq!(c.snapshot().revision, original.revision);
    }
    for (id, axis) in [(9999, 0), (c.project().robots[0].root, 0), (arm, 3)] {
        assert!(
            edit(
                &mut c,
                Command::SetJointRange {
                    id,
                    axis,
                    range: [-0.5, 0.5]
                }
            )
            .is_err()
        );
    }
    assert!(
        c.apply(EditRequest {
            project_id: original.project_id,
            expected_revision: original.revision + 1,
            command: Command::SetJointRange {
                id: arm,
                axis: 0,
                range: [-0.5, 0.5]
            }
        })
        .is_err()
    );
    assert_eq!(c.snapshot().undo_depth, 0);
}
#[test]
fn range_drag_preview_restores_original_pose_on_expansion_and_cancel() {
    let mut c = human();
    let arm = id(&c, "left_upper_arm");
    edit(
        &mut c,
        Command::SetJointAngle {
            id: arm,
            axis: 0,
            angle: 0.8,
        },
    )
    .unwrap();
    let original = c.snapshot();
    let initial = original.robots[0]
        .links
        .iter()
        .find(|l| l.object_id == arm)
        .unwrap()
        .range;
    let token = c
        .begin_joint_edit(
            "native-joint",
            original.revision,
            original.project_id,
            arm,
            0,
            JointEdit::Upper,
        )
        .unwrap()
        .joint_preview
        .unwrap()
        .token;
    for upper in [0.7, 0.5, 0.3] {
        let s = c
            .preview_joint_range(
                "native-joint",
                token,
                original.revision,
                original.project_id,
                [initial[0], upper],
            )
            .unwrap();
        assert_eq!(s.undo_depth, original.undo_depth);
        assert_eq!(s.revision, original.revision);
        assert_eq!(
            s.robots[0]
                .links
                .iter()
                .find(|l| l.object_id == arm)
                .unwrap()
                .angle(0),
            upper
        );
    }
    c.preview_joint_range(
        "native-joint",
        token,
        original.revision,
        original.project_id,
        [initial[0], 1.],
    )
    .unwrap();
    assert_eq!(
        c.snapshot().robots[0]
            .links
            .iter()
            .find(|l| l.object_id == arm)
            .unwrap()
            .angle(0),
        0.8
    );
    assert!(
        c.preview_joint(
            "native-joint",
            token,
            original.revision,
            original.project_id,
            0.2
        )
        .is_err()
    );
    assert!(
        c.preview_joint_range(
            "scene",
            token,
            original.revision,
            original.project_id,
            [initial[0], 0.4]
        )
        .is_err()
    );
    assert!(
        c.preview_joint_range(
            "native-joint",
            token,
            original.revision,
            original.project_id,
            [-1., 0.4]
        )
        .is_err()
    );
    c.cancel_joint("native-joint", token).unwrap();
    assert_eq!(c.snapshot().objects, original.objects);
    assert_eq!(c.project().robots, original.robots);
    let next = c
        .begin_joint_edit(
            "native-joint",
            original.revision,
            original.project_id,
            arm,
            0,
            JointEdit::Upper,
        )
        .unwrap()
        .joint_preview
        .unwrap()
        .token;
    assert!(
        c.preview_joint_range(
            "native-joint",
            token,
            original.revision,
            original.project_id,
            [initial[0], 0.3]
        )
        .is_err()
    );
    c.preview_joint_range(
        "native-joint",
        next,
        original.revision,
        original.project_id,
        [initial[0], 0.3],
    )
    .unwrap();
    let preview = c.snapshot();
    c.commit_joint("native-joint", next, original.revision, original.project_id)
        .unwrap();
    assert_eq!(c.snapshot().objects, preview.objects);
    assert_eq!(c.snapshot().undo_depth, original.undo_depth + 1);
    edit(&mut c, Command::Undo).unwrap();
    assert_eq!(c.snapshot().objects, original.objects);
    assert_eq!(c.project().robots, original.robots);
}

#[test]
#[ignore = "requires Python with MuJoCo; verifies edited bounds and nonzero initial pose"]
fn edited_joint_ranges_export_and_run_in_mujoco() {
    let mut c = human();
    let arm = id(&c, "left_upper_arm");
    edit(
        &mut c,
        Command::SetJointRange {
            id: arm,
            axis: 0,
            range: [0.2, 0.5],
        },
    )
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("limits.xml");
    std::fs::write(&path, mujoco::export(c.project()).unwrap()).unwrap();
    let script = r#"
import mujoco,numpy as np,sys
m=mujoco.MjModel.from_xml_path(sys.argv[1]);d=mujoco.MjData(m)
j=m.joint('j'+sys.argv[2]+'_0');a=m.actuator('hold'+sys.argv[2]+'_0')
assert np.max(np.abs(j.range-np.array([0.2,0.5])))<1e-6
assert np.max(np.abs(a.ctrlrange-np.array([0.2,0.5])))<1e-6
k=mujoco.mj_name2id(m,mujoco.mjtObj.mjOBJ_KEY,'authored');assert k>=0
mujoco.mj_resetDataKeyframe(m,d,k)
assert abs(d.qpos[m.jnt_qposadr[j.id]]-0.2)<1e-6
assert abs(d.ctrl[a.id]-0.2)<1e-6
"#;
    let output = std::process::Command::new(
        std::env::var_os("ATOM_MUJOCO_PYTHON").unwrap_or_else(|| "python3".into()),
    )
    .args(["-I", "-c", script])
    .arg(path)
    .arg(arm.to_string())
    .output()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let authored = c.snapshot();
    let token = c
        .begin_simulation(authored.revision, authored.project_id)
        .unwrap();
    let result = mujoco::run(c.project(), 1).unwrap();
    c.finish_simulation(token, Ok(result), 1).unwrap();
    c.reset_simulation();
    assert_eq!(c.snapshot().objects, authored.objects);
    assert_eq!(c.project().robots, authored.robots);
}
