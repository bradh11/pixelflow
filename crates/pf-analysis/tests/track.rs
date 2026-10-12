//! The audio track of generated music files.

use pf_analysis::{AudioTrack, audio_track, audio_track_file};
use std::f32::consts::TAU;

const RATE: u32 = 22_050;

fn tone(amplitude: f32, seconds: f32) -> Vec<f32> {
    (0..(seconds * RATE as f32) as usize)
        .map(|i| amplitude * (TAU * 440.0 * i as f32 / RATE as f32).sin())
        .collect()
}

/// A 16-bit WAV with a channel for each of `channels` (all the same length).
fn write_wav(path: &std::path::Path, channels: &[&[f32]]) {
    let count = channels.len() as u16;
    let frames = channels[0].len();
    let data_len = (frames * channels.len() * 2) as u32;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * u32::from(count) * 2).to_le_bytes());
    out.extend_from_slice(&(count * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        for channel in channels {
            let s = (channel[i].clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            out.extend_from_slice(&s.to_le_bytes());
        }
    }
    std::fs::write(path, out).unwrap();
}

fn track(path: &std::path::Path) -> AudioTrack {
    audio_track_file(path, 50, &|| false, &|_| {}).unwrap()
}

#[test]
fn a_files_peaks_follow_its_louder_channel() {
    let dir = std::env::temp_dir().join(format!("pf-analysis-track-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // A second on both sides, a second on the right alone, a second mirrored, a quiet second
    // on the left alone.
    let (loud, none, quiet) = (tone(0.8, 1.0), vec![0.0; RATE as usize], tone(0.2, 1.0));
    let mirrored: Vec<f32> = loud.iter().map(|s| -s).collect();
    let left = [&loud[..], &none, &loud, &quiet].concat();
    let right = [&loud[..], &loud, &mirrored, &none].concat();
    let stereo = dir.join("stereo.wav");
    write_wav(&stereo, &[&left, &right]);
    let t = track(&stereo);
    assert_eq!(t.len(), 80);
    let peaks: Vec<f32> = (0..4).map(|second| t.peak(second * 20 + 10)).collect();
    let troughs: Vec<f32> = (0..4).map(|second| t.trough(second * 20 + 10)).collect();
    for (got, want) in peaks.iter().chain(&troughs).zip([1.0, 1.0, 1.0, 0.25].repeat(2)) {
        assert!((got - want).abs() < 0.01, "{peaks:?} {troughs:?}");
    }
    // How loud it sounds is still the mix's: silence where the channels cancel.
    assert_eq!(t.level(50), 0.0);
    assert!(t.level(10) > 0.99);

    // A file with one channel, and one with the same on both sides, read alike: as the samples
    // themselves do.
    let (one, two) = (dir.join("one.wav"), dir.join("two.wav"));
    write_wav(&one, &[&left]);
    write_wav(&two, &[&left, &left]);
    let decoded: Vec<f32> = pf_audio::MonoSamples::open(&one).unwrap().collect();
    assert_eq!(track(&one), audio_track(decoded, RATE, 50));
    assert_eq!(track(&two), track(&one));
    let _ = std::fs::remove_dir_all(&dir);
}
