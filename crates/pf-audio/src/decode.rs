//! Opening music files for decoding.
//!
//! The decoders are other people's code reading whatever a file says, and a music file's path
//! can come from a sequence someone else made. A decoder that panics on a file made to trip it
//! up is a file that can't be read, nothing worse.

use crate::error::AudioError;
use crate::progress::{CountedFile, ReadPosition};
use rodio::{Decoder, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// A decoder reading a music file, keeping count of how far into the file it has read.
pub(crate) type FileDecoder = Decoder<BufReader<CountedFile>>;

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
    let opened = File::open(path).and_then(CountedFile::new);
    let (file, position) = opened.map_err(|source| AudioError::Open {
        path: shown.clone(),
        source,
    })?;
    let len = file.len();
    let decoder = guarded(path, || {
        let decoder = Decoder::builder()
            .with_data(BufReader::new(file))
            .with_byte_len(len)
            .with_seekable(true)
            .build()
            .map_err(|e| AudioError::Decode {
                path: shown,
                reason: e.to_string(),
            })?;
        // What every reader asks next: a decoder that can't say is one that can't be used.
        let _ = (decoder.sample_rate(), decoder.channels());
        Ok(decoder)
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
    fn a_panic_part_way_through_is_the_end_of_the_sound() {
        let mut samples = [1.0f32, 2.0]
            .into_iter()
            .chain(std::iter::from_fn(|| panic!("damaged")));
        assert_eq!(next_sample(&mut samples), Some(1.0));
        assert_eq!(next_sample(&mut samples), Some(2.0));
        assert_eq!(next_sample(&mut samples), None);
    }
}
