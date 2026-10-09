//! Find lyrics: the open sequence's song's words and when each is sung, as Lyrics, Lyrics
//! (words), Lyrics (syllables), Lyrics (phonemes), and Vocals timing tracks, in one undo step.
//! Syllables and mouth shapes can also be made again from a words track already there
//! ([`syllables_from_words`]), nothing looked up.
//!
//! Only when the user presses it, and only once the assistant is set up (see
//! [`pf_ai::lyrics::gate`]). LRCLIB is asked for published lyrics with the song's name and
//! length only; the song's audio goes to OpenAI only with an OpenAI key and the user's say-so
//! (`upload`, asked for in the window). The key never leaves Rust.

use crate::assistant::AiState;
use crate::{AppState, Reply, message};
use pf_ai::lyrics::{self, LyricsCache, LyricsGate, Services, Step};
use pf_ai::{Cancel, ProviderId};
use pf_engine::{EngineError, SequenceEditResult};
use pf_sequence::{TimingKind, TimingTrackId};
use serde::Serialize;
use std::path::PathBuf;
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

/// Find lyrics' state: who's asked, where answers are kept, and the search in progress.
pub(crate) struct LyricsState {
    services: Arc<Services>,
    cache: Option<LyricsCache>,
    running: Mutex<Option<Cancel>>,
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
        }
    }

    /// LRCLIB and OpenAI over HTTPS, kept in the app's cache folder.
    pub(crate) fn live(cache_dir: Option<PathBuf>) -> Self {
        Self::new(Services::live(), cache_dir)
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
    pub notes: Vec<String>,
    pub lines: usize,
    pub words: usize,
    /// Words whose timing is a guess.
    pub unsure_words: usize,
}

/// Finds the open sequence's lyrics and adds them as timing tracks (one undo step). `upload`:
/// the user agreed to send the song's audio to OpenAI (used only with an OpenAI key).
#[tauri::command]
pub(crate) async fn find_lyrics<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    ai: State<'_, AiState>,
    lyrics_state: State<'_, LyricsState>,
    provider: Option<ProviderId>,
    upload: bool,
) -> Reply<LyricsFound> {
    let vault = ai.vault();
    let gate = lyrics::gate(&vault, provider);
    if !gate.ready {
        return Err(gate.reason.unwrap_or_default());
    }
    let (doc, music, duration_ms, sections) = {
        let engine = state.engine();
        let doc = engine
            .sequence_doc_id()
            .ok_or_else(|| EngineError::NoSequence.to_string())?;
        let music = engine
            .sequence_music()
            .ok_or_else(|| "This sequence has no music yet. Choose a song for it first.".to_string())?;
        let seq = engine.sequence_document();
        let duration = seq.map_or(0, |s| s.duration_ms);
        let sections: Vec<u64> = seq
            .and_then(|s| s.timing_tracks.iter().find(|t| t.kind == TimingKind::Sections))
            .map(|t| t.marks.iter().map(|m| m.start_ms).collect())
            .unwrap_or_default();
        (doc, music, duration, sections)
    };
    let recognizer = if gate.recognizer && upload {
        Some(vault.key(ProviderId::Openai).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let cancel = {
        let mut running = lock(&lyrics_state.running);
        if running.is_some() {
            return Err("Lyrics are already being found for this song.".to_string());
        }
        let cancel = Cancel::new();
        *running = Some(cancel.clone());
        cancel
    };
    let _running = Running(&lyrics_state.running);
    let services = Arc::clone(&lyrics_state.services);
    let cache = lyrics_state.cache.clone();
    let looked = music.clone();
    let stop = cancel.clone();
    let found = tauri::async_runtime::spawn_blocking(move || {
        let request = lyrics::Request {
            path: &looked,
            duration_ms,
            sections_ms: &sections,
            recognizer,
            cache: cache.as_ref(),
            cancel: &stop,
        };
        lyrics::find_lyrics(&services, &request, &mut |step: Step| {
            let _ = app.emit(LYRICS_PROGRESS_EVENT, LyricsProgress { label: step.label() });
        })
    })
    .await
    .map_err(|_| "Something went wrong finding the lyrics.".to_string())??;
    if cancel.is_cancelled() {
        return Err("Stopped.".to_string());
    }
    let mut engine = state.engine();
    if engine.sequence_doc_id() != Some(doc) || engine.sequence_music().as_deref() != Some(music.as_path()) {
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
        notes: found.notes,
        lines: found.phrases.len(),
        words: found.phrases.iter().map(|p| p.words.len()).sum(),
        unsure_words: found.unsure_words,
    })
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

/// Stops finding lyrics (it ends at its next step, adding nothing).
#[tauri::command]
pub(crate) async fn cancel_lyrics(lyrics_state: State<'_, LyricsState>) -> Reply<()> {
    if let Some(cancel) = lock(&lyrics_state.running).as_ref() {
        cancel.cancel();
    }
    Ok(())
}
