//! The engine: owns the show, applies edits, saves, and runs live output.

use crate::edit::Edit;
use crate::error::EngineError;
use crate::history::History;
use crate::output::{OutputSession, OutputStatus, PatternSpec, TargetSpec};
use crate::persist::{self, HistoryEntry};
use crate::snapshot::{ShowSnapshot, Summary};
use pf_mapping::ChannelMap;
use pf_model::{IssueCode, Severity, Show, ValidationReport};
use pf_output::{Transport, UdpTransport};
use pf_patterns::{Target, resolve_target};
use std::io;
use std::mem;
use std::path::{Path, PathBuf};

/// Undo steps kept in memory.
const UNDO_LIMIT: usize = 200;
/// Autosaved versions kept on disk per show.
const AUTOSAVE_KEEP: usize = 50;

type TransportFactory = Box<dyn Fn() -> io::Result<Box<dyn Transport>> + Send>;

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
    output_generation: u64,
    transport: TransportFactory,
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
            history: History::new(UNDO_LIMIT),
            data_dir: data_dir.into(),
            output: None,
            output_generation: 0,
            transport: Box::new(|| {
                let udp = UdpTransport::bind("0.0.0.0:0".parse().expect("valid address"))?;
                Ok(Box::new(udp) as Box<dyn Transport>)
            }),
        }
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
        let before = mem::replace(&mut self.show, next);
        self.history.record(before);
        self.changed();
        Ok(self.snapshot())
    }

    pub fn undo(&mut self) -> ShowSnapshot {
        if let Some(previous) = self.history.undo(self.show.clone()) {
            self.show = previous;
            self.changed();
        }
        self.snapshot()
    }

    pub fn redo(&mut self) -> ShowSnapshot {
        if let Some(next) = self.history.redo(self.show.clone()) {
            self.show = next;
            self.changed();
        }
        self.snapshot()
    }

    /// Starts a new, empty, unsaved show (stops output and clears undo history).
    pub fn new_show(&mut self, name: &str) -> ShowSnapshot {
        self.replace_show(Show::new(name), None);
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
        if let Some(error) = report.issues.iter().find(|i| i.severity == Severity::Error) {
            return Err(EngineError::ShowHasErrors(error.message.clone()));
        }
        self.stop_session();
        let transport = (self.transport)().map_err(EngineError::Network)?;
        let plan = pf_output::build_plan(&self.show, &map);
        self.output_generation += 1;
        let session = OutputSession::start(
            &self.show,
            &map,
            plan,
            pattern,
            target,
            transport,
            self.output_generation,
        )?;
        let status = session.status();
        self.output = Some(session);
        Ok(status)
    }

    /// Stops live output (controllers are blacked out).
    pub fn stop_output(&mut self) -> OutputStatus {
        self.stop_session();
        OutputStatus::stopped(self.output_generation)
    }

    pub fn output_status(&self) -> OutputStatus {
        self.output.as_ref().map_or_else(
            || OutputStatus::stopped(self.output_generation),
            OutputSession::status,
        )
    }

    /// The latest painted frame while output runs (prop order, RGB/RGBW per pixel).
    pub fn preview_frame(&self) -> Option<Vec<u8>> {
        self.output.as_ref().map(OutputSession::preview)
    }

    fn history_dir(&self) -> PathBuf {
        persist::history_dir(&self.data_dir, self.path.as_deref())
    }

    fn replace_show(&mut self, show: Show, path: Option<PathBuf>) {
        self.stop_session();
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
    }

    /// Keeps running output in step with the show: restarts it only when the output plan or
    /// the pattern's target pixels changed (moving props in the layout does neither), and
    /// stops it if the show now has errors.
    fn sync_output(&mut self) {
        let Some(session) = &self.output else {
            return;
        };
        let (map, report) = analyze(&self.show);
        if report.has_errors() {
            self.stop_session();
            return;
        }
        let plan = pf_output::build_plan(&self.show, &map);
        let targets = resolve_target(&self.show, &map, &Target::from(&session.target));
        if plan == session.plan && targets == session.targets {
            return;
        }
        let (pattern, target) = (session.pattern.clone(), session.target.clone());
        if self.start_output(pattern, target).is_err() {
            self.stop_session();
        }
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
