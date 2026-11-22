//! Oriented-box interference checks against the same link shapes used by FK.
use super::*;

const CONTACT_EPSILON: f64 = 1e-6;
type Vector = [f64; 3];

fn dot(a: Vector, b: Vector) -> f64 {
    a.into_iter().zip(b).map(|(x, y)| x * y).sum()
}
fn cross(a: Vector, b: Vector) -> Vector {
    std::array::from_fn(|i| a[(i + 1) % 3] * b[(i + 2) % 3] - a[(i + 2) % 3] * b[(i + 1) % 3])
}

struct BoxShape {
    center: Vector,
    axes: [Vector; 3],
    half: Vector,
    bounds: Vector,
}
impl BoxShape {
    fn new(object: &SceneObject, frame: Rigid) -> Self {
        let basis = mul(frame.r, from_euler(object.rotation));
        let half = object.scale.map(|v| f64::from(v) / 2.);
        Self {
            center: frame.point(object.position).map(f64::from),
            axes: std::array::from_fn(|i| std::array::from_fn(|j| basis[j][i])),
            half,
            bounds: basis.map(|row| dot(row.map(f64::abs), half)),
        }
    }

    fn penetration(&self, other: &Self) -> f64 {
        let offset = std::array::from_fn(|i| other.center[i] - self.center[i]);
        if (0..3).any(|i| offset[i].abs() >= self.bounds[i] + other.bounds[i] - CONTACT_EPSILON) {
            return 0.;
        }
        let mut depth = f64::INFINITY;
        // The six face normals and nine edge cross products are the separating axes.
        let edges = self
            .axes
            .into_iter()
            .flat_map(|a| other.axes.map(|b| cross(a, b)));
        for axis in self.axes.into_iter().chain(other.axes).chain(edges) {
            let length = dot(axis, axis).sqrt();
            if length < 1e-8 {
                continue;
            }
            let radius = |b: &Self| dot(b.axes.map(|v| dot(v, axis).abs()), b.half);
            let overlap = (radius(self) + radius(other) - dot(offset, axis).abs()) / length;
            if overlap <= CONTACT_EPSILON {
                return 0.;
            }
            depth = depth.min(overlap);
        }
        depth
    }
}

pub(super) struct CollisionModel<'a> {
    objects: Vec<&'a SceneObject>,
    pairs: Vec<(usize, usize)>,
}
impl<'a> CollisionModel<'a> {
    pub(super) fn new(project: &'a Project, robot: &Robot) -> Result<Self> {
        let mut objects = Vec::with_capacity(robot.links.len());
        let mut pairs = Vec::new();
        for (i, link) in robot.links.iter().enumerate() {
            let object = project
                .objects
                .iter()
                .find(|o| o.id == link.object_id)
                .filter(|o| o.mesh_ref.is_none())
                .ok_or_else(|| Error::Invalid("self-collision IK requires box links".into()))?;
            objects.push(object);
            for (j, other) in robot.links[..i].iter().enumerate() {
                // Direct parent/child links may overlap at their joint by construction.
                if link.parent != Some(other.object_id) {
                    pairs.push((j, i));
                }
            }
        }
        Ok(Self { objects, pairs })
    }

    pub(super) fn penalty(&self, frames: &BTreeMap<u32, Rigid>) -> f64 {
        let shapes: Vec<_> = self
            .objects
            .iter()
            .map(|object| BoxShape::new(object, frames[&object.id]))
            .collect();
        self.pairs
            .iter()
            .map(|&(i, j)| shapes[i].penetration(&shapes[j]).powi(2))
            .sum()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IkImpact {
    Floor { link: u32 },
    Obstacle { link: u32, obstacle: u32 },
    SelfCollision { links: [u32; 2] },
}

#[derive(Clone, Debug, Serialize)]
pub struct IkPathReport {
    #[serde(skip_serializing)]
    pub points: Vec<[f32; 3]>,
    pub impact: Option<IkImpact>,
    pub fraction: f32,
    /// World position of the colliding link's center at the failed sample.
    #[serde(skip_serializing)]
    pub marker: Option<[f32; 3]>,
}

pub(super) struct Environment {
    floor: bool,
    obstacles: Vec<(u32, BoxShape)>,
}

impl Environment {
    pub(super) fn new(project: &Project, robot: &Robot, frames: &BTreeMap<u32, Rigid>) -> Self {
        let obstacles = project
            .objects
            .iter()
            .filter(|o| !robot.links.iter().any(|l| l.object_id == o.id))
            .map(|object| {
                let frame = frames
                    .get(&object.id)
                    .copied()
                    .unwrap_or(Rigid { r: ID, t: [0.; 3] });
                let mut bounds = object.clone();
                if let Some(asset) = project
                    .assets
                    .iter()
                    .find(|a| Some(a.id) == object.mesh_ref)
                {
                    let mut min = [f32::INFINITY; 3];
                    let mut max = [f32::NEG_INFINITY; 3];
                    for point in &asset.mesh.positions {
                        for i in 0..3 {
                            min[i] = min[i].min(point[i]);
                            max[i] = max[i].max(point[i]);
                        }
                    }
                    let center = vector(
                        from_euler(object.rotation),
                        std::array::from_fn(|i| {
                            f64::from((min[i] + max[i]) * 0.5 * object.scale[i])
                        }),
                    );
                    bounds.position =
                        std::array::from_fn(|i| object.position[i] + center[i] as f32);
                    bounds.scale =
                        std::array::from_fn(|i| (max[i] - min[i]).max(1e-6) * object.scale[i]);
                }
                (object.id, BoxShape::new(&bounds, frame))
            })
            .collect();
        Self {
            floor: project.physics.floor_enabled,
            obstacles,
        }
    }
}

impl CollisionModel<'_> {
    pub(super) fn impact(
        &self,
        frames: &BTreeMap<u32, Rigid>,
        environment: &Environment,
    ) -> Option<(IkImpact, [f32; 3])> {
        let shapes: Vec<_> = self
            .objects
            .iter()
            .map(|o| BoxShape::new(o, frames[&o.id]))
            .collect();
        for (object, shape) in self.objects.iter().zip(&shapes) {
            let link = object.id;
            let center = shape.center.map(|v| v as f32);
            if environment.floor && shape.center[1] - shape.bounds[1] < -CONTACT_EPSILON {
                return Some((IkImpact::Floor { link }, center));
            }
            for (obstacle, other) in &environment.obstacles {
                if shape.penetration(other) > 0. {
                    return Some((
                        IkImpact::Obstacle {
                            link,
                            obstacle: *obstacle,
                        },
                        center,
                    ));
                }
            }
        }
        self.pairs
            .iter()
            .find(|&&(i, j)| shapes[i].penetration(&shapes[j]) > 0.)
            .map(|&(i, j)| {
                (
                    IkImpact::SelfCollision {
                        links: [self.objects[i].id, self.objects[j].id],
                    },
                    shapes[j].center.map(|v| v as f32),
                )
            })
    }
}
