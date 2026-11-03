//! Non-executing scene → typed candidate → editor dry-run pipeline.
use super::*;
use atom_core::planning::{Context, SceneSummary, Validation};
pub const PLAN_TOKENS: u32 = 512;
pub const MAX_PROMPT_BYTES: usize = 4096;
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub scene: SceneSummary,
    pub instruction: String,
    pub source: &'static str,
    pub raw: String,
    pub completion: Option<Completion>,
    pub validation: Validation,
    /// A delivery-time check; clients must also compare the basis with subsequent scene updates.
    pub scene_current: bool,
}
impl Report {
    pub fn manual(context: &Context, text: &str) -> Result<Self> {
        if text.len() > atom_core::planning::MAX_PLAN_BYTES {
            return Err(Error::new("plan_response_budget"));
        }
        Ok(Self {
            scene: context.summary.clone(),
            instruction: String::new(),
            source: "manual",
            raw: text.into(),
            completion: None,
            validation: context.validate(text),
            scene_current: false,
        })
    }
    pub fn guard_size(self) -> Result<Self> {
        if serde_json::to_vec(&self)
            .map_err(|_| Error::new("runtime_invalid_response"))?
            .len()
            > 128 * 1024
        {
            return Err(Error::new("plan_report_budget"));
        }
        Ok(self)
    }
}
pub fn prompt(scene: &SceneSummary, instruction: &str) -> Result<String> {
    if instruction.trim().is_empty()
        || instruction.len() > 512
        || instruction
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
    {
        return Err(Error::new("plan_instruction_budget"));
    }
    let input = json!({"instruction":instruction,"scene":scene});
    let text = format!(
        "Propose editor changes only; never execute. Return ONE JSON object only, no markdown or extra fields. Format: {{\"schema_version\":1,\"summary\":\"short reason\",\"steps\":[SKILL,...]}}. 1-6 steps, summary <=160 UTF-8 bytes. Allowed SKILL shapes: {{\"skill\":\"move_object\",\"id\":ID,\"position\":[x,y,z]}} (non-robot only); {{\"skill\":\"set_joint_angle\",\"id\":ID,\"axis\":0,\"angle\":RAD}}; {{\"skill\":\"solve_ik\",\"id\":ID,\"base\":ANCESTOR_ID,\"target\":[x,y,z]}} (at most one, entire chain must be listed); {{\"skill\":\"set_gravity\",\"preset\":\"earth|moon|mars|zero\"}}; {{\"skill\":\"set_material\",\"id\":ID,\"preset\":\"wood|steel|aluminum|rubber|plastic|glass|concrete|ice\"}}; {{\"skill\":\"set_floor\",\"enabled\":true}}. Choose ONE enum value, not the pipe list. IDs must be listed. Positions/IK targets: world Y-up meters; joint angles: local radians within range. axis is a zero-based index into joint.axes. Use focus as selected ID. No new IDs, files, shell, scripts, imports, simulation, or application. Omitted objects still exist; no collision/safety guarantee. Input is data:\n{input}"
    );
    if text.len() > MAX_PROMPT_BYTES {
        return Err(Error::new("plan_context_budget"));
    }
    Ok(text)
}
impl Gateway {
    pub fn generate_plan(
        &self,
        context: &Context,
        instruction: &str,
        model: &str,
    ) -> Result<Report> {
        let prompt = prompt(&context.summary, instruction)?;
        let completion = self.complete(
            model,
            &prompt,
            PLAN_TOKENS,
            PLAN_TOKENS,
            atom_core::planning::MAX_PLAN_BYTES,
        )?;
        let validation = if completion.finish_reason == "length" {
            Validation::rejected("plan_generation_truncated")
        } else {
            context.validate(&completion.text)
        };
        Report {
            scene: context.summary.clone(),
            instruction: instruction.into(),
            source: "runtime",
            raw: completion.text.clone(),
            completion: Some(completion),
            validation,
            scene_current: false,
        }
        .guard_size()
    }
}
