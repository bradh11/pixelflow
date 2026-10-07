//! The open sequence's song: analyzed (tempo, beats, bars, energy, sections) the first time the
//! assistant asks, kept for the chat, and turned into timing tracks in the draft. The analysis
//! runs on the chat's thread, never holding the engine, and stops when the user presses Stop.
//! The song file is only ever read.

use crate::draft::Draft;
use crate::provider::Cancel;
use pf_analysis::Analysis;
use pf_engine::SequenceEdit;
use pf_sequence::TimingTrack;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Analyzes a song file, giving up when cancelled.
pub type Analyzer = dyn Fn(&Path, &Cancel) -> Result<Analysis, String> + Send + Sync;

/// The real analysis ([`pf_analysis`]).
pub fn default_analyzer() -> Arc<Analyzer> {
    Arc::new(|path: &Path, cancel: &Cancel| {
        pf_analysis::analyze_file_cancellable(path, &|| cancel.is_cancelled()).map_err(|e| e.to_string())
    })
}

/// The song as one tool call sees it.
pub struct Song<'a> {
    /// The open sequence's music file, if it has one.
    pub music: Option<&'a Path>,
    /// The last analysis, by file.
    pub cache: &'a mut Option<(PathBuf, Arc<Analysis>)>,
    pub analyzer: &'a Analyzer,
    pub cancel: &'a Cancel,
}

/// Bar times listed by `analyze_song`, at most.
const MAX_LISTED_BARS: usize = 400;

impl Song<'_> {
    /// The song's analysis, run now if this song hasn't been analyzed yet.
    pub fn analysis(&mut self, draft: &Draft) -> Result<Arc<Analysis>, String> {
        if draft.sequence().is_none() {
            return Err(
                "No sequence is open. Offer ask_for_song so the user can pick a song for a new one.".into(),
            );
        }
        let Some(path) = self.music else {
            return Err(
                "This sequence has no song. Its music is set on the Sequence screen; or use ask_for_song for a new sequence from a song.".into(),
            );
        };
        if let Some((cached, analysis)) = self.cache.as_ref()
            && cached == path
        {
            return Ok(Arc::clone(analysis));
        }
        let analysis = Arc::new((self.analyzer)(path, self.cancel)?);
        *self.cache = Some((path.to_path_buf(), Arc::clone(&analysis)));
        Ok(analysis)
    }
}

/// What `analyze_song` answers: tempo, counts, bar times, and sections with their energy.
pub fn describe(analysis: &Analysis) -> Value {
    let sections: Vec<Value> = analysis
        .sections()
        .iter()
        .map(|s| {
            json!({
                "label": s.label,
                "startMs": s.start_ms,
                "endMs": s.end_ms,
                "energy": s.energy,
                "level": s.level,
            })
        })
        .collect();
    json!({
        "durationMs": analysis.duration_ms,
        "tempoBpm": analysis.tempo_bpm.map(|t| (t * 10.0).round() / 10.0),
        "beats": analysis.beats.len(),
        "barsMs": analysis.bars.iter().take(MAX_LISTED_BARS).collect::<Vec<_>>(),
        "sections": sections,
    })
}

/// The tracks `add_song_timing` can add, by name.
pub const TRACK_CHOICES: [&str; 4] = ["beats", "bars", "sections", "onsets"];

/// Adds the song's timing tracks to the draft (one draft step); a track already there by name
/// and kind is reused. Answers each track's name, id, mark count, and whether it was added.
pub fn add_timing(analysis: &Analysis, draft: &mut Draft, wanted: &[String]) -> Result<Value, String> {
    let doc = draft
        .sequence()
        .ok_or("No sequence is open. Offer ask_for_song first.")?;
    let [beats, bars, onsets] = <[TimingTrack; 3]>::try_from(analysis.timing_tracks())
        .map_err(|_| "The song's analysis came back incomplete.".to_string())?;
    let mut out = Vec::new();
    let mut edits = Vec::new();
    for name in wanted {
        let track = match name.as_str() {
            "beats" => beats.clone(),
            "bars" => bars.clone(),
            "sections" => analysis.sections_track(),
            "onsets" => onsets.clone(),
            other => {
                return Err(format!(
                    "There's no \"{other}\" timing; choose from {}.",
                    TRACK_CHOICES.join(", ")
                ));
            }
        };
        let existing = doc
            .timing_tracks
            .iter()
            .find(|t| t.name == track.name && t.kind == track.kind);
        match existing {
            Some(t) => {
                out.push(json!({ "name": t.name, "id": t.id, "marks": t.marks.len(), "added": false }))
            }
            None => {
                out.push(
                    json!({ "name": track.name, "id": track.id, "marks": track.marks.len(), "added": true }),
                );
                edits.push(SequenceEdit::AddTimingTrack { track });
            }
        }
    }
    if !edits.is_empty() {
        draft.edit_sequence_batch(edits).map_err(|e| e.to_string())?;
    }
    Ok(Value::Array(out))
}

/// A timing track named or identified by `key` (its id or its name, any case).
pub fn find_track<'a>(tracks: &'a [TimingTrack], key: &str) -> Option<&'a TimingTrack> {
    tracks
        .iter()
        .find(|t| t.id.to_string() == key)
        .or_else(|| tracks.iter().find(|t| t.name.eq_ignore_ascii_case(key.trim())))
}
