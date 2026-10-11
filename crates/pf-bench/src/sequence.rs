//! Synthetic sequences for a synthetic show, built in code: the same sequence every time.
//!
//! A sequence is timed to a made-up song at 120 beats a minute (see [`synthetic_audio`]), in
//! four-second phrases. There are two:
//!
//! - **typical**: every prop has a row of effects suited to it, with gaps; about a third of the
//!   rows have two to four layers; each group has a row that plays part of the time; some effects
//!   have fades, sparkles, blur, or a setting that changes over the effect.
//! - **worst case**: every prop row has four layers of heavy effects for the whole sequence, with
//!   sparkles, blur, and curves, and every group row two more on top.
//!
//! Both use the same sixteen kinds of effect ([`KINDS`]): cheap ones (On, Color Wash), drawn ones
//! (Plasma, Butterfly, Fire, Text, Picture, Dancer), ones that follow the music (VU Meter, Pulse),
//! and the rest between.

use crate::show::{Category, GroupKind, SyntheticShow, fixed_id};
use pf_render::{AudioSource, AudioTrack, Pictures};
use pf_sequence::{
    Axis, BarsParams, Blend, ButterflyParams, ColorWashParams, Curve, CurveShape, DancerParams, Effect,
    EffectId, EffectKind, EffectParams, Layer, Mark, MeteorsParams, PictureFit, PictureMovement,
    PictureParams, PinwheelParams, PlasmaColors, PlasmaParams, PulseParams, PulseSource, Rgb, Row, RowId,
    Sequence, SpiralParams, Target, TextMovement, TextParams, TimingKind, TimingTrack, TimingTrackId,
    TwinkleParams, VuMeterParams, VuMeterType,
};
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;
use std::sync::Arc;

/// Forty frames a second.
pub const FRAME_MS: u32 = 25;
/// The song's beat: 120 a minute.
pub const BEAT_MS: u64 = 500;
/// Eight beats: effects start and end on phrases.
pub const PHRASE_MS: u64 = 4_000;
/// How long a synthetic sequence is unless asked otherwise.
pub const DEFAULT_SECONDS: u32 = 90;
/// What the Picture effects name as their file (the picture is made in memory, never read).
pub const PICTURE_FILE: &str = "pf-bench/snowflake.png";
const SAMPLE_RATE: u32 = 44_100;

/// The kinds of effect the synthetic sequences use.
pub const KINDS: [EffectKind; 16] = [
    EffectKind::On,
    EffectKind::ColorWash,
    EffectKind::Bars,
    EffectKind::Spiral,
    EffectKind::Pinwheel,
    EffectKind::Plasma,
    EffectKind::Butterfly,
    EffectKind::Twinkle,
    EffectKind::Meteors,
    EffectKind::Fire,
    EffectKind::Ripple,
    EffectKind::Text,
    EffectKind::VuMeter,
    EffectKind::Pulse,
    EffectKind::Picture,
    EffectKind::Dancer,
];

/// Which synthetic sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "camelCase")]
pub enum Variant {
    /// A representative sequence.
    Typical,
    /// Every prop busy with layers of heavy effects.
    #[value(name = "worst", alias = "worst-case")]
    WorstCase,
}

impl Variant {
    pub const ALL: [Variant; 2] = [Variant::Typical, Variant::WorstCase];

    pub fn label(self) -> &'static str {
        match self {
            Variant::Typical => "typical",
            Variant::WorstCase => "worst",
        }
    }
}

/// A synthetic sequence and what it's made of.
#[derive(Debug, Clone)]
pub struct SyntheticSequence {
    pub sequence: Sequence,
    pub summary: SequenceSummary,
}

/// The music and the picture a synthetic sequence draws from, as a renderer takes them.
#[derive(Debug, Clone)]
pub struct Media {
    pub audio: AudioSource,
    pub pictures: Pictures,
}

/// What a synthetic sequence is made of, for reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceSummary {
    pub variant: Variant,
    pub seconds: u32,
    pub frame_ms: u32,
    pub rows: usize,
    /// Rows with two or more layers.
    pub layered_rows: usize,
    pub most_layers: usize,
    pub effects: usize,
    pub by_kind: Vec<KindCount>,
    pub with_fades: usize,
    pub with_sparkles: usize,
    pub with_blur: usize,
    pub with_curves: usize,
    /// How many effects draw in a frame, on average and at most.
    pub effects_per_frame: f32,
    pub most_effects_per_frame: usize,
    /// How many times each of the show's pixels is drawn in a frame, on average and at most
    /// (a pixel under a prop effect and a group effect is drawn twice).
    pub layers_per_pixel: f32,
    pub most_layers_per_pixel: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KindCount {
    pub kind: String,
    pub effects: usize,
}

/// Mixes `x` into a number that looks random and is the same every time.
fn mixed(x: u64) -> u64 {
    let x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// A roll of the dice for row `row` at slot `slot`: the same every time, different for each `what`.
fn roll(row: u64, slot: u64, what: u64) -> u64 {
    mixed(mixed(mixed(row) ^ slot) ^ what)
}

const PALETTES: [[Rgb; 3]; 5] = [
    [Rgb::RED, Rgb::GREEN, Rgb::WHITE],
    [Rgb::new(0, 120, 255), Rgb::WHITE, Rgb::new(0, 255, 220)],
    [
        Rgb::new(255, 170, 0),
        Rgb::new(255, 60, 0),
        Rgb::new(255, 240, 180),
    ],
    [Rgb::new(255, 0, 180), Rgb::new(90, 0, 255), Rgb::new(0, 220, 255)],
    [Rgb::RED, Rgb::new(255, 200, 0), Rgb::BLUE],
];

/// What suits each kind of prop, in a typical sequence.
fn menu(category: Category) -> &'static [EffectKind] {
    use EffectKind::*;
    match category {
        Category::Outline => &[On, ColorWash, Twinkle, Meteors, Bars, Pulse, Ripple],
        Category::Arch => &[Bars, ColorWash, Meteors, Twinkle, On, Pulse, Butterfly],
        Category::Tree => &[
            Spiral, Pinwheel, Bars, Meteors, Fire, Twinkle, Butterfly, ColorWash, Ripple, Plasma,
        ],
        Category::Accent => &[Pinwheel, Ripple, On, ColorWash, Twinkle, Pulse, Spiral],
        Category::Matrix => &[
            Plasma, Butterfly, Fire, Text, VuMeter, Picture, Dancer, Pinwheel, Ripple, Meteors, Bars, Spiral,
            ColorWash,
        ],
    }
}

/// What goes over a row's first layer in a typical sequence.
const OVERLAYS: [EffectKind; 6] = [
    EffectKind::Twinkle,
    EffectKind::Meteors,
    EffectKind::Pulse,
    EffectKind::Bars,
    EffectKind::Ripple,
    EffectKind::ColorWash,
];

/// The heavy effects each kind of prop gets in the worst case.
fn heavy(category: Category) -> &'static [EffectKind] {
    use EffectKind::*;
    match category {
        Category::Matrix => &[
            Plasma, Butterfly, Fire, Pinwheel, Ripple, Spiral, VuMeter, Meteors, Text, Dancer, Picture,
        ],
        Category::Tree => &[
            Spiral, Pinwheel, Butterfly, Plasma, Fire, Meteors, Ripple, Twinkle,
        ],
        _ => &[
            Butterfly, Plasma, Meteors, Twinkle, Bars, Ripple, Pulse, ColorWash,
        ],
    }
}

/// What a group's row plays, and how much of the time (percent of phrases) in a typical sequence.
fn group_menu(kind: GroupKind) -> (&'static [EffectKind], u64) {
    use EffectKind::*;
    match kind {
        GroupKind::Of(Category::Outline) => (&[ColorWash, Twinkle, Bars], 35),
        GroupKind::Of(Category::Arch) => (&[Bars, Meteors, ColorWash], 40),
        GroupKind::Of(Category::Tree) => (&[Spiral, Fire, Twinkle], 30),
        GroupKind::Of(Category::Accent) => (&[Pinwheel, ColorWash, Twinkle], 30),
        GroupKind::Of(Category::Matrix) => (&[Plasma, Butterfly, Ripple], 15),
        GroupKind::WholeHouse => (&[Ripple, ColorWash, Twinkle, Bars], 20),
    }
}

/// The heavy effects every group gets in the worst case.
const GROUP_HEAVY: [EffectKind; 6] = [
    EffectKind::Plasma,
    EffectKind::Butterfly,
    EffectKind::Ripple,
    EffectKind::Spiral,
    EffectKind::Twinkle,
    EffectKind::Meteors,
];

const BLENDS: [Blend; 4] = [Blend::Add, Blend::Max, Blend::Normal, Blend::Average];

/// What an effect is drawn on: decides its settings and whether blur is worth having.
#[derive(Clone, Copy)]
pub(crate) struct On {
    /// A matrix's rows, for sizing text.
    pub rows: u32,
    /// A surface (a matrix, a tree, or a group) rather than a line.
    pub surface: bool,
}

/// Makes a sequence's effects.
pub(crate) struct Maker {
    variant: Variant,
    beats: TimingTrackId,
    effects: u64,
}

impl Maker {
    /// One effect of `kind` with nothing added: no fades, sparkles, blur, or curves.
    pub(crate) fn plain(&mut self, kind: EffectKind, span: (u64, u64), dice: u64, on: On) -> Effect {
        let mut effect = Effect::new(kind, span.0, span.1)
            .with_params(self.params(kind, dice, on))
            .with_palette(PALETTES[(dice >> 8) as usize % PALETTES.len()]);
        self.effects += 1;
        // The id seeds the effect's randomness, so it is spread out rather than counted.
        effect.id = EffectId(fixed_id(5, mixed(self.effects) >> 8));
        effect
    }

    /// The settings for `kind`, varied a little by `dice`.
    fn params(&self, kind: EffectKind, dice: u64, on: On) -> EffectParams {
        match kind {
            EffectKind::ColorWash => EffectParams::ColorWash(ColorWashParams {
                cycles: 2.0,
                ..Default::default()
            }),
            EffectKind::Bars => EffectParams::Bars(BarsParams {
                count: 4 + (dice % 5) as u32,
                speed: 1.0,
                axis: if dice & 8 == 0 {
                    Axis::Vertical
                } else {
                    Axis::Horizontal
                },
                ..Default::default()
            }),
            EffectKind::Spiral => EffectParams::Spiral(SpiralParams {
                count: 3 + (dice % 4) as u32,
                ..Default::default()
            }),
            EffectKind::Pinwheel => EffectParams::Pinwheel(PinwheelParams {
                arms: 3 + (dice % 4) as u32,
                ..Default::default()
            }),
            EffectKind::Plasma => EffectParams::Plasma(PlasmaParams {
                colors: if dice & 1 == 0 {
                    PlasmaColors::Palette
                } else {
                    PlasmaColors::Rainbow
                },
                ..Default::default()
            }),
            EffectKind::Butterfly => EffectParams::Butterfly(ButterflyParams {
                pattern: 1 + (dice % 4) as u32,
                ..Default::default()
            }),
            EffectKind::Twinkle => EffectParams::Twinkle(TwinkleParams {
                density: 0.4,
                rate: 2.0,
            }),
            EffectKind::Meteors => EffectParams::Meteors(MeteorsParams {
                count: 10 + (dice % 20) as u32,
                ..Default::default()
            }),
            EffectKind::Text => EffectParams::Text(TextParams {
                text: if dice & 1 == 0 {
                    "MERRY CHRISTMAS"
                } else {
                    "PIXELFLOW 200K"
                }
                .to_string(),
                movement: TextMovement::Left,
                size: (on.rows * 6 / 10).clamp(8, 48),
                ..Default::default()
            }),
            EffectKind::VuMeter => EffectParams::VuMeter(VuMeterParams {
                meter: [
                    VuMeterType::Spectrogram,
                    VuMeterType::VolumeBars,
                    VuMeterType::Waveform,
                ][(dice % 3) as usize],
                bars: 16,
                ..Default::default()
            }),
            EffectKind::Pulse => EffectParams::Pulse(PulseParams {
                source: if dice & 1 == 0 {
                    PulseSource::Marks
                } else {
                    PulseSource::Bass
                },
                timing_track: Some(self.beats),
                ..Default::default()
            }),
            EffectKind::Picture => EffectParams::Picture(PictureParams {
                file: PICTURE_FILE.to_string(),
                fit: PictureFit::Fill,
                movement: [
                    PictureMovement::None,
                    PictureMovement::Left,
                    PictureMovement::ZoomIn,
                ][(dice % 3) as usize],
                ..Default::default()
            }),
            EffectKind::Dancer => EffectParams::Dancer(DancerParams {
                timing_track: Some(self.beats),
                count: 1 + (dice % 3) as u32,
                bass_bounce: 0.5,
                ..Default::default()
            }),
            kind => EffectParams::default_for(kind),
        }
    }

    /// One effect, with the fades, sparkles, blur, and curve its dice give it. `layer` is its
    /// layer on the row (0 at the bottom).
    fn effect(&mut self, kind: EffectKind, span: (u64, u64), dice: u64, layer: usize, on: On) -> Effect {
        let mut effect = self.plain(kind, span, dice, on);
        if layer > 0 {
            effect.blend = BLENDS[(dice >> 12) as usize % BLENDS.len()];
        }
        let worst = self.variant == Variant::WorstCase;
        let chance = |shift: u32, percent: u64| (dice >> shift) % 100 < percent;
        if !worst && chance(16, 30) {
            effect.fade_in_ms = 250;
            effect.fade_out_ms = 500;
        }
        if if worst {
            layer.is_multiple_of(2)
        } else {
            chance(24, 20)
        } {
            effect.sparkles = 40 + ((dice >> 20) % 80) as u32;
        }
        if on.surface
            && if worst {
                layer == 0 && dice.is_multiple_of(3)
            } else {
                chance(32, 12)
            }
        {
            effect.blur = 1 + ((dice >> 28) % 4) as u32;
        }
        if if worst { layer % 2 == 1 } else { chance(40, 15) } {
            self.curve(&mut effect, dice);
        }
        effect
    }

    /// Makes the effect's speed (or the nearest thing it has) rise and fall over it.
    pub(crate) fn speed_curve(effect: &mut Effect) {
        let kind = effect.kind();
        let found = ["speed", "rate", "cycles", "height"].into_iter().find_map(|key| {
            let now = effect.params.number(key)?;
            Some((key, now, Effect::curve_range(kind, key)?))
        });
        if let Some((key, now, range)) = found {
            let low = (now * 0.5).max(range.min);
            let high = (now * 2.0).min(range.max);
            effect
                .curves
                .insert(key.to_string(), Curve::shaped(CurveShape::Sine, low, high, 2.0));
        }
    }

    /// Makes one of the effect's settings change over it: its speed, and now and then its
    /// sparkles with the music.
    fn curve(&self, effect: &mut Effect, dice: u64) {
        Self::speed_curve(effect);
        if (dice >> 44).is_multiple_of(3) {
            effect
                .curves
                .insert("sparkles".to_string(), Curve::music(0.0, 150.0, 0.0, false));
        }
    }
}

/// The song the sequences are timed to, as the renderer reads it: a kick on every beat, a snare
/// on every other, hats between, a bass line, and a pad, louder and quieter by phrase.
pub fn synthetic_audio(seconds: u32) -> Arc<AudioTrack> {
    let total = u64::from(seconds) * u64::from(SAMPLE_RATE);
    let samples = (0..total).map(|i| {
        let t = i as f32 / SAMPLE_RATE as f32;
        let beat = (i * 2 / u64::from(SAMPLE_RATE)) as u32;
        let in_beat = t * 2.0 - beat as f32;
        let noise = (mixed(i) >> 40) as f32 / (1u64 << 23) as f32 - 1.0;
        let kick =
            (TAU * (50.0 + 80.0 * (-30.0 * in_beat).exp()) * in_beat / 2.0).sin() * (-8.0 * in_beat).exp();
        let snare = if beat % 2 == 1 {
            noise * (-18.0 * in_beat).exp()
        } else {
            0.0
        };
        let hat = noise * (-60.0 * (in_beat - 0.5).abs()).exp();
        let bass_note = [55.0, 43.65, 65.41, 49.0][(beat / 4 % 4) as usize];
        let bass = (TAU * bass_note * t).sin();
        let pad: f32 = [220.0f32, 277.18, 329.63, 440.0]
            .iter()
            .map(|f| (TAU * f * t).sin())
            .sum::<f32>()
            * (0.5 + 0.5 * (TAU * t / 8.0).sin());
        let loud = 0.55 + 0.15 * (beat / 8 % 4) as f32;
        (loud * (0.7 * kick + 0.3 * snare + 0.12 * hat + 0.25 * bass + 0.05 * pad)).clamp(-1.0, 1.0)
    });
    Arc::new(pf_analysis::audio_track(samples, SAMPLE_RATE, FRAME_MS))
}

/// The picture the Picture effects draw: a 256 × 256 PNG of a six-armed flake on clear.
pub fn synthetic_picture() -> Vec<u8> {
    let image = image::RgbaImage::from_fn(256, 256, |x, y| {
        let (dx, dy) = (x as f32 - 127.5, y as f32 - 127.5);
        let (radius, angle) = (dx.hypot(dy) / 127.5, dy.atan2(dx));
        let arm = (angle * 3.0).cos().abs().powf(8.0);
        let ring = (radius * 12.0).sin().abs();
        let lit = radius < 1.0 && (arm > 0.4 || ring > 0.92);
        if lit {
            let shade = (255.0 * (1.0 - 0.6 * radius)) as u8;
            image::Rgba([shade, (255.0 * arm.max(ring * 0.5)) as u8, 255, 255])
        } else {
            image::Rgba([0, 0, 0, 0])
        }
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("a PNG written to memory");
    bytes.into_inner()
}

/// A sequence `seconds` long with the song's beats and bars as timing tracks and no rows yet, and
/// what makes effects for it.
pub(crate) fn empty_sequence(name: String, variant: Variant, seconds: u32) -> (Sequence, Maker) {
    let duration_ms = u64::from(seconds) * 1000;
    let mut sequence = Sequence::new(name, duration_ms);
    sequence.frame_ms = FRAME_MS;
    let marks = |every: u64| -> Vec<Mark> {
        (0..duration_ms.div_ceil(every))
            .map(|n| Mark::new(n * every, ((n + 1) * every).min(duration_ms), (n + 1).to_string()))
            .collect()
    };
    let mut beats = TimingTrack::new("Beats", TimingKind::Beats, marks(BEAT_MS));
    beats.id = TimingTrackId(fixed_id(6, 0));
    let mut bars = TimingTrack::new("Bars", TimingKind::Bars, marks(BEAT_MS * 4));
    bars.id = TimingTrackId(fixed_id(6, 1));
    let maker = Maker {
        variant,
        beats: beats.id,
        effects: 0,
    };
    sequence.timing_tracks = vec![beats, bars];
    (sequence, maker)
}

/// The synthetic sequence of `variant` for `show`, `seconds` long.
pub fn synthetic_sequence(show: &SyntheticShow, variant: Variant, seconds: u32) -> SyntheticSequence {
    let duration_ms = u64::from(seconds) * 1000;
    let (mut sequence, mut maker) = empty_sequence(
        format!("{} ({})", show.show.name, variant.label()),
        variant,
        seconds,
    );

    let phrases = duration_ms.div_ceil(PHRASE_MS);
    let span = |from: u64, phrases: u64| (from * PHRASE_MS, ((from + phrases) * PHRASE_MS).min(duration_ms));
    let worst = variant == Variant::WorstCase;
    let matrix_rows = |i: usize| match show.show.props[i].shape {
        pf_model::ShapeSource::Generator(pf_model::Generator::Matrix { rows, .. }) => rows,
        _ => 0,
    };
    for (i, prop) in show.props.iter().enumerate() {
        let row_n = i as u64;
        let on = On {
            rows: matrix_rows(i),
            surface: matches!(prop.category, Category::Tree | Category::Matrix),
        };
        let layers = if worst {
            4
        } else {
            // About a third of the rows are layered.
            match roll(row_n, 0, 1) % 100 {
                0..65 => 1,
                65..85 => 2,
                85..95 => 3,
                _ => 4,
            }
        };
        let mut row = Row::new(Target::Prop(prop.id));
        row.id = RowId(fixed_id(7, row_n));
        row.layers = (0..layers)
            .map(|layer| {
                let layer_n = layer as u64;
                let mut effects: Vec<Effect> = Vec::new();
                let mut phrase = 0;
                while phrase < phrases {
                    let dice = roll(row_n, phrase, 10 + layer_n);
                    if worst {
                        let kinds = heavy(prop.category);
                        let kind = kinds[(i + (phrase / 2) as usize + layer * 3) % kinds.len()];
                        effects.push(maker.effect(kind, span(phrase, 2), dice, layer, on));
                        phrase += 2;
                        continue;
                    }
                    // The first layer rests now and then; the layers over it come and go.
                    let rests = if layer == 0 { 12 } else { 40 };
                    if roll(row_n, phrase, 20 + layer_n) % 100 < rests {
                        phrase += 1;
                        continue;
                    }
                    let (kinds, length) = if layer == 0 {
                        (menu(prop.category), 1 + (dice >> 4) % 2)
                    } else {
                        (&OVERLAYS[..], 1)
                    };
                    // Each row works through its menu from a place of its own, so every kind
                    // comes round whatever part of the sequence plays.
                    let kind = kinds[(i * 5 + layer + effects.len()) % kinds.len()];
                    effects.push(maker.effect(kind, span(phrase, length), dice, layer, on));
                    phrase += length;
                }
                Layer { effects }
            })
            .collect();
        sequence.rows.push(row);
    }

    // Group rows come after the props', so where they're lit they cover them.
    for (i, group) in show.groups.iter().enumerate() {
        let row_n = 1_000_000 + i as u64;
        let on = On {
            rows: 0,
            surface: true,
        };
        let (kinds, percent) = group_menu(group.kind);
        let mut row = Row::new(Target::Group(group.id));
        row.id = RowId(fixed_id(7, row_n));
        row.layers = (0..if worst { 2 } else { 1 })
            .map(|layer| {
                let mut effects = Vec::new();
                for phrase in 0..phrases {
                    let dice = roll(row_n, phrase, 10 + layer as u64);
                    let kind = if worst {
                        GROUP_HEAVY[(i + phrase as usize + layer * 2) % GROUP_HEAVY.len()]
                    } else if roll(row_n, phrase, 20) % 100 < percent {
                        kinds[(dice >> 48) as usize % kinds.len()]
                    } else {
                        continue;
                    };
                    effects.push(maker.effect(kind, span(phrase, 1), dice, layer, on));
                }
                Layer { effects }
            })
            .collect();
        sequence.rows.push(row);
    }

    let summary = summarize(show, &sequence, variant, seconds);
    SyntheticSequence { sequence, summary }
}

impl Media {
    /// No music and no picture: effects that follow the music draw as in silence, and Picture
    /// effects draw nothing.
    pub fn none() -> Self {
        Self {
            audio: AudioSource::none(),
            pictures: Pictures::none(),
        }
    }
}

/// The song (`seconds` of it) and the picture, ready for a renderer.
pub fn synthetic_media(seconds: u32) -> Media {
    let picture = synthetic_picture();
    let pictures = Pictures::new(move |file| {
        if file == PICTURE_FILE {
            Ok(picture.clone())
        } else {
            Err("it isn't there".to_string())
        }
    });
    Media {
        audio: AudioSource::ready(synthetic_audio(seconds)),
        // Waiting, as an export reads them: the picture is there for the first frame it's drawn in.
        pictures: pictures.waiting(),
    }
}

/// How many pixels a row's target has.
pub(crate) fn target_pixels(show: &SyntheticShow, target: Target) -> u32 {
    match target {
        Target::Prop(id) => show.props.iter().find(|p| p.id == id).map_or(0, |p| p.pixels),
        Target::Group(id) => show.groups.iter().find(|g| g.id == id).map_or(0, |g| g.pixels),
        Target::Region { .. } => 0,
    }
}

fn summarize(show: &SyntheticShow, sequence: &Sequence, variant: Variant, seconds: u32) -> SequenceSummary {
    let effects: Vec<&Effect> = sequence.effects().collect();
    let by_kind = EffectKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let count = effects.iter().filter(|e| e.kind() == kind).count();
            (count > 0).then(|| KindCount {
                kind: kind.label().to_string(),
                effects: count,
            })
        })
        .collect();
    // Twice a second is often enough: effects start and end on four-second phrases.
    let moments: Vec<u64> = (0..sequence.duration_ms).step_by(500).collect();
    let pixels = show.pixels().max(1) as f32;
    let (mut drawn, mut most_drawn, mut covered, mut most_covered) = (0usize, 0usize, 0.0f32, 0.0f32);
    for &t in &moments {
        let (mut count, mut layers) = (0usize, 0u64);
        for row in &sequence.rows {
            let active = row
                .layers
                .iter()
                .flat_map(|l| &l.effects)
                .filter(|e| e.is_active_at(t))
                .count();
            count += active;
            layers += active as u64 * u64::from(target_pixels(show, row.target));
        }
        drawn += count;
        most_drawn = most_drawn.max(count);
        covered += layers as f32 / pixels;
        most_covered = most_covered.max(layers as f32 / pixels);
    }
    let n = moments.len().max(1) as f32;
    SequenceSummary {
        variant,
        seconds,
        frame_ms: sequence.frame_ms,
        rows: sequence.rows.len(),
        layered_rows: sequence.rows.iter().filter(|r| r.layers.len() > 1).count(),
        most_layers: sequence.rows.iter().map(|r| r.layers.len()).max().unwrap_or(0),
        effects: effects.len(),
        by_kind,
        with_fades: effects.iter().filter(|e| e.fade_in_ms > 0).count(),
        with_sparkles: effects.iter().filter(|e| e.sparkles > 0).count(),
        with_blur: effects.iter().filter(|e| e.blur > 0).count(),
        with_curves: effects.iter().filter(|e| !e.curves.is_empty()).count(),
        effects_per_frame: drawn as f32 / n,
        most_effects_per_frame: most_drawn,
        layers_per_pixel: covered / n,
        most_layers_per_pixel: most_covered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::show::synthetic_show;

    #[test]
    fn both_sequences_use_every_kind_and_open_as_a_file_would() {
        let show = synthetic_show(20_000);
        for variant in Variant::ALL {
            let made = synthetic_sequence(&show, variant, 60);
            let checked = pf_sequence::check_sequence(&made.sequence).expect("a sequence that opens");
            assert_eq!(checked, made.sequence, "nothing was clamped or dropped");
            // The typical sequence uses every kind; the worst case leaves out the cheapest.
            for kind in KINDS {
                let cheap = matches!(kind, EffectKind::On);
                assert!(
                    made.sequence.effects().any(|e| e.kind() == kind)
                        || (variant == Variant::WorstCase && cheap),
                    "{variant:?} has no {kind:?}"
                );
            }
            let issues = pf_sequence::validate_sequence(&made.sequence, &show.show);
            let errors: Vec<_> = issues
                .iter()
                .filter(|i| i.severity == pf_model::Severity::Error)
                .collect();
            assert!(errors.is_empty(), "{variant:?}: {errors:?}");
            let s = &made.summary;
            assert!(s.with_blur > 0 && s.with_sparkles > 0 && s.with_curves > 0);
            assert_eq!(s.rows, show.props.len() + show.groups.len());
        }
    }

    #[test]
    fn a_typical_sequence_layers_some_rows_and_the_worst_case_all_of_them() {
        let show = synthetic_show(20_000);
        let typical = synthetic_sequence(&show, Variant::Typical, 60).summary;
        let worst = synthetic_sequence(&show, Variant::WorstCase, 60).summary;
        assert!(typical.layered_rows > 0 && typical.layered_rows < typical.rows / 2);
        assert_eq!((typical.most_layers, worst.most_layers), (4, 4));
        assert_eq!(worst.layered_rows, worst.rows);
        // Every prop under four layers and two groups' two each.
        assert!(worst.layers_per_pixel >= 7.9, "{}", worst.layers_per_pixel);
        assert!(typical.layers_per_pixel > 0.8 && typical.layers_per_pixel < 3.0);
    }

    #[test]
    fn a_sequence_is_the_same_every_time_with_effects_that_never_overlap_on_a_layer() {
        let show = synthetic_show(1_500);
        let a = synthetic_sequence(&show, Variant::Typical, 30).sequence;
        let b = synthetic_sequence(&show, Variant::Typical, 30).sequence;
        assert_eq!(a, b);
        for layer in a.rows.iter().flat_map(|r| &r.layers) {
            for pair in layer.effects.windows(2) {
                assert!(pair[0].end_ms <= pair[1].start_ms);
            }
            assert!(layer.effects.iter().all(|e| e.end_ms <= a.duration_ms));
        }
    }

    #[test]
    fn the_song_has_beats_and_the_picture_is_a_picture() {
        let track = synthetic_audio(4);
        assert_eq!(track.frame_ms(), FRAME_MS);
        let loudest = (0..track.len() as u64).map(|f| track.peak(f)).fold(0.0, f32::max);
        assert!(loudest > 0.3, "{loudest}");
        let picture = image::load_from_memory(&synthetic_picture()).unwrap();
        assert_eq!((picture.width(), picture.height()), (256, 256));
    }
}
