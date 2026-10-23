use atom_core::{assets::*, *};
use std::sync::Arc;
fn mesh() -> TriangleMesh {
    TriangleMesh {
        positions: vec![
            [-0.5, -0.5, -0.5],
            [0.5, -0.5, -0.5],
            [0., 0.5, -0.5],
            [0., 0., 0.5],
        ],
        colors: vec![[0.8, 0.3, 0.1]; 4],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    }
}
fn asset() -> ImportedAsset {
    let mesh = mesh();
    let hash = mesh.digest();
    ImportedAsset {
        id: Uuid::new_v4(),
        name: "Tetra".into(),
        source: AssetSource {
            kind: "local".into(),
            repo: String::new(),
            revision: String::new(),
            package: String::new(),
            entry: "test.usda".into(),
            license: "test".into(),
            author: "ATOM".into(),
            source_hash: "0".repeat(64),
            cache_path: String::new(),
            up_axis: "Z".into(),
            meters_per_unit: 1.,
            prim_paths: vec!["/Tetra".into()],
            sdk_version: "test".into(),
        },
        mesh: Arc::new(mesh),
        mesh_hash: hash,
        warnings: vec!["static_mesh_import".into()],
    }
}
fn edit(core: &mut AppCore, command: Command) -> Result<Snapshot> {
    let s = core.snapshot();
    core.apply(EditRequest {
        expected_revision: s.revision,
        project_id: s.project_id,
        command,
    })
}
fn import(core: &mut AppCore) -> Snapshot {
    edit(
        core,
        Command::ImportAsset {
            asset: Box::new(asset()),
            position: [2., 3., 4.],
            scale: [1., 2., 3.],
            mass: Some(1.5),
        },
    )
    .unwrap()
}
#[test]
fn imported_mesh_is_portable_undoable_and_absent_from_ui_payload() {
    let mut core = AppCore::new(Project::new("Assets").unwrap()).unwrap();
    let imported = import(&mut core);
    let id = imported.objects.last().unwrap().id;
    assert!(imported.objects.last().unwrap().fixed);
    assert_eq!(imported.assets[0].triangles, 4);
    let bytes = serde_json::to_string(&imported).unwrap();
    assert!(!bytes.contains("positions"));
    assert!(!bytes.contains("indices"));
    let temp = tempfile::tempdir().unwrap();
    let mut store = ProjectStore::open(temp.path().join("mesh.json")).unwrap();
    store.save(core.project()).unwrap();
    core.mark_saved();
    edit(&mut core, Command::Undo).unwrap();
    assert!(core.project().assets.is_empty());
    edit(&mut core, Command::Redo).unwrap();
    assert!(!core.snapshot().dirty);
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.assets, core.project().assets);
    assert_eq!(loaded.objects.last().unwrap().mass_override, Some(1.5));
    edit(&mut core, Command::RemoveBox { id }).unwrap();
    assert!(core.project().assets.is_empty());
    edit(&mut core, Command::Undo).unwrap();
    assert_eq!(core.project().assets.len(), 1);
    edit(
        &mut core,
        Command::SetMaterial {
            id: Some(id),
            material: PhysicalMaterial::preset(MaterialPreset::Rubber),
        },
    )
    .unwrap();
    assert!(
        core.project()
            .objects
            .last()
            .unwrap()
            .mass_override
            .is_none()
    );
    edit(&mut core, Command::Undo).unwrap();
    assert_eq!(
        core.project().objects.last().unwrap().mass_override,
        Some(1.5)
    );
}
#[test]
fn malformed_meshes_hashes_and_references_do_not_commit() {
    let mut core = AppCore::new(Project::new("Assets").unwrap()).unwrap();
    for mutation in 0..4 {
        let mut a = asset();
        match mutation {
            0 => Arc::make_mut(&mut a.mesh).indices[0] = 99,
            1 => Arc::make_mut(&mut a.mesh).positions[0][0] = f32::NAN,
            2 => a.mesh_hash = "a".repeat(64),
            _ => Arc::make_mut(&mut a.mesh).colors.pop().map(|_| ()).unwrap(),
        }
        assert!(
            edit(
                &mut core,
                Command::ImportAsset {
                    asset: Box::new(a),
                    position: [0.; 3],
                    scale: [1.; 3],
                    mass: None
                }
            )
            .is_err()
        );
    }
    assert_eq!(core.snapshot().revision, 0);
    assert_eq!(core.snapshot().undo_depth, 0);
    let mut project = core.project().clone();
    project.objects[0].mesh_ref = Some(Uuid::new_v4());
    assert!(project.validate().is_err());
}
#[test]
fn mesh_import_is_internal_not_a_large_webview_command() {
    assert!(serde_json::from_str::<Command>(r#"{"kind":"import_asset","asset":{}}"#).is_err());
}
#[test]
fn v2_projects_migrate_with_no_asset_references() {
    let mut old = serde_json::to_value(Project::new("V2").unwrap()).unwrap();
    old["schema_version"] = 2.into();
    old.as_object_mut().unwrap().remove("assets");
    for o in old["objects"].as_array_mut().unwrap() {
        o.as_object_mut().unwrap().remove("mesh_ref");
        o.as_object_mut().unwrap().remove("mass_override");
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("old.json");
    std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    let loaded = ProjectStore::open(path).unwrap().load().unwrap().unwrap();
    assert_eq!(loaded.schema_version, SCHEMA_VERSION);
    assert!(loaded.assets.is_empty());
}
fn fixture(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/usd")
        .join(name)
        .to_string_lossy()
        .into()
}
#[test]
#[ignore = "requires official OpenUSD SDK; run --ignored"]
fn live_usd_composition_units_copy_and_missing_dependency_failures() {
    let temp = tempfile::tempdir().unwrap();
    let command = prepare(
        ImportSource::Local {
            path: fixture("assembly.usda"),
        },
        temp.path(),
    )
    .unwrap();
    let mut project = Project::new("USD").unwrap();
    project.objects.clear();
    let mut core = AppCore::new(project).unwrap();
    let s = edit(&mut core, command).unwrap();
    let o = &s.objects[0];
    assert!((o.position[0] - 3.5).abs() < 1e-6);
    assert!((o.position[1] - 4.5).abs() < 1e-6);
    assert!((o.position[2] + 3.5).abs() < 1e-6);
    assert_eq!(o.scale, [3., 1., 1.]);
    assert_eq!(s.assets[0].triangles, 8);
    assert_eq!(s.assets[0].source.up_axis, "Z");
    let cached = std::path::Path::new(&s.assets[0].source.cache_path);
    assert!(cached.join("component.usda").is_file());
    assert!(cached.join("assembly.usda").is_file());
    for name in ["missing.usda", "escape.usda"] {
        assert!(
            matches!(prepare(ImportSource::Local{path:fixture(name)},temp.path()),Err(Error::Asset(code)) if code=="usd_dependency")
        );
    }
}
#[test]
#[ignore = "requires official OpenUSD SDK; run --ignored"]
fn live_concave_polygon_is_triangulated_without_filling_the_notch() {
    let temp = tempfile::tempdir().unwrap();
    let command = prepare(
        ImportSource::Local {
            path: fixture("concave.usda"),
        },
        temp.path(),
    )
    .unwrap();
    let Command::ImportAsset { asset, scale, .. } = command else {
        panic!("import")
    };
    assert_eq!(asset.mesh.indices.len(), 9);
    let mut area = 0.;
    for face in asset.mesh.indices.chunks(3) {
        let p = face
            .iter()
            .map(|&i| asset.mesh.positions[i as usize])
            .collect::<Vec<_>>();
        area += ((p[1][0] - p[0][0]) * (p[2][2] - p[0][2])
            - (p[1][2] - p[0][2]) * (p[2][0] - p[0][0]))
            .abs()
            * 0.5
            * scale[0]
            * scale[2];
    }
    assert!((area - 3.).abs() < 1e-6);
}
#[test]
#[ignore = "requires MuJoCo; run --ignored"]
fn live_mesh_collision_uses_geometry_and_explicit_mass() {
    let mut project = Project::new("Mesh physics").unwrap();
    project.objects.clear();
    let mut core = AppCore::new(project).unwrap();
    import(&mut core);
    let id = core.snapshot().objects[0].id;
    let before = core.project().objects[0].clone();
    let fixed = mujoco::run(core.project(), 1).unwrap();
    assert_eq!(fixed.poses[0].position, before.position);
    assert_eq!(fixed.poses[0].rotation, before.rotation);
    edit(&mut core, Command::SetFixed { id, fixed: false }).unwrap();
    let result = mujoco::run(core.project(), 100).unwrap();
    assert!(result.poses[0].position[1] < 3.);
    assert_eq!(result.poses.len(), 1);
    let xml = mujoco::export(core.project()).unwrap();
    assert!(xml.contains("type=\"mesh\""));
    assert!(xml.contains("mass=\"1.5\""));
}

#[test]
fn importing_reserves_the_shared_document_and_failure_releases_it() {
    let mut core = AppCore::new(Project::new("Import lock").unwrap()).unwrap();
    let s = core.snapshot();
    let token = core.begin_import(s.revision, s.project_id).unwrap();
    assert!(core.snapshot().asset_importing);
    assert!(edit(&mut core, Command::AddBox).is_err());
    assert!(
        core.begin_transform("scene", s.revision, s.project_id, 1)
            .is_err()
    );
    assert!(
        core.finish_import(token, Err(Error::Asset("usd_cancelled".into())))
            .is_err()
    );
    assert!(!core.has_preview());
    assert_eq!(core.snapshot().undo_depth, 0);
    let s = core.snapshot();
    let token = core.begin_import(s.revision, s.project_id).unwrap();
    let command = Command::ImportAsset {
        asset: Box::new(asset()),
        position: [0., 1., 0.],
        scale: [1.; 3],
        mass: None,
    };
    core.finish_import(token, Ok(command)).unwrap();
    assert_eq!(core.snapshot().undo_depth, 1);
    assert!(!core.snapshot().asset_importing);
}
#[test]
#[ignore = "requires OpenUSD adapter Python; verifies cancellation kills its worker"]
fn live_cancelled_import_returns_without_editing_or_waiting_for_download() {
    let cancel = std::sync::atomic::AtomicBool::new(true);
    let temp = tempfile::tempdir().unwrap();
    let result = prepare_cancellable(
        ImportSource::Local {
            path: fixture("assembly.usda"),
        },
        temp.path(),
        Some(&cancel),
    );
    assert!(matches!(result,Err(Error::Asset(code)) if code=="usd_cancelled"));
}
