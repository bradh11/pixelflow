//! What the Assistant knows about making light shows, fetched when it needs it (the `get_guide`
//! tool) rather than carried in every request.
//!
//! - **Reference** ([`reference`]): every effect kind, setting, choice, blend, curve shape, render
//!   style, cue, and timing track, with ranges and defaults. Generated from the tables the app runs
//!   on, never typed out.
//! - **Craft** (`guides/effects/*.md`): for each effect kind, what PixelFlow draws with it on a
//!   string, an arch, a matrix, a tree, and the whole house, which settings matter, what it pairs
//!   with, what to avoid, and named ready-made looks. Written from frames drawn by the engine.
//! - **Show design** (`guides/topics/*.md`): reading a song, giving props roles, layering,
//!   palettes, motion, vocals, beginnings and endings, mistakes, safety; and the parts of the
//!   editor every effect shares (blends, curves, layouts, timing tracks, cues).
//!
//! The guides are Markdown kept in the repository and compiled into the app ([`library`]). A
//! guide's names in backticks are PixelFlow's own names (settings, choices, tools), and its looks
//! are effects as `place_effects` takes them: tests check every one against the real thing.

#[cfg(any(test, feature = "test-fixtures"))]
pub mod check;
pub mod guide;
pub mod library;
pub mod reference;

pub use guide::{Guide, Look};

use pf_sequence::EffectKind;
use serde_json::Value;

/// The kinds of thing an effect is drawn on, as the guides speak of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Suit {
    /// One line of pixels: a roofline, an outline, a strip.
    String,
    Arch,
    Matrix,
    Tree,
    /// Every prop at once, as one group.
    House,
}

impl Suit {
    pub const ALL: [Suit; 5] = [Suit::String, Suit::Arch, Suit::Matrix, Suit::Tree, Suit::House];

    /// The word a guide's look uses in `suits`.
    pub fn word(self) -> &'static str {
        match self {
            Suit::String => "string",
            Suit::Arch => "arch",
            Suit::Matrix => "matrix",
            Suit::Tree => "tree",
            Suit::House => "house",
        }
    }

    pub fn words() -> Vec<&'static str> {
        Self::ALL.iter().map(|s| s.word()).collect()
    }

    pub fn parse(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.word() == word)
    }
}

/// The longest a guide may be as the Assistant reads it (bytes), reference and all: several fit
/// in one answer.
pub const MAX_GUIDE_BYTES: usize = 12_000;
/// Guides one `get_guide` call may ask for.
pub const MAX_GUIDES_PER_CALL: usize = 8;
/// How much one `get_guide` answer may hold (under the limit on every tool answer).
const ANSWER_BYTES: usize = crate::run::MAX_RESULT_CHARS - 2_000;

/// The written guide for `kind`, read. (`None` only for a guide that can't be read, which the
/// tests don't let through: the reference then stands alone.)
pub fn effect_guide(kind: EffectKind) -> Option<Guide> {
    guide::parse(library::effect_source(kind)).ok()
}

/// The guide for `kind` as the Assistant reads it: the written guide, then the reference.
pub fn effect_text(kind: EffectKind) -> String {
    let reference = reference::effect(kind);
    match effect_guide(kind) {
        Some(guide) => format!("{}\n{reference}", guide.text.trim_end()),
        None => format!("# {}\n\n{reference}", reference::info(kind).label),
    }
}

/// The topic `key`, read, with its generated lists in place.
pub fn topic_guide(key: &str) -> Option<Guide> {
    guide::parse(&fill(library::topic_source(key)?)).ok()
}

/// The topic `key` as the Assistant reads it.
pub fn topic_text(key: &str) -> Option<String> {
    topic_guide(key).map(|g| g.text.trim_end().to_string())
}

/// The lists a topic can ask for by `{{name}}` on a line of its own, each generated.
pub const GENERATED: [&str; 13] = [
    "blends",
    "row-blending",
    "curve-shapes",
    "curve-fields",
    "render-styles",
    "buffer-transforms",
    "group-layouts",
    "timing-kinds",
    "song-tracks",
    "cues",
    "effect-wide",
    "effects",
    "looks-by-target",
];

fn generated(name: &str) -> Option<String> {
    Some(match name {
        "blends" => reference::blends(),
        "row-blending" => reference::row_blending(),
        "curve-shapes" => reference::curve_shapes(),
        "curve-fields" => reference::curve_fields(),
        "render-styles" => reference::render_styles(),
        "buffer-transforms" => reference::buffer_transforms(),
        "group-layouts" => reference::group_layouts(),
        "timing-kinds" => reference::timing_kinds(),
        "song-tracks" => reference::song_tracks(),
        "cues" => reference::cues(),
        "effect-wide" => reference::effect_wide(),
        "effects" => effect_index(),
        "looks-by-target" => looks_by_target(),
        _ => return None,
    })
}

/// `source` with each `{{name}}` replaced by its generated list.
pub(crate) fn fill(source: &str) -> String {
    let mut out = source.to_string();
    for name in GENERATED {
        let mark = format!("{{{{{name}}}}}");
        if out.contains(&mark)
            && let Some(list) = generated(name)
        {
            out = out.replace(&mark, list.trim_end());
        }
    }
    out
}

/// Every effect kind in a line: its key and what it's for.
fn effect_index() -> String {
    EffectKind::ALL
        .iter()
        .map(|&kind| {
            let summary = effect_guide(kind)
                .map_or_else(|| reference::info(kind).description.to_string(), |g| g.summary);
            format!("- `{}`: {summary}\n", reference::key_of(&kind))
        })
        .collect()
}

/// For each kind of target, the effects with a look written for it.
fn looks_by_target() -> String {
    Suit::ALL
        .iter()
        .map(|&suit| {
            let kinds: Vec<String> = EffectKind::ALL
                .iter()
                .filter(|&&kind| {
                    effect_guide(kind).is_some_and(|g| g.looks.iter().any(|l| l.suits.contains(&suit)))
                })
                .map(reference::key_of)
                .collect();
            format!("- {}: {}\n", suit.word(), kinds.join(", "))
        })
        .collect()
}

/// What `get_guide` answers without being asked for anything: every guide there is, in a line
/// each.
pub fn index() -> String {
    let mut out = format!(
        "PixelFlow's guides. Ask for up to {MAX_GUIDES_PER_CALL} at once, e.g. get_guide({{\"effects\": [\"bars\", \"spiral\"], \"topics\": [\"blends\"]}}).\n\nTopics:\n"
    );
    for key in library::topic_keys() {
        let summary = topic_guide(key).map(|g| g.summary).unwrap_or_default();
        out.push_str(&format!("- `{key}`: {summary}\n"));
    }
    out.push_str("\nEffects (each guide: what it draws on each kind of prop, the settings that matter, ready-made looks, then every setting):\n");
    out.push_str(&effect_index());
    out
}

fn asked(input: &Value, field: &str) -> Result<Vec<String>, String> {
    match &input[field] {
        Value::Null => Ok(Vec::new()),
        Value::String(one) => Ok(vec![one.clone()]),
        Value::Array(items) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or(format!("{field} must be a list of names."))
            })
            .collect(),
        _ => Err(format!("{field} must be a list of names.")),
    }
}

/// `get_guide`: the guides asked for (effects by kind, topics by name), or the index of them all.
/// A name that isn't a guide is refused with the names that are.
pub fn answer(input: &Value) -> Result<String, String> {
    answer_once(input, &mut GuidesRead::default())
}

/// The guides already read in a chat, by key ("topic:method", "effect:bars"): each is in the
/// conversation, so reading it again only says so.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuidesRead(std::collections::BTreeSet<String>);

impl GuidesRead {
    pub fn contains(&self, key: &str) -> bool {
        self.0.contains(key)
    }
}

/// [`answer`] in a chat that has read the guides in `read` already: those are named, not sent
/// again, and the ones sent now are added to `read`. More than [`MAX_GUIDES_PER_CALL`] at once
/// sends the first that many and names the rest to ask for next.
pub fn answer_once(input: &Value, read: &mut GuidesRead) -> Result<String, String> {
    if let Some(unknown) = input
        .as_object()
        .and_then(|o| o.keys().find(|k| !["effects", "topics"].contains(&k.as_str())))
    {
        return Err(format!(
            "get_guide has no {unknown}. It takes effects and topics."
        ));
    }
    let effects = asked(input, "effects")?;
    let topics = asked(input, "topics")?;
    if effects.is_empty() && topics.is_empty() {
        return Ok(index());
    }
    // Every name is checked first, so a wrong one is refused before anything is sent.
    let mut guides: Vec<(String, String)> = Vec::new();
    for key in &topics {
        if !library::topic_keys().contains(&key.as_str()) {
            return Err(format!(
                "There's no topic \"{key}\". Topics: {}.",
                library::topic_keys().join(", ")
            ));
        }
        guides.push((key.clone(), format!("topic:{key}")));
    }
    for key in &effects {
        if reference::kind_named(key).is_none() {
            return Err(format!(
                "There's no effect \"{key}\". Effects: {}.",
                reference::kind_keys().join(", ")
            ));
        }
        guides.push((key.clone(), format!("effect:{key}")));
    }
    let mut seen = std::collections::BTreeSet::new();
    guides.retain(|(_, id)| seen.insert(id.clone()));
    let (again, new): (Vec<_>, Vec<_>) = guides.into_iter().partition(|(_, id)| read.contains(id));
    let mut out = String::new();
    let mut left_out: Vec<String> = Vec::new();
    for (i, (key, id)) in new.into_iter().enumerate() {
        let text = match id.split_once(':') {
            Some(("topic", _)) => topic_text(&key),
            _ => reference::kind_named(&key).map(effect_text),
        }
        .unwrap_or_default();
        if i >= MAX_GUIDES_PER_CALL || (!out.is_empty() && out.len() + text.len() > ANSWER_BYTES) {
            left_out.push(key);
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n---\n\n");
        }
        out.push_str(&text);
        read.0.insert(id);
    }
    if !again.is_empty() {
        let names: Vec<&str> = again.iter().map(|(key, _)| key.as_str()).collect();
        if !out.is_empty() {
            out.push_str("\n\n---\n\n");
        }
        out.push_str(&format!(
            "Already read in this chat, and unchanged (look back at them above): {}.",
            names.join(", ")
        ));
    }
    if !left_out.is_empty() {
        out.push_str(&format!(
            "\n\n---\n\nNot included, to keep this answer short ({MAX_GUIDES_PER_CALL} at a time): {}. Ask for them next.",
            left_out.join(", ")
        ));
    }
    Ok(out)
}
