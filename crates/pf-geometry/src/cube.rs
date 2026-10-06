//! Cubes as xLights wires them (`CubeModel::BuildCube`): strands laid in layers from a corner,
//! the wiring built for one start and style, then turned and mirrored into place.

use pf_model::{CubeStart, CubeStyle, StrandStyle, Vec3};

/// `{ turns about X, turns about Y, turns about Z, mirror left to right }` per start corner and
/// style, in xLights' order (`CubeModel.cpp`).
const TRANSFORMS: [[i64; 4]; 48] = [
    [1, 0, -1, 0],
    [0, 0, -1, 1],
    [0, -1, 0, 1],
    [0, 0, 0, 0],
    [-1, 2, 0, 1],
    [-1, -1, 0, 0],
    [1, 0, -1, 1],
    [0, 0, -1, 0],
    [0, -1, 0, 0],
    [0, 0, 0, 1],
    [-1, 2, 0, 0],
    [-1, -1, 0, 1],
    [1, 0, 1, 1],
    [0, 0, 1, 0],
    [0, -1, 2, 0],
    [0, 0, 2, 1],
    [-1, 2, 2, 0],
    [-1, -1, 2, 1],
    [1, 0, 1, 0],
    [0, 0, 1, 1],
    [0, -1, 2, 1],
    [0, 0, 2, 0],
    [-1, 2, 2, 1],
    [-1, -1, 2, 0],
    [-1, 0, -1, 1],
    [0, 2, 1, 0],
    [0, 1, 0, 0],
    [0, 2, 0, 1],
    [-1, 0, 0, 0],
    [-1, 1, 0, 1],
    [-1, 0, -1, 0],
    [0, 2, 1, 1],
    [0, 1, 0, 1],
    [0, 2, 0, 0],
    [-1, 0, 0, 1],
    [-1, 1, 0, 0],
    [-1, 0, 1, 0],
    [0, 2, -1, 1],
    [0, -1, 2, 0],
    [2, 0, 0, 0],
    [-1, 2, 2, 0],
    [1, -1, 0, 0],
    [-1, 0, 1, 1],
    [0, 2, -1, 0],
    [0, -1, 2, 1],
    [2, 0, 0, 1],
    [-1, 2, 2, 1],
    [1, -1, 0, 1],
];

type Cell = (i64, i64, i64);

fn rotate_x90(p: &mut Cell, by: i64, mut h: i64, mut d: i64) {
    for _ in 0..by.abs() {
        if by > 0 {
            let t = p.1;
            p.1 = d - p.2 - 1;
            p.2 = t;
        } else {
            let t = p.2;
            p.2 = h - p.1 - 1;
            p.1 = t;
        }
        std::mem::swap(&mut h, &mut d);
    }
}

fn rotate_y90(p: &mut Cell, by: i64, mut w: i64, mut d: i64) {
    for _ in 0..by.abs() {
        if by > 0 {
            let t = p.2;
            p.2 = w - p.0 - 1;
            p.0 = t;
        } else {
            let t = p.0;
            p.0 = d - p.2 - 1;
            p.2 = t;
        }
        std::mem::swap(&mut w, &mut d);
    }
}

fn rotate_z90(p: &mut Cell, by: i64, mut w: i64, mut h: i64) {
    for _ in 0..by.abs() {
        if by > 0 {
            let t = p.0;
            p.0 = p.1;
            p.1 = w - t - 1;
        } else {
            let t = p.0;
            p.0 = h - p.1 - 1;
            p.1 = t;
        }
        std::mem::swap(&mut w, &mut h);
    }
}

/// The cell `[x, y, z]` of each pixel of a `width` x `height` x `depth` cube in wiring order:
/// x from the left, y from the bottom, z from the front.
pub fn cube_cells(
    width: u32,
    height: u32,
    depth: u32,
    start: CubeStart,
    style: CubeStyle,
    strand: StrandStyle,
    strand_per_layer: bool,
) -> Vec<[u32; 3]> {
    let strand = match strand {
        StrandStyle::ZigZag => 0,
        StrandStyle::NoZigZag => 1,
        StrandStyle::AlternatePixel => 2,
    };
    let [xr, yr, zr, mirror] = TRANSFORMS[start as usize * 6 + style as usize];
    // The wiring is built in a cube turned so its strands run along x and its layers stack in z.
    let (mut width, mut height, mut depth) = (i64::from(width), i64::from(height), i64::from(depth));
    if zr.abs() == 1 {
        std::mem::swap(&mut width, &mut height);
    }
    if yr.abs() == 1 {
        std::mem::swap(&mut width, &mut depth);
    }
    if xr.abs() == 1 {
        std::mem::swap(&mut height, &mut depth);
    }
    let total = width * height * depth;
    (0..total)
        .map(|i| {
            let z = i / (width * height);
            let base = i % (width * height);
            let mut y = base / width;
            let mut x = if (strand == 1 || y % 2 == 0) && strand != 2 {
                base % width
            } else if strand == 2 {
                let pos = base % width + 1;
                if pos <= (width + 1) / 2 {
                    2 * (pos - 1)
                } else {
                    (width - pos) * 2 + 1
                }
            } else {
                width - base % width - 1
            };
            if !strand_per_layer && z % 2 != 0 {
                y = height - y - 1;
                if height % 2 != 0 && strand == 0 {
                    x = width - x - 1;
                }
            }
            let mut p = (x, y, z);
            let (mut w, mut h, mut d) = (width, height, depth);
            rotate_x90(&mut p, xr, h, d);
            if xr.abs() == 1 {
                std::mem::swap(&mut h, &mut d);
            }
            rotate_y90(&mut p, yr, w, d);
            if yr.abs() == 1 {
                std::mem::swap(&mut w, &mut d);
            }
            rotate_z90(&mut p, zr, w, h);
            if zr.abs() == 1 {
                std::mem::swap(&mut w, &mut h);
            }
            if mirror > 0 {
                p.0 = w - p.0 - 1;
            }
            [p.0 as u32, p.1 as u32, p.2 as u32]
        })
        .collect()
}

pub(crate) struct Cube {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub spacing: f32,
    pub start: CubeStart,
    pub style: CubeStyle,
    pub strand_style: StrandStyle,
    pub strand_per_layer: bool,
}

/// Pixels `spacing` apart, centered, the front layer toward +z.
pub(crate) fn positions(c: Cube) -> Vec<Vec3> {
    let middle = |n: u32| (n as f32 - 1.0) / 2.0;
    let (mx, my, mz) = (middle(c.width), middle(c.height), middle(c.depth));
    cube_cells(
        c.width,
        c.height,
        c.depth,
        c.start,
        c.style,
        c.strand_style,
        c.strand_per_layer,
    )
    .into_iter()
    .map(|[x, y, z]| {
        Vec3::new(
            (x as f32 - mx) * c.spacing,
            (y as f32 - my) * c.spacing,
            (mz - z as f32) * c.spacing,
        )
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    fn cells(start: CubeStart, style: CubeStyle, strand: StrandStyle, per_layer: bool) -> Vec<[u32; 3]> {
        cube_cells(2, 3, 2, start, style, strand, per_layer)
    }

    #[test]
    fn vertical_strands_run_up_and_down_from_the_front_bottom_left_toward_the_back() {
        let c = cells(
            CubeStart::FrontBottomLeft,
            CubeStyle::VerticalFrontBack,
            StrandStyle::ZigZag,
            false,
        );
        assert_eq!(c.len(), 12);
        // Up the front left edge, down the back left edge, then up the back right edge and down
        // the front right one.
        assert_eq!(
            c,
            [
                [0, 0, 0],
                [0, 1, 0],
                [0, 2, 0],
                [0, 2, 1],
                [0, 1, 1],
                [0, 0, 1],
                [1, 0, 1],
                [1, 1, 1],
                [1, 2, 1],
                [1, 2, 0],
                [1, 1, 0],
                [1, 0, 0],
            ]
        );
    }

    #[test]
    fn horizontal_and_stacked_styles_run_across_and_every_cell_is_used_once() {
        let c = cells(
            CubeStart::FrontBottomLeft,
            CubeStyle::HorizontalFrontBack,
            StrandStyle::NoZigZag,
            false,
        );
        // Front to back along the bottom left edge first.
        assert_eq!(&c[..2], [[0, 0, 0], [0, 0, 1]]);
        let c = cells(
            CubeStart::FrontBottomLeft,
            CubeStyle::HorizontalLeftRight,
            StrandStyle::NoZigZag,
            false,
        );
        assert_eq!(&c[..2], [[0, 0, 0], [1, 0, 0]]);
        for start in [
            CubeStart::FrontBottomLeft,
            CubeStart::BackTopRight,
            CubeStart::FrontTopRight,
        ] {
            for style in [
                CubeStyle::VerticalFrontBack,
                CubeStyle::VerticalLeftRight,
                CubeStyle::HorizontalFrontBack,
                CubeStyle::HorizontalLeftRight,
                CubeStyle::StackedFrontBack,
                CubeStyle::StackedLeftRight,
            ] {
                for strand in [
                    StrandStyle::ZigZag,
                    StrandStyle::NoZigZag,
                    StrandStyle::AlternatePixel,
                ] {
                    for per_layer in [false, true] {
                        let mut c = cells(start, style, strand, per_layer);
                        c.sort();
                        c.dedup();
                        assert_eq!(c.len(), 12, "{start:?} {style:?} {strand:?} {per_layer}");
                        assert!(c.iter().all(|&[x, y, z]| x < 2 && y < 3 && z < 2));
                    }
                }
            }
        }
        // The first pixel is at the start corner for vertical styles. (xLights' own table starts
        // some horizontal and stacked styles from a back-top corner at the front; kept as is.)
        let c = cells(
            CubeStart::BackTopRight,
            CubeStyle::VerticalLeftRight,
            StrandStyle::ZigZag,
            true,
        );
        assert_eq!(c[0], [1, 2, 1]);
        let c = cells(
            CubeStart::BackTopRight,
            CubeStyle::StackedLeftRight,
            StrandStyle::ZigZag,
            true,
        );
        assert_eq!(c[0], [0, 2, 0]);
    }

    #[test]
    fn pixels_are_spaced_evenly_and_centered_with_the_front_toward_the_viewer() {
        let p = positions(Cube {
            width: 2,
            height: 3,
            depth: 2,
            spacing: 0.5,
            start: CubeStart::FrontBottomLeft,
            style: CubeStyle::VerticalFrontBack,
            strand_style: StrandStyle::ZigZag,
            strand_per_layer: false,
        });
        assert_close(p[0], Vec3::new(-0.25, -0.5, 0.25));
        assert_close(p[2], Vec3::new(-0.25, 0.5, 0.25));
        assert_close(p[3], Vec3::new(-0.25, 0.5, -0.25));
        assert_close(p[11], Vec3::new(0.25, -0.5, 0.25));
    }
}
