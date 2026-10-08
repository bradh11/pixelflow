//! The effects drawn shape by shape on the target's grid, as xLights draws them: Shape, Fan,
//! Morph, and Circles.

use pf_render::{Canvas, Colors, EffectTime, Pixel, Rgba, Shade, Shader, Shape};
use pf_sequence::*;

const SEED: u64 = 0xC0FFEE;
const GRID: Canvas = Canvas {
    columns: 31,
    rows: 31,
};

/// Time `elapsed_ms` into an effect lasting `length_ms`.
fn time(elapsed_ms: u64, length_ms: u64) -> EffectTime {
    EffectTime::within(1000, 1000 + length_ms, 1000 + elapsed_ms)
}

/// The pixel at grid cell (x, y).
fn cell(x: u32, y: u32, canvas: Canvas) -> Pixel {
    Pixel {
        u: x as f32 / (canvas.columns - 1) as f32,
        v: y as f32 / (canvas.rows - 1) as f32,
        index: y * canvas.columns + x,
        count: canvas.columns * canvas.rows,
    }
}

/// Every cell's color, bottom row first.
fn frame(shader: &Shader, canvas: Canvas) -> Vec<Rgba> {
    (0..canvas.rows)
        .flat_map(|y| (0..canvas.columns).map(move |x| (x, y)))
        .map(|(x, y)| shader.shade(&cell(x, y, canvas)))
        .collect()
}

fn draw(params: &EffectParams, t: EffectTime, palette: &[Rgb], seed: u64, canvas: Canvas) -> Vec<Rgba> {
    frame(
        &Shader::new(params, &t, Colors::new(palette), seed, canvas),
        canvas,
    )
}

/// The lit cells, as (x, y).
fn lit(cells: &[Rgba], canvas: Canvas) -> Vec<(u32, u32)> {
    cells
        .iter()
        .enumerate()
        .filter(|(_, c)| c.a > 0.0)
        .map(|(i, _)| (i as u32 % canvas.columns, i as u32 / canvas.columns))
        .collect()
}

fn at(cells: &[Rgba], x: u32, y: u32, canvas: Canvas) -> Rgba {
    cells[(y * canvas.columns + x) as usize]
}

// ---------------------------------------------------------------------------------------------
// Fan

fn fan(p: FanParams) -> EffectParams {
    EffectParams::Fan(p)
}

#[test]
fn fan_blades_fill_their_share_of_the_ring() {
    // Full-length blades, as wide as their slice: the whole ring lights.
    let full = fan(FanParams {
        duration: 100.0,
        blade_width: 100.0,
        blend_edges: false,
        ..FanParams::default()
    });
    let cells = draw(&full, time(500, 1000), &[Rgb::RED], SEED, GRID);
    // The center is cell (15, 15); the outer radius 50 reaches 31 × 50 / 200 = 7.75 cells.
    for (x, y) in lit(&cells, GRID) {
        let r = (f64::from(x) - 15.0).hypot(f64::from(y) - 15.0);
        assert!(r <= 7.75, "({x}, {y}) is {r} out");
    }
    let disc = (0..31u32)
        .flat_map(|y| (0..31u32).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let r = (f64::from(x) - 15.0).hypot(f64::from(y) - 15.0);
            (31.0 / 200.0..=7.75).contains(&r)
        })
        .count();
    assert_eq!(lit(&cells, GRID).len(), disc, "every cell in the ring");
    assert_eq!(at(&cells, 15, 15, GRID).a, 0.0, "inside the inner radius");

    // Half-width blades light about half of it.
    let half = draw(
        &fan(FanParams {
            duration: 100.0,
            ..FanParams::default()
        }),
        time(500, 1000),
        &[Rgb::RED],
        SEED,
        GRID,
    );
    let share = lit(&half, GRID).len() as f64 / disc as f64;
    assert!((0.35..0.65).contains(&share), "{share}");
}

#[test]
fn fan_blades_grow_turn_and_shrink() {
    let p = fan(FanParams::default());
    // At the start the blades haven't grown yet; by the middle they're at full length.
    let start = lit(&draw(&p, time(0, 1000), &[Rgb::RED], SEED, GRID), GRID).len();
    let middle = lit(&draw(&p, time(500, 1000), &[Rgb::RED], SEED, GRID), GRID).len();
    assert!(start < 3 && middle > 50, "{start} then {middle}");
    // They turn: two moments in the middle differ, unless the fan doesn't turn.
    let a = draw(&p, time(400, 1000), &[Rgb::RED], SEED, GRID);
    let b = draw(&p, time(450, 1000), &[Rgb::RED], SEED, GRID);
    assert_ne!(lit(&a, GRID), lit(&b, GRID));
    let still = fan(FanParams {
        revolutions: 0.0,
        ..FanParams::default()
    });
    assert_eq!(
        lit(&draw(&still, time(400, 1000), &[Rgb::RED], SEED, GRID), GRID),
        lit(&draw(&still, time(450, 1000), &[Rgb::RED], SEED, GRID), GRID)
    );
    // Reversed, they turn the other way: the same picture mirrored in time isn't the same.
    let reverse = fan(FanParams {
        direction: Direction::Reverse,
        ..FanParams::default()
    });
    assert_ne!(
        lit(&draw(&reverse, time(450, 1000), &[Rgb::RED], SEED, GRID), GRID),
        lit(&b, GRID)
    );
}

#[test]
fn fan_colors_sit_side_by_side_and_edges_soften() {
    let p = fan(FanParams {
        duration: 100.0,
        ..FanParams::default()
    });
    let cells = draw(&p, time(500, 1000), &[Rgb::RED, Rgb::BLUE], SEED, GRID);
    let reds = cells.iter().filter(|c| c.a > 0.0 && c.r > 0.5).count();
    let blues = cells.iter().filter(|c| c.a > 0.0 && c.b > 0.5).count();
    assert!(reds > 10 && blues > 10, "{reds} red, {blues} blue");
    // Soft edges: some cells partly covered; hard edges: all fully.
    assert!(cells.iter().any(|c| c.a > 0.0 && c.a < 0.9));
    let hard = draw(
        &fan(FanParams {
            duration: 100.0,
            blend_edges: false,
            ..FanParams::default()
        }),
        time(500, 1000),
        &[Rgb::RED],
        SEED,
        GRID,
    );
    assert!(hard.iter().all(|c| c.a == 0.0 || c.a == 1.0));
}

#[test]
fn fan_radii_in_pixels_without_scaling() {
    let p = fan(FanParams {
        duration: 100.0,
        blade_width: 100.0,
        start_radius: 0.0,
        end_radius: 3.0,
        scale: false,
        blend_edges: false,
        ..FanParams::default()
    });
    let cells = draw(&p, time(500, 1000), &[Rgb::RED], SEED, GRID);
    let all = lit(&cells, GRID);
    assert!(all.contains(&(18, 15)) && !all.contains(&(19, 15)), "{all:?}");
    // Off center: the fan moves with it.
    let left = fan(FanParams {
        center_x: 20.0,
        ..match p {
            EffectParams::Fan(f) => f,
            _ => unreachable!(),
        }
    });
    let moved = lit(&draw(&left, time(500, 1000), &[Rgb::RED], SEED, GRID), GRID);
    let x_mean = moved.iter().map(|&(x, _)| f64::from(x)).sum::<f64>() / moved.len() as f64;
    assert!((x_mean - 6.0).abs() < 1.0, "{x_mean}");
}

// ---------------------------------------------------------------------------------------------
// Shape

fn one_shape(object: ShapeObject) -> ShapeParams {
    ShapeParams {
        shape: object,
        count: 1,
        lifetime: 100.0,
        start_size: 8.0,
        growth: 0.0,
        fade: false,
        random_location: false,
        random_start: false,
        ..ShapeParams::default()
    }
}

#[test]
fn every_shape_draws_an_outline_around_its_center() {
    for object in [
        ShapeObject::Circle,
        ShapeObject::Ellipse,
        ShapeObject::Triangle,
        ShapeObject::Square,
        ShapeObject::Pentagon,
        ShapeObject::Hexagon,
        ShapeObject::Octagon,
        ShapeObject::Star,
        ShapeObject::Heart,
        ShapeObject::Tree,
        ShapeObject::Snowflake,
        ShapeObject::CandyCane,
        ShapeObject::Crucifix,
        ShapeObject::Present,
        ShapeObject::Random,
    ] {
        let cells = draw(
            &EffectParams::Shape(one_shape(object)),
            time(100, 1000),
            &[Rgb::RED],
            SEED,
            GRID,
        );
        let all = lit(&cells, GRID);
        assert!(all.len() > 8, "{object:?} draws: {}", all.len());
        // (A heart's point reaches π/2 radii below its center.)
        for &(x, y) in &all {
            assert!(
                x.abs_diff(15) <= 9 && y.abs_diff(15) <= 13,
                "{object:?} at ({x}, {y})"
            );
        }
    }
    // A circle of radius 8 around the center: its edge, not its middle.
    let circle = draw(
        &EffectParams::Shape(one_shape(ShapeObject::Circle)),
        time(100, 1000),
        &[Rgb::RED],
        SEED,
        GRID,
    );
    assert_eq!(at(&circle, 23, 15, GRID).a, 1.0);
    assert_eq!(at(&circle, 15, 23, GRID).a, 1.0);
    assert_eq!(at(&circle, 15, 15, GRID).a, 0.0);
    // Thicker outlines fill inward.
    let thick = draw(
        &EffectParams::Shape(ShapeParams {
            thickness: 3,
            ..one_shape(ShapeObject::Circle)
        }),
        time(100, 1000),
        &[Rgb::RED],
        SEED,
        GRID,
    );
    assert_eq!(at(&thick, 21, 15, GRID).a, 1.0);
    assert!(lit(&thick, GRID).len() > lit(&circle, GRID).len());
}

#[test]
fn shapes_grow_and_fade_over_their_lifetime_and_come_back() {
    let p = ShapeParams {
        start_size: 2.0,
        growth: 10.0,
        lifetime: 50.0,
        fade: true,
        ..one_shape(ShapeObject::Circle)
    };
    let params = EffectParams::Shape(p.clone());
    // Lives of 1 s in a 2 s effect: half way through its first life the radius is 2 + 5.
    let half = draw(&params, time(500, 2000), &[Rgb::RED], SEED, GRID);
    assert!(at(&half, 22, 15, GRID).a > 0.0, "radius 7");
    assert!((at(&half, 22, 15, GRID).a - 0.5).abs() < 0.01, "half faded");
    let late = draw(&params, time(900, 2000), &[Rgb::RED], SEED, GRID);
    assert!(at(&late, 26, 15, GRID).a > 0.0 && at(&late, 26, 15, GRID).a < 0.15);
    // Its next life starts small again.
    let again = draw(&params, time(1100, 2000), &[Rgb::RED], SEED, GRID);
    assert!(at(&again, 18, 15, GRID).a > 0.8, "{:?}", lit(&again, GRID));
    // Shrinking stops at nothing rather than turning inside out.
    let shrink = EffectParams::Shape(ShapeParams {
        start_size: 3.0,
        growth: -20.0,
        ..p
    });
    let gone = draw(&shrink, time(900, 2000), &[Rgb::RED], SEED, GRID);
    assert_eq!(
        lit(&gone, GRID),
        vec![(15, 15)],
        "a radius-0 circle is its center"
    );
}

#[test]
fn random_shapes_are_reproducible_and_spread_out() {
    let p = EffectParams::Shape(ShapeParams {
        count: 6,
        lifetime: 20.0,
        start_size: 2.0,
        ..ShapeParams::default()
    });
    let a = draw(&p, time(1234, 5000), &[Rgb::RED, Rgb::GREEN], SEED, GRID);
    let b = draw(&p, time(1234, 5000), &[Rgb::RED, Rgb::GREEN], SEED, GRID);
    assert_eq!(a, b, "the same frame twice");
    let other = draw(&p, time(1234, 5000), &[Rgb::RED, Rgb::GREEN], SEED + 1, GRID);
    assert_ne!(
        lit(&a, GRID),
        lit(&other, GRID),
        "another effect puts them elsewhere"
    );
    let xs: Vec<u32> = lit(&a, GRID).iter().map(|&(x, _)| x).collect();
    assert!(xs.iter().max().unwrap() - xs.iter().min().unwrap() > 8, "{xs:?}");
    // Both palette colors are used.
    assert!(a.iter().any(|c| c.a > 0.0 && c.r > 0.1) && a.iter().any(|c| c.a > 0.0 && c.g > 0.1));
    // Fewer shapes, fewer lit cells.
    let one = EffectParams::Shape(ShapeParams {
        count: 1,
        lifetime: 20.0,
        start_size: 2.0,
        ..ShapeParams::default()
    });
    assert!(lit(&draw(&one, time(1234, 5000), &[Rgb::RED], SEED, GRID), GRID).len() < lit(&a, GRID).len());
}

#[test]
fn shapes_drift_with_their_speed() {
    let p = ShapeParams {
        speed: 10.0,
        direction: 0.0,
        start_size: 1.0,
        ..one_shape(ShapeObject::Circle)
    };
    let cells = draw(&EffectParams::Shape(p), time(500, 2000), &[Rgb::RED], SEED, GRID);
    // 10 pixels a second for half a second: 5 to the right.
    assert!(at(&cells, 21, 15, GRID).a > 0.0 && at(&cells, 16, 15, GRID).a == 0.0);
}

#[test]
fn shapes_on_a_timing_track_appear_at_its_marks() {
    let p = ShapeParams {
        lifetime: 10.0,
        timing_track: Some(TimingTrackId::new()),
        ..one_shape(ShapeObject::Square)
    };
    let shape = |elapsed: u64| {
        let s = Shape::new(
            &p,
            &time(elapsed, 4000),
            Colors::new(&[Rgb::RED, Rgb::BLUE]),
            SEED,
            GRID,
            Some(&[1000, 2000]),
        );
        frame(&Shader::Shape(s), GRID)
    };
    assert!(lit(&shape(900), GRID).is_empty(), "before the first mark");
    let first = shape(1100);
    assert!(!lit(&first, GRID).is_empty() && first.iter().any(|c| c.a > 0.0 && c.r > 0.5));
    assert!(
        lit(&shape(1500), GRID).is_empty(),
        "its lifetime (400 ms) is over"
    );
    let second = shape(2100);
    assert!(second.iter().any(|c| c.a > 0.0 && c.b > 0.5), "the next color");
    // Without the renderer's marks (a bare shader), none appear.
    let bare = draw(
        &EffectParams::Shape(p.clone()),
        time(1100, 4000),
        &[Rgb::RED],
        SEED,
        GRID,
    );
    assert!(lit(&bare, GRID).is_empty());
}

// ---------------------------------------------------------------------------------------------
// Morph

#[test]
fn morph_sweeps_a_head_and_a_fading_tail_from_the_start_line_to_the_end_line() {
    // The default: from the bottom edge to the top edge, the head taking a fifth of the effect.
    let p = EffectParams::Morph(MorphParams::default());
    let pal = [Rgb::RED, Rgb::BLUE];
    // A tenth of the way through, the head is half way up, in the first color.
    let cells = draw(&p, time(100, 1000), &pal, SEED, GRID);
    let row = |y: u32| (0..31).filter(|&x| at(&cells, x, y, GRID).a > 0.0).count();
    let head_rows: Vec<u32> = (0..31).filter(|&y| at(&cells, 15, y, GRID).r > 0.5).collect();
    assert!(
        !head_rows.is_empty() && head_rows.iter().all(|y| (14..=17).contains(y)),
        "{head_rows:?}"
    );
    assert_eq!(row(*head_rows.last().unwrap()), 31, "the line spans the prop");
    assert_eq!(row(25), 0, "above the head is dark");
    // Below it, the tail in the second color, fading toward its end once the head has gone.
    let tail = at(&cells, 15, 10, GRID);
    assert!(tail.b > 0.5 && tail.a > 0.0);
    let leaving = draw(&p, time(600, 1000), &pal, SEED, GRID);
    let (back, front) = (at(&leaving, 15, 1, GRID), at(&leaving, 15, 29, GRID));
    assert!(
        back.a > 0.3 && back.a < 0.6 && front.a > 0.9,
        "{back:?} {front:?}"
    );
    // Later, the head is gone off the top and the tail is leaving.
    let late = draw(&p, time(900, 1000), &pal, SEED, GRID);
    assert!(late.iter().all(|c| c.r < 0.5 || c.a == 0.0), "no head");
    assert!(lit(&late, GRID).len() < 31 * 31);
    // Reproducible.
    assert_eq!(draw(&p, time(100, 1000), &pal, SEED, GRID), cells);
}

#[test]
fn morph_lines_follow_their_ends_and_repeat() {
    // A vertical line sweeping from the left edge to the right edge.
    let p = MorphParams {
        start_x1: 0.0,
        start_y1: 0.0,
        start_x2: 0.0,
        start_y2: 100.0,
        end_x1: 100.0,
        end_y1: 0.0,
        end_x2: 100.0,
        end_y2: 100.0,
        head_duration: 100.0,
        ..MorphParams::default()
    };
    let cells = draw(&EffectParams::Morph(p), time(500, 1000), &[Rgb::RED], SEED, GRID);
    let columns: Vec<u32> = lit(&cells, GRID).iter().map(|&(x, _)| x).collect();
    assert!(columns.iter().all(|x| (14..=17).contains(x)), "{columns:?}");
    // Copies of a line sweeping up sit side by side across the prop, `repeatSpacing` apart.
    let up = MorphParams {
        start_x1: 0.0,
        start_x2: 0.0,
        start_y1: 0.0,
        start_y2: 0.0,
        end_x1: 0.0,
        end_x2: 0.0,
        end_y1: 100.0,
        end_y2: 100.0,
        head_duration: 100.0,
        repeats: 3,
        repeat_spacing: 5,
        ..MorphParams::default()
    };
    let cells = draw(&EffectParams::Morph(up), time(500, 1000), &[Rgb::RED], SEED, GRID);
    let mut xs: Vec<u32> = lit(&cells, GRID).iter().map(|&(x, _)| x).collect();
    xs.sort_unstable();
    xs.dedup();
    assert_eq!(xs, vec![0, 5, 10, 15]);
}

// ---------------------------------------------------------------------------------------------
// Circles

#[test]
fn circles_move_and_stay_reproducible() {
    let p = EffectParams::Circles(CirclesParams::default());
    let pal = [Rgb::RED, Rgb::GREEN, Rgb::BLUE];
    let a = draw(&p, time(0, 5000), &pal, SEED, GRID);
    // Three discs of radius 5 (81 cells each), maybe overlapping.
    let n = lit(&a, GRID).len();
    assert!((81..=243).contains(&n), "{n}");
    assert!(a.iter().any(|c| c.a > 0.0 && c.r > 0.5));
    let later = draw(&p, time(1000, 5000), &pal, SEED, GRID);
    assert_ne!(lit(&a, GRID), lit(&later, GRID), "they move");
    assert_eq!(draw(&p, time(1000, 5000), &pal, SEED, GRID), later);
    assert_ne!(
        lit(&draw(&p, time(0, 5000), &pal, SEED + 7, GRID), GRID),
        lit(&a, GRID)
    );
}

#[test]
fn bouncing_circles_stay_inside_and_wrapping_ones_come_round() {
    let bounce = EffectParams::Circles(CirclesParams {
        count: 1,
        size: 3,
        speed: 30.0,
        bounce: true,
        ..CirclesParams::default()
    });
    for ms in (0..20_000).step_by(700) {
        let cells = draw(&bounce, time(ms, 20_000), &[Rgb::RED], SEED, GRID);
        let all = lit(&cells, GRID);
        assert_eq!(all.len(), 37, "the whole radius-3 disc at {ms} ms: {all:?}");
    }
    // Wrapping, a ball crossing an edge shows on both sides at some moment.
    let wrap = EffectParams::Circles(CirclesParams {
        count: 1,
        size: 3,
        speed: 30.0,
        ..CirclesParams::default()
    });
    let split = (0..20_000).step_by(50).any(|ms| {
        let all = lit(&draw(&wrap, time(ms, 20_000), &[Rgb::RED], SEED, GRID), GRID);
        let xs: Vec<u32> = all.iter().map(|&(x, _)| x).collect();
        let ys: Vec<u32> = all.iter().map(|&(_, y)| y).collect();
        let spread = |v: &[u32]| v.iter().max().unwrap_or(&0) - v.iter().min().unwrap_or(&0);
        spread(&xs) > 20 || spread(&ys) > 20
    });
    assert!(split);
}

#[test]
fn circle_looks() {
    let pal = [Rgb::RED, Rgb::BLUE];
    let look = |look: CirclesLook| {
        EffectParams::Circles(CirclesParams {
            look,
            count: 2,
            ..CirclesParams::default()
        })
    };
    let solid = lit(
        &draw(&look(CirclesLook::Solid), time(300, 5000), &pal, SEED, GRID),
        GRID,
    )
    .len();
    let bubbles = draw(&look(CirclesLook::Bubbles), time(300, 5000), &pal, SEED, GRID);
    assert!(
        !lit(&bubbles, GRID).is_empty() && lit(&bubbles, GRID).len() < solid,
        "outlines only"
    );
    let fading = draw(&look(CirclesLook::Fading), time(300, 5000), &pal, SEED, GRID);
    assert!(fading.iter().any(|c| c.a > 0.0 && c.a < 0.5) && fading.iter().any(|c| c.a == 1.0));
    let plasma = draw(&look(CirclesLook::Plasma), time(300, 5000), &pal, SEED, GRID);
    assert!(lit(&plasma, GRID).len() > 20, "blobs around the balls");
    // Rings spread from the center: early on only near it, later over the whole prop.
    let early = draw(&look(CirclesLook::Radial), time(50, 5000), &pal, SEED, GRID);
    assert!(at(&early, 15, 15, GRID).a > 0.0 && at(&early, 0, 0, GRID).a == 0.0);
    let later = draw(&look(CirclesLook::Radial), time(3000, 5000), &pal, SEED, GRID);
    assert_eq!(lit(&later, GRID).len(), 31 * 31 - 4 * lit_corner_gap(&later));
    assert!(
        later.iter().any(|c| c.r > 0.5) && later.iter().any(|c| c.b > 0.5),
        "both colors"
    );
    let rainbow = draw(
        &look(CirclesLook::RainbowRadial),
        time(3000, 5000),
        &pal,
        SEED,
        GRID,
    );
    assert!(rainbow.iter().any(|c| c.g > 0.5), "colors beyond the palette");
}

/// Cells left dark in one corner (the rings stop at the prop's height from the center).
fn lit_corner_gap(cells: &[Rgba]) -> usize {
    (0..31u32)
        .flat_map(|y| (0..31u32).map(move |x| (x, y)))
        .filter(|&(x, y)| x < 15 && y < 15 && at(cells, x, y, GRID).a == 0.0)
        .count()
}

#[test]
fn hostile_settings_never_panic() {
    let wild = [
        EffectParams::Fan(FanParams {
            start_radius: f32::NAN,
            end_radius: f32::INFINITY,
            blades: 0,
            elements: 0,
            acceleration: 10.0,
            ..FanParams::default()
        }),
        EffectParams::Shape(ShapeParams {
            count: u32::MAX,
            thickness: u32::MAX,
            start_size: 1e9,
            lifetime: 0.0,
            points: 0,
            shape: ShapeObject::Random,
            ..ShapeParams::default()
        }),
        EffectParams::Morph(MorphParams {
            repeats: u32::MAX,
            auto_repeat: true,
            head_duration: 0.0,
            stagger: -1e9,
            ..MorphParams::default()
        }),
        EffectParams::Circles(CirclesParams {
            count: u32::MAX,
            size: u32::MAX,
            speed: f32::NAN,
            look: CirclesLook::RainbowRadial,
            ..CirclesParams::default()
        }),
    ];
    for canvas in [
        Canvas { columns: 1, rows: 1 },
        Canvas { columns: 40, rows: 1 },
        Canvas { columns: 1, rows: 40 },
        GRID,
    ] {
        for p in &wild {
            for ms in [0, 1, 999, 1000, 10_000_000] {
                for c in draw(p, time(ms, 1000), &[Rgb::RED], SEED, canvas) {
                    assert!(c.r.is_finite() && c.a.is_finite() && (0.0..=1.0).contains(&c.a));
                }
            }
        }
    }
}
