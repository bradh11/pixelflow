//! Many effects in one tool call, so a whole song fits in a reasonable number of steps:
//! `place_effects` puts one effect on many rows over a time range, cut at a timing track's
//! marks (all of them, or those whose label matches: a sung word or phrase) and spread across
//! the rows; `repeat_effects` copies a stretch of effects to other times. Each is one batch of [`SequenceEdit`]s on the draft: all of it, or (with the reason)
//! none of it.
//!
//! Still words placed on a track are held for as long as they take to read, by the rule the
//! review reads them by; given no text of their own, they say the words sung at each mark,
//! short ones grouped so each can be read.

use crate::diff::target_name;
use crate::draft::Draft;
use crate::song::find_track;
use pf_engine::SequenceEdit;
use pf_sequence::{
    Effect, EffectId, EffectKind, EffectParams, Mark, Row, RowId, TextCountdown, TextMovement, TimingKind,
    TimingTrack, format_ms,
};
use serde_json::Value;
use std::collections::BTreeSet;

/// Effects one call may add, at most.
pub const MAX_PLACED: usize = 2_000;
/// Copies one `repeat_effects` call may make, at most.
const MAX_REPEATS: usize = 200;
/// The longest `match` taken.
const MAX_MATCH_CHARS: usize = 200;

/// The lowercase words of a label, letters and digits only ("Hang ya bells up?" is hang, ya,
/// bells, up).
fn label_words(label: &str) -> Vec<String> {
    label
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// The marks matching `pattern`, each as the first and last mark it covers (by index into
/// `marks`). Plain words match whole words in any case, punctuation aside, and several in a
/// row match words in a row, across marks too ("hang ya bells up" on a words track covers
/// four marks); `|` separates choices. Anything else with regex characters is a
/// case-insensitive regular expression tried on each label.
pub fn matching(marks: &[&Mark], pattern: &str) -> Result<Vec<(usize, usize)>, String> {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern.chars().count() > MAX_MATCH_CHARS {
        return Err(format!(
            "match must be a word, a phrase, or a pattern of up to {MAX_MATCH_CHARS} characters."
        ));
    }
    if pattern.contains(['\\', '^', '$', '.', '*', '+', '?', '(', ')', '[', ']', '{', '}']) {
        let regex = regex::RegexBuilder::new(pattern)
            .case_insensitive(true)
            .size_limit(1 << 20)
            .build()
            .map_err(|_| {
                format!("\"{pattern}\" isn't a pattern PixelFlow can use; give a word or a phrase.")
            })?;
        return Ok((0..marks.len())
            .filter(|&k| regex.is_match(&marks[k].label))
            .map(|k| (k, k))
            .collect());
    }
    // Every word of every mark, with the mark it's in.
    let words: Vec<(String, usize)> = marks
        .iter()
        .enumerate()
        .flat_map(|(k, m)| label_words(&m.label).into_iter().map(move |w| (w, k)))
        .collect();
    let mut found: Vec<(usize, usize)> = Vec::new();
    for choice in pattern.split('|') {
        let wanted = label_words(choice);
        if wanted.is_empty() || wanted.len() > words.len() {
            continue;
        }
        for at in 0..=words.len() - wanted.len() {
            if words[at..at + wanted.len()]
                .iter()
                .zip(&wanted)
                .all(|((w, _), x)| w == x)
            {
                found.push((words[at].1, words[at + wanted.len() - 1].1));
            }
        }
    }
    found.sort_unstable();
    found.dedup();
    Ok(found)
}

/// The track `place_effects` cuts at: `track`, or, to match words on a lyrics track, its words
/// track ("Lyrics (words)" beside "Lyrics") when there is one.
fn cut_track<'a>(tracks: &'a [TimingTrack], track: &'a TimingTrack, matching: bool) -> &'a TimingTrack {
    if !matching || track.kind != TimingKind::Lyrics {
        return track;
    }
    let words = format!("{} (words)", track.name);
    tracks
        .iter()
        .find(|t| t.kind == TimingKind::Words && t.name == words && !t.marks.is_empty())
        .unwrap_or(track)
}

/// The words a syllables track's syllables belong to ("Lyrics (words)" beside "Lyrics
/// (syllables)"), else any words track.
fn syllables_words<'a>(tracks: &'a [TimingTrack], syllables: &TimingTrack) -> Option<&'a TimingTrack> {
    let base = syllables.name.strip_suffix(" (syllables)")?;
    let words = format!("{base} (words)");
    tracks
        .iter()
        .find(|t| t.kind == TimingKind::Words && t.name == words && !t.marks.is_empty())
        .or_else(|| crate::lyrics::tracks::words_track(tracks))
}

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

pub(crate) fn text_list(value: &Value, what: &str) -> Result<Vec<String>, String> {
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
pub(crate) fn known_keys(value: &Value, known: &[&str], what: &str) -> Result<(), String> {
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

pub(crate) fn rows_named<'a>(rows: &'a [Row], ids: &[String]) -> Result<Vec<&'a Row>, String> {
    ids.iter()
        .map(|id| {
            rows.iter().find(|r| r.id.to_string() == *id).ok_or_else(|| {
                format!("There's no row with the id \"{id}\". get_open_sequence lists the rows.")
            })
        })
        .collect()
}

pub(crate) fn plural(n: usize, one: &str, many: &str) -> String {
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
            "match",
            "marksEach",
            "spread",
            "layer",
            "replace",
        ],
        "place_effects",
    )?;
    let effect = crate::effect_spec::parse(
        &input["effect"],
        &crate::effect_spec::Context {
            tracks: &doc.timing_tracks,
            pictures: Some(&draft.base().images),
        },
    )?;
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
    let pattern = match &input["match"] {
        Value::Null => None,
        Value::String(text) => Some(text.as_str()),
        _ => return Err("match must be text: a word, a phrase, or a pattern.".into()),
    };
    // Words cut at a track with no text of their own say the words of the marks they're cut at.
    let says_marks = effect.kind() == EffectKind::Text
        && input["track"].is_string()
        && input["effect"]["settings"]["text"].is_null();
    let mut cut_at = None;
    // Where the next mark after each slot starts: still words may stay up until then.
    let mut next_mark: Vec<Option<u64>> = Vec::new();
    // Each slot: when, and the words of the marks it covers.
    let slots: Vec<Slot> = match input["track"].as_str() {
        None if pattern.is_some() => {
            return Err("match needs a track whose marks to match, like \"Lyrics\".".into());
        }
        None => vec![(from, to, String::new())],
        Some(key) => {
            let track = find_track(&doc.timing_tracks, key).ok_or_else(|| {
                format!("There's no timing track \"{key}\". get_open_sequence lists them; add_song_timing makes Beats, Bars, Sections, and syllables.")
            })?;
            let track = cut_track(&doc.timing_tracks, track, pattern.is_some() || says_marks);
            cut_at = Some(track.id);
            let each = input["marksEach"].as_u64().unwrap_or(1).clamp(1, 10_000) as usize;
            fn starting_in(t: &TimingTrack, from: u64, to: u64) -> Vec<&Mark> {
                t.marks
                    .iter()
                    .filter(|m| m.start_ms >= from && m.start_ms < to)
                    .collect()
            }
            let within = |t| starting_in(t, from, to);
            let marks = within(track);
            let matched = |marks: &[&Mark], pattern: &str| -> Result<Vec<Slot>, String> {
                Ok(matching(marks, pattern)?
                    .into_iter()
                    .map(|(first, last)| {
                        (
                            marks[first].start_ms,
                            marks[last].end_ms,
                            words_of(&marks[first..=last]),
                        )
                    })
                    .collect())
            };
            let words = crate::lyrics::tracks::is_syllables(track)
                .then(|| syllables_words(&doc.timing_tracks, track))
                .flatten();
            let spans: Vec<Slot> = match (pattern, words) {
                // Words matched on the words track, each of their syllables a mark.
                (Some(pattern), Some(words)) => {
                    let sung = matched(&within(words), pattern)?;
                    marks
                        .iter()
                        .filter(|m| sung.iter().any(|(s, e, _)| m.start_ms >= *s && m.start_ms < *e))
                        .map(|m| (m.start_ms, m.end_ms, words_of(&[m])))
                        .collect()
                }
                (Some(pattern), None) => matched(&marks, pattern)?,
                (None, _) => marks
                    .iter()
                    .map(|m| (m.start_ms, m.end_ms, words_of(&[m])))
                    .collect(),
            };
            let slots: Vec<Slot> = spans
                .chunks(each)
                .filter_map(|chunk| {
                    let start = chunk.first()?.0;
                    let end = chunk.last()?.1.min(to);
                    let words: Vec<&str> = chunk
                        .iter()
                        .map(|s| s.2.as_str())
                        .filter(|w| !w.is_empty())
                        .collect();
                    (end > start).then(|| (start, end, words.join(" ")))
                })
                .collect();
            if slots.is_empty() {
                let matched = pattern.map_or(String::new(), |p| format!(" match \"{p}\" and"));
                return Err(format!(
                    "No marks of \"{}\"{matched} start between {} and {}.",
                    track.name,
                    format_ms(from),
                    format_ms(to)
                ));
            }
            next_mark = slots
                .iter()
                .map(|s| track.marks.iter().map(|m| m.start_ms).filter(|&t| t >= s.1).min())
                .collect();
            slots
        }
    };
    let still = match &effect.params {
        EffectParams::Text(text) => {
            text.movement == TextMovement::None && text.countdown == TextCountdown::None
        }
        _ => false,
    };
    let spread = Spread::parse(input["spread"].as_str())?;
    let layer = input["layer"].as_u64().unwrap_or(0) as usize;
    let mut plan = Plan::new(draft, input["replace"].as_bool().unwrap_or(false));
    let mut lit = 0;
    // Still words up for less time than their letters take to read: when, what, how long.
    let mut brief: BTreeSet<(u64, String, u64)> = BTreeSet::new();
    for (i, row) in rows.iter().enumerate() {
        let pieces = if still && cut_at.is_some() {
            still_words(draft.show(), row, &effect, says_marks, &slots, &next_mark, to)
        } else {
            slots.clone()
        };
        let mut any = false;
        for (j, (start, end, words)) in pieces.into_iter().enumerate() {
            if !spread.gets(i, rows.len(), j) {
                continue;
            }
            let mut one = placed(&effect, start, end);
            if let EffectParams::Text(text) = &mut one.params {
                if says_marks {
                    text.text = words.to_uppercase();
                }
                if still && crate::review::read_ms(&text.text) > end - start {
                    brief.insert((start, text.text.replace('\n', ""), end - start));
                }
            }
            plan.add(row, layer, one, &[])?;
            any = true;
        }
        lit += usize::from(any);
    }
    let (count, removed, edits) = (plan.added.len(), plan.removed.len(), plan.edits);
    if count == 0 {
        return Err("That spread gives none of those rows an effect: add rows, or use more marks.".into());
    }
    draft.edit_sequence_batch(edits).map_err(|e| e.to_string())?;
    if let Some(id) = cut_at {
        draft.placed_on(id);
    }
    let replaced = if removed > 0 {
        format!(", replacing {}", plural(removed, "effect", "effects"))
    } else {
        String::new()
    };
    let label = effect.kind().label();
    let mut out = format!(
        "Placed {} on {} from {} to {}{replaced} (in your draft).",
        plural(count, &format!("{label} effect"), &format!("{label} effects")),
        plural(lit, "row", "rows"),
        format_ms(from),
        format_ms(to)
    );
    if says_marks {
        out.push_str(if still {
            " Each says the words sung then, up for as long as they take to read (300 ms and 60 ms a letter): short words share the grid, and a word too wide for it is cut over two lines."
        } else {
            " Each says the words of its mark."
        });
    }
    if let Some((_, text, ms)) = brief.first() {
        let wanted = crate::review::read_ms(text);
        out.push_str(&format!(
            " {} up for less time than it takes to read (\"{text}\": {ms} ms, {wanted} wanted, 300 ms and 60 ms a letter). Leave out text to have each effect say the words sung then, grouped and held so each can be read; or show fewer words for longer.",
            plural(brief.len(), "of these is", "of these are")
        ));
    }
    Ok(out)
}

/// A slot effects are placed in: its start, its end, and the words of the marks it covers.
type Slot = (u64, u64, String);

/// The words of `marks`, in a line.
fn words_of(marks: &[&Mark]) -> String {
    marks
        .iter()
        .map(|m| m.label.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A gap this short between still words and the next mark is bridged rather than left dark.
const BRIDGE_MS: u64 = 250;

/// The still words `row` shows at `slots`: with `says_marks`, the sung words grouped and held so
/// each can be read on the row's grid, by the rule the review reads them by; otherwise each
/// slot, held for as long as its words take to read when there's time before the next mark
/// (`next_mark`, and `to`). A short gap before the next mark is bridged.
fn still_words(
    show: &pf_model::Show,
    row: &Row,
    effect: &Effect,
    says_marks: bool,
    slots: &[Slot],
    next_mark: &[Option<u64>],
    to: u64,
) -> Vec<Slot> {
    let EffectParams::Text(text) = &effect.params else {
        return slots.to_vec();
    };
    let grid = match row.target {
        pf_sequence::Target::Prop(id) => show
            .props
            .iter()
            .find(|p| p.id == id)
            .and_then(crate::review::text_grid),
        _ => None,
    };
    if says_marks && let Some((columns, rows)) = grid {
        return crate::review::sung_words(slots, to, text.size, columns, rows);
    }
    slots
        .iter()
        .enumerate()
        .map(|(k, (start, end, words))| {
            let shown = if says_marks {
                words.to_uppercase()
            } else {
                text.text.clone()
            };
            let room = next_mark
                .get(k)
                .copied()
                .flatten()
                .unwrap_or(to)
                .min(to)
                .max(*end);
            let mut stop = (*end).max(start + crate::review::read_ms(&shown)).min(room);
            if room - stop < BRIDGE_MS {
                stop = room;
            }
            (*start, stop, words.clone())
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn matched(labels: &[&str], pattern: &str) -> Result<Vec<(usize, usize)>, String> {
        let marks: Vec<Mark> = labels
            .iter()
            .enumerate()
            .map(|(i, l)| Mark::new(i as u64 * 1_000, i as u64 * 1_000 + 900, *l))
            .collect();
        let refs: Vec<&Mark> = marks.iter().collect();
        matching(&refs, pattern)
    }

    #[test]
    fn plain_words_match_whole_words_in_any_case() {
        let lines = [
            "Paper lanterns glowing",
            "Lantern light!",
            "Snowy rooftops",
            "LANTERNS, lanterns",
        ];
        assert_eq!(matched(&lines, "lanterns"), Ok(vec![(0, 0), (3, 3)]));
        assert_eq!(matched(&lines, "Lantern"), Ok(vec![(1, 1)]));
        // Choices.
        assert_eq!(matched(&lines, "snowy|light"), Ok(vec![(1, 1), (2, 2)]));
        assert_eq!(matched(&lines, "chimney"), Ok(vec![]));
    }

    #[test]
    fn a_phrase_covers_words_in_a_row_across_marks() {
        let words = [
            "Hang",
            "ya",
            "bells",
            "up?",
            "Lanterns!",
            "hang",
            "ya",
            "bells",
            "up",
        ];
        assert_eq!(matched(&words, "hang ya bells up"), Ok(vec![(0, 3), (5, 8)]));
        assert_eq!(matched(&words, "lanterns"), Ok(vec![(4, 4)]));
        // Apostrophes don't split a word.
        assert_eq!(matched(&["Shinin' bright"], "shinin"), Ok(vec![(0, 0)]));
    }

    #[test]
    fn patterns_are_regular_expressions_and_bad_ones_are_explained() {
        let words = ["Lantern", "lanterns", "glow", "glowing"];
        assert_eq!(matched(&words, "^lanterns?$"), Ok(vec![(0, 0), (1, 1)]));
        assert_eq!(matched(&words, "glow.*"), Ok(vec![(2, 2), (3, 3)]));
        assert!(
            matched(&words, "(unclosed")
                .unwrap_err()
                .contains("isn't a pattern")
        );
        assert!(matched(&words, "  ").is_err());
        assert!(matched(&words, &"x".repeat(300)).is_err());
    }
}
