use super::*;
use crate::planning::{
    Axis, Basis, Context, Joint, MAX_SCENE_BYTES, MAX_SCENE_OBJECTS, Object, SceneSummary,
};
impl AppCore {
    pub fn planning_summary(&self, focus: Option<u32>) -> Result<SceneSummary> {
        if self.has_preview() {
            return Err(Error::EditBusy);
        }
        if focus.is_some_and(|id| !self.project.objects.iter().any(|o| o.id == id)) {
            return Err(Error::Invalid("planning focus".into()));
        }
        let mut ids = vec![];
        if let Some(id) = focus {
            ids.push(id);
            let mut cursor = id;
            for _ in 0..MAX_OBJECTS {
                let parent = self
                    .project
                    .robots
                    .iter()
                    .flat_map(|r| &r.links)
                    .find(|l| l.object_id == cursor)
                    .and_then(|l| l.parent);
                if let Some(p) = parent {
                    ids.push(p);
                    cursor = p;
                } else {
                    break;
                }
            }
        }
        for o in &self.project.objects {
            if !ids.contains(&o.id) {
                ids.push(o.id)
            }
        }
        let mut summary = SceneSummary {
            schema_version: 1,
            basis: Basis {
                project_id: self.project.id,
                revision: self.revision,
                sequence: self.sequence,
            },
            focus,
            coordinate_frame: "posed_world_y_up_m_rad",
            gravity: self.project.physics.gravity.clone(),
            floor_enabled: self.project.physics.floor_enabled,
            total_objects: self.project.objects.len(),
            omitted_objects: self.project.objects.len(),
            objects: vec![],
        };
        for id in ids.into_iter().take(MAX_SCENE_OBJECTS) {
            let o = self.posed_objects.iter().find(|o| o.id == id).unwrap();
            let link = self
                .project
                .robots
                .iter()
                .flat_map(|r| &r.links)
                .find(|l| l.object_id == id);
            let joint = link.filter(|l| l.parent.is_some()).map(|l| Joint {
                parent: l.parent.unwrap(),
                axes: l
                    .axes()
                    .enumerate()
                    .map(|(i, a)| Axis {
                        axis: a.axis,
                        range: a.range,
                        angle: l.angle(i),
                    })
                    .collect(),
            });
            summary.objects.push(Object {
                id,
                kind: if link.is_some() {
                    "robot_link"
                } else if o.mesh_ref.is_some() {
                    "mesh"
                } else {
                    "box"
                },
                position: o.position,
                rotation: o.rotation,
                scale: o.scale,
                fixed: o.fixed,
                joint,
            });
            summary.omitted_objects = summary.total_objects - summary.objects.len();
            if serde_json::to_vec(&summary)?.len() > MAX_SCENE_BYTES {
                summary.objects.pop();
                summary.omitted_objects = summary.total_objects - summary.objects.len();
                break;
            }
        }
        Ok(summary)
    }
    pub fn planning_context(&self, focus: Option<u32>) -> Result<Context> {
        Ok(Context {
            summary: self.planning_summary(focus)?,
            core: self.fork_authoring()?,
        })
    }
}

impl AppCore {
    pub fn planning_matches(&self, basis: &Basis) -> bool {
        !self.has_preview()
            && basis.project_id == self.project.id
            && basis.revision == self.revision
            && basis.sequence == self.sequence
    }
    pub fn planning_context_at(&self, basis: &Basis, focus: Option<u32>) -> Result<Context> {
        self.check(basis.revision, basis.project_id)?;
        if basis.sequence != self.sequence {
            return Err(Error::Conflict(self.revision));
        }
        self.planning_context(focus)
    }
}
