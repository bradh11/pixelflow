//! Effects that follow the music: music curves and sparkles, Tendril's and Shape's music
//! settings, and the VU Meter, drawn from synthetic songs.

use pf_model::{Generator, Prop, ShapeSource, Show};
use pf_render::audio::{Audio, AudioSource, AudioTrack, RenderContext};
use pf_render::{Canvas, Colors, EffectTime, Pixel, Rgba, Shade, Shader};
use pf_sequence::*;
use std::f32::consts::TAU;
use std::sync::Arc;

const RATE: u32 = 44_100;
const FRAME_MS: u32 = 25;

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

/// One loud second, then one at a quarter.
fn loud_then_quiet() -> Arc<AudioTrack> {
    song(&[(1.0, 1.0), (1.0, 0.25)])
}

fn strip(nodes: u32) -> Show {
    let mut show = Show::new("t");
    show.props.push(Prop::new(
        "Strip",
        ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
    ));
    show
}

/// A 20 × 20 matrix.
fn matrix() -> Show {
    let mut show = Show::new("t");
    show.props.push(Prop::new(
        "Matrix",
        ShapeSource::Generator(Generator::Matrix {
            columns: 20,
            rows: 20,
            width: 1.0,
            height: 1.0,
            wiring: Default::default(),
        }),
    ));
    show
}

fn sequence(show: &Show, effects: Vec<Effect>) -> Sequence {
    let mut seq = Sequence::new("s", 2000);
    seq.frame_ms = FRAME_MS;
    let mut row = Row::new(Target::Prop(show.props[0].id));
    row.layers[0].effects = effects;
    seq.rows.push(row);
    seq
}

fn renderer(show: &Show, audio: AudioSource) -> pf_render::Renderer {
    let mut r = pf_render::Renderer::new(show, &pf_mapping::map_show(show).0);
    r.set_audio(audio);
    r
}

fn render(r: &mut pf_render::Renderer, seq: &Sequence, t_ms: u64) -> Vec<u8> {
    let mut frame = vec![0u8; r.frame_len()];
    r.render(seq, t_ms, &mut frame);
    frame
}

#[test]
fn music_curves_follow_the_music_and_sit_halfway_until_it_is_there() {
    let show = strip(4);
    let mut on = Effect::new(EffectKind::On, 0, 2000).with_palette([Rgb::RED]);
    on.curves
        .insert("startLevel".into(), Curve::music(0.0, 1.0, 0.0, false));
    on.curves
        .insert("endLevel".into(), Curve::music(0.0, 1.0, 0.0, false));
    let seq = sequence(&show, vec![on]);
    let track = loud_then_quiet();
    let mut r = renderer(&show, AudioSource::ready(track.clone()));
    assert_eq!(render(&mut r, &seq, 500)[0], 255, "the loud second");
    let quiet = render(&mut r, &seq, 1500)[0];
    assert!((62..=66).contains(&quiet), "a quarter as loud: {quiet}");

    // While the music is on its way the curve sits halfway, then follows it.
    let (pending, fill) = AudioSource::pending();
    let mut r = renderer(&show, pending);
    assert_eq!(render(&mut r, &seq, 500)[0], 128);
    fill.fill(track);
    assert_eq!(render(&mut r, &seq, 500)[0], 255);
    let mut silent = renderer(&show, AudioSource::none());
    assert_eq!(render(&mut silent, &seq, 1500)[0], 128);
}

#[test]
fn music_sparkles_thin_out_as_the_music_quietens() {
    let show = strip(4000);
    let mut on = Effect::new(EffectKind::On, 0, 2000).with_palette([Rgb::RED]);
    on.sparkles = 200;
    on.music_sparkles = true;
    let seq = sequence(&show, vec![on]);
    let mut r = renderer(&show, AudioSource::ready(loud_then_quiet()));
    let sparkling = |frame: &[u8]| frame.chunks(3).filter(|p| p[1] > 0).count() as f64 / 4000.0;
    // xLights: Sparkles × the peak (rounded down: a peak just under 1 makes 199), each lit pixel
    // flashing 5 frames in every 208 − that.
    let loud = sparkling(&render(&mut r, &seq, 500));
    let quiet = sparkling(&render(&mut r, &seq, 1500));
    assert!((loud - 5.0 / 9.0).abs() < 0.06, "{loud}");
    assert!((quiet - 5.0 / 159.0).abs() < 0.015, "{quiet}");
    let mut steady = renderer(&show, AudioSource::none());
    assert!((sparkling(&render(&mut steady, &seq, 1500)) - 5.0 / 8.0).abs() < 0.06);
}

const GRID: Canvas = Canvas {
    columns: 12,
    rows: 20,
};

fn cell(x: u32, y: u32) -> Pixel {
    Pixel {
        u: x as f32 / (GRID.columns - 1) as f32,
        v: y as f32 / (GRID.rows - 1) as f32,
        index: y * GRID.columns + x,
        count: GRID.columns * GRID.rows,
    }
}

/// The VU Meter at `t_ms` into an effect starting at 0, on a 12 × 20 grid: each column's lit
/// cells, bottom first.
fn meter(p: &VuMeterParams, track: Option<&AudioTrack>, tracks: &[TimingTrack], t_ms: u64) -> Vec<Vec<Rgba>> {
    let cx = RenderContext::new(track.map(|t| Audio::new(t, FRAME_MS)), tracks, FRAME_MS);
    let time = EffectTime::within(0, 2000, t_ms).with_frame_ms(FRAME_MS);
    let shader = Shader::in_context(
        &EffectParams::VuMeter(p.clone()),
        &time,
        Colors::new(&[Rgb::RED, Rgb::GREEN, Rgb::BLUE]),
        7,
        GRID,
        &cx,
    );
    (0..GRID.columns)
        .map(|x| (0..GRID.rows).map(|y| shader.shade(&cell(x, y))).collect())
        .collect()
}

fn height(column: &[Rgba]) -> usize {
    column.iter().filter(|c| c.a > 0.0).count()
}

#[test]
fn level_bars_rise_with_the_level() {
    let track = loud_then_quiet();
    // Level Jump with every frame triggering: the bars stand at the music's peak.
    let p = VuMeterParams {
        meter: VuMeterType::LevelJump,
        sensitivity: 0,
        ..VuMeterParams::default()
    };
    let loud = meter(&p, Some(&track), &[], 500);
    let quiet = meter(&p, Some(&track), &[], 1500);
    assert!(loud.iter().all(|c| height(c) == 20), "{}", height(&loud[0]));
    assert!(
        quiet.iter().all(|c| height(c) == 5),
        "a quarter: {}",
        height(&quiet[0])
    );
    // Colored by the palette from the bottom up.
    assert_eq!(loud[0][0], Rgba::opaque([1.0, 0.0, 0.0]));
    // Volume bars: the last 12 frames' peaks, oldest at the left (heights rounded down, as
    // xLights: a peak just under 1 is 19 of 20).
    let p = VuMeterParams {
        meter: VuMeterType::VolumeBars,
        bars: 12,
        ..VuMeterParams::default()
    };
    let crossing = meter(&p, Some(&track), &[], 1000 + 6 * u64::from(FRAME_MS));
    let heights: Vec<usize> = crossing.iter().map(|c| height(c)).collect();
    assert_eq!(heights, [19, 19, 19, 19, 19, 19, 4, 4, 4, 4, 4, 4]);
    // Without the music, nothing (as in xLights).
    assert!(meter(&p, None, &[], 500).iter().all(|c| height(c) == 0));
}

#[test]
fn a_spectrogram_lights_the_bar_of_a_tone() {
    // 220 Hz is MIDI note 57: with notes 36 to 84 in 7 bars (7 notes each), bar 3.
    let samples: Vec<f32> = (0..RATE as usize)
        .map(|i| 0.5 * (TAU * 220.0 * i as f32 / RATE as f32).sin())
        .collect();
    let track = pf_analysis::audio_track(samples, RATE, FRAME_MS);
    let p = VuMeterParams {
        meter: VuMeterType::Spectrogram,
        bars: 7,
        slow_falls: false,
        ..VuMeterParams::default()
    };
    let columns = meter(&p, Some(&track), &[], 500);
    // 12 columns over 7 bars: bar j covers columns up to (j + 1) × 12/7.
    let bar_heights: Vec<usize> = [0, 2, 4, 6, 7, 9, 11]
        .iter()
        .map(|&x| height(&columns[x]))
        .collect();
    let tallest = (0..7).max_by_key(|&j| bar_heights[j]).unwrap();
    assert_eq!(tallest, 3, "{bar_heights:?}");
    assert!(
        bar_heights[3] >= 18 && bar_heights[0] < bar_heights[3],
        "{bar_heights:?}"
    );
    // With peaks, the last color marks each bar's peak.
    let peaks = meter(
        &VuMeterParams {
            meter: VuMeterType::SpectrogramPeak,
            ..p.clone()
        },
        Some(&track),
        &[],
        500,
    );
    assert!(peaks[0].iter().any(|c| *c == Rgba::opaque([0.0, 0.0, 1.0])));
}

#[test]
fn timing_events_draw_on_their_marks() {
    let track = TimingTrack::new(
        "Beats",
        TimingKind::Beats,
        vec![Mark::new(100, 200, "kick"), Mark::new(500, 600, "snare")],
    );
    let tracks = [track];
    let p = VuMeterParams {
        meter: VuMeterType::TimingEventSpike,
        bars: 4,
        timing_track: Some(tracks[0].id),
        ..VuMeterParams::default()
    };
    // At frame 6 the spike shows the marks of frames 2 to 5: the one at frame 4 (100 ms), in
    // its column (3 grid columns a bar).
    let columns = meter(&p, None, &tracks, 150);
    let lit: Vec<usize> = (0..12).filter(|&x| height(&columns[x]) > 0).collect();
    assert_eq!(lit, vec![6, 7, 8]);
    // A color on each mark, the next palette color each time (xLights starts on the first color
    // before any mark, so the first mark shows the second), faint between marks.
    let colors = VuMeterParams {
        meter: VuMeterType::TimingEventColor,
        sensitivity: 0,
        ..p.clone()
    };
    assert_eq!(
        meter(&colors, None, &tracks, 50)[0][0].a,
        0.0,
        "before any mark, transparent"
    );
    assert_eq!(
        meter(&colors, None, &tracks, 150)[0][0],
        Rgba::opaque([0.0, 1.0, 0.0])
    );
    assert_eq!(meter(&colors, None, &tracks, 300)[0][0].a, 0.0);
    assert_eq!(
        meter(&colors, None, &tracks, 550)[0][0],
        Rgba::opaque([0.0, 0.0, 1.0])
    );
    // The filter keeps the snare only.
    let snare = VuMeterParams {
        filter: "snare".into(),
        ..colors.clone()
    };
    assert_eq!(
        meter(&snare, None, &tracks, 550)[0][0],
        Rgba::opaque([0.0, 1.0, 0.0])
    );
    // Pulses jump up on each mark and fade over `bars` frames.
    let pulse = VuMeterParams {
        meter: VuMeterType::TimingEventPulse,
        ..p.clone()
    };
    let level = |t| meter(&pulse, None, &tracks, t)[0][0].a;
    assert_eq!(level(100), 1.0);
    assert!(
        (level(125) - 0.75).abs() < 0.01 && level(200) == 0.0,
        "{} {}",
        level(125),
        level(200)
    );
}

#[test]
fn level_shapes_grow_with_the_level() {
    let track = loud_then_quiet();
    let p = VuMeterParams {
        meter: VuMeterType::LevelShape,
        shape: VuMeterShape::FilledSquare,
        sensitivity: 14,
        slow_falls: false,
        ..VuMeterParams::default()
    };
    let lit = |t| {
        meter(&p, Some(&track), &[], t)
            .iter()
            .map(|c| height(c))
            .sum::<usize>()
    };
    let (loud, quiet) = (lit(500), lit(1500));
    assert!(loud > 3 * quiet && quiet > 0, "{loud} {quiet}");
}

#[test]
fn tendrils_and_shapes_follow_the_music() {
    let show = strip(30);
    let track = loud_then_quiet();
    // Shapes fire when the music passes the trigger level: in the loud second only.
    let shapes = Effect::new(EffectKind::Shape, 0, 2000).with_params(EffectParams::Shape(ShapeParams {
        fire_on_music: true,
        trigger_level: 50.0,
        lifetime: 5.0,
        random_location: false,
        start_size: 3.0,
        growth: 0.0,
        ..ShapeParams::default()
    }));
    let seq = sequence(&show, vec![shapes]);
    let mut r = renderer(&show, AudioSource::ready(track.clone()));
    let lit = |frame: &[u8]| frame.chunks(3).filter(|p| p.iter().any(|&v| v > 0)).count();
    // They fire at frame 0 and every 21 frames while it stays loud: 525 ms is the second.
    assert!(lit(&render(&mut r, &seq, 540)) > 0);
    assert_eq!(lit(&render(&mut r, &seq, 1500)), 0, "too quiet to fire");
    let mut silent = renderer(&show, AudioSource::none());
    assert_eq!(
        lit(&render(&mut silent, &seq, 540)),
        0,
        "nothing fires without the music"
    );

    // A tendril on a music line rides as high as the music is loud: higher than without it.
    let show = matrix();
    let tendril = |movement| {
        Effect::new(EffectKind::Tendril, 0, 2000).with_params(EffectParams::Tendril(TendrilParams {
            movement,
            ..TendrilParams::default()
        }))
    };
    let a = sequence(&show, vec![tendril(TendrilMovement::MusicLine)]);
    let mut loud = renderer(&show, AudioSource::ready(track.clone()));
    let mut none = renderer(&show, AudioSource::none());
    // The highest lit row (the matrix is wired in rows from the bottom, 20 pixels each).
    let top = |frame: Vec<u8>| {
        frame
            .chunks(3)
            .enumerate()
            .filter(|(_, p)| p.iter().any(|&v| v > 0))
            .map(|(i, _)| i / 20)
            .max()
    };
    let (with, without) = (top(render(&mut loud, &a, 900)), top(render(&mut none, &a, 900)));
    assert!(with > without, "{with:?} {without:?}");
}

#[test]
fn frames_with_music_are_the_same_however_they_are_reached() {
    let show = strip(60);
    let track = song(&[(0.5, 0.9), (0.5, 0.2), (0.5, 0.7), (0.5, 0.4)]);
    let beats = TimingTrack::new(
        "Beats",
        TimingKind::Beats,
        (0..8).map(|i| Mark::new(i * 250, i * 250 + 100, "")).collect(),
    );
    let mut vu = Effect::new(EffectKind::VuMeter, 0, 2000).with_palette([Rgb::RED, Rgb::BLUE]);
    vu.params = EffectParams::VuMeter(VuMeterParams {
        meter: VuMeterType::SpectrogramPeak,
        ..VuMeterParams::default()
    });
    let mut pulse = Effect::new(EffectKind::VuMeter, 0, 2000).with_palette([Rgb::GREEN]);
    pulse.params = EffectParams::VuMeter(VuMeterParams {
        meter: VuMeterType::TimingEventPulse,
        timing_track: Some(beats.id),
        ..VuMeterParams::default()
    });
    pulse.blend = Blend::Add;
    let mut twinkle = Effect::new(EffectKind::Twinkle, 0, 2000);
    twinkle.sparkles = 150;
    twinkle.music_sparkles = true;
    twinkle.blend = Blend::Max;
    let trigger = Curve {
        trigger: 60.0,
        fade: 6.0,
        ..Curve::shaped(CurveShape::MusicTrigger, 0.0, 1.0, 1.0)
    };
    twinkle.curves.insert("density".into(), trigger);
    let mut tendril =
        Effect::new(EffectKind::Tendril, 0, 2000).with_params(EffectParams::Tendril(TendrilParams {
            movement: TendrilMovement::MusicCircle,
            ..TendrilParams::default()
        }));
    tendril.blend = Blend::Add;
    let mut seq = sequence(&show, vec![vu]);
    seq.timing_tracks.push(beats);
    seq.rows[0].layers.push(Layer { effects: vec![pulse] });
    seq.rows[0].layers.push(Layer {
        effects: vec![twinkle],
    });
    seq.rows[0].layers.push(Layer {
        effects: vec![tendril],
    });
    let audio = AudioSource::ready(track);
    // Played through.
    let mut playing = renderer(&show, audio.clone());
    let played: Vec<Vec<u8>> = (0..80).map(|i| render(&mut playing, &seq, i * 25)).collect();
    assert!(played.iter().any(|f| f.iter().any(|&v| v > 0)));
    // Scrubbed, out of order, on another renderer; and the same frame twice.
    let mut scrubbing = renderer(&show, audio.clone());
    for i in [57u64, 3, 79, 40, 41, 12, 12, 66, 0] {
        assert_eq!(
            render(&mut scrubbing, &seq, i * 25),
            played[i as usize],
            "frame {i}"
        );
    }
    // Exported: every frame as the preview drew it.
    let mut exporting = renderer(&show, audio);
    for (i, frame) in played.iter().enumerate() {
        let mut out = vec![0u8; exporting.frame_len()];
        exporting.render_frame(&seq, i as u64, &mut out);
        assert_eq!(&out, frame, "frame {i}");
    }
}
