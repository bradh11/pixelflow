use pf_model::{Transform, Vec3};

/// Applies scale, then rotation about X, Y, Z (degrees), then translation.
pub fn apply_transform(p: Vec3, t: &Transform) -> Vec3 {
    let scaled = Vec3::new(p.x * t.scale.x, p.y * t.scale.y, p.z * t.scale.z);
    let r = t.rotation_deg;
    let rotated = rotate_z(rotate_y(rotate_x(scaled, r.x), r.y), r.z);
    rotated + t.position
}

fn rotate_x(p: Vec3, degrees: f32) -> Vec3 {
    let (s, c) = degrees.to_radians().sin_cos();
    Vec3::new(p.x, p.y * c - p.z * s, p.y * s + p.z * c)
}

fn rotate_y(p: Vec3, degrees: f32) -> Vec3 {
    let (s, c) = degrees.to_radians().sin_cos();
    Vec3::new(p.x * c + p.z * s, p.y, -p.x * s + p.z * c)
}

fn rotate_z(p: Vec3, degrees: f32) -> Vec3 {
    let (s, c) = degrees.to_radians().sin_cos();
    Vec3::new(p.x * c - p.y * s, p.x * s + p.y * c, p.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    #[test]
    fn identity_leaves_points_unchanged() {
        let p = Vec3::new(1.0, 2.0, 3.0);
        assert_close(apply_transform(p, &Transform::default()), p);
    }

    #[test]
    fn scales_then_rotates_then_translates() {
        let t = Transform {
            position: Vec3::new(10.0, 0.0, 0.0),
            rotation_deg: Vec3::new(0.0, 0.0, 90.0),
            scale: Vec3::new(2.0, 1.0, 1.0),
        };
        // (1,0,0) -> scale (2,0,0) -> rotate 90° about Z (0,2,0) -> translate (10,2,0)
        assert_close(
            apply_transform(Vec3::new(1.0, 0.0, 0.0), &t),
            Vec3::new(10.0, 2.0, 0.0),
        );
    }

    #[test]
    fn rotation_about_y_moves_x_toward_negative_z() {
        let t = Transform {
            rotation_deg: Vec3::new(0.0, 90.0, 0.0),
            ..Transform::default()
        };
        assert_close(
            apply_transform(Vec3::new(1.0, 0.0, 0.0), &t),
            Vec3::new(0.0, 0.0, -1.0),
        );
    }
}
