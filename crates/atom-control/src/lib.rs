//! Read-only Control API v1. No network, model files or GPU buffers enter the API.
pub mod ipc;
mod schemas;
pub use atom_core::Uuid;
use atom_core::{AppCore, EditRequest, Snapshot};
pub use schemas::{request_schema, tools};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;
pub const API_VERSION: u32 = 1;
pub const MAX_REQUEST: usize = 64 * 1024;
pub const MAX_RESPONSE: usize = 256 * 1024;
pub const COMMANDS: &[&str] = &[
    "rename",
    "add_box",
    "remove_box",
    "transform",
    "set_gravity",
    "set_material",
    "set_fixed",
    "set_floor",
    "set_joint_range",
    "set_joint_angle",
    "solve_ik",
    "add_robot",
    "undo",
    "redo",
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guard {
    pub project_id: Uuid,
    pub revision: u64,
    pub sequence: u64,
}
impl Guard {
    pub fn snapshot(s: &Snapshot) -> Self {
        Self {
            project_id: s.project_id,
            revision: s.revision,
            sequence: s.sequence,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Capabilities,
    State,
    SceneList {
        guard: Guard,
        offset: usize,
        limit: usize,
    },
    DryRun {
        guard: Guard,
        command: Value,
    },
    Diagnose,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub api_version: u32,
    pub request_id: Uuid,
    pub operation: Operation,
}
impl Request {
    pub fn new(operation: Operation) -> Self {
        Self {
            api_version: API_VERSION,
            request_id: Uuid::new_v4(),
            operation,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiError {
    pub code: String,
    pub retryable: bool,
}
impl ApiError {
    pub fn new(code: &str) -> Self {
        Self {
            code: code.into(),
            retryable: matches!(
                code,
                "busy" | "timeout" | "not_running" | "revision_conflict" | "state_conflict"
            ),
        }
    }
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.code)
    }
}
impl std::error::Error for ApiError {}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Ok { data: Value },
    Error { error: ApiError },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response {
    pub api_version: u32,
    pub request_id: Uuid,
    pub basis: Option<Guard>,
    pub replayed: bool,
    #[serde(flatten)]
    pub outcome: Outcome,
}
impl Response {
    pub fn error(id: Uuid, code: &str) -> Self {
        Self {
            api_version: API_VERSION,
            request_id: id,
            basis: None,
            replayed: false,
            outcome: Outcome::Error {
                error: ApiError::new(code),
            },
        }
    }
    pub fn into_result(self) -> Result<Value, ApiError> {
        match self.outcome {
            Outcome::Ok { data } => Ok(data),
            Outcome::Error { error } => Err(error),
        }
    }
}
pub fn capabilities() -> Value {
    json!({"api_version":API_VERSION,"application":"ATOM","state_owner":"desktop_shared_core","permissions":["state:read","scene:read","edit:dry_run"],"write":false,"physics_execution":false,"training":false,"transports":{"local_ipc":cfg!(unix),"mcp_stdio":true,"remote_http":false},"limits":{"request_bytes":MAX_REQUEST,"response_bytes":MAX_RESPONSE,"scene_page":64,"dry_run_changes":64,"cached_requests":64},"coordinates":{"objects":"editor_world_y_up","gravity":"physics_z_up","dry_run_diff":"authored_rest_y_up"},"units":{"position":"m","rotation":"rad","time":"s","density":"kg/m3"},"dry_run_commands":COMMANDS})
}
#[derive(Default)]
pub struct Service {
    cache: VecDeque<(Uuid, Vec<u8>, Response)>,
}
impl Service {
    pub fn handle(&mut self, core: &AppCore, request: Request) -> Response {
        let id = request.request_id;
        let encoded = match serde_json::to_vec(&request) {
            Ok(b) if b.len() <= MAX_REQUEST => b,
            _ => return Response::error(id, "request_too_large"),
        };
        if request.api_version != API_VERSION {
            return Response::error(id, "unsupported_api_version");
        }
        if let Some((_, old, response)) = self.cache.iter().find(|(key, _, _)| *key == id) {
            return if old == &encoded {
                let mut r = response.clone();
                r.replayed = true;
                r
            } else {
                Response::error(id, "request_conflict")
            };
        }
        let snapshot = core.snapshot();
        let basis = Guard::snapshot(&snapshot);
        let result = (|| -> Result<Value, ApiError> {
            match request.operation {
                Operation::Capabilities => Ok(capabilities()),
                Operation::State => Ok(state(core, &snapshot)),
                Operation::Diagnose => Ok(
                    json!({"connected":true,"state_owner":"desktop_shared_core","api_version":API_VERSION,"guard":basis,"previewing":snapshot.previewing,"permissions":capabilities()["permissions"],"physics_probe":"not_performed","llm_probe":"not_performed"}),
                ),
                Operation::SceneList {
                    guard,
                    offset,
                    limit,
                } => {
                    check(&basis, &guard)?;
                    if !(1..=64).contains(&limit) || offset > snapshot.objects.len() {
                        return Err(ApiError::new("invalid_cursor"));
                    }
                    let objects: Vec<_> =
                        snapshot.objects.iter().skip(offset).take(limit).collect();
                    let next = offset + objects.len();
                    Ok(
                        json!({"guard":basis,"objects":objects,"offset":offset,"next_offset":if next<snapshot.objects.len(){Some(next)} else {None},"total":snapshot.objects.len(),"coordinate_frame":"displayed_world_y_up"}),
                    )
                }
                Operation::DryRun { guard, command } => {
                    check(&basis, &guard)?;
                    if !command
                        .get("kind")
                        .and_then(Value::as_str)
                        .is_some_and(|k| COMMANDS.contains(&k))
                    {
                        return Err(ApiError::new("unsupported_operation"));
                    }
                    if command["kind"] == "set_material" && command.get("id").is_none() {
                        return Err(ApiError::new("invalid_arguments"));
                    }
                    let command = serde_json::from_value(command)
                        .map_err(|_| ApiError::new("invalid_arguments"))?;
                    let report = core
                        .dry_run(EditRequest {
                            project_id: guard.project_id,
                            expected_revision: guard.revision,
                            command,
                        })
                        .map_err(|e| ApiError::new(e.code()))?;
                    serde_json::to_value(report).map_err(|_| ApiError::new("internal_error"))
                }
            }
        })();
        let outcome = match result {
            Ok(data) => Outcome::Ok { data },
            Err(error) => Outcome::Error { error },
        };
        let mut response = Response {
            api_version: API_VERSION,
            request_id: id,
            basis: Some(basis),
            replayed: false,
            outcome,
        };
        if serde_json::to_vec(&response).map_or(true, |b| b.len() > MAX_RESPONSE) {
            response = Response::error(id, "response_too_large");
        }
        self.cache.push_back((id, encoded, response.clone()));
        if self.cache.len() > 64 {
            self.cache.pop_front();
        }
        response
    }
}
fn check(current: &Guard, expected: &Guard) -> Result<(), ApiError> {
    if current.project_id != expected.project_id || current.revision != expected.revision {
        Err(ApiError::new("revision_conflict"))
    } else if current.sequence != expected.sequence {
        Err(ApiError::new("state_conflict"))
    } else {
        Ok(())
    }
}
fn state(core: &AppCore, s: &Snapshot) -> Value {
    let mode = if let Some(r) = &s.reach {
        if r.live {
            "reach_recording"
        } else {
            "reach_playback"
        }
    } else if s.asset_importing {
        "asset_import"
    } else if s.simulation_running {
        "simulation_running"
    } else if s.simulation.is_some() {
        "simulation_preview"
    } else if s.joint_preview.is_some() {
        "joint_preview"
    } else if s.previewing {
        "transform_preview"
    } else {
        "authored"
    };
    let rendered = core.render_project();
    let robots:Vec<_>=rendered.robots.iter().map(|r|json!({"root":r.root,"template":r.template,"links":r.links.len(),"rotational_dof":r.links.iter().filter(|l|l.parent.is_some()).map(|l|l.axes().count()).sum::<usize>()})).collect();
    json!({"guard":Guard::snapshot(s),"name":s.name,"dirty":s.dirty,"mode":mode,"previewing":s.previewing,"can_undo":s.can_undo,"can_redo":s.can_redo,"undo_depth":s.undo_depth,"redo_depth":s.redo_depth,"object_count":s.objects.len(),"asset_count":s.assets.len(),"robots":robots,"physics":s.physics,"authored_physics":core.project().physics,"reach":s.reach,"simulation":s.simulation.as_ref().map(|r|json!({"engine":r.engine,"time":r.time,"contacts":r.contacts})),"units":capabilities()["units"]})
}
