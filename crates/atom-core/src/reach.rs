//! Reproducible isolated dual-arm Reach v1, bounded recording and portable playback.
mod worker;
use crate::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
};
const DT: f64 = 0.02;
const RADIUS: f64 = 0.035;
const HOLD: u32 = 5;
const MAX_BYTES: u64 = 64 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub seed: u32,
    pub robot: u32,
    pub horizon: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Channel {
    pub id: u32,
    pub axis: usize,
    pub range: [f32; 2],
    pub qpos_index: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub profile: String,
    pub config: Config,
    pub project: Project,
    pub source_sha256: String,
    pub source_revision: Option<u64>,
    pub model_sha256: String,
    pub endpoint: u32,
    pub target: [f32; 3],
    pub channels: Vec<Channel>,
    pub initial_action: Vec<f64>,
    pub reference_action: Vec<f64>,
    pub physics_dt: f64,
    pub substeps: u32,
    pub tolerance: f64,
    pub hold_steps: u32,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn prepare(source: &Project, config: Config) -> Result<Manifest> {
    source.validate()?;
    if !(1..=500).contains(&config.horizon) {
        return Err(Error::Invalid("Reach horizon 1..500".into()));
    }
    let robot = source
        .robots
        .iter()
        .find(|r| r.root == config.robot && r.template == RobotTemplate::DualArm)
        .ok_or(Error::Invalid("Reach requires a dual-arm template".into()))?;
    let ids: std::collections::BTreeSet<_> = robot.links.iter().map(|l| l.object_id).collect();
    let endpoint = robot
        .links
        .iter()
        .find(|l| l.parent.is_some() && !robot.links.iter().any(|c| c.parent == Some(l.object_id)))
        .ok_or_else(|| Error::Invalid("Reach requires an articulated endpoint".into()))?
        .object_id;
    let mut project = source.clone();
    project.objects.retain(|o| ids.contains(&o.id));
    project.robots.retain(|r| r.root == config.robot);
    project.assets.clear();
    project.physics.gravity = Gravity {
        preset: GravityPreset::Zero,
        vector: [0.; 3],
    };
    if project.objects.iter().any(|o| o.mesh_ref.is_some()) {
        return Err(Error::Invalid("Reach v1 requires box links".into()));
    }
    let mut channels: Vec<_> = robot
        .links
        .iter()
        .filter(|l| l.parent.is_some())
        .flat_map(|l| {
            l.axes().enumerate().map(move |(axis, a)| Channel {
                id: l.object_id,
                axis,
                range: a.range,
                qpos_index: 0,
            })
        })
        .collect();
    fn joint_order(r: &robots::Robot, id: u32, out: &mut Vec<(u32, usize)>) {
        let link = r.links.iter().find(|l| l.object_id == id).unwrap();
        if link.parent.is_some() {
            out.extend(link.axes().enumerate().map(|(axis, _)| (id, axis)));
        }
        for child in r.links.iter().filter(|l| l.parent == Some(id)) {
            joint_order(r, child.object_id, out);
        }
    }
    let mut order = Vec::new();
    joint_order(robot, robot.root, &mut order);
    for c in &mut channels {
        c.qpos_index = order.iter().position(|v| *v == (c.id, c.axis)).unwrap();
    }
    let initial_action: Vec<_> = channels
        .iter()
        .map(|a| {
            robot
                .links
                .iter()
                .find(|l| l.object_id == a.id)
                .unwrap()
                .angle(a.axis) as f64
        })
        .collect();
    let mut goal = project.clone();
    let mut state = u64::from(config.seed);
    let mut chain = std::collections::BTreeSet::new();
    let mut cursor = Some(endpoint);
    while let Some(id) = cursor {
        chain.insert(id);
        cursor = robot
            .links
            .iter()
            .find(|l| l.object_id == id)
            .unwrap()
            .parent;
    }
    // SplitMix64-v1, fixed draw order. Generated joint goals are a reference controller,
    // not a learned policy; target position is obtained by FK before any physics runs.
    for (i, c) in channels.iter().enumerate() {
        if chain.contains(&c.id) {
            state = state.wrapping_add(0x9e3779b97f4a7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
            z ^= z >> 31;
            let unit = ((z >> 11) as f64) * (1. / 9007199254740992.);
            let angle = (initial_action[i] + (unit - 0.5) * 0.5)
                .clamp(c.range[0] as f64, c.range[1] as f64) as f32;
            goal.set_joint_angle(c.id, c.axis, angle)?;
        }
    }
    let reference_action = channels
        .iter()
        .map(|c| {
            goal.robots[0]
                .links
                .iter()
                .find(|l| l.object_id == c.id)
                .unwrap()
                .angle(c.axis) as f64
        })
        .collect();
    let target = goal
        .posed_objects()
        .iter()
        .find(|o| o.id == endpoint)
        .unwrap()
        .position;
    Ok(Manifest {
        profile: "atom-reach-dual-arm-zero-g-v1/splitmix64-v1".into(),
        config,
        source_sha256: digest(&serde_json::to_vec(source)?),
        source_revision: None,
        model_sha256: digest(mujoco::export(&project)?.as_bytes()),
        project,
        endpoint,
        target,
        channels,
        initial_action,
        reference_action,
        physics_dt: 0.002,
        substeps: 10,
        tolerance: RADIUS,
        hold_steps: HOLD,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub engine: String,
    pub python: String,
    pub platform: String,
    pub time: f64,
    pub qpos: Vec<f64>,
    pub qvel: Vec<f64>,
    pub endpoint: [f64; 3],
    pub poses: Vec<mujoco::Pose>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Running,
    Success,
    Timeout,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub index: u32,
    /// Action held over (previous.time, this.time]; None only at reset.
    pub action: Option<Vec<f64>>,
    pub observation: Observation,
    pub distance: f64,
    pub outcome: Outcome,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Episode {
    pub schema_version: u32,
    pub id: Uuid,
    pub manifest: Manifest,
    pub frames: Vec<Frame>,
}
fn distance(endpoint: [f64; 3], target: [f32; 3]) -> f64 {
    endpoint
        .iter()
        .zip(target)
        .map(|(a, b)| (a - f64::from(b)).powi(2))
        .sum::<f64>()
        .sqrt()
}
fn action_valid(m: &Manifest, a: &[f64]) -> bool {
    a.len() == m.channels.len()
        && a.iter()
            .zip(&m.channels)
            .all(|(v, c)| v.is_finite() && *v >= c.range[0] as f64 && *v <= c.range[1] as f64)
}
fn observation_valid(m: &Manifest, o: &Observation, index: u32) -> bool {
    let expected: std::collections::BTreeSet<_> = m.project.objects.iter().map(|o| o.id).collect();
    let actual: std::collections::BTreeSet<_> = o.poses.iter().map(|o| o.id).collect();
    expected == actual
        && o.poses.len() == expected.len()
        && (o.time - index as f64 * DT).abs() < 1e-7
        && o.time.is_finite()
        && o.engine.starts_with("MuJoCo ")
        && o.engine.len() <= 80
        && !o.python.is_empty()
        && o.python.len() <= 40
        && !o.platform.is_empty()
        && o.platform.len() <= 200
        && o.qpos.len() == m.channels.len()
        && o.qvel.len() == m.channels.len()
        && o.qpos
            .iter()
            .chain(&o.qvel)
            .chain(&o.endpoint)
            .all(|v| v.is_finite() && v.abs() <= 1e8)
        && o.poses.iter().all(|p| {
            p.position
                .iter()
                .chain(&p.rotation)
                .all(|v| v.is_finite() && v.abs() <= 10_000.)
        })
        && o.poses
            .iter()
            .find(|p| p.id == m.endpoint)
            .is_some_and(|p| {
                p.position
                    .iter()
                    .zip(o.endpoint)
                    .all(|(a, b)| (f64::from(*a) - b).abs() < 1e-4)
            })
}
fn initial_valid(m: &Manifest, o: &Observation) -> bool {
    let expected = m.project.posed_objects();
    m.channels
        .iter()
        .enumerate()
        .all(|(i, c)| (o.qpos[c.qpos_index] - m.initial_action[i]).abs() < 1e-6)
        && o.qvel.iter().all(|v| v.abs() < 1e-10)
        && expected.iter().all(|a| {
            o.poses.iter().find(|p| p.id == a.id).is_some_and(|b| {
                a.position
                    .iter()
                    .zip(b.position)
                    .all(|(x, y)| (*x - y).abs() < 1e-4)
            })
        })
}
fn outcome(index: u32, hits: u32, horizon: u32) -> Outcome {
    if hits >= HOLD {
        Outcome::Success
    } else if index >= horizon {
        Outcome::Timeout
    } else {
        Outcome::Running
    }
}
impl Episode {
    pub fn validate(&self) -> Result<()> {
        let bad = || Error::Invalid("invalid or incomplete Episode".into());
        let m = &self.manifest;
        if self.schema_version != 1
            || self.frames.is_empty()
            || self.frames.len() > 501
            || m.source_sha256.len() != 64
            || !m.source_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(bad());
        }
        let rebuilt = prepare(&m.project, m.config.clone())?;
        // Compare the complete deterministic task definition, except original scene provenance.
        let mut expected = rebuilt;
        expected.source_sha256 = m.source_sha256.clone();
        expected.source_revision = m.source_revision;
        if serde_json::to_vec(m)? != serde_json::to_vec(&expected)? {
            return Err(bad());
        }
        let mut hits = 0;
        let mut terminated = false;
        let engine = &self.frames[0].observation.engine;
        let python = &self.frames[0].observation.python;
        let platform = &self.frames[0].observation.platform;
        for (i, f) in self.frames.iter().enumerate() {
            if terminated
                || f.index != i as u32
                || f.index > m.config.horizon
                || !observation_valid(m, &f.observation, f.index)
                || &f.observation.engine != engine
                || &f.observation.python != python
                || &f.observation.platform != platform
            {
                return Err(bad());
            }
            if i == 0 {
                if f.action.is_some() || !initial_valid(m, &f.observation) {
                    return Err(bad());
                }
            } else if !f.action.as_ref().is_some_and(|a| action_valid(m, a)) {
                return Err(bad());
            }
            let d = distance(f.observation.endpoint, m.target);
            if !f.distance.is_finite() || (d - f.distance).abs() > 1e-9 {
                return Err(bad());
            }
            hits = if i > 0 && d <= RADIUS { hits + 1 } else { 0 };
            let result = outcome(f.index, hits, m.config.horizon);
            if f.outcome != result {
                return Err(bad());
            }
            terminated = result != Outcome::Running;
        }
        Ok(())
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        let envelope = serde_json::json!({"sha256":digest(&bytes),"episode":self});
        let bytes = serde_json::to_vec(&envelope)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(Error::Invalid("Episode size".into()));
        }
        if path.exists() {
            let old = Self::load(path)?;
            if serde_json::to_vec(&old)? == serde_json::to_vec(self)? {
                return Ok(());
            }
            return Err(Error::ExternalChange);
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
        tmp.write_all(&bytes)?;
        tmp.as_file().sync_all()?;
        tmp.persist_noclobber(path)
            .map_err(|e| Error::Io(e.error))?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Envelope {
            sha256: String,
            episode: Episode,
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(Error::Invalid("Episode size".into()));
        }
        let envelope: Envelope = serde_json::from_slice(&bytes)?;
        if digest(&serde_json::to_vec(&envelope.episode)?) != envelope.sha256 {
            return Err(Error::Invalid("Episode checksum".into()));
        }
        envelope.episode.validate()?;
        Ok(envelope.episode)
    }
    pub fn objects_at(&self, index: usize) -> Result<Vec<SceneObject>> {
        let frame = self
            .frames
            .get(index)
            .ok_or_else(|| Error::Invalid("Episode frame".into()))?;
        let mut objects = self.manifest.project.objects.clone();
        for p in &frame.observation.poses {
            let o = objects
                .iter_mut()
                .find(|o| o.id == p.id)
                .ok_or(Error::PhysicsFailed)?;
            o.position = p.position;
            o.rotation = p.rotation;
        }
        Ok(objects)
    }
}
pub struct Runner {
    pub episode: Episode,
    worker: worker::Worker,
    hits: u32,
    failed: bool,
}
impl Runner {
    pub fn reset(project: &Project, config: Config) -> Result<Self> {
        let manifest = prepare(project, config)?;
        let mut worker = worker::Worker::start(&manifest)?;
        let o = worker.request(serde_json::json!({"op":"reset"}))?;
        if !observation_valid(&manifest, &o, 0) || !initial_valid(&manifest, &o) {
            return Err(Error::PhysicsFailed);
        }
        let frame = Frame {
            index: 0,
            action: None,
            distance: distance(o.endpoint, manifest.target),
            observation: o,
            outcome: Outcome::Running,
        };
        Ok(Self {
            episode: Episode {
                schema_version: 1,
                id: Uuid::new_v4(),
                manifest,
                frames: vec![frame],
            },
            worker,
            hits: 0,
            failed: false,
        })
    }
    pub fn step(&mut self, action: Option<Vec<f64>>) -> Result<&Frame> {
        let last = self.episode.frames.last().unwrap();
        if self.failed || last.outcome != Outcome::Running {
            return Err(Error::Invalid("Reach already terminated or failed".into()));
        }
        let m = &self.episode.manifest;
        let action = action.unwrap_or_else(|| m.reference_action.clone());
        if !action_valid(m, &action) {
            return Err(Error::Invalid("Reach action".into()));
        }
        let index = last.index + 1;
        let o = self
            .worker
            .request(serde_json::json!({"op":"step","action":action}));
        let o = match o {
            Ok(o)
                if observation_valid(m, &o, index)
                    && o.engine == last.observation.engine
                    && o.python == last.observation.python
                    && o.platform == last.observation.platform =>
            {
                o
            }
            _ => {
                self.failed = true;
                self.worker.stop();
                return Err(Error::PhysicsFailed);
            }
        };
        let d = distance(o.endpoint, m.target);
        self.hits = if d <= RADIUS { self.hits + 1 } else { 0 };
        self.episode.frames.push(Frame {
            index,
            action: Some(action),
            observation: o,
            distance: d,
            outcome: outcome(index, self.hits, m.config.horizon),
        });
        if self.episode.frames.last().unwrap().outcome != Outcome::Running {
            self.worker.stop();
        }
        Ok(self.episode.frames.last().unwrap())
    }
    pub fn run(&mut self) -> Result<()> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while self.episode.frames.last().unwrap().outcome == Outcome::Running {
            if std::time::Instant::now() >= deadline {
                self.failed = true;
                self.worker.stop();
                return Err(Error::PhysicsFailed);
            }
            self.step(None)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub id: Uuid,
    pub seed: u32,
    pub robot: u32,
    pub cursor: usize,
    pub frames: usize,
    pub step: u32,
    pub horizon: u32,
    pub time: f64,
    pub distance: f64,
    pub outcome: Outcome,
    pub final_outcome: Outcome,
    pub target: [f32; 3],
    pub endpoint: u32,
    pub live: bool,
    pub playing: bool,
    pub saved: Option<String>,
}
pub(crate) struct Preview {
    runner: Option<Runner>,
    recording: Option<Episode>,
    pub objects: Vec<SceneObject>,
    pub scene: threex_core::Scene,
    pub cursor: usize,
    pub play_token: Option<u64>,
    saved: Option<String>,
}
impl Preview {
    pub fn live(runner: Runner) -> Result<Self> {
        let mut p = Self {
            runner: Some(runner),
            recording: None,
            objects: vec![],
            scene: threex_core::Scene::default(),
            cursor: 0,
            play_token: None,
            saved: None,
        };
        p.refresh()?;
        Ok(p)
    }
    pub fn loaded(episode: Episode) -> Result<Self> {
        episode.validate()?;
        let mut p = Self {
            runner: None,
            recording: Some(episode),
            objects: vec![],
            scene: threex_core::Scene::default(),
            cursor: 0,
            play_token: None,
            saved: None,
        };
        p.refresh()?;
        Ok(p)
    }
    pub fn episode(&self) -> &Episode {
        self.runner
            .as_ref()
            .map_or_else(|| self.recording.as_ref().unwrap(), |r| &r.episode)
    }
    pub fn summary(&self) -> Summary {
        let e = self.episode();
        let f = &e.frames[self.cursor];
        Summary {
            id: e.id,
            seed: e.manifest.config.seed,
            robot: e.manifest.config.robot,
            cursor: self.cursor,
            frames: e.frames.len(),
            step: f.index,
            horizon: e.manifest.config.horizon,
            time: f.observation.time,
            distance: f.distance,
            outcome: f.outcome,
            final_outcome: e.frames.last().unwrap().outcome,
            target: e.manifest.target,
            endpoint: e.manifest.endpoint,
            live: self.runner.as_ref().is_some_and(|r| !r.failed),
            playing: self.play_token.is_some(),
            saved: self.saved.clone(),
        }
    }
    pub fn refresh(&mut self) -> Result<()> {
        self.objects = self.episode().objects_at(self.cursor)?;
        let mut project = self.episode().manifest.project.clone();
        project.objects = self.objects.clone();
        project.robots.clear();
        // Marker is renderer-only and cannot be selected or written to the authored document.
        let mut marker = project.objects[0].clone();
        marker.id = (1..=crate::MAX_OBJECTS as u32 + 1)
            .find(|id| !project.objects.iter().any(|o| o.id == *id))
            .unwrap();
        marker.label = "Reach target".into();
        marker.position = self.episode().manifest.target;
        marker.rotation = [0.; 3];
        marker.scale = [0.045; 3];
        marker.color = [0.42, 0.85, 0.42];
        marker.fixed = true;
        if project.objects.len() < crate::MAX_OBJECTS {
            project.objects.push(marker);
        }
        self.scene = project.render_scene()?;
        Ok(())
    }
    pub fn step(&mut self, count: u32, action: Option<Vec<f64>>) -> Result<()> {
        if self.play_token.is_some()
            || self.cursor + 1 != self.episode().frames.len()
            || !(1..=500).contains(&count)
        {
            return Err(Error::EditBusy);
        }
        let r = self
            .runner
            .as_mut()
            .ok_or_else(|| Error::Invalid("recorded Episode is read-only".into()))?;
        if r.failed || r.episode.frames.last().unwrap().outcome != Outcome::Running {
            return Err(Error::Invalid("Reach already terminated or failed".into()));
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        for _ in 0..count {
            if std::time::Instant::now() >= deadline {
                r.failed = true;
                r.worker.stop();
                return Err(Error::PhysicsFailed);
            }
            if r.episode.frames.last().unwrap().outcome != Outcome::Running {
                break;
            }
            r.step(action.clone())?;
        }
        self.cursor = self.episode().frames.len() - 1;
        self.refresh()
    }
    pub fn seek(&mut self, index: usize) -> Result<()> {
        if index >= self.episode().frames.len() {
            return Err(Error::Invalid("Episode frame".into()));
        }
        self.cursor = index;
        self.refresh()
    }
    pub fn save(&mut self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        let e = self.episode();
        let name = format!("reach-{}-{}.episode.json", e.id, e.frames.len() - 1);
        e.save(&dir.join(&name))?;
        self.saved = Some(name);
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Reset {
        config: Config,
    },
    Step {
        id: Uuid,
        expected_step: u32,
        action: Option<Vec<f64>>,
    },
    Run {
        id: Uuid,
        expected_step: u32,
    },
    Seek {
        id: Uuid,
        index: usize,
    },
    Play {
        id: Uuid,
    },
    Pause {
        id: Uuid,
    },
    Close {
        id: Uuid,
    },
    Save {
        id: Uuid,
    },
    Load {
        name: String,
    },
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub project_id: Uuid,
    pub expected_revision: u64,
    pub expected_sequence: u64,
    pub command: Command,
}
pub fn valid_name(name: &str) -> bool {
    name.len() <= 100
        && name.starts_with("reach-")
        && name.ends_with(".episode.json")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}
pub fn catalog(dir: &Path) -> Result<Vec<String>> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut names: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| valid_name(n))
        .collect();
    names.sort();
    names.reverse();
    names.truncate(200);
    Ok(names)
}
#[cfg(test)]
mod tests;
