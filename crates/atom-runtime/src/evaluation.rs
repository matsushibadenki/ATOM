//! Fixed, public evaluation-only tasks. Never consumes or changes a user's document.
use super::*;
use atom_core::{AppCore, Project, planning::Context};
use std::collections::{BTreeMap, BTreeSet};
pub const SUITE_ID: &str = "atom-skillplan-holdout-v1";
pub const MAX_BUNDLE_BYTES: usize = 128 * 1024;
struct Case {
    id: &'static str,
    instruction: &'static str,
    context: Context,
    reference: String,
}
fn cases() -> Vec<Case> {
    let specs = [
        (
            "move-en",
            "Move object 1 to world position [1,2,3] meters. Preserve everything else.",
            json!([{"skill":"move_object","id":1,"position":[1,2,3]}]),
        ),
        (
            "gravity-ja",
            "重力を月のプリセットに変更してください。それ以外は維持してください。",
            json!([{"skill":"set_gravity","preset":"moon"}]),
        ),
        (
            "material-zh",
            "将物体1的物理材质设为橡胶，其余保持不变。",
            json!([{"skill":"set_material","id":1,"preset":"rubber"}]),
        ),
        (
            "compound-en",
            "Move object 1 to [0.5,1,0] meters and disable the floor. Preserve everything else.",
            json!([{"skill":"move_object","id":1,"position":[0.5,1,0]},{"skill":"set_floor","enabled":false}]),
        ),
        (
            "noop-ja",
            "床を有効にしてください。それ以外は維持してください。",
            json!([{"skill":"set_floor","enabled":true}]),
        ),
        (
            "joint-zh",
            "将选中的关节的第0轴角度设为0.25弧度，其余保持不变。",
            json!([{"skill":"set_joint_angle","id":2,"axis":0,"angle":0.25}]),
        ),
    ];
    specs
        .into_iter()
        .map(|(id, instruction, steps)| {
            let mut project = if id == "joint-zh" {
                serde_json::from_str(include_str!("../../../templates/dual-arm.atom.json")).unwrap()
            } else {
                Project::new("Evaluation fixture").unwrap()
            };
            project.id =
                serde_json::from_value(json!("af003415-e305-45d1-91ce-a22d77398ba1")).unwrap();
            let context = AppCore::new(project)
                .unwrap()
                .planning_context(Some(if id == "joint-zh" { 2 } else { 1 }))
                .unwrap();
            Case {
                id,
                instruction,
                context,
                reference: json!({"schema_version":1,"summary":"Reference", "steps":steps})
                    .to_string(),
            }
        })
        .collect()
}
fn fingerprint(cases: &[Case]) -> String {
    // Includes exact prompts, initial scenes and scoring references. Any change invalidates old bundles.
    let values: Vec<_> = cases.iter().map(|c| json!({"id":c.id,"prompt":planning::prompt(&c.context.summary,c.instruction).unwrap(),"reference":c.reference})).collect();
    digest(
        &serde_json::to_vec(
            &json!({"suite":SUITE_ID,"scorer":"exact_net_authoring_diff_v1","cases":values}),
        )
        .unwrap(),
    )
}
pub fn manifest() -> Value {
    let cases = cases();
    json!({"schema_version":1,"suite_id":SUITE_ID,"suite_sha256":fingerprint(&cases),"use":"evaluation_only_public_not_training","scorer":"exact_net_authoring_diff_v1","cases":cases.iter().map(|c| json!({"case_id":c.id,"instruction":c.instruction,"scene":c.context.summary,"prompt":planning::prompt(&c.context.summary,c.instruction).unwrap()})).collect::<Vec<_>>()})
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub schema_version: u32,
    pub suite_sha256: String,
    pub candidates: Vec<Candidate>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub case_id: String,
    pub outcome: Outcome,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    Generated { raw: String, finish_reason: String },
    Failure { code: String },
}
#[derive(Debug, Serialize)]
pub struct Row {
    pub case_id: String,
    pub category: &'static str,
    pub codes: Vec<String>,
    pub failed_step: Option<usize>,
    pub response_sha256: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Evaluation {
    pub schema_version: u32,
    pub suite_id: &'static str,
    pub suite_sha256: String,
    pub candidates_sha256: String,
    pub total: usize,
    pub passed: usize,
    pub counts: BTreeMap<String, usize>,
    pub cases: Vec<Row>,
}
fn delta(v: &atom_core::DryRun) -> Value {
    json!(
        v.changes
            .iter()
            .map(|c| (&c.path, &c.after))
            .collect::<BTreeMap<_, _>>()
    )
}
pub fn evaluate(bundle: &Bundle) -> Result<Evaluation> {
    let cases = cases();
    if bundle.schema_version != 1 || bundle.suite_sha256 != fingerprint(&cases) {
        return Err(Error::new("evaluation_suite_mismatch"));
    }
    if bundle.candidates.len() > cases.len() {
        return Err(Error::new("evaluation_case_mismatch"));
    }
    let ids: BTreeSet<_> = cases.iter().map(|c| c.id).collect();
    let mut seen = BTreeSet::new();
    for c in &bundle.candidates {
        if !ids.contains(c.case_id.as_str()) || !seen.insert(&c.case_id) {
            return Err(Error::new("evaluation_case_mismatch"));
        }
        match &c.outcome {
            Outcome::Generated { raw, finish_reason }
                if raw.len() <= atom_core::planning::MAX_PLAN_BYTES
                    && matches!(finish_reason.as_str(), "stop" | "length") => {}
            Outcome::Failure { code }
                if !code.is_empty()
                    && code.len() <= 96
                    && code
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') => {}
            _ => return Err(Error::new("evaluation_candidate_budget")),
        }
    }
    let mut rows = vec![];
    for c in &cases {
        let mut row = Row {
            case_id: c.id.into(),
            category: "missing",
            codes: vec![],
            failed_step: None,
            response_sha256: None,
        };
        if let Some(candidate) = bundle.candidates.iter().find(|v| v.case_id == c.id) {
            match &candidate.outcome {
                Outcome::Failure { code } => {
                    row.category = "generation_failure";
                    row.codes.push(code.clone());
                }
                Outcome::Generated { raw, finish_reason } => {
                    row.response_sha256 = Some(digest(raw.as_bytes()));
                    if finish_reason == "length" {
                        row.category = "truncated";
                    } else {
                        let validation = c.context.validate(raw);
                        if !validation.valid {
                            row.category = "validation_failure";
                            row.failed_step = validation.issues.first().and_then(|i| i.step);
                            row.codes = validation.issues.into_iter().map(|i| i.code).collect();
                        } else {
                            let expected = c.context.validate(&c.reference);
                            let expected = expected
                                .dry_run
                                .filter(|d| expected.valid && !d.truncated)
                                .ok_or_else(|| Error::new("evaluation_reference_invalid"))?;
                            let actual = validation.dry_run.unwrap();
                            row.category =
                                if !actual.truncated && delta(&actual) == delta(&expected) {
                                    "passed"
                                } else {
                                    "intent_mismatch"
                                };
                        }
                    }
                }
            }
        }
        rows.push(row);
    }
    let mut counts = BTreeMap::new();
    for row in &rows {
        *counts.entry(row.category.to_owned()).or_insert(0) += 1;
    }
    // Stable input hash independent of candidate ordering.
    let canonical: BTreeMap<_, _> = bundle
        .candidates
        .iter()
        .map(|c| (&c.case_id, &c.outcome))
        .collect();
    Ok(Evaluation {
        schema_version: 1,
        suite_id: SUITE_ID,
        suite_sha256: fingerprint(&cases),
        candidates_sha256: digest(&serde_json::to_vec(&canonical).unwrap()),
        total: rows.len(),
        passed: *counts.get("passed").unwrap_or(&0),
        counts,
        cases: rows,
    })
}
/// Sequential, six bounded requests; failures remain in the denominator. No retries.
pub fn generate(gateway: &Gateway, model: &str) -> (Bundle, Vec<Value>) {
    let cases = cases();
    let mut evidence = vec![];
    let candidates = cases
        .iter()
        .map(|c| {
            let outcome = match gateway.generate_plan(&c.context, c.instruction, model) {
                Ok(report) => {
                    let completion = report.completion.unwrap();
                    evidence.push(json!({"case_id":c.id,"completion":completion}));
                    Outcome::Generated {
                        raw: report.raw,
                        finish_reason: completion.finish_reason,
                    }
                }
                Err(e) => Outcome::Failure { code: e.code },
            };
            Candidate {
                case_id: c.id.into(),
                outcome,
            }
        })
        .collect();
    (
        Bundle {
            schema_version: 1,
            suite_sha256: fingerprint(&cases),
            candidates,
        },
        evidence,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn references() -> Bundle {
        let cases = cases();
        Bundle {
            schema_version: 1,
            suite_sha256: fingerprint(&cases),
            candidates: cases
                .iter()
                .map(|c| Candidate {
                    case_id: c.id.into(),
                    outcome: Outcome::Generated {
                        raw: c.reference.clone(),
                        finish_reason: "stop".into(),
                    },
                })
                .collect(),
        }
    }
    #[test]
    fn references_pass_and_manifest_is_reproducible_without_answers() {
        assert_eq!(manifest(), manifest());
        let bundle = references();
        let report = evaluate(&bundle).unwrap();
        assert_eq!((report.passed, report.total), (6, 6));
        let manifest = manifest();
        for case in manifest["cases"].as_array().unwrap() {
            assert!(case.get("reference").is_none());
            assert!(case["prompt"].as_str().unwrap().len() <= planning::MAX_PROMPT_BYTES);
        }
        let mut reversed = bundle.clone();
        reversed.candidates.reverse();
        assert_eq!(
            report.candidates_sha256,
            evaluate(&reversed).unwrap().candidates_sha256
        );
    }
    #[test]
    fn all_failures_stay_in_denominator_and_valid_wrong_intent_fails() {
        let mut b = references();
        b.candidates[0].outcome = Outcome::Generated {
            raw: cases()[4].reference.clone(),
            finish_reason: "stop".into(),
        };
        b.candidates[1].outcome = Outcome::Failure {
            code: "runtime_timeout".into(),
        };
        b.candidates[2].outcome = Outcome::Generated {
            raw: "{}".into(),
            finish_reason: "stop".into(),
        };
        b.candidates[3].outcome = Outcome::Generated {
            raw: cases()[3].reference.clone(),
            finish_reason: "length".into(),
        };
        b.candidates.pop();
        let r = evaluate(&b).unwrap();
        assert_eq!((r.passed, r.total), (1, 6));
        for key in [
            "passed",
            "missing",
            "truncated",
            "generation_failure",
            "validation_failure",
            "intent_mismatch",
        ] {
            assert_eq!(r.counts[key], 1);
        }
    }
    #[test]
    fn rejects_suite_drift_duplicate_unknown_and_oversized_inputs() {
        let mut b = references();
        b.suite_sha256 = "wrong".into();
        assert_eq!(evaluate(&b).unwrap_err().code, "evaluation_suite_mismatch");
        let mut b = references();
        b.candidates.push(b.candidates[0].clone());
        assert!(evaluate(&b).is_err());
        let mut b = references();
        b.candidates[0].case_id = "unknown".into();
        assert!(evaluate(&b).is_err());
        let mut b = references();
        b.candidates[0].outcome = Outcome::Generated {
            raw: "x".repeat(8193),
            finish_reason: "stop".into(),
        };
        assert!(evaluate(&b).is_err());
    }
    #[test]
    fn equivalent_final_state_passes_but_unrequested_side_effect_fails() {
        let mut b = references();
        let plan = |steps: Value| {
            json!({"schema_version":1,"summary":"Alternative sequence","steps":steps}).to_string()
        };
        b.candidates[0].outcome = Outcome::Generated {
            raw: plan(
                json!([{"skill":"move_object","id":1,"position":[0,1,0]},{"skill":"move_object","id":1,"position":[1,2,3]}]),
            ),
            finish_reason: "stop".into(),
        };
        assert_eq!(evaluate(&b).unwrap().passed, 6);
        b.candidates[0].outcome = Outcome::Generated {
            raw: plan(
                json!([{"skill":"move_object","id":1,"position":[1,2,3]},{"skill":"set_floor","enabled":false}]),
            ),
            finish_reason: "stop".into(),
        };
        assert_eq!(evaluate(&b).unwrap().cases[0].category, "intent_mismatch");
    }
}
