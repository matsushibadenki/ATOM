use atom_core::{planning::*, *};
use serde_json::json;
fn edit(core: &mut AppCore, command: Command) {
    let s = core.snapshot();
    core.apply(EditRequest {
        project_id: s.project_id,
        expected_revision: s.revision,
        command,
    })
    .unwrap();
}
fn bytes(core: &AppCore) -> Vec<u8> {
    serde_json::to_vec(&core.snapshot()).unwrap()
}
fn text(steps: serde_json::Value) -> String {
    json!({"schema_version":1,"summary":"Candidate only","steps":steps}).to_string()
}
#[test]
fn bounded_summary_prioritizes_focus_omits_names_and_uses_live_unsaved_pose() {
    let mut p = Project::new("private project /do/not/send").unwrap();
    p.objects[0].label = "private object /do/not/send".into();
    let mut core = AppCore::new(p).unwrap();
    for _ in 0..20 {
        edit(&mut core, Command::AddBox)
    }
    let id = core.project().objects.last().unwrap().id;
    let mut transform = Transform::from(core.project().objects.last().unwrap());
    transform.position = [1., 2., 3.];
    edit(&mut core, Command::Transform { id, transform });
    let before = bytes(&core);
    let s = core.planning_summary(Some(id)).unwrap();
    let data = serde_json::to_vec(&s).unwrap();
    assert!(data.len() <= MAX_SCENE_BYTES);
    assert!(s.objects.len() <= MAX_SCENE_OBJECTS);
    assert_eq!(s.objects[0].id, id);
    assert_eq!(s.objects[0].position, [1., 2., 3.]);
    assert!(s.omitted_objects > 0);
    assert_eq!(s.total_objects, s.omitted_objects + s.objects.len());
    assert!(!String::from_utf8(data.clone()).unwrap().contains("private"));
    assert_eq!(
        data,
        serde_json::to_vec(&core.planning_summary(Some(id)).unwrap()).unwrap()
    );
    assert_eq!(before, bytes(&core));
    assert!(core.planning_context(Some(u32::MAX)).is_err());
}
#[test]
fn sequential_dry_run_preserves_document_history_and_applies_dependencies_on_a_fork() {
    let mut core = AppCore::new(Project::new("unsaved").unwrap()).unwrap();
    edit(&mut core, Command::AddBox);
    edit(&mut core, Command::Undo);
    core.mark_saved();
    let id = core.project().objects[0].id;
    let before = bytes(&core);
    let context = core.planning_context(Some(id)).unwrap();
    let report=context.validate(&text(json!([{"skill":"move_object","id":id,"position":[1.,2.,3.]},{"skill":"set_material","id":id,"preset":"rubber"},{"skill":"move_object","id":id,"position":[4.,5.,6.]}])));
    assert!(report.valid);
    assert_eq!(report.steps.len(), 3);
    let d = report.dry_run.unwrap();
    assert_eq!(d.predicted_revision, core.snapshot().revision + 3);
    assert_eq!(d.undo_depth_after, core.snapshot().undo_depth + 3);
    assert_eq!(d.redo_depth_after, 0);
    let c = d
        .changes
        .iter()
        .find(|c| c.path == format!("/objects/{id}"))
        .unwrap();
    assert_eq!(c.after["position"], json!([4., 5., 6.]));
    assert_eq!(c.after["material"]["preset"], "rubber");
    assert_eq!(before, bytes(&core));
}
#[test]
fn invalid_late_step_returns_no_partial_diff_and_strict_schema_rejects_unlisted_commands() {
    let core = AppCore::new(Project::new("Safe").unwrap()).unwrap();
    let id = core.project().objects[0].id;
    let c = core.planning_context(None).unwrap();
    let before = bytes(&core);
    let r=c.validate(&text(json!([{"skill":"set_gravity","preset":"moon"},{"skill":"move_object","id":99999,"position":[0,0,0]}])));
    assert!(!r.valid);
    assert_eq!(r.issues[0].step, Some(1));
    assert!(r.steps.is_empty() && r.dry_run.is_none());
    for bad in [
        text(json!([])),
        text(json!([{"skill":"undo"}])),
        text(json!([{"skill":"move_object","id":id,"position":[0,0,0],"shell":"oops"}])),
        "```json\n{}\n```".into(),
        "{} {}".into(),
        text(json!([{"skill":"set_gravity","preset":"custom"}])),
    ] {
        assert!(!c.validate(&bad).valid);
    }
    assert!(!c.validate(&"x".repeat(MAX_PLAN_BYTES + 1)).valid);
    assert_eq!(before, bytes(&core));
}
#[test]
fn joint_limits_pose_context_scoping_and_busy_stale_guards_are_enforced() {
    let p: Project =
        serde_json::from_str(include_str!("../../../templates/dual-arm.atom.json")).unwrap();
    let mut core = AppCore::new(p).unwrap();
    let link = core.project().robots[0].links[1].object_id;
    edit(
        &mut core,
        Command::SetJointAngle {
            id: link,
            axis: 0,
            angle: 0.3,
        },
    );
    let c = core.planning_context(Some(link)).unwrap();
    assert_eq!(
        c.summary.objects[0].position,
        core.snapshot()
            .objects
            .iter()
            .find(|o| o.id == link)
            .unwrap()
            .position
    );
    assert_eq!(
        c.summary.objects[0].joint.as_ref().unwrap().axes[0].angle,
        0.3
    );
    let before = bytes(&core);
    let r = c.validate(&text(
        json!([{"skill":"set_joint_angle","id":link,"axis":0,"angle":100}]),
    ));
    assert_eq!(r.issues[0].code, "plan_joint_limit");
    assert!(r.dry_run.is_none());
    assert_eq!(before, bytes(&core));
    let basis = c.summary.basis.clone();
    assert!(core.planning_matches(&basis));
    edit(&mut core, Command::SetFloor { enabled: false });
    assert!(!core.planning_matches(&basis));
    assert!(core.planning_context_at(&basis, Some(link)).is_err());
    let now = Basis::snapshot(&core.snapshot());
    let s = core.snapshot();
    core.begin_joint("gui", s.revision, s.project_id, link, 0)
        .unwrap();
    assert!(!core.planning_matches(&now));
    assert!(matches!(
        core.planning_summary(Some(link)),
        Err(Error::EditBusy)
    ));
}
#[test]
fn gravity_material_and_floor_skills_use_canonical_gui_commands() {
    let core = AppCore::new(Project::new("Physics").unwrap()).unwrap();
    let id = core.project().objects[0].id;
    let c = core.planning_context(None).unwrap();
    let r=c.validate(&text(json!([{"skill":"set_gravity","preset":"moon"},{"skill":"set_material","id":id,"preset":"ice"},{"skill":"set_floor","enabled":false}])));
    assert!(r.valid);
    let d = r.dry_run.unwrap();
    let physics = &d
        .changes
        .iter()
        .find(|c| c.path == "/physics")
        .unwrap()
        .after;
    assert_eq!(physics["gravity"]["vector"], json!([0., 0., -1.62]));
    assert_eq!(physics["floor_enabled"], false);
    assert_eq!(core.snapshot().physics.gravity.preset, GravityPreset::Earth);
}

#[test]
fn ik_is_allowed_only_for_a_fully_scoped_chain_and_context_survives_later_edits() {
    let p: Project =
        serde_json::from_str(include_str!("../../../templates/dual-arm.atom.json")).unwrap();
    let mut core = AppCore::new(p).unwrap();
    let link = core.project().robots[0].links[1].object_id;
    let base = core.project().robots[0].root;
    let c = core.planning_context(Some(link)).unwrap();
    let target = c.summary.objects[0].position;
    let raw = text(json!([{"skill":"solve_ik","id":link,"base":base,"target":target}]));
    let r = c.validate(&raw);
    assert!(r.valid, "{:?}", r.issues);
    assert!(!r.dry_run.unwrap().would_change);
    edit(
        &mut core,
        Command::Rename {
            name: "Concurrent edit".into(),
        },
    );
    let before = bytes(&core);
    let r = c.validate(&raw);
    assert!(r.valid);
    assert!(!core.planning_matches(&c.summary.basis));
    assert_eq!(r.dry_run.unwrap().revision, c.summary.basis.revision);
    assert_eq!(before, bytes(&core));
    let too_many = text(
        json!([{"skill":"solve_ik","id":link,"base":base,"target":target},{"skill":"solve_ik","id":link,"base":base,"target":target}]),
    );
    assert_eq!(c.validate(&too_many).issues[0].code, "plan_ik_budget");
    let invalid = "{\"schema_version\":1,\"schema_version\":1,\"summary\":\"x\",\"steps\":[]}";
    assert_eq!(c.validate(invalid).issues[0].code, "plan_invalid_json");
}
