//! Small value types shared across the model.

use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul, Sub};

/// A 3D point or vector in layout units. +X is right, +Y is up, +Z points toward the viewer.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    pub const ONE: Vec3 = Vec3::new(1.0, 1.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

/// Placement of a prop in the layout. Applied as scale, then rotation (X, then Y, then Z,
/// in degrees), then translation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct Transform {
    pub position: Vec3,
    pub rotation_deg: Vec3,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation_deg: Vec3::ZERO,
            scale: Vec3::ONE,
        }
    }
}

/// Order in which a pixel expects its color channels on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "UPPERCASE")]
pub enum ColorOrder {
    #[default]
    Rgb,
    Rbg,
    Grb,
    Gbr,
    Brg,
    Bgr,
    Rgbw,
    Grbw,
}

impl ColorOrder {
    /// Number of channels (bytes) each pixel uses: 3 for RGB orders, 4 for RGBW orders.
    pub fn channels_per_pixel(self) -> u8 {
        match self {
            ColorOrder::Rgbw | ColorOrder::Grbw => 4,
            _ => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgbw_orders_use_four_channels_and_rgb_orders_three() {
        assert_eq!(ColorOrder::Grbw.channels_per_pixel(), 4);
        assert_eq!(ColorOrder::Rgbw.channels_per_pixel(), 4);
        assert_eq!(ColorOrder::Bgr.channels_per_pixel(), 3);
    }

    #[test]
    fn color_order_serializes_uppercase() {
        assert_eq!(serde_json::to_string(&ColorOrder::Grb).unwrap(), "\"GRB\"");
    }

    #[test]
    fn default_transform_is_identity_and_uses_camel_case_keys() {
        let t = Transform::default();
        assert_eq!(t.scale, Vec3::ONE);
        let json = serde_json::to_value(t).unwrap();
        assert!(json.get("rotationDeg").is_some());
    }

    #[test]
    fn vec3_arithmetic() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(a + a, Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(a - a, Vec3::ZERO);
        assert_eq!(a * 2.0, Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(Vec3::new(3.0, 4.0, 0.0).length(), 5.0);
    }
}
