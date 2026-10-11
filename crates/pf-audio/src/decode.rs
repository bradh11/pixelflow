//! Opening music files for decoding.
//!
//! The decoders are other people's code reading whatever a file says, and a music file's path
//! can come from a sequence someone else made. A decoder that panics on a file made to trip it
//! up is a file that can't be read, nothing worse; so is sound whose header says something no
//! music file does. And only an ordinary file is ever opened as music: a pipe or a device named
//! in a sequence could otherwise be read for ever.

use crate::error::AudioError;
use crate::progress::{CountedFile, ReadPosition};
use rodio::{Decoder, Source};
use std::fs::File;
use std::io::BufReader;
use std::ops::RangeInclusive;
use std::path::Path;

/// The sample rates read, in Hz: from telephone quality to the highest studio rate. A header can
/// claim any rate, and everything that works with the sound sizes itself from it: a rate of
/// 1 Hz makes a second of sound hours long, and one of 4,000,000,000 Hz asks for buffers to
/// match.
const SAMPLE_RATES: RangeInclusive<u32> = 8_000..=384_000;
/// The most channels read (7.1 surround).
const MAX_CHANNELS: u16 = 8;

/// A decoder reading a music file, keeping count of how far into the file it has read.
pub(crate) type FileDecoder = Decoder<BufReader<CountedFile>>;

fn open_error(path: &Path, source: std::io::Error) -> AudioError {
    AudioError::Open {
        path: path.display().to_string(),
        source,
    }
}

/// Opens the music file at `path` for reading. Only an ordinary file is opened, never a pipe or
/// a device: those can be read (or waited on) for ever.
pub fn open_music_file(path: &Path) -> Result<File, AudioError> {
    let not_a_file = || std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a file");
    // Looked at before opening: opening a pipe waits for whatever writes to it.
    let named = std::fs::metadata(path).map_err(|e| open_error(path, e))?;
    if !named.is_file() {
        return Err(open_error(path, not_a_file()));
    }
    let file = File::open(path).map_err(|e| open_error(path, e))?;
    // And again once open, so it's this file the check was made on.
    let opened = file.metadata().map_err(|e| open_error(path, e))?;
    if !opened.is_file() {
        return Err(open_error(path, not_a_file()));
    }
    Ok(file)
}

/// Why sound at `rate` Hz in `channels` channels isn't read, or `None` when it is.
pub(crate) fn format_problem(rate: u32, channels: u16) -> Option<String> {
    if !SAMPLE_RATES.contains(&rate) {
        Some(format!("a sample rate of {rate} Hz"))
    } else if channels == 0 || channels > MAX_CHANNELS {
        Some(format!("{channels} channels"))
    } else {
        None
    }
}

/// Runs `work`, which calls into a decoder: a panic there is reported as a file that can't be
/// decoded.
pub(crate) fn guarded<T>(path: &Path, work: impl FnOnce() -> Result<T, AudioError>) -> Result<T, AudioError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|_| {
        Err(AudioError::Decode {
            path: path.display().to_string(),
            reason: "its decoder gave up on it".to_string(),
        })
    })
}

/// Opens `path` for decoding, with where in the file the decoder is (for progress). Given the
/// file's length, so the decoder can jump backwards (an MP3 or WAV read as a plain stream can
/// only go forwards) and find an M4A's index at the end of the file.
pub(crate) fn open_decoder(path: &Path) -> Result<(FileDecoder, ReadPosition), AudioError> {
    let shown = path.display().to_string();
    let (file, position) = CountedFile::new(open_music_file(path)?).map_err(|e| open_error(path, e))?;
    let len = file.len();
    let decode_error = |reason: String| AudioError::Decode {
        path: shown.clone(),
        reason,
    };
    let decoder = guarded(path, || {
        let decoder = Decoder::builder()
            .with_data(BufReader::new(file))
            .with_byte_len(len)
            .with_seekable(true)
            .build()
            .map_err(|e| decode_error(e.to_string()))?;
        match format_problem(decoder.sample_rate().get(), decoder.channels().get()) {
            Some(problem) => Err(decode_error(problem)),
            None => Ok(decoder),
        }
    })?;
    Ok((decoder, position))
}

/// The decoder's next sample, with a panic part way through (a file damaged past its header)
/// taken as the end of the sound.
pub(crate) fn next_sample<S: Iterator>(decoder: &mut S) -> Option<S::Item> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decoder.next())).unwrap_or(None)
}

#[cfg(test)]
pub(crate) mod testing {
    /// A 16-bit PCM WAV file's bytes: `frames` of silence at `rate` Hz in `channels` channels.
    pub(crate) fn wav(rate: u32, channels: u16, frames: u32) -> Vec<u8> {
        let data_len = frames * u32::from(channels) * 2;
        let mut out = b"RIFF".to_vec();
        out.extend((36 + data_len).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(channels.to_le_bytes());
        out.extend(rate.to_le_bytes());
        out.extend(rate.wrapping_mul(u32::from(channels) * 2).to_le_bytes());
        out.extend((channels * 2).to_le_bytes());
        out.extend(16u16.to_le_bytes());
        out.extend(b"data");
        out.extend(data_len.to_le_bytes());
        out.extend(std::iter::repeat_n(0u8, data_len as usize));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::testing::wav;
    use super::*;

    fn decode_problem(result: Result<(FileDecoder, ReadPosition), AudioError>) -> String {
        match result {
            Err(AudioError::Decode { reason, .. }) => reason,
            Err(other) => panic!("{other:?}"),
            Ok(_) => panic!("it opened"),
        }
    }

    #[test]
    fn a_file_whose_header_trips_the_decoder_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        // The decoder panics on a sample rate of zero.
        let path = dir.path().join("zero.wav");
        std::fs::write(&path, wav(0, 2, 100)).unwrap();
        assert_eq!(decode_problem(open_decoder(&path)), "its decoder gave up on it");
        let ok = dir.path().join("ok.wav");
        std::fs::write(&ok, wav(44_100, 2, 100)).unwrap();
        assert_eq!(open_decoder(&ok).unwrap().0.count(), 200);
    }

    #[test]
    fn only_sample_rates_and_channel_counts_that_make_sense_are_read() {
        let dir = tempfile::tempdir().unwrap();
        for (name, rate, problem) in [
            ("slow.wav", 1, "a sample rate of 1 Hz"),
            ("low.wav", 7_999, "a sample rate of 7999 Hz"),
            ("fast.wav", 4_000_000_000, "a sample rate of 4000000000 Hz"),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, wav(rate, 2, 100)).unwrap();
            assert_eq!(decode_problem(open_decoder(&path)), problem);
        }
        for rate in [8_000, 44_100, 384_000] {
            let path = dir.path().join(format!("{rate}.wav"));
            std::fs::write(&path, wav(rate, 1, 10)).unwrap();
            assert!(open_decoder(&path).is_ok(), "{rate}");
        }
        assert_eq!(format_problem(48_000, 8), None);
        assert_eq!(format_problem(48_000, 9).as_deref(), Some("9 channels"));
        assert_eq!(format_problem(48_000, 0).as_deref(), Some("0 channels"));
    }

    #[test]
    fn only_an_ordinary_file_is_opened_as_music() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("folder.mp3");
        std::fs::create_dir(&folder).unwrap();
        let message = open_music_file(&folder).unwrap_err().to_string();
        assert!(
            message.starts_with("PixelFlow couldn't open the music file"),
            "{message}"
        );
        let gone = open_music_file(&dir.path().join("gone.mp3"))
            .unwrap_err()
            .to_string();
        assert!(gone.starts_with("PixelFlow can't find the music file"), "{gone}");
        let song = dir.path().join("song.wav");
        std::fs::write(&song, wav(44_100, 2, 10)).unwrap();
        assert!(open_music_file(&song).is_ok());
    }

    /// Reading a device named as music would never end.
    #[cfg(unix)]
    #[test]
    fn a_device_is_not_opened_as_music() {
        let device = Path::new("/dev/zero");
        let message = open_music_file(device).unwrap_err().to_string();
        assert!(
            message.starts_with("PixelFlow couldn't open the music file"),
            "{message}"
        );
        assert!(open_decoder(device).is_err());
    }

    #[test]
    fn a_panic_part_way_through_is_the_end_of_the_sound() {
        let mut samples = [1.0f32, 2.0]
            .into_iter()
            .chain(std::iter::from_fn(|| panic!("damaged")));
        assert_eq!(next_sample(&mut samples), Some(1.0));
        assert_eq!(next_sample(&mut samples), Some(2.0));
        assert_eq!(next_sample(&mut samples), None);
    }
}
