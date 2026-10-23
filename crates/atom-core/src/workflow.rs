//! Foundation workflow: generate and validate task specifications, without simulating them.
use crate::{Error, Result};
use futures::future::BoxFuture;
use std::{collections::BTreeMap, sync::Arc};
use unge_core::{Cardinality, Command, DataType, Document, Edge, Editor, Endpoint, Id, Port, Rect};
use unge_executor::{
    Cancellation, Definition, ExecutionContext, Inputs, LocalizedText, NodeExecutor, Outputs,
    Registry, Scheduler, Value,
};

struct TaskSet;
impl NodeExecutor for TaskSet {
    fn execute(
        &self,
        _: ExecutionContext,
        _: Inputs,
    ) -> BoxFuture<'_, std::result::Result<Outputs, String>> {
        Box::pin(async {
            let tasks: Vec<_> = (0..8)
                .map(|seed| serde_json::json!({"task": "reach", "seed": seed}))
                .collect();
            Ok(BTreeMap::from([(
                "tasks".into(),
                Value::Json(serde_json::json!(tasks)),
            )]))
        })
    }
}
struct Validate;
impl NodeExecutor for Validate {
    fn execute(
        &self,
        _: ExecutionContext,
        inputs: Inputs,
    ) -> BoxFuture<'_, std::result::Result<Outputs, String>> {
        Box::pin(async move {
            let Some(Value::Json(value)) = inputs.get("tasks").and_then(|v| v.first()) else {
                return Err("missing tasks".into());
            };
            let tasks = value.as_array().ok_or("tasks must be an array")?;
            if tasks.is_empty()
                || tasks.len() > 64
                || tasks
                    .iter()
                    .any(|t| t["task"] != "reach" || t["seed"].as_u64().is_none())
            {
                return Err("invalid task specification".into());
            }
            Ok(BTreeMap::from([(
                "count".into(),
                Value::Int(tasks.len() as i64),
            )]))
        })
    }
}
fn text(en: &str, ja: &str, zh_cn: &str) -> LocalizedText {
    LocalizedText {
        en: en.into(),
        ja: ja.into(),
        zh_cn: zh_cn.into(),
    }
}
fn port(name: &str, data_type: DataType) -> Port {
    Port {
        name: name.into(),
        data_type,
        cardinality: Cardinality::Single,
        required: true,
    }
}
pub fn registry() -> Registry {
    let mut r = Registry::default();
    r.register(
        Definition {
            type_id: "atom.task_set".into(),
            version: "1".into(),
            name: text("Tasks", "タスク", "任务"),
            description: text(
                "Generate eight Reach specifications",
                "Reachの設定を8件生成",
                "生成8个Reach配置",
            ),
            inputs: vec![],
            outputs: vec![port("tasks", DataType::Json)],
            pure: true,
            property_schema: Default::default(),
        },
        Arc::new(TaskSet),
    )
    .expect("static task definition");
    r.register(
        Definition {
            type_id: "atom.validate_tasks".into(),
            version: "1".into(),
            name: text("Validate", "検証", "验证"),
            description: text(
                "Validate task specifications",
                "タスク設定を検証",
                "验证任务配置",
            ),
            inputs: vec![port("tasks", DataType::Json)],
            outputs: vec![port("count", DataType::Int)],
            pure: true,
            property_schema: Default::default(),
        },
        Arc::new(Validate),
    )
    .expect("static validation definition");
    r
}
pub fn initial_document() -> Result<Document> {
    let registry = Arc::new(registry());
    let a = registry
        .definition("atom.task_set")
        .expect("registered")
        .instantiate();
    let b = registry
        .definition("atom.validate_tasks")
        .expect("registered")
        .instantiate();
    let edge = Edge {
        id: Id::new_v4(),
        from: Endpoint {
            node: a.id,
            port: "tasks".into(),
        },
        to: Endpoint {
            node: b.id,
            port: "tasks".into(),
        },
    };
    let mut editor = Editor::new(Document::default(), 32)
        .and_then(|e| e.with_validator(registry))
        .map_err(|e| Error::Invalid(e.to_string()))?;
    editor
        .execute(Command::Batch {
            commands: vec![
                Command::AddNode {
                    node: a,
                    rect: Rect {
                        x: 40.,
                        y: 80.,
                        ..Default::default()
                    },
                },
                Command::AddNode {
                    node: b,
                    rect: Rect {
                        x: 360.,
                        y: 80.,
                        ..Default::default()
                    },
                },
                Command::Connect { edge },
            ],
        })
        .map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(editor.document().clone())
}
#[derive(Debug, serde::Serialize)]
pub struct ValidationReport {
    pub completed_nodes: usize,
    pub task_count: i64,
    pub simulated_episodes: usize,
}
pub async fn validate_tasks(document: &Document) -> Result<ValidationReport> {
    let registry = registry();
    let report = Scheduler::new(1, 32)
        .run(document.graph(), &registry, Cancellation::default())
        .await
        .map_err(Error::Invalid)?;
    if report.nodes.len() != 2
        || report
            .nodes
            .values()
            .any(|n| n.status != unge_executor::Status::Completed)
    {
        return Err(Error::Invalid("workflow did not complete".into()));
    }
    let task_count = report
        .nodes
        .values()
        .find_map(|n| match n.outputs.get("count") {
            Some(Value::Int(n)) => Some(*n),
            _ => None,
        })
        .ok_or_else(|| Error::Invalid("missing validation output".into()))?;
    Ok(ValidationReport {
        completed_nodes: report.nodes.len(),
        task_count,
        simulated_episodes: 0,
    })
}
