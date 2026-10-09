//! Lines of words aligned to a song's [`Emission`]: each line looked for in its own stretch
//! ([`crate::windows`]), its letters placed by forced alignment ([`crate::ctc`]), and read back
//! as letter, word, and line times.
//!
//! A word starts when its first letter is heard and ends after its last; its confidence is its
//! letters' mean. A line's confidence is its words' mean. The model hears a letter for a step or
//! two and then blanks, so a held vowel shows as its first steps only: a word's end is where its
//! last letter was heard, not where the voice lets go of it (see [`crate::sounds`]).

use crate::ctc::{self, Emission, FRAME_MS};
use crate::text::Transcript;
use crate::windows::{self, Line, MAX_WINDOW_MS, PAD_MS};

/// When one letter was heard (ms), and how sure the model was (0–1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharTime {
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: f32,
}

/// When one word was heard: from its first letter to its last.
#[derive(Debug, Clone, PartialEq)]
pub struct WordTime {
    pub start_ms: u64,
    pub end_ms: u64,
    /// Its letters' mean confidence (0–1).
    pub confidence: f32,
    /// Each of its characters' times, `None` for what the model doesn't hear ("!", "4").
    pub chars: Vec<Option<CharTime>>,
}

/// One line's words, each `None` when it couldn't be placed (no letters, or the line's
/// stretch was too short for them).
#[derive(Debug, Clone, PartialEq)]
pub struct LineTime {
    pub words: Vec<Option<WordTime>>,
    /// Its placed words' mean confidence, 0 when none were placed.
    pub confidence: f32,
}

fn frame_ms(frame: usize) -> u64 {
    (frame as f64 * FRAME_MS).round() as u64
}

fn ms_frame(ms: u64) -> usize {
    (ms as f64 / FRAME_MS).round() as usize
}

/// Aligns `lines` (in order, each with its rough time) to `emission`, each a second either side
/// of where it's thought to be.
pub fn align_lines(emission: &Emission, lines: &[Line]) -> Vec<LineTime> {
    let song_ms = frame_ms(emission.frames());
    align_lines_within(emission, lines, PAD_MS, MAX_WINDOW_MS, song_ms)
}

/// [`align_lines`] with the padding and longest stretch given; `max_ms` at least the song's
/// length aligns the words of the whole song as one, for lyrics with no times at all.
pub fn align_lines_within(
    emission: &Emission,
    lines: &[Line],
    pad_ms: u64,
    max_ms: u64,
    song_ms: u64,
) -> Vec<LineTime> {
    let mut out: Vec<LineTime> = lines
        .iter()
        .map(|l| LineTime {
            words: vec![None; l.words.len()],
            confidence: 0.0,
        })
        .collect();
    for window in windows::windows(lines, pad_ms, max_ms, song_ms) {
        let in_window = &lines[window.lines.clone()];
        // All the window's words as one transcript, remembering whose they are.
        let words: Vec<&str> = in_window
            .iter()
            .flat_map(|l| l.words.iter().map(String::as_str))
            .collect();
        let owners: Vec<(usize, usize)> = window
            .lines
            .clone()
            .flat_map(|i| (0..lines[i].words.len()).map(move |w| (i, w)))
            .collect();
        let transcript = Transcript::new(&words);
        let frames = ms_frame(window.start_ms)..ms_frame(window.end_ms);
        let Some(spans) = ctc::force_align(emission, frames, &transcript.tokens) else {
            continue;
        };
        let mut placed: Vec<Option<WordTime>> = vec![None; words.len()];
        for (span, owner) in spans.iter().zip(&transcript.owners) {
            let Some((w, c)) = *owner else {
                continue;
            };
            let char_time = CharTime {
                start_ms: frame_ms(span.start),
                end_ms: frame_ms(span.end),
                confidence: span.score,
            };
            let word = placed[w].get_or_insert_with(|| WordTime {
                start_ms: char_time.start_ms,
                end_ms: char_time.end_ms,
                confidence: 0.0,
                chars: vec![None; words[w].chars().count()],
            });
            word.end_ms = char_time.end_ms;
            word.chars[c] = Some(char_time);
        }
        for (word, &(line, w)) in placed.into_iter().zip(&owners) {
            out[line].words[w] = word.map(|mut word| {
                let heard: Vec<f32> = word.chars.iter().flatten().map(|c| c.confidence).collect();
                word.confidence = heard.iter().sum::<f32>() / heard.len().max(1) as f32;
                word
            });
        }
    }
    for line in &mut out {
        let placed: Vec<f32> = line.words.iter().flatten().map(|w| w.confidence).collect();
        line.confidence = placed.iter().sum::<f32>() / placed.len().max(1) as f32;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::{self, BLANK, WORD_GAP};

    /// An emission over `path` (a token each 20 ms step): that token likely.
    fn emission_for(path: &[u32]) -> Emission {
        let mut logits = Vec::new();
        for &p in path {
            for v in 0..vocab::SIZE as u32 {
                logits.push(if v == p { 8.0 } else { 0.0 });
            }
        }
        Emission::from_logits(vocab::SIZE, logits)
    }

    fn t(c: char) -> u32 {
        vocab::token(c).unwrap()
    }

    fn line(words: &[&str], start_ms: u64, end_ms: u64) -> Line {
        Line {
            words: words.iter().map(|w| w.to_string()).collect(),
            start_ms,
            end_ms,
        }
    }

    /// Silence, then "HI YO" at step `at` (H I | Y O), then silence: `len` steps.
    fn hi_yo(at: usize, len: usize) -> Vec<u32> {
        let mut path = vec![BLANK; len];
        for (k, token) in [t('h'), t('i'), BLANK, WORD_GAP, t('y'), BLANK, BLANK, t('o')]
            .into_iter()
            .enumerate()
        {
            path[at + k] = token;
        }
        path
    }

    #[test]
    fn letters_words_and_lines_get_their_times() {
        // 4 s of steps; "Hi yo!" heard from step 100 (2 s).
        let e = emission_for(&hi_yo(100, 200));
        let lines = align_lines(&e, &[line(&["Hi", "yo!"], 1_800, 2_400)]);
        let words = &lines[0].words;
        let hi = words[0].as_ref().unwrap();
        assert_eq!((hi.start_ms, hi.end_ms), (2_000, 2_040));
        assert_eq!(hi.chars[1].unwrap().start_ms, 2_020);
        let yo = words[1].as_ref().unwrap();
        assert_eq!((yo.start_ms, yo.end_ms), (2_080, 2_160));
        // The "!" isn't heard.
        assert_eq!(yo.chars.len(), 3);
        assert!(yo.chars[2].is_none());
        assert!(lines[0].confidence > 0.9, "{}", lines[0].confidence);
    }

    #[test]
    fn each_line_is_looked_for_near_its_rough_time_and_lines_meet_where_they_overlap() {
        // "hi yo" twice: at 1 s and at 5 s. Each line finds its own.
        let mut path = hi_yo(50, 400);
        path[250..258].copy_from_slice(&hi_yo(0, 8));
        let e = emission_for(&path);
        let lines = align_lines(
            &e,
            &[
                line(&["hi", "yo"], 1_200, 1_400),
                line(&["hi", "yo"], 4_800, 5_300),
            ],
        );
        assert_eq!(lines[0].words[0].as_ref().unwrap().start_ms, 1_000);
        assert_eq!(lines[1].words[0].as_ref().unwrap().start_ms, 5_000);
        // Rough times that overlap: aligned together, still each its own.
        let lines = align_lines(
            &e,
            &[
                line(&["hi", "yo"], 1_200, 4_000),
                line(&["hi", "yo"], 4_500, 5_300),
            ],
        );
        assert_eq!(lines[0].words[0].as_ref().unwrap().start_ms, 1_000);
        assert_eq!(lines[1].words[0].as_ref().unwrap().start_ms, 5_000);
    }

    #[test]
    fn a_line_that_cant_be_placed_or_has_no_letters_is_left_unplaced() {
        let e = emission_for(&hi_yo(10, 30));
        // Words with nothing the model hears.
        let lines = align_lines(&e, &[line(&["42"], 0, 400)]);
        assert_eq!(lines[0].words, [None]);
        assert_eq!(lines[0].confidence, 0.0);
        // More letters than steps.
        let lines = align_lines_within(&e, &[line(&["supercalifragilistic"; 3], 0, 100)], 0, 45_000, 600);
        assert!(lines[0].words.iter().all(Option::is_none));
    }
}
