//! A show's music: the clock sequence playback follows, waveforms for the timeline, and finding
//! the audio file a sequence was made for.

mod clock;
mod decode;
mod error;
mod find;
mod music;
mod waveform;

pub use clock::{AudioClock, SilentClock};
pub use error::AudioError;
pub use find::find_audio;
pub use music::MusicPlayer;
pub use waveform::{Waveform, waveform};
