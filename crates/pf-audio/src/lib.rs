//! A show's music: the clock sequence playback follows, waveforms for the timeline, finding the
//! audio file a sequence was made for, and what a file says about itself (its tags).

mod clock;
mod decode;
mod error;
mod find;
mod mono;
mod music;
mod stereo;
mod tags;
mod wav;
mod waveform;

pub use clock::{AudioClock, SilentClock};
pub use error::AudioError;
pub use find::find_audio;
pub use mono::MonoSamples;
pub use music::MusicPlayer;
pub use stereo::StereoFrames;
pub use tags::{SongTags, read_tags, title_from_file_name};
pub use wav::{Resampler, mono_at_rate, wav_bytes};
pub use waveform::{Waveform, waveform};
