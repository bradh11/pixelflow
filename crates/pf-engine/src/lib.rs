//! The PixelFlow engine: the single owner of the open show.
//!
//! Every change goes through [`Engine::apply`] as a batch of [`Edit`]s, which is validated,
//! applied atomically, and recorded as one undo step. The engine also saves and opens show
//! files (atomically, with autosave history) and runs live test-pattern output and sequence playback. It has no
//! UI dependencies; the desktop app is a thin bridge over this API.
//!
//! Alongside the show, the engine holds one open **sequence document** (an authored sequence,
//! saved in its own file) with its own undo history: [`Engine::edit_sequence`] applies
//! [`SequenceEdit`]s, [`Engine::play_sequence_doc`] plays it live with its music, and
//! [`Engine::sequence_export`] exports it as an `.fseq` file for FPP.

mod edit;
mod engine;
mod error;
mod history;
mod output;
mod persist;
mod playback;
mod recovery;
mod sequence_doc;
mod snapshot;

pub use edit::Edit;
pub use engine::SequenceExport;
pub use engine::{CheckedShow, Engine};
pub use error::EngineError;
pub use history::History;
pub use output::{ControllerStatus, OutputStatus, PatternSpec, TargetSpec};
pub use persist::{HistoryEntry, load_show, save_show_atomic, write_atomic};
pub use pf_render::export::{ExportBlock, ExportLayout, ExportSummary};
pub use playback::{
    ClockFactory, PlayRequest, PlaybackReady, PlaybackStatus, music_clocks, sequence_entry_for,
};
pub use recovery::SequenceRecovery;
pub use sequence_doc::{
    PlacedEffect, SequenceChanges, SequenceEdit, SequenceEditResult, SequenceInfo, SequenceSnapshot,
    load_sequence, save_sequence_atomic,
};
pub use snapshot::{PreviewProp, ShowSnapshot, Summary};
