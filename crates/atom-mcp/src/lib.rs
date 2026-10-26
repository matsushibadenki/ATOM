//! MCP stdio bridge: 2026-07-28 per-request metadata + 2025-11-25 initialization.
//! This process never opens ProjectStore or owns an AppCore.
use atom_control::{ApiError, Guard, Operation, Request, ipc::Client};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{BufRead, Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
pub const MODERN: &str = "2026-07-28";
pub const LEGACY: &str = "2025-11-25";
const INFO: (&str, &str) = ("atom-mcp", env!("CARGO_PKG_VERSION"));
const INSTRUCTIONS: &str = "Read-only bridge to the running ATOM desktop. Read atom_project_get first; pass its guard to scene pages/dry-run. A dry-run never commits edits or tests dynamics. atom_diagnose reports connection failures without starting another document. No mesh data, arbitrary files, execution or remote HTTP tools are exposed.";
fn rpc_error(id: Value, code: i32, message: &str, data: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message,"data":data}})
}
fn rpc_result(id: Value, mut result: Value, modern: bool) -> Value {
    if modern {
        result["resultType"] = json!("complete");
        result["_meta"] =
            json!({"io.modelcontextprotocol/serverInfo":{"name":INFO.0,"version":INFO.1}});
    }
    json!({"jsonrpc":"2.0","id":id,"result":result})
}
fn version_error(id: Value, requested: &str) -> Value {
    rpc_error(
        id,
        -32022,
        "Unsupported protocol version",
        json!({"supported":[MODERN,LEGACY],"requested":requested}),
    )
}
fn valid_id(id: &Value) -> bool {
    id.is_string() || id.is_number()
}
#[derive(Default)]
pub struct Protocol {
    initialized: bool,
    ready: bool,
}
pub enum Dispatch {
    Immediate(Option<Value>),
    Tool(Call),
}
pub struct Call {
    pub id: Value,
    pub modern: bool,
    pub name: String,
    pub arguments: Value,
}
impl Protocol {
    pub fn prepare(&mut self, message: Value) -> Dispatch {
        let id = message.get("id").cloned().unwrap_or(Value::Null);
        if !message.is_object()
            || message["jsonrpc"] != "2.0"
            || !message["method"].is_string()
            || message.get("id").is_some_and(|v| !valid_id(v))
            || message.get("params").is_some_and(|p| !p.is_object())
            || message.get("result").is_some()
            || message.get("error").is_some()
        {
            return Dispatch::Immediate(Some(rpc_error(
                id,
                -32600,
                "Invalid request",
                Value::Null,
            )));
        }
        let method = message["method"].as_str().unwrap();
        let params = message.get("params").cloned().unwrap_or(json!({}));
        if message.get("id").is_none() {
            if method == "notifications/initialized" && self.initialized {
                self.ready = true;
            }
            return Dispatch::Immediate(None);
        }
        if params.get("_meta").is_some_and(|m| !m.is_object()) {
            return Dispatch::Immediate(Some(rpc_error(
                id,
                -32602,
                "Invalid metadata",
                Value::Null,
            )));
        }
        if method == "initialize" {
            if self.initialized
                || !params["protocolVersion"].is_string()
                || !params["capabilities"].is_object()
                || !params["clientInfo"]["name"].is_string()
                || !params["clientInfo"]["version"].is_string()
            {
                return Dispatch::Immediate(Some(rpc_error(
                    id,
                    -32602,
                    "Invalid initialization",
                    Value::Null,
                )));
            }
            self.initialized = true;
            return Dispatch::Immediate(Some(rpc_result(
                id,
                json!({"protocolVersion":LEGACY,"capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":INFO.0,"version":INFO.1},"instructions":INSTRUCTIONS}),
                false,
            )));
        }
        let meta = &params["_meta"];
        let modern = meta
            .get("io.modelcontextprotocol/protocolVersion")
            .is_some()
            || meta
                .get("io.modelcontextprotocol/clientCapabilities")
                .is_some();
        if modern {
            let Some(version) = meta["io.modelcontextprotocol/protocolVersion"].as_str() else {
                return Dispatch::Immediate(Some(rpc_error(
                    id,
                    -32602,
                    "Missing protocol metadata",
                    Value::Null,
                )));
            };
            if version != MODERN {
                return Dispatch::Immediate(Some(version_error(id, version)));
            }
            if !meta["io.modelcontextprotocol/clientCapabilities"].is_object()
                || meta
                    .get("io.modelcontextprotocol/clientInfo")
                    .is_some_and(|i| !i["name"].is_string() || !i["version"].is_string())
            {
                return Dispatch::Immediate(Some(rpc_error(
                    id,
                    -32602,
                    "Invalid client metadata",
                    Value::Null,
                )));
            }
        } else if method != "ping" && !self.ready {
            return Dispatch::Immediate(Some(rpc_error(
                id,
                -32002,
                "Initialize or send 2026-07-28 request metadata",
                json!({"supported":[MODERN,LEGACY]}),
            )));
        }
        let result = match method {
            "server/discover" if modern => {
                json!({"supportedVersions":[MODERN,LEGACY],"capabilities":{"tools":{"listChanged":false}},"instructions":INSTRUCTIONS,"ttlMs":0,"cacheScope":"public"})
            }
            "ping" if !modern => json!({}),
            "tools/list" => {
                if params.get("cursor").is_some() {
                    return Dispatch::Immediate(Some(rpc_error(
                        id,
                        -32602,
                        "Invalid cursor",
                        Value::Null,
                    )));
                }
                if modern {
                    json!({"tools":atom_control::tools(),"ttlMs":0,"cacheScope":"public"})
                } else {
                    json!({"tools":atom_control::tools()})
                }
            }
            "tools/call" => {
                let Some(name) = params["name"].as_str() else {
                    return Dispatch::Immediate(Some(rpc_error(
                        id,
                        -32602,
                        "Missing tool name",
                        Value::Null,
                    )));
                };
                if !atom_control::tools().iter().any(|t| t["name"] == name) {
                    return Dispatch::Immediate(Some(rpc_error(
                        id,
                        -32602,
                        "Unknown tool",
                        Value::Null,
                    )));
                }
                if params.get("task").is_some()
                    || params.get("requestState").is_some()
                    || params.get("inputResponses").is_some()
                {
                    return Dispatch::Immediate(Some(rpc_error(
                        id,
                        -32602,
                        "Tasks and multi-round-trip tools are unsupported",
                        Value::Null,
                    )));
                }
                return Dispatch::Tool(Call {
                    id,
                    modern,
                    name: name.into(),
                    arguments: params.get("arguments").cloned().unwrap_or(json!({})),
                });
            }
            _ => {
                return Dispatch::Immediate(Some(rpc_error(
                    id,
                    -32601,
                    "Method not found",
                    Value::Null,
                )));
            }
        };
        Dispatch::Immediate(Some(rpc_result(id, result, modern)))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SceneArgs {
    guard: Guard,
    offset: usize,
    limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DryArgs {
    guard: Guard,
    command: Value,
}
impl Call {
    pub fn execute(self, client: &Client) -> Value {
        let result = (|| -> Result<Value, ApiError> {
            let parse = |v: Value| {
                serde_json::from_value::<Empty>(v).map_err(|_| ApiError::new("invalid_arguments"))
            };
            let operation = match self.name.as_str() {
                "atom_capabilities" => {
                    parse(self.arguments)?;
                    Operation::Capabilities
                }
                "atom_project_get" => {
                    parse(self.arguments)?;
                    Operation::State
                }
                "atom_diagnose" => {
                    parse(self.arguments)?;
                    return serde_json::to_value(client.diagnose())
                        .map_err(|_| ApiError::new("internal_error"));
                }
                "atom_scene_list" => {
                    let a: SceneArgs = serde_json::from_value(self.arguments)
                        .map_err(|_| ApiError::new("invalid_arguments"))?;
                    Operation::SceneList {
                        guard: a.guard,
                        offset: a.offset,
                        limit: a.limit,
                    }
                }
                "atom_edit_dry_run" => {
                    let a: DryArgs = serde_json::from_value(self.arguments)
                        .map_err(|_| ApiError::new("invalid_arguments"))?;
                    Operation::DryRun {
                        guard: a.guard,
                        command: a.command,
                    }
                }
                _ => return Err(ApiError::new("unsupported_operation")),
            };
            let response = client.request(&Request::new(operation))?;
            // Retain basis/request ID/replayed metadata, including for rejected dry-runs.
            serde_json::to_value(response).map_err(|_| ApiError::new("internal_error"))
        })();
        let (value, is_error) = match result {
            Ok(value) => {
                let error = value["status"] == "error";
                (value, error)
            }
            Err(error) => (json!({"status":"error","error":error}), true),
        };
        let text = serde_json::to_string(&value).unwrap_or_else(|_| "internal_error".into());
        rpc_result(
            self.id,
            json!({"content":[{"type":"text","text":text}],"structuredContent":value,"isError":is_error}),
            self.modern,
        )
    }
}
enum Event {
    Input(Vec<u8>),
    TooLarge,
    Closed,
    Finished {
        key: String,
        response: Value,
        cancelled: Arc<AtomicBool>,
    },
}
fn write_reply(out: &mut impl Write, response: &Value) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(response)?;
    if bytes.len() > 3 * atom_control::MAX_RESPONSE + 8192 {
        return Err(std::io::Error::other("MCP reply budget"));
    }
    out.write_all(&bytes)?;
    out.write_all(b"\n")?;
    out.flush()
}
/// One writer, up to eight bounded read-only tool calls; cancellation suppresses replies.
pub fn serve(
    input: impl Read + Send + 'static,
    mut output: impl Write,
    client: Client,
) -> std::io::Result<()> {
    let (tx, rx) = std::sync::mpsc::sync_channel(64);
    let input_tx = tx.clone();
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(input);
        loop {
            let mut line = vec![];
            match (&mut reader)
                .take(atom_control::MAX_REQUEST as u64 + 1)
                .read_until(b'\n', &mut line)
            {
                Ok(0) | Err(_) => break,
                Ok(n) if n > atom_control::MAX_REQUEST => {
                    let _ = input_tx.send(Event::TooLarge);
                    break;
                }
                Ok(_) => {
                    if input_tx.send(Event::Input(line)).is_err() {
                        return;
                    }
                }
            }
        }
        let _ = input_tx.send(Event::Closed);
    });
    let mut protocol = Protocol::default();
    let mut active: HashMap<String, Arc<AtomicBool>> = HashMap::new();
    while let Ok(event) = rx.recv() {
        match event {
            Event::Closed => {
                for flag in active.values() {
                    flag.store(true, Ordering::Release);
                }
                break;
            }
            Event::TooLarge => write_reply(
                &mut output,
                &rpc_error(Value::Null, -32600, "Message too large", Value::Null),
            )?,
            Event::Finished {
                key,
                response,
                cancelled,
            } => {
                active.remove(&key);
                if !cancelled.load(Ordering::Acquire) {
                    write_reply(&mut output, &response)?;
                }
            }
            Event::Input(bytes) => {
                let message: Value = match serde_json::from_slice(&bytes) {
                    Ok(v) => v,
                    Err(_) => {
                        write_reply(
                            &mut output,
                            &rpc_error(Value::Null, -32700, "Parse error", Value::Null),
                        )?;
                        continue;
                    }
                };
                if message["jsonrpc"] == "2.0"
                    && message["method"] == "notifications/cancelled"
                    && message.get("id").is_none()
                {
                    if let Some(id) = message["params"].get("requestId") {
                        let key = id.to_string();
                        if let Some(flag) = active.get(&key) {
                            flag.store(true, Ordering::Release);
                        }
                    }
                    continue;
                }
                let key = message.get("id").map(Value::to_string);
                if let Some(flag) = key.as_ref().and_then(|key| active.get(key)) {
                    flag.store(true, Ordering::Release);
                    write_reply(
                        &mut output,
                        &rpc_error(
                            message["id"].clone(),
                            -32600,
                            "Duplicate in-flight request ID",
                            Value::Null,
                        ),
                    )?;
                    continue;
                }
                match protocol.prepare(message) {
                    Dispatch::Immediate(Some(response)) => write_reply(&mut output, &response)?,
                    Dispatch::Immediate(None) => {}
                    Dispatch::Tool(call) => {
                        if active.len() >= 8 {
                            write_reply(
                                &mut output,
                                &rpc_error(call.id, -32000, "Server busy", Value::Null),
                            )?;
                            continue;
                        }
                        let key = call.id.to_string();
                        let cancelled = Arc::new(AtomicBool::new(false));
                        active.insert(key.clone(), cancelled.clone());
                        let tx = tx.clone();
                        let client = client.clone();
                        std::thread::spawn(move || {
                            let response = call.execute(&client);
                            let _ = tx.send(Event::Finished {
                                key,
                                response,
                                cancelled,
                            });
                        });
                    }
                }
            }
        }
    }
    Ok(())
}
