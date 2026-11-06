//! Bounded CPU worker jobs. Only complete, hash-checked job directories can resume/evaluate.
use super::*;
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    sync::mpsc,
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub seed: u32,
    pub max_steps: u32,
    pub max_seconds: u32,
    pub batch_size: u32,
    pub learning_rate: f64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            seed: 7,
            max_steps: 200,
            max_seconds: 60,
            batch_size: 32,
            learning_rate: 0.001,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        if !(1..=10000).contains(&self.max_steps)
            || !(1..=300).contains(&self.max_seconds)
            || !(1..=256).contains(&self.batch_size)
            || !self.learning_rate.is_finite()
            || !(0.000001..=0.1).contains(&self.learning_rate)
        {
            return Err(err("learning_job_budget"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Job {
    pub directory: PathBuf,
    pub manifest_sha256: String,
    pub result: serde_json::Value,
}
fn read(path: &Path, cap: u64) -> Result<Vec<u8>> {
    if !std::fs::symlink_metadata(path)
        .map_err(|_| err("learning_checkpoint_invalid"))?
        .is_file()
    {
        return Err(err("learning_checkpoint_invalid"));
    }
    let mut b = vec![];
    std::fs::File::open(path)
        .map_err(|_| err("learning_io"))?
        .take(cap + 1)
        .read_to_end(&mut b)
        .map_err(|_| err("learning_io"))?;
    if b.len() as u64 > cap {
        return Err(err("learning_size_limit"));
    }
    Ok(b)
}
type CheckedJob = (serde_json::Value, Vec<(String, Vec<u8>)>);
fn checked_job(path: &Path) -> Result<CheckedJob> {
    if !std::fs::symlink_metadata(path)
        .map_err(|_| err("learning_checkpoint_invalid"))?
        .is_dir()
    {
        return Err(err("learning_checkpoint_invalid"));
    }
    let manifest = read(&path.join("manifest.json"), 4096)?;
    let m: serde_json::Value =
        serde_json::from_slice(&manifest).map_err(|_| err("learning_checkpoint_invalid"))?;
    if m["schema_version"] != 1
        || m["format"] != "atom-bc-job-v1"
        || m["files"].as_object().is_none_or(|v| v.len() != 3)
    {
        return Err(err("learning_checkpoint_invalid"));
    }
    let mut files = vec![];
    let mut report = None;
    for name in ["last.pt", "best.pt", "report.json"] {
        let bytes = read(
            &path.join(name),
            if name == "report.json" {
                256 * 1024
            } else {
                8 * 1024 * 1024
            },
        )?;
        if m["files"][name].as_str() != Some(hash(&bytes).as_str()) {
            return Err(err("learning_checkpoint_hash"));
        }
        if name == "report.json" {
            report = Some(
                serde_json::from_slice::<serde_json::Value>(&bytes)
                    .map_err(|_| err("learning_checkpoint_invalid"))?,
            );
        }
        files.push((name.into(), bytes));
    }
    files.push(("manifest.json".into(), manifest));
    let r = report.unwrap();
    if r["schema_version"] != 1
        || r["kind"] != "training"
        || r["source_sha256"] != m["source_sha256"]
        || r["artifact_sha256"] != m["artifact_sha256"]
    {
        return Err(err("learning_checkpoint_invalid"));
    }
    Ok((r, files))
}
pub fn train(
    data: &Validated,
    cfg: &Config,
    out: &Path,
    resume: Option<&Path>,
    cancel: &AtomicBool,
) -> Result<Job> {
    execute(data, cfg, out, resume, cancel, false)
}
pub fn evaluate(data: &Validated, job: &Path, out: &Path, cancel: &AtomicBool) -> Result<Job> {
    let (report, _) = checked_job(job)?;
    let cfg: Config = serde_json::from_value(report["config"].clone())
        .map_err(|_| err("learning_checkpoint_invalid"))?;
    execute(data, &cfg, out, Some(job), cancel, true)
}
fn execute(
    data: &Validated,
    cfg: &Config,
    out: &Path,
    resume: Option<&Path>,
    cancel: &AtomicBool,
    evaluate: bool,
) -> Result<Job> {
    cfg.validate()?;
    if !data.summary.fit_data_ready || !data.summary.test_data_ready {
        return Err(err("learning_job_requires_splits"));
    }
    if out.exists() {
        return Err(err("learning_output_conflict"));
    }
    let parent = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = std::fs::canonicalize(parent).map_err(|_| err("learning_io"))?;
    let stage = tempfile::tempdir_in(&parent).map_err(|_| err("learning_io"))?;
    let scripts = stage.path().join("scripts");
    std::fs::create_dir(&scripts).map_err(|_| err("learning_io"))?;
    std::fs::write(
        scripts.join("adapter.py"),
        include_str!("../learning_worker.py"),
    )
    .map_err(|_| err("learning_io"))?;
    std::fs::write(scripts.join("train.py"), include_str!("train_worker.py"))
        .map_err(|_| err("learning_io"))?;
    let artifact = scripts.join("data.bc.json");
    let artifact_hash = data.prepare()?.save(&artifact)?;
    let mut resume_arg = PathBuf::from("-");
    if let Some(path) = resume {
        let (report, files) = checked_job(path)?;
        if report["artifact_sha256"] != artifact_hash
            || report["source_sha256"] != data.summary.source_sha256
        {
            return Err(err("learning_checkpoint_data_mismatch"));
        }
        resume_arg = scripts.join("resume");
        std::fs::create_dir(&resume_arg).map_err(|_| err("learning_io"))?;
        for (name, bytes) in files {
            std::fs::write(resume_arg.join(name), bytes).map_err(|_| err("learning_io"))?;
        }
    }
    let config = scripts.join("request.json");
    std::fs::write(&config,serde_json::to_vec(&json!({"op":if evaluate {"evaluate"} else {"train"},"artifact_sha256":artifact_hash,"config":cfg})).unwrap()).map_err(|_|err("learning_io"))?;
    let cancel_path = scripts.join("cancel");
    let mut child = Process(
        Command::new(python_executable())
            .arg("-I")
            .arg(scripts.join("train.py"))
            .arg(&artifact)
            .arg(config)
            .arg(stage.path())
            .arg(resume_arg)
            .arg(&cancel_path)
            .env_remove("ATOM_RUNTIME_TOKEN")
            .current_dir(&scripts)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| err("learning_python_unavailable"))?,
    );
    let stdout = child.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut b = vec![];
        let r = stdout.take(256 * 1024 + 1).read_to_end(&mut b).map(|_| b);
        let _ = tx.send(r);
    });
    let deadline = Instant::now() + Duration::from_secs(u64::from(cfg.max_seconds) + 30);
    let mut cancelled = None;
    let bytes = loop {
        if cancel.load(Ordering::Relaxed) && cancelled.is_none() {
            std::fs::write(&cancel_path, b"cancel").map_err(|_| err("learning_io"))?;
            cancelled = Some(Instant::now());
        }
        if cancelled.is_some_and(|t| t.elapsed() > Duration::from_secs(5)) {
            return Err(err("learning_job_cancelled"));
        }
        if Instant::now() > deadline {
            return Err(err("learning_job_timeout"));
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(v) => break v.map_err(|_| err("learning_worker_protocol"))?,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err(err("learning_worker_protocol")),
        }
    };
    if bytes.len() > 256 * 1024 {
        return Err(err("learning_worker_protocol"));
    }
    loop {
        match child
            .0
            .try_wait()
            .map_err(|_| err("learning_worker_protocol"))?
        {
            Some(s) if s.success() => break,
            Some(_) => {
                let response: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(|_| err("learning_worker_protocol"))?;
                let code = response["code"]
                    .as_str()
                    .unwrap_or("learning_training_failed");
                return Err(err(
                    if matches!(
                        code,
                        "learning_checkpoint_config"
                            | "learning_checkpoint_step"
                            | "learning_job_cancelled"
                            | "learning_job_budget"
                            | "learning_nonfinite"
                    ) {
                        code
                    } else {
                        "learning_training_failed"
                    },
                ));
            }
            None => {
                if Instant::now() > deadline {
                    return Err(err("learning_job_timeout"));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    let response: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| err("learning_worker_protocol"))?;
    let r = &response["result"];
    if response["ok"] != true
        || r["schema_version"] != 1
        || r["source_sha256"] != data.summary.source_sha256
        || r["artifact_sha256"] != artifact_hash
    {
        return Err(err("learning_worker_protocol"));
    }
    let mut files = vec![];
    let manifest_hash = if evaluate {
        if r["kind"] != "evaluation" || r["trained"] != false {
            return Err(err("learning_worker_protocol"));
        }
        let bytes = read(&stage.path().join("evaluation.json"), 256 * 1024)?;
        if serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .as_ref()
            != Some(r)
        {
            return Err(err("learning_worker_protocol"));
        }
        let receipt=serde_json::to_vec(&json!({"schema_version":1,"format":"atom-bc-evaluation-v1","source_sha256":data.summary.source_sha256,"evaluation_sha256":hash(&bytes)})).unwrap();
        let digest = hash(&receipt);
        files.push(("evaluation.json".into(), bytes));
        files.push(("manifest.json".into(), receipt));
        digest
    } else {
        if r["kind"] != "training"
            || r["weights_updated"] != true
            || r["tensor_batch_verified"] != true
            || r["checkpoint_reload_verified"] != true
            || r["step"]
                .as_u64()
                .is_none_or(|n| n == 0 || n > u64::from(cfg.max_steps))
            || r["rows"] != serde_json::to_value(&data.summary.rows).unwrap()
        {
            return Err(err("learning_worker_protocol"));
        }
        let (report, checked) = checked_job(stage.path())?;
        if &report != r {
            return Err(err("learning_worker_protocol"));
        }
        let digest = hash(&read(&stage.path().join("manifest.json"), 4096)?);
        files = checked;
        digest
    };
    // Exclusive directory; manifest is the final commit marker. Existing outputs are never replaced.
    std::fs::create_dir(out).map_err(|_| err("learning_output_conflict"))?;
    files.sort_by_key(|(n, _)| n == "manifest.json");
    for (name, bytes) in files {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(out.join(name))
            .map_err(|_| err("learning_io"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| err("learning_io"))?;
    }
    #[cfg(unix)]
    std::fs::File::open(out)
        .and_then(|f| f.sync_all())
        .and_then(|_| std::fs::File::open(&parent)?.sync_all())
        .map_err(|_| err("learning_io"))?;
    Ok(Job {
        directory: out.to_owned(),
        manifest_sha256: manifest_hash,
        result: r.clone(),
    })
}

/// Inspect a committed checkpoint directory without deserializing Python objects.
pub fn inspect(path: &Path) -> Result<Job> {
    let (result, _) = checked_job(path)?;
    Ok(Job {
        directory: path.to_owned(),
        manifest_sha256: hash(&read(&path.join("manifest.json"), 4096)?),
        result,
    })
}
pub fn valid_name(name: &str) -> bool {
    name.starts_with("bc-")
        && name.len() <= 96
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
pub fn catalog(dir: &Path) -> Result<Vec<String>> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut names = vec![];
    for entry in std::fs::read_dir(dir)
        .map_err(|_| err("learning_io"))?
        .filter_map(|e| e.ok())
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !valid_name(&name) || !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        if read(&entry.path().join("manifest.json"), 4096)
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .is_some_and(|m| m["format"] == "atom-bc-job-v1")
        {
            names.push(name);
        }
    }
    names.sort();
    names.reverse();
    names.truncate(64);
    Ok(names)
}
