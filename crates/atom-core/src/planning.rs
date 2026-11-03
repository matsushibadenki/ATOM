//! Bounded authored-scene context and a non-executable, allowlisted SkillPlan v1.
use crate::*;
pub const MAX_SCENE_BYTES: usize = 2048;
pub const MAX_SCENE_OBJECTS: usize = 8;
pub const MAX_PLAN_BYTES: usize = 8192;
pub const MAX_STEPS: usize = 6;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Basis {
    pub project_id: Uuid,
    pub revision: u64,
    pub sequence: u64,
}
impl Basis {
    pub fn snapshot(s: &Snapshot) -> Self {
        Self {
            project_id: s.project_id,
            revision: s.revision,
            sequence: s.sequence,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Joint {
    pub parent: u32,
    pub axes: Vec<Axis>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Axis {
    pub axis: [f32; 3],
    pub range: [f32; 2],
    pub angle: f32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Object {
    pub id: u32,
    pub kind: &'static str,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub fixed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joint: Option<Joint>,
}
#[derive(Clone, Debug, Serialize)]
pub struct SceneSummary {
    pub schema_version: u32,
    pub basis: Basis,
    pub focus: Option<u32>,
    pub coordinate_frame: &'static str,
    #[serde(rename = "gravity_z_up")]
    pub gravity: Gravity,
    pub floor_enabled: bool,
    pub total_objects: usize,
    pub omitted_objects: usize,
    pub objects: Vec<Object>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillPlan {
    pub schema_version: u32,
    pub summary: String,
    pub steps: Vec<Skill>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "skill", rename_all = "snake_case", deny_unknown_fields)]
pub enum Skill {
    MoveObject {
        id: u32,
        position: [f32; 3],
    },
    SetJointAngle {
        id: u32,
        axis: usize,
        angle: f32,
    },
    SolveIk {
        id: u32,
        base: u32,
        target: [f32; 3],
    },
    SetGravity {
        preset: GravityPreset,
    },
    SetMaterial {
        id: u32,
        preset: MaterialPreset,
    },
    SetFloor {
        enabled: bool,
    },
}
#[derive(Clone, Debug, Serialize)]
pub struct Issue {
    pub step: Option<usize>,
    pub code: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct StepPreview {
    pub index: usize,
    pub would_change: bool,
    pub predicted_revision: u64,
    pub total_changes: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct Validation {
    pub valid: bool,
    pub plan: Option<SkillPlan>,
    pub issues: Vec<Issue>,
    pub steps: Vec<StepPreview>,
    pub dry_run: Option<DryRun>,
}
impl Validation {
    pub fn rejected(code: &str) -> Self {
        Self {
            valid: false,
            plan: None,
            issues: vec![Issue {
                step: None,
                code: code.into(),
            }],
            steps: vec![],
            dry_run: None,
        }
    }
}
pub struct Context {
    pub summary: SceneSummary,
    pub(crate) core: AppCore,
}
impl Context {
    pub fn validate(&self, text: &str) -> Validation {
        if text.len() > MAX_PLAN_BYTES {
            return Validation::rejected("plan_response_budget");
        }
        let plan = match serde_json::from_str::<SkillPlan>(text) {
            Ok(p) => p,
            Err(_) => return Validation::rejected("plan_invalid_json"),
        };
        let mut report = Validation {
            valid: false,
            plan: Some(plan.clone()),
            issues: vec![],
            steps: vec![],
            dry_run: None,
        };
        let bad = if plan.schema_version != 1 {
            Some("plan_version")
        } else if plan.summary.trim().is_empty()
            || plan.summary.len() > 160
            || plan.summary.chars().any(char::is_control)
        {
            Some("plan_invalid_summary")
        } else if plan.steps.is_empty() || plan.steps.len() > MAX_STEPS {
            Some("plan_step_budget")
        } else if plan
            .steps
            .iter()
            .filter(|s| matches!(s, Skill::SolveIk { .. }))
            .count()
            > 1
        {
            Some("plan_ik_budget")
        } else {
            None
        };
        if let Some(code) = bad {
            report.issues.push(Issue {
                step: None,
                code: code.into(),
            });
            return report;
        }
        let mut trial = match self.core.fork_authoring() {
            Ok(c) => c,
            Err(_) => return Validation::rejected("plan_scene_busy"),
        };
        for (index, skill) in plan.steps.iter().enumerate() {
            let result = (|| -> std::result::Result<DryRun, String> {
                let command = skill.command(&trial, &self.summary)?;
                let s = trial.snapshot();
                let dry = trial
                    .dry_run(EditRequest {
                        project_id: s.project_id,
                        expected_revision: s.revision,
                        command,
                    })
                    .map_err(|e| e.code().to_owned())?;
                let command = skill.command(&trial, &self.summary)?;
                trial
                    .apply(EditRequest {
                        project_id: s.project_id,
                        expected_revision: s.revision,
                        command,
                    })
                    .map_err(|e| e.code().to_owned())?;
                Ok(dry)
            })();
            match result {
                Ok(d) => report.steps.push(StepPreview {
                    index,
                    would_change: d.would_change,
                    predicted_revision: d.predicted_revision,
                    total_changes: d.total_changes,
                }),
                Err(code) => {
                    report.issues.push(Issue {
                        step: Some(index),
                        code,
                    });
                    report.steps.clear();
                    return report;
                }
            }
        }
        let mut dry = self.core.authoring_diff(&trial, true);
        if self.summary.omitted_objects > 0 {
            dry.warnings.push("plan_summary_truncated".into());
        }
        report.valid = true;
        report.dry_run = Some(dry);
        report
    }
}
impl Skill {
    fn command(
        &self,
        core: &AppCore,
        scene: &SceneSummary,
    ) -> std::result::Result<Command, String> {
        let scoped = |id: &u32| {
            if scene.objects.iter().any(|o| o.id == *id) {
                Ok(())
            } else {
                Err("plan_object_out_of_scope".to_owned())
            }
        };
        Ok(match *self {
            Self::MoveObject { id, position } => {
                scoped(&id)?;
                if core
                    .project()
                    .robots
                    .iter()
                    .any(|r| r.links.iter().any(|l| l.object_id == id))
                {
                    return Err("plan_robot_move_unsupported".into());
                }
                let o = core
                    .project()
                    .objects
                    .iter()
                    .find(|o| o.id == id)
                    .ok_or("plan_object_out_of_scope")?;
                if position.iter().any(|v| !v.is_finite()) {
                    return Err("plan_invalid_number".into());
                }
                let mut transform = Transform::from(o);
                transform.position = position;
                Command::Transform { id, transform }
            }
            Self::SetJointAngle { id, axis, angle } => {
                scoped(&id)?;
                let link = core
                    .project()
                    .robots
                    .iter()
                    .flat_map(|r| &r.links)
                    .find(|l| l.object_id == id && l.parent.is_some())
                    .ok_or("plan_joint_missing")?;
                let spec = link.axes().nth(axis).ok_or("plan_joint_missing")?;
                if !angle.is_finite() || angle < spec.range[0] || angle > spec.range[1] {
                    return Err("plan_joint_limit".into());
                }
                Command::SetJointAngle { id, axis, angle }
            }
            Self::SolveIk { id, base, target } => {
                scoped(&id)?;
                scoped(&base)?;
                let robot = core
                    .project()
                    .robots
                    .iter()
                    .find(|r| r.links.iter().any(|l| l.object_id == id))
                    .ok_or("plan_joint_missing")?;
                let mut cursor = id;
                let mut found = false;
                for _ in 0..robot.links.len() {
                    scoped(&cursor)?;
                    if cursor == base {
                        found = true;
                        break;
                    }
                    cursor = robot
                        .links
                        .iter()
                        .find(|l| l.object_id == cursor)
                        .and_then(|l| l.parent)
                        .ok_or("plan_ik_chain")?;
                }
                if !found || id == base || target.iter().any(|v| !v.is_finite()) {
                    return Err("plan_ik_chain".into());
                }
                Command::SolveIk { id, base, target }
            }
            Self::SetGravity { preset } => Command::SetGravity {
                gravity: Gravity {
                    preset,
                    vector: preset.vector().ok_or("plan_custom_preset")?,
                },
            },
            Self::SetMaterial { id, preset } => {
                scoped(&id)?;
                if preset == MaterialPreset::Custom {
                    return Err("plan_custom_preset".into());
                }
                Command::SetMaterial {
                    id: Some(id),
                    material: PhysicalMaterial::preset(preset),
                }
            }
            Self::SetFloor { enabled } => Command::SetFloor { enabled },
        })
    }
}
