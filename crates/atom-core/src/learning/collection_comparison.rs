//! Paired comparison of collection rankings on one frozen probe pool.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub mining: String,
    pub attempts: u32,
    pub collection_seconds: u32,
    pub validation_seconds: u32,
    pub training: training::Config,
}

pub fn run(
    root: &Path,
    project: &crate::Project,
    config: &Config,
    cancel: &AtomicBool,
) -> Result<serde_json::Value> {
    config.training.validate()?;
    let pool = mining::inspect(root, &config.mining)?;
    let report = &pool.report;
    if report.status != mining::Status::Completed
        || !(1..report.config.count).contains(&config.attempts)
        || !(1..=300).contains(&config.collection_seconds)
        || !(1..=300).contains(&config.validation_seconds)
    {
        return Err(err("learning_collection_comparison_config"));
    }
    let source = Validated::load(&root.join("exports").join(&report.source))?;
    let validation_tasks = source.summary.groups[&Split::Validation];
    if validation_tasks < 8 {
        return Err(err("learning_collection_comparison_validation_count"));
    }
    rollout::validation_spec(&source, project)?;
    let baseline = mining::distance_baseline(root, &pool.file)?;
    let initial = registry::select(root, &report.config.policy)?;
    let parent = root.join("training").join(&initial.job);
    let id = crate::Uuid::new_v4();
    let mut arms = Vec::new();
    let mut policies = Vec::new();
    for (method, mining) in [
        ("final_distance", baseline.file),
        ("train_distribution", pool.file.clone()),
    ] {
        let collected = collection::run(
            root,
            &report.source,
            project,
            &collection::Config {
                start_seed: report.config.start_seed,
                count: config.attempts,
                horizon: report.config.horizon,
                max_seconds: config.collection_seconds,
                mining: Some(mining),
            },
            cancel,
            |_| {},
        )?;
        if collected.report.status != collection::Status::Completed {
            return Err(err("learning_collection_comparison_incomplete"));
        }
        let batch = collected
            .report
            .exported
            .as_deref()
            .ok_or_else(|| err("learning_collection_comparison_no_demos"))?;
        let data = Validated::load(&root.join("exports").join(batch))?;
        let job = format!("bc-collect-{id}-{}", arms.len());
        let trained = training::warm_start_validation(
            &data,
            &config.training,
            &root.join("training").join(&job),
            &parent,
            cancel,
        )?;
        if trained.result["status"] != "completed"
            || trained.result["step"] != config.training.max_steps
            || cancel.load(Ordering::Relaxed)
        {
            return Err(err("learning_collection_comparison_incomplete"));
        }
        let policy = registry::register(root, &job, batch)?;
        arms.push(
            json!({"method":method,"collection":collected.file,"batch":batch,"policy":policy.id,
            "seeds":collected.report.entries.iter().map(|entry| entry.seed).collect::<Vec<_>>(),
            "accepted":collected.report.accepted,"best_step":trained.result["best_step"]}),
        );
        policies.push(policy.id);
    }
    let evaluation = campaign::run(
        root,
        project,
        &campaign::Config {
            policies,
            max_seconds: config.validation_seconds,
        },
        cancel,
        |_| {},
    )?;
    for (arm, candidate) in arms.iter_mut().zip(&evaluation.report.candidates) {
        arm["validation"] = json!(candidate.score);
    }
    let saturated = report
        .probes
        .iter()
        .filter(|probe| probe.steps > 0 && probe.outside_train_rows == Some(probe.steps as usize))
        .count();
    let result = json!({"format":"atom-collection-comparison-v1","id":id,"config":config,
        "initial_policy":report.config.policy,"initial_checkpoint_sha256":initial.checkpoint_sha256,
        "validation_tasks":validation_tasks,"campaign":evaluation.file,"status":evaluation.report.status,
        "saturated_probes":saturated,"total_probes":report.probes.len(),
        "saturated_fraction":saturated as f64 / report.probes.len() as f64,"arms":arms});
    collection::write_atomic(
        &root
            .join("collection-comparisons")
            .join(format!("{id}.json")),
        &result,
    )?;
    Ok(result)
}
