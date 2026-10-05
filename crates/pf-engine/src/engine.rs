//! The engine: owns the show, applies edits, saves, and runs live output.

use crate::edit::Edit;
use crate::error::EngineError;
use crate::history::History;
use crate::output::{OutputSession, OutputStatus, PatternSpec, TargetSpec, output_key};
use crate::persist::{self, HistoryEntry};
use crate::playback::{self, ClockFactory, PlayRequest, PlaybackSession, PlaybackStatus};
use crate::snapshot::{PreviewProp, ShowSnapshot, Summary};
use pf_mapping::ChannelMap;
use pf_model::{IssueCode, SequenceId, Severity, Show, ValidationReport};
use pf_output::{OutputSettings, Transport, UdpTransport};
use pf_patterns::{Target, resolve_target};
use std::io;
use std::mem;
use std::path::{Path, PathBuf};

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
        self.play(&request, position_ms)
    }

    /// Plays one of the show's sequences with its music, lined up by its offset.
    pub fn play_sequence(&mut self, id: SequenceId, position_ms: u64) -> Result<PlaybackStatus, EngineError> {
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
        self.play(&request, position_ms)
    }

    fn play(&mut self, request: &PlayRequest, position_ms: u64) -> Result<PlaybackStatus, EngineError> {
        self.stop_session();
        self.stop_reason = None;
        self.stop_playback();
        let (map, _) = analyze(&self.show);
        let transport = (self.transport)().map_err(EngineError::Network)?;
        self.playback_generation += 1;
        let session = PlaybackSession::start(
            &self.show,
            &map,
            request,
            position_ms,
            transport,
            self.output_settings.clone(),
            &self.clocks,
        )?;
        let status = session.status();
        self.playback = Some(session);
        self.playback_stop_reason = None;
        Ok(status)
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
        let (map, _) = analyze(&self.show);
        self.show
            .props
            .iter()
            .filter_map(|prop| {
                let layout = map.prop_layout(prop.id)?;
                let points = pf_geometry::world_positions(prop)
                    .into_iter()
                    .take(layout.nodes as usize)
                    .flat_map(|p| [p.x, p.y])
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
        self.playback.as_ref().map(PlaybackSession::sequence_frame)
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
    /// none of these); otherwise restarts at the same position (still paused if it was), or stops,
    /// saying why, if no controller can receive the sequence any more.
    fn sync_playback(&mut self) {
        let Some(session) = &self.playback else {
            return;
        };
        let (map, _) = analyze(&self.show);
        let (routes, _) = playback::routes(&self.show, session.channels());
        let (old_routes, old_map) = session.built_from();
        if routes.is_empty() {
            self.halt_playback("Playback stopped because no controller has sequence channels anymore.");
            return;
        }
        let mut request = session.request();
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
        if routes == old_routes && map == *old_map && !files_changed {
            return;
        }
        let status = session.status();
        if status.state == "ended" {
            // Nothing is sending; the next play builds a fresh session from the edited show.
            return;
        }
        match self.play(&request, status.position_ms) {
            Ok(_) => {
                if status.state == "paused" {
                    self.set_playback_paused(true);
                }
            }
            Err(error) => self.halt_playback(&error.to_string()),
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
