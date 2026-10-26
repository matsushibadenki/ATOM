#![cfg(unix)]
use atom_control::{
    Operation, Request, Service,
    ipc::{Client, Server},
};
use atom_core::{AppCore, Command, EditRequest, Project};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command as Process, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
struct Bridge {
    child: Child,
    input: ChildStdin,
    replies: mpsc::Receiver<Value>,
}
impl Bridge {
    fn start(path: &std::path::Path) -> Self {
        let mut child = Process::new(env!("CARGO_BIN_EXE_atom-mcp"))
            .arg("--project")
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let line = line.unwrap();
                let value: Value =
                    serde_json::from_str(&line).expect("stdout must contain only JSON-RPC");
                if tx.send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input,
            replies: rx,
        }
    }
    fn send(&mut self, value: Value) {
        serde_json::to_writer(&mut self.input, &value).unwrap();
        self.input.write_all(b"\n").unwrap();
        self.input.flush().unwrap();
    }
    fn recv(&self) -> Value {
        self.replies
            .recv_timeout(Duration::from_secs(5))
            .expect("MCP reply deadline")
    }
    fn legacy(&mut self) {
        self.send(json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":atom_mcp::LEGACY,"capabilities":{},"clientInfo":{"name":"ATOM test client","version":"1"}}}));
        assert_eq!(self.recv()["result"]["protocolVersion"], atom_mcp::LEGACY);
        self.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    }
    fn rpc(&mut self, id: u32, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        let r = self.recv();
        assert_eq!(r["id"], id);
        r
    }
    fn tool(&mut self, id: u32, name: &str, arguments: Value) -> Value {
        self.rpc(id, "tools/call", json!({"name":name,"arguments":arguments}))
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn host(path: &std::path::Path, core: Arc<Mutex<AppCore>>, delay: bool) -> Server {
    let service = Mutex::new(Service::default());
    Server::start(path, move |r: Request| {
        if delay && matches!(r.operation, Operation::DryRun { .. }) {
            std::thread::sleep(Duration::from_millis(200));
        }
        service.lock().unwrap().handle(&core.lock().unwrap(), r)
    })
    .unwrap()
}
fn core() -> Arc<Mutex<AppCore>> {
    Arc::new(Mutex::new(
        AppCore::new(Project::new("GUI Shared").unwrap()).unwrap(),
    ))
}
fn edit(core: &Arc<Mutex<AppCore>>, command: Command) {
    let mut c = core.lock().unwrap();
    let s = c.snapshot();
    c.apply(EditRequest {
        project_id: s.project_id,
        expected_revision: s.revision,
        command,
    })
    .unwrap();
}
fn modern(mut p: Value) -> Value {
    p["_meta"] = json!({"io.modelcontextprotocol/protocolVersion":atom_mcp::MODERN,"io.modelcontextprotocol/clientCapabilities":{}});
    p
}
#[test]
fn legacy_stdio_uses_live_unsaved_gui_state_and_dry_run_never_commits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gui.atom.json");
    let core = core();
    let _server = host(&path, core.clone(), false);
    let mut bridge = Bridge::start(&path);
    bridge.legacy();
    assert_eq!(
        bridge.rpc(1, "tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    edit(
        &core,
        Command::Rename {
            name: "Changed in GUI".into(),
        },
    );
    let state = bridge.tool(2, "atom_project_get", json!({}));
    let data = &state["result"]["structuredContent"]["data"];
    assert_eq!(data["name"], "Changed in GUI");
    assert_eq!(data["dirty"], true);
    let guard = data["guard"].clone();
    let before = serde_json::to_vec(&core.lock().unwrap().snapshot()).unwrap();
    let dry = bridge.tool(
        3,
        "atom_edit_dry_run",
        json!({"guard":guard,"command":{"kind":"rename","name":"Candidate"}}),
    );
    assert_eq!(dry["result"]["isError"], false);
    assert_eq!(
        dry["result"]["structuredContent"]["data"]["changes"][0]["after"],
        "Candidate"
    );
    assert_eq!(
        before,
        serde_json::to_vec(&core.lock().unwrap().snapshot()).unwrap()
    );
    let scene = bridge.tool(
        4,
        "atom_scene_list",
        json!({"guard":guard,"offset":0,"limit":1}),
    );
    assert_eq!(
        scene["result"]["structuredContent"]["data"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    edit(&core, Command::AddBox);
    assert_eq!(
        bridge.tool(
            5,
            "atom_scene_list",
            json!({"guard":guard,"offset":0,"limit":1})
        )["result"]["structuredContent"]["error"]["code"],
        "revision_conflict"
    );
    assert_eq!(
        bridge.tool(6, "atom_diagnose", json!({}))["result"]["structuredContent"]["connected"],
        true
    );
    assert!(!path.exists());
}
#[test]
fn modern_requests_are_stateless_and_require_per_request_capabilities() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gui.atom.json");
    let core = core();
    let _server = host(&path, core, false);
    let mut b = Bridge::start(&path);
    let discover = b.rpc(1, "server/discover", modern(json!({})));
    assert_eq!(discover["result"]["resultType"], "complete");
    assert_eq!(discover["result"]["supportedVersions"][0], atom_mcp::MODERN);
    let tools = b.rpc(2, "tools/list", modern(json!({})));
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 5);
    assert_eq!(b.rpc(3, "tools/list", json!({}))["error"]["code"], -32002);
    assert_eq!(b.rpc(4,"tools/list",json!({"_meta":{"io.modelcontextprotocol/protocolVersion":"2099-01-01","io.modelcontextprotocol/clientCapabilities":{}}}))["error"]["code"],-32022);
    assert_eq!(
        b.rpc(
            5,
            "tools/list",
            json!({"_meta":{"io.modelcontextprotocol/protocolVersion":atom_mcp::MODERN}})
        )["error"]["code"],
        -32602
    );
    let state = b.rpc(
        6,
        "tools/call",
        modern(json!({"name":"atom_project_get","arguments":{}})),
    );
    assert_eq!(state["result"]["resultType"], "complete");
    assert_eq!(state["result"]["isError"], false);
}
#[test]
fn absent_gui_and_restart_do_not_create_another_document() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.atom.json");
    let mut b = Bridge::start(&path);
    b.legacy();
    assert_eq!(
        b.tool(1, "atom_project_get", json!({}))["result"]["structuredContent"]["error"]["code"],
        "not_running"
    );
    let diag = b.tool(2, "atom_diagnose", json!({}));
    assert_eq!(diag["result"]["isError"], false);
    assert_eq!(diag["result"]["structuredContent"]["connected"], false);
    assert!(!path.exists());
    let core = core();
    let server = host(&path, core.clone(), false);
    assert_eq!(
        b.tool(3, "atom_project_get", json!({}))["result"]["structuredContent"]["data"]["name"],
        "GUI Shared"
    );
    drop(server);
    let _server = host(&path, core, false);
    assert_eq!(
        b.tool(4, "atom_diagnose", json!({}))["result"]["structuredContent"]["connected"],
        true
    );
    drop(b);
    assert!(Client::new(&path).diagnose().connected);
}
#[test]
fn cancellation_suppresses_reply_but_does_not_stop_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cancel.atom.json");
    let core = core();
    let _server = host(&path, core.clone(), true);
    let mut b = Bridge::start(&path);
    b.legacy();
    let g = atom_control::Guard::snapshot(&core.lock().unwrap().snapshot());
    let before = serde_json::to_vec(&core.lock().unwrap().snapshot()).unwrap();
    b.send(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"atom_edit_dry_run","arguments":{"guard":g,"command":{"kind":"add_box"}}}}));
    b.send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}));
    assert_eq!(b.rpc(2, "ping", json!({}))["result"], json!({}));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(b.rpc(3, "ping", json!({}))["result"], json!({}));
    assert_eq!(
        before,
        serde_json::to_vec(&core.lock().unwrap().snapshot()).unwrap()
    );
    assert!(Client::new(&path).diagnose().connected);
}
#[test]
fn malformed_input_unknown_tools_and_extra_arguments_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid.atom.json");
    let mut b = Bridge::start(&path);
    b.legacy();
    b.input.write_all(b"{\n").unwrap();
    b.input.flush().unwrap();
    assert_eq!(b.recv()["error"]["code"], -32700);
    assert_eq!(
        b.tool(1, "arbitrary_shell", json!({"command":"anything"}))["error"]["code"],
        -32602
    );
    assert_eq!(
        b.tool(2, "atom_project_get", json!({"file":"secret"}))["result"]["structuredContent"]["error"]
            ["code"],
        "invalid_arguments"
    );
    b.send(json!([]));
    assert_eq!(b.recv()["error"]["code"], -32600);
    b.send(json!({"jsonrpc":"2.0","id":null,"method":"ping"}));
    assert_eq!(b.recv()["error"]["code"], -32600);
}
