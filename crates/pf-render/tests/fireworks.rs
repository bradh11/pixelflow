//! Fireworks, held to what it promises: when bursts go off and how dark it is between them, what
//! each setting changes, where the colors come from, and that a frame depends on nothing but its
//! settings, palette, seed, target, and time.
//!
//! The rig, unless a test says otherwise: a buffer 101 columns by 101 rows, the palette red,
//! green, blue, an effect a minute long with 25 ms frames, and the burst of [`plain`]. Distances
//! are in cells from where the burst went off. A pixel is *lit* when its alpha is above 0.

use pf_model::{Generator, Prop, ShapeSource, Show, Transform, Vec3};
use pf_render::audio::{Audio, AudioSource, AudioTrack, RenderContext, effect_follows_music};
use pf_render::{Canvas, Colors, EffectTime, Fireworks, Pixel, RenderPool, Renderer, Rgba, Shade, Shader};
use pf_sequence::{
    Effect, EffectKind, EffectParams, FireworksColors, FireworksParams, Mark, Rgb, Row, Sequence, Target,
    TimingKind, TimingTrack, TimingTrackId,
};
use std::collections::BTreeSet;
use std::f32::consts::TAU;
use std::sync::Arc;

const RIG: Canvas = Canvas {
    columns: 101,
    rows: 101,
};
const PALETTE: [Rgb; 3] = [Rgb::RED, Rgb::GREEN, Rgb::BLUE];
const SEED: u64 = 0xF1BE;
const FRAME_MS: u32 = 25;

const RED: [f32; 3] = [1.0, 0.0, 0.0];
const GREEN: [f32; 3] = [0.0, 1.0, 0.0];
const BLUE: [f32; 3] = [0.0, 0.0, 1.0];

/// A burst a test can measure. It goes off in the middle of the target as the effect starts.
/// Sizes are cells: its 24 sparks are one cell wide, soft, and fly 40 cells in the second they
/// glow, at one speed, without tails and without falling. The next burst is two to four seconds
/// off.
fn plain() -> FireworksParams {
    FireworksParams {
        rate: 0.5,
        sparks: 24,
        size: 40.0,
        life: 1.0,
        trail: 0.0,
        fall: 0.0,
        random_location: false,
        spark_size: 1.0,
        slowing: 0.0,
        scale: false,
        ..FireworksParams::default()
    }
}

/// `elapsed_ms` into a minute-long effect.
fn at_ms(elapsed_ms: u64) -> EffectTime {
    EffectTime::within(0, 60_000, elapsed_ms).with_frame_ms(FRAME_MS)
}

/// The pixel in column `x` and row `y` (from the bottom) of `canvas`.
fn pixel(x: u32, y: u32, canvas: Canvas) -> Pixel {
    let at = |i: u32, n: u32| if n <= 1 { 0.5 } else { i as f32 / (n - 1) as f32 };
    Pixel {
        u: at(x, canvas.columns),
        v: at(y, canvas.rows),
        index: y * canvas.columns + x,
        count: canvas.columns * canvas.rows,
    }
}

/// One frame: every pixel's color, with its column and row.
struct Frame {
    canvas: Canvas,
    pixels: Vec<Rgba>,
}

impl Frame {
    fn of(shader: &impl Shade, canvas: Canvas) -> Self {
        let pixels = (0..canvas.rows)
            .flat_map(|y| (0..canvas.columns).map(move |x| (x, y)))
            .map(|(x, y)| shader.shade(&pixel(x, y, canvas)))
            .collect();
        Self { canvas, pixels }
    }

    fn at(&self, x: i32, y: i32) -> Rgba {
        let (columns, rows) = (self.canvas.columns as i32, self.canvas.rows as i32);
        if (0..columns).contains(&x) && (0..rows).contains(&y) {
            self.pixels[(y * columns + x) as usize]
        } else {
            Rgba::CLEAR
        }
    }

    /// The lit pixels: column, row, color.
    fn lit(&self) -> Vec<(i32, i32, Rgba)> {
        let columns = self.canvas.columns as usize;
        self.pixels
            .iter()
            .enumerate()
            .filter(|(_, c)| c.a > 0.0)
            .map(|(i, c)| ((i % columns) as i32, (i / columns) as i32, *c))
            .collect()
    }

    fn dark(&self) -> bool {
        self.pixels.iter().all(|c| *c == Rgba::CLEAR)
    }

    fn brightest(&self) -> f32 {
        self.pixels.iter().map(|c| c.a).fold(0.0, f32::max)
    }

    /// How far the lit pixels are from (`x`, `y`): the nearest and the farthest.
    fn span_from(&self, x: f32, y: f32) -> (f32, f32) {
        self.lit()
            .iter()
            .map(|&(px, py, _)| (px as f32 - x).hypot(py as f32 - y))
            .fold((f32::INFINITY, 0.0), |(near, far), d| (near.min(d), far.max(d)))
    }

    /// The lit pixels in groups that touch (sideways, up and down, or cornerwise).
    fn blobs(&self) -> Vec<Vec<(i32, i32)>> {
        let mut left: BTreeSet<(i32, i32)> = self.lit().iter().map(|&(x, y, _)| (x, y)).collect();
        let mut blobs = Vec::new();
        while let Some(&start) = left.iter().next() {
            left.remove(&start);
            let (mut blob, mut edge) = (vec![start], vec![start]);
            while let Some((x, y)) = edge.pop() {
                for (dx, dy) in (-1..=1).flat_map(|dx| (-1..=1).map(move |dy| (dx, dy))) {
                    if left.remove(&(x + dx, y + dy)) {
                        blob.push((x + dx, y + dy));
                        edge.push((x + dx, y + dy));
                    }
                }
            }
            blobs.push(blob);
        }
        blobs
    }

    fn bits(&self) -> Vec<[u32; 4]> {
        self.pixels
            .iter()
            .map(|c| [c.r, c.g, c.b, c.a].map(f32::to_bits))
            .collect()
    }
}

fn rgb(c: Rgba) -> [f32; 3] {
    [c.r, c.g, c.b]
}

/// `p` at `ms` on `canvas`, with the rig's palette and seed and no music or timing tracks.
fn draw_on(p: &FireworksParams, ms: u64, canvas: Canvas) -> Frame {
    draw_with(p, ms, canvas, &PALETTE, SEED)
}

fn draw(p: &FireworksParams, ms: u64) -> Frame {
    draw_on(p, ms, RIG)
}

fn draw_with(p: &FireworksParams, ms: u64, canvas: Canvas, palette: &[Rgb], seed: u64) -> Frame {
    let fireworks = Fireworks::new(
        p,
        &at_ms(ms),
        Colors::new(palette),
        seed,
        canvas,
        &RenderContext::default(),
    );
    Frame::of(&fireworks, canvas)
}

// ---------------------------------------------------------------------------------------------
// When bursts go off

#[test]
fn an_effect_starts_with_a_burst_and_is_dark_once_its_sparks_have_faded() {
    let p = plain();
    // As it goes off every spark is at the center: one pixel, at full brightness.
    let lit = draw(&p, 0).lit();
    assert_eq!(lit.len(), 1, "{lit:?}");
    assert_eq!((lit[0].0, lit[0].1, lit[0].2.a), (50, 50, 1.0));
    // Then they have flown: the center is dark, and the light is round about it.
    let flown = draw(&p, 500);
    assert_eq!(flown.at(50, 50), Rgba::CLEAR);
    assert!(flown.lit().len() >= 20, "{}", flown.lit().len());
    // In its last frame it is all but out, and after a second it is out.
    let last = draw(&p, 975);
    assert!(!last.dark() && last.brightest() <= 0.03, "{}", last.brightest());
    for ms in (1000..2000).step_by(25) {
        assert!(draw(&p, ms).dark(), "at {ms}");
    }
}

#[test]
fn bursts_come_at_the_rate_set_one_in_each_stretch_of_time_and_it_is_dark_between_them() {
    // Two a second, each alight for a fifth of a second, on a buffer the sparks soon leave.
    let small = Canvas {
        columns: 21,
        rows: 21,
    };
    let p = FireworksParams {
        rate: 2.0,
        life: 0.2,
        ..plain()
    };
    // A burst has just gone off when the center, dark a moment ago, is lit: its sparks are
    // there for its first few milliseconds only.
    let center = |p: &FireworksParams, ms: u64| {
        let fireworks = Fireworks::new(
            p,
            &at_ms(ms),
            Colors::new(&PALETTE),
            SEED,
            small,
            &RenderContext::default(),
        );
        fireworks.shade(&pixel(10, 10, small)).a
    };
    let went_off = |p: &FireworksParams| -> Vec<u64> {
        let mut starts = Vec::new();
        let mut before = 0.0;
        for ms in (0..10_000).step_by(2) {
            let now = center(p, ms);
            if before == 0.0 && now > 0.0 {
                starts.push(ms);
            }
            before = now;
        }
        starts
    };
    let starts = went_off(&p);
    // One in every half second: the first as the effect starts, the rest whenever.
    assert_eq!(starts.len(), 20, "{starts:?}");
    assert_eq!(starts[0], 0);
    for (stretch, &start) in starts.iter().enumerate() {
        assert_eq!(start / 500, stretch as u64, "{starts:?}");
    }
    let within: Vec<u64> = starts[1..].iter().map(|s| s % 500).collect();
    assert!(
        *within.iter().min().unwrap() < 150 && *within.iter().max().unwrap() > 350,
        "bursts go off at moments of their own: {within:?}"
    );
    // Dark from the moment a burst has faded to the moment the next goes off.
    let mut dark = 0;
    for ms in (0..10_000).step_by(5) {
        let since = starts.iter().rev().find(|&&s| s <= ms).map(|s| ms - s);
        let to_next = starts.iter().find(|&&s| s > ms).map(|s| s - ms);
        if since.is_some_and(|since| since >= 205) && to_next.is_none_or(|next| next > 5) {
            assert!(draw_on(&p, ms, small).dark(), "at {ms}");
            dark += 1;
        }
    }
    assert!(dark > 500, "most of the time is dark: {dark} of 2000 moments");
    // Half as often: half as many.
    let slower = FireworksParams { rate: 1.0, ..p };
    assert_eq!(went_off(&slower).len(), 10);
}

/// A timing track with a mark starting at each of `at`.
fn track(at: &[u64]) -> TimingTrack {
    TimingTrack::new(
        "Hits",
        TimingKind::Custom,
        at.iter()
            .map(|&ms| Mark::new(ms, ms.saturating_add(10), ""))
            .collect(),
    )
}

#[test]
fn on_a_timing_track_a_burst_goes_off_at_each_mark() {
    // The effect runs from half a second to ten; marks before it and after it are not its own.
    let tracks = [track(&[200, 1000, 2500, 2600, 12_000])];
    let p = FireworksParams {
        timing_track: Some(tracks[0].id),
        life: 0.4,
        ..plain()
    };
    let cx = RenderContext::new(None, &tracks, FRAME_MS);
    let draw = |p: &FireworksParams, t_ms: u64| {
        let time = EffectTime::within(500, 10_000, t_ms).with_frame_ms(FRAME_MS);
        Frame::of(
            &Fireworks::new(p, &time, Colors::new(&PALETTE), SEED, RIG, &cx),
            RIG,
        )
    };
    // Nothing as the effect starts, nor from the mark before it; then one at 1000.
    for t in (500..1000).step_by(25) {
        assert!(draw(&p, t).dark(), "at {t}");
    }
    assert_eq!(draw(&p, 1000).at(50, 50).a, 1.0);
    assert!(!draw(&p, 1375).dark());
    for t in (1400..2500).step_by(25) {
        assert!(draw(&p, t).dark(), "at {t}");
    }
    // Two marks close together: the second goes off while the first is still alight.
    assert_eq!(draw(&p, 2500).at(50, 50).a, 1.0);
    let both = draw(&p, 2600);
    assert_eq!(both.at(50, 50).a, 1.0);
    assert!(both.lit().len() > 1, "the first is still flying");
    for t in (3000..10_000).step_by(250) {
        assert!(draw(&p, t).dark(), "at {t}");
    }
    // The rate plays no part.
    let faster = FireworksParams { rate: 20.0, ..p };
    for t in [600, 1000, 1200, 2000, 2550, 2700, 5000] {
        assert_eq!(draw(&faster, t).bits(), draw(&p, t).bits(), "at {t}");
    }
    // A track that isn't there sets nothing off.
    let gone = FireworksParams {
        timing_track: Some(TimingTrackId::new()),
        ..p
    };
    for t in (500..4000).step_by(100) {
        assert!(draw(&gone, t).dark(), "at {t}");
    }
}

/// A 440 Hz tone at `amplitude` for each of `parts` (seconds, amplitude).
fn song(parts: &[(f32, f32)]) -> Arc<AudioTrack> {
    const RATE: u32 = 44_100;
    let mut samples = Vec::new();
    for &(seconds, amplitude) in parts {
        let n = (seconds * RATE as f32) as usize;
        let start = samples.len();
        samples.extend((0..n).map(|i| amplitude * (TAU * 440.0 * (start + i) as f32 / RATE as f32).sin()));
    }
    Arc::new(pf_analysis::audio_track(samples, RATE, FRAME_MS))
}

#[test]
fn with_the_music_a_burst_goes_off_when_it_gets_loud_and_again_while_it_stays_loud() {
    // Quiet for a second, loud for 1.2, quiet again.
    let music = song(&[(1.0, 0.05), (1.2, 1.0), (1.0, 0.05)]);
    let p = FireworksParams {
        fire_on_music: true,
        trigger_level: 50.0,
        life: 0.3,
        ..plain()
    };
    let draw = |p: &FireworksParams, music: Option<&AudioTrack>, ms: u64| {
        let cx = RenderContext::new(music.map(|m| Audio::new(m, FRAME_MS)), &[], FRAME_MS);
        Frame::of(
            &Fireworks::new(p, &at_ms(ms), Colors::new(&PALETTE), SEED, RIG, &cx),
            RIG,
        )
    };
    // One as it gets loud, and one every half second it has stayed loud: at 1000, 1500, 2000.
    for ms in (0..3200).step_by(25) {
        let alight = [1000, 1500, 2000]
            .iter()
            .any(|&burst| (burst..burst + 300).contains(&ms));
        assert_eq!(!draw(&p, Some(&music), ms).dark(), alight, "at {ms}");
    }
    for burst in [1000, 1500, 2000] {
        assert_eq!(draw(&p, Some(&music), burst).at(50, 50).a, 1.0, "at {burst}");
    }
    // A trigger level the music never reaches sets nothing off; nor does no music at all.
    let never = FireworksParams {
        trigger_level: 100.0,
        ..p
    };
    for ms in (0..3200).step_by(100) {
        assert!(draw(&never, Some(&music), ms).dark(), "at {ms}");
        assert!(draw(&p, None, ms).dark(), "at {ms}");
    }
    // An effect that starts while the music is loud goes off at once.
    let cx = RenderContext::new(Some(Audio::new(&music, FRAME_MS)), &[], FRAME_MS);
    let late = EffectTime::within(1200, 5000, 1200).with_frame_ms(FRAME_MS);
    let burst = Fireworks::new(&p, &late, Colors::new(&PALETTE), SEED, RIG, &cx);
    assert_eq!(Frame::of(&burst, RIG).at(50, 50).a, 1.0);
    // The renderer knows to wait for the music for one, and only then.
    let effect = |p: FireworksParams| {
        Effect::new(EffectKind::Fireworks, 0, 1000).with_params(EffectParams::Fireworks(p))
    };
    assert!(effect_follows_music(&effect(p)));
    assert!(!effect_follows_music(&effect(plain())));
    assert!(!effect_follows_music(&effect(FireworksParams {
        timing_track: Some(TimingTrackId::new()),
        ..p
    })));
}

// ---------------------------------------------------------------------------------------------
// What a burst is

#[test]
fn a_burst_has_as_many_sparks_as_asked_for() {
    // Three quarters of the way through, the sparks are 4 to 30 cells out. Each is a blob of
    // its own unless two touch, which the more there are the more often they do.
    for (sparks, whole_bursts, least) in [(1, 12, 1), (3, 10, 2), (8, 7, 6), (24, 0, 18), (60, 0, 36)] {
        let mut whole = 0;
        for seed in 0..12 {
            let p = FireworksParams { sparks, ..plain() };
            let blobs = draw_with(&p, 750, RIG, &PALETTE, seed).blobs();
            assert!(
                (least..=sparks as usize).contains(&blobs.len()),
                "{sparks} sparks, seed {seed}: {} blobs",
                blobs.len()
            );
            if blobs.len() == sparks as usize {
                whole += 1;
                // A spark a cell wide lights the cell it is in, or shares its light with the
                // cells next to it.
                assert!(
                    blobs.iter().all(|blob| blob.len() <= 4),
                    "{sparks} sparks, seed {seed}"
                );
            }
        }
        assert!(
            whole >= whole_bursts,
            "{sparks} sparks: {whole} of 12 bursts whole"
        );
    }
}

#[test]
fn sparks_fly_out_as_far_as_the_burst_size_says_and_none_stays_at_the_center() {
    for (size, ms) in [(40.0, 250), (40.0, 500), (40.0, 750), (20.0, 500), (10.0, 900)] {
        let p = FireworksParams { size, ..plain() };
        let reach = size * ms as f32 / 1000.0;
        let (near, far) = draw(&p, ms).span_from(50.0, 50.0);
        // A spark a cell wide shows a cell from its middle at the most, and the slowest
        // flies 15% as far as the fastest.
        assert!(
            far <= reach + 1.0 && far >= 0.95 * reach - 1.0,
            "size {size} at {ms}: {far}"
        );
        assert!(near >= 0.15 * reach - 1.0, "size {size} at {ms}: nearest {near}");
    }
    // All round the center, not to one side: light in every quarter.
    let lit = draw(&plain(), 500).lit();
    for (right, up) in [(true, true), (true, false), (false, true), (false, false)] {
        assert!(
            lit.iter().any(|&(x, y, _)| (x > 50) == right && (y > 50) == up),
            "right {right}, up {up}"
        );
    }
}

#[test]
fn slowing_sparks_start_faster_and_arrive_at_the_same_place() {
    let slowed = FireworksParams {
        slowing: 1.0,
        ..plain()
    };
    // Half way through its glow a slowed burst has flown three quarters of the way, an even
    // one half; at the end both are 40 cells out.
    let far = |p: &FireworksParams, ms: u64| draw(p, ms).span_from(50.0, 50.0).1;
    assert!(
        (far(&plain(), 500) - 20.0).abs() <= 1.75,
        "{}",
        far(&plain(), 500)
    );
    assert!((far(&slowed, 500) - 30.0).abs() <= 2.25, "{}", far(&slowed, 500));
    assert!((far(&plain(), 990) - 39.6).abs() <= 2.75);
    assert!((far(&slowed, 990) - 40.0).abs() <= 2.75);
    // It all but stops at the end: it moves less in its last tenth of a second than its first.
    assert!(far(&slowed, 990) - far(&slowed, 890) < 2.5);
    assert!(far(&slowed, 100) > 6.0);
}

#[test]
fn sparks_fade_evenly_to_nothing_over_the_glow_time() {
    // Sparks five cells wide cover the pixels at their middles whole.
    let p = FireworksParams {
        spark_size: 5.0,
        ..plain()
    };
    for ms in [0u64, 100, 250, 500, 750, 900, 990] {
        let level = 1.0 - ms as f32 / 1000.0;
        assert!((draw(&p, ms).brightest() - level).abs() < 1e-6, "at {ms}");
    }
    // Twice the glow time: half as far gone at the same moment.
    let longer = FireworksParams { life: 2.0, ..p };
    assert!((draw(&longer, 500).brightest() - 0.75).abs() < 1e-6);
    assert!(!draw(&longer, 1975).dark() && draw(&longer, 2000).dark());
}

#[test]
fn crisp_sparks_light_whole_pixels_and_soft_ones_share_their_light() {
    let crisp = FireworksParams {
        soft: false,
        sparks: 8,
        ..plain()
    };
    let soft = FireworksParams { soft: true, ..crisp };
    let mut soft_pixels = 0;
    for ms in (300..1000).step_by(25) {
        let level = 1.0 - ms as f32 / 1000.0;
        // A crisp spark one cell wide is one pixel, at the burst's brightness, wherever in its
        // cell the spark is: never more pixels than sparks, and as many unless two share one.
        let lit = draw(&crisp, ms).lit();
        assert!((7..=8).contains(&lit.len()), "at {ms}: {}", lit.len());
        assert!(lit.iter().all(|&(_, _, c)| c.a == level), "at {ms}");
        // A soft one's light is spread over the pixels round it, and adds up to about as much.
        let spread = draw(&soft, ms).lit();
        let light: f32 = spread.iter().map(|&(_, _, c)| c.a).sum();
        assert!(
            (0.95..=1.25).contains(&(light / (8.0 * level))),
            "at {ms}: {}",
            light / (8.0 * level)
        );
        soft_pixels += spread.len();
    }
    assert!(soft_pixels > 2 * 28 * 8, "{soft_pixels} pixels over 28 frames");
    // Wider crisp sparks are squares of whole pixels: three cells wide, nine pixels.
    let wide = FireworksParams {
        spark_size: 3.0,
        sparks: 1,
        ..crisp
    };
    for ms in [300, 555, 800] {
        let frame = draw(&wide, ms);
        let lit = frame.lit();
        assert_eq!(lit.len(), 9, "at {ms}");
        let (xs, ys): (BTreeSet<i32>, BTreeSet<i32>) = lit.iter().map(|&(x, y, _)| (x, y)).unzip();
        assert_eq!((xs.len(), ys.len()), (3, 3), "at {ms}");
    }
    // On a string too: twelve sparks each way, a pixel each unless two share one.
    let string = Canvas { columns: 61, rows: 1 };
    let along = draw_on(&FireworksParams { sparks: 24, ..crisp }, 500, string).lit();
    assert!((16..=24).contains(&along.len()), "{}", along.len());
    assert!(along.iter().all(|&(_, _, c)| c.a == 0.5));
}

#[test]
fn tails_run_from_each_spark_back_toward_the_center_and_fade_along_the_way() {
    let dots = draw(&plain(), 750);
    let tailed = FireworksParams {
        trail: 0.5,
        sparks: 8,
        ..plain()
    };
    let streaks = draw(&tailed, 750);
    let heads = draw(&FireworksParams { sparks: 8, ..plain() }, 750);
    assert!(streaks.lit().len() >= 4 * heads.lit().len());
    assert!(dots.lit().len() < streaks.lit().len());
    // Every spark is still there, as bright.
    for (x, y, c) in heads.lit() {
        assert!(streaks.at(x, y).a >= c.a - 1e-6, "({x}, {y})");
    }
    // Behind each: lit nine tenths and six tenths of the way out, dimmer further back, and
    // dark where a tail half its flight long has ended.
    for blob in heads.blobs() {
        let n = blob.len() as f32;
        let (x, y) = blob.iter().fold((0.0, 0.0), |(sx, sy), &(x, y)| {
            (sx + (x - 50) as f32 / n, sy + (y - 50) as f32 / n)
        });
        let back = |share: f32| {
            streaks
                .at(50 + (x * share).round() as i32, 50 + (y * share).round() as i32)
                .a
        };
        assert!(back(0.9) > back(0.6) && back(0.6) > 0.0, "spark at ({x}, {y})");
        assert_eq!(back(0.4), 0.0, "spark at ({x}, {y})");
    }
}

#[test]
fn falling_sparks_drop_together_faster_as_they_go() {
    // A burst 20 cells in size that falls its own size by the end: a quarter of that half way
    // through.
    let level = FireworksParams {
        size: 20.0,
        ..plain()
    };
    let falling = FireworksParams { fall: 1.0, ..level };
    for (ms, drop) in [(0, 0), (500, 5), (1000 - 25, 19)] {
        let (still, fell) = (draw(&level, ms), draw(&falling, ms));
        if ms == 500 {
            // The same picture, five rows down.
            for (x, y, c) in still.lit() {
                assert!((fell.at(x, y - 5).a - c.a).abs() < 1e-3, "({x}, {y})");
            }
            assert_eq!(fell.lit().len(), still.lit().len());
        }
        let height = |frame: &Frame| {
            let lit = frame.lit();
            lit.iter().map(|&(_, y, _)| y as f32).sum::<f32>() / lit.len() as f32
        };
        assert!(
            (height(&still) - height(&fell) - drop as f32).abs() < 0.6,
            "at {ms}: {} against {}",
            height(&still),
            height(&fell)
        );
    }
}

#[test]
fn sizes_in_percent_follow_the_prop_and_a_spark_is_never_smaller_than_a_pixel() {
    let p = FireworksParams {
        scale: true,
        size: 50.0,
        spark_size: 2.0,
        ..plain()
    };
    // A burst size of 50 flies a quarter of the longer side: 12.75, 25.25, and 50.25 cells.
    for cells in [51u32, 101, 201] {
        let canvas = Canvas {
            columns: cells,
            rows: cells / 2 + 1,
        };
        let frame = draw_on(&p, 990, canvas);
        let middle = ((cells - 1) / 2) as f32;
        let (_, far) = frame.span_from(middle, (canvas.rows - 1) as f32 / 2.0);
        let reach = 0.99 * cells as f32 / 4.0;
        let spark = (0.02 * cells as f32).max(1.0);
        assert!(
            far <= reach + spark / 2.0 + 0.5 && far >= 0.9 * reach - spark / 2.0 - 0.5,
            "{cells} cells: {far}"
        );
    }
    // A spark two percent of 201 cells wide is four cells: about sixteen pixels to a spark,
    // where on 21 cells it is the one pixel it can't be smaller than.
    let pixels_of_a_spark = |cells: u32| {
        let canvas = Canvas {
            columns: cells,
            rows: cells,
        };
        let single = FireworksParams { sparks: 1, ..p };
        let blobs = draw_on(&single, 700, canvas).blobs();
        let sizes = blobs.iter().map(Vec::len);
        (sizes.clone().min().unwrap_or(0), sizes.max().unwrap_or(0))
    };
    let (least, most) = pixels_of_a_spark(201);
    assert!(least >= 12 && most <= 26, "{least} to {most}");
    let (_, most) = pixels_of_a_spark(21);
    assert!((1..=4).contains(&most), "{most}");
    // In cells, the same burst whatever the buffer.
    let cells = FireworksParams {
        size: 10.0,
        ..plain()
    };
    for canvas in [
        RIG,
        Canvas {
            columns: 201,
            rows: 61,
        },
    ] {
        let middle = ((canvas.columns - 1) / 2) as f32;
        let (_, far) = draw_on(&cells, 990, canvas).span_from(middle, (canvas.rows - 1) as f32 / 2.0);
        assert!((far - 9.9).abs() <= 1.0, "{canvas:?}: {far}");
    }
}

#[test]
fn bursts_go_off_all_over_the_prop_or_where_they_are_told() {
    // As a burst goes off it is one pixel: where it is.
    let wild = FireworksParams {
        random_location: true,
        ..plain()
    };
    let places: Vec<(i32, i32)> = (0..80)
        .map(|seed| {
            let lit = draw_with(&wild, 0, RIG, &PALETTE, seed).lit();
            assert!((1..=4).contains(&lit.len()), "seed {seed}: {lit:?}");
            (lit[0].0, lit[0].1)
        })
        .collect();
    for (right, up) in [(true, true), (true, false), (false, true), (false, false)] {
        let here = places
            .iter()
            .filter(|&&(x, y)| (x > 50) == right && (y > 50) == up)
            .count();
        assert!(here >= 10, "right {right}, up {up}: {here} of 80");
    }
    assert!(places.iter().any(|&(x, _)| x < 10) && places.iter().any(|&(x, _)| x > 90));
    assert!(places.iter().any(|&(_, y)| y < 10) && places.iter().any(|&(_, y)| y > 90));
    // It stays where it went off: its sparks fly out from there.
    for seed in 0..5 {
        let (x, y) = places[seed as usize];
        let (near, far) = draw_with(&wild, 500, RIG, &PALETTE, seed).span_from(x as f32, y as f32);
        assert!(near >= 3.0 && far <= 22.5, "seed {seed}: {near} to {far}");
    }
    // Told where: a quarter of the way across and three quarters up, whatever the seed.
    let told = FireworksParams {
        center_x: 25.0,
        center_y: 75.0,
        ..plain()
    };
    for seed in 0..5 {
        let lit = draw_with(&told, 0, RIG, &PALETTE, seed).lit();
        assert_eq!((lit.len(), lit[0].0, lit[0].1), (1, 25, 75), "seed {seed}");
    }
}

// ---------------------------------------------------------------------------------------------
// Colors

#[test]
fn colors_come_from_the_palette() {
    let palette = [RED, GREEN, BLUE];
    // One color a burst: every spark the same, and each burst its own pick.
    let mut seen = BTreeSet::new();
    for seed in 0..40 {
        let lit = draw_with(&plain(), 400, RIG, &PALETTE, seed).lit();
        let color = rgb(lit[0].2);
        assert!(palette.contains(&color), "seed {seed}: {color:?}");
        assert!(lit.iter().all(|&(_, _, c)| rgb(c) == color), "seed {seed}");
        seen.insert(palette.iter().position(|c| *c == color));
    }
    assert_eq!(seen.len(), 3, "every palette color gets its bursts");
    // A color for each spark: every palette color in one burst, and a spark one color all
    // over and all its life.
    let each = FireworksParams {
        colors: FireworksColors::Spark,
        ..plain()
    };
    let used: BTreeSet<_> = draw(&each, 600)
        .lit()
        .iter()
        .map(|&(_, _, c)| palette.iter().position(|p| *p == rgb(c)))
        .collect();
    assert!(!used.contains(&None) && used.len() == 3, "{used:?}");
    let single = FireworksParams { sparks: 1, ..each };
    let mut seen = BTreeSet::new();
    for seed in 0..40 {
        let colors: BTreeSet<_> = [0, 300, 600, 900]
            .iter()
            .flat_map(|&ms| draw_with(&single, ms, RIG, &PALETTE, seed).lit())
            .map(|(_, _, c)| palette.iter().position(|p| *p == rgb(c)))
            .collect();
        assert!(
            colors.len() == 1 && !colors.contains(&None),
            "seed {seed}: {colors:?}"
        );
        seen.extend(colors);
    }
    assert_eq!(seen.len(), 3);
    // Through the palette as they fade: red, to green half way, to blue.
    let fading = FireworksParams {
        colors: FireworksColors::Fade,
        ..plain()
    };
    for (ms, color) in [
        (0, RED),
        (250, [0.5, 0.5, 0.0]),
        (500, GREEN),
        (750, [0.0, 0.5, 0.5]),
    ] {
        let lit = draw(&fading, ms).lit();
        assert!(!lit.is_empty());
        for (_, _, c) in lit {
            let close = rgb(c).iter().zip(color).all(|(a, b)| (a - b).abs() < 1e-5);
            assert!(close, "at {ms}: {c:?}");
        }
    }
    // No palette at all is white, and the color is never dimmed: the fade is in the alpha.
    let white = draw_with(&plain(), 500, RIG, &[], SEED);
    assert!(
        white
            .lit()
            .iter()
            .all(|&(_, _, c)| rgb(c) == [1.0; 3] && c.a <= 0.5)
    );
}

// ---------------------------------------------------------------------------------------------
// Nothing but the time

#[test]
fn a_frame_depends_on_nothing_but_its_time() {
    // Everything on: random moments and places, tails, falling, slowing, a color for each spark.
    let p = FireworksParams {
        rate: 3.0,
        colors: FireworksColors::Spark,
        ..FireworksParams::default()
    };
    let times = [0u64, 25, 475, 1337, 4000, 20_025, 59_975];
    let forward: Vec<_> = times.iter().map(|&ms| draw(&p, ms).bits()).collect();
    // Backward, and each again after other moments have been drawn.
    for (&ms, bits) in times.iter().zip(&forward).rev() {
        assert_eq!(&draw(&p, ms).bits(), bits, "at {ms}");
        let _ = draw(&p, ms + 300);
        assert_eq!(&draw(&p, ms).bits(), bits, "at {ms}");
    }
    assert!(forward.windows(2).all(|w| w[0] != w[1]), "it moves");
    // One pixel asked for on its own is the pixel of the whole frame.
    let fireworks = Fireworks::new(
        &p,
        &at_ms(1337),
        Colors::new(&PALETTE),
        SEED,
        RIG,
        &RenderContext::default(),
    );
    for (x, y) in [(0, 0), (50, 50), (13, 77), (100, 100), (64, 5)] {
        let alone = fireworks.shade(&pixel(x, y, RIG));
        let bits = [alone.r, alone.g, alone.b, alone.a].map(f32::to_bits);
        assert_eq!(bits, forward[3][(y * 101 + x) as usize]);
    }
    // Another seed is another show; the same seed is the same one.
    assert_ne!(draw_with(&p, 1337, RIG, &PALETTE, SEED + 1).bits(), forward[3]);
    // Through the renderer's own door, with the settings clamped first.
    let shader = Shader::new(
        &EffectParams::Fireworks(p),
        &at_ms(1337),
        Colors::new(&PALETTE),
        SEED,
        RIG,
    );
    assert_eq!(Frame::of(&shader, RIG).bits(), forward[3]);
}

/// A 24 by 16 matrix and a 60-pixel string beside it.
fn show() -> Show {
    let mut show = Show::new("t");
    show.props.push(Prop::new(
        "Matrix",
        ShapeSource::Generator(Generator::Matrix {
            columns: 24,
            rows: 16,
            width: 2.4,
            height: 1.6,
            wiring: Default::default(),
        }),
    ));
    let mut string = Prop::new(
        "String",
        ShapeSource::Generator(Generator::Line {
            nodes: 60,
            length: 3.0,
        }),
    );
    string.transform = Transform {
        position: Vec3::new(4.0, 0.0, 0.0),
        ..Transform::default()
    };
    show.props.push(string);
    show
}

#[test]
fn scrubbing_shows_what_playing_shows_on_any_number_of_threads() {
    let show = show();
    let mut seq = Sequence::new("s", 6000);
    seq.frame_ms = FRAME_MS;
    for (prop, rate) in show.props.iter().zip([2.0, 1.0]) {
        let params = FireworksParams {
            rate,
            life: 0.6,
            ..FireworksParams::default()
        };
        let mut row = Row::new(Target::Prop(prop.id));
        row.layers[0].effects = vec![
            Effect::new(EffectKind::Fireworks, 0, 6000)
                .with_params(EffectParams::Fireworks(params))
                .with_palette(PALETTE),
        ];
        seq.rows.push(row);
    }
    let map = pf_mapping::map_show(&show).0;
    let mut played = Renderer::new(&show, &map);
    played.set_audio(AudioSource::none());
    let mut frames = Vec::new();
    for index in 0..240 {
        let mut frame = vec![0u8; played.frame_len()];
        played.render_frame(&seq, index, &mut frame);
        frames.push(frame);
    }
    // Both props light up, the matrix (the first 24 × 16 pixels) and the string after it, and
    // both go dark between bursts.
    let split = 24 * 16 * 3;
    let lit = |part: &[u8]| part.iter().any(|&b| b > 0);
    for (name, range) in [("matrix", 0..split), ("string", split..frames[0].len())] {
        let alight = frames.iter().filter(|f| lit(&f[range.clone()])).count();
        assert!(
            alight > 40 && alight < 240,
            "{name}: lit on {alight} of 240 frames"
        );
    }
    // Any frame on its own, in any order, drawn on one thread or three.
    let pool = RenderPool::new(3).expect("three threads");
    for threads in [None, Some(pool)] {
        let mut scrubbed = Renderer::new(&show, &map);
        scrubbed.set_audio(AudioSource::none());
        scrubbed.set_pool(threads);
        for index in [239u64, 0, 17, 120, 16, 18, 199, 1, 77] {
            let mut frame = vec![0u8; scrubbed.frame_len()];
            scrubbed.render_frame(&seq, index, &mut frame);
            assert_eq!(frame, frames[index as usize], "frame {index}");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Every shape of target

#[test]
fn on_a_string_a_burst_runs_both_ways_along_it() {
    let string = Canvas { columns: 61, rows: 1 };
    let p = plain();
    // As it goes off: the one pixel in the middle.
    let lit = draw_on(&p, 0, string).lit();
    assert_eq!(lit.len(), 1);
    assert_eq!((lit[0].0, lit[0].2.a), (30, 1.0));
    // Half way: twelve sparks each way, 3 to 20 cells out, and dark between them and the middle.
    let frame = draw_on(&p, 500, string);
    let lit = frame.lit();
    let left = lit.iter().filter(|&&(x, _, _)| x < 30).count();
    let right = lit.len() - left;
    assert!(left >= 9 && right >= 9, "{left} and {right}");
    for &(x, _, c) in &lit {
        let away = (x - 30).abs() as f32;
        assert!((2.0..=21.0).contains(&away), "pixel {x}");
        assert!(c.a <= 0.5 + 1e-6);
    }
    // Far more of the burst shows than on the one row of a matrix it would cross.
    let across = draw(&p, 500);
    let row: usize = (0..101).filter(|&x| across.at(x, 50).a > 0.0).count();
    assert!(lit.len() >= 3 * row.max(1), "{} against {row}", lit.len());
    // The settings do what they do anywhere: fewer sparks, less light; tails join them up.
    let fewer = FireworksParams { sparks: 6, ..p };
    assert!(draw_on(&fewer, 500, string).lit().len() < lit.len());
    let tails = FireworksParams { trail: 1.0, ..p };
    let joined = draw_on(&tails, 500, string);
    assert!((12..=48).all(|x| joined.at(x, 0).a > 0.0 || (x - 30i32).abs() <= 1));
    // Up a string that stands on end, and falling down it.
    let standing = Canvas { columns: 1, rows: 61 };
    let up = draw_on(&p, 500, standing);
    for (x, _, c) in lit {
        assert_eq!(up.at(0, x).a, c.a, "pixel {x}");
    }
    let falling = FireworksParams { fall: 0.5, ..p };
    let fell = draw_on(&falling, 500, standing);
    for (_, y, c) in up.lit() {
        assert!((fell.at(0, y - 5).a - c.a).abs() < 1e-3, "pixel {y}");
    }
    // A few rows are still a line: every row shows the same.
    let ribbon = Canvas { columns: 61, rows: 3 };
    let rows = draw_on(&p, 500, ribbon);
    for x in 0..61 {
        assert_eq!(rows.at(x, 0), frame.at(x, 0));
        assert_eq!(rows.at(x, 2), frame.at(x, 0));
    }
}

#[test]
fn a_small_matrix_a_tree_and_a_big_group_all_show_a_burst() {
    // As a new effect is, but for where it goes off and how often.
    let p = FireworksParams {
        rate: 0.2,
        random_location: false,
        ..FireworksParams::default()
    };
    for (columns, rows) in [(16, 8), (12, 12), (16, 50), (50, 20), (139, 257), (400, 200)] {
        let canvas = Canvas { columns, rows };
        let pixels = (columns * rows) as usize;
        // Half way through its glow the burst is open: a good many pixels lit, most of them
        // dark, and every one a palette color at a brightness between 0 and 1.
        let frame = draw_on(&p, 750, canvas);
        let lit = frame.lit();
        assert!(lit.len() >= 12, "{columns} × {rows}: {} lit", lit.len());
        assert!(lit.len() * 2 < pixels, "{columns} × {rows}: {} lit", lit.len());
        for &(_, _, c) in &lit {
            assert!([RED, GREEN, BLUE].contains(&rgb(c)) && c.a > 0.0 && c.a <= 0.5);
        }
        // It is gone when its glow time is up, until the next.
        for ms in (1500..5000).step_by(250) {
            assert!(draw_on(&p, ms, canvas).dark(), "{columns} × {rows} at {ms}");
        }
    }
}

#[test]
fn an_outline_catches_a_burst_where_its_sparks_cross_it() {
    // A window frame of 320 pixels round a rectangle five wide to three high. Its buffer has
    // about as many cells as it has pixels (23 by 14), so a cell is some four pixels of the
    // frame's edge, and nearly every cell is empty.
    let canvas = Canvas {
        columns: 23,
        rows: 14,
    };
    let outline: Vec<Pixel> = (0..320u32)
        .map(|n| {
            let (u, v) = match n {
                0..100 => (n as f32 / 100.0, 0.0),
                100..160 => (1.0, (n - 100) as f32 / 60.0),
                160..260 => (1.0 - (n - 160) as f32 / 100.0, 1.0),
                _ => (0.0, 1.0 - (n - 260) as f32 / 60.0),
            };
            Pixel {
                u,
                v,
                index: n,
                count: 320,
            }
        })
        .collect();
    let lit_on = |p: &FireworksParams, ms: u64| {
        let fireworks = Fireworks::new(
            p,
            &at_ms(ms),
            Colors::new(&PALETTE),
            SEED,
            canvas,
            &RenderContext::default(),
        );
        outline.iter().filter(|px| fireworks.shade(px).a > 0.1).count()
    };
    // As a new effect is, a burst a second for twenty seconds: most of the time a stretch of
    // the frame is alight, a good length of it at the most, and never the whole of it.
    let p = FireworksParams::default();
    let lit: Vec<usize> = (0..20_000).step_by(50).map(|ms| lit_on(&p, ms)).collect();
    let alight = lit.iter().filter(|&&n| n >= 8).count();
    assert!(alight * 2 > lit.len(), "{alight} of {} moments", lit.len());
    let most = *lit.iter().max().unwrap();
    assert!((40..300).contains(&most), "{most} pixels at the most");
    // Bursts that happen less often leave it dark between them.
    let rare = FireworksParams { rate: 0.2, ..p };
    let dark = (0..20_000)
        .step_by(50)
        .filter(|&ms| lit_on(&rare, ms) == 0)
        .count();
    assert!(dark > 200, "{dark} of 400 moments");
}

#[test]
fn tiny_buffers_and_hostile_input_never_panic_or_give_anything_but_a_color() {
    let sizes = [
        (0, 0),
        (1, 1),
        (1, 2),
        (2, 1),
        (2, 2),
        (3, 3),
        (4, 4),
        (5, 4),
        (1, 500),
        (500, 1),
        (3, 1000),
        (u32::MAX, 1),
        (u32::MAX, u32::MAX),
    ];
    let odd = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0, 0.0, 1e30];
    let mut settings = vec![FireworksParams::default(), plain()];
    for &v in &odd {
        settings.push(FireworksParams {
            rate: v,
            sparks: 0,
            size: v,
            life: v,
            trail: v,
            fall: v,
            colors: FireworksColors::Spark,
            trigger_level: v,
            random_location: false,
            center_x: v,
            center_y: v,
            spark_size: v,
            slowing: v,
            soft: v < 0.0,
            scale: v > 0.0,
            ..FireworksParams::default()
        });
        settings.push(FireworksParams {
            rate: 20.0,
            sparks: u32::MAX,
            size: 1000.0,
            life: 10.0,
            trail: 1.0,
            fall: 2.0,
            spark_size: v,
            colors: FireworksColors::Fade,
            ..FireworksParams::default()
        });
    }
    let pixels = [
        (0.0, 0.0),
        (0.5, 0.5),
        (1.0, 1.0),
        (0.25, 0.9),
        (-3.0, 7.0),
        (f32::NAN, 0.5),
        (0.5, f32::INFINITY),
        (f32::NEG_INFINITY, f32::NAN),
    ];
    let times = [
        EffectTime::within(0, 1, 0),
        EffectTime::within(0, 60_000, 1).with_frame_ms(0),
        EffectTime::within(0, 60_000, 999),
        EffectTime::within(5, 5, 5),
        EffectTime::within(0, u64::MAX, u64::MAX / 2),
        EffectTime::within(u64::MAX - 10, u64::MAX, u64::MAX),
    ];
    let mut drawn = 0;
    for p in &settings {
        for &(columns, rows) in &sizes {
            for time in &times {
                let canvas = Canvas { columns, rows };
                for palette in [&PALETTE[..], &[]] {
                    let raw = Fireworks::new(
                        p,
                        time,
                        Colors::new(palette),
                        u64::MAX,
                        canvas,
                        &RenderContext::default(),
                    );
                    let clamped = Shader::new(
                        &EffectParams::Fireworks(*p),
                        time,
                        Colors::new(palette),
                        7,
                        canvas,
                    );
                    for &(u, v) in &pixels {
                        let px = Pixel {
                            u,
                            v,
                            index: 0,
                            count: 1,
                        };
                        for c in [raw.shade(&px), clamped.shade(&px)] {
                            let ok = |x: f32| x.is_finite() && (0.0..=1.0).contains(&x);
                            assert!(
                                ok(c.r) && ok(c.g) && ok(c.b) && ok(c.a),
                                "{p:?} on {columns} × {rows}: {c:?}"
                            );
                            drawn += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(drawn > 10_000);
    // A single pixel is lit while a burst is alight on it, and dark after.
    let one = Canvas { columns: 1, rows: 1 };
    assert_eq!(draw_on(&plain(), 0, one).at(0, 0).a, 1.0);
    assert!(draw_on(&plain(), 1500, one).dark());
    // Marks and music that make no sense set nothing off, and nothing breaks.
    let tracks = [track(&[0, 0, 5, u64::MAX - 1])];
    let music = song(&[(0.2, 1.0)]);
    let cx = RenderContext::new(Some(Audio::new(&music, 1)), &tracks, 0);
    for p in [
        FireworksParams {
            timing_track: Some(tracks[0].id),
            ..plain()
        },
        FireworksParams {
            fire_on_music: true,
            trigger_level: 0.0,
            life: 10.0,
            ..plain()
        },
    ] {
        for time in &times {
            let fireworks = Fireworks::new(&p, time, Colors::new(&PALETTE), SEED, RIG, &cx);
            let c = fireworks.shade(&pixel(50, 50, RIG));
            assert!(c.a.is_finite() && (0.0..=1.0).contains(&c.a));
        }
    }
}
