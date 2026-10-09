//! Locking word times onto the voice ([`pf_analysis::VocalTrack`]): speech recognition's word
//! times drift 100–300 ms on music, and published lines are often a little early or late.
//!
//! 1. **The whole song**: word starts (as an impulse train) are compared with where the voice
//!    starts a sound, every [`LAG_STEP_MS`] from −[`SHIFT_RANGE_MS`] to +[`SHIFT_RANGE_MS`].
//!    When one shift stands out clearly ([`SHIFT_SURE`]), every word moves by it. Each line
//!    with enough words is then checked the same way, and moved again only when its own
//!    evidence is strong ([`LINE_SURE`]).
//! 2. **Each word's start**: within [`SEARCH_MS`] of where it now is, onto the strongest place
//!    the voice starts a sound, nearer ones preferred: a hiss or pop for a word starting with a
//!    consonant like that (P B T D K G F V S Z SH CH JH TH, from [`pf_lexicon`]), a pitched
//!    rise for the rest. A word with no clear onset nearby keeps its shifted time.
//! 3. **Each word's end**: where the voice stops ([`VocalTrack::offsets`]) or dips before the
//!    next word (a drop of [`DIP`] or more from the word's loudest), or held on to the next
//!    word less [`END_GAP_MS`] when it doesn't; a last word in a line is held at most [`HOLD_MS`] past its own end while the
//!    voice still sounds. Gaps where the voice rests are left empty.
//!
//! Words stay in order, each at least [`MIN_WORD_MS`] long. Without recognized words (line
//! times only, [`spread_onto_voice`]), each line's start is moved onto the voice's first
//! onset near it, its words onto the onsets inside it, and their ends to where the voice dips.

use super::combine::{Phrase, Word, WordSource};
use pf_analysis::VocalTrack;
use pf_lexicon::Arpa;
use serde::Serialize;

/// The furthest the whole song is shifted (ms).
pub const SHIFT_RANGE_MS: i64 = 400;
/// The furthest a line is shifted again after the whole song (ms).
pub const LINE_SHIFT_RANGE_MS: i64 = 250;
/// Shifts tried, this far apart (ms).
pub const LAG_STEP_MS: i64 = 5;
/// A shift smaller than this (ms) is within what the onsets themselves can tell (a sung vowel
/// swells for a few tens of ms after its consonant), and isn't made.
pub const MIN_SHIFT_MS: i64 = 40;
/// How far the best shift's score must stand above the rest (robust z-score) to be used.
pub const SHIFT_SURE: f32 = 5.0;
/// The same for one line, which has fewer words to go by.
pub const LINE_SURE: f32 = 5.0;
/// The fewest timed words in a line for it to be shifted on its own.
const LINE_MIN_WORDS: usize = 4;
/// How far a word's start may move onto an onset (ms).
pub const SEARCH_MS: f64 = 150.0;
/// How far a published line's start may move onto the voice's first onset (ms).
pub const LINE_SEARCH_MS: f64 = 400.0;
/// How quickly an onset's pull falls off with distance (ms).
const NEAR_MS: f64 = 100.0;
/// The shortest a word is made (ms).
pub const MIN_WORD_MS: u64 = 60;
/// The gap left before the next word when the voice runs straight on (ms).
pub const END_GAP_MS: u64 = 10;
/// How far a word may be held past its own end while the voice still sounds (ms).
pub const HOLD_MS: u64 = 400;
/// The fall in loudness (0–1, of 50 dB) from a word's loudest that ends it.
pub const DIP: f32 = 0.16;
/// The word ends where loudness comes within this of the dip's bottom, or this share of the
/// way down into it.
const DIP_EDGE: f32 = 0.06;
const DIP_SHARE: f32 = 0.35;
/// Onsets this far apart (ms) are rivals for a word's start.
const RIVAL_MS: f64 = 40.0;
/// How clear an onset must be (0–1: above the music around it, and above its rivals) for a
/// word to be moved onto it.
pub const SURE_ONSET: f32 = 0.3;

/// What locking did, for the user and for checking it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// The whole song's shift (ms; negative is earlier), when one stood out.
    pub shift_ms: i64,
    /// The best shift found, made or not.
    pub best_shift_ms: i64,
    /// How clearly it stood out (robust z-score of the best shift).
    pub shift_score: f32,
    pub shifted: bool,
    /// Lines shifted on their own.
    pub lines_shifted: usize,
    pub words: usize,
    /// Words moved onto a clear onset.
    pub locked: usize,
    /// Words whose start moved, earlier and later.
    pub earlier: usize,
    pub later: usize,
    /// How far starts moved (ms): mean, median, and 90th percentile of the distance.
    pub mean_move_ms: f64,
    pub median_move_ms: f64,
    pub p90_move_ms: f64,
    /// The mean distance (ms) from word starts to the nearest onset, before and after.
    pub onset_distance_before_ms: f64,
    pub onset_distance_after_ms: f64,
    /// The share of neighbouring words in a line with a gap over 50 ms, before and after.
    pub gaps_before: f64,
    pub gaps_after: f64,
    /// The median word length (ms), before and after.
    pub median_length_before_ms: f64,
    pub median_length_after_ms: f64,
}

impl Report {
    /// What to tell the user ("Word timing locked to the vocals (average shift 120 ms).");
    /// `None` when nothing moved.
    pub fn sentence(&self) -> Option<String> {
        if self.words == 0 || (self.locked == 0 && !self.shifted) {
            return None;
        }
        Some(format!(
            "Word timing locked to the vocals (average shift {} ms).",
            self.mean_move_ms.round()
        ))
    }
}

/// Words with their times locked to the voice.
#[derive(Debug, Clone, PartialEq)]
pub struct Locked {
    pub phrases: Vec<Phrase>,
    pub report: Report,
}

/// Whether `word` starts with a consonant heard as a hiss or pop.
pub fn starts_hissing(word: &str) -> bool {
    let first = pf_lexicon::pronounce(word).and_then(|p| p.phones.first().copied());
    first.is_some_and(|p| {
        matches!(
            p.arpa,
            Arpa::P
                | Arpa::B
                | Arpa::T
                | Arpa::D
                | Arpa::K
                | Arpa::G
                | Arpa::F
                | Arpa::V
                | Arpa::S
                | Arpa::Z
                | Arpa::Sh
                | Arpa::Ch
                | Arpa::Jh
                | Arpa::Th
        )
    })
}

/// The voice's onset strength at frame `i` for a word that starts hissing or not.
fn strength(voice: &VocalTrack, i: usize, hissing: bool) -> f32 {
    let pitched = voice.onset[i].max(voice.rise[i]);
    if hissing {
        voice.consonant[i].max(0.6 * pitched)
    } else {
        pitched.max(0.5 * voice.consonant[i])
    }
}

/// The highest strength within ~15 ms of `ms` (a little slack for jitter).
fn strength_near(voice: &VocalTrack, ms: f64, hissing: bool) -> f32 {
    let Some(i) = voice.frame_at(ms) else {
        return 0.0;
    };
    let reach = (15.0 / voice.hop_ms).round() as usize;
    (i.saturating_sub(reach)..=(i + reach).min(voice.len() - 1))
        .map(|j| strength(voice, j, hissing))
        .fold(0.0, f32::max)
}

/// How well `starts` (ms, with whether each starts hissing) meet the voice's onsets shifted
/// by each lag within ±`range` (ms): the mean onset strength at the shifted starts.
pub fn shift_scores(voice: &VocalTrack, starts: &[(u64, bool)], range: i64) -> Vec<(i64, f32)> {
    (-range / LAG_STEP_MS..=range / LAG_STEP_MS)
        .map(|k| {
            let lag = k * LAG_STEP_MS;
            let sum: f32 = starts
                .iter()
                .map(|&(s, hissing)| strength_near(voice, s as f64 + lag as f64, hissing))
                .sum();
            (lag, sum / starts.len().max(1) as f32)
        })
        .collect()
}

/// The best shift for `starts` within ±`range` (see [`shift_scores`]): the shift and how far
/// its score stands above the others (a robust z-score: from their median, in median absolute
/// deviations).
pub fn best_shift(voice: &VocalTrack, starts: &[(u64, bool)], range: i64) -> (i64, f32) {
    if starts.is_empty() || voice.is_empty() || range < LAG_STEP_MS {
        return (0, 0.0);
    }
    let scores = shift_scores(voice, starts, range);
    // The best, the smaller shift on a tie.
    let Some(&(lag, top)) = scores
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.abs().cmp(&a.0.abs())))
    else {
        return (0, 0.0);
    };
    let mut sorted: Vec<f32> = scores.iter().map(|s| s.1).collect();
    sorted.sort_by(f32::total_cmp);
    let median = sorted[sorted.len() / 2];
    let mut spread: Vec<f32> = sorted.iter().map(|s| (s - median).abs()).collect();
    spread.sort_by(f32::total_cmp);
    let mad = spread[spread.len() / 2] * 1.4826 + 1e-4;
    (lag, (top - median) / mad)
}

/// Whether the word's time came from something that heard or stamped it (not shared out).
fn timed(word: &Word) -> bool {
    word.source != WordSource::Spread
}

fn shift(t: u64, by: i64) -> u64 {
    (t as i64 + by).max(0) as u64
}

/// Locks recognized (or stamped) word times onto the voice (see the module notes). Ends are
/// cut at `end_ms`.
pub fn lock_to_voice(phrases: &[Phrase], voice: &VocalTrack, end_ms: u64) -> Locked {
    let mut out = phrases.to_vec();
    let mut report = Report::default();
    if voice.is_empty() || phrases.is_empty() {
        return Locked { phrases: out, report };
    }
    let starts: Vec<(u64, bool)> = phrases
        .iter()
        .flat_map(|p| &p.words)
        .filter(|w| timed(w))
        .map(|w| (w.start_ms, starts_hissing(&w.text)))
        .collect();
    let (lag, score) = best_shift(voice, &starts, SHIFT_RANGE_MS);
    report.best_shift_ms = lag;
    report.shift_score = score;
    if score >= SHIFT_SURE && lag.abs() >= MIN_SHIFT_MS {
        report.shift_ms = lag;
        report.shifted = true;
        for word in out.iter_mut().flat_map(|p| &mut p.words) {
            word.start_ms = shift(word.start_ms, lag);
            word.end_ms = shift(word.end_ms, lag);
        }
    }
    for phrase in &mut out {
        let starts: Vec<(u64, bool)> = phrase
            .words
            .iter()
            .filter(|w| timed(w))
            .map(|w| (w.start_ms, starts_hissing(&w.text)))
            .collect();
        if starts.len() < LINE_MIN_WORDS {
            continue;
        }
        let (lag, score) = best_shift(voice, &starts, LINE_SHIFT_RANGE_MS);
        if score >= LINE_SURE && lag.abs() >= MIN_SHIFT_MS {
            report.lines_shifted += 1;
            for word in &mut phrase.words {
                word.start_ms = shift(word.start_ms, lag);
                word.end_ms = shift(word.end_ms, lag);
            }
        }
    }
    let locked = place_words(&mut out, voice, end_ms);
    report.locked = locked;
    measure(phrases, &out, voice, &mut report);
    Locked { phrases: out, report }
}

/// The clearest onset for a word starting near `at` (ms), within `reach` of it and between
/// `floor` and `ceiling`: its time and how clear it is (0–1).
fn onset_near(
    voice: &VocalTrack,
    at: f64,
    (floor, ceiling): (f64, f64),
    (reach, near): (f64, f64),
    hissing: bool,
) -> Option<(f64, f32)> {
    let lo = (at - reach).max(floor);
    let hi = (at + reach).min(ceiling);
    if hi <= lo {
        return None;
    }
    let first = voice.frame_at(lo)?;
    let last = voice.frame_at(hi)?;
    // What's usual around here, to tell a clear onset from busy music.
    let around = (250.0 / voice.hop_ms).round() as usize;
    let centre = voice.frame_at(at)?;
    let span = centre.saturating_sub(around)..(centre + around + 1).min(voice.len());
    let mut usual: Vec<f32> = span.map(|i| strength(voice, i, hissing)).collect();
    usual.sort_by(f32::total_cmp);
    let usual = usual[usual.len() / 2];
    let mut peaks: Vec<(f64, f32, f64)> = Vec::new();
    for i in first..=last {
        let v = strength(voice, i, hissing);
        let left = i.checked_sub(1).map_or(0.0, |j| strength(voice, j, hissing));
        let right = if i + 1 < voice.len() {
            strength(voice, i + 1, hissing)
        } else {
            0.0
        };
        let t = voice.time_ms(i);
        if v < left || v < right || t < lo || t > hi {
            continue;
        }
        let above = (v - usual).max(0.0);
        let pull = (-(t - at).powi(2) / (2.0 * near * near)).exp();
        peaks.push((t, (above / 0.35).min(1.0), f64::from(above) * pull));
    }
    let &(t, clear, score) = peaks.iter().max_by(|a, b| a.2.total_cmp(&b.2))?;
    // Clear, and clearly the one: another onset nearly as good makes it a guess.
    let runner_up = peaks
        .iter()
        .filter(|p| (p.0 - t).abs() >= RIVAL_MS)
        .map(|p| p.2)
        .fold(0.0, f64::max);
    let alone = if score > 0.0 { 1.0 - runner_up / score } else { 0.0 };
    Some((t, clear * alone as f32))
}

/// Where the word starting at `start` ends, before `next` (the next word's start, if any):
/// see the module notes. `own_end` is where its source said it ends.
fn word_end(voice: &VocalTrack, start: u64, own_end: u64, next: Option<u64>, end_ms: u64) -> u64 {
    let length = own_end.saturating_sub(start);
    let hold = own_end.max(start + MIN_WORD_MS) + HOLD_MS;
    let limit = match next {
        Some(n) => n.saturating_sub(END_GAP_MS).min(hold),
        None => hold,
    }
    .min(end_ms)
    .max(start + MIN_WORD_MS);
    let from = start + MIN_WORD_MS.max(length * 2 / 5).min(limit - start);
    let (Some(a), Some(b), Some(s)) = (
        voice.frame_at(from as f64),
        voice.frame_at(limit as f64),
        voice.frame_at(start as f64),
    ) else {
        return own_end.clamp(start + 1, limit.max(start + 1));
    };
    // Where the voice stops (and stays stopped a while) first.
    if let Some(&stop) = voice.offsets.iter().find(|&&o| o > from && o <= limit) {
        return stop.max(start + MIN_WORD_MS);
    }
    // Busy music never lets the voice fall silent: the deepest dip before the next word.
    let lowest = (a..=b).min_by(|&i, &j| voice.energy[i].total_cmp(&voice.energy[j]).then(i.cmp(&j)));
    let Some(bottom) = lowest else {
        return limit;
    };
    let loudest = voice.energy[s..=bottom].iter().copied().fold(0.0, f32::max);
    if loudest - voice.energy[bottom] >= DIP {
        // The word ends where the fall into the dip begins.
        let floor = voice.energy[bottom] + DIP_EDGE.max(DIP_SHARE * (loudest - voice.energy[bottom]));
        let mut i = bottom;
        while i > a && voice.energy[i - 1] < floor {
            i -= 1;
        }
        return (voice.time_ms(i).round() as u64).clamp(start + MIN_WORD_MS, limit);
    }
    limit
}

/// A word whose first syllable is an unstressed vowel alone, followed by a consonant
/// ("a·fraid", "a·round", "e·nough"): the rest as spelled ("fraid") and the first syllable's
/// share of the word's length.
pub fn dropped_lead(word: &str) -> Option<(String, f64)> {
    let syllables = pf_lexicon::syllables(word);
    let (first, rest) = syllables.split_first()?;
    let lone = match first.phones.as_slice() {
        [p] => p.is_vowel() && p.stress == 0,
        _ => false,
    };
    let consonant = rest
        .first()
        .and_then(|s| s.phones.first())
        .is_some_and(|p| !p.is_vowel());
    if !lone || !consonant {
        return None;
    }
    let total: f64 = syllables.iter().map(pf_lexicon::Syllable::weight).sum();
    let spelled: String = rest.iter().map(|s| s.text.as_str()).collect();
    let spelled = spelled.trim_matches(|c: char| !c.is_alphanumeric()).to_string();
    (!spelled.is_empty() && total > 0.0).then(|| (spelled, first.weight() / total))
}

/// Moves each word's start onto the clearest onset near it, and its end to where the voice
/// dips; returns how many words were moved onto an onset.
fn place_words(phrases: &mut [Phrase], voice: &VocalTrack, end_ms: u64) -> usize {
    let mut locked = 0;
    // Starts first, in order, each after the one before.
    let count: usize = phrases.iter().map(|p| p.words.len()).sum();
    let original: Vec<(u64, u64)> = phrases
        .iter()
        .flat_map(|p| &p.words)
        .map(|w| (w.start_ms, w.end_ms))
        .collect();
    let mut floor = 0u64;
    let mut k = 0;
    for phrase in phrases.iter_mut() {
        for word in &mut phrase.words {
            let (start, _) = original[k];
            let ceiling = original
                .get(k + 1)
                .map_or(end_ms, |n| n.0.max(start + MIN_WORD_MS))
                .saturating_sub(MIN_WORD_MS);
            let hissing = starts_hissing(word.sung.as_deref().unwrap_or(&word.text));
            let found = onset_near(
                voice,
                start as f64,
                (floor as f64, ceiling as f64),
                (SEARCH_MS, NEAR_MS),
                hissing,
            );
            word.start_ms = match found {
                Some((t, clear)) if clear >= SURE_ONSET => {
                    locked += 1;
                    t.round() as u64
                }
                _ => start,
            }
            .max(floor);
            // The voice starting well after the word, with nothing sung where it was heard to
            // start: its unstressed first vowel wasn't sung ("'fraid").
            if word.sung.is_none()
                && let Some((rest, share)) = dropped_lead(&word.text)
            {
                let lead = (share * original[k].1.saturating_sub(start) as f64) as u64;
                let late = word.start_ms.saturating_sub(start);
                if late >= MIN_WORD_MS.max(lead * 7 / 10) && !voice.is_voiced(start as f64 + 10.0) {
                    word.sung = Some(rest);
                }
            }
            floor = word.start_ms + MIN_WORD_MS;
            k += 1;
        }
    }
    // Then ends, each before the next start.
    let starts: Vec<u64> = phrases
        .iter()
        .flat_map(|p| &p.words)
        .map(|w| w.start_ms)
        .collect();
    let mut k = 0;
    for phrase in phrases.iter_mut() {
        for word in &mut phrase.words {
            let own_end = original[k].1.max(word.start_ms + MIN_WORD_MS);
            let next = starts.get(k + 1).copied();
            word.end_ms = word_end(voice, word.start_ms, own_end, next, end_ms);
            if let Some(n) = next {
                word.end_ms = word.end_ms.min(n).max(word.start_ms + 1);
            }
            k += 1;
        }
        phrase.start_ms = phrase.words.first().map_or(phrase.start_ms, |w| w.start_ms);
        phrase.end_ms = phrase.words.last().map_or(phrase.end_ms, |w| w.end_ms);
    }
    debug_assert_eq!(k, count);
    locked
}

/// Line times only: each line's start moved onto the voice's first clear onset near it, its
/// words onto the onsets inside it, ends where the voice dips (see the module notes).
pub fn spread_onto_voice(phrases: &[Phrase], voice: &VocalTrack, end_ms: u64) -> Locked {
    let mut out = phrases.to_vec();
    let mut report = Report::default();
    if voice.is_empty() || phrases.is_empty() {
        return Locked { phrases: out, report };
    }
    let mut floor = 0u64;
    for k in 0..out.len() {
        let next_line = out.get(k + 1).map_or(end_ms, |p| p.start_ms);
        let phrase = &mut out[k];
        let Some(first) = phrase.words.first() else {
            continue;
        };
        let hissing = starts_hissing(&first.text);
        let start = phrase.start_ms;
        // A published line is often a little early or late: its start onto the clearest onset
        // near it, nearer ones preferred.
        let ceiling = next_line.saturating_sub(MIN_WORD_MS) as f64;
        let moved = match onset_near(
            voice,
            start as f64,
            (floor as f64, ceiling),
            (LINE_SEARCH_MS, LINE_SEARCH_MS / 2.0),
            hissing,
        ) {
            Some((t, clear)) if clear >= SURE_ONSET => t.round() as i64 - start as i64,
            _ => 0,
        };
        if moved != 0 {
            report.lines_shifted += 1;
            for word in &mut phrase.words {
                word.start_ms = shift(word.start_ms, moved).min(next_line.saturating_sub(1));
                word.end_ms = shift(word.end_ms, moved).min(next_line);
            }
        }
        if let Some(last) = phrase.words.last() {
            floor = last.start_ms + MIN_WORD_MS;
        }
    }
    report.locked = place_words(&mut out, voice, end_ms);
    measure(phrases, &out, voice, &mut report);
    Locked { phrases: out, report }
}

fn percentile(sorted: &[f64], share: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f64 * share).round() as usize]
}

/// The numbers in the report, comparing `before` and `after`.
fn measure(before: &[Phrase], after: &[Phrase], voice: &VocalTrack, report: &mut Report) {
    let words = |p: &[Phrase]| -> Vec<Word> { p.iter().flat_map(|p| p.words.clone()).collect() };
    let (a, b) = (words(before), words(after));
    report.words = b.len();
    let mut moves: Vec<f64> = a
        .iter()
        .zip(&b)
        .map(|(x, y)| x.start_ms.abs_diff(y.start_ms) as f64)
        .collect();
    report.earlier = a.iter().zip(&b).filter(|(x, y)| y.start_ms < x.start_ms).count();
    report.later = a.iter().zip(&b).filter(|(x, y)| y.start_ms > x.start_ms).count();
    moves.sort_by(f64::total_cmp);
    report.mean_move_ms = moves.iter().sum::<f64>() / moves.len().max(1) as f64;
    report.median_move_ms = percentile(&moves, 0.5);
    report.p90_move_ms = percentile(&moves, 0.9);
    let onsets: Vec<u64> = {
        let mut all = voice.onsets.clone();
        all.extend(&voice.consonant_onsets);
        all.sort_unstable();
        all
    };
    let distance = |w: &[Word]| -> f64 {
        let d: Vec<f64> = w
            .iter()
            .map(|w| {
                let at = onsets.partition_point(|&o| o < w.start_ms);
                let after = onsets.get(at).map(|o| o - w.start_ms);
                let before = at.checked_sub(1).map(|i| w.start_ms - onsets[i]);
                after.into_iter().chain(before).min().unwrap_or(0) as f64
            })
            .collect();
        d.iter().sum::<f64>() / d.len().max(1) as f64
    };
    report.onset_distance_before_ms = distance(&a);
    report.onset_distance_after_ms = distance(&b);
    let gaps = |p: &[Phrase]| -> f64 {
        let pairs: Vec<bool> = p
            .iter()
            .flat_map(|p| p.words.windows(2).map(|w| w[1].start_ms > w[0].end_ms + 50))
            .collect();
        pairs.iter().filter(|&&g| g).count() as f64 / pairs.len().max(1) as f64
    };
    report.gaps_before = gaps(before);
    report.gaps_after = gaps(after);
    let median_length = |w: &[Word]| {
        let mut l: Vec<f64> = w.iter().map(|w| (w.end_ms - w.start_ms) as f64).collect();
        l.sort_by(f64::total_cmp);
        percentile(&l, 0.5)
    };
    report.median_length_before_ms = median_length(&a);
    report.median_length_after_ms = median_length(&b);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::tracks::lyric_tracks;
    use pf_analysis::vocal_track;

    const RATE: u32 = 16_000;

    /// Made-up sung words and when each is really sung (start, end ms): rests between some.
    const SUNG: [(&str, u64, u64); 8] = [
        ("sparkle", 1_000, 1_380),
        ("ember", 1_600, 1_950),
        ("tinsel", 2_300, 2_650),
        ("glow", 2_800, 3_300),
        ("orbit", 3_700, 4_000),
        ("snowy", 4_200, 4_600),
        ("candle", 5_000, 5_350),
        ("kettle", 5_700, 6_300),
    ];

    fn noise(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    }

    /// A made-up song: a centred "voice" singing `SUNG` (each word a pitched note, starting
    /// with a short hiss for the words starting with a consonant like that), over a chord
    /// panned hard left, another hard right, and noise different on each side. `mono` puts
    /// it all in the middle instead.
    fn song(mono: bool) -> Vec<(f32, f32)> {
        let n = 7 * RATE as usize;
        let mut seed = 5;
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let ms = (i as u64 * 1000) / u64::from(RATE);
                let tone = |hz: f32| (std::f32::consts::TAU * hz * t).sin();
                let left = 0.08 * (tone(330.0) + tone(415.0) + tone(495.0));
                let right = 0.08 * (tone(262.0) + tone(311.0) + tone(392.0));
                let (nl, nr) = (noise(&mut seed) * 0.05, noise(&mut seed) * 0.05);
                let mut voice = 0.0;
                if let Some(&(word, start, end)) = SUNG.iter().find(|w| ms >= w.1 && ms < w.2) {
                    let fade = ((ms - start) as f32 / 10.0)
                        .min((end - ms) as f32 / 10.0)
                        .min(1.0);
                    let hz = 200.0 + 25.0 * (start / 100 % 5) as f32;
                    voice = (1..=6).map(|h| tone(hz * h as f32) / h as f32).sum::<f32>() * 0.25 * fade;
                    if starts_hissing(word) && ms - start < 50 {
                        voice += noise(&mut seed) * 0.4;
                    }
                }
                if mono {
                    let m = voice + (left + right) / 2.0 + nl;
                    (m, m)
                } else {
                    (voice + left + nl, voice + right + nr)
                }
            })
            .collect()
    }

    /// The words as a recognizer might give them: `late` ms late, each a little off
    /// (`jitter`), back to back within a line (lines of four).
    fn heard(late: i64, jitter: &[i64]) -> Vec<Phrase> {
        let starts: Vec<u64> = SUNG
            .iter()
            .zip(jitter.iter().cycle())
            .map(|(w, j)| (w.1 as i64 + late + j) as u64)
            .collect();
        SUNG.chunks(4)
            .enumerate()
            .map(|(l, line)| {
                let words: Vec<Word> = line
                    .iter()
                    .enumerate()
                    .map(|(k, w)| {
                        let i = l * 4 + k;
                        let end = if k + 1 < line.len() {
                            starts[i + 1]
                        } else {
                            (w.2 as i64 + late) as u64
                        };
                        Word {
                            text: w.0.to_string(),
                            start_ms: starts[i],
                            end_ms: end,
                            source: WordSource::Matched,
                            confidence: 0.95,
                            sung: None,
                        }
                    })
                    .collect();
                Phrase {
                    text: line.iter().map(|w| w.0).collect::<Vec<_>>().join(" "),
                    start_ms: words[0].start_ms,
                    end_ms: words[words.len() - 1].end_ms,
                    words,
                }
            })
            .collect()
    }

    const JITTER: [i64; 8] = [60, -80, 20, -40, 80, -20, 40, -60];

    fn words(phrases: &[Phrase]) -> Vec<&Word> {
        phrases.iter().flat_map(|p| &p.words).collect()
    }

    fn check_locked(voice: &VocalTrack) {
        let locked = lock_to_voice(&heard(200, &JITTER), voice, 7_000);
        let report = &locked.report;
        // The whole song, or each line, moved earlier.
        assert!(report.shifted || report.lines_shifted > 0, "{report:?}");
        assert!((-300..=-100).contains(&report.best_shift_ms), "{report:?}");
        for (word, sung) in words(&locked.phrases).iter().zip(SUNG) {
            assert!(
                word.start_ms.abs_diff(sung.1) <= 30,
                "{}: {} for {}",
                word.text,
                word.start_ms,
                sung.1
            );
        }
        assert!(report.onset_distance_after_ms < report.onset_distance_before_ms);
        assert!(
            report
                .sentence()
                .unwrap()
                .starts_with("Word timing locked to the vocals")
        );
    }

    #[test]
    fn late_jittered_words_are_locked_onto_the_voice() {
        check_locked(&vocal_track(song(false), RATE));
    }

    #[test]
    fn a_mono_song_locks_on_its_middle() {
        let voice = vocal_track(song(true), RATE);
        assert!(!voice.stereo);
        check_locked(&voice);
    }

    #[test]
    fn ends_land_where_the_voice_stops_leaving_rests_empty() {
        let voice = vocal_track(song(false), RATE);
        let locked = lock_to_voice(&heard(0, &[0]), &voice, 7_000);
        let words = words(&locked.phrases);
        // Each word ends near where its note stops (not at the next word), and a rest is
        // left between words sung apart.
        for (word, sung) in words.iter().zip(SUNG) {
            assert!(
                word.end_ms.abs_diff(sung.2) <= 60,
                "{}: ends {} for {}",
                word.text,
                word.end_ms,
                sung.2
            );
        }
        assert!(words.windows(2).all(|w| w[0].end_ms < w[1].start_ms));
        assert!(locked.report.gaps_after > locked.report.gaps_before);
        // Syllables and mouth shapes follow the words: nothing in the rests, so a singing
        // face rests there.
        let tracks = lyric_tracks(&locked.phrases, &[], &voice.activity.onsets, 7_000);
        for track in &tracks[1..4] {
            for mark in &track.marks {
                assert!(
                    words
                        .iter()
                        .any(|w| mark.start_ms >= w.start_ms && mark.end_ms <= w.end_ms),
                    "{}: {mark:?}",
                    track.name
                );
            }
        }
        let rest = (SUNG[0].2 + SUNG[1].1) / 2;
        assert!(
            tracks[3]
                .marks
                .iter()
                .all(|m| rest < m.start_ms || rest >= m.end_ms)
        );
    }

    #[test]
    fn a_clear_shift_is_found_and_none_in_noise() {
        let voice = vocal_track(song(false), RATE);
        let starts: Vec<(u64, bool)> = SUNG.iter().map(|w| (w.1 + 150, starts_hissing(w.0))).collect();
        let (lag, score) = best_shift(&voice, &starts, SHIFT_RANGE_MS);
        assert!(lag.abs_diff(-150) <= 20, "{lag}");
        assert!(score >= SHIFT_SURE, "{score}");
        // Wide noise and nothing sung: no shift stands out.
        let mut seed = 9;
        let noisy: Vec<(f32, f32)> = (0..7 * RATE as usize)
            .map(|_| (noise(&mut seed) * 0.3, noise(&mut seed) * 0.3))
            .collect();
        let voice = vocal_track(noisy, RATE);
        let (_, score) = best_shift(&voice, &starts, SHIFT_RANGE_MS);
        assert!(score < SHIFT_SURE, "{score}");
        let locked = lock_to_voice(&heard(150, &[0]), &voice, 7_000);
        assert!(!locked.report.shifted);
    }

    #[test]
    fn words_stay_in_order_and_long_enough() {
        let voice = vocal_track(song(false), RATE);
        // Crowded words, some on top of each other.
        let mut phrases = heard(0, &[0]);
        for (k, w) in phrases[0].words.iter_mut().enumerate() {
            w.start_ms = 1_000 + 20 * k as u64;
            w.end_ms = w.start_ms + 20;
        }
        let locked = lock_to_voice(&phrases, &voice, 7_000);
        let words = words(&locked.phrases);
        assert!(
            words
                .windows(2)
                .all(|w| w[0].start_ms < w[1].start_ms && w[0].end_ms <= w[1].start_ms)
        );
        assert!(
            words.iter().all(|w| w.end_ms >= w.start_ms + MIN_WORD_MS),
            "{words:?}"
        );
        // Lines keep their words: each line spans its own.
        for p in &locked.phrases {
            assert_eq!(
                (p.start_ms, p.end_ms),
                (p.words[0].start_ms, p.words[p.words.len() - 1].end_ms)
            );
        }
    }

    #[test]
    fn line_times_alone_are_spread_onto_the_voice() {
        let voice = vocal_track(song(false), RATE);
        // A published line 250 ms early, its words shared out evenly.
        let line: Vec<Word> = SUNG[..4]
            .iter()
            .enumerate()
            .map(|(k, w)| Word {
                text: w.0.to_string(),
                start_ms: 750 + 600 * k as u64,
                end_ms: 750 + 600 * (k as u64 + 1),
                source: WordSource::Spread,
                confidence: 0.4,
                sung: None,
            })
            .collect();
        let phrase = Phrase {
            text: "line".into(),
            start_ms: 750,
            end_ms: 3_150,
            words: line,
        };
        let locked = spread_onto_voice(&[phrase], &voice, 7_000);
        let words = words(&locked.phrases);
        assert!(words[0].start_ms.abs_diff(SUNG[0].1) <= 30, "{words:?}");
        assert!(locked.report.lines_shifted == 1);
        // Rests are left between words sung apart.
        assert!(words[0].end_ms < words[1].start_ms);
    }

    #[test]
    fn without_a_voice_nothing_moves() {
        let phrases = heard(200, &JITTER);
        let locked = lock_to_voice(&phrases, &VocalTrack::default(), 7_000);
        assert_eq!(locked.phrases, phrases);
        assert_eq!(locked.report.sentence(), None);
    }

    #[test]
    fn an_unstressed_first_vowel_may_go_unsung() {
        assert_eq!(dropped_lead("afraid").map(|d| d.0).as_deref(), Some("fraid"));
        assert_eq!(dropped_lead("about").map(|d| d.0).as_deref(), Some("bout"));
        assert_eq!(dropped_lead("paper"), None);
        assert_eq!(dropped_lead("glow"), None);
    }
}
