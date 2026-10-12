//! `change_effects`: many effects already in the sequence changed in one call, as a person does
//! with a selection in the editor: their settings, colors, and mixing; how long each lasts and
//! when; copied to other rows; moved to another row or layer; or removed. The effects are chosen
//! by id, row, time, layer, and kind. One batch of [`SequenceEdit`]s on the draft: all of it, or
//! (with the reason) none of it.

use crate::arrange::{MAX_PLACED, known_keys, plural, rows_named, text_list};
use crate::diff::target_name;
use crate::draft::Draft;
use crate::effect_spec::{self, Context};
use crate::knowledge::reference;
use pf_engine::SequenceEdit;
use pf_sequence::{Effect, EffectId, EffectKind, Row, RowId, format_ms};
use serde_json::Value;
use std::collections::BTreeSet;

/// What `change_effects` takes: which effects, then what to do with them.
pub const SELECTORS: [&str; 6] = ["ids", "rowIds", "fromMs", "toMs", "layer", "kinds"];
pub const ACTIONS: [&str; 6] = [
    "change",
    "lengthMs",
    "shiftMs",
    "copyToRowIds",
    "moveTo",
    "remove",
];

/// One effect as it will be: where it goes, and what puts it there.
struct Planned {
    row: RowId,
    layer: usize,
    effect: Effect,
    /// The effect it comes from (itself changed or moved), or `None` for a new copy.
    from: Option<EffectId>,
    edit: SequenceEdit,
}

fn whole(input: &Value, field: &str) -> Result<Option<u64>, String> {
    match &input[field] {
        Value::Null => Ok(None),
        value => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("{field} must be a whole number of milliseconds.")),
    }
}

/// `change_effects` (see the tool's description).
pub fn change(draft: &mut Draft, input: &Value) -> Result<String, String> {
    let doc = draft
        .sequence()
        .ok_or("No sequence is open. Offer ask_for_song so the user can pick a song for a new one.")?;
    let mut known: Vec<&str> = SELECTORS.to_vec();
    known.extend(ACTIONS);
    known.push("replace");
    known_keys(input, &known, "change_effects")?;

    // Which effects.
    let ids = text_list(&input["ids"], "ids")?;
    let row_ids = text_list(&input["rowIds"], "rowIds")?;
    let rows: Vec<&Row> = if row_ids.is_empty() {
        doc.rows.iter().collect()
    } else {
        rows_named(&doc.rows, &row_ids)?
    };
    let (from, to) = (whole(input, "fromMs")?, whole(input, "toMs")?);
    let layer = whole(input, "layer")?.map(|l| l as usize);
    let kinds: Vec<EffectKind> = text_list(&input["kinds"], "kinds")?
        .iter()
        .map(|k| {
            reference::kind_named(k).ok_or_else(|| {
                format!(
                    "There's no effect kind \"{k}\"; choose from {}.",
                    reference::kind_keys().join(", ")
                )
            })
        })
        .collect::<Result<_, _>>()?;
    if ids.is_empty()
        && row_ids.is_empty()
        && from.is_none()
        && to.is_none()
        && layer.is_none()
        && kinds.is_empty()
    {
        return Err(format!(
            "Say which effects: any of {} (all that you give must hold). fromMs: 0 alone is every effect.",
            SELECTORS.join(", ")
        ));
    }
    for id in &ids {
        if !doc.effects().any(|e| e.id.to_string() == *id) {
            return Err(format!(
                "There's no effect with the id \"{id}\". list_sequence_effects gives ids."
            ));
        }
    }
    let chosen: Vec<(&Row, usize, &Effect)> = rows
        .iter()
        .flat_map(|row| {
            row.layers
                .iter()
                .enumerate()
                .flat_map(move |(l, layer)| layer.effects.iter().map(move |e| (*row, l, e)))
        })
        .filter(|(_, l, e)| {
            (ids.is_empty() || ids.iter().any(|id| e.id.to_string() == *id))
                && from.is_none_or(|t| e.start_ms >= t)
                && to.is_none_or(|t| e.start_ms < t)
                && layer.is_none_or(|wanted| *l == wanted)
                && (kinds.is_empty() || kinds.contains(&e.kind()))
        })
        .collect();
    if chosen.is_empty() {
        return Err("No effects match that: list_sequence_effects shows what's there (toMs is where an effect starts before, not where it ends).".into());
    }
    if chosen.len() > MAX_PLACED {
        return Err(format!(
            "That's {} effects, more than {MAX_PLACED} in one call: narrow it (by row, or a shorter time).",
            chosen.len()
        ));
    }

    // What to do with them.
    let given = |field: &str| !input[field].is_null();
    let remove = input["remove"].as_bool().unwrap_or(false);
    let reshaping = ["change", "lengthMs", "shiftMs"].iter().any(|f| given(f));
    let copying = given("copyToRowIds");
    let moving = given("moveTo");
    if [remove, reshaping, copying, moving]
        .iter()
        .filter(|x| **x)
        .count()
        != 1
    {
        return Err(format!(
            "Give one thing to do: change, lengthMs, and shiftMs (any of them together), or copyToRowIds, or moveTo, or remove: true. ({} match.)",
            plural(chosen.len(), "effect", "effects")
        ));
    }
    let replace = input["replace"].as_bool().unwrap_or(false);
    let touched: BTreeSet<RowId> = chosen.iter().map(|(row, _, _)| row.id).collect();
    let on_rows = plural(touched.len(), "row", "rows");
    let count = plural(chosen.len(), "effect", "effects");
    if remove {
        let edits = chosen
            .iter()
            .map(|(_, _, e)| SequenceEdit::RemoveEffect { id: e.id })
            .collect();
        draft.edit_sequence_batch(edits).map_err(|e| e.to_string())?;
        return Ok(format!("Removed {count} from {on_rows} (in your draft)."));
    }

    let cx = Context {
        tracks: &doc.timing_tracks,
        pictures: Some(&draft.base().images),
    };
    let duration = doc.duration_ms;
    let mut planned: Vec<Planned> = Vec::new();
    let said = if reshaping {
        let length = whole(input, "lengthMs")?;
        if length == Some(0) {
            return Err("lengthMs must be more than 0.".into());
        }
        let shift = match &input["shiftMs"] {
            Value::Null => 0,
            value => value
                .as_i64()
                .ok_or("shiftMs must be a whole number of milliseconds (negative is earlier).")?,
        };
        for (row, l, effect) in &chosen {
            let mut next = match &input["change"] {
                Value::Null => (*effect).clone(),
                changes => effect_spec::changed(effect, changes, &cx).map_err(|e| {
                    format!(
                        "Nothing was changed: the {} effect on {} at {}: {e}",
                        effect.kind().label(),
                        target_name(draft.show(), row.target),
                        format_ms(effect.start_ms)
                    )
                })?,
            };
            let start = i128::from(effect.start_ms) + i128::from(shift);
            if start < 0 {
                return Err(format!(
                    "Nothing was changed: that would start an effect before the sequence does (the one at {}).",
                    format_ms(effect.start_ms)
                ));
            }
            next.start_ms = start as u64;
            next.end_ms = next.start_ms + length.unwrap_or(effect.duration_ms());
            let retimed = (next.start_ms, next.end_ms) != (effect.start_ms, effect.end_ms);
            let edit = if input["change"].is_null() {
                SequenceEdit::SetEffectTiming {
                    id: effect.id,
                    start_ms: next.start_ms,
                    end_ms: next.end_ms,
                }
            } else {
                SequenceEdit::UpdateEffect { effect: next.clone() }
            };
            if retimed || !input["change"].is_null() {
                planned.push(Planned {
                    row: row.id,
                    layer: *l,
                    effect: next,
                    from: Some(effect.id),
                    edit,
                });
            }
        }
        format!("Changed {count} on {on_rows}")
    } else if copying {
        let targets = text_list(&input["copyToRowIds"], "copyToRowIds")?;
        let targets = rows_named(&doc.rows, &targets)?;
        if targets.is_empty() {
            return Err("Give at least one row in copyToRowIds.".into());
        }
        for target in &targets {
            for (row, l, effect) in &chosen {
                if row.id == target.id {
                    continue;
                }
                let mut copy = (*effect).clone();
                copy.id = EffectId::new();
                planned.push(Planned {
                    row: target.id,
                    layer: *l,
                    edit: SequenceEdit::AddEffect {
                        row: target.id,
                        layer: *l,
                        effect: copy.clone(),
                    },
                    effect: copy,
                    from: None,
                });
            }
        }
        format!("Copied {count} to {}", plural(targets.len(), "row", "rows"))
    } else {
        let to = &input["moveTo"];
        known_keys(to, &["rowId", "layer"], "moveTo")?;
        let target = match to["rowId"].as_str() {
            Some(id) => Some(rows_named(&doc.rows, &[id.to_string()])?[0]),
            None => None,
        };
        let to_layer = whole(to, "layer")?.map(|l| l as usize);
        if target.is_none() && to_layer.is_none() {
            return Err("moveTo needs a rowId, a layer, or both.".into());
        }
        for (row, l, effect) in &chosen {
            let (row, layer) = (target.unwrap_or(row), to_layer.unwrap_or(*l));
            planned.push(Planned {
                row: row.id,
                layer,
                effect: (*effect).clone(),
                from: Some(effect.id),
                edit: SequenceEdit::MoveEffect {
                    id: effect.id,
                    row: row.id,
                    layer,
                    start_ms: effect.start_ms,
                    end_ms: effect.end_ms,
                },
            });
        }
        format!("Moved {count}")
    };
    if planned.is_empty() {
        return Err("That changes nothing: those effects are already that way.".into());
    }

    // Each lands inside the sequence, on a layer its row has (or one new one), clear of the
    // effects that stay and of each other.
    let moving_ids: BTreeSet<EffectId> = planned.iter().filter_map(|p| p.from).collect();
    let mut edits: Vec<SequenceEdit> = Vec::new();
    let mut in_the_way: BTreeSet<EffectId> = BTreeSet::new();
    for (i, p) in planned.iter().enumerate() {
        let row = doc.row(p.row).ok_or("That row isn't in the sequence anymore.")?;
        let name = target_name(draft.show(), row.target);
        if p.effect.start_ms >= duration {
            return Err(format!(
                "Nothing was changed: that would put an effect on {name} at {}, past the end of the sequence at {}.",
                format_ms(p.effect.start_ms),
                format_ms(duration)
            ));
        }
        if p.layer > row.layers.len() {
            return Err(format!(
                "{name} has {}; layer can be at most {} (a new layer on top).",
                plural(row.layers.len(), "layer", "layers"),
                row.layers.len()
            ));
        }
        let clash = |start: u64, end: u64| {
            format!(
                "Nothing was changed: {name} already has an effect on layer {} there ({}–{}). Use another layer, or replace: true to take its place.",
                p.layer,
                format_ms(start),
                format_ms(end)
            )
        };
        let staying = row
            .layers
            .get(p.layer)
            .into_iter()
            .flat_map(|l| &l.effects)
            .filter(|e| !moving_ids.contains(&e.id) && e.overlaps(&p.effect));
        for other in staying {
            if !replace {
                return Err(clash(other.start_ms, other.end_ms));
            }
            in_the_way.insert(other.id);
        }
        if let Some(other) = planned[..i]
            .iter()
            .find(|o| o.row == p.row && o.layer == p.layer && o.effect.overlaps(&p.effect))
        {
            return Err(format!(
                "Nothing was changed: two of those effects would overlap on {name}'s layer {} ({}–{}).",
                p.layer,
                format_ms(other.effect.start_ms),
                format_ms(other.effect.end_ms)
            ));
        }
    }
    edits.extend(in_the_way.iter().map(|id| SequenceEdit::RemoveEffect { id: *id }));
    let replaced = in_the_way.len();
    // An effect past the end is cut to it.
    edits.extend(planned.into_iter().map(|p| match p.edit {
        SequenceEdit::SetEffectTiming { id, start_ms, end_ms } => SequenceEdit::SetEffectTiming {
            id,
            start_ms,
            end_ms: end_ms.min(duration),
        },
        SequenceEdit::UpdateEffect { mut effect } => {
            effect.end_ms = effect.end_ms.min(duration);
            SequenceEdit::UpdateEffect { effect }
        }
        other => other,
    }));
    draft.edit_sequence_batch(edits).map_err(|e| e.to_string())?;
    let replacing = if replaced > 0 {
        format!(", replacing {}", plural(replaced, "effect", "effects"))
    } else {
        String::new()
    };
    Ok(format!("{said}{replacing} (in your draft)."))
}
