use pf_model::{Transform, Vec3};

/// Applies scale, then rotation about X, Y, Z (degrees), then translation.
pub fn apply_transform(p: Vec3, t: &Transform) -> Vec3 {
    let scaled = Vec3::new(p.x * t.scale.x, p.y * t.scale.y, p.z * t.scale.z);
    let r = t.rotation_deg;
    let rotated = rotate_z(rotate_y(rotate_x(scaled, r.x), r.y), r.z);
    rotated + t.position
}

/// The transform's rotation alone as a row-major 3 × 3 matrix: its columns are where it turns
/// the prop's own right, up and front (toward the street).
pub fn rotation_basis(t: &Transform) -> [f32; 9] {
    let r = t.rotation_deg;
    let turn = |p: Vec3| rotate_z(rotate_y(rotate_x(p, r.x), r.y), r.z);
    let [x, y, z] = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
    ]
    .map(turn);
    [x.x, y.x, z.x, x.y, y.y, z.y, x.z, y.z, z.z]
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

    #[test]
    fn the_rotation_basis_turns_as_the_transform_does_without_moving_or_scaling() {
        let t = Transform {
            position: Vec3::new(5.0, 6.0, 7.0),
            rotation_deg: Vec3::new(10.0, 20.0, 180.0),
            scale: Vec3::new(3.0, 3.0, 3.0),
        };
        let b = rotation_basis(&t);
        for (i, axis) in [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        ]
        .into_iter()
        .enumerate()
        {
            let turned = apply_transform(axis, &t) - apply_transform(Vec3::ZERO, &t);
            assert_close(Vec3::new(b[i], b[3 + i], b[6 + i]), turned * (1.0 / 3.0));
        }
        assert_eq!(
            rotation_basis(&Transform::default()),
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
        );
    }
}
