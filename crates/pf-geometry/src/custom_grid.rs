use crate::OwnGrid;
use pf_model::Vec3;

/// One layout unit per cell, centered on the origin, top row highest.
/// A node placed in several cells sits at their average; a node in no cell sits at the origin.
pub(crate) fn positions(columns: u32, rows: u32, cells: &[u32]) -> Vec<Vec3> {
    let count = cells.iter().copied().max().unwrap_or(0) as usize;
    let mut sums = vec![Vec3::ZERO; count];
    let mut hits = vec![0u32; count];
    for (index, &node) in cells.iter().enumerate() {
        if node == 0 || columns == 0 {
            continue;
        }
        let col = index as u32 % columns;
        let row = index as u32 / columns;
        let cell = Vec3::new(
            col as f32 - (columns as f32 - 1.0) / 2.0,
            (rows as f32 - 1.0) / 2.0 - row as f32,
            0.0,
        );
        let slot = node as usize - 1;
        sums[slot] = sums[slot] + cell;
        hits[slot] += 1;
    }
    sums.into_iter()
        .zip(hits)
        .map(|(sum, n)| {
            if n == 0 {
                Vec3::ZERO
            } else {
                sum * (1.0 / n as f32)
            }
        })
        .collect()
}

/// The grid a custom shape's effects are drawn on: its own cells, as in xLights. A node placed
/// in several cells is drawn from the first (reading the rows from the top); a node in no cell
/// shares the bottom left one.
pub(crate) fn grid(columns: u32, rows: u32, cells: &[u32]) -> OwnGrid {
    let count = cells.iter().copied().max().unwrap_or(0) as usize;
    let mut at: Vec<Option<[u32; 2]>> = vec![None; count];
    if columns > 0 {
        for (index, &node) in cells.iter().enumerate() {
            let (col, row) = (index as u32 % columns, index as u32 / columns);
            if node > 0 && row < rows {
                at[node as usize - 1].get_or_insert([col, rows - 1 - row]);
            }
        }
    }
    OwnGrid {
        columns,
        rows,
        cells: at.into_iter().map(Option::unwrap_or_default).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::Vec3;

    #[test]
    fn the_grid_drawn_on_is_the_shapes_own_cells() {
        // 3 x 2 grid:  1 0 2
        //              0 3 1
        let g = grid(3, 2, &[1, 0, 2, 0, 3, 1]);
        assert_eq!((g.columns, g.rows), (3, 2));
        assert_eq!(
            g.cells,
            vec![[0, 1], [2, 1], [1, 0]],
            "node 1 from its first cell"
        );
        assert_eq!(grid(2, 1, &[3, 1]).cells, vec![[1, 0], [0, 0], [0, 0]]);
    }

    #[test]
    fn places_nodes_by_number_with_top_row_highest() {
        // 3 x 2 grid:  1 0 2
        //              0 3 0
        let p = positions(3, 2, &[1, 0, 2, 0, 3, 0]);
        assert_eq!(
            p,
            vec![
                Vec3::new(-1.0, 0.5, 0.0),
                Vec3::new(1.0, 0.5, 0.0),
                Vec3::new(0.0, -0.5, 0.0)
            ]
        );
    }

    #[test]
    fn node_in_two_cells_is_averaged_and_missing_node_sits_at_origin() {
        let p = positions(3, 1, &[3, 0, 3]);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0], Vec3::ZERO);
        assert_eq!(p[2], Vec3::ZERO);
        let p = positions(3, 1, &[1, 0, 1]);
        assert_eq!(p[0], Vec3::ZERO);
    }
}
