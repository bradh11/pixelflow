//! Audio errors, written for people.

use std::io::ErrorKind;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("{}", open_message(path, source))]
    Open { path: String, source: std::io::Error },
    /// The file isn't audio PixelFlow can decode (`reason` has the decoder's details, for logs).
    #[error(
        "PixelFlow can't play {path}: it isn't a music file it knows (MP3, M4A, WAV, OGG, or FLAC), or it's damaged."
    )]
    Decode { path: String, reason: String },
    /// The file is larger than any music file ([`crate::MAX_MUSIC_BYTES`]).
    #[error("{path} is larger than 2 GB, which is more than PixelFlow reads as music.")]
    TooLarge { path: String },
    /// There's no working sound output (the detail is the system's, for logs).
    #[error("No sound output is available.")]
    NoOutput(String),
    /// Reading the file was stopped part way.
    #[error("Stopped.")]
    Stopped,
}

fn open_message(path: &str, source: &std::io::Error) -> String {
    match source.kind() {
        ErrorKind::NotFound => format!("PixelFlow can't find the music file {path}."),
        ErrorKind::PermissionDenied => format!("PixelFlow isn't allowed to open the music file {path}."),
        _ => format!("PixelFlow couldn't open the music file {path}."),
    }
}
