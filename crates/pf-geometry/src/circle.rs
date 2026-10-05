use pf_model::Vec3;
use std::f32::consts::{FRAC_PI_2, TAU};

/// Starts at the top and runs clockwise.
pub(crate) fn positions(nodes: u32, radius: f32) -> Vec<Vec3> {
    (0..nodes)
        .map(|i| {
            let angle = FRAC_PI_2 - TAU * i as f32 / nodes as f32;
            Vec3::new(radius * angle.cos(), radius * angle.sin(), 0.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    #[test]
    fn starts_at_top_and_runs_clockwise() {
        let p = positions(4, 1.0);
        assert_close(p[0], Vec3::new(0.0, 1.0, 0.0));
        assert_close(p[1], Vec3::new(1.0, 0.0, 0.0));
        assert_close(p[2], Vec3::new(0.0, -1.0, 0.0));
        assert_close(p[3], Vec3::new(-1.0, 0.0, 0.0));
    }
}
