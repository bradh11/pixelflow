//! The open sequence document: edits with their own undo history, files on disk, and what the
//! UI sees after every change. It lives alongside the show (a sequence refers to the show's
//! props and groups but is saved in its own file).

use crate::error::EngineError;
use crate::persist::write_atomic;
use pf_model::Show;
use pf_sequence::{
    Effect, EffectId, EffectParams, MAX_SEQUENCE_BYTES, Mark, Row, RowId, Sequence, SequenceIssue,
    TimingKind, TimingTrack, TimingTrackId,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Undo steps kept for a sequence.
const UNDO_LIMIT: usize = 200;
/// Approximate memory the sequence undo stack may hold.
const UNDO_BYTE_BUDGET: usize = 256 * 1024 * 1024;

/// One change to the open sequence. Batches are applied atomically by
/// [`crate::Engine::edit_sequence`] as one undo step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SequenceEdit {
    /// Name, music, length, and frame time.
    UpdateInfo {
        name: String,
        audio: Option<String>,
        duration_ms: u64,
        frame_ms: u32,
    },
    /// Adds a row at `index` (or at the end).
    AddRow {
        row: Row,
        #[serde(default)]
        index: Option<usize>,
    },
    RemoveRow {
        id: RowId,
    },
    /// Moves a row to `index` (clamped to the end).
    MoveRow {
        id: RowId,
        index: usize,
    },
    /// Adds an empty layer to a row at `index` (or on top).
    AddLayer {
        row: RowId,
        #[serde(default)]
        index: Option<usize>,
    },
    /// Removes a layer and its effects.
    RemoveLayer {
        row: RowId,
        layer: usize,
    },
    /// Adds an effect to a layer; `layer` may be one past the top to start a new layer.
    AddEffect {
        row: RowId,
        layer: usize,
        effect: Effect,
    },
    /// Replaces the effect with the same id (settings, palette, blend, fades, timing).
    UpdateEffect {
        effect: Effect,
    },
    /// Moves or resizes an effect in time.
    SetEffectTiming {
        id: EffectId,
        start_ms: u64,
        end_ms: u64,
    },
    SetEffectParams {
        id: EffectId,
        params: EffectParams,
    },
    /// Moves an effect to another row or layer (and time).
    MoveEffect {
        id: EffectId,
        row: RowId,
        layer: usize,
        start_ms: u64,
        end_ms: u64,
    },
    RemoveEffect {
        id: EffectId,
    },
    AddTimingTrack {
        track: TimingTrack,
    },
    /// Replaces the timing track with the same id (name, kind, marks).
    UpdateTimingTrack {
        track: TimingTrack,
    },
    RemoveTimingTrack {
        id: TimingTrackId,
    },
    RenameTimingTrack {
        id: TimingTrackId,
        name: String,
    },
    /// Moves a timing track to `index` (clamped to the end).
    MoveTimingTrack {
        id: TimingTrackId,
        index: usize,
    },
    /// Adds marks to a track, each where it belongs in time. Marks on a track never overlap.
    AddMarks {
        track: TimingTrackId,
        marks: Vec<Mark>,
    },
    /// Replaces the mark at `index` (moves it, resizes it, or changes its label).
    SetMark {
        track: TimingTrackId,
        index: usize,
        mark: Mark,
    },
    RemoveMarks {
        track: TimingTrackId,
        indices: Vec<usize>,
    },
    /// Splits the mark at `index` in two at `at_ms`; the first part keeps the label.
    SplitMark {
        track: TimingTrackId,
        index: usize,
        at_ms: u64,
    },
    /// Joins the mark at `index` with the one after it (their labels joined with a space).
    MergeMarks {
        track: TimingTrackId,
        index: usize,
    },
    /// A mark every `every_ms` from `from_ms` to `to_ms`, replacing the marks there.
    GenerateMarks {
        track: TimingTrackId,
        every_ms: u64,
        from_ms: u64,
        to_ms: u64,
    },
    /// Replaces a track's marks with every `every`th mark of `from` (1 = a copy).
    CopyMarks {
        from: TimingTrackId,
        to: TimingTrackId,
        every: usize,
    },
    /// One mark per line of lyrics, spread over `from_ms..to_ms` by letter count, replacing the
    /// marks there.
    SpreadLyrics {
        track: TimingTrackId,
        lines: Vec<String>,
        from_ms: u64,
        to_ms: u64,
    },
    /// Labels the marks at `indices` with `labels`, in order (as many labels as marks).
    LabelMarks {
        track: TimingTrackId,
        indices: Vec<usize>,
        labels: Vec<String>,
    },
    /// Breaks each phrase mark at `indices` of `track` into word marks on `words` (by letter
    /// count), replacing the marks there under each phrase.
    BreakIntoWords {
        track: TimingTrackId,
        indices: Vec<usize>,
        words: TimingTrackId,
    },
}

/// Something an edit names is gone (the UI's copy is out of date, or it was just removed).
fn not_found(kind: &'static str) -> EngineError {
    EngineError::InvalidEdit(format!("That {kind} isn't in the sequence anymore."))
}

fn check_timing(start_ms: u64, end_ms: u64) -> Result<(), EngineError> {
    if end_ms <= start_ms {
        return Err(EngineError::InvalidEdit(
            "An effect must end after it starts.".to_string(),
        ));
    }
    Ok(())
}

fn track_index(doc: &Sequence, id: TimingTrackId) -> Result<usize, EngineError> {
    doc.timing_tracks
        .iter()
        .position(|t| t.id == id)
        .ok_or_else(|| not_found("timing track"))
}

/// A timing track whose marks may be changed (phonemes are made from words as a whole, never mark
/// by mark).
fn marks_mut(doc: &mut Sequence, id: TimingTrackId) -> Result<&mut TimingTrack, EngineError> {
    let at = track_index(doc, id)?;
    let track = &mut doc.timing_tracks[at];
    if track.kind == TimingKind::Phonemes {
        return Err(invalid(
            "Phoneme tracks can't be edited mark by mark; edit the words, then use Break into syllables on the words track.".to_string(),
        ));
    }
    Ok(track)
}

fn mark_gone() -> EngineError {
    invalid("That mark isn't on the timing track anymore.".to_string())
}

fn invalid(message: String) -> EngineError {
    EngineError::InvalidEdit(message)
}

/// "1 thing" or "3 things".
fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// Refused when something would end after the sequence does (at `duration_ms`).
fn check_inside(end_ms: u64, duration_ms: u64) -> Result<(), EngineError> {
    if end_ms > duration_ms {
        return Err(invalid(format!(
            "That would run past the end of the sequence at {}.",
            pf_sequence::format_ms(duration_ms)
        )));
    }
    Ok(())
}

/// Puts `mark` in place of the mark at `index`, keeping the track in order; refused when it has
/// no length, would overlap another mark, or would newly run past the end (at `duration_ms`).
fn set_mark(track: &mut TimingTrack, index: usize, mark: &Mark, duration_ms: u64) -> Result<(), EngineError> {
    let Some(old) = track.marks.get(index) else {
        return Err(mark_gone());
    };
    pf_sequence::check_mark(mark).map_err(invalid)?;
    // A mark already past the end (the sequence was shortened) can still be relabelled.
    if mark.end_ms > old.end_ms {
        check_inside(mark.end_ms, duration_ms)?;
    }
    if let Some(other) = track.overlap_with(mark, &[index]) {
        return Err(invalid(track.overlap_message(&track.marks[other])));
    }
    track.marks.remove(index);
    let at = track.insert_index(mark.start_ms);
    track.marks.insert(at, mark.clone());
    Ok(())
}

/// Replaces the marks of `track` that share time with `from..to` with `marks`.
fn replace_range(
    track: &mut TimingTrack,
    from_ms: u64,
    to_ms: u64,
    marks: &[Mark],
) -> Result<(), EngineError> {
    track.clear_range(from_ms, to_ms);
    track.add_marks(marks).map_err(invalid)
}

fn row_mut(doc: &mut Sequence, id: RowId) -> Result<&mut Row, EngineError> {
    doc.row_mut(id).ok_or_else(|| not_found("row"))
}

fn effect_mut(doc: &mut Sequence, id: EffectId) -> Result<&mut Effect, EngineError> {
    let (r, l, e) = doc.locate_effect(id).ok_or_else(|| not_found("effect"))?;
    Ok(&mut doc.rows[r].layers[l].effects[e])
}

fn no_layer(layer: usize) -> EngineError {
    EngineError::InvalidEdit(format!("That row has no layer {}.", layer + 1))
}

fn layer_mut(row: &mut Row, layer: usize) -> Result<&mut pf_sequence::Layer, EngineError> {
    if layer == row.layers.len() {
        row.layers.push(pf_sequence::Layer::default());
    }
    row.layers.get_mut(layer).ok_or_else(|| no_layer(layer))
}

impl SequenceEdit {
    pub(crate) fn apply(&self, doc: &mut Sequence) -> Result<(), EngineError> {
        match self {
            SequenceEdit::UpdateInfo {
                name,
                audio,
                duration_ms,
                frame_ms,
            } => {
                doc.name = name.clone();
                doc.audio = audio.clone();
                doc.duration_ms = *duration_ms;
                doc.frame_ms = *frame_ms;
            }
            SequenceEdit::AddRow { row, index } => {
                if doc.row(row.id).is_some() {
                    return Err(EngineError::DuplicateId { kind: "row" });
                }
                let at = index.unwrap_or(doc.rows.len()).min(doc.rows.len());
                doc.rows.insert(at, row.clone());
            }
            SequenceEdit::RemoveRow { id } => {
                let at = doc
                    .rows
                    .iter()
                    .position(|r| r.id == *id)
                    .ok_or_else(|| not_found("row"))?;
                doc.rows.remove(at);
            }
            SequenceEdit::MoveRow { id, index } => {
                let at = doc
                    .rows
                    .iter()
                    .position(|r| r.id == *id)
                    .ok_or_else(|| not_found("row"))?;
                let row = doc.rows.remove(at);
                let to = (*index).min(doc.rows.len());
                doc.rows.insert(to, row);
            }
            SequenceEdit::AddLayer { row, index } => {
                let row = row_mut(doc, *row)?;
                let at = index.unwrap_or(row.layers.len()).min(row.layers.len());
                row.layers.insert(at, pf_sequence::Layer::default());
            }
            SequenceEdit::RemoveLayer { row, layer } => {
                let row = row_mut(doc, *row)?;
                if *layer >= row.layers.len() {
                    return Err(no_layer(*layer));
                }
                row.layers.remove(*layer);
            }
            SequenceEdit::AddEffect { row, layer, effect } => {
                check_timing(effect.start_ms, effect.end_ms)?;
                if doc.locate_effect(effect.id).is_some() {
                    return Err(EngineError::InvalidEdit(
                        "An effect with that id already exists.".to_string(),
                    ));
                }
                let row = row_mut(doc, *row)?;
                layer_mut(row, *layer)?.effects.push(effect.clone());
            }
            SequenceEdit::UpdateEffect { effect } => {
                check_timing(effect.start_ms, effect.end_ms)?;
                *effect_mut(doc, effect.id)? = effect.clone();
            }
            SequenceEdit::SetEffectTiming { id, start_ms, end_ms } => {
                check_timing(*start_ms, *end_ms)?;
                let effect = effect_mut(doc, *id)?;
                effect.start_ms = *start_ms;
                effect.end_ms = *end_ms;
            }
            SequenceEdit::SetEffectParams { id, params } => {
                effect_mut(doc, *id)?.params = params.clone();
            }
            SequenceEdit::MoveEffect {
                id,
                row,
                layer,
                start_ms,
                end_ms,
            } => {
                check_timing(*start_ms, *end_ms)?;
                // Check the destination before taking the effect out, so a failed move changes nothing.
                let target = doc.row(*row).ok_or_else(|| not_found("row"))?;
                if *layer > target.layers.len() {
                    return Err(no_layer(*layer));
                }
                let (r, l, e) = doc.locate_effect(*id).ok_or_else(|| not_found("effect"))?;
                let mut effect = doc.rows[r].layers[l].effects.remove(e);
                effect.start_ms = *start_ms;
                effect.end_ms = *end_ms;
                let row = row_mut(doc, *row)?;
                layer_mut(row, *layer)?.effects.push(effect);
            }
            SequenceEdit::RemoveEffect { id } => {
                let (r, l, e) = doc.locate_effect(*id).ok_or_else(|| not_found("effect"))?;
                doc.rows[r].layers[l].effects.remove(e);
            }
            SequenceEdit::AddTimingTrack { track } => {
                track.check_marks().map_err(invalid)?;
                if doc.timing_track(track.id).is_some() {
                    return Err(EngineError::DuplicateId { kind: "timing track" });
                }
                doc.timing_tracks.push(track.clone());
            }
            SequenceEdit::UpdateTimingTrack { track } => {
                track.check_marks().map_err(invalid)?;
                let existing = doc
                    .timing_tracks
                    .iter_mut()
                    .find(|t| t.id == track.id)
                    .ok_or_else(|| not_found("timing track"))?;
                *existing = track.clone();
            }
            SequenceEdit::RemoveTimingTrack { id } => {
                let at = track_index(doc, *id)?;
                doc.timing_tracks.remove(at);
            }
            SequenceEdit::RenameTimingTrack { id, name } => {
                let name = name.trim();
                if name.is_empty() {
                    return Err(invalid("A timing track needs a name.".to_string()));
                }
                let at = track_index(doc, *id)?;
                doc.timing_tracks[at].name = name.to_string();
            }
            SequenceEdit::MoveTimingTrack { id, index } => {
                let at = track_index(doc, *id)?;
                let track = doc.timing_tracks.remove(at);
                let to = (*index).min(doc.timing_tracks.len());
                doc.timing_tracks.insert(to, track);
            }
            SequenceEdit::AddMarks { track, marks } => {
                let duration_ms = doc.duration_ms;
                let track = marks_mut(doc, *track)?;
                if let Some(end) = marks.iter().map(|m| m.end_ms).max() {
                    check_inside(end, duration_ms)?;
                }
                track.add_marks(marks).map_err(invalid)?;
            }
            SequenceEdit::SetMark { track, index, mark } => {
                let duration_ms = doc.duration_ms;
                set_mark(marks_mut(doc, *track)?, *index, mark, duration_ms)?;
            }
            SequenceEdit::RemoveMarks { track, indices } => {
                let track = marks_mut(doc, *track)?;
                if indices.iter().any(|&i| i >= track.marks.len()) {
                    return Err(mark_gone());
                }
                let gone: HashSet<usize> = indices.iter().copied().collect();
                let mut i = 0;
                track.marks.retain(|_| {
                    i += 1;
                    !gone.contains(&(i - 1))
                });
            }
            SequenceEdit::SplitMark { track, index, at_ms } => {
                let track = marks_mut(doc, *track)?;
                let mark = track.marks.get(*index).ok_or_else(mark_gone)?.clone();
                if *at_ms <= mark.start_ms || *at_ms >= mark.end_ms {
                    return Err(invalid(format!(
                        "Split a mark at a time inside it (between {} and {}).",
                        pf_sequence::format_ms(mark.start_ms),
                        pf_sequence::format_ms(mark.end_ms)
                    )));
                }
                track.marks[*index].end_ms = *at_ms;
                track.marks.insert(index + 1, Mark::new(*at_ms, mark.end_ms, ""));
            }
            SequenceEdit::MergeMarks { track, index } => {
                let track = marks_mut(doc, *track)?;
                if *index >= track.marks.len() {
                    return Err(mark_gone());
                }
                let Some(next) = track.marks.get(index + 1).cloned() else {
                    return Err(invalid(
                        "There's no mark after that one to merge it with.".to_string(),
                    ));
                };
                let first = &mut track.marks[*index];
                first.end_ms = first.end_ms.max(next.end_ms);
                first.label = [first.label.trim(), next.label.trim()]
                    .into_iter()
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                track.marks.remove(index + 1);
            }
            SequenceEdit::GenerateMarks {
                track,
                every_ms,
                from_ms,
                to_ms,
            } => {
                check_inside(*to_ms, doc.duration_ms)?;
                let marks = pf_sequence::fixed_marks(*every_ms, *from_ms, *to_ms).map_err(invalid)?;
                replace_range(marks_mut(doc, *track)?, *from_ms, *to_ms, &marks)?;
            }
            SequenceEdit::CopyMarks { from, to, every } => {
                let source = &doc.timing_tracks[track_index(doc, *from)?];
                let marks = pf_sequence::every_nth_mark(&source.marks, *every).map_err(invalid)?;
                let track = marks_mut(doc, *to)?;
                track.marks.clear();
                track.add_marks(&marks).map_err(invalid)?;
            }
            SequenceEdit::SpreadLyrics {
                track,
                lines,
                from_ms,
                to_ms,
            } => {
                check_inside(*to_ms, doc.duration_ms)?;
                let marks = pf_sequence::spread_phrases(lines, *from_ms, *to_ms).map_err(invalid)?;
                replace_range(marks_mut(doc, *track)?, *from_ms, *to_ms, &marks)?;
            }
            SequenceEdit::LabelMarks {
                track,
                indices,
                labels,
            } => {
                let track = marks_mut(doc, *track)?;
                if indices.len() != labels.len() {
                    return Err(invalid(format!(
                        "There are {} and {}; choose one mark per line, or spread the lyrics over a time range instead.",
                        count(labels.len(), "line of lyrics", "lines of lyrics"),
                        count(indices.len(), "chosen mark", "chosen marks")
                    )));
                }
                if indices.iter().any(|&i| i >= track.marks.len()) {
                    return Err(mark_gone());
                }
                for (&i, label) in indices.iter().zip(labels) {
                    track.marks[i].label = label.trim().to_string();
                }
            }
            SequenceEdit::BreakIntoWords {
                track,
                indices,
                words,
            } => {
                if track == words {
                    return Err(invalid(
                        "Put the words on a different timing track from the phrases.".to_string(),
                    ));
                }
                let phrases = &doc.timing_tracks[track_index(doc, *track)?];
                let chosen: Vec<Mark> = indices
                    .iter()
                    .map(|&i| phrases.marks.get(i).cloned().ok_or_else(mark_gone))
                    .collect::<Result<_, _>>()?;
                let target = marks_mut(doc, *words)?;
                let mut made = 0;
                for phrase in &chosen {
                    let split = pf_sequence::split_words(phrase).map_err(invalid)?;
                    if split.is_empty() {
                        continue;
                    }
                    made += split.len();
                    replace_range(target, phrase.start_ms, phrase.end_ms, &split)?;
                }
                if made == 0 {
                    return Err(invalid(
                        "Those marks have no words in them yet. Give them lyrics first.".to_string(),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Ids that must be unique across the document (a batch could add the same id twice).
pub(crate) fn check_unique_ids(doc: &Sequence) -> Result<(), EngineError> {
    let mut rows = HashSet::with_capacity(doc.rows.len());
    if !doc.rows.iter().all(|r| rows.insert(r.id)) {
        return Err(EngineError::DuplicateId { kind: "row" });
    }
    let mut effects = HashSet::with_capacity(doc.effect_count());
    if !doc.effects().all(|e| effects.insert(e.id)) {
        return Err(EngineError::InvalidEdit(
            "An effect with that id already exists.".to_string(),
        ));
    }
    let mut tracks = HashSet::with_capacity(doc.timing_tracks.len());
    if !doc.timing_tracks.iter().all(|t| tracks.insert(t.id)) {
        return Err(EngineError::DuplicateId { kind: "timing track" });
    }
    Ok(())
}

/// What the UI sees of the open sequence: the whole document. Sent when a sequence is opened,
/// created, or saved, and by [`crate::Engine::sequence_doc`] for a full resync; edits, undo, and
/// redo answer with a lighter [`SequenceEditResult`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceSnapshot {
    /// Increases on every change.
    pub revision: u64,
    /// Where the sequence is saved, if it has been.
    pub path: Option<String>,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub sequence: Sequence,
    /// Problems in the sequence (checked against the current show), errors first.
    pub issues: Vec<SequenceIssue>,
}

/// The answer to an edit, undo, or redo: what changed, without the whole document.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceEditResult {
    /// Increases on every change (also when an edit merges into a gesture's undo step).
    pub revision: u64,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// False when nothing changed (an edit that set what was already there, or nothing to undo).
    pub changed: bool,
    /// Apply these to the previous document to get the new one (see [`SequenceChanges`]).
    pub changes: SequenceChanges,
    /// Problems in the whole sequence now (checked against the current show), errors first.
    pub issues: Vec<SequenceIssue>,
    /// The show's revision now: it changes too when undo or redo took back (or brought back) a
    /// show change made together with this sequence change.
    pub show_revision: u64,
}

/// The sequence's name, music, length, and frame time.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceInfo {
    pub name: String,
    pub audio: Option<String>,
    pub duration_ms: u64,
    pub frame_ms: u32,
}

impl SequenceInfo {
    fn of(doc: &Sequence) -> Self {
        Self {
            name: doc.name.clone(),
            audio: doc.audio.clone(),
            duration_ms: doc.duration_ms,
            frame_ms: doc.frame_ms,
        }
    }
}

/// An effect and where it now sits: `index` within `layer` of `row`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedEffect {
    pub row: RowId,
    pub layer: usize,
    pub index: usize,
    pub effect: Effect,
}

/// What an edit, undo, or redo changed. To bring a copy of the document up to date, in order:
/// 1. `info` (when set) replaces the name, music, length, and frame time;
/// 2. drop `removedRows`; replace each of `rows` by id (or append it); then, when `rowOrder` is
///    set, sort the rows into that order;
/// 3. take `removedEffects` and every effect in `effects` out of wherever they are; then insert
///    each of `effects` at its `index` in its row's layer, lowest index first;
/// 4. timing tracks like rows: `removedTimingTracks`, `timingTracks`, `trackOrder`.
///
/// Edits list single effects where they can (dragging one effect sends just that effect); a row
/// whose layers were added, removed, or renumbered comes whole in `rows`, as does every row an
/// undo or redo touched.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceChanges {
    pub info: Option<SequenceInfo>,
    pub rows: Vec<Row>,
    pub removed_rows: Vec<RowId>,
    pub row_order: Option<Vec<RowId>>,
    pub effects: Vec<PlacedEffect>,
    pub removed_effects: Vec<EffectId>,
    pub timing_tracks: Vec<TimingTrack>,
    pub removed_timing_tracks: Vec<TimingTrackId>,
    pub track_order: Option<Vec<TimingTrackId>>,
}

/// Rows and timing tracks: things kept in a list and found by id.
trait Keyed: Clone + PartialEq {
    type Id: Copy + Eq + std::hash::Hash;
    fn key(&self) -> Self::Id;
    fn bytes(&self) -> usize;
}

impl Keyed for Row {
    type Id = RowId;
    fn key(&self) -> RowId {
        self.id
    }
    fn bytes(&self) -> usize {
        96 + self
            .layers
            .iter()
            .map(|l| 32 + 192 * l.effects.len())
            .sum::<usize>()
    }
}

impl Keyed for TimingTrack {
    type Id = TimingTrackId;
    fn key(&self) -> TimingTrackId {
        self.id
    }
    fn bytes(&self) -> usize {
        64 + 48 * self.marks.len()
    }
}

/// Some items of a list (each as it was, or `None` where it didn't exist) and maybe the list's
/// order, captured before an edit changed them.
#[derive(Debug, Clone)]
struct ListParts<T: Keyed> {
    items: Vec<(T::Id, Option<T>)>,
    order: Option<Vec<T::Id>>,
}

impl<T: Keyed> Default for ListParts<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            order: None,
        }
    }
}

impl<T: Keyed> ListParts<T> {
    fn has(&self, id: T::Id) -> bool {
        self.items.iter().any(|(i, _)| *i == id)
    }

    fn capture(&mut self, list: &[T], id: T::Id) {
        if !self.has(id) {
            self.items
                .push((id, list.iter().find(|x| x.key() == id).cloned()));
        }
    }

    fn capture_order(&mut self, list: &[T]) {
        if self.order.is_none() {
            self.order = Some(list.iter().map(Keyed::key).collect());
        }
    }

    /// The same items and order as they are in `list` now.
    fn current(&self, list: &[T]) -> Self {
        let mut now = Self::default();
        for (id, _) in &self.items {
            now.capture(list, *id);
        }
        if self.order.is_some() {
            now.capture_order(list);
        }
        now
    }

    fn unchanged(&self, list: &[T]) -> bool {
        self.items
            .iter()
            .all(|(id, before)| list.iter().find(|x| x.key() == *id) == before.as_ref())
            && self
                .order
                .as_ref()
                .is_none_or(|order| order.iter().copied().eq(list.iter().map(Keyed::key)))
    }

    fn restore(self, list: &mut Vec<T>) {
        for (id, item) in self.items {
            let at = list.iter().position(|x| x.key() == id);
            match (at, item) {
                (Some(i), Some(item)) => list[i] = item,
                (Some(i), None) => {
                    list.remove(i);
                }
                (None, Some(item)) => list.push(item),
                (None, None) => {}
            }
        }
        if let Some(order) = self.order {
            let rank: std::collections::HashMap<T::Id, usize> =
                order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
            list.sort_by_key(|x| rank.get(&x.key()).copied().unwrap_or(usize::MAX));
        }
    }

    /// Adds what `other` captured that this doesn't have yet (keeping the older captures).
    fn merge_missing(&mut self, other: Self) {
        for (id, item) in other.items {
            if !self.has(id) {
                self.items.push((id, item));
            }
        }
        if self.order.is_none() {
            self.order = other.order;
        }
    }

    fn bytes(&self) -> usize {
        self.items
            .iter()
            .map(|(_, item)| 32 + item.as_ref().map_or(0, Keyed::bytes))
            .sum::<usize>()
            + self.order.as_ref().map_or(0, |o| 16 * o.len())
    }

    /// The captured items as they are in `list` now (present ones) and the ids that are gone.
    fn now_in(&self, list: &[T]) -> (Vec<T>, Vec<T::Id>) {
        let (mut present, mut gone) = (Vec::new(), Vec::new());
        for (id, _) in &self.items {
            match list.iter().find(|x| x.key() == *id) {
                Some(item) => present.push(item.clone()),
                None => gone.push(*id),
            }
        }
        (present, gone)
    }
}

/// The parts of the document an undo step restores: only what its edits touched (the info, some
/// rows, some timing tracks, and the order of either list). A drag on one effect stores one row,
/// not the whole document.
#[derive(Debug, Clone, Default)]
struct Parts {
    info: Option<SequenceInfo>,
    rows: ListParts<Row>,
    tracks: ListParts<TimingTrack>,
    /// The whole document instead (only for documents opened with repeated ids, where parts
    /// can't be found reliably by id).
    whole: Option<Box<Sequence>>,
}

impl Parts {
    fn capture_info(&mut self, doc: &Sequence) {
        if self.info.is_none() {
            self.info = Some(SequenceInfo::of(doc));
        }
    }

    fn current(&self, doc: &Sequence) -> Self {
        if self.whole.is_some() {
            return whole_document(doc);
        }
        Self {
            info: self.info.as_ref().map(|_| SequenceInfo::of(doc)),
            rows: self.rows.current(&doc.rows),
            tracks: self.tracks.current(&doc.timing_tracks),
            whole: None,
        }
    }

    fn unchanged(&self, doc: &Sequence) -> bool {
        if let Some(whole) = &self.whole {
            return **whole == *doc;
        }
        self.info
            .as_ref()
            .is_none_or(|info| *info == SequenceInfo::of(doc))
            && self.rows.unchanged(&doc.rows)
            && self.tracks.unchanged(&doc.timing_tracks)
    }

    fn restore(self, doc: &mut Sequence) {
        if let Some(whole) = self.whole {
            *doc = *whole;
            return;
        }
        if let Some(info) = self.info {
            doc.name = info.name;
            doc.audio = info.audio;
            doc.duration_ms = info.duration_ms;
            doc.frame_ms = info.frame_ms;
        }
        self.rows.restore(&mut doc.rows);
        self.tracks.restore(&mut doc.timing_tracks);
    }

    fn merge_missing(&mut self, other: Parts) {
        if self.whole.is_some() {
            return;
        }
        if other.whole.is_some() {
            *self = other;
            return;
        }
        if self.info.is_none() {
            self.info = other.info;
        }
        self.rows.merge_missing(other.rows);
        self.tracks.merge_missing(other.tracks);
    }

    fn bytes(&self) -> usize {
        if let Some(whole) = &self.whole {
            let rows: usize = whole.rows.iter().map(Keyed::bytes).sum();
            let tracks: usize = whole.timing_tracks.iter().map(Keyed::bytes).sum();
            return 512 + rows + tracks;
        }
        64 + self
            .info
            .as_ref()
            .map_or(0, |i| i.name.len() + i.audio.as_ref().map_or(0, String::len))
            + self.rows.bytes()
            + self.tracks.bytes()
    }

    /// Everything these parts cover, as it is now (whole rows and tracks): what an undo or redo
    /// changed.
    fn changes(&self, doc: &Sequence) -> SequenceChanges {
        if self.whole.is_some() {
            return whole_document_changes(doc);
        }
        let (rows, removed_rows) = self.rows.now_in(&doc.rows);
        let (timing_tracks, removed_timing_tracks) = self.tracks.now_in(&doc.timing_tracks);
        SequenceChanges {
            info: self.info.as_ref().map(|_| SequenceInfo::of(doc)),
            rows,
            removed_rows,
            row_order: self
                .rows
                .order
                .as_ref()
                .map(|_| doc.rows.iter().map(|r| r.id).collect()),
            timing_tracks,
            removed_timing_tracks,
            track_order: self
                .tracks
                .order
                .as_ref()
                .map(|_| doc.timing_tracks.iter().map(|t| t.id).collect()),
            ..SequenceChanges::default()
        }
    }
}

/// What a batch of edits did, for listing single effects in its [`SequenceChanges`].
#[derive(Debug, Default)]
struct Touched {
    /// Rows whose layers were added or removed (sent whole).
    restructured: HashSet<RowId>,
    effects: Vec<EffectId>,
    /// True when the batch adds anything with an id (so ids must be checked for duplicates).
    adds: bool,
}

impl Touched {
    fn effect(&mut self, id: EffectId) {
        if !self.effects.contains(&id) {
            self.effects.push(id);
        }
    }
}

fn row_of_effect(doc: &Sequence, id: EffectId) -> Option<RowId> {
    doc.locate_effect(id).map(|(r, _, _)| doc.rows[r].id)
}

/// Captures, before `edit` is applied, everything it may change.
fn touch(edit: &SequenceEdit, doc: &Sequence, parts: &mut Parts, touched: &mut Touched) {
    let rows = &doc.rows;
    match edit {
        SequenceEdit::UpdateInfo { .. } => parts.capture_info(doc),
        SequenceEdit::AddRow { row, .. } => {
            parts.rows.capture(rows, row.id);
            parts.rows.capture_order(rows);
            touched.restructured.insert(row.id);
            touched.adds = true;
        }
        SequenceEdit::RemoveRow { id } => {
            parts.rows.capture(rows, *id);
            parts.rows.capture_order(rows);
        }
        SequenceEdit::MoveRow { .. } => parts.rows.capture_order(rows),
        SequenceEdit::AddLayer { row, .. } | SequenceEdit::RemoveLayer { row, .. } => {
            parts.rows.capture(rows, *row);
            touched.restructured.insert(*row);
        }
        SequenceEdit::AddEffect { row, effect, .. } => {
            parts.rows.capture(rows, *row);
            touched.effect(effect.id);
            touched.adds = true;
        }
        SequenceEdit::UpdateEffect { effect } => {
            if let Some(row) = row_of_effect(doc, effect.id) {
                parts.rows.capture(rows, row);
            }
            touched.effect(effect.id);
        }
        SequenceEdit::SetEffectTiming { id, .. }
        | SequenceEdit::SetEffectParams { id, .. }
        | SequenceEdit::RemoveEffect { id } => {
            if let Some(row) = row_of_effect(doc, *id) {
                parts.rows.capture(rows, row);
            }
            touched.effect(*id);
        }
        SequenceEdit::MoveEffect { id, row, .. } => {
            if let Some(from) = row_of_effect(doc, *id) {
                parts.rows.capture(rows, from);
            }
            parts.rows.capture(rows, *row);
            touched.effect(*id);
        }
        SequenceEdit::AddTimingTrack { track } => {
            parts.tracks.capture(&doc.timing_tracks, track.id);
            parts.tracks.capture_order(&doc.timing_tracks);
            touched.adds = true;
        }
        SequenceEdit::UpdateTimingTrack { track } => parts.tracks.capture(&doc.timing_tracks, track.id),
        SequenceEdit::RemoveTimingTrack { id } => {
            parts.tracks.capture(&doc.timing_tracks, *id);
            parts.tracks.capture_order(&doc.timing_tracks);
        }
        SequenceEdit::MoveTimingTrack { .. } => parts.tracks.capture_order(&doc.timing_tracks),
        SequenceEdit::RenameTimingTrack { id: track, .. }
        | SequenceEdit::AddMarks { track, .. }
        | SequenceEdit::SetMark { track, .. }
        | SequenceEdit::RemoveMarks { track, .. }
        | SequenceEdit::SplitMark { track, .. }
        | SequenceEdit::MergeMarks { track, .. }
        | SequenceEdit::GenerateMarks { track, .. }
        | SequenceEdit::CopyMarks { to: track, .. }
        | SequenceEdit::SpreadLyrics { track, .. }
        | SequenceEdit::LabelMarks { track, .. }
        | SequenceEdit::BreakIntoWords { words: track, .. } => {
            parts.tracks.capture(&doc.timing_tracks, *track)
        }
    }
}

/// What a batch of edits changed: single effects where possible, whole rows where layers moved.
fn edit_changes(doc: &Sequence, parts: &Parts, touched: &Touched) -> SequenceChanges {
    let mut changes = parts.changes(doc);
    // Rows go whole only when their layers changed (or they're new); otherwise effects go singly.
    let before: std::collections::HashMap<RowId, Option<usize>> = parts
        .rows
        .items
        .iter()
        .map(|(id, row)| (*id, row.as_ref().map(|r| r.layers.len())))
        .collect();
    changes.rows.retain(|row| {
        touched.restructured.contains(&row.id)
            || before.get(&row.id).copied().flatten() != Some(row.layers.len())
    });
    let mut whole: HashSet<RowId> = changes.rows.iter().map(|r| r.id).collect();
    // An effect that moved into a row sent whole must also leave its old row: send that whole too
    // (until nothing more changes, since that row may have received an effect from a third).
    let moved: HashSet<EffectId> = touched.effects.iter().copied().collect();
    while !whole.is_empty() {
        let mut more = Vec::new();
        for (id, before) in &parts.rows.items {
            let Some(before) = before else { continue };
            if whole.contains(id) {
                continue;
            }
            let lost_to_whole = before.layers.iter().flat_map(|l| &l.effects).any(|e| {
                moved.contains(&e.id)
                    && row_of_effect(doc, e.id).is_some_and(|now| now != *id && whole.contains(&now))
            });
            if lost_to_whole && let Some(row) = doc.row(*id) {
                more.push(row.clone());
            }
        }
        if more.is_empty() {
            break;
        }
        whole.extend(more.iter().map(|r| r.id));
        changes.rows.extend(more);
    }
    // Many effects (a paste): index the document once instead of searching it for each.
    let index: Option<std::collections::HashMap<EffectId, (usize, usize, usize)>> =
        (touched.effects.len() > 8).then(|| {
            doc.rows
                .iter()
                .enumerate()
                .flat_map(|(r, row)| {
                    row.layers.iter().enumerate().flat_map(move |(l, layer)| {
                        layer
                            .effects
                            .iter()
                            .enumerate()
                            .map(move |(e, effect)| (effect.id, (r, l, e)))
                    })
                })
                .collect()
        });
    for &id in &touched.effects {
        let at = match &index {
            Some(index) => index.get(&id).copied(),
            None => doc.locate_effect(id),
        };
        match at {
            Some((r, l, e)) => {
                let row = &doc.rows[r];
                if !whole.contains(&row.id) {
                    changes.effects.push(PlacedEffect {
                        row: row.id,
                        layer: l,
                        index: e,
                        effect: row.layers[l].effects[e].clone(),
                    });
                }
            }
            None => changes.removed_effects.push(id),
        }
    }
    changes
}

/// One undo (or redo) step: the parts of the document to put back.
#[derive(Debug, Clone)]
struct Step {
    parts: Parts,
    bytes: usize,
    /// The gesture this step belongs to: later edits with the same gesture merge into it.
    gesture: Option<String>,
    /// Identifies the step as it moves between undo and redo.
    serial: u64,
}

impl Step {
    fn new(parts: Parts, gesture: Option<String>, serial: u64) -> Self {
        Self {
            bytes: parts.bytes(),
            parts,
            gesture,
            serial,
        }
    }
}

/// The open sequence and its undo history.
///
/// Undo steps store only the rows and timing tracks their edits touched (as they were before),
/// so a step costs about the size of the rows it changed: dragging one effect on a 3,000-effect
/// row stores that row once per gesture. At most [`UNDO_LIMIT`] steps and about
/// [`UNDO_BYTE_BUDGET`] bytes are kept; the oldest go first (the newest step is always kept).
#[derive(Debug, Clone)]
pub(crate) struct OpenSequence {
    pub doc: Sequence,
    pub path: Option<PathBuf>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    undo_bytes: usize,
    revision: u64,
    saved_revision: u64,
    /// Identifies this document (stays the same across edits; new for every open or new).
    id: u64,
    /// The gesture the last edit belonged to, while it's still the latest thing that happened.
    last_gesture: Option<String>,
    /// False when the document was opened with repeated row, effect, or track ids: edits then
    /// snapshot the whole document, since parts can't be found reliably by id.
    ids_unique: bool,
    /// The serial the next new undo step gets.
    next_serial: u64,
}

impl OpenSequence {
    /// The serial of the step undo would take back next.
    pub fn next_undo(&self) -> Option<u64> {
        self.undo.last().map(|step| step.serial)
    }

    /// The serial of the step redo would bring back next.
    pub fn next_redo(&self) -> Option<u64> {
        self.redo.last().map(|step| step.serial)
    }

    pub fn new(doc: Sequence, path: Option<PathBuf>, revision: u64) -> Self {
        let ids_unique = check_unique_ids(&doc).is_ok();
        Self {
            doc,
            path,
            undo: Vec::new(),
            redo: Vec::new(),
            undo_bytes: 0,
            revision,
            saved_revision: revision,
            id: revision,
            last_gesture: None,
            next_serial: 1,
            ids_unique,
        }
    }

    /// A document that has never been saved and has unsaved changes (an import).
    pub fn unsaved(doc: Sequence, revision: u64) -> Self {
        let mut open = Self::new(doc, None, revision);
        open.mark_unsaved();
        open
    }

    /// Identifies the document (not its revision): see [`crate::Engine::sequence_doc_id`].
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Applies a batch as one undo step, or, when `gesture` names the gesture of the previous
    /// edit, merges it into that edit's step (so a whole drag undoes at once). Returns what
    /// changed (`None` when nothing did); on error nothing changes.
    pub fn apply(
        &mut self,
        edits: &[SequenceEdit],
        gesture: Option<&str>,
    ) -> Result<Option<SequenceChanges>, EngineError> {
        let mut parts = Parts::default();
        let mut touched = Touched::default();
        let whole = !self.ids_unique;
        let before = whole.then(|| self.doc.clone());
        let result = (|| {
            for edit in edits {
                touch(edit, &self.doc, &mut parts, &mut touched);
                edit.apply(&mut self.doc)?;
            }
            if let Some(problem) = pf_sequence::limit_problems(&self.doc).into_iter().next() {
                return Err(EngineError::TooLarge(problem));
            }
            if touched.adds {
                check_unique_ids(&self.doc)?;
            }
            Ok(())
        })();
        if let Some(before) = before {
            // Repeated ids: parts can't be found by id, so undo puts the whole document back.
            if let Err(error) = result {
                self.doc = before;
                return Err(error);
            }
            parts = whole_document(&before);
        } else if let Err(error) = result {
            parts.restore(&mut self.doc);
            return Err(error);
        }
        if parts.unchanged(&self.doc) {
            return Ok(None);
        }
        let changes = if whole {
            self.ids_unique = check_unique_ids(&self.doc).is_ok();
            whole_document_changes(&self.doc)
        } else {
            edit_changes(&self.doc, &parts, &touched)
        };
        let merge = gesture.is_some()
            && self.last_gesture.as_deref() == gesture
            && self.undo.last().is_some_and(|s| s.gesture.as_deref() == gesture);
        if merge {
            let top = self.undo.last_mut().expect("checked above");
            top.parts.merge_missing(parts);
            let bytes = top.parts.bytes();
            self.undo_bytes = self.undo_bytes - top.bytes + bytes;
            top.bytes = bytes;
            self.trim();
        } else {
            let serial = self.next_serial;
            self.next_serial += 1;
            self.record(Step::new(parts, gesture.map(str::to_owned), serial));
        }
        self.last_gesture = gesture.map(str::to_owned);
        self.redo.clear();
        self.revision += 1;
        Ok(Some(changes))
    }

    fn record(&mut self, step: Step) {
        self.undo_bytes += step.bytes;
        self.undo.push(step);
        self.trim();
    }

    fn trim(&mut self) {
        while self.undo.len() > 1 && (self.undo.len() > UNDO_LIMIT || self.undo_bytes > UNDO_BYTE_BUDGET) {
            let dropped = self.undo.remove(0);
            self.undo_bytes -= dropped.bytes;
        }
    }

    /// Undoes the last step; returns what changed, or `None` when there was nothing to undo.
    pub fn undo(&mut self) -> Option<SequenceChanges> {
        let step = self.undo.pop()?;
        self.undo_bytes -= step.bytes;
        let now = step.parts.current(&self.doc);
        let changes_for = step.parts.clone();
        step.parts.restore(&mut self.doc);
        self.redo.push(Step::new(now, step.gesture, step.serial));
        self.last_gesture = None;
        self.ids_unique = check_unique_ids(&self.doc).is_ok();
        self.revision += 1;
        Some(changes_for.changes(&self.doc))
    }

    /// Redoes the last undone step; returns what changed, or `None` when there was nothing.
    pub fn redo(&mut self) -> Option<SequenceChanges> {
        let step = self.redo.pop()?;
        let before = step.parts.current(&self.doc);
        let changes_for = step.parts.clone();
        step.parts.restore(&mut self.doc);
        self.record(Step::new(before, step.gesture, step.serial));
        self.last_gesture = None;
        self.ids_unique = check_unique_ids(&self.doc).is_ok();
        self.revision += 1;
        Some(changes_for.changes(&self.doc))
    }

    /// Rewrites relative music for the document's move to `to`: relative to the old file's
    /// folder before, relative to the new folder after when the music is inside it, otherwise a
    /// full path. Undo history is rewritten too, so undoing doesn't bring back a path that now
    /// points elsewhere. An unsaved document's relative music is left as is (it's taken to be
    /// next to the file it's first saved as).
    pub fn rebase_audio(&mut self, to: &Path) {
        let (Some(from), Some(to)) = (self.path.as_deref().and_then(Path::parent), to.parent()) else {
            return;
        };
        if from == to {
            return;
        }
        let rebase = |audio: &mut Option<String>| {
            if let Some(a) = audio.as_mut() {
                *a = rebase_audio(a, from, to);
            }
        };
        let before = self.doc.audio.clone();
        rebase(&mut self.doc.audio);
        for step in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            if let Some(info) = step.parts.info.as_mut() {
                rebase(&mut info.audio);
            }
            if let Some(whole) = step.parts.whole.as_mut() {
                rebase(&mut whole.audio);
            }
        }
        if self.doc.audio != before {
            self.revision += 1;
        }
    }

    #[cfg(test)]
    fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn snapshot_revision(&self) -> u64 {
        self.revision
    }

    /// Whether there are changes since the document was opened or last saved.
    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    /// Marks the document as having unsaved changes (one recovered after a crash, or an
    /// import). Revisions start at 1, so 0 never matches.
    pub fn mark_unsaved(&mut self) {
        self.saved_revision = 0;
    }

    pub fn mark_saved(&mut self, path: &Path) {
        self.path = Some(path.to_path_buf());
        self.saved_revision = self.revision;
    }

    pub fn snapshot(&self, show: &Show) -> SequenceSnapshot {
        SequenceSnapshot {
            revision: self.revision,
            path: self.path.as_deref().map(pf_model::path_to_text),
            dirty: self.revision != self.saved_revision,
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            sequence: self.doc.clone(),
            issues: pf_sequence::validate_sequence(&self.doc, show),
        }
    }

    /// The reply to an edit, undo, or redo that changed `changes` (`None`: nothing changed).
    pub fn edit_result(&self, changes: Option<SequenceChanges>, show: &Show) -> SequenceEditResult {
        SequenceEditResult {
            revision: self.revision,
            dirty: self.revision != self.saved_revision,
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            changed: changes.is_some(),
            changes: changes.unwrap_or_default(),
            issues: pf_sequence::validate_sequence(&self.doc, show),
            // Filled in by the engine, which knows the show's revision.
            show_revision: 0,
        }
    }
}

/// `audio` (relative to `from`) as seen from `to`: relative when it's inside `to`, else a full
/// path. Full paths stay as they are.
fn rebase_audio(audio: &str, from: &Path, to: &Path) -> String {
    if pf_model::is_full_path_text(audio) || audio.is_empty() {
        return audio.to_string();
    }
    pf_model::relative_text(&pf_model::resolve_text(audio, from), to)
}

/// `doc` as a sequence file at `path` stores it: music inside the file's folder (or a folder
/// below it) relative to that folder, so the folder can move.
fn stored_at<'a>(path: &Path, doc: &'a Sequence) -> std::borrow::Cow<'a, Sequence> {
    let folder = path.parent().filter(|p| !p.as_os_str().is_empty());
    match (folder, doc.audio.as_deref()) {
        (Some(folder), Some(audio)) if pf_model::relative_text(audio, folder) != audio => {
            let mut stored = doc.clone();
            stored.audio = Some(pf_model::relative_text(audio, folder));
            std::borrow::Cow::Owned(stored)
        }
        _ => std::borrow::Cow::Borrowed(doc),
    }
}

/// Parts holding the whole document.
fn whole_document(doc: &Sequence) -> Parts {
    Parts {
        whole: Some(Box::new(doc.clone())),
        ..Parts::default()
    }
}

/// Changes that replace everything (after an edit to a document with repeated ids).
fn whole_document_changes(doc: &Sequence) -> SequenceChanges {
    SequenceChanges {
        info: Some(SequenceInfo::of(doc)),
        rows: doc.rows.clone(),
        row_order: Some(doc.rows.iter().map(|r| r.id).collect()),
        timing_tracks: doc.timing_tracks.clone(),
        track_order: Some(doc.timing_tracks.iter().map(|t| t.id).collect()),
        ..SequenceChanges::default()
    }
}

/// Reads and parses a sequence file (running schema migrations and size limits).
pub fn load_sequence(path: &Path) -> Result<Sequence, EngineError> {
    let read_err = |source| EngineError::Read {
        path: path.to_path_buf(),
        source,
    };
    let size = fs::metadata(path).map_err(read_err)?.len();
    if size > MAX_SEQUENCE_BYTES as u64 {
        return Err(EngineError::InvalidSequence {
            path: path.to_path_buf(),
            reason: format!(
                "it is {} MB; PixelFlow reads sequence files up to {} MB",
                size / (1024 * 1024),
                MAX_SEQUENCE_BYTES / (1024 * 1024)
            ),
        });
    }
    let text = fs::read_to_string(path).map_err(read_err)?;
    pf_sequence::sequence_from_json(&text).map_err(|e| EngineError::InvalidSequence {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })
}

/// Saves a sequence so that a crash never leaves a half-written file. Music inside the file's
/// folder is stored relative to it.
pub fn save_sequence_atomic(path: &Path, doc: &Sequence) -> Result<(), EngineError> {
    write_sequence(path, &stored_at(path, doc))
}

/// Writes a sequence file atomically, its music path as it is.
pub(crate) fn write_sequence(path: &Path, doc: &Sequence) -> Result<(), EngineError> {
    let json = pf_sequence::sequence_to_json(doc).map_err(|e| EngineError::Write {
        path: path.to_path_buf(),
        source: std::io::Error::other(e),
    })?;
    write_atomic(path, json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::PropId;
    use pf_sequence::{EffectKind, Target};

    fn open() -> (OpenSequence, RowId) {
        let mut doc = Sequence::new("s", 10_000);
        let row = Row::new(Target::Prop(PropId::new()));
        let id = row.id;
        doc.rows.push(row);
        (OpenSequence::new(doc, None, 0), id)
    }

    #[test]
    fn edits_apply_as_one_undo_step_and_failures_change_nothing() {
        let (mut open, row) = open();
        let effect = Effect::new(EffectKind::On, 0, 1000);
        let id = effect.id;
        let changed = open
            .apply(
                &[
                    SequenceEdit::AddEffect {
                        row,
                        layer: 0,
                        effect,
                    },
                    SequenceEdit::SetEffectTiming {
                        id,
                        start_ms: 500,
                        end_ms: 1500,
                    },
                    SequenceEdit::AddEffect {
                        row,
                        layer: 1,
                        effect: Effect::new(EffectKind::Twinkle, 0, 100),
                    },
                ],
                None,
            )
            .unwrap();
        assert!(changed.is_some());
        assert_eq!(
            open.doc.rows[0].layers.len(),
            2,
            "layer one past the top starts a new layer"
        );
        assert_eq!(open.doc.effect(id).unwrap().start_ms, 500);
        let before = open.doc.clone();
        let err = open
            .apply(
                &[
                    SequenceEdit::RemoveEffect { id },
                    SequenceEdit::SetEffectTiming {
                        id,
                        start_ms: 0,
                        end_ms: 1,
                    },
                ],
                None,
            )
            .unwrap_err();
        assert_eq!(err.to_string(), "That effect isn't in the sequence anymore.");
        assert_eq!(open.doc, before, "nothing changed");
        assert!(open.undo().is_some());
        assert_eq!(open.doc.effect_count(), 0);
        assert!(open.redo().is_some());
        assert_eq!(open.doc, before);
    }

    #[test]
    fn bad_edits_are_explained() {
        let (mut open, row) = open();
        let err = open
            .apply(
                &[SequenceEdit::AddEffect {
                    row,
                    layer: 0,
                    effect: Effect::new(EffectKind::On, 500, 500),
                }],
                None,
            )
            .unwrap_err();
        assert_eq!(err.to_string(), "An effect must end after it starts.");
        let effect = Effect::new(EffectKind::On, 0, 10);
        let err = open
            .apply(
                &[
                    SequenceEdit::AddEffect {
                        row,
                        layer: 0,
                        effect: effect.clone(),
                    },
                    SequenceEdit::AddEffect {
                        row,
                        layer: 0,
                        effect,
                    },
                ],
                None,
            )
            .unwrap_err();
        assert_eq!(err.to_string(), "An effect with that id already exists.");
        let err = open
            .apply(
                &[SequenceEdit::AddEffect {
                    row,
                    layer: 5,
                    effect: Effect::new(EffectKind::On, 0, 10),
                }],
                None,
            )
            .unwrap_err();
        assert_eq!(err.to_string(), "That row has no layer 6.");
        let err = open
            .apply(
                &[SequenceEdit::UpdateInfo {
                    name: "s".into(),
                    audio: None,
                    duration_ms: 99_999_999_999,
                    frame_ms: 25,
                }],
                None,
            )
            .unwrap_err();
        assert!(err.to_string().contains("at most 4 hours"), "{err}");
    }

    #[test]
    fn moving_effects_rows_and_layers() {
        let (mut open, first) = open();
        let second = Row::new(Target::Prop(PropId::new()));
        let second_id = second.id;
        let effect = Effect::new(EffectKind::Chase, 0, 1000);
        let id = effect.id;
        open.apply(
            &[
                SequenceEdit::AddRow {
                    row: second,
                    index: Some(0),
                },
                SequenceEdit::AddEffect {
                    row: first,
                    layer: 0,
                    effect,
                },
                SequenceEdit::MoveEffect {
                    id,
                    row: second_id,
                    layer: 1,
                    start_ms: 2000,
                    end_ms: 2500,
                },
                SequenceEdit::MoveRow {
                    id: second_id,
                    index: 9,
                },
            ],
            None,
        )
        .unwrap();
        assert_eq!(open.doc.rows[1].id, second_id, "moved to the end");
        assert_eq!(open.doc.locate_effect(id), Some((1, 1, 0)));
        assert_eq!(open.doc.effect(id).unwrap().start_ms, 2000);

        // A move to a missing layer fails without losing the effect.
        let err = open
            .apply(
                &[SequenceEdit::MoveEffect {
                    id,
                    row: first,
                    layer: 3,
                    start_ms: 0,
                    end_ms: 10,
                }],
                None,
            )
            .unwrap_err();
        assert_eq!(err.to_string(), "That row has no layer 4.");
        assert!(open.doc.effect(id).is_some());

        open.apply(
            &[
                SequenceEdit::RemoveLayer {
                    row: second_id,
                    layer: 1,
                },
                SequenceEdit::RemoveRow { id: first },
            ],
            None,
        )
        .unwrap();
        assert_eq!(open.doc.rows.len(), 1);
        assert_eq!(open.doc.effect_count(), 0);
    }

    #[test]
    fn edits_round_trip_as_ui_json() {
        let json = serde_json::json!({ "type": "setEffectTiming",
            "id": "33333333-0000-4000-8000-000000000001", "startMs": 5, "endMs": 10 });
        let edit: SequenceEdit = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(&edit).unwrap(), json);
        let add: SequenceEdit = serde_json::from_value(serde_json::json!({ "type": "addRow",
            "row": { "id": "33333333-0000-4000-8000-000000000002",
                     "target": { "group": "33333333-0000-4000-8000-000000000003" } } }))
        .unwrap();
        assert!(matches!(add, SequenceEdit::AddRow { index: None, .. }));
    }

    #[test]
    fn files_round_trip_and_problems_are_plain() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.pfseq.json");
        let (open, _) = open();
        save_sequence_atomic(&path, &open.doc).unwrap();
        assert_eq!(load_sequence(&path).unwrap(), open.doc);
        std::fs::write(&path, "{ \"schemaVersion\": 1 }").unwrap();
        let err = load_sequence(&path).unwrap_err().to_string();
        assert!(err.contains("is not a valid sequence file"), "{err}");
        assert!(err.contains("missing field"), "{err}");
        assert!(
            load_sequence(&dir.path().join("nope.json"))
                .unwrap_err()
                .to_string()
                .starts_with("Could not read")
        );
    }

    /// Brings a copy of the document up to date the way [`SequenceChanges`] says the UI should.
    fn apply_changes(doc: &mut Sequence, c: &SequenceChanges) {
        if let Some(info) = &c.info {
            doc.name = info.name.clone();
            doc.audio = info.audio.clone();
            doc.duration_ms = info.duration_ms;
            doc.frame_ms = info.frame_ms;
        }
        doc.rows.retain(|r| !c.removed_rows.contains(&r.id));
        for row in &c.rows {
            match doc.rows.iter_mut().find(|r| r.id == row.id) {
                Some(r) => *r = row.clone(),
                None => doc.rows.push(row.clone()),
            }
        }
        if let Some(order) = &c.row_order {
            doc.rows.sort_by_key(|r| order.iter().position(|id| *id == r.id));
        }
        let gone: Vec<EffectId> = c
            .removed_effects
            .iter()
            .copied()
            .chain(c.effects.iter().map(|p| p.effect.id))
            .collect();
        for layer in doc.rows.iter_mut().flat_map(|r| &mut r.layers) {
            layer.effects.retain(|e| !gone.contains(&e.id));
        }
        let mut placed: Vec<&PlacedEffect> = c.effects.iter().collect();
        placed.sort_by_key(|p| p.index);
        for p in placed {
            let row = doc.rows.iter_mut().find(|r| r.id == p.row).unwrap();
            row.layers[p.layer].effects.insert(p.index, p.effect.clone());
        }
        doc.timing_tracks
            .retain(|t| !c.removed_timing_tracks.contains(&t.id));
        for track in &c.timing_tracks {
            match doc.timing_tracks.iter_mut().find(|t| t.id == track.id) {
                Some(t) => *t = track.clone(),
                None => doc.timing_tracks.push(track.clone()),
            }
        }
        if let Some(order) = &c.track_order {
            doc.timing_tracks
                .sort_by_key(|t| order.iter().position(|id| *id == t.id));
        }
    }

    #[test]
    fn changes_bring_a_copy_up_to_date_through_edits_undo_and_redo() {
        let (mut open, first) = open();
        let mut copy = open.doc.clone();
        let second = Row::new(Target::Prop(PropId::new()));
        let second_id = second.id;
        let (a, b, c) = (
            Effect::new(EffectKind::On, 0, 100),
            Effect::new(EffectKind::Chase, 100, 200),
            Effect::new(EffectKind::Twinkle, 200, 300),
        );
        let (a_id, b_id, c_id) = (a.id, b.id, c.id);
        let track = TimingTrack::new("Beats", pf_sequence::TimingKind::Beats, vec![]);
        let track_id = track.id;
        let batches: Vec<Vec<SequenceEdit>> = vec![
            vec![
                SequenceEdit::AddRow {
                    row: second,
                    index: Some(0),
                },
                SequenceEdit::AddEffect {
                    row: first,
                    layer: 0,
                    effect: a,
                },
                SequenceEdit::AddEffect {
                    row: first,
                    layer: 0,
                    effect: b,
                },
                SequenceEdit::AddEffect {
                    row: first,
                    layer: 0,
                    effect: c,
                },
                SequenceEdit::AddTimingTrack { track: track.clone() },
            ],
            vec![SequenceEdit::SetEffectTiming {
                id: a_id,
                start_ms: 50,
                end_ms: 150,
            }],
            vec![SequenceEdit::MoveEffect {
                id: a_id,
                row: first,
                layer: 0,
                start_ms: 400,
                end_ms: 500,
            }],
            vec![
                SequenceEdit::RemoveEffect { id: b_id },
                SequenceEdit::MoveEffect {
                    id: c_id,
                    row: second_id,
                    layer: 1,
                    start_ms: 0,
                    end_ms: 10,
                },
            ],
            vec![SequenceEdit::AddLayer {
                row: first,
                index: Some(0),
            }],
            vec![SequenceEdit::MoveRow {
                id: second_id,
                index: 5,
            }],
            vec![SequenceEdit::UpdateInfo {
                name: "t".into(),
                audio: None,
                duration_ms: 20_000,
                frame_ms: 50,
            }],
            vec![SequenceEdit::RemoveTimingTrack { id: track_id }],
            vec![SequenceEdit::RemoveRow { id: first }],
        ];
        for edits in &batches {
            let changes = open.apply(edits, None).unwrap().expect("changed");
            apply_changes(&mut copy, &changes);
            assert_eq!(copy, open.doc, "after {edits:?}");
        }
        while let Some(changes) = open.undo() {
            apply_changes(&mut copy, &changes);
            assert_eq!(copy, open.doc, "after an undo");
        }
        while let Some(changes) = open.redo() {
            apply_changes(&mut copy, &changes);
            assert_eq!(copy, open.doc, "after a redo");
        }
    }

    #[test]
    fn dragging_one_effect_sends_and_stores_only_what_it_touched() {
        let (mut open, row) = open();
        let mut big = Row::new(Target::Prop(PropId::new()));
        big.layers[0].effects = (0..3000)
            .map(|i| Effect::new(EffectKind::On, i * 2, i * 2 + 1))
            .collect();
        let effect = Effect::new(EffectKind::Chase, 0, 1000);
        let id = effect.id;
        open.apply(
            &[
                SequenceEdit::AddRow {
                    row: big,
                    index: None,
                },
                SequenceEdit::AddEffect {
                    row,
                    layer: 0,
                    effect,
                },
            ],
            None,
        )
        .unwrap();
        let changes = open
            .apply(
                &[SequenceEdit::SetEffectTiming {
                    id,
                    start_ms: 10,
                    end_ms: 1010,
                }],
                Some("drag"),
            )
            .unwrap()
            .unwrap();
        assert!(changes.rows.is_empty() && changes.info.is_none() && changes.row_order.is_none());
        assert_eq!(changes.effects.len(), 1);
        assert_eq!(
            (
                changes.effects[0].row,
                changes.effects[0].layer,
                changes.effects[0].index
            ),
            (row, 0, 0)
        );
        assert_eq!(changes.effects[0].effect.start_ms, 10);
        // The undo step holds the small row, not the 3,000-effect one.
        let step = open.undo.last().unwrap();
        assert_eq!(step.parts.rows.items.len(), 1);
        assert!(step.bytes < 2_000, "{}", step.bytes);
    }

    #[test]
    fn edits_in_one_gesture_are_one_undo_step() {
        let (mut open, row) = open();
        let effect = Effect::new(EffectKind::On, 0, 1000);
        let id = effect.id;
        open.apply(
            &[SequenceEdit::AddEffect {
                row,
                layer: 0,
                effect,
            }],
            None,
        )
        .unwrap();
        let timing = |start_ms| SequenceEdit::SetEffectTiming {
            id,
            start_ms,
            end_ms: start_ms + 1000,
        };
        let revision = open.revision;
        for start in [10, 20, 30, 40] {
            open.apply(&[timing(start)], Some("drag-1")).unwrap();
        }
        assert_eq!(open.revision, revision + 4, "every edit is a new revision");
        assert_eq!(open.undo.len(), 2, "add + one drag");
        // Setting what's already there changes nothing and keeps the gesture going.
        assert!(open.apply(&[timing(40)], Some("drag-1")).unwrap().is_none());
        open.apply(&[timing(50)], Some("drag-1")).unwrap();
        assert_eq!(open.undo.len(), 2);
        // Another gesture is another step.
        open.apply(&[timing(60)], Some("drag-2")).unwrap();
        assert_eq!(open.undo.len(), 3);
        open.undo().unwrap();
        assert_eq!(open.doc.effect(id).unwrap().start_ms, 50);
        // An undo ends the gesture: the same id afterwards starts a new step.
        open.apply(&[timing(70)], Some("drag-1")).unwrap();
        open.apply(&[timing(80)], Some("drag-1")).unwrap();
        assert_eq!(open.undo.len(), 3);
        assert!(!open.can_redo(), "a new edit clears redo");
        open.undo().unwrap();
        assert_eq!(
            open.doc.effect(id).unwrap().start_ms,
            50,
            "the whole drag undoes at once"
        );
        open.undo().unwrap();
        assert_eq!(
            open.doc.effect(id).unwrap().start_ms,
            0,
            "back to before the first drag"
        );
        // An edit outside the gesture also ends it.
        open.apply(&[timing(5)], Some("g")).unwrap();
        open.apply(&[timing(6)], None).unwrap();
        open.apply(&[timing(7)], Some("g")).unwrap();
        assert_eq!(open.undo.len(), 4);
    }

    #[test]
    fn failed_batches_leave_the_document_and_its_history_alone() {
        let (mut open, row) = open();
        let effect = Effect::new(EffectKind::On, 0, 1000);
        let id = effect.id;
        open.apply(
            &[SequenceEdit::AddEffect {
                row,
                layer: 0,
                effect,
            }],
            None,
        )
        .unwrap();
        let before = open.doc.clone();
        let err = open
            .apply(
                &[
                    SequenceEdit::AddLayer { row, index: None },
                    SequenceEdit::SetEffectTiming {
                        id,
                        start_ms: 5,
                        end_ms: 6,
                    },
                    SequenceEdit::RemoveRow { id: RowId::new() },
                ],
                Some("g"),
            )
            .unwrap_err();
        assert_eq!(err.to_string(), "That row isn't in the sequence anymore.");
        assert_eq!(open.doc, before);
        assert_eq!(open.undo.len(), 1);
    }

    #[test]
    fn several_effects_changed_together_are_one_undo_step_or_refused_together() {
        let (mut open, row) = open();
        let effects: Vec<Effect> = (0..3)
            .map(|i| Effect::new(EffectKind::Chase, i * 2000, i * 2000 + 1000))
            .collect();
        let ids: Vec<EffectId> = effects.iter().map(|e| e.id).collect();
        let adds: Vec<SequenceEdit> = effects
            .iter()
            .map(|effect| SequenceEdit::AddEffect {
                row,
                layer: 0,
                effect: effect.clone(),
            })
            .collect();
        open.apply(&adds, None).unwrap();
        let steps = open.undo.len();

        // One change from the settings panel: every selected effect gets new colors and a fade.
        let recolor = |e: &Effect| {
            let mut changed = e
                .clone()
                .with_palette(vec![pf_sequence::Rgb::RED, pf_sequence::Rgb::BLUE]);
            changed.fade_in_ms = 250;
            SequenceEdit::UpdateEffect { effect: changed }
        };
        let batch: Vec<SequenceEdit> = ids
            .iter()
            .map(|id| recolor(open.doc.effect(*id).unwrap()))
            .collect();
        let changes = open.apply(&batch, None).unwrap().unwrap();
        assert_eq!(changes.effects.len(), 3);
        assert_eq!(open.undo.len(), steps + 1, "one undo step for all three");
        assert!(
            ids.iter()
                .all(|id| open.doc.effect(*id).unwrap().fade_in_ms == 250)
        );
        open.undo().unwrap();
        assert!(
            ids.iter().all(|id| open.doc.effect(*id).unwrap().fade_in_ms == 0),
            "one undo takes back every effect's change"
        );
        open.redo().unwrap();

        // One effect in the batch can't take the change: none of them changes.
        let before = open.doc.clone();
        let mut batch: Vec<SequenceEdit> = ids
            .iter()
            .map(|id| {
                let mut e = open.doc.effect(*id).unwrap().clone();
                e.blend = pf_sequence::Blend::Add;
                SequenceEdit::UpdateEffect { effect: e }
            })
            .collect();
        let bad = open.doc.effect(ids[2]).unwrap();
        batch[2] = serde_json::from_value(serde_json::json!({ "type": "updateEffect", "effect": {
            "id": bad.id, "startMs": bad.start_ms, "endMs": bad.end_ms,
            "params": { "kind": "chase", "speed": 1e39 }, "blend": "add" } }))
        .unwrap();
        let err = open.apply(&batch, None).unwrap_err().to_string();
        assert_eq!(
            err,
            "The Chase effect at 0:04.000 has a setting PixelFlow can't use: Speed isn't a usable number; use 0 to 50."
        );
        assert_eq!(open.doc, before, "the whole batch is refused");
        assert_eq!(open.undo.len(), steps + 1);

        // Timing is checked per effect too.
        let mut batch: Vec<SequenceEdit> = ids
            .iter()
            .map(|id| SequenceEdit::SetEffectTiming {
                id: *id,
                start_ms: open.doc.effect(*id).unwrap().start_ms + 100,
                end_ms: open.doc.effect(*id).unwrap().end_ms + 100,
            })
            .collect();
        batch[1] = SequenceEdit::SetEffectTiming {
            id: ids[1],
            start_ms: 3000,
            end_ms: 3000,
        };
        let err = open.apply(&batch, None).unwrap_err().to_string();
        assert_eq!(err, "An effect must end after it starts.");
        assert_eq!(open.doc, before);
    }

    fn spans(track: &TimingTrack) -> Vec<(u64, u64, &str)> {
        track
            .marks
            .iter()
            .map(|m| (m.start_ms, m.end_ms, m.label.as_str()))
            .collect()
    }

    /// An open sequence with one empty track of `kind`.
    fn with_track(kind: pf_sequence::TimingKind) -> (OpenSequence, TimingTrackId) {
        let (mut open, _) = open();
        let track = TimingTrack::new("Lyrics", kind, vec![]);
        let id = track.id;
        open.apply(&[SequenceEdit::AddTimingTrack { track }], None)
            .unwrap();
        (open, id)
    }

    fn err(open: &mut OpenSequence, edit: SequenceEdit) -> String {
        let before = open.doc.clone();
        let message = open.apply(&[edit], None).unwrap_err().to_string();
        assert_eq!(open.doc, before, "a refused edit changes nothing");
        message
    }

    #[test]
    fn marks_are_added_moved_split_merged_and_removed_with_undo() {
        let (mut open, track) = with_track(pf_sequence::TimingKind::Lyrics);
        let t = |open: &OpenSequence| open.doc.timing_track(track).unwrap().clone();
        open.apply(
            &[SequenceEdit::AddMarks {
                track,
                marks: vec![Mark::new(2000, 3000, "two"), Mark::new(0, 1000, "one")],
            }],
            None,
        )
        .unwrap();
        assert_eq!(spans(&t(&open)), vec![(0, 1000, "one"), (2000, 3000, "two")]);

        // A drag on one mark is one undo step.
        for start in [100, 200, 300] {
            open.apply(
                &[SequenceEdit::SetMark {
                    track,
                    index: 0,
                    mark: Mark::new(start, start + 1000, "one"),
                }],
                Some("drag"),
            )
            .unwrap();
        }
        assert_eq!(spans(&t(&open))[0], (300, 1300, "one"));
        open.undo().unwrap();
        assert_eq!(spans(&t(&open))[0], (0, 1000, "one"));
        open.redo().unwrap();

        open.apply(
            &[SequenceEdit::SplitMark {
                track,
                index: 1,
                at_ms: 2400,
            }],
            None,
        )
        .unwrap();
        assert_eq!(
            spans(&t(&open)),
            vec![(300, 1300, "one"), (2000, 2400, "two"), (2400, 3000, "")]
        );
        open.apply(&[SequenceEdit::MergeMarks { track, index: 0 }], None)
            .unwrap();
        assert_eq!(spans(&t(&open)), vec![(300, 2400, "one two"), (2400, 3000, "")]);
        open.apply(
            &[SequenceEdit::RemoveMarks {
                track,
                indices: vec![1, 0],
            }],
            None,
        )
        .unwrap();
        assert!(t(&open).marks.is_empty());
        // Each step undoes on its own, back to the two marks.
        for _ in 0..3 {
            open.undo().unwrap();
        }
        assert_eq!(spans(&t(&open)), vec![(300, 1300, "one"), (2000, 3000, "two")]);
    }

    #[test]
    fn bad_mark_edits_are_explained_and_change_nothing() {
        let (mut open, track) = with_track(pf_sequence::TimingKind::Lyrics);
        open.apply(
            &[SequenceEdit::AddMarks {
                track,
                marks: vec![Mark::new(0, 1000, "a"), Mark::new(1000, 2000, "b")],
            }],
            None,
        )
        .unwrap();
        let overlap = |index, start, end| SequenceEdit::SetMark {
            track,
            index,
            mark: Mark::new(start, end, ""),
        };
        assert_eq!(
            err(&mut open, overlap(0, 500, 1500)),
            "That would overlap the mark at 0:01.000 on 'Lyrics'; marks on a timing track can't overlap."
        );
        assert_eq!(
            err(&mut open, overlap(1, 1500, 1500)),
            "A mark must end after it starts."
        );
        assert_eq!(
            err(&mut open, overlap(5, 3000, 4000)),
            "That mark isn't on the timing track anymore."
        );
        assert_eq!(
            err(
                &mut open,
                SequenceEdit::AddMarks {
                    track,
                    marks: vec![Mark::new(1900, 2100, "")]
                }
            ),
            "That would overlap the mark at 0:01.000 on 'Lyrics'; marks on a timing track can't overlap."
        );
        assert_eq!(
            err(
                &mut open,
                SequenceEdit::SplitMark {
                    track,
                    index: 0,
                    at_ms: 1000
                }
            ),
            "Split a mark at a time inside it (between 0:00.000 and 0:01.000)."
        );
        assert_eq!(
            err(&mut open, SequenceEdit::MergeMarks { track, index: 1 }),
            "There's no mark after that one to merge it with."
        );
        assert_eq!(
            err(
                &mut open,
                SequenceEdit::RemoveMarks {
                    track,
                    indices: vec![0, 2]
                }
            ),
            "That mark isn't on the timing track anymore."
        );
        assert_eq!(
            err(
                &mut open,
                SequenceEdit::RenameTimingTrack {
                    id: track,
                    name: "  ".into()
                }
            ),
            "A timing track needs a name."
        );
        assert_eq!(
            err(
                &mut open,
                SequenceEdit::AddMarks {
                    track: TimingTrackId::new(),
                    marks: vec![]
                }
            ),
            "That timing track isn't in the sequence anymore."
        );
        // A long label is refused like any other text that's too long.
        let long = "x".repeat(pf_sequence::MAX_TEXT_LEN + 1);
        assert!(
            err(
                &mut open,
                SequenceEdit::LabelMarks {
                    track,
                    indices: vec![0],
                    labels: vec![long]
                }
            )
            .contains("longer than 4096 characters")
        );
        // Phonemes from xLights stay as they are.
        let (mut phonemes, id) = with_track(pf_sequence::TimingKind::Phonemes);
        assert_eq!(
            err(
                &mut phonemes,
                SequenceEdit::AddMarks {
                    track: id,
                    marks: vec![Mark::new(0, 10, "AI")]
                }
            ),
            "Phoneme tracks can't be edited mark by mark; edit the words, then use Break into syllables on the words track."
        );
    }

    #[test]
    fn marks_stay_inside_the_sequence_and_tracks_come_in_tidy() {
        // The sequence is 10 s long.
        let (mut open, track) = with_track(pf_sequence::TimingKind::Lyrics);
        let past = "That would run past the end of the sequence at 0:10.000.";
        assert_eq!(
            err(
                &mut open,
                SequenceEdit::AddMarks {
                    track,
                    marks: vec![Mark::new(9500, 10_500, "")]
                }
            ),
            past
        );
        for edit in [
            SequenceEdit::GenerateMarks {
                track,
                every_ms: 500,
                from_ms: 0,
                to_ms: 12_000,
            },
            SequenceEdit::SpreadLyrics {
                track,
                lines: vec!["la".into()],
                from_ms: 0,
                to_ms: 12_000,
            },
            // Times near the top of the range are refused, not overflowed.
            SequenceEdit::GenerateMarks {
                track,
                every_ms: 10,
                from_ms: u64::MAX - 15,
                to_ms: u64::MAX,
            },
        ] {
            assert_eq!(err(&mut open, edit), past);
        }
        open.apply(
            &[SequenceEdit::AddMarks {
                track,
                marks: vec![Mark::new(9000, 10_000, "end")],
            }],
            None,
        )
        .unwrap();
        assert_eq!(
            err(
                &mut open,
                SequenceEdit::SetMark {
                    track,
                    index: 0,
                    mark: Mark::new(9500, 10_500, "end")
                }
            ),
            past
        );
        // A new or replaced track must have its marks in order, without overlaps.
        let messy = TimingTrack::new(
            "Messy",
            pf_sequence::TimingKind::Custom,
            vec![Mark::new(1000, 2000, ""), Mark::new(0, 1500, "")],
        );
        let refused = "The marks on 'Messy' must be in order, each with some length, and not overlap.";
        assert_eq!(
            err(&mut open, SequenceEdit::AddTimingTrack { track: messy.clone() }),
            refused
        );
        let mut replaced = messy.clone();
        replaced.id = track;
        assert_eq!(
            err(&mut open, SequenceEdit::UpdateTimingTrack { track: replaced }),
            refused
        );
    }

    #[test]
    fn tracks_are_renamed_and_reordered_with_undo() {
        let (mut open, first) = with_track(pf_sequence::TimingKind::Beats);
        let second = TimingTrack::new("Words", pf_sequence::TimingKind::Words, vec![]);
        let second_id = second.id;
        open.apply(&[SequenceEdit::AddTimingTrack { track: second }], None)
            .unwrap();
        let mut copy = open.doc.clone();
        let changes = open
            .apply(
                &[
                    SequenceEdit::RenameTimingTrack {
                        id: first,
                        name: " Beat ".into(),
                    },
                    SequenceEdit::MoveTimingTrack {
                        id: second_id,
                        index: 0,
                    },
                ],
                None,
            )
            .unwrap()
            .unwrap();
        apply_changes(&mut copy, &changes);
        assert_eq!(copy, open.doc);
        let names: Vec<&str> = open.doc.timing_tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["Words", "Beat"]);
        let changes = open.undo().unwrap();
        apply_changes(&mut copy, &changes);
        assert_eq!(copy, open.doc);
        assert_eq!(open.doc.timing_tracks[0].name, "Lyrics");
    }

    #[test]
    fn marks_are_generated_copied_and_lyrics_spread_and_broken_into_words() {
        let (mut open, lyrics) = with_track(pf_sequence::TimingKind::Lyrics);
        let words = TimingTrack::new(
            "Lyrics (words)",
            pf_sequence::TimingKind::Words,
            vec![Mark::new(0, 100, "old")],
        );
        let words_id = words.id;
        let beats = TimingTrack::new("Beats", pf_sequence::TimingKind::Beats, vec![]);
        let beats_id = beats.id;
        let bars = TimingTrack::new("Bars", pf_sequence::TimingKind::Bars, vec![]);
        let bars_id = bars.id;
        open.apply(
            &[
                SequenceEdit::AddTimingTrack { track: words },
                SequenceEdit::AddTimingTrack { track: beats },
                SequenceEdit::AddTimingTrack { track: bars },
            ],
            None,
        )
        .unwrap();
        let t = |open: &OpenSequence, id| open.doc.timing_track(id).unwrap().clone();

        open.apply(
            &[SequenceEdit::GenerateMarks {
                track: beats_id,
                every_ms: 500,
                from_ms: 0,
                to_ms: 4000,
            }],
            None,
        )
        .unwrap();
        assert_eq!(t(&open, beats_id).marks.len(), 8);
        open.apply(
            &[SequenceEdit::CopyMarks {
                from: beats_id,
                to: bars_id,
                every: 4,
            }],
            None,
        )
        .unwrap();
        assert_eq!(spans(&t(&open, bars_id)), vec![(0, 2000, ""), (2000, 4000, "")]);
        // Generating again over part of the song replaces the marks there.
        open.apply(
            &[SequenceEdit::GenerateMarks {
                track: beats_id,
                every_ms: 1000,
                from_ms: 2000,
                to_ms: 4000,
            }],
            None,
        )
        .unwrap();
        assert_eq!(t(&open, beats_id).marks.len(), 6);

        open.apply(
            &[SequenceEdit::SpreadLyrics {
                track: lyrics,
                lines: vec!["Deck the halls".into(), "fa la".into()],
                from_ms: 0,
                to_ms: 2000,
            }],
            None,
        )
        .unwrap();
        // 12 + 4 letters over two seconds.
        assert_eq!(
            spans(&t(&open, lyrics)),
            vec![(0, 1500, "Deck the halls"), (1500, 2000, "fa la")]
        );
        open.apply(
            &[SequenceEdit::BreakIntoWords {
                track: lyrics,
                indices: vec![0, 1],
                words: words_id,
            }],
            None,
        )
        .unwrap();
        assert_eq!(
            spans(&t(&open, words_id)),
            vec![
                (0, 500, "Deck"),
                (500, 875, "the"),
                (875, 1500, "halls"),
                (1500, 1750, "fa"),
                (1750, 2000, "la")
            ],
            "the old word under the phrase is replaced"
        );
        // Lyrics typed onto marks tapped out by ear.
        open.apply(
            &[SequenceEdit::LabelMarks {
                track: bars_id,
                indices: vec![0, 1],
                labels: vec![" Jingle bells ".into(), "jingle all the way".into()],
            }],
            None,
        )
        .unwrap();
        assert_eq!(t(&open, bars_id).marks[0].label, "Jingle bells");

        assert_eq!(
            err(
                &mut open,
                SequenceEdit::LabelMarks {
                    track: bars_id,
                    indices: vec![0],
                    labels: vec!["a".into(), "b".into()]
                }
            ),
            "There are 2 lines of lyrics and 1 chosen mark; choose one mark per line, or spread the lyrics over a time range instead."
        );
        assert_eq!(
            err(
                &mut open,
                SequenceEdit::BreakIntoWords {
                    track: beats_id,
                    indices: vec![0],
                    words: words_id
                }
            ),
            "Those marks have no words in them yet. Give them lyrics first."
        );
        assert_eq!(
            err(
                &mut open,
                SequenceEdit::BreakIntoWords {
                    track: lyrics,
                    indices: vec![0],
                    words: lyrics
                }
            ),
            "Put the words on a different timing track from the phrases."
        );
        assert_eq!(
            err(
                &mut open,
                SequenceEdit::GenerateMarks {
                    track: beats_id,
                    every_ms: 1,
                    from_ms: 0,
                    to_ms: 100
                }
            ),
            "Marks must be at least 10 ms apart."
        );
        assert_eq!(
            err(
                &mut open,
                SequenceEdit::SpreadLyrics {
                    track: lyrics,
                    lines: vec![],
                    from_ms: 0,
                    to_ms: 100
                }
            ),
            "Paste at least one line of lyrics."
        );
        // Each was one undo step: undo them all back to the empty tracks.
        while open.undo().is_some() {}
        assert!(open.doc.timing_tracks.is_empty());
    }

    #[test]
    fn mark_edits_round_trip_as_ui_json() {
        let json = serde_json::json!({ "type": "splitMark",
            "track": "33333333-0000-4000-8000-000000000001", "index": 2, "atMs": 1500 });
        let edit: SequenceEdit = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(&edit).unwrap(), json);
        let json = serde_json::json!({ "type": "setMark",
            "track": "33333333-0000-4000-8000-000000000001", "index": 0,
            "mark": { "startMs": 0, "endMs": 10, "label": "Hi" } });
        let edit: SequenceEdit = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(&edit).unwrap(), json);
    }

    #[test]
    fn settings_outside_their_range_are_refused_with_a_plain_message() {
        let (mut open, row) = open();
        let effect = Effect::new(EffectKind::Chase, 0, 1000);
        let id = effect.id;
        open.apply(
            &[SequenceEdit::AddEffect {
                row,
                layer: 0,
                effect,
            }],
            None,
        )
        .unwrap();
        // 1e39 overflows an f32: it would be infinity, which can't be saved.
        let edit: SequenceEdit = serde_json::from_value(serde_json::json!({
            "type": "setEffectParams", "id": id, "params": { "kind": "chase", "speed": 1e39 } }))
        .unwrap();
        let err = open.apply(&[edit], None).unwrap_err().to_string();
        assert_eq!(
            err,
            "The Chase effect at 0:00.000 has a setting PixelFlow can't use: Speed isn't a usable number; use 0 to 50."
        );
        assert_eq!(
            open.doc.effect(id).unwrap().params,
            EffectParams::default_for(EffectKind::Chase)
        );
    }

    #[test]
    fn documents_with_repeated_ids_still_edit_and_undo() {
        let mut doc = Sequence::new("s", 10_000);
        let row = Row::new(Target::Prop(PropId::new()));
        let row_id = row.id;
        doc.rows = vec![row.clone(), row];
        let mut open = OpenSequence::new(doc.clone(), None, 0);
        let changes = open
            .apply(
                &[SequenceEdit::AddLayer {
                    row: row_id,
                    index: None,
                }],
                None,
            )
            .unwrap()
            .unwrap();
        assert_eq!(changes.rows.len(), 2, "everything is sent");
        assert_eq!(open.doc.rows[0].layers.len(), 2);
        assert_eq!(open.doc.rows[1].layers.len(), 1);
        open.undo().unwrap();
        assert_eq!(open.doc, doc);
        // Removing the repeat makes the ids unique again.
        open.apply(&[SequenceEdit::RemoveRow { id: row_id }], None)
            .unwrap();
        assert!(open.ids_unique);
    }

    #[test]
    fn save_as_keeps_relative_music_pointing_at_the_same_file() {
        let (mut open, _) = open();
        open.apply(
            &[SequenceEdit::UpdateInfo {
                name: "s".into(),
                audio: Some("song.mp3".into()),
                duration_ms: 10_000,
                frame_ms: 25,
            }],
            None,
        )
        .unwrap();
        // Unsaved: relative music is taken to be next to the first file.
        open.rebase_audio(Path::new("/shows/a/song.pfseq.json"));
        assert_eq!(open.doc.audio.as_deref(), Some("song.mp3"));
        open.mark_saved(Path::new("/shows/a/song.pfseq.json"));
        open.apply(
            &[SequenceEdit::UpdateInfo {
                name: "t".into(),
                audio: Some("music/song.mp3".into()),
                duration_ms: 10_000,
                frame_ms: 25,
            }],
            None,
        )
        .unwrap();
        open.rebase_audio(Path::new("/shows/b/song.pfseq.json"));
        assert_eq!(open.doc.audio.as_deref(), Some("/shows/a/music/song.mp3"));
        // Undo history follows, so undoing doesn't bring back a path that means another file.
        open.undo().unwrap();
        assert_eq!(open.doc.audio.as_deref(), Some("/shows/a/song.mp3"));
        open.mark_saved(Path::new("/shows/b/song.pfseq.json"));
        open.rebase_audio(Path::new("/shows/song.pfseq.json"));
        assert_eq!(
            open.doc.audio.as_deref(),
            Some("/shows/a/song.mp3"),
            "full paths stay"
        );
        assert_eq!(
            rebase_audio(
                "a/song.mp3",
                Path::new("/shows"),
                Path::new("/shows/a").parent().unwrap()
            ),
            "a/song.mp3"
        );
        assert_eq!(
            rebase_audio("song.mp3", Path::new("/x/y"), Path::new("/x")),
            "y/song.mp3"
        );
    }
}
