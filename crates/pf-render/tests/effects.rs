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
    assert_eq!(
        shade(&dimming, time(250, 1000), strip(0), &[Rgb::WHITE], STRIP).a,
        0.75
    );
    assert_eq!(
        rgb(shade(&dimming, time(500, 1000), strip(0), &[Rgb::WHITE], STRIP)),
        [128; 3]
    );
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

#[test]
fn bars_slide_along_the_chosen_axis() {
    let bars = EffectParams::Bars(BarsParams {
        count: 2,
        speed: 0.25,
        axis: Axis::Vertical,
        direction: Direction::Forward,
    });
    let pal = [Rgb::RED, Rgb::GREEN];
    let color = |ms: u64, v: f32| rgb(shade(&bars, time(ms, 10_000), at(0.3, v), &pal, GRID));
    // Bars of half a slot: v in [0, 0.25) red, [0.5, 0.75) green.
    assert_eq!(color(0, 0.1), [255, 0, 0]);
    assert_eq!(color(0, 0.3), [0, 0, 0]);
    assert_eq!(color(0, 0.6), [0, 255, 0]);
    // After 1 s the bars moved up a quarter.
    assert_eq!(color(1000, 0.3), [255, 0, 0]);
    assert_eq!(color(1000, 0.1), [0, 0, 0]);

    let across = EffectParams::Bars(BarsParams {
        count: 2,
        speed: 0.0,
        axis: Axis::Horizontal,
        direction: Direction::Forward,
    });
    assert_eq!(
        rgb(shade(&across, time(0, 1000), at(0.6, 0.0), &pal, GRID)),
        [0, 255, 0]
    );
    assert_eq!(
        rgb(shade(&across, time(0, 1000), at(0.3, 0.0), &pal, GRID)),
        [0, 0, 0]
    );
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

#[test]
fn spiral_stripes_rotate() {
    let spiral = EffectParams::Spiral(SpiralParams {
        count: 2,
        speed: 0.5,
        thickness: 0.5,
        twist: 1.0,
        direction: Direction::Forward,
    });
    let pal = [Rgb::RED, Rgb::GREEN];
    let color = |ms: u64, u: f32, v: f32| rgb(shade(&spiral, time(ms, 10_000), at(u, v), &pal, GRID));
    // s = (u + v - rotation) * 2: stripe 0 for s in [0, 0.5), gap [0.5, 1), stripe 1 in [1, 1.5).
    assert_eq!(color(0, 0.1, 0.1), [255, 0, 0]);
    assert_eq!(color(0, 0.2, 0.2), [0, 0, 0]);
    assert_eq!(color(0, 0.3, 0.3), [0, 255, 0]);
    // Same diagonal, same color: stripes slant.
    assert_eq!(color(0, 0.0, 0.6), [0, 255, 0]);
    // After 0.5 s the stripes turned a quarter (rotation 0.25).
    assert_eq!(color(500, 0.2, 0.2), [255, 0, 0]);
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
