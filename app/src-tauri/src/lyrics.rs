//! Find lyrics: the open sequence's song's words and when each is sung, as Lyrics, Lyrics
//! (words), Lyrics (syllables), Lyrics (phonemes), and Vocals timing tracks, in one undo step.
//! Syllables and mouth shapes can also be made again from a words track already there
//! ([`syllables_from_words`]), the lyrics tracks nudged earlier or later together
//! ([`nudge_lyrics`]), or their words locked onto the song's voice again ([`retime_lyrics`]),
//! nothing looked up.
//!
//! Only when the user presses it, and only once the assistant is set up (see
//! [`pf_ai::lyrics::gate`]). LRCLIB is asked for published lyrics with the song's name and
//! length only; the song's audio goes to OpenAI only with an OpenAI key and the user's say-so
//! (`upload`, asked for in the window). The key never leaves Rust.
//!
//! What was gathered for the last song is kept, so other published lyrics, or lyrics the user
//! pastes, can be lined up again without asking anyone ([`choose_lyrics`]). What went wrong
//! talking to LRCLIB or OpenAI is logged in detail; the user is told in plain words.

use crate::assistant::AiState;
use crate::{AppState, Reply, message};
use pf_ai::lyrics::{self, CandidateView, Choice, Gathered, LyricsCache, LyricsGate, Services, Step};
use pf_ai::{Cancel, ProviderId};
use pf_engine::{EngineError, SequenceEditResult};
use pf_sequence::{TimingKind, TimingTrackId};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tauri::{AppHandle, Emitter, Runtime, State};

/// The event Find lyrics sends as it goes (see [`LyricsProgress`]).
pub(crate) const LYRICS_PROGRESS_EVENT: &str = "lyrics-progress";

/// What Find lyrics is doing now.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LyricsProgress {
    pub label: &'static str,
}

/// What was gathered for the last song lyrics were found for.
struct Last {
    music: PathBuf,
    gathered: Gathered,
}

/// Find lyrics' state: who's asked, where answers are kept, the search in progress, and what
/// was gathered last.
pub(crate) struct LyricsState {
    services: Arc<Services>,
    cache: Option<LyricsCache>,
    running: Mutex<Option<Cancel>>,
    last: Mutex<Option<Last>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl LyricsState {
    pub(crate) fn new(services: Services, cache_dir: Option<PathBuf>) -> Self {
        Self {
            services: Arc::new(services),
            cache: cache_dir.as_deref().map(LyricsCache::new),
            running: Mutex::default(),
            last: Mutex::default(),
        }
    }

    /// LRCLIB and OpenAI over HTTPS, kept in the app's cache folder, problems logged.
    pub(crate) fn live(cache_dir: Option<PathBuf>) -> Self {
        let services = Services {
            log: Box::new(|line| log::warn!("find lyrics: {line}")),
            ..Services::live()
        };
        Self::new(services, cache_dir)
    }
}

/// Marks the search in progress as over when dropped.
struct Running<'a>(&'a Mutex<Option<Cancel>>);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        *lock(self.0) = None;
    }
}

/// Whether Find lyrics can run with the chosen `provider`, and whether it can hear the words in
/// the song itself. Never reads the key out.
#[tauri::command]
pub(crate) async fn lyrics_gate(ai: State<'_, AiState>, provider: Option<ProviderId>) -> Reply<LyricsGate> {
    let vault = ai.vault();
    tauri::async_runtime::spawn_blocking(move || lyrics::gate(&vault, provider))
        .await
        .map_err(|_| "Something went wrong checking the assistant's setup.".to_string())
}

/// What Find lyrics did: the edit's reply, and what to tell the user.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LyricsFound {
    pub result: SequenceEditResult,
    /// "Lyrics from LRCLIB, word timing from OpenAI."
    pub summary: String,
    /// "Lyrics: Lantern Band — Lantern Song (LRCLIB) · word timing: OpenAI"
    pub source: String,
    pub notes: Vec<String>,
    pub lines: usize,
    pub words: usize,
    /// Words whose timing is a guess.
    pub unsure_words: usize,
    /// The published lyrics that could be the song, best first, to pick another.
    pub candidates: Vec<CandidateView>,
    /// The candidate used.
    pub chosen: Option<i64>,
    /// Whether the user's pasted lyrics were used.
    pub pasted: bool,
    /// "Word timing locked to the vocals (average shift 120 ms).", when it was.
    pub timing_note: Option<String>,
}

/// The open sequence's song: its id, music, length, and section starts.
struct SequenceSong {
    doc: u64,
    music: PathBuf,
    duration_ms: u64,
    sections: Vec<u64>,
}

fn sequence_song(state: &AppState) -> Result<SequenceSong, String> {
    let engine = state.engine();
    let doc = engine
        .sequence_doc_id()
        .ok_or_else(|| EngineError::NoSequence.to_string())?;
    let music = engine
        .sequence_music()
        .ok_or_else(|| "This sequence has no music yet. Choose a song for it first.".to_string())?;
    let seq = engine.sequence_document();
    let duration_ms = seq.map_or(0, |s| s.duration_ms);
    let sections: Vec<u64> = seq
        .and_then(|s| s.timing_tracks.iter().find(|t| t.kind == TimingKind::Sections))
        .map(|t| t.marks.iter().map(|m| m.start_ms).collect())
        .unwrap_or_default();
    Ok(SequenceSong {
        doc,
        music,
        duration_ms,
        sections,
    })
}

/// Marks Find lyrics as running (refused while it already is), with the way to stop it.
fn start(lyrics_state: &LyricsState) -> Result<Cancel, String> {
    let mut running = lock(&lyrics_state.running);
    if running.is_some() {
        return Err("Lyrics are already being found for this song.".to_string());
    }
    let cancel = Cancel::new();
    *running = Some(cancel.clone());
    Ok(cancel)
}

/// Adds what was `found` to the sequence as one undo step, unless the sequence or its music
/// changed meanwhile.
fn add_found(state: &AppState, doc: u64, music: &Path, found: lyrics::Found) -> Reply<LyricsFound> {
    let mut engine = state.engine();
    if engine.sequence_doc_id() != Some(doc) || engine.sequence_music().as_deref() != Some(music) {
        return Err(
            "The sequence or its music changed while the lyrics were being found. Find them again."
                .to_string(),
        );
    }
    let existing = engine
        .sequence_document()
        .map(|s| s.timing_tracks.clone())
        .unwrap_or_default();
    let edits = lyrics::tracks::track_edits(&existing, found.tracks);
    let result = engine.edit_sequence(edits).map_err(message)?;
    Ok(LyricsFound {
        result,
        summary: found.summary,
        source: found.source,
        notes: found.notes,
        lines: found.phrases.len(),
        words: found.phrases.iter().map(|p| p.words.len()).sum(),
        unsure_words: found.unsure_words,
        candidates: found.candidates,
        chosen: found.chosen,
        pasted: found.pasted,
        timing_note: found.timing_note,
    })
}

/// Finds the open sequence's lyrics and adds them as timing tracks (one undo step). `upload`:
/// the user agreed to send the song's audio to OpenAI (used only with an OpenAI key).
/// `language`: the user's Lyrics language (ISO 639-1; English when not given), for when
/// neither the published lyrics nor the song's tags say. `fresh`: Find again, without what's
/// kept for the song.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn find_lyrics<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    ai: State<'_, AiState>,
    lyrics_state: State<'_, LyricsState>,
    provider: Option<ProviderId>,
    upload: bool,
    language: Option<String>,
    fresh: Option<bool>,
) -> Reply<LyricsFound> {
    let vault = ai.vault();
    let gate = lyrics::gate(&vault, provider);
    if !gate.ready {
        return Err(gate.reason.unwrap_or_default());
    }
    let language = language
        .as_deref()
        .and_then(lyrics::language::code)
        .unwrap_or_else(|| lyrics::language::DEFAULT.to_string());
    let song = sequence_song(&state)?;
    let recognizer = if gate.recognizer && upload {
        Some(vault.key(ProviderId::Openai).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let cancel = start(&lyrics_state)?;
    let _running = Running(&lyrics_state.running);
    let services = Arc::clone(&lyrics_state.services);
    let cache = lyrics_state.cache.clone();
    let looked = song.music.clone();
    let sections = song.sections.clone();
    let stop = cancel.clone();
    let (found, gathered) = tauri::async_runtime::spawn_blocking(move || {
        let request = lyrics::Request {
            path: &looked,
            duration_ms: song.duration_ms,
            sections_ms: &sections,
            recognizer,
            cache: cache.as_ref(),
            cancel: &stop,
            language: &language,
            fresh: fresh.unwrap_or(false),
        };
        let mut on_step = |step: Step| {
            let _ = app.emit(LYRICS_PROGRESS_EVENT, LyricsProgress { label: step.label() });
        };
        let gathered = lyrics::gather(&services, &request, &mut on_step)?;
        let found = lyrics::assemble(&services, &request, &gathered, &mut on_step)?;
        Ok::<_, String>((found, gathered))
    })
    .await
    .map_err(|_| "Something went wrong finding the lyrics.".to_string())??;
    if cancel.is_cancelled() {
        return Err("Stopped.".to_string());
    }
    *lock(&lyrics_state.last) = Some(Last {
        music: song.music.clone(),
        gathered,
    });
    add_found(&state, song.doc, &song.music, found)
}

/// Lines the open sequence's lyrics up again with other lyrics: another of the published ones
/// found, or lyrics the user pasted (plain lines or LRC). Uses what Find lyrics gathered for
/// the song, asking no one, and keeps the choice for the song. One undo step.
#[tauri::command]
pub(crate) async fn choose_lyrics(
    state: State<'_, AppState>,
    lyrics_state: State<'_, LyricsState>,
    choice: Choice,
) -> Reply<LyricsFound> {
    let song = sequence_song(&state)?;
    let mut gathered = match lock(&lyrics_state.last).as_ref() {
        Some(last) if last.music == song.music => last.gathered.clone(),
        _ => return Err("Find this song's lyrics first.".to_string()),
    };
    gathered.choose(choice, lyrics_state.cache.as_ref())?;
    let cancel = start(&lyrics_state)?;
    let _running = Running(&lyrics_state.running);
    let services = Arc::clone(&lyrics_state.services);
    let cache = lyrics_state.cache.clone();
    let looked = song.music.clone();
    let stop = cancel.clone();
    let kept = gathered.clone();
    let found = tauri::async_runtime::spawn_blocking(move || {
        let request = lyrics::Request {
            path: &looked,
            duration_ms: song.duration_ms,
            sections_ms: &[],
            recognizer: None,
            cache: cache.as_ref(),
            cancel: &stop,
            language: lyrics::language::DEFAULT,
            fresh: false,
        };
        lyrics::assemble(&services, &request, &kept, &mut |_| {})
    })
    .await
    .map_err(|_| "Something went wrong lining up the lyrics.".to_string())??;
    if cancel.is_cancelled() {
        return Err("Stopped.".to_string());
    }
    *lock(&lyrics_state.last) = Some(Last {
        music: song.music.clone(),
        gathered,
    });
    add_found(&state, song.doc, &song.music, found)
}

/// Makes the open sequence's syllables and mouth shapes (its "<name> (syllables)" and "<name>
/// (phonemes)" tracks) again from the words on the words track `track`, without looking the
/// lyrics up: one undo step, replacing the tracks of those names.
pub(crate) fn syllables_from_words_in(
    engine: &mut pf_engine::Engine,
    track: TimingTrackId,
) -> Result<SequenceEditResult, String> {
    let seq = engine
        .sequence_document()
        .ok_or_else(|| EngineError::NoSequence.to_string())?;
    let words = seq
        .timing_track(track)
        .filter(|t| t.kind == TimingKind::Words)
        .ok_or_else(|| "That isn't a words track.".to_string())?;
    if words.marks.is_empty() {
        return Err(format!("{} has no words yet.", words.name));
    }
    let edits = lyrics::tracks::from_words_track(&seq.timing_tracks, words, &[], seq.duration_ms);
    engine.edit_sequence(edits).map_err(message)
}

/// See [`syllables_from_words_in`].
#[tauri::command]
pub(crate) async fn syllables_from_words(
    state: State<'_, AppState>,
    track: TimingTrackId,
) -> Reply<SequenceEditResult> {
    syllables_from_words_in(&mut state.engine(), track)
}

/// Moves the lyrics tracks `track` belongs with (its lines, words, syllables, and phonemes) by
/// `ms` (negative: earlier) together: one undo step.
pub(crate) fn nudge_lyrics_in(
    engine: &mut pf_engine::Engine,
    track: TimingTrackId,
    ms: i64,
) -> Result<SequenceEditResult, String> {
    let seq = engine
        .sequence_document()
        .ok_or_else(|| EngineError::NoSequence.to_string())?;
    let edits = lyrics::tracks::nudge_edits(&seq.timing_tracks, track, ms, seq.duration_ms)
        .ok_or_else(|| "That isn't a lyrics track with marks to move.".to_string())?;
    engine.edit_sequence(edits).map_err(message)
}

/// See [`nudge_lyrics_in`].
#[tauri::command]
pub(crate) async fn nudge_lyrics(
    state: State<'_, AppState>,
    track: TimingTrackId,
    ms: i64,
) -> Reply<SequenceEditResult> {
    nudge_lyrics_in(&mut state.engine(), track, ms)
}

/// What Re-time to vocals did: the edit's reply, and what to tell the user.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LyricsRetimed {
    pub result: SequenceEditResult,
    /// "Word timing locked to the vocals (average shift 40 ms)."
    pub note: String,
}

/// Locks the words on the lyrics tracks `track` belongs with onto `voice` (the song's lead
/// vocal) again, making the lines, syllables, and phonemes again from them: one undo step.
pub(crate) fn retime_lyrics_in(
    engine: &mut pf_engine::Engine,
    track: TimingTrackId,
    voice: &pf_analysis::VocalTrack,
) -> Result<LyricsRetimed, String> {
    let seq = engine
        .sequence_document()
        .ok_or_else(|| EngineError::NoSequence.to_string())?;
    let (edits, report) = lyrics::tracks::retime_edits(&seq.timing_tracks, track, voice, seq.duration_ms)
        .ok_or_else(|| "There are no words to re-time on that lyrics track.".to_string())?;
    let note = report
        .sentence()
        .unwrap_or_else(|| "The words already sit where the vocals start.".to_string());
    let result = engine.edit_sequence(edits).map_err(message)?;
    Ok(LyricsRetimed { result, note })
}

/// Re-time to vocals: locks the words already on the lyrics tracks `track` belongs with onto
/// the song's voice (see [`retime_lyrics_in`]), reading only the song file (or what's kept of
/// it): nothing is sent anywhere.
#[tauri::command]
pub(crate) async fn retime_lyrics(
    state: State<'_, AppState>,
    lyrics_state: State<'_, LyricsState>,
    track: TimingTrackId,
) -> Reply<LyricsRetimed> {
    let song = sequence_song(&state)?;
    let cancel = start(&lyrics_state)?;
    let _running = Running(&lyrics_state.running);
    let services = Arc::clone(&lyrics_state.services);
    let cache = lyrics_state.cache.clone();
    let music = song.music.clone();
    let stop = cancel.clone();
    let voice = tauri::async_runtime::spawn_blocking(move || {
        let hash = lyrics::cache::file_hash(&music, &|| stop.is_cancelled());
        lyrics::read_voice(&services, &music, hash.as_deref(), cache.as_ref(), &stop)
    })
    .await
    .map_err(|_| "Something went wrong listening to the song.".to_string())?
    .ok_or_else(|| "The song's music couldn't be read.".to_string())?;
    if cancel.is_cancelled() {
        return Err("Stopped.".to_string());
    }
    let mut engine = state.engine();
    if engine.sequence_doc_id() != Some(song.doc) || engine.sequence_music().as_deref() != Some(&song.music) {
        return Err("The sequence or its music changed meanwhile. Try again.".to_string());
    }
    retime_lyrics_in(&mut engine, track, &voice)
}

/// Stops finding lyrics (it ends at its next step, adding nothing).
#[tauri::command]
pub(crate) async fn cancel_lyrics(lyrics_state: State<'_, LyricsState>) -> Reply<()> {
    if let Some(cancel) = lock(&lyrics_state.running).as_ref() {
        cancel.cancel();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_engine::Engine;
    use pf_sequence::{Mark, TimingTrack};

    /// A sequence with made-up lyrics tracks: two words, their syllables and mouth shapes.
    fn engine() -> (Engine, tempfile::TempDir, Vec<TimingTrackId>) {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = Engine::new(dir.path());
        engine.new_show("Show");
        engine.new_sequence_doc("Song", 10_000, None).unwrap();
        let tracks = vec![
            TimingTrack::new(
                "Lyrics",
                TimingKind::Lyrics,
                vec![Mark::new(1_000, 2_000, "Paper lantern")],
            ),
            TimingTrack::new(
                "Lyrics (words)",
                TimingKind::Words,
                vec![
                    Mark::new(1_000, 1_400, "Paper"),
                    Mark::new(1_500, 2_000, "lantern"),
                ],
            ),
            TimingTrack::new(
                "Lyrics (syllables)",
                TimingKind::Custom,
                vec![Mark::new(1_000, 1_200, "Pa"), Mark::new(1_200, 1_400, "per")],
            ),
            TimingTrack::new(
                "Lyrics (phonemes)",
                TimingKind::Phonemes,
                vec![Mark::new(1_000, 1_050, "MBP")],
            ),
            TimingTrack::new("Beats", TimingKind::Beats, vec![Mark::new(0, 500, "1")]),
        ];
        let ids = tracks.iter().map(|t| t.id).collect();
        let edits = tracks
            .into_iter()
            .map(|track| pf_engine::SequenceEdit::AddTimingTrack { track })
            .collect();
        engine.edit_sequence(edits).unwrap();
        (engine, dir, ids)
    }

    fn starts(engine: &Engine) -> Vec<(String, u64)> {
        engine
            .sequence_document()
            .unwrap()
            .timing_tracks
            .iter()
            .map(|t| (t.name.clone(), t.marks[0].start_ms))
            .collect()
    }

    #[test]
    fn a_nudge_moves_all_four_lyrics_tracks_in_one_undo_step() {
        let (mut engine, _dir, ids) = engine();
        nudge_lyrics_in(&mut engine, ids[1], -50).unwrap();
        let moved: Vec<u64> = starts(&engine).iter().map(|s| s.1).collect();
        // The four lyrics tracks 50 ms earlier; the beats where they were.
        assert_eq!(moved, [950, 950, 950, 950, 0]);
        engine.undo_sequence().unwrap();
        let back: Vec<u64> = starts(&engine).iter().map(|s| s.1).collect();
        assert_eq!(back, [1_000, 1_000, 1_000, 1_000, 0]);
        assert!(nudge_lyrics_in(&mut engine, ids[4], 10).is_err());
    }

    #[test]
    fn re_time_to_vocals_needs_only_the_voice() {
        let (mut engine, _dir, ids) = engine();
        // A voice with nothing in it: the words keep their time, and the user is told.
        let voice = pf_analysis::VocalTrack {
            hop_ms: 10.0,
            energy: vec![0.0; 1_000],
            onset: vec![0.0; 1_000],
            consonant: vec![0.0; 1_000],
            rise: vec![0.0; 1_000],
            voiced: vec![false; 1_000],
            ..Default::default()
        };
        let done = retime_lyrics_in(&mut engine, ids[0], &voice).unwrap();
        assert_eq!(done.note, "The words already sit where the vocals start.");
        let seq = engine.sequence_document().unwrap();
        assert_eq!(seq.timing_tracks[1].marks[0].start_ms, 1_000);
        // One undo step.
        engine.undo_sequence().unwrap();
        assert!(retime_lyrics_in(&mut engine, ids[4], &voice).is_err());
    }
}
