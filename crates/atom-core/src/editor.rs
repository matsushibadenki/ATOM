//! Bounded document history and one owner-scoped transform transaction.
use super::*;
use std::collections::VecDeque;
const HISTORY_LIMIT: usize = 128;
#[derive(Clone)]
struct Version {
    project: Project,
    id: u64,
}
struct Preview {
    owner: String,
    object: SceneObject,
    scene: Option<threex_core::Scene>,
}
pub struct AppCore {
    project: Project,
    scene: threex_core::Scene,
    revision: u64,
    sequence: u64,
    next_object_id: u32,
    current_version: u64,
    next_version: u64,
    saved_version: u64,
    undo: VecDeque<Version>,
    redo: VecDeque<Version>,
    preview: Option<Preview>,
    simulation: Option<(Project, threex_core::Scene, mujoco::SimulationResult)>,
    simulation_token: Option<u64>,
    import_token: Option<u64>,
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
            sequence: 0,
            next_object_id,
            current_version: 0,
            next_version: 1,
            saved_version: 0,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            preview: None,
            simulation: None,
            simulation_token: None,
            import_token: None,
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        let mut objects = self
            .simulation
            .as_ref()
            .map_or(&self.project, |s| &s.0)
            .objects
            .clone();
        if let Some(p) = &self.preview
            && let Some(o) = objects.iter_mut().find(|o| o.id == p.object.id)
        {
            *o = p.object.clone();
        }
        Snapshot {
            project_id: self.project.id,
            revision: self.revision,
            sequence: self.sequence,
            name: self.project.name.clone(),
            dirty: self.current_version != self.saved_version,
            previewing: self.has_preview(),
            can_undo: !self.has_preview() && !self.undo.is_empty(),
            can_redo: !self.has_preview() && !self.redo.is_empty(),
            undo_depth: self.undo.len(),
            redo_depth: self.redo.len(),
            objects,
            asset_importing: self.import_token.is_some(),
            assets: self.project.assets.iter().map(|a| a.summary()).collect(),
            physics: self.project.physics.clone(),
            robots: self.project.robots.clone(),
            simulation_running: self.simulation_token.is_some(),
            simulation: self.simulation.as_ref().map(|s| s.2.clone()),
            workflow_nodes: self.project.workflow.graph().nodes().len(),
            workflow_edges: self.project.workflow.graph().edges().len(),
        }
    }
    pub fn project(&self) -> &Project {
        &self.project
    }
    pub fn object(&self, id: u32) -> Option<&SceneObject> {
        if let Some(p) = &self.preview
            && p.object.id == id
        {
            Some(&p.object)
        } else {
            self.simulation
                .as_ref()
                .map_or(&self.project, |s| &s.0)
                .objects
                .iter()
                .find(|o| o.id == id)
        }
    }
    pub fn scene(&self) -> &threex_core::Scene {
        self.preview
            .as_ref()
            .and_then(|p| p.scene.as_ref())
            .unwrap_or_else(|| self.simulation.as_ref().map_or(&self.scene, |s| &s.1))
    }
    pub fn has_transform_preview(&self) -> bool {
        self.preview.is_some()
    }
    pub fn has_preview(&self) -> bool {
        self.preview.is_some()
            || self.simulation.is_some()
            || self.simulation_token.is_some()
            || self.import_token.is_some()
    }
    pub fn preview_owned_by(&self, owner: &str) -> bool {
        self.preview.as_ref().is_some_and(|p| p.owner == owner)
    }
    pub fn mark_saved(&mut self) {
        self.saved_version = self.current_version;
        self.sequence = self.sequence.saturating_add(1);
    }
    fn check(&self, expected: u64, project_id: Uuid) -> Result<()> {
        if expected != self.revision || project_id != self.project.id {
            Err(Error::Conflict(self.revision))
        } else {
            Ok(())
        }
    }
    fn counters(&self) -> Result<(u64, u64)> {
        Ok((
            self.revision
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("revision exhausted".into()))?,
            self.sequence
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("sequence exhausted".into()))?,
        ))
    }
    pub fn apply(&mut self, request: EditRequest) -> Result<Snapshot> {
        self.check(request.expected_revision, request.project_id)?;
        if self.has_preview() {
            return Err(Error::EditBusy);
        }
        match request.command {
            Command::Undo => return self.travel(true),
            Command::Redo => return self.travel(false),
            _ => {}
        }
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
                    position: [(candidate.objects.len() % 4) as f32 * 1.4, 1., 0.],
                    rotation: [0.35, 0.5, 0.],
                    scale: [0.7; 3],
                    color: NEUTRAL_MATERIAL,
                    label: String::new(),
                    material: PhysicalMaterial::default(),
                    fixed: false,
                    mesh_ref: None,
                    mass_override: None,
                });
            }
            Command::RemoveBox { id } => {
                if !candidate.objects.iter().any(|o| o.id == id) {
                    return Err(Error::Invalid("missing object".into()));
                }
                if let Some(robot) = candidate
                    .robots
                    .iter()
                    .find(|r| r.links.iter().any(|l| l.object_id == id))
                {
                    if robot.root != id {
                        return Err(Error::Invalid(
                            "remove the robot root to remove the template".into(),
                        ));
                    }
                    let ids: Vec<_> = robot.links.iter().map(|l| l.object_id).collect();
                    candidate.objects.retain(|o| !ids.contains(&o.id));
                    candidate.robots.retain(|r| r.root != id);
                } else {
                    candidate.objects.retain(|o| o.id != id);
                }
            }
            Command::Transform { id, transform } => {
                let object = candidate
                    .objects
                    .iter_mut()
                    .find(|o| o.id == id)
                    .ok_or_else(|| Error::Invalid("missing object".into()))?;
                transform.assign(object);
            }
            Command::SetGravity { gravity } => candidate.physics.gravity = gravity,
            Command::SetMaterial { id, material } => {
                if let Some(id) = id
                    && let Some(o) = candidate.objects.iter_mut().find(|o| o.id == id)
                {
                    o.mass_override = None;
                }
                if let Some(id) = id {
                    candidate
                        .objects
                        .iter_mut()
                        .find(|o| o.id == id)
                        .ok_or_else(|| Error::Invalid("missing object".into()))?
                        .material = material;
                } else {
                    candidate.physics.floor = material;
                }
            }
            Command::SetFixed { id, fixed } => {
                if candidate
                    .robots
                    .iter()
                    .any(|r| r.links.iter().any(|l| l.object_id == id))
                {
                    return Err(Error::Invalid(
                        "robot base mode is defined by template".into(),
                    ));
                }
                candidate
                    .objects
                    .iter_mut()
                    .find(|o| o.id == id)
                    .ok_or_else(|| Error::Invalid("missing object".into()))?
                    .fixed = fixed;
            }
            Command::SetFloor { enabled } => candidate.physics.floor_enabled = enabled,
            Command::AddRobot { template } => {
                let (mut robot, mut objects, end) = robots::build(template, next)?;
                if !candidate.objects.is_empty() {
                    let right = candidate
                        .objects
                        .iter()
                        .map(|o| {
                            o.position[0] + o.scale.iter().map(|v| v * v).sum::<f32>().sqrt() / 2.
                        })
                        .fold(f32::NEG_INFINITY, f32::max);
                    let left = objects
                        .iter()
                        .map(|o| o.position[0] - o.scale[0] / 2.)
                        .fold(f32::INFINITY, f32::min);
                    let offset = right + 0.5 - left;
                    for o in &mut objects {
                        o.position[0] += offset;
                    }
                    for l in &mut robot.links {
                        l.anchor[0] += offset;
                    }
                }
                candidate.robots.push(robot);
                candidate.objects.extend(objects);
                next = end;
            }
            Command::ImportAsset {
                asset,
                position,
                scale,
                mass,
            } => {
                next = next
                    .checked_add(1)
                    .ok_or_else(|| Error::Invalid("object identity exhausted".into()))?;
                let id = asset.id;
                let label = asset
                    .name
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(32)
                    .collect::<String>();
                candidate.objects.push(SceneObject {
                    id: self.next_object_id,
                    position,
                    rotation: [0.; 3],
                    scale,
                    color: [1.; 3],
                    label,
                    material: PhysicalMaterial {
                        preset: MaterialPreset::Custom,
                        ..PhysicalMaterial::default()
                    },
                    fixed: true,
                    mesh_ref: Some(id),
                    mass_override: mass,
                });
                candidate.assets.push(*asset);
            }
            Command::Undo | Command::Redo => unreachable!(),
        }
        candidate
            .assets
            .retain(|a| candidate.objects.iter().any(|o| o.mesh_ref == Some(a.id)));
        candidate.validate()?;
        if candidate.name == self.project.name
            && candidate.objects == self.project.objects
            && candidate.physics == self.project.physics
            && candidate.robots == self.project.robots
        {
            return Ok(self.snapshot());
        }
        let scene = candidate.render_scene()?;
        let (revision, sequence) = self.counters()?;
        let next_version = self
            .next_version
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("history identity exhausted".into()))?;
        self.undo.push_back(Version {
            project: std::mem::replace(&mut self.project, candidate),
            id: self.current_version,
        });
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
        self.current_version = self.next_version;
        self.next_version = next_version;
        self.scene = scene;
        self.revision = revision;
        self.sequence = sequence;
        self.next_object_id = next;
        Ok(self.snapshot())
    }
    fn travel(&mut self, backward: bool) -> Result<Snapshot> {
        let stack = if backward { &self.undo } else { &self.redo };
        let version = stack.back().ok_or(Error::HistoryEmpty)?.clone();
        let scene = version.project.render_scene()?;
        let (revision, sequence) = self.counters()?;
        let previous = Version {
            project: std::mem::replace(&mut self.project, version.project),
            id: self.current_version,
        };
        if backward {
            self.undo.pop_back();
            self.redo.push_back(previous);
        } else {
            self.redo.pop_back();
            self.undo.push_back(previous);
        }
        self.current_version = version.id;
        self.scene = scene;
        self.revision = revision;
        self.sequence = sequence;
        Ok(self.snapshot())
    }
    pub fn begin_transform(
        &mut self,
        owner: &str,
        expected: u64,
        project_id: Uuid,
        id: u32,
    ) -> Result<Snapshot> {
        self.check(expected, project_id)?;
        if self.has_preview() {
            return Err(Error::EditBusy);
        }
        let object = self
            .project
            .objects
            .iter()
            .find(|o| o.id == id)
            .ok_or_else(|| Error::Invalid("missing object".into()))?
            .clone();
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("sequence exhausted".into()))?;
        self.preview = Some(Preview {
            owner: owner.into(),
            object,
            scene: None,
        });
        self.sequence = sequence;
        Ok(self.snapshot())
    }
    pub fn preview_transform(
        &mut self,
        owner: &str,
        expected: u64,
        project_id: Uuid,
        transform: Transform,
    ) -> Result<Snapshot> {
        self.check(expected, project_id)?;
        let p = self
            .preview
            .as_ref()
            .filter(|p| p.owner == owner)
            .ok_or(Error::EditBusy)?;
        if Transform::from(&p.object) == transform {
            return Ok(self.snapshot());
        }
        let mut project = self.project.clone();
        let object = project
            .objects
            .iter_mut()
            .find(|o| o.id == p.object.id)
            .ok_or_else(|| Error::Invalid("missing object".into()))?;
        transform.assign(object);
        let object = object.clone();
        let scene = project.render_scene()?;
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("sequence exhausted".into()))?;
        self.preview = Some(Preview {
            owner: owner.into(),
            object,
            scene: Some(scene),
        });
        self.sequence = sequence;
        Ok(self.snapshot())
    }
    pub fn cancel_transform(&mut self, owner: &str) -> Result<Snapshot> {
        if self.preview.is_none() {
            return Ok(self.snapshot());
        }
        if !self.preview_owned_by(owner) {
            return Err(Error::EditBusy);
        }
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("sequence exhausted".into()))?;
        self.preview = None;
        self.sequence = sequence;
        Ok(self.snapshot())
    }
    pub fn commit_transform(
        &mut self,
        owner: &str,
        expected: u64,
        project_id: Uuid,
    ) -> Result<Snapshot> {
        self.check(expected, project_id)?;
        if !self.preview_owned_by(owner) {
            return Err(Error::EditBusy);
        }
        let p = self.preview.take().unwrap();
        let sequence = self.sequence;
        match self.apply(EditRequest {
            expected_revision: expected,
            project_id,
            command: Command::Transform {
                id: p.object.id,
                transform: Transform::from(&p.object),
            },
        }) {
            Ok(_) => {
                if self.sequence == sequence {
                    self.sequence = self.sequence.saturating_add(1);
                }
                Ok(self.snapshot())
            }
            Err(e) => {
                self.preview = Some(p);
                Err(e)
            }
        }
    }
    pub fn begin_import(&mut self, expected: u64, project_id: Uuid) -> Result<u64> {
        self.check(expected, project_id)?;
        if self.has_preview() {
            return Err(Error::EditBusy);
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("sequence exhausted".into()))?;
        self.import_token = Some(self.sequence);
        Ok(self.sequence)
    }
    pub fn finish_import(&mut self, token: u64, command: Result<Command>) -> Result<Snapshot> {
        if self.import_token != Some(token) {
            return Err(Error::EditBusy);
        }
        self.import_token = None;
        self.sequence = self.sequence.saturating_add(1);
        self.apply(EditRequest {
            expected_revision: self.revision,
            project_id: self.project.id,
            command: command?,
        })
    }
    pub fn begin_simulation(&mut self, expected: u64, project_id: Uuid) -> Result<u64> {
        self.check(expected, project_id)?;
        if self.preview.is_some() || self.simulation_token.is_some() || self.import_token.is_some()
        {
            return Err(Error::EditBusy);
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("sequence exhausted".into()))?;
        self.simulation = None;
        self.simulation_token = Some(self.sequence);
        Ok(self.sequence)
    }
    pub fn finish_simulation(
        &mut self,
        token: u64,
        result: Result<mujoco::SimulationResult>,
        steps: u32,
    ) -> Result<Snapshot> {
        if self.simulation_token != Some(token) {
            return Ok(self.snapshot());
        }
        self.simulation_token = None;
        self.sequence = self.sequence.saturating_add(1);
        let result = result?;
        mujoco::validate_result(&self.project, &result, steps)?;
        let mut project = self.project.clone();
        for pose in &result.poses {
            let o = project
                .objects
                .iter_mut()
                .find(|o| o.id == pose.id)
                .ok_or(Error::PhysicsFailed)?;
            o.position = pose.position;
            o.rotation = pose.rotation;
        }
        let scene = project.render_scene()?;
        self.simulation = Some((project, scene, result));
        Ok(self.snapshot())
    }
    pub fn reset_simulation(&mut self) -> Snapshot {
        self.simulation = None;
        self.simulation_token = None;
        self.sequence = self.sequence.saturating_add(1);
        self.snapshot()
    }
    pub fn reload(&mut self, project: Project) -> Result<()> {
        if self.has_preview() {
            return Err(Error::EditBusy);
        }
        if self.snapshot().dirty {
            return Err(Error::Unsaved);
        }
        let (revision, sequence) = self.counters()?;
        let mut replacement = Self::new(project)?;
        replacement.revision = revision;
        replacement.sequence = sequence;
        replacement.next_object_id = replacement.next_object_id.max(self.next_object_id);
        *self = replacement;
        Ok(())
    }
}
