//! Song structure on synthetic songs: sections from timbre and harmony at equal loudness, repeats
//! grouped, the downbeat, hits, and breaks.

use pf_analysis::{Analysis, EventKind, analyze};
use std::f32::consts::TAU;

const RATE: u32 = 22_050;
/// 120 BPM: a beat every 0.5 s, a bar every 2 s.
const BEAT_S: f32 = 0.5;

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

/// A sustained chord, `notes` in Hz, each with `harmonics` partials (1 = pure sine, more =
/// brighter), from `from` to `to` (s), scaled to `rms`.
fn chord(out: &mut [f32], from: f32, to: f32, notes: &[f32], harmonics: usize, rms: f32) {
    let (a, b) = (at(from), at(to).min(out.len()));
    let mut part: Vec<f32> = (a..b)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            notes
                .iter()
                .flat_map(|&f| (1..=harmonics).map(move |h| (TAU * f * h as f32 * t).sin() / h as f32))
                .sum()
        })
        .collect();
    let now = (part.iter().map(|x| x * x).sum::<f32>() / part.len().max(1) as f32).sqrt();
    for (o, p) in out[a..b].iter_mut().zip(&mut part) {
        *o += *p * rms / now.max(1e-9);
    }
}

/// A kick (a click, then a falling 60 Hz thump) at `t` (s).
fn kick(out: &mut [f32], t: f32, level: f32, noise: &mut Noise) {
    for k in 0..at(0.2) {
        if let Some(s) = out.get_mut(at(t) + k) {
            let x = k as f32 / RATE as f32;
            let click = if x < 0.008 { 0.4 * noise.next() } else { 0.0 };
            *s += level * ((-x / 0.06).exp() * (TAU * 60.0 * x).sin() + click);
        }
    }
}

/// A snare (a burst of noise) at `t` (s).
fn snare(out: &mut [f32], t: f32, level: f32, noise: &mut Noise) {
    for k in 0..at(0.12) {
        if let Some(s) = out.get_mut(at(t) + k) {
            *s += level * (-(k as f32 / RATE as f32) / 0.03).exp() * noise.next();
        }
    }
}

/// A steady drum beat from `from` to `to` (s): kick on beats 1 and 3, snare on 2 and 4, with
/// beat 1 at `first_downbeat` (s).
fn drums(out: &mut [f32], from: f32, to: f32, first_downbeat: f32) {
    let mut noise = Noise(5);
    let mut beat = ((from - first_downbeat) / BEAT_S).ceil() as i64;
    loop {
        let t = first_downbeat + beat as f32 * BEAT_S;
        if t >= to {
            break;
        }
        if beat.rem_euclid(2) == 0 {
            kick(out, t, 0.5, &mut noise);
        } else {
            snare(out, t, 0.25, &mut noise);
        }
        beat += 1;
    }
}

const C_MAJOR: [f32; 3] = [261.6, 329.6, 392.0];
const F_SHARP_MINOR: [f32; 3] = [370.0, 440.0, 554.4];
const B_FLAT_MAJOR: [f32; 3] = [233.1, 293.7, 349.2];
const E_MINOR: [f32; 3] = [329.6, 392.0, 493.9];

/// A song of `parts` (chord, partials) of `part_s` seconds each, equally loud, over one drum beat.
fn song(parts: &[(&[f32], usize)], part_s: f32) -> Vec<f32> {
    let total = part_s * parts.len() as f32;
    let mut out = vec![0.0f32; at(total)];
    for (i, &(notes, harmonics)) in parts.iter().enumerate() {
        let from = i as f32 * part_s;
        chord(&mut out, from, from + part_s, notes, harmonics, 0.15);
    }
    drums(&mut out, 0.0, total, 0.0);
    out
}

fn starts(a: &Analysis) -> Vec<u64> {
    a.sections.iter().map(|s| s.start_ms).collect()
}

#[test]
fn sections_change_where_timbre_and_harmony_do_at_equal_loudness() {
    let parts: [(&[f32], usize); 4] = [
        (&C_MAJOR, 1),
        (&F_SHARP_MINOR, 6),
        (&B_FLAT_MAJOR, 3),
        (&E_MINOR, 10),
    ];
    let a = analyze(song(&parts, 16.0), RATE);
    assert!((a.tempo_bpm.unwrap() - 120.0).abs() < 2.0, "{:?}", a.tempo_bpm);
    let found = starts(&a);
    assert_eq!(found.len(), 4, "{:?}", a.sections);
    for (i, expected) in [0u64, 16_000, 32_000, 48_000].into_iter().enumerate() {
        assert!(
            found[i].abs_diff(expected) <= 2_000,
            "within a bar of {expected}: {found:?}"
        );
        assert!(
            a.bars.iter().any(|&b| b == found[i]) || found[i] == 0,
            "on a bar line: {found:?}"
        );
    }
    assert_eq!(a.sections.last().unwrap().end_ms, a.duration_ms);
    let groups: Vec<&str> = a.sections.iter().map(|s| s.group.as_str()).collect();
    assert_eq!(groups, ["A", "B", "C", "D"], "nothing repeats");
    // Loudness is level throughout, so it isn't what found them.
    let energies: Vec<f32> = a.sections.iter().map(|s| s.energy).collect();
    assert!(energies.iter().all(|&e| e > 0.8), "{energies:?}");
    assert!(a.confidence.sections > 0.0 && a.confidence.sections <= 1.0);
    // Sections and the timing track agree.
    let track = a.sections_track();
    assert_eq!(track.marks.len(), 4);
    assert_eq!(track.marks[1].start_ms, found[1]);
}

#[test]
fn repeated_material_shares_a_group() {
    let a_part: (&[f32], usize) = (&C_MAJOR, 1);
    let b_part: (&[f32], usize) = (&F_SHARP_MINOR, 8);
    let a = analyze(song(&[a_part, b_part, a_part, b_part, a_part], 16.0), RATE);
    let groups: Vec<&str> = a.sections.iter().map(|s| s.group.as_str()).collect();
    assert_eq!(groups, ["A", "B", "A", "B", "A"], "{:?}", a.sections);
    let labels: Vec<&str> = a.sections.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels[1], labels[3], "{labels:?}");
    assert_eq!(labels[0], labels[2], "{labels:?}");
    for (s, expected) in a.sections.iter().zip([0u64, 16_000, 32_000, 48_000, 64_000]) {
        assert!(s.start_ms.abs_diff(expected) <= 2_000, "{:?}", a.sections);
        assert!((0.0..=1.0).contains(&s.confidence));
    }
    // Repeated labels are numbered on the timing track.
    let track = a.sections_track();
    assert!(track.marks[3].label.ends_with(" 2"), "{:?}", track.marks);
}

#[test]
fn the_chorus_is_the_louder_repeat() {
    let verse: (&[f32], usize) = (&C_MAJOR, 1);
    let chorus: (&[f32], usize) = (&F_SHARP_MINOR, 8);
    let mut audio = song(&[verse, chorus, verse, chorus], 16.0);
    // The choruses are 6 dB louder.
    for (i, s) in audio.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        if (16.0..32.0).contains(&t) || t >= 48.0 {
            *s *= 2.0;
        }
    }
    let a = analyze(audio, RATE);
    let labels: Vec<&str> = a.sections.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, ["Verse", "Chorus", "Verse", "Chorus"], "{:?}", a.sections);
    assert!(a.sections[1].energy > a.sections[0].energy);
}

#[test]
fn bar_one_is_found_from_kick_and_chord_changes() {
    // Kick on 1 and 3, snare on 2 and 4, and a new chord every bar; the song starts on beat 3,
    // so the first downbeat is at 1.25 s.
    let first_downbeat = 1.25;
    let total = 40.0;
    let mut out = vec![0.0f32; at(total)];
    let chords = [C_MAJOR, E_MINOR, B_FLAT_MAJOR, F_SHARP_MINOR];
    let mut bar = 0;
    let mut t = first_downbeat - 2.0;
    while t < total {
        chord(
            &mut out,
            t.max(0.0),
            (t + 2.0).min(total),
            &chords[bar % 4],
            3,
            0.08,
        );
        t += 2.0;
        bar += 1;
    }
    drums(&mut out, 0.25, total, first_downbeat);
    let a = analyze(out, RATE);
    assert!((a.tempo_bpm.unwrap() - 120.0).abs() < 2.0, "{:?}", a.tempo_bpm);
    let first = a.bars[0];
    assert!(
        (first as f32 - first_downbeat * 1000.0).rem_euclid(2000.0) < 40.0
            || (first as f32 - first_downbeat * 1000.0).rem_euclid(2000.0) > 1960.0,
        "bars at {:?}",
        &a.bars[..4]
    );
    assert!(first < 3_400, "the first full bar: {first}");
    assert!(a.confidence.downbeat > 0.3, "{:?}", a.confidence);
    assert_eq!(a.bar_energy.len(), a.bars.len());
}

#[test]
fn hits_and_breaks_are_found() {
    let parts: [(&[f32], usize); 3] = [(&C_MAJOR, 3), (&C_MAJOR, 3), (&C_MAJOR, 3)];
    let mut audio = song(&parts, 16.0);
    // Two stabs, much louder than the beat; then the music stops for a second at 30 s.
    let mut noise = Noise(9);
    let stabs = [12.25f32, 37.75];
    for &t in &stabs {
        for k in 0..at(0.25) {
            let x = k as f32 / RATE as f32;
            audio[at(t) + k] += (-x / 0.12).exp() * (0.8 * noise.next() + 0.4 * (TAU * 196.0 * x).sin());
        }
    }
    audio[at(30.0)..at(31.0)].fill(0.0);
    let a = analyze(audio, RATE);

    let hits: Vec<_> = a.events.iter().filter(|e| e.kind == EventKind::Hit).collect();
    let strongest: Vec<u64> = {
        let mut h = hits.clone();
        h.sort_by(|x, y| y.strength.total_cmp(&x.strength));
        h.iter().take(3).map(|e| e.time_ms).collect()
    };
    for t in stabs {
        let ms = (t * 1000.0) as u64;
        assert!(
            strongest.iter().any(|&h| h.abs_diff(ms) <= 60),
            "a hit at {ms}: {strongest:?}"
        );
    }
    assert!(hits.len() <= 5, "a few a minute, not every beat: {}", hits.len());
    assert!(hits.iter().all(|h| (0.0..=1.0).contains(&h.strength)));

    let breaks: Vec<_> = a.events.iter().filter(|e| e.kind == EventKind::Break).collect();
    assert_eq!(breaks.len(), 1, "{:?}", a.events);
    assert!(breaks[0].time_ms.abs_diff(30_000) <= 60, "{:?}", breaks[0]);
    assert!(
        breaks[0].duration_ms.unwrap().abs_diff(1_000) <= 150,
        "{:?}",
        breaks[0]
    );
    assert!(
        a.events.windows(2).all(|w| w[0].time_ms <= w[1].time_ms),
        "in time order"
    );

    let accents = a.accents_track();
    assert_eq!(accents.name, "Accents");
    assert!(accents.check_marks().is_ok());
    assert!(
        accents
            .marks
            .iter()
            .any(|m| m.label == "Break" && m.start_ms.abs_diff(30_000) <= 60)
    );
}

#[test]
fn drops_and_builds_follow_the_energy() {
    // Quiet for 16 s, then rising over 8 s, then full on.
    let total = 48.0;
    let mut audio = song(&[(&C_MAJOR, 3), (&C_MAJOR, 3), (&C_MAJOR, 3)], 16.0);
    for (i, s) in audio.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let gain = if t < 16.0 {
            0.15
        } else if t < 24.0 {
            0.15 + 0.85 * (t - 16.0) / 8.0
        } else {
            1.0
        };
        *s *= gain;
    }
    let a = analyze(audio, RATE);
    assert_eq!(a.duration_ms, (total * 1000.0) as u64);
    let builds: Vec<_> = a.events.iter().filter(|e| e.kind == EventKind::Build).collect();
    assert!(
        builds
            .iter()
            .any(|b| b.time_ms.abs_diff(16_000) <= 2_000 && b.duration_ms.unwrap() >= 4_000),
        "{:?}",
        a.events
    );
    let quiet = a.bar_energy[2].overall;
    let loud = a.bar_energy[a.bar_energy.len() - 2].overall;
    assert!(quiet < 0.2 && loud > 0.9, "{quiet} {loud}");
    for e in &a.bar_energy {
        for x in [e.overall, e.low, e.mid, e.high] {
            assert!((0.0..=1.0).contains(&x));
        }
    }

    // A sudden jump instead is a drop.
    let mut audio = song(&[(&C_MAJOR, 3), (&C_MAJOR, 3), (&C_MAJOR, 3)], 16.0);
    for (i, s) in audio.iter_mut().enumerate() {
        if i < at(24.0) {
            *s *= 0.15;
        }
    }
    let a = analyze(audio, RATE);
    let drops: Vec<_> = a.events.iter().filter(|e| e.kind == EventKind::Drop).collect();
    assert_eq!(drops.len(), 1, "{:?}", a.events);
    assert!(drops[0].time_ms.abs_diff(24_000) <= 600, "{:?}", drops[0]);
}

#[test]
fn analysis_serializes_its_structure() {
    let a = analyze(song(&[(&C_MAJOR, 1), (&E_MINOR, 6)], 16.0), RATE);
    let json = serde_json::to_value(&a).unwrap();
    assert!(json["sections"][0]["group"].is_string());
    assert!(json["sections"][0]["confidence"].is_number());
    assert!(json["barEnergy"][0]["low"].is_number());
    assert!(json["confidence"]["downbeat"].is_number());
    assert!(json["events"].is_array());
    assert!(json.get("energy").is_none(), "per-second energy stays out");
}

#[test]
fn a_song_under_half_a_minute_is_analyzed() {
    // Too short to aim for a section boundary, though the harmony changes.
    let a = analyze(song(&[(&C_MAJOR, 1), (&E_MINOR, 6)], 11.0), RATE);
    assert_eq!(a.duration_ms, 22_000);
    assert!(!a.sections().is_empty());
}
