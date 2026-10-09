//! Exporting a sequence as an MP4 video: the 2D preview's front view of the show (with the layout
//! photo behind it, if wanted) and the sequence's music, to share or post.
//!
//! [`export_video`] renders each video frame with the same renderer the preview and `.fseq`
//! export use, at exactly the sequence frame playback would show then (see [`timing`]), draws it
//! as the preview draws it (see [`raster`]), and encodes it. Frames are drawn on several threads
//! at once and encoded in order, so a song exports faster than it plays.
//!
//! Built in, the video is H.264 from OpenH264 and the sound AAC-LC from a pure-Rust encoder, in an
//! MP4 written here, nothing installed needed. When ffmpeg is installed it can do the encoding
//! instead (see [`ffmpeg`]).

pub mod aac;
mod export;
pub mod ffmpeg;
pub mod h264;
pub mod mp4;
pub mod raster;
pub mod timing;
pub mod yuv;

pub use export::{Progress, Stage, VideoOptions, VideoSummary, export_video, load_photo};
pub use ffmpeg::Ffmpeg;

/// Why a video couldn't be made.
#[derive(Debug, thiserror::Error)]
pub enum VideoError {
    #[error("The export was cancelled.")]
    Cancelled,
    #[error("There's nothing to show: add props on the Layout screen first.")]
    NothingToShow,
    #[error("The range to export is empty. Choose a start before its end, inside the sequence.")]
    EmptyRange,
    #[error("{0}")]
    Music(String),
    #[error("{0}")]
    Encode(String),
    #[error("{0}")]
    Ffmpeg(String),
    #[error("Couldn't write {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
}
