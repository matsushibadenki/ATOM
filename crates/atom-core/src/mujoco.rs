//! Bounded headless MuJoCo adapter. Rust owns configuration, files and lifecycle.
use crate::*;
use serde::{Deserialize, Serialize};
use std::{
    fmt::Write as _,
    io::Read,
    process::{Command as Process, Stdio},
    time::{Duration, Instant},
};
const WORKER: &str = include_str!("mujoco_worker.py");
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pose {
    pub id: u32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulationResult {
    pub engine: String,
    pub time: f64,
    pub contacts: usize,
    pub poses: Vec<Pose>,
}
fn vec3(v: impl IntoIterator<Item = impl std::fmt::Display>) -> String {
    v.into_iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}
fn geom(out: &mut String, o: &SceneObject, origin: [f32; 3]) {
    let p: [f32; 3] = std::array::from_fn(|i| o.position[i] - origin[i]);
    // xyz intrinsic rotations match ThreeX's Rx * Ry * Rz. Geometry uses full extents.
    let shape = if o.mesh_ref.is_some() {
        format!(
            "type=\"mesh\" mesh=\"m{}\" user=\"1 {}\"",
            o.id,
            vec3(o.rotation)
        )
    } else {
        String::from("type=\"box\"")
    };
    let size = if o.mesh_ref.is_none() {
        format!(" size=\"{}\"", vec3(o.scale.map(|v| v / 2.)))
    } else {
        String::new()
    };
    let mass = if let Some(mass) = o.mass_override {
        format!("mass=\"{mass}\"")
    } else {
        format!("density=\"{}\"", o.material.density)
    };
    let _ = writeln!(
        out,
        "<geom name=\"o{}\" {} pos=\"{}\" euler=\"{}\"{} {} friction=\"{}\" condim=\"6\" rgba=\"{} 1\"/>",
        o.id,
        shape,
        vec3(p),
        vec3(o.rotation),
        size,
        mass,
        vec3(o.material.friction),
        vec3(o.color)
    );
}
/// MJCF uses editor Y-up to avoid lossy Euler conversion. Canonical gravity is rotated here.
pub fn export(project: &Project) -> Result<String> {
    project.validate()?;
    let mut out = format!(
        "<mujoco model=\"ATOM\"><compiler angle=\"radian\" eulerseq=\"xyz\"/><option timestep=\"0.002\" gravity=\"{}\" integrator=\"implicitfast\"/><worldbody>\n",
        vec3(project.physics.gravity.editor_vector())
    );
    let mut assets = String::from("<asset>");
    for object in project.objects.iter().filter(|o| o.mesh_ref.is_some()) {
        let asset = project
            .assets
            .iter()
            .find(|a| Some(a.id) == object.mesh_ref)
            .unwrap();
        let positions = asset
            .mesh
            .positions
            .iter()
            .flat_map(|p| p.iter())
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let faces = asset
            .mesh
            .indices
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let _ = write!(
            assets,
            "<mesh name=\"m{}\" vertex=\"{}\" face=\"{}\" scale=\"{}\"/>",
            object.id,
            positions,
            faces,
            vec3(object.scale)
        );
    }
    assets.push_str("</asset><worldbody>");
    if !project.assets.is_empty() {
        out = out.replacen("<worldbody>", "<size nuser_geom=\"4\"/><worldbody>", 1);
        out = out.replacen("<worldbody>", &assets, 1);
    }
    if project.physics.floor_enabled {
        let _ = writeln!(
            out,
            "<geom name=\"floor\" type=\"plane\" size=\"0 0 0.1\" quat=\"0.7071067811865476 -0.7071067811865476 0 0\" friction=\"{}\" condim=\"6\" rgba=\"0.55 0.52 0.46 1\"/>",
            vec3(project.physics.floor.friction)
        );
    }
    let robot_ids: std::collections::BTreeSet<_> = project
        .robots
        .iter()
        .flat_map(|r| r.links.iter().map(|l| l.object_id))
        .collect();
    for o in &project.objects {
        if robot_ids.contains(&o.id) {
            continue;
        }
        let _ = writeln!(
            out,
            "<body name=\"b{}\" pos=\"{}\">",
            o.id,
            vec3(o.position)
        );
        if !o.fixed {
            out.push_str("<freejoint/>\n");
        }
        geom(&mut out, o, o.position);
        out.push_str("</body>\n");
    }
    fn link(
        out: &mut String,
        project: &Project,
        robot: &robots::Robot,
        l: &robots::RobotLink,
        parent_origin: [f32; 3],
    ) {
        let origin = l.anchor;
        let relative: [f32; 3] = std::array::from_fn(|i| origin[i] - parent_origin[i]);
        let _ = writeln!(
            out,
            "<body name=\"b{}\" pos=\"{}\">",
            l.object_id,
            vec3(relative)
        );
        if l.parent.is_some() {
            if !l.extra_axes.is_empty()
                || robot.links.iter().any(|link| !link.extra_axes.is_empty())
            {
                for (i, a) in l.axes().enumerate() {
                    let _ = writeln!(
                        out,
                        "<joint name=\"j{}_{}\" type=\"hinge\" axis=\"{}\" limited=\"true\" range=\"{}\" damping=\"0.1\" armature=\"0.001\"/>",
                        l.object_id,
                        i,
                        vec3(a.axis),
                        vec3(a.range)
                    );
                }
            } else {
                let _ = writeln!(
                    out,
                    "<joint name=\"j{}\" type=\"hinge\" axis=\"{}\" limited=\"true\" range=\"{}\" damping=\"5\" armature=\"0.02\"/>",
                    l.object_id,
                    vec3(l.axis),
                    vec3(l.range)
                );
            }
        } else if robot.template == robots::RobotTemplate::Humanoid {
            out.push_str("<freejoint/>\n");
        }
        geom(
            out,
            project
                .objects
                .iter()
                .find(|o| o.id == l.object_id)
                .unwrap(),
            origin,
        );
        for child in robot
            .links
            .iter()
            .filter(|child| child.parent == Some(l.object_id))
        {
            link(out, project, robot, child, origin);
        }
        out.push_str("</body>\n");
    }
    for robot in &project.robots {
        link(&mut out, project, robot, &robot.links[0], [0.; 3]);
    }
    out.push_str("</worldbody><contact>\n");
    if project.physics.floor_enabled {
        for o in &project.objects {
            let f: [f64; 3] = std::array::from_fn(|i| {
                (o.material.friction[i] * project.physics.floor.friction[i]).sqrt()
            });
            let _ = writeln!(
                out,
                "<pair geom1=\"floor\" geom2=\"o{}\" condim=\"6\" friction=\"{} {} {} {} {}\"/>",
                o.id, f[0], f[0], f[1], f[2], f[2]
            );
        }
    }
    out.push_str("</contact><actuator>\n");
    for robot in &project.robots {
        for l in robot.links.iter().filter(|l| l.parent.is_some()) {
            if !l.extra_axes.is_empty()
                || robot.links.iter().any(|link| !link.extra_axes.is_empty())
            {
                let object = project
                    .objects
                    .iter()
                    .find(|o| o.id == l.object_id)
                    .unwrap();
                let small = object.label.contains("finger_")
                    || object.label.contains("toe_")
                    || object.label == "jaw";
                let (kp, force) = if small { (2., 2.) } else { (100., 100.) };
                for (i, a) in l.axes().enumerate() {
                    let _ = writeln!(
                        out,
                        "<position name=\"hold{}_{}\" joint=\"j{}_{}\" kp=\"{}\" ctrllimited=\"true\" ctrlrange=\"{}\" forcelimited=\"true\" forcerange=\"-{} {}\"/>",
                        l.object_id,
                        i,
                        l.object_id,
                        i,
                        kp,
                        vec3(a.range),
                        force,
                        force
                    );
                }
                continue;
            }
            let _ = writeln!(
                out,
                "<position name=\"hold{}\" joint=\"j{}\" kp=\"100\" ctrllimited=\"true\" ctrlrange=\"{}\" forcelimited=\"true\" forcerange=\"-100 100\"/>",
                l.object_id,
                l.object_id,
                vec3(l.range)
            );
        }
    }
    out.push_str("</actuator>");
    if project
        .robots
        .iter()
        .flat_map(|r| &r.links)
        .any(|l| l.angles.iter().any(|a| *a != 0.))
    {
        // qpos follows depth-first body/joint order; ctrl follows actuator declaration order.
        let mut qpos = Vec::<f32>::new();
        for object in &project.objects {
            if !object.fixed
                && !project
                    .robots
                    .iter()
                    .any(|r| r.links.iter().any(|l| l.object_id == object.id))
            {
                qpos.extend(object.position);
                qpos.extend([1., 0., 0., 0.]);
            }
        }
        fn append_pose(robot: &robots::Robot, link: &robots::RobotLink, qpos: &mut Vec<f32>) {
            if link.parent.is_some() {
                qpos.extend(link.axes().enumerate().map(|(i, _)| link.angle(i)));
            } else if robot.template == robots::RobotTemplate::Humanoid {
                qpos.extend(link.anchor);
                qpos.extend([1., 0., 0., 0.]);
            }
            for child in robot
                .links
                .iter()
                .filter(|l| l.parent == Some(link.object_id))
            {
                append_pose(robot, child, qpos);
            }
        }
        for robot in &project.robots {
            append_pose(robot, &robot.links[0], &mut qpos);
        }
        let ctrl: Vec<f32> = project
            .robots
            .iter()
            .flat_map(|r| &r.links)
            .filter(|l| l.parent.is_some())
            .flat_map(|l| l.axes().enumerate().map(|(i, _)| l.angle(i)))
            .collect();
        let _ = writeln!(
            out,
            "<keyframe><key name=\"authored\" qpos=\"{}\" ctrl=\"{}\"/></keyframe>",
            vec3(qpos),
            vec3(ctrl)
        );
    }
    out.push_str("</mujoco>\n");
    Ok(out)
}
pub fn run(project: &Project, steps: u32) -> Result<SimulationResult> {
    if !(1..=1000).contains(&steps) {
        return Err(Error::Invalid("simulation step budget".into()));
    }
    let temp = tempfile::tempdir()?;
    let model = temp.path().join("model.xml");
    let output = temp.path().join("result.json");
    std::fs::write(&model, export(project)?)?;
    let python = std::env::var_os("ATOM_MUJOCO_PYTHON").unwrap_or_else(|| "python3".into());
    let mut child = Process::new(python)
        .arg("-I")
        .arg("-c")
        .arg(WORKER)
        .arg(&model)
        .arg(&output)
        .arg(steps.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| Error::PhysicsUnavailable)?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < Duration::from_secs(15) => {
                std::thread::sleep(Duration::from_millis(10))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::PhysicsFailed);
            }
        }
    };
    if status.code() == Some(42) {
        return Err(Error::PhysicsUnavailable);
    }
    if !status.success() {
        return Err(Error::PhysicsFailed);
    }
    let mut bytes = Vec::new();
    std::fs::File::open(output)?
        .take(128 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 128 * 1024 {
        return Err(Error::PhysicsFailed);
    }
    let result: SimulationResult = serde_json::from_slice(&bytes)?;
    validate_result(project, &result, steps)?;
    Ok(result)
}
pub fn validate_result(project: &Project, result: &SimulationResult, steps: u32) -> Result<()> {
    let expected: std::collections::BTreeSet<_> = project.objects.iter().map(|o| o.id).collect();
    let actual: std::collections::BTreeSet<_> = result.poses.iter().map(|p| p.id).collect();
    if expected != actual
        || result.poses.len() != expected.len()
        || !result.time.is_finite()
        || (result.time - steps as f64 * 0.002).abs() > 1e-7
        || result.engine.len() > 80
        || result.poses.iter().any(|p| {
            p.position
                .iter()
                .chain(p.rotation.iter())
                .any(|v| !v.is_finite() || v.abs() > 10_000.)
        })
    {
        return Err(Error::PhysicsFailed);
    }
    Ok(())
}
