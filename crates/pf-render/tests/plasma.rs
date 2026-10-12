//! Plasma, held to its behaviour specification: one test for each acceptance property, numbered
//! as the specification numbers them (`p01` to `p26`), then the goals of the design that no
//! property covers (`goal_…`).
//!
//! The rig, unless a test says otherwise: a buffer 101 columns by 101 rows, the palette red,
//! green, blue, an effect 10 seconds long with 50 ms frames, and every setting at its default
//! (colors from the palette, twist 1, line density 1, speed 10) except those named. *Frame n*
//! is the moment `n` frame times after the effect's start. *G21* is the 441 positions with `u`
//! and `v` each one of 0, 0.05, … 1. *Brightness* is the red channel of the white coloring, and
//! the *crossings* along a line are the times it passes one half from one sample to the next.
//!
//! Samples that must be the same for the very same inputs are compared bit for bit. Where
//! different settings or buffers should show the same thing, a difference of [`SAME`] passes.

use pf_render::{Canvas, Colors, EffectTime, Pixel, Plasma, Rgba, Shade, Shader};
use pf_sequence::{EffectParams, PlasmaColors, PlasmaParams, Rgb};
use std::f64::consts::{FRAC_PI_2, TAU};

const RIG: Canvas = Canvas {
    columns: 101,
    rows: 101,
};
const PALETTE: [Rgb; 3] = [Rgb::RED, Rgb::GREEN, Rgb::BLUE];
const BLACK_WHITE: [Rgb; 2] = [Rgb::BLACK, Rgb::WHITE];

const COLORINGS: [PlasmaColors; 5] = [
    PlasmaColors::Palette,
    PlasmaColors::White,
    PlasmaColors::RedGreen,
    PlasmaColors::BlueGreen,
    PlasmaColors::Rainbow,
];
const FIXED: [PlasmaColors; 4] = [
    PlasmaColors::White,
    PlasmaColors::RedGreen,
    PlasmaColors::BlueGreen,
    PlasmaColors::Rainbow,
];

/// What samples that should show the same thing by different settings or buffers may differ by.
const SAME: f32 = 0.001;

/// One frame of a Plasma: what it is set to, on what, and when.
#[derive(Debug, Clone)]
struct Shot {
    p: PlasmaParams,
    palette: Vec<Rgb>,
    canvas: Canvas,
    frame_ms: u32,
    elapsed_ms: u64,
    length_ms: u64,
    start_ms: u64,
    seed: u64,
}

/// The rig's frame 0.
fn shot() -> Shot {
    Shot {
        p: PlasmaParams::default(),
        palette: PALETTE.to_vec(),
        canvas: RIG,
        frame_ms: 50,
        elapsed_ms: 0,
        length_ms: 10_000,
        start_ms: 0,
        seed: 0,
    }
}

impl Shot {
    fn colors(mut self, colors: PlasmaColors) -> Self {
        self.p.colors = colors;
        self
    }

    fn twist(mut self, twist: u32) -> Self {
        self.p.twist = twist;
        self
    }

    fn density(mut self, density: u32) -> Self {
        self.p.density = density;
        self
    }

    fn speed(mut self, speed: f32) -> Self {
        self.p.speed = speed;
        self
    }

    fn palette(mut self, palette: &[Rgb]) -> Self {
        self.palette = palette.to_vec();
        self
    }

    fn on(mut self, columns: u32, rows: u32) -> Self {
        self.canvas = Canvas { columns, rows };
        self
    }

    fn frame_ms(mut self, frame_ms: u32) -> Self {
        self.frame_ms = frame_ms;
        self
    }

    /// At frame `n`, counted in the frame time already set.
    fn frame(mut self, n: u64) -> Self {
        self.elapsed_ms = n * u64::from(self.frame_ms);
        self
    }

    fn elapsed_ms(mut self, elapsed_ms: u64) -> Self {
        self.elapsed_ms = elapsed_ms;
        self
    }

    fn time(&self) -> EffectTime {
        EffectTime::within(
            self.start_ms,
            self.start_ms + self.length_ms,
            self.start_ms + self.elapsed_ms,
        )
        .with_frame_ms(self.frame_ms)
    }

    /// The effect as the renderer builds it, with its settings pulled into their ranges.
    fn shader(&self) -> Shader {
        Shader::new(
            &EffectParams::Plasma(self.p),
            &self.time(),
            Colors::new(&self.palette),
            self.seed,
            self.canvas,
        )
    }

    /// The effect built directly, with its settings as they are.
    fn direct(&self) -> Plasma {
        Plasma::new(&self.p, &self.time(), Colors::new(&self.palette), self.canvas)
    }

    fn at(&self, u: f32, v: f32) -> Rgba {
        self.shader().shade(&pixel(u, v))
    }

    fn samples(&self, points: &[(f32, f32)]) -> Vec<Rgba> {
        let shader = self.shader();
        points.iter().map(|&(u, v)| shader.shade(&pixel(u, v))).collect()
    }

    fn g21(&self) -> Vec<Rgba> {
        self.samples(&g21())
    }

    /// The red channel at each point, in the white coloring.
    fn brightness(&self, points: &[(f32, f32)]) -> Vec<f64> {
        let white = self.clone().colors(PlasmaColors::White);
        white.samples(points).iter().map(|c| f64::from(c.r)).collect()
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

/// Every `(i / (across - 1), k / (up - 1))`, row by row from the bottom.
fn grid(across: u32, up: u32) -> Vec<(f32, f32)> {
    let at = |i: u32, n: u32| i as f32 / (n - 1) as f32;
    (0..up)
        .flat_map(|k| (0..across).map(move |i| (at(i, across), at(k, up))))
        .collect()
}

fn g21() -> Vec<(f32, f32)> {
    grid(21, 21)
}

/// The 1,001 positions along the middle row of a buffer, or up its middle column.
fn middle_row() -> Vec<(f32, f32)> {
    (0..=1000).map(|i| (i as f32 / 1000.0, 0.5)).collect()
}

fn middle_column() -> Vec<(f32, f32)> {
    (0..=1000).map(|i| (0.5, i as f32 / 1000.0)).collect()
}

fn channels(c: &Rgba) -> [f32; 4] {
    [c.r, c.g, c.b, c.a]
}

fn bits(samples: &[Rgba]) -> Vec<[u32; 4]> {
    samples.iter().map(|c| channels(c).map(f32::to_bits)).collect()
}

/// The largest difference in any channel, or alpha, between two sets of samples.
fn far(a: &[Rgba], b: &[Rgba]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .flat_map(|(x, y)| channels(x).into_iter().zip(channels(y)))
        .map(|(p, q)| (p - q).abs())
        .fold(0.0, f32::max)
}

/// Finite, from 0 to 1, and opaque.
fn sound(c: &Rgba) -> bool {
    c.a == 1.0
        && [c.r, c.g, c.b]
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}

fn crossings(levels: &[f64]) -> usize {
    levels
        .windows(2)
        .filter(|pair| (pair[0] - 0.5) * (pair[1] - 0.5) < 0.0)
        .count()
}

fn mean(values: impl IntoIterator<Item = f64>) -> f64 {
    let (sum, count) = values
        .into_iter()
        .fold((0.0, 0usize), |(sum, count), v| (sum + v, count + 1));
    sum / count as f64
}

/// The raised sine: a sine lifted to run from 0 to 1.
fn rs(radians: f64) -> f64 {
    (1.0 + radians.sin()) / 2.0
}

fn unit_colors(palette: &[Rgb]) -> Vec<[f64; 3]> {
    palette
        .iter()
        .map(|c| [c.r, c.g, c.b].map(|byte| f64::from(byte) / 255.0))
        .collect()
}

/// The palette's color at `position` (0 the first color, 1 the last): its colors at equal steps,
/// with a straight-line blend between neighbors.
fn blend(palette: &[[f64; 3]], position: f64) -> [f64; 3] {
    if palette.len() == 1 {
        return palette[0];
    }
    let place = position * (palette.len() - 1) as f64;
    let below = (place as usize).min(palette.len() - 2);
    let part = place - below as f64;
    [0, 1, 2].map(|k| palette[below][k] + (palette[below + 1][k] - palette[below][k]) * part)
}

/// The settings properties 13 and 14 are tried at: twist, line density, speed, frame.
const SIX: [(u32, u32, f32, u64); 6] = [
    (1, 1, 10.0, 0),
    (1, 1, 10.0, 777),
    (3, 2, 44.0, 300),
    (10, 10, 90.0, 55),
    (2, 6, 16.0, 400),
    (7, 3, 100.0, 12),
];

/// Property 1: Opaque, and in range. Every sample, in every coloring, with any palette (including
/// one black color and none), any settings and any frame: alpha exactly 1, and red, green and
/// blue finite numbers from 0 to 1.
#[test]
fn p01_opaque_and_in_range() {
    let settings = [
        (1, 1, 10.0, 0),
        (10, 10, 100.0, 7),
        (4, 3, 50.0, 400),
        (1, 10, 0.0, 6000),
    ];
    let palettes: [&[Rgb]; 3] = [&PALETTE, &[Rgb::BLACK], &[]];
    let mut samples = 0;
    for colors in COLORINGS {
        for (twist, density, speed, frame) in settings {
            for palette in palettes {
                let shot = shot()
                    .colors(colors)
                    .twist(twist)
                    .density(density)
                    .speed(speed)
                    .frame(frame)
                    .palette(palette);
                for sample in shot.g21() {
                    assert!(sound(&sample), "{shot:?}: {sample:?}");
                    samples += 1;
                }
            }
        }
    }
    assert_eq!(samples, 26_460);
}

/// Property 2: A frame depends on nothing else. For fixed settings, palette, buffer size, position
/// and frames elapsed, every sample is the same bit for bit: whether the effect is 1 second or
/// 60 seconds long; wherever it starts in the sequence; whatever random seed the renderer
/// supplies; asked twice; asked in the opposite order; asked from several threads at once.
#[test]
fn p02_a_frame_depends_on_nothing_else() {
    let base = shot().twist(3).density(2).speed(44.0).elapsed_ms(3000);
    let reference = bits(&base.g21());
    let others = [
        Shot {
            length_ms: 1000,
            ..base.clone()
        },
        Shot {
            length_ms: 60_000,
            ..base.clone()
        },
        Shot {
            start_ms: 12_345,
            ..base.clone()
        },
        Shot {
            seed: 987_654_321,
            ..base.clone()
        },
        Shot {
            start_ms: 7,
            length_ms: 3001,
            ..base.clone()
        },
    ];
    for other in others {
        assert_eq!(reference, bits(&other.g21()), "{other:?}");
    }
    assert_eq!(reference, bits(&base.g21()), "asked twice");

    let backwards: Vec<(f32, f32)> = g21().into_iter().rev().collect();
    let mut asked_backwards = bits(&base.samples(&backwards));
    asked_backwards.reverse();
    assert_eq!(reference, asked_backwards);

    // From 8 and from 12 threads at once, twenty times over each, sharing one effect.
    fn shareable<T: Send + Sync>(_: &T) {}
    let plasma = base.direct();
    shareable(&plasma);
    let points = g21();
    let draw = || -> bool {
        (0..20).all(|_| {
            let frame: Vec<Rgba> = points.iter().map(|&(u, v)| plasma.shade(&pixel(u, v))).collect();
            bits(&frame) == reference
        })
    };
    for threads in [8, 12] {
        let agreed = std::thread::scope(|scope| {
            let running: Vec<_> = (0..threads).map(|_| scope.spawn(draw)).collect();
            running.into_iter().all(|thread| thread.join().unwrap_or(false))
        });
        assert!(agreed, "{threads} threads");
    }
}

/// Property 3: The picture holds for a frame, and counts frames. G21 at 1,000 ms is the same at
/// 1,001, 1,025 and 1,049 ms, and different at 1,050 ms. G21 at 1,000 ms with 50 ms frames is the
/// same as at 500 ms with 25 ms frames, 400 ms with 20 ms, 660 ms with 33 ms, 2,000 ms with
/// 100 ms, and 20 ms with 1 ms.
#[test]
fn p03_the_picture_holds_for_a_frame_and_counts_frames() {
    let at = |elapsed_ms: u64, frame_ms: u32| shot().frame_ms(frame_ms).elapsed_ms(elapsed_ms).g21();
    let reference = at(1000, 50);
    for within in [1001, 1025, 1049] {
        assert_eq!(bits(&reference), bits(&at(within, 50)), "{within} ms");
    }
    let next = far(&reference, &at(1050, 50));
    assert!(next > 0.005, "the next frame differs by {next}");

    for (elapsed_ms, frame_ms) in [(500, 25), (400, 20), (660, 33), (2000, 100), (20, 1)] {
        assert_eq!(
            bits(&reference),
            bits(&at(elapsed_ms, frame_ms)),
            "{elapsed_ms} ms with {frame_ms} ms frames"
        );
    }
}

/// Property 4: Speed counts by its whole part. At frame 100, G21 at speed 10.25, 10.5 and 10.999
/// is the same as at speed 10; at speed 11 and at 9.999 some sample differs by more than 0.005.
#[test]
fn p04_speed_counts_by_its_whole_part() {
    let at = |speed: f32| shot().speed(speed).frame(100).g21();
    let ten = at(10.0);
    for speed in [10.25, 10.5, 10.999] {
        assert_eq!(bits(&ten), bits(&at(speed)), "speed {speed}");
    }
    for speed in [11.0, 9.999] {
        let off = far(&ten, &at(speed));
        assert!(off > 0.005, "speed {speed} differs by {off}");
    }
}

/// Property 5: The clock law. With twist 3, line density 2, rainbow: G21 is the same at (speed,
/// frame) = (100, 0), (99, 1), (67, 33), (50, 50), (10, 90), (1, 99) and (0, 100); and the same at
/// (100, 2), (98, 8), (50, 152) and (0, 302). Speed 50 at frame 51 differs from speed 100 at
/// frame 0 by more than 0.005 somewhere.
#[test]
fn p05_the_clock_law() {
    let at = |speed: f32, frame: u64| {
        shot()
            .colors(PlasmaColors::Rainbow)
            .twist(3)
            .density(2)
            .speed(speed)
            .frame(frame)
            .g21()
    };
    let a_third: [(f32, u64); 7] = [
        (100.0, 0),
        (99.0, 1),
        (67.0, 33),
        (50.0, 50),
        (10.0, 90),
        (1.0, 99),
        (0.0, 100),
    ];
    let one: [(f32, u64); 4] = [(100.0, 2), (98.0, 8), (50.0, 152), (0.0, 302)];
    for group in [&a_third[..], &one[..]] {
        let first = at(group[0].0, group[0].1);
        for &(speed, frame) in &group[1..] {
            let off = far(&first, &at(speed, frame));
            assert!(off <= SAME, "speed {speed} at frame {frame} differs by {off}");
        }
    }
    let off = far(&at(100.0, 0), &at(50.0, 51));
    assert!(off > 0.005, "one frame off differs by {off}");
}

/// Property 6: Speed 0 still moves. At speed 0, between frame 0 and frame 2,000 some G21 sample's
/// brightness changes by more than 0.5 and the mean change is more than 0.1.
#[test]
fn p06_speed_0_still_moves() {
    let at = |frame: u64| shot().speed(0.0).frame(frame).brightness(&g21());
    let (start, later) = (at(0), at(2000));
    let changes: Vec<f64> = start.iter().zip(&later).map(|(a, b)| (a - b).abs()).collect();
    let most = changes.iter().copied().fold(0.0, f64::max);
    let mean = mean(changes);
    assert!(most > 0.5 && mean > 0.1, "largest change {most}, mean {mean}");
}

/// The mean change in brightness from one frame to the next, over G21 and frames 0, 60, … 2,940.
fn pace(speed: f32, density: u32) -> f64 {
    let points = g21();
    let at = |frame: u64| {
        shot()
            .speed(speed)
            .density(density)
            .frame(frame)
            .brightness(&points)
    };
    mean((0..3000).step_by(60).flat_map(|frame| {
        let (now, next) = (at(frame), at(frame + 1));
        now.into_iter().zip(next).map(|(a, b)| (a - b).abs())
    }))
}

/// Property 7: How fast each speed is. The mean absolute change in brightness between one frame
/// and the next is 0.0031 at speed 0, 0.0034 at 10, 0.0068 at 50, 0.0317 at 90 and 0.2823 at 100,
/// each within 15%. At speed 100 and line density 10 it is between 0.35 and 0.46.
#[test]
fn p07_how_fast_each_speed_is() {
    for (speed, want) in [
        (0.0, 0.0031),
        (10.0, 0.0034),
        (50.0, 0.0068),
        (90.0, 0.0317),
        (100.0, 0.2823),
    ] {
        let got = pace(speed, 1);
        assert!(
            (got - want).abs() <= 0.15 * want,
            "speed {speed}: {got}, not {want}"
        );
    }
    let dense = pace(100.0, 10);
    assert!((0.35..=0.46).contains(&dense), "speed 100 at density 10: {dense}");
}

/// Property 8: One row, one column, one pixel. With rainbow, twist 4, line density 3, at frame 40,
/// on buffers 41 by 17, 101 by 101 and 7 by 300: the samples along `v = 0` are the same as those
/// at the same `u` on a buffer of the same width and one row (at any `v`); the samples along
/// `u = 0` are the same as those at the same `v` on a buffer one column wide (at any `u`); and
/// the sample at (0, 0) is the same as any sample of a 1 by 1 buffer.
#[test]
fn p08_one_row_one_column_one_pixel() {
    let on = |columns: u32, rows: u32| {
        shot()
            .colors(PlasmaColors::Rainbow)
            .twist(4)
            .density(3)
            .frame(40)
            .on(columns, rows)
    };
    for (columns, rows) in [(41, 17), (101, 101), (7, 300)] {
        let whole = on(columns, rows);
        let along = |i: u32, n: u32| i as f32 / (n - 1) as f32;

        let bottom: Vec<(f32, f32)> = (0..columns).map(|i| (along(i, columns), 0.0)).collect();
        for v in [0.5, 0.0, 0.9] {
            let line: Vec<(f32, f32)> = bottom.iter().map(|&(u, _)| (u, v)).collect();
            let off = far(&whole.samples(&bottom), &on(columns, 1).samples(&line));
            assert!(
                off <= SAME,
                "{columns} by {rows}: the bottom row differs by {off}"
            );
        }

        let left: Vec<(f32, f32)> = (0..rows).map(|k| (0.0, along(k, rows))).collect();
        for u in [0.5, 0.0, 0.9] {
            let line: Vec<(f32, f32)> = left.iter().map(|&(_, v)| (u, v)).collect();
            let off = far(&whole.samples(&left), &on(1, rows).samples(&line));
            assert!(
                off <= SAME,
                "{columns} by {rows}: the left column differs by {off}"
            );
        }

        for (u, v) in [(0.5, 0.5), (0.0, 1.0), (0.3, 0.8)] {
            let off = far(&[whole.at(0.0, 0.0)], &[on(1, 1).at(u, v)]);
            assert!(off <= SAME, "{columns} by {rows}: the corner differs by {off}");
        }
    }
}

/// Property 9: Laid over the buffer whatever its size. With rainbow, twist 2, line density 5, at
/// frame 80: the nine samples with `u` and `v` each 0, ½ or 1 are the same on buffers 3 by 3,
/// 101 by 101, 201 by 51, 5 by 9, 11 by 1,001 and 1,001 by 1,001. The sixteen with `u` and `v`
/// each 0, ⅓, ⅔ or 1 are the same on 4 by 4, 7 by 7, 100 by 100 and 31 by 61.
#[test]
fn p09_laid_over_the_buffer_whatever_its_size() {
    let on = |columns: u32, rows: u32, points: &[(f32, f32)]| {
        shot()
            .colors(PlasmaColors::Rainbow)
            .twist(2)
            .density(5)
            .frame(80)
            .on(columns, rows)
            .samples(points)
    };
    let halves = grid(3, 3);
    let reference = on(3, 3, &halves);
    for (columns, rows) in [(101, 101), (201, 51), (5, 9), (11, 1001), (1001, 1001)] {
        let off = far(&reference, &on(columns, rows, &halves));
        assert!(off <= SAME, "{columns} by {rows} differs by {off}");
    }
    let thirds = grid(4, 4);
    let reference = on(4, 4, &thirds);
    for (columns, rows) in [(7, 7), (100, 100), (31, 61)] {
        let off = far(&reference, &on(columns, rows, &thirds));
        assert!(off <= SAME, "{columns} by {rows} differs by {off}");
    }
}

/// Property 10: Nearest cell; exact halves go up. With rainbow, line density 4, at frame 20. On a
/// buffer 11 by 11: a sample at `u = i / 10 + e` for `e` = -0.049, -0.02, 0.02, 0.049 (kept
/// within 0 to 1) is the same as the sample at `u = i / 10`; likewise in `v`. On a buffer 5 by 5:
/// the samples at `u` = 0.125, 0.375, 0.625 and 0.875 (with `v` = 0) are the same as those at
/// `u` = 0.25, 0.5, 0.75 and 1; likewise in `v`.
#[test]
fn p10_nearest_cell_and_exact_halves_go_up() {
    let on = |cells: u32| {
        shot()
            .colors(PlasmaColors::Rainbow)
            .density(4)
            .frame(20)
            .on(cells, cells)
    };
    let eleven = on(11);
    let cells = bits(&eleven.samples(&grid(11, 11)));
    for e in [-0.049f32, -0.02, 0.02, 0.049] {
        let nudged = |at: f32| (at + e).clamp(0.0, 1.0);
        let across: Vec<(f32, f32)> = grid(11, 11).into_iter().map(|(u, v)| (nudged(u), v)).collect();
        assert_eq!(cells, bits(&eleven.samples(&across)), "u moved by {e}");
        let up: Vec<(f32, f32)> = grid(11, 11).into_iter().map(|(u, v)| (u, nudged(v))).collect();
        assert_eq!(cells, bits(&eleven.samples(&up)), "v moved by {e}");
    }

    let five = on(5);
    for (half_way, higher) in [(0.125, 0.25), (0.375, 0.5), (0.625, 0.75), (0.875, 1.0)] {
        assert_eq!(
            bits(&[five.at(half_way, 0.0)]),
            bits(&[five.at(higher, 0.0)]),
            "u = {half_way}"
        );
        assert_eq!(
            bits(&[five.at(0.0, half_way)]),
            bits(&[five.at(0.0, higher)]),
            "v = {half_way}"
        );
    }
}

/// Property 11: Outside, and unusable, positions. Same settings, buffer 5 by 5. A `u` of -0.3 or
/// -10⁹ gives the sample at `u = 0`; 1.2 or 10⁹ gives the sample at `u = 1`; not-a-number,
/// +infinity and -infinity each give the sample at `u = 0`. Likewise `v`. Nothing fails.
#[test]
fn p11_outside_and_unusable_positions() {
    let five = shot().colors(PlasmaColors::Rainbow).density(4).frame(20).on(5, 5);
    for (given, lands) in [
        (-0.3, 0.0),
        (-1e9, 0.0),
        (1.2, 1.0),
        (1e9, 1.0),
        (f32::NAN, 0.0),
        (f32::INFINITY, 0.0),
        (f32::NEG_INFINITY, 0.0),
    ] {
        for other in [0.0, 0.5, 1.0] {
            assert_eq!(
                bits(&[five.at(given, other)]),
                bits(&[five.at(lands, other)]),
                "u = {given}"
            );
            assert_eq!(
                bits(&[five.at(other, given)]),
                bits(&[five.at(other, lands)]),
                "v = {given}"
            );
        }
    }
}

/// Property 12: A coarser grid above 1,048,576 cells. With rainbow, line density 10, twist 7, at
/// frame 20, sampling 40,001 evenly spaced positions along `v = 0.37` (and along `u = 0.37`):
/// the number of distinct runs of equal color is 1,024 (1,024) on a buffer 1,024 by 1,024, 1,024
/// (1,023) on 1,025 by 1,024, 1,024 (1,024) on 2,048 by 2,048, 1,023 (1,023) on 1,039 by 1,039,
/// 1,254 (836) on 1,500 by 1,000, 3,739 (280) on 4,000 by 300, and 512 (2,048) on 1,303 by
/// 5,212. And at line density 3 the samples at `(i / 1023, j / 1023)` are the same on 2,048 by
/// 2,048 as on 1,024 by 1,024.
#[test]
fn p12_a_coarser_grid_above_1048576_cells() {
    let runs = |columns: u32, rows: u32, across: bool| {
        let line: Vec<(f32, f32)> = (0..=40_000)
            .map(|i| i as f32 / 40_000.0)
            .map(|at| if across { (at, 0.37) } else { (0.37, at) })
            .collect();
        let samples = shot()
            .colors(PlasmaColors::Rainbow)
            .density(10)
            .twist(7)
            .frame(20)
            .on(columns, rows)
            .samples(&line);
        1 + bits(&samples)
            .windows(2)
            .filter(|pair| pair[0] != pair[1])
            .count()
    };
    for ((columns, rows), grid) in [
        ((1024, 1024), (1024, 1024)),
        ((1025, 1024), (1024, 1023)),
        ((2048, 2048), (1024, 1024)),
        ((1039, 1039), (1023, 1023)),
        ((1500, 1000), (1254, 836)),
        ((4000, 300), (3739, 280)),
        ((1303, 5212), (512, 2048)),
    ] {
        let seen = (runs(columns, rows, true), runs(columns, rows, false));
        assert_eq!(seen, grid, "{columns} by {rows}");
    }

    let points: Vec<(f32, f32)> = [0, 1, 500, 1023]
        .into_iter()
        .flat_map(|j| {
            (0..1024)
                .step_by(31)
                .map(move |i| (i as f32 / 1023.0, j as f32 / 1023.0))
        })
        .collect();
    let on = |cells: u32| {
        shot()
            .colors(PlasmaColors::Rainbow)
            .density(3)
            .frame(20)
            .on(cells, cells)
            .samples(&points)
    };
    assert_eq!(bits(&on(2048)), bits(&on(1024)));
}

/// Property 13: The colorings are one picture in different colors. For the same settings, frame
/// and position, with `W` the brightness, `(r, g)` the red and green of red-and-green, and
/// `s = 2r - 1`, `c = 2g - 1`: white has three equal channels; red-and-green has `r = W`, blue
/// 0, and `s² + c² = 1`; blue-and-green has red 1/255, green `g`, blue `W`; rainbow has red `W`,
/// green `½ - s/4 + (√3/4) c`, blue `½ - s/4 - (√3/4) c`; and the palette black, white has all
/// three channels equal to rainbow's green. Each within 0.0001, over G21, at six settings.
#[test]
fn p13_the_colorings_are_one_picture_in_different_colors() {
    let root = 3f64.sqrt() / 4.0;
    for (twist, density, speed, frame) in SIX {
        let with = |colors: PlasmaColors| {
            shot()
                .colors(colors)
                .twist(twist)
                .density(density)
                .speed(speed)
                .frame(frame)
                .palette(&BLACK_WHITE)
                .g21()
        };
        let white = with(PlasmaColors::White);
        let red_green = with(PlasmaColors::RedGreen);
        let blue_green = with(PlasmaColors::BlueGreen);
        let rainbow = with(PlasmaColors::Rainbow);
        let palette = with(PlasmaColors::Palette);
        for k in 0..white.len() {
            let [w, rg, bg, rb, pal] = [white[k], red_green[k], blue_green[k], rainbow[k], palette[k]]
                .map(|c| [c.r, c.g, c.b].map(f64::from));
            let (s, c) = (2.0 * rg[0] - 1.0, 2.0 * rg[1] - 1.0);
            let departures = [
                w[0] - w[1],
                w[0] - w[2],
                rg[0] - w[0],
                rg[2],
                s * s + c * c - 1.0,
                bg[2] - w[0],
                bg[1] - rg[1],
                bg[0] - 1.0 / 255.0,
                rb[0] - w[0],
                rb[1] - (0.5 - s / 4.0 + root * c),
                rb[2] - (0.5 - s / 4.0 - root * c),
                pal[0] - rb[1],
                pal[1] - rb[1],
                pal[2] - rb[1],
            ];
            for (which, departure) in departures.into_iter().enumerate() {
                assert!(
                    departure.abs() <= 1e-4,
                    "twist {twist}, density {density}, speed {speed}, frame {frame}, sample {k}: \
                     relation {which} is off by {departure}"
                );
            }
        }
    }
}

/// Property 14: Density multiplies the color's angle. With red-and-green, for each of the six
/// settings of property 13 taken at line density 1: let `a` be the angle whose sine is `2r - 1`
/// and whose cosine is `2g - 1`. Then the same settings at line density `k` (2, 3, 5, 10) give
/// red `rs(k a)` and green `rs(k a + 90°)`, within 0.0001.
#[test]
fn p14_density_multiplies_the_colors_angle() {
    for (twist, _, speed, frame) in SIX {
        let at = |density: u32| {
            shot()
                .colors(PlasmaColors::RedGreen)
                .twist(twist)
                .density(density)
                .speed(speed)
                .frame(frame)
                .g21()
        };
        let first = at(1);
        for density in [2, 3, 5, 10] {
            for (one, many) in first.iter().zip(at(density)) {
                let angle = (2.0 * f64::from(one.r) - 1.0).atan2(2.0 * f64::from(one.g) - 1.0);
                let turned = f64::from(density) * angle;
                let off = (f64::from(many.r) - rs(turned))
                    .abs()
                    .max((f64::from(many.g) - rs(turned + FRAC_PI_2)).abs());
                assert!(
                    off <= 1e-4,
                    "twist {twist}, speed {speed}, frame {frame}, density {density}: off by {off}"
                );
            }
        }
    }
}

/// Property 15: The palette is a straight-line blend through evenly spaced colors. With twist 2,
/// line density 3, speed 44, at frame 300, over G21: let `p` be the sample with the palette
/// black, white. For palettes of 2, 3, 4, 8 and 32 colors, the sample is the blend of the two
/// colors either side of position `p × (k - 1)`, within 0.0001.
#[test]
fn p15_the_palette_is_a_straight_line_blend_through_evenly_spaced_colors() {
    let with = |palette: &[Rgb]| {
        shot()
            .twist(2)
            .density(3)
            .speed(44.0)
            .frame(300)
            .palette(palette)
            .g21()
    };
    let hex = |colors: &[&str]| -> Vec<Rgb> { colors.iter().filter_map(|c| Rgb::from_hex(c)).collect() };
    let palettes = [
        hex(&["ff0000", "0000ff"]),
        PALETTE.to_vec(),
        hex(&["ffffff", "800000", "008000", "0000ff"]),
        hex(&[
            "000000", "ff0000", "ffff00", "00ff00", "00ffff", "0000ff", "ff00ff", "ffffff",
        ]),
        (0..32u32)
            .map(|i| Rgb::new((i * 37 % 256) as u8, (i * 91 % 256) as u8, (i * 53 % 256) as u8))
            .collect(),
    ];
    assert_eq!(palettes.each_ref().map(Vec::len), [2, 3, 4, 8, 32]);

    let positions = with(&BLACK_WHITE);
    for palette in &palettes {
        let colors = unit_colors(palette);
        for (position, sample) in positions.iter().zip(with(palette)) {
            let want = blend(&colors, f64::from(position.r));
            for (got, want) in [sample.r, sample.g, sample.b].into_iter().zip(want) {
                let off = (f64::from(got) - want).abs();
                assert!(off <= 1e-4, "{} colors: off by {off}", palette.len());
            }
        }
    }
}

/// Property 16: One color is a flat fill; none is white. With the single color (51, 102, 204) and
/// line density 7, every G21 sample at frames 0, 100 and 5,000 is exactly (0.2, 0.4, 0.8),
/// alpha 1. With an empty palette every sample is exactly (1, 1, 1), alpha 1.
#[test]
fn p16_one_color_is_a_flat_fill_and_none_is_white() {
    for frame in [0, 100, 5000] {
        let one = shot().palette(&[Rgb::new(51, 102, 204)]).density(7).frame(frame);
        for sample in one.g21() {
            assert_eq!(channels(&sample), [0.2, 0.4, 0.8, 1.0], "frame {frame}");
        }
        for sample in shot().palette(&[]).frame(frame).g21() {
            assert_eq!(channels(&sample), [1.0, 1.0, 1.0, 1.0], "frame {frame}");
        }
    }
}

/// Property 17: The fixed colorings ignore the palette. White, red-and-green, blue-and-green and
/// rainbow give the same samples with the palette red, green, blue as with none.
#[test]
fn p17_the_fixed_colorings_ignore_the_palette() {
    for colors in FIXED {
        let with = |palette: &[Rgb]| {
            shot()
                .colors(colors)
                .twist(2)
                .density(3)
                .speed(44.0)
                .frame(300)
                .palette(palette)
                .g21()
        };
        assert_eq!(bits(&with(&PALETTE)), bits(&with(&[])), "{colors:?}");
    }
}

/// Property 18: As much light as dark. Over the 676 positions with `u` and `v` each one of 0,
/// 0.04, … 1, and frames 0, 80, 160, … 7,920, at line density 1 and at 10: every channel that
/// varies has a mean within 0.04 of 0.5, a standard deviation within 0.01 of 0.354, a least
/// value under 0.01 and a greatest over 0.99. And at frame 0 alone the standard deviation of
/// brightness over G21 is above 0.3: the first frame is a full picture.
#[test]
fn p18_as_much_light_as_dark() {
    let spread = |values: &[f64]| {
        let middle = mean(values.iter().copied());
        (middle, mean(values.iter().map(|v| (v - middle).powi(2))).sqrt())
    };
    let points = grid(26, 26);
    let varying: [(PlasmaColors, &[usize]); 4] = [
        (PlasmaColors::White, &[0]),
        (PlasmaColors::RedGreen, &[0, 1]),
        (PlasmaColors::BlueGreen, &[1, 2]),
        (PlasmaColors::Rainbow, &[0, 1, 2]),
    ];
    for (colors, which) in varying {
        for density in [1, 10] {
            let samples: Vec<Rgba> = (0..8000)
                .step_by(80)
                .flat_map(|frame| {
                    shot()
                        .colors(colors)
                        .density(density)
                        .frame(frame)
                        .samples(&points)
                })
                .collect();
            for &channel in which {
                let values: Vec<f64> = samples.iter().map(|c| f64::from(channels(c)[channel])).collect();
                let (middle, deviation) = spread(&values);
                let least = values.iter().copied().fold(f64::INFINITY, f64::min);
                let greatest = values.iter().copied().fold(0.0, f64::max);
                assert!(
                    (middle - 0.5).abs() <= 0.04
                        && (deviation - 0.354).abs() <= 0.01
                        && least < 0.01
                        && greatest > 0.99,
                    "{colors:?} at density {density}, channel {channel}: mean {middle}, deviation \
                     {deviation}, from {least} to {greatest}"
                );
            }
        }
    }
    let (_, deviation) = spread(&shot().brightness(&g21()));
    assert!(deviation > 0.3, "the first frame's deviation is {deviation}");
}

/// The crossings along the middle row and the middle column of a buffer 1,001 by 1,001, averaged
/// over the given frames.
fn bands(twist: u32, density: u32, speed: f32, frames: impl Iterator<Item = u64> + Clone) -> (f64, f64) {
    let along = |line: &[(f32, f32)]| {
        mean(frames.clone().map(|frame| {
            let shot = shot()
                .twist(twist)
                .density(density)
                .speed(speed)
                .on(1001, 1001)
                .frame(frame);
            crossings(&shot.brightness(line)) as f64
        }))
    };
    (along(&middle_row()), along(&middle_column()))
}

/// Frames 0, 200, … 5,800.
fn thirty_frames() -> impl Iterator<Item = u64> + Clone {
    (0..6000).step_by(200)
}

/// Whether a count of crossings is the one wanted, within 15% plus 0.3.
fn about(got: f64, want: f64) -> bool {
    (got - want).abs() <= 0.15 * want + 0.3
}

/// Property 19: Density sets how many bands there are. On a buffer 1,001 by 1,001, averaged over
/// frames 0, 200, … 5,800, the crossings along the middle row are 3.9, 7.0, 17.5 and 34.8 at line
/// density 1, 2, 5 and 10, and along the middle column 2.3, 4.9, 11.8 and 23.8, each within 15%
/// plus 0.3.
#[test]
fn p19_density_sets_how_many_bands_there_are() {
    for (density, row, column) in [(1, 3.9, 2.3), (2, 7.0, 4.9), (5, 17.5, 11.8), (10, 34.8, 23.8)] {
        let got = bands(1, density, 10.0, thirty_frames());
        assert!(
            about(got.0, row) && about(got.1, column),
            "density {density}: {got:?}, not ({row}, {column})"
        );
    }
}

/// Property 20: Twist adds rings. The same measurement at line density 1: along the middle row
/// 3.9, 4.8 and 5.6 at twist 1, 4 and 10; along the middle column 2.3, 3.3 and 4.9, each within
/// 15% plus 0.3; and the column count at twist 10 is more than 1.7 times that at twist 1.
#[test]
fn p20_twist_adds_rings() {
    let mut columns = Vec::new();
    for (twist, row, column) in [(1, 3.9, 2.3), (4, 4.8, 3.3), (10, 5.6, 4.9)] {
        let got = bands(twist, 1, 10.0, thirty_frames());
        assert!(
            about(got.0, row) && about(got.1, column),
            "twist {twist}: {got:?}, not ({row}, {column})"
        );
        columns.push(got.1);
    }
    assert!(columns[2] > 1.7 * columns[0], "{columns:?}");
}

/// Property 21: The twist rings spread out as the clock runs on. At speed 100, on a buffer 1,001
/// by 1,001, the crossings along the middle column averaged over frames 0, 2, … 58 and over
/// frames 6,000, 6,002, … 6,058: at twist 10, 4.6 (within 0.8) and then 2.3 (within 0.6); at
/// twist 1, 2.1 and then 2.3 (each within 0.6).
#[test]
fn p21_the_twist_rings_spread_out_as_the_clock_runs_on() {
    let column = |twist: u32, from: u64| bands(twist, 1, 100.0, (from..from + 60).step_by(2)).1;
    for (twist, from, want, within) in [
        (10, 0, 4.6, 0.8),
        (10, 6000, 2.3, 0.6),
        (1, 0, 2.1, 0.6),
        (1, 6000, 2.3, 0.6),
    ] {
        let got = column(twist, from);
        assert!(
            (got - want).abs() <= within,
            "twist {twist} from frame {from}: {got}, not {want}"
        );
    }
}

/// Property 22: Smooth: no hard edges. On a buffer 1,001 by 1,001, over frames 0, 200, … 5,800,
/// the largest step in brightness between neighboring samples of the middle row and of the
/// middle column is between 0.012 and 0.025 at the defaults, and no more than 0.36 at twist 10,
/// line density 10.
#[test]
fn p22_smooth_no_hard_edges() {
    let largest_step = |twist: u32, density: u32| {
        let mut largest = 0.0f64;
        for frame in thirty_frames() {
            let shot = shot().twist(twist).density(density).on(1001, 1001).frame(frame);
            for line in [middle_row(), middle_column()] {
                for pair in shot.brightness(&line).windows(2) {
                    largest = largest.max((pair[0] - pair[1]).abs());
                }
            }
        }
        largest
    };
    let plain = largest_step(1, 1);
    assert!((0.012..=0.025).contains(&plain), "at the defaults: {plain}");
    let busiest = largest_step(10, 10);
    assert!(busiest <= 0.36, "at twist 10, density 10: {busiest}");
}

/// Property 23: The bands drift to the left. On a buffer 1,001 by 1,001, for frames 0, 300, …
/// 5,700: take brightness along the bottom row at frame `f` and at frame `f + 20`, and find the
/// whole number of cells `k` from -60 to 60 for which the later row, cell for cell, is most like
/// the earlier row moved `k` cells to the left (least mean squared difference over the cells both
/// cover). The mean of `k` is between 5 and 12, and `k` is positive at 14 or more of the 20
/// moments.
#[test]
fn p23_the_bands_drift_to_the_left() {
    let bottom: Vec<(f32, f32)> = (0..=1000).map(|i| (i as f32 / 1000.0, 0.0)).collect();
    let row = |frame: u64| shot().on(1001, 1001).frame(frame).brightness(&bottom);
    let shifts: Vec<i64> = (0..6000)
        .step_by(300)
        .map(|frame| {
            let (earlier, later) = (row(frame), row(frame + 20));
            let misfit = |k: i64| {
                let covered = (0.max(-k)..1001.min(1001 - k)).map(|i| (i as usize, (i + k) as usize));
                mean(covered.map(|(i, moved)| (later[i] - earlier[moved]).powi(2)))
            };
            (-60..=60)
                .min_by(|&a, &b| misfit(a).total_cmp(&misfit(b)))
                .unwrap_or(0)
        })
        .collect();
    let average = mean(shifts.iter().map(|&k| k as f64));
    let leftward = shifts.iter().filter(|&&k| k > 0).count();
    assert!(
        (5.0..=12.0).contains(&average) && leftward >= 14,
        "moved left by {average} cells on average, leftward at {leftward} of {}: {shifts:?}",
        shifts.len()
    );
}

/// One row of the reference table: what today's picture returned for a setting, a buffer, a
/// moment and a position.
struct Reference {
    case: u32,
    shot: Shot,
    at: (f32, f32),
    color: [f32; 4],
}

/// A fixed sample of the specification's reference table: 480 of its 3,900 rows, from every one
/// of its 50 cases and 156 frames, with more of the cases where rounding decides the cell.
fn reference() -> Vec<Reference> {
    fn number<T: std::str::FromStr>(text: &str) -> T {
        match text.parse() {
            Ok(value) => value,
            Err(_) => panic!("not a number in the reference table: {text}"),
        }
    }
    include_str!("data/plasma_reference.tsv")
        .lines()
        .skip(1)
        .map(|line| {
            let field: Vec<&str> = line.split('\t').collect();
            assert_eq!(field.len(), 16, "{line}");
            let colors = match field[1] {
                "palette" => PlasmaColors::Palette,
                "redGreen" => PlasmaColors::RedGreen,
                "blueGreen" => PlasmaColors::BlueGreen,
                "rainbow" => PlasmaColors::Rainbow,
                "white" => PlasmaColors::White,
                other => panic!("no such coloring: {other}"),
            };
            let palette: Vec<Rgb> = match field[9] {
                "-" => Vec::new(),
                list => list.split(',').filter_map(Rgb::from_hex).collect(),
            };
            Reference {
                case: number(field[0]),
                shot: Shot {
                    p: PlasmaParams {
                        colors,
                        twist: number(field[2]),
                        density: number(field[3]),
                        speed: number(field[4]),
                    },
                    palette,
                    canvas: Canvas {
                        columns: number(field[5]),
                        rows: number(field[6]),
                    },
                    frame_ms: number(field[7]),
                    elapsed_ms: number(field[8]),
                    length_ms: 600_000,
                    start_ms: 0,
                    seed: 0,
                },
                at: (number(field[10]), number(field[11])),
                color: [12, 13, 14, 15].map(|k| number(field[k])),
            }
        })
        .collect()
}

/// The color a channel shows on an 8-bit pixel.
fn eight_bit(level: f32) -> i32 {
    (level.clamp(0.0, 1.0) * 255.0 + 0.5) as i32
}

/// Property 24: The reference table. Every row within 0.01 in each of red, green, blue and alpha.
/// (The table is today's picture, exact to the last digit; the picture it speckles about is the
/// same to 0.00015, and on an 8-bit pixel nearly every row is the very same color.)
#[test]
fn p24_the_reference_table() {
    let rows = reference();
    assert_eq!(rows.len(), 480);
    let cases: std::collections::BTreeSet<u32> = rows.iter().map(|row| row.case).collect();
    assert_eq!(cases.len(), 50);

    let (mut worst, mut identical) = (0.0f32, 0);
    for row in &rows {
        let got = channels(&row.shot.at(row.at.0, row.at.1));
        let off = got
            .into_iter()
            .zip(row.color)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(
            off <= 0.01,
            "case {}, {:?} at {:?}: {got:?}",
            row.case,
            row.shot,
            row.at
        );
        worst = worst.max(off);

        let steps = got
            .into_iter()
            .zip(row.color)
            .map(|(a, b)| (eight_bit(a) - eight_bit(b)).abs())
            .fold(0, i32::max);
        assert!(steps <= 1, "case {}: {got:?}, not {:?}", row.case, row.color);
        identical += usize::from(steps == 0);
    }
    // Far closer than the property asks: what is left is today's own speckle.
    assert!(worst <= 0.0005, "the furthest row is {worst} off");
    assert!(
        identical >= rows.len() - 2,
        "only {identical} of {} rows are the same 8-bit color",
        rows.len()
    );
}

/// Property 25: Tiny buffers and extreme settings. On buffers 1 by 1, 2 by 1, 1 by 2, 2 by 2, 3 by
/// 3 and 0 by 0, in every coloring, with twist 10, line density 10, speed 100, at 0 ms, 50 ms
/// and one hour, at positions (0, 0), (1, 1), (0.5, 0.5), (not-a-number, infinity) and (-1, 2):
/// no failure, every color finite and from 0 to 1, alpha 1. A buffer 0 by 0 gives the sample of
/// a buffer 1 by 1. A frame time of 0 gives the samples of a frame time of 1 ms.
#[test]
fn p25_tiny_buffers_and_extreme_settings() {
    let positions = [
        (0.0, 0.0),
        (1.0, 1.0),
        (0.5, 0.5),
        (f32::NAN, f32::INFINITY),
        (-1.0, 2.0),
    ];
    let mut samples = 0;
    for (columns, rows) in [(1, 1), (2, 1), (1, 2), (2, 2), (3, 3), (0, 0)] {
        for colors in COLORINGS {
            for elapsed_ms in [0, 50, 3_600_000] {
                let shot = Shot {
                    length_ms: 4_000_000,
                    ..shot()
                        .colors(colors)
                        .twist(10)
                        .density(10)
                        .speed(100.0)
                        .on(columns, rows)
                        .elapsed_ms(elapsed_ms)
                };
                for sample in shot.samples(&positions) {
                    assert!(sound(&sample), "{shot:?}: {sample:?}");
                    samples += 1;
                }
            }
        }
    }
    assert_eq!(samples, 450);

    let rainbow = || shot().colors(PlasmaColors::Rainbow);
    assert_eq!(
        bits(&[rainbow().frame(20).on(0, 0).at(0.3, 0.8)]),
        bits(&[rainbow().frame(20).on(1, 1).at(0.5, 0.5)])
    );
    assert_eq!(
        bits(&rainbow().frame_ms(0).elapsed_ms(20).g21()),
        bits(&rainbow().frame_ms(1).elapsed_ms(20).g21())
    );
}

/// Property 26: Settings outside their ranges (as the renderer hands them on, already pulled in).
/// At frame 40, rainbow, G21: speed -5 is the same as 0; 250 and infinity the same as 100;
/// not-a-number the same as 10; twist 0 the same as 1 and 50 the same as 10; line density 0 the
/// same as 1 and 50 the same as 10. And handed one directly, the effect takes a twist or a line
/// density outside 1 to 10 as the nearest of the two, a speed below 0 as 0, one above 100 as 100,
/// and one that is not a number as 0.
#[test]
fn p26_settings_outside_their_ranges() {
    let base = || shot().colors(PlasmaColors::Rainbow).frame(40);
    let through_the_renderer = |shot: Shot| bits(&shot.g21());
    let directly = |shot: Shot| {
        let plasma = shot.direct();
        let samples: Vec<Rgba> = g21().iter().map(|&(u, v)| plasma.shade(&pixel(u, v))).collect();
        bits(&samples)
    };

    let speeds = |not_a_number_is: f32| {
        [
            (-5.0, 0.0),
            (250.0, 100.0),
            (f32::INFINITY, 100.0),
            (f32::NEG_INFINITY, 0.0),
            (f32::NAN, not_a_number_is),
        ]
    };
    for (given, taken) in speeds(10.0) {
        assert_eq!(
            through_the_renderer(base().speed(given)),
            through_the_renderer(base().speed(taken)),
            "speed {given}"
        );
    }
    for (given, taken) in speeds(0.0) {
        assert_eq!(
            directly(base().speed(given)),
            directly(base().speed(taken)),
            "speed {given}, handed over directly"
        );
    }
    for (given, taken) in [(0, 1), (50, 10), (u32::MAX, 10)] {
        for build in [through_the_renderer, directly] {
            assert_eq!(
                build(base().twist(given)),
                build(base().twist(taken)),
                "twist {given}"
            );
            assert_eq!(
                build(base().density(given)),
                build(base().density(taken)),
                "density {given}"
            );
        }
    }
    // Directly or through the renderer, settings within their ranges give the same picture.
    let busy = base().twist(7).density(3).speed(44.5);
    assert_eq!(through_the_renderer(busy.clone()), directly(busy));
}

/// The specification's description of the picture, worked in double precision with the system's
/// own sines: the grid a buffer is drawn on, the cell a pixel belongs to, the seven waves at that
/// cell's place, and the coloring.
fn described(shot: &Shot, u: f32, v: f32) -> [f64; 3] {
    // The grid: the buffer's, or a coarser one above 1,048,576 cells.
    let (mut columns, mut rows) = (
        u64::from(shot.canvas.columns.max(1)),
        u64::from(shot.canvas.rows.max(1)),
    );
    if columns * rows > 1_048_576 {
        let shrink = (1_048_576.0 / (columns as f64 * rows as f64)).sqrt();
        columns = ((columns as f64 * shrink) as u64).max(1);
        rows = ((rows as f64 * shrink) as u64).max(1);
    }
    // The nearest cell, by a single-precision product, an exact half going up.
    let cell = |at: f32, cells: u64| -> f64 {
        if cells <= 1 || !at.is_finite() {
            return 0.0;
        }
        let product = at.clamp(0.0, 1.0) * (cells - 1) as f32;
        (f64::from(product) + 0.5).floor().min((cells - 1) as f64) / (cells - 1) as f64
    };
    let (x, y) = (cell(u, columns), cell(v, rows));

    // What the renderer hands on: settings pulled into their ranges.
    let speed = if shot.p.speed.is_nan() {
        10.0
    } else {
        shot.p.speed.clamp(0.0, 100.0)
    };
    let twist = f64::from(shot.p.twist.clamp(1, 10));
    let density = f64::from(shot.p.density.clamp(1, 10));

    let frames = shot.elapsed_ms / u64::from(shot.frame_ms.max(1));
    let clock = (frames as f64 + 1.0) / (303.0 - 3.0 * f64::from(speed).floor());
    let (g, h) = ((clock / 2.0).sin(), (clock / 3.0).cos());
    let (x0, y0) = (-0.5 * (clock / 5.0).sin(), -0.5 * (clock / 3.0).cos());
    let distance_squared = (x - x0).powi(2) + (y - y0).powi(2);
    let sum = (x + clock).sin()
        + (y / 2.0 + clock / 2.0).sin()
        + (x / 2.0 + y / 2.0 + clock / 2.0).sin()
        + (x.hypot(y) + clock).sin()
        + (10.0 * x + clock).sin()
        + (10.0 * (g * x + h * y) + clock).sin()
        + (distance_squared * 50.0 * twist + clock).sqrt().sin();
    let angle = FRAC_PI_2 * density * sum;

    let (quarter, third) = (FRAC_PI_2, TAU / 3.0);
    match shot.p.colors {
        PlasmaColors::White => [rs(angle); 3],
        PlasmaColors::RedGreen => [rs(angle), rs(angle + quarter), 0.0],
        PlasmaColors::BlueGreen => [1.0 / 255.0, rs(angle + quarter), rs(angle)],
        PlasmaColors::Rainbow => [rs(angle), rs(angle + third), rs(angle + 2.0 * third)],
        PlasmaColors::Palette if shot.palette.is_empty() => [1.0; 3],
        PlasmaColors::Palette => blend(&unit_colors(&shot.palette), rs(angle + third)),
    }
}

/// Numbers to draw settings from: the same ones every run.
struct Dice(u64);

impl Dice {
    fn roll(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A whole number from `least` to `most`.
    fn within(&mut self, least: u64, most: u64) -> u64 {
        least + self.roll() % (most - least + 1)
    }

    /// A fraction from 0 up to 1.
    fn fraction(&mut self) -> f64 {
        (self.roll() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn one_of<T: Copy>(&mut self, choices: &[T]) -> T {
        choices[self.within(0, choices.len() as u64 - 1) as usize]
    }
}

/// A Plasma drawn at random: any coloring, twist, line density and speed (whole or not), palettes
/// of none to 32 colors (the harshest, black and white by turns, among them), buffers from one
/// cell to 25 million, frame times of 1 to 100 ms, and moments from the first frame to four
/// hours at the top speed.
fn any_shot(dice: &mut Dice) -> Shot {
    const BUFFERS: [(u32, u32); 23] = [
        (1, 1),
        (2, 1),
        (1, 2),
        (2, 2),
        (3, 3),
        (5, 9),
        (11, 11),
        (23, 13),
        (41, 1),
        (1, 50),
        (101, 101),
        (101, 51),
        (206, 175),
        (400, 178),
        (400, 221),
        (139, 259),
        (1001, 1001),
        (1024, 1024),
        (1025, 1024),
        (1039, 1039),
        (1500, 1000),
        (4000, 300),
        (5000, 5000),
    ];
    let colors = dice.one_of(&[
        PlasmaColors::Palette,
        PlasmaColors::Palette,
        PlasmaColors::Palette,
        PlasmaColors::Palette,
        PlasmaColors::White,
        PlasmaColors::RedGreen,
        PlasmaColors::BlueGreen,
        PlasmaColors::Rainbow,
    ]);
    let speed = match dice.within(0, 3) {
        0 => dice.one_of(&[0.0, 10.0, 16.0, 44.0, 50.0, 90.0, 99.0, 100.0]),
        1 => dice.within(0, 100) as f32,
        _ => (dice.fraction() * 100.0) as f32,
    };
    let palette: Vec<Rgb> = match dice.one_of(&[0, 1, 2, 3, 3, 3, 4, 4, 5, 8, 32, 33]) {
        33 => (0..32)
            .map(|k| if k % 2 == 0 { Rgb::BLACK } else { Rgb::WHITE })
            .collect(),
        count => (0..count)
            .map(|_| Rgb::new(dice.roll() as u8, dice.roll() as u8, dice.roll() as u8))
            .collect(),
    };
    let (columns, rows) = dice.one_of(&BUFFERS);
    let frame_ms = dice.one_of(&[1, 10, 20, 25, 33, 50, 50, 50, 100]);
    let frame = match dice.within(0, 4) {
        0 => dice.within(0, 1),
        1 => dice.within(0, 400),
        2 | 3 => dice.within(0, 12_000),
        _ => dice.within(0, 1_440_000),
    };
    Shot {
        p: PlasmaParams {
            colors,
            twist: dice.within(1, 10) as u32,
            density: dice.within(1, 10) as u32,
            speed,
        },
        palette,
        canvas: Canvas { columns, rows },
        frame_ms,
        elapsed_ms: frame * u64::from(frame_ms) + dice.within(0, u64::from(frame_ms) - 1),
        length_ms: 14_400_000,
        start_ms: 0,
        seed: 0,
    }
}

/// A position drawn at random: mostly on a cell or anywhere on the buffer, sometimes outside
/// it, and now and then not a usable number at all.
fn any_position(dice: &mut Dice, canvas: Canvas) -> (f32, f32) {
    let on_a_cell = |dice: &mut Dice, cells: u32| {
        if cells > 1 {
            dice.within(0, u64::from(cells) - 1) as f32 / (cells - 1) as f32
        } else {
            0.5
        }
    };
    match dice.within(0, 19) {
        0..=10 => (on_a_cell(dice, canvas.columns), on_a_cell(dice, canvas.rows)),
        11..=16 => (dice.fraction() as f32, dice.fraction() as f32),
        17 | 18 => (
            (dice.fraction() * 2.0 - 0.5) as f32,
            (dice.fraction() * 2.0 - 0.5) as f32,
        ),
        _ => (
            dice.one_of(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1e9, -1e9]),
            dice.fraction() as f32,
        ),
    }
}

/// The picture is the specification's, with nothing of its own added: over a large sweep of
/// settings, palettes, buffers, moments and positions, every channel is the description's worked
/// in double precision, to within what the effect's own sine can be off by (a hundred-millionth
/// in each of seven waves and in the color, magnified by the line density and by the palette's
/// steps), and an 8-bit pixel shows the very same color but for a handful in a hundred thousand
/// that sit on the line between two.
///
/// That keeps it inside what the specification allows a side-by-side with today's picture, whose
/// own speckle is hundreds of times this at any clock and grows with it. This picture's does not
/// grow: four hours in at the top speed it is as close as on the first frame.
#[test]
fn goal_the_picture_is_the_description_worked_in_double_precision() {
    let mut dice = Dice(2026);
    let (mut samples, mut identical, mut late) = (0usize, 0usize, 0usize);
    let mut worst = 0.0f64;
    for _ in 0..5000 {
        let shot = any_shot(&mut dice);
        let shader = shot.shader();
        let steps = match shot.p.colors {
            PlasmaColors::Palette => shot.palette.len().saturating_sub(1).max(1),
            _ => 1,
        } as f64;
        let density = f64::from(shot.p.density);
        let own = 2e-7 + steps * (5.5e-8 * density + 1e-8);
        let frames = shot.elapsed_ms / u64::from(shot.frame_ms);
        let clock = (frames as f64 + 1.0) / (303.0 - 3.0 * f64::from(shot.p.speed).floor());
        // What the specification allows between a new picture and today's.
        let allowed = 0.0005 + density * steps * (0.000_004 + 0.000_000_2 * clock);
        assert!(own < allowed / 25.0);

        for _ in 0..40 {
            let (u, v) = any_position(&mut dice, shot.canvas);
            let got = shader.shade(&pixel(u, v));
            assert!(sound(&got), "{shot:?} at ({u}, {v}): {got:?}");
            let want = described(&shot, u, v);
            let mut steps_off = 0;
            for (got, want) in [got.r, got.g, got.b].into_iter().zip(want) {
                let off = (f64::from(got) - want).abs();
                assert!(
                    off <= own,
                    "{shot:?} at ({u}, {v}): {got}, not {want}: {off:e} off, over {own:e}"
                );
                worst = worst.max(off / own);
                steps_off = steps_off.max((eight_bit(got) - eight_bit(want as f32)).abs());
            }
            assert!(steps_off <= 1, "{shot:?} at ({u}, {v}): {got:?}, not {want:?}");
            samples += 1;
            identical += usize::from(steps_off == 0);
            late += usize::from(clock > 1000.0);
        }
    }
    assert_eq!(samples, 200_000);
    assert!(late > 10_000, "only {late} samples late in a long effect");
    assert!(
        identical * 10_000 >= samples * 9_999,
        "{} of {samples} samples are another 8-bit color",
        samples - identical
    );
    // The bound is the sine's own, not a loose one: somewhere the picture comes near it.
    assert!(worst > 0.25, "never more than {worst} of the way to the bound");
}

/// The specification's two worked examples, to the five places it gives them.
#[test]
fn goal_the_worked_examples() {
    struct Example {
        shot: Shot,
        at: (f32, f32),
        white: f64,
        red_green: [f64; 3],
        blue_green: [f64; 3],
        rainbow: [f64; 3],
        palette: [f64; 3],
        black_white: f64,
    }
    let examples = [
        Example {
            shot: shot(),
            at: (0.5, 0.5),
            white: 0.99851,
            red_green: [0.99851, 0.53863, 0.0],
            blue_green: [0.00392, 0.53863, 0.99851],
            rainbow: [0.99851, 0.28420, 0.21729],
            palette: [0.43160, 0.56840, 0.0],
            black_white: 0.28420,
        },
        Example {
            shot: shot().twist(3).density(2).speed(44.0).frame(300),
            at: (0.25, 0.75),
            white: 0.99379,
            red_green: [0.99379, 0.57854, 0.0],
            blue_green: [0.00392, 0.57854, 0.99379],
            rainbow: [0.99379, 0.32112, 0.18509],
            palette: [0.35776, 0.64224, 0.0],
            black_white: 0.32112,
        },
    ];
    for (which, example) in examples.into_iter().enumerate() {
        let sample = |colors: PlasmaColors, palette: &[Rgb]| {
            let c = example
                .shot
                .clone()
                .colors(colors)
                .palette(palette)
                .at(example.at.0, example.at.1);
            [c.r, c.g, c.b].map(f64::from)
        };
        let cases = [
            (sample(PlasmaColors::White, &PALETTE), [example.white; 3]),
            (sample(PlasmaColors::RedGreen, &PALETTE), example.red_green),
            (sample(PlasmaColors::BlueGreen, &PALETTE), example.blue_green),
            (sample(PlasmaColors::Rainbow, &PALETTE), example.rainbow),
            (sample(PlasmaColors::Palette, &PALETTE), example.palette),
            (
                sample(PlasmaColors::Palette, &BLACK_WHITE),
                [example.black_white; 3],
            ),
        ];
        for (got, want) in cases {
            for (got, want) in got.into_iter().zip(want) {
                assert!(
                    (got - want).abs() <= 0.000_015,
                    "example {}: {got}, not {want}",
                    which + 1
                );
            }
        }
    }
}

/// However long the effect has run, the picture stays the smooth field it starts as: four hours
/// in at the top speed, neighboring cells of a wide buffer are no further apart than on the
/// first frames, and the frame is still the one the description gives.
#[test]
fn goal_smooth_however_long_it_has_run() {
    let late = Shot {
        length_ms: 14_400_000,
        ..shot().speed(100.0).frame_ms(10).on(1001, 1001).frame(1_440_000)
    };
    for line in [middle_row(), middle_column()] {
        let levels = late.brightness(&line);
        let largest = levels
            .windows(2)
            .map(|pair| (pair[0] - pair[1]).abs())
            .fold(0.0, f64::max);
        assert!(largest <= 0.025, "a step of {largest} between neighbors");

        let white = late.clone().colors(PlasmaColors::White);
        for (&(u, v), level) in line.iter().zip(&levels) {
            let want = described(&white, u, v)[0];
            assert!((level - want).abs() <= 3e-7, "at ({u}, {v}): {level}, not {want}");
        }
    }
}

/// No sine comes from the system's maths library, and everything else is whole numbers and sums
/// that every machine rounds alike, so the same pixel gets the same bits on every machine: these
/// were worked out on one.
#[test]
fn goal_the_same_bits_on_every_machine() {
    let busy = shot().twist(3).density(2).speed(44.0).frame(300);
    let late = shot().twist(10).density(10).speed(100.0).frame(72_000);
    let samples = [
        shot().at(0.5, 0.5),
        busy.clone().at(0.25, 0.75),
        busy.clone().colors(PlasmaColors::Rainbow).at(0.25, 0.75),
        busy.clone()
            .colors(PlasmaColors::RedGreen)
            .on(400, 178)
            .at(0.9, 0.1),
        late.colors(PlasmaColors::White).at(1.0, 1.0),
        busy.palette(&BLACK_WHITE).on(5000, 5000).at(0.123, 0.987),
    ];
    let bits = samples.map(|c| [c.r, c.g, c.b].map(f32::to_bits));
    assert_eq!(bits, PINNED, "{bits:#010x?}");
}

/// The bits [`goal_the_same_bits_on_every_machine`] holds the effect to.
const PINNED: [[u32; 3]; 6] = [
    [0x3edc_fa48, 0x3f11_82dc, 0x0000_0000],
    [0x3eb7_2c8e, 0x3f24_69b9, 0x0000_0000],
    [0x3f7e_693d, 0x3ea4_69b9, 0x3e3d_879c],
    [0x3f7f_f173, 0x3f03_d074, 0x0000_0000],
    [0x3f79_6248, 0x3f79_6248, 0x3f79_6248],
    [0x3f42_d619, 0x3f42_d619, 0x3f42_d619],
];
