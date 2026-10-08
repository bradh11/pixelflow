//! Pinwheel, Snowflakes, Plasma, Butterfly, Garlands, Lines, Life, Tendril, and Text, drawn as
//! xLights draws them on the target's grid.

use pf_render::{Canvas, Colors, EffectTime, Pixel, Rgba, Shade, Shader};
use pf_sequence::*;

const SEED: u64 = 0xBEE5;
const GRID: Canvas = Canvas {
    columns: 31,
    rows: 31,
};
const PALETTE: [Rgb; 3] = [Rgb::RED, Rgb::GREEN, Rgb::BLUE];

/// Time `elapsed_ms` into an effect lasting `length_ms`, in a sequence with 50 ms frames.
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
fn draw_on(params: &EffectParams, t: EffectTime, palette: &[Rgb], canvas: Canvas) -> Vec<Rgba> {
    let shader = Shader::new(params, &t, Colors::new(palette), SEED, canvas);
    (0..canvas.rows)
        .flat_map(|y| (0..canvas.columns).map(move |x| (x, y)))
        .map(|(x, y)| shader.shade(&cell(x, y, canvas)))
        .collect()
}

fn draw(params: &EffectParams, t: EffectTime) -> Vec<Rgba> {
    draw_on(params, t, &PALETTE, GRID)
}

fn at(cells: &[Rgba], x: u32, y: u32) -> Rgba {
    cells[(y * GRID.columns + x) as usize]
}

fn lit_count(cells: &[Rgba]) -> usize {
    cells.iter().filter(|c| c.a > 0.0).count()
}

/// The 8-bit colors shown, for comparing frames.
fn shown(cells: &[Rgba]) -> Vec<[u8; 3]> {
    cells.iter().map(|c| c.to_rgb8()).collect()
}

// ---------------------------------------------------------------------------------------------
// Pinwheel

fn pinwheel(p: PinwheelParams) -> EffectParams {
    EffectParams::Pinwheel(p)
}

#[test]
fn a_pinwheel_arm_points_down_at_the_start_and_turns_with_speed() {
    let still = pinwheel(PinwheelParams {
        arms: 1,
        speed: 0.0,
        ..Default::default()
    });
    let cells = draw(&still, time(0, 10_000));
    // xLights' new method starts the first arm at 270 degrees: straight down from the center.
    assert!(at(&cells, 15, 5).a > 0.0, "below the center");
    assert_eq!(at(&cells, 15, 25).a, 0.0, "nothing above it");
    assert_eq!(
        at(&cells, 15, 5).to_rgb8(),
        [0, 255, 0],
        "arms take the palette's colors from the second on"
    );
    assert_eq!(
        shown(&cells),
        shown(&draw(&still, time(3000, 10_000))),
        "no speed, no turning"
    );

    let turning = pinwheel(PinwheelParams {
        arms: 1,
        speed: 10.0,
        ..Default::default()
    });
    // 10 turns it 200 degrees a second: 90 degrees in 450 ms, counterclockwise from down to the
    // right.
    let later = draw(&turning, time(450, 10_000));
    assert!(
        at(&later, 25, 15).a > 0.0 && at(&later, 15, 5).a == 0.0,
        "pointing right"
    );
    let cw = pinwheel(PinwheelParams {
        counterclockwise: false,
        ..match turning {
            EffectParams::Pinwheel(p) => p,
            _ => unreachable!(),
        }
    });
    let later = draw(&cw, time(450, 10_000));
    assert!(at(&later, 5, 15).a > 0.0, "clockwise: pointing left");
}

#[test]
fn pinwheel_arms_thickness_and_shading() {
    let thin = draw(&pinwheel(PinwheelParams::default()), time(0, 1000));
    let thick = draw(
        &pinwheel(PinwheelParams {
            thickness: 50.0,
            ..Default::default()
        }),
        time(0, 1000),
    );
    assert!(
        lit_count(&thick) > 3 * lit_count(&thin),
        "{} vs {}",
        lit_count(&thick),
        lit_count(&thin)
    );
    // Three arms in the second, third, and first colors.
    let colors: std::collections::BTreeSet<[u8; 3]> =
        thick.iter().filter(|c| c.a > 0.0).map(|c| c.to_rgb8()).collect();
    assert_eq!(colors.len(), 3);
    // Shaded arms are partly see-through across their width; flat ones aren't.
    assert!(thick.iter().all(|c| c.a == 0.0 || c.a == 1.0));
    let raised = draw(
        &pinwheel(PinwheelParams {
            thickness: 50.0,
            shading: PinwheelShading::Raised,
            ..Default::default()
        }),
        time(0, 1000),
    );
    assert!(raised.iter().any(|c| c.a > 0.0 && c.a < 0.9));
    // Twist bends the arms; the center moves with center X.
    let twisted = draw(
        &pinwheel(PinwheelParams {
            thickness: 50.0,
            twist: 180.0,
            ..Default::default()
        }),
        time(0, 1000),
    );
    assert_ne!(shown(&twisted), shown(&thick));
    let moved = draw(
        &pinwheel(PinwheelParams {
            arms: 1,
            center_x: 100.0,
            speed: 0.0,
            ..Default::default()
        }),
        time(0, 1000),
    );
    assert!(
        at(&moved, 30, 5).a > 0.0 && at(&moved, 15, 5).a == 0.0,
        "center at the right edge"
    );
}

#[test]
fn pinwheel_spokes_start_pointing_right() {
    let spokes = pinwheel(PinwheelParams {
        arms: 1,
        speed: 0.0,
        style: PinwheelStyle::Spokes,
        thickness: 10.0,
        shading: PinwheelShading::Sweep,
        ..Default::default()
    });
    let cells = draw(&spokes, time(0, 1000));
    assert!(at(&cells, 25, 15).a > 0.0, "the old method starts at 0 degrees");
    assert_eq!(at(&cells, 5, 15).a, 0.0);
    assert!(
        cells.iter().any(|c| c.a > 0.0 && c.a < 1.0),
        "swept spokes fade across the arm"
    );
    // Long arms reach past the grid without drawing outside it.
    let long = pinwheel(PinwheelParams {
        arm_size: 400.0,
        style: PinwheelStyle::Spokes,
        ..Default::default()
    });
    assert!(lit_count(&draw(&long, time(5000, 10_000))) > 0);
}

// ---------------------------------------------------------------------------------------------
// Snowflakes

fn snow(p: SnowflakesParams) -> EffectParams {
    EffectParams::Snowflakes(p)
}

#[test]
fn blowing_snow_scatters_flakes_and_slides_them_across() {
    let p = snow(SnowflakesParams {
        count: 20,
        flake: SnowflakeShape::Cross,
        ..Default::default()
    });
    let first = draw(&p, time(0, 10_000));
    // Twenty two-color crosses, shown twice: centers in red, arms in green.
    let reds = first.iter().filter(|c| c.to_rgb8() == [255, 0, 0]).count();
    assert!((10..=40).contains(&reds), "{reds}");
    assert!(first.iter().any(|c| c.to_rgb8() == [0, 255, 0]));
    assert_eq!(shown(&first), shown(&draw(&p, time(0, 10_000))), "reproducible");
    // 10 moves the field a cell up every 100 ms (and across every 200).
    assert_ne!(shown(&first), shown(&draw(&p, time(100, 10_000))));
    let still = snow(SnowflakesParams {
        count: 20,
        speed: 0.0,
        ..Default::default()
    });
    assert_eq!(
        shown(&draw(&still, time(0, 10_000))),
        shown(&draw(&still, time(5000, 10_000)))
    );
    // A different seed scatters them elsewhere.
    let elsewhere = Shader::new(&p, &time(0, 10_000), Colors::new(&PALETTE), SEED + 1, GRID);
    let other: Vec<Rgba> = (0..GRID.rows)
        .flat_map(|y| (0..GRID.columns).map(move |x| (x, y)))
        .map(|(x, y)| elsewhere.shade(&cell(x, y, GRID)))
        .collect();
    assert_ne!(shown(&first), shown(&other));
}

#[test]
fn falling_snow_falls_and_piling_snow_fills_the_bottom() {
    let falling = snow(SnowflakesParams {
        count: 10,
        motion: SnowflakesMotion::Falling,
        speed: 29.0,
        ..Default::default()
    });
    let rows_lit = |cells: &[Rgba]| -> Vec<u32> {
        (0..GRID.rows)
            .filter(|&y| (0..GRID.columns).any(|x| at(cells, x, y).a > 0.0))
            .collect()
    };
    // Speed 29 moves every frame; each flake falls a row a frame, new ones starting at the top.
    let a = draw(&falling, time(1000, 60_000));
    let b = draw(&falling, time(1050, 60_000));
    assert_ne!(shown(&a), shown(&b));
    assert!(
        (20..40).any(|f| rows_lit(&draw(&falling, time(f * 50, 60_000))).contains(&(GRID.rows - 1))),
        "new flakes at the top"
    );
    assert_eq!(
        shown(&a),
        shown(&draw(&falling, time(1000, 60_000))),
        "reproducible"
    );
    // Falling snow leaves the bottom; piled snow stays and fills it.
    let piling = snow(SnowflakesParams {
        count: 30,
        motion: SnowflakesMotion::PilingUp,
        speed: 50.0,
        ..Default::default()
    });
    let late = draw(&piling, time(20_000, 60_000));
    assert!(
        (0..GRID.columns).all(|x| at(&late, x, 0).a > 0.0),
        "the bottom row is full"
    );
    let late_falling = draw(&falling, time(20_000, 60_000));
    assert!(
        lit_count(&late_falling) <= 10 * 2,
        "only the falling flakes: {}",
        lit_count(&late_falling)
    );
}

#[test]
fn slow_snow_moves_every_few_frames() {
    // Speed 0 moves once every 30 frames.
    let slow = snow(SnowflakesParams {
        count: 10,
        motion: SnowflakesMotion::Falling,
        speed: 0.0,
        ..Default::default()
    });
    assert_eq!(
        shown(&draw(&slow, time(50, 60_000))),
        shown(&draw(&slow, time(1000, 60_000)))
    );
    assert_ne!(
        shown(&draw(&slow, time(1000, 60_000))),
        shown(&draw(&slow, time(1500, 60_000)))
    );
}

// ---------------------------------------------------------------------------------------------
// Plasma

#[test]
fn plasma_fills_every_cell_and_flows_with_the_frames() {
    let p = EffectParams::Plasma(PlasmaParams::default());
    let first = draw(&p, time(0, 10_000));
    assert_eq!(lit_count(&first), first.len());
    assert!(first.iter().all(|c| c.a == 1.0));
    let later = draw(&p, time(1000, 10_000));
    assert_ne!(shown(&first), shown(&later));
    assert_eq!(shown(&later), shown(&draw(&p, time(1000, 10_000))));
    // xLights moves plasma a step a frame: half the frame time, twice the pace.
    let fast_frames = draw(&p, time(500, 10_000).with_frame_ms(25));
    assert_eq!(shown(&fast_frames), shown(&later));
    // The palette blends across it; the fixed schemes ignore it.
    let palette_colors: std::collections::BTreeSet<[u8; 3]> = first.iter().map(|c| c.to_rgb8()).collect();
    assert!(palette_colors.len() > 20);
    assert!(
        first
            .iter()
            .all(|c| c.to_rgb8().iter().filter(|&&v| v > 0).count() <= 2),
        "between neighbouring palette colors"
    );
    let white = draw(
        &EffectParams::Plasma(PlasmaParams {
            colors: PlasmaColors::White,
            ..Default::default()
        }),
        time(0, 10_000),
    );
    assert!(white.iter().all(|c| {
        let [r, g, b] = c.to_rgb8();
        r == g && g == b
    }));
    let denser = draw(
        &EffectParams::Plasma(PlasmaParams {
            density: 5,
            ..Default::default()
        }),
        time(0, 10_000),
    );
    assert_ne!(shown(&denser), shown(&first));
}

// ---------------------------------------------------------------------------------------------
// Butterfly

#[test]
fn butterfly_patterns_shift_over_time() {
    for pattern in 1..=10 {
        let p = EffectParams::Butterfly(ButterflyParams {
            pattern,
            colors: ButterflyColors::Palette,
            ..Default::default()
        });
        let a = draw(&p, time(500, 10_000));
        let b = draw(&p, time(4000, 10_000));
        assert_ne!(shown(&a), shown(&b), "pattern {pattern} moves");
        assert_eq!(
            shown(&a),
            shown(&draw(&p, time(500, 10_000))),
            "pattern {pattern} reproducible"
        );
        assert_eq!(lit_count(&a), a.len(), "pattern {pattern} fills the grid");
    }
    // Rainbow colors are fully saturated hues.
    let rainbow = draw(
        &EffectParams::Butterfly(ButterflyParams::default()),
        time(500, 10_000),
    );
    assert!(rainbow.iter().all(|c| c.to_rgb8().contains(&255)));
    // Reverse runs the wings the other way.
    let reverse = draw(
        &EffectParams::Butterfly(ButterflyParams {
            direction: Direction::Reverse,
            ..Default::default()
        }),
        time(500, 10_000),
    );
    assert_ne!(shown(&reverse), shown(&rainbow));
}

#[test]
fn butterfly_chunks_leave_bands_dark() {
    let banded = draw(
        &EffectParams::Butterfly(ButterflyParams {
            pattern: 2,
            chunks: 4,
            skip: 2,
            ..Default::default()
        }),
        time(500, 10_000),
    );
    let dark = banded.len() - lit_count(&banded);
    assert!(dark > 0 && dark < banded.len(), "{dark}");
}

// ---------------------------------------------------------------------------------------------
// Garlands

#[test]
fn garlands_stack_up_from_the_bottom_over_the_effect() {
    let p = EffectParams::Garlands(GarlandsParams::default());
    let rows_full = |cells: &[Rgba]| {
        (0..GRID.rows)
            .filter(|&y| (0..GRID.columns).all(|x| at(cells, x, y).a > 0.0))
            .count()
    };
    // They start spread out, `spacing` apart above the bottom row, and slide down into place.
    let start = draw(&p, time(0, 10_000));
    let middle = draw(&p, time(5000, 10_000));
    let end = draw(&p, time(9950, 10_000));
    assert!((0..GRID.columns).all(|x| at(&start, x, 0).a == 0.0));
    assert!(rows_full(&start) > 0 && rows_full(&middle) > rows_full(&start));
    assert_eq!(rows_full(&end), GRID.rows as usize, "every row placed by the end");
    // Each row's garland is one color, the palette running from the top row to the bottom.
    assert_eq!(at(&end, 3, 0).to_rgb8(), at(&end, 20, 0).to_rgb8());
    assert_eq!(at(&end, 0, 30).to_rgb8(), [255, 0, 0]);
    // Down stacks from the top.
    let down = draw(
        &EffectParams::Garlands(GarlandsParams {
            direction: GarlandsDirection::Down,
            ..Default::default()
        }),
        time(2000, 10_000),
    );
    let top_lit = (0..GRID.columns).filter(|&x| at(&down, x, 30).a > 0.0).count();
    let bottom_lit = (0..GRID.columns).filter(|&x| at(&down, x, 0).a > 0.0).count();
    assert!(top_lit > bottom_lit);
    // Swags hang below the line in a repeating pattern.
    let swags = draw(
        &EffectParams::Garlands(GarlandsParams {
            shape: GarlandShape::Swags,
            ..Default::default()
        }),
        time(5000, 10_000),
    );
    assert_ne!(shown(&swags), shown(&middle));
}

// ---------------------------------------------------------------------------------------------
// Lines

#[test]
fn lines_bounce_around_with_fading_trails() {
    let p = EffectParams::Lines(LinesParams {
        count: 2,
        trails: 3,
        ..Default::default()
    });
    let a = draw(&p, time(1000, 10_000));
    let b = draw(&p, time(1050, 10_000));
    assert!(lit_count(&a) > 0);
    assert_ne!(shown(&a), shown(&b), "the points move every frame");
    assert_eq!(shown(&a), shown(&draw(&p, time(1000, 10_000))), "reproducible");
    // Trails show dimmer than the line itself.
    assert!(a.iter().any(|c| c.a > 0.0 && c.a < 1.0));
    let unfaded = draw(
        &EffectParams::Lines(LinesParams {
            count: 2,
            trails: 3,
            fade_trails: false,
            ..Default::default()
        }),
        time(1000, 10_000),
    );
    assert!(unfaded.iter().all(|c| c.a == 0.0 || c.a == 1.0));
    // Lines take the palette colors in turn; thicker lines light more.
    let thick = draw(
        &EffectParams::Lines(LinesParams {
            count: 2,
            thickness: 4,
            ..Default::default()
        }),
        time(1000, 10_000),
    );
    let thin = draw(
        &EffectParams::Lines(LinesParams {
            count: 2,
            ..Default::default()
        }),
        time(1000, 10_000),
    );
    assert!(lit_count(&thick) > lit_count(&thin));
    let colors: std::collections::BTreeSet<[u8; 3]> =
        thin.iter().filter(|c| c.a > 0.0).map(|c| c.to_rgb8()).collect();
    assert_eq!(colors, [[255, 0, 0], [0, 255, 0]].into_iter().collect());
}

// ---------------------------------------------------------------------------------------------
// Life

#[test]
fn life_starts_from_random_cells_and_follows_the_rules() {
    let p = EffectParams::Life(LifeParams {
        density: 60,
        ..Default::default()
    });
    let start = draw(&p, time(0, 10_000));
    // 60% of half the 961 cells, some picked twice.
    assert!((200..=289).contains(&lit_count(&start)), "{}", lit_count(&start));
    // Speed 10 moves on a generation every 100 ms.
    assert_eq!(shown(&start), shown(&draw(&p, time(50, 10_000))));
    let next = draw(&p, time(100, 10_000));
    assert_ne!(shown(&start), shown(&next));
    // The classic rules, checked cell by cell (the grid wraps around).
    let alive =
        |cells: &[Rgba], x: i32, y: i32| at(cells, x.rem_euclid(31) as u32, y.rem_euclid(31) as u32).a > 0.0;
    for y in 0..31 {
        for x in 0..31 {
            let n = (-1..=1)
                .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                .filter(|&(dx, dy)| (dx, dy) != (0, 0) && alive(&start, x + dx, y + dy))
                .count();
            let expect = if alive(&start, x, y) {
                n == 2 || n == 3
            } else {
                n == 3
            };
            assert_eq!(alive(&next, x, y), expect, "({x}, {y}) with {n} neighbors");
        }
    }
    assert_eq!(shown(&next), shown(&draw(&p, time(100, 10_000))), "reproducible");
}

// ---------------------------------------------------------------------------------------------
// Tendril

#[test]
fn tendrils_follow_a_point_around_a_circle() {
    let p = EffectParams::Tendril(TendrilParams::default());
    let a = draw(&p, time(1000, 10_000));
    let b = draw(&p, time(2000, 10_000));
    assert!(lit_count(&a) > 0);
    assert_ne!(shown(&a), shown(&b));
    assert_eq!(shown(&a), shown(&draw(&p, time(1000, 10_000))), "reproducible");
    // Thicker tendrils light more; their color runs through the palette over the effect.
    let thick = draw(
        &EffectParams::Tendril(TendrilParams {
            thickness: 6.0,
            ..Default::default()
        }),
        time(1000, 10_000),
    );
    assert!(lit_count(&thick) > lit_count(&a));
    let near_end = draw(&p, time(9950, 10_000));
    let brightest = |cells: &[Rgba]| {
        cells
            .iter()
            .map(|c| c.to_rgb8())
            .max_by_key(|c| c.iter().map(|&v| u32::from(v)).sum::<u32>())
    };
    assert_eq!(brightest(&near_end).map(|c| c[2]), Some(255), "blue by the end");
    // Held at a point, the tendril gathers there.
    let held = draw(
        &EffectParams::Tendril(TendrilParams {
            movement: TendrilMovement::Manual,
            manual_x: 80.0,
            manual_y: 80.0,
            ..Default::default()
        }),
        time(5000, 10_000),
    );
    assert!(at(&held, 24, 24).a > 0.0 && at(&held, 5, 5).a == 0.0);
}

// ---------------------------------------------------------------------------------------------
// Text

fn text(p: TextParams) -> EffectParams {
    EffectParams::Text(p)
}

/// The grid as text, `#` for lit cells, top row first.
fn picture(cells: &[Rgba], canvas: Canvas) -> Vec<String> {
    (0..canvas.rows)
        .rev()
        .map(|y| {
            (0..canvas.columns)
                .map(|x| {
                    if cells[(y * canvas.columns + x) as usize].a > 0.0 {
                        '#'
                    } else {
                        '.'
                    }
                })
                .collect()
        })
        .collect()
}

#[test]
fn text_sits_in_the_middle_in_the_pixel_font() {
    let canvas = Canvas {
        columns: 13,
        rows: 10,
    };
    let cells = draw_on(
        &text(TextParams {
            text: "HI".into(),
            ..Default::default()
        }),
        time(0, 1000),
        &[Rgb::WHITE],
        canvas,
    );
    assert_eq!(
        picture(&cells, canvas),
        [
            ".............",
            ".#...#..###..",
            ".#...#...#...",
            ".#...#...#...",
            ".#####...#...",
            ".#...#...#...",
            ".#...#...#...",
            ".#...#..###..",
            ".............",
            ".............",
        ]
    );
    // Several palette colors go letter by letter.
    let two = draw_on(
        &text(TextParams {
            text: "HI".into(),
            ..Default::default()
        }),
        time(0, 1000),
        &PALETTE,
        canvas,
    );
    assert_eq!(two[(8 * 13 + 1) as usize].to_rgb8(), [255, 0, 0]);
    assert_eq!(two[(8 * 13 + 9) as usize].to_rgb8(), [0, 255, 0]);
    // Twice the height, twice the size.
    let big = draw_on(
        &text(TextParams {
            text: "I".into(),
            size: 16,
            ..Default::default()
        }),
        time(0, 1000),
        &[Rgb::WHITE],
        Canvas {
            columns: 20,
            rows: 20,
        },
    );
    assert_eq!(big.iter().filter(|c| c.a > 0.0).count(), 4 * 11);
}

#[test]
fn text_scrolls_across_and_stops_in_the_middle() {
    let canvas = Canvas { columns: 30, rows: 9 };
    let left = |elapsed| {
        let cells = draw_on(
            &text(TextParams {
                text: "A".into(),
                movement: TextMovement::Left,
                ..Default::default()
            }),
            time(elapsed, 100_000),
            &[Rgb::WHITE],
            canvas,
        );
        let columns: Vec<u32> = (0..canvas.columns)
            .filter(|&x| (0..canvas.rows).any(|y| cells[(y * canvas.columns + x) as usize].a > 0.0))
            .collect();
        columns
    };
    // Starts just off the right edge and moves left a cell every 8 counts (10 counts a frame at 10).
    assert!(left(0).is_empty() || left(0)[0] > 25, "{:?}", left(0));
    let mid = left(500);
    let later = left(1000);
    assert!(!mid.is_empty() && !later.is_empty());
    assert!(later[0] < mid[0], "{mid:?} then {later:?}");
    // Stopping in the middle: it comes to rest there.
    let stop = |elapsed| {
        draw_on(
            &text(TextParams {
                text: "A".into(),
                movement: TextMovement::Left,
                to_center: true,
                ..Default::default()
            }),
            time(elapsed, 100_000),
            &[Rgb::WHITE],
            canvas,
        )
    };
    assert_eq!(shown(&stop(20_000)), shown(&stop(40_000)));
    let still = draw_on(
        &text(TextParams {
            text: "A".into(),
            ..Default::default()
        }),
        time(0, 1000),
        &[Rgb::WHITE],
        canvas,
    );
    assert_eq!(shown(&stop(20_000)), shown(&still));
}

#[test]
fn text_stacks_and_counts_down() {
    let canvas = Canvas { columns: 7, rows: 18 };
    let stacked = draw_on(
        &text(TextParams {
            text: "IT".into(),
            orientation: TextOrientation::StackedDown,
            ..Default::default()
        }),
        time(0, 1000),
        &[Rgb::WHITE],
        canvas,
    );
    let rows = picture(&stacked, canvas);
    assert_eq!(rows[1], "..###..", "I on top");
    assert_eq!(rows[9], ".#####.", "T under it");
    let countdown = |elapsed| {
        let p = text(TextParams {
            text: "3".into(),
            countdown: TextCountdown::Seconds,
            ..Default::default()
        });
        picture(
            &draw_on(
                &p,
                time(elapsed, 10_000),
                &[Rgb::WHITE],
                Canvas { columns: 7, rows: 9 },
            ),
            Canvas { columns: 7, rows: 9 },
        )
    };
    let three = countdown(0);
    assert_eq!(three, countdown(900));
    assert_ne!(three, countdown(1100), "a second later it says 2");
    assert_eq!(countdown(3100), countdown(9000), "and stops at 0");
}

#[test]
fn hostile_settings_never_panic() {
    let tiny = Canvas { columns: 1, rows: 1 };
    let all = [
        pinwheel(PinwheelParams {
            arms: 20,
            arm_size: 400.0,
            twist: -360.0,
            thickness: 100.0,
            speed: 50.0,
            ..Default::default()
        }),
        snow(SnowflakesParams {
            count: 100,
            flake: SnowflakeShape::Random,
            motion: SnowflakesMotion::PilingUp,
            warmup: 100,
            ..Default::default()
        }),
        EffectParams::Plasma(PlasmaParams {
            speed: 100.0,
            ..Default::default()
        }),
        EffectParams::Butterfly(ButterflyParams {
            pattern: 10,
            chunks: 10,
            skip: 10,
            speed: 100.0,
            ..Default::default()
        }),
        EffectParams::Garlands(GarlandsParams {
            cycles: 20.0,
            direction: GarlandsDirection::RightThenLeft,
            ..Default::default()
        }),
        EffectParams::Lines(LinesParams {
            count: 20,
            points: 6,
            thickness: 10,
            speed: 10.0,
            trails: 10,
            ..Default::default()
        }),
        EffectParams::Life(LifeParams {
            density: 100,
            speed: 30,
            rules: LifeRules::B25678S5678,
        }),
        EffectParams::Tendril(TendrilParams {
            movement: TendrilMovement::Random,
            tendrils: 20,
            length: 100,
            thickness: 20.0,
            movement_size: 0.0,
            ..Default::default()
        }),
        text(TextParams {
            text: "€ ünïcödé\\n2nd line".into(),
            movement: TextMovement::Wavy,
            size: 100,
            ..Default::default()
        }),
    ];
    for params in &all {
        for canvas in [
            tiny,
            GRID,
            Canvas {
                columns: 200,
                rows: 3,
            },
        ] {
            for elapsed in [0, 50, 2000] {
                let _ = draw_on(params, time(elapsed, 3000), &[], canvas);
            }
        }
    }
}
