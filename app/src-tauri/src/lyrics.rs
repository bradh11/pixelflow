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
//!
//! **On-device alignment** (`align`, the user's Settings → AI choice): the words are lined up
//! with the song on this computer ([`pf_ai::lyrics::forced`]), with its model downloaded once,
//! only when the user agrees ([`download_alignment_models`]), into the app's data folder.

use crate::assistant::AiState;
use crate::progress::{AUDIO_PROGRESS_EVENT, AudioProgress, AudioTask, Throttle};
use crate::{AppState, Reply, message};
use pf_ai::lyrics::forced::{OnDevice, SongAligner};
use pf_ai::lyrics::{self, CandidateView, Choice, Gathered, LyricsCache, LyricsGate, Services, Step};
use pf_ai::{Cancel, ProviderId};
use pf_align::ModelStore;
use pf_engine::{EngineError, SequenceEditResult};
use pf_sequence::{TimingKind, TimingTrackId};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Runtime, State};

/// The event Find lyrics sends as it goes (see [`LyricsProgress`]).
pub(crate) const LYRICS_PROGRESS_EVENT: &str = "lyrics-progress";

/// What Find lyrics is doing now.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LyricsProgress {
    pub label: &'static str,
    /// How far a step that reads the whole song has got (0–1); none at the start of a step,
    /// and for steps that can't tell.
    pub fraction: Option<f32>,
}

/// What was gathered for the last song lyrics were found for.
struct Last {
    music: PathBuf,
    gathered: Gathered,
}

/// Fetches a model's files into its folder, telling its second argument the bytes so far and
/// the bytes in all, until done or its third says to stop.
type ModelFetch = dyn Fn(&ModelStore, &dyn Fn(u64, u64), &dyn Fn() -> bool) -> Result<(), pf_align::AlignError>
    + Send
    + Sync;

/// Find lyrics' state: who's asked, where answers are kept, the search in progress, what was
/// gathered last, and the on-device aligner's model.
pub(crate) struct LyricsState {
    services: Arc<Services>,
    cache: Option<LyricsCache>,
    running: Mutex<Option<Cancel>>,
    last: Mutex<Option<Last>>,
    /// Where the alignment model is kept, when the app has a data folder.
    models: Option<ModelStore>,
    /// The aligner, loaded once it's first used.
    aligner: Mutex<Option<Arc<OnDevice>>>,
    /// The model download in progress.
    downloading: Mutex<Option<Cancel>>,
    /// How the model is downloaded ([`ModelStore::download`]; tests use a stand-in).
    fetch: Arc<ModelFetch>,
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
            models: None,
            aligner: Mutex::default(),
            downloading: Mutex::default(),
            fetch: Arc::new(ModelStore::download),
        }
    }

    /// The alignment model kept under `data_dir`.
    pub(crate) fn with_models(mut self, data_dir: Option<PathBuf>) -> Self {
        self.models = data_dir.map(|dir| ModelStore::new(&dir, pf_align::WAV2VEC2));
        self
    }

    /// The model kept in `store`, fetched by `fetch` rather than over the network.
    #[cfg(test)]
    pub(crate) fn with_model_fetch(mut self, store: ModelStore, fetch: Arc<ModelFetch>) -> Self {
        self.models = Some(store);
        self.fetch = fetch;
        self
    }

    /// LRCLIB and OpenAI over HTTPS, kept in the app's cache folder, problems logged; the
    /// alignment model in its data folder.
    pub(crate) fn live(cache_dir: Option<PathBuf>, data_dir: Option<PathBuf>) -> Self {
        let services = Services {
            log: Box::new(|line| log::warn!("find lyrics: {line}")),
            ..Services::live()
        };
        Self::new(services, cache_dir).with_models(data_dir)
    }

    /// The on-device aligner, when its model is downloaded (loaded once, on first use).
    fn aligner(&self) -> Option<Arc<OnDevice>> {
        let store = self.models.as_ref()?;
        let mut kept = lock(&self.aligner);
        if kept.is_none() {
            *kept = OnDevice::new(store).map(Arc::new);
        }
        kept.clone().filter(|_| store.is_installed())
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

/// Sends the progress of an on-device alignment step on `path` as the window's other long
/// music work does ([`AUDIO_PROGRESS_EVENT`]).
fn alignment_progress<R: Runtime>(app: &AppHandle<R>, step: &str, path: &Path, fraction: f32) {
    let task = if step == Step::Separating.label() {
        AudioTask::Separate
    } else if step == Step::Aligning.label() {
        AudioTask::Align
    } else {
        return;
    };
    let _ = app.emit(
        AUDIO_PROGRESS_EVENT,
        AudioProgress {
            task,
            path: pf_model::path_to_text(path),
            stage: task.stage(),
            fraction,
        },
    );
}

/// Finds the open sequence's lyrics and adds them as timing tracks (one undo step). `upload`:
/// the user agreed to send the song's audio to OpenAI (used only with an OpenAI key).
/// `language`: the user's Lyrics language (ISO 639-1; English when not given), for when
/// neither the published lyrics nor the song's tags say. `fresh`: Find again, without what's
/// kept for the song. `align`: time the words on this computer, when its model is downloaded.
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
    align: Option<bool>,
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
    let aligner = if align.unwrap_or(false) {
        lyrics_state.aligner()
    } else {
        None
    };
    let looked = song.music.clone();
    let sections = song.sections.clone();
    let stop = cancel.clone();
    let (found, gathered) = tauri::async_runtime::spawn_blocking(move || {
        // The step the song is being read for, and how often a reading reports.
        let at = std::cell::Cell::new(Step::ReadingSong.label());
        let throttle = Throttle::default();
        let progress = |fraction: f32| {
            if throttle.due(fraction, Instant::now()) {
                let label = at.get();
                let _ = app.emit(
                    LYRICS_PROGRESS_EVENT,
                    LyricsProgress {
                        label,
                        fraction: Some(fraction),
                    },
                );
                alignment_progress(&app, label, &looked, fraction);
            }
        };
        let request = lyrics::Request {
            path: &looked,
            duration_ms: song.duration_ms,
            sections_ms: &sections,
            recognizer,
            cache: cache.as_ref(),
            cancel: &stop,
            language: &language,
            fresh: fresh.unwrap_or(false),
            progress: &progress,
            aligner: aligner.as_deref().map(|a| a as &dyn SongAligner),
        };
        let mut on_step = |step: Step| {
            at.set(step.label());
            let _ = app.emit(
                LYRICS_PROGRESS_EVENT,
                LyricsProgress {
                    label: step.label(),
                    fraction: None,
                },
            );
            alignment_progress(&app, step.label(), &looked, 0.0);
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
/// the song, asking no one, and keeps the choice for the song. One undo step. `align`: time the
/// words on this computer, as [`find_lyrics`].
#[tauri::command]
pub(crate) async fn choose_lyrics(
    state: State<'_, AppState>,
    lyrics_state: State<'_, LyricsState>,
    choice: Choice,
    align: Option<bool>,
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
    let aligner = if align.unwrap_or(false) {
        lyrics_state.aligner()
    } else {
        None
    };
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
            progress: &|_| {},
            aligner: aligner.as_deref().map(|a| a as &dyn SongAligner),
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
        lyrics::read_voice(&services, &music, hash.as_deref(), cache.as_ref(), &stop, &|_| {})
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

/// The event the alignment model's download reports its progress with.
pub(crate) const ALIGN_MODELS_PROGRESS_EVENT: &str = "align-models-progress";

/// The on-device alignment model, as Settings shows it: what it is, where it comes from, how
/// big, and whether it's here.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AlignModels {
    /// Whether the app has somewhere to keep it.
    pub available: bool,
    pub installed: bool,
    pub downloading: bool,
    pub name: &'static str,
    pub licence: &'static str,
    /// The page that describes it.
    pub source: &'static str,
    /// Where each file is downloaded from.
    pub urls: Vec<&'static str>,
    pub bytes: u64,
}

/// How far the model's download has got.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelsProgress {
    pub received: u64,
    pub total: u64,
}

fn align_models(lyrics_state: &LyricsState) -> AlignModels {
    let manifest = pf_align::WAV2VEC2;
    AlignModels {
        available: lyrics_state.models.is_some(),
        installed: lyrics_state.models.as_ref().is_some_and(ModelStore::is_installed),
        downloading: lock(&lyrics_state.downloading).is_some(),
        name: manifest.name,
        licence: manifest.licence,
        source: manifest.source,
        urls: manifest.files.iter().map(|f| f.url).collect(),
        bytes: manifest.bytes(),
    }
}

/// The on-device alignment model: what it is and whether it's downloaded.
#[tauri::command]
pub(crate) async fn alignment_models(lyrics_state: State<'_, LyricsState>) -> Reply<AlignModels> {
    Ok(align_models(&lyrics_state))
}

/// Downloads the on-device alignment model (after the user agreed in Settings), each file
/// checked against its SHA-256 before it's kept. Reports its progress
/// ([`ALIGN_MODELS_PROGRESS_EVENT`]); [`cancel_alignment_download`] stops it, keeping nothing.
#[tauri::command]
pub(crate) async fn download_alignment_models<R: Runtime>(
    app: AppHandle<R>,
    lyrics_state: State<'_, LyricsState>,
) -> Reply<AlignModels> {
    let store = lyrics_state
        .models
        .clone()
        .ok_or_else(|| "PixelFlow has no folder to keep the model in.".to_string())?;
    let cancel = {
        let mut downloading = lock(&lyrics_state.downloading);
        if downloading.is_some() {
            return Err("The model is already downloading.".to_string());
        }
        let cancel = Cancel::new();
        *downloading = Some(cancel.clone());
        cancel
    };
    let _downloading = Running(&lyrics_state.downloading);
    let stop = cancel.clone();
    let fetch = Arc::clone(&lyrics_state.fetch);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let throttle = Throttle::default();
        fetch(
            &store,
            &|received, total| {
                let fraction = received as f32 / total.max(1) as f32;
                if throttle.due(fraction, Instant::now()) {
                    let _ = app.emit(ALIGN_MODELS_PROGRESS_EVENT, ModelsProgress { received, total });
                }
            },
            &|| stop.is_cancelled(),
        )
    })
    .await
    .map_err(|_| "Something went wrong downloading the model.".to_string())?;
    match result {
        Ok(()) => {}
        Err(pf_align::AlignError::Cancelled) => return Err("Stopped.".to_string()),
        Err(error) => {
            log::warn!("alignment model download: {error:?}");
            return Err(error.to_string());
        }
    }
    drop(_downloading);
    Ok(align_models(&lyrics_state))
}

/// Stops the model download (nothing half-downloaded is kept).
#[tauri::command]
pub(crate) async fn cancel_alignment_download(lyrics_state: State<'_, LyricsState>) -> Reply<()> {
    if let Some(cancel) = lock(&lyrics_state.downloading).as_ref() {
        cancel.cancel();
    }
    Ok(())
}

/// Deletes the on-device alignment model from the app's data folder.
#[tauri::command]
pub(crate) async fn remove_alignment_models(lyrics_state: State<'_, LyricsState>) -> Reply<AlignModels> {
    if lock(&lyrics_state.downloading).is_some() {
        return Err("Wait for the download to finish, or stop it, first.".to_string());
    }
    *lock(&lyrics_state.aligner) = None;
    if let Some(store) = &lyrics_state.models {
        store.remove().map_err(|e| {
            log::warn!("removing the alignment model: {e:?}");
            "The model couldn't be removed.".to_string()
        })?;
    }
    Ok(align_models(&lyrics_state))
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
    use crate::testing::{call, call_at, files_under, heard};
    use pf_ai::testing::{FAKE_KEY, FakeTransport, Reply};
    use pf_align::{Manifest, ModelFile};
    use pf_engine::Engine;
    use pf_sequence::{Mark, TimingTrack};
    use serde_json::{Value, json};
    use tauri::test::MockRuntime;
    use tauri::{App, Listener, WebviewWindow, WebviewWindowBuilder};

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

    /// Run once while a song's voice is read: a step that reports nothing of its own to cancel
    /// at.
    type WhileReading = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;

    /// The app with the assistant set up and Find lyrics' services faked: LRCLIB answers with
    /// recorded replies, and the song's voice is "read" in two reports. No keychain, no network.
    struct Finder {
        app: App<MockRuntime>,
        webview: WebviewWindow<MockRuntime>,
        lrclib: Arc<FakeTransport>,
        while_reading: WhileReading,
        dir: tempfile::TempDir,
    }

    /// A five-byte model ("hello"), with its SHA-256.
    const MODEL: Manifest = Manifest {
        id: "test-model",
        name: "Test",
        licence: "MIT",
        source: "https://example.invalid",
        files: &[ModelFile {
            name: "model.onnx",
            url: "https://example.invalid/model.onnx",
            sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            bytes: 5,
        }],
    };

    /// A file that arrives a byte at a time, so its download has steps to report and to be
    /// stopped between.
    struct Trickle(&'static [u8]);

    impl std::io::Read for Trickle {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            match (self.0.split_first(), buf.first_mut()) {
                (Some((byte, rest)), Some(slot)) => {
                    *slot = *byte;
                    self.0 = rest;
                    Ok(1)
                }
                _ => Ok(0),
            }
        }
    }

    /// Downloads `store`'s model as [`ModelStore::download`] does, but from memory.
    fn fetch_from_memory(
        store: &ModelStore,
        progress: &dyn Fn(u64, u64),
        stop: &dyn Fn() -> bool,
    ) -> Result<(), pf_align::AlignError> {
        let total = store.manifest().bytes();
        for file in store.manifest().files {
            store.install_from(file, &mut Trickle(b"hello"), &|n| progress(n, total), stop)?;
        }
        progress(total, total);
        Ok(())
    }

    fn finder() -> Finder {
        let dir = tempfile::tempdir().unwrap();
        let lrclib = Arc::new(FakeTransport::default());
        let nobody = || Arc::new(FakeTransport::default());
        let while_reading = WhileReading::default();
        let reading = Arc::clone(&while_reading);
        let services = Services {
            lrclib: lyrics::lrclib::Lrclib::new(lrclib.clone()).with_retry_delay(std::time::Duration::ZERO),
            transcriber: lyrics::transcribe::Transcriber::new(nobody()),
            voice: Box::new(move |_, _, progress| {
                progress(0.0);
                let then = lock(&reading).take();
                if let Some(then) = then {
                    then();
                }
                progress(1.0);
                Ok(pf_analysis::VocalTrack::default())
            }),
            tags: Box::new(|path| pf_audio::read_tags(path).ok()),
            log: Box::new(|_| {}),
        };
        let vault = pf_ai::KeyVault::new(Box::new(pf_ai::MemoryStore::new()));
        let app = crate::testing::builder(dir.path())
            .manage(AiState::new(vault, pf_ai::providers(nobody())))
            .manage(LyricsState::new(services, None).with_model_fetch(
                ModelStore::new(&dir.path().join("data"), MODEL),
                Arc::new(fetch_from_memory),
            ))
            .build(crate::context())
            .unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        // With an Anthropic key the song's words are only looked up: its audio goes nowhere.
        call(
            &webview,
            "set_api_key",
            json!({ "provider": "anthropic", "key": FAKE_KEY }),
        )
        .unwrap();
        // A new sequence whose music is a silent 4 s WAV named like the song.
        let song = dir.path().join("01 - Lantern Song.wav");
        std::fs::write(&song, pf_audio::wav_bytes(&vec![0.0; 64_000], 16_000)).unwrap();
        call(
            &webview,
            "new_sequence_doc",
            json!({ "name": "Lantern Song", "durationMs": 4_000, "audio": song.to_string_lossy() }),
        )
        .unwrap();
        Finder {
            app,
            webview,
            lrclib,
            while_reading,
            dir,
        }
    }

    impl Finder {
        /// Asks for the song's lyrics, LRCLIB answering with made-up ones.
        fn find(&self) -> Result<Value, Value> {
            self.lrclib.push(Reply::ok(
                json!([{
                    "id": 1, "trackName": "Lantern Song", "artistName": "Lantern Band", "albumName": "Made Up",
                    "duration": 4.0, "instrumental": false,
                    "plainLyrics": "Paper lanterns glowing\nSnowy rooftops shine",
                    "syncedLyrics": "[00:00.50]Paper lanterns glowing\n[00:02.00]Snowy rooftops shine\n[00:03.50]",
                }])
                .to_string(),
            ));
            call(
                &self.webview,
                "find_lyrics",
                json!({ "provider": "anthropic", "upload": false }),
            )
        }

        /// The open sequence's timing tracks.
        fn tracks(&self) -> Value {
            call(&self.webview, "get_sequence_doc", json!({})).unwrap()["sequence"]["timingTracks"].clone()
        }

        /// Runs `then` the next time a song's voice is read, and keeps what it answers.
        fn while_reading<T: Send + 'static>(
            &self,
            then: impl FnOnce(&WebviewWindow<MockRuntime>) -> T + Send + 'static,
        ) -> Arc<Mutex<Option<T>>> {
            let answer = Arc::new(Mutex::new(None));
            let kept = Arc::clone(&answer);
            let webview = self.webview.clone();
            *lock(&self.while_reading) = Some(Box::new(move || *lock(&kept) = Some(then(&webview))));
            answer
        }
    }

    fn step(label: &str) -> Value {
        json!({ "label": label, "fraction": null })
    }

    #[test]
    fn finding_lyrics_reports_each_step_and_how_far_the_song_has_been_read() {
        let f = finder();
        let steps = heard(&f.app, "lyrics-progress");
        let music = heard(&f.app, "audio-progress");
        f.find().unwrap();
        assert_eq!(
            *lock(&steps),
            [
                step("Reading the song"),
                step("Looking up published lyrics"),
                step("Lining up the words"),
                json!({ "label": "Lining up the words", "fraction": 0.0 }),
                json!({ "label": "Lining up the words", "fraction": 1.0 }),
            ]
        );
        // Nothing was aligned on this computer, so there's no music work to show a bar for.
        assert_eq!(*lock(&music), Vec::<Value>::new());
        assert_eq!(f.tracks().as_array().unwrap().len(), 5);
    }

    #[test]
    fn a_search_stopped_at_its_first_step_asks_no_one_and_the_next_one_runs() {
        let f = finder();
        let steps = heard(&f.app, "lyrics-progress");
        // Nothing is being found: stopping does nothing, now or to the search after it.
        call(&f.webview, "cancel_lyrics", json!({})).unwrap();

        call_at(&f.app, &f.webview, "lyrics-progress", |_| true, "cancel_lyrics");
        assert_eq!(f.find().unwrap_err(), "Stopped.");
        assert_eq!(std::mem::take(&mut *lock(&steps)), [step("Reading the song")]);
        assert!(f.lrclib.requests().is_empty());
        assert_eq!(f.tracks(), json!([]));
        let other = call(
            &f.webview,
            "choose_lyrics",
            json!({ "choice": { "candidate": 1 } }),
        );
        assert_eq!(other.unwrap_err(), "Find this song's lyrics first.");

        // The stop is spent, and the search is no longer running: the next one finds them.
        f.find().unwrap();
        assert_eq!(lock(&steps).len(), 5);
        assert_eq!(f.tracks().as_array().unwrap().len(), 5);
    }

    #[test]
    fn a_search_stopped_while_the_words_are_lined_up_adds_nothing() {
        let f = finder();
        call_at(
            &f.app,
            &f.webview,
            "lyrics-progress",
            |progress| progress["label"] == "Lining up the words" && progress["fraction"] == 0.0,
            "cancel_lyrics",
        );
        assert_eq!(f.find().unwrap_err(), "Stopped.");
        // The lyrics were looked up, but nothing of them is kept or added.
        assert_eq!(f.lrclib.requests().len(), 1);
        assert_eq!(f.tracks(), json!([]));
        let doc = call(&f.webview, "get_sequence_doc", json!({})).unwrap();
        assert_eq!((&doc["canUndo"], &doc["dirty"]), (&json!(false), &json!(false)));
        let other = call(
            &f.webview,
            "choose_lyrics",
            json!({ "choice": { "candidate": 1 } }),
        );
        assert_eq!(other.unwrap_err(), "Find this song's lyrics first.");
        f.find().unwrap();
        assert_eq!(f.tracks().as_array().unwrap().len(), 5);
    }

    #[test]
    fn lining_up_other_lyrics_and_re_timing_stop_too_and_only_one_runs_at_a_time() {
        let f = finder();
        f.find().unwrap();
        let found = f.tracks();
        let pasted = json!({ "choice": { "pasted": "[00:01.00]Paper lanterns\n[00:02.50]Snowy rooftops\n[00:03.50]" } });
        let lyrics = found[0]["id"].clone();

        for (command, args, done) in [
            (
                "retime_lyrics",
                json!({ "track": lyrics }),
                ("note", json!("The words already sit where the vocals start.")),
            ),
            ("choose_lyrics", pasted, ("pasted", json!(true))),
        ] {
            // While it reads the song's voice: a search is refused, then it's stopped.
            let refused = f.while_reading(|webview| {
                let refused = call(
                    webview,
                    "find_lyrics",
                    json!({ "provider": "anthropic", "upload": false }),
                );
                call(webview, "cancel_lyrics", json!({})).unwrap();
                refused
            });
            assert_eq!(
                call(&f.webview, command, args.clone()).unwrap_err(),
                "Stopped.",
                "{command}"
            );
            assert_eq!(
                lock(&refused).take().unwrap().unwrap_err(),
                "Lyrics are already being found for this song."
            );
            assert_eq!(f.tracks(), found, "{command}");
            // Not stopped the next time.
            let reply = call(&f.webview, command, args).unwrap();
            assert_eq!(reply[done.0], done.1, "{command}");
        }
        assert_eq!(f.tracks()[0]["marks"][0]["label"], "Paper lanterns");
        assert_eq!(f.lrclib.requests().len(), 1, "only the search asked LRCLIB");
    }

    #[test]
    fn the_model_download_reports_its_bytes_and_stops_keeping_nothing() {
        let f = finder();
        let models = f.dir.path().join("data");
        let state = |webview: &WebviewWindow<MockRuntime>| {
            let models = call(webview, "alignment_models", json!({})).unwrap();
            (
                models["installed"].as_bool().unwrap(),
                models["downloading"].as_bool().unwrap(),
            )
        };
        assert_eq!(state(&f.webview), (false, false));
        let events = heard(&f.app, "align-models-progress");
        // Nothing is downloading: stopping does nothing, now or to the download after it.
        call(&f.webview, "cancel_alignment_download", json!({})).unwrap();

        // As the first bytes are reported: it's downloading, a second download and a removal are
        // refused, and then it's stopped.
        let during: Arc<Mutex<Vec<Value>>> = Arc::default();
        let seen = Arc::clone(&during);
        let webview = f.webview.clone();
        f.app.once_any("align-models-progress", move |_| {
            let refused = |command: &str| call(&webview, command, json!({})).unwrap_err();
            *lock(&seen) = vec![
                json!(state(&webview)),
                refused("download_alignment_models"),
                refused("remove_alignment_models"),
            ];
            call(&webview, "cancel_alignment_download", json!({})).unwrap();
        });
        let stopped = call(&f.webview, "download_alignment_models", json!({})).unwrap_err();
        assert_eq!(stopped, "Stopped.");
        assert_eq!(
            *lock(&during),
            [
                json!([false, true]),
                json!("The model is already downloading."),
                json!("Wait for the download to finish, or stop it, first."),
            ]
        );
        assert_eq!(
            std::mem::take(&mut *lock(&events)),
            [json!({ "received": 1, "total": 5 })]
        );
        assert_eq!(files_under(&models), Vec::<String>::new());
        assert_eq!(state(&f.webview), (false, false));

        // The stop is spent: the next download runs to its last byte and keeps the model.
        let done = call(&f.webview, "download_alignment_models", json!({})).unwrap();
        assert_eq!(
            (&done["installed"], &done["downloading"]),
            (&json!(true), &json!(false))
        );
        let sent = lock(&events).clone();
        assert_eq!(sent[0], json!({ "received": 1, "total": 5 }));
        assert_eq!(sent.last().unwrap(), &json!({ "received": 5, "total": 5 }));
        let received: Vec<u64> = sent.iter().map(|e| e["received"].as_u64().unwrap()).collect();
        assert!(received.windows(2).all(|pair| pair[0] <= pair[1]), "{received:?}");
        assert!(
            sent.iter()
                .all(|e| e["total"] == 5 && e.as_object().unwrap().len() == 2)
        );
        assert_eq!(files_under(&models), ["models/test-model/model.onnx"]);

        let removed = call(&f.webview, "remove_alignment_models", json!({})).unwrap();
        assert_eq!(removed["installed"], false);
        assert_eq!(files_under(&models), Vec::<String>::new());
    }
}
