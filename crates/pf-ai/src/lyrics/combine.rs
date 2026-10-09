//! Putting the words together: published text and timing (LRC lines, sometimes with word stamps),
//! the recognizer's words, or both.
//!
//! - **Published lines and recognized words**: the recognizer's words are lined up with the
//!   published ones in order (Needleman–Wunsch on normalised words, a match only allowed near
//!   the published line's time). The published spelling wins and the recognizer's timing wins;
//!   a published word with nothing heard for it shares the time between its neighbours.
//! - **Published lines only**: each line's words share the line's time by syllables (or keep
//!   their own enhanced-LRC stamps), nudged onto nearby vocal onsets.
//! - **Plain published text and recognized words**: lined up the same way, without line times.
//! - **Recognized words only**: as heard, in the lines the recognizer heard.
//!
//! Every word says where its text and time came from ([`WordSource`]) and how sure that is.

use super::lrc::LrcLine;
use super::transcribe::{Heard, HeardWord};
use serde::Serialize;

/// Where a word's time came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WordSource {
    /// The published lyrics' own word stamp.
    Published,
    /// The published word, at the time the recognizer heard it.
    Matched,
    /// The recognizer's word and time (no published text).
    Heard,
    /// A share of its line's time by syllables (or of the gap between heard neighbours).
    Spread,
}

/// One sung word.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub source: WordSource,
    /// 0–1: how sure its time is.
    pub confidence: f32,
}

/// One sung line and its words.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Phrase {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub words: Vec<Word>,
}

/// How sure each kind of time is.
const STAMPED: f32 = 0.9;
const MATCHED: f32 = 0.95;
const MATCHED_ALIKE: f32 = 0.8;
const MATCHED_UNLIKE: f32 = 0.5;
const HEARD: f32 = 0.7;
const SPREAD: f32 = 0.4;
const FILLED: f32 = 0.3;

/// How far (ms) a heard word may be from its published line's time and still be matched to it.
const LINE_SLACK_MS: u64 = 2_000;
/// How far (ms) a word's start moves to meet a vocal onset.
const NUDGE_MS: u64 = 120;
/// Time per syllable for words before the first or after the last one heard.
const EDGE_SYLLABLE_MS: u64 = 350;
/// Words heard closer together than this belong to one line (without the recognizer's lines).
const LINE_GAP_MS: u64 = 1_000;
/// The most words in a line made from heard words alone.
const MAX_LINE_WORDS: usize = 12;
/// The most alignment cells worked out (published words × heard words).
const MAX_CELLS: usize = 40_000_000;

/// A word for comparing: lowercase letters and digits only ("Shinin'" is "shinin").
pub fn normalize(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// About how many syllables a word has: its vowel groups (a final silent "e" left out), each
/// digit one, at least one. A word with letters beyond a–z is split as
/// [`pf_lexicon::syllables`] splits it.
pub fn syllables(word: &str) -> u32 {
    let chars: Vec<char> = word.chars().collect();
    if pf_lexicon::foreign::has_foreign_letters(&chars) {
        return (pf_lexicon::syllables(word).len() as u32).max(1);
    }
    let word = normalize(word);
    let digits = word.chars().filter(char::is_ascii_digit).count() as u32;
    let letters: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
    let vowel = |c: char| "aeiouyàáâäèéêëìíîïòóôöùúûü".contains(c);
    let mut groups = 0u32;
    let mut previous = false;
    for &c in &letters {
        let v = vowel(c);
        if v && !previous {
            groups += 1;
        }
        previous = v;
    }
    let n = letters.len();
    let silent_e = n > 2 && letters[n - 1] == 'e' && !vowel(letters[n - 2]) && letters[n - 2] != 'l';
    if silent_e && groups > 1 {
        groups -= 1;
    }
    (groups + digits).max(1)
}

/// The words of a line: split on spaces, with punctuation standing alone ("-", "…") kept with
/// the word before it.
pub fn split_words(text: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut leading = String::new();
    for token in text.split_whitespace() {
        if !token.chars().any(char::is_alphanumeric) {
            match words.last_mut() {
                Some(last) => {
                    last.push(' ');
                    last.push_str(token);
                }
                None => leading.push_str(token),
            }
        } else if leading.is_empty() {
            words.push(token.to_string());
        } else {
            words.push(format!("{} {token}", std::mem::take(&mut leading)));
        }
    }
    words
}

/// How alike two normalised words are, 0–1: the same is 1; one starting with the other (a word
/// heard as two, "snow" for "snowflakes") 0.6; else by edit distance.
pub fn likeness(a: &str, b: &str) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let by_edits = {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        let mut row: Vec<usize> = (0..=b.len()).collect();
        for i in 1..=a.len() {
            let mut diagonal = row[0];
            row[0] = i;
            for j in 1..=b.len() {
                let above = row[j];
                row[j] = (above + 1)
                    .min(row[j - 1] + 1)
                    .min(diagonal + usize::from(a[i - 1] != b[j - 1]));
                diagonal = above;
            }
        }
        1.0 - row[b.len()] as f32 / a.len().max(b.len()) as f32
    };
    let prefix = if part_of(a, b) { 0.6 } else { 0.0 };
    by_edits.max(prefix)
}

/// Whether one word starts with the other (at least 3 letters of it).
fn part_of(a: &str, b: &str) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short.chars().count() >= 3 && long.starts_with(short) && short != long
}

/// Lines up `published` words with `heard` words in order (Needleman–Wunsch: alike words score,
/// unlike ones and skipped ones cost), only pairing `i` with `j` where `allowed(i, j)`. For each
/// published word: the heard word it lines up with and how alike they are. Extra heard words
/// are left out; published words with nothing heard get `None`.
pub fn align(
    published: &[String],
    heard: &[String],
    allowed: &dyn Fn(usize, usize) -> bool,
) -> Vec<Option<(usize, f32)>> {
    let (n, m) = (published.len(), heard.len());
    let mut out = vec![None; n];
    if n == 0 || m == 0 || n.saturating_mul(m) > MAX_CELLS {
        return out;
    }
    const GAP: i32 = -1;
    let pair = |i: usize, j: usize| -> Option<(i32, f32)> {
        if !allowed(i, j) {
            return None;
        }
        let alike = likeness(&published[i], &heard[j]);
        // A word heard in parts goes with its first part.
        let score = if alike >= 0.8 {
            3
        } else if part_of(&published[i], &heard[j]) {
            2
        } else if alike >= 0.5 {
            1
        } else {
            -1
        };
        Some((score, alike))
    };
    // Directions: 0 diagonal, 1 up (published word skipped), 2 left (heard word skipped).
    let mut from = vec![0u8; (n + 1) * (m + 1)];
    let mut previous: Vec<i32> = (0..=m as i32).map(|j| j * GAP).collect();
    for i in 1..=n {
        let mut row = vec![0i32; m + 1];
        row[0] = i as i32 * GAP;
        from[i * (m + 1)] = 1;
        for j in 1..=m {
            let up = previous[j] + GAP;
            let left = row[j - 1] + GAP;
            let diagonal = pair(i - 1, j - 1).map(|(s, _)| previous[j - 1] + s);
            let (best, way) = match diagonal {
                Some(d) if d >= up && d >= left => (d, 0),
                _ if up >= left => (up, 1),
                _ => (left, 2),
            };
            row[j] = best;
            from[i * (m + 1) + j] = way;
        }
        previous = row;
    }
    from[1..=m].fill(2);
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        match from[i * (m + 1) + j] {
            0 => {
                let alike = likeness(&published[i - 1], &heard[j - 1]);
                out[i - 1] = Some((j - 1, alike));
                i -= 1;
                j -= 1;
            }
            1 => i -= 1,
            _ => j -= 1,
        }
    }
    out
}

/// `count` spans sharing `from..to` by `weights`, each at least 1 ms (past `to` when there isn't
/// room).
fn share(from: u64, to: u64, weights: &[u32]) -> Vec<(u64, u64)> {
    let total: u64 = weights.iter().map(|&w| u64::from(w.max(1))).sum::<u64>().max(1);
    let length = to.saturating_sub(from);
    let mut spans = Vec::with_capacity(weights.len());
    let mut start = from;
    let mut sum = 0;
    for &w in weights {
        sum += u64::from(w.max(1));
        let end = (from + length * sum / total).max(start + 1);
        spans.push((start, end));
        start = end;
    }
    spans
}

/// Moves each span's start onto a vocal onset within reach, keeping them in order and each at
/// least 1 ms long; ends follow the next start.
fn nudge(spans: &mut [(u64, u64)], onsets: &[u64]) {
    let count = spans.len();
    for k in 0..count {
        let (start, end) = spans[k];
        let floor = if k == 0 {
            start.saturating_sub(NUDGE_MS)
        } else {
            spans[k - 1].0 + 1
        };
        let ceiling = if k + 1 < count { spans[k + 1].0 } else { end }.saturating_sub(1);
        let at = onsets.partition_point(|&o| o < start.saturating_sub(NUDGE_MS));
        let best = onsets[at..]
            .iter()
            .take_while(|&&o| o <= start + NUDGE_MS)
            .filter(|&&o| o >= floor && o <= ceiling)
            .min_by_key(|&&o| o.abs_diff(start));
        if let Some(&onset) = best {
            spans[k].0 = onset;
            if k > 0 {
                spans[k - 1].1 = onset;
            }
        }
    }
}

/// A published line and the time it's sung in.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedLine {
    pub start_ms: u64,
    pub end_ms: u64,
    pub words: Vec<String>,
    /// Enhanced-LRC word stamps, when the line has them (one per word).
    pub stamped: Vec<u64>,
    pub stamped_end: Option<u64>,
}

/// The longest a line is taken to last: a line before a long instrumental stretch (with no
/// end marked) would otherwise spread its words across the whole stretch.
fn longest_line_ms(words: &[String]) -> u64 {
    let syllables: u64 = words.iter().map(|w| u64::from(syllables(w))).sum();
    1_500 + 900 * syllables
}

/// LRC lines with their times: each runs to the next line (or an empty line marking its end),
/// no longer than its words could take, and not past `duration_ms`.
pub fn timed_lines(lines: &[LrcLine], duration_ms: u64) -> Vec<TimedLine> {
    let mut out = Vec::new();
    for (k, line) in lines.iter().enumerate() {
        let words = split_words(&line.text);
        if words.is_empty() || line.start_ms >= duration_ms {
            continue;
        }
        let next = lines.get(k + 1).map_or(duration_ms, |l| l.start_ms);
        let end = next
            .min(line.start_ms + longest_line_ms(&words))
            .min(duration_ms)
            .max(line.start_ms + 1);
        let stamped: Vec<u64> = if line.words.len() == words.len() {
            line.words.iter().map(|(at, _)| *at).collect()
        } else {
            Vec::new()
        };
        out.push(TimedLine {
            start_ms: line.start_ms,
            end_ms: end,
            words,
            stamped,
            stamped_end: line.words_end_ms,
        });
    }
    out
}

fn phrase(words: Vec<Word>) -> Option<Phrase> {
    let start_ms = words.first()?.start_ms;
    let end_ms = words.last()?.end_ms;
    let text = words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    Some(Phrase {
        text,
        start_ms,
        end_ms,
        words,
    })
}

/// Makes word times safe for timing marks: in order, each at least 1 ms, none overlapping the
/// next, phrases likewise.
fn tidy(phrases: &mut Vec<Phrase>) {
    let mut floor = 0u64;
    for phrase in phrases.iter_mut() {
        for word in &mut phrase.words {
            word.start_ms = word.start_ms.max(floor);
            word.end_ms = word.end_ms.max(word.start_ms + 1);
            floor = word.start_ms + 1;
        }
        let count = phrase.words.len();
        for k in 0..count.saturating_sub(1) {
            let next = phrase.words[k + 1].start_ms;
            phrase.words[k].end_ms = phrase.words[k].end_ms.min(next).max(phrase.words[k].start_ms + 1);
        }
        if let Some(last) = phrase.words.last() {
            floor = floor.max(last.end_ms);
        }
        phrase.start_ms = phrase.words.first().map_or(0, |w| w.start_ms);
        phrase.end_ms = phrase.words.last().map_or(0, |w| w.end_ms);
    }
    // A line that starts before the last one ends cuts the last one short.
    for k in 1..phrases.len() {
        let next = phrases[k].start_ms;
        let before = &mut phrases[k - 1];
        if before.end_ms > next {
            before.end_ms = next.max(before.start_ms + 1);
            if let Some(last) = before.words.last_mut() {
                last.end_ms = last.end_ms.min(before.end_ms).max(last.start_ms + 1);
            }
        }
    }
    phrases.retain(|p| !p.words.is_empty());
}

/// Published lines alone: stamped words keep their stamps; the rest share their line's time by
/// syllables, nudged onto `onsets`.
pub fn from_lines(lines: &[TimedLine], onsets: &[u64]) -> Vec<Phrase> {
    let mut phrases: Vec<Phrase> = lines
        .iter()
        .filter_map(|line| {
            let words: Vec<Word> = if !line.stamped.is_empty() {
                line.words
                    .iter()
                    .zip(&line.stamped)
                    .enumerate()
                    .map(|(k, (text, &start))| {
                        let end = line
                            .stamped
                            .get(k + 1)
                            .copied()
                            .or(line.stamped_end)
                            .unwrap_or(line.end_ms);
                        Word {
                            text: text.clone(),
                            start_ms: start,
                            end_ms: end,
                            source: WordSource::Published,
                            confidence: STAMPED,
                        }
                    })
                    .collect()
            } else {
                let weights: Vec<u32> = line.words.iter().map(|w| syllables(w)).collect();
                let mut spans = share(line.start_ms, line.end_ms, &weights);
                nudge(&mut spans, onsets);
                line.words
                    .iter()
                    .zip(spans)
                    .map(|(text, (start, end))| Word {
                        text: text.clone(),
                        start_ms: start,
                        end_ms: end,
                        source: WordSource::Spread,
                        confidence: SPREAD,
                    })
                    .collect()
            };
            phrase(words)
        })
        .collect();
    tidy(&mut phrases);
    phrases
}

/// A published word and the time found for it so far.
struct Slot {
    text: String,
    line: usize,
    time: Option<(u64, u64, WordSource, f32)>,
}

/// Times for the words with none: each run between timed words shares the gap by syllables,
/// kept within `window(line)` when the run is all in one line whose time is known; at the very
/// start or end without one, it takes [`EDGE_SYLLABLE_MS`] a syllable.
fn fill(slots: &mut [Slot], window: &dyn Fn(usize) -> Option<(u64, u64)>, duration_ms: u64) {
    let mut k = 0;
    while k < slots.len() {
        if slots[k].time.is_some() {
            k += 1;
            continue;
        }
        let run_end = (k..slots.len())
            .find(|&e| slots[e].time.is_some())
            .unwrap_or(slots.len());
        let weights: Vec<u32> = slots[k..run_end].iter().map(|s| syllables(&s.text)).collect();
        let edge: u64 = weights.iter().map(|&w| u64::from(w) * EDGE_SYLLABLE_MS).sum();
        let before = k.checked_sub(1).and_then(|p| slots[p].time.map(|t| t.1));
        let after = slots.get(run_end).and_then(|s| s.time.map(|t| t.0));
        let one_line = slots[k..run_end].iter().all(|s| s.line == slots[k].line);
        let line_window = if one_line { window(slots[k].line) } else { None };
        let (mut from, mut to) = match (before, after, line_window) {
            (Some(b), Some(a), _) => (b, a),
            (Some(b), None, Some((_, line_end))) => (b, line_end),
            (None, Some(a), Some((line_start, _))) => (line_start, a),
            (None, None, Some(line)) => line,
            (Some(b), None, None) => (b, (b + edge).min(duration_ms)),
            (None, Some(a), None) => (a.saturating_sub(edge), a),
            (None, None, None) => (0, edge.min(duration_ms)),
        };
        // Inside the line's own time, where the line is known.
        if let Some((line_start, line_end)) = line_window {
            let lo = from.max(line_start);
            let hi = to.min(line_end);
            if hi > lo {
                (from, to) = (lo, hi);
            }
        }
        for (slot, (start, end)) in slots[k..run_end].iter_mut().zip(share(from, to, &weights)) {
            slot.time = Some((start, end, WordSource::Spread, FILLED));
        }
        k = run_end;
    }
}

fn heard_texts(heard: &[HeardWord]) -> Vec<String> {
    heard.iter().map(|w| normalize(&w.text)).collect()
}

/// Groups timed slots back into their lines.
fn phrases_from_slots(slots: Vec<Slot>) -> Vec<Phrase> {
    let mut lines: Vec<Vec<Word>> = Vec::new();
    let mut current = usize::MAX;
    for slot in slots {
        let Some((start_ms, end_ms, source, confidence)) = slot.time else {
            continue;
        };
        if slot.line != current {
            lines.push(Vec::new());
            current = slot.line;
        }
        if let Some(line) = lines.last_mut() {
            line.push(Word {
                text: slot.text,
                start_ms,
                end_ms,
                source,
                confidence,
            });
        }
    }
    let mut phrases: Vec<Phrase> = lines.into_iter().filter_map(phrase).collect();
    tidy(&mut phrases);
    phrases
}

fn matched(alike: f32) -> f32 {
    if alike >= 0.999 {
        MATCHED
    } else if alike >= 0.5 {
        MATCHED_ALIKE
    } else {
        MATCHED_UNLIKE
    }
}

/// Published lines with the recognizer's timing (see the module notes).
pub fn from_lines_and_heard(lines: &[TimedLine], heard: &Heard, duration_ms: u64) -> Vec<Phrase> {
    let mut slots: Vec<Slot> = lines
        .iter()
        .enumerate()
        .flat_map(|(l, line)| {
            line.words.iter().map(move |w| Slot {
                text: w.clone(),
                line: l,
                time: None,
            })
        })
        .collect();
    let published: Vec<String> = slots.iter().map(|s| normalize(&s.text)).collect();
    let words = &heard.words;
    let allowed = |i: usize, j: usize| {
        let line = &lines[slots[i].line];
        let at = words[j].start_ms;
        at + LINE_SLACK_MS >= line.start_ms && at <= line.end_ms + LINE_SLACK_MS
    };
    let pairs = align(&published, &heard_texts(words), &allowed);
    for (slot, pair) in slots.iter_mut().zip(pairs) {
        if let Some((j, alike)) = pair {
            let w = &words[j];
            slot.time = Some((w.start_ms, w.end_ms, WordSource::Matched, matched(alike)));
        }
    }
    let window = |l: usize| lines.get(l).map(|line| (line.start_ms, line.end_ms));
    fill(&mut slots, &window, duration_ms);
    phrases_from_slots(slots)
}

/// Plain published lines with the recognizer's timing.
pub fn from_plain_and_heard(lines: &[String], heard: &Heard, duration_ms: u64) -> Vec<Phrase> {
    let mut slots: Vec<Slot> = lines
        .iter()
        .enumerate()
        .flat_map(|(l, line)| {
            split_words(line).into_iter().map(move |w| Slot {
                text: w,
                line: l,
                time: None,
            })
        })
        .collect();
    let published: Vec<String> = slots.iter().map(|s| normalize(&s.text)).collect();
    let pairs = align(&published, &heard_texts(&heard.words), &|_, _| true);
    // Without a single word heard alike, these aren't the words that were sung.
    if !pairs.iter().flatten().any(|&(_, alike)| alike >= 0.5) {
        return Vec::new();
    }
    for (slot, pair) in slots.iter_mut().zip(pairs) {
        if let Some((j, alike)) = pair {
            let w = &heard.words[j];
            slot.time = Some((w.start_ms, w.end_ms, WordSource::Matched, matched(alike)));
        }
    }
    fill(&mut slots, &|_| None, duration_ms);
    phrases_from_slots(slots)
}

/// The recognizer's words as they are, in the lines it heard (or split at pauses).
pub fn from_heard(heard: &Heard) -> Vec<Phrase> {
    let mut lines: Vec<Vec<Word>> = Vec::new();
    let mut current: Option<usize> = None;
    let mut last_end = 0u64;
    for w in &heard.words {
        let segment = heard
            .lines
            .iter()
            .position(|&(s, e)| w.start_ms >= s && w.start_ms < e);
        let new_line = match (segment, current) {
            (Some(s), Some(c)) => s != c,
            (Some(_), None) => true,
            (None, _) => {
                lines.is_empty()
                    || w.start_ms.saturating_sub(last_end) > LINE_GAP_MS
                    || lines.last().is_some_and(|l| l.len() >= MAX_LINE_WORDS)
            }
        };
        if new_line || lines.is_empty() {
            lines.push(Vec::new());
        }
        current = segment;
        last_end = w.end_ms;
        if let Some(line) = lines.last_mut() {
            line.push(Word {
                text: w.text.clone(),
                start_ms: w.start_ms,
                end_ms: w.end_ms,
                source: WordSource::Heard,
                confidence: HEARD,
            });
        }
    }
    let mut phrases: Vec<Phrase> = lines.into_iter().filter_map(phrase).collect();
    tidy(&mut phrases);
    phrases
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::lrc::parse_lrc;

    fn heard(words: &[(&str, u64, u64)]) -> Heard {
        Heard {
            words: words
                .iter()
                .map(|&(t, s, e)| HeardWord {
                    text: t.into(),
                    start_ms: s,
                    end_ms: e,
                })
                .collect(),
            lines: Vec::new(),
        }
    }

    fn texts(phrases: &[Phrase]) -> Vec<Vec<(&str, u64, u64, WordSource)>> {
        phrases
            .iter()
            .map(|p| {
                p.words
                    .iter()
                    .map(|w| (w.text.as_str(), w.start_ms, w.end_ms, w.source))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn syllables_are_counted_well_enough() {
        for (word, n) in [
            ("glow", 1),
            ("lanterns", 2),
            ("Paper", 2),
            ("shine", 1),
            ("rooftops", 2),
            ("candle", 2),
            ("celebration", 4),
            ("Lanternlight!", 3),
            ("rock'n'roll", 2),
            ("1999", 4),
            ("hmm", 1),
            ("молоко", 3),
            ("привет!", 2),
            ("corazón", 3),
        ] {
            assert_eq!(syllables(word), n, "{word}");
        }
    }

    #[test]
    fn words_share_a_line_by_syllables_and_meet_onsets() {
        let lines = timed_lines(&parse_lrc("[00:10.00]Paper lanterns glow\n[00:13.00]\n"), 60_000);
        assert_eq!(lines[0].end_ms, 13_000);
        // 2 + 2 + 1 syllables over 3 s: 1.2 s, 1.2 s, 0.6 s.
        let phrases = from_lines(&lines, &[]);
        assert_eq!(
            texts(&phrases),
            [vec![
                ("Paper", 10_000, 11_200, WordSource::Spread),
                ("lanterns", 11_200, 12_400, WordSource::Spread),
                ("glow", 12_400, 13_000, WordSource::Spread),
            ]]
        );
        assert_eq!(phrases[0].words[0].confidence, SPREAD);
        // An onset 80 ms after "glow" would start pulls it there; one 300 ms off doesn't.
        let phrases = from_lines(&lines, &[11_500, 12_480]);
        assert_eq!(phrases[0].words[1].end_ms, 12_480);
        assert_eq!(phrases[0].words[2].start_ms, 12_480);
        assert_eq!(phrases[0].words[1].start_ms, 11_200);
    }

    #[test]
    fn a_line_before_a_long_gap_is_not_stretched_across_it() {
        let lines = timed_lines(&parse_lrc("[00:10.00]Glow\n[00:40.00]Shine"), 60_000);
        assert_eq!(lines[0].end_ms, 10_000 + longest_line_ms(&["Glow".to_string()]));
        assert_eq!(lines[1].end_ms, 40_000 + longest_line_ms(&["Shine".to_string()]));
    }

    #[test]
    fn enhanced_stamps_are_kept() {
        let lines = timed_lines(
            &parse_lrc("[00:05.00]<00:05.00>Paper <00:05.40>lanterns <00:06.10>glowing <00:07.00>"),
            60_000,
        );
        let phrases = from_lines(&lines, &[5_100]);
        assert_eq!(
            texts(&phrases),
            [vec![
                ("Paper", 5_000, 5_400, WordSource::Published),
                ("lanterns", 5_400, 6_100, WordSource::Published),
                ("glowing", 6_100, 7_000, WordSource::Published),
            ]]
        );
    }

    #[test]
    fn alignment_handles_misheard_missing_and_extra_words() {
        let published: Vec<String> = ["paper", "lanterns", "glowing", "on", "snowy", "rooftops"]
            .map(String::from)
            .to_vec();
        // "lantern" misheard, "on" missed, "oh" and "yeah" extra.
        let heard: Vec<String> = ["oh", "paper", "lantern", "glowing", "snowy", "yeah", "rooftops"]
            .map(String::from)
            .to_vec();
        let pairs = align(&published, &heard, &|_, _| true);
        let found: Vec<Option<usize>> = pairs.iter().map(|p| p.map(|(j, _)| j)).collect();
        assert_eq!(found, [Some(1), Some(2), Some(3), None, Some(4), Some(6)]);
        assert_eq!(pairs[0].unwrap().1, 1.0);
        assert!(pairs[1].unwrap().1 >= 0.8);
        // A word heard as two pairs with its first part.
        let pairs = align(
            &["ring".into(), "snowflakes".into()],
            &["ring".into(), "snow".into(), "flakes".into()],
            &|_, _| true,
        );
        assert_eq!(pairs[1].map(|p| p.0), Some(1));
        // Only where allowed.
        let pairs = align(&["glow".into()], &["glow".into()], &|_, _| false);
        assert_eq!(pairs, [None]);
    }

    #[test]
    fn published_spelling_and_heard_timing_win_within_the_line() {
        let lrc = "[00:10.00]Hang ya bells up?\n[00:13.00]Lanternlight!\n[00:16.00]\n[00:30.00]Paper lanterns glowing\n[00:34.00]";
        let lines = timed_lines(&parse_lrc(lrc), 60_000);
        let heard = heard(&[
            ("hang", 10_100, 10_300),
            ("you", 10_300, 10_500),
            ("bells", 10_500, 11_000),
            ("up", 11_000, 11_600),
            ("lanternlight", 13_200, 14_500),
            // Heard again much later: no published line there, so it's left out.
            ("lanternlight", 50_000, 51_000),
            ("paper", 30_500, 31_000),
            ("glowing", 32_000, 33_000),
        ]);
        let phrases = from_lines_and_heard(&lines, &heard, 60_000);
        assert_eq!(
            texts(&phrases),
            [
                vec![
                    ("Hang", 10_100, 10_300, WordSource::Matched),
                    ("ya", 10_300, 10_500, WordSource::Matched),
                    ("bells", 10_500, 11_000, WordSource::Matched),
                    ("up?", 11_000, 11_600, WordSource::Matched),
                ],
                vec![("Lanternlight!", 13_200, 14_500, WordSource::Matched)],
                vec![
                    ("Paper", 30_500, 31_000, WordSource::Matched),
                    // Not heard: the gap between its neighbours.
                    ("lanterns", 31_000, 32_000, WordSource::Spread),
                    ("glowing", 32_000, 33_000, WordSource::Matched),
                ],
            ]
        );
        assert_eq!(phrases[0].text, "Hang ya bells up?");
        assert_eq!(phrases[0].words[0].confidence, MATCHED);
        assert_eq!(phrases[0].words[1].confidence, MATCHED_UNLIKE);
        assert_eq!(phrases[2].words[1].confidence, FILLED);
    }

    #[test]
    fn a_line_with_nothing_heard_keeps_its_own_time() {
        let lines = timed_lines(
            &parse_lrc("[00:10.00]Paper lanterns\n[00:12.00]Snowy rooftops\n[00:14.00]"),
            60_000,
        );
        let heard = heard(&[("paper", 10_200, 10_800), ("lanterns", 10_800, 11_700)]);
        let phrases = from_lines_and_heard(&lines, &heard, 60_000);
        // Inside its own line's time, not stretched back to the line before.
        assert_eq!(
            texts(&phrases)[1],
            [
                ("Snowy", 12_000, 13_000, WordSource::Spread),
                ("rooftops", 13_000, 14_000, WordSource::Spread)
            ]
        );
    }

    #[test]
    fn plain_lyrics_take_heard_timing_and_heard_words_stand_alone() {
        let lines = vec!["Paper lanterns glowing".to_string(), "Snowy rooftops".to_string()];
        let h = heard(&[
            ("paper", 1_000, 1_500),
            ("lanterns", 1_500, 2_000),
            ("glowing", 2_000, 2_600),
            ("snowy", 4_000, 4_400),
        ]);
        let phrases = from_plain_and_heard(&lines, &h, 60_000);
        assert_eq!(phrases.len(), 2);
        assert_eq!(phrases[1].words[0].start_ms, 4_000);
        // "rooftops" (2 syllables) after the last word heard.
        assert_eq!(
            (phrases[1].words[1].start_ms, phrases[1].words[1].end_ms),
            (4_400, 4_400 + 2 * EDGE_SYLLABLE_MS)
        );
        // Nothing heard matches: no timing to go by.
        assert!(from_plain_and_heard(&lines, &heard(&[("zzz", 0, 10)]), 60_000).is_empty());

        let mut alone = h.clone();
        alone.words.push(HeardWord {
            text: "shine".into(),
            start_ms: 4_400,
            end_ms: 4_900,
        });
        let phrases = from_heard(&alone);
        let lines: Vec<&str> = phrases.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(lines, ["paper lanterns glowing", "snowy shine"]);
        assert!(
            phrases
                .iter()
                .flat_map(|p| &p.words)
                .all(|w| w.source == WordSource::Heard)
        );
        // The recognizer's own lines win over pauses.
        alone.lines = vec![(1_000, 2_000), (2_000, 5_000)];
        let lines: Vec<String> = from_heard(&alone).into_iter().map(|p| p.text).collect();
        assert_eq!(lines, ["paper lanterns", "glowing snowy shine"]);
    }

    #[test]
    fn words_never_overlap() {
        let h = heard(&[
            ("glow", 1_000, 2_000),
            ("shine", 1_500, 2_500),
            ("bright", 1_500, 1_500),
        ]);
        let phrases = from_heard(&h);
        let words = &phrases[0].words;
        assert!(words.windows(2).all(|w| w[0].end_ms <= w[1].start_ms));
        assert!(words.iter().all(|w| w.end_ms > w.start_ms));
    }
}
