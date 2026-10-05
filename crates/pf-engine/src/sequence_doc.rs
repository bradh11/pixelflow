//! The open sequence document: edits with their own undo history, files on disk, and what the
//! UI sees after every change. It lives alongside the show (a sequence refers to the show's
//! props and groups but is saved in its own file).

use crate::error::EngineError;
use crate::persist::write_atomic;
use pf_model::Show;
use pf_sequence::{
    Effect, EffectId, EffectParams, MAX_SEQUENCE_BYTES, Row, RowId, Sequence, SequenceIssue, TimingTrack,
    TimingTrackId,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::mem;
use std::path::{Path, PathBuf};

/// Undo steps kept for a sequence.
const UNDO_LIMIT: usize = 200;
/// Approximate memory the sequence undo stack may hold.
const UNDO_BYTE_BUDGET: usize = 256 * 1024 * 1024;

/// One change to the open sequence. Batches are applied atomically by
/// [`crate::Engine::edit_sequence`] as one undo step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

fn not_found(kind: &'static str) -> EngineError {
    EngineError::NotFound { kind }
}

fn check_timing(start_ms: u64, end_ms: u64) -> Result<(), EngineError> {
    if end_ms <= start_ms {
        return Err(EngineError::InvalidEdit(
            "An effect must end after it starts.".to_string(),
        ));
    }
    Ok(())
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
                if doc.timing_track(track.id).is_some() {
                    return Err(EngineError::DuplicateId { kind: "timing track" });
                }
                doc.timing_tracks.push(track.clone());
            }
            SequenceEdit::UpdateTimingTrack { track } => {
                let existing = doc
                    .timing_tracks
                    .iter_mut()
                    .find(|t| t.id == track.id)
                    .ok_or_else(|| not_found("timing track"))?;
                *existing = track.clone();
            }
            SequenceEdit::RemoveTimingTrack { id } => {
                let at = doc
                    .timing_tracks
                    .iter()
                    .position(|t| t.id == *id)
                    .ok_or_else(|| not_found("timing track"))?;
                doc.timing_tracks.remove(at);
            }
        }
        Ok(())
    }
}

/// Ids that must be unique across the document (a batch could add the same id twice).
fn check_unique_ids(doc: &Sequence) -> Result<(), EngineError> {
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

/// What the UI sees of the open sequence after every change.
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

/// A rough in-memory size of a sequence, to bound undo memory.
fn estimated_bytes(doc: &Sequence) -> usize {
    let marks: usize = doc.timing_tracks.iter().map(|t| 64 + 48 * t.marks.len()).sum();
    512 + 96 * doc.rows.len() + 192 * doc.effect_count() + marks
}

/// The open sequence and its undo history.
#[derive(Debug, Clone)]
pub(crate) struct OpenSequence {
    pub doc: Sequence,
    pub path: Option<PathBuf>,
    undo: Vec<(Sequence, usize)>,
    redo: Vec<Sequence>,
    undo_bytes: usize,
    revision: u64,
    saved_revision: u64,
}

impl OpenSequence {
    pub fn new(doc: Sequence, path: Option<PathBuf>, revision: u64) -> Self {
        Self {
            doc,
            path,
            undo: Vec::new(),
            redo: Vec::new(),
            undo_bytes: 0,
            revision,
            saved_revision: revision,
        }
    }

    /// Applies a batch as one undo step. Returns whether anything changed; on error nothing does.
    pub fn apply(&mut self, edits: &[SequenceEdit]) -> Result<bool, EngineError> {
        let mut next = self.doc.clone();
        for edit in edits {
            edit.apply(&mut next)?;
        }
        if let Some(problem) = pf_sequence::limit_problems(&next).into_iter().next() {
            return Err(EngineError::TooLarge(problem));
        }
        check_unique_ids(&next)?;
        if next == self.doc {
            return Ok(false);
        }
        let before = mem::replace(&mut self.doc, next);
        self.record(before);
        self.redo.clear();
        self.revision += 1;
        Ok(true)
    }

    fn record(&mut self, before: Sequence) {
        let bytes = estimated_bytes(&before);
        self.undo.push((before, bytes));
        self.undo_bytes += bytes;
        while self.undo.len() > 1 && (self.undo.len() > UNDO_LIMIT || self.undo_bytes > UNDO_BYTE_BUDGET) {
            let (_, dropped) = self.undo.remove(0);
            self.undo_bytes -= dropped;
        }
    }

    pub fn undo(&mut self) -> bool {
        let Some((previous, bytes)) = self.undo.pop() else {
            return false;
        };
        self.undo_bytes -= bytes;
        let current = mem::replace(&mut self.doc, previous);
        self.redo.push(current);
        self.revision += 1;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let current = mem::replace(&mut self.doc, next);
        self.record(current);
        self.revision += 1;
        true
    }

    pub fn snapshot_revision(&self) -> u64 {
        self.revision
    }

    pub fn mark_saved(&mut self, path: &Path) {
        self.path = Some(path.to_path_buf());
        self.saved_revision = self.revision;
    }

    pub fn snapshot(&self, show: &Show) -> SequenceSnapshot {
        SequenceSnapshot {
            revision: self.revision,
            path: self.path.as_ref().map(|p| p.display().to_string()),
            dirty: self.revision != self.saved_revision,
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            sequence: self.doc.clone(),
            issues: pf_sequence::validate_sequence(&self.doc, show),
        }
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

/// Saves a sequence so that a crash never leaves a half-written file.
pub fn save_sequence_atomic(path: &Path, doc: &Sequence) -> Result<(), EngineError> {
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
            .apply(&[
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
            ])
            .unwrap();
        assert!(changed);
        assert_eq!(
            open.doc.rows[0].layers.len(),
            2,
            "layer one past the top starts a new layer"
        );
        assert_eq!(open.doc.effect(id).unwrap().start_ms, 500);
        let before = open.doc.clone();
        let err = open
            .apply(&[
                SequenceEdit::RemoveEffect { id },
                SequenceEdit::SetEffectTiming {
                    id,
                    start_ms: 0,
                    end_ms: 1,
                },
            ])
            .unwrap_err();
        assert_eq!(err.to_string(), "There is no effect with that id.");
        assert_eq!(open.doc, before, "nothing changed");
        assert!(open.undo());
        assert_eq!(open.doc.effect_count(), 0);
        assert!(open.redo());
        assert_eq!(open.doc, before);
    }

    #[test]
    fn bad_edits_are_explained() {
        let (mut open, row) = open();
        let err = open
            .apply(&[SequenceEdit::AddEffect {
                row,
                layer: 0,
                effect: Effect::new(EffectKind::On, 500, 500),
            }])
            .unwrap_err();
        assert_eq!(err.to_string(), "An effect must end after it starts.");
        let effect = Effect::new(EffectKind::On, 0, 10);
        let err = open
            .apply(&[
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
            ])
            .unwrap_err();
        assert_eq!(err.to_string(), "An effect with that id already exists.");
        let err = open
            .apply(&[SequenceEdit::AddEffect {
                row,
                layer: 5,
                effect: Effect::new(EffectKind::On, 0, 10),
            }])
            .unwrap_err();
        assert_eq!(err.to_string(), "That row has no layer 6.");
        let err = open
            .apply(&[SequenceEdit::UpdateInfo {
                name: "s".into(),
                audio: None,
                duration_ms: 99_999_999_999,
                frame_ms: 25,
            }])
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
        open.apply(&[
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
        ])
        .unwrap();
        assert_eq!(open.doc.rows[1].id, second_id, "moved to the end");
        assert_eq!(open.doc.locate_effect(id), Some((1, 1, 0)));
        assert_eq!(open.doc.effect(id).unwrap().start_ms, 2000);

        // A move to a missing layer fails without losing the effect.
        let err = open
            .apply(&[SequenceEdit::MoveEffect {
                id,
                row: first,
                layer: 3,
                start_ms: 0,
                end_ms: 10,
            }])
            .unwrap_err();
        assert_eq!(err.to_string(), "That row has no layer 4.");
        assert!(open.doc.effect(id).is_some());

        open.apply(&[
            SequenceEdit::RemoveLayer {
                row: second_id,
                layer: 1,
            },
            SequenceEdit::RemoveRow { id: first },
        ])
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
}
