//! Reuse the exact editor command path on a detached authoring/history candidate.
use super::*;
use serde_json::{Value, json};
#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub path: String,
    pub before: Value,
    pub after: Value,
}
#[derive(Clone, Debug, Serialize)]
pub struct DryRun {
    pub project_id: Uuid,
    pub revision: u64,
    pub sequence: u64,
    pub would_change: bool,
    pub predicted_revision: u64,
    pub undo_depth_before: usize,
    pub undo_depth_after: usize,
    pub redo_depth_after: usize,
    pub changes: Vec<Change>,
    pub total_changes: usize,
    pub truncated: bool,
    pub warnings: Vec<String>,
}
impl AppCore {
    pub fn dry_run(&self, request: EditRequest) -> Result<DryRun> {
        self.check(request.expected_revision, request.project_id)?;
        if self.has_preview() {
            return Err(Error::EditBusy);
        }
        // Preserve IDs, save points and both history stacks; AppCore::new alone would
        // incorrectly predict Undo/Redo and reuse deleted object IDs.
        let mut fork = Self::new(self.project.clone())?;
        fork.revision = self.revision;
        fork.sequence = self.sequence;
        fork.next_object_id = self.next_object_id;
        fork.current_version = self.current_version;
        fork.next_version = self.next_version;
        fork.saved_version = self.saved_version;
        fork.undo = self.undo.clone();
        fork.redo = self.redo.clone();
        let is_physical = matches!(
            &request.command,
            Command::SetGravity { .. }
                | Command::SetMaterial { .. }
                | Command::SetFloor { .. }
                | Command::SetJointAngle { .. }
                | Command::SetJointRange { .. }
                | Command::SolveIk { .. }
        );
        let result = fork.apply(request)?;
        let before = &self.project;
        let after = &fork.project;
        let mut changes = vec![];
        let mut push = |path: String, a: Value, b: Value| {
            if a != b {
                changes.push(Change {
                    path,
                    before: a,
                    after: b,
                });
            }
        };
        push("/name".into(), json!(before.name), json!(after.name));
        push(
            "/physics".into(),
            json!(before.physics),
            json!(after.physics),
        );
        let ids: BTreeSet<_> = before
            .objects
            .iter()
            .chain(&after.objects)
            .map(|o| o.id)
            .collect();
        for id in ids {
            push(
                format!("/objects/{id}"),
                json!(before.objects.iter().find(|o| o.id == id)),
                json!(after.objects.iter().find(|o| o.id == id)),
            );
        }
        let roots: BTreeSet<_> = before
            .robots
            .iter()
            .chain(&after.robots)
            .map(|r| r.root)
            .collect();
        for root in roots {
            let a = before.robots.iter().find(|r| r.root == root);
            let b = after.robots.iter().find(|r| r.root == root);
            // Joint angles/ranges can move descendants while rest geometry stays unchanged.
            if let (Some(a), Some(b)) = (a, b) {
                for link in &a.links {
                    push(
                        format!("/robots/{root}/links/{}", link.object_id),
                        json!(link),
                        json!(b.links.iter().find(|l| l.object_id == link.object_id)),
                    );
                }
            } else {
                push(format!("/robots/{root}"), json!(a), json!(b));
            }
        }
        let total_changes = changes.len();
        changes.truncate(64);
        Ok(DryRun {
            project_id: self.project.id,
            revision: self.revision,
            sequence: self.sequence,
            would_change: result.revision != self.revision,
            predicted_revision: result.revision,
            undo_depth_before: self.undo.len(),
            undo_depth_after: result.undo_depth,
            redo_depth_after: result.redo_depth,
            changes,
            total_changes,
            truncated: total_changes > 64,
            warnings: if is_physical {
                vec!["validation_only_no_physics_or_collision_test".into()]
            } else {
                vec![]
            },
        })
    }
}
