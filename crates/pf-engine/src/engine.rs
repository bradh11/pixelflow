//! The engine: owns the show, applies edits, saves, and runs live output.

use crate::edit::Edit;
use crate::error::EngineError;
use crate::history::History;
use crate::output::{OutputSession, OutputStatus, PatternSpec, TargetSpec, output_key};
use crate::persist::{self, HistoryEntry};
use crate::playback::{
    self, ClockFactory, DocumentRequest, PlayRequest, PlaybackReady, PlaybackSession, PlaybackStatus,
    SessionKind, document_music,
};
use crate::recovery::{self, SequenceRecovery};
use crate::sequence_doc::{self, OpenSequence, SequenceEdit, SequenceEditResult, SequenceSnapshot};
use crate::snapshot::{PreviewProp, ShowSnapshot, Summary};
use pf_mapping::ChannelMap;
use pf_model::{IssueCode, SequenceId, Severity, Show, ValidationReport};
use pf_output::{OutputSettings, Transport, UdpTransport};
use pf_patterns::{Target, resolve_target};
use pf_render::Renderer;
use pf_render::export::{ExportLayout, ExportSummary};
use pf_sequence::Sequence;
use std::io;
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Undo steps kept in memory.
const UNDO_LIMIT: usize = 200;
/// Approximate memory the undo stack may hold.
const UNDO_BYTE_BUDGET: usize = 256 * 1024 * 1024;
/// Autosaved versions kept on disk per show.
const AUTOSAVE_KEEP: usize = 50;

type TransportFactory = Box<dyn Fn() -> io::Result<Box<dyn Transport>> + Send>;

/// A show checked exactly as opening a show file checks it (size limits included), ready for
/// [`Engine::adopt_show`]. Checking a large show takes a while, so it happens before the engine
/// is locked.
#[derive(Debug, Clone)]
pub struct CheckedShow(Show);

impl CheckedShow {
    pub fn new(show: Show) -> Result<Self, EngineError> {
        pf_model::check_show(&show)
            .map(CheckedShow)
            .map_err(|e| EngineError::InvalidShow(e.to_string()))
    }
}

/// The single owner of the open show.
pub struct Engine {
    show: Show,
    path: Option<PathBuf>,
    revision: u64,
    saved_revision: u64,
    autosaved_revision: u64,
    history: History,
    data_dir: PathBuf,
    output: Option<OutputSession>,
    playback: Option<PlaybackSession>,
    output_generation: u64,
    playback_generation: u64,
    stop_reason: Option<String>,
    playback_stop_reason: Option<String>,
    /// One sACN identity for the engine's lifetime, so restarts keep the same source.
    output_settings: OutputSettings,
    transport: TransportFactory,
    clocks: ClockFactory,
    /// Music volume for playback (0.0–1.0), kept across sequences.
    volume: f32,
    /// The open sequence document, if any.
    sequence: Option<OpenSequence>,
    /// Counts sequence document changes across documents, so snapshot revisions only grow.
    sequence_revision: u64,
    /// A renderer for previewing the open sequence, and the show revision it was made for.
    preview_renderer: Option<(u64, Renderer)>,
    /// Whether a playing sequence document is sent to the controllers (else only the preview).
    send_sequence_doc: bool,
    /// Names this run's kept unsaved sequence (see [`Engine::autosave_sequence`]).
    session: String,
    /// The sequence document and revision last kept, so an unchanged one isn't written again.
    sequence_autosaved: Option<(u64, u64)>,
}

/// Everything needed to export the open sequence, copied out of the engine so a long export
/// doesn't hold it.
#[derive(Debug, Clone)]
pub struct SequenceExport {
    show: Show,
    map: ChannelMap,
    sequence: Sequence,
    /// The show's first error, if it has any (noted in the summary).
    show_error: Option<String>,
}

impl SequenceExport {
    /// The channel space the file will use.
    pub fn layout(&self) -> ExportLayout {
        pf_render::export::export_layout(&self.show, &self.map)
    }

    /// Renders every frame and writes the `.fseq` file atomically. `progress` gets (frames done,
    /// total frames) and returns `false` to cancel (the error says so, and no file is written).
    pub fn run(
        &self,
        path: &Path,
        progress: impl FnMut(u32, u32) -> bool,
    ) -> Result<ExportSummary, EngineError> {
        let mut summary =
            pf_render::export::export_fseq_file(&self.show, &self.map, &self.sequence, path, progress)
                .map_err(|e| EngineError::Export(e.to_string()))?;
        if let Some(error) = &self.show_error {
            summary.notes.insert(
                0,
                format!(
                    "The show has errors, so some props may be missing or wrong in this file. Fix them and export again: {error}"
                ),
            );
        }
        Ok(summary)
    }
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("show", &self.show.name)
            .field("path", &self.path)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// An engine with a new untitled show. `data_dir` holds autosave history.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            show: Show::new("Untitled Show"),
            path: None,
            revision: 0,
            saved_revision: 0,
            autosaved_revision: 0,
            history: History::new(UNDO_LIMIT, UNDO_BYTE_BUDGET),
            data_dir: data_dir.into(),
            output: None,
            playback: None,
            output_generation: 0,
            playback_generation: 0,
            stop_reason: None,
            playback_stop_reason: None,
            output_settings: OutputSettings::default(),
            transport: Box::new(|| {
                let udp = UdpTransport::bind("0.0.0.0:0".parse().expect("valid address"))?;
                Ok(Box::new(udp) as Box<dyn Transport>)
            }),
            clocks: playback::music_clocks(),
            volume: 1.0,
            sequence: None,
            sequence_revision: 0,
            preview_renderer: None,
            send_sequence_doc: true,
            session: recovery::new_session(),
            sequence_autosaved: None,
        }
    }

    /// Replaces how playback keeps time (tests use a silent clock instead of the sound output).
    pub fn with_clocks(mut self, clocks: ClockFactory) -> Self {
        self.clocks = clocks;
        self
    }

    /// Replaces how output sockets are created (tests use an in-memory recorder).
    pub fn with_transport(
        mut self,
        factory: impl Fn() -> io::Result<Box<dyn Transport>> + Send + 'static,
    ) -> Self {
        self.transport = Box::new(factory);
        self
    }

    pub fn show(&self) -> &Show {
        &self.show
    }

    /// Goes up by one with every change to the show (edits, undo, redo, opening another show).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The full state for the UI.
    pub fn snapshot(&self) -> ShowSnapshot {
        let (map, report) = analyze(&self.show);
        let mut issues = report.issues;
        issues.sort_by_key(|i| std::cmp::Reverse(i.severity));
        ShowSnapshot {
            revision: self.revision,
            path: self.path.as_ref().map(|p| p.display().to_string()),
            dirty: self.revision != self.saved_revision,
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
            summary: Summary {
                props: self.show.props.len(),
                pixels: self.show.props.iter().map(|p| u64::from(p.node_count())).sum(),
                controllers: self.show.controllers.len(),
                universes: map.universe_count(),
            },
            show: self.show.clone(),
            issues,
            channel_map: map,
        }
    }

    /// Applies a batch of edits as one undo step. Nothing changes if any edit fails or the
    /// result would exceed PixelFlow's size limits.
    pub fn apply(&mut self, edits: Vec<Edit>) -> Result<ShowSnapshot, EngineError> {
        let mut next = self.show.clone();
        for edit in &edits {
            edit.apply(&mut next)?;
        }
        let report = pf_model::validate_show(&next);
        if let Some(issue) = report.issues.iter().find(|i| i.code == IssueCode::LimitExceeded) {
            return Err(EngineError::TooLarge(issue.message.clone()));
        }
        if next == self.show {
            return Ok(self.snapshot());
        }
        let before = mem::replace(&mut self.show, next);
        self.history.record(before);
        self.changed();
        Ok(self.snapshot())
    }

    pub fn undo(&mut self) -> ShowSnapshot {
        if self.history.can_undo() {
            let current = mem::replace(&mut self.show, Show::new(""));
            if let Some(previous) = self.history.undo(current) {
                self.show = previous;
            }
            self.changed();
        }
        self.snapshot()
    }

    pub fn redo(&mut self) -> ShowSnapshot {
        if self.history.can_redo() {
            let current = mem::replace(&mut self.show, Show::new(""));
            if let Some(next) = self.history.redo(current) {
                self.show = next;
            }
            self.changed();
        }
        self.snapshot()
    }

    /// Starts a new, empty, unsaved show (stops output and clears undo history).
    pub fn new_show(&mut self, name: &str) -> ShowSnapshot {
        self.replace_show(Show::new(name), None);
        self.snapshot()
    }

    /// Replaces the open show with `show` (one imported from xLights, for example) as a new,
    /// unsaved show. The show was checked like a file being opened when `show` was made.
    pub fn adopt_show(&mut self, show: CheckedShow) -> ShowSnapshot {
        self.replace_show(show.0, None);
        // Unsaved, so the user is asked before it's discarded.
        self.changed();
        self.snapshot()
    }

    /// Opens a show file. On failure the current show is left untouched.
    pub fn open(&mut self, path: &Path) -> Result<ShowSnapshot, EngineError> {
        let show = persist::load_show(path)?;
        self.replace_show(show, Some(path.to_path_buf()));
        Ok(self.snapshot())
    }

    /// Saves to the current file.
    pub fn save(&mut self) -> Result<ShowSnapshot, EngineError> {
        let path = self.path.clone().ok_or(EngineError::NoPath)?;
        self.save_as(&path)
    }

    /// Saves to `path` and makes it the current file.
    pub fn save_as(&mut self, path: &Path) -> Result<ShowSnapshot, EngineError> {
        persist::save_show_atomic(path, &self.show)?;
        self.path = Some(path.to_path_buf());
        self.saved_revision = self.revision;
        // The history folder follows the file, so the next autosave writes a copy there.
        self.autosaved_revision = u64::MAX;
        Ok(self.snapshot())
    }

    /// Writes a history copy if the show changed since the last autosave.
    pub fn autosave(&mut self) -> Result<Option<HistoryEntry>, EngineError> {
        if self.revision == self.autosaved_revision {
            return Ok(None);
        }
        let entry = persist::write_history(&self.history_dir(), &self.show, AUTOSAVE_KEEP)?;
        self.autosaved_revision = self.revision;
        Ok(Some(entry))
    }

    /// Autosaved versions of the current show, newest first.
    pub fn history(&self) -> Vec<HistoryEntry> {
        persist::list_history(&self.history_dir())
    }

    /// Restores an autosaved version as an undoable change.
    pub fn restore(&mut self, id: &str) -> Result<ShowSnapshot, EngineError> {
        if id.contains(['/', '\\']) || !self.history().iter().any(|e| e.id == id) {
            return Err(EngineError::UnknownHistoryEntry);
        }
        let restored = persist::load_show(&self.history_dir().join(id))?;
        let before = mem::replace(&mut self.show, restored);
        self.history.record(before);
        self.changed();
        Ok(self.snapshot())
    }

    /// Starts (or replaces) a live test pattern. Refuses when the show has errors.
    pub fn start_output(
        &mut self,
        pattern: PatternSpec,
        target: TargetSpec,
    ) -> Result<OutputStatus, EngineError> {
        pattern.to_pattern()?;
        let (map, report) = analyze(&self.show);
        if let Some(error) = first_error(&report) {
            return Err(EngineError::ShowHasErrors(error.message.clone()));
        }
        if resolve_target(&self.show, &map, &Target::from(&target)).is_empty() {
            return Err(EngineError::NothingToLight);
        }
        self.launch(map, pattern, target)
    }

    /// Starts a session for an already-validated show, replacing any running one.
    fn launch(
        &mut self,
        map: ChannelMap,
        pattern: PatternSpec,
        target: TargetSpec,
    ) -> Result<OutputStatus, EngineError> {
        self.stop_session();
        self.stop_playback();
        let transport = (self.transport)().map_err(EngineError::Network)?;
        self.output_generation += 1;
        let session = OutputSession::start(
            &self.show,
            &map,
            pattern,
            target,
            transport,
            self.output_generation,
            self.output_settings.clone(),
        )?;
        let status = session.status();
        self.output = Some(session);
        self.stop_reason = None;
        Ok(status)
    }

    /// Stops live output (controllers are blacked out).
    pub fn stop_output(&mut self) -> OutputStatus {
        self.stop_session();
        self.stop_reason = None;
        OutputStatus::stopped(self.output_generation, None)
    }

    pub fn output_status(&self) -> OutputStatus {
        self.output.as_ref().map_or_else(
            || OutputStatus::stopped(self.output_generation, self.stop_reason.clone()),
            OutputSession::status,
        )
    }

    /// The latest painted frame while output runs (prop order, RGB/RGBW per pixel).
    pub fn preview_frame(&self) -> Option<Vec<u8>> {
        self.output.as_ref().map(OutputSession::preview)
    }

    /// Plays a rendered sequence (`.fseq`) from `position_ms` to every controller that knows
    /// its sequence channels, without music. Stops a running test pattern or sequence first.
    pub fn start_playback(&mut self, path: &Path, position_ms: u64) -> Result<PlaybackStatus, EngineError> {
        let request = PlayRequest {
            path: path.to_path_buf(),
            music: None,
            offset_ms: 0,
            volume: self.volume,
            sequence: None,
        };
        self.play(&request, position_ms, false)?.wait();
        self.current_status()
    }

    /// Plays one of the show's sequences with its music, lined up by its offset, once the music
    /// is open (waiting while holding the engine: see [`Engine::begin_sequence`] to wait without).
    pub fn play_sequence(&mut self, id: SequenceId, position_ms: u64) -> Result<PlaybackStatus, EngineError> {
        self.begin_sequence(id, position_ms)?.wait();
        self.current_status()
    }

    /// Starts one of the show's sequences with its music and returns at once, with something to
    /// wait on until the music is open (opening a sound device can take a moment).
    pub fn begin_sequence(&mut self, id: SequenceId, position_ms: u64) -> Result<PlaybackReady, EngineError> {
        let entry = self
            .show
            .sequences
            .iter()
            .find(|s| s.id == id)
            .ok_or(EngineError::NotFound { kind: "sequence" })?
            .clone();
        let request = PlayRequest {
            path: PathBuf::from(&entry.path),
            music: entry.audio.as_ref().map(PathBuf::from),
            offset_ms: entry.offset_ms,
            volume: self.volume,
            sequence: Some(entry.id),
        };
        self.play(&request, position_ms, false)
    }

    /// Adds a sequence to the show as one undo step. One with the same name as another gets a
    /// number, like "Medley (2)", so the two can be told apart.
    pub fn add_sequence(
        &mut self,
        mut sequence: pf_model::SequenceEntry,
    ) -> Result<ShowSnapshot, EngineError> {
        let taken = |name: &str| self.show.sequences.iter().any(|s| s.name == name);
        if taken(&sequence.name) {
            let base = sequence.name.clone();
            let mut n = 2;
            while taken(&format!("{base} ({n})")) {
                n += 1;
            }
            sequence.name = format!("{base} ({n})");
        }
        self.apply(vec![Edit::AddSequence { sequence }])
    }

    fn current_status(&self) -> Result<PlaybackStatus, EngineError> {
        self.playback_status()
            .ok_or_else(|| EngineError::Playback("Playback stopped before it started.".to_string()))
    }

    fn play(
        &mut self,
        request: &PlayRequest,
        position_ms: u64,
        paused: bool,
    ) -> Result<PlaybackReady, EngineError> {
        self.stop_session();
        self.stop_reason = None;
        self.stop_playback();
        let (map, _) = analyze(&self.show);
        let transport = (self.transport)().map_err(EngineError::Network)?;
        self.playback_generation += 1;
        let mut session = PlaybackSession::start(
            &self.show,
            &map,
            request,
            position_ms,
            paused,
            transport,
            self.output_settings.clone(),
            &self.clocks,
        )?;
        let ready = session.take_ready();
        self.playback = Some(session);
        self.playback_stop_reason = None;
        Ok(ready)
    }

    /// Sets the music volume (0.0–1.0) for playback, now and later.
    pub fn set_playback_volume(&mut self, volume: f32) -> Option<PlaybackStatus> {
        self.volume = volume.clamp(0.0, 1.0);
        let session = self.playback.as_ref()?;
        session.set_volume(self.volume);
        Some(session.status())
    }

    /// Pauses or resumes playback (controllers keep showing the paused frame).
    pub fn set_playback_paused(&mut self, paused: bool) -> Option<PlaybackStatus> {
        let session = self.playback.as_ref()?;
        session.set_paused(paused);
        Some(session.status())
    }

    /// Jumps to `position_ms` in the playing sequence.
    pub fn seek_playback(&mut self, position_ms: u64) -> Option<PlaybackStatus> {
        let session = self.playback.as_ref()?;
        session.seek(position_ms);
        Some(session.status())
    }

    /// Stops playback (controllers are blacked out).
    pub fn stop_playback(&mut self) {
        if let Some(session) = self.playback.take() {
            session.stop();
        }
        self.playback_stop_reason = None;
    }

    /// Why playback was stopped by an edit to the show, if it was (cleared by the next start or stop).
    pub fn playback_stop_reason(&self) -> Option<&str> {
        self.playback_stop_reason.as_deref()
    }

    /// Counts playback sessions started; it changes when an edit restarts playback.
    pub fn playback_generation(&self) -> u64 {
        self.playback_generation
    }

    /// The playing sequence's state, or `None` when nothing is playing.
    pub fn playback_status(&self) -> Option<PlaybackStatus> {
        self.playback.as_ref().map(PlaybackSession::status)
    }

    /// The props as they look right now: the playing sequence's frame, else the test pattern's
    /// (prop order, RGB/RGBW per pixel).
    pub fn live_frame(&self) -> Option<Vec<u8>> {
        self.playback
            .as_ref()
            .map(PlaybackSession::preview)
            .or_else(|| self.preview_frame())
    }

    /// Every prop's pixel positions for the 2D preview (front view: x right, y up), with where
    /// its colors sit in [`Engine::live_frame`].
    pub fn preview_props(&self) -> Vec<PreviewProp> {
        self.preview_with(|p| [p.x, p.y].into_iter())
    }

    /// Every prop's pixel positions for the 3D view (x right, y up, z toward the street), as
    /// x, y, z triples, with where its colors sit in [`Engine::live_frame`].
    pub fn preview_props_3d(&self) -> Vec<PreviewProp> {
        self.preview_with(|p| [p.x, p.y, p.z].into_iter())
    }

    fn preview_with<I: Iterator<Item = f32>>(
        &self,
        coords: impl Fn(pf_model::Vec3) -> I,
    ) -> Vec<PreviewProp> {
        let (map, _) = analyze(&self.show);
        self.show
            .props
            .iter()
            .filter_map(|prop| {
                let layout = map.prop_layout(prop.id)?;
                let points = pf_geometry::world_positions(prop)
                    .into_iter()
                    .take(layout.nodes as usize)
                    .flat_map(&coords)
                    .collect();
                Some(PreviewProp {
                    prop: prop.id,
                    frame_offset: layout.frame_offset,
                    channels_per_pixel: layout.channels_per_pixel,
                    points,
                })
            })
            .collect()
    }

    /// The playing sequence's current frame: every channel, as sent to the controllers.
    pub fn sequence_frame(&self) -> Option<Vec<u8>> {
        self.playback.as_ref().and_then(PlaybackSession::sequence_frame)
    }

    // --- Sequence documents -------------------------------------------------------------------

    /// Starts a new, unsaved sequence document with `audio` as its music, if given (replacing
    /// the open one, without asking). It starts with nothing to undo.
    pub fn new_sequence_doc(
        &mut self,
        name: &str,
        duration_ms: u64,
        audio: Option<&str>,
    ) -> Result<SequenceSnapshot, EngineError> {
        let mut doc = Sequence::new(name, duration_ms);
        doc.audio = audio.filter(|a| !a.trim().is_empty()).map(str::to_owned);
        if let Some(problem) = pf_sequence::limit_problems(&doc).into_iter().next() {
            return Err(EngineError::TooLarge(problem));
        }
        self.replace_sequence(OpenSequence::new(doc, None, self.sequence_revision + 1));
        Ok(self.sequence_snapshot_unchecked())
    }

    /// Opens a sequence built elsewhere (an import) as a new, unsaved document with unsaved
    /// changes, replacing the open one without asking. It is checked exactly as opening a file
    /// would check it; on failure the open sequence is left untouched.
    pub fn adopt_sequence_doc(&mut self, doc: Sequence) -> Result<SequenceSnapshot, EngineError> {
        let doc = pf_sequence::check_sequence(&doc).map_err(|e| EngineError::TooLarge(e.to_string()))?;
        self.replace_sequence(OpenSequence::unsaved(doc, self.sequence_revision + 1));
        Ok(self.sequence_snapshot_unchecked())
    }

    /// Opens a sequence file. On failure the open sequence is left untouched.
    pub fn open_sequence_doc(&mut self, path: &Path) -> Result<SequenceSnapshot, EngineError> {
        let doc = sequence_doc::load_sequence(path)?;
        self.replace_sequence(OpenSequence::new(
            doc,
            Some(path.to_path_buf()),
            self.sequence_revision + 1,
        ));
        Ok(self.sequence_snapshot_unchecked())
    }

    /// Saves the open sequence to its file.
    pub fn save_sequence_doc(&mut self) -> Result<SequenceSnapshot, EngineError> {
        let open = self.sequence.as_ref().ok_or(EngineError::NoSequence)?;
        let path = open.path.clone().ok_or(EngineError::SequenceNoPath)?;
        self.save_sequence_doc_as(&path)
    }

    /// Saves the open sequence to `path` and makes it the sequence's file. Music given relative
    /// to the old file's folder is rewritten to stay the same file (relative to the new folder
    /// when it's inside it, otherwise as a full path); an unsaved sequence's relative music is
    /// taken to be next to the new file.
    pub fn save_sequence_doc_as(&mut self, path: &Path) -> Result<SequenceSnapshot, EngineError> {
        let open = self.sequence.as_mut().ok_or(EngineError::NoSequence)?;
        let mut moved = open.clone();
        moved.rebase_audio(path);
        sequence_doc::save_sequence_atomic(path, &moved.doc)?;
        *open = moved;
        open.mark_saved(path);
        // Saved: there's nothing left to recover.
        self.forget_sequence_autosave();
        Ok(self.sequence_snapshot_unchecked())
    }

    /// Closes the open sequence (stopping it if it's playing).
    pub fn close_sequence_doc(&mut self) {
        self.stop_document_playback();
        self.sequence = None;
        self.forget_sequence_autosave();
    }

    /// Keeps the open sequence on disk while it has unsaved changes (call it now and then, and
    /// when the app quits), so the work can be recovered if PixelFlow closes without saving it.
    /// Writes only when the sequence changed since it was last kept; returns whether it wrote.
    /// A saved, closed, or replaced sequence's copy is removed.
    pub fn autosave_sequence(&mut self) -> Result<bool, EngineError> {
        let Some(open) = self.sequence.as_ref().filter(|o| o.is_dirty()) else {
            self.forget_sequence_autosave();
            return Ok(false);
        };
        let kept = (open.id(), open.snapshot_revision());
        if self.sequence_autosaved == Some(kept) {
            return Ok(false);
        }
        recovery::write(
            &recovery::dir(&self.data_dir),
            &self.session,
            &open.doc,
            open.path.as_deref(),
        )?;
        self.sequence_autosaved = Some(kept);
        Ok(true)
    }

    /// Unsaved sequences kept by earlier runs of PixelFlow (newest first), to offer back.
    pub fn sequence_recoveries(&self) -> Vec<SequenceRecovery> {
        recovery::list(&recovery::dir(&self.data_dir), &self.session)
    }

    /// Opens a kept unsaved sequence (replacing the open one, without asking). It opens with
    /// unsaved changes and its old file, if it had one, so Save writes it back there. Its kept
    /// copy is then this run's to keep up to date.
    pub fn recover_sequence(&mut self, id: &str) -> Result<SequenceSnapshot, EngineError> {
        let dir = recovery::dir(&self.data_dir);
        let (doc, path) = recovery::load(&dir, id, &self.session)?;
        if let Some(problem) = pf_sequence::limit_problems(&doc).into_iter().next() {
            return Err(EngineError::TooLarge(problem));
        }
        let mut open = OpenSequence::new(doc, path, self.sequence_revision + 1);
        open.mark_unsaved();
        self.replace_sequence(open);
        recovery::remove(&dir, id);
        // Kept again under this run's name at once; if that fails, the next autosave tries again
        // (the sequence stays open and unsaved meanwhile).
        let _ = self.autosave_sequence();
        Ok(self.sequence_snapshot_unchecked())
    }

    /// Throws away a kept unsaved sequence from an earlier run.
    pub fn discard_sequence_recovery(&mut self, id: &str) {
        if id != self.session {
            recovery::remove(&recovery::dir(&self.data_dir), id);
        }
    }

    fn forget_sequence_autosave(&mut self) {
        recovery::remove(&recovery::dir(&self.data_dir), &self.session);
        self.sequence_autosaved = None;
    }

    /// Identifies the open sequence document: it stays the same across edits and changes when
    /// another document is opened or created (to tell, after a slow job, whether it's still the
    /// one the job started on).
    pub fn sequence_doc_id(&self) -> Option<u64> {
        self.sequence.as_ref().map(OpenSequence::id)
    }

    /// The whole open sequence, for the UI (when it opens one, or to resync).
    pub fn sequence_doc(&self) -> Option<SequenceSnapshot> {
        self.sequence.as_ref().map(|open| open.snapshot(&self.show))
    }

    fn sequence_snapshot_unchecked(&self) -> SequenceSnapshot {
        self.sequence_doc().expect("a sequence is open")
    }

    /// Applies a batch of edits to the open sequence as one undo step. Nothing changes if any
    /// edit fails, a setting is outside its range, or the result would exceed the size limits.
    /// A playing sequence shows the change from its next frame. The reply lists what changed,
    /// not the whole document (see [`Engine::sequence_doc`] for that).
    pub fn edit_sequence(&mut self, edits: Vec<SequenceEdit>) -> Result<SequenceEditResult, EngineError> {
        self.edit_sequence_gesture(edits, None)
    }

    /// Like [`Engine::edit_sequence`], as part of a gesture (a drag, say): consecutive edits with
    /// the same `gesture` id merge into one undo step, so undo takes back the whole gesture. Any
    /// other edit, an undo, or a redo in between ends the gesture.
    pub fn edit_sequence_gesture(
        &mut self,
        edits: Vec<SequenceEdit>,
        gesture: Option<&str>,
    ) -> Result<SequenceEditResult, EngineError> {
        let open = self.sequence.as_mut().ok_or(EngineError::NoSequence)?;
        let changes = open.apply(&edits, gesture)?;
        if changes.is_some() {
            self.sequence_changed();
        }
        Ok(self.sequence_result(changes))
    }

    pub fn undo_sequence(&mut self) -> Result<SequenceEditResult, EngineError> {
        let open = self.sequence.as_mut().ok_or(EngineError::NoSequence)?;
        let changes = open.undo();
        if changes.is_some() {
            self.sequence_changed();
        }
        Ok(self.sequence_result(changes))
    }

    pub fn redo_sequence(&mut self) -> Result<SequenceEditResult, EngineError> {
        let open = self.sequence.as_mut().ok_or(EngineError::NoSequence)?;
        let changes = open.redo();
        if changes.is_some() {
            self.sequence_changed();
        }
        Ok(self.sequence_result(changes))
    }

    fn sequence_result(&self, changes: Option<sequence_doc::SequenceChanges>) -> SequenceEditResult {
        self.sequence
            .as_ref()
            .expect("a sequence is open")
            .edit_result(changes, &self.show)
    }

    /// The open sequence's music file (relative paths resolved next to the document), if any.
    pub fn sequence_music(&self) -> Option<PathBuf> {
        let open = self.sequence.as_ref()?;
        document_music(open.path.as_deref(), open.doc.audio.as_deref())
    }

    /// Adds timing tracks (from beat detection, say) as one undo step, replacing any tracks
    /// with the same names so running detection again doesn't pile up copies.
    pub fn replace_timing_tracks(
        &mut self,
        tracks: Vec<pf_sequence::TimingTrack>,
    ) -> Result<SequenceEditResult, EngineError> {
        let open = self.sequence.as_ref().ok_or(EngineError::NoSequence)?;
        let mut edits: Vec<SequenceEdit> = open
            .doc
            .timing_tracks
            .iter()
            .filter(|t| tracks.iter().any(|n| n.name == t.name))
            .map(|t| SequenceEdit::RemoveTimingTrack { id: t.id })
            .collect();
        edits.extend(
            tracks
                .into_iter()
                .map(|track| SequenceEdit::AddTimingTrack { track }),
        );
        self.edit_sequence(edits)
    }

    /// The open sequence as it looks at `position_ms` (a show frame: prop order, RGB/RGBW per
    /// pixel), for scrubbing the timeline without playing.
    pub fn sequence_doc_frame(&mut self, position_ms: u64) -> Option<Vec<u8>> {
        let open = self.sequence.as_ref()?;
        if self
            .preview_renderer
            .as_ref()
            .is_none_or(|(rev, _)| *rev != self.revision)
        {
            let (map, _) = analyze(&self.show);
            self.preview_renderer = Some((self.revision, Renderer::new(&self.show, &map)));
        }
        let (_, renderer) = self.preview_renderer.as_mut()?;
        let mut frame = vec![0u8; renderer.frame_len()];
        renderer.render(&open.doc, position_ms, &mut frame);
        Some(frame)
    }

    /// Plays the open sequence from `position_ms` with its music, once the music is open (waiting
    /// while holding the engine: see [`Engine::begin_sequence_doc`] to wait without). It is
    /// rendered live, sent to the controllers through the show's output plan, and shown in the
    /// preview; edits to the sequence show up while it plays. Stops a running test pattern or
    /// sequence first.
    pub fn play_sequence_doc(&mut self, position_ms: u64) -> Result<PlaybackStatus, EngineError> {
        self.begin_sequence_doc(position_ms)?.wait();
        self.current_status()
    }

    /// Starts the open sequence like [`Engine::play_sequence_doc`] and returns at once, with
    /// something to wait on until the music is open (opening a sound device can take a moment).
    pub fn begin_sequence_doc(&mut self, position_ms: u64) -> Result<PlaybackReady, EngineError> {
        self.play_document(position_ms, false)
    }

    fn play_document(&mut self, position_ms: u64, paused: bool) -> Result<PlaybackReady, EngineError> {
        let open = self.sequence.as_ref().ok_or(EngineError::NoSequence)?;
        let doc = Arc::new(open.doc.clone());
        let path = open.path.clone();
        let music = document_music(path.as_deref(), doc.audio.as_deref());
        self.stop_session();
        self.stop_reason = None;
        self.stop_playback();
        let (map, report) = analyze(&self.show);
        let request = DocumentRequest {
            doc,
            path,
            music,
            show_error: first_error(&report).map(|i| i.message.clone()),
            send: self.send_sequence_doc,
            volume: self.volume,
        };
        let transport = (self.transport)().map_err(EngineError::Network)?;
        self.playback_generation += 1;
        let mut session = PlaybackSession::start_document(
            &self.show,
            &map,
            request,
            position_ms,
            paused,
            transport,
            self.output_settings.clone(),
            &self.clocks,
        )?;
        let ready = session.take_ready();
        self.playback = Some(session);
        self.playback_stop_reason = None;
        Ok(ready)
    }

    /// Whether a playing sequence document goes out to the controllers (on by default) or only
    /// to the preview, for editing without lighting up the house. A playing document switches
    /// at once, without restarting its music.
    pub fn set_sequence_doc_output(&mut self, send: bool) -> Option<PlaybackStatus> {
        self.send_sequence_doc = send;
        if self
            .playback
            .as_ref()
            .is_some_and(|s| matches!(s.kind(), SessionKind::Document { .. }))
        {
            self.sync_document_playback();
        }
        self.playback_status()
    }

    /// Adds an export of the open sequence (the `.fseq` file at `fseq`) to the show's sequences,
    /// named after the sequence and with its music, as one undo step on the show. When the show
    /// already lists that file, its entry is updated instead (name and music; nothing else).
    pub fn add_sequence_doc_to_show(&mut self, fseq: &Path) -> Result<ShowSnapshot, EngineError> {
        let open = self.sequence.as_ref().ok_or(EngineError::NoSequence)?;
        let name = match open.doc.name.trim() {
            "" => "Sequence".to_string(),
            name => name.to_string(),
        };
        let path = fseq.display().to_string();
        let audio = self.sequence_music().map(|p| p.display().to_string());
        // Exported to the same file again: bring that entry up to date instead of adding another.
        if let Some(existing) = self.show.sequences.iter().find(|s| s.path == path) {
            let taken = |n: &str| {
                self.show
                    .sequences
                    .iter()
                    .any(|s| s.name == n && s.id != existing.id)
            };
            let mut updated = existing.clone();
            if !taken(&name) {
                updated.name = name;
            }
            updated.audio = audio;
            return self.apply(vec![Edit::UpdateSequence { sequence: updated }]);
        }
        let mut entry = pf_model::SequenceEntry::new(name, path);
        entry.audio = audio;
        self.add_sequence(entry)
    }

    /// Whether a playing sequence document goes out to the controllers.
    pub fn sequence_doc_output(&self) -> bool {
        self.send_sequence_doc
    }

    /// What exporting the open sequence needs, to run without holding the engine.
    pub fn sequence_export(&self) -> Result<SequenceExport, EngineError> {
        let open = self.sequence.as_ref().ok_or(EngineError::NoSequence)?;
        let (map, report) = analyze(&self.show);
        Ok(SequenceExport {
            show: self.show.clone(),
            map,
            sequence: open.doc.clone(),
            show_error: first_error(&report).map(|i| i.message.clone()),
        })
    }

    /// Exports the open sequence as an `.fseq` file (see [`SequenceExport::run`]).
    pub fn export_sequence_doc(&self, path: &Path) -> Result<ExportSummary, EngineError> {
        self.sequence_export()?.run(path, |_, _| true)
    }

    fn replace_sequence(&mut self, open: OpenSequence) {
        self.stop_document_playback();
        // The old sequence's kept copy goes with it (the UI asks before dropping changes).
        self.forget_sequence_autosave();
        self.sequence_revision = open.snapshot_revision();
        self.sequence = Some(open);
    }

    fn stop_document_playback(&mut self) {
        if self
            .playback
            .as_ref()
            .is_some_and(|s| matches!(s.kind(), SessionKind::Document { .. }))
        {
            self.stop_playback();
        }
    }

    /// Keeps a playing sequence document in step with its edits: new music restarts it at the
    /// same position; a new frame time also changes how often the output sends (without
    /// reopening the music); anything else shows from the next frame.
    fn sequence_changed(&mut self) {
        let Some(open) = &self.sequence else {
            return;
        };
        self.sequence_revision = open.snapshot_revision();
        let Some(session) = &self.playback else {
            return;
        };
        let SessionKind::Document { music, frame_ms, .. } = session.kind() else {
            return;
        };
        let new_music = document_music(open.path.as_deref(), open.doc.audio.as_deref());
        if new_music != *music {
            self.restart_document_playback();
            return;
        }
        let new_rate = *frame_ms != open.doc.frame_ms;
        session.update_document(Arc::new(open.doc.clone()));
        if new_rate {
            self.rebuild_document_output();
        }
    }

    /// Restarts a playing sequence document at the same position (still paused if it was).
    /// Edits come in under the engine lock, so this doesn't wait for the new music to open.
    fn restart_document_playback(&mut self) {
        let Some(session) = &self.playback else {
            return;
        };
        let status = session.status();
        if status.state == "ended" {
            // Nothing is sending; the next play builds a fresh session.
            return;
        }
        if let Err(error) = self.play_document(status.position_ms, status.state == "paused") {
            self.halt_playback(&error.to_string());
        }
    }

    /// Sends a playing sequence document through the edited show's output plan from where it is,
    /// keeping its music playing.
    fn rebuild_document_output(&mut self) {
        if self
            .playback
            .as_ref()
            .is_none_or(|session| session.status().state == "ended")
        {
            // Nothing is sending; the next play builds a fresh session from the edited show.
            return;
        }
        let transport = match (self.transport)() {
            Ok(transport) => transport,
            Err(error) => {
                self.halt_playback(&EngineError::Network(error).to_string());
                return;
            }
        };
        let (map, report) = analyze(&self.show);
        let show_error = first_error(&report).map(|i| i.message.clone());
        let (Some(open), Some(session)) = (&self.sequence, self.playback.as_mut()) else {
            return;
        };
        self.playback_generation += 1;
        session.rebuild_document(
            &self.show,
            &map,
            &open.doc,
            show_error.as_deref(),
            self.send_sequence_doc,
            transport,
            self.output_settings.clone(),
        );
    }

    fn history_dir(&self) -> PathBuf {
        persist::history_dir(&self.data_dir, self.path.as_deref())
    }

    fn replace_show(&mut self, show: Show, path: Option<PathBuf>) {
        self.stop_session();
        self.stop_playback();
        self.stop_reason = None;
        self.playback_stop_reason = None;
        self.show = show;
        self.path = path;
        self.history.clear();
        self.revision += 1;
        self.saved_revision = self.revision;
        self.autosaved_revision = self.revision;
    }

    fn changed(&mut self) {
        self.revision += 1;
        self.sync_output();
        self.sync_playback();
    }

    /// Keeps a playing sequence in step with the show: does nothing when the controllers' sequence
    /// blocks, addresses, protocols, and the channel layout are unchanged (moving props changes
    /// none of these); sends to the new ones without restarting the music when they changed; restarts
    /// at the same position (still paused if it was) when the sequence or music file changed; or
    /// stops, saying why, if no controller can receive the sequence any more.
    fn sync_playback(&mut self) {
        let Some(session) = &self.playback else {
            return;
        };
        let (old_routes, old_map, channels) = match session.kind() {
            SessionKind::File {
                routes,
                map,
                channels,
                ..
            } => (routes, map, *channels),
            SessionKind::Document { .. } => {
                self.sync_document_playback();
                return;
            }
        };
        let (map, _) = analyze(&self.show);
        let (routes, _) = playback::routes(&self.show, channels);
        if routes.is_empty() {
            self.halt_playback("Playback stopped because no controller has sequence channels anymore.");
            return;
        }
        let built_from_same = routes == *old_routes && map == *old_map;
        let Some(mut request) = session.request() else {
            return;
        };
        // A playing sequence entry that was edited: its offset applies live; new files restart.
        let mut files_changed = false;
        if let Some(id) = request.sequence {
            match self.show.sequences.iter().find(|s| s.id == id) {
                None => {
                    self.halt_playback("Playback stopped because its sequence was removed from the show.");
                    return;
                }
                Some(entry) => {
                    if entry.offset_ms != request.offset_ms {
                        session.set_offset(entry.offset_ms);
                        request.offset_ms = entry.offset_ms;
                    }
                    let music = entry.audio.as_ref().map(PathBuf::from);
                    let path = PathBuf::from(&entry.path);
                    files_changed = music != request.music || path != request.path;
                    request.music = music;
                    request.path = path;
                }
            }
        }
        if built_from_same && !files_changed {
            return;
        }
        let status = session.status();
        if status.state == "ended" {
            // Nothing is sending; the next play builds a fresh session from the edited show.
            return;
        }
        if !files_changed {
            let transport = match (self.transport)() {
                Ok(transport) => transport,
                Err(error) => {
                    self.halt_playback(&EngineError::Network(error).to_string());
                    return;
                }
            };
            self.playback_generation += 1;
            let settings = self.output_settings.clone();
            if let Some(session) = self.playback.as_mut() {
                session.rebuild(&self.show, &map, routes, transport, settings);
            }
            return;
        }
        // Edits come in under the engine lock: don't wait for the new music to open here.
        if let Err(error) = self.play(&request, status.position_ms, status.state == "paused") {
            self.halt_playback(&error.to_string());
        }
    }

    /// Keeps a playing sequence document in step with the show: a change to the wiring, addresses,
    /// channel layout, or whether the show has errors sends through the new output plan from the
    /// same position without reopening the music; anything else (props moved, say) just redraws
    /// with the new layout.
    fn sync_document_playback(&mut self) {
        let Some(session) = &self.playback else {
            return;
        };
        let SessionKind::Document {
            key,
            map: old_map,
            preview_only,
            sending,
            ..
        } = session.kind()
        else {
            return;
        };
        let (map, report) = analyze(&self.show);
        let has_errors = first_error(&report).is_some();
        if has_errors == *preview_only
            && *sending == self.send_sequence_doc
            && output_key(&self.show, &map) == *key
            && map == *old_map
        {
            session.update_renderer(Renderer::new(&self.show, &map));
        } else {
            self.rebuild_document_output();
        }
    }

    /// Stops playback because of the show, remembering why for the UI.
    fn halt_playback(&mut self, reason: &str) {
        self.stop_playback();
        self.playback_stop_reason = Some(reason.to_string());
    }

    /// Keeps running output in step with the show: restarts it only when the wiring, addresses,
    /// frame rate, or the pattern's target pixels changed (moving props in the layout changes
    /// none of these, so no DNS lookup or restart happens), and stops it, saying why, if the show
    /// now has errors or the target has no pixels.
    fn sync_output(&mut self) {
        let Some(session) = &self.output else {
            return;
        };
        let (map, report) = analyze(&self.show);
        if let Some(error) = first_error(&report) {
            let reason = format!(
                "Output stopped because the show now has errors: {}",
                error.message
            );
            self.halt(reason);
            return;
        }
        let targets = resolve_target(&self.show, &map, &Target::from(&session.target));
        if targets.is_empty() {
            self.halt("Output stopped because the target no longer has any pixels.".to_string());
            return;
        }
        if output_key(&self.show, &map) == session.key && targets == session.targets {
            return;
        }
        let (pattern, target) = (session.pattern.clone(), session.target.clone());
        if let Err(error) = self.launch(map, pattern, target) {
            self.halt(error.to_string());
        }
    }

    /// Stops output because of the show, remembering why for the UI.
    fn halt(&mut self, reason: String) {
        self.stop_session();
        self.stop_reason = Some(reason);
    }

    fn stop_session(&mut self) {
        if let Some(session) = self.output.take() {
            session.stop();
        }
    }
}

/// The channel map plus every structural and wiring issue.
fn analyze(show: &Show) -> (ChannelMap, ValidationReport) {
    let mut report = pf_model::validate_show(show);
    let (map, wiring) = pf_mapping::map_show(show);
    report.extend(wiring);
    (map, report)
}

fn first_error(report: &ValidationReport) -> Option<&pf_model::Issue> {
    report.issues.iter().find(|i| i.severity == Severity::Error)
}
