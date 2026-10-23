//! SI physics configuration. Gravity is canonical Z-up; the editor is Y-up.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GravityPreset {
    #[default]
    Earth,
    Moon,
    Mars,
    Zero,
    Custom,
}
impl GravityPreset {
    pub fn vector(self) -> Option<[f64; 3]> {
        Some([
            0.,
            0.,
            -match self {
                Self::Earth => 9.80665,
                Self::Moon => 1.62,
                Self::Mars => 3.71,
                Self::Zero => 0.,
                Self::Custom => return None,
            },
        ])
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gravity {
    pub preset: GravityPreset,
    pub vector: [f64; 3],
}
impl Default for Gravity {
    fn default() -> Self {
        Self {
            preset: GravityPreset::Earth,
            vector: GravityPreset::Earth.vector().unwrap(),
        }
    }
}
impl Gravity {
    pub fn validate(&self) -> Result<()> {
        if self
            .vector
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1000.)
            || self.preset.vector().is_some_and(|v| v != self.vector)
        {
            return Err(Error::Invalid("gravity vector/preset".into()));
        }
        Ok(())
    }
    pub fn editor_vector(&self) -> [f64; 3] {
        [self.vector[0], self.vector[2], -self.vector[1]]
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialPreset {
    #[default]
    Wood,
    Steel,
    Aluminum,
    Rubber,
    Plastic,
    Glass,
    Concrete,
    Ice,
    Custom,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalMaterial {
    pub preset: MaterialPreset,
    pub density: f64,
    /// MuJoCo sliding (dimensionless), torsional (m), rolling (m).
    pub friction: [f64; 3],
}
impl Default for PhysicalMaterial {
    fn default() -> Self {
        Self::preset(MaterialPreset::Wood)
    }
}
impl PhysicalMaterial {
    /// Illustrative dry-contact starting points, not calibrated engineering data.
    pub fn preset(preset: MaterialPreset) -> Self {
        let (density, sliding) = match preset {
            MaterialPreset::Wood => (600., 0.6),
            MaterialPreset::Steel => (7850., 0.5),
            MaterialPreset::Aluminum => (2700., 0.4),
            MaterialPreset::Rubber => (1100., 1.2),
            MaterialPreset::Plastic => (950., 0.4),
            MaterialPreset::Glass => (2500., 0.3),
            MaterialPreset::Concrete => (2300., 0.8),
            MaterialPreset::Ice => (917., 0.05),
            MaterialPreset::Custom => (600., 0.6),
        };
        Self {
            preset,
            density,
            friction: [sliding, 0.005, 0.0001],
        }
    }
    pub fn validate(&self) -> Result<()> {
        if !self.density.is_finite()
            || !(0.001..=100_000.).contains(&self.density)
            || self
                .friction
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=100.).contains(v))
            || (self.preset != MaterialPreset::Custom && *self != Self::preset(self.preset))
        {
            return Err(Error::Invalid("physical material".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsSpec {
    pub gravity: Gravity,
    pub floor: PhysicalMaterial,
    pub floor_enabled: bool,
}
impl Default for PhysicsSpec {
    fn default() -> Self {
        Self {
            gravity: Gravity::default(),
            floor: PhysicalMaterial::preset(MaterialPreset::Concrete),
            floor_enabled: true,
        }
    }
}
impl PhysicsSpec {
    pub fn validate(&self) -> Result<()> {
        self.gravity.validate()?;
        self.floor.validate()
    }
}
