use crate::spread;
use pf_model::Vec3;
use std::f32::consts::TAU;

/// Strings arranged around a cone, each running from the base (y = 0) to the top
/// (y = height). With `serpentine`, odd strings run top to bottom.
pub(crate) fn positions(
    strings: u32,
    nodes_per_string: u32,
    height: f32,
    base_radius: f32,
    top_radius: f32,
    serpentine: bool,
) -> Vec<Vec3> {
    let mut out = Vec::with_capacity(strings as usize * nodes_per_string as usize);
    for s in 0..strings {
        let angle = TAU * s as f32 / strings as f32;
        for j in 0..nodes_per_string {
            let mut t = spread(j, nodes_per_string);
            if serpentine && s % 2 == 1 {
                t = 1.0 - t;
            }
            let radius = base_radius + (top_radius - base_radius) * t;
            out.push(Vec3::new(radius * angle.sin(), t * height, radius * angle.cos()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    #[test]
    fn strings_run_base_to_top_and_taper() {
        let p = positions(4, 3, 6.0, 2.0, 0.0, false);
        assert_eq!(p.len(), 12);
        assert_close(p[0], Vec3::new(0.0, 0.0, 2.0));
        assert_close(p[2], Vec3::new(0.0, 6.0, 0.0));
        assert_close(p[3], Vec3::new(2.0, 0.0, 0.0));
    }

    #[test]
    fn serpentine_reverses_odd_strings() {
        let p = positions(2, 3, 6.0, 2.0, 2.0, true);
        assert_close(p[3], Vec3::new(0.0, 6.0, -2.0));
        assert_close(p[5], Vec3::new(0.0, 0.0, -2.0));
    }
}
