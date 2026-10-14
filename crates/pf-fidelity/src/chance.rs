//! What chance decides in an effect.
//!
//! Some effects draw from their seed where their light falls, and some when it comes. Two
//! renders of such an effect never agree moment for moment, in PixelFlow or in xLights, and
//! nobody watching can tell them apart: fireworks go off as often, as big, and in the same
//! colors, at other moments and in other places. Holding them against each other second by
//! second calls that a difference.
//!
//! So the look score does two things for them ([`crate::look`]), and [`chance`] says for which:
//!
//! - **Anything an effect draws from its seed** ([`Chance::Drawn`] and [`Chance::Moments`]) is
//!   allowed for by rendering the sequence again from other seeds. What PixelFlow's own renders
//!   disagree on is chance, and isn't held against the import. An effect whose renders agree
//!   from seed to seed is allowed nothing.
//! - **An effect whose moments are chance** ([`Chance::Moments`]) is read over the whole effect
//!   as well: how much of the time how much is lit, all its colors at once, where its light
//!   fell, and never when.
//!
//! The second is the one that could hide a fault, and it is declared here, from the effect's
//! kind and its settings, never read off the frames: an effect that ought to keep time and
//! doesn't would look just like one that chance moves. An effect set off by its own clock, a
//! timing track, or the music keeps its moments whatever else it draws by chance, and coming in
//! late still shows. A kind added to PixelFlow has to be given a line below before this crate
//! builds.

use pf_sequence::{
    CirclesLook, Effect, EffectId, EffectParams, FaceEyes, FireStyle, FireworksColors, FireworksParams,
    ShapeObject, ShapeParams, TendrilMovement, VuMeterType,
};
use serde::Serialize;

/// What chance decides in an effect, least first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Chance {
    /// Nothing: every render of it is the same. It is compared moment by moment with xLights'
    /// render alone.
    #[default]
    Nothing,
    /// What it draws: which pixels, where, in which of its colors. Something else says when:
    /// its own clock, a timing track, or the music. It is compared moment by moment, next to
    /// PixelFlow's own renders from other seeds.
    Drawn,
    /// When its light comes, as well: nothing in it happens at a set moment. It is compared
    /// over the whole effect, next to PixelFlow's own renders from other seeds.
    Moments,
}

impl Chance {
    /// In a few words, for a table.
    pub fn title(self) -> &'static str {
        match self {
            Chance::Nothing => "nothing",
            Chance::Drawn => "what it draws",
            Chance::Moments => "its moments",
        }
    }
}

/// What chance decides in an effect, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Declared {
    pub chance: Chance,
    /// What it is about the effect that chance decides; `None` when nothing is.
    pub why: Option<&'static str>,
}

const fn moments(why: &'static str) -> Declared {
    Declared {
        chance: Chance::Moments,
        why: Some(why),
    }
}

const fn drawn(why: &'static str) -> Declared {
    Declared {
        chance: Chance::Drawn,
        why: Some(why),
    }
}

const NOTHING: Declared = Declared {
    chance: Chance::Nothing,
    why: None,
};

/// What chance decides in `effect`, from its kind and settings.
pub fn chance(effect: &Effect) -> Declared {
    let own = of_kind(&effect.params);
    // Sparkles flash each lit pixel at a moment of its own, on whatever the effect is.
    if own.chance == Chance::Nothing && (effect.sparkles > 0 || effect.curves.contains_key("sparkles")) {
        return drawn("its sparkles flash on chance pixels");
    }
    own
}

/// What chance decides in an effect of these settings, its sparkles aside.
fn of_kind(params: &EffectParams) -> Declared {
    match params {
        EffectParams::Fireworks(p) => fireworks(p),
        EffectParams::Shape(p) => shape(p),
        // Balls set off from chance places in chance directions, and nothing brings them back
        // at a set moment. The rings spread from their center by the clock.
        EffectParams::Circles(p) => match p.look {
            CirclesLook::Solid | CirclesLook::Fading | CirclesLook::Bubbles | CirclesLook::Plasma => {
                moments("its balls start in chance places and head off in chance directions")
            }
            CirclesLook::Radial | CirclesLook::RainbowRadial => NOTHING,
        },
        EffectParams::Meteors(_) => {
            moments("each meteor falls in a chance lane, starting at a chance moment")
        }
        EffectParams::Snowflakes(_) => moments("its flakes are scattered by chance"),
        EffectParams::Lines(_) => {
            moments("its lines start in chance places and head off in chance directions")
        }
        EffectParams::Lightning(_) => moments("it strikes at chance moments"),
        EffectParams::Tendril(p) => match p.movement {
            TendrilMovement::Random => moments("the point it follows wanders by chance"),
            // A path worked out from the clock, the music, or a point held still.
            TendrilMovement::Square
            | TendrilMovement::Circle
            | TendrilMovement::HorizontalZigZag
            | TendrilMovement::HorizontalZigZagReturn
            | TendrilMovement::VerticalZigZag
            | TendrilMovement::VerticalZigZagReturn
            | TendrilMovement::MusicLine
            | TendrilMovement::MusicCircle
            | TendrilMovement::Manual => NOTHING,
        },
        // A new generation comes by the clock; which cells it starts from is chance.
        EffectParams::Life(_) => drawn("its first cells are scattered by chance"),
        // The bar lights with the music or on a mark; which bar is chance.
        EffectParams::VuMeter(p) => match p.meter {
            VuMeterType::LevelRandomBar
            | VuMeterType::NoteLevelRandomBar
            | VuMeterType::TimingEventRandomBar => drawn("the bar that lights is picked by chance"),
            _ => NOTHING,
        },
        // The fire burns where it is set to, at the height it is set to (or the music's): its
        // flames are chance, and the flicker they give the whole prop.
        EffectParams::Fire(p) => match p.style {
            FireStyle::Flicker => drawn("its flames are drawn anew by chance every frame"),
            FireStyle::Rolling => drawn("its flames flare up by chance"),
        },
        // Each pixel keeps its own time, and over a prop they add up to the same light at
        // every moment: nothing a viewer sees happens at a chance moment.
        EffectParams::Twinkle(_) => drawn("each pixel twinkles from a chance moment of its own"),
        // The flashes come by the clock.
        EffectParams::Strobe(_) => drawn("the pixels of each flash are picked by chance"),
        // A face sings from its timing track; its blinks are a frame or two of its eyes.
        EffectParams::Faces(p) => match p.eyes {
            FaceEyes::Auto => drawn("its eyes blink at chance moments"),
            FaceEyes::Open | FaceEyes::Closed => NOTHING,
        },
        // Worked out from the clock, the settings, a timing track, the music, or a file.
        EffectParams::On(_)
        | EffectParams::Off(_)
        | EffectParams::ColorWash(_)
        | EffectParams::Fade(_)
        | EffectParams::Chase(_)
        | EffectParams::Bars(_)
        | EffectParams::Wave(_)
        | EffectParams::Shimmer(_)
        | EffectParams::Spiral(_)
        | EffectParams::Ripple(_)
        | EffectParams::Fan(_)
        | EffectParams::Morph(_)
        | EffectParams::Pinwheel(_)
        | EffectParams::Plasma(_)
        | EffectParams::Butterfly(_)
        | EffectParams::Garlands(_)
        | EffectParams::Text(_)
        | EffectParams::Impact(_)
        | EffectParams::Wipe(_)
        | EffectParams::Pulse(_)
        | EffectParams::Sing(_)
        | EffectParams::ColorShift(_)
        | EffectParams::Dancer(_)
        | EffectParams::Picture(_)
        | EffectParams::Shockwave(_)
        | EffectParams::Fill(_)
        // A shader is a function of the time and its inputs: its date is fixed and nothing is drawn
        // at random.
        | EffectParams::Shader(_)
        | EffectParams::Dmx(_) => NOTHING,
    }
}

/// Bursts at chance moments, unless a timing track or the music sets them off; then chance is
/// left with where each goes off, which palette color it takes, and how its sparks fly.
fn fireworks(p: &FireworksParams) -> Declared {
    if p.timing_track.is_none() && !p.fire_on_music {
        moments("its bursts go off at chance moments")
    } else if p.random_location {
        drawn("each burst goes off in a chance place")
    } else if p.colors == FireworksColors::Burst {
        drawn("each burst takes a chance color")
    } else {
        drawn("each burst's sparks fly off by chance")
    }
}

/// `count` shapes, each replaced as it goes. Staggered, each keeps its own time from a chance
/// start. Otherwise they all appear together by the clock, or one appears at each mark or with
/// the music, and chance is left with where each appears, how it drifts, and what it is.
fn shape(p: &ShapeParams) -> Declared {
    let set_off = p.timing_track.is_some() || p.fire_on_music;
    if !set_off && p.random_start {
        moments("its shapes appear at chance moments")
    } else if p.random_location {
        drawn("each shape appears in a chance place")
    } else if p.random_movement {
        drawn("each shape drifts off in a chance direction")
    } else if p.shape == ShapeObject::Random {
        drawn("each shape is picked by chance")
    } else {
        NOTHING
    }
}

/// An effect id from two numbers.
pub(crate) fn id_of(high: u64, low: u64) -> EffectId {
    let text = format!(
        "\"{:08x}-{:04x}-{:04x}-{:04x}-{:012x}\"",
        high >> 32,
        (high >> 16) & 0xFFFF,
        high & 0xFFFF,
        low >> 48,
        low & 0xFFFF_FFFF_FFFF
    );
    serde_json::from_str(&text).unwrap_or_default()
}

/// A well-mixed number from another (the same on every platform).
pub(crate) fn mixed(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// The id an effect is given for PixelFlow's `draw`th other render of its sequence: an effect
/// draws its chance from its id ([`EffectId::seed`]), so another id is another draw. The same
/// effect and draw give the same id every time.
pub fn reseeded(id: EffectId, draw: u64) -> EffectId {
    let seed = id.seed();
    id_of(
        mixed(seed ^ draw.wrapping_mul(0xC2B2_AE3D_27D4_EB4F)),
        mixed(seed.rotate_left(32) ^ draw),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_sequence::{Curve, EffectKind, TimingTrackId};

    fn of(params: EffectParams) -> Chance {
        chance(&Effect::new(params.kind(), 0, 1000).with_params(params)).chance
    }

    #[test]
    fn every_kind_is_declared_and_says_why_when_chance_has_a_hand_in_it() {
        for kind in EffectKind::ALL {
            let declared = chance(&Effect::new(kind, 0, 1000));
            assert_eq!(
                declared.why.is_some(),
                declared.chance != Chance::Nothing,
                "{kind:?}"
            );
        }
        // The kinds whose own settings leave their moments to chance, and those that draw on
        // chance while something else sets their moments.
        let with = |chance_of: Chance| -> Vec<EffectKind> {
            EffectKind::ALL
                .into_iter()
                .filter(|&kind| chance(&Effect::new(kind, 0, 1000)).chance == chance_of)
                .collect()
        };
        assert_eq!(
            with(Chance::Moments),
            [
                EffectKind::Meteors,
                EffectKind::Shape,
                EffectKind::Circles,
                EffectKind::Snowflakes,
                EffectKind::Lines,
                EffectKind::Lightning,
                EffectKind::Fireworks,
            ]
        );
        assert_eq!(
            with(Chance::Drawn),
            [
                EffectKind::Twinkle,
                EffectKind::Strobe,
                EffectKind::Fire,
                EffectKind::Life,
                EffectKind::Faces,
            ]
        );
    }

    #[test]
    fn sparkles_are_chance_on_whatever_they_are_on() {
        let mut on = Effect::new(EffectKind::On, 0, 1000);
        assert_eq!(chance(&on).chance, Chance::Nothing);
        on.sparkles = 40;
        assert_eq!(chance(&on).chance, Chance::Drawn);
        let mut curved = Effect::new(EffectKind::Bars, 0, 1000);
        curved.curves.insert("sparkles".into(), Curve::ramp(0.0, 100.0));
        assert_eq!(chance(&curved).chance, Chance::Drawn);
        // They take nothing from an effect whose moments are chance already.
        let mut snow = Effect::new(EffectKind::Snowflakes, 0, 1000);
        snow.sparkles = 40;
        assert_eq!(chance(&snow).chance, Chance::Moments);
    }

    #[test]
    fn fireworks_set_off_by_a_track_or_the_music_keep_their_moments() {
        let fireworks = |change: &dyn Fn(&mut FireworksParams)| {
            let mut p = FireworksParams::default();
            change(&mut p);
            of(EffectParams::Fireworks(p))
        };
        assert_eq!(fireworks(&|_| {}), Chance::Moments);
        // In one place, still at chance moments.
        assert_eq!(fireworks(&|p| p.random_location = false), Chance::Moments);
        assert_eq!(
            fireworks(&|p| p.timing_track = Some(TimingTrackId::new())),
            Chance::Drawn
        );
        assert_eq!(fireworks(&|p| p.fire_on_music = true), Chance::Drawn);
        assert_eq!(
            fireworks(&|p| {
                p.fire_on_music = true;
                p.random_location = false;
                p.colors = FireworksColors::Fade;
            }),
            Chance::Drawn
        );
    }

    #[test]
    fn shapes_keep_their_moments_unless_their_start_is_staggered() {
        let shape = |change: &dyn Fn(&mut ShapeParams)| {
            let mut p = ShapeParams::default();
            change(&mut p);
            of(EffectParams::Shape(p))
        };
        assert_eq!(shape(&|_| {}), Chance::Moments);
        assert_eq!(shape(&|p| p.random_start = false), Chance::Drawn);
        assert_eq!(
            shape(&|p| p.timing_track = Some(TimingTrackId::new())),
            Chance::Drawn
        );
        assert_eq!(shape(&|p| p.fire_on_music = true), Chance::Drawn);
        // All together, in one place, drifting one way, all the same shape: nothing is chance.
        let fixed = |p: &mut ShapeParams| {
            p.random_start = false;
            p.random_location = false;
        };
        assert_eq!(shape(&fixed), Chance::Nothing);
        assert_eq!(
            shape(&|p| {
                fixed(p);
                p.random_movement = true;
            }),
            Chance::Drawn
        );
        assert_eq!(
            shape(&|p| {
                fixed(p);
                p.shape = ShapeObject::Random;
            }),
            Chance::Drawn
        );
    }

    #[test]
    fn another_draw_is_another_seed_and_the_same_one_every_time() {
        let id = EffectId::new();
        assert_eq!(reseeded(id, 1), reseeded(id, 1));
        let seeds = [id, reseeded(id, 1), reseeded(id, 2), reseeded(EffectId::new(), 1)].map(EffectId::seed);
        for (i, a) in seeds.iter().enumerate() {
            for b in &seeds[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(id_of(1, 2).to_string(), "00000000-0000-0001-0000-000000000002");
    }

    #[test]
    fn a_path_or_a_ring_worked_out_from_the_clock_is_not_chance() {
        let tendril = |movement| {
            of(EffectParams::Tendril(pf_sequence::TendrilParams {
                movement,
                ..Default::default()
            }))
        };
        assert_eq!(tendril(TendrilMovement::Random), Chance::Moments);
        assert_eq!(tendril(TendrilMovement::Circle), Chance::Nothing);
        assert_eq!(tendril(TendrilMovement::MusicLine), Chance::Nothing);
        let circles = |look| {
            of(EffectParams::Circles(pf_sequence::CirclesParams {
                look,
                ..Default::default()
            }))
        };
        assert_eq!(circles(CirclesLook::Bubbles), Chance::Moments);
        assert_eq!(circles(CirclesLook::Radial), Chance::Nothing);
        let meter = |meter| {
            of(EffectParams::VuMeter(pf_sequence::VuMeterParams {
                meter,
                ..Default::default()
            }))
        };
        assert_eq!(meter(VuMeterType::LevelRandomBar), Chance::Drawn);
        assert_eq!(meter(VuMeterType::LevelBar), Chance::Nothing);
        let face = |eyes| {
            of(EffectParams::Faces(pf_sequence::FacesParams {
                eyes,
                ..Default::default()
            }))
        };
        assert_eq!(face(FaceEyes::Auto), Chance::Drawn);
        assert_eq!(face(FaceEyes::Open), Chance::Nothing);
    }
}
