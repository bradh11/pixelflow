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
    /// An edit with a value the show can't hold, explained in plain language.
    #[error("{0}")]
    InvalidEdit(String),
    /// A show built in memory (an import, for example) that a show file couldn't hold.
    #[error("This show can't be opened: {0}")]
    InvalidShow(String),
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
    #[error("{0}")]
    Playback(String),
    #[error("{path} is not a valid sequence file: {reason}")]
    InvalidSequence { path: PathBuf, reason: String },
    #[error("No sequence is open. Create or open one first.")]
    NoSequence,
    #[error("This sequence has not been saved yet. Choose where to save it.")]
    SequenceNoPath,
    #[error("{0}")]
    Export(String),
    #[error("That unsaved sequence isn't there anymore.")]
    UnknownRecovery,
    #[error(
        "Save the show first, so PixelFlow knows which folder to look in. Or use Locate… to choose the file."
    )]
    NoFolderToSearch,
    #[error(
        "Save the sequence first, so PixelFlow knows which folder to look in. Or use Locate… to choose the file."
    )]
    SequenceNoFolderToSearch,
    /// A file chosen to replace a missing one that isn't there either (named plainly).
    #[error("{0} isn't there anymore. Choose another file.")]
    FileGone(String),
    #[error("Another show was opened while PixelFlow was looking. Look again.")]
    SearchOutdated,
    #[error("The open sequence has no music to look for.")]
    NoSequenceMusic,
    #[error("None of the open sequence's pictures are missing.")]
    NoSequencePictures,
}
