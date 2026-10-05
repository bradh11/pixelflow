//! The PixelFlow engine: the single owner of the open show.
//!
//! Every change goes through [`Engine::apply`] as a batch of [`Edit`]s, which is validated,
//! applied atomically, and recorded as one undo step. The engine also saves and opens show
//! files (atomically, with autosave history) and runs live test-pattern output and sequence playback. It has no
//! UI dependencies; the desktop app is a thin bridge over this API.

mod edit;
mod engine;
mod error;
mod history;
mod output;
mod persist;
mod playback;
mod snapshot;

pub use edit::Edit;
pub use engine::Engine;
pub use error::EngineError;
pub use history::History;
pub use output::{ControllerStatus, OutputStatus, PatternSpec, TargetSpec};
pub use persist::{HistoryEntry, load_show, save_show_atomic};
pub use playback::PlaybackStatus;
pub use snapshot::{PreviewProp, ShowSnapshot, Summary};
