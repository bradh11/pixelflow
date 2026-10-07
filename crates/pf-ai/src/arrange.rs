//! Many effects in one tool call, so a whole song fits in a reasonable number of steps:
//! `place_effects` puts one effect on many rows over a time range, cut at a timing track's
//! marks and spread across the rows; `repeat_effects` copies a stretch of effects to other
//! times. Each is one batch of [`SequenceEdit`]s on the draft: all of it, or (with the reason)
//! none of it.

use crate::diff::target_name;
use crate::draft::Draft;
use crate::song::find_track;
use pf_engine::SequenceEdit;
use pf_sequence::{Blend, Effect, EffectId, EffectKind, EffectParams, Palette, Rgb, Row, RowId, format_ms};
use serde_json::{Value, json};

/// Effects one call may add, at most.
pub const MAX_PLACED: usize = 2_000;
/// Copies one `repeat_effects` call may make, at most.
const MAX_REPEATS: usize = 200;

/// How effects are shared out among the rows, slot by slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spread {
    /// Every row, every slot.
    Together,
    /// Neighbouring rows take turns: row `i` gets slots `j` with `i + j` even.
    Alternate,
    /// One row at a time, in order: slot `j` goes to row `j % rows`.
    Sweep,
    /// Row `i` joins at slot `i` and stays.
    Build,
}

pub const SPREADS: [&str; 4] = ["together", "alternate", "sweep", "build"];

impl Spread {
    fn parse(text: Option<&str>) -> Result<Self, String> {
        Ok(match text.unwrap_or("together") {
            "together" => Spread::Together,
            "alternate" => Spread::Alternate,
            "sweep" => Spread::Sweep,
            "build" => Spread::Build,
            other => {
                return Err(format!(
                    "There's no spread \"{other}\"; choose from {}.",
                    SPREADS.join(", ")
                ));
            }
        })
    }

    fn gets(self, row: usize, rows: usize, slot: usize) -> bool {
        match self {
            Spread::Together => true,
            Spread::Alternate => (row + slot).is_multiple_of(2),
            Spread::Sweep => slot % rows.max(1) == row,
            Spread::Build => slot >= row,
        }
    }
}

fn text_list(value: &Value, what: &str) -> Result<Vec<String>, String> {
    match value {
        Value::Null => Ok(Vec::new()),
        Value::Array(items) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or(format!("{what} must be a list of text."))
            })
            .collect(),
        _ => Err(format!("{what} must be a list.")),
    }
}

/// Refuses fields `what` doesn't take (a misspelled option would otherwise be ignored).
fn known_keys(value: &Value, known: &[&str], what: &str) -> Result<(), String> {
    let unknown: Vec<&str> = value
        .as_object()
        .map(|o| {
            o.keys()
                .map(String::as_str)
                .filter(|k| !known.contains(k))
                .collect()
        })
        .unwrap_or_default();
    if unknown.is_empty() {
        return Ok(());
    }
    Err(format!(
        "Nothing was drafted: {what} has no {}. It takes {}.",
        unknown.join(", "),
        known.join(", ")
    ))
}

fn time(input: &Value, field: &str) -> Result<u64, String> {
    input[field]
        .as_u64()
        .ok_or_else(|| format!("{field} must be a whole number of milliseconds."))
}

/// The effect described by `place_effects`' `effect`: a kind with settings, colors, blend, and
/// fades (its id and times are set per copy).
fn template(spec: &Value) -> Result<Effect, String> {
    known_keys(
        spec,
        &["kind", "settings", "colors", "blend", "fadeInMs", "fadeOutMs"],
        "effect",
    )?;
    let kind = spec["kind"].as_str().unwrap_or_default();
    if serde_json::from_value::<EffectKind>(json!(kind)).is_err() {
        let kinds: Vec<String> = EffectKind::ALL
            .iter()
            .filter_map(|k| serde_json::to_value(k).ok())
            .filter_map(|k| k.as_str().map(str::to_string))
            .collect();
        return Err(format!(
            "There's no effect kind \"{kind}\"; choose from {}.",
            kinds.join(", ")
        ));
    }
    let mut params = match &spec["settings"] {
        Value::Object(settings) => {
            let mut object = settings.clone();
            object.insert("kind".into(), json!(kind));
            let given = Value::Object(object);
            let params = serde_json::from_value::<EffectParams>(given.clone()).map_err(|e| {
                format!("Those settings don't fit a {kind} effect: {e}. list_effect_kinds lists them.")
            })?;
            let ignored: Vec<String> =
                crate::tools::ignored_keys(&given, &serde_json::to_value(&params).unwrap_or(Value::Null))
                    .into_iter()
                    .map(|k| format!("settings.{k}"))
                    .collect();
            if !ignored.is_empty() {
                return Err(crate::tools::ignored_message(&ignored));
            }
            params
        }
        Value::Null => {
            serde_json::from_value::<EffectParams>(json!({ "kind": kind })).map_err(|e| e.to_string())?
        }
        _ => return Err("settings must be an object.".into()),
    };
    params.sanitize();
    let colors = text_list(&spec["colors"], "colors")?;
    if colors.len() > pf_sequence::MAX_PALETTE_COLORS {
        return Err(format!(
            "An effect takes at most {} colors.",
            pf_sequence::MAX_PALETTE_COLORS
        ));
    }
    let colors: Vec<Rgb> = colors
        .iter()
        .map(|c| {
            serde_json::from_value::<Rgb>(json!(c))
                .map_err(|_| format!("\"{c}\" isn't a color; use \"#rrggbb\"."))
        })
        .collect::<Result<_, _>>()?;
    let blend = match &spec["blend"] {
        Value::Null => Blend::default(),
        value => serde_json::from_value::<Blend>(value.clone())
            .map_err(|_| "blend is one of normal, add, max, multiply.".to_string())?,
    };
    let fade = |field: &str| u32::try_from(spec[field].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
    let mut effect = Effect::new(params.kind(), 0, 1).with_params(params);
    if !colors.is_empty() {
        effect.palette = Palette::new(colors);
    }
    effect.blend = blend;
    effect.fade_in_ms = fade("fadeInMs");
    effect.fade_out_ms = fade("fadeOutMs");
    Ok(effect)
}

/// `effect` placed from `start` to `end`, with a new id and fades that fit.
fn placed(effect: &Effect, start_ms: u64, end_ms: u64) -> Effect {
    let mut out = effect.clone();
    out.id = EffectId::new();
    out.start_ms = start_ms;
    out.end_ms = end_ms;
    let half = u32::try_from((end_ms - start_ms) / 2).unwrap_or(u32::MAX);
    out.fade_in_ms = out.fade_in_ms.min(half);
    out.fade_out_ms = out.fade_out_ms.min(half);
    out
}

fn rows_named<'a>(rows: &'a [Row], ids: &[String]) -> Result<Vec<&'a Row>, String> {
    ids.iter()
        .map(|id| {
            rows.iter().find(|r| r.id.to_string() == *id).ok_or_else(|| {
                format!("There's no row with the id \"{id}\". get_open_sequence lists the rows.")
            })
        })
        .collect()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// Plans additions on rows' layers: refuses an overlap with what's there (or with another
/// addition), or, with `replace`, removes what's there in the way first.
struct Plan<'a> {
    draft: &'a Draft,
    replace: bool,
    removed: Vec<EffectId>,
    added: Vec<(RowId, usize, Effect)>,
    edits: Vec<SequenceEdit>,
}

impl<'a> Plan<'a> {
    fn new(draft: &'a Draft, replace: bool) -> Self {
        Self {
            draft,
            replace,
            removed: Vec::new(),
            added: Vec::new(),
            edits: Vec::new(),
        }
    }

    /// Adds `effect` on `row`'s `layer`, unless something is in the way. `keep` effects are
    /// never replaced (a pattern being copied).
    fn add(&mut self, row: &Row, layer: usize, effect: Effect, keep: &[EffectId]) -> Result<(), String> {
        if layer > row.layers.len() {
            return Err(format!(
                "{} has {}; layer can be at most {} (a new layer on top).",
                self.name(row),
                plural(row.layers.len(), "layer", "layers"),
                row.layers.len()
            ));
        }
        let existing = row.layers.get(layer).map_or(&[][..], |l| &l.effects[..]);
        for other in existing {
            if self.removed.contains(&other.id) || !other.overlaps(&effect) {
                continue;
            }
            if self.replace && !keep.contains(&other.id) {
                self.removed.push(other.id);
                self.edits.push(SequenceEdit::RemoveEffect { id: other.id });
            } else {
                return Err(self.in_the_way(row, layer, other.start_ms, other.end_ms));
            }
        }
        if let Some((_, _, other)) = self
            .added
            .iter()
            .find(|(r, l, e)| *r == row.id && *l == layer && e.overlaps(&effect))
        {
            return Err(self.in_the_way(row, layer, other.start_ms, other.end_ms));
        }
        self.edits.push(SequenceEdit::AddEffect {
            row: row.id,
            layer,
            effect: effect.clone(),
        });
        self.added.push((row.id, layer, effect));
        if self.added.len() > MAX_PLACED {
            return Err(format!(
                "That's more than {MAX_PLACED} effects in one call: split it up (by section, say)."
            ));
        }
        Ok(())
    }

    fn name(&self, row: &Row) -> String {
        target_name(self.draft.show(), row.target)
    }

    fn in_the_way(&self, row: &Row, layer: usize, start_ms: u64, end_ms: u64) -> String {
        format!(
            "{} already has effects on layer {layer} there ({}–{}). Use another layer, or replace: true to take their place.",
            self.name(row),
            format_ms(start_ms),
            format_ms(end_ms)
        )
    }
}

/// `place_effects`: one effect on many rows over a time range (see the tool's description).
pub fn place(draft: &mut Draft, input: &Value) -> Result<String, String> {
    let doc = draft
        .sequence()
        .ok_or("No sequence is open. Offer ask_for_song so the user can pick a song for a new one.")?;
    known_keys(
        input,
        &[
            "rowIds",
            "fromMs",
            "toMs",
            "effect",
            "track",
            "marksEach",
            "spread",
            "layer",
            "replace",
        ],
        "place_effects",
    )?;
    let effect = template(&input["effect"])?;
    let ids = text_list(&input["rowIds"], "rowIds")?;
    if ids.is_empty() {
        return Err("Give at least one row in rowIds.".into());
    }
    let rows = rows_named(&doc.rows, &ids)?;
    let from = time(input, "fromMs")?;
    let to = time(input, "toMs")?.min(doc.duration_ms);
    if to <= from {
        return Err(format!(
            "fromMs must come before toMs, inside the sequence (it's {} long).",
            format_ms(doc.duration_ms)
        ));
    }
    let slots: Vec<(u64, u64)> = match input["track"].as_str() {
        None => vec![(from, to)],
        Some(key) => {
            let track = find_track(&doc.timing_tracks, key).ok_or_else(|| {
                format!("There's no timing track \"{key}\". get_open_sequence lists them; add_song_timing makes Beats, Bars, and Sections.")
            })?;
            let each = input["marksEach"].as_u64().unwrap_or(1).clamp(1, 10_000) as usize;
            let marks: Vec<_> = track
                .marks
                .iter()
                .filter(|m| m.start_ms >= from && m.start_ms < to)
                .collect();
            let slots: Vec<(u64, u64)> = marks
                .chunks(each)
                .filter_map(|chunk| {
                    let start = chunk.first()?.start_ms;
                    let end = chunk.last()?.end_ms.min(to);
                    (end > start).then_some((start, end))
                })
                .collect();
            if slots.is_empty() {
                return Err(format!(
                    "No marks of \"{}\" start between {} and {}.",
                    track.name,
                    format_ms(from),
                    format_ms(to)
                ));
            }
            slots
        }
    };
    let spread = Spread::parse(input["spread"].as_str())?;
    let layer = input["layer"].as_u64().unwrap_or(0) as usize;
    let mut plan = Plan::new(draft, input["replace"].as_bool().unwrap_or(false));
    let mut lit = 0;
    for (i, row) in rows.iter().enumerate() {
        let mut any = false;
        for (j, &(start, end)) in slots.iter().enumerate() {
            if spread.gets(i, rows.len(), j) {
                plan.add(row, layer, placed(&effect, start, end), &[])?;
                any = true;
            }
        }
        lit += usize::from(any);
    }
    let (count, removed, edits) = (plan.added.len(), plan.removed.len(), plan.edits);
    if count == 0 {
        return Err("That spread gives none of those rows an effect: add rows, or use more marks.".into());
    }
    draft.edit_sequence_batch(edits).map_err(|e| e.to_string())?;
    let replaced = if removed > 0 {
        format!(", replacing {}", plural(removed, "effect", "effects"))
    } else {
        String::new()
    };
    let label = effect.kind().label();
    Ok(format!(
        "Placed {} on {} from {} to {}{replaced} (in your draft).",
        plural(count, &format!("{label} effect"), &format!("{label} effects")),
        plural(lit, "row", "rows"),
        format_ms(from),
        format_ms(to)
    ))
}

/// `repeat_effects`: copies the effects that start in a time range to other start times, on the
/// same rows and layers (see the tool's description).
pub fn repeat(draft: &mut Draft, input: &Value) -> Result<String, String> {
    let doc = draft
        .sequence()
        .ok_or("No sequence is open. Offer ask_for_song so the user can pick a song for a new one.")?;
    known_keys(
        input,
        &["fromMs", "toMs", "startsMs", "rowIds", "replace"],
        "repeat_effects",
    )?;
    let from = time(input, "fromMs")?;
    let to = time(input, "toMs")?;
    if to <= from {
        return Err("fromMs must come before toMs.".into());
    }
    let starts: Vec<u64> = match &input["startsMs"] {
        Value::Array(items) => items
            .iter()
            .map(|v| v.as_u64().ok_or("startsMs must be a list of times in ms."))
            .collect::<Result<_, _>>()?,
        _ => return Err("startsMs must be a list of times in ms.".into()),
    };
    if starts.is_empty() || starts.len() > MAX_REPEATS {
        return Err(format!("Give between 1 and {MAX_REPEATS} start times."));
    }
    if let Some(late) = starts.iter().find(|&&s| s >= doc.duration_ms) {
        return Err(format!(
            "Nothing was copied: a start time ({late} ms) is past the end of the sequence at {}.",
            format_ms(doc.duration_ms)
        ));
    }
    let only = text_list(&input["rowIds"], "rowIds")?;
    let rows: Vec<&Row> = if only.is_empty() {
        doc.rows.iter().collect()
    } else {
        rows_named(&doc.rows, &only)?
    };
    let pattern: Vec<(&Row, usize, &Effect)> = rows
        .iter()
        .flat_map(|row| {
            row.layers.iter().enumerate().flat_map(move |(l, layer)| {
                layer
                    .effects
                    .iter()
                    .filter(move |e| e.start_ms >= from && e.start_ms < to)
                    .map(move |e| (*row, l, e))
            })
        })
        .collect();
    if pattern.is_empty() {
        return Err(format!(
            "There are no effects starting between {} and {} to copy.",
            format_ms(from),
            format_ms(to)
        ));
    }
    let keep: Vec<EffectId> = pattern.iter().map(|(_, _, e)| e.id).collect();
    let pattern_size = pattern.len();
    let mut plan = Plan::new(draft, input["replace"].as_bool().unwrap_or(false));
    for &start in &starts {
        for &(row, layer, effect) in &pattern {
            // Starts are inside the sequence (checked above), so this can't overflow.
            let begin = start.saturating_add(effect.start_ms - from);
            if begin >= doc.duration_ms {
                continue;
            }
            let end = begin.saturating_add(effect.duration_ms()).min(doc.duration_ms);
            plan.add(row, layer, placed(effect, begin, end), &keep)?;
        }
    }
    let (count, edits) = (plan.added.len(), plan.edits);
    if count == 0 {
        return Err("Every copy would start after the sequence ends.".into());
    }
    draft.edit_sequence_batch(edits).map_err(|e| e.to_string())?;
    Ok(format!(
        "Copied {} (a pattern of {}, to {}) in your draft.",
        plural(count, "effect", "effects"),
        plural(pattern_size, "effect", "effects"),
        plural(starts.len(), "start time", "start times")
    ))
}
