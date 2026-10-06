//! Poly lines: pixels along a chain of straight or curved stretches.

use pf_model::{PolySegment, Vec3};

/// Straight pieces a curved stretch is measured and walked along (the TypeScript mirror in
/// `app/src/lib/geometry.ts` uses the same number, so both put pixels in the same places).
pub const CURVE_STEPS: usize = 25;

/// The point at `t` (0–1) along a cubic Bézier from `a` to `b` with control points `c`.
pub fn bezier(a: Vec3, c: [Vec3; 2], b: Vec3, t: f32) -> Vec3 {
    let u = 1.0 - t;
    a * (u * u * u) + c[0] * (3.0 * u * u * t) + c[1] * (3.0 * u * t * t) + b * (t * t * t)
}

/// One stretch as a chain of straight pieces, with the distance along it at each joint.
struct Path {
    joints: Vec<Vec3>,
    at: Vec<f32>,
}

impl Path {
    fn new(a: Vec3, b: Vec3, curve: Option<[Vec3; 2]>) -> Self {
        let joints: Vec<Vec3> = match curve {
            Some(c) => (0..=CURVE_STEPS)
                .map(|i| bezier(a, c, b, i as f32 / CURVE_STEPS as f32))
                .collect(),
            None => vec![a, b],
        };
        let mut at = Vec::with_capacity(joints.len());
        let mut sum = 0.0;
        at.push(0.0);
        for w in joints.windows(2) {
            sum += (w[1] - w[0]).length();
            at.push(sum);
        }
        Path { joints, at }
    }

    fn len(&self) -> f32 {
        self.at.last().copied().unwrap_or(0.0)
    }

    /// The point `d` along the stretch (clamped to its ends).
    fn point(&self, d: f32) -> Vec3 {
        let last = self.joints.len() - 1;
        if d <= 0.0 || self.len() <= 0.0 {
            return self.joints[0];
        }
        // The first joint at or beyond `d`.
        let k = self.at.partition_point(|&a| a < d).clamp(1, last);
        let (from, span) = (self.at[k - 1], self.at[k] - self.at[k - 1]);
        let t = if span > 0.0 {
            ((d - from) / span).min(1.0)
        } else {
            0.0
        };
        self.joints[k - 1] + (self.joints[k] - self.joints[k - 1]) * t
    }
}

/// Pixel positions along the line: each stretch's own pixels spaced evenly with half a gap at
/// each end, or `spread` pixels every `length / spread` from the first point.
pub(crate) fn positions(vertices: &[Vec3], segments: &[PolySegment], spread: Option<u32>) -> Vec<Vec3> {
    let count = spread.unwrap_or_else(|| segments.iter().fold(0u32, |n, s| n.saturating_add(s.nodes)));
    if vertices.len() < 2 {
        let at = vertices.first().copied().unwrap_or(Vec3::ZERO);
        return vec![at; count as usize];
    }
    let paths: Vec<Path> = vertices
        .windows(2)
        .enumerate()
        .map(|(k, w)| Path::new(w[0], w[1], segments.get(k).and_then(|s| s.curve)))
        .collect();
    let mut out = Vec::with_capacity(count as usize);
    match spread {
        Some(n) => {
            let total: f32 = paths.iter().map(Path::len).sum();
            let step = total / n.max(1) as f32;
            let (mut k, mut base) = (0usize, 0.0f32);
            for i in 0..n {
                let d = i as f32 * step;
                while k + 1 < paths.len() && d > base + paths[k].len() {
                    base += paths[k].len();
                    k += 1;
                }
                out.push(paths[k].point(d - base));
            }
        }
        None => {
            for (k, path) in paths.iter().enumerate() {
                let n = segments.get(k).map_or(0, |s| s.nodes);
                for i in 0..n {
                    out.push(path.point((i as f32 + 0.5) / n as f32 * path.len()));
                }
            }
            // Stretches beyond the points (a damaged file) still get their pixels.
            let last = vertices[vertices.len() - 1];
            out.resize(count as usize, last);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    fn v(x: f32, y: f32) -> Vec3 {
        Vec3::new(x, y, 0.0)
    }

    #[test]
    fn each_stretch_spaces_its_pixels_with_half_gaps_at_the_ends() {
        let p = positions(&[v(0.0, 0.0), v(4.0, 0.0)], &[PolySegment::straight(4)], None);
        let xs: Vec<f32> = p.iter().map(|p| p.x).collect();
        assert_eq!(xs, [0.5, 1.5, 2.5, 3.5]);
    }

    #[test]
    fn pixels_turn_the_corner_from_the_first_point_to_the_last() {
        let p = positions(
            &[v(0.0, 0.0), v(2.0, 0.0), v(2.0, 2.0)],
            &[PolySegment::straight(2), PolySegment::straight(2)],
            None,
        );
        for (got, want) in p.iter().zip([v(0.5, 0.0), v(1.5, 0.0), v(2.0, 0.5), v(2.0, 1.5)]) {
            assert_close(*got, want);
        }
    }

    #[test]
    fn spreading_evenly_starts_on_the_first_point_like_xlights() {
        let p = positions(
            &[v(0.0, 0.0), v(50.0, 0.0), v(100.0, 0.0)],
            &[PolySegment::straight(9), PolySegment::straight(9)],
            Some(4),
        );
        let xs: Vec<f32> = p.iter().map(|p| p.x).collect();
        assert_eq!(xs, [0.0, 25.0, 50.0, 75.0]);
        // Across a corner the distance is measured along the line.
        let p = positions(
            &[v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0)],
            &[PolySegment::straight(0), PolySegment::straight(0)],
            Some(4),
        );
        assert_close(p[3], v(1.0, 0.5));
    }

    #[test]
    fn a_curved_stretch_follows_its_bezier_by_distance() {
        let curve = Some([v(0.0, 1.0), v(2.0, 1.0)]);
        let p = positions(
            &[v(0.0, 0.0), v(2.0, 0.0)],
            &[PolySegment { nodes: 1, curve }],
            None,
        );
        // Symmetric curve: the middle of its length is its peak, at t = 0.5 (within how finely the
        // curve is measured: its 25 pieces put a joint either side of the peak).
        assert!((p[0] - v(1.0, 0.75)).length() < 2e-3, "{p:?}");
        assert_close(
            bezier(v(0.0, 0.0), [v(0.0, 1.0), v(2.0, 1.0)], v(2.0, 0.0), 0.0),
            v(0.0, 0.0),
        );
        let three = positions(
            &[v(0.0, 0.0), v(2.0, 0.0)],
            &[PolySegment { nodes: 3, curve }],
            None,
        );
        assert!(three[0].x < 0.6 && three[0].y > 0.3, "{three:?}");
        assert!((three[0].x - (2.0 - three[2].x)).abs() < 1e-4);
    }

    #[test]
    fn damaged_lines_still_give_every_pixel() {
        assert_eq!(
            positions(&[], &[PolySegment::straight(3)], None),
            vec![Vec3::ZERO; 3]
        );
        assert_eq!(positions(&[v(1.0, 1.0)], &[], Some(2)), vec![v(1.0, 1.0); 2]);
        let p = positions(
            &[v(0.0, 0.0), v(1.0, 0.0)],
            &[PolySegment::straight(1), PolySegment::straight(2)],
            None,
        );
        assert_eq!(p.len(), 3);
        // Zero-length stretches put their pixels on the point.
        let p = positions(&[v(1.0, 1.0), v(1.0, 1.0)], &[PolySegment::straight(2)], None);
        assert_eq!(p, vec![v(1.0, 1.0); 2]);
        assert_eq!(positions(&[v(0.0, 0.0), v(1.0, 0.0)], &[], Some(0)), vec![]);
    }
}
