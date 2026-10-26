//! Private per-project discovery and Unix socket transport, never an HTTP listener.
use crate::*;
use std::path::{Path, PathBuf};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Check {
    pub stage: String,
    pub ok: bool,
    pub code: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagnostic {
    pub connected: bool,
    pub checks: Vec<Check>,
    pub instance_id: Option<Uuid>,
    pub state: Option<Value>,
}
#[derive(Clone)]
pub struct Client {
    project: PathBuf,
}
impl Client {
    pub fn new(project: impl AsRef<Path>) -> Self {
        Self {
            project: project.as_ref().to_owned(),
        }
    }
    pub fn call(&self, operation: Operation) -> Result<Response, ApiError> {
        self.request(&Request::new(operation))
    }
}
pub fn descriptor_path(project: &Path) -> Result<PathBuf, ApiError> {
    let parent = project
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(|_| ApiError::new("not_running"))?;
    let name = project
        .file_name()
        .ok_or_else(|| ApiError::new("invalid_arguments"))?;
    Ok(parent.join(format!("{}.control.json", name.to_string_lossy())))
}
pub fn mcp_config(project: &Path, executable: &Path) -> Result<Value, ApiError> {
    let project = descriptor_path(project)?.with_file_name(
        project
            .file_name()
            .ok_or_else(|| ApiError::new("invalid_arguments"))?,
    );
    let executable = executable
        .canonicalize()
        .map_err(|_| ApiError::new("mcp_binary_missing"))?;
    Ok(json!({"mcpServers":{"atom":{"command":executable,"args":["--project",project]}}}))
}
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::Server;
#[cfg(not(unix))]
pub struct Server;
#[cfg(not(unix))]
impl Server {
    pub fn start(
        _: &Path,
        _: impl Fn(Request) -> Response + Send + Sync + 'static,
    ) -> Result<Self, ApiError> {
        Err(ApiError::new("unsupported_transport"))
    }
}
#[cfg(not(unix))]
impl Client {
    pub fn request(&self, _: &Request) -> Result<Response, ApiError> {
        Err(ApiError::new("unsupported_transport"))
    }
    pub fn diagnose(&self) -> Diagnostic {
        Diagnostic {
            connected: false,
            checks: vec![Check {
                stage: "transport".into(),
                ok: false,
                code: "unsupported_transport".into(),
            }],
            instance_id: None,
            state: None,
        }
    }
}
