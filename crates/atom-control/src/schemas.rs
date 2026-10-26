use super::*;
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
pub fn tools() -> Vec<Value> {
    let guard = object(
        json!({"project_id":{"type":"string","format":"uuid"},"revision":{"type":"integer","minimum":0},"sequence":{"type":"integer","minimum":0}}),
        &["project_id", "revision", "sequence"],
    );
    let empty = object(json!({}), &[]);
    [
        ("atom_capabilities","Capabilities / 機能一覧 / 功能列表","Read implemented capabilities and limits. No simulation or training probe.",empty.clone()),
        ("atom_project_get","Shared state / 共有状態 / 共享状态","Read the running desktop's live Rust state, including unsaved edits and preview mode. No mesh buffers or local paths.",empty.clone()),
        ("atom_scene_list","Scene objects / 物体一覧 / 物体列表","Read a bounded page of displayed world-space objects. Use the exact project/revision/sequence guard from atom_project_get; refresh it when the scene changes.",object(json!({"guard":guard,"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":64}}),&["guard","offset","limit"])),
        ("atom_edit_dry_run","Edit dry-run / 変更の事前検証 / 修改预检","Validate one GUI editor command against the current authored document and history. Returns a bounded authored-rest diff; never changes state, history or files. Rejects while a preview/job is active. Does not test physical stability or collisions.",object(json!({"guard":guard,"command":command_schema()}),&["guard","command"])),
        ("atom_diagnose","Connection diagnosis / 接続診断 / 连接诊断","Check descriptor, local transport/authentication, API version and shared-state access. Reports not_running without launching another project or probing physics/LLM runtimes.",empty)
    ].into_iter().map(|(name,title,description,input)|json!({"name":name,"title":title,"description":description,"inputSchema":input,"outputSchema":output_schema(name),"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})).collect()
}
fn output_schema(name: &str) -> Value {
    let error = object(
        json!({"code":{"type":"string"},"retryable":{"type":"boolean"}}),
        &["code", "retryable"],
    );
    let local_error = object(
        json!({"status":{"const":"error"},"error":error}),
        &["status", "error"],
    );
    let report = if name == "atom_diagnose" {
        object(
            json!({"connected":{"type":"boolean"},"checks":{"type":"array","items":object(json!({"stage":{"type":"string"},"ok":{"type":"boolean"},"code":{"type":"string"}}),&["stage","ok","code"])},"instance_id":{"type":["string","null"]},"state":{"type":["object","null"]}}),
            &["connected", "checks", "instance_id", "state"],
        )
    } else {
        let guard = object(
            json!({"project_id":{"type":"string","format":"uuid"},"revision":{"type":"integer","minimum":0},"sequence":{"type":"integer","minimum":0}}),
            &["project_id", "revision", "sequence"],
        );
        let properties = json!({"api_version":{"const":API_VERSION},"request_id":{"type":"string","format":"uuid"},"basis":{"anyOf":[guard,{"type":"null"}]},"replayed":{"type":"boolean"},"status":{"enum":["ok","error"]},"data":{"type":"object"},"error":error});
        let mut report = object(
            properties,
            &["api_version", "request_id", "basis", "replayed", "status"],
        );
        report["oneOf"] = json!([{"properties":{"status":{"const":"ok"}},"required":["data"],"not":{"required":["error"]}},{"properties":{"status":{"const":"error"}},"required":["error"],"not":{"required":["data"]}}]);
        report
    };
    json!({"type":"object","oneOf":[report,local_error]})
}
fn number() -> Value {
    json!({"type":"number"})
}
fn vec3() -> Value {
    json!({"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3})
}
fn id() -> Value {
    json!({"type":"integer","minimum":1,"maximum":4294967295u64})
}
fn variant(kind: &str, properties: Value, required: &[&str]) -> Value {
    let mut props = properties.as_object().unwrap().clone();
    props.insert("kind".into(), json!({"const":kind}));
    let mut req = vec!["kind"];
    req.extend_from_slice(required);
    object(Value::Object(props), &req)
}
pub fn command_schema() -> Value {
    let material = object(
        json!({"preset":{"enum":["wood","steel","aluminum","rubber","plastic","glass","concrete","ice","custom"]},"density":number(),"friction":vec3()}),
        &["preset", "density", "friction"],
    );
    let transform = object(
        json!({"position":vec3(),"rotation":vec3(),"scale":vec3()}),
        &["position", "rotation", "scale"],
    );
    let variants = vec![
        variant("rename", json!({"name":{"type":"string"}}), &["name"]),
        variant("add_box", json!({}), &[]),
        variant("remove_box", json!({"id":id()}), &["id"]),
        variant(
            "transform",
            json!({"id":id(),"transform":transform}),
            &["id", "transform"],
        ),
        variant(
            "set_gravity",
            json!({"gravity":object(json!({"preset":{"enum":["earth","moon","mars","zero","custom"]},"vector":vec3()}),&["preset","vector"])}),
            &["gravity"],
        ),
        variant(
            "set_material",
            json!({"id":{"anyOf":[id(),{"type":"null"}]},"material":material}),
            &["id", "material"],
        ),
        variant(
            "set_fixed",
            json!({"id":id(),"fixed":{"type":"boolean"}}),
            &["id", "fixed"],
        ),
        variant(
            "set_floor",
            json!({"enabled":{"type":"boolean"}}),
            &["enabled"],
        ),
        variant(
            "set_joint_angle",
            json!({"id":id(),"axis":{"type":"integer","minimum":0,"maximum":2},"angle":number()}),
            &["id", "axis", "angle"],
        ),
        variant(
            "set_joint_range",
            json!({"id":id(),"axis":{"type":"integer","minimum":0,"maximum":2},"range":{"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2}}),
            &["id", "axis", "range"],
        ),
        variant(
            "solve_ik",
            json!({"id":id(),"base":id(),"target":vec3()}),
            &["id", "base", "target"],
        ),
        variant(
            "add_robot",
            json!({"template":{"enum":["humanoid","dual_arm"]}}),
            &["template"],
        ),
        variant("undo", json!({}), &[]),
        variant("redo", json!({}), &[]),
    ];
    json!({"oneOf":variants})
}

/// Public request body; authentication/discovery is deliberately not exported.
pub fn request_schema() -> Value {
    let guard = object(
        json!({"project_id":{"type":"string","format":"uuid"},"revision":{"type":"integer","minimum":0},"sequence":{"type":"integer","minimum":0}}),
        &["project_id", "revision", "sequence"],
    );
    let mut operations = vec![];
    for (name, properties, required) in [
        ("capabilities", json!({}), vec![]),
        ("state", json!({}), vec![]),
        ("diagnose", json!({}), vec![]),
        (
            "scene_list",
            json!({"guard":guard,"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":64}}),
            vec!["guard", "offset", "limit"],
        ),
        (
            "dry_run",
            json!({"guard":guard,"command":command_schema()}),
            vec!["guard", "command"],
        ),
    ] {
        let mut fields = properties.as_object().unwrap().clone();
        fields.insert("method".into(), json!({"const":name}));
        let mut keys = vec!["method"];
        keys.extend(required);
        operations.push(object(Value::Object(fields), &keys));
    }
    let mut schema = object(
        json!({"api_version":{"const":API_VERSION},"request_id":{"type":"string","format":"uuid"},"operation":{"oneOf":operations}}),
        &["api_version", "request_id", "operation"],
    );
    schema["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
    schema["$id"] = json!("urn:atom:control:v1:request");
    schema
}
