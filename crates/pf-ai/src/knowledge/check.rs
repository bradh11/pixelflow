//! Whether the written guides keep up with PixelFlow: every effect kind has one; it says what
//! the effect draws on each kind of target; it names every setting and choice the kind has, and
//! none it doesn't; and each of its looks is a real effect that draws on every target it was
//! written for. Each problem is said in words that tell a writer what to write.
//!
//! The tests run this on the guides compiled into the app; the `guide_sheets` example runs it on
//! a folder of guides as they are on disk, for someone in the middle of writing one.

use super::{Guide, Look, MAX_GUIDE_BYTES, Suit, guide, library, reference};
use crate::effect_spec::{self, Context};
use crate::stage::{self, Stage};
use crate::tools::Toolbox;
use pf_model::GroupLayout;
use pf_sequence::{EffectKind, SettingValue};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

/// The sections every effect's guide has, in this order.
pub const EFFECT_SECTIONS: [&str; 4] = ["What it draws", "Settings", "When to use it", "Looks"];
/// The fewest looks an effect's guide gives (an effect with no settings needs only one).
pub const MIN_LOOKS: usize = 2;
/// When a look is tried: from the stage's first chorus, for four seconds, drawn at these moments.
const TRY_FROM_MS: u64 = 24_000;
const TRY_FOR_MS: u64 = 4_000;
const TRY_AT_MS: [u64; 8] = [25, 250, 500, 750, 1_250, 2_000, 2_750, 3_900];

/// Every name the Assistant can say to a tool: tool names, their inputs' field names and choices
/// (walked out of the tool definitions themselves), and the guides' own names.
pub fn tool_vocabulary() -> BTreeSet<String> {
    fn walk(value: &Value, out: &mut BTreeSet<String>) {
        match value {
            Value::Object(map) => {
                for (key, v) in map {
                    match key.as_str() {
                        "properties" | "$defs" => {
                            if let Some(named) = v.as_object() {
                                out.extend(named.keys().cloned());
                            }
                        }
                        "enum" => {
                            if let Some(names) = v.as_array() {
                                out.extend(names.iter().filter_map(|n| n.as_str().map(str::to_string)));
                            }
                        }
                        "const" => out.extend(v.as_str().map(str::to_string)),
                        _ => {}
                    }
                    walk(v, out);
                }
            }
            Value::Array(items) => items.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut names = BTreeSet::new();
    for tool in Toolbox::new().tools() {
        names.insert(tool.spec.name.clone());
        walk(&tool.spec.input_schema, &mut names);
    }
    // Prop shapes' settings are looked up, like effects'.
    for shape in crate::tools::shape_types() {
        if let Some(schema) = crate::tools::shape_settings(&shape) {
            walk(&schema, &mut names);
        }
    }
    names.extend(library::topic_keys().into_iter().map(String::from));
    names.extend(crate::cues::TARGET_WORDS.iter().map(|w| w.to_string()));
    names.extend(crate::cues::CHARACTERS.iter().map(|w| w.to_string()));
    names.extend(effect_spec::EFFECT_WIDE.iter().map(|w| w.to_string()));
    names.extend(GroupLayout::ALL.iter().map(reference::key_of));
    names.extend(Suit::words().into_iter().map(String::from));
    names
}

/// The setting keys and choice values of `kind`.
fn own_vocabulary(kind: EffectKind) -> (Vec<&'static str>, Vec<&'static str>) {
    let mut keys = Vec::new();
    let mut choices = Vec::new();
    for setting in reference::info(kind).settings {
        keys.push(setting.key);
        if let SettingValue::Choice { options, .. } = setting.value {
            choices.extend(options.iter().map(|o| o.value));
        }
    }
    (keys, choices)
}

/// Every setting key and choice value of every effect kind.
pub fn effect_vocabulary() -> BTreeSet<String> {
    EffectKind::ALL
        .into_iter()
        .flat_map(|kind| {
            let (keys, choices) = own_vocabulary(kind);
            keys.into_iter().chain(choices).map(String::from)
        })
        .collect()
}

/// Whether a name in backticks is one of PixelFlow's own names (a word in camelCase, or with
/// underscores or hyphens), not an example value like `#ff0000` or `"Sections:4"`.
fn is_a_name(text: &str) -> bool {
    text.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// `kind.setting`, when `text` is written that way and names a real setting of a real kind.
fn names_another_kinds_setting(text: &str) -> Option<bool> {
    let (kind, setting) = text.split_once('.')?;
    if !is_a_name(kind) || !is_a_name(setting) {
        return None;
    }
    Some(reference::kind_named(kind).is_some_and(|k| k.settings().iter().any(|s| s.key == setting)))
}

fn listed(names: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    names
        .into_iter()
        .map(|n| format!("`{}`", n.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// How to write a guide, said when one is missing.
fn how_to_write(kind: EffectKind) -> String {
    let key = reference::key_of(&kind);
    let (keys, _) = own_vocabulary(kind);
    format!(
        "The Assistant has no guide for the {label} effect, so it can't design with it.\n\
         Write crates/pf-ai/guides/effects/{key}.md and add its line to `effect_source` in crates/pf-ai/src/knowledge/library.rs.\n\
         First draw it: `cargo run -p pf-ai --example guide_sheets -- <a scratch folder> --kinds {key} --auto` and look at the sheets.\n\
         The guide is \"# {label}\", a sentence on what it's for, then these sections: {sections}.\n\
         - What it draws: what you saw on a string, an arch, a matrix, a tree, and the whole house, at speed and slowly.\n\
         - Settings: what each of {settings} is for and a good starting value, each named in backticks, with every choice of every list.\n\
         - When to use it: what it's good for, what it pairs with, what to avoid.\n\
         - Looks: at least {MIN_LOOKS} named looks as ```look blocks ({{\"name\", \"suits\", \"note\", \"effect\"}}).\n\
         docs/dev/assistant-guides.md has the format and an example.",
        label = kind.label(),
        sections = EFFECT_SECTIONS.join(", "),
        settings = if keys.is_empty() {
            "its settings (it has none)".to_string()
        } else {
            listed(keys)
        },
    )
}

/// The look as an effect on the stage, timed to be tried.
fn look_effect(look: &Look, stage: &Stage) -> Result<pf_sequence::Effect, String> {
    let doc = stage.sequence();
    let pictures = [stage::PICTURE.to_string()];
    let mut effect = effect_spec::parse(
        &look.effect,
        &Context {
            tracks: &doc.timing_tracks,
            pictures: Some(&pictures),
        },
    )?;
    effect.start_ms = TRY_FROM_MS;
    effect.end_ms = TRY_FROM_MS + TRY_FOR_MS;
    Ok(effect)
}

/// Whether `effect` lights anything on `suit`'s target at any of the moments it's tried.
fn draws(effect: &pf_sequence::Effect, suit: Suit, stage: &Stage) -> bool {
    let doc = stage.with_effects(suit, std::slice::from_ref(effect));
    let times: Vec<u64> = TRY_AT_MS.iter().map(|t| TRY_FROM_MS + t).collect();
    let pixels = stage.pixels(suit);
    stage
        .frames(&doc, &times, &stage::pictures())
        .iter()
        .any(|frame| {
            pixels.iter().any(|p| {
                frame
                    .get(p.offset..p.offset + 3)
                    .is_some_and(|c| c.iter().any(|v| *v > 12))
            })
        })
}

/// What's wrong with the written guide for `kind` (`None`: there isn't one), in words that say
/// what to write. Empty when nothing is.
pub fn effect_problems(kind: EffectKind, source: Option<&str>, stage: &Stage) -> Vec<String> {
    let key = reference::key_of(&kind);
    let Some(source) = source else {
        return vec![how_to_write(kind)];
    };
    let file = format!("crates/pf-ai/guides/effects/{key}.md");
    let guide: Guide = match guide::parse(source) {
        Ok(guide) => guide,
        Err(e) => return vec![format!("{file}: {e}.")],
    };
    let mut problems = Vec::new();
    if guide.title != kind.label() {
        problems.push(format!(
            "{file}: its title is \"{}\"; the effect is called \"{}\".",
            guide.title,
            kind.label()
        ));
    }
    if guide.sections != EFFECT_SECTIONS {
        problems.push(format!(
            "{file}: its sections are {:?}; an effect's guide has exactly these, in order: {}.",
            guide.sections,
            EFFECT_SECTIONS.join(", ")
        ));
    }
    // What it draws on each kind of target.
    let draws_on = guide.section("What it draws").to_lowercase();
    let unsaid: Vec<&str> = Suit::words()
        .into_iter()
        .filter(|word| !draws_on.contains(word))
        .collect();
    if !unsaid.is_empty() {
        problems.push(format!(
            "{file}: \"What it draws\" doesn't say what the {} effect looks like on: {}. Say it for each of string, arch, matrix, tree, and (whole) house, from the frames.",
            kind.label(),
            unsaid.join(", ")
        ));
    }
    // Every setting and choice the kind has.
    let (keys, choices) = own_vocabulary(kind);
    let named: BTreeSet<&str> = guide.names.iter().map(String::as_str).collect();
    let missing: Vec<&str> = keys.iter().copied().filter(|k| !named.contains(k)).collect();
    if !missing.is_empty() {
        problems.push(format!(
            "{file}: the {} effect has settings its guide doesn't mention: {}. Say what each is for (and a good value), naming it in backticks. `get_guide` adds each one's range and default by itself.",
            kind.label(),
            listed(missing)
        ));
    }
    let missing: Vec<&str> = choices.iter().copied().filter(|c| !named.contains(c)).collect();
    if !missing.is_empty() {
        problems.push(format!(
            "{file}: the {} effect has choices its guide doesn't mention: {}. Say what each looks like or when to pick it, naming it in backticks.",
            kind.label(),
            listed(missing)
        ));
    }
    // No name it doesn't have.
    let general = tool_vocabulary();
    let kinds = reference::kind_keys();
    let unknown: BTreeSet<&str> = named
        .iter()
        .copied()
        .filter(|name| match names_another_kinds_setting(name) {
            Some(real) => !real,
            None => {
                is_a_name(name)
                    && !keys.contains(name)
                    && !choices.contains(name)
                    && !general.contains(*name)
                    && !kinds.iter().any(|k| k == name)
            }
        })
        .collect();
    if !unknown.is_empty() {
        problems.push(format!(
            "{file}: it names things the {} effect (and PixelFlow's tools) don't have: {}. Names in backticks are for this effect's settings and choices, other effect kinds, another kind's setting as `kind.setting`, and tools and their inputs, exactly as they're spelled; write anything else as plain words. This effect's settings: {}.",
            kind.label(),
            listed(unknown),
            if keys.is_empty() { "none".to_string() } else { listed(&keys) }
        ));
    }
    // Looks: real effects of this kind that draw where they say.
    let fewest = if keys.is_empty() { 1 } else { MIN_LOOKS };
    if guide.looks.len() < fewest {
        problems.push(format!(
            "{file}: it has {} looks; give at least {fewest}, each a ```look block.",
            guide.looks.len()
        ));
    }
    let mut seen = BTreeSet::new();
    for look in &guide.looks {
        if !seen.insert(look.name.as_str()) {
            problems.push(format!("{file}: two looks are called \"{}\".", look.name));
        }
        if look.effect["kind"] != key.as_str() {
            problems.push(format!(
                "{file}: the look \"{}\" is a {} effect; a guide's looks are of its own kind (\"kind\": \"{key}\").",
                look.name, look.effect["kind"]
            ));
            continue;
        }
        match look_effect(look, stage) {
            Err(e) => problems.push(format!(
                "{file}: the look \"{}\" isn't an effect PixelFlow can place: {e}",
                look.name
            )),
            Ok(effect) if kind != EffectKind::Off => {
                for &suit in &look.suits {
                    if !draws(&effect, suit, stage) {
                        problems.push(format!(
                            "{file}: the look \"{}\" lights nothing on the {} in its first four seconds. Take \"{}\" out of its suits, or change it so it draws there.",
                            look.name,
                            suit.word(),
                            suit.word()
                        ));
                    }
                }
            }
            Ok(_) => {}
        }
    }
    let length = guide.text.len() + reference::effect(kind).len();
    if length > MAX_GUIDE_BYTES {
        problems.push(format!(
            "{file}: with its reference it is {length} bytes as the Assistant reads it; keep it under {MAX_GUIDE_BYTES} so several fit in one answer."
        ));
    }
    problems
}

/// What's wrong with the topic `key` as written in `source`.
pub fn topic_problems(key: &str, source: &str) -> Vec<String> {
    let file = format!("crates/pf-ai/guides/topics/{key}.md");
    let filled = super::fill(source);
    let guide = match guide::parse(&filled) {
        Ok(guide) => guide,
        Err(e) => return vec![format!("{file}: {e}.")],
    };
    let mut problems = Vec::new();
    if guide.text.contains("{{") {
        problems.push(format!(
            "{file}: it asks for a list PixelFlow doesn't generate; the lists are: {}.",
            super::GENERATED.join(", ")
        ));
    }
    let mut known = tool_vocabulary();
    known.extend(effect_vocabulary());
    known.extend(reference::kind_keys());
    let unknown: BTreeSet<&str> = guide
        .names
        .iter()
        .map(String::as_str)
        .filter(|name| match names_another_kinds_setting(name) {
            Some(real) => !real,
            None => is_a_name(name) && !known.contains(*name),
        })
        .collect();
    if !unknown.is_empty() {
        problems.push(format!(
            "{file}: it names things PixelFlow doesn't have: {}. Names in backticks are for settings, choices, effect kinds, topics, and tools exactly as they're spelled; write anything else as plain words.",
            listed(unknown)
        ));
    }
    let stage = Stage::new();
    for look in &guide.looks {
        if let Err(e) = look_effect(look, &stage) {
            problems.push(format!(
                "{file}: the look \"{}\" isn't an effect PixelFlow can place: {e}",
                look.name
            ));
        }
    }
    if guide.text.len() > MAX_GUIDE_BYTES {
        problems.push(format!(
            "{file}: it is {} bytes as the Assistant reads it; keep it under {MAX_GUIDE_BYTES}.",
            guide.text.len()
        ));
    }
    problems
}

/// Everything wrong with the guides: the ones compiled into the app, or, with `folder`, the ones
/// in that folder (`effects/<kind>.md`, `topics/<key>.md`) as they are on disk.
pub fn problems(folder: Option<&Path>) -> Vec<String> {
    let stage = Stage::new();
    let read = |part: &str, name: &str| -> Option<String> {
        std::fs::read_to_string(folder?.join(part).join(format!("{name}.md"))).ok()
    };
    let mut problems = Vec::new();
    for kind in EffectKind::ALL {
        let source = match folder {
            Some(_) => read("effects", &reference::key_of(&kind)),
            None => library::effect_source(kind).map(str::to_string),
        };
        problems.extend(effect_problems(kind, source.as_deref(), &stage));
    }
    for key in library::topic_keys() {
        let source = match folder {
            Some(_) => read("topics", key),
            None => library::topic_source(key).map(str::to_string),
        };
        match source {
            Some(source) => problems.extend(topic_problems(key, &source)),
            None => problems.push(format!(
                "There is no \"{key}\" topic: write crates/pf-ai/guides/topics/{key}.md."
            )),
        }
    }
    problems
}
