//! Retained, portable mesh assets. Geometry stays in Rust and is absent from UI snapshots.
use crate::{Error, Result, Uuid};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
const WORKER: &str = include_str!("usd_worker.py");
pub const MAX_VERTICES: usize = 300_000;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriangleMesh {
    pub positions: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}
impl TriangleMesh {
    pub fn validate(&self) -> Result<()> {
        let n = self.positions.len();
        if !(3..=MAX_VERTICES).contains(&n)
            || self.colors.len() != n
            || self.indices.len() < 3
            || self.indices.len() > MAX_VERTICES * 3
            || !self.indices.len().is_multiple_of(3)
            || self
                .positions
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > 0.5001)
            || self
                .colors
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            || self.indices.iter().any(|&i| i as usize >= n)
        {
            return Err(Error::Invalid("mesh bounds/topology".into()));
        }
        Ok(())
    }
    pub fn digest(&self) -> String {
        let mut h = Sha256::new();
        h.update(b"ATOM mesh v1");
        for v in self
            .positions
            .iter()
            .flatten()
            .chain(self.colors.iter().flatten())
        {
            h.update(v.to_le_bytes());
        }
        for i in &self.indices {
            h.update(i.to_le_bytes());
        }
        format!("{:x}", h.finalize())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetSource {
    pub kind: String,
    pub repo: String,
    pub revision: String,
    pub package: String,
    pub entry: String,
    pub license: String,
    pub author: String,
    pub source_hash: String,
    pub cache_path: String,
    pub up_axis: String,
    pub meters_per_unit: f64,
    pub prim_paths: Vec<String>,
    pub sdk_version: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportedAsset {
    pub id: Uuid,
    pub name: String,
    pub source: AssetSource,
    pub mesh: Arc<TriangleMesh>,
    pub mesh_hash: String,
    pub warnings: Vec<String>,
}
impl ImportedAsset {
    pub fn validate(&self) -> Result<()> {
        self.mesh.validate()?;
        if self.mesh_hash != self.mesh.digest()
            || self.id.is_nil()
            || self.mesh_hash.len() != 64
            || self.name.is_empty()
            || self.name.len() > 128
            || self.warnings.len() > 32
            || self.warnings.iter().any(|s| s.len() > 128)
            || self.source.prim_paths.len() > 256
            || self.source.prim_paths.iter().any(|s| s.len() > 2048)
            || !self.source.meters_per_unit.is_finite()
            || self.source.meters_per_unit <= 0.
            || !["Y", "Z"].contains(&self.source.up_axis.as_str())
            || [
                &self.source.kind,
                &self.source.repo,
                &self.source.revision,
                &self.source.package,
                &self.source.entry,
                &self.source.license,
                &self.source.author,
                &self.source.source_hash,
                &self.source.cache_path,
                &self.source.sdk_version,
            ]
            .iter()
            .any(|s| s.len() > 4096)
        {
            return Err(Error::Invalid("asset metadata".into()));
        }
        Ok(())
    }
    pub fn summary(&self) -> AssetSummary {
        AssetSummary {
            id: self.id,
            name: self.name.clone(),
            source: self.source.clone(),
            vertices: self.mesh.positions.len(),
            triangles: self.mesh.indices.len() / 3,
            warnings: self.warnings.clone(),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct AssetSummary {
    pub id: Uuid,
    pub name: String,
    pub source: AssetSource,
    pub vertices: usize,
    pub triangles: usize,
    pub warnings: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Converted {
    name: String,
    source: AssetSource,
    mesh: TriangleMesh,
    position: [f32; 3],
    scale: [f32; 3],
    mass: Option<f64>,
    warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ImportSource {
    Local {
        path: String,
    },
    HuggingFace {
        repo: String,
        package: String,
        revision: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Catalog {
    pub repo: String,
    pub revision: String,
    pub packages: Vec<String>,
}
fn python() -> PathBuf {
    if let Some(path) = std::env::var_os("ATOM_USD_PYTHON") {
        return path.into();
    }
    let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.venv-usd/bin/python");
    if bundled.is_file() {
        bundled
    } else {
        PathBuf::from("python3")
    }
}
fn execute(
    request: serde_json::Value,
    seconds: u64,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<serde_json::Value> {
    let temp = tempfile::tempdir()?;
    let input = temp.path().join("request.json");
    let output = temp.path().join("result.json");
    std::fs::write(&input, serde_json::to_vec(&request)?)?;
    let mut child = Command::new(python())
        .args(["-I", "-c", WORKER])
        .arg(&input)
        .arg(&output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| Error::Asset("usd_unavailable".into()))?;
    let start = Instant::now();
    loop {
        if cancel.is_some_and(|c| c.load(std::sync::atomic::Ordering::Relaxed)) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Asset("usd_cancelled".into()));
        }
        match child.try_wait() {
            Ok(Some(s)) if s.success() => break,
            Ok(None) if start.elapsed() < Duration::from_secs(seconds) => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Asset("usd_failed".into()));
            }
        }
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&output)?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(Error::Asset("usd_budget".into()));
    }
    let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
    if let Some(code) = value.get("error").and_then(|v| v.as_str()) {
        eprintln!(
            "ATOM USD: {}",
            value.get("detail").unwrap_or(&serde_json::Value::Null)
        );
        return Err(Error::Asset(code.into()));
    }
    value
        .get_mut("ok")
        .map(serde_json::Value::take)
        .ok_or_else(|| Error::Asset("usd_failed".into()))
}
pub fn catalog(repo: &str, revision: &str) -> Result<Catalog> {
    let result: Catalog = serde_json::from_value(execute(
        serde_json::json!({"mode":"catalog","repo":repo,"revision":revision}),
        90,
        None,
    )?)?;
    if result.packages.len() > 8000 || result.packages.iter().any(|s| s.len() > 1024) {
        return Err(Error::Asset("usd_budget".into()));
    }
    Ok(result)
}
/// Returns an import command for the shared editor. The command is never deserialized from JS.
pub fn prepare(source: ImportSource, cache: &Path) -> Result<crate::Command> {
    prepare_cancellable(source, cache, None)
}
pub fn prepare_cancellable(
    source: ImportSource,
    cache: &Path,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<crate::Command> {
    let mut request = serde_json::json!({"cache":cache});
    match source {
        ImportSource::Local { path } => {
            request["mode"] = "local".into();
            request["path"] = path.into();
        }
        ImportSource::HuggingFace {
            repo,
            package,
            revision,
        } => {
            request["mode"] = "hub".into();
            request["repo"] = repo.into();
            request["package"] = package.into();
            request["revision"] = revision.into();
        }
    }
    let data: Converted = serde_json::from_value(execute(request, 180, cancel)?)?;
    let mesh_hash = data.mesh.digest();
    let asset = ImportedAsset {
        id: Uuid::new_v4(),
        name: data.name,
        source: data.source,
        mesh: Arc::new(data.mesh),
        mesh_hash,
        warnings: data.warnings,
    };
    asset.validate()?;
    Ok(crate::Command::ImportAsset {
        asset: Box::new(asset),
        position: data.position,
        scale: data.scale,
        mass: data.mass,
    })
}
