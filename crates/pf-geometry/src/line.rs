use crate::spread;
use pf_model::Vec3;

/// Evenly spaced along X from `-length/2` to `+length/2`.
pub(crate) fn positions(nodes: u32, length: f32) -> Vec<Vec3> {
    (0..nodes)
        .map(|i| Vec3::new(-length / 2.0 + spread(i, nodes) * length, 0.0, 0.0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    #[test]
    fn spans_length_centered_on_origin() {
        let p = positions(5, 4.0);
        assert_eq!(p.len(), 5);
        assert_close(p[0], Vec3::new(-2.0, 0.0, 0.0));
        assert_close(p[2], Vec3::ZERO);
        assert_close(p[4], Vec3::new(2.0, 0.0, 0.0));
    }

    #[test]
    fn single_node_sits_at_center() {
        assert_close(positions(1, 4.0)[0], Vec3::ZERO);
    }
}
