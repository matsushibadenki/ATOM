//! Articulated engineering approximation, not a claim about a universal anatomical joint count.
use super::*;
struct Builder {
    objects: Vec<SceneObject>,
    links: Vec<RobotLink>,
    next: u32,
}
impl Builder {
    fn add(
        &mut self,
        name: &str,
        parent: Option<u32>,
        anchor: [f32; 3],
        center: [f32; 3],
        size: [f32; 3],
        ranges: &[[f32; 2]],
    ) -> Result<u32> {
        let id = self.next;
        self.next = id
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("object identity exhausted".into()))?;
        let axes = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        self.links.push(RobotLink {
            object_id: id,
            parent,
            anchor,
            axis: axes[0],
            range: ranges[0],
            angles: Vec::new(),
            extra_axes: ranges
                .iter()
                .enumerate()
                .skip(1)
                .map(|(i, r)| JointAxis {
                    axis: axes[i],
                    range: *r,
                })
                .collect(),
        });
        self.objects.push(SceneObject {
            id,
            label: name.into(),
            position: center,
            rotation: [0.; 3],
            scale: size,
            color: if name.contains("finger") || name.contains("toe") {
                [0.70, 0.57, 0.43]
            } else {
                [0.75, 0.69, 0.57]
            },
            material: PhysicalMaterial {
                preset: MaterialPreset::Custom,
                density: 1000.,
                ..PhysicalMaterial::preset(MaterialPreset::Rubber)
            },
            fixed: false,
            mesh_ref: None,
            mass_override: None,
        });
        Ok(id)
    }
}
pub(super) fn build(start: u32) -> Result<(Robot, Vec<SceneObject>, u32)> {
    let mut b = Builder {
        objects: Vec::new(),
        links: Vec::new(),
        next: start,
    };
    let root = b.add(
        "pelvis",
        None,
        [0., 0.98, 0.],
        [0., 0.98, 0.],
        [0.28, 0.16, 0.18],
        &[[-0.1, 0.1]],
    )?;
    let mut spine = root;
    // L5→L1, T12→T1, C7→C1; sacrum is represented by the rigid pelvis.
    let mut y = 1.06;
    let mut thoracic_top = root;
    for (region, n, h, width, depth) in [
        ("lumbar", 5, 0.035, 0.22, 0.16),
        ("thoracic", 12, 0.025, 0.30, 0.18),
        ("cervical", 7, 0.015, 0.065, 0.065),
    ] {
        for number in (1..=n).rev() {
            spine = b.add(
                &format!("{region}_{number}"),
                Some(spine),
                [0., y, 0.],
                [0., y + h / 2., 0.],
                [width, h * 0.85, depth],
                &[[-0.12, 0.12], [-0.12, 0.12], [-0.10, 0.10]],
            )?;
            if region == "thoracic" && number == 1 {
                thoracic_top = spine;
            }
            y += h;
        }
    }
    let head = b.add(
        "head",
        Some(spine),
        [0., y, 0.],
        [0., y + 0.10, 0.],
        [0.16, 0.19, 0.15],
        &[[-0.35, 0.35], [-0.35, 0.35], [-0.20, 0.20]],
    )?;
    b.add(
        "jaw",
        Some(head),
        [0., y + 0.03, 0.04],
        [0., y + 0.01, 0.075],
        [0.10, 0.025, 0.035],
        &[[-0.45, 0.01]],
    )?;
    for (side, s) in [("left", -1.), ("right", 1.)] {
        // Clavicular girdle is a reduced two-axis shoulder-girdle connection.
        let clavicle = b.add(
            &format!("{side}_clavicle"),
            Some(thoracic_top),
            [s * 0.08, 1.49, 0.],
            [s * 0.155, 1.49, 0.],
            [0.13, 0.045, 0.06],
            &[[-0.3, 0.3], [-0.4, 0.4]],
        )?;
        let arm = b.add(
            &format!("{side}_upper_arm"),
            Some(clavicle),
            [s * 0.24, 1.49, 0.],
            [s * 0.24, 1.345, 0.],
            [0.08, 0.27, 0.08],
            &[[-2.0, 2.0], [-1.5, 1.5], [-2.5, 2.5]],
        )?;
        let forearm = b.add(
            &format!("{side}_forearm"),
            Some(arm),
            [s * 0.24, 1.20, 0.],
            [s * 0.24, 1.075, 0.],
            [0.065, 0.23, 0.065],
            &[[-2.5, 0.02], [-1.4, 1.4]],
        )?;
        let hand = b.add(
            &format!("{side}_palm"),
            Some(forearm),
            [s * 0.24, 0.95, 0.],
            [s * 0.24, 0.905, 0.],
            [0.085, 0.08, 0.035],
            &[[-1.2, 1.2], [-0.4, 0.4]],
        )?;
        for digit in 1..=5 {
            let count = if digit == 1 { 2 } else { 3 };
            let x = s * (0.24 + (3. - digit as f32) * 0.019);
            let mut parent = hand;
            for segment in 1..=count {
                let top = 0.86 - (segment - 1) as f32 * 0.025;
                let ranges = if segment == 1 {
                    vec![[-0.1, 1.5], [-0.3, 0.3]]
                } else {
                    vec![[-0.05, 1.8]]
                };
                parent = b.add(
                    &format!("{side}_finger_{digit}_{segment}"),
                    Some(parent),
                    [x, top, 0.],
                    [x, top - 0.011, 0.],
                    [0.013, 0.021, 0.018],
                    &ranges,
                )?;
            }
        }
        let thigh = b.add(
            &format!("{side}_thigh"),
            Some(root),
            [s * 0.09, 0.9, 0.],
            [s * 0.09, 0.705, 0.],
            [0.11, 0.37, 0.12],
            &[[-0.5, 2.0], [-0.7, 0.7], [-0.6, 0.6]],
        )?;
        let shin = b.add(
            &format!("{side}_shin"),
            Some(thigh),
            [s * 0.09, 0.51, 0.],
            [s * 0.09, 0.32, 0.],
            [0.08, 0.36, 0.09],
            &[[-2.4, 0.02]],
        )?;
        let ankle = b.add(
            &format!("{side}_ankle"),
            Some(shin),
            [s * 0.09, 0.13, 0.],
            [s * 0.09, 0.12, 0.],
            [0.065, 0.025, 0.065],
            &[[-0.7, 0.5]],
        )?;
        let foot = b.add(
            &format!("{side}_foot"),
            Some(ankle),
            [s * 0.09, 0.1, 0.],
            [s * 0.09, 0.07, 0.045],
            [0.10, 0.055, 0.16],
            &[[-0.35, 0.35]],
        )?;
        // Subtalar inversion/eversion rotates about the forward axis.
        b.links.last_mut().unwrap().axis = [0., 0., 1.];
        for digit in 1..=5 {
            let count = if digit == 1 { 2 } else { 3 };
            let x = s * (0.09 + (3. - digit as f32) * 0.019);
            let mut parent = foot;
            for segment in 1..=count {
                let z = 0.128 + (segment - 1) as f32 * 0.019;
                let ranges = if segment == 1 {
                    vec![[-0.7, 0.7], [-0.2, 0.2]]
                } else {
                    vec![[-0.1, 1.0]]
                };
                parent = b.add(
                    &format!("{side}_toe_{digit}_{segment}"),
                    Some(parent),
                    [x, 0.065, z],
                    [x, 0.065, z + 0.008],
                    [0.013, 0.025, 0.015],
                    &ranges,
                )?;
            }
        }
    }
    let robot = Robot {
        template: RobotTemplate::Humanoid,
        root,
        links: b.links,
    };
    robot.validate(&b.objects)?;
    Ok((robot, b.objects, b.next))
}
