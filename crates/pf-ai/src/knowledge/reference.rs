//! The reference half of the Assistant's knowledge: what every effect kind, setting, choice,
//! blend, curve shape, render style, cue, and timing track is, with its range and default.
//! Nothing here is typed out: it is read from the tables the app itself runs on (the effect
//! catalog, the edit types' JSON Schemas, the cue and track lists), so it can't fall behind them.

use pf_model::{BufferTransform, GroupLayout, RenderStyle};
use pf_sequence::{
    Blend, CurveShape, Effect, EffectInfo, EffectKind, RowBlending, SettingInfo, SettingValue, TimingKind,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// A value's name in files and tool inputs (`"colorWash"`).
pub fn key_of<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// The effect kind `key` names.
pub fn kind_named(key: &str) -> Option<EffectKind> {
    EffectKind::ALL.into_iter().find(|k| key_of(k) == key)
}

/// Every effect kind's key, in menu order.
pub fn kind_keys() -> Vec<String> {
    EffectKind::ALL.iter().map(key_of).collect()
}

fn schema<T: schemars::JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap_or(Value::Null)
}

/// A doc comment on one line, without a trailing full stop's worth of space.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A schema's choices: each value with what its doc comment says of it (nothing, for a choice
/// documented only in the code).
fn choices_of(definition: &Value) -> Vec<(String, String)> {
    if let Some(options) = definition["oneOf"].as_array() {
        return options
            .iter()
            .filter_map(|o| {
                Some((
                    o["const"].as_str()?.to_string(),
                    one_line(o["description"].as_str().unwrap_or_default()),
                ))
            })
            .collect();
    }
    definition["enum"]
        .as_array()
        .map(|names| {
            names
                .iter()
                .filter_map(|n| Some((n.as_str()?.to_string(), String::new())))
                .collect()
        })
        .unwrap_or_default()
}

/// What the doc comments say of each choice of the choice setting `key` of `kind`, by value.
fn choice_notes(kind: EffectKind, key: &str) -> BTreeMap<String, String> {
    static PARAMS: OnceLock<Value> = OnceLock::new();
    let root = PARAMS.get_or_init(schema::<pf_sequence::EffectParams>);
    let tag = key_of(&kind);
    let find = || {
        let variant = root["oneOf"]
            .as_array()?
            .iter()
            .find(|v| v["properties"]["kind"]["const"] == tag.as_str())?;
        let params = variant["$ref"].as_str()?.strip_prefix("#/$defs/")?;
        let property = &root["$defs"][params]["properties"][key];
        let name = property["$ref"].as_str()?.strip_prefix("#/$defs/")?;
        Some(choices_of(&root["$defs"][name]))
    };
    find().unwrap_or_default().into_iter().collect()
}

/// A number as a person writes it (`0.05`, `20`).
fn number(n: f32) -> String {
    format!("{n}")
}

fn unit(unit: Option<&str>) -> String {
    unit.map_or(
        String::new(),
        |u| {
            if u == "%" { "%".into() } else { format!(" {u}") }
        },
    )
}

/// One setting as a line: its key, the name people see, what it holds, its default, and what it
/// does.
fn setting_line(kind: EffectKind, setting: &SettingInfo) -> String {
    let more = if setting.more { ", under More" } else { "" };
    let holds = match &setting.value {
        SettingValue::Number {
            min,
            max,
            default,
            unit: u,
            ..
        } => format!(
            "{} to {}{}; default {}",
            number(*min),
            number(*max),
            unit(*u),
            number(*default)
        ),
        SettingValue::Int {
            min,
            max,
            default,
            unit: u,
            ..
        } => format!("whole number {min} to {max}{}; default {default}", unit(*u)),
        SettingValue::Bool { default } => format!("true or false; default {default}"),
        SettingValue::Choice { default, options } => {
            let notes = choice_notes(kind, setting.key);
            let listed: Vec<String> = options
                .iter()
                .map(|o| {
                    match notes
                        .get(o.value)
                        .filter(|n| !n.is_empty() && !n.trim_end_matches('.').eq_ignore_ascii_case(o.label))
                    {
                        Some(note) => format!("`{}` ({}: {})", o.value, o.label, note.trim_end_matches('.')),
                        None => format!("`{}` ({})", o.value, o.label),
                    }
                })
                .collect();
            format!("{}; default `{default}`", listed.join(" | "))
        }
        SettingValue::Face { .. } => "the name of one of the prop's faces; \"\" is its first".to_string(),
        SettingValue::Text { default } if !default.is_empty() => format!(
            "text, up to {} characters; default \"{default}\"",
            pf_sequence::MAX_TEXT_LEN
        ),
        SettingValue::Text { .. } => {
            format!("text, up to {} characters", pf_sequence::MAX_TEXT_LEN)
        }
        SettingValue::Image { .. } => {
            "one of the show's pictures, as get_open_sequence lists them; \"\" is none".to_string()
        }
        SettingValue::TimingTrack { .. } => {
            "a timing track: its id (in place_effects and change_effects, its name will do); none by default"
                .to_string()
        }
        SettingValue::ShaderFile { default } => format!(
            "a shader's file, as list_shaders lists them (`pixelflow:<name>` for PixelFlow's own); default `{default}`"
        ),
        SettingValue::ShaderInputs { .. } => {
            "the shader's own inputs by name, each as list_shaders with its file describes it; one left out is as the shader has it"
                .to_string()
        }
    };
    format!(
        "- `{}` ({}{more}): {holds}. {}",
        setting.key,
        setting.label,
        one_line(&setting.description)
    )
}

/// The catalog's entry for `kind`.
pub fn info(kind: EffectKind) -> EffectInfo {
    pf_sequence::effect_catalog()
        .into_iter()
        .find(|e| e.kind == kind)
        .expect("the catalog lists every kind")
}

/// The settings of `kind` a curve can drive (its number settings), by key.
pub fn curve_keys(kind: EffectKind) -> Vec<&'static str> {
    kind.settings()
        .iter()
        .map(|s| s.key)
        .filter(|key| Effect::curve_range(kind, key).is_some())
        .collect()
}

/// The effect-wide settings a curve can drive on any effect.
pub fn effect_wide_curve_keys() -> Vec<&'static str> {
    let mut keys = vec!["sparkles", "blur"];
    keys.extend(Effect::ADJUSTMENT_CURVES);
    keys
}

/// Every setting of `kind`: key, label, range or choices, default, and what it does.
pub fn effect(kind: EffectKind) -> String {
    let info = info(kind);
    let mut out = format!(
        "## Reference\n\n{} (`{}`): {}\n",
        info.label,
        key_of(&kind),
        info.description
    );
    if info.settings.is_empty() {
        out.push_str("It has no settings of its own.\n");
    }
    for setting in &info.settings {
        out.push_str(&setting_line(kind, setting));
        out.push('\n');
    }
    let curves = curve_keys(kind);
    if !curves.is_empty() {
        out.push_str(&format!(
            "Curves can drive: {} (and, on any effect, {}).\n",
            curves.join(", "),
            effect_wide_curve_keys().join(", ")
        ));
    }
    out.push_str("Every effect also takes colors, blend, fades, sparkles, blur, color adjustments, a render style, a turn or flip, and curves: topic `effect-wide`.\n");
    out
}

/// A list of choices as lines: `- \`value\`: what it does`.
fn choice_lines(choices: &[(String, String)]) -> String {
    choices
        .iter()
        .map(|(value, note)| {
            if note.is_empty() {
                format!("- `{value}`\n")
            } else {
                format!("- `{value}`: {note}\n")
            }
        })
        .collect()
}

/// Names in a row: `` `a`, `b`, `c` ``.
fn names(values: impl IntoIterator<Item = String>) -> String {
    values
        .into_iter()
        .map(|v| format!("`{v}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every blend, with what it does.
pub fn blends() -> String {
    let documented: BTreeMap<String, String> = choices_of(&schema::<Blend>()).into_iter().collect();
    let all: Vec<(String, String)> = Blend::ALL
        .iter()
        .map(|b| {
            let key = b.key().to_string();
            let note = documented.get(&key).cloned().unwrap_or_default();
            (key, note)
        })
        .collect();
    choice_lines(&all)
}

/// The three ways a row meets the rows before it.
pub fn row_blending() -> String {
    names(RowBlending::ALL.iter().map(key_of))
}

/// Every curve shape, with what it does.
pub fn curve_shapes() -> String {
    let documented: BTreeMap<String, String> = choices_of(&schema::<CurveShape>()).into_iter().collect();
    let all: Vec<(String, String)> = CurveShape::ALL
        .iter()
        .map(|s| {
            let key = key_of(s);
            let note = documented.get(&key).cloned().unwrap_or_default();
            (key, note)
        })
        .collect();
    choice_lines(&all)
}

/// A curve's fields, with what the doc comments say of each.
pub fn curve_fields() -> String {
    let root = schema::<pf_sequence::Curve>();
    root["properties"]
        .as_object()
        .map(|properties| {
            properties
                .iter()
                .map(|(key, property)| {
                    let note = one_line(property["description"].as_str().unwrap_or_default());
                    if note.is_empty() {
                        format!("- `{key}`\n")
                    } else {
                        format!("- `{key}`: {note}\n")
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn render_styles() -> String {
    names(RenderStyle::ALL.iter().map(key_of))
}

pub fn buffer_transforms() -> String {
    names(buffer_transform_keys())
}

/// Every turn-or-flip's key (the type keeps no list of its own: its schema does).
pub fn buffer_transform_keys() -> Vec<String> {
    choices_of(&schema::<BufferTransform>())
        .into_iter()
        .map(|(key, _)| key)
        .collect()
}

pub fn group_layouts() -> String {
    names(GroupLayout::ALL.iter().map(key_of))
}

/// Every timing track kind's key.
pub fn timing_kind_keys() -> Vec<String> {
    choices_of(&schema::<TimingKind>())
        .into_iter()
        .map(|(key, _)| key)
        .collect()
}

pub fn timing_kinds() -> String {
    names(timing_kind_keys())
}

/// The tracks `add_song_timing` makes.
pub fn song_tracks() -> String {
    names(crate::song::TRACK_CHOICES.iter().map(|t| t.to_string()))
}

/// The settings every effect has besides its kind's own: key and what the doc comment says.
pub fn effect_wide() -> String {
    let root = schema::<Effect>();
    let skipped = ["id", "startMs", "endMs", "params"];
    root["properties"]
        .as_object()
        .map(|properties| {
            properties
                .iter()
                .filter(|(key, _)| !skipped.contains(&key.as_str()))
                .map(|(key, property)| {
                    let note = one_line(property["description"].as_str().unwrap_or_default());
                    if note.is_empty() {
                        format!("- `{key}`\n")
                    } else {
                        format!("- `{key}`: {note}\n")
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The keys of [`effect_wide`].
pub fn effect_wide_keys() -> Vec<String> {
    let root = schema::<Effect>();
    let skipped = ["id", "startMs", "endMs", "params"];
    root["properties"]
        .as_object()
        .map(|properties| {
            properties
                .keys()
                .filter(|key| !skipped.contains(&key.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Every cue `stage_cue` takes, with what it stages.
pub fn cues() -> String {
    crate::cues::CUE_NOTES
        .iter()
        .map(|(cue, note)| format!("- `{cue}`: {note}\n"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_effects_reference_gives_every_setting_its_range_default_and_choices() {
        let bars = effect(EffectKind::Bars);
        assert!(
            bars.starts_with("## Reference\n\nBars (`bars`): Stripes"),
            "{bars}"
        );
        assert!(
            bars.contains("- `count` (Bars): whole number 1 to 100; default 4. Bars visible at once."),
            "{bars}"
        );
        assert!(
            bars.contains("- `speed` (Speed): 0 to 20 per second; default 0.5."),
            "{bars}"
        );
        // A choice's values carry what the code's own comments say of them.
        assert!(
            bars.contains("`split` (From the center: Apart from the center, mirrored about it"),
            "{bars}"
        );
        assert!(
            bars.contains("- `center` (Center, under More): -100 to 100%; default 0."),
            "{bars}"
        );
        assert!(
            bars.contains("Curves can drive: count, speed, fill, center (and"),
            "{bars}"
        );
        assert!(effect(EffectKind::Off).contains("no settings of its own"));
        let chase = effect(EffectKind::Chase);
        assert!(
            chase.contains("`timingTrack` (Step on marks of, under More): a timing track"),
            "{chase}"
        );
    }

    #[test]
    fn blends_and_curve_shapes_are_listed_with_what_the_code_says_of_them() {
        let blends = blends();
        assert!(blends.contains("- `add`: Adds its light.\n"), "{blends}");
        assert_eq!(blends.lines().count(), Blend::ALL.len());
        let shapes = curve_shapes();
        assert!(
            shapes.contains("- `musicTrigger`: Jumps to `to` while the music is louder"),
            "{shapes}"
        );
        assert_eq!(shapes.lines().count(), CurveShape::ALL.len());
        assert!(curve_fields().contains("- `cycles`: Repeats over the effect"));
        assert!(effect_wide().contains("- `sparkles`: Sparkles on lit pixels: 0 (none) to 200."));
        assert_eq!(buffer_transform_keys().len(), 8);
        assert_eq!(timing_kind_keys().len(), 7);
    }
}
