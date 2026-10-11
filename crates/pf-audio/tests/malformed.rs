//! Music files made to trip their decoders up: each is a file that can't be read, reported as
//! one, whichever way it's asked for.

use pf_audio::{AudioError, MonoSamples, MusicPlayer, StereoFrames, probe, read_tags, waveform};
use std::path::Path;

/// A 16-bit PCM WAV file: `frames` of silence at `rate` Hz in `channels` channels.
fn write_wav(path: &Path, rate: u32, channels: u16, frames: u32) {
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
    std::fs::write(path, out).unwrap();
}

/// Every way a music file is read, each answering with its error (`None`: it was read).
fn every_reader(path: &Path) -> Vec<(&'static str, Option<AudioError>)> {
    vec![
        ("probe", probe(path, &|_| {}).err()),
        ("tags", read_tags(path).err()),
        ("waveform", waveform(path, 4).err()),
        ("mono", MonoSamples::open(path).err()),
        ("stereo", StereoFrames::open(path).err()),
        ("playing", MusicPlayer::open(path).err()),
    ]
}

#[test]
fn sound_at_a_rate_no_music_has_is_not_read() {
    let dir = tempfile::tempdir().unwrap();
    // At 1 Hz a hundred samples are a hundred seconds; at 4 GHz every buffer sized from the rate
    // is enormous.
    for (name, rate) in [("slow.wav", 1), ("fast.wav", 4_000_000_000)] {
        let path = dir.path().join(name);
        write_wav(&path, rate, 2, 100);
        for (reader, error) in every_reader(&path) {
            // Tags are read without looking at the sound.
            if reader == "tags" {
                continue;
            }
            let error = error.unwrap_or_else(|| panic!("{reader} read {name}"));
            assert!(matches!(error, AudioError::Decode { .. }), "{reader}: {error:?}");
        }
    }
    let fine = dir.path().join("fine.wav");
    write_wav(&fine, 44_100, 2, 100);
    assert_eq!(probe(&fine, &|_| {}).unwrap().sample_rate, 44_100);
    assert_eq!(MonoSamples::open(&fine).unwrap().count(), 100);
}

#[test]
fn a_wav_that_says_its_sample_rate_is_zero_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("zero.wav");
    // The decoder panics working out this file's time base.
    write_wav(&path, 0, 2, 100);
    for (reader, error) in every_reader(&path) {
        let error = error.unwrap_or_else(|| panic!("{reader} read it"));
        assert!(matches!(error, AudioError::Decode { .. }), "{reader}: {error:?}");
        assert!(error.to_string().contains("or it's damaged"), "{reader}: {error}");
    }
}
