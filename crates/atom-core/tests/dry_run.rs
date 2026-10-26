use atom_core::*;
fn apply(core: &mut AppCore, command: Command) -> Snapshot {
    let s = core.snapshot();
    core.apply(EditRequest {
        project_id: s.project_id,
        expected_revision: s.revision,
        command,
    })
    .unwrap()
}
fn dry(core: &AppCore, command: Command) -> Result<DryRun> {
    let s = core.snapshot();
    core.dry_run(EditRequest {
        project_id: s.project_id,
        expected_revision: s.revision,
        command,
    })
}
fn bytes(core: &AppCore) -> Vec<u8> {
    serde_json::to_vec(&core.snapshot()).unwrap()
}
#[test]
fn dry_run_preserves_history_savepoint_and_next_identity() {
    let mut core = AppCore::new(Project::new("Dry QA").unwrap()).unwrap();
    apply(&mut core, Command::AddBox);
    let deleted = core.snapshot().objects.last().unwrap().id;
    apply(&mut core, Command::Undo);
    core.mark_saved();
    let before = bytes(&core);
    let report = dry(&core, Command::AddBox).unwrap();
    assert!(report.would_change);
    assert!(
        report
            .changes
            .iter()
            .any(|c| c.path == format!("/objects/{}", deleted + 1))
    );
    assert_eq!(bytes(&core), before);
    let redo = dry(&core, Command::Redo).unwrap();
    assert_eq!(redo.predicted_revision, core.snapshot().revision + 1);
    assert_eq!(bytes(&core), before);
    let predicted = dry(
        &core,
        Command::Rename {
            name: "New name".into(),
        },
    )
    .unwrap();
    let actual = apply(
        &mut core,
        Command::Rename {
            name: "New name".into(),
        },
    );
    assert_eq!(predicted.predicted_revision, actual.revision);
    assert_eq!(predicted.undo_depth_after, actual.undo_depth);
    apply(&mut core, Command::Undo);
    assert!(!core.snapshot().dirty);
    let actual = apply(&mut core, Command::AddBox);
    assert_eq!(actual.objects.last().unwrap().id, deleted + 1);
}
#[test]
fn joint_range_dry_run_predicts_clamp_atomically_and_rejects_busy() {
    let mut p = Project::new("Dry joint").unwrap();
    let (r, objects, _) = robots::build(RobotTemplate::DualArm, 1).unwrap();
    let joint = r.links[1].object_id;
    p.objects = objects;
    p.robots = vec![r];
    let mut core = AppCore::new(p).unwrap();
    apply(
        &mut core,
        Command::SetJointAngle {
            id: joint,
            axis: 0,
            angle: 0.8,
        },
    );
    let before = bytes(&core);
    let report = dry(
        &core,
        Command::SetJointRange {
            id: joint,
            axis: 0,
            range: [-0.2, 0.3],
        },
    )
    .unwrap();
    assert_eq!(bytes(&core), before);
    assert_eq!(report.changes.len(), 1);
    assert_eq!(
        report.changes[0].after["angles"][0],
        serde_json::json!(0.3f32)
    );
    assert_eq!(
        report.warnings,
        vec!["validation_only_no_physics_or_collision_test"]
    );
    let s = core.snapshot();
    core.begin_joint("gui", s.revision, s.project_id, joint, 0)
        .unwrap();
    let before = bytes(&core);
    assert!(matches!(dry(&core, Command::Undo), Err(Error::EditBusy)));
    assert_eq!(bytes(&core), before);
}
#[test]
fn invalid_or_noop_dry_run_never_changes_document_or_sequence() {
    let core = AppCore::new(Project::new("Dry QA").unwrap()).unwrap();
    let before = bytes(&core);
    assert!(
        !dry(
            &core,
            Command::Rename {
                name: " Dry QA ".into()
            }
        )
        .unwrap()
        .would_change
    );
    assert!(
        dry(
            &core,
            Command::SetGravity {
                gravity: Gravity {
                    preset: GravityPreset::Moon,
                    vector: [0.; 3]
                }
            }
        )
        .is_err()
    );
    assert!(dry(&core, Command::RemoveBox { id: u32::MAX }).is_err());
    assert!(matches!(
        core.dry_run(EditRequest {
            project_id: core.project().id,
            expected_revision: 99,
            command: Command::AddBox
        }),
        Err(Error::Conflict(_))
    ));
    assert_eq!(bytes(&core), before);
}
