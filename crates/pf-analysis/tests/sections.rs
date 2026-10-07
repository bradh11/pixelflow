//! Song sections from loudness, and stopping an analysis part way.

use pf_analysis::{AnalysisError, Level, analyze, analyze_cancellable};
use std::sync::atomic::{AtomicUsize, Ordering};

const RATE: u32 = 22_050;

/// A song in three parts: quiet, loud, quiet, with a 120 BPM click throughout (accented every
/// 4th beat, so bars are every 2 s from 0).
fn three_parts(quiet_s: u32, loud_s: u32) -> Vec<f32> {
    let total = 2 * quiet_s + loud_s;
    let n = (total * RATE) as usize;
    let mut out = vec![0.0f32; n];
    let mut seed = 11u64;
    let burst = (RATE / 100) as usize;
    for (i, s) in out.iter_mut().enumerate() {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let noise = ((seed >> 40) as f32 / (1u64 << 23) as f32) - 1.0;
        let t = i as u32 / RATE;
        let loud = t >= quiet_s && t < quiet_s + loud_s;
        *s = noise * if loud { 0.5 } else { 0.03 };
    }
    for beat in 0..(total * 2) {
        let start = (beat as usize) * (RATE as usize) / 2;
        let level = if beat % 4 == 0 { 0.9 } else { 0.4 };
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

#[test]
fn sections_follow_the_songs_loudness() {
    let a = analyze(three_parts(24, 32), RATE);
    assert_eq!(a.duration_ms, 80_000);
    assert_eq!(a.energy.len(), 80, "one loudness value per second");
    assert!(a.energy.iter().all(|e| (0.0..=1.0).contains(e)));
    assert!(a.energy[40] > 0.8 && a.energy[5] < 0.3, "{:?}", a.energy);

    let sections = a.sections();
    let levels: Vec<Level> = sections.iter().map(|s| s.level).collect();
    assert_eq!(levels, [Level::Low, Level::High, Level::Low], "{sections:?}");
    assert_eq!(sections[0].start_ms, 0);
    assert_eq!(sections[2].end_ms, 80_000);
    for pair in sections.windows(2) {
        assert_eq!(pair[0].end_ms, pair[1].start_ms, "sections cover the song");
    }
    assert!(sections[1].start_ms.abs_diff(24_000) <= 2_000, "{sections:?}");
    assert!(sections[1].end_ms.abs_diff(56_000) <= 2_000, "{sections:?}");
    assert_eq!(sections[0].label, "Intro");
    assert_eq!(sections[2].label, "Outro");

    let track = a.sections_track();
    assert_eq!(track.name, "Sections");
    assert_eq!(track.marks.len(), 3);
    assert_eq!(track.marks[1].label, sections[1].label);
}

#[test]
fn silence_is_one_quiet_section() {
    let a = analyze(vec![0.0; RATE as usize * 10], RATE);
    let sections = a.sections();
    assert_eq!(sections.len(), 1);
    assert_eq!((sections[0].start_ms, sections[0].end_ms), (0, 10_000));
    assert_eq!(sections[0].level, Level::Low);
    assert!(analyze(std::iter::empty(), RATE).sections().is_empty());
}

#[test]
fn an_analysis_stops_when_asked() {
    let checks = AtomicUsize::new(0);
    let stopped = analyze_cancellable(vec![0.1f32; RATE as usize * 30], RATE, &|| {
        checks.fetch_add(1, Ordering::SeqCst) >= 2
    });
    assert!(matches!(stopped, Err(AnalysisError::Cancelled)));
    assert!(
        checks.load(Ordering::SeqCst) < 20,
        "it stops early, not at the end"
    );
    assert!(analyze_cancellable(vec![0.1f32; RATE as usize], RATE, &|| false).is_ok());
}
