//! Original primitive templates, independent of vendor assets and real robot claims.
mod human;
use crate::{Error, MaterialPreset, PhysicalMaterial, Result, SceneObject};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RobotTemplate {
    Humanoid,
    DualArm,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointAxis {
    pub axis: [f32; 3],
    pub range: [f32; 2],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RobotLink {
    pub object_id: u32,
    pub parent: Option<u32>,
    /// Initial joint pivot in editor world coordinates, meters.
    pub anchor: [f32; 3],
    pub axis: [f32; 3],
    pub range: [f32; 2],
    /// Additional rotational axes at the same anatomical connection. Empty in legacy files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_axes: Vec<JointAxis>,
}
impl RobotLink {
    pub fn axes(&self) -> impl Iterator<Item = JointAxis> + '_ {
        std::iter::once(JointAxis {
            axis: self.axis,
            range: self.range,
        })
        .chain(self.extra_axes.iter().cloned())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Robot {
    pub template: RobotTemplate,
    pub root: u32,
    pub links: Vec<RobotLink>,
}
impl Robot {
    pub fn validate(&self, objects: &[SceneObject]) -> Result<()> {
        let mut seen = BTreeSet::new();
        if self.links.is_empty() || self.links.len() > crate::MAX_OBJECTS {
            return Err(Error::Invalid("robot links".into()));
        }
        for (i, link) in self.links.iter().enumerate() {
            if !objects.iter().any(|o| o.id == link.object_id)
                || !seen.insert(link.object_id)
                || (i == 0 && (link.object_id != self.root || link.parent.is_some()))
                || (i > 0
                    && !link
                        .parent
                        .is_some_and(|p| p != link.object_id && seen.contains(&p)))
                || link
                    .anchor
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() > 10_000.)
                || link.axis.iter().any(|v| !v.is_finite())
                || (link.axis.iter().map(|v| v * v).sum::<f32>() - 1.).abs() > 1e-4
                || link
                    .range
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() > std::f32::consts::PI)
                || link.range[0] >= link.range[1]
                || link.extra_axes.len() > 2
                || (link.parent.is_none() && !link.extra_axes.is_empty())
                || link.axes().any(|a| {
                    a.axis.iter().any(|v| !v.is_finite())
                        || (a.axis.iter().map(|v| v * v).sum::<f32>() - 1.).abs() > 1e-4
                        || a.range
                            .iter()
                            .any(|v| !v.is_finite() || v.abs() > std::f32::consts::PI)
                        || a.range[0] >= a.range[1]
                })
                || link.axes().enumerate().any(|(i, a)| {
                    link.axes().take(i).any(|b| {
                        a.axis
                            .iter()
                            .zip(b.axis)
                            .map(|(x, y)| x * y)
                            .sum::<f32>()
                            .abs()
                            > 1e-4
                    })
                })
            {
                return Err(Error::Invalid("robot topology/joint".into()));
            }
        }
        Ok(())
    }
}
/// All dimensions are full box extents, matching the native ThreeX unit cube.
pub fn build(template: RobotTemplate, start: u32) -> Result<(Robot, Vec<SceneObject>, u32)> {
    if template == RobotTemplate::Humanoid {
        return human::build(start);
    }
    let mut objects = Vec::new();
    let mut links = Vec::new();
    let mut next = start;
    let mut add = |name: &str,
                   parent: Option<u32>,
                   position: [f32; 3],
                   scale: [f32; 3],
                   anchor: [f32; 3],
                   axis: [f32; 3],
                   range: [f32; 2]|
     -> Result<u32> {
        let id = next;
        next = next
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("object identity exhausted".into()))?;
        objects.push(SceneObject {
            id,
            label: name.into(),
            position,
            rotation: [0.; 3],
            scale,
            color: if name.contains("foot") || name.contains("gripper") {
                [0.35, 0.33, 0.30]
            } else {
                [0.75, 0.69, 0.57]
            },
            material: PhysicalMaterial::preset(
                if name.contains("foot") || name.contains("gripper") {
                    MaterialPreset::Rubber
                } else {
                    MaterialPreset::Aluminum
                },
            ),
            fixed: false,
            mesh_ref: None,
            mass_override: None,
        });
        links.push(RobotLink {
            object_id: id,
            parent,
            anchor,
            axis,
            range,
            extra_axes: Vec::new(),
        });
        Ok(id)
    };
    let root;
    match template {
        RobotTemplate::Humanoid => unreachable!("human template built above"),
        RobotTemplate::DualArm => {
            root = add(
                "base",
                None,
                [0., 0.42, 0.],
                [0.45, 0.80, 0.35],
                [0., 0.42, 0.],
                [0., 1., 0.],
                [-1., 1.],
            )?;
            for (side, s) in [("left", -1.), ("right", 1.)] {
                let shoulder = add(
                    &format!("{side}_shoulder"),
                    Some(root),
                    [s * 0.32, 0.85, 0.],
                    [0.16, 0.18, 0.18],
                    [s * 0.25, 0.82, 0.],
                    [0., 1., 0.],
                    [-2.5, 2.5],
                )?;
                let upper = add(
                    &format!("{side}_upper_arm"),
                    Some(shoulder),
                    [s * 0.52, 0.85, 0.],
                    [0.26, 0.12, 0.12],
                    [s * 0.38, 0.85, 0.],
                    [0., 0., 1.],
                    [-1.5, 1.5],
                )?;
                let elbow = add(
                    &format!("{side}_elbow"),
                    Some(upper),
                    [s * 0.72, 0.85, 0.],
                    [0.12, 0.14, 0.14],
                    [s * 0.67, 0.85, 0.],
                    [0., 1., 0.],
                    [-2., 2.],
                )?;
                let forearm = add(
                    &format!("{side}_forearm"),
                    Some(elbow),
                    [s * 0.90, 0.85, 0.],
                    [0.24, 0.10, 0.10],
                    [s * 0.78, 0.85, 0.],
                    [0., 0., 1.],
                    [-1.5, 1.5],
                )?;
                let wrist = add(
                    &format!("{side}_wrist"),
                    Some(forearm),
                    [s * 1.07, 0.85, 0.],
                    [0.10, 0.12, 0.12],
                    [s * 1.03, 0.85, 0.],
                    [1., 0., 0.],
                    [-2.5, 2.5],
                )?;
                add(
                    &format!("{side}_gripper"),
                    Some(wrist),
                    [s * 1.20, 0.85, 0.],
                    [0.16, 0.08, 0.16],
                    [s * 1.12, 0.85, 0.],
                    [0., 1., 0.],
                    [-1., 1.],
                )?;
            }
        }
    }
    objects[0].fixed = template == RobotTemplate::DualArm;
    // Place added templates separately without changing existing objects.
    let offset = ((start.saturating_sub(1)) / 13) as f32 * 2.8;
    for o in &mut objects {
        o.position[0] += offset;
    }
    for l in &mut links {
        l.anchor[0] += offset;
    }
    Ok((
        Robot {
            template,
            root,
            links,
        },
        objects,
        next,
    ))
}
