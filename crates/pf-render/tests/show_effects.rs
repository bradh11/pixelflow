//! The show effects: Impact, Wipe, Lightning, Pulse, Sing, Color Shift, and Chase from prop to
//! prop, drawn on grids, on small shows, and on the demo show.

use pf_model::{Generator, Group, Prop, RenderStyle, ShapeSource, Show, Transform, Vec3};
use pf_render::audio::{AudioSource, AudioTrack};
use pf_render::{Canvas, Colors, EffectTime, Pixel, RenderContext, Renderer, Rgba, Shade, Shader};
use pf_sequence::*;
use std::f32::consts::TAU;
use std::sync::Arc;

const FRAME_MS: u32 = 25;

/// A `columns` × `rows` grid of cells, bottom row first.
fn grid(columns: u32, rows: u32) -> (Canvas, Vec<Pixel>) {
    let at = |i: u32, n: u32| if n <= 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
    let pixels = (0..rows)
        .flat_map(|y| {
            (0..columns).map(move |x| Pixel {
                u: at(x, columns),
                v: at(y, rows),
                index: y * columns + x,
                count: columns * rows,
            })
        })
        .collect();
    (Canvas { columns, rows }, pixels)
}

/// `params` drawn on every pixel of `grid` at `t_ms` into an effect of `length_ms`.
fn draw(
    params: EffectParams,
    palette: &[Rgb],
    seed: u64,
    (canvas, pixels): &(Canvas, Vec<Pixel>),
    t_ms: u64,
    length_ms: u64,
    cx: &RenderContext,
) -> Vec<Rgba> {
    let time = EffectTime::within(0, length_ms, t_ms).with_frame_ms(FRAME_MS);
    let shader = Shader::in_context(&params, &time, Colors::new(palette), seed, *canvas, cx);
    pixels.iter().map(|px| shader.shade(px)).collect()
}

fn shade(
    params: EffectParams,
    palette: &[Rgb],
    grid: &(Canvas, Vec<Pixel>),
    t_ms: u64,
    length_ms: u64,
) -> Vec<Rgba> {
    draw(
        params,
        palette,
        7,
        grid,
        t_ms,
        length_ms,
        &RenderContext::default(),
    )
}

/// How bright a pixel shows over black (its strongest channel), 0–1.
fn bright(c: &Rgba) -> f32 {
    c.r.max(c.g).max(c.b) * c.a.clamp(0.0, 1.0)
}

fn near(a: f32, b: f32, within: f32) -> bool {
    (a - b).abs() <= within
}

// ---------------------------------------------------------------------------------------------
// Impact

#[test]
fn impact_hits_full_and_fades_by_its_curve() {
    let g = grid(10, 10);
    let impact = |p: ImpactParams| EffectParams::Impact(p);
    let level = |p: ImpactParams, t: u64| bright(&shade(impact(p), &[Rgb::RED], &g, t, 1000)[0]);
    let exp = ImpactParams::default();
    assert!(near(level(exp, 0), 1.0, 1e-4), "full at the hit");
    assert!(level(exp, 999) < 0.01, "dark by the end");
    let even = ImpactParams {
        decay: ImpactDecay::Linear,
        ..exp
    };
    assert!(near(level(even, 500), 0.5, 0.01));
    assert!(level(exp, 200) < level(even, 200) - 0.2, "fast, then slow");
    let punch = ImpactParams {
        decay: ImpactDecay::Punch,
        ..exp
    };
    assert!(level(punch, 100) < level(punch, 250) - 0.2, "dips, then rebounds");
    // Hold: full brightness, then the fade over what's left.
    let held = ImpactParams { hold: 400.0, ..even };
    assert!(near(level(held, 300), 1.0, 1e-4));
    assert!(near(level(held, 700), 0.5, 0.01));
    // White by default; the first palette color when asked.
    let white = &shade(impact(exp), &[Rgb::RED], &g, 0, 1000)[0];
    assert!(white.g > 0.99 && white.b > 0.99);
    let red = &shade(
        impact(ImpactParams {
            color: HitColor::Palette,
            ..exp
        }),
        &[Rgb::RED],
        &g,
        0,
        1000,
    )[0];
    assert!(red.r > 0.99 && red.g < 0.01);
    // Shifting colors: from white through the palette.
    let shift = impact(ImpactParams {
        color_shift: true,
        decay: ImpactDecay::Linear,
        ..exp
    });
    let late = &shade(shift, &[Rgb::BLUE], &g, 900, 1000)[0];
    assert!(late.b > 0.95 && late.r < 0.15, "{late:?}");
}

#[test]
fn an_impact_blooms_out_from_its_point() {
    let g = grid(21, 21);
    let p = EffectParams::Impact(ImpactParams {
        bloom: 400.0,
        decay: ImpactDecay::Linear,
        ..ImpactParams::default()
    });
    let center = (10 * 21 + 10) as usize;
    let early = shade(p.clone(), &[], &g, 100, 2000);
    assert!(bright(&early[center]) > 0.9, "lit at the hit point");
    assert_eq!(bright(&early[0]), 0.0, "not yet at the corner");
    let lit = |frame: &[Rgba]| frame.iter().filter(|c| bright(c) > 0.0).count();
    let later = shade(p.clone(), &[], &g, 300, 2000);
    assert!(lit(&later) > lit(&early), "spreading");
    assert!(
        bright(&shade(p, &[], &g, 500, 2000)[0]) > 0.7,
        "everywhere once it has bloomed"
    );
}

// ---------------------------------------------------------------------------------------------
// Wipe

fn wipe(p: WipeParams) -> EffectParams {
    EffectParams::Wipe(p)
}

/// Each column's brightness on a row of a wiped grid.
fn columns_lit(p: WipeParams, t: u64) -> Vec<f32> {
    let g = grid(11, 1);
    shade(wipe(p), &[Rgb::WHITE], &g, t, 1000)
        .iter()
        .map(bright)
        .collect()
}

#[test]
fn wipes_sweep_on_hold_and_clear() {
    let crisp = WipeParams {
        softness: 0.0,
        ..WipeParams::default()
    };
    // Half the effect to cross: a quarter of the way through, the first half is lit.
    let quarter = columns_lit(crisp, 250);
    assert!(quarter[..5].iter().all(|&b| b > 0.99), "{quarter:?}");
    assert!(quarter[6..].iter().all(|&b| b == 0.0), "{quarter:?}");
    assert!(columns_lit(crisp, 0).iter().all(|&b| b == 0.0), "starts dark");
    assert!(columns_lit(crisp, 750).iter().all(|&b| b > 0.99), "then holds");
    let left = WipeParams {
        direction: Sweep::RightToLeft,
        ..crisp
    };
    let from_right = columns_lit(left, 250);
    assert!(from_right[10] > 0.99 && from_right[0] == 0.0);
    // Off: lit, cleared from where the sweep starts.
    let off = WipeParams {
        mode: WipeMode::Off,
        ..crisp
    };
    let clearing = columns_lit(off, 250);
    assert!(clearing[0] == 0.0 && clearing[10] > 0.99, "{clearing:?}");
    assert!(columns_lit(off, 750).iter().all(|&b| b == 0.0));
    // On, then off.
    let both = WipeParams {
        mode: WipeMode::OnOff,
        duration: 30.0,
        ..crisp
    };
    assert!(columns_lit(both, 500).iter().all(|&b| b > 0.99));
    let ending = columns_lit(both, 850);
    assert!(ending[0] == 0.0 && ending[10] > 0.99, "{ending:?}");
    assert!(
        columns_lit(both, 990)[..10].iter().all(|&b| b < 0.05),
        "all but gone"
    );
    // A soft edge shades the pixels at the edge.
    let soft = columns_lit(WipeParams::default(), 250);
    assert!(soft.iter().any(|&b| b > 0.05 && b < 0.95), "{soft:?}");
}

#[test]
fn a_bar_wipe_sweeps_a_bar_across() {
    let bar = WipeParams {
        softness: 0.0,
        band: 0.2,
        duration: 80.0,
        ..WipeParams::default()
    };
    let middle = columns_lit(bar, 480);
    let lit: Vec<usize> = (0..11).filter(|&i| middle[i] > 0.5).collect();
    assert!((1..=3).contains(&lit.len()), "{middle:?}");
    assert!(lit.iter().all(|&i| (3..=8).contains(&i)), "{middle:?}");
    assert!(columns_lit(bar, 0).iter().all(|&b| b == 0.0));
    assert!(
        columns_lit(bar, 900).iter().all(|&b| b == 0.0),
        "gone once it has crossed"
    );
}

#[test]
fn radial_and_center_wipes_grow_from_the_middle() {
    let g = grid(21, 21);
    let at = |x: u32, y: u32| (y * 21 + x) as usize;
    for direction in [Sweep::Radial, Sweep::CenterOut] {
        let frame = shade(
            wipe(WipeParams {
                direction,
                softness: 0.0,
                ..WipeParams::default()
            }),
            &[],
            &g,
            150,
            1000,
        );
        assert!(bright(&frame[at(10, 10)]) > 0.99, "{direction:?}");
        assert_eq!(bright(&frame[at(0, 10)]), 0.0, "{direction:?}");
    }
}

/// Props at `xs` (layout units), each a short horizontal strip, all in one group.
fn strips_at(xs: &[f32]) -> Show {
    let mut show = Show::new("t");
    for (i, &x) in xs.iter().enumerate() {
        let mut prop = Prop::new(
            format!("Strip {i}"),
            ShapeSource::Generator(Generator::Line {
                nodes: 4,
                length: 1.0,
            }),
        );
        prop.transform = Transform {
            position: Vec3::new(x, 0.0, 0.0),
            ..Transform::default()
        };
        show.props.push(prop);
    }
    let mut group = Group::new("All");
    group.members = show.props.iter().map(|p| p.id.into()).collect();
    show.groups.push(group);
    show
}

fn renderer(show: &Show) -> Renderer {
    Renderer::new(show, &pf_mapping::map_show(show).0)
}

/// Each prop's brightest channel, prop by prop, at `t_ms`.
fn props_lit(r: &mut Renderer, show: &Show, seq: &Sequence, t_ms: u64) -> Vec<u8> {
    let mut frame = vec![0u8; r.frame_len()];
    r.render(seq, t_ms, &mut frame);
    let per = frame.len() / show.props.len();
    frame.chunks(per).map(|p| *p.iter().max().unwrap()).collect()
}

fn group_sequence(show: &Show, effect: Effect) -> Sequence {
    let mut seq = Sequence::new("s", 10_000);
    seq.frame_ms = FRAME_MS;
    let mut row = Row::new(Target::Group(show.groups[0].id));
    row.layers[0].effects = vec![effect];
    seq.rows.push(row);
    seq
}

#[test]
fn a_wipe_on_a_group_drawn_per_preview_crosses_the_layout() {
    // Members listed right to left: the sweep goes by where they are, not their order.
    let show = strips_at(&[8.0, 0.0, 4.0]);
    let mut effect = Effect::new(EffectKind::Wipe, 0, 1000).with_params(wipe(WipeParams {
        softness: 0.0,
        duration: 100.0,
        ..WipeParams::default()
    }));
    effect.render_style = RenderStyle::PerPreview;
    let seq = group_sequence(&show, effect);
    let mut r = renderer(&show);
    // A third of the way: the left prop (listed second) only.
    assert_eq!(props_lit(&mut r, &show, &seq, 300), vec![0, 255, 0]);
    assert_eq!(props_lit(&mut r, &show, &seq, 650), vec![0, 255, 255]);
    assert_eq!(props_lit(&mut r, &show, &seq, 999), vec![255, 255, 255]);
}

// ---------------------------------------------------------------------------------------------
// Lightning

/// The whole target's flash, every 5 ms for `seconds`, on a line (where lightning flashes).
fn flashes(p: LightningParams, seed: u64, seconds: u64) -> Vec<f32> {
    let g = grid(10, 1);
    let length = seconds * 1000;
    (0..length / 5)
        .map(|k| {
            let frame = draw(
                EffectParams::Lightning(p),
                &[],
                seed,
                &g,
                k * 5,
                length,
                &RenderContext::default(),
            );
            bright(&frame[0])
        })
        .collect()
}

#[test]
fn lightning_strikes_flicker_and_fade_the_same_way_every_time() {
    let p = LightningParams::default();
    let levels = flashes(p, 42, 10);
    assert_eq!(levels, flashes(p, 42, 10), "the same seed, the same strikes");
    assert_ne!(levels, flashes(p, 43, 10), "another seed, other strikes");
    assert!(levels.iter().any(|&l| l > 0.95), "full-brightness strokes");
    assert!(
        levels.iter().filter(|&&l| l < 0.01).count() > levels.len() / 3,
        "dark between strikes"
    );
    // Strikes start at least 200 ms apart at one a second, so a flash coming back within 150 ms
    // of fading below half is a re-strike.
    let mut restrikes = 0;
    let mut faded_at = None;
    for (k, pair) in levels.windows(2).enumerate() {
        if pair[0] >= 0.5 && pair[1] < 0.5 {
            faded_at = Some(k);
        }
        if pair[0] < 0.5
            && pair[1] >= 0.5
            && let Some(at) = faded_at
            && (k - at) * 5 <= 150
        {
            restrikes += 1;
        }
    }
    assert!(restrikes >= 2, "{restrikes} re-strikes");
    // About one strike a second; lit more of the time with more density.
    let starts = levels.windows(2).filter(|w| w[0] < 0.01 && w[1] >= 0.3).count();
    assert!((6..=14).contains(&starts), "{starts} strikes in 10 seconds");
    let lit = |levels: &[f32]| levels.iter().filter(|&&l| l > 0.3).count();
    let one = lit(&levels);
    let dense = lit(&flashes(LightningParams { density: 4.0, ..p }, 42, 10));
    assert!(dense > one, "{dense} vs {one}");
}

/// The frame with the most lit cells over the first `seconds`, on a matrix.
fn brightest_bolt(p: LightningParams, g: &(Canvas, Vec<Pixel>), seconds: u64) -> Vec<Rgba> {
    let lit = |f: &[Rgba]| f.iter().filter(|c| bright(c) > 0.5).count();
    (0..seconds * 1000 / 25)
        .map(|k| {
            draw(
                EffectParams::Lightning(p),
                &[],
                3,
                g,
                k * 25,
                seconds * 1000,
                &RenderContext::default(),
            )
        })
        .max_by_key(|f| lit(f))
        .unwrap()
}

#[test]
fn lightning_draws_bolts_on_a_matrix_and_flashes_on_a_line() {
    let g = grid(30, 30);
    let p = LightningParams {
        branches: 0.0,
        glow: 0.2,
        ..LightningParams::default()
    };
    let frame = brightest_bolt(p, &g, 4);
    let bolt: Vec<usize> = (0..frame.len()).filter(|&i| bright(&frame[i]) > 0.5).collect();
    assert!(
        bolt.len() >= 30 && bolt.len() < 300,
        "{} cells in the bolt",
        bolt.len()
    );
    let rows: Vec<usize> = bolt.iter().map(|i| i / 30).collect();
    assert!(
        rows.contains(&29) && rows.iter().any(|&r| r <= 2),
        "from the top to the bottom"
    );
    // The rest of the sky glows faintly.
    let glow = frame.iter().map(bright).filter(|&b| b > 0.0 && b < 0.3).count();
    assert!(glow > 500, "{glow}");
    // Forks light more of the matrix.
    let forked = brightest_bolt(LightningParams { branches: 1.0, ..p }, &g, 4);
    let lit = |f: &[Rgba]| f.iter().filter(|c| bright(c) > 0.5).count();
    assert!(lit(&forked) > lit(&frame), "{} vs {}", lit(&forked), lit(&frame));
    // Flash only: the whole matrix at once.
    let flash = brightest_bolt(
        LightningParams {
            flash_only: true,
            ..p
        },
        &g,
        4,
    );
    assert!(
        flash
            .iter()
            .all(|c| bright(c) > 0.5 && bright(c) == bright(&flash[0]))
    );
}

// ---------------------------------------------------------------------------------------------
// Pulse

fn track(name: &str, kind: TimingKind, marks: Vec<Mark>) -> TimingTrack {
    TimingTrack::new(name, kind, marks)
}

fn beats(every_ms: u64, count: u64) -> TimingTrack {
    track(
        "Beats",
        TimingKind::Beats,
        (0..count)
            .map(|k| Mark::new(k * every_ms, k * every_ms + 50, ""))
            .collect(),
    )
}

fn one_pixel() -> (Canvas, Vec<Pixel>) {
    grid(1, 1)
}

#[test]
fn a_pulse_breathes_on_a_timing_tracks_marks() {
    let tracks = [beats(500, 8)];
    let cx = RenderContext::new(None, &tracks, FRAME_MS);
    let p = PulseParams {
        timing_track: Some(tracks[0].id),
        min: 0.1,
        ..PulseParams::default()
    };
    let g = one_pixel();
    let at = |p: PulseParams, t: u64| {
        draw(
            EffectParams::Pulse(p),
            &[Rgb::RED, Rgb::GREEN],
            1,
            &g,
            t,
            4000,
            &cx,
        )[0]
    };
    assert!(near(bright(&at(p, 0)), 1.0, 1e-4), "brightest on the beat");
    assert!(near(bright(&at(p, 250)), 0.1, 1e-3), "lowest between beats");
    assert!(bright(&at(p, 400)) > bright(&at(p, 300)), "breathing back in");
    // Each beat takes the next palette color.
    assert!(at(p, 0).r > 0.99 && at(p, 500).g > 0.99);
    // Shapes.
    let saw = PulseParams {
        shape: PulseShape::Saw,
        ..p
    };
    assert!(near(bright(&at(saw, 250)), 0.55, 0.01));
    let square = PulseParams {
        shape: PulseShape::Square,
        ..p
    };
    assert!(near(bright(&at(square, 200)), 1.0, 1e-4) && near(bright(&at(square, 300)), 0.1, 1e-4));
    let heart = PulseParams {
        shape: PulseShape::Heartbeat,
        ..p
    };
    let dub = bright(&at(heart, 140));
    assert!(dub > 0.5 && dub < 0.7 && bright(&at(heart, 75)) < dub, "{dub}");
    // Without a timing track it pulses twice a second; with one the sequence lost, it rests low.
    let free = PulseParams {
        timing_track: None,
        ..p
    };
    assert!(near(bright(&at(free, 1000)), 1.0, 1e-4) && near(bright(&at(free, 1250)), 0.1, 1e-3));
    let lost = PulseParams {
        timing_track: Some(TimingTrackId::new()),
        ..p
    };
    assert!(near(bright(&at(lost, 1000)), 0.1, 1e-4));
}

const RATE: u32 = 44_100;

/// A 440 Hz tone at `amplitude` for each of `parts` (seconds, amplitude).
fn song(parts: &[(f32, f32)]) -> Arc<AudioTrack> {
    let mut samples = Vec::new();
    for &(seconds, amplitude) in parts {
        let n = (seconds * RATE as f32) as usize;
        let start = samples.len();
        samples.extend((0..n).map(|i| amplitude * (TAU * 440.0 * (start + i) as f32 / RATE as f32).sin()));
    }
    Arc::new(pf_analysis::audio_track(samples, RATE, FRAME_MS))
}

#[test]
fn a_pulse_follows_the_music_with_its_attack_and_release() {
    // A loud second, a silent one, then loud again.
    let music = song(&[(1.0, 1.0), (1.0, 0.0), (1.0, 1.0)]);
    let cx = RenderContext::new(Some(pf_render::Audio::new(&music, FRAME_MS)), &[], FRAME_MS);
    let g = one_pixel();
    let level = |p: PulseParams, t: u64| bright(&draw(EffectParams::Pulse(p), &[], 1, &g, t, 3000, &cx)[0]);
    let p = PulseParams {
        source: PulseSource::Level,
        min: 0.0,
        attack: 0.0,
        release: 0.0,
        ..PulseParams::default()
    };
    assert!(level(p, 500) > 0.9, "{}", level(p, 500));
    assert!(level(p, 1500) < 0.1, "{}", level(p, 1500));
    // A slow release falls back gradually after the music stops.
    let slow = PulseParams { release: 600.0, ..p };
    let (after, later) = (level(slow, 1150), level(slow, 1700));
    assert!(after > level(p, 1150) + 0.2, "{after}");
    assert!(after > later + 0.2 && later > 0.0, "{after} then {later}");
    // A slow attack rises gradually when it comes back.
    let gentle = PulseParams { attack: 400.0, ..p };
    assert!(level(gentle, 2075) < level(p, 2075) - 0.2);
    assert!(level(gentle, 2900) > 0.85);
    // Between its lowest and highest.
    let ranged = PulseParams {
        min: 0.2,
        max: 0.6,
        ..p
    };
    assert!(near(level(ranged, 1500), 0.2, 0.05) && level(ranged, 500) <= 0.6 + 1e-4);
    // Without the music, it stays at its lowest, as in silence.
    let silent = RenderContext::default();
    assert!(near(
        bright(&draw(EffectParams::Pulse(ranged), &[], 1, &g, 500, 3000, &silent)[0]),
        0.2,
        1e-4
    ));
    // The bass and new sounds follow the music too.
    for source in [PulseSource::Bass, PulseSource::Onsets] {
        let p = PulseParams { source, ..p };
        assert!(level(p, 1500) <= level(p, 500), "{source:?}");
    }
    assert!(pf_render::audio::effect_follows_music(
        &Effect::new(EffectKind::Pulse, 0, 1).with_params(EffectParams::Pulse(PulseParams {
            source: PulseSource::Bass,
            ..PulseParams::default()
        }))
    ));
    assert!(!pf_render::audio::effect_follows_music(&Effect::new(
        EffectKind::Pulse,
        0,
        1
    )));
}

#[test]
fn a_pulse_with_the_music_renders_through_the_renderer() {
    let show = strips_at(&[0.0]);
    let effect = Effect::new(EffectKind::Pulse, 0, 3000).with_params(EffectParams::Pulse(PulseParams {
        source: PulseSource::Level,
        min: 0.0,
        release: 0.0,
        ..PulseParams::default()
    }));
    let seq = group_sequence(&show, effect);
    let mut r = renderer(&show);
    r.set_audio(AudioSource::ready(song(&[(1.0, 1.0), (1.0, 0.0)])));
    assert!(props_lit(&mut r, &show, &seq, 500)[0] > 230);
    assert!(props_lit(&mut r, &show, &seq, 1500)[0] < 25);
}

// ---------------------------------------------------------------------------------------------
// Sing

#[test]
fn sing_opens_the_mouth_by_the_phonemes() {
    let tracks = [track(
        "Lyrics (phonemes)",
        TimingKind::Phonemes,
        vec![
            Mark::new(0, 200, "MBP"),
            Mark::new(200, 400, "AI"),
            Mark::new(400, 600, "E"),
            Mark::new(600, 800, "FV"),
        ],
    )];
    let cx = RenderContext::new(None, &tracks, FRAME_MS);
    let sing = |mode, min| {
        EffectParams::Sing(SingParams {
            mode,
            timing_track: Some(tracks[0].id),
            min,
        })
    };
    let g = one_pixel();
    let level = |t| bright(&draw(sing(SingMode::Mouth, 0.0), &[], 1, &g, t, 2000, &cx)[0]);
    assert_eq!(level(100), 0.0, "lips shut");
    assert!(near(level(300), 1.0, 1e-4), "wide open");
    assert!(near(level(500), pf_render::openness(pf_model::Phoneme::E), 1e-4));
    assert!(level(700) < level(500) && level(700) > 0.0);
    assert_eq!(level(900), 0.0, "at rest after the last");
    let floor = bright(&draw(sing(SingMode::Mouth, 0.3), &[], 1, &g, 100, 2000, &cx)[0]);
    assert!(near(floor, 0.3, 1e-4), "the closed brightness");
    // The mouth bar: a band up the middle as wide as the mouth is open.
    let tall = grid(5, 21);
    let rows_lit = |t| {
        draw(sing(SingMode::BarMouth, 0.0), &[], 1, &tall, t, 2000, &cx)
            .iter()
            .filter(|c| bright(c) > 0.5)
            .count()
            / 5
    };
    assert_eq!(rows_lit(100), 0);
    assert_eq!(rows_lit(300), 21);
    let e = rows_lit(500);
    assert!((13..=17).contains(&e), "{e} rows for E");
    assert!(rows_lit(700) < e && rows_lit(700) > 0);
    // On a line, the bar runs along it.
    let line = grid(21, 1);
    let along = draw(sing(SingMode::BarMouth, 0.0), &[], 1, &line, 500, 2000, &cx);
    assert!(bright(&along[10]) > 0.9 && bright(&along[0]) == 0.0);
}

#[test]
fn sing_pops_words_and_fills_across_them() {
    let tracks = [track(
        "Lyrics (words)",
        TimingKind::Words,
        vec![Mark::new(0, 1000, "la"), Mark::new(1000, 2000, "lo")],
    )];
    let cx = RenderContext::new(None, &tracks, FRAME_MS);
    let sing = |mode| {
        EffectParams::Sing(SingParams {
            mode,
            timing_track: Some(tracks[0].id),
            min: 0.0,
        })
    };
    let g = one_pixel();
    let pop = |t| {
        draw(
            sing(SingMode::WordPop),
            &[Rgb::RED, Rgb::BLUE],
            1,
            &g,
            t,
            3000,
            &cx,
        )[0]
    };
    assert!(
        near(bright(&pop(0)), 1.0, 1e-4) && bright(&pop(900)) < 0.1,
        "a flash fading over the word"
    );
    assert!(pop(1000).b > 0.99, "the next word, the next color");
    // Karaoke: the first color filling left to right over each word.
    let line = grid(11, 1);
    let fill = draw(
        sing(SingMode::Karaoke),
        &[Rgb::RED, Rgb::BLUE],
        1,
        &line,
        500,
        3000,
        &cx,
    );
    assert!(fill[..5].iter().all(|c| c.r > 0.99 && c.a > 0.99), "{fill:?}");
    assert!(
        fill[6..].iter().all(|c| bright(c) == 0.0),
        "the rest still to sing"
    );
    // Without its timing track, it doesn't sing.
    let silent = draw(
        sing(SingMode::Mouth),
        &[],
        1,
        &g,
        500,
        3000,
        &RenderContext::default(),
    );
    assert_eq!(bright(&silent[0]), 0.0);
}

#[test]
fn sing_follows_made_up_syllables_and_their_vowels() {
    let tracks = [track(
        "Lyrics (syllables)",
        TimingKind::Custom,
        vec![
            Mark::new(0, 400, "zoo"),
            Mark::new(400, 800, "bee"),
            Mark::new(800, 1200, "mmm"),
        ],
    )];
    let open = |t| pf_render::mouth_open(&tracks[0], t);
    assert!(open(300) > 0.5, "zoo opens on its vowel: {}", open(300));
    assert!(open(399) < 0.1, "and closes before the next syllable");
    assert!(open(700) > 0.5);
    assert!(open(1000) < open(700), "hummed");
}

// ---------------------------------------------------------------------------------------------
// Chase from prop to prop

#[test]
fn a_chase_goes_prop_by_prop_in_group_order_or_left_to_right() {
    let show = strips_at(&[8.0, 0.0, 4.0]);
    let marks = beats(100, 30);
    let chase = |order| {
        Effect::new(EffectKind::Chase, 0, 3000).with_params(EffectParams::Chase(ChaseParams {
            order,
            timing_track: Some(marks.id),
            ..ChaseParams::default()
        }))
    };
    let mut r = renderer(&show);
    let mut seq = group_sequence(&show, chase(ChaseOrder::Props));
    seq.timing_tracks.push(marks.clone());
    // One prop per mark, in the group's order.
    let lit = |r: &mut Renderer, seq: &Sequence, t| props_lit(r, &show, seq, t);
    assert_eq!(lit(&mut r, &seq, 50), vec![255, 0, 0]);
    assert_eq!(lit(&mut r, &seq, 150), vec![0, 255, 0]);
    assert_eq!(lit(&mut r, &seq, 250), vec![0, 0, 255]);
    assert_eq!(lit(&mut r, &seq, 350), vec![255, 0, 0], "and around again");
    // Left to right by where they stand: the second-listed prop is leftmost.
    let mut across = group_sequence(&show, chase(ChaseOrder::PropsAcross));
    across.timing_tracks.push(marks.clone());
    assert_eq!(lit(&mut r, &across, 50), vec![0, 255, 0]);
    assert_eq!(lit(&mut r, &across, 150), vec![0, 0, 255]);
    assert_eq!(lit(&mut r, &across, 250), vec![255, 0, 0]);
    // At a rate instead: one trip across the three props a second.
    let mut rate = chase(ChaseOrder::Props);
    rate.params = EffectParams::Chase(ChaseParams {
        order: ChaseOrder::Props,
        ..ChaseParams::default()
    });
    let rate = group_sequence(&show, rate);
    assert_eq!(lit(&mut r, &rate, 100), vec![255, 0, 0]);
    assert_eq!(lit(&mut r, &rate, 500), vec![0, 255, 0]);
    assert_eq!(lit(&mut r, &rate, 900), vec![0, 0, 255]);
}

#[test]
fn a_chase_steps_along_the_pixels_on_marks() {
    let g = grid(10, 1);
    let tracks = [beats(100, 30)];
    let cx = RenderContext::new(None, &tracks, FRAME_MS);
    let p = EffectParams::Chase(ChaseParams {
        width: 0.2,
        timing_track: Some(tracks[0].id),
        order: ChaseOrder::Across,
        ..ChaseParams::default()
    });
    let lit = |t| -> Vec<usize> {
        let frame = draw(p.clone(), &[], 1, &g, t, 3000, &cx);
        (0..10).filter(|&i| bright(&frame[i]) > 0.5).collect()
    };
    // A band's length (a fifth of the prop) per mark; still between marks.
    let first = lit(50);
    assert_eq!(first, lit(90));
    let second = lit(150);
    assert_ne!(first, second);
    assert_eq!(second.len(), 2);
    assert_eq!(
        second.iter().map(|i| (i + 8) % 10).collect::<Vec<_>>(),
        first,
        "{first:?} → {second:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Color Shift

#[test]
fn a_color_shift_changes_through_the_palette() {
    let g = grid(11, 1);
    let shift =
        |p: ColorShiftParams, colors: &[Rgb], t| shade(EffectParams::ColorShift(p), colors, &g, t, 1000);
    let even = ColorShiftParams {
        ease: ShiftEase::Linear,
        duration: 50.0,
        ..ColorShiftParams::default()
    };
    let rb = [Rgb::RED, Rgb::BLUE];
    assert!(shift(even, &rb, 0)[0].r > 0.99);
    let middle = shift(even, &rb, 250)[0];
    assert!(
        near(middle.r, 0.5, 0.02) && near(middle.b, 0.5, 0.02),
        "{middle:?}"
    );
    assert!(shift(even, &rb, 600)[0].b > 0.99, "then holds the new color");
    let instant = ColorShiftParams {
        ease: ShiftEase::Instant,
        ..even
    };
    assert!(shift(instant, &rb, 25)[0].b > 0.99);
    // Eased: slower at the ends than an even change.
    let eased = ColorShiftParams {
        ease: ShiftEase::Smooth,
        ..even
    };
    assert!(shift(eased, &rb, 50)[0].b < shift(even, &rb, 50)[0].b);
    // Three colors: the second change halfway through.
    let rgb = [Rgb::RED, Rgb::GREEN, Rgb::BLUE];
    let quick = ColorShiftParams {
        duration: 10.0,
        ..even
    };
    assert!(shift(quick, &rgb, 400)[0].g > 0.99);
    assert!(shift(quick, &rgb, 700)[0].b > 0.99);
    // Staggered all the way: the new color travels left to right.
    let sweep = ColorShiftParams {
        ease: ShiftEase::Instant,
        stagger: 100.0,
        ..even
    };
    let crossing = shift(sweep, &rb, 125);
    assert!(crossing[0].b > 0.99 && crossing[10].r > 0.99, "{crossing:?}");
    let down = shift(
        ColorShiftParams {
            direction: Sweep::RightToLeft,
            ..sweep
        },
        &rb,
        125,
    );
    assert!(down[10].b > 0.99 && down[0].r > 0.99);
}

// ---------------------------------------------------------------------------------------------
// The demo show

fn demo() -> Show {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/shows/demo.pixelflow.json"
    ))
    .unwrap();
    pf_model::show_from_json(&text).unwrap()
}

#[test]
fn every_show_effect_lights_the_demo_show() {
    let mut show = demo();
    let mut house = Group::new("Whole house");
    house.members = show.props.iter().map(|p| p.id.into()).collect();
    show.groups.push(house);
    let house = show.groups.last().unwrap().id;
    let palette = [Rgb::RED, Rgb::GREEN, Rgb::BLUE];
    let mut seq = Sequence::new("Demo", 4000);
    seq.frame_ms = FRAME_MS;
    let words = track(
        "Lyrics (words)",
        TimingKind::Words,
        vec![
            Mark::new(0, 600, "fa"),
            Mark::new(700, 1300, "la"),
            Mark::new(1400, 2600, "zoom"),
        ],
    );
    let beat = beats(500, 8);
    let kinds: Vec<EffectParams> = vec![
        EffectParams::Impact(ImpactParams {
            bloom: 300.0,
            color_shift: true,
            ..ImpactParams::default()
        }),
        EffectParams::Wipe(WipeParams {
            mode: WipeMode::OnOff,
            ..WipeParams::default()
        }),
        EffectParams::Lightning(LightningParams {
            density: 3.0,
            branches: 0.6,
            ..LightningParams::default()
        }),
        EffectParams::Pulse(PulseParams {
            timing_track: Some(beat.id),
            shape: PulseShape::Heartbeat,
            ..PulseParams::default()
        }),
        EffectParams::Sing(SingParams {
            timing_track: Some(words.id),
            min: 0.1,
            ..SingParams::default()
        }),
        EffectParams::ColorShift(ColorShiftParams {
            stagger: 60.0,
            ..ColorShiftParams::default()
        }),
        EffectParams::Chase(ChaseParams {
            order: ChaseOrder::PropsAcross,
            timing_track: Some(beat.id),
            ..ChaseParams::default()
        }),
    ];
    seq.timing_tracks = vec![words, beat];
    let targets: Vec<Target> = show
        .props
        .iter()
        .map(|p| Target::Prop(p.id))
        .chain([Target::Group(house)])
        .collect();
    for params in kinds {
        let kind = params.kind();
        for (n, &target) in targets.iter().enumerate() {
            for style in [RenderStyle::Default, RenderStyle::PerPreview] {
                let mut effect = Effect::new(kind, 0, 4000)
                    .with_palette(palette)
                    .with_params(params.clone());
                effect.render_style = style;
                let mut s = seq.clone();
                let mut row = Row::new(target);
                row.layers[0].effects = vec![effect];
                s.rows.push(row);
                assert_eq!(validate_sequence(&s, &show), vec![], "{kind:?}");
                let mut r = renderer(&show);
                let mut lit_any = false;
                let mut frames = Vec::new();
                for t in (0..4000).step_by(125) {
                    let mut frame = vec![0u8; r.frame_len()];
                    r.render(&s, t, &mut frame);
                    lit_any |= frame.iter().any(|&b| b > 0);
                    frames.push(frame);
                }
                assert!(lit_any, "{kind:?} on target {n} ({style:?}) lights something");
                assert!(
                    frames.windows(2).any(|w| w[0] != w[1]),
                    "{kind:?} on target {n} ({style:?}) changes over time"
                );
                // Every frame renders the same again on its own (seeking, export).
                let mut again = renderer(&show);
                let mut frame = vec![0u8; again.frame_len()];
                again.render(&s, 1875, &mut frame);
                assert_eq!(frame, frames[15], "{kind:?} on target {n} ({style:?}) seeks");
            }
        }
    }
}
