//! A show's music: the clock sequence playback follows, waveforms for the timeline, and finding
//! the audio file a sequence was made for.

mod clock;
mod error;
mod find;
mod waveform;

pub use clock::{AudioClock, MusicPlayer, SilentClock};
pub use error::AudioError;
pub use find::find_audio;
pub use waveform::{Waveform, waveform};
