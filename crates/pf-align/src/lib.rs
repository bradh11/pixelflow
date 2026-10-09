//! Lining a song's known words up with its audio on this computer, to the letter: forced
//! alignment, the way WhisperX does it.
//!
//! 1. **The voice brought forward** ([`voice`]): the song at 16 kHz, each frequency kept by how
//!    alike its two sides are, so the lead vocal (in the middle of the mix) stands out from wide
//!    instruments. No separation model: this is what the aligner hears.
//! 2. **Letters heard** ([`model`], [`song`]): a speech model (wav2vec2-base-960h, Apache-2.0,
//!    downloaded once with the user's say-so, [`store`]) says how likely each letter is every
//!    20 ms ([`Emission`]), over the whole song in pieces, several at once.
//! 3. **Forced alignment** ([`ctc`]): the known words, as letters ([`text`]), placed on those
//!    20 ms steps by the most likely path (a CTC Viterbi trellis with blanks), each line within
//!    its own stretch of the song, a second either side ([`windows`]).
//! 4. **Words, syllables, and sounds** ([`align`], [`sounds`]): a word spans its letters; each
//!    of its sounds (from [`pf_lexicon`]) starts when the letters it's written with are heard.
//!
//! Every letter, word, and line says how sure it is, so the caller can keep its own timing
//! where the aligner is unsure. Nothing here talks to anyone except [`store`]'s download, and
//! only when asked.

pub mod align;
pub mod ctc;
pub mod model;
pub mod song;
pub mod sounds;
pub mod store;
pub mod text;
pub mod vocab;
pub mod voice;
pub mod windows;

pub use align::{CharTime, LineTime, WordTime, align_lines};
pub use ctc::{Emission, Span, force_align};
pub use model::{Acoustic, Wav2Vec2};
pub use sounds::{PhoneTime, SyllableTime, word_sounds};
pub use store::{Manifest, ModelFile, ModelStore, WAV2VEC2};
pub use windows::{Line, Window};

use std::path::Path;

/// Something went wrong aligning, or getting the model.
#[derive(Debug, thiserror::Error)]
pub enum AlignError {
    #[error("Stopped.")]
    Cancelled,
    #[error("The song couldn't be read: {0}")]
    Audio(String),
    #[error("The alignment model couldn't be used: {0}")]
    Model(String),
    #[error("The alignment model couldn't be downloaded: {0}")]
    Download(String),
    #[error("The downloaded model wasn't what was expected (its checksum didn't match), so it wasn't kept.")]
    Checksum { expected: String, found: String },
    #[error("The alignment model couldn't be saved: {0}")]
    Io(#[from] std::io::Error),
}

/// What aligning a song is doing, for its progress bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Bringing the voice forward ([`voice`]).
    Separating,
    /// Hearing the letters ([`song`]).
    Aligning,
}

/// The song at `path` heard letter by letter ([`voice::centre_voice`], then
/// [`song::emission_of`]), telling `progress` each stage and how far it has got (0–1).
pub fn song_emission(
    acoustic: &dyn Acoustic,
    path: &Path,
    stop: &(dyn Fn() -> bool + Sync),
    progress: &dyn Fn(Stage, f32),
) -> Result<Emission, AlignError> {
    progress(Stage::Separating, 0.0);
    let samples = voice::centre_voice(path, stop, &|f| progress(Stage::Separating, f))?;
    progress(Stage::Separating, 1.0);
    progress(Stage::Aligning, 0.0);
    let emission = song::emission_of(acoustic, &samples, stop, &|f| progress(Stage::Aligning, f))?;
    progress(Stage::Aligning, 1.0);
    Ok(emission)
}
