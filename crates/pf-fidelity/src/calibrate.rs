//! Holding the look score against renders whose answer is known: `pf-fidelity calibrate`.
//!
//! No xLights is needed for this. One of PixelFlow's own renders of an effect stands in for
//! xLights' frames, and the effect is rendered again from other seeds, or with one setting
//! changed, and scored against it as an imported sequence is against its reference. Three
//! things are asked of the score:
//!
//! - **The same effect from other seeds matches itself.** If it doesn't, the score is calling
//!   chance a difference.
//! - **A setting that is wrong still shows.** Twice the bursts, half the size, another color,
//!   steady light where there should be bursts: each must come out close or different.
//! - **A short effect, or one with few events, says how little it can say.**
//! - **An effect that keeps time is still held to it**, alone, with sparkles on it, and under
//!   an effect that draws on chance.
//!
//! Every pair is scored moment by moment and, when the effect draws on chance, next to
//! PixelFlow's other seeds ([`crate::look`]), so the two can be read side by side. The seeds
//! are fixed: the same build prints the same tables.

use crate::chance::{self, Chance};
use crate::look::{Component, Components, Grid, Look, LookStats, Parts, Scratch, Verdict, next_to_seeds};
use crate::metrics::Limits;
use pf_model::{Generator, Group, Prop, Rgb, ShapeSource, Show, Transform, Vec3};
use pf_render::{AudioSource, Renderer, SceneGeometry};
use pf_sequence::{
    ChaseParams, CirclesParams, Direction, Effect, EffectId, EffectKind, EffectParams, FireParams, FireStyle,
    FireworksParams, Layer, LifeParams, LightningParams, LinesParams, MeteorsParams, Row, Sequence,
    ShapeParams, ShimmerParams, SnowflakesMotion, SnowflakesParams, SpiralParams, StrobeParams, Target,
    TendrilMovement, TendrilParams, TwinkleParams,
};
use pf_xlights::sequence::ByVerdict;
use std::fmt::Write as _;
use std::rc::Rc;

/// One of the calibration's tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    /// Each effect against itself from other seeds.
    Same,
    /// Each effect against itself with one thing wrong.
    Wrong,
    /// Fireworks of several lengths.
    Short,
    /// Effects that keep time, with their timing wrong.
    Timed,
}

/// How long the effects run, how many times each line is scored (each from fresh seeds), the
/// frame time, how many more renders from other seeds an effect that draws on chance is held
/// against, and the one table to make (all of them otherwise).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub seconds: u32,
    pub pairs: u32,
    pub frame_ms: u32,
    pub seeds: u32,
    pub only: Option<Table>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            seconds: 60,
            pairs: 8,
            frame_ms: 25,
            seeds: crate::compare::OTHER_SEEDS,
            only: None,
        }
    }
}

/// What the effects are drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum On {
    /// A matrix 32 by 16: every cell of the effect's grid has a pixel.
    Matrix,
    /// A group of eight outline props spread over a yard: about one cell in sixty of the
    /// group's grid has a pixel, as on a real yard's group of outlines, so most of what an
    /// effect draws falls between the props.
    Outlines,
}

impl On {
    fn title(self) -> &'static str {
        match self {
            On::Matrix => "matrix",
            On::Outlines => "outlines",
        }
    }
}

/// The scores of one way of comparing over a line's pairs.
#[derive(Debug, Clone, PartialEq)]
pub struct Tally {
    pub scores: Vec<f64>,
    pub verdicts: ByVerdict<usize>,
    /// The six parts, averaged over the pairs.
    pub parts: Components,
    /// How alike PixelFlow's own renders were from one seed to the next, averaged (a look next
    /// to the other seeds only).
    pub itself: Option<f64>,
}

impl Tally {
    fn of(looks: &[Look]) -> Self {
        let n = looks.len().max(1) as f64;
        let part = |c: Component| looks.iter().map(|l| l.components.get(c)).sum::<f64>() / n;
        let mut verdicts = ByVerdict::default();
        for look in looks {
            verdicts.add(look.verdict, 1);
        }
        let own: Vec<f64> = looks.iter().filter_map(|l| l.itself).collect();
        Self {
            scores: looks.iter().map(|l| l.score).collect(),
            verdicts,
            parts: Components {
                coverage: part(Component::Coverage),
                brightness: part(Component::Brightness),
                color: part(Component::Color),
                pattern: part(Component::Pattern),
                motion: part(Component::Motion),
                rhythm: part(Component::Rhythm),
            },
            itself: (!own.is_empty()).then(|| own.iter().sum::<f64>() / own.len() as f64),
        }
    }

    pub fn least(&self) -> f64 {
        self.scores.iter().copied().fold(f64::INFINITY, f64::min)
    }

    pub fn most(&self) -> f64 {
        self.scores.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    }

    pub fn middle(&self) -> f64 {
        let mut sorted = self.scores.clone();
        sorted.sort_by(f64::total_cmp);
        match sorted.len() {
            0 => 0.0,
            n if n % 2 == 1 => sorted[n / 2],
            n => (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0,
        }
    }
}

/// One pair of effects, scored over several pairs of seeds.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// The effect that should be shown, and what was shown in its place ("the same" for the
    /// same effect from another seed).
    pub effect: String,
    pub shown: String,
    pub on: On,
    pub seconds: u32,
    /// What chance decides in the effect shown: how the tool would compare it.
    pub chance: Chance,
    pub by_moment: Tally,
    /// Next to PixelFlow's other seeds; `None` when chance decides nothing in it.
    pub over: Option<Tally>,
}

impl Line {
    /// The scores the effect is judged on.
    pub fn judged(&self) -> &Tally {
        self.over.as_ref().unwrap_or(&self.by_moment)
    }
}

/// What the effects were drawn on: how many pixels, on a grid of how many cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drawn {
    pub on: On,
    pub pixels: usize,
    pub columns: u32,
    pub rows: u32,
}

/// The three tables.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    pub settings: Settings,
    pub drawn_on: Vec<Drawn>,
    /// Each effect against itself from another seed.
    pub same: Vec<Line>,
    /// Each effect against itself with one thing wrong.
    pub wrong: Vec<Line>,
    /// Fireworks of several lengths against itself and against twice the bursts.
    pub short: Vec<Line>,
    /// Effects that keep time, a little early or a little fast.
    pub timed: Vec<Line>,
}

/// An effect id from three numbers: a seed that is the same on every run.
fn id(line: u64, pair: u64, render: u64) -> EffectId {
    let from = line
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(pair.wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
        .wrapping_add(render.wrapping_mul(0x1656_67B1_9E37_79F9));
    chance::id_of(chance::mixed(from), chance::mixed(!from))
}

/// A show to draw on, its renderer, and where the target's pixels are in a frame.
struct Stage {
    on: On,
    target: Target,
    renderer: Renderer,
    grid: Rc<Grid>,
    /// Cells across and up the target's own grid.
    cells: (u32, u32),
    /// Each of the target's pixels: its first byte in the show's frame.
    at: Vec<usize>,
}

impl Stage {
    fn new(on: On) -> Self {
        let mut show = Show::new("calibration");
        let target = match on {
            On::Matrix => {
                let matrix = Prop::new(
                    "Matrix",
                    ShapeSource::Generator(Generator::Matrix {
                        columns: 32,
                        rows: 16,
                        width: 3.2,
                        height: 1.6,
                        wiring: Default::default(),
                    }),
                );
                let id = matrix.id;
                show.props.push(matrix);
                Target::Prop(id)
            }
            On::Outlines => {
                // Rings, stars, and arches of 150 pixels each, in two rows across 8 by 4 metres.
                let mut group = Group::new("Outlines");
                for k in 0..8u32 {
                    let shape = match k % 3 {
                        0 => Generator::circle(150, 0.45),
                        1 => Generator::star(5, 150, 0.5, 0.22),
                        _ => Generator::arch(150, 0.9, 0.6),
                    };
                    let mut prop = Prop::new(format!("Outline {}", k + 1), ShapeSource::Generator(shape));
                    prop.transform = Transform {
                        position: Vec3::new((k % 4) as f32 * 2.4, (k / 4) as f32 * 2.6, 0.0),
                        ..Transform::default()
                    };
                    group.members.push(prop.id.into());
                    show.props.push(prop);
                }
                let id = group.id;
                show.groups.push(group);
                Target::Group(id)
            }
        };
        let map = pf_mapping::map_show(&show).0;
        let geometry = SceneGeometry::new(&show, &map);
        let buffer = geometry.buffer(target);
        let places: Vec<(f32, f32)> = buffer.pixels.iter().map(|px| (px.u, px.v)).collect();
        let grid = Rc::new(Grid::new(&places, buffer.columns, buffer.rows));
        // Every prop here is RGB, so pixel `g` of the show is three bytes from `3 g`.
        let at = buffer.show_pixels().iter().map(|&g| g as usize * 3).collect();
        let mut renderer = Renderer::new(&show, &map);
        renderer.set_audio(AudioSource::none());
        Self {
            on,
            target,
            renderer,
            grid,
            cells: (buffer.columns, buffer.rows),
            at,
        }
    }

    /// The target's pixels in every frame of a picture: its effects, one to a layer, the
    /// first at the bottom.
    fn render(&mut self, picture: &[Effect], frame_ms: u32) -> Vec<Vec<[u8; 3]>> {
        let end_ms = picture.iter().map(|effect| effect.end_ms).max().unwrap_or(0);
        let mut seq = Sequence::new("calibration", end_ms);
        seq.frame_ms = frame_ms;
        let mut row = Row::new(self.target);
        row.layers = picture
            .iter()
            .map(|effect| Layer {
                effects: vec![effect.clone()],
            })
            .collect();
        seq.rows.push(row);
        let mut frame = vec![0u8; self.renderer.frame_len()];
        (0..end_ms / u64::from(frame_ms))
            .map(|index| {
                self.renderer.render_frame(&seq, index, &mut frame);
                self.at
                    .iter()
                    .map(|&at| [frame[at], frame[at + 1], frame[at + 2]])
                    .collect()
            })
            .collect()
    }

    /// The look of the picture `ours` next to `theirs`: moment by moment, and (when chance
    /// decides something in `ours`) next to PixelFlow's renders of `ours` from the `others`
    /// seeds. As in a sequence, the picture is judged as its first effect's: by what chance
    /// decides in that one, or as no more than drawn by chance when it is another that draws.
    fn look(
        &mut self,
        ours: &[Effect],
        others: &[Vec<Effect>],
        theirs: &[Effect],
        frame_ms: u32,
    ) -> (Look, Option<Look>) {
        let chance = picture_chance(ours);
        let held = chance != Chance::Nothing && !others.is_empty();
        let reference = self.render(theirs, frame_ms);
        let mut renders = vec![self.render(ours, frame_ms)];
        if held {
            renders.extend(others.iter().map(|other| self.render(other, frame_ms)));
        }
        let grid = self.grid.clone();
        let pair = |a: &[Vec<[u8; 3]>], b: &[Vec<[u8; 3]>]| -> (Look, Parts) {
            let mut stats = LookStats::new(grid.clone(), frame_ms, &Limits::default());
            if held {
                stats = stats.over(chance);
            }
            let mut scratch = Scratch::default();
            for (frame, (ours, theirs)) in a.iter().zip(b).enumerate() {
                for (pixel, (&ours, &theirs)) in ours.iter().zip(theirs).enumerate() {
                    stats.add(&mut scratch, frame as u32, pixel, [ours, theirs]);
                }
                stats.end_frame(&mut scratch, frame as u32);
            }
            stats
                .looks()
                .unwrap_or_else(|| unreachable!("an effect has frames"))
        };
        let (by_moment, first) = pair(&renders[0], &reference);
        if !held {
            return (by_moment, None);
        }
        let mut between = vec![first];
        between.extend(renders[1..].iter().map(|render| pair(render, &reference).1));
        let mut within = Vec::new();
        for (a, one) in renders.iter().enumerate() {
            for other in &renders[a + 1..] {
                within.push(pair(one, other).1);
            }
        }
        (
            by_moment,
            Some(next_to_seeds(&by_moment, chance, &between, &within)),
        )
    }
}

/// What chance decides in a picture, judged as its first effect's.
fn picture_chance(picture: &[Effect]) -> Chance {
    let own = picture
        .first()
        .map_or(Chance::Nothing, |e| chance::chance(e).chance);
    let shared = picture[1.min(picture.len())..]
        .iter()
        .any(|e| chance::chance(e).chance != Chance::Nothing);
    if shared { own.max(Chance::Drawn) } else { own }
}

/// A pair to score: what should be shown, what is shown in its place, and what to call them.
struct Pair {
    effect: &'static str,
    shown: String,
    /// Each a picture: its effects, one to a layer, the first at the bottom.
    theirs: Vec<Effect>,
    ours: Vec<Effect>,
}

const WHITE: Rgb = Rgb::WHITE;
const RED: Rgb = Rgb::RED;
const GREEN: Rgb = Rgb::GREEN;
const BLUE: Rgb = Rgb::BLUE;
const MAGENTA: Rgb = Rgb::new(255, 0, 255);

fn effect(params: EffectParams, palette: &[Rgb], seconds: u32) -> Effect {
    Effect::new(params.kind(), 0, u64::from(seconds) * 1000)
        .with_params(params)
        .with_palette(palette.to_vec())
}

/// Fireworks as PixelFlow sets one up, in three colors.
fn fireworks(seconds: u32, change: impl Fn(&mut FireworksParams)) -> Effect {
    let mut p = FireworksParams::default();
    change(&mut p);
    effect(EffectParams::Fireworks(p), &[WHITE, RED, MAGENTA], seconds)
}

/// Fireworks that light a good part of the prop: a big burst a second of fat sparks with long
/// tails, so that half as much light is plainly less and twice as much isn't the whole prop.
fn big_fireworks(seconds: u32, change: impl Fn(&mut FireworksParams)) -> Effect {
    fireworks(seconds, |p| {
        p.rate = 1.0;
        p.sparks = 150;
        p.size = 90.0;
        p.spark_size = 6.0;
        p.trail = 0.6;
        change(p);
    })
}

/// Fireworks as an xLights sequence's usual settings come across: sixteen bursts over the
/// effect, sparks one cell wide flying two cells a frame for twenty-five frames.
fn imported_fireworks(seconds: u32, frame_ms: u32, change: impl Fn(&mut FireworksParams)) -> Effect {
    let mut p = FireworksParams {
        rate: 16.0 / seconds.max(1) as f32,
        sparks: 50,
        size: 50.0,
        life: 25.0 * frame_ms as f32 / 1000.0,
        trail: 0.0,
        fall: 0.1,
        spark_size: 1.0,
        slowing: 0.0,
        soft: false,
        scale: false,
        ..FireworksParams::default()
    };
    change(&mut p);
    effect(EffectParams::Fireworks(p), &[WHITE, RED, MAGENTA], seconds)
}

/// Fireworks with dark between its bursts: one every four seconds, glowing for 0.6 of one.
fn sparse_fireworks(seconds: u32, change: impl Fn(&mut FireworksParams)) -> Effect {
    fireworks(seconds, |p| {
        p.rate = 0.25;
        p.life = 0.6;
        change(p);
    })
}

/// Three balls two cells in radius: small enough that twice as many light twice as much.
fn small_balls() -> CirclesParams {
    CirclesParams {
        size: 2,
        ..CirclesParams::default()
    }
}

/// Every seeded kind as PixelFlow sets it up, with what chance decides in it or without.
fn kinds(seconds: u32, frame_ms: u32) -> Vec<(&'static str, Effect)> {
    let two = [RED, GREEN];
    let three = [RED, GREEN, BLUE];
    vec![
        ("Fireworks", fireworks(seconds, |_| {})),
        ("Fireworks, big bursts", big_fireworks(seconds, |_| {})),
        (
            "Fireworks, as imported",
            imported_fireworks(seconds, frame_ms, |_| {}),
        ),
        (
            "Fireworks, dark between bursts",
            sparse_fireworks(seconds, |_| {}),
        ),
        (
            "Circles",
            effect(EffectParams::Circles(small_balls()), &three, seconds),
        ),
        (
            "Meteors",
            effect(EffectParams::Meteors(MeteorsParams::default()), &two, seconds),
        ),
        (
            "Snowflakes, blowing",
            effect(
                EffectParams::Snowflakes(SnowflakesParams {
                    count: 20,
                    ..SnowflakesParams::default()
                }),
                &[WHITE],
                seconds,
            ),
        ),
        (
            "Snowflakes, falling",
            effect(
                EffectParams::Snowflakes(SnowflakesParams {
                    count: 20,
                    motion: SnowflakesMotion::Falling,
                    ..SnowflakesParams::default()
                }),
                &[WHITE],
                seconds,
            ),
        ),
        (
            "Lines",
            effect(EffectParams::Lines(LinesParams::default()), &three, seconds),
        ),
        (
            "Lightning",
            effect(
                EffectParams::Lightning(LightningParams::default()),
                &[WHITE],
                seconds,
            ),
        ),
        (
            "Tendril, wandering",
            effect(
                EffectParams::Tendril(TendrilParams {
                    movement: TendrilMovement::Random,
                    ..TendrilParams::default()
                }),
                &two,
                seconds,
            ),
        ),
        (
            "Shape",
            effect(EffectParams::Shape(ShapeParams::default()), &three, seconds),
        ),
        (
            "Shape, all at once",
            effect(
                EffectParams::Shape(ShapeParams {
                    random_start: false,
                    ..ShapeParams::default()
                }),
                &three,
                seconds,
            ),
        ),
        (
            "Life",
            effect(EffectParams::Life(LifeParams::default()), &three, seconds),
        ),
        (
            "Twinkle",
            effect(EffectParams::Twinkle(TwinkleParams::default()), &two, seconds),
        ),
        (
            "Strobe",
            effect(EffectParams::Strobe(StrobeParams::default()), &[WHITE], seconds),
        ),
        (
            "Fire, rolling",
            effect(EffectParams::Fire(FireParams::default()), &[RED], seconds),
        ),
        (
            "Fire, flickering",
            effect(
                EffectParams::Fire(FireParams {
                    style: FireStyle::Flicker,
                    ..FireParams::default()
                }),
                &[RED],
                seconds,
            ),
        ),
        ("On with sparkles", {
            let mut on = effect(EffectParams::default_for(EffectKind::On), &[BLUE], seconds);
            on.sparkles = 100;
            on
        }),
    ]
}

/// Fireworks with one thing wrong, and a few other kinds with twice as much of something.
fn mistakes(seconds: u32, frame_ms: u32) -> Vec<Pair> {
    let right = || big_fireworks(seconds, |_| {});
    let wrong = |shown: &str, ours: Effect| Pair {
        effect: "Fireworks, big bursts",
        shown: shown.to_string(),
        theirs: vec![right()],
        ours: vec![ours],
    };
    let changed =
        |shown: &str, change: &dyn Fn(&mut FireworksParams)| wrong(shown, big_fireworks(seconds, change));
    let recolored = |palette: &[Rgb]| {
        let mut effect = right();
        effect.palette = pf_sequence::Palette::new(palette.to_vec());
        effect
    };
    let dimmed = |brightness: f32| {
        let mut effect = right();
        effect.brightness = brightness;
        effect
    };
    let mut pairs = vec![
        changed("twice as many bursts", &|p| p.rate *= 2.0),
        changed("half as many bursts", &|p| p.rate /= 2.0),
        changed("bursts twice the size", &|p| p.size *= 2.0),
        changed("bursts half the size", &|p| p.size /= 2.0),
        wrong("all in a wrong color (green)", recolored(&[GREEN])),
        wrong(
            "one of its three colors wrong (green for red)",
            recolored(&[WHITE, GREEN, MAGENTA]),
        ),
        wrong("half as bright", dimmed(0.5)),
        changed("sparks glow twice as long", &|p| p.life *= 2.0),
        changed("sparks glow half as long", &|p| p.life /= 2.0),
        changed("every burst in the middle", &|p| p.random_location = false),
        // As much light, spread evenly over the time: twenty bursts for each one, of a
        // twentieth of the sparks.
        changed(
            "steady light: as many sparks, spread evenly over the time",
            &|p| {
                p.rate *= 20.0;
                p.sparks = (p.sparks / 20).max(1);
            },
        ),
        wrong(
            "a steady twinkle",
            effect(
                EffectParams::Twinkle(TwinkleParams::default()),
                &[WHITE, RED, MAGENTA],
                seconds,
            ),
        ),
        wrong("nothing at all", dimmed(0.0)),
    ];
    // The same mistakes on fireworks that light little of the prop.
    let sparse = |name: &'static str, shown: &str, theirs: Effect, ours: Effect| Pair {
        effect: name,
        shown: shown.to_string(),
        theirs: vec![theirs],
        ours: vec![ours],
    };
    pairs.extend([
        sparse(
            "Fireworks",
            "twice as many bursts",
            fireworks(seconds, |_| {}),
            fireworks(seconds, |p| p.rate *= 2.0),
        ),
        sparse("Fireworks", "half as bright", fireworks(seconds, |_| {}), {
            let mut effect = fireworks(seconds, |_| {});
            effect.brightness = 0.5;
            effect
        }),
        sparse(
            "Fireworks, dark between bursts",
            "twice as many bursts",
            sparse_fireworks(seconds, |_| {}),
            sparse_fireworks(seconds, |p| p.rate *= 2.0),
        ),
        sparse(
            "Fireworks, dark between bursts",
            "steady light: as many sparks, spread evenly over the time",
            sparse_fireworks(seconds, |_| {}),
            sparse_fireworks(seconds, |p| {
                p.rate *= 20.0;
                p.sparks = (p.sparks / 20).max(1);
            }),
        ),
        sparse(
            "Fireworks, as imported",
            "twice as many bursts",
            imported_fireworks(seconds, frame_ms, |_| {}),
            imported_fireworks(seconds, frame_ms, |p| p.rate *= 2.0),
        ),
        sparse(
            "Fireworks, as imported",
            "four times as many bursts",
            imported_fireworks(seconds, frame_ms, |_| {}),
            imported_fireworks(seconds, frame_ms, |p| p.rate *= 4.0),
        ),
    ]);
    let other =
        |name: &'static str, shown: &str, theirs: EffectParams, ours: EffectParams, palette: &[Rgb]| Pair {
            effect: name,
            shown: shown.to_string(),
            theirs: vec![effect(theirs, palette, seconds)],
            ours: vec![effect(ours, palette, seconds)],
        };
    let circles = small_balls();
    let snow = SnowflakesParams {
        count: 20,
        ..SnowflakesParams::default()
    };
    let meteors = MeteorsParams::default();
    let lightning = LightningParams::default();
    let life = LifeParams::default();
    pairs.extend([
        other(
            "Circles",
            "twice as many balls",
            EffectParams::Circles(circles),
            EffectParams::Circles(CirclesParams {
                count: circles.count * 2,
                ..circles
            }),
            &[RED, GREEN, BLUE],
        ),
        other(
            "Circles",
            "balls twice the size",
            EffectParams::Circles(circles),
            EffectParams::Circles(CirclesParams {
                size: circles.size * 2,
                ..circles
            }),
            &[RED, GREEN, BLUE],
        ),
        other(
            "Snowflakes, blowing",
            "twice as many flakes",
            EffectParams::Snowflakes(snow),
            EffectParams::Snowflakes(SnowflakesParams {
                count: snow.count * 2,
                ..snow
            }),
            &[WHITE],
        ),
        other(
            "Meteors",
            "twice as many meteors",
            EffectParams::Meteors(meteors),
            EffectParams::Meteors(MeteorsParams {
                count: meteors.count * 2,
                ..meteors
            }),
            &[RED, GREEN],
        ),
        other(
            "Meteors",
            "falling half as fast",
            EffectParams::Meteors(meteors),
            EffectParams::Meteors(MeteorsParams {
                speed: meteors.speed / 2.0,
                ..meteors
            }),
            &[RED, GREEN],
        ),
        other(
            "Lightning",
            "twice as many strikes",
            EffectParams::Lightning(lightning),
            EffectParams::Lightning(LightningParams {
                density: lightning.density * 2.0,
                ..lightning
            }),
            &[WHITE],
        ),
        other(
            "Life",
            "a new generation twice as often",
            EffectParams::Life(life),
            EffectParams::Life(LifeParams {
                speed: life.speed * 2,
                ..life
            }),
            &[RED, GREEN, BLUE],
        ),
    ]);
    pairs
}

/// Effects that keep time, with their timing wrong: alone, with sparkles on them, and under
/// falling snow. Chance has a hand in the last two pictures, and none in when the stripes turn,
/// where the chase has got to, or when the prop flickers.
fn timed(seconds: u32) -> Vec<Pair> {
    let snow = || {
        effect(
            EffectParams::Snowflakes(SnowflakesParams {
                count: 20,
                motion: SnowflakesMotion::Falling,
                ..SnowflakesParams::default()
            }),
            &[WHITE],
            seconds,
        )
    };
    // Each fault: the effect's name, what is shown, and the effect as it should be (`false`)
    // or with the fault (`true`).
    type Faulty = Box<dyn Fn(bool) -> Effect>;
    let spirals = |late_ms: u64| -> Faulty {
        Box::new(move |faulty| {
            let mut effect = effect(
                EffectParams::Spiral(SpiralParams::default()),
                &[RED, GREEN],
                seconds,
            );
            // The reference starts later, so PixelFlow's stripes are that far ahead from then
            // on.
            effect.start_ms = if faulty { 0 } else { late_ms };
            effect
        })
    };
    let chase = |speed: f32| -> Faulty {
        Box::new(move |faulty| {
            let params = ChaseParams {
                speed: if faulty { speed } else { 1.0 },
                ..ChaseParams::default()
            };
            effect(EffectParams::Chase(params), &[WHITE], seconds)
        })
    };
    // On for a quarter of a second in every half: a quarter of a second late it is on exactly
    // when it should be off.
    let shimmer = |late_ms: u64| -> Faulty {
        Box::new(move |faulty| {
            let params = ShimmerParams { rate: 2.0, duty: 0.5 };
            let mut effect = effect(EffectParams::Shimmer(params), &[WHITE], seconds);
            effect.start_ms = if faulty { late_ms } else { 0 };
            effect
        })
    };
    let faults: [(&'static str, &str, Faulty); 5] = [
        ("Spirals", "a quarter of a second early", spirals(250)),
        ("Chase", "a hundredth faster", chase(1.01)),
        ("Chase", "half as fast", chase(0.5)),
        ("Chase", "the other way", {
            Box::new(move |faulty| {
                let params = ChaseParams {
                    direction: if faulty {
                        Direction::Reverse
                    } else {
                        Direction::Forward
                    },
                    ..ChaseParams::default()
                };
                effect(EffectParams::Chase(params), &[WHITE], seconds)
            })
        }),
        ("Shimmer", "a quarter of a second late", shimmer(250)),
    ];
    let mut pairs = Vec::new();
    for (name, shown, make) in &faults {
        for (how, sparkles, under_snow) in [
            ("alone", 0, false),
            ("with sparkles", 60, false),
            ("under falling snow", 0, true),
        ] {
            let picture = |faulty: bool| {
                let mut effect = make(faulty);
                effect.sparkles = sparkles;
                let mut picture = vec![effect];
                if under_snow {
                    picture.push(snow());
                }
                picture
            };
            pairs.push(Pair {
                effect: name,
                shown: format!("{shown}, {how}"),
                theirs: picture(false),
                ours: picture(true),
            });
        }
    }
    pairs
}

/// Scores `pair` on `stage`, `pairs` times over, each from fresh seeds.
fn line(stage: &mut Stage, number: u64, pair: &Pair, settings: Settings) -> Line {
    let mut by_moment = Vec::new();
    let mut over = Vec::new();
    for k in 0..u64::from(settings.pairs) {
        let seeded = |picture: &[Effect], render: u64| -> Vec<Effect> {
            picture
                .iter()
                .enumerate()
                .map(|(layer, effect)| {
                    let mut effect = effect.clone();
                    effect.id = id(number, k, render * 8 + layer as u64);
                    effect
                })
                .collect()
        };
        let theirs = seeded(&pair.theirs, 0);
        let ours = seeded(&pair.ours, 1);
        let others: Vec<Vec<Effect>> = (0..u64::from(settings.seeds))
            .map(|n| seeded(&pair.ours, 2 + n))
            .collect();
        let (moment, held) = stage.look(&ours, &others, &theirs, settings.frame_ms);
        by_moment.push(moment);
        over.extend(held);
    }
    let seconds = pair.theirs.iter().map(|effect| effect.end_ms).max().unwrap_or(0) / 1000;
    Line {
        effect: pair.effect.to_string(),
        shown: pair.shown.clone(),
        on: stage.on,
        seconds: seconds as u32,
        chance: picture_chance(&pair.ours),
        by_moment: Tally::of(&by_moment),
        over: (!over.is_empty()).then(|| Tally::of(&over)),
    }
}

/// Renders and scores every pair. `progress` hears of each line as it starts.
pub fn run(settings: Settings, mut progress: impl FnMut(&str)) -> Calibration {
    let mut stages = [Stage::new(On::Matrix), Stage::new(On::Outlines)];
    let mut number = 0u64;
    let mut score = |stage: &mut Stage, pair: &Pair, settings: Settings| {
        number += 1;
        progress(&format!("{} ({}): {}", pair.effect, stage.on.title(), pair.shown));
        line(stage, number, pair, settings)
    };
    let (mut same, mut wrong, mut short, mut kept) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let wanted = |table: Table| settings.only.is_none_or(|only| only == table);
    for stage in &mut stages {
        if !wanted(Table::Same) {
            break;
        }
        for (name, effect) in kinds(settings.seconds, settings.frame_ms) {
            let pair = Pair {
                effect: name,
                shown: "the same".to_string(),
                theirs: vec![effect.clone()],
                ours: vec![effect],
            };
            same.push(score(stage, &pair, settings));
        }
    }
    for stage in &mut stages {
        if !wanted(Table::Timed) {
            break;
        }
        for pair in timed(settings.seconds) {
            kept.push(score(stage, &pair, settings));
        }
    }
    for stage in &mut stages {
        if !wanted(Table::Wrong) {
            break;
        }
        for pair in mistakes(settings.seconds, settings.frame_ms) {
            wrong.push(score(stage, &pair, settings));
        }
    }
    // Big bursts, one a second, for one second and up: one burst, two, four, and so on.
    for seconds in [1, 2, 4, 8, 15, 30, 60, 120] {
        if !wanted(Table::Short) {
            break;
        }
        for (shown, rate) in [("the same", 1.0), ("twice as many bursts", 2.0)] {
            let pair = Pair {
                effect: "Fireworks, big bursts",
                shown: shown.to_string(),
                theirs: vec![big_fireworks(seconds, |_| {})],
                ours: vec![big_fireworks(seconds, |p| p.rate = rate)],
            };
            short.push(score(&mut stages[0], &pair, settings));
        }
    }
    Calibration {
        settings,
        drawn_on: stages
            .iter()
            .map(|stage| Drawn {
                on: stage.on,
                pixels: stage.at.len(),
                columns: stage.cells.0,
                rows: stage.cells.1,
            })
            .collect(),
        same,
        wrong,
        short,
        timed: kept,
    }
}

fn counts(verdicts: &ByVerdict<usize>) -> String {
    format!(
        "{} / {} / {}",
        verdicts.get(Verdict::Matches),
        verdicts.get(Verdict::Close),
        verdicts.get(Verdict::Different)
    )
}

fn tally(tally: &Tally) -> String {
    format!(
        "{:.0} ({:.0}–{:.0}) | {}",
        tally.middle(),
        tally.least(),
        tally.most(),
        counts(&tally.verdicts)
    )
}

fn parts(tally: &Tally) -> String {
    Component::ALL
        .iter()
        .map(|&c| format!("{:.2}", tally.parts.get(c)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn table(out: &mut String, lines: &[Line], lengths: bool) {
    let _ = writeln!(
        out,
        "| Effect | Shown | On | {}Chance decides | By moment: look (least–most) | Matches / close / different | Next to other seeds: look (least–most) | Matches / close / different | Itself | Parts judged on |",
        if lengths { "Seconds | " } else { "" }
    );
    let _ = writeln!(
        out,
        "|---|---|---|{}---|---|---|---|---|---|---|",
        if lengths { "---|" } else { "" }
    );
    for line in lines {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {}{} | {} | {} | {} | {} |",
            line.effect,
            line.shown,
            line.on.title(),
            if lengths {
                format!("{} | ", line.seconds)
            } else {
                String::new()
            },
            line.chance.title(),
            tally(&line.by_moment),
            line.over.as_ref().map_or_else(|| "– | –".to_string(), tally),
            line.over
                .as_ref()
                .and_then(|t| t.itself)
                .map_or_else(|| "–".to_string(), |v| format!("{v:.2}")),
            parts(line.judged()),
        );
    }
}

/// The three tables, for reading.
pub fn markdown(calibration: &Calibration) -> String {
    let mut out = String::new();
    let o = &mut out;
    let settings = calibration.settings;
    let _ = writeln!(o, "# Look score calibration\n");
    let _ = writeln!(
        o,
        "PixelFlow's own renders held against each other: effects {} s long in {} ms frames, each \
         line scored {} times from fresh seeds. \"By moment\" compares every part of the look \
         moment by moment. \"Next to other seeds\" is what an effect that draws on chance is judged \
         on: the same held against {} more of PixelFlow's own renders of it from other seeds, and \
         read over the whole effect when chance decides its moments. A look is the middle score, \
         with the least and the most. \"Itself\" is how alike PixelFlow's own renders are from one \
         seed to the next in the part chance moves most: under {:.1}, chance alone moves the effect \
         by a factor of two and the score says little. The parts are coverage, brightness, color, \
         pattern, motion, and rhythm, averaged.\n",
        settings.seconds,
        settings.frame_ms,
        settings.pairs,
        settings.seeds,
        crate::look::LITTLE
    );
    for drawn in &calibration.drawn_on {
        let _ = writeln!(
            o,
            "- The {}: {} pixels on a grid of {} by {} cells.",
            drawn.on.title(),
            drawn.pixels,
            drawn.columns,
            drawn.rows
        );
    }
    if !calibration.same.is_empty() {
        let _ = writeln!(o, "\n## The same effect from other seeds\n");
        let _ = writeln!(o, "Every line should match.\n");
        table(o, &calibration.same, false);
    }
    if !calibration.wrong.is_empty() {
        let _ = writeln!(o, "\n## One thing wrong\n");
        let _ = writeln!(o, "No line should match.\n");
        table(o, &calibration.wrong, false);
    }
    if !calibration.short.is_empty() {
        let _ = writeln!(o, "\n## Short effects and few events\n");
        let _ = writeln!(
            o,
            "Fireworks with a big burst a second, from one second and one burst to two minutes.\n"
        );
        table(o, &calibration.short, true);
    }
    if !calibration.timed.is_empty() {
        let _ = writeln!(o, "\n## Effects that keep time\n");
        let _ = writeln!(
            o,
            "Effects that keep time, with their timing wrong: alone (judged as ever, moment by \
             moment), with sparkles on them, and under falling snow (both held against other \
             seeds). What a fault costs alone, it should cost the other two. Where it costs \
             nothing alone, the look score never saw it.\n"
        );
        table(o, &calibration.timed, false);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_are_the_same_every_run_and_differ_from_one_another() {
        assert_eq!(id(3, 1, 0), id(3, 1, 0));
        let seeds = [id(3, 1, 0), id(3, 1, 1), id(3, 2, 0), id(4, 1, 0)].map(EffectId::seed);
        for (i, a) in seeds.iter().enumerate() {
            assert_ne!(*a, 0);
            for b in &seeds[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn a_tally_reads_its_scores() {
        let look = |score: f64| {
            let mut stats = LookStats::new(Rc::new(Grid::new(&[(0.0, 0.0)], 1, 1)), 25, &Limits::default());
            let mut scratch = Scratch::default();
            stats.add(&mut scratch, 0, 0, [[255; 3]; 2]);
            stats.end_frame(&mut scratch, 0);
            let mut look = stats.finish().unwrap();
            look.score = score;
            look.verdict = crate::look::verdict(score);
            look
        };
        let tally = Tally::of(&[look(90.0), look(60.0), look(30.0), look(100.0)]);
        assert_eq!((tally.least(), tally.most(), tally.middle()), (30.0, 100.0, 75.0));
        assert_eq!(
            (
                tally.verdicts.matches,
                tally.verdicts.close,
                tally.verdicts.different
            ),
            (2, 1, 1)
        );
        assert_eq!(tally.itself, None);
    }
}
