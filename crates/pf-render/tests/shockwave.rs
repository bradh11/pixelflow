//! Shockwave, held to its behaviour specification: one test for each acceptance property, numbered
//! as the specification numbers them (`p01` to `p30`); then what its other sections say that no
//! property covers (`also_…`), the habits it lists as defects and keeps (`kept_…`), and the
//! measured numbers that came with it (`measured_…`).
//!
//! The rig, unless a test says otherwise: a buffer 101 columns by 101 rows, so the center cell at
//! the default center is (50, 50); the palette red alone; an effect 1,050 ms long with 50 ms
//! frames, so frame `k` of its 21 is `k` twentieths of the way; and every setting at its default
//! except those named. *In cells* is `scale` off and *hard* is `blendEdges` off. A cell is *lit*
//! when its alpha is above 0. Distances are in cells from the center cell, and *the row* is the
//! cells to the right of the center cell on its own row, named by their distance.
//!
//! The measured numbers are in `tests/shockwave/`, tab-separated with one header line. The
//! reference table (`reference.tsv`) and the widths (`widths.tsv`) are whole. The other three are
//! fixed samples of files too long to keep here: every fifth unlit and every 79th lit case of the
//! edge cells, a third each of the three kinds of restart (the old ring's end, exactly nothing,
//! and a hair above nothing), and every fifth thousandth of the acceleration response.
//! `measured_whole_files` runs the files they were drawn from, when told where those are.

use pf_render::{Canvas, Colors, EffectTime, Pixel, Rgba, Shade, Shader};
use pf_sequence::{EffectParams, Rgb, ShockwaveParams};

const RIG: Canvas = Canvas {
    columns: 101,
    rows: 101,
};
/// The rig's center cell.
const MIDDLE: (u32, u32) = (50, 50);
const RED: [Rgb; 1] = [Rgb::RED];
/// Black then white: a lit sample's red, green and blue are then the ring's progress.
const GAUGE: [Rgb; 2] = [Rgb::BLACK, Rgb::WHITE];

const LIT_RED: Rgba = Rgba::new(1.0, 0.0, 0.0, 1.0);

fn canvas(columns: u32, rows: u32) -> Canvas {
    Canvas { columns, rows }
}

fn defaults() -> ShockwaveParams {
    ShockwaveParams::default()
}

/// A ring whose radius and width go from the first of each pair to the second, counted in
/// cells, with soft edges.
fn in_cells(radius: (f32, f32), width: (f32, f32)) -> ShockwaveParams {
    ShockwaveParams {
        scale: false,
        ..in_percent(radius, width)
    }
}

/// The same, counted in percent of the buffer.
fn in_percent(radius: (f32, f32), width: (f32, f32)) -> ShockwaveParams {
    ShockwaveParams {
        start_radius: radius.0,
        end_radius: radius.1,
        start_width: width.0,
        end_width: width.1,
        ..defaults()
    }
}

/// A ring that keeps one radius and one width throughout.
fn still(radius: f32, width: f32) -> ((f32, f32), (f32, f32)) {
    ((radius, radius), (width, width))
}

fn hard(p: ShockwaveParams) -> ShockwaveParams {
    ShockwaveParams {
        blend_edges: false,
        ..p
    }
}

/// One lit cell, the center cell: a hard disc of no radius, as thin as a band gets.
fn dot() -> ShockwaveParams {
    hard(in_cells((0.0, 0.0), (0.0, 0.0)))
}

/// A hard disc that covers the center cell on every frame, to read the ring's color from.
fn disc() -> ShockwaveParams {
    hard(in_cells((0.0, 0.0), (10.0, 10.0)))
}

/// Frame `k` of an effect `frames` frames long, with 50 ms frames.
fn frame(k: u64, frames: u64) -> EffectTime {
    EffectTime::within(0, frames * 50, k * 50).with_frame_ms(50)
}

/// `at_ms` into an effect `length_ms` long, with frames of `frame_ms`.
fn timed(length_ms: u64, frame_ms: u32, at_ms: u64) -> EffectTime {
    EffectTime::within(0, length_ms, at_ms).with_frame_ms(frame_ms)
}

/// Where cell `x` of `cells` is, as a pixel position, worked out in single precision.
fn place(x: u32, cells: u32) -> f32 {
    if cells > 1 {
        x as f32 / (cells - 1) as f32
    } else {
        0.5
    }
}

fn pixel(u: f32, v: f32) -> Pixel {
    Pixel {
        u,
        v,
        index: 0,
        count: 1,
    }
}

fn rgb(c: Rgba) -> [f32; 3] {
    [c.r, c.g, c.b]
}

fn bits(c: Rgba) -> [u32; 4] {
    [c.r, c.g, c.b, c.a].map(f32::to_bits)
}

/// An alpha as its whole number of 255ths, which every alpha is.
fn level(c: Rgba) -> u32 {
    let level = (c.a * 255.0).round();
    assert_eq!(
        (level / 255.0).to_bits(),
        c.a.to_bits(),
        "alpha {} is not a whole number of 255ths",
        c.a
    );
    level as u32
}

/// The squared distance between two cells: always a whole number.
fn apart(a: (u32, u32), b: (i64, i64)) -> i64 {
    let (across, up) = (i64::from(a.0) - b.0, i64::from(a.1) - b.1);
    across * across + up * up
}

fn distance(a: (u32, u32), b: (i64, i64)) -> f64 {
    (apart(a, b) as f64).sqrt()
}

/// The cells of `canvas` whose distance from the cell `center` is `inner` to `outer`, both
/// included, bottom row first. Squared distances are whole numbers, so the comparison is exact
/// wherever the bounds' own squares are (whole cells, halves and quarters), and the other bounds
/// used below have no cell within reach of them.
fn ring(canvas: Canvas, center: (i64, i64), inner: f64, outer: f64) -> Vec<(u32, u32)> {
    all_cells(canvas)
        .filter(|&cell| {
            let apart = apart(cell, center) as f64;
            apart >= inner * inner && apart <= outer * outer
        })
        .collect()
}

fn all_cells(canvas: Canvas) -> impl Iterator<Item = (u32, u32)> {
    (0..canvas.rows).flat_map(move |y| (0..canvas.columns).map(move |x| (x, y)))
}

/// One frame of a Shockwave.
struct Frame {
    shader: Shader,
    canvas: Canvas,
}

impl Frame {
    fn new(p: ShockwaveParams, palette: &[Rgb], canvas: Canvas, time: EffectTime) -> Self {
        Self::seeded(p, palette, canvas, time, 0)
    }

    fn seeded(p: ShockwaveParams, palette: &[Rgb], canvas: Canvas, time: EffectTime, seed: u64) -> Self {
        Self {
            shader: Shader::new(
                &EffectParams::Shockwave(p),
                &time,
                Colors::new(palette),
                seed,
                canvas,
            ),
            canvas,
        }
    }

    /// Frame `k` of the rig's 21, in red on the rig's buffer.
    fn rig(p: ShockwaveParams, k: u64) -> Self {
        Self::new(p, &RED, RIG, frame(k, 21))
    }

    fn at(&self, u: f32, v: f32) -> Rgba {
        self.shader.shade(&pixel(u, v))
    }

    /// The sample at cell (x, y), counting from the bottom left.
    fn cell(&self, (x, y): (u32, u32)) -> Rgba {
        self.at(place(x, self.canvas.columns), place(y, self.canvas.rows))
    }

    /// Every cell of the buffer with its sample, bottom row first.
    fn cells(&self) -> Vec<((u32, u32), Rgba)> {
        all_cells(self.canvas)
            .map(|cell| (cell, self.cell(cell)))
            .collect()
    }

    fn bits(&self) -> Vec<[u32; 4]> {
        all_cells(self.canvas).map(|cell| bits(self.cell(cell))).collect()
    }

    /// The lit cells, bottom row first.
    fn lit(&self) -> Vec<(u32, u32)> {
        all_cells(self.canvas)
            .filter(|&cell| self.cell(cell).a > 0.0)
            .collect()
    }

    /// The lit cells of the row to the right of `center`, by their distance from it.
    fn row_from(&self, center: (u32, u32)) -> Vec<u32> {
        (0..self.canvas.columns - center.0)
            .filter(|d| self.cell((center.0 + d, center.1)).a > 0.0)
            .collect()
    }

    /// The lit cells of the rig's row.
    fn row(&self) -> Vec<u32> {
        self.row_from(MIDDLE)
    }

    /// The lit cells of a buffer one row high (or one column wide), by their place along it.
    fn line(&self) -> Vec<u32> {
        assert!(self.canvas.rows == 1 || self.canvas.columns == 1);
        self.lit().into_iter().map(|(x, y)| x.max(y)).collect()
    }

    /// The lit cells must be exactly `cells`, each returning exactly `color`, and every other
    /// cell must return exactly nothing: black, with no alpha.
    fn expect(&self, cells: &[(u32, u32)], color: Rgba, what: &str) {
        assert_eq!(self.lit(), cells, "{what}: the lit cells");
        for (cell, c) in self.cells() {
            let want = if cells.contains(&cell) { color } else { Rgba::CLEAR };
            assert_eq!(bits(c), bits(want), "{what}: cell {cell:?}");
        }
    }
}

/// The lit cells of the rig's row on frame `k`.
fn row(p: ShockwaveParams, k: u64) -> Vec<u32> {
    Frame::rig(p, k).row()
}

fn span(from: u32, to: u32) -> Vec<u32> {
    (from..=to).collect()
}

/// The ring's progress at `time`, read off the color of a disc: red, green and blue alike.
fn progress(p: ShockwaveParams, time: EffectTime) -> f32 {
    let p = ShockwaveParams {
        cycles: p.cycles,
        acceleration: p.acceleration,
        ..disc()
    };
    let c = Frame::new(p, &GAUGE, canvas(3, 3), time).cell((1, 1));
    assert_eq!(c.a, 1.0);
    assert!(c.r == c.g && c.g == c.b);
    c.r
}

/// The progress of each frame of an effect `frames` frames long with `rings` rings.
fn progress_by_frame(rings: u32, frames: u64) -> Vec<f32> {
    let p = ShockwaveParams {
        cycles: rings,
        ..defaults()
    };
    (0..frames).map(|k| progress(p, frame(k, frames))).collect()
}

fn accelerated(acceleration: f32) -> ShockwaveParams {
    ShockwaveParams {
        acceleration,
        ..defaults()
    }
}

fn assert_near(got: &[f32], want: &[f64], tolerance: f64, what: &str) {
    assert_eq!(got.len(), want.len(), "{what}");
    for (k, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(
            (f64::from(*g) - w).abs() <= tolerance,
            "{what}: item {k} is {g}, not {w}"
        );
    }
}

/// `values`, `times` times over.
fn repeated(values: &[f64], times: usize) -> Vec<f64> {
    values.repeat(times)
}

/// Property 1: Unlit is black and clear; lit is the palette color.
#[test]
fn p01_unlit_is_black_and_clear_and_lit_is_the_palette_color() {
    let p = hard(in_cells((30.0, 30.0), (4.0, 4.0)));
    let band = ring(RIG, (50, 50), 28.0, 32.0);
    assert_eq!(band.len(), 760);
    for k in [0, 1, 7, 13, 20] {
        Frame::rig(p, k).expect(&band, LIT_RED, &format!("frame {k}"));
    }
}

/// Property 2: The center is one whole cell.
#[test]
fn p02_the_center_is_one_whole_cell() {
    let cases = [
        ((101, 101), (50, 50)),
        ((100, 100), (50, 50)),
        ((41, 21), (20, 10)),
        ((40, 20), (20, 10)),
        ((50, 20), (25, 10)),
        ((3, 3), (1, 1)),
        ((2, 2), (1, 1)),
        ((1, 1), (0, 0)),
    ];
    for ((columns, rows), center) in cases {
        let frame = Frame::new(dot(), &RED, canvas(columns, rows), frame(0, 21));
        frame.expect(&[center], LIT_RED, &format!("{columns} by {rows}"));
    }
}

/// The center settings of the table in the specification's section 2, and the center cell's
/// column each gives on a row of so many cells.
const CENTER_SETTINGS: [f32; 13] = [
    0.0, 1.0, 3.0, 10.0, 25.0, 33.0, 49.0, 50.0, 51.0, 75.0, 90.0, 99.0, 100.0,
];
const CENTER_CELLS: [(u32, [u32; 13]); 8] = [
    (1, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
    (2, [0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2]),
    (7, [0, 0, 0, 0, 1, 2, 3, 3, 3, 5, 6, 6, 7]),
    (40, [0, 0, 1, 4, 10, 13, 19, 20, 20, 30, 36, 39, 40]),
    (41, [0, 0, 1, 4, 10, 13, 20, 20, 20, 30, 36, 40, 41]),
    (100, [0, 1, 3, 10, 25, 33, 49, 50, 51, 75, 90, 99, 100]),
    (101, [0, 1, 3, 10, 25, 33, 49, 50, 51, 75, 90, 99, 101]),
    (200, [0, 2, 6, 20, 50, 66, 98, 100, 102, 150, 180, 198, 200]),
];

/// Property 3: Center settings.
#[test]
fn p03_center_settings() {
    for (cells, columns) in CENTER_CELLS {
        for (setting, column) in CENTER_SETTINGS.into_iter().zip(columns) {
            // A cell beyond the last is off the buffer: nothing is lit.
            let on_the_line: Vec<u32> = if column < cells { vec![column] } else { vec![] };

            let across = ShockwaveParams {
                center_x: setting,
                ..dot()
            };
            let lit = Frame::new(across, &RED, canvas(cells, 1), frame(0, 21)).line();
            assert_eq!(lit, on_the_line, "centerX {setting} on a row of {cells}");

            let up = ShockwaveParams {
                center_y: setting,
                ..dot()
            };
            let lit = Frame::new(up, &RED, canvas(1, cells), frame(0, 21)).line();
            assert_eq!(lit, on_the_line, "centerY {setting} on a column of {cells}");
        }
    }

    let both = ShockwaveParams {
        center_x: 25.0,
        center_y: 75.0,
        ..dot()
    };
    for ((columns, rows), center) in [((101, 101), (25, 75)), ((41, 21), (10, 15)), ((50, 20), (12, 15))] {
        let frame = Frame::new(both, &RED, canvas(columns, rows), frame(0, 21));
        frame.expect(&[center], LIT_RED, &format!("{columns} by {rows}"));
    }
}

/// Property 4: A center of 100 is one cell past the last.
#[test]
fn p04_a_center_of_100_is_one_cell_past_the_last() {
    let buffer = canvas(41, 41);
    let disc = |width: f32, center_x: f32, center_y: f32| ShockwaveParams {
        center_x,
        center_y,
        ..hard(in_cells((0.0, 0.0), (width, width)))
    };
    // Column 38 of row 20, and rows 18 to 22 of columns 39 and 40.
    let mut beside: Vec<(u32, u32)> = vec![(38, 20)];
    beside.extend((18..=22).flat_map(|y| [(39, y), (40, y)]));
    beside.sort_by_key(|&(x, y)| (y, x));
    assert_eq!(beside.len(), 11);

    let right = Frame::new(disc(6.0, 100.0, 50.0), &RED, buffer, frame(0, 21));
    right.expect(&beside, LIT_RED, "centerX 100");

    let mut above: Vec<(u32, u32)> = beside.iter().map(|&(x, y)| (y, x)).collect();
    above.sort_by_key(|&(x, y)| (y, x));
    let top = Frame::new(disc(6.0, 50.0, 100.0), &RED, buffer, frame(0, 21));
    top.expect(&above, LIT_RED, "centerY 100");

    for (center_x, center_y) in [(100.0, 50.0), (50.0, 100.0)] {
        let thin = Frame::new(disc(0.0, center_x, center_y), &RED, buffer, frame(0, 21));
        thin.expect(&[], LIT_RED, "width 0");
    }
}

/// Property 5: A radius counts cells.
#[test]
fn p05_a_radius_counts_cells() {
    for (radius, count) in [(10.0, 248), (30.0, 760), (45.0, 1136)] {
        let band = ring(RIG, (50, 50), f64::from(radius) - 2.0, f64::from(radius) + 2.0);
        assert_eq!(band.len(), count, "radius {radius}");
        for k in [0, 9, 20] {
            let p = hard(in_cells((radius, radius), (4.0, 4.0)));
            Frame::rig(p, k).expect(&band, LIT_RED, &format!("radius {radius}, frame {k}"));
        }
    }
}

/// Property 6: A width counts cells, and the band is never under half a cell.
#[test]
fn p06_a_width_counts_cells_and_the_band_is_never_under_half_a_cell() {
    let cases = [
        (0.0, 0.25, 92),
        (1.0, 0.5, 200),
        (2.0, 1.0, 384),
        (3.0, 1.5, 564),
        (4.0, 2.0, 760),
        (5.0, 2.5, 936),
        (10.0, 5.0, 1912),
        (20.0, 10.0, 3780),
    ];
    for (width, either_side, count) in cases {
        let band = ring(RIG, (50, 50), 30.0 - either_side, 30.0 + either_side);
        assert_eq!(band.len(), count, "width {width}");
        let p = hard(in_cells((30.0, 30.0), (width, width)));
        Frame::rig(p, 3).expect(&band, LIT_RED, &format!("width {width}"));
    }
}

/// Property 7: In percent, sizes follow the longer side.
#[test]
fn p07_in_percent_sizes_follow_the_longer_side() {
    let cases = [
        ((101, 101), (50.0, 10.0), (50, 50), (20.25, 30.25), 1572),
        ((101, 101), (100.0, 5.0), (50, 50), (48.0, 53.0), 1396),
        ((100, 100), (50.0, 10.0), (50, 50), (20.0, 30.0), 1576),
        ((201, 81), (40.0, 10.0), (100, 40), (30.2, 50.2), 4256),
        ((81, 201), (40.0, 10.0), (40, 100), (30.2, 50.2), 4256),
    ];
    for ((columns, rows), (radius, width), center, (inner, outer), count) in cases {
        let buffer = canvas(columns, rows);
        let band = ring(buffer, center, inner, outer);
        let what = format!("{columns} by {rows}, radius {radius}, width {width}");
        assert_eq!(band.len(), count, "{what}");
        let (radius, width) = still(radius, width);
        let p = hard(in_percent(radius, width));
        Frame::new(p, &RED, buffer, frame(4, 21)).expect(&band, LIT_RED, &what);
    }
}

/// A hard ring in percent on a row of 101 cells with the center at its first cell, so that a
/// cell's column is its distance: the lit cells.
fn on_a_row_of_101(radius: f32, width: f32) -> Vec<u32> {
    let (radius, width) = still(radius, width);
    let p = ShockwaveParams {
        center_x: 0.0,
        ..hard(in_percent(radius, width))
    };
    Frame::new(p, &RED, canvas(101, 1), frame(0, 21)).line()
}

/// Property 8: In percent, widths are whole cells and radii are not.
#[test]
fn p08_in_percent_widths_are_whole_cells_and_radii_are_not() {
    // A radius of 50 is 25.25 cells.
    for width in [0.0, 1.0, 1.99] {
        assert_eq!(on_a_row_of_101(50.0, width), [25], "width {width}");
    }
    for width in [2.0, 2.97] {
        assert_eq!(on_a_row_of_101(50.0, width), [25, 26], "width {width}");
    }
    assert_eq!(on_a_row_of_101(50.0, 3.0), span(24, 26));
    assert_eq!(on_a_row_of_101(50.0, 5.0), span(23, 27));
    assert_eq!(on_a_row_of_101(50.0, 10.0), span(21, 30));

    // A width of 4 is 4 cells.
    for (radius, from, to) in [
        (40.0, 19, 22),
        (41.0, 19, 22),
        (42.0, 20, 23),
        (43.0, 20, 23),
        (44.0, 21, 24),
        (100.0, 49, 52),
    ] {
        assert_eq!(on_a_row_of_101(radius, 4.0), span(from, to), "radius {radius}");
    }
}

/// Property 9: Fractions are dropped.
#[test]
fn p09_fractions_are_dropped() {
    type Set = fn(&mut ShockwaveParams, f32);
    // The setting, a value with a fraction, the whole number it must be the same as, and the
    // next whole number, which it must not.
    let cases: [(&str, Set, f32, f32, f32); 10] = [
        ("centerX", |p, v| p.center_x = v, 50.9, 50.0, 51.0),
        ("centerY", |p, v| p.center_y = v, 49.99, 49.0, 50.0),
        ("startRadius", |p, v| p.start_radius = v, 3.9, 3.0, 4.0),
        ("endRadius", |p, v| p.end_radius = v, 60.99, 60.0, 61.0),
        ("startWidth", |p, v| p.start_width = v, 5.9, 5.0, 6.0),
        ("endWidth", |p, v| p.end_width = v, 10.5, 10.0, 11.0),
        ("acceleration", |p, v| p.acceleration = v, 2.9, 2.0, 3.0),
        ("acceleration", |p, v| p.acceleration = v, -2.9, -2.0, -3.0),
        ("acceleration", |p, v| p.acceleration = v, 0.9, 0.0, 1.0),
        ("acceleration", |p, v| p.acceleration = v, -0.9, 0.0, -1.0),
    ];
    for (name, set, with_fraction, whole, next) in cases {
        for scale in [false, true] {
            for blend_edges in [false, true] {
                let base = ShockwaveParams {
                    scale,
                    blend_edges,
                    ..defaults()
                };
                let frames = |value: f32| -> Vec<Vec<[u32; 4]>> {
                    let mut p = base;
                    set(&mut p, value);
                    [0, 7, 14, 20].map(|k| Frame::rig(p, k).bits()).to_vec()
                };
                let what = format!("{name} {with_fraction}, scale {scale}, blendEdges {blend_edges}");
                assert!(frames(with_fraction) == frames(whole), "{what} is not {whole}");
                assert!(frames(with_fraction) != frames(next), "{what} is {next}");
            }
        }
    }
}

/// Property 10: Equal steps, the start on the first frame and the end on the last.
#[test]
fn p10_equal_steps_from_the_start_on_the_first_frame_to_the_end_on_the_last() {
    let p = hard(in_cells((0.0, 20.0), (4.0, 4.0)));
    for k in 0..=20u32 {
        assert_eq!(
            row(p, u64::from(k)),
            span(k.saturating_sub(2), k + 2),
            "frame {k}"
        );
    }
}

/// Property 11: The width steps the same way.
#[test]
fn p11_the_width_steps_the_same_way() {
    let p = hard(in_cells((30.0, 30.0), (0.0, 20.0)));
    for (k, from, to) in [
        (0, 30, 30),
        (1, 30, 30),
        (2, 29, 31),
        (5, 28, 32),
        (10, 25, 35),
        (15, 23, 37),
        (19, 21, 39),
        (20, 20, 40),
    ] {
        assert_eq!(row(p, k), span(from, to), "frame {k}");
    }
}

/// Property 12: How many frames an effect has.
#[test]
fn p12_how_many_frames_an_effect_has() {
    // Length and frame time in ms, the frames that makes, the frame number at 500 ms, and the
    // progress shown there as the specification tables it.
    let cases = [
        (1000, 50, 20, 10, 0.5263),
        (975, 50, 20, 10, 0.5263),
        (951, 50, 20, 10, 0.5263),
        (950, 50, 19, 10, 0.5556),
        (1001, 50, 21, 10, 0.5000),
        (1050, 50, 21, 10, 0.5000),
        (1000, 25, 40, 20, 0.5128),
        (1000, 20, 50, 25, 0.5102),
        (1000, 33, 31, 15, 0.5000),
        (1000, 40, 25, 12, 0.5000),
        (1000, 100, 10, 5, 0.5556),
    ];
    for (length, frame_ms, frames, at_frame, tabled) in cases {
        let shown = f64::from(progress(defaults(), timed(length, frame_ms, 500)));
        let what = format!("{length} ms at {frame_ms} ms a frame shows {shown}");
        assert!((shown - tabled).abs() <= 0.0001, "{what}");
        assert!(
            (shown - f64::from(at_frame) / f64::from(frames - 1)).abs() <= 1e-6,
            "{what}"
        );
    }

    assert_eq!(progress(defaults(), timed(50, 50, 0)), 0.0);
    assert_eq!(progress(defaults(), timed(50, 50, 49)), 0.0);
    for (at, shown) in [(0, 0.0), (49, 0.0), (50, 1.0), (99, 1.0)] {
        assert_eq!(progress(defaults(), timed(100, 50, at)), shown, "{at} ms of 100");
    }
}

/// Property 13: A frame holds.
#[test]
fn p13_a_frame_holds() {
    let at = |ms: u64| Frame::new(defaults(), &RED, RIG, timed(1050, 50, ms)).bits();
    let held = at(500);
    assert!(held.iter().any(|c| c[3] != 0));
    for ms in [501, 525, 549] {
        assert!(at(ms) == held, "{ms} ms");
    }
    assert!(at(550) != held);
    assert!(at(499) != held);
}

/// Property 14: A frame depends on nothing else.
#[test]
fn p14_a_frame_depends_on_nothing_else() {
    let p = ShockwaveParams {
        center_x: 30.0,
        cycles: 2,
        acceleration: -3.0,
        ..in_cells((2.0, 60.0), (3.0, 25.0))
    };
    let palette = [Rgb::RED, Rgb::GREEN, Rgb::BLUE];
    let elapsed = 350;
    let reference = Frame::new(p, &palette, RIG, timed(1050, 50, elapsed)).bits();
    assert!(reference.iter().any(|c| c[3] != 0));

    // Wherever the effect sits in the sequence, and whatever the seed.
    for start in [0, 60_000, 123_457] {
        for seed in [0, 1, 0xDEAD_BEEF, u64::MAX] {
            let time = EffectTime::within(start, start + 1050, start + elapsed).with_frame_ms(50);
            let frame = Frame::seeded(p, &palette, RIG, time, seed);
            assert!(frame.bits() == reference, "starting at {start}, seed {seed}");
        }
    }

    // Frames asked for in another order, and samples asked for backwards.
    let frames: Vec<Frame> = [1000, 0, elapsed, 50]
        .into_iter()
        .map(|ms| Frame::new(p, &palette, RIG, timed(1050, 50, ms)))
        .collect();
    let _ = frames[0].bits();
    let _ = frames[1].bits();
    let mut backwards: Vec<[u32; 4]> = all_cells(RIG)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|cell| bits(frames[2].cell(cell)))
        .collect();
    backwards.reverse();
    assert!(backwards == reference);

    // From several threads at once, sharing one frame.
    let shared = &frames[2];
    let drawn: Vec<Vec<[u32; 4]>> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..4).map(|_| scope.spawn(|| shared.bits())).collect();
        threads.into_iter().map(|t| t.join().unwrap()).collect()
    });
    for frame in drawn {
        assert!(frame == reference);
    }
}

/// Property 15: Rings.
#[test]
fn p15_rings() {
    let ninths: Vec<f64> = (0..10).map(|k| f64::from(k) / 9.0).collect();
    let on_20_frames: [(u32, Vec<f64>); 11] = [
        (1, (0..20).map(|k| f64::from(k) / 19.0).collect()),
        (2, repeated(&ninths, 2)),
        (
            3,
            vec![
                0.0, 0.1765, 0.3529, 0.5294, 0.7059, 0.8824, 1.0, 0.0588, 0.2353, 0.4118, 0.5882, 0.7647,
                0.9412, 1.0, 0.1176, 0.2941, 0.4706, 0.6471, 0.8235, 1.0,
            ],
        ),
        (4, repeated(&[0.0, 0.25, 0.5, 0.75, 1.0], 4)),
        (5, repeated(&[0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0], 5)),
        (
            6,
            vec![
                0.0, 0.4286, 0.8571, 1.0, 0.2857, 0.7143, 1.0, 0.1429, 0.5714, 1.0, 1.0, 0.4286, 0.8571, 1.0,
                0.2857, 0.7143, 1.0, 0.1429, 0.5714, 1.0,
            ],
        ),
        (
            7,
            vec![
                0.0, 0.5385, 1.0, 0.0769, 0.6154, 1.0, 0.1538, 0.6923, 1.0, 0.2308, 0.7692, 1.0, 0.3077,
                0.8462, 1.0, 0.3846, 0.9231, 1.0, 0.4615, 1.0,
            ],
        ),
        (10, repeated(&[0.0, 1.0], 10)),
        (15, repeated(&[0.0, 1.0, 1.0, 1.0], 5)),
        (18, [vec![0.0], vec![1.0; 19]].concat()),
        (19, [vec![0.0], vec![1.0; 19]].concat()),
    ];
    for (rings, tabled) in on_20_frames {
        let what = format!("{rings} rings on 20 frames");
        assert_near(&progress_by_frame(rings, 20), &tabled, 0.0001, &what);
    }
    for rings in [20, 21, 37, 100] {
        assert_eq!(
            progress_by_frame(rings, 20),
            [0.0; 20],
            "{rings} rings on 20 frames"
        );
    }

    let half_steps: Vec<f64> = (0..21)
        .map(|k| match k {
            0..=9 => f64::from(k) / 9.5,
            10 | 20 => 1.0,
            _ => (f64::from(k) - 10.5) / 9.5,
        })
        .collect();
    let on_21_frames: [(u32, Vec<f64>); 5] = [
        (2, half_steps),
        (
            3,
            repeated(&[0.0, 1.0 / 6.0, 2.0 / 6.0, 0.5, 4.0 / 6.0, 5.0 / 6.0, 1.0], 3),
        ),
        (
            4,
            vec![
                0.0, 0.2353, 0.4706, 0.7059, 0.9412, 1.0, 0.1765, 0.4118, 0.6471, 0.8824, 1.0, 0.1176,
                0.3529, 0.5882, 0.8235, 1.0, 0.0588, 0.2941, 0.5294, 0.7647, 1.0,
            ],
        ),
        (7, repeated(&[0.0, 0.5, 1.0], 7)),
        (20, [vec![0.0], vec![1.0; 20]].concat()),
    ];
    for (rings, tabled) in on_21_frames {
        let what = format!("{rings} rings on 21 frames");
        assert_near(&progress_by_frame(rings, 21), &tabled, 0.0001, &what);
    }
    for rings in [21, 22] {
        assert_eq!(
            progress_by_frame(rings, 21),
            [0.0; 21],
            "{rings} rings on 21 frames"
        );
    }

    // Two rings over 20 frames: the second ten frames are the first ten again.
    let p = ShockwaveParams {
        cycles: 2,
        ..hard(in_cells((0.0, 20.0), (4.0, 4.0)))
    };
    let at = |k: u64| Frame::new(p, &RED, RIG, frame(k, 20)).bits();
    for k in 0..10 {
        assert!(at(k + 10) == at(k), "frame {}", k + 10);
    }
    assert!(at(0) != at(9));
}

/// Property 16: More rings than the effect can show.
#[test]
fn p16_more_rings_than_the_effect_can_show() {
    let rings = |cycles: u32| ShockwaveParams {
        cycles,
        ..hard(in_cells((0.0, 20.0), (4.0, 4.0)))
    };
    let at = |cycles: u32, k: u64| Frame::new(rings(cycles), &RED, RIG, frame(k, 20));
    let first = at(1, 0).bits();
    let last = at(1, 19).bits();
    assert!(first != last);

    for cycles in [20, 21, 40, 100] {
        for k in 0..20 {
            assert!(at(cycles, k).bits() == first, "{cycles} rings, frame {k}");
        }
    }

    assert!(at(19, 0).bits() == first);
    for k in 1..20 {
        assert!(at(19, k).bits() == last, "19 rings, frame {k}");
    }

    // With 18 rings likewise, but for the four cells 22 out along the center's row and column
    // on the two frames where a ring should be exactly at its end.
    let compass = [(50, 28), (28, 50), (72, 50), (50, 72)];
    assert!(at(18, 0).bits() == first);
    for k in 1..20 {
        let frame = at(18, k);
        if k == 9 || k == 19 {
            let mut lit = at(1, 19).lit();
            lit.retain(|cell| !compass.contains(cell));
            frame.expect(&lit, LIT_RED, &format!("18 rings, frame {k}"));
        } else {
            assert!(frame.bits() == last, "18 rings, frame {k}");
        }
    }
    for cell in compass {
        assert_eq!(at(1, 19).cell(cell), LIT_RED);
    }
}

/// Progress after acceleration at a few shares of a ring's time, as the specification's section
/// 3 tables it: the accelerations, then each time with the progress at each.
const ACCELERATIONS: [f32; 8] = [-10.0, -7.0, -5.0, -1.0, 0.0, 1.0, 5.0, 10.0];
const ACCELERATED: [(u64, [f64; 8]); 5] = [
    (100, [0.4095, 0.3323, 0.2753, 0.1462, 0.1, 0.0316, 0.0032, 0.0100]),
    (
        250,
        [0.7627, 0.6681, 0.5848, 0.3505, 0.25, 0.1250, 0.0236, 0.0221],
    ),
    (500, [0.9688, 0.9298, 0.8797, 0.6464, 0.5, 0.3536, 0.1213, 0.0625]),
    (
        750,
        [0.9990, 0.9951, 0.9855, 0.8750, 0.75, 0.6495, 0.3779, 0.2208],
    ),
    (900, [1.0000, 0.9999, 0.9991, 0.9684, 0.9, 0.8538, 0.6853, 0.5314]),
];

/// Property 17: Acceleration.
#[test]
fn p17_acceleration() {
    // An effect of 1,001 frames: frame `k` is `k` thousandths of the ring's time.
    let shown = |acceleration: f32, k: u64| progress(accelerated(acceleration), frame(k, 1001));

    for (k, tabled) in ACCELERATED {
        for (acceleration, want) in ACCELERATIONS.into_iter().zip(tabled) {
            let got = f64::from(shown(acceleration, k));
            assert!(
                (got - want).abs() <= 0.0005,
                "acceleration {acceleration} at {k} thousandths shows {got}, not {want}"
            );
        }
    }

    // Appendix A: every whole acceleration at every 25th thousandth.
    let appendix = Table::read(ACCELERATION);
    let mut held = 0;
    for line in appendix
        .rows
        .iter()
        .filter(|line| thousandth(line).is_multiple_of(25))
    {
        let k = thousandth(line);
        for (column, acceleration) in (-10..=10).enumerate() {
            let want: f64 = line[column + 1].parse().unwrap();
            let got = f64::from(shown(acceleration as f32, k));
            assert!(
                (got - want).abs() <= 0.0005,
                "acceleration {acceleration} at {k} thousandths shows {got}, not {want}"
            );
            held += 1;
        }
    }
    assert_eq!(held, 41 * 21);

    // Every bend starts at 0, ends at 1 and never runs backwards.
    for acceleration in -10..=10 {
        let acceleration = acceleration as f32;
        let mut before = shown(acceleration, 0);
        assert_eq!(before, 0.0, "acceleration {acceleration}");
        for k in 1..=1000 {
            let now = shown(acceleration, k);
            assert!(now >= before, "acceleration {acceleration} falls at frame {k}");
            before = now;
        }
        assert_eq!(before, 1.0, "acceleration {acceleration}");
    }

    let cases: [(f32, [&[u32]; 3]); 7] = [
        (-10.0, [&[30, 31], &[38, 39], &[39, 40]]),
        (-5.0, [&[23, 24], &[35, 36], &[39, 40]]),
        (-1.0, [&[14, 15], &[25, 26], &[34, 35, 36]]),
        (0.0, [&[9, 10, 11], &[19, 20, 21], &[29, 30, 31]]),
        (1.0, [&[4, 5, 6], &[14, 15], &[25, 26]]),
        (5.0, [&[0, 1], &[4, 5], &[15, 16]]),
        (10.0, [&[0, 1], &[2, 3], &[8, 9]]),
    ];
    for (acceleration, lit) in cases {
        let p = ShockwaveParams {
            acceleration,
            ..hard(in_cells((0.0, 40.0), (2.0, 2.0)))
        };
        for (k, cells) in [5, 10, 15].into_iter().zip(lit) {
            assert_eq!(row(p, k), cells, "acceleration {acceleration}, frame {k}");
        }
    }
}

/// Property 18: Acceleration bends the color and the width with the radius.
#[test]
fn p18_acceleration_bends_the_color_and_the_width_with_the_radius() {
    let red_then_blue = [Rgb::RED, Rgb::BLUE];
    for (acceleration, color) in [(-10.0, [0.03125, 0.0, 0.96875]), (10.0, [0.9375, 0.0, 0.0625])] {
        let p = ShockwaveParams {
            acceleration,
            ..disc()
        };
        let c = Frame::new(p, &red_then_blue, RIG, frame(10, 21)).cell(MIDDLE);
        assert_eq!(c.a, 1.0);
        for (got, want) in rgb(c).into_iter().zip(color) {
            assert!((got - want).abs() <= 0.0005, "acceleration {acceleration}: {c:?}");
        }
    }

    // Half way through 1,001 frames, on a row of 1,001 cells from the center.
    for (acceleration, from, to) in [(-10.0, 404, 596), (0.0, 450, 550), (10.0, 494, 506)] {
        let p = ShockwaveParams {
            center_x: 0.0,
            acceleration,
            ..hard(in_cells((500.0, 500.0), (0.0, 200.0)))
        };
        let lit = Frame::new(p, &RED, canvas(1001, 1), frame(500, 1001)).line();
        assert_eq!(lit, span(from, to), "acceleration {acceleration}");
    }
}

/// Property 19: Soft edges.
#[test]
fn p19_soft_edges() {
    let frame = Frame::rig(in_cells((30.0, 30.0), (20.0, 20.0)), 6);
    let rising = [0, 25, 50, 76, 102, 127, 153, 178, 204, 229, 255];
    for (step, want) in rising.into_iter().enumerate() {
        let step = step as u32;
        assert_eq!(level(frame.cell((70 + step, 50))), want, "cell {}", 20 + step);
        assert_eq!(level(frame.cell((90 - step, 50))), want, "cell {}", 40 - step);
    }

    let (mut lit, mut at_half_strength) = (0, 0);
    for (cell, c) in frame.cells() {
        let whole = f64::from(level(c));
        // The straight line from 1 at distance 30 to 0 at distances 20 and 40, in 255ths.
        let line = (255.0 * (1.0 - (distance(cell, (50, 50)) - 30.0).abs() / 10.0)).max(0.0);
        assert!(whole <= line + 1e-9, "cell {cell:?} is above the line");
        assert!(
            whole >= line - 1.0 - 1e-9,
            "cell {cell:?} is more than a 255th below"
        );
        if c.a > 0.0 {
            lit += 1;
            assert_eq!(rgb(c), [1.0, 0.0, 0.0], "cell {cell:?}");
        }
        if c.a >= 0.5 {
            at_half_strength += 1;
        }
    }
    assert_eq!((lit, at_half_strength), (3740, 1872));
}

/// Property 20: Cells on an edge.
#[test]
fn p20_cells_on_an_edge() {
    let soft = in_cells((0.0, 20.0), (4.0, 4.0));
    let sharp = Frame::rig(hard(soft), 10);
    assert_eq!(sharp.cell((58, 50)), LIT_RED);
    assert_eq!(sharp.cell((62, 50)), LIT_RED);

    let soft = Frame::rig(soft, 10);
    for (cell, want) in [(8, 0), (9, 127), (10, 255), (11, 127), (12, 0)] {
        assert_eq!(level(soft.cell((50 + cell, 50))), want, "cell {cell}");
    }
}

/// The palettes of the table in the specification's section 4, and the color each gives at
/// progress 0, a quarter, a half, three quarters and 1.
fn palette_table() -> Vec<(Vec<Rgb>, [[f32; 3]; 5])> {
    const R: Rgb = Rgb::RED;
    const G: Rgb = Rgb::GREEN;
    const B: Rgb = Rgb::BLUE;
    const W: Rgb = Rgb::WHITE;
    let white = [1.0, 1.0, 1.0];
    let red = [1.0, 0.0, 0.0];
    vec![
        (vec![], [white; 5]),
        (vec![R], [red; 5]),
        (
            vec![R, B],
            [
                red,
                [0.75, 0.0, 0.25],
                [0.5, 0.0, 0.5],
                [0.25, 0.0, 0.75],
                [0.0, 0.0, 1.0],
            ],
        ),
        (
            vec![R, G, B],
            [
                red,
                [0.5, 0.5, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.5, 0.5],
                [0.0, 0.0, 1.0],
            ],
        ),
        (
            vec![R, G, B, W],
            [red, [0.25, 0.75, 0.0], [0.0, 0.5, 0.5], [0.25, 0.25, 1.0], white],
        ),
        (
            vec![R, G, B, W, Rgb::BLACK],
            [red, [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], white, [0.0, 0.0, 0.0]],
        ),
    ]
}

/// Property 21: Color through the palette.
#[test]
fn p21_color_through_the_palette() {
    for (palette, colors) in palette_table() {
        for (k, want) in [0, 5, 10, 15, 20].into_iter().zip(colors) {
            let c = Frame::new(disc(), &palette, RIG, frame(k, 21)).cell(MIDDLE);
            assert_eq!(c.a, 1.0);
            for (got, want) in rgb(c).into_iter().zip(want) {
                assert!(
                    (got - want).abs() <= 0.0001,
                    "{} colors, frame {k}: {c:?}",
                    palette.len()
                );
            }
        }
    }

    // With two rings the color starts again with the second.
    let twice = ShockwaveParams { cycles: 2, ..disc() };
    let palette = [Rgb::RED, Rgb::GREEN, Rgb::BLUE];
    for (k, want) in [
        (0, [1.0, 0.0, 0.0]),
        (10, [1.0, 0.0, 0.0]),
        (9, [0.0, 0.0, 1.0]),
        (19, [0.0, 0.0, 1.0]),
    ] {
        let c = Frame::new(twice, &palette, RIG, frame(k, 20)).cell(MIDDLE);
        assert_eq!((rgb(c), c.a), (want, 1.0), "frame {k}");
    }

    // A black palette color is an opaque black ring.
    let black = Frame::new(disc(), &[Rgb::BLACK], RIG, frame(3, 21));
    let covered = ring(RIG, (50, 50), 0.0, 5.0);
    black.expect(&covered, Rgba::BLACK, "black");
}

/// Property 22: Round, not stretched.
#[test]
fn p22_round_not_stretched() {
    let buffer = canvas(101, 41);

    let (radius, width) = still(15.0, 2.0);
    let lit = Frame::new(hard(in_cells(radius, width)), &RED, buffer, frame(0, 21)).lit();
    assert_eq!(lit.len(), 188);
    let columns = lit.iter().map(|cell| cell.0);
    let rows = lit.iter().map(|cell| cell.1);
    assert_eq!((columns.clone().min(), columns.max()), (Some(34), Some(66)));
    assert_eq!((rows.clone().min(), rows.max()), (Some(4), Some(36)));

    let (radius, width) = still(50.0, 10.0);
    let band = ring(buffer, (50, 20), 20.25, 30.25);
    assert_eq!(band.len(), 972);
    let frame = Frame::new(hard(in_percent(radius, width)), &RED, buffer, frame(0, 21));
    frame.expect(&band, LIT_RED, "in percent");
    let columns = band.iter().map(|cell| cell.0);
    assert_eq!((columns.clone().min(), columns.max()), (Some(20), Some(80)));
    for y in 0..41 {
        assert!(band.iter().any(|cell| cell.1 == y), "row {y}");
    }
}

/// Property 23: A single row and a single column.
#[test]
fn p23_a_single_row_and_a_single_column() {
    let cases: [(u64, &[u32]); 5] = [
        (0, &[19, 20, 21]),
        (5, &[14, 15, 25, 26]),
        (10, &[8, 9, 10, 30, 31, 32]),
        (15, &[2, 3, 4, 5, 35, 36, 37, 38]),
        (19, &[0, 1, 39, 40]),
    ];
    for buffer in [canvas(41, 1), canvas(1, 41)] {
        for (k, lit) in cases {
            let frame = Frame::new(hard(defaults()), &RED, buffer, frame(k, 20));
            assert_eq!(frame.line(), lit, "{buffer:?}, frame {k}");
        }
    }
}

/// Property 24: Thin rings on a line.
#[test]
fn p24_thin_rings_on_a_line() {
    let p = hard(in_cells((0.0, 20.0), (0.0, 0.0)));
    let lit = |k: u64, frames: u64| Frame::new(p, &RED, canvas(41, 1), frame(k, frames)).line();

    for k in 0..20 {
        let count = match k {
            0 => 1,
            5..=14 => 0,
            _ => 2,
        };
        assert_eq!(lit(k, 20).len(), count, "frame {k} of 20");
    }

    assert_eq!(lit(0, 21), [20]);
    for k in 1..=20u32 {
        assert_eq!(lit(u64::from(k), 21), [20 - k, 20 + k], "frame {k} of 21");
    }
}

/// Property 25: Tiny buffers.
#[test]
fn p25_tiny_buffers() {
    let at = |cells: u32, k: u64| Frame::new(defaults(), &RED, canvas(cells, cells), frame(k, 20));

    for (k, want) in [(0, 249), (5, 117), (10, 0), (15, 0), (19, 0)] {
        assert_eq!(level(at(1, k).cell((0, 0))), want, "1 by 1, frame {k}");
    }

    let two = at(2, 19);
    for (cell, want) in [((1, 0), 1.0), ((0, 1), 1.0), ((0, 0), 0.0), ((1, 1), 0.0)] {
        assert_eq!(two.cell(cell).a, want, "2 by 2, cell {cell:?}");
    }

    let three = at(3, 15);
    for (cell, c) in three.cells() {
        let want = match cell {
            (1, 1) => 0,
            (1, _) | (_, 1) => 63,
            _ => 23,
        };
        assert_eq!(level(c), want, "3 by 3, cell {cell:?}");
    }
}

/// Property 26: Each pixel takes its nearest cell.
#[test]
fn p26_each_pixel_takes_its_nearest_cell() {
    let nine = Frame::new(dot(), &RED, canvas(9, 9), frame(0, 21));
    assert_eq!(nine.lit(), [(4, 4)]);
    for along in [0.4375, 0.46875, 0.5, 0.546875] {
        assert_eq!(nine.at(along, 0.5), LIT_RED, "u {along}");
        assert_eq!(nine.at(0.5, along), LIT_RED, "v {along}");
    }
    for along in [0.421875, 0.5625] {
        assert_eq!(nine.at(along, 0.5), Rgba::CLEAR, "u {along}");
        assert_eq!(nine.at(0.5, along), Rgba::CLEAR, "v {along}");
    }

    // On 5 by 5, a center setting of 20 for each cell: 0 is the first and 80 the last.
    let lit_at = |center_x: f32, center_y: f32, u: f32, v: f32| {
        let p = ShockwaveParams {
            center_x,
            center_y,
            ..dot()
        };
        Frame::new(p, &RED, canvas(5, 5), frame(0, 21)).at(u, v).a == 1.0
    };
    // A position, and the cell it takes of the five along its side.
    let takes = [
        (-0.5, 0),
        (1.5, 4),
        (f32::NAN, 0),
        (f32::INFINITY, 0),
        (f32::NEG_INFINITY, 0),
        (0.0, 0),
        (1.0, 4),
        (0.3, 1),
    ];
    for (position, taken) in takes {
        for cell in 0..5u8 {
            let setting = f32::from(cell) * 20.0;
            assert_eq!(
                lit_at(setting, 50.0, position, 0.5),
                cell == taken,
                "u {position} against column {cell}"
            );
            assert_eq!(
                lit_at(50.0, setting, 0.5, position),
                cell == taken,
                "v {position} against row {cell}"
            );
        }
    }
}

/// The stretch of `u` (or of `v`, with `upright`) that is lit on the line through (`u`, `v`),
/// about the lit place `from`: where it starts and where it stops, found by halving.
fn lit_stretch(frame: &Frame, upright: bool, other: f32, from: f64, reach: f64) -> (f64, f64) {
    let lit = |along: f64| {
        let along = along as f32;
        let c = if upright {
            frame.at(other, along)
        } else {
            frame.at(along, other)
        };
        c.a > 0.0
    };
    assert!(lit(from), "nothing is lit at {from}");
    let edge = |mut dark: f64| {
        assert!(!lit(dark), "{dark} is lit too");
        let mut bright = from;
        for _ in 0..60 {
            let middle = (dark + bright) / 2.0;
            if lit(middle) {
                bright = middle;
            } else {
                dark = middle;
            }
        }
        bright
    };
    (edge(from - reach), edge(from + reach))
}

/// Property 27: Buffers of more than 1,048,576 cells.
#[test]
fn p27_buffers_of_more_than_1048576_cells() {
    // The buffer, and the grid its picture is on.
    let cases = [
        ((1024, 1024), (1024, 1024)),
        ((2048, 2048), (1024, 1024)),
        ((2048, 1024), (1448, 724)),
        ((1100, 1000), (1073, 976)),
    ];
    for ((columns, rows), (grid_columns, grid_rows)) in cases {
        let frame = Frame::new(dot(), &RED, canvas(columns, rows), frame(0, 21));
        // The one lit cell is the grid's center cell.
        let cell = 1.0 / f64::from(grid_columns - 1);
        let u = f64::from(grid_columns / 2) * cell;
        let v = (f64::from(grid_rows / 2) / f64::from(grid_rows - 1)) as f32;
        let (from, to) = lit_stretch(&frame, false, v, u, 2.0 * cell);
        let width = to - from;
        assert!(
            (width / cell - 1.0).abs() <= 0.01,
            "{columns} by {rows}: the lit stretch is 1/{} wide",
            1.0 / width
        );
    }

    // On 2048 by 2048 a radius of 100 cells is 100 cells of the coarse grid: about 200 of the
    // buffer's own columns.
    let (radius, width) = still(100.0, 2.0);
    let frame = Frame::new(
        hard(in_cells(radius, width)),
        &RED,
        canvas(2048, 2048),
        frame(0, 21),
    );
    let center = 512.0 / 1023.0;
    let (from, to) = lit_stretch(
        &frame,
        false,
        center as f32,
        center + 100.0 / 1023.0,
        3.0 / 1023.0,
    );
    let own_columns = |u: f64| (u - center) * 2047.0;
    assert!(
        (own_columns(from) - 197.0).abs() <= 0.5,
        "from {}",
        own_columns(from)
    );
    assert!((own_columns(to) - 203.0).abs() <= 0.5, "to {}", own_columns(to));
}

/// Property 28: A ring that closes in and thins.
#[test]
fn p28_a_ring_that_closes_in_and_thins() {
    let p = hard(in_cells((40.0, 0.0), (10.0, 0.0)));
    for (k, from, to) in [
        (0, 35, 45),
        (5, 27, 33),
        (10, 18, 22),
        (15, 9, 11),
        (19, 2, 2),
        (20, 0, 0),
    ] {
        assert_eq!(row(p, k), span(from, to), "frame {k}");
    }
}

/// Property 29: The default effect on the rig.
#[test]
fn p29_the_default_effect_on_the_rig() {
    /// One frame as the specification tables it: how many cells are lit, the nearest and the
    /// furthest of them, and alpha in 255ths along the row from the first distance given.
    struct Tabled {
        frame: u64,
        lit: usize,
        nearest: f64,
        furthest: f64,
        first: u32,
        alphas: &'static [u32],
    }
    let tabled = |frame, lit, (nearest, furthest), first, alphas| Tabled {
        frame,
        lit,
        nearest,
        furthest,
        first,
        alphas,
    };
    let cases = [
        tabled(0, 25, (0.0, 2.83), 0, &[203, 204, 102]),
        tabled(5, 504, (9.90, 16.03), 10, &[9, 91, 173, 254, 173, 92, 10]),
        tabled(
            10,
            1220,
            (21.84, 29.21),
            22,
            &[16, 84, 152, 220, 221, 153, 85, 17],
        ),
        tabled(
            15,
            2060,
            (33.73, 42.30),
            34,
            &[21, 80, 138, 196, 254, 196, 138, 80, 21],
        ),
        tabled(20, 2560, (45.54, 55.47), 46, &[25, 76, 127, 178, 229]),
    ];
    for case in cases {
        let Tabled {
            frame: k,
            nearest,
            furthest,
            first,
            alphas,
            ..
        } = case;
        let frame = Frame::rig(defaults(), k);
        let lit = frame.lit();
        assert_eq!(lit.len(), case.lit, "frame {k}");
        let distances = lit.iter().map(|&cell| distance(cell, (50, 50)));
        let least = distances.clone().fold(f64::INFINITY, f64::min);
        let most = distances.fold(0.0, f64::max);
        assert!((least - nearest).abs() <= 0.005, "frame {k}: nearest {least}");
        assert!((most - furthest).abs() <= 0.005, "frame {k}: furthest {most}");

        for (d, &want) in (first..).zip(alphas) {
            let got = level(frame.cell((50 + d, 50)));
            assert!(got.abs_diff(want) <= 1, "frame {k}, cell {d}: {got}, not {want}");
        }
        // The row is lit nowhere else.
        let along: Vec<u32> = (first..).take(alphas.len()).filter(|d| *d <= 50).collect();
        assert_eq!(frame.row(), along, "frame {k}");
    }
}

/// Property 30: Nothing breaks at the extremes.
#[test]
fn p30_nothing_breaks_at_the_extremes() {
    let buffers = [(1, 1), (2, 1), (1, 2), (2, 2), (3, 3), (7, 5), (101, 3)];
    let radii = [(0.0, 750.0), (750.0, 0.0), (750.0, 750.0), (0.0, 0.0)];
    let widths = [(0.0, 255.0), (255.0, 0.0)];
    let hour = 3_600_000;
    let times = [
        timed(1050, 50, 0),
        timed(1050, 50, 500),
        timed(1050, 50, 1000),
        timed(1, 50, 0),
        timed(hour, 50, hour - 50),
    ];
    let hostile = [
        (f32::NAN, 0.5),
        (0.5, f32::NAN),
        (f32::INFINITY, f32::NEG_INFINITY),
        (-3.0, 7.0),
    ];
    // Every combination of the two-way choices, one bit of a count for each, and of the radii.
    let settings: Vec<ShockwaveParams> = (0..512u32)
        .map(|choice| {
            let either = |bit: u32| choice >> bit & 1 == 1;
            ShockwaveParams {
                center_x: if either(0) { 100.0 } else { 0.0 },
                center_y: if either(1) { 100.0 } else { 0.0 },
                cycles: if either(2) { 100 } else { 1 },
                acceleration: if either(3) { 10.0 } else { -10.0 },
                blend_edges: either(4),
                scale: either(5),
                ..in_cells(radii[(choice >> 7) as usize], widths[usize::from(either(6))])
            }
        })
        .collect();
    let mut distinct = settings.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), 512);

    let mut frames = 0;
    for (columns, rows) in buffers {
        for p in &settings {
            for time in times {
                let frame = Frame::new(*p, &RED, canvas(columns, rows), time);
                let cells = frame.cells().into_iter().map(|(_, c)| c);
                let stray = hostile.map(|(u, v)| frame.at(u, v));
                for c in cells.chain(stray) {
                    for part in [c.r, c.g, c.b, c.a] {
                        assert!(
                            (0.0..=1.0).contains(&part),
                            "{p:?} at {time:?} on {columns} by {rows}: {c:?}"
                        );
                    }
                }
                frames += 1;
            }
        }
    }
    assert_eq!(frames, 17_920);
}

/// Section 2: in percent a width is a whole number of cells at the start and at the end, and
/// takes every value between them on the way.
#[test]
fn also_percent_widths_are_whole_cells_only_at_the_start_and_the_end() {
    // A hard ring on a row of so many cells with the center at its first cell.
    let lit = |cells: u32, radius: f32, width: f32| {
        let (radius, width) = still(radius, width);
        let p = ShockwaveParams {
            center_x: 0.0,
            ..hard(in_percent(radius, width))
        };
        Frame::new(p, &RED, canvas(cells, 1), frame(0, 21)).line()
    };
    // The longer side, a width in percent, the cells it comes to, and the cells lit at a radius
    // of 50 percent, a quarter of a cell past a whole number (25.25, 10.25 and 75.25 cells).
    let cases = [
        (101, 1.0, 1, (25, 25)),
        (101, 2.0, 2, (25, 26)),
        (101, 5.0, 5, (23, 27)),
        (101, 10.0, 10, (21, 30)),
        (101, 33.0, 33, (9, 41)),
        (41, 1.0, 0, (10, 10)),
        (41, 2.0, 0, (10, 10)),
        (41, 5.0, 2, (10, 11)),
        (41, 10.0, 4, (9, 12)),
        (41, 33.0, 13, (4, 16)),
        (301, 1.0, 3, (74, 76)),
        (301, 2.0, 6, (73, 78)),
        (301, 5.0, 15, (68, 82)),
        (301, 33.0, 99, (26, 124)),
    ];
    for (cells, percent, width, (from, to)) in cases {
        let what = format!("{percent} percent of {cells} is {width} cells");
        assert_eq!(lit(cells, 50.0, percent), span(from, to), "{what}");
        if width >= 1 {
            assert_eq!((to - from + 1), width, "{what}");
        }
    }
    // What tells no cells from one: at a radius of 100 percent, half way between two cells of
    // 41, a band half a cell wide lights neither and a band one cell wide would light both.
    assert_eq!(lit(41, 100.0, 1.0), [0u32; 0]);
    assert_eq!(lit(41, 100.0, 2.0), [0u32; 0]);
    assert_eq!(lit(101, 100.0, 1.0), [50, 51]);

    // From 0 to 3 percent of 101 the width is 1.5 cells half way: at a radius of 25.25 cells the
    // band reaches from 24.5 to 26, and cell 25 is a third of the way in from its inner edge.
    let p = ShockwaveParams {
        center_x: 0.0,
        ..in_percent((50.0, 50.0), (0.0, 3.0))
    };
    let half_way = Frame::new(p, &RED, canvas(101, 1), frame(1, 3));
    assert_eq!(half_way.line(), [25]);
    assert_eq!(level(half_way.cell((25, 0))), 170);
    let sharp = Frame::new(hard(p), &RED, canvas(101, 1), frame(1, 3));
    assert_eq!(sharp.line(), [25, 26]);
}

/// Section 2: at the top of the ranges, in cells, a radius of 750 with a width of 255 lights the
/// cells 623 to 877 from the center; in percent the ring is far outside any buffer.
#[test]
fn also_the_top_of_the_ranges() {
    let (radius, width) = still(750.0, 255.0);
    let p = ShockwaveParams {
        center_x: 0.0,
        ..hard(in_cells(radius, width))
    };
    assert_eq!(
        Frame::new(p, &RED, canvas(1501, 1), frame(0, 21)).line(),
        span(623, 877)
    );
    for (columns, rows) in [(1501, 1), (101, 101), (7, 5)] {
        let p = ShockwaveParams { scale: true, ..p };
        let frame = Frame::new(p, &RED, canvas(columns, rows), frame(0, 21));
        frame.expect(&[], LIT_RED, "750 percent");
    }
}

/// Section 4: what a soft ring amounts to, and a soft disc.
#[test]
fn also_a_soft_ring_is_half_the_light_of_a_hard_one() {
    let p = in_cells((30.0, 30.0), (20.0, 20.0));
    assert_eq!(Frame::rig(hard(p), 0).lit().len(), 3780);
    let soft = Frame::rig(p, 0);
    let sum: f64 = soft.cells().iter().map(|(_, c)| f64::from(c.a)).sum();
    assert_eq!(sum.round(), 1878.0);
    // Full strength only where the distance is exactly the radius.
    for (cell, c) in soft.cells() {
        assert_eq!(c.a == 1.0, apart(cell, (50, 50)) == 900, "cell {cell:?}");
    }

    // A soft disc is brightest at the distance of its radius, not at its center.
    let disc = Frame::rig(in_cells((3.0, 3.0), (10.0, 10.0)), 0);
    let along: Vec<u32> = (0..10).map(|d| level(disc.cell((50 + d, 50)))).collect();
    assert_eq!(along, [102, 153, 204, 255, 204, 153, 102, 50, 0, 0]);
    assert_eq!(disc.row(), span(0, 7));
}

/// Section 4: outside the band the color is black as well as clear, inside the ring as outside;
/// a cell of a soft band whose alpha has come down to nothing shows nothing either.
#[test]
fn also_what_is_left_unlit_is_black() {
    let frame = Frame::new(
        in_cells((30.0, 30.0), (20.0, 20.0)),
        &[Rgb::WHITE],
        RIG,
        frame(0, 21),
    );
    for (cell, c) in frame.cells() {
        let apart = apart(cell, (50, 50));
        if !(400..=1600).contains(&apart) {
            assert_eq!(bits(c), bits(Rgba::CLEAR), "cell {cell:?}");
        } else if apart == 400 || apart == 1600 {
            assert_eq!(c.a, 0.0, "cell {cell:?}, on an edge");
        } else {
            assert_eq!(rgb(c), [1.0, 1.0, 1.0], "cell {cell:?}");
        }
    }
}

/// Section 5.2: pixels sharing a cell match, and a pixel between two cells shows what the nearer
/// one shows.
#[test]
fn also_the_picture_is_made_of_whole_cells() {
    let buffer = canvas(41, 21);
    let frame = Frame::new(defaults(), &[Rgb::RED, Rgb::BLUE], buffer, frame(9, 20));
    assert!(frame.lit().len() > 20);
    for (cell, c) in frame.cells() {
        let (u, v) = (place(cell.0, 41), place(cell.1, 21));
        for (across, up) in [
            (0.3, 0.0),
            (-0.3, 0.3),
            (0.0, -0.45),
            (0.49, 0.49),
            (-0.49, -0.49),
        ] {
            let near = frame.at(u + across / 40.0, v + up / 20.0);
            // Past the buffer's own edges a position counts as the edge.
            assert_eq!(
                bits(near),
                bits(c),
                "cell {cell:?} from {across}, {up} of a cell away"
            );
        }
    }
}

/// Section 5.3: at the default center a ring sits half a cell to the right and up on even
/// counts of cells.
#[test]
fn also_a_centered_ring_is_half_a_cell_off_on_even_sizes() {
    // Along the center's own row and column, the first and last cells a thin ring of this radius
    // lights: (left, right, bottom, top).
    let ends = |radius: f32| {
        let (radius, width) = still(radius, 0.0);
        let frame = Frame::new(
            hard(in_cells(radius, width)),
            &RED,
            canvas(100, 100),
            frame(0, 21),
        );
        let lit = |cell: (u32, u32)| frame.cell(cell).a > 0.0;
        (lit((0, 50)), lit((99, 50)), lit((50, 0)), lit((50, 99)))
    };
    // It reaches the last column and the top row one cell sooner than the first and the bottom.
    assert_eq!(ends(48.0), (false, false, false, false));
    assert_eq!(ends(49.0), (false, true, false, true));
    assert_eq!(ends(50.0), (true, false, true, false));

    // On odd counts it is exactly centered: the frame is its own mirror image both ways.
    let frame = Frame::rig(defaults(), 12);
    for (cell, c) in frame.cells() {
        assert_eq!(bits(c), bits(frame.cell((100 - cell.0, cell.1))), "cell {cell:?}");
        assert_eq!(bits(c), bits(frame.cell((cell.0, 100 - cell.1))), "cell {cell:?}");
    }
}

/// The number of cells along one side of the grid a buffer's picture is on: a cell 250 cells
/// from the grid's first is lit alone, and where the middle of its stretch falls along the side
/// says how many cells the side has.
fn grid_side(buffer: Canvas, upright: bool) -> u32 {
    let (radius, width) = still(250.0, 0.0);
    let p = ShockwaveParams {
        center_x: 0.0,
        center_y: 0.0,
        ..hard(in_cells(radius, width))
    };
    let frame = Frame::new(p, &RED, buffer, frame(0, 21));
    let side = if upright { buffer.rows } else { buffer.columns };
    let lit = |along: f64| {
        let along = along as f32;
        let c = if upright {
            frame.at(0.0, along)
        } else {
            frame.at(along, 0.0)
        };
        c.a > 0.0
    };
    let step = 1.0 / (8.0 * f64::from(side));
    let found = (0..=8 * side)
        .map(|k| f64::from(k) * step)
        .find(|&along| lit(along))
        .expect("a lit cell along the side");
    // Far enough either side to be off the cell, however coarse the grid.
    let (from, to) = lit_stretch(&frame, upright, 0.0, found, 40.0 * step);
    (250.0 / ((from + to) / 2.0)).round() as u32 + 1
}

/// Section 5.6: the grid a buffer of more than 1,048,576 cells gets.
#[test]
fn also_the_grid_of_a_buffer_of_more_than_1048576_cells() {
    let cases = [
        ((1000, 1000), (1000, 1000)),
        ((1024, 1024), (1024, 1024)),
        ((1025, 1024), (1024, 1023)),
        ((1100, 1000), (1073, 976)),
        ((2000, 600), (1869, 560)),
        ((4096, 300), (3783, 277)),
        ((2048, 1024), (1448, 724)),
        ((2048, 2048), (1024, 1024)),
        ((3000, 3000), (1024, 1024)),
    ];
    for ((columns, rows), grid) in cases {
        let buffer = canvas(columns, rows);
        assert_eq!(
            (grid_side(buffer, false), grid_side(buffer, true)),
            grid,
            "{columns} by {rows}"
        );
    }

    // Sizes in percent keep their proportion to the buffer: a radius of 50 is a quarter of the
    // longer side on the coarse grid as on a fine one.
    let (radius, width) = still(50.0, 0.0);
    let p = ShockwaveParams {
        center_x: 0.0,
        center_y: 0.0,
        ..hard(in_percent(radius, width))
    };
    let frame = Frame::new(p, &RED, canvas(2048, 2048), frame(0, 21));
    let (from, to) = lit_stretch(&frame, false, 0.0, 0.25 * 1024.0 / 1023.0, 2.0 / 1023.0);
    assert!(((from + to) / 2.0 - 0.25).abs() < 0.001);
}

/// Section 6: the edge cases no property names.
#[test]
fn also_edge_cases() {
    // Equal radii hold the ring still while its width and color go on changing.
    let p = hard(in_cells((30.0, 30.0), (2.0, 12.0)));
    let middles: Vec<f64> = [0, 10, 20]
        .into_iter()
        .map(|k| {
            let row = Frame::new(p, &[Rgb::RED, Rgb::BLUE], RIG, frame(k, 21)).row();
            f64::from(row[0] + row[row.len() - 1]) / 2.0
        })
        .collect();
    assert_eq!(middles, [30.0, 30.0, 30.0]);
    assert_eq!(row(p, 0).len(), 3);
    assert_eq!(row(p, 20).len(), 13);

    // A radius of 0 is a disc as far out as half the width.
    assert_eq!(row(hard(in_cells((0.0, 0.0), (9.0, 9.0))), 0), span(0, 4));
    // A width of twice the radius or more fills the middle.
    assert_eq!(row(hard(in_cells((6.0, 6.0), (12.0, 12.0))), 0), span(0, 12));
    assert_eq!(row(hard(in_cells((6.0, 6.0), (11.0, 11.0))), 0), span(1, 11));

    // A radius beyond the buffer leaves only the band's inner part on it, and then nothing.
    let (radius, width) = still(80.0, 30.0);
    let partly = Frame::rig(hard(in_cells(radius, width)), 0);
    assert_eq!(partly.lit(), ring(RIG, (50, 50), 65.0, 95.0));
    assert!(distance((0, 0), (50, 50)) > 65.0 && !partly.lit().is_empty());
    let (radius, width) = still(90.0, 30.0);
    Frame::rig(hard(in_cells(radius, width)), 0).expect(&[], LIT_RED, "radius 90");

    // The least width applies to the width of each frame, not to its start and its end: from 0
    // to 1 over five frames the band is half a cell wide on the second frame, and the cell one
    // up from the row's 30th is a fifteenth of the way from the radius to its edge.
    let widening = in_cells((30.0, 30.0), (0.0, 1.0));
    let second = Frame::new(widening, &RED, RIG, frame(1, 5));
    assert_eq!(level(second.cell((80, 51))), 238);

    // An effect of one frame shows the start; of two frames, the start and then the end.
    let p = hard(in_cells((10.0, 30.0), (2.0, 6.0)));
    let rows = |length: u64, at: u64| Frame::new(p, &RED, RIG, timed(length, 50, at)).row();
    assert_eq!(rows(50, 0), span(9, 11));
    assert_eq!(rows(1, 0), span(9, 11));
    assert_eq!(rows(100, 0), span(9, 11));
    assert_eq!(rows(100, 50), span(27, 33));

    // An hour: progress still reaches exactly 1 on the last frame, and not before.
    let hour = 3_600_000;
    assert_eq!(progress(defaults(), timed(hour, 50, hour - 50)), 1.0);
    assert_eq!(progress(defaults(), timed(hour, 50, hour - 1)), 1.0);
    assert!(progress(defaults(), timed(hour, 50, hour - 51)) < 1.0);

    // An effect that starts between two of the sequence's frames is told a time that is not a
    // whole number of its own frames, and rounds it down like any other.
    let between = EffectTime::within(1_020, 2_070, 1_550).with_frame_ms(50);
    assert_eq!(between.elapsed_ms, 530);
    assert_eq!(
        progress(defaults(), between),
        progress(defaults(), timed(1050, 50, 500))
    );
}

/// Section 3: the ring's radius, width and color all follow one progress, ring by ring.
#[test]
fn also_each_ring_is_bent_on_its_own() {
    // Two rings over 42 frames have 21 frames each, and each goes as one ring does over 21.
    for acceleration in [-4.0, 6.0] {
        let alone = |k: u64| progress(accelerated(acceleration), frame(k, 21));
        let p = ShockwaveParams {
            cycles: 2,
            ..accelerated(acceleration)
        };
        for k in 0..=20 {
            for ring in 0..2 {
                let shown = progress(p, frame(k + 21 * ring, 42));
                assert!((shown - alone(k)).abs() <= 1e-6, "frame {}", k + 21 * ring);
            }
        }
    }
}

/// Section 8, item 1: a ring under a cell wide breaks into dots, and on a line of pixels it
/// blinks.
#[test]
fn kept_1_thin_rings_break_into_dots_and_blink() {
    let at_30 = |width: f32, blend_edges: bool| {
        let (radius, width) = still(30.0, width);
        Frame::rig(
            ShockwaveParams {
                blend_edges,
                ..in_cells(radius, width)
            },
            0,
        )
    };
    let counts = |frame: &Frame| {
        let cells = frame.cells();
        (
            cells.iter().filter(|(_, c)| c.a > 0.0).count(),
            cells.iter().filter(|(_, c)| c.a >= 0.5).count(),
        )
    };
    assert_eq!(counts(&at_30(0.0, false)), (92, 92));
    assert_eq!(counts(&at_30(1.0, false)), (200, 200));
    assert_eq!(counts(&at_30(1.0, true)), (200, 92));
    assert_eq!(counts(&at_30(0.0, true)), (92, 60));

    // Round the circle in ten-degree stretches: the thinnest ring leaves some of them dark, and
    // a ring one cell wide none.
    let dark_stretches = |frame: &Frame| {
        let mut lit = [false; 36];
        for (x, y) in frame.lit() {
            let degrees = (f64::from(y) - 50.0).atan2(f64::from(x) - 50.0).to_degrees();
            lit[(degrees.rem_euclid(360.0) / 10.0) as usize % 36] = true;
        }
        lit.iter().filter(|lit| !**lit).count()
    };
    assert!(dark_stretches(&at_30(0.0, false)) > 0);
    assert_eq!(dark_stretches(&at_30(1.0, false)), 0);

    // On a row of 41 cells, radius 0 to 20 over 20 frames.
    let on_a_row = |width: f32, blend_edges: bool, k: u64| {
        let p = ShockwaveParams {
            blend_edges,
            ..in_cells((0.0, 20.0), (width, width))
        };
        Frame::new(p, &RED, canvas(41, 1), frame(k, 20))
    };
    for k in 0..20 {
        let dark_for_the_thinnest = (5..=14).contains(&k);
        assert_eq!(
            on_a_row(0.0, false, k).lit().is_empty(),
            dark_for_the_thinnest,
            "frame {k}"
        );
        let soft = on_a_row(1.0, true, k);
        let brightest = soft.cells().iter().map(|(_, c)| c.a).fold(0.0, f32::max);
        assert!(brightest > 0.0, "frame {k}");
        assert_eq!(brightest < 0.5, dark_for_the_thinnest, "frame {k}: {brightest}");
        for width in [2.0, 3.0, 5.0] {
            for blend_edges in [false, true] {
                assert!(
                    !on_a_row(width, blend_edges, k).lit().is_empty(),
                    "width {width}, frame {k}"
                );
            }
        }
    }

    // The default effect on a single row of so many cells is dark on so many of its 20 frames.
    for (cells, dark) in [
        (5, 10),
        (8, 10),
        (10, 8),
        (12, 7),
        (16, 7),
        (19, 10),
        (20, 0),
        (21, 0),
        (41, 0),
    ] {
        let dark_frames = (0..20)
            .filter(|&k| {
                Frame::new(defaults(), &RED, canvas(cells, 1), frame(k, 20))
                    .lit()
                    .is_empty()
            })
            .count();
        assert_eq!(dark_frames, dark, "a row of {cells} cells");
    }
}

/// The frame and ring counts, up to 40 frames, at which a frame a ring's share begins exactly on
/// shows the ring before at its end (the specification's section 8, item 3).
const OLD_RINGS_END: [(u64, &[u32]); 22] = [
    (10, &[6]),
    (14, &[6, 12]),
    (15, &[9]),
    (16, &[10]),
    (18, &[10, 14]),
    (20, &[6, 12, 14, 18]),
    (21, &[9, 18]),
    (22, &[10, 18, 20]),
    (24, &[15]),
    (25, &[15]),
    (26, &[10, 14, 20, 22]),
    (27, &[15, 21]),
    (28, &[6, 12, 18, 24]),
    (30, &[9, 18, 21, 27]),
    (32, &[10, 20, 22, 26]),
    (33, &[15, 27, 30]),
    (34, &[6, 12, 24, 26]),
    (35, &[15, 21, 30]),
    (36, &[10, 14, 20, 22, 28, 34]),
    (38, &[14, 18, 22, 28, 34, 36]),
    (39, &[15, 21, 30, 33]),
    (40, &[6, 12, 14, 18, 24, 25, 26, 28, 34, 36]),
];

/// Section 8, item 3: a ring's first frame is sometimes the end of the ring before.
#[test]
fn kept_3_a_rings_first_frame_is_sometimes_the_end_of_the_ring_before() {
    let mut exact_restarts = 0;
    for frames in 2..=40u64 {
        // A ring needs more than a frame to itself to be seen moving at all.
        for rings in 2..frames {
            if frames % rings == 0 {
                continue;
            }
            let old = OLD_RINGS_END
                .iter()
                .any(|(f, r)| *f == frames && r.contains(&(rings as u32)));
            let p = ShockwaveParams {
                cycles: rings as u32,
                ..defaults()
            };
            // The frames on which a later ring's share begins exactly.
            for ring in (1..rings).filter(|ring| ring * frames % rings == 0) {
                let k = ring * frames / rings;
                let shown = progress(p, frame(k, frames));
                if old {
                    assert_eq!(shown, 1.0, "{rings} rings on {frames} frames, frame {k}");
                } else {
                    assert!(
                        shown < 1e-12,
                        "{rings} rings on {frames} frames, frame {k}: {shown}"
                    );
                }
                exact_restarts += 1;
            }
        }
    }
    assert!(exact_restarts > 300, "{exact_restarts}");
}

/// Section 8, item 8: a stronger positive acceleration is not always a slower start.
#[test]
fn kept_8_a_stronger_positive_acceleration_is_not_always_a_slower_start() {
    let shown = |acceleration: f32, k: u64| progress(accelerated(acceleration), frame(k, 1001));
    for other in [5.0, 6.0, 7.0, 8.0, 9.0] {
        for k in [25, 50, 75, 100] {
            assert!(shown(10.0, k) > shown(other, k), "against {other} at {k}");
        }
        assert!(shown(10.0, 900) < shown(other, 900), "against {other}");
    }
    assert!((shown(10.0, 100) - 0.0100).abs() < 0.0001);
    assert!((shown(6.0, 100) - 0.0028).abs() < 0.0001);
}

/// A table of measured numbers: tab-separated, under one header line.
struct Table {
    rows: Vec<Vec<&'static str>>,
}

impl Table {
    fn read(text: &'static str) -> Self {
        Self {
            rows: text
                .lines()
                .skip(1)
                .filter(|line| !line.is_empty())
                .map(|line| line.split('\t').collect())
                .collect(),
        }
    }
}

const REFERENCE: &str = include_str!("shockwave/reference.tsv");
const EDGES: &str = include_str!("shockwave/edges.sample.tsv");
const RESTARTS: &str = include_str!("shockwave/restarts.sample.tsv");
const WIDTHS: &str = include_str!("shockwave/widths.tsv");
const ACCELERATION: &str = include_str!("shockwave/acceleration.sample.tsv");

fn number<T: std::str::FromStr>(text: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    text.parse().unwrap()
}

/// Which thousandth of the ring's time a row of the acceleration table is for.
fn thousandth(line: &[&str]) -> u64 {
    (number::<f64>(line[0]) * 1000.0).round() as u64
}

/// Settings as the reference table writes them: those that differ from the defaults.
fn settings(text: &str) -> ShockwaveParams {
    let mut p = defaults();
    if text == "(defaults)" {
        return p;
    }
    for setting in text.split(", ") {
        let (key, value) = setting.split_once(' ').unwrap();
        match key {
            "scale" => p.scale = value == "on",
            "blendEdges" => p.blend_edges = value == "on",
            "centerX" => p.center_x = number(value),
            "centerY" => p.center_y = number(value),
            "startRadius" => p.start_radius = number(value),
            "endRadius" => p.end_radius = number(value),
            "startWidth" => p.start_width = number(value),
            "endWidth" => p.end_width = number(value),
            "acceleration" => p.acceleration = number(value),
            "cycles" => p.cycles = number(value),
            other => panic!("unknown setting {other}"),
        }
    }
    p
}

/// How many of the reference table's samples the effect returns exactly, number for number, and
/// how many it differs on only in the color of a cell that shows nothing (which is free).
fn reference_held(table: &Table) -> (usize, usize) {
    let (mut exact, mut free) = (0, 0);
    for line in &table.rows {
        let buffer = canvas(number(line[1]), number(line[2]));
        let time = timed(number(line[3]), number(line[4]), number(line[5]));
        let palette: Vec<Rgb> = line[6]
            .split(';')
            .map(|color| {
                let parts: Vec<u8> = color.split(',').map(number).collect();
                Rgb::new(parts[0], parts[1], parts[2])
            })
            .collect();
        let got =
            Frame::new(settings(line[7]), &palette, buffer, time).cell((number(line[8]), number(line[9])));
        let want = Rgba::new(
            number(line[10]),
            number(line[11]),
            number(line[12]),
            number(line[13]),
        );
        if bits(got) == bits(want) {
            exact += 1;
        } else if got.a == 0.0 && want.a == 0.0 && rgb(want) != [0.0; 3] {
            free += 1;
        } else {
            panic!("reference row {}: {got:?}, not {want:?}", line.join(" "));
        }
    }
    (exact, free)
}

/// Appendix B: the reference table, every sample exactly as it was measured.
#[test]
fn measured_reference_table() {
    let table = Table::read(REFERENCE);
    assert_eq!(table.rows.len(), 352);
    assert_eq!(reference_held(&table), (352, 0));
}

/// How many of the measured edge cases come out as measured. Each is a hard ring in cells on a
/// row of 141 cells with the center at its first cell, whose settings and frame put one of its
/// edges exactly on a cell: the cell is lit or it is not.
fn edges_held(table: &Table) -> usize {
    let mut held = 0;
    for line in &table.rows {
        let p = ShockwaveParams {
            center_x: 0.0,
            ..hard(in_cells(
                (number(line[1]), number(line[2])),
                (number(line[3]), number(line[4])),
            ))
        };
        let frame = Frame::new(p, &RED, canvas(141, 1), frame(number(line[5]), number(line[0])));
        let c = frame.cell((number(line[7]), 0));
        assert!(c == LIT_RED || c == Rgba::CLEAR);
        if (c == LIT_RED) == (line[8] == "1") {
            held += 1;
        }
    }
    held
}

/// Section 8, item 4: with hard edges a cell exactly on an edge is now and then left unlit.
#[test]
fn measured_edge_cells() {
    let table = Table::read(EDGES);
    let unlit = table.rows.iter().filter(|line| line[8] == "0").count();
    assert_eq!((table.rows.len(), unlit), (493, 193));
    assert_eq!(edges_held(&table), 493);
}

/// How many of the measured restarts show the progress measured: the frames on which a ring's
/// share begins exactly, read off the color, to the last bit.
fn restarts_held(table: &Table) -> usize {
    let mut held = 0;
    for line in &table.rows {
        let p = ShockwaveParams {
            cycles: number(line[1]),
            ..defaults()
        };
        let shown = progress(p, frame(number(line[2]), number(line[0])));
        if shown.to_bits() == number::<f32>(line[3]).to_bits() {
            held += 1;
        }
    }
    held
}

/// Section 8, item 3: which ring a frame shows when a ring's share begins exactly on it.
#[test]
fn measured_restarts() {
    let table = Table::read(RESTARTS);
    let old = table.rows.iter().filter(|line| line[3] == "1.0").count();
    assert_eq!((table.rows.len(), old), (511, 170));
    assert_eq!(restarts_held(&table), 511);
}

/// How wide, in cells, a hard disc of this width in percent is drawn on a square buffer with
/// the center at its first cell. Along the bottom row the disc reaches half its width, rounded
/// down; and the cell one row up from the end of that reach is inside the disc only when the
/// width is odd.
fn drawn_width(side: u32, percent: f32) -> u32 {
    let (radius, width) = still(0.0, percent);
    let p = ShockwaveParams {
        center_x: 0.0,
        center_y: 0.0,
        ..hard(in_percent(radius, width))
    };
    let frame = Frame::new(p, &RED, canvas(side, side), frame(0, 21));
    let reach = frame.row_from((0, 0)).into_iter().max().unwrap();
    assert!((1..side).contains(&reach), "the disc's edge is off the buffer");
    2 * reach + u32::from(frame.cell((reach, 1)).a > 0.0)
}

/// How many of the measured widths are drawn as measured.
fn widths_held(table: &Table) -> usize {
    table
        .rows
        .iter()
        .filter(|line| drawn_width(number(line[0]), number(line[1])) == number::<u32>(line[3]))
        .count()
}

/// Section 8, item 5: in percent, a width that should be an exact whole number of cells is
/// occasionally one cell less.
#[test]
fn measured_widths() {
    let table = Table::read(WIDTHS);
    let short: Vec<(u32, u32)> = table
        .rows
        .iter()
        .filter(|line| line[2] != line[3])
        .map(|line| (number(line[0]), number(line[1])))
        .collect();
    assert_eq!(table.rows.len(), 489);
    assert_eq!(short, [(58, 50), (70, 90), (114, 50), (116, 25), (116, 50)]);
    assert_eq!(widths_held(&table), 489);

    // The five, as the specification gives them.
    for (side, percent, drawn) in [
        (58, 50.0, 28),
        (70, 90.0, 62),
        (114, 50.0, 56),
        (116, 25.0, 28),
        (116, 50.0, 57),
    ] {
        assert_eq!(drawn_width(side, percent), drawn, "{percent} percent of {side}");
    }
}

/// How many of the measured progresses come out to the last bit: every whole acceleration at
/// each of the table's times, on an effect of 1,001 frames.
fn acceleration_held(table: &Table) -> usize {
    let mut held = 0;
    for line in &table.rows {
        for (column, acceleration) in (-10..=10).enumerate() {
            let shown = progress(accelerated(acceleration as f32), frame(thousandth(line), 1001));
            if shown.to_bits() == number::<f32>(line[column + 1]).to_bits() {
                held += 1;
            }
        }
    }
    held
}

/// Appendix A: the acceleration response.
#[test]
fn measured_acceleration() {
    let table = Table::read(ACCELERATION);
    assert_eq!(table.rows.len(), 201);
    assert_eq!(acceleration_held(&table), 201 * 21);
}

/// The whole of the five measured files, of which the tests above carry samples. They are not
/// kept with the tests: run this with `SHOCKWAVE_MEASURED` naming the directory they are in,
/// and it prints the share of each the effect reproduces exactly.
#[test]
#[ignore = "needs the whole measured files: set SHOCKWAVE_MEASURED to their directory"]
fn measured_whole_files() {
    let directory = std::env::var("SHOCKWAVE_MEASURED").expect("SHOCKWAVE_MEASURED");
    let read = |name: &str| -> Table {
        let text = std::fs::read_to_string(format!("{directory}/{name}")).unwrap();
        Table::read(text.leak())
    };

    let reference = read("reference.tsv");
    let (exact, free) = reference_held(&reference);
    println!(
        "reference.tsv: {exact} of {} exactly ({free} more but for a color that shows nothing)",
        reference.rows.len()
    );

    let edges = read("edges.tsv");
    let held = edges_held(&edges);
    println!("edges.tsv: {held} of {}", edges.rows.len());

    let restarts = read("restarts.tsv");
    let shown = restarts_held(&restarts);
    println!("restarts.tsv: {shown} of {}", restarts.rows.len());

    let widths = read("widths.tsv");
    let drawn = widths_held(&widths);
    println!("widths.tsv: {drawn} of {}", widths.rows.len());

    let acceleration = read("acceleration.tsv");
    let bent = acceleration_held(&acceleration);
    println!("acceleration.tsv: {bent} of {}", acceleration.rows.len() * 21);

    assert_eq!(exact + free, reference.rows.len());
    assert_eq!(held, edges.rows.len());
    assert_eq!(shown, restarts.rows.len());
    assert_eq!(drawn, widths.rows.len());
    assert_eq!(bent, acceleration.rows.len() * 21);
}
