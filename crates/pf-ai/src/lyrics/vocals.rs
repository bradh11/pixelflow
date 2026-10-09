//! Where the singing is: stretches of sung words, broken where the voice rests for more than
//! about 1.5 s, then fine-tuned by [`pf_analysis::VocalActivity`] (a held last note keeps the
//! stretch going a little past the last word's start).

use pf_analysis::VocalActivity;

/// A rest longer than this ends a sung stretch.
pub const REST_MS: u64 = 1_500;
/// How far a stretch may grow past its last word while the voice still sounds.
const HELD_NOTE_MS: u64 = 600;
/// How far a stretch may start before its first word while the voice already sounds.
const LEAD_IN_MS: u64 = 200;
/// Steps in which a stretch grows.
const STEP_MS: u64 = 20;

/// Sung stretches from word times (in order): words closer than [`REST_MS`] share one.
pub fn regions_from_words(words: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut regions: Vec<(u64, u64)> = Vec::new();
    for &(start, end) in words {
        match regions.last_mut() {
            Some(last) if start <= last.1 + REST_MS => last.1 = last.1.max(end),
            _ => regions.push((start, end)),
        }
    }
    regions
}

/// Grows each stretch at its edges while the voice still sounds there, never into the next
/// stretch or past `end_ms`.
pub fn refine(regions: &[(u64, u64)], voice: &VocalActivity, end_ms: u64) -> Vec<(u64, u64)> {
    let mut out: Vec<(u64, u64)> = Vec::with_capacity(regions.len());
    for (k, &(start, end)) in regions.iter().enumerate() {
        let floor = out.last().map_or(0, |r: &(u64, u64)| r.1);
        let ceiling = regions.get(k + 1).map_or(end_ms, |r| r.0).min(end_ms);
        let mut s = start;
        while s > floor && start - s < LEAD_IN_MS && voice.is_vocal(s.saturating_sub(STEP_MS)) {
            s = s.saturating_sub(STEP_MS).max(floor);
        }
        let mut e = end;
        while e < ceiling && e - end < HELD_NOTE_MS && voice.is_vocal(e) {
            e = (e + STEP_MS).min(ceiling);
        }
        out.push((s, e.max(s + 1)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rests_over_a_second_and_a_half_end_a_stretch() {
        let words = [
            (1_000, 1_400),
            (1_400, 2_000),
            (3_000, 3_500),
            (5_100, 5_600),
            (20_000, 21_000),
        ];
        assert_eq!(
            regions_from_words(&words),
            [(1_000, 3_500), (5_100, 5_600), (20_000, 21_000)]
        );
        assert!(regions_from_words(&[]).is_empty());
    }

    #[test]
    fn a_held_note_keeps_the_stretch_going() {
        // The voice sounds from 0.9 s to 5.9 s (one value per 100 ms).
        let level: Vec<f32> = (0..100)
            .map(|i| if (9..59).contains(&i) { 0.9 } else { 0.0 })
            .collect();
        let voice = VocalActivity {
            hop_ms: 100.0,
            level,
            onsets: Vec::new(),
        };
        let refined = refine(&[(1_000, 3_500), (5_600, 5_700)], &voice, 10_000);
        // Starts 100 ms sooner and holds 600 ms longer; the second starts 200 ms sooner (the
        // most) and holds until the voice stops at 5.9 s.
        assert_eq!(refined, [(900, 4_100), (5_400, 5_900)]);
        // Without the voice sounding, nothing changes.
        let silent = VocalActivity {
            hop_ms: 100.0,
            level: vec![0.0; 100],
            onsets: Vec::new(),
        };
        assert_eq!(refine(&[(1_000, 3_500)], &silent, 10_000), [(1_000, 3_500)]);
    }
}
