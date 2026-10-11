//! Each effect's colors at chosen pixels and times.

use pf_render::{Canvas, EffectTime, Pixel, Rgba, shade_pixel};
use pf_sequence::*;

const SEED: u64 = 0xC0FFEE;
const STRIP: Canvas = Canvas { columns: 10, rows: 1 };
const GRID: Canvas = Canvas {
    columns: 10,
    rows: 10,
};

/// Pixel `index` of a 10-pixel horizontal strip.
fn strip(index: u32) -> Pixel {
    Pixel {
        u: index as f32 / 9.0,
        v: 0.5,
        index,
        count: 10,
    }
}

fn at(u: f32, v: f32) -> Pixel {
    Pixel {
        u,
        v,
        index: 0,
        count: 1,
    }
}

/// Time `elapsed_ms` into an effect lasting `length_ms`.
fn time(elapsed_ms: u64, length_ms: u64) -> EffectTime {
    EffectTime::within(1000, 1000 + length_ms, 1000 + elapsed_ms)
}

fn shade(params: &EffectParams, t: EffectTime, px: Pixel, palette: &[Rgb], canvas: Canvas) -> Rgba {
    shade_pixel(params, t, &px, palette, SEED, canvas)
}

/// The color as it shows over black.
fn rgb(c: Rgba) -> [u8; 3] {
    c.to_rgb8()
}

/// Which strip pixels an effect lights (coverage above half).
fn lit(params: &EffectParams, t: EffectTime, palette: &[Rgb]) -> Vec<u32> {
    (0..10)
        .filter(|&i| shade(params, t, strip(i), palette, STRIP).a > 0.5)
        .collect()
}

#[test]
fn effect_time_runs_from_zero_to_one() {
    assert_eq!(time(0, 2000).t_norm, 0.0);
    assert_eq!(time(500, 2000).t_norm, 0.25);
    assert_eq!(time(500, 2000).elapsed_ms, 500);
    assert_eq!(
        EffectTime::within(100, 100, 100).t_norm,
        0.0,
        "zero-length effects don't divide by zero"
    );
}

#[test]
fn on_is_solid_or_a_palette_ramp_with_a_level_ramp() {
    let solid = EffectParams::On(OnParams::default());
    let c = shade(&solid, time(0, 1000), strip(7), &[Rgb::RED, Rgb::BLUE], STRIP);
    assert_eq!((rgb(c), c.a), ([255, 0, 0], 1.0));

    let ramp = EffectParams::On(OnParams {
        gradient: Gradient::Horizontal,
        ..OnParams::default()
    });
    let pal = [Rgb::RED, Rgb::BLUE];
    assert_eq!(
        rgb(shade(&ramp, time(0, 1000), strip(0), &pal, STRIP)),
        [255, 0, 0]
    );
    assert_eq!(
        rgb(shade(&ramp, time(0, 1000), strip(9), &pal, STRIP)),
        [0, 0, 255]
    );
    assert_eq!(
        rgb(shade(&ramp, time(0, 1000), at(0.5, 0.5), &pal, STRIP)),
        [128, 0, 128]
    );

    let dimming = EffectParams::On(OnParams {
        start_level: 1.0,
        end_level: 0.0,
        ..OnParams::default()
    });
    // The ramp is counted in frames, as xLights counts it: over 5 frames (0-4) it is a quarter
    // of the way on frame 1 and at the end level on the last frame.
    let white = |p: &EffectParams, ms: u64| shade(p, time(ms, 250), strip(0), &[Rgb::WHITE], STRIP);
    assert_eq!(white(&dimming, 50).a, 0.75);
    assert_eq!(rgb(white(&dimming, 100)), [128; 3]);
    assert_eq!(white(&dimming, 200).a, 0.0);
    // Dimmed, it lets what's below show through; solid (xLights' On), the color itself dims
    // and still covers.
    let c = white(&dimming, 50);
    assert_eq!(([c.r, c.g, c.b], c.a), ([1.0; 3], 0.75));
    let solid = EffectParams::On(OnParams {
        start_level: 1.0,
        end_level: 0.0,
        solid: true,
        ..OnParams::default()
    });
    let c = white(&solid, 50);
    assert_eq!(([c.r, c.g, c.b], c.a), ([0.75; 3], 1.0));
    assert_eq!(white(&solid, 200), Rgba::BLACK, "black, not clear");
    // Transparency is how much shows through whatever the brightness.
    let sheer = EffectParams::On(OnParams {
        start_level: 0.5,
        end_level: 0.5,
        solid: true,
        transparency: 0.25,
        ..OnParams::default()
    });
    let c = white(&sheer, 0);
    assert_eq!(([c.r, c.g, c.b], c.a), ([0.5; 3], 0.75));

    // Shimmer: every other frame the second palette color, or nothing when there's only one.
    let shimmer = EffectParams::On(OnParams {
        shimmer: true,
        ..OnParams::default()
    });
    let frame = |ms: u64, palette: &[Rgb]| shade(&shimmer, time(ms, 1000), strip(0), palette, STRIP);
    assert_eq!(rgb(frame(0, &[Rgb::RED, Rgb::BLUE])), [255, 0, 0]);
    assert_eq!(rgb(frame(50, &[Rgb::RED, Rgb::BLUE])), [0, 0, 255]);
    assert_eq!(rgb(frame(100, &[Rgb::RED, Rgb::BLUE])), [255, 0, 0]);
    assert_eq!(frame(50, &[Rgb::RED]), Rgba::CLEAR);
    assert_eq!(rgb(frame(100, &[Rgb::RED])), [255, 0, 0]);
}

#[test]
fn off_is_opaque_black() {
    let c = shade(
        &EffectParams::Off(OffParams {}),
        time(10, 100),
        strip(3),
        &[],
        STRIP,
    );
    assert_eq!(c, Rgba::BLACK);
}

#[test]
fn color_wash_goes_through_the_palette_over_the_effect() {
    let wash = EffectParams::ColorWash(ColorWashParams::default());
    let pal = [Rgb::RED, Rgb::GREEN, Rgb::BLUE];
    let color_at = |ms: u64| rgb(shade(&wash, time(ms, 1000), strip(4), &pal, STRIP));
    assert_eq!(color_at(0), [255, 0, 0]);
    assert_eq!(color_at(250), [128, 128, 0]);
    assert_eq!(color_at(500), [0, 255, 0]);
    assert_eq!(color_at(999), [0, 1, 254]);

    // Two cycles go there and back again.
    let twice = EffectParams::ColorWash(ColorWashParams {
        cycles: 2.0,
        ..ColorWashParams::default()
    });
    assert_eq!(
        rgb(shade(&twice, time(500, 1000), strip(0), &pal, STRIP)),
        [0, 0, 255]
    );
    assert_eq!(
        rgb(shade(&twice, time(750, 1000), strip(0), &pal, STRIP)),
        [0, 255, 0]
    );

    // A gradient spreads the colors across the prop as well.
    let across = EffectParams::ColorWash(ColorWashParams {
        cycles: 1.0,
        gradient: Gradient::Vertical,
    });
    assert_eq!(
        rgb(shade(&across, time(0, 1000), at(0.0, 1.0), &pal, GRID)),
        [0, 0, 255]
    );
    assert_eq!(
        rgb(shade(&across, time(0, 1000), at(0.0, 0.5), &pal, GRID)),
        [0, 255, 0]
    );
}

#[test]
fn fade_in_and_out() {
    let fade_in = EffectParams::Fade(FadeParams::default());
    let fade_out = EffectParams::Fade(FadeParams {
        direction: FadeDirection::Out,
    });
    let pal = [Rgb::new(200, 100, 0)];
    assert_eq!(shade(&fade_in, time(0, 1000), strip(0), &pal, STRIP).a, 0.0);
    assert_eq!(
        rgb(shade(&fade_in, time(500, 1000), strip(0), &pal, STRIP)),
        [100, 50, 0]
    );
    assert_eq!(shade(&fade_out, time(250, 1000), strip(0), &pal, STRIP).a, 0.75);
}

#[test]
fn chase_moves_bands_along_the_wiring() {
    let chase = EffectParams::Chase(ChaseParams::default()); // 1 band, 20 % wide, 1 trip/s
    let pal = [Rgb::RED];
    assert_eq!(lit(&chase, time(0, 4000), &pal), vec![0, 1]);
    assert_eq!(lit(&chase, time(500, 4000), &pal), vec![5, 6]);
    assert_eq!(lit(&chase, time(900, 4000), &pal), vec![0, 9], "wraps around");
    assert_eq!(
        lit(&chase, time(1500, 4000), &pal),
        vec![5, 6],
        "repeats every second"
    );

    let reverse = EffectParams::Chase(ChaseParams {
        direction: Direction::Reverse,
        ..ChaseParams::default()
    });
    assert_eq!(lit(&reverse, time(200, 4000), &pal), vec![8, 9]);

    let bounce = EffectParams::Chase(ChaseParams {
        bounce: true,
        ..ChaseParams::default()
    });
    assert_eq!(
        lit(&bounce, time(1200, 4000), &pal),
        vec![8, 9],
        "on the way back"
    );

    // Marquee: two bands, half the spacing lit, alternating palette colors.
    let marquee = EffectParams::Chase(ChaseParams {
        bands: 2,
        width: 0.4,
        ..ChaseParams::default()
    });
    let pal = [Rgb::RED, Rgb::BLUE];
    assert_eq!(lit(&marquee, time(0, 4000), &pal), vec![0, 1, 5, 6]);
    assert_eq!(
        rgb(shade(&marquee, time(0, 4000), strip(0), &pal, STRIP)),
        [255, 0, 0]
    );
    assert_eq!(
        rgb(shade(&marquee, time(0, 4000), strip(5), &pal, STRIP)),
        [0, 0, 255]
    );
    assert_eq!(shade(&marquee, time(0, 4000), strip(3), &pal, STRIP), Rgba::CLEAR);
}

/// Red and green bars on a grid 12 cells long their way and 4 across, `elapsed_ms` into a 1 s
/// effect (20 frames): the color of each line of cells, from the bottom or the left.
fn bars_colors(p: BarsParams, elapsed_ms: u64) -> Vec<Rgba> {
    let params = EffectParams::Bars(p);
    (0..12)
        .map(|line| {
            let along = line as f32 / 11.0;
            let (px, canvas) = match p.axis {
                Axis::Vertical => (at(0.4, along), Canvas { columns: 4, rows: 12 }),
                Axis::Horizontal => (at(along, 0.4), Canvas { columns: 12, rows: 4 }),
            };
            shade(
                &params,
                time(elapsed_ms, 1000),
                px,
                &[Rgb::RED, Rgb::GREEN],
                canvas,
            )
        })
        .collect()
}

/// [`bars_colors`] as 'r', 'g', 'w' (white), or '.' (clear) for each line of cells.
fn bars_line(p: BarsParams, elapsed_ms: u64) -> String {
    bars_colors(p, elapsed_ms)
        .iter()
        .map(|c| match [c.r, c.g, c.b].map(|v| (v * 255.0).round() as u8) {
            _ if c.a <= 0.0 => '.',
            [255, 0, 0] => 'r',
            [0, 255, 0] => 'g',
            [255, 255, 255] => 'w',
            _ => '?',
        })
        .collect()
}

#[test]
fn bars_are_drawn_cell_by_cell_as_xlights_draws_them() {
    // xLights' Bars with the palette twice over (4 bars) and no gaps: each bar is
    // ceil(12 / 4) = 3 cells, and the palette a block of 6. Bars going down count cells up
    // from the bottom; bars going up count down from the top, so the colors run the other way.
    let down = BarsParams {
        count: 4,
        speed: 0.0,
        axis: Axis::Vertical,
        direction: Direction::Reverse,
        motion: BarsMotion::Slide,
        fill: 1.0,
        ..BarsParams::default()
    };
    let up = BarsParams {
        direction: Direction::Forward,
        ..down
    };
    assert_eq!(bars_line(down, 0), "rrrgggrrrggg");
    assert_eq!(bars_line(up, 0), "gggrrrgggrrr");
    // Bars that don't divide the grid are the next whole size: 5 bars of ceil(12 / 5) = 3.
    assert_eq!(bars_line(BarsParams { count: 5, ..down }, 0), "rrrgggrrrggg");
    // Across the prop they count from the left going left, from the right going right.
    let left = BarsParams {
        axis: Axis::Horizontal,
        ..down
    };
    assert_eq!(bars_line(left, 0), "rrrgggrrrggg");

    // Moving: a block a cycle. At a prop's length a second that's 2 cycles over the second,
    // 10 frames each, and xLights counts the last frame of a cycle as its end: frame 4 is 4/9
    // of the way, trunc(4/9 × 6) = 2 cells; frame 9 is a whole block on (the same as none),
    // and frame 10 starts again.
    let moving_up = BarsParams { speed: 1.0, ..up };
    assert_eq!(bars_line(moving_up, 200), "rrgggrrrgggr", "2 cells up");
    assert_eq!(bars_line(moving_up, 450), bars_line(up, 0));
    assert_eq!(bars_line(moving_up, 500), bars_line(up, 0));
    let moving_down = BarsParams { speed: 1.0, ..down };
    assert_eq!(bars_line(moving_down, 200), "rgggrrrgggrr", "2 cells down");

    // 'expand': drawn like bars going down from the center's line (cell 6 of 12) and below,
    // mirrored above it, so they move apart. 'compress': drawn from the center up, mirrored
    // below, so they close in.
    let expand = BarsParams {
        motion: BarsMotion::Split,
        direction: Direction::Forward,
        ..down
    };
    let compress = BarsParams {
        direction: Direction::Reverse,
        ..expand
    };
    assert_eq!(bars_line(expand, 0), "rrrgggrgggrr");
    assert_eq!(bars_line(compress, 0), "rgggrrrrrggg");
    assert_eq!(
        bars_line(BarsParams { speed: 1.0, ..expand }, 200),
        "rgggrrrrrggg",
        "2 cells out from the center"
    );
    assert_eq!(
        bars_line(
            BarsParams {
                speed: 1.0,
                ..compress
            },
            200
        ),
        "rrrgggrgggrr",
        "2 cells in toward it"
    );
    // The center moves: 50% is cell trunc(12 × 150 / 200) = 9.
    assert_eq!(
        bars_line(
            BarsParams {
                center: 50.0,
                ..expand
            },
            0
        ),
        "rrrgggrrrgrr"
    );

    // 'Alternate': a whole bar at a time, the prop's length (4 bars) a cycle. At a prop's
    // length a second, frame 4 of 0-19 is floor(4/19 × 4) = 0 bars on; frame 5 is 1.
    let stepping = BarsParams {
        motion: BarsMotion::Step,
        speed: 1.0,
        ..up
    };
    assert_eq!(bars_line(stepping, 200), "gggrrrgggrrr");
    assert_eq!(bars_line(stepping, 250), "rrrgggrrrggg");
}

#[test]
fn bars_take_xlights_3d_highlight_and_gradient_looks_and_pixelflows_gaps() {
    let down = BarsParams {
        count: 4,
        speed: 0.0,
        axis: Axis::Vertical,
        direction: Direction::Reverse,
        motion: BarsMotion::Slide,
        fill: 1.0,
        ..BarsParams::default()
    };
    // "3D": each 3-cell bar covers 2/3, 1/3, then nothing, from the edge it leads with (never
    // fully: xLights takes a cell off the count).
    let shaded = bars_colors(BarsParams { shaded: true, ..down }, 0);
    let cover: Vec<f32> = shaded.iter().take(4).map(|c| c.a).collect();
    assert_eq!(cover, [2.0 / 3.0, 1.0 / 3.0, 0.0, 2.0 / 3.0]);
    assert_eq!([shaded[0].r, shaded[0].g], [1.0, 0.0], "the color stays whole");
    // Going up, the leading edge is the bar's top.
    let up = BarsParams {
        direction: Direction::Forward,
        shaded: true,
        ..down
    };
    let cover: Vec<f32> = bars_colors(up, 0).iter().take(3).map(|c| c.a).collect();
    assert_eq!(cover, [0.0, 1.0 / 3.0, 2.0 / 3.0]);

    // Highlight: each bar's leading line is white.
    let highlight = BarsParams {
        highlight: true,
        ..down
    };
    assert_eq!(bars_line(highlight, 0), "wrrwggwrrwgg");

    // Gradient: each bar blends into the next color, a third of the way per cell.
    let gradient = bars_colors(
        BarsParams {
            gradient: true,
            ..down
        },
        0,
    );
    let rgb8: Vec<[u8; 3]> = gradient.iter().take(5).map(|c| rgb(*c)).collect();
    assert_eq!(
        rgb8,
        [[255, 0, 0], [170, 85, 0], [85, 170, 0], [0, 255, 0], [85, 170, 0]]
    );

    // PixelFlow's own bars leave part of each dark: half of 3 cells lights 2.
    assert_eq!(bars_line(BarsParams { fill: 0.5, ..down }, 0), "rr.gg.rr.gg.");
    assert_eq!(bars_line(BarsParams { fill: 0.0, ..down }, 0), "............");
}

#[test]
fn wave_draws_a_rolling_sine_line() {
    let wave = EffectParams::Wave(WaveParams {
        cycles: 1.0,
        speed: 1.0,
        height: 0.8,
        thickness: 0.1,
        direction: Direction::Forward,
    });
    let pal = [Rgb::BLUE];
    let a = |ms: u64, u: f32, v: f32| shade(&wave, time(ms, 5000), at(u, v), &pal, GRID).a;
    // At t = 0: center at u = 0.25 is 0.5 + 0.4 = 0.9; at u = 0.75 it's 0.1.
    assert_eq!(a(0, 0.25, 0.9), 1.0);
    assert_eq!(a(0, 0.25, 0.5), 0.0);
    assert_eq!(a(0, 0.75, 0.1), 1.0);
    // A quarter second later the wave rolled a quarter: the peak is at u = 0.5.
    assert_eq!(a(250, 0.5, 0.9), 1.0);
    assert_eq!(a(250, 0.25, 0.9), 0.0);
}

#[test]
fn twinkle_is_deterministic_and_lights_about_the_density() {
    let twinkle = EffectParams::Twinkle(TwinkleParams {
        density: 0.5,
        rate: 2.0,
    });
    let pal = [Rgb::RED, Rgb::GREEN, Rgb::BLUE];
    let many = |ms: u64| -> Vec<Rgba> {
        (0..4000u32)
            .map(|i| {
                let px = Pixel {
                    u: 0.0,
                    v: 0.0,
                    index: i,
                    count: 4000,
                };
                shade_pixel(&twinkle, time(ms, 60_000), &px, &pal, SEED, STRIP)
            })
            .collect()
    };
    let frame = many(12_345);
    assert_eq!(frame, many(12_345), "same time, same twinkles");
    assert_ne!(frame, many(12_545), "twinkles change over time");
    let other_seed: Vec<Rgba> = (0..4000u32)
        .map(|i| {
            let px = Pixel {
                u: 0.0,
                v: 0.0,
                index: i,
                count: 4000,
            };
            shade_pixel(&twinkle, time(12_345, 60_000), &px, &pal, SEED + 1, STRIP)
        })
        .collect();
    assert_ne!(frame, other_seed, "each effect twinkles differently");
    let lit = frame.iter().filter(|c| c.a > 0.0).count() as f32 / 4000.0;
    assert!(
        (lit - 0.5).abs() < 0.05,
        "about half the pixels are twinkling: {lit}"
    );
    assert!(frame.iter().all(|c| c.a <= 1.0));

    let none = EffectParams::Twinkle(TwinkleParams {
        density: 0.0,
        rate: 2.0,
    });
    assert!((0..10).all(|i| shade(&none, time(777, 5000), strip(i), &pal, STRIP).a == 0.0));
}

#[test]
fn shimmer_flickers_the_whole_prop() {
    let shimmer = EffectParams::Shimmer(ShimmerParams {
        rate: 10.0,
        duty: 0.5,
    });
    let pal = [Rgb::WHITE];
    assert_eq!(lit(&shimmer, time(20, 1000), &pal).len(), 10);
    assert!(lit(&shimmer, time(70, 1000), &pal).is_empty());
    assert_eq!(lit(&shimmer, time(120, 1000), &pal).len(), 10);
}

#[test]
fn strobe_flashes_a_random_set_each_flash() {
    let strobe = EffectParams::Strobe(StrobeParams {
        rate: 4.0,
        density: 0.5,
    });
    let pal = [Rgb::WHITE];
    let count = |ms: u64| {
        (0..2000u32)
            .filter(|&i| {
                let px = Pixel {
                    u: 0.0,
                    v: 0.0,
                    index: i,
                    count: 2000,
                };
                shade_pixel(&strobe, time(ms, 10_000), &px, &pal, SEED, STRIP).a > 0.0
            })
            .collect::<Vec<_>>()
    };
    let first = count(10);
    assert!((first.len() as f32 / 2000.0 - 0.5).abs() < 0.05);
    assert_eq!(first, count(100), "the same flash lights the same pixels");
    assert!(count(200).is_empty(), "dark between flashes");
    assert_ne!(first, count(260), "the next flash picks new pixels");
}

/// A tree of 16 strings of 50 pixels, as its strings-by-pixels grid.
const TREE: Canvas = Canvas {
    columns: 16,
    rows: 50,
};

/// Row `y` of a red and green spiral on the tree, `elapsed_ms` into a 1 s effect (20 frames):
/// the color of each of its 16 cells.
fn spiral_cells(p: SpiralParams, elapsed_ms: u64, y: u32) -> Vec<Rgba> {
    let params = EffectParams::Spiral(p);
    (0..16)
        .map(|x| {
            let px = at(x as f32 / 15.0, y as f32 / 49.0);
            shade(&params, time(elapsed_ms, 1000), px, &[Rgb::RED, Rgb::GREEN], TREE)
        })
        .collect()
}

/// [`spiral_cells`] as 'r', 'g', or '.' for each cell.
fn spiral_row(p: SpiralParams, elapsed_ms: u64, y: u32) -> String {
    spiral_cells(p, elapsed_ms, y)
        .iter()
        .map(|c| match rgb(*c) {
            [255, 0, 0] => 'r',
            [0, 255, 0] => 'g',
            _ => '.',
        })
        .collect()
}

#[test]
fn spirals_are_drawn_cell_by_cell_as_xlights_draws_them() {
    // xLights' Spirals with 2 stripes, Thickness 49, Rotation 37, still: the stripes start
    // 16 / 2 = 8 cells apart, each trunc(8 × 0.49 + 1) = 4 wide, and row y is shifted
    // trunc(y × 37 / 50) cells: the rotation counts cells across the height, not turns, so
    // from the bottom row to the top the stripes go 36 cells round (2.25 times).
    let still = SpiralParams {
        count: 2,
        speed: 0.0,
        thickness: 0.49,
        twist: 0.0,
        lean: -37.0,
        direction: Direction::Forward,
        ..SpiralParams::default()
    };
    assert_eq!(spiral_row(still, 0, 0), "rrrr....gggg....");
    assert_eq!(
        spiral_row(still, 0, 1),
        "rrrr....gggg....",
        "0.74 of a cell: not yet"
    );
    assert_eq!(spiral_row(still, 0, 2), ".rrrr....gggg...");
    assert_eq!(
        spiral_row(still, 0, 25),
        "..rrrr....gggg..",
        "18 cells: once round and 2"
    );
    assert_eq!(spiral_row(still, 0, 49), "....rrrr....gggg", "36 cells");
    // The same setting on a group's 400-cell grid would lean the stripes a tenth of the way
    // across; counted in turns it would wind as tightly there as on the tree.

    // PixelFlow's twist counts turns, winding the other way: one turn is 16 cells here.
    let twisted = SpiralParams {
        twist: 1.0,
        lean: 0.0,
        ..still
    };
    let leaning = SpiralParams { lean: 16.0, ..still };
    for y in [0, 10, 30, 49] {
        assert_eq!(spiral_row(twisted, 0, y), spiral_row(leaning, 0, y), "row {y}");
    }
    assert_eq!(
        spiral_row(twisted, 0, 25),
        "gggg....rrrr....",
        "half way up, half way round"
    );

    // A stripe over two cells wide loses its part cell; a narrower one keeps it. A full-width
    // one is a cell over its share, and the next stripe covers that.
    let wide = |thickness| spiral_row(SpiralParams { thickness, ..still }, 0, 0);
    assert_eq!(wide(0.1), "rr......gg......", "ceil(1.8)");
    assert_eq!(wide(0.2), "rr......gg......", "trunc(2.6)");
    assert_eq!(
        wide(1.0),
        "grrrrrrrgggggggg",
        "9 each: the second wraps onto the first"
    );

    // Turning: one turn over the effect's 20 frames. A quarter of the way (frame 5 of 0-19)
    // the stripes have moved trunc(5 / 19 × 160) / 10 = 4.2 cells, forward or back.
    let turning = SpiralParams {
        speed: 1.0,
        lean: 0.0,
        ..still
    };
    assert_eq!(spiral_row(turning, 250, 0), "....rrrr....gggg");
    let back = SpiralParams {
        direction: Direction::Reverse,
        ..turning
    };
    // Going back, cells below zero round the other way (C's cast), so the stripe that wraps
    // sits a cell further on than the one that doesn't.
    assert_eq!(spiral_row(back, 250, 0), "...gggg.....rrrr");

    // The width is counted in whole percent: 70% of 10 cells is 7 and a cell, not a hair
    // under 8 that loses its part cell.
    let one = EffectParams::Spiral(SpiralParams {
        count: 1,
        thickness: 0.7,
        ..still
    });
    let lit = (0..10)
        .filter(|&x| shade(&one, time(0, 1000), at(x as f32 / 9.0, 0.0), &[Rgb::RED], GRID).a > 0.0)
        .count();
    assert_eq!(lit, 8);
}

#[test]
fn spirals_take_xlights_3d_blend_grow_and_shrink() {
    // 2 stripes 8 cells apart, 8 × 49 / 100 + 1 = 4.92 cells wide.
    let still = SpiralParams {
        count: 2,
        speed: 0.0,
        thickness: 0.49,
        twist: 0.0,
        lean: -37.0,
        direction: Direction::Forward,
        ..SpiralParams::default()
    };
    let cover = |p: SpiralParams| -> Vec<f32> { spiral_cells(p, 0, 0).iter().take(5).map(|c| c.a).collect() };
    let near = |got: Vec<f32>, want: [f64; 5]| {
        for (g, w) in got.iter().zip(want) {
            assert!((f64::from(*g) - w).abs() < 1e-6, "{got:?} {want:?}");
        }
    };
    // "3D": each cell of a stripe covers the share of the width from it on, so the stripe
    // fades across; leaning the other way it fades the other way (a cell's share and up).
    let shaded = SpiralParams {
        shaded: true,
        ..still
    };
    near(cover(shaded), [1.0, 3.92 / 4.92, 2.92 / 4.92, 1.92 / 4.92, 0.0]);
    let other_way = SpiralParams { lean: 37.0, ..shaded };
    near(
        cover(other_way),
        [1.0 / 4.92, 2.0 / 4.92, 3.0 / 4.92, 4.0 / 4.92, 0.0],
    );
    assert_eq!(
        spiral_cells(shaded, 0, 0)[1].to_rgb8(),
        [(255.0f64 * 3.92 / 4.92).round() as u8, 0, 0],
        "the color itself stays whole"
    );

    // "Blend": every stripe takes the palette from the top row (red) down to the bottom
    // (49/50 of the way to green), and keeps its part cell: 5 wide.
    let blended = SpiralParams {
        gradient: true,
        ..still
    };
    let row = |y: u32| -> Vec<[u8; 3]> {
        spiral_cells(blended, 0, y)
            .iter()
            .filter(|c| c.a > 0.0)
            .map(|c| rgb(*c))
            .collect()
    };
    assert_eq!(row(49), vec![[255, 0, 0]; 10]);
    assert_eq!(row(0), vec![[5, 250, 0]; 10]);

    // Grow: stripes a cell wide take up whole cells of the 7 between them as the turn goes,
    // trunc(7 × 5/19) = 1 more at frame 5 and all 7 at the last. Shrink runs it backward.
    let thin = SpiralParams {
        thickness: 0.0,
        speed: 1.0,
        lean: 0.0,
        ..still
    };
    assert_eq!(spiral_row(thin, 250, 0), "....r.......g...");
    let growing = SpiralParams { grow: true, ..thin };
    assert_eq!(spiral_row(growing, 250, 0), "....rr......gg..");
    assert_eq!(spiral_row(growing, 950, 0), "rrrrrrrrgggggggg");
    let shrinking = SpiralParams { shrink: true, ..thin };
    assert_eq!(spiral_row(shrinking, 0, 0), "rrrrrrrrgggggggg");
    assert_eq!(spiral_row(shrinking, 950, 0), "r.......g.......");
    // Both: out for the first half of the turn, back for the second. Frame 9 is 9/19 of the
    // way, trunc(7 × 18/19) = 6 more, and the last frame none.
    let both = SpiralParams {
        grow: true,
        shrink: true,
        ..thin
    };
    assert_eq!(
        spiral_cells(both, 450, 0).iter().filter(|c| c.a > 0.0).count(),
        14
    );
    assert_eq!(spiral_row(both, 950, 0), "r.......g.......");
}

#[test]
fn fire_burns_from_the_bottom_and_is_reproducible() {
    let fire = EffectParams::Fire(FireParams::default());
    let pal: [Rgb; 0] = [];
    let grid = |ms: u64| -> Vec<Rgba> {
        (0..20)
            .flat_map(|y| (0..20).map(move |x| (x, y)))
            .map(|(x, y)| {
                shade(
                    &fire,
                    time(ms, 60_000),
                    at(x as f32 / 19.0, y as f32 / 19.0),
                    &pal,
                    GRID,
                )
            })
            .collect()
    };
    let frame = grid(5_000);
    assert_eq!(frame, grid(5_000), "the same frame every time");
    assert_ne!(frame, grid(5_050), "flames move");
    let row_heat = |y: usize| frame[y * 20..y * 20 + 20].iter().map(|c| c.a).sum::<f32>();
    assert!(row_heat(0) > 5.0, "the bottom is burning: {}", row_heat(0));
    assert!(row_heat(0) > row_heat(14), "cooler higher up");
    assert!(
        (17..20).all(|y| row_heat(y) == 0.0),
        "nothing above the flame height"
    );
    // Fire colors run from red (coolest) to white: green never exceeds red, blue never exceeds green.
    for c in &frame {
        assert!(c.g <= c.r + 1e-6 && c.b <= c.g + 1e-6, "{c:?}");
    }
    // The fire starts cold: in the first frame only the sparks at the very bottom glow.
    let first = grid(0);
    assert!(first[2 * 20..].iter().all(|c| c.a == 0.0));
}

#[test]
fn meteors_fall_with_fading_tails() {
    let meteors = EffectParams::Meteors(MeteorsParams {
        count: 1,
        speed: 1.0,
        length: 0.5,
        direction: MeteorDirection::Down,
    });
    let pal = [Rgb::WHITE];
    // A single-column canvas: the meteor's lane covers everything across.
    let column = Canvas { columns: 1, rows: 20 };
    let levels = |ms: u64| -> Vec<f32> {
        (0..=10)
            .map(|y| {
                shade(
                    &meteors,
                    time(ms, 60_000),
                    at(0.5, 1.0 - y as f32 / 10.0),
                    &pal,
                    column,
                )
                .a
            })
            .collect()
    };
    // Find the head at some time, then check the tail behind it fades and it moves down.
    let frame = levels(3_000);
    let head = frame.iter().cloned().fold(0.0, f32::max);
    assert!(head > 0.0, "{frame:?}");
    let head_at = frame.iter().position(|&a| a == head).unwrap();
    for pair in frame[..=head_at].windows(2) {
        assert!(pair[0] <= pair[1], "tail brightens toward the head: {frame:?}");
    }
    let later = levels(3_100);
    let later_head = later
        .iter()
        .position(|&a| a == later.iter().cloned().fold(0.0, f32::max));
    assert!(later_head.unwrap() >= head_at, "{frame:?} then {later:?}");
    assert_eq!(levels(3_000), frame, "reproducible");
}

#[test]
fn meteors_on_a_flat_strip_run_along_it() {
    let meteors = EffectParams::Meteors(MeteorsParams {
        count: 3,
        ..MeteorsParams::default()
    });
    let frames: Vec<Vec<u32>> = (0..20)
        .map(|k| lit(&meteors, time(k * 50, 60_000), &[Rgb::RED]))
        .collect();
    // Never all ten pixels lit at once (that would mean falling "across" the flat strip).
    assert!(frames.iter().all(|f| f.len() < 10), "{frames:?}");
    assert!(frames.iter().any(|f| !f.is_empty()), "{frames:?}");
}

#[test]
fn ripple_rings_spread_from_the_center() {
    let ripple = EffectParams::Ripple(RippleParams {
        speed: 1.0,
        spacing: 0.5,
        thickness: 0.1,
    });
    let pal = [Rgb::RED, Rgb::BLUE];
    // Distance from the center, 1.0 at the corners, along the diagonal.
    let px = |r: f32| at(0.5 + r * 0.5, 0.5 + r * 0.5);
    let shade_at = |ms: u64, r: f32| shade(&ripple, time(ms, 10_000), px(r), &pal, GRID);
    // After 0.3 s the first ring is at r = 0.3.
    assert!(shade_at(300, 0.3).a > 0.99);
    assert_eq!(rgb(shade_at(300, 0.3)), [255, 0, 0]);
    assert_eq!(shade_at(300, 0.6), Rgba::CLEAR, "not there yet");
    assert_eq!(shade_at(300, 0.1), Rgba::CLEAR, "between rings");
    // After 0.8 s the first ring is at 0.8 and the second (blue) at 0.3.
    assert_eq!(rgb(shade_at(800, 0.8)), [255, 0, 0]);
    assert_eq!(rgb(shade_at(800, 0.3)), [0, 0, 255]);
}

#[test]
fn hostile_settings_never_panic_or_produce_garbage() {
    let params = [
        EffectParams::Chase(ChaseParams {
            speed: f32::NAN,
            width: f32::INFINITY,
            bands: u32::MAX,
            ..ChaseParams::default()
        }),
        EffectParams::Bars(BarsParams {
            count: 0,
            speed: f32::MAX,
            ..BarsParams::default()
        }),
        EffectParams::Bars(BarsParams {
            count: u32::MAX,
            motion: BarsMotion::Split,
            fill: f32::NAN,
            center: f32::INFINITY,
            ..BarsParams::default()
        }),
        EffectParams::Wave(WaveParams {
            cycles: f32::NEG_INFINITY,
            speed: -1e30,
            height: f32::NAN,
            thickness: -5.0,
            direction: Direction::Reverse,
        }),
        EffectParams::Twinkle(TwinkleParams {
            density: f32::NAN,
            rate: f32::INFINITY,
        }),
        EffectParams::Spiral(SpiralParams {
            count: 0,
            twist: f32::NAN,
            ..SpiralParams::default()
        }),
        EffectParams::Meteors(MeteorsParams {
            count: u32::MAX,
            speed: 0.0,
            length: f32::NAN,
            direction: MeteorDirection::Left,
        }),
        EffectParams::Ripple(RippleParams {
            speed: f32::NAN,
            spacing: 0.0,
            thickness: f32::INFINITY,
        }),
        EffectParams::ColorWash(ColorWashParams {
            cycles: f32::INFINITY,
            gradient: Gradient::Horizontal,
        }),
        EffectParams::Fire(FireParams {
            height: f32::NAN,
            sparks: 99.0,
        }),
    ];
    for p in &params {
        for ms in [0, 1, 999_999_999] {
            for i in 0..10 {
                let c = shade_pixel(p, time(ms, 1_000_000_000), &strip(i), &[], u64::MAX, STRIP);
                let ok = |x: f32| x.is_finite() && (0.0..=1.0).contains(&x);
                assert!(ok(c.r) && ok(c.g) && ok(c.b) && ok(c.a), "{p:?} at {ms}: {c:?}");
            }
        }
    }
}

#[test]
fn ripple_rings_stay_crisp_hours_into_an_effect() {
    // Rings 25 ms apart in time: 400,000 ring periods later (about 2.8 hours) the picture is the
    // same. With an f32 front the ring position drifts by more than the ring thickness.
    let ripple = EffectParams::Ripple(RippleParams {
        speed: 20.0,
        spacing: 0.5,
        thickness: 0.01,
    });
    let length = 4 * 60 * 60 * 1000;
    let coverage = |ms: u64| -> Vec<f32> {
        (0..200)
            .map(|i| {
                let r = i as f32 / 200.0;
                shade(
                    &ripple,
                    time(ms, length),
                    at(0.5 + r * 0.5, 0.5 + r * 0.5),
                    &[Rgb::WHITE],
                    GRID,
                )
                .a
            })
            .collect()
    };
    let early = coverage(1013);
    assert!(early.iter().any(|&a| a > 0.5), "rings are drawn");
    let later = coverage(1013 + 400_000 * 25);
    assert!(
        early.iter().zip(&later).all(|(a, b)| (a - b).abs() < 1e-3),
        "{early:?}\n{later:?}"
    );
}

#[test]
fn the_renderer_clamps_to_the_settings_table() {
    let at_max = EffectParams::Chase(ChaseParams {
        speed: 50.0,
        bands: 1000,
        ..ChaseParams::default()
    });
    let beyond = EffectParams::Chase(ChaseParams {
        speed: 5000.0,
        bands: 100_000,
        ..ChaseParams::default()
    });
    for ms in [0, 7, 333, 1234] {
        for i in 0..10 {
            assert_eq!(
                shade(&beyond, time(ms, 5000), strip(i), &[Rgb::RED], STRIP),
                shade(&at_max, time(ms, 5000), strip(i), &[Rgb::RED], STRIP)
            );
        }
    }
    let count = MeteorsParams::SETTINGS
        .iter()
        .find(|s| s.key == "count")
        .unwrap()
        .range;
    assert!(matches!(count, SettingRange::Int { max, .. } if max == pf_render::MAX_METEORS));
}
