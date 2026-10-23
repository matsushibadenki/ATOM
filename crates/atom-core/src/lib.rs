//! Window-independent project owner. UI reads snapshots; all edits use revisions.
pub mod assets;
mod editor;
pub mod mujoco;
pub mod physics;
pub mod reach;
pub mod robots;
mod storage;
pub use editor::AppCore;
pub use physics::{Gravity, GravityPreset, MaterialPreset, PhysicalMaterial, PhysicsSpec};
pub use robots::RobotTemplate;
pub mod workflow;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub use storage::ProjectStore;
pub use uuid::Uuid;

pub const NEUTRAL_MATERIAL: [f32; 3] = [0.62, 0.58, 0.51];

pub const SCHEMA_VERSION: u32 = 3;
pub const MAX_OBJECTS: usize = 256;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("revision conflict: current revision is {0}")]
    Conflict(u64),
    #[error("unsupported project version")]
    Version,
    #[error("invalid project: {0}")]
    Invalid(String),
    #[error("position IK did not converge within the joint limits and solve budget")]
    IkUnreachable,
    #[error("project is already open by another process")]
    Locked,
    #[error("project file changed outside this session")]
    ExternalChange,
    #[error("an edit or simulation preview is active")]
    EditBusy,
    #[error("no history in this direction")]
    HistoryEmpty,
    #[error("MuJoCo Python runtime unavailable; set ATOM_MUJOCO_PYTHON")]
    PhysicsUnavailable,
    #[error("MuJoCo simulation failed or exceeded its budget")]
    PhysicsFailed,
    #[error("unsaved changes")]
    Unsaved,
    #[error("OpenUSD: {0}")]
    Asset(String),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
}
impl Error {
    pub fn code(&self) -> &str {
        match self {
            Self::Conflict(_) => "revision_conflict",
            Self::Version => "unsupported_version",
            Self::Invalid(_) => "invalid_project",
            Self::IkUnreachable => "ik_unreachable",
            Self::Locked => "project_locked",
            Self::ExternalChange => "external_change",
            Self::EditBusy => "edit_busy",
            Self::HistoryEmpty => "history_empty",
            Self::PhysicsUnavailable => "physics_unavailable",
            Self::PhysicsFailed => "physics_failed",
            Self::Unsaved => "unsaved_changes",
            Self::Asset(code) => code,
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
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub material: PhysicalMaterial,
    #[serde(default)]
    pub fixed: bool,
    #[serde(default)]
    pub mesh_ref: Option<Uuid>,
    #[serde(default)]
    pub mass_override: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    pub objects: Vec<SceneObject>,
    pub workflow: unge_core::Document,
    #[serde(default)]
    pub physics: PhysicsSpec,
    #[serde(default)]
    pub robots: Vec<robots::Robot>,
    #[serde(default)]
    pub assets: Vec<assets::ImportedAsset>,
}
impl Project {
    pub fn new(name: &str) -> Result<Self> {
        let project = Self {
            schema_version: SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: name.into(),
            objects: vec![SceneObject {
                id: 1,
                position: [0., 1., 0.],
                rotation: [0.35, 0.5, 0.],
                scale: [1.; 3],
                color: NEUTRAL_MATERIAL,
                label: String::new(),
                material: PhysicalMaterial::default(),
                fixed: false,
                mesh_ref: None,
                mass_override: None,
            }],
            workflow: workflow::initial_document()?,
            physics: PhysicsSpec::default(),
            robots: Vec::new(),
            assets: Vec::new(),
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
        self.physics.validate()?;
        if self.assets.len() > 32
            || self
                .assets
                .iter()
                .map(|a| a.mesh.positions.len())
                .sum::<usize>()
                > 900_000
        {
            return Err(Error::Invalid("asset capacity".into()));
        }
        let mut asset_ids = BTreeSet::new();
        for a in &self.assets {
            a.validate()?;
            if !asset_ids.insert(a.id) {
                return Err(Error::Invalid("duplicate asset".into()));
            }
        }
        let mut robot_ids = BTreeSet::new();
        for robot in &self.robots {
            robot.validate(&self.objects)?;
            for link in &robot.links {
                if !robot_ids.insert(link.object_id) {
                    return Err(Error::Invalid("duplicate robot membership".into()));
                }
            }
        }
        let mut ids = BTreeSet::new();
        for o in &self.objects {
            o.material.validate()?;
            if o.mesh_ref.is_some_and(|id| !asset_ids.contains(&id))
                || o.mass_override
                    .is_some_and(|m| !m.is_finite() || !(0.000001..=1e9).contains(&m))
            {
                return Err(Error::Invalid("object asset/mass".into()));
            }
            if o.label.len() > 64 || o.label.chars().any(char::is_control) {
                return Err(Error::Invalid("object label".into()));
            }
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
        for o in &self.posed_objects() {
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    pub position: [f32; 3],
    /// XYZ Euler angles, radians. UI authoring uses degrees.
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
}
impl From<&SceneObject> for Transform {
    fn from(o: &SceneObject) -> Self {
        Self {
            position: o.position,
            rotation: o.rotation,
            scale: o.scale,
        }
    }
}
impl Transform {
    pub fn assign(&self, object: &mut SceneObject) {
        object.position = self.position;
        object.rotation = self.rotation;
        object.scale = self.scale;
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Rename {
        name: String,
    },
    AddBox,
    RemoveBox {
        id: u32,
    },
    Transform {
        id: u32,
        transform: Transform,
    },
    SetGravity {
        gravity: Gravity,
    },
    SetMaterial {
        id: Option<u32>,
        material: PhysicalMaterial,
    },
    SetFixed {
        id: u32,
        fixed: bool,
    },
    SetFloor {
        enabled: bool,
    },
    SetJointRange {
        id: u32,
        axis: usize,
        range: [f32; 2],
    },
    SetJointAngle {
        id: u32,
        axis: usize,
        angle: f32,
    },
    SolveIk {
        id: u32,
        /// Ancestor held fixed; only its descendants on the path to id may rotate.
        base: u32,
        /// Selected object's center in editor world coordinates (Y up), meters.
        target: [f32; 3],
    },
    AddRobot {
        template: RobotTemplate,
    },
    #[serde(skip)]
    ImportAsset {
        asset: Box<assets::ImportedAsset>,
        position: [f32; 3],
        scale: [f32; 3],
        mass: Option<f64>,
    },
    Undo,
    Redo,
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
    /// Orders previews, cancellations and save acknowledgements within a revision.
    pub sequence: u64,
    pub name: String,
    pub dirty: bool,
    pub previewing: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_depth: usize,
    pub redo_depth: usize,
    pub objects: Vec<SceneObject>,
    pub physics: PhysicsSpec,
    pub robots: Vec<robots::Robot>,
    pub joint_preview: Option<JointPreviewInfo>,
    pub reach: Option<reach::Summary>,
    pub simulation_running: bool,
    pub simulation: Option<mujoco::SimulationResult>,
    pub asset_importing: bool,
    pub assets: Vec<assets::AssetSummary>,
    pub workflow_nodes: usize,
    pub workflow_edges: usize,
}

/// Transient joint transaction identity; never part of the saved Project.
#[derive(Clone, Debug, Serialize)]
pub struct JointPreviewInfo {
    pub token: u64,
    pub object_id: u32,
    pub axis: usize,
    pub edit: JointEdit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JointEdit {
    #[default]
    Angle,
    Lower,
    Upper,
}
