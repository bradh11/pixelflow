use crate::spread;
use pf_model::{Corner, MatrixWiring, Orientation, Vec3};

/// Grid centered on the origin. Node order follows the wiring: start corner,
/// strings along rows or columns, optional zig-zag.
pub(crate) fn positions(columns: u32, rows: u32, width: f32, height: f32, wiring: MatrixWiring) -> Vec<Vec3> {
    let count = columns.saturating_mul(rows);
    (0..count)
        .map(|k| {
            let (col, row) = cell_for_node(k, columns, rows, wiring);
            Vec3::new(
                -width / 2.0 + spread(col, columns) * width,
                -height / 2.0 + spread(row, rows) * height,
                0.0,
            )
        })
        .collect()
}

/// Returns `(column, row)` with column 0 on the left and row 0 at the bottom.
pub(crate) fn cell_for_node(k: u32, columns: u32, rows: u32, wiring: MatrixWiring) -> (u32, u32) {
    let string_len = match wiring.orientation {
        Orientation::Horizontal => columns,
        Orientation::Vertical => rows,
    };
    let string = k / string_len;
    let mut along = k % string_len;
    if wiring.serpentine && string % 2 == 1 {
        along = string_len - 1 - along;
    }
    let (mut col, mut row) = match wiring.orientation {
        Orientation::Horizontal => (along, string),
        Orientation::Vertical => (string, along),
    };
    if matches!(wiring.start, Corner::BottomRight | Corner::TopRight) {
        col = columns - 1 - col;
    }
    if matches!(wiring.start, Corner::TopLeft | Corner::TopRight) {
        row = rows - 1 - row;
    }
    (col, row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::{Corner, MatrixWiring, Orientation, Vec3};

    fn wiring(start: Corner, orientation: Orientation, serpentine: bool) -> MatrixWiring {
        MatrixWiring {
            start,
            orientation,
            serpentine,
        }
    }

    #[test]
    fn horizontal_serpentine_from_bottom_left() {
        let w = wiring(Corner::BottomLeft, Orientation::Horizontal, true);
        let cells: Vec<_> = (0..6).map(|k| cell_for_node(k, 3, 2, w)).collect();
        assert_eq!(cells, vec![(0, 0), (1, 0), (2, 0), (2, 1), (1, 1), (0, 1)]);
    }

    #[test]
    fn vertical_non_serpentine_from_top_right() {
        let w = wiring(Corner::TopRight, Orientation::Vertical, false);
        let cells: Vec<_> = (0..6).map(|k| cell_for_node(k, 3, 2, w)).collect();
        assert_eq!(cells, vec![(2, 1), (2, 0), (1, 1), (1, 0), (0, 1), (0, 0)]);
    }

    #[test]
    fn positions_span_width_and_height() {
        let p = positions(3, 2, 4.0, 2.0, MatrixWiring::default());
        assert_eq!(p.len(), 6);
        assert_eq!(p[0], Vec3::new(-2.0, -1.0, 0.0));
        assert_eq!(p[2], Vec3::new(2.0, -1.0, 0.0));
        assert_eq!(p[3], Vec3::new(2.0, 1.0, 0.0));
    }
}
