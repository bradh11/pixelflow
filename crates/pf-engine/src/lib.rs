//! The PixelFlow engine: the single owner of the open show.
//!
//! Every change goes through [`Engine::apply`] as a batch of [`Edit`]s, which is validated,
//! applied atomically, and recorded as one undo step. The engine also saves and opens show
//! files (atomically, with autosave history) and runs live test-pattern output. It has no
//! UI dependencies; the desktop app is a thin bridge over this API.

mod edit;
mod error;
mod history;

pub use edit::Edit;
pub use error::EngineError;
pub use history::History;
