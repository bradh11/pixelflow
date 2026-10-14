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

/// Whether `kind`'s guide has a look for a single run of pixels (a string or an arch): `None`
/// for a kind without a guide. One without is what the review's fit criterion flags on an
/// outline or an arch.
pub fn suits_a_run(kind: EffectKind) -> Option<bool> {
    let guide = effect_guide(kind)?;
    Some(
        guide
            .looks
            .iter()
            .any(|l| l.suits.contains(&Suit::String) || l.suits.contains(&Suit::Arch)),
    )
}

/// The longest a guide may be as the Assistant reads it (bytes), reference and all: several fit
/// in one answer.
pub const MAX_GUIDE_BYTES: usize = 12_000;
/// Guides one `get_guide` call may ask for.
pub const MAX_GUIDES_PER_CALL: usize = 12;
/// How much one `get_guide` answer may hold: what a design job reads before it starts, in one
/// round trip rather than two.
pub const MAX_GUIDE_ANSWER_BYTES: usize = 48_000;

/// The parts of an effect's guide `get_guide` can answer on their own (`parts`): the name it
/// takes, and the heading the guide gives it.
pub const PARTS: [(&str, &str); 5] = [
    ("draws", "What it draws"),
    ("settings", "Settings"),
    ("use", "When to use it"),
    ("looks", "Looks"),
    ("reference", "Reference"),
];

/// The guide for `kind`, only the `parts` named (from [`PARTS`], in their order): its title and
/// summary, then those sections.
pub fn effect_parts(kind: EffectKind, parts: &[&str]) -> String {
    let text = effect_text(kind);
    let mut pieces = text.split("\n## ");
    let mut out = pieces.next().unwrap_or_default().trim_end().to_string();
    let sections: Vec<&str> = pieces.collect();
    for (part, heading) in PARTS {
        if !parts.contains(&part) {
            continue;
        }
        if let Some(section) = sections.iter().find(|s| s.split('\n').next() == Some(heading)) {
            out.push_str("\n\n## ");
            out.push_str(section.trim_end());
        }
    }
    out
}

/// The written guide for `kind`, read. (`None` only for a guide that can't be read, which the
/// tests don't let through: the reference then stands alone.)
pub fn effect_guide(kind: EffectKind) -> Option<Guide> {
    guide::parse(library::effect_source(kind)).ok()
}

/// The guide for `kind` as the Assistant reads it: the written guide, then the reference.
pub fn effect_text(kind: EffectKind) -> String {
    let reference = reference::effect(kind);
    match effect_guide(kind) {
        Some(guide) => format!("{}\n\n{reference}", guide.text.trim_end()),
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
pub const GENERATED: [&str; 15] = [
    "effect-overview",
    "grid-only",
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
        "effect-overview" => effect_overview(),
        "looks-by-target" => looks_by_target(),
        "grid-only" => grid_only(),
        _ => return None,
    })
}

/// The kinds with no look for a single run of pixels: the ones not to put on an outline or an
/// arch (the review says so when they are).
fn grid_only() -> String {
    let kinds: Vec<String> = EffectKind::ALL
        .iter()
        .filter(|&&kind| suits_a_run(kind) == Some(false))
        .map(|kind| format!("`{}`", reference::key_of(kind)))
        .collect();
    format!("{}\n", kinds.join(", "))
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

/// Every effect in a line, for choosing between them without reading every guide: its key,
/// the kinds of target its looks were written for, and what it's for.
fn effect_overview() -> String {
    EffectKind::ALL
        .iter()
        .map(|&kind| {
            let key = reference::key_of(&kind);
            let Some(guide) = effect_guide(kind) else {
                return format!("- `{key}`: {}\n", reference::info(kind).description);
            };
            let suits: Vec<&str> = Suit::ALL
                .iter()
                .filter(|suit| guide.looks.iter().any(|l| l.suits.contains(suit)))
                .map(|suit| suit.word())
                .collect();
            format!("- `{key}` ({}): {}\n", suits.join(", "), guide.summary)
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
    let parts: Vec<&str> = PARTS.iter().map(|(part, _)| *part).collect();
    let mut out = format!(
        "PixelFlow's guides. Ask for up to {MAX_GUIDES_PER_CALL} at once, e.g. get_guide({{\"effects\": [\"bars\", \"spiral\"], \"topics\": [\"blends\"]}}). An effect's guide can come in parts, e.g. \"parts\": [\"looks\"] (parts: {}). Topic `choosing` sets every effect side by side.\n\nTopics:\n",
        parts.join(", ")
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
    if let Some(unknown) = input.as_object().and_then(|o| {
        o.keys()
            .find(|k| !["effects", "topics", "parts"].contains(&k.as_str()))
    }) {
        return Err(format!(
            "get_guide has no {unknown}. It takes effects, topics, and parts."
        ));
    }
    let effects = asked(input, "effects")?;
    let topics = asked(input, "topics")?;
    let parts = asked(input, "parts")?;
    let names: Vec<&str> = PARTS.iter().map(|(name, _)| *name).collect();
    if let Some(unknown) = parts.iter().find(|p| !names.contains(&p.as_str())) {
        return Err(format!(
            "An effect's guide has no part \"{}\". Parts: {}.",
            unknown.chars().take(40).collect::<String>(),
            names.join(", ")
        ));
    }
    if effects.is_empty() && topics.is_empty() {
        return Ok(index());
    }
    // Every name is checked first, so a wrong one is refused before anything is sent.
    for key in &topics {
        if !library::topic_keys().contains(&key.as_str()) {
            return Err(format!(
                "There's no topic \"{key}\". Topics: {}.",
                library::topic_keys().join(", ")
            ));
        }
    }
    for key in &effects {
        if reference::kind_named(key).is_none() {
            return Err(format!(
                "There's no effect \"{key}\". Effects: {}.",
                reference::kind_keys().join(", ")
            ));
        }
    }
    // Each guide asked for: its name, what it marks read, and its text (`None` when this chat
    // has read all of it that was asked for). Topics come whole; an effect's guide whole, or
    // only the parts asked for that haven't been read.
    let mut guides: Vec<(String, Vec<String>, Option<String>)> = Vec::new();
    for key in &topics {
        let id = format!("topic:{key}");
        let text = (!read.contains(&id)).then(|| topic_text(key).unwrap_or_default());
        guides.push((key.clone(), vec![id], text));
    }
    for key in &effects {
        let whole = format!("effect:{key}");
        let kind = reference::kind_named(key);
        if parts.is_empty() {
            let text = (!read.contains(&whole)).then(|| kind.map(effect_text).unwrap_or_default());
            guides.push((key.clone(), vec![whole], text));
            continue;
        }
        let unread: Vec<&str> = PARTS
            .iter()
            .map(|(part, _)| *part)
            .filter(|part| parts.iter().any(|p| p == part))
            .filter(|part| !read.contains(&whole) && !read.contains(&format!("{whole}#{part}")))
            .collect();
        let name = format!("{key} ({})", parts.join(", "));
        let ids = unread.iter().map(|part| format!("{whole}#{part}")).collect();
        let text = (!unread.is_empty()).then(|| kind.map(|k| effect_parts(k, &unread)).unwrap_or_default());
        guides.push((name, ids, text));
    }
    let mut seen = std::collections::BTreeSet::new();
    guides.retain(|(name, _, _)| seen.insert(name.clone()));
    let (new, again): (Vec<_>, Vec<_>) = guides.into_iter().partition(|(_, _, text)| text.is_some());
    let mut out = String::new();
    let mut left_out: Vec<String> = Vec::new();
    for (i, (key, ids, text)) in new.into_iter().enumerate() {
        let text = text.unwrap_or_default();
        // Room is kept for the lines saying what was left out and what was read before.
        if i >= MAX_GUIDES_PER_CALL
            || (!out.is_empty() && out.len() + text.len() > MAX_GUIDE_ANSWER_BYTES - 1_000)
        {
            left_out.push(key);
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n---\n\n");
        }
        out.push_str(&text);
        read.0.extend(ids);
    }
    if !again.is_empty() {
        let names: Vec<&str> = again.iter().map(|(key, _, _)| key.as_str()).collect();
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
