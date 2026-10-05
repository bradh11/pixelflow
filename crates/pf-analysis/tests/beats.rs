//! Beat detection on synthetic click tracks.

use pf_analysis::{Analysis, analyze, analyze_file};
use pf_sequence::TimingKind;

const RATE: u32 = 44_100;

/// Deterministic noise in -1..1.
struct Noise(u64);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) as f32 / (1u64 << 23) as f32) - 1.0
    }
}

/// `seconds` of audio with a click (a 10 ms decaying 2 kHz burst) at each time in `clicks_ms`,
/// clicks in `accents` louder, plus noise of amplitude `noise`.
fn render(seconds: f32, clicks: &[(u64, f32)], noise: f32) -> Vec<f32> {
    let n = (seconds * RATE as f32) as usize;
    let mut out = vec![0.0f32; n];
    let mut rng = Noise(7);
    for s in out.iter_mut() {
        *s = noise * rng.next();
    }
    let burst = (RATE / 100) as usize;
    for &(ms, level) in clicks {
        let start = (ms * u64::from(RATE) / 1000) as usize;
        for k in 0..burst {
            if let Some(s) = out.get_mut(start + k) {
                let t = k as f32 / RATE as f32;
                *s += level
                    * (-(k as f32) / burst as f32 * 5.0).exp()
                    * (std::f32::consts::TAU * 2000.0 * t).sin();
            }
        }
    }
    out
}

/// Clicks every `period_ms` from `first_ms` for `seconds`; every 4th (starting with the first)
/// at full level and the rest at `others`.
fn click_track(period_ms: u64, first_ms: u64, seconds: f32, others: f32) -> Vec<(u64, f32)> {
    (0..)
        .map(|i| (first_ms + i * period_ms, if i % 4 == 0 { 0.9 } else { others }))
        .take_while(|&(ms, _)| (ms as f32) < seconds * 1000.0 - 100.0)
        .collect()
}

fn nearest(times: &[u64], t: u64) -> u64 {
    times.iter().map(|&x| x.abs_diff(t)).min().unwrap_or(u64::MAX)
}

fn check_beats(a: &Analysis, clicks: &[(u64, f32)], period_ms: u64) {
    let click_times: Vec<u64> = clicks.iter().map(|c| c.0).collect();
    assert!(
        a.beats.len() + 2 >= clicks.len(),
        "{} beats for {} clicks",
        a.beats.len(),
        clicks.len()
    );
    for &b in &a.beats {
        assert!(
            nearest(&click_times, b) <= 20,
            "beat at {b} ms is not on a click: {:?}",
            a.beats
        );
    }
    for pair in a.beats.windows(2) {
        let gap = pair[1] - pair[0];
        assert!(
            gap.abs_diff(period_ms) <= 20,
            "beats {} ms apart: {:?}",
            gap,
            a.beats
        );
    }
}

#[test]
fn a_120_bpm_click_track_has_beats_every_500_ms() {
    let clicks = click_track(500, 250, 30.0, 0.9);
    let a = analyze(render(30.0, &clicks, 0.0), RATE);
    assert_eq!(a.duration_ms, 30_000);
    let bpm = a.tempo_bpm.unwrap();
    assert!((bpm - 120.0).abs() < 2.0, "{bpm} BPM");
    check_beats(&a, &clicks, 500);
    // Every click is an onset, within 15 ms.
    assert_eq!(a.onsets.len(), clicks.len(), "{:?}", a.onsets);
    for &(ms, _) in &clicks {
        assert!(nearest(&a.onsets, ms) <= 15, "click at {ms} ms");
    }
}

#[test]
fn other_tempos_are_found_too() {
    for (period, expected) in [(632u64, 95.0f32), (400, 150.0), (857, 70.0)] {
        let clicks = click_track(period, 300, 40.0, 0.9);
        let a = analyze(render(40.0, &clicks, 0.0), RATE);
        let bpm = a.tempo_bpm.unwrap();
        assert!((bpm - expected).abs() < 2.5, "{period} ms: {bpm} BPM");
        check_beats(&a, &clicks, period);
    }
}

#[test]
fn beats_survive_background_noise() {
    let clicks = click_track(500, 100, 30.0, 0.9);
    let a = analyze(render(30.0, &clicks, 0.15), RATE);
    let bpm = a.tempo_bpm.unwrap();
    assert!((bpm - 120.0).abs() < 2.0, "{bpm} BPM");
    check_beats(&a, &clicks, 500);
}

#[test]
fn bars_start_on_the_accented_beat() {
    // Accents on every 4th click, starting with the third click (so not the first beat).
    let clicks: Vec<(u64, f32)> = (0..56u64)
        .map(|i| (200 + i * 500, if i % 4 == 2 { 0.9 } else { 0.3 }))
        .collect();
    let a = analyze(render(29.0, &clicks, 0.0), RATE);
    let accents: Vec<u64> = clicks.iter().filter(|c| c.1 > 0.5).map(|c| c.0).collect();
    assert!(!a.bars.is_empty());
    for &bar in &a.bars {
        assert!(nearest(&accents, bar) <= 20, "bar at {bar} is not on an accent");
    }
    for pair in a.bars.windows(2) {
        assert!((pair[1] - pair[0]).abs_diff(2000) <= 40, "{:?}", a.bars);
    }

    let tracks = a.timing_tracks();
    let names: Vec<(&str, TimingKind)> = tracks.iter().map(|t| (t.name.as_str(), t.kind)).collect();
    assert_eq!(
        names,
        vec![
            ("Beats", TimingKind::Beats),
            ("Bars", TimingKind::Bars),
            ("Onsets", TimingKind::Custom)
        ]
    );
    let beats = &tracks[0].marks;
    let downbeat = beats
        .iter()
        .find(|m| nearest(&accents, m.start_ms) <= 20)
        .unwrap();
    assert_eq!(downbeat.label, "1", "beats count from each bar's first beat");
    let first_label: u32 = beats[0].label.parse().unwrap();
    assert_eq!(first_label, 3, "the first click is beat 3 of its bar");
    assert!(
        beats.windows(2).all(|w| w[0].end_ms == w[1].start_ms),
        "marks span to the next"
    );
    assert_eq!(tracks[1].marks[0].label, "1");
}

#[test]
fn silence_has_no_beats_and_short_clips_are_fine() {
    let a = analyze(vec![0.0; RATE as usize * 5], RATE);
    assert_eq!((a.tempo_bpm, a.beats.len(), a.onsets.len()), (None, 0, 0));
    assert_eq!(a.duration_ms, 5000);
    let a = analyze(vec![0.5; 100], RATE);
    assert!(a.beats.is_empty());
    let a = analyze(std::iter::empty(), RATE);
    assert_eq!(a.duration_ms, 0);
    let garbage = analyze([f32::NAN, f32::INFINITY, -1e9].repeat(10_000), RATE);
    assert!(garbage.duration_ms > 0);
    assert!(a.timing_tracks().iter().all(|t| t.marks.is_empty()));
}

#[test]
fn files_are_decoded_and_problems_reported() {
    let dir = std::env::temp_dir().join(format!("pf-analysis-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let clicks = click_track(500, 250, 12.0, 0.9);
    let samples = render(12.0, &clicks, 0.0);
    let path = dir.join("clicks.wav");
    write_wav(&path, &samples);
    let a = analyze_file(&path).unwrap();
    assert!((a.tempo_bpm.unwrap() - 120.0).abs() < 2.0);
    check_beats(&a, &clicks, 500);
    let err = analyze_file(&dir.join("missing.wav")).unwrap_err();
    assert!(
        err.to_string().starts_with("PixelFlow can't find the music file"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A 16-bit mono WAV.
fn write_wav(path: &std::path::Path, samples: &[f32]) {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}

#[test]
fn quieter_off_beat_notes_do_not_move_the_beat() {
    let beats = click_track(500, 250, 30.0, 0.9);
    let mut rng = Noise(99);
    let mut all = beats.clone();
    for &(ms, _) in &beats {
        // An off-beat note after most beats, somewhere in the gap.
        if rng.next() > -0.4 {
            let offset = 150 + ((rng.next() + 1.0) * 100.0) as u64;
            all.push((ms + offset, 0.35));
        }
    }
    let a = analyze(render(30.0, &all, 0.02), RATE);
    assert!((a.tempo_bpm.unwrap() - 120.0).abs() < 2.0, "{:?}", a.tempo_bpm);
    check_beats(&a, &beats, 500);
    assert!(a.onsets.len() > beats.len(), "off-beat notes are onsets too");
}

/// Run with `cargo test -p pf-analysis --release -- --ignored --nocapture`.
#[test]
#[ignore = "benchmark"]
fn benchmark_three_minutes() {
    let clicks = click_track(500, 250, 180.0, 0.6);
    let samples = render(180.0, &clicks, 0.1);
    let started = std::time::Instant::now();
    let a = analyze(samples, RATE);
    println!(
        "3 min at 44.1 kHz: {:.0} ms, {} beats, {:?} BPM",
        started.elapsed().as_secs_f64() * 1000.0,
        a.beats.len(),
        a.tempo_bpm
    );
}
