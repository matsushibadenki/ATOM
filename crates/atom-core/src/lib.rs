//! Window-independent project owner. UI reads snapshots; all edits use revisions.
mod storage;
pub mod workflow;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub use storage::ProjectStore;
use uuid::Uuid;

pub const NEUTRAL_MATERIAL: [f32; 3] = [0.62, 0.58, 0.51];

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_OBJECTS: usize = 256;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("revision conflict: current revision is {0}")]
    Conflict(u64),
    #[error("unsupported project version")]
    Version,
    #[error("invalid project: {0}")]
    Invalid(String),
    #[error("project is already open by another process")]
    Locked,
    #[error("project file changed outside this session")]
    ExternalChange,
    #[error("unsaved changes")]
    Unsaved,
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}
impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Conflict(_) => "revision_conflict",
            Self::Version => "unsupported_version",
            Self::Invalid(_) => "invalid_project",
            Self::Locked => "project_locked",
            Self::ExternalChange => "external_change",
            Self::Unsaved => "unsaved_changes",
            Self::Io(_) => "io_error",
            Self::Json(_) => "invalid_json",
        }
    }
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SceneObject {
    pub id: u32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub color: [f32; 3],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    pub objects: Vec<SceneObject>,
    pub workflow: unge_core::Document,
}
impl Project {
    pub fn new(name: &str) -> Result<Self> {
        let project = Self {
            schema_version: SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: name.into(),
            objects: vec![SceneObject {
                id: 1,
                position: [0., 0., 0.],
                rotation: [0.35, 0.5, 0.],
                scale: [1.; 3],
                color: NEUTRAL_MATERIAL,
            }],
            workflow: workflow::initial_document()?,
        };
        project.validate()?;
        Ok(project)
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(Error::Version);
        }
        if self.id.is_nil()
            || self.name.trim().is_empty()
            || self.name.chars().count() > 80
            || self.name.chars().any(char::is_control)
        {
            return Err(Error::Invalid("name or identity".into()));
        }
        if self.objects.len() > MAX_OBJECTS {
            return Err(Error::Invalid("object capacity".into()));
        }
        let mut ids = BTreeSet::new();
        for o in &self.objects {
            if o.id == 0
                || !ids.insert(o.id)
                || !o
                    .position
                    .iter()
                    .chain(o.rotation.iter())
                    .all(|v| v.is_finite() && v.abs() <= 10_000.)
                || !o
                    .scale
                    .iter()
                    .all(|v| v.is_finite() && *v > 0. && *v <= 100.)
                || !o
                    .color
                    .iter()
                    .all(|v| v.is_finite() && (0. ..=1.).contains(v))
            {
                return Err(Error::Invalid("object bounds or identity".into()));
            }
        }
        if self.workflow.graph().nodes().len() > 32 || self.workflow.graph().edges().len() > 64 {
            return Err(Error::Invalid("workflow capacity".into()));
        }
        let registry = std::sync::Arc::new(workflow::registry());
        unge_core::Editor::new(self.workflow.clone(), 32)
            .and_then(|e| e.with_validator(registry))
            .map_err(|e| Error::Invalid(e.to_string()))?;
        Ok(())
    }
    pub fn render_scene(&self) -> Result<threex_core::Scene> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(self.objects.len() * 128);
        for o in &self.objects {
            // The old foundation's blue preset is displayed as warm neutral; keep
            // original file values and authored custom colors intact.
            let color = if o.color == [0.47, 0.64, 1.] {
                NEUTRAL_MATERIAL
            } else {
                o.color
            };
            // ThreeX create allocates a default entity; update carries its attributes.
            bytes.extend(1u32.to_le_bytes());
            bytes.extend(o.id.to_le_bytes());
            bytes.extend([0u8; 56]);
            bytes.extend(2u32.to_le_bytes());
            bytes.extend(o.id.to_le_bytes());
            for v in o
                .position
                .iter()
                .chain(o.rotation.iter())
                .chain(o.scale.iter())
                .chain(color.iter())
            {
                bytes.extend(v.to_le_bytes());
            }
            bytes.extend(3u32.to_le_bytes()); // visible + mesh, Box geometry
            bytes.extend(0u32.to_le_bytes()); // world parent
        }
        let mut scene = threex_core::Scene::default();
        scene.apply(&bytes).map_err(Error::Invalid)?;
        Ok(scene)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Rename { name: String },
    AddBox,
    RemoveBox { id: u32 },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditRequest {
    pub expected_revision: u64,
    pub project_id: Uuid,
    pub command: Command,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub project_id: Uuid,
    pub revision: u64,
    pub name: String,
    pub dirty: bool,
    pub objects: Vec<SceneObject>,
    pub workflow_nodes: usize,
    pub workflow_edges: usize,
}
pub struct AppCore {
    project: Project,
    scene: threex_core::Scene,
    revision: u64,
    dirty: bool,
    next_object_id: u32,
}
impl AppCore {
    pub fn new(project: Project) -> Result<Self> {
        let scene = project.render_scene()?;
        let next_object_id = project
            .objects
            .iter()
            .map(|o| o.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("object identity exhausted".into()))?;
        Ok(Self {
            project,
            scene,
            revision: 0,
            dirty: false,
            next_object_id,
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            project_id: self.project.id,
            revision: self.revision,
            name: self.project.name.clone(),
            dirty: self.dirty,
            objects: self.project.objects.clone(),
            workflow_nodes: self.project.workflow.graph().nodes().len(),
            workflow_edges: self.project.workflow.graph().edges().len(),
        }
    }
    pub fn project(&self) -> &Project {
        &self.project
    }
    pub fn scene(&self) -> &threex_core::Scene {
        &self.scene
    }
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }
    pub fn apply(&mut self, request: EditRequest) -> Result<Snapshot> {
        if request.expected_revision != self.revision || request.project_id != self.project.id {
            return Err(Error::Conflict(self.revision));
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("revision exhausted".into()))?;
        let mut candidate = self.project.clone();
        let mut next = self.next_object_id;
        match request.command {
            Command::Rename { name } => candidate.name = name.trim().into(),
            Command::AddBox => {
                next = next
                    .checked_add(1)
                    .ok_or_else(|| Error::Invalid("object identity exhausted".into()))?;
                candidate.objects.push(SceneObject {
                    id: self.next_object_id,
                    position: [(candidate.objects.len() % 4) as f32 * 1.4, 0., 0.],
                    rotation: [0.35, 0.5, 0.],
                    scale: [0.7; 3],
                    color: NEUTRAL_MATERIAL,
                });
            }
            Command::RemoveBox { id } => {
                if !candidate.objects.iter().any(|o| o.id == id) {
                    return Err(Error::Invalid("missing object".into()));
                }
                candidate.objects.retain(|o| o.id != id);
            }
        }
        // Validate and derive before publishing; rejected edits never partially mutate state.
        let scene = candidate.render_scene()?;
        self.project = candidate;
        self.scene = scene;
        self.revision = revision;
        self.dirty = true;
        self.next_object_id = next;
        Ok(self.snapshot())
    }
    pub fn reload(&mut self, project: Project) -> Result<()> {
        if self.dirty {
            return Err(Error::Unsaved);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("revision exhausted".into()))?;
        let mut replacement = Self::new(project)?;
        replacement.revision = revision;
        *self = replacement;
        Ok(())
    }
}
