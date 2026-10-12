//! The written guides, compiled into the app from `crates/pf-ai/guides`.

use pf_sequence::EffectKind;

/// The written guide for each effect kind that has one, as Markdown.
const EFFECTS: &[(EffectKind, &str)] = &[];

/// The topics, in the order the index lists them: key, and the guide as Markdown.
const TOPICS: &[(&str, &str)] = &[
    ("effect-wide", include_str!("../../guides/topics/effect-wide.md")),
    ("blends", include_str!("../../guides/topics/blends.md")),
    ("curves", include_str!("../../guides/topics/curves.md")),
    ("layouts", include_str!("../../guides/topics/layouts.md")),
    ("timing", include_str!("../../guides/topics/timing.md")),
    ("cues", include_str!("../../guides/topics/cues.md")),
];

/// The written guide for `kind`, as Markdown.
pub fn effect_source(kind: EffectKind) -> Option<&'static str> {
    EFFECTS
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, source)| *source)
}

/// Every topic's key, in order.
pub fn topic_keys() -> Vec<&'static str> {
    TOPICS.iter().map(|(key, _)| *key).collect()
}

/// The topic `key`, as Markdown.
pub fn topic_source(key: &str) -> Option<&'static str> {
    TOPICS.iter().find(|(k, _)| *k == key).map(|(_, source)| *source)
}
