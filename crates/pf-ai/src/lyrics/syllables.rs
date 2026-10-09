//! Syllables and mouth shapes from timed words, for the Lyrics (syllables) and Lyrics
//! (phonemes) tracks.
//!
//! - **Syllables:** each word is split as it's said ([`pf_lexicon::syllables`]) and its time
//!   shared by how long each syllable is held ([`pf_lexicon::Syllable::weight`]): a diphthong
//!   or long vowel more than a reduced one, a consonant adding a little. In a word longer than
//!   [`LONG_WORD_MS`], a boundary moves onto a nearby place the voice starts a note, at most
//!   [`NUDGE_MS`] and never squeezing a syllable under [`MIN_SYLLABLE_MS`].
//! - **Phonemes:** each syllable's time shared among its mouth shapes as xLights shares a
//!   word's (`LyricBreakdown.cpp`): MBP and etc (closing the mouth, the consonants) get
//!   [`SHORT_SHAPE_MS`] each and the vowels share the rest.
//!
//! Every syllable starts where the one before ends, the first at its word's start and the last
//! ending at its word's end.

use pf_lexicon::{Phone, mouth_shapes};
use pf_sequence::Mark;

/// Words longer than this have their syllable boundaries moved onto where the voice starts a
/// note.
pub const LONG_WORD_MS: u64 = 250;
/// How far a syllable boundary moves onto a vocal onset, at most.
pub const NUDGE_MS: u64 = 60;
/// A move never leaves a syllable shorter than this (or than half its share, if that's less).
pub const MIN_SYLLABLE_MS: u64 = 40;
/// How long xLights holds an MBP or etc mouth (the vowels share the rest).
pub const SHORT_SHAPE_MS: u64 = 50;

/// The marks for one lyrics track's syllables and mouth shapes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sung {
    pub syllables: Vec<Mark>,
    pub phonemes: Vec<Mark>,
}

/// `start..end` shared by `weights`, exactly and in order: each part at least 1 ms (`None`
/// when the span is shorter than that allows).
pub fn share(start: u64, end: u64, weights: &[f64]) -> Option<Vec<(u64, u64)>> {
    let k = weights.len() as u64;
    let length = end.checked_sub(start)?;
    if k == 0 || length < k {
        return None;
    }
    let total: f64 = weights.iter().map(|w| w.max(0.0)).sum();
    let mut bounds = vec![start];
    let mut sum = 0.0;
    for (i, w) in weights.iter().enumerate().take(weights.len() - 1) {
        sum += w.max(0.0);
        let even = (i as f64 + 1.0) / k as f64;
        let at = if total > 0.0 { sum / total } else { even };
        let i = i as u64;
        let lowest = bounds[bounds.len() - 1] + 1;
        let highest = end - (k - 1 - i);
        let b = start + (length as f64 * at).round() as u64;
        bounds.push(b.clamp(lowest, highest));
    }
    bounds.push(end);
    Some(bounds.windows(2).map(|w| (w[0], w[1])).collect())
}

/// Moves the boundaries between `parts` (back to back) onto the nearest vocal onset within
/// reach: [`NUDGE_MS`], or less where a syllable would end up shorter than [`MIN_SYLLABLE_MS`]
/// (or half its share). `onsets` in order.
pub fn nudge(parts: &mut [(u64, u64)], onsets: &[u64]) {
    let shortest: Vec<u64> = parts
        .iter()
        .map(|&(s, e)| MIN_SYLLABLE_MS.min((e - s) / 2).max(1))
        .collect();
    for i in 1..parts.len() {
        let b = parts[i].0;
        let lowest = (parts[i - 1].0 + shortest[i - 1]).max(b.saturating_sub(NUDGE_MS));
        let highest = parts[i].1.saturating_sub(shortest[i]).min(b + NUDGE_MS);
        if lowest > highest {
            continue;
        }
        let from = onsets.partition_point(|&o| o < lowest);
        let near = onsets[from..]
            .iter()
            .take_while(|&&o| o <= highest)
            .min_by_key(|&&o| (o.abs_diff(b), o));
        if let Some(&o) = near {
            parts[i - 1].1 = o;
            parts[i].0 = o;
        }
    }
}

/// Mouth-shape marks for `phones` sung from `start` to `end`, as xLights times a word's: MBP
/// and etc [`SHORT_SHAPE_MS`] each (or an even share, when that's shorter), the others the
/// rest. Too short for them all: the vowel's shape only.
pub fn shape_marks(start: u64, end: u64, phones: &[Phone]) -> Vec<Mark> {
    let shapes = mouth_shapes(phones);
    let length = end.saturating_sub(start);
    if shapes.is_empty() || length == 0 {
        return Vec::new();
    }
    if length < shapes.len() as u64 {
        let vowel = phones.iter().find(|p| p.is_vowel()).unwrap_or(&phones[0]);
        return vec![Mark::new(start, end, vowel.arpa.mouth())];
    }
    let n = shapes.len() as f64;
    let is_short = |s: &str| matches!(s, "MBP" | "etc");
    let shorts = shapes.iter().filter(|s| is_short(s)).count() as f64;
    let even = length as f64 / n;
    let (short, long) = if shapes.len() > 1 && n > shorts {
        let short = SHORT_SHAPE_MS as f64;
        let short = if even < short { even } else { short };
        (short, (length as f64 - shorts * short) / (n - shorts))
    } else {
        (even, even)
    };
    let mut out = Vec::new();
    let (mut at, mut so_far) = (start, 0.0);
    for (k, shape) in shapes.iter().enumerate() {
        so_far += if is_short(shape) { short } else { long };
        let next = if k + 1 == shapes.len() {
            end
        } else {
            (start + so_far.round() as u64).clamp(at, end)
        };
        if next > at {
            out.push(Mark::new(at, next, *shape));
        }
        at = next;
    }
    out
}

/// The syllable and mouth-shape marks for a word whose syllables and sounds were each timed
/// ([`super::forced`]): a mark per syllable, and one per sound's mouth shape, a run of `etc`
/// kept as one (as [`mouth_shapes`] keeps it).
pub fn timed_marks(syllables: &[pf_align::SyllableTime]) -> Sung {
    let mut sung = Sung::default();
    for syllable in syllables {
        if syllable.end_ms > syllable.start_ms {
            sung.syllables.push(Mark::new(
                syllable.start_ms,
                syllable.end_ms,
                syllable.text.clone(),
            ));
        }
        for phone in &syllable.phones {
            let shape = phone.phone.arpa.mouth();
            match sung.phonemes.last_mut() {
                Some(last) if shape == "etc" && last.label == "etc" && last.end_ms == phone.start_ms => {
                    last.end_ms = phone.end_ms;
                }
                _ => sung.phonemes.push(Mark::new(phone.start_ms, phone.end_ms, shape)),
            }
        }
    }
    sung
}

/// The syllables and mouth shapes for timed `words` (in order, not overlapping): the
/// syllables of a word longer than [`LONG_WORD_MS`] nudged onto `onsets` (where the voice
/// starts a note, in order). A word too short to split stays whole.
pub fn sung_marks(words: &[Mark], onsets: &[u64]) -> Sung {
    let mut sung = Sung::default();
    for word in words {
        let syllables = pf_lexicon::syllables(word.said());
        let weights: Vec<f64> = syllables.iter().map(pf_lexicon::Syllable::weight).collect();
        let Some(mut parts) = share(word.start_ms, word.end_ms, &weights) else {
            if word.end_ms > word.start_ms {
                sung.syllables
                    .push(Mark::new(word.start_ms, word.end_ms, word.label.clone()));
                let phones: Vec<Phone> = syllables.iter().flat_map(|s| s.phones.clone()).collect();
                sung.phonemes
                    .extend(shape_marks(word.start_ms, word.end_ms, &phones));
            }
            continue;
        };
        if word.end_ms - word.start_ms > LONG_WORD_MS {
            nudge(&mut parts, onsets);
        }
        for (syllable, &(start, end)) in syllables.iter().zip(&parts) {
            sung.syllables.push(Mark::new(start, end, syllable.text.clone()));
            sung.phonemes.extend(shape_marks(start, end, &syllable.phones));
        }
    }
    sung
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(marks: &[Mark]) -> Vec<(u64, u64, &str)> {
        marks
            .iter()
            .map(|m| (m.start_ms, m.end_ms, m.label.as_str()))
            .collect()
    }

    #[test]
    fn a_words_time_is_shared_exactly_with_none_overlapping() {
        for (weights, start, end) in [
            (&[1.0, 2.0, 1.0][..], 1_000, 1_997),
            (&[1.5, 0.7, 0.7, 0.7], 0, 7),
            (&[1.0, 1.0], 10, 12),
            (&[0.0, 0.0, 0.0], 0, 100),
            (&[3.0], 5, 6),
        ] {
            let parts = share(start, end, weights).unwrap();
            assert_eq!(parts.len(), weights.len());
            assert_eq!(parts[0].0, start);
            assert_eq!(parts.last().unwrap().1, end);
            for pair in parts.windows(2) {
                assert_eq!(pair[0].1, pair[1].0, "{parts:?}");
            }
            assert!(parts.iter().all(|(s, e)| e > s), "{parts:?}");
        }
        assert_eq!(
            share(1_000, 2_000, &[1.0, 3.0]),
            Some(vec![(1_000, 1_250), (1_250, 2_000)])
        );
        assert_eq!(
            share(0, 2, &[1.0, 1.0, 1.0]),
            None,
            "a millisecond each won't fit"
        );
        assert_eq!(share(5, 5, &[1.0]), None);
    }

    #[test]
    fn long_vowels_get_longer_syllables() {
        // "beautiful": beau (UW1) holds longest, ti (AH0) shortest.
        let sung = sung_marks(&[Mark::new(0, 900, "beautiful")], &[]);
        assert_eq!(
            sung.syllables
                .iter()
                .map(|m| m.label.as_str())
                .collect::<Vec<_>>(),
            ["beau", "ti", "ful"]
        );
        let lengths: Vec<u64> = sung.syllables.iter().map(|m| m.end_ms - m.start_ms).collect();
        assert!(lengths[0] > lengths[2] && lengths[2] > lengths[1], "{lengths:?}");
        assert_eq!(lengths.iter().sum::<u64>(), 900);
    }

    #[test]
    fn onsets_nudge_boundaries_only_so_far() {
        let parts = vec![(1_000, 1_300), (1_300, 1_600)];
        // An onset 40 ms off: taken.
        let mut nudged = parts.clone();
        nudge(&mut nudged, &[900, 1_340, 2_000]);
        assert_eq!(nudged, [(1_000, 1_340), (1_340, 1_600)]);
        // The nearest of two in reach.
        let mut nudged = parts.clone();
        nudge(&mut nudged, &[1_250, 1_290, 1_330]);
        assert_eq!(nudged, [(1_000, 1_290), (1_290, 1_600)]);
        // Beyond NUDGE_MS: left alone.
        let mut nudged = parts.clone();
        nudge(&mut nudged, &[1_300 - NUDGE_MS - 1, 1_300 + NUDGE_MS + 1]);
        assert_eq!(nudged, parts);
        // Never squeezing a syllable under its least (half its 50 ms share here).
        let mut short = vec![(0, 50), (50, 400)];
        nudge(&mut short, &[10, 24]);
        assert_eq!(short, [(0, 50), (50, 400)]);
        nudge(&mut short, &[10, 25]);
        assert_eq!(short, [(0, 25), (25, 400)]);
        // Only long words are nudged, the same every time.
        let onsets = [1_150, 1_230, 5_120];
        let short_word = sung_marks(&[Mark::new(5_000, 5_240, "gonna")], &onsets);
        assert_eq!(
            short_word.syllables[1].start_ms,
            share(5_000, 5_240, &weights("gonna")).unwrap()[1].0
        );
        let long_word = sung_marks(&[Mark::new(1_000, 1_400, "gonna")], &onsets);
        assert_eq!(
            long_word,
            sung_marks(&[Mark::new(1_000, 1_400, "gonna")], &onsets)
        );
        let moved = long_word.syllables[1].start_ms;
        let was = share(1_000, 1_400, &weights("gonna")).unwrap()[1].0;
        assert!(moved.abs_diff(was) <= NUDGE_MS, "{was} → {moved}");
        assert!(onsets.contains(&moved), "{moved}");
    }

    fn weights(word: &str) -> Vec<f64> {
        pf_lexicon::syllables(word)
            .iter()
            .map(pf_lexicon::Syllable::weight)
            .collect()
    }

    #[test]
    fn mouth_shapes_share_a_syllable_as_xlights_shares_a_word() {
        // "ghost": G OW S T is etc O etc; the etc runs get 50 ms, the O the rest.
        let phones = pf_lexicon::parse_phones("G OW1 S T").unwrap();
        assert_eq!(
            spans(&shape_marks(1_000, 1_400, &phones)),
            [(1_000, 1_050, "etc"), (1_050, 1_350, "O"), (1_350, 1_400, "etc")]
        );
        // Shorter than 50 ms each: an even share.
        assert_eq!(
            spans(&shape_marks(0, 60, &phones)),
            [(0, 20, "etc"), (20, 40, "O"), (40, 60, "etc")]
        );
        // Too short for all three: the vowel.
        assert_eq!(spans(&shape_marks(0, 2, &phones)), [(0, 2, "O")]);
        assert!(shape_marks(0, 100, &[]).is_empty());
    }

    #[test]
    fn words_become_syllables_and_mouth_shapes() {
        let words = [
            Mark::new(1_000, 1_900, "Ghostbusters!"),
            Mark::new(2_000, 2_001, "glow"),
        ];
        let sung = sung_marks(&words, &[]);
        let labels: Vec<&str> = sung.syllables.iter().map(|m| m.label.as_str()).collect();
        assert_eq!(labels, ["Ghost", "bus", "ters!", "glow"]);
        assert_eq!(sung.syllables[0].start_ms, 1_000);
        assert_eq!(sung.syllables[2].end_ms, 1_900);
        // Phonemes stay inside their syllables, in order, back to back.
        for syllable in &sung.syllables[..3] {
            let inside: Vec<&Mark> = sung
                .phonemes
                .iter()
                .filter(|p| p.start_ms >= syllable.start_ms && p.end_ms <= syllable.end_ms)
                .collect();
            assert_eq!(inside.first().unwrap().start_ms, syllable.start_ms);
            assert_eq!(inside.last().unwrap().end_ms, syllable.end_ms);
        }
        assert_eq!(
            spans(&sung.phonemes[..3]),
            [(1_000, 1_050, "etc"), (1_050, 1_347, "O"), (1_347, 1_397, "etc")]
        );
        // A 1 ms word stays whole with its vowel's shape.
        assert_eq!(
            spans(&sung.phonemes[sung.phonemes.len() - 1..]),
            [(2_000, 2_001, "O")]
        );
    }

    #[test]
    fn timed_sounds_become_marks_with_etc_runs_kept_as_one() {
        let heard = |start: u64| pf_align::CharTime {
            start_ms: start,
            end_ms: start + 20,
            confidence: 0.9,
        };
        // "ghost": g h o s t heard at 1000, 1020, 1040, 1500, 1560; held to 1700.
        let chars: Vec<_> = [1_000, 1_020, 1_040, 1_500, 1_560]
            .into_iter()
            .map(|t| Some(heard(t)))
            .collect();
        let sounds = pf_align::word_sounds("ghost", 1_000, 1_700, &chars);
        let sung = timed_marks(&sounds);
        let spans: Vec<(u64, u64, &str)> = sung
            .phonemes
            .iter()
            .map(|m| (m.start_ms, m.end_ms, m.label.as_str()))
            .collect();
        // G is etc, OW is O, S T one etc run.
        assert_eq!(
            spans,
            [(1_000, 1_040, "etc"), (1_040, 1_500, "O"), (1_500, 1_680, "etc")]
        );
        assert_eq!(sung.syllables.len(), 1);
        assert_eq!(
            (sung.syllables[0].start_ms, sung.syllables[0].end_ms),
            (1_000, 1_700)
        );
    }
}
