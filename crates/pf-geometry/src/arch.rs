use crate::spread;
use pf_model::Vec3;
use std::f32::consts::PI;

/// Half-ellipse from the left base (`-width/2, 0`) over the top (`0, height`)
/// to the right base (`width/2, 0`).
pub(crate) fn positions(nodes: u32, width: f32, height: f32) -> Vec<Vec3> {
    (0..nodes)
        .map(|i| {
            let angle = PI * (1.0 - spread(i, nodes));
            Vec3::new(width / 2.0 * angle.cos(), height * angle.sin(), 0.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    #[test]
    fn runs_left_base_to_top_to_right_base() {
        let p = positions(3, 4.0, 2.0);
        assert_close(p[0], Vec3::new(-2.0, 0.0, 0.0));
        assert_close(p[1], Vec3::new(0.0, 2.0, 0.0));
        assert_close(p[2], Vec3::new(2.0, 0.0, 0.0));
    }
}
