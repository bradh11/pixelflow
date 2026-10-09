//! Moments on synthetic songs: drums told apart, a fill before a new section, stop-time, a
//! breakdown, a riser into an impact, a held last chord, and a key change.

use pf_analysis::{Analysis, Moment, MomentKind, analyze};
use std::f32::consts::TAU;

const RATE: u32 = 22_050;
/// 120 BPM: a beat every 0.5 s, a bar every 2 s.
const BEAT: f32 = 0.5;
const BAR: f32 = 4.0 * BEAT;

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

fn at(seconds: f32) -> usize {
    (seconds * RATE as f32) as usize
}

/// A sustained chord, `notes` in Hz with a few partials, from `from` to `to` (s), at `level`.
fn chord(out: &mut [f32], from: f32, to: f32, notes: &[f32], level: f32) {
    let (a, b) = (at(from), at(to).min(out.len()));
    for (i, o) in out[a..b].iter_mut().enumerate() {
        let t = (a + i) as f32 / RATE as f32;
        // A short fade in and out, so the chord's edges aren't clicks.
        let edge = ((i as f32 / RATE as f32) / 0.01)
            .min(((b - a - i) as f32 / RATE as f32) / 0.01)
            .min(1.0);
        let s: f32 = notes
            .iter()
            .flat_map(|&f| (1..=3).map(move |h| (TAU * f * h as f32 * t).sin() / h as f32))
            .sum();
        *o += level * edge * s / notes.len() as f32;
    }
}

/// A kick: a falling 60 Hz thump.
fn kick(out: &mut [f32], t: f32, level: f32) {
    for k in 0..at(0.25) {
        if let Some(s) = out.get_mut(at(t) + k) {
            let x = k as f32 / RATE as f32;
            *s += level * (-x / 0.07).exp() * (TAU * (50.0 + 60.0 * (-x / 0.02).exp()) * x).sin();
        }
    }
}

/// A snare: a burst of noise with a 190 Hz body.
fn snare(out: &mut [f32], t: f32, level: f32, noise: &mut Noise) {
    for k in 0..at(0.18) {
        if let Some(s) = out.get_mut(at(t) + k) {
            let x = k as f32 / RATE as f32;
            let body = 0.6 * (-x / 0.05).exp() * (TAU * 190.0 * x).sin();
            *s += level * ((-x / 0.045).exp() * noise.next() + body);
        }
    }
}

/// Bright noise (white noise, differenced twice), decaying over `decay` s: a hi-hat, or a crash.
fn cymbal(out: &mut [f32], t: f32, level: f32, decay: f32, noise: &mut Noise) {
    let (mut x1, mut x2) = (0.0, 0.0);
    for k in 0..at(5.0 * decay) {
        let x0 = noise.next();
        let bright = (x0 - 2.0 * x1 + x2) / 4.0;
        x2 = x1;
        x1 = x0;
        if let Some(s) = out.get_mut(at(t) + k) {
            *s += level * (-(k as f32 / RATE as f32) / decay).exp() * bright;
        }
    }
}

/// A rock beat from bar `from` to bar `to`: kicks on 1 and 3, snares on 2 and 4, hi-hats on
/// every eighth.
fn groove(out: &mut [f32], from: usize, to: usize, noise: &mut Noise) {
    for bar in from..to {
        let t = bar as f32 * BAR;
        for beat in 0..4 {
            let b = t + beat as f32 * BEAT;
            if beat % 2 == 0 {
                kick(out, b, 0.6);
            } else {
                snare(out, b, 0.35, noise);
            }
            cymbal(out, b, 0.25, 0.03, noise);
            cymbal(out, b + BEAT / 2.0, 0.2, 0.03, noise);
        }
    }
}

const C: [f32; 3] = [261.6, 329.6, 392.0];
const F: [f32; 3] = [349.2, 440.0, 523.3];
const G: [f32; 3] = [392.0, 493.9, 587.3];
const D: [f32; 3] = [293.7, 370.0, 440.0];
const A: [f32; 3] = [440.0, 554.4, 659.3];
const E_MINOR: [f32; 3] = [329.6, 392.0, 493.9];

fn run(name: &str, out: Vec<f32>) -> Analysis {
    if let Ok(dir) = std::env::var("PF_DUMP") {
        std::fs::write(
            format!("{dir}/{name}.f32"),
            out.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>(),
        )
        .unwrap();
    }
    analyze(out, RATE)
}

fn song(bars: usize) -> Vec<f32> {
    vec![0.0; at(bars as f32 * BAR)]
}

fn of(a: &Analysis, kind: MomentKind) -> Vec<&Moment> {
    a.moments.iter().filter(|m| m.kind == kind).collect()
}

fn near(ms: u64, seconds: f32, within_ms: u64) -> bool {
    ms.abs_diff((seconds * 1000.0) as u64) <= within_ms
}

#[test]
fn drums_are_told_apart_and_a_fill_leads_into_the_new_section() {
    let mut out = song(16);
    let mut noise = Noise(3);
    groove(&mut out, 0, 7, &mut noise);
    // Bar 8: the first half as usual, then sixteenth-note snares into bar 9.
    let t = 7.0 * BAR;
    kick(&mut out, t, 0.6);
    snare(&mut out, t + BEAT, 0.35, &mut noise);
    for k in 0..8 {
        snare(&mut out, t + 2.0 * BEAT + k as f32 * BEAT / 4.0, 0.3, &mut noise);
    }
    groove(&mut out, 8, 16, &mut noise);
    chord(&mut out, 0.0, 8.0 * BAR, &C, 0.15);
    chord(&mut out, 8.0 * BAR, 16.0 * BAR, &E_MINOR, 0.15);
    let a = run("fill", out);
    assert!((a.tempo_bpm.unwrap() - 120.0).abs() < 2.0, "{:?}", a.tempo_bpm);

    // About 2 kicks, 2 snares, and 8 hi-hats a bar.
    let total =
        |f: fn(&pf_analysis::BarDrums) -> u16| a.bar_drums.iter().map(|b| u32::from(f(b))).sum::<u32>();
    let bars = a.bar_drums.len() as f32;
    let per_bar = |n: u32| n as f32 / bars;
    assert!(
        (1.5..=2.5).contains(&per_bar(total(|b| b.kick))),
        "kicks {:?}",
        a.bar_drums
    );
    assert!(
        (1.8..=3.0).contains(&per_bar(total(|b| b.snare))),
        "snares {:?}",
        a.bar_drums
    );
    assert!(
        (5.0..=9.0).contains(&per_bar(total(|b| b.hat))),
        "hats {:?}",
        a.bar_drums
    );
    assert_eq!(total(|b| b.crash), 0);
    let busiest = a.bar_drums.iter().map(|b| b.snare).max().unwrap();
    assert!(busiest >= 7, "the fill's bar: {:?}", a.bar_drums);

    // Into the next downbeat (within a beat of bar 9: the downbeat found may be a beat off).
    let fills = of(&a, MomentKind::Fill);
    let fill = fills
        .iter()
        .find(|m| {
            m.end_ms
                .is_some_and(|e| near(e, 16.0, 550) && a.bars.contains(&e))
        })
        .unwrap_or_else(|| panic!("a fill into bar 9: {:?}", a.moments));
    assert!(fill.time_ms >= 14_950 && fill.time_ms <= 15_600, "{fill:?}");
    assert_eq!(fill.suggest.word(), "chase");
    assert!(fills.len() <= 2, "{fills:?}");
}

#[test]
fn a_stop_time_passage_is_one_stop() {
    let mut out = song(14);
    let mut noise = Noise(5);
    groove(&mut out, 0, 5, &mut noise);
    chord(&mut out, 0.0, 5.0 * BAR, &C, 0.15);
    // Bars 6–9: one stab on each downbeat, then nothing.
    for bar in 5..9 {
        let t = bar as f32 * BAR;
        kick(&mut out, t, 0.6);
        snare(&mut out, t, 0.35, &mut noise);
        cymbal(&mut out, t, 0.2, 0.03, &mut noise);
        chord(&mut out, t, t + 0.2, &C, 0.15);
    }
    groove(&mut out, 9, 14, &mut noise);
    chord(&mut out, 9.0 * BAR, 14.0 * BAR, &C, 0.15);
    let a = run("stoptime", out);
    let stops = of(&a, MomentKind::Stop);
    let passage = stops
        .iter()
        .find(|m| m.label.as_deref() == Some("stop-time"))
        .unwrap_or_else(|| panic!("a stop-time passage: {:?}", a.moments));
    assert!(near(passage.time_ms, 10.0, 150), "{passage:?}");
    assert!(near(passage.end_ms.unwrap(), 18.0, 150), "{passage:?}");
    assert_eq!(passage.suggest.word(), "blackout");
    // Its stops aren't listed one by one, and the band's return is a restart (or a drop: one
    // moment stands for both).
    assert!(
        stops
            .iter()
            .all(|m| m.time_ms < 9_000 || m.time_ms > 19_000 || m == passage),
        "{stops:?}"
    );
    assert!(
        a.moments
            .iter()
            .any(|m| matches!(m.kind, MomentKind::Restart | MomentKind::Drop) && near(m.time_ms, 18.0, 150)),
        "{:?}",
        a.moments
    );
}

#[test]
fn a_full_stop_and_a_held_stop() {
    let mut out = song(12);
    let mut noise = Noise(7);
    for bar in 0..12 {
        let t = bar as f32 * BAR;
        match bar {
            // Bar 5: one beat, then silence. Bar 9: one beat of drums, the chord rings on.
            4 | 8 => {
                kick(&mut out, t, 0.6);
                cymbal(&mut out, t, 0.25, 0.03, &mut noise);
                chord(&mut out, t, if bar == 4 { t + 0.2 } else { t + BAR }, &C, 0.15);
            }
            _ => {
                groove(&mut out, bar, bar + 1, &mut noise);
                chord(&mut out, t, t + BAR, &C, 0.15);
            }
        }
    }
    let a = run("stops", out);
    let stops = of(&a, MomentKind::Stop);
    let full = stops.iter().find(|m| near(m.time_ms, 8.0, 150));
    let held = stops.iter().find(|m| near(m.time_ms, 16.0, 150));
    assert_eq!(full.and_then(|m| m.label.as_deref()), Some("full"), "{stops:?}");
    assert_eq!(held.and_then(|m| m.label.as_deref()), Some("held"), "{stops:?}");
    assert!(near(full.unwrap().end_ms.unwrap(), 10.0, 150));
    for back in [10.0, 18.0] {
        assert!(
            of(&a, MomentKind::Restart)
                .iter()
                .any(|m| near(m.time_ms, back, 150)),
            "{:?}",
            a.moments
        );
    }
}

#[test]
fn a_breakdown_is_where_the_drums_drop_out() {
    let mut out = song(32);
    let mut noise = Noise(9);
    groove(&mut out, 0, 16, &mut noise);
    groove(&mut out, 24, 32, &mut noise);
    for bar in 0..32 {
        let notes = [&C, &F, &G, &C][bar % 4];
        chord(&mut out, bar as f32 * BAR, (bar + 1) as f32 * BAR, notes, 0.15);
    }
    let a = run("breakdown", out);
    let breakdowns = of(&a, MomentKind::Breakdown);
    assert_eq!(breakdowns.len(), 1, "{:?}", a.moments);
    let b = breakdowns[0];
    assert!(near(b.time_ms, 32.0, 2_100), "{b:?}");
    assert!(near(b.end_ms.unwrap(), 48.0, 2_100), "{b:?}");
    assert!(b.strength >= 0.5, "{b:?}");
    assert_eq!(b.suggest.word(), "minimal");
    assert!(
        a.moments
            .iter()
            .any(|m| matches!(m.kind, MomentKind::Restart | MomentKind::Drop) && near(m.time_ms, 48.0, 300)),
        "the drums back: {:?}",
        a.moments
    );
    // The chords held through it aren't holds of their own.
    assert!(
        of(&a, MomentKind::Hold)
            .iter()
            .all(|h| h.end_ms.unwrap() < 32_000 || h.time_ms > 48_000)
    );
}

#[test]
fn a_riser_builds_into_an_impact() {
    let mut out = song(18);
    let mut noise = Noise(11);
    groove(&mut out, 0, 8, &mut noise);
    chord(&mut out, 0.0, 8.0 * BAR, &C, 0.1);
    // Bars 9–12: noise and a tone rising, snares getting denser and harder.
    let (from, to) = (8.0 * BAR, 12.0 * BAR);
    let mut phase = 0.0f32;
    for (i, o) in out.iter_mut().enumerate().take(at(to)).skip(at(from)) {
        let x = (i as f32 / RATE as f32 - from) / (to - from);
        let hz = 400.0 * 10f32.powf(x);
        phase += TAU * hz / RATE as f32;
        *o += (0.02 + 0.25 * x) * (0.5 * noise.next() + 0.5 * phase.sin());
    }
    for bar in 8..12 {
        let step = BEAT / [1.0, 2.0, 4.0, 4.0][bar - 8];
        let mut t = bar as f32 * BAR;
        while t < (bar + 1) as f32 * BAR - 0.01 {
            snare(&mut out, t, 0.15 + 0.05 * (bar - 8) as f32, &mut noise);
            t += step;
        }
    }
    chord(&mut out, 8.0 * BAR, 12.0 * BAR, &C, 0.1);
    // The impact: a crash, a kick, and the band louder from bar 13.
    cymbal(&mut out, 12.0 * BAR, 0.9, 1.2, &mut noise);
    kick(&mut out, 12.0 * BAR, 0.9);
    groove(&mut out, 12, 18, &mut noise);
    chord(&mut out, 12.0 * BAR, 18.0 * BAR, &G, 0.25);
    let a = run("riser", out);
    let impact = of(&a, MomentKind::Impact)
        .into_iter()
        .find(|m| near(m.time_ms, 24.0, 150))
        .unwrap_or_else(|| panic!("an impact at 0:24: {:?}", a.moments));
    let build = of(&a, MomentKind::Build)
        .into_iter()
        .find(|m| m.label.as_deref() == Some("into impact"))
        .unwrap_or_else(|| panic!("a build into the impact: {:?}", a.moments));
    assert_eq!(build.end_ms, Some(impact.time_ms));
    assert!(build.time_ms >= 15_000 && build.time_ms <= 21_000, "{build:?}");
    assert!(
        a.drums
            .iter()
            .any(|d| d.drum == pf_analysis::Drum::Crash && near(d.time_ms, 24.0, 100)),
        "{:?}",
        a.drums
    );
    // It's among the song's most important moments.
    let top: Vec<_> = a.top_moments(5).iter().map(|m| (m.kind, m.time_ms)).collect();
    assert!(top.iter().any(|&(_, t)| near(t, 24.0, 150)), "{top:?}");
}

#[test]
fn the_last_chord_is_held() {
    let mut out = song(11);
    let mut noise = Noise(13);
    groove(&mut out, 0, 8, &mut noise);
    chord(&mut out, 0.0, 8.0 * BAR, &C, 0.15);
    // The band hits the downbeat of bar 9 and the chord rings for two bars.
    kick(&mut out, 8.0 * BAR, 0.6);
    snare(&mut out, 8.0 * BAR, 0.35, &mut noise);
    chord(&mut out, 8.0 * BAR, 10.5 * BAR, &G, 0.2);
    let a = run("hold", out);
    let hold = of(&a, MomentKind::Hold)
        .into_iter()
        .find(|m| m.time_ms >= 15_500)
        .unwrap_or_else(|| panic!("a hold: {:?}", a.moments));
    assert!(near(hold.time_ms, 16.0, 500), "{hold:?}");
    assert!(hold.end_ms.unwrap() - hold.time_ms >= 2_000, "{hold:?}");
    assert_eq!(hold.label.as_deref(), Some("end"), "{hold:?}");
    assert_eq!(hold.suggest.word(), "sustain");
}

#[test]
fn a_key_change_is_found_where_the_song_moves_up() {
    let mut out = song(32);
    let mut noise = Noise(17);
    groove(&mut out, 0, 32, &mut noise);
    // I–IV–V–I in C for 16 bars, then in D.
    for bar in 0..32 {
        let notes = if bar < 16 {
            [&C, &F, &G, &C][bar % 4]
        } else {
            [&D, &G, &A, &D][bar % 4]
        };
        chord(&mut out, bar as f32 * BAR, (bar + 1) as f32 * BAR, notes, 0.15);
    }
    let a = run("key", out);
    let changes = of(&a, MomentKind::KeyChange);
    assert_eq!(changes.len(), 1, "{:?} {:?}", changes, a.sections);
    assert!(near(changes[0].time_ms, 32.0, 2_100), "{:?}", changes[0]);
    assert_eq!(changes[0].label.as_deref(), Some("C→D"));
    assert_eq!(changes[0].suggest.word(), "color-shift");
}

#[test]
fn moments_serialize_and_become_timing_tracks() {
    let mut out = song(12);
    let mut noise = Noise(19);
    groove(&mut out, 0, 12, &mut noise);
    chord(&mut out, 0.0, 12.0 * BAR, &C, 0.15);
    out[at(4.0 * BAR + 0.3)..at(5.0 * BAR - 0.02)].fill(0.0);
    let a = run("serialize", out);
    let json = serde_json::to_value(&a).unwrap();
    let first = &json["moments"][0];
    assert!(
        first["timeMs"].is_u64() && first["importance"].is_f64() && first["suggest"].is_string(),
        "{first}"
    );
    assert!(json["barDrums"][0]["kick"].is_u64());
    assert!(json.get("shoutCues").is_none());
    assert!(
        json["moments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["kind"] == "stop")
    );

    let moments = a.moments_track();
    assert_eq!(moments.name, "Moments");
    assert!(
        moments.marks.iter().any(|m| m.label.starts_with("Stop")),
        "{:?}",
        moments.marks
    );
    assert!(moments.marks.windows(2).all(|w| w[0].end_ms <= w[1].start_ms));
    let drums = a.drums_track();
    assert_eq!(drums.name, "Drums");
    assert!(
        drums
            .marks
            .iter()
            .all(|m| ["Kick", "Snare", "Crash"].contains(&m.label.as_str()))
    );
    assert!(drums.marks.windows(2).all(|w| w[0].end_ms <= w[1].start_ms));
}
