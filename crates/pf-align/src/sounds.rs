//! A word's syllables and sounds timed from its letters: each sound (phone) starts when the
//! first letter it's written with was heard ([`pf_lexicon::spelled_syllables`]: "gh" for the G
//! of "ghost", "o" for its OW), and lasts until the next one starts. Sounds written with the
//! same letter ("x" is K S) share its time; a letter the model didn't hear takes its time from
//! the letters either side. The last sound runs to the word's end, but a closing consonant only
//! for [`FINAL_CONSONANT_MS`]: the mouth rests after it.

use crate::align::CharTime;
use pf_lexicon::Phone;

/// The longest a word's last sound is held when it's a consonant (ms).
pub const FINAL_CONSONANT_MS: u64 = 120;

/// One sound, timed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhoneTime {
    pub phone: Phone,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// One syllable, timed, with its sounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyllableTime {
    /// Its share of the word as written ("Ghost", "bus", "ters!").
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub phones: Vec<PhoneTime>,
}

/// When each character of a word of `n` characters was heard, from `chars` (a time per
/// character, `None` where not heard): those not heard are placed between the ones either side
/// (the word's start before the first, its end after the last).
fn char_starts(chars: &[Option<CharTime>], n: usize, start_ms: u64, end_ms: u64) -> Vec<f64> {
    let first_heard = chars.first().copied().flatten().is_some();
    let known: Vec<(f64, f64)> = (!first_heard)
        .then_some((0.0, start_ms as f64))
        .into_iter()
        .chain((0..n).filter_map(|i| {
            chars
                .get(i)
                .copied()
                .flatten()
                .map(|c| (i as f64, c.start_ms as f64))
        }))
        .chain(std::iter::once((n as f64, end_ms as f64)))
        .collect();
    (0..n)
        .map(|i| {
            let x = i as f64;
            let after = known.iter().position(|&(k, _)| k >= x).unwrap_or(known.len() - 1);
            let (k1, t1) = known[after];
            if k1 == x || after == 0 {
                return t1;
            }
            let (k0, t0) = known[after - 1];
            t0 + (t1 - t0) * (x - k0) / (k1 - k0)
        })
        .collect()
}

/// `word`'s syllables and sounds, sung from `start_ms` to `end_ms`, timed by when its
/// characters were heard (`chars`, one per character of `word`; see the module notes). A word
/// with no sounds ("42") is one syllable without any.
pub fn word_sounds(word: &str, start_ms: u64, end_ms: u64, chars: &[Option<CharTime>]) -> Vec<SyllableTime> {
    let end_ms = end_ms.max(start_ms);
    let spelled = pf_lexicon::spelled_syllables(word);
    let n = word.chars().count();
    let heard = char_starts(chars, n, start_ms, end_ms);
    // Each phone's start: when its first letter was heard, in order, inside the word.
    let mut phones: Vec<(usize, Phone, f64)> = Vec::new();
    for (s, syllable) in spelled.iter().enumerate() {
        for (phone, &at) in syllable.syllable.phones.iter().zip(&syllable.phone_starts) {
            let t = heard.get(at).copied().unwrap_or(end_ms as f64);
            phones.push((s, *phone, t));
        }
    }
    let span = (end_ms - start_ms) as f64;
    let mut floor = start_ms as f64;
    for (k, p) in phones.iter_mut().enumerate() {
        p.2 = if k == 0 {
            start_ms as f64
        } else {
            p.2.clamp(floor, end_ms as f64)
        };
        floor = p.2;
    }
    // Phones heard at the same time (written with one letter) share it with the next.
    let mut k = 0;
    while k < phones.len() {
        let same = phones[k..].iter().take_while(|p| p.2 == phones[k].2).count();
        let next = phones.get(k + same).map_or(end_ms as f64, |p| p.2);
        for j in 1..same {
            phones[k + j].2 = phones[k].2 + (next - phones[k].2) * j as f64 / same as f64;
        }
        k += same;
    }
    // Too short for every phone a millisecond: shared evenly.
    let round = |t: f64| t.round() as u64;
    let distinct = phones.windows(2).all(|w| round(w[0].2) < round(w[1].2));
    if !distinct || (span as usize) < phones.len() {
        let count = phones.len() as f64;
        for (k, p) in phones.iter_mut().enumerate() {
            p.2 = start_ms as f64 + span * k as f64 / count;
        }
    }
    let mut out: Vec<SyllableTime> = spelled
        .iter()
        .map(|s| SyllableTime {
            text: s.syllable.text.clone(),
            start_ms,
            end_ms,
            phones: Vec::new(),
        })
        .collect();
    for (k, &(s, phone, t)) in phones.iter().enumerate() {
        let start = round(t);
        let end = match phones.get(k + 1) {
            Some(next) => round(next.2),
            None if phone.is_vowel() => end_ms,
            None => end_ms.min(start + FINAL_CONSONANT_MS),
        };
        if end > start {
            out[s].phones.push(PhoneTime {
                phone,
                start_ms: start,
                end_ms: end,
            });
        }
    }
    // Syllables meet: each from its first sound (the first from the word's start) to the next.
    let starts: Vec<u64> = out
        .iter()
        .enumerate()
        .map(|(i, s)| match (i, s.phones.first()) {
            (0, _) => start_ms,
            (_, Some(p)) => p.start_ms,
            (_, None) => end_ms,
        })
        .collect();
    for (i, s) in out.iter_mut().enumerate() {
        s.start_ms = starts[i];
        s.end_ms = starts.get(i + 1).copied().unwrap_or(end_ms).max(s.start_ms);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_lexicon::spelled;

    fn heard(starts: &[Option<u64>]) -> Vec<Option<CharTime>> {
        starts
            .iter()
            .map(|s| {
                s.map(|t| CharTime {
                    start_ms: t,
                    end_ms: t + 20,
                    confidence: 0.9,
                })
            })
            .collect()
    }

    fn phones(s: &[SyllableTime]) -> Vec<(String, u64, u64)> {
        s.iter()
            .flat_map(|s| &s.phones)
            .map(|p| (spelled(&[p.phone]), p.start_ms, p.end_ms))
            .collect()
    }

    #[test]
    fn each_sound_starts_when_its_letters_are_heard() {
        // "ghost" held: g h at 1000, o at 1040, s at 1500, t at 1560; the word to 1700.
        let chars = heard(&[Some(1_000), Some(1_020), Some(1_040), Some(1_500), Some(1_560)]);
        let s = word_sounds("ghost", 1_000, 1_700, &chars);
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].start_ms, s[0].end_ms), (1_000, 1_700));
        assert_eq!(
            phones(&s),
            [
                ("G".into(), 1_000, 1_040),
                ("OW1".into(), 1_040, 1_500),
                ("S".into(), 1_500, 1_560),
                // A closing consonant isn't held to the word's end.
                ("T".into(), 1_560, 1_680),
            ]
        );
    }

    #[test]
    fn syllables_meet_and_take_their_sounds_times() {
        // "afraid": a at 2000, f 2100, r 2140, a 2180, i 2300, d 2600; to 2700.
        let chars = heard(&[
            Some(2_000),
            Some(2_100),
            Some(2_140),
            Some(2_180),
            Some(2_300),
            Some(2_600),
        ]);
        let s = word_sounds("afraid", 2_000, 2_700, &chars);
        let spans: Vec<(&str, u64, u64)> = s
            .iter()
            .map(|s| (s.text.as_str(), s.start_ms, s.end_ms))
            .collect();
        assert_eq!(spans, [("a", 2_000, 2_100), ("fraid", 2_100, 2_700)]);
        assert_eq!(phones(&s)[3], ("EY1".into(), 2_180, 2_600));
        // The vowel ending a word holds to its end.
        let s = word_sounds("go", 0, 900, &heard(&[Some(0), Some(60)]));
        assert_eq!(phones(&s), [("G".into(), 0, 60), ("OW1".into(), 60, 900)]);
    }

    #[test]
    fn letters_not_heard_and_sounds_sharing_a_letter_are_shared_out() {
        // "box": B O K S, the x heard at 300; the o not heard.
        let s = word_sounds("box", 0, 600, &heard(&[Some(0), None, Some(300)]));
        assert_eq!(
            phones(&s),
            [
                ("B".into(), 0, 150),
                ("AA1".into(), 150, 300),
                ("K".into(), 300, 450),
                ("S".into(), 450, 570),
            ]
        );
        // Nothing heard at all: spread by letters.
        let s = word_sounds("ghost", 0, 500, &[]);
        assert_eq!(phones(&s)[0], ("G".into(), 0, 200));
        // Too short to give each sound a millisecond: shared evenly, nothing empty.
        let s = word_sounds("ghost", 0, 3, &[]);
        assert!(phones(&s).iter().all(|p| p.2 > p.1));
        // No sounds.
        let s = word_sounds("42", 0, 300, &[]);
        assert_eq!((s.len(), s[0].phones.len(), s[0].end_ms), (1, 0, 300));
    }
}
