use atom_core::*;
fn edit(core: &mut AppCore, command: Command) -> Result<Snapshot> {
    let s = core.snapshot();
    core.apply(EditRequest {
        expected_revision: s.revision,
        project_id: s.project_id,
        command,
    })
}
#[test]
fn transform_preview_commits_as_one_history_step_and_undo_restores_saved_point() {
    let mut core = AppCore::new(Project::new("History").unwrap()).unwrap();
    let s = core.snapshot();
    let original = core.project().objects[0].clone();
    core.begin_transform("viewport", s.revision, s.project_id, 1)
        .unwrap();
    for x in [1., 2., 3.] {
        let mut pose = Transform::from(&original);
        pose.position[0] = x;
        pose.rotation[1] = 0.8;
        pose.scale = [1.5, 2., 0.5];
        core.preview_transform("viewport", 0, s.project_id, pose)
            .unwrap();
    }
    assert_eq!(core.snapshot().revision, 0);
    assert_eq!(core.snapshot().undo_depth, 0);
    assert!(core.snapshot().previewing);
    assert_eq!(core.project().objects[0], original);
    assert_eq!(core.scene().object(1).unwrap().position, [3., 1., 0.]);
    let committed = core.commit_transform("viewport", 0, s.project_id).unwrap();
    assert_eq!(committed.undo_depth, 1);
    assert!(committed.dirty);
    assert!(!committed.previewing);
    let undo = edit(&mut core, Command::Undo).unwrap();
    assert_eq!(undo.objects[0], original);
    assert!(!undo.dirty);
    assert!(undo.can_redo);
    assert_eq!(undo.revision, 2);
    let redo = edit(&mut core, Command::Redo).unwrap();
    assert_eq!(redo.objects, committed.objects);
    assert!(redo.dirty);
    assert_eq!(core.scene().object(1).unwrap().scale, [1.5, 2., 0.5]);
}
#[test]
fn preview_ownership_invalid_values_stale_requests_and_cancel_are_atomic() {
    let mut core = AppCore::new(Project::new("History").unwrap()).unwrap();
    let s = core.snapshot();
    let mut pose = Transform::from(&s.objects[0]);
    core.begin_transform("scene", 0, s.project_id, 1).unwrap();
    let sequence = core.snapshot().sequence;
    pose.scale[0] = 0.;
    assert!(
        core.preview_transform("scene", 0, s.project_id, pose.clone())
            .is_err()
    );
    assert_eq!(core.snapshot().sequence, sequence);
    pose.scale[0] = 1.;
    assert!(matches!(
        core.preview_transform("other", 0, s.project_id, pose.clone()),
        Err(Error::EditBusy)
    ));
    assert!(matches!(
        core.preview_transform("scene", 1, s.project_id, pose.clone()),
        Err(Error::Conflict(0))
    ));
    assert!(matches!(
        edit(&mut core, Command::AddBox),
        Err(Error::EditBusy)
    ));
    assert!(core.cancel_transform("other").is_err());
    pose.position[1] = 4.;
    core.preview_transform("scene", 0, s.project_id, pose)
        .unwrap();
    let canceled = core.cancel_transform("scene").unwrap();
    assert_eq!(canceled.objects, s.objects);
    assert!(!canceled.can_undo);
    assert_eq!(canceled.revision, 0);
    assert!(!canceled.dirty);
    assert!(canceled.sequence > sequence);
    core.begin_transform("scene", 0, s.project_id, 1).unwrap();
    let no_op = core.commit_transform("scene", 0, s.project_id).unwrap();
    assert_eq!(no_op.revision, 0);
    assert!(!no_op.can_undo);
    assert!(!no_op.previewing);
}
#[test]
fn undo_redo_saved_identity_and_redo_branches_are_correct() {
    let mut core = AppCore::new(Project::new("History").unwrap()).unwrap();
    edit(&mut core, Command::Rename { name: "A".into() }).unwrap();
    core.mark_saved();
    edit(&mut core, Command::Rename { name: "B".into() }).unwrap();
    assert!(core.snapshot().dirty);
    assert!(!edit(&mut core, Command::Undo).unwrap().dirty);
    assert!(edit(&mut core, Command::Redo).unwrap().dirty);
    core.mark_saved();
    assert!(edit(&mut core, Command::Undo).unwrap().dirty);
    assert!(!edit(&mut core, Command::Redo).unwrap().dirty);
    edit(&mut core, Command::Undo).unwrap();
    edit(&mut core, Command::Rename { name: "C".into() }).unwrap();
    assert!(!core.snapshot().can_redo);
    assert!(core.snapshot().dirty);
    let revision = core.snapshot().revision;
    assert!(matches!(
        edit(&mut core, Command::Redo),
        Err(Error::HistoryEmpty)
    ));
    assert_eq!(core.snapshot().revision, revision);
}
#[test]
fn add_delete_transform_and_reload_keep_history_and_entity_identity_consistent() {
    let mut core = AppCore::new(Project::new("History").unwrap()).unwrap();
    edit(&mut core, Command::AddBox).unwrap();
    let added = core.snapshot().objects[1].clone();
    edit(&mut core, Command::RemoveBox { id: added.id }).unwrap();
    assert_eq!(edit(&mut core, Command::Undo).unwrap().objects[1], added);
    edit(&mut core, Command::Undo).unwrap();
    let newer = edit(&mut core, Command::AddBox).unwrap();
    assert!(newer.objects[1].id > added.id);
    core.mark_saved();
    let project = core.project().clone();
    let revision = core.snapshot().revision;
    core.reload(project).unwrap();
    assert!(core.snapshot().revision > revision);
    assert!(!core.snapshot().can_undo);
    assert!(!core.snapshot().can_redo);
    assert!(!core.snapshot().dirty);
}
#[test]
fn history_is_bounded_and_invalid_transform_does_not_consume_redo() {
    let mut core = AppCore::new(Project::new("History").unwrap()).unwrap();
    for x in 1..=140 {
        let mut transform = Transform::from(&core.project().objects[0]);
        transform.position[0] = x as f32;
        edit(&mut core, Command::Transform { id: 1, transform }).unwrap();
    }
    assert_eq!(core.snapshot().undo_depth, 128);
    for _ in 0..128 {
        edit(&mut core, Command::Undo).unwrap();
    }
    assert_eq!(core.project().objects[0].position[0], 12.);
    assert!(!core.snapshot().can_undo);
    let before = core.snapshot();
    let mut transform = Transform::from(&before.objects[0]);
    transform.position[0] = f32::NAN;
    assert!(edit(&mut core, Command::Transform { id: 1, transform }).is_err());
    assert_eq!(core.snapshot().sequence, before.sequence);
    assert_eq!(core.snapshot().redo_depth, 128);
    for _ in 0..128 {
        edit(&mut core, Command::Redo).unwrap();
    }
    assert_eq!(core.project().objects[0].position[0], 140.);
}
