//! Sequence file errors, written for people.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum FseqError {
    #[error("Could not read the sequence file: {0}")]
    Io(#[from] io::Error),
    #[error("This isn't an FPP sequence (.fseq) file.")]
    NotFseq,
    #[error("This sequence uses {0}, which PixelFlow can't read yet.")]
    Unsupported(String),
    #[error("The sequence file is damaged: {0}")]
    Corrupt(String),
}

pub(crate) fn corrupt(reason: impl Into<String>) -> FseqError {
    FseqError::Corrupt(reason.into())
}
