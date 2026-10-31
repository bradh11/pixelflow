//! The written guides, compiled into the app from `crates/pf-ai/guides`.

use pf_sequence::EffectKind;

/// The topics, in the order the index lists them: key, and the guide as Markdown. Show design
/// first (how a designer works), then what every effect shares.
const TOPICS: &[(&str, &str)] = &[
    ("method", include_str!("../../guides/topics/method.md")),
    ("song", include_str!("../../guides/topics/song.md")),
    ("roles", include_str!("../../guides/topics/roles.md")),
    ("choosing", include_str!("../../guides/topics/choosing.md")),
    ("range", include_str!("../../guides/topics/range.md")),
    ("layering", include_str!("../../guides/topics/layering.md")),
    ("palettes", include_str!("../../guides/topics/palettes.md")),
    ("motion", include_str!("../../guides/topics/motion.md")),
    ("intensity", include_str!("../../guides/topics/intensity.md")),
    ("vocals", include_str!("../../guides/topics/vocals.md")),
    (
        "openings-endings",
        include_str!("../../guides/topics/openings-endings.md"),
    ),
    ("mistakes", include_str!("../../guides/topics/mistakes.md")),
    ("safety", include_str!("../../guides/topics/safety.md")),
    (
        "music-reactive",
        include_str!("../../guides/topics/music-reactive.md"),
    ),
    ("editing", include_str!("../../guides/topics/editing.md")),
    ("effect-wide", include_str!("../../guides/topics/effect-wide.md")),
    ("blends", include_str!("../../guides/topics/blends.md")),
    ("curves", include_str!("../../guides/topics/curves.md")),
    ("layouts", include_str!("../../guides/topics/layouts.md")),
    ("timing", include_str!("../../guides/topics/timing.md")),
    ("cues", include_str!("../../guides/topics/cues.md")),
];

/// The written guide for `kind`, as Markdown.
///
/// **Adding an effect kind? This match is where the build stops until the Assistant has been
/// taught about it.** To get past it:
///
/// 1. Create `crates/pf-ai/guides/effects/<kind>.md` (named for the kind's key, like
///    `colorWash.md`) with a first line of `# <its label>`, and add its line below. The crate
///    builds again, and `cargo test -p pf-ai --test knowledge` now fails with everything the
///    guide has to say, spelled out for this effect (its settings and choices by name).
/// 2. Draw the effect on every kind of prop and look at it:
///    `cargo run -p pf-ai --example guide_sheets -- <a scratch folder> --kinds <kind> --auto`
/// 3. Write the guide from what you saw: what it draws on a string, an arch, a matrix, a tree,
///    and the whole house; what each of its settings and choices is for; when to use it; and at
///    least two named looks. `docs/dev/assistant-guides.md` has the format and the rules.
/// 4. `cargo run -p pf-ai --example guide_sheets -- --check --kinds <kind> --guides crates/pf-ai/guides`
///    says what the guide still lacks, as the tests will.
pub fn effect_source(kind: EffectKind) -> &'static str {
    match kind {
        EffectKind::On => include_str!("../../guides/effects/on.md"),
        EffectKind::Off => include_str!("../../guides/effects/off.md"),
        EffectKind::ColorWash => include_str!("../../guides/effects/colorWash.md"),
        EffectKind::Fade => include_str!("../../guides/effects/fade.md"),
        EffectKind::Chase => include_str!("../../guides/effects/chase.md"),
        EffectKind::Bars => include_str!("../../guides/effects/bars.md"),
        EffectKind::Wave => include_str!("../../guides/effects/wave.md"),
        EffectKind::Twinkle => include_str!("../../guides/effects/twinkle.md"),
        EffectKind::Shimmer => include_str!("../../guides/effects/shimmer.md"),
        EffectKind::Strobe => include_str!("../../guides/effects/strobe.md"),
        EffectKind::Spiral => include_str!("../../guides/effects/spiral.md"),
        EffectKind::Fire => include_str!("../../guides/effects/fire.md"),
        EffectKind::Meteors => include_str!("../../guides/effects/meteors.md"),
        EffectKind::Ripple => include_str!("../../guides/effects/ripple.md"),
        EffectKind::Shape => include_str!("../../guides/effects/shape.md"),
        EffectKind::Fan => include_str!("../../guides/effects/fan.md"),
        EffectKind::Morph => include_str!("../../guides/effects/morph.md"),
        EffectKind::Circles => include_str!("../../guides/effects/circles.md"),
        EffectKind::Pinwheel => include_str!("../../guides/effects/pinwheel.md"),
        EffectKind::Snowflakes => include_str!("../../guides/effects/snowflakes.md"),
        EffectKind::Plasma => include_str!("../../guides/effects/plasma.md"),
        EffectKind::Butterfly => include_str!("../../guides/effects/butterfly.md"),
        EffectKind::Garlands => include_str!("../../guides/effects/garlands.md"),
        EffectKind::Lines => include_str!("../../guides/effects/lines.md"),
        EffectKind::Life => include_str!("../../guides/effects/life.md"),
        EffectKind::Tendril => include_str!("../../guides/effects/tendril.md"),
        EffectKind::Text => include_str!("../../guides/effects/text.md"),
        EffectKind::Faces => include_str!("../../guides/effects/faces.md"),
        EffectKind::VuMeter => include_str!("../../guides/effects/vuMeter.md"),
        EffectKind::Impact => include_str!("../../guides/effects/impact.md"),
        EffectKind::Wipe => include_str!("../../guides/effects/wipe.md"),
        EffectKind::Lightning => include_str!("../../guides/effects/lightning.md"),
        EffectKind::Pulse => include_str!("../../guides/effects/pulse.md"),
        EffectKind::Sing => include_str!("../../guides/effects/sing.md"),
        EffectKind::ColorShift => include_str!("../../guides/effects/colorShift.md"),
        EffectKind::Dancer => include_str!("../../guides/effects/dancer.md"),
        EffectKind::Picture => include_str!("../../guides/effects/picture.md"),
        EffectKind::Shockwave => include_str!("../../guides/effects/shockwave.md"),
        EffectKind::Fill => include_str!("../../guides/effects/fill.md"),
        EffectKind::Fireworks => include_str!("../../guides/effects/fireworks.md"),
        EffectKind::Shader => include_str!("../../guides/effects/shader.md"),
        EffectKind::Dmx => include_str!("../../guides/effects/dmx.md"),
        EffectKind::MovingHead => include_str!("../../guides/effects/movingHead.md"),
        EffectKind::Servo => include_str!("../../guides/effects/servo.md"),
    }
}

/// Every topic's key, in order.
pub fn topic_keys() -> Vec<&'static str> {
    TOPICS.iter().map(|(key, _)| *key).collect()
}

/// The topic `key`, as Markdown.
pub fn topic_source(key: &str) -> Option<&'static str> {
    TOPICS.iter().find(|(k, _)| *k == key).map(|(_, source)| *source)
}
