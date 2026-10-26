use atom_control::*;
use atom_core::{AppCore, Command, EditRequest, Project};
use serde_json::{Value, json};
fn apply(c: &mut AppCore, command: Command) {
    let s = c.snapshot();
    c.apply(EditRequest {
        project_id: s.project_id,
        expected_revision: s.revision,
        command,
    })
    .unwrap();
}
fn guard(c: &AppCore) -> Guard {
    Guard::snapshot(&c.snapshot())
}
#[test]
fn same_live_state_unsaved_edits_dry_run_and_history() {
    let mut c = AppCore::new(Project::new("Shared").unwrap()).unwrap();
    let mut api = Service::default();
    apply(
        &mut c,
        Command::Rename {
            name: "Unsaved GUI edit".into(),
        },
    );
    let state = api
        .handle(&c, Request::new(Operation::State))
        .into_result()
        .unwrap();
    assert_eq!(state["name"], "Unsaved GUI edit");
    assert_eq!(state["dirty"], true);
    let before = serde_json::to_vec(&c.snapshot()).unwrap();
    let request = Request::new(Operation::DryRun {
        guard: guard(&c),
        command: json!({"kind":"rename","name":"Candidate only"}),
    });
    let response = api.handle(&c, request.clone());
    assert_eq!(
        response.clone().into_result().unwrap()["changes"][0]["after"],
        "Candidate only"
    );
    assert_eq!(before, serde_json::to_vec(&c.snapshot()).unwrap());
    let replay = api.handle(&c, request.clone());
    assert!(replay.replayed);
    assert_eq!(
        replay.into_result().unwrap(),
        response.into_result().unwrap()
    );
    let conflict = Request {
        operation: Operation::State,
        ..request
    };
    assert_eq!(
        api.handle(&c, conflict).into_result().unwrap_err().code,
        "request_conflict"
    );
    apply(&mut c, Command::Undo);
    assert_eq!(
        api.handle(&c, Request::new(Operation::State))
            .into_result()
            .unwrap()["name"],
        "Shared"
    );
}
#[test]
fn guards_pagination_validation_and_preview_modes() {
    let mut c = AppCore::new(Project::new("Pages").unwrap()).unwrap();
    apply(
        &mut c,
        Command::AddRobot {
            template: atom_core::RobotTemplate::Humanoid,
        },
    );
    let mut api = Service::default();
    let g = guard(&c);
    let a = api
        .handle(
            &c,
            Request::new(Operation::SceneList {
                guard: g.clone(),
                offset: 0,
                limit: 64,
            }),
        )
        .into_result()
        .unwrap();
    assert_eq!(a["objects"].as_array().unwrap().len(), 64);
    assert_eq!(a["next_offset"], 64);
    let b = api
        .handle(
            &c,
            Request::new(Operation::SceneList {
                guard: g.clone(),
                offset: 64,
                limit: 64,
            }),
        )
        .into_result()
        .unwrap();
    assert!(b["objects"].as_array().unwrap().len() < 64);
    assert!(b["next_offset"].is_null());
    assert_eq!(
        api.handle(
            &c,
            Request::new(Operation::SceneList {
                guard: g.clone(),
                offset: 0,
                limit: 65
            })
        )
        .into_result()
        .unwrap_err()
        .code,
        "invalid_cursor"
    );
    let mut seq = g.clone();
    seq.sequence += 1;
    assert_eq!(
        api.handle(
            &c,
            Request::new(Operation::DryRun {
                guard: seq,
                command: json!({"kind":"add_box"})
            })
        )
        .into_result()
        .unwrap_err()
        .code,
        "state_conflict"
    );
    apply(&mut c, Command::AddBox);
    assert_eq!(
        api.handle(
            &c,
            Request::new(Operation::SceneList {
                guard: g,
                offset: 0,
                limit: 1
            })
        )
        .into_result()
        .unwrap_err()
        .code,
        "revision_conflict"
    );
    let id = c.snapshot().objects[0].id;
    c.begin_transform("gui", c.snapshot().revision, c.project().id, id)
        .unwrap();
    let g = guard(&c);
    assert_eq!(
        api.handle(&c, Request::new(Operation::State))
            .into_result()
            .unwrap()["mode"],
        "transform_preview"
    );
    assert_eq!(
        api.handle(
            &c,
            Request::new(Operation::DryRun {
                guard: g,
                command: json!({"kind":"add_box"})
            })
        )
        .into_result()
        .unwrap_err()
        .code,
        "edit_busy"
    );
}
#[test]
fn schemas_and_unsupported_calls_are_explicit() {
    let c = AppCore::new(Project::new("Schemas").unwrap()).unwrap();
    let mut api = Service::default();
    let mut request = Request::new(Operation::State);
    request.api_version = 99;
    assert_eq!(
        api.handle(&c, request).into_result().unwrap_err().code,
        "unsupported_api_version"
    );
    for (command, expected) in [
        (json!({"kind":"simulate"}), "unsupported_operation"),
        (json!({"kind":"rename","name":22}), "invalid_arguments"),
        (
            json!({"kind":"set_gravity","gravity":{"preset":"moon","vector":[0,0,0]}}),
            "invalid_project",
        ),
    ] {
        assert_eq!(
            api.handle(
                &c,
                Request::new(Operation::DryRun {
                    guard: guard(&c),
                    command
                })
            )
            .into_result()
            .unwrap_err()
            .code,
            expected
        );
    }
    let tools = tools();
    assert_eq!(tools.len(), 5);
    assert!(
        tools
            .iter()
            .all(|t| t["annotations"]["readOnlyHint"] == true)
    );
    let choices = &tools[3]["inputSchema"]["properties"]["command"]["oneOf"];
    let names: Vec<_> = choices
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["properties"]["kind"]["const"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), COMMANDS.len());
    assert!(COMMANDS.iter().all(|name| names.contains(name)));
    let state = api
        .handle(&c, Request::new(Operation::State))
        .into_result()
        .unwrap();
    let encoded = state.to_string();
    assert!(!encoded.contains("positions"));
    assert!(!encoded.contains("workflow"));
    assert_eq!(capabilities()["write"], false);
    let _: Value = capabilities();
}
