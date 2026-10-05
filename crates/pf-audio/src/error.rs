//! Audio errors, written for people.

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("Could not open {path}: {source}")]
    Open { path: String, source: std::io::Error },
    #[error("Could not play {path}: {reason}")]
    Decode { path: String, reason: String },
    #[error("No sound output is available: {0}")]
    NoOutput(String),
}
