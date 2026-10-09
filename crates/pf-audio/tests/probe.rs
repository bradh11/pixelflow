//! Finding a music file's length without decoding it, and progress while decoding.
//!
//! The MP3s in `fixtures` are made up: a second of a 440 Hz tone, then a second of noise, mono
//! at 22.05 kHz, made with LAME: through ffmpeg at 32 kbit/s for the constant bitrate one (with
//! its Info header), and `lame -V 9 -t` for the variable bitrate one (no Xing header).

use pf_audio::{FoundBy, probe, waveform, waveform_reporting};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn reports(run: impl FnOnce(&dyn Fn(f32))) -> Vec<f32> {
    let seen = RefCell::new(Vec::new());
    run(&|f| seen.borrow_mut().push(f));
    seen.into_inner()
}

/// `seconds` of a 16-bit stereo 440 Hz tone at `rate`.
fn write_wav(path: &Path, rate: u32, seconds: u32) {
    let samples: Vec<i16> = (0..rate * seconds)
        .flat_map(|i| {
            let v = (std::f32::consts::TAU * 440.0 * i as f32 / rate as f32).sin() * 0.5;
            let s = (v * f32::from(i16::MAX)) as i16;
            [s, s]
        })
        .collect();
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}

#[test]
fn a_wav_is_measured_from_its_header() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tone.wav");
    write_wav(&path, 8000, 3);
    let seen = reports(|p| {
        let info = probe(&path, p).unwrap();
        assert_eq!(info.duration_ms, 3000);
        assert_eq!((info.sample_rate, info.channels), (8000, 2));
        assert_eq!(info.codec, "pcm_s16le");
        assert_eq!(info.found_by, FoundBy::Header);
    });
    // Nothing to wait for: only that it's done.
    assert_eq!(seen, [1.0]);
}

#[test]
fn an_mp3_with_an_info_header_is_measured_from_it() {
    let path = fixture("tone-cbr.mp3");
    let info = probe(&path, &|_| {}).unwrap();
    assert_eq!(info.found_by, FoundBy::Header);
    assert_eq!(
        (info.sample_rate, info.channels, info.codec.as_str()),
        (22_050, 1, "mp3")
    );
    let decoded = waveform(&path, 10).unwrap().duration_ms;
    assert!(
        info.duration_ms.abs_diff(decoded) <= 50,
        "{} vs {decoded}",
        info.duration_ms
    );
    assert!(info.duration_ms.abs_diff(2000) <= 60, "{}", info.duration_ms);
}

#[test]
fn a_variable_bitrate_mp3_without_a_header_has_its_frames_counted() {
    let path = fixture("tone-vbr-untagged.mp3");
    let seen = reports(|p| {
        let info = probe(&path, p).unwrap();
        assert_eq!(info.found_by, FoundBy::Frames);
        // The tone's frames are small and the noise's big: a guess from the first frames' size
        // would be well off. Every frame counted (LAME's delay and padding included) is about
        // two seconds.
        assert!(info.duration_ms.abs_diff(2000) <= 80, "{}", info.duration_ms);
        let guessed = pf_audio::read_tags(&path).unwrap().duration_ms.unwrap();
        assert!(guessed.abs_diff(2000) > 80, "the bitrate guess was {guessed}");
    });
    assert_eq!(seen.last(), Some(&1.0));
    assert!(seen.windows(2).all(|w| w[1] > w[0]), "{seen:?}");
}

#[test]
fn files_that_arent_music_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.mp3");
    std::fs::write(&junk, b"not audio").unwrap();
    let err = probe(&junk, &|_| {}).unwrap_err().to_string();
    assert!(err.starts_with("PixelFlow can't play"), "{err}");
    let err = probe(&dir.path().join("missing.mp3"), &|_| {})
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("PixelFlow can't find the music file"), "{err}");
}

#[test]
fn decoding_reports_progress_forwards_in_steps_up_to_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.wav");
    write_wav(&path, 22_050, 20);
    let seen = reports(|p| {
        let w = waveform_reporting(&path, 50, p).unwrap();
        assert_eq!(w.duration_ms, 20_000);
    });
    assert!(seen.len() >= 10, "too few reports: {seen:?}");
    assert!(seen.len() <= 102, "too many reports: {}", seen.len());
    assert!(seen.windows(2).all(|w| w[1] > w[0]), "{seen:?}");
    assert!(
        seen.windows(2).all(|w| w[1] - w[0] >= 0.0099 || w[1] == 1.0),
        "{seen:?}"
    );
    assert_eq!(seen.last(), Some(&1.0));
}

/// Set `PIXELFLOW_AUDIO=/path/to/song.mp3` (several: separated by `:`) to compare the quick
/// length with the decoded one on real files.
#[test]
fn real_audio_probes_like_it_decodes_when_provided() {
    let Ok(paths) = std::env::var("PIXELFLOW_AUDIO") else {
        return;
    };
    for path in paths.split(':').map(Path::new) {
        let started = std::time::Instant::now();
        let info = probe(path, &|_| {}).unwrap();
        let probed_in = started.elapsed();
        let started = std::time::Instant::now();
        let decoded = waveform(path, 100).unwrap().duration_ms;
        let decoded_in = started.elapsed();
        eprintln!(
            "{}: probe {} ms ({:?}) in {probed_in:?}; decode {decoded} ms in {decoded_in:?}",
            path.display(),
            info.duration_ms,
            info.found_by,
        );
        assert!(
            info.duration_ms.abs_diff(decoded) <= 50,
            "{} vs {decoded}",
            info.duration_ms
        );
    }
}
