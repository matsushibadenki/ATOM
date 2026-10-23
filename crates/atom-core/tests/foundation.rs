use atom_core::*;
fn request(core: &AppCore, command: Command) -> EditRequest {
    let s = core.snapshot();
    EditRequest {
        expected_revision: s.revision,
        project_id: s.project_id,
        command,
    }
}

#[test]
fn competing_windows_and_failed_edits_are_atomic() {
    let mut core = AppCore::new(Project::new("Lab").unwrap()).unwrap();
    let stale = request(
        &core,
        Command::Rename {
            name: "stale".into(),
        },
    );
    let add = request(&core, Command::AddBox);
    core.apply(add).unwrap();
    assert!(matches!(core.apply(stale), Err(Error::Conflict(1))));
    let bad = request(&core, Command::Rename { name: " ".into() });
    assert!(core.apply(bad).is_err());
    assert_eq!(core.snapshot().revision, 1);
    assert_eq!(core.snapshot().name, "Lab");
    assert_eq!(core.scene().instances().len(), 2);
    assert_eq!(core.scene().object(2).unwrap().position, [1.4, 0., 0.]);
    assert_eq!(core.scene().object(2).unwrap().scale, [0.7; 3]);
}

#[test]
fn save_roundtrip_lock_and_external_changes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lab.atom.json");
    let mut store = ProjectStore::open(&path).unwrap();
    let project = Project::new("研究室 / 实验室").unwrap();
    store.save(&project).unwrap();
    assert!(matches!(ProjectStore::open(&path), Err(Error::Locked)));
    drop(store);
    let mut store = ProjectStore::open(&path).unwrap();
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.id, project.id);
    assert_eq!(loaded.name, project.name);
    std::fs::write(&path, b"external edit").unwrap();
    assert!(matches!(store.save(&loaded), Err(Error::ExternalChange)));
    assert_eq!(std::fs::read(&path).unwrap(), b"external edit");
}

#[test]
fn invalid_files_and_future_versions_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lab.atom.json");
    let mut project = Project::new("Lab").unwrap();
    project.schema_version = 100;
    let bytes = serde_json::to_vec(&project).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        ProjectStore::open(&path).unwrap().load(),
        Err(Error::Version)
    ));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn reload_does_not_reuse_a_revision_or_discard_unsaved_changes() {
    let project = Project::new("Lab").unwrap();
    let mut core = AppCore::new(project.clone()).unwrap();
    let old = request(&core, Command::AddBox);
    core.reload(project.clone()).unwrap();
    assert!(matches!(core.apply(old), Err(Error::Conflict(1))));
    core.apply(request(&core, Command::AddBox)).unwrap();
    assert!(matches!(core.reload(project), Err(Error::Unsaved)));
}

#[test]
fn unge_executes_task_validation_without_claiming_simulation() {
    let project = Project::new("Lab").unwrap();
    let report = futures::executor::block_on(workflow::validate_tasks(&project.workflow)).unwrap();
    assert_eq!(report.completed_nodes, 2);
    assert_eq!(report.task_count, 8);
    assert_eq!(report.simulated_episodes, 0);
}

#[test]
fn non_finite_geometry_and_duplicate_ids_are_rejected() {
    let mut project = Project::new("Lab").unwrap();
    project.objects[0].scale[0] = f32::NAN;
    assert!(project.render_scene().is_err());
    project.objects[0].scale[0] = 1.;
    project.objects.push(project.objects[0].clone());
    assert!(project.validate().is_err());
}

#[test]
fn warm_material_preserves_saved_legacy_values_and_authored_custom_colors() {
    let mut project = Project::new("Material").unwrap();
    assert_eq!(project.objects[0].color, NEUTRAL_MATERIAL);
    project.objects[0].color = [0.47, 0.64, 1.];
    let bytes = serde_json::to_vec(&project).unwrap();
    let scene = project.render_scene().unwrap();
    assert_eq!(scene.object(1).unwrap().color, NEUTRAL_MATERIAL);
    assert_eq!(serde_json::to_vec(&project).unwrap(), bytes);
    project.objects[0].color = [0.8, 0.2, 0.1];
    assert_eq!(
        project.render_scene().unwrap().object(1).unwrap().color,
        [0.8, 0.2, 0.1]
    );
    let mut core = AppCore::new(project).unwrap();
    core.apply(request(&core, Command::AddBox)).unwrap();
    assert_eq!(core.project().objects[1].color, NEUTRAL_MATERIAL);
}
