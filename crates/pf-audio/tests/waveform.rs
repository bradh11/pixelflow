//! Waveforms of generated audio files.

use pf_audio::waveform;
use std::f32::consts::PI;

/// A 16-bit stereo WAV: one second of silence, then one second of a loud 440 Hz tone.
fn write_wav(path: &std::path::Path) {
    let rate = 8000u32;
    let samples: Vec<i16> = (0..rate * 2)
        .flat_map(|i| {
            let v = if i < rate {
                0.0
            } else {
                (2.0 * PI * 440.0 * i as f32 / rate as f32).sin() * 0.8
            };
            let s = (v * i16::MAX as f32) as i16;
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
fn peaks_follow_the_loudness() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tone.wav");
    write_wav(&path);
    let w = waveform(&path, 20).unwrap();
    assert_eq!(w.duration_ms, 2000);
    assert_eq!(w.peaks.len(), 20);
    assert!(
        w.peaks[..9].iter().all(|&p| p < 0.01),
        "silent first second: {:?}",
        w.peaks
    );
    assert!(
        w.peaks[11..].iter().all(|&p| p > 0.7 && p <= 1.0),
        "loud second second: {:?}",
        w.peaks
    );
}

#[test]
fn unreadable_files_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.mp3");
    std::fs::write(&junk, b"not audio").unwrap();
    assert!(
        waveform(&junk, 10)
            .unwrap_err()
            .to_string()
            .starts_with("PixelFlow can't play")
    );
    assert!(
        waveform(&dir.path().join("missing.mp3"), 10)
            .unwrap_err()
            .to_string()
            .starts_with("PixelFlow can't find the music file")
    );
}

#[test]
fn mono_samples_average_the_channels_at_the_source_rate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tone.wav");
    write_wav(&path);
    let samples = pf_audio::MonoSamples::open(&path).unwrap();
    assert_eq!(samples.sample_rate(), 8000);
    let all: Vec<f32> = samples.collect();
    assert_eq!(all.len(), 16_000, "two seconds of mono");
    assert!(all[..8000].iter().all(|&s| s == 0.0));
    let peak = all[8000..].iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!((peak - 0.8).abs() < 0.01, "{peak}");
    let err = pf_audio::MonoSamples::open(&dir.path().join("missing.wav")).unwrap_err();
    assert!(
        err.to_string().starts_with("PixelFlow can't find the music file"),
        "{err}"
    );
}

/// Set `PIXELFLOW_AUDIO=/path/to/song.mp3` to check a real file decodes.
#[test]
fn real_audio_decodes_when_provided() {
    let Ok(path) = std::env::var("PIXELFLOW_AUDIO") else {
        return;
    };
    let w = waveform(std::path::Path::new(&path), 100).unwrap();
    eprintln!(
        "{path}: {} ms, max peak {}",
        w.duration_ms,
        w.peaks.iter().copied().fold(0.0, f32::max)
    );
    assert!(w.duration_ms > 0);
}
