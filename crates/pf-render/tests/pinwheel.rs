//! Pinwheel, held to its behaviour specification: one test for each acceptance property, numbered
//! as the specification numbers them (`p01` to `p25`), then the goals of the design that no
//! property covers (`goal_…`).
//!
//! The rig, unless a test says otherwise: a buffer 101 columns by 101 rows, the palette red,
//! green, blue, an effect 10 seconds long with 50 ms frames, and every setting at its default
//! except those named. Distances are in cells and angles in degrees, counterclockwise from
//! pointing right, both from the wheel's center. A sample is *lit* when its alpha is 0.5 or more.

use pf_render::{Canvas, Colors, EffectTime, Pinwheel, Pixel, Rgba, Shade, Shader};
use pf_sequence::{EffectParams, PinwheelParams, PinwheelShading, PinwheelStyle, Rgb};

const RIG: Canvas = Canvas {
    columns: 101,
    rows: 101,
};
const PALETTE: [Rgb; 3] = [Rgb::RED, Rgb::GREEN, Rgb::BLUE];
const ONE: [Rgb; 1] = [Rgb::WHITE];

const RED: [f32; 3] = [1.0, 0.0, 0.0];
const GREEN: [f32; 3] = [0.0, 1.0, 0.0];
const BLUE: [f32; 3] = [0.0, 0.0, 1.0];
const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

const SMOOTH: PinwheelStyle = PinwheelStyle::Smooth;
const SPOKES: PinwheelStyle = PinwheelStyle::Spokes;
const STYLES: [PinwheelStyle; 2] = [SMOOTH, SPOKES];
const SHADINGS: [PinwheelShading; 4] = [
    PinwheelShading::Flat,
    PinwheelShading::Raised,
    PinwheelShading::Sunken,
    PinwheelShading::Sweep,
];

fn defaults() -> PinwheelParams {
    PinwheelParams::default()
}

fn square(cells: u32) -> Canvas {
    Canvas {
        columns: cells,
        rows: cells,
    }
}

/// `elapsed_ms` into a 10-second effect with 50 ms frames.
fn at_ms(elapsed_ms: u64) -> EffectTime {
    EffectTime::within(0, 10_000, elapsed_ms).with_frame_ms(50)
}

/// Where an arm ends, in cells, with the center in the middle.
fn arm_length(p: &PinwheelParams, canvas: Canvas) -> f64 {
    let (columns, rows) = (f64::from(canvas.columns), f64::from(canvas.rows));
    let reference = match p.style {
        PinwheelStyle::Smooth => columns.hypot(rows) / 2.0,
        PinwheelStyle::Spokes => columns.max(rows) / 2.0,
    };
    reference * f64::from(p.arm_size) / 100.0
}

/// An arm as the specification places it: where its center line leaves the center, and its color.
#[derive(Debug, Clone, Copy)]
struct Arm {
    degrees: f64,
    color: [f32; 3],
}

/// The arms of a wheel `elapsed_ms` into its effect, first arm first.
fn arms_of(p: &PinwheelParams, palette: &[Rgb], elapsed_ms: u64) -> Vec<Arm> {
    let turned = f64::from(p.speed) * 20.0 * elapsed_ms as f64 / 1000.0;
    let sector = 360.0 / f64::from(p.arms);
    let offset = f64::from(p.offset);
    (0..p.arms)
        .map(|k| {
            let round = f64::from(k) * sector;
            let degrees = match (p.style, p.counterclockwise) {
                (PinwheelStyle::Smooth, true) => 270.0 + offset + turned - round,
                (PinwheelStyle::Smooth, false) => 270.0 - offset - turned + round,
                (PinwheelStyle::Spokes, true) => offset + turned + round,
                (PinwheelStyle::Spokes, false) => offset - turned + round,
            };
            let color = match palette.len() {
                0 => WHITE,
                n => {
                    let c = palette[(k as usize + 1) % n];
                    [c.r, c.g, c.b].map(|v| f32::from(v) / 255.0)
                }
            };
            Arm {
                degrees: degrees.rem_euclid(360.0),
                color,
            }
        })
        .collect()
}

/// How far apart two directions are (0 to 180 degrees).
fn apart(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// How far counterclockwise of `b` the direction `a` is (-180 to 180 degrees).
fn ahead(a: f64, b: f64) -> f64 {
    (a - b + 180.0).rem_euclid(360.0) - 180.0
}

fn rgb(c: Rgba) -> [f32; 3] {
    [c.r, c.g, c.b]
}

fn lit(c: &Rgba) -> bool {
    c.a >= 0.5
}

fn bits(c: &Rgba) -> [u32; 4] {
    [c.r, c.g, c.b, c.a].map(f32::to_bits)
}

fn all_bits(frame: &[Rgba]) -> Vec<[u32; 4]> {
    frame.iter().map(bits).collect()
}

/// One frame of a Pinwheel.
struct Frame {
    shader: Shader,
    canvas: Canvas,
    /// The wheel's center, across and up the buffer.
    center: (f64, f64),
}

impl Frame {
    fn new(p: PinwheelParams, palette: &[Rgb], canvas: Canvas, elapsed_ms: u64) -> Self {
        Self::timed(p, palette, canvas, at_ms(elapsed_ms), 0)
    }

    fn timed(p: PinwheelParams, palette: &[Rgb], canvas: Canvas, time: EffectTime, seed: u64) -> Self {
        Self {
            shader: Shader::new(
                &EffectParams::Pinwheel(p),
                &time,
                Colors::new(palette),
                seed,
                canvas,
            ),
            canvas,
            center: (
                center_of(p.center_x, canvas.columns),
                center_of(p.center_y, canvas.rows),
            ),
        }
    }

    fn at(&self, u: f32, v: f32) -> Rgba {
        self.shader.shade(&Pixel {
            u,
            v,
            index: 0,
            count: 1,
        })
    }

    /// Where the point `r` cells from the center at `degrees` is, across and up the buffer.
    fn place(&self, r: f64, degrees: f64) -> (f64, f64) {
        let (sin, cos) = degrees.to_radians().sin_cos();
        (
            self.center.0 + r * cos / f64::from(self.canvas.columns - 1),
            self.center.1 + r * sin / f64::from(self.canvas.rows - 1),
        )
    }

    /// Whether that point is on the buffer.
    fn holds(&self, r: f64, degrees: f64) -> bool {
        let (u, v) = self.place(r, degrees);
        (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v)
    }

    /// The sample `r` cells from the center at `degrees`.
    fn sample(&self, r: f64, degrees: f64) -> Rgba {
        let (u, v) = self.place(r, degrees);
        self.at(u as f32, v as f32)
    }

    /// The ring at `r`: `samples` samples, evenly spaced from 0 degrees round.
    fn ring_of(&self, r: f64, samples: usize) -> Vec<Rgba> {
        (0..samples)
            .map(|k| self.sample(r, k as f64 * 360.0 / samples as f64))
            .collect()
    }

    /// The ring at `r`, sampled every half degree.
    fn ring(&self, r: f64) -> Vec<Rgba> {
        self.ring_of(r, 720)
    }

    /// The pixel in column `x` of row `y`, counting from the bottom left.
    fn pixel(&self, x: u32, y: u32) -> Rgba {
        self.at(along(x, self.canvas.columns), along(y, self.canvas.rows))
    }

    /// Every pixel of the buffer, bottom row first.
    fn pixels(&self) -> Vec<Rgba> {
        let Canvas { columns, rows } = self.canvas;
        (0..rows)
            .flat_map(|y| (0..columns).map(move |x| (x, y)))
            .map(|(x, y)| self.pixel(x, y))
            .collect()
    }

    /// Every pixel with its distance from the wheel's center in cells.
    fn pixels_by_distance(&self) -> Vec<(f64, Rgba)> {
        let Canvas { columns, rows } = self.canvas;
        let (cx, cy) = (
            self.center.0 * f64::from(columns - 1),
            self.center.1 * f64::from(rows - 1),
        );
        (0..rows)
            .flat_map(|y| (0..columns).map(move |x| (x, y)))
            .map(|(x, y)| ((f64::from(x) - cx).hypot(f64::from(y) - cy), self.pixel(x, y)))
            .collect()
    }
}

/// Where a center setting puts the wheel's center along a side of `cells` cells, as a pixel
/// position: the setting runs over the side's whole extent, from one edge of the buffer at -100
/// to the other at 100, half a cell beyond the first and last pixels' own places.
fn center_of(setting: f32, cells: u32) -> f64 {
    let share = 0.5 + f64::from(setting) / 200.0;
    if cells > 1 {
        (share * f64::from(cells) - 0.5) / f64::from(cells - 1)
    } else {
        0.5
    }
}

/// Where pixel `i` of `count` in a row (or column) sits: evenly from 0 to 1, or in the middle
/// when there is only one.
fn along(i: u32, count: u32) -> f32 {
    if count <= 1 {
        0.5
    } else {
        i as f32 / (count - 1) as f32
    }
}

/// A maximal stretch of neighboring lit samples on a ring.
#[derive(Debug, Clone, Copy)]
struct Run {
    /// The angle half way along it.
    center: f64,
    /// The angle it spans.
    width: f64,
    /// The color of its middle sample.
    color: [f32; 3],
}

/// The runs on a ring (which wraps), in order round it.
fn runs(ring: &[Rgba]) -> Vec<Run> {
    let n = ring.len();
    let step = 360.0 / n as f64;
    let Some(gap) = ring.iter().position(|c| !lit(c)) else {
        return vec![Run {
            center: 180.0,
            width: 360.0,
            color: rgb(ring[0]),
        }];
    };
    let mut found = Vec::new();
    let mut first = None;
    for at in gap + 1..=gap + n {
        match (lit(&ring[at % n]), first) {
            (true, None) => first = Some(at),
            (false, Some(from)) => {
                found.push(Run {
                    center: ((from + at - 1) as f64 / 2.0 * step).rem_euclid(360.0),
                    width: (at - from) as f64 * step,
                    color: rgb(ring[((from + at - 1) / 2) % n]),
                });
                first = None;
            }
            _ => {}
        }
    }
    found.sort_by(|a, b| a.center.total_cmp(&b.center));
    found
}

/// The single run on a ring.
fn only_run(ring: &[Rgba]) -> Run {
    let found = runs(ring);
    assert_eq!(found.len(), 1, "expected one run: {found:?}");
    found[0]
}

/// The run on a ring centered nearest `degrees`.
fn run_near(ring: &[Rgba], degrees: f64) -> Run {
    runs(ring)
        .into_iter()
        .min_by(|a, b| apart(a.center, degrees).total_cmp(&apart(b.center, degrees)))
        .expect("a run")
}

fn mean_alpha(ring: &[Rgba]) -> f64 {
    ring.iter().map(|c| f64::from(c.a)).sum::<f64>() / ring.len() as f64
}

// ---------------------------------------------------------------------------------------------
// The acceptance properties.

/// Property 1: Unlit is clear; lit is the palette color.
#[test]
fn p01_unlit_is_clear_and_lit_is_the_palette_color() {
    for style in STYLES {
        let p = PinwheelParams {
            arms: 4,
            thickness: 50.0,
            style,
            ..defaults()
        };
        let frame = Frame::new(p, &PALETTE, RIG, 0);
        let arms = arms_of(&p, &PALETTE, 0);
        let (mut inside, mut outside) = (0, 0);
        // On the pixels of the grid and at places between them.
        for (shift_x, shift_y) in [(0.0, 0.0), (0.37, 0.61)] {
            for y in -45..=45 {
                for x in -45..=45 {
                    let (x, y) = (f64::from(x) + shift_x, f64::from(y) + shift_y);
                    let r = x.hypot(y);
                    if r > 45.0 {
                        continue;
                    }
                    let degrees = y.atan2(x).to_degrees();
                    let arm = arms
                        .iter()
                        .min_by(|a, b| apart(a.degrees, degrees).total_cmp(&apart(b.degrees, degrees)))
                        .unwrap();
                    // Each arm is 45 degrees wide: how far past its nearer edge the sample is.
                    let past = apart(arm.degrees, degrees) - 22.5;
                    let cells = r * past.abs().to_radians().sin();
                    let got = frame.at(0.5 + x as f32 / 100.0, 0.5 + y as f32 / 100.0);
                    if past > 0.0 && cells > 2.0 {
                        assert_eq!(got.a, 0.0, "{style:?} outside at ({x}, {y})");
                        outside += 1;
                    } else if past < 0.0 && cells > 2.0 {
                        assert_eq!(got.a, 1.0, "{style:?} inside at ({x}, {y})");
                        assert_eq!(rgb(got), arm.color, "{style:?} at ({x}, {y})");
                        inside += 1;
                    }
                }
            }
        }
        assert!(
            inside > 3000 && outside > 3000,
            "{inside} inside, {outside} outside"
        );
    }
}

/// Property 2: Arm count and even spacing, for every count (the ones that do not divide 360 are new).
#[test]
fn p02_arm_count_and_even_spacing() {
    let counts = [1, 2, 3, 4, 5, 6, 8, 12, 20, 7, 11, 13, 14, 16, 17, 19];
    for style in STYLES {
        for arms in counts {
            let p = PinwheelParams {
                arms,
                thickness: 30.0,
                speed: 0.0,
                style,
                ..defaults()
            };
            let found = runs(&Frame::new(p, &ONE, RIG, 0).ring(40.0));
            assert_eq!(found.len(), arms as usize, "{style:?} with {arms} arms");
            let sector = 360.0 / f64::from(arms);
            for (k, run) in found.iter().enumerate() {
                let next = found[(k + 1) % found.len()];
                let gap = (next.center - run.center).rem_euclid(360.0);
                let gap = if arms == 1 { 360.0 } else { gap };
                assert!(
                    (gap - sector).abs() <= 3.0,
                    "{style:?} with {arms} arms: {gap} between runs"
                );
            }
        }
    }
}

/// Property 3: Thickness sets the angular width.
#[test]
fn p03_thickness_sets_the_angular_width() {
    for (style, allowed) in [(SMOOTH, 3.0), (SPOKES, 4.0)] {
        for thickness in [10.0, 25.0, 50.0, 75.0] {
            let p = PinwheelParams {
                arms: 4,
                speed: 0.0,
                thickness,
                style,
                ..defaults()
            };
            let found = runs(&Frame::new(p, &ONE, RIG, 0).ring(40.0));
            assert_eq!(found.len(), 4);
            let want = f64::from(thickness) / 100.0 * 90.0;
            for run in found {
                assert!(
                    (run.width - want).abs() <= allowed,
                    "{style:?} at thickness {thickness}: {} wide, not {want}",
                    run.width
                );
            }
        }
    }
}

/// Property 4: The lit share does not depend on resolution (smooth).
#[test]
fn p04_the_lit_share_does_not_depend_on_resolution() {
    for cells in [51, 101, 201] {
        for (thickness, share) in [(25.0, 0.25), (50.0, 0.50)] {
            for shading in SHADINGS {
                let p = PinwheelParams {
                    arms: 4,
                    speed: 0.0,
                    thickness,
                    shading,
                    ..defaults()
                };
                let canvas = square(cells);
                let ring = Frame::new(p, &ONE, canvas, 0).ring(0.6 * arm_length(&p, canvas));
                let (want, allowed) = match shading {
                    PinwheelShading::Flat => (share, 0.03),
                    _ => (share / 2.0, 0.04),
                };
                let got = mean_alpha(&ring);
                assert!(
                    (got - want).abs() <= allowed,
                    "{cells} cells, thickness {thickness}, {shading:?}: {got}, not {want}"
                );
            }
        }
    }
}

/// Property 5: Full thickness leaves no gaps.
#[test]
fn p05_full_thickness_leaves_no_gaps() {
    let p = PinwheelParams {
        thickness: 100.0,
        speed: 0.0,
        ..defaults()
    };
    let frame = Frame::new(p, &PALETTE, RIG, 0);
    let length = arm_length(&p, RIG);
    for (r, c) in frame.pixels_by_distance() {
        if r < length - 1.0 {
            assert!(c.a >= 0.99, "a pixel {r} cells out has alpha {}", c.a);
        }
    }
    // Along the boundaries between arms, and either side of them.
    let arms = arms_of(&p, &PALETTE, 0);
    for arm in &arms {
        for lean in [-0.5, -0.1, 0.0, 0.1, 0.5] {
            let boundary = arm.degrees + 60.0 + lean;
            for step in 0..=((length - 1.0) * 4.0) as u32 {
                let r = f64::from(step) / 4.0;
                if frame.holds(r, boundary) {
                    let got = frame.sample(r, boundary);
                    assert!(got.a >= 0.99, "{r} cells out at {boundary}: {}", got.a);
                }
            }
        }
    }
    // Three stretches of color, a third of the circle each.
    let ring = frame.ring(40.0);
    let mut changes = Vec::new();
    for k in 0..ring.len() {
        if rgb(ring[k]) != rgb(ring[(k + 1) % ring.len()]) {
            changes.push(k as f64 * 0.5);
        }
    }
    assert_eq!(changes.len(), 3, "{changes:?}");
    for k in 0..3 {
        let stretch = (changes[(k + 1) % 3] - changes[k]).rem_euclid(360.0);
        assert!((stretch - 120.0).abs() <= 3.0, "a stretch of {stretch}");
    }

    let spokes = Frame::new(PinwheelParams { style: SPOKES, ..p }, &PALETTE, RIG, 0);
    for (r, c) in spokes.pixels_by_distance() {
        if r <= 49.0 {
            assert!(lit(&c), "spokes: a pixel {r} cells out has alpha {}", c.a);
        }
    }
    for k in 0..7200 {
        for r in [0.3, 7.7, 23.1, 48.9] {
            let got = spokes.sample(r, f64::from(k) * 0.05);
            assert!(
                lit(&got),
                "spokes: {r} cells out at {}: {}",
                f64::from(k) * 0.05,
                got.a
            );
        }
    }
}

/// Property 6: Thin arms never vanish (new): every arm can be followed from the center to its end, in
/// every shading and both styles, twisted or not.
#[test]
fn p06_thin_arms_never_vanish() {
    for style in STYLES {
        for shading in SHADINGS {
            for arms in [1, 3, 12, 20] {
                for twist in [0.0, 360.0] {
                    let frames = [0.0, 1.0].map(|thickness| {
                        let p = PinwheelParams {
                            arms,
                            twist,
                            thickness,
                            shading,
                            speed: 0.0,
                            style,
                            ..defaults()
                        };
                        (p, Frame::new(p, &ONE, RIG, 0))
                    });
                    for (p, frame) in &frames {
                        let length = arm_length(p, RIG);
                        let what = format!(
                            "{style:?}, {shading:?}, {arms} arms, twist {twist}, thickness {}",
                            p.thickness
                        );
                        let mut followed = 0u32;
                        for arm in arms_of(p, &ONE, 0) {
                            for r in 3..=(length - 2.0).floor() as u32 {
                                let r = f64::from(r);
                                let line = arm.degrees + f64::from(twist) * r / length;
                                if !frame.holds(r, line) {
                                    continue;
                                }
                                // Within a cell and a half of the center line, along the ring.
                                let found = (-12..=12).any(|k| {
                                    let cells = f64::from(k) / 8.0;
                                    lit(&frame.sample(r, line + (cells / r).to_degrees()))
                                });
                                assert!(found, "{what}: nothing lit {r} cells out near {line}");
                                followed += 1;
                            }
                        }
                        assert!(followed >= 40 * arms, "{what}: only {followed} places checked");
                    }
                    if style == SMOOTH {
                        assert_eq!(
                            all_bits(&frames[0].1.pixels()),
                            all_bits(&frames[1].1.pixels()),
                            "{shading:?}, {arms} arms, twist {twist}: thickness 0 and 1 differ"
                        );
                    }
                }
            }
        }
    }
}

/// Property 6, where the rays of the spokes style stand apart: a twisted arm crosses from ray to
/// ray, often in less than a cell, and must still be an unbroken run of lit cells. At every
/// whole distance from 58 cells out to two short of the arm's end, the cell that holds the arm
/// there (on the whole degree nearest its center line) is lit, at any twist; and the wheel
/// lights within a tenth as many pixels as the picture imports were judged against, which grows
/// with twist as the arm's path lengthens.
#[test]
fn p06_twisted_spokes_are_unbroken_where_the_rays_stand_apart() {
    /// For each twist, the lit pixels of the picture judged against.
    type Judged = [(f32, f64)];
    // The buffer's size, the arms' thickness, and what was measured there.
    let rigs: [(u32, f32, &Judged); 3] = [
        (
            400,
            1.0,
            &[
                (0.0, 2672.0),
                (90.0, 3111.0),
                (-90.0, 3029.0),
                (180.0, 3490.0),
                (-180.0, 3477.0),
                (360.0, 4284.0),
                (-360.0, 4316.0),
            ],
        ),
        (400, 0.0, &[(0.0, 2668.0), (180.0, 3386.0), (-360.0, 4300.0)]),
        (
            200,
            1.0,
            &[(0.0, 1352.0), (180.0, 1991.0), (360.0, 2214.0), (-360.0, 2208.0)],
        ),
    ];
    for (cells, thickness, judged) in rigs {
        let canvas = square(cells);
        let (middle, size) = (f64::from(cells - 1) / 2.0, i64::from(cells));
        for &(twist, lit_there) in judged {
            let p = PinwheelParams {
                arms: 12,
                thickness,
                twist,
                speed: 0.0,
                style: SPOKES,
                ..defaults()
            };
            let what = format!("{cells} cells, thickness {thickness}, twist {twist}");
            let alphas: Vec<f32> = Frame::new(p, &ONE, canvas, 0)
                .pixels()
                .iter()
                .map(|c| c.a)
                .collect();
            let lit_at = |x: i64, y: i64| {
                (0..size).contains(&x) && (0..size).contains(&y) && alphas[(y * size + x) as usize] >= 0.5
            };
            let length = arm_length(&p, canvas);
            for arm in arms_of(&p, &ONE, 0) {
                for r in 58..=(length - 2.0) as u32 {
                    let r = f64::from(r);
                    let line = arm.degrees + f64::from(twist) * r / length;
                    // The whole degree nearest the center line (either, where it is half way).
                    let half_way = (line - line.floor() - 0.5).abs() < 1e-3;
                    let rays = if half_way {
                        vec![line.floor(), line.ceil()]
                    } else {
                        vec![line.round()]
                    };
                    let held = rays.iter().any(|ray| {
                        let (sin, cos) = ray.to_radians().sin_cos();
                        let (x, y) = (middle + r * cos, middle + r * sin);
                        // The pixel whose cell holds that place (either, on the line between two).
                        (-1..=1).any(|dy| {
                            (-1..=1).any(|dx| {
                                let (px, py) = (x.round() as i64 + dx, y.round() as i64 + dy);
                                (px as f64 - x).abs() <= 0.502
                                    && (py as f64 - y).abs() <= 0.502
                                    && lit_at(px, py)
                            })
                        })
                    });
                    assert!(held, "{what}: the arm at {} is broken {r} cells out", arm.degrees);
                }
            }
            let lit = alphas.iter().filter(|a| **a >= 0.5).count() as f64;
            assert!(
                (lit / lit_there - 1.0).abs() <= 0.10,
                "{what}: {lit} pixels lit, {lit_there} in the picture judged against"
            );
        }
    }
    // A wide arm wound right round narrows without breaking, and keeps its light.
    let p = PinwheelParams {
        arms: 4,
        thickness: 30.0,
        twist: 360.0,
        speed: 0.0,
        style: SPOKES,
        ..defaults()
    };
    let lit = Frame::new(p, &ONE, RIG, 0)
        .pixels()
        .iter()
        .filter(|c| lit(c))
        .count() as f64;
    assert!(
        (lit / 2996.0 - 1.0).abs() <= 0.10,
        "{lit} pixels lit, 2996 in the picture judged against"
    );
}

/// Property 7: Thin arms stay thin.
#[test]
fn p07_thin_arms_stay_thin() {
    let p = PinwheelParams {
        speed: 0.0,
        ..defaults()
    };
    let found = runs(&Frame::new(p, &ONE, RIG, 0).ring(20.0));
    assert_eq!(found.len(), 3);
    for run in found {
        assert!(
            (2.0..=7.2).contains(&run.width),
            "a run {} degrees wide",
            run.width
        );
    }
}

/// Property 8: Starting direction.
#[test]
fn p08_starting_direction() {
    for (style, want) in [(SMOOTH, 270.0), (SPOKES, 0.0)] {
        let p = PinwheelParams {
            arms: 1,
            thickness: 10.0,
            speed: 0.0,
            style,
            ..defaults()
        };
        let run = only_run(&Frame::new(p, &ONE, RIG, 0).ring(40.0));
        assert!(
            apart(run.center, want) <= 3.0,
            "{style:?}: centered at {}",
            run.center
        );
    }
}

/// Property 9: Rate and direction of rotation.
#[test]
fn p09_rate_and_direction_of_rotation() {
    for style in STYLES {
        for counterclockwise in [true, false] {
            for (speed, turn) in [(5.0, 40.0), (10.0, 80.0), (25.0, 200.0)] {
                let p = PinwheelParams {
                    arms: 1,
                    thickness: 10.0,
                    speed,
                    counterclockwise,
                    style,
                    ..defaults()
                };
                let before = only_run(&Frame::new(p, &ONE, RIG, 0).ring(40.0)).center;
                let after = only_run(&Frame::new(p, &ONE, RIG, 400).ring(40.0)).center;
                let want = before + if counterclockwise { turn } else { -turn };
                assert!(
                    apart(after, want) <= 3.0,
                    "{style:?}, counterclockwise {counterclockwise}, speed {speed}: {before} to {after}"
                );
            }
        }
        // A full turn every 1.8 seconds at speed 10.
        let p = PinwheelParams {
            arms: 1,
            thickness: 10.0,
            style,
            ..defaults()
        };
        let start = Frame::new(p, &ONE, RIG, 0).pixels();
        let turn = Frame::new(p, &ONE, RIG, 1800).pixels();
        for (a, b) in start.iter().zip(&turn) {
            assert!((a.a - b.a).abs() <= 0.02);
        }
    }
}

/// Property 10: Speed 0 is still.
#[test]
fn p10_speed_0_is_still() {
    let settings = [
        defaults(),
        PinwheelParams {
            arms: 7,
            twist: 133.5,
            thickness: 40.0,
            shading: PinwheelShading::Sweep,
            offset: 77.7,
            center_x: 31.0,
            center_y: -12.5,
            ..defaults()
        },
        PinwheelParams {
            arms: 12,
            arm_size: 250.0,
            twist: -360.0,
            thickness: 1.0,
            counterclockwise: false,
            shading: PinwheelShading::Raised,
            style: SPOKES,
            ..defaults()
        },
    ];
    for p in settings {
        let p = PinwheelParams { speed: 0.0, ..p };
        let first = all_bits(&Frame::new(p, &PALETTE, RIG, 0).pixels());
        assert!(first.iter().any(|c| c[3] != 0), "something is drawn");
        for elapsed in [1000, 7000] {
            assert_eq!(first, all_bits(&Frame::new(p, &PALETTE, RIG, elapsed).pixels()));
        }
    }
}

/// Property 11: A frame depends on nothing else: not on the effect's length, where it starts, the frame
/// rate, the random seed, the order pixels and frames are asked for, or the thread asking.
#[test]
fn p11_a_frame_depends_on_nothing_else() {
    let p = PinwheelParams {
        arms: 5,
        twist: 80.0,
        thickness: 35.0,
        shading: PinwheelShading::Sweep,
        center_x: 20.0,
        ..defaults()
    };
    let elapsed = 400;
    let reference = all_bits(&Frame::new(p, &PALETTE, RIG, elapsed).pixels());
    assert!(reference.iter().any(|c| c[3] != 0));

    let other_times = [
        EffectTime::within(0, 1000, elapsed).with_frame_ms(50),
        EffectTime::within(0, 60_000, elapsed).with_frame_ms(50),
        EffectTime::within(123_450, 133_450, 123_450 + elapsed).with_frame_ms(50),
        EffectTime::within(0, 10_000, elapsed).with_frame_ms(20),
    ];
    for time in other_times {
        for seed in [0, 0xDEAD_BEEF] {
            let frame = Frame::timed(p, &PALETTE, RIG, time, seed);
            assert_eq!(reference, all_bits(&frame.pixels()), "{time:?}, seed {seed}");
        }
    }

    // Frames asked for in another order, and pixels asked for backwards.
    let frames: Vec<Frame> = [900, 0, elapsed, 50]
        .into_iter()
        .map(|t| Frame::new(p, &PALETTE, RIG, t))
        .collect();
    let _ = frames[0].pixels();
    let _ = frames[1].pixels();
    let mut backwards: Vec<[u32; 4]> = (0..RIG.rows)
        .rev()
        .flat_map(|y| (0..RIG.columns).rev().map(move |x| (x, y)))
        .map(|(x, y)| bits(&frames[2].pixel(x, y)))
        .collect();
    backwards.reverse();
    assert_eq!(reference, backwards);

    // From several threads at once, sharing one wheel.
    fn shareable<T: Send + Sync>(_: &T) {}
    let wheel = Pinwheel::new(&p, &at_ms(elapsed), Colors::new(&PALETTE), RIG);
    shareable(&wheel);
    let draw = || -> Vec<[u32; 4]> {
        (0..RIG.rows)
            .flat_map(|y| (0..RIG.columns).map(move |x| (x, y)))
            .map(|(x, y)| {
                bits(&wheel.shade(&Pixel {
                    u: along(x, RIG.columns),
                    v: along(y, RIG.rows),
                    index: 0,
                    count: 1,
                }))
            })
            .collect()
    };
    let drawn: Vec<Vec<[u32; 4]>> = std::thread::scope(|scope| {
        let threads: Vec<_> = (0..4).map(|_| scope.spawn(draw)).collect();
        threads.into_iter().map(|t| t.join().unwrap()).collect()
    });
    for frame in drawn {
        assert_eq!(reference, frame);
    }
}

/// Property 12: Start angle.
#[test]
fn p12_start_angle() {
    for (style, counterclockwise, want) in [
        (SMOOTH, true, 0.0),
        (SMOOTH, false, 180.0),
        (SPOKES, true, 90.0),
        (SPOKES, false, 90.0),
    ] {
        let p = PinwheelParams {
            arms: 1,
            thickness: 10.0,
            speed: 0.0,
            offset: 90.0,
            counterclockwise,
            style,
            ..defaults()
        };
        let run = only_run(&Frame::new(p, &ONE, RIG, 0).ring(40.0));
        assert!(
            apart(run.center, want) <= 3.0,
            "{style:?}, counterclockwise {counterclockwise}: centered at {}",
            run.center
        );
        let full = Frame::new(PinwheelParams { offset: 360.0, ..p }, &ONE, RIG, 0);
        let none = Frame::new(PinwheelParams { offset: 0.0, ..p }, &ONE, RIG, 0);
        assert_eq!(all_bits(&full.pixels()), all_bits(&none.pixels()));
    }
}

/// Property 13: Center.
#[test]
fn p13_center() {
    let disc = |center_x: f32, center_y: f32| {
        let p = PinwheelParams {
            arms: 1,
            thickness: 100.0,
            arm_size: 20.0,
            center_x,
            center_y,
            ..defaults()
        };
        Frame::new(p, &ONE, RIG, 0)
    };
    // The lit pixels, as (column, row).
    let lit_pixels = |frame: &Frame| -> Vec<(f64, f64)> {
        (0..RIG.rows)
            .flat_map(|y| (0..RIG.columns).map(move |x| (x, y)))
            .filter(|&(x, y)| lit(&frame.pixel(x, y)))
            .map(|(x, y)| (f64::from(x), f64::from(y)))
            .collect()
    };
    for (center_x, center_y) in [(0.0, 0.0), (50.0, 0.0), (0.0, -50.0), (-50.0, 50.0)] {
        let found = lit_pixels(&disc(center_x, center_y));
        assert!(found.len() > 500, "a disc of {} pixels", found.len());
        let n = found.len() as f64;
        let across = found.iter().map(|p| p.0).sum::<f64>() / n;
        let up = found.iter().map(|p| p.1).sum::<f64>() / n;
        let want = (50.0 + f64::from(center_x) / 2.0, 50.0 + f64::from(center_y) / 2.0);
        assert!(
            (across - want.0).abs() <= 1.0 && (up - want.1).abs() <= 1.0,
            "center ({center_x}, {center_y}): the disc is about ({across}, {up})"
        );
    }
    // On an edge: half a disc, 28.6 cells long on that edge and no more than 16 deep.
    for (center_x, center_y) in [(100.0, 0.0), (-100.0, 0.0), (0.0, 100.0), (0.0, -100.0)] {
        let found = lit_pixels(&disc(center_x, center_y));
        // Each lit pixel as (how far in from the edge, where along the edge).
        let from_edge: Vec<(f64, f64)> = found
            .iter()
            .map(|&(x, y)| match (center_x as i32, center_y as i32) {
                (100, _) => (100.0 - x, y),
                (-100, _) => (x, y),
                (_, 100) => (100.0 - y, x),
                _ => (y, x),
            })
            .collect();
        assert!(from_edge.iter().all(|p| p.0 <= 16.0));
        let on_edge: Vec<f64> = from_edge.iter().filter(|p| p.0 == 0.0).map(|p| p.1).collect();
        let length = on_edge.len() as f64;
        let middle = on_edge.iter().sum::<f64>() / length;
        assert!(
            (length - 28.6).abs() <= 2.0,
            "({center_x}, {center_y}): {length} lit on the edge"
        );
        assert!(
            (middle - 50.0).abs() <= 1.0,
            "({center_x}, {center_y}): centered at {middle}"
        );
    }
}

/// Property 14: Arm length.
#[test]
fn p14_arm_length() {
    let wheel = |style, arm_size| {
        let p = PinwheelParams {
            thickness: 100.0,
            arm_size,
            style,
            ..defaults()
        };
        Frame::new(p, &ONE, RIG, 0)
    };
    for (style, arm_size, radius) in [
        (SMOOTH, 10.0, 7.1),
        (SMOOTH, 25.0, 17.9),
        (SMOOTH, 50.0, 35.7),
        (SPOKES, 50.0, 25.3),
    ] {
        for (r, c) in wheel(style, arm_size).pixels_by_distance() {
            if r < radius - 1.5 {
                assert!(lit(&c), "{style:?} at {arm_size}: unlit {r} cells out");
            } else if r > radius + 1.5 {
                assert!(!lit(&c), "{style:?} at {arm_size}: lit {r} cells out");
            }
        }
    }
    assert!(wheel(SMOOTH, 100.0).pixels().iter().all(lit));
    assert!(wheel(SMOOTH, 0.0).pixels().iter().all(|c| *c == Rgba::CLEAR));
    assert!(wheel(SPOKES, 0.0).pixels().iter().all(|c| *c == Rgba::CLEAR));

    let sides = [(50, 0), (50, 100), (0, 50), (100, 50)];
    let corners = [(0, 0), (0, 100), (100, 0), (100, 100)];
    let reaching = wheel(SPOKES, 100.0);
    assert!(sides.iter().all(|&(x, y)| lit(&reaching.pixel(x, y))));
    assert!(corners.iter().all(|&(x, y)| !lit(&reaching.pixel(x, y))));
    let beyond = wheel(SPOKES, 150.0);
    assert!(corners.iter().all(|&(x, y)| lit(&beyond.pixel(x, y))));
}

/// Property 15: Round, not stretched.
#[test]
fn p15_round_not_stretched() {
    let canvas = Canvas {
        columns: 101,
        rows: 41,
    };
    let p = PinwheelParams {
        thickness: 100.0,
        arm_size: 30.0,
        ..defaults()
    };
    let frame = Frame::new(p, &ONE, canvas, 0);
    let wide = (0..canvas.columns)
        .filter(|&x| (0..canvas.rows).any(|y| lit(&frame.pixel(x, y))))
        .count() as f64;
    let tall = (0..canvas.rows)
        .filter(|&y| (0..canvas.columns).any(|x| lit(&frame.pixel(x, y))))
        .count() as f64;
    assert!(
        (wide - 33.0).abs() <= 3.0 && (tall - 33.0).abs() <= 3.0,
        "{wide} by {tall}"
    );
}

/// Property 16: Twist.
#[test]
fn p16_twist() {
    for (style, turns) in [
        (SMOOTH, [(20.0, 25.0), (40.0, 50.0)]),
        (SPOKES, [(20.0, 36.0), (40.0, 71.0)]),
    ] {
        for counterclockwise in [true, false] {
            let wheel = |twist| {
                let p = PinwheelParams {
                    arms: 1,
                    thickness: 10.0,
                    speed: 0.0,
                    twist,
                    counterclockwise,
                    style,
                    ..defaults()
                };
                Frame::new(p, &ONE, RIG, 0)
            };
            let straight = wheel(0.0);
            for (twist, way) in [(90.0, 1.0), (-90.0, -1.0)] {
                let twisted = wheel(twist);
                for (r, turn) in turns {
                    let from = only_run(&straight.ring(r)).center;
                    let to = only_run(&twisted.ring(r)).center;
                    assert!(
                        (ahead(to, from) - way * turn).abs() <= 4.0,
                        "{style:?}, counterclockwise {counterclockwise}, twist {twist}, \
                         {r} cells out: {from} to {to}"
                    );
                }
            }
        }
    }
}

/// Property 17: Which color on which arm.
#[test]
fn p17_which_color_on_which_arm() {
    let wheel = |style, counterclockwise, arms, palette: &[Rgb]| {
        let p = PinwheelParams {
            arms,
            thickness: 30.0,
            speed: 0.0,
            counterclockwise,
            style,
            ..defaults()
        };
        Frame::new(p, palette, RIG, 0)
    };
    for (style, counterclockwise, want) in [
        (SMOOTH, true, [(270.0, GREEN), (150.0, BLUE), (30.0, RED)]),
        (SMOOTH, false, [(270.0, GREEN), (30.0, BLUE), (150.0, RED)]),
        (SPOKES, true, [(0.0, GREEN), (120.0, BLUE), (240.0, RED)]),
        (SPOKES, false, [(0.0, GREEN), (120.0, BLUE), (240.0, RED)]),
    ] {
        let ring = wheel(style, counterclockwise, 3, &PALETTE).ring(40.0);
        assert_eq!(runs(&ring).len(), 3);
        for (degrees, color) in want {
            let run = run_near(&ring, degrees);
            assert!(
                apart(run.center, degrees) <= 3.0 && run.color == color,
                "{style:?}, counterclockwise {counterclockwise}: {run:?} where {color:?} belongs at {degrees}"
            );
        }
    }
    for style in STYLES {
        for (palette, arms, want) in [
            (&[Rgb::RED][..], 3, vec![RED; 3]),
            (&[][..], 3, vec![WHITE; 3]),
            (&PALETTE[..], 1, vec![GREEN]),
            (&[Rgb::RED, Rgb::BLUE][..], 5, vec![BLUE, RED, BLUE, RED, BLUE]),
        ] {
            let frame = wheel(style, true, arms, palette);
            let ring = frame.ring(40.0);
            let p = PinwheelParams {
                arms,
                style,
                ..defaults()
            };
            for (arm, color) in arms_of(&p, palette, 0).iter().zip(want) {
                assert_eq!(arm.color, color);
                assert_eq!(
                    run_near(&ring, arm.degrees).color,
                    color,
                    "{style:?}, {arms} arms"
                );
            }
        }
        // Two arms: green and blue, and red is nowhere.
        let frame = wheel(style, true, 2, &PALETTE);
        let mut seen: Vec<[f32; 3]> = runs(&frame.ring(40.0)).iter().map(|r| r.color).collect();
        seen.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(seen, [BLUE, GREEN]);
        assert!(frame.pixels().iter().all(|c| c.a == 0.0 || rgb(*c) != RED));
        // A black arm covers what is beneath it.
        let black = wheel(style, true, 1, &[Rgb::BLACK]);
        let run = only_run(&black.ring(40.0));
        assert_eq!(black.sample(40.0, run.center), Rgba::BLACK);
    }
}

/// Property 18: Colors ride with their arms.
#[test]
fn p18_colors_ride_with_their_arms() {
    let p = PinwheelParams {
        thickness: 30.0,
        ..defaults()
    };
    let ring = Frame::new(p, &PALETTE, RIG, 600).ring(40.0);
    for (degrees, color) in [(30.0, GREEN), (270.0, BLUE), (150.0, RED)] {
        let run = run_near(&ring, degrees);
        assert!(
            apart(run.center, degrees) <= 3.0 && run.color == color,
            "{run:?} at {degrees}"
        );
    }
}

/// Property 19: Shading profiles.
#[test]
fn p19_shading_profiles() {
    for (style, counterclockwise, center, bright) in [
        (SMOOTH, true, 270.0, 1.0),
        (SMOOTH, false, 270.0, -1.0),
        (SPOKES, true, 0.0, 1.0),
        (SPOKES, false, 0.0, 1.0),
    ] {
        let what = format!("{style:?}, counterclockwise {counterclockwise}");
        let wheel = |shading| {
            let p = PinwheelParams {
                arms: 2,
                thickness: 40.0,
                speed: 0.0,
                shading,
                counterclockwise,
                style,
                ..defaults()
            };
            Frame::new(p, &PALETTE, RIG, 0)
        };
        // The alpha on the ring at 40, `across` degrees counterclockwise of the arm's center
        // line (the arm is 72 degrees wide), which is always the first arm's green.
        let read = |frame: &Frame, across: f64| {
            let got = frame.sample(40.0, center + across);
            if got.a > 0.0 {
                assert_eq!(rgb(got), GREEN, "{what} at {across}");
            }
            f64::from(got.a)
        };
        let near = |got: f64, want: f64| (got - want).abs() <= 0.1;

        let flat = wheel(PinwheelShading::Flat);
        for k in -33..=33 {
            assert_eq!(read(&flat, f64::from(k)), 1.0, "{what}: flat at {k}");
        }

        let raised = wheel(PinwheelShading::Raised);
        assert!(read(&raised, 0.0) >= 0.9, "{what}");
        assert!(
            near(read(&raised, -18.0), 0.5) && near(read(&raised, 18.0), 0.5),
            "{what}"
        );
        assert!(
            read(&raised, -33.0) <= 0.2 && read(&raised, 33.0) <= 0.2,
            "{what}"
        );

        let sunken = wheel(PinwheelShading::Sunken);
        assert!(read(&sunken, 0.0) <= 0.1, "{what}");
        assert!(
            near(read(&sunken, -18.0), 0.5) && near(read(&sunken, 18.0), 0.5),
            "{what}"
        );
        assert!(
            read(&sunken, -33.0) >= 0.8 && read(&sunken, 33.0) >= 0.8,
            "{what}"
        );

        let sweep = wheel(PinwheelShading::Sweep);
        assert!(
            read(&sweep, bright * 33.0) >= 0.85,
            "{what}: {}",
            read(&sweep, bright * 33.0)
        );
        assert!(near(read(&sweep, 0.0), 0.5), "{what}");
        assert!(read(&sweep, bright * -33.0) <= 0.15, "{what}");
        // From there to the dark edge it only falls (the soft band at the bright edge itself
        // rises to full within a cell, as every edge does).
        let mut last = read(&sweep, bright * 33.0);
        for k in -65..=72 {
            let next = read(&sweep, bright * f64::from(-k) * 0.5);
            assert!(
                next <= last + 0.05,
                "{what}: the sweep rises from {last} to {next}"
            );
            last = next;
        }

        // The same all the way along the arm.
        if style == SMOOTH {
            for frame in [&flat, &raised, &sunken, &sweep] {
                for k in -11..=11 {
                    let degrees = center + f64::from(k) * 3.0;
                    let alphas = [15.0, 30.0, 45.0].map(|r| frame.sample(r, degrees).a);
                    let (low, high) = alphas
                        .iter()
                        .fold((f32::MAX, f32::MIN), |(low, high), &a| (low.min(a), high.max(a)));
                    assert!(high - low <= 0.1, "{what}: {alphas:?} along {degrees}");
                }
            }
        }
    }
}

/// Property 20: Mirror image (smooth): turning clockwise is the picture turning counterclockwise, flipped
/// left to right.
#[test]
fn p20_mirror_image() {
    for elapsed in [0, 150, 400] {
        let wheel = |counterclockwise| {
            let p = PinwheelParams {
                thickness: 30.0,
                counterclockwise,
                ..defaults()
            };
            Frame::new(p, &PALETTE, RIG, elapsed)
        };
        let (on, off) = (wheel(true), wheel(false));
        let mut differ = 0;
        for y in 0..RIG.rows {
            for x in 0..RIG.columns {
                let (a, b) = (on.pixel(RIG.columns - 1 - x, y), off.pixel(x, y));
                if lit(&a) != lit(&b) || (lit(&a) && rgb(a) != rgb(b)) {
                    differ += 1;
                }
            }
        }
        let share = f64::from(differ) / f64::from(RIG.columns * RIG.rows);
        assert!(share <= 0.04, "at {elapsed} ms, {differ} pixels differ");
    }
}

/// Property 21: Spokes: rays separate with distance. Out to about 57 cells, where a degree is a cell
/// wide, the rays overlap and the wheel is solid; beyond, they stand apart.
#[test]
fn p21_spokes_rays_separate_with_distance() {
    let p = PinwheelParams {
        arms: 4,
        thickness: 100.0,
        speed: 0.0,
        style: SPOKES,
        ..defaults()
    };
    let frame = Frame::new(p, &ONE, square(401), 0);
    let share = |r: f64| {
        let ring = frame.ring_of(r, 7200);
        ring.iter().filter(|c| lit(c)).count() as f64 / ring.len() as f64
    };
    assert!(share(40.0) >= 0.98, "{}", share(40.0));
    // Solid means solid: no pixel within 57 cells of the center is left out.
    for (r, c) in frame.pixels_by_distance() {
        assert!(r > 57.0 || c.a == 1.0, "a pixel {r} cells out has alpha {}", c.a);
    }
    assert!((0.40..=0.70).contains(&share(115.0)), "{}", share(115.0));
    assert!((0.22..=0.40).contains(&share(190.0)), "{}", share(190.0));
    let separate = runs(&frame.ring_of(150.0, 7200)).len();
    assert!((340..=380).contains(&separate), "{separate} runs");
}

/// Property 22: Spokes: the rays stay put while the wheel turns.
#[test]
fn p22_spokes_the_rays_stay_put() {
    let p = PinwheelParams {
        arms: 4,
        thickness: 100.0,
        speed: 1.0,
        style: SPOKES,
        ..defaults()
    };
    let canvas = square(401);
    let at = |elapsed| {
        let time = EffectTime::within(0, 10_000, elapsed).with_frame_ms(25);
        Frame::timed(p, &ONE, canvas, time, 0).pixels_by_distance()
    };
    let (before, after) = (at(0), at(125));
    let (mut same, mut all) = (0, 0);
    for ((r, a), (_, b)) in before.iter().zip(&after) {
        if (100.0..=190.0).contains(r) {
            all += 1;
            same += usize::from(lit(a) == lit(b));
        }
    }
    assert!(same as f64 >= 0.97 * all as f64, "{same} of {all} the same");
}

/// Property 23: A single row and a single column.
#[test]
fn p23_a_single_row_and_a_single_column() {
    let row = Canvas { columns: 41, rows: 1 };
    let column = Canvas { columns: 1, rows: 41 };
    // How many of the 41 pixels are lit at each sample time, every 50 ms from 0 to 850.
    let lit_counts = |style, canvas| -> Vec<usize> {
        (0..18)
            .map(|k| {
                let p = PinwheelParams {
                    arms: 2,
                    thickness: 20.0,
                    style,
                    ..defaults()
                };
                let frame = Frame::new(p, &ONE, canvas, k * 50);
                frame.pixels().iter().filter(|c| lit(c)).count()
            })
            .collect()
    };

    // Smooth, a row: the arms sweep over it around 450 ms.
    let counts = lit_counts(SMOOTH, row);
    let full: Vec<usize> = (0..18).filter(|&k| counts[k] == 41).collect();
    assert!((3..=5).contains(&full.len()), "{counts:?}");
    assert!(full.windows(2).all(|w| w[1] == w[0] + 1), "{counts:?}");
    let middle = (full[0] + full[full.len() - 1]) as f64 / 2.0 * 50.0;
    assert!((middle - 450.0).abs() <= 50.0, "{counts:?}");
    for (k, &count) in counts.iter().enumerate() {
        if k + 3 <= full[0] || k >= full[full.len() - 1] + 3 {
            assert!(count <= 5, "{count} lit at {} ms: {counts:?}", k * 50);
        }
    }

    // Smooth, a column: fully lit around 0 ms, and coming back at 900.
    let counts = lit_counts(SMOOTH, column);
    assert_eq!((counts[0], counts[1], counts[17]), (41, 41, 41), "{counts:?}");
    for (k, &count) in counts.iter().enumerate() {
        if (4..=14).contains(&k) {
            assert!(count <= 5, "{count} lit at {} ms: {counts:?}", k * 50);
        }
    }

    // Spokes start a quarter turn round from smooth.
    let counts = lit_counts(SPOKES, row);
    assert!(counts[0] >= 39 && counts[9] <= 4, "{counts:?}");
    let counts = lit_counts(SPOKES, column);
    assert!(counts[0] <= 4 && counts[9] >= 39, "{counts:?}");
}

/// Property 24: Tiny buffers and hostile input.
#[test]
fn p24_tiny_buffers_and_hostile_input() {
    let tiny = [(1, 1), (2, 1), (2, 2), (3, 3)].map(|(columns, rows)| Canvas { columns, rows });
    let sound = |c: Rgba| {
        [c.r, c.g, c.b, c.a]
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    };
    for canvas in tiny {
        for style in STYLES {
            for shading in SHADINGS {
                let hostile = PinwheelParams {
                    arms: 20,
                    arm_size: 400.0,
                    twist: -360.0,
                    thickness: 100.0,
                    speed: 50.0,
                    shading,
                    style,
                    ..defaults()
                };
                for elapsed in [0, 50, 3_600_000] {
                    let time = EffectTime::within(0, 7_200_000, elapsed).with_frame_ms(50);
                    let frame = Frame::timed(hostile, &PALETTE, canvas, time, 0);
                    assert!(frame.pixels().into_iter().all(sound));
                    // A pixel that is nowhere counts as being at 0.
                    for (u, v) in [
                        (f32::NAN, 0.5),
                        (0.5, f32::NAN),
                        (f32::NAN, f32::NAN),
                        (f32::INFINITY, f32::NEG_INFINITY),
                    ] {
                        assert!(sound(frame.at(u, v)));
                    }
                    assert_eq!(
                        bits(&frame.at(f32::NAN, f32::INFINITY)),
                        bits(&frame.at(0.0, 0.0))
                    );
                }
            }
        }
        let full = PinwheelParams {
            thickness: 100.0,
            ..defaults()
        };
        let frame = Frame::new(full, &PALETTE, canvas, 0);
        assert!(frame.pixels().iter().all(lit), "{canvas:?}");
    }
    // A single pixel is the center of the wheel: lit however short the arms, unless they
    // have no length at all.
    let one = Canvas { columns: 1, rows: 1 };
    for style in STYLES {
        for shading in SHADINGS {
            for arm_size in [0.5, 1.0, 100.0, 400.0] {
                let p = PinwheelParams {
                    arm_size,
                    shading,
                    style,
                    ..defaults()
                };
                assert!(
                    lit(&Frame::new(p, &ONE, one, 250).at(0.5, 0.5)),
                    "{style:?}, {shading:?}"
                );
            }
            let none = PinwheelParams {
                arm_size: 0.0,
                shading,
                style,
                ..defaults()
            };
            assert_eq!(Frame::new(none, &ONE, one, 250).at(0.5, 0.5), Rgba::CLEAR);
        }
    }
}

/// Property 25: Fractional settings and soft edges (new).
#[test]
fn p25_fractional_settings_and_soft_edges() {
    let wheel = |offset| {
        let p = PinwheelParams {
            arms: 1,
            thickness: 50.0,
            speed: 0.0,
            offset,
            ..defaults()
        };
        Frame::new(p, &ONE, RIG, 0)
    };
    // The arm is the lower half of the circle; its leading edge starts pointing right.
    let alphas: Vec<f32> = (0..=50)
        .map(|k| wheel(k as f32 * 0.1).sample(40.0, 2.5).a)
        .collect();
    assert_eq!((alphas[0], alphas[50]), (0.0, 1.0), "{alphas:?}");
    assert!(alphas.windows(2).all(|w| w[1] >= w[0] - 0.01), "{alphas:?}");
    let mut values: Vec<u32> = alphas.iter().map(|a| a.to_bits()).collect();
    values.sort_unstable();
    values.dedup();
    assert!(values.len() >= 5, "only {} values: {alphas:?}", values.len());

    // The partly lit band across an edge is never more than a cell and a half wide: across the
    // sides of arms near and far, straight and twisted, and across their ends.
    let partly = |a: f32| a > 0.0 && a < 1.0;
    for (arms, thickness, twist) in [(1, 50.0, 0.0), (4, 50.0, 0.0), (3, 40.0, 200.0), (7, 60.0, -90.0)] {
        for style in STYLES {
            let p = PinwheelParams {
                arms,
                thickness,
                twist,
                speed: 0.0,
                arm_size: 80.0,
                style,
                ..defaults()
            };
            let frame = Frame::new(p, &ONE, RIG, 0);
            let length = arm_length(&p, RIG);
            let what = format!("{style:?}, {arms} arms, thickness {thickness}, twist {twist}");
            // Across the sides, round rings: runs of partly lit samples, in cells of arc. A
            // twisted arm crosses a ring at a slant, so its band is that much longer along it.
            for r in [8.0, 20.0, 33.0] {
                let lean = f64::from(twist).to_radians() * r / length;
                let allowed = 1.5 * (1.0 + lean * lean).sqrt() + 0.05;
                let samples = 14_400;
                let ring = frame.ring_of(r, samples);
                let start = ring
                    .iter()
                    .position(|c| !partly(c.a))
                    .expect("not all partly lit");
                let mut run = 0;
                for k in 1..=samples {
                    if partly(ring[(start + k) % samples].a) {
                        run += 1;
                    } else {
                        let cells = f64::from(run) * std::f64::consts::TAU * r / samples as f64;
                        assert!(
                            cells <= allowed,
                            "{what}: a band {cells} cells wide {r} cells out"
                        );
                        run = 0;
                    }
                }
            }
            // Across the end, along lines out from the center, on the same wheel with its arms
            // touching so that the end is the only edge there.
            let frame = Frame::new(
                PinwheelParams {
                    thickness: 100.0,
                    ..p
                },
                &ONE,
                RIG,
                0,
            );
            for k in 0..360 {
                let degrees = f64::from(k) + 0.25;
                let band = (0..400)
                    .map(|step| length - 2.0 + f64::from(step) * 0.01)
                    .filter(|&r| frame.holds(r, degrees) && partly(frame.sample(r, degrees).a))
                    .fold(None, |band: Option<(f64, f64)>, r| match band {
                        None => Some((r, r)),
                        Some((from, _)) => Some((from, r)),
                    });
                if let Some((from, to)) = band {
                    assert!(to - from <= 1.5, "{what}: the end's band runs {from} to {to}");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Goals of the design that the acceptance properties leave unmeasured.

/// An edge crossing a pixel passes through the alphas in between, at the arm's sides and at its
/// end, and the light an arm sheds is what its shape says: nothing is lost or gained in the
/// soft edge.
#[test]
fn goal_edges_move_smoothly_and_keep_the_arms_light() {
    // The end of the arms moving over a pixel 30 cells out as the arm length grows.
    let alphas: Vec<f32> = (0..=40)
        .map(|k| {
            let p = PinwheelParams {
                thickness: 100.0,
                arm_size: 40.0 + k as f32 * 0.1,
                ..defaults()
            };
            Frame::new(p, &ONE, RIG, 0).sample(30.0, 77.0).a
        })
        .collect();
    assert_eq!((alphas[0], alphas[40]), (0.0, 1.0), "{alphas:?}");
    assert!(alphas.windows(2).all(|w| w[1] >= w[0]), "{alphas:?}");
    assert!(
        alphas.iter().filter(|a| **a > 0.0 && **a < 1.0).count() >= 3,
        "{alphas:?}"
    );

    // The whole disc's light is its area, to within a hundredth.
    for arm_size in [20.0, 33.3, 50.0] {
        let p = PinwheelParams {
            thickness: 100.0,
            arm_size,
            ..defaults()
        };
        let light: f64 = Frame::new(p, &ONE, RIG, 0)
            .pixels()
            .iter()
            .map(|c| f64::from(c.a))
            .sum();
        let area = std::f64::consts::PI * arm_length(&p, RIG).powi(2);
        assert!(
            (light / area - 1.0).abs() <= 0.01,
            "arm length {arm_size}: {light} of {area}"
        );
    }
    // And a wide arm's light is its share of the disc.
    let p = PinwheelParams {
        arms: 5,
        thickness: 37.25,
        arm_size: 60.0,
        speed: 0.0,
        ..defaults()
    };
    let light: f64 = Frame::new(p, &ONE, RIG, 0)
        .pixels()
        .iter()
        .map(|c| f64::from(c.a))
        .sum();
    let area = std::f64::consts::PI * arm_length(&p, RIG).powi(2) * 0.3725;
    assert!((light / area - 1.0).abs() <= 0.02, "{light} of {area}");
}

/// No light is spilled: a one-cell line lights one pixel across where it runs through pixels,
/// two where it runs between them, never three, and nothing beside it is lit at all.
#[test]
fn goal_a_one_cell_line_stays_one_cell_wide() {
    let p = PinwheelParams {
        arms: 12,
        speed: 0.0,
        ..defaults()
    };
    // An odd number of columns: the first arm runs straight down the middle column.
    let odd = Frame::new(p, &ONE, RIG, 0);
    for y in 0..=40 {
        let across: Vec<f32> = (47..=53).map(|x| odd.pixel(x, y).a).collect();
        assert_eq!(across, [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0], "row {y}");
    }
    // An even number: it runs between the two middle columns, and they share it.
    let even = Frame::new(p, &ONE, square(100), 0);
    for y in 0..=40 {
        let across: Vec<f32> = (46..=53).map(|x| even.pixel(x, y).a).collect();
        assert!(across[3] > 0.0 && across[4] > 0.0, "row {y}: {across:?}");
        assert!((across[3] + across[4] - 1.0).abs() <= 0.02, "row {y}: {across:?}");
        assert!(
            across[..3].iter().chain(&across[5..]).all(|a| *a == 0.0),
            "row {y}: {across:?}"
        );
    }
    // At any angle, a pixel more than five eighths of a cell from every arm's center line is not
    // lit at all: half the line's width, and half its soft band, which is a quarter of that width.
    let turned = Frame::new(PinwheelParams { offset: 17.3, ..p }, &ONE, RIG, 0);
    let arms = arms_of(&PinwheelParams { offset: 17.3, ..p }, &ONE, 0);
    for y in 0..RIG.rows {
        for x in 0..RIG.columns {
            let (dx, dy) = (f64::from(x) - 50.0, f64::from(y) - 50.0);
            let (r, degrees) = (dx.hypot(dy), dy.atan2(dx).to_degrees());
            let nearest = arms
                .iter()
                .map(|arm| apart(arm.degrees, degrees))
                .fold(f64::MAX, f64::min);
            let away = r * nearest.min(90.0).to_radians().sin();
            if away > 0.63 {
                assert_eq!(
                    turned.pixel(x, y).a,
                    0.0,
                    "({x}, {y}) is {away} cells from an arm"
                );
            }
        }
    }
}

/// Each pixel is judged at its own place, so the wheel is the same shape whatever the buffer's
/// resolution: the same place on the wheel gets the same answer on buffers of 51, 101, and 201
/// cells, once it is clear of the edges' soft bands and of the middle (where arms are kept a
/// cell wide, and a cell is a different share of each buffer).
#[test]
fn goal_the_same_shape_at_any_resolution() {
    let p = PinwheelParams {
        arms: 5,
        thickness: 45.0,
        offset: 7.7,
        speed: 0.0,
        ..defaults()
    };
    let frames = [51, 101, 201].map(|cells| Frame::new(p, &PALETTE, square(cells), 0));
    let (mut agreed, mut lit_places) = (0, 0);
    for k in 0..4000 {
        // Places spread over the buffer, as shares of half its width from the middle.
        let (across, up) = (
            f64::from(k % 61) / 30.5 - 1.0 + 0.003,
            f64::from(k % 67) / 33.5 - 1.0 + 0.007,
        );
        if across.hypot(up) < 0.1 {
            continue;
        }
        let seen = frames
            .each_ref()
            .map(|frame| frame.at((0.5 + across / 2.0) as f32, (0.5 + up / 2.0) as f32));
        if seen.iter().all(|c| c.a == 0.0 || c.a == 1.0) {
            assert!(
                seen.windows(2).all(|w| w[0] == w[1]),
                "at ({across}, {up}): {seen:?}"
            );
            agreed += 1;
            lit_places += usize::from(seen[0].a == 1.0);
        }
    }
    assert!(
        agreed > 3000 && lit_places > 1000,
        "{agreed} places clear of an edge, {lit_places} lit"
    );
}

/// Fractions are honoured in every number setting.
#[test]
fn goal_fractions_are_honoured() {
    let fine = |p: PinwheelParams, elapsed: u64, r: f64| {
        let ring = Frame::new(p, &ONE, RIG, elapsed).ring_of(r, 7200);
        only_run(&ring)
    };
    let base = PinwheelParams {
        arms: 1,
        thickness: 10.0,
        speed: 0.0,
        ..defaults()
    };
    let still = fine(base, 0, 40.0);

    // Start angle 10.5 turns the wheel ten and a half degrees.
    let turned = fine(PinwheelParams { offset: 10.5, ..base }, 0, 40.0);
    assert!(
        (ahead(turned.center, still.center) - 10.5).abs() <= 0.15,
        "{turned:?}"
    );

    // Speed 3.8 is 76 degrees a second.
    let spun = fine(PinwheelParams { speed: 3.8, ..base }, 1000, 40.0);
    assert!(
        (ahead(spun.center, still.center) - 76.0).abs() <= 0.15,
        "{spun:?}"
    );

    // Twist 10.9 turns the arm 10.9 degrees at its end: 6.1 degrees 40 cells out.
    let twisted = fine(PinwheelParams { twist: 10.9, ..base }, 0, 40.0);
    let want = 10.9 * 40.0 / arm_length(&base, RIG);
    assert!(
        (ahead(twisted.center, still.center) - want).abs() <= 0.15,
        "{twisted:?}"
    );

    // Thickness 37.25 of a full circle is 134.1 degrees.
    let wide = fine(
        PinwheelParams {
            thickness: 37.25,
            ..base
        },
        0,
        40.0,
    );
    assert!((wide.width - 134.1).abs() <= 0.2, "{wide:?}");

    // Arm length 37.3 ends the arms 26.64 cells out.
    let p = PinwheelParams {
        thickness: 100.0,
        arm_size: 37.3,
        ..base
    };
    let frame = Frame::new(p, &ONE, RIG, 0);
    let end = (0..4000)
        .map(|k| 20.0 + f64::from(k) * 0.0025)
        .find(|&r| !lit(&frame.sample(r, 33.0)))
        .unwrap();
    assert!(
        (end - arm_length(&p, RIG)).abs() <= 0.02,
        "the arms end {end} cells out"
    );

    // Center 12.7 puts the center 12.7% of the way from the middle to the right edge: 6.41 of
    // the 50.5 cells there are, and -3.3 puts it 1.67 cells below the middle.
    let p = PinwheelParams {
        thickness: 100.0,
        arm_size: 20.0,
        center_x: 12.7,
        center_y: -3.3,
        ..base
    };
    let frame = Frame::new(p, &ONE, RIG, 0);
    let (mut light, mut across, mut up) = (0.0, 0.0, 0.0);
    for y in 0..RIG.rows {
        for x in 0..RIG.columns {
            let a = f64::from(frame.pixel(x, y).a);
            light += a;
            across += a * f64::from(x);
            up += a * f64::from(y);
        }
    }
    let (across, up) = (across / light, up / light);
    assert!(
        (across - 56.4135).abs() <= 0.05 && (up - 48.3335).abs() <= 0.05,
        "({across}, {up})"
    );
}

/// The center settings run over the buffer's whole extent, edge to edge, as xLights' picture
/// has them: on a buffer of `n` columns a setting of `c` puts the center `n * (0.5 + c / 200)`
/// cells from the left edge, which is half a cell less than that from the first column's own
/// place. (A wheel a quarter of the way from the middle to the right edge of a 400-column
/// buffer is centered on the line between columns 249 and 250.)
#[test]
fn goal_the_center_is_placed_by_the_buffers_whole_extent() {
    let buffers = [
        RIG,
        Canvas {
            columns: 100,
            rows: 60,
        },
        Canvas {
            columns: 400,
            rows: 178,
        },
    ];
    for style in STYLES {
        for canvas in buffers {
            for (center_x, center_y) in [(0.0, 0.0), (25.0, 0.0), (50.0, -50.0), (-61.5, 33.3)] {
                // A disc 10 cells in radius, all of it on the buffer.
                let p = PinwheelParams {
                    arms: 1,
                    thickness: 100.0,
                    arm_size: 100.0,
                    center_x,
                    center_y,
                    style,
                    ..defaults()
                };
                let arm_size = (1000.0 / arm_length(&p, canvas)) as f32;
                let frame = Frame::new(PinwheelParams { arm_size, ..p }, &ONE, canvas, 0);
                let (mut light, mut across, mut up) = (0.0, 0.0, 0.0);
                for y in 0..canvas.rows {
                    for x in 0..canvas.columns {
                        let a = f64::from(frame.pixel(x, y).a);
                        light += a;
                        across += a * f64::from(x);
                        up += a * f64::from(y);
                    }
                }
                assert!(light > 250.0, "a disc of {light} pixels");
                let (across, up) = (across / light, up / light);
                let want = (
                    f64::from(canvas.columns) * (0.5 + f64::from(center_x) / 200.0) - 0.5,
                    f64::from(canvas.rows) * (0.5 + f64::from(center_y) / 200.0) - 0.5,
                );
                assert!(
                    (across - want.0).abs() <= 0.05 && (up - want.1).abs() <= 0.05,
                    "{style:?} on {canvas:?} at ({center_x}, {center_y}): the disc is about \
                     ({across}, {up}), not {want:?}"
                );
            }
        }
    }
}

/// On a buffer one row high the setting for the center's height does nothing, and on one a
/// single column wide the setting across does nothing (sections 5.7 and 6): the row is a line
/// through the center whatever that setting says, so an arm lying along it lights all of it at
/// full strength, and every frame is the frame the setting at 0 gives, bit for bit.
#[test]
fn goal_the_center_setting_does_nothing_on_a_dimension_one_cell_wide() {
    let row = Canvas {
        columns: 200,
        rows: 1,
    };
    let column = Canvas {
        columns: 1,
        rows: 200,
    };
    // The setting for the dimension the buffer does not have.
    let moved = |p: PinwheelParams, canvas: Canvas, to: f32| {
        if canvas.rows == 1 {
            PinwheelParams { center_y: to, ..p }
        } else {
            PinwheelParams { center_x: to, ..p }
        }
    };
    for style in STYLES {
        for canvas in [row, column] {
            // Four arms lie along the row and along the column: thin ones, and wide ones.
            for thickness in [0.0, 20.0] {
                let p = PinwheelParams {
                    arms: 4,
                    thickness,
                    speed: 0.0,
                    style,
                    ..defaults()
                };
                for to in [0.0, 100.0, -100.0, 37.5] {
                    let frame = Frame::new(moved(p, canvas, to), &ONE, canvas, 0).pixels();
                    let full = frame.iter().filter(|c| c.a == 1.0).count();
                    assert_eq!(
                        full, 200,
                        "{style:?}, thickness {thickness}, {canvas:?}, the missing dimension's center at {to}"
                    );
                }
            }
            // A wheel off center along the line, twisted, shaded and turning.
            let p = PinwheelParams {
                arms: 3,
                thickness: 30.0,
                twist: 200.0,
                speed: 7.0,
                shading: PinwheelShading::Sweep,
                center_x: 31.0,
                center_y: 31.0,
                style,
                ..defaults()
            };
            for elapsed in [0, 350, 4200] {
                let at_0 = all_bits(&Frame::new(moved(p, canvas, 0.0), &PALETTE, canvas, elapsed).pixels());
                assert!(at_0.iter().any(|c| c[3] != 0), "something is drawn");
                for to in [100.0, -100.0, 12.5] {
                    let there =
                        all_bits(&Frame::new(moved(p, canvas, to), &PALETTE, canvas, elapsed).pixels());
                    assert_eq!(
                        there, at_0,
                        "{style:?}, {canvas:?} at {elapsed} ms, the missing dimension's center at {to}"
                    );
                }
            }
        }
    }
}

/// Arms are evenly spaced, and as wide as each other, for every count, to a fraction of a degree.
/// (Spokes 40 cells out are their rays: up to a degree wider than their angle, and the cells the
/// rays' marks fall in, which reach half a cell to 0.71 of one either side.)
#[test]
fn goal_every_arm_count_is_evenly_spaced() {
    let cell = (1.0f64 / 40.0).to_degrees();
    for (style, wider, allowed) in [(SMOOTH, 0.0, 0.2), (SPOKES, 1.2 * cell + 0.5, 0.85)] {
        for arms in 1..=20 {
            let p = PinwheelParams {
                arms,
                thickness: 20.0,
                speed: 0.0,
                style,
                ..defaults()
            };
            let ring = Frame::new(p, &ONE, RIG, 0).ring_of(40.0, 7200);
            assert_eq!(runs(&ring).len(), arms as usize);
            let wide = 0.2 * 360.0 / f64::from(arms) + wider;
            for arm in arms_of(&p, &ONE, 0) {
                let run = run_near(&ring, arm.degrees);
                assert!(
                    apart(run.center, arm.degrees) <= allowed && (run.width - wide).abs() <= allowed,
                    "{style:?}, {arms} arms: {run:?} for the arm at {}",
                    arm.degrees
                );
            }
        }
    }
}

/// The center is the true middle and the picture is exactly as symmetric as its pixels: on a
/// buffer whose pixel places are exact (65 by 65, steps of 1/64), turning it a quarter turn or
/// flipping it gives the very same bits.
#[test]
fn goal_true_center_and_exact_symmetry() {
    let canvas = square(65);
    let draw = |p: PinwheelParams, palette: &[Rgb], elapsed| {
        all_bits(&Frame::new(p, palette, canvas, elapsed).pixels())
    };
    // The pixel in column `x` of row `y` of a frame drawn that way.
    let at = |frame: &[[u32; 4]], x: usize, y: usize| frame[y * 65 + x];
    for shading in SHADINGS {
        for style in STYLES {
            // Arm counts that are multiples of four repeat every quarter turn, wherever the
            // wheel has turned to and however it is twisted. (For spokes that is so where they
            // are solid, which arms 26 cells long are all the way: further out a ray's marks
            // light cells, and a mark on the line between two cells lights the upper one, which
            // a quarter turn makes another.)
            for arms in [4, 8, 12, 16, 20] {
                let p = PinwheelParams {
                    arms,
                    arm_size: if style == SPOKES { 80.0 } else { 100.0 },
                    thickness: 30.0,
                    twist: 45.0,
                    offset: 12.3,
                    shading,
                    style,
                    ..defaults()
                };
                let frame = draw(p, &ONE, 350);
                assert!(frame.iter().any(|c| c[3] != 0));
                for y in 0..65 {
                    for x in 0..65 {
                        // (x, y) a quarter turn counterclockwise about the middle.
                        assert_eq!(
                            at(&frame, x, y),
                            at(&frame, 64 - y, x),
                            "{style:?}, {shading:?}, {arms} arms at ({x}, {y})"
                        );
                    }
                }
            }
        }
        // A smooth wheel at its start is its own reflection, left to right, for any arm count
        // (a sweep has a bright side, so it is the wheel turning the other way that mirrors it).
        for arms in [1, 2, 3, 5, 7, 12, 19] {
            let p = PinwheelParams {
                arms,
                thickness: 30.0,
                speed: 0.0,
                shading,
                ..defaults()
            };
            let on = draw(p, &ONE, 0);
            let other = PinwheelParams {
                counterclockwise: false,
                ..p
            };
            let off = draw(other, &ONE, 0);
            for y in 0..65 {
                for x in 0..65 {
                    assert_eq!(
                        at(&on, x, y),
                        at(&off, 64 - x, y),
                        "{shading:?}, {arms} arms at ({x}, {y})"
                    );
                    if shading != PinwheelShading::Sweep {
                        assert_eq!(
                            at(&on, x, y),
                            at(&on, 64 - x, y),
                            "{shading:?}, {arms} arms at ({x}, {y})"
                        );
                    }
                }
            }
        }
    }
    // Turning clockwise is the exact reflection at every moment, colors and all.
    for elapsed in [0, 150, 400, 7350] {
        let p = PinwheelParams {
            arms: 5,
            thickness: 40.0,
            offset: 33.3,
            speed: 7.5,
            shading: PinwheelShading::Sweep,
            ..defaults()
        };
        let on = draw(p, &PALETTE, elapsed);
        let other = PinwheelParams {
            counterclockwise: false,
            ..p
        };
        let off = draw(other, &PALETTE, elapsed);
        for y in 0..65 {
            for x in 0..65 {
                assert_eq!(at(&on, x, y), at(&off, 64 - x, y), "at {elapsed} ms, ({x}, {y})");
            }
        }
    }
    // On a buffer with an even number of cells the middle falls between pixels, and the wheel is
    // centered there all the same.
    for cells in [64, 65, 100, 101] {
        let p = PinwheelParams {
            arms: 4,
            thickness: 50.0,
            speed: 0.0,
            arm_size: 60.0,
            ..defaults()
        };
        let frame = Frame::new(p, &ONE, square(cells), 0);
        let (mut light, mut across, mut up) = (0.0, 0.0, 0.0);
        for y in 0..cells {
            for x in 0..cells {
                let a = f64::from(frame.pixel(x, y).a);
                light += a;
                across += a * f64::from(x);
                up += a * f64::from(y);
                let opposite = frame.pixel(cells - 1 - x, cells - 1 - y).a;
                assert!((opposite - a as f32).abs() <= 1e-3, "{cells} cells at ({x}, {y})");
            }
        }
        let middle = f64::from(cells - 1) / 2.0;
        assert!(
            (across / light - middle).abs() <= 1e-3,
            "{cells} cells: {}",
            across / light
        );
        assert!(
            (up / light - middle).abs() <= 1e-3,
            "{cells} cells: {}",
            up / light
        );
    }
}

/// Hours into an effect the wheel is exactly where the clock puts it.
#[test]
fn goal_stable_over_long_effects() {
    let hours = |elapsed_ms| EffectTime::within(0, 36_000_000, elapsed_ms).with_frame_ms(50);
    let p = PinwheelParams {
        arms: 7,
        thickness: 3.0,
        twist: 200.0,
        speed: 18.0,
        ..defaults()
    };
    // Speed 18 is a turn a second: five hours on, to the millisecond, it is where it started.
    let start = Frame::timed(p, &PALETTE, RIG, hours(0), 0).pixels();
    let later = Frame::timed(p, &PALETTE, RIG, hours(18_000_000), 0).pixels();
    assert_eq!(all_bits(&start), all_bits(&later));
    // And one frame after that it has moved exactly as far as one frame after the start.
    let p = PinwheelParams { speed: 7.0, ..p };
    let start = Frame::timed(p, &PALETTE, RIG, hours(50), 0).pixels();
    let later = Frame::timed(p, &PALETTE, RIG, hours(18_000_050), 0).pixels();
    for (a, b) in start.iter().zip(&later) {
        assert!((a.a - b.a).abs() <= 1e-4);
        assert!(a.a.min(b.a) < 0.01 || rgb(*a) == rgb(*b));
    }
}

/// The wheel turns by the clock, not by whole frames: half way between two frames it is half
/// way between where they show it.
#[test]
fn goal_time_is_used_as_given() {
    let p = PinwheelParams {
        arms: 1,
        thickness: 10.0,
        ..defaults()
    };
    let center = |elapsed| only_run(&Frame::new(p, &ONE, RIG, elapsed).ring_of(40.0, 7200)).center;
    let (first, between, second) = (center(0), center(25), center(50));
    assert!(
        (ahead(between, first) - 5.0).abs() <= 0.1,
        "{first}, {between}, {second}"
    );
    assert!(
        (ahead(second, first) - 10.0).abs() <= 0.1,
        "{first}, {between}, {second}"
    );
}

/// The arctangent that finds a pixel's direction is good to a hundredth of a cell a thousand
/// cells from the center: an arm's edge, and every ray, sits where the settings put it all the
/// way round.
#[test]
fn goal_directions_are_true_all_the_way_round() {
    let canvas = square(2001);
    let lean = (0.01_f64 / 990.0).to_degrees();
    let p = PinwheelParams {
        arms: 1,
        thickness: 50.0,
        speed: 0.0,
        arm_size: 200.0,
        ..defaults()
    };
    for k in 0..3600 {
        // The arm's counterclockwise edge, turned to every tenth of a degree in turn.
        let offset = k as f32 * 0.1;
        let frame = Frame::new(PinwheelParams { offset, ..p }, &ONE, canvas, 0);
        let edge = f64::from(offset);
        let (inside, outside) = (frame.sample(990.0, edge - lean), frame.sample(990.0, edge + lean));
        assert!(
            inside.a > 0.5 && outside.a < 0.5,
            "at {edge}: {} and {}",
            inside.a,
            outside.a
        );
    }
    // Every ray sits on its whole degree: a sample just under half a cell to either side of it
    // has one of its marks in its cell whichever way the ray points, one more than 0.71 of a cell
    // away never has, and along a row or a column the line between is at half a cell exactly.
    let rays = PinwheelParams {
        thickness: 100.0,
        style: SPOKES,
        ..p
    };
    let frame = Frame::new(rays, &ONE, canvas, 0);
    let turn = |cells: f64| (cells / 990.0).to_degrees();
    for ray in 0..360 {
        for side in [-1.0, 1.0] {
            let at = |cells: f64| frame.sample(990.0, f64::from(ray) + side * turn(cells));
            assert!(
                lit(&at(0.49)) && !lit(&at(0.72)),
                "ray {ray}: {} and {}",
                at(0.49).a,
                at(0.72).a
            );
            if ray % 90 == 0 {
                assert!(!lit(&at(0.51)), "ray {ray}: {}", at(0.51).a);
            }
        }
    }
}

/// Spokes glide where the wheel is solid: close to the center an arm's edge crosses a pixel
/// through the alphas in between, as a smooth arm's does, instead of stepping a degree at a time.
#[test]
fn goal_spokes_glide_near_the_center() {
    let wheel = |offset| {
        let p = PinwheelParams {
            arms: 1,
            thickness: 50.0,
            speed: 0.0,
            offset,
            style: SPOKES,
            ..defaults()
        };
        Frame::new(p, &ONE, RIG, 0)
    };
    // The arm is the right half of the circle and half a cell more (a ray's reach) each side:
    // 12 cells out, its counterclockwise edge starts at 92.4 degrees.
    let edge = 90.0 + (0.5f64 / 12.0).to_degrees();
    let alphas: Vec<f32> = (0..=50)
        .map(|k| wheel(k as f32 * 0.1).sample(12.0, edge + 2.5).a)
        .collect();
    assert_eq!((alphas[0], alphas[50]), (0.0, 1.0), "{alphas:?}");
    assert!(alphas.windows(2).all(|w| w[1] >= w[0] - 0.01), "{alphas:?}");
    let mut values: Vec<u32> = alphas.iter().map(|a| a.to_bits()).collect();
    values.sort_unstable();
    values.dedup();
    assert!(values.len() >= 20, "only {} values: {alphas:?}", values.len());
}

/// Far from the center a thin spokes arm is the one ray nearest its center line: the cells its
/// marks fall in, on a whole degree, and staying on it until the arm's center line is nearer the
/// next.
#[test]
fn goal_a_thin_spoke_is_the_nearest_ray() {
    let canvas = square(401);
    for thickness in [0.0, 1.0] {
        let ring = |offset: f32| {
            let p = PinwheelParams {
                arms: 12,
                thickness,
                speed: 0.0,
                offset,
                style: SPOKES,
                ..defaults()
            };
            Frame::new(p, &ONE, canvas, 0).ring_of(150.0, 14_400)
        };
        let cell = (1.0f64 / 150.0).to_degrees();
        for (offset, ray) in [(0.3, 0.0), (0.4, 0.0), (0.6, 1.0), (1.45, 1.0), (359.7, 0.0)] {
            let ring = ring(offset);
            assert_eq!(runs(&ring).len(), 12, "start angle {offset}");
            for k in 0..12 {
                let want = ray + 30.0 * f64::from(k);
                let run = run_near(&ring, want);
                assert!(
                    apart(run.center, want) <= 0.03
                        && run.width >= cell - 0.06
                        && run.width <= 1.42 * cell + 0.06,
                    "start angle {offset}: {run:?}, not a ray at {want}"
                );
            }
        }
    }
}

/// Far from the center an arm lights as many rays as it is whole degrees wide, and one more,
/// wherever it sits: also when its edges fall exactly on rays, where the ray on its
/// counterclockwise edge is lit and the one on its clockwise edge is not.
#[test]
fn goal_an_arm_lights_its_width_in_rays_and_one_more() {
    let canvas = square(401);
    for (arms, thickness, rays) in [
        (10, 20.0, 8),
        (6, 50.0, 31),
        (4, 10.0, 10),
        (7, 30.0, 16),
        (12, 0.0, 1),
    ] {
        for offset in [0.0, 0.25, 0.5, 0.77, 3.0] {
            let p = PinwheelParams {
                arms,
                thickness,
                speed: 0.0,
                offset,
                style: SPOKES,
                ..defaults()
            };
            let found = runs(&Frame::new(p, &ONE, canvas, 0).ring_of(150.0, 14_400));
            assert_eq!(
                found.len(),
                (arms * rays) as usize,
                "{arms} arms, thickness {thickness}, start angle {offset}"
            );
            // Each is a string of marks on a whole degree: a sample's cell holds one when the
            // sample is within half a cell of the ray going along a row or a column, and up to
            // 0.71 of a cell on a diagonal.
            let cell = (1.0f64 / 150.0).to_degrees();
            for run in found {
                assert!(
                    run.width >= cell - 0.06 && run.width <= 1.42 * cell + 0.06,
                    "{run:?}"
                );
                let ray = run.center.round();
                assert!(apart(run.center, ray) <= 0.03, "{run:?} is not on a whole degree");
            }
        }
    }
}

/// A shaded arm drawn as rays is shaded ray by ray, counting from its clockwise edge: the k-th of
/// m rays takes the shading k/m of the way across. So a swept arm 30 degrees wide is 31 rays at
/// 0, 1/31, … 30/31: its first ray is dark and its last falls one step short of full (as
/// xLights' reference renders of the corpus show it). An arm of one ray is at full strength.
#[test]
fn goal_shading_runs_over_an_arms_rays() {
    let wheel = |shading, thickness| {
        let p = PinwheelParams {
            arms: 6,
            thickness,
            speed: 0.0,
            shading,
            style: SPOKES,
            ..defaults()
        };
        Frame::new(p, &PALETTE, square(401), 0)
    };
    // The first arm is centered on 0 degrees: its rays are at -15 to 15.
    let ray = |frame: &Frame, k: i32| {
        let got = frame.sample(150.0, f64::from(k - 15));
        assert!(got.a == 0.0 || rgb(got) == GREEN);
        f64::from(got.a)
    };
    let (sweep, raised, sunken) = (
        wheel(PinwheelShading::Sweep, 50.0),
        wheel(PinwheelShading::Raised, 50.0),
        wheel(PinwheelShading::Sunken, 50.0),
    );
    for k in 0..=30 {
        let across = f64::from(k) / 31.0;
        let near = |got: f64, want: f64| (got - want).abs() <= 0.005;
        assert!(near(ray(&sweep, k), across), "sweep, ray {k}: {}", ray(&sweep, k));
        let peak = 1.0 - (2.0 * across - 1.0).abs();
        assert!(
            near(ray(&raised, k), peak),
            "raised, ray {k}: {}",
            ray(&raised, k)
        );
        assert!(
            near(ray(&sunken, k), 1.0 - peak),
            "sunken, ray {k}: {}",
            ray(&sunken, k)
        );
    }
    assert_eq!(ray(&sweep, 0), 0.0, "the clockwise ray of a sweep is dark");
    for frame in [&sweep, &raised, &sunken] {
        assert_eq!(ray(frame, -1), 0.0);
        assert_eq!(ray(frame, 31), 0.0);
        assert_eq!(frame.sample(150.0, 0.5).a, 0.0, "between two rays");
    }
    // Arms of a single ray (here 0.6 of a degree wide, and none at all) are at full strength.
    for shading in SHADINGS {
        for thickness in [0.0, 1.0] {
            assert_eq!(
                wheel(shading, thickness).sample(150.0, 0.0).a,
                1.0,
                "{shading:?}, {thickness}"
            );
        }
    }
}
