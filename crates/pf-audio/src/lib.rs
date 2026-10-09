//! A show's music: the clock sequence playback follows, waveforms for the timeline, finding the
//! audio file a sequence was made for, and what a file says about itself (its tags).

mod clock;
mod decode;
mod error;
mod find;
mod metronome;
mod mono;
mod music;
mod probe;
mod progress;
mod stereo;
mod tags;
mod wav;
mod waveform;

pub use clock::{AudioClock, SilentClock};
pub use error::AudioError;
pub use find::find_audio;
pub use metronome::{METRONOME_RATE, Metronome};
pub use mono::MonoSamples;
pub use music::{MusicPlayer, OutputInfo, SLOWEST};
pub use probe::{AudioInfo, FoundBy, probe};
pub use progress::{Progress, ReadPosition, no_progress, reported};
pub use stereo::StereoFrames;
pub use tags::{SongTags, read_tags, title_from_file_name};
pub use wav::{Resampler, mono_at_rate, mono_at_rate_reporting, wav_bytes};
pub use waveform::{Waveform, waveform, waveform_reporting};
