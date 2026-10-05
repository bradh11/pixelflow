//! Engine errors, written as plain-language messages for the UI.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("There is no {kind} with that id.")]
    NotFound { kind: &'static str },
    #[error("A {kind} with that id already exists.")]
    DuplicateId { kind: &'static str },
    #[error("{0}")]
    TooLarge(String),
    #[error("This show has not been saved yet. Choose where to save it.")]
    NoPath,
    #[error("Could not read {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("Could not save {path}: {source}")]
    Write { path: PathBuf, source: std::io::Error },
    #[error("{path} is not a valid show file: {source}")]
    InvalidFile {
        path: PathBuf,
        source: pf_model::ModelError,
    },
    #[error("There is no saved version with that id.")]
    UnknownHistoryEntry,
    #[error("Fix the show's errors before starting output. {0}")]
    ShowHasErrors(String),
    #[error("The chosen target has no pixels to light. Wire props to it first.")]
    NothingToLight,
    #[error("Could not open a network socket for output: {0}")]
    Network(std::io::Error),
    #[error("'{0}' is not a color. Use six or eight hex digits, like ff8000.")]
    BadColor(String),
}
