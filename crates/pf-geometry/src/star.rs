use pf_model::Vec3;
use std::f32::consts::{FRAC_PI_2, PI};

/// Pixels spaced evenly along the closed star outline, starting at the top tip
/// and running clockwise.
pub(crate) fn positions(points: u32, nodes: u32, outer_radius: f32, inner_radius: f32) -> Vec<Vec3> {
    if points == 0 || nodes == 0 {
        return vec![Vec3::ZERO; nodes as usize];
    }
    let vertex_count = points * 2;
    let vertices: Vec<Vec3> = (0..vertex_count)
        .map(|v| {
            let radius = if v % 2 == 0 { outer_radius } else { inner_radius };
            let angle = FRAC_PI_2 - PI * v as f32 / points as f32;
            Vec3::new(radius * angle.cos(), radius * angle.sin(), 0.0)
        })
        .collect();
    let edges: Vec<(Vec3, Vec3)> = (0..vertices.len())
        .map(|v| (vertices[v], vertices[(v + 1) % vertices.len()]))
        .collect();
    let perimeter: f32 = edges.iter().map(|(a, b)| (*b - *a).length()).sum();

    (0..nodes)
        .map(|i| {
            let mut distance = perimeter * i as f32 / nodes as f32;
            for &(a, b) in &edges {
                let edge_length = (b - a).length();
                if distance <= edge_length && edge_length > 0.0 {
                    return a + (b - a) * (distance / edge_length);
                }
                distance -= edge_length;
            }
            vertices[0]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;
    use std::f32::consts::PI;

    #[test]
    fn first_pixel_is_top_tip_and_all_lie_within_outer_radius() {
        let p = positions(5, 100, 2.0, 1.0);
        assert_eq!(p.len(), 100);
        assert_close(p[0], Vec3::new(0.0, 2.0, 0.0));
        assert!(p.iter().all(|v| v.length() <= 2.0 + 1e-4));
    }

    #[test]
    fn one_pixel_per_vertex_lands_on_vertices() {
        let p = positions(4, 8, 2.0, 1.0);
        assert_close(
            p[1],
            Vec3::new(1.0 * (PI / 4.0).cos(), 1.0 * (PI / 4.0).sin(), 0.0),
        );
        assert_close(p[2], Vec3::new(2.0, 0.0, 0.0));
    }
}
