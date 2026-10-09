//! Word timing found on this computer ([`pf_align`]): once the words are known (published,
//! pasted, or heard), each line is lined up with the song letter by letter, and each word's
//! start, syllables, and sounds come from when its letters were heard.
//!
//! - **Sure enough or not**: a line the aligner is unsure of ([`MIN_LINE_CONFIDENCE`]) keeps
//!   the timing it had, as does a word ([`MIN_WORD_CONFIDENCE`]); a kept word that no longer
//!   fits between its aligned neighbours moves only as far as it must to fit.
//! - **Starts**: where the word's first letter was heard, moved onto a clear onset of the voice
//!   within [`ONSET_REACH_MS`] (the model's steps are 20 ms; the voice is measured finer).
//! - **Ends**: the model hears a letter for a step or two, not while it's held, so a word's end
//!   is where the voice lets go after its last letter ([`refine`]'s word ends), before the next.
//! - **Unsung letters**: a word whose first syllable is a lone unstressed vowel ("a·fraid")
//!   that the aligner barely heard ([`ELIDED_CONFIDENCE`]) was sung without it ("'fraid"): it
//!   starts at the rest, and its sounds are the rest's.
//! - **No times at all** (plain lyrics, nothing heard): the whole song's words are aligned as
//!   one, words the aligner couldn't place shared out between those it could.
//!
//! What the model heard of a song is kept by the song file's hash and the model's id
//! ([`super::cache`]), so lining up other lyrics for it again is quick.

use super::combine::{Phrase, Word, WordSource};
use super::refine;
use crate::provider::Cancel;
use pf_align::{CharTime, Emission, Line, SyllableTime, WordTime};
use pf_analysis::VocalTrack;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// The language the aligner's model hears (its letters are English's).
pub const LANGUAGE: &str = "en";
/// Lines whose words the aligner was less sure of than this (0–1, on average) keep their
/// timing.
pub const MIN_LINE_CONFIDENCE: f32 = 0.15;
/// Words the aligner was less sure of than this keep their timing.
pub const MIN_WORD_CONFIDENCE: f32 = 0.25;
/// A lead vowel heard less surely than this wasn't sung.
pub const ELIDED_CONFIDENCE: f32 = 0.15;
/// The shortest a word is made (ms).
const MIN_WORD_MS: u64 = refine::MIN_WORD_MS;
/// How far an aligned word's start moves onto a clear onset of the voice (ms).
pub const ONSET_REACH_MS: f64 = 30.0;
/// Room given each word placed before the first or after the last aligned one, with no times
/// to go by (ms).
const EDGE_WORD_MS: u64 = 400;

/// Hears a song for alignment; tests use a stand-in.
pub trait SongAligner: Send + Sync {
    /// The model's id: what's heard with it is kept under it.
    fn model(&self) -> &str;
    /// The song's voice brought forward, mono at 16 kHz ([`pf_align::voice`]).
    fn voice(&self, path: &Path, cancel: &Cancel, progress: &dyn Fn(f32)) -> Result<Vec<f32>, String>;
    /// `voice` heard letter by letter ([`pf_align::song`]).
    fn hear(&self, voice: &[f32], cancel: &Cancel, progress: &dyn Fn(f32)) -> Result<Emission, String>;
}

/// The real aligner: wav2vec2 from the model store, loaded the first time it's needed.
pub struct OnDevice {
    id: String,
    path: PathBuf,
    loaded: OnceLock<Result<Arc<pf_align::Wav2Vec2>, String>>,
}

impl OnDevice {
    /// The aligner for the model installed in `store`; `None` when it isn't.
    pub fn new(store: &pf_align::ModelStore) -> Option<Self> {
        if !store.is_installed() {
            return None;
        }
        Some(Self {
            id: store.manifest().id.to_string(),
            path: store.model_path()?,
            loaded: OnceLock::new(),
        })
    }

    fn model_ready(&self) -> Result<Arc<pf_align::Wav2Vec2>, String> {
        self.loaded
            .get_or_init(|| {
                pf_align::Wav2Vec2::load(&self.path)
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            })
            .clone()
    }
}

impl SongAligner for OnDevice {
    fn model(&self) -> &str {
        &self.id
    }

    fn voice(&self, path: &Path, cancel: &Cancel, progress: &dyn Fn(f32)) -> Result<Vec<f32>, String> {
        pf_align::voice::centre_voice(path, &|| cancel.is_cancelled(), progress).map_err(|e| e.to_string())
    }

    fn hear(&self, voice: &[f32], cancel: &Cancel, progress: &dyn Fn(f32)) -> Result<Emission, String> {
        let model = self.model_ready()?;
        pf_align::song::emission_of(model.as_ref(), voice, &|| cancel.is_cancelled(), progress)
            .map_err(|e| e.to_string())
    }
}

/// What aligning did, for the user and for checking it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub words: usize,
    /// Words placed by the aligner.
    pub aligned: usize,
    /// Words found sung without their first vowel ("'fraid").
    pub elided: usize,
    /// Each line's confidence (0–1).
    pub line_confidence: Vec<f32>,
    /// How far aligned words' starts moved from the timing they had (ms): mean and median.
    pub mean_move_ms: f64,
    pub median_move_ms: f64,
}

impl Report {
    /// What to tell the user ("Word timing found on this computer for 180 of 231 words
    /// (average change 40 ms)."); `None` when no word was placed.
    pub fn sentence(&self) -> Option<String> {
        if self.aligned == 0 {
            return None;
        }
        let change = if self.mean_move_ms > 0.0 {
            format!(" (average change {} ms)", self.mean_move_ms.round())
        } else {
            String::new()
        };
        Some(format!(
            "Word timing found on this computer for {} of {} words{change}.",
            self.aligned, self.words
        ))
    }

    /// Whether most words were placed by the aligner.
    pub fn mostly_aligned(&self) -> bool {
        self.aligned * 2 >= self.words.max(1)
    }
}

/// The words with their aligned times, and each word's syllables and sounds when the aligner
/// placed it (in order, one per word).
#[derive(Debug, Clone, PartialEq)]
pub struct Aligned {
    pub phrases: Vec<Phrase>,
    pub sounds: Vec<Option<Vec<SyllableTime>>>,
    pub report: Report,
}

/// What the aligner says of a word it's sure enough of.
struct Placed {
    start: u64,
    /// Where its last letter was heard.
    heard_end: u64,
    confidence: f32,
    /// What it was sung as when its first vowel wasn't sung, and that part's letters.
    sung: Option<String>,
    chars: Vec<Option<CharTime>>,
}

/// Where `rest` (a word's spelling less its lead, "fraid") starts in `text` ("Afraid!"), as a
/// character index.
fn rest_at(text: &str, rest: &str) -> Option<usize> {
    let chars: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    let rest: Vec<char> = rest.chars().flat_map(char::to_lowercase).collect();
    if chars.len() != text.chars().count() || rest.is_empty() {
        return None;
    }
    (1..chars.len()).find(|&k| chars[k..].starts_with(&rest))
}

/// The aligner's word `heard`, as `word` is placed: its start, and whether its lead vowel was
/// sung.
fn placed(word: &Word, heard: &WordTime) -> Placed {
    let plain = Placed {
        start: heard.start_ms,
        heard_end: heard.end_ms,
        confidence: heard.confidence,
        sung: None,
        chars: heard.chars.clone(),
    };
    if word.sung.is_some() {
        return plain;
    }
    let Some((rest, _)) = refine::dropped_lead(&word.text) else {
        return plain;
    };
    let Some(at) = rest_at(&word.text, &rest) else {
        return plain;
    };
    let lead_unsure = heard.chars[..at]
        .iter()
        .flatten()
        .all(|c| c.confidence < ELIDED_CONFIDENCE);
    let rest_chars: Vec<Option<CharTime>> =
        heard.chars[at..(at + rest.chars().count()).min(heard.chars.len())].to_vec();
    match rest_chars.iter().flatten().next() {
        Some(first) if lead_unsure => Placed {
            start: first.start_ms,
            sung: Some(rest),
            chars: rest_chars,
            ..plain
        },
        _ => plain,
    }
}

/// Starts for the words `run` (none placed) between `lo` and `hi`: with `keep`, each where it
/// was (`olds`), moved only as far as it must to stay in order inside; else shared out evenly.
fn fill(starts: &mut [Option<u64>], olds: &[u64], run: std::ops::Range<usize>, lo: u64, hi: u64, keep: bool) {
    let count = run.len() as u64;
    let room = hi.saturating_sub(lo);
    // Too little room to keep them apart: shared out.
    let keep = keep && room >= (count - 1) * MIN_WORD_MS;
    let mut floor = lo;
    for (j, k) in run.clone().enumerate() {
        let start = if keep {
            let ceiling = hi - (count - 1 - j as u64) * MIN_WORD_MS;
            olds[k].clamp(floor, ceiling.max(floor))
        } else {
            lo + room * j as u64 / count
        };
        starts[k] = Some(start);
        floor = start + MIN_WORD_MS;
    }
}

/// Lines `phrases` up with the song's `emission` (see the module notes): each line within a
/// second of where it is, or, with `whole_song` (no times to go by), all as one. Ends are
/// found on `voice` when there is one; nothing goes past `end_ms`.
pub fn align(
    phrases: &[Phrase],
    emission: &Emission,
    voice: Option<&VocalTrack>,
    end_ms: u64,
    whole_song: bool,
) -> Aligned {
    let said = |w: &Word| w.sung.clone().unwrap_or_else(|| w.text.clone());
    let lines: Vec<Line> = phrases
        .iter()
        .map(|p| Line {
            words: p.words.iter().map(said).collect(),
            start_ms: if whole_song { 0 } else { p.start_ms },
            end_ms: if whole_song { end_ms } else { p.end_ms },
        })
        .collect();
    let song_ms = end_ms.min((emission.frames() as f64 * pf_align::ctc::FRAME_MS) as u64);
    let timed = if whole_song {
        pf_align::align::align_lines_within(emission, &lines, 0, u64::MAX, song_ms)
    } else {
        pf_align::align_lines(emission, &lines)
    };
    let words: Vec<&Word> = phrases.iter().flat_map(|p| &p.words).collect();
    let n = words.len();
    let mut found: Vec<Option<Placed>> = Vec::with_capacity(n);
    for (phrase, line) in phrases.iter().zip(&timed) {
        let sure = line.confidence >= MIN_LINE_CONFIDENCE;
        for (word, heard) in phrase.words.iter().zip(&line.words) {
            found.push(
                heard
                    .as_ref()
                    .filter(|h| sure && h.confidence >= MIN_WORD_CONFIDENCE)
                    .map(|h| placed(word, h)),
            );
        }
    }
    let mut report = Report {
        words: n,
        aligned: found.iter().flatten().count(),
        elided: found.iter().flatten().filter(|p| p.sung.is_some()).count(),
        line_confidence: timed.iter().map(|l| l.confidence).collect(),
        ..Report::default()
    };
    // Starts: placed words where the aligner put them, the rest kept or shared out between.
    let olds: Vec<u64> = words.iter().map(|w| w.start_ms).collect();
    let mut starts: Vec<Option<u64>> = found.iter().map(|f| f.as_ref().map(|p| p.start)).collect();
    let mut k = 0;
    while k < n {
        if starts[k].is_some() {
            k += 1;
            continue;
        }
        let a = k;
        while k < n && starts[k].is_none() {
            k += 1;
        }
        let run = a..k;
        let before = a.checked_sub(1).and_then(|i| starts[i]);
        let after = starts.get(k).copied().flatten();
        let count = run.len() as u64;
        match (before, after, whole_song) {
            // Nothing placed at all: the song's start to its end.
            (None, None, true) => fill(&mut starts, &olds, run, 0, end_ms, false),
            (None, Some(b), true) => {
                let lo = b.saturating_sub(count * EDGE_WORD_MS);
                fill(&mut starts, &olds, run, lo, b.saturating_sub(MIN_WORD_MS), false);
            }
            (Some(a), None, true) => {
                let lo = a + MIN_WORD_MS;
                fill(
                    &mut starts,
                    &olds,
                    run,
                    lo,
                    (lo + count * EDGE_WORD_MS).min(end_ms),
                    false,
                );
            }
            (before, after, _) => {
                let lo = before.map_or(0, |s| s + MIN_WORD_MS);
                let hi = after.map_or(end_ms, |s| s.saturating_sub(MIN_WORD_MS));
                fill(&mut starts, &olds, run, lo, hi.max(lo), !whole_song);
            }
        }
    }
    let mut starts: Vec<u64> = starts.into_iter().map(Option::unwrap_or_default).collect();
    // The aligner's steps are 20 ms; the voice's onsets are finer. A placed word moves onto a
    // clear one within a step and a half.
    if let Some(voice) = voice {
        for k in 0..n {
            let Some(place) = &found[k] else {
                continue;
            };
            let floor = k.checked_sub(1).map_or(0.0, |j| (starts[j] + MIN_WORD_MS) as f64);
            let ceiling = starts
                .get(k + 1)
                .map_or(end_ms, |&s| s.saturating_sub(MIN_WORD_MS)) as f64;
            let said = place.sung.clone().unwrap_or_else(|| said(words[k]));
            let near = refine::onset_near(
                voice,
                starts[k] as f64,
                (floor, ceiling),
                (ONSET_REACH_MS, ONSET_REACH_MS),
                refine::starts_hissing(&said),
            );
            if let Some((t, clear)) = near
                && clear >= refine::SURE_ONSET
            {
                starts[k] = t.round() as u64;
            }
        }
    }
    for k in 1..n {
        starts[k] = starts[k].max(starts[k - 1] + 1);
    }
    // Ends: where the voice lets go after the last letter heard, before the next word.
    let mut out = phrases.to_vec();
    let mut sounds = Vec::with_capacity(n);
    let mut moves = Vec::new();
    let mut k = 0;
    for phrase in &mut out {
        for word in &mut phrase.words {
            let start = starts[k];
            let next = starts.get(k + 1).copied();
            let limit = next.unwrap_or(end_ms).min(end_ms).max(start + 1);
            let place = found[k].take();
            let end = match (&place, voice) {
                // Never before its last letter was heard, whatever the voice's loudness says.
                (Some(p), Some(v)) => {
                    let heard = p.heard_end.max(start + MIN_WORD_MS);
                    refine::word_end(v, start, heard, next, end_ms).max(heard)
                }
                (Some(p), None) => p.heard_end.max(start + MIN_WORD_MS),
                (None, Some(v)) if whole_song => {
                    refine::word_end(v, start, start + MIN_WORD_MS, next, end_ms)
                }
                (None, _) if whole_song => start + EDGE_WORD_MS,
                (None, _) => word.end_ms,
            }
            .clamp(start + 1, limit);
            match place {
                Some(p) => {
                    if !whole_song {
                        moves.push(start.abs_diff(word.start_ms) as f64);
                    }
                    let said = p.sung.clone().unwrap_or_else(|| said(word));
                    sounds.push(Some(pf_align::word_sounds(&said, start, end, &p.chars)));
                    if p.sung.is_some() {
                        word.sung = p.sung;
                    }
                    word.source = WordSource::Aligned;
                    word.confidence = 0.5 + p.confidence.clamp(0.0, 1.0) / 2.0;
                }
                None => {
                    sounds.push(None);
                    if whole_song {
                        word.source = WordSource::Spread;
                        word.confidence = word.confidence.min(0.3);
                    }
                }
            }
            word.start_ms = start;
            word.end_ms = end;
            k += 1;
        }
        phrase.start_ms = phrase.words.first().map_or(phrase.start_ms, |w| w.start_ms);
        phrase.end_ms = phrase.words.last().map_or(phrase.end_ms, |w| w.end_ms);
    }
    if !moves.is_empty() {
        report.mean_move_ms = moves.iter().sum::<f64>() / moves.len() as f64;
        moves.sort_by(f64::total_cmp);
        report.median_move_ms = moves[moves.len() / 2];
    }
    Aligned {
        phrases: out,
        sounds,
        report,
    }
}

/// What the aligner's model heard of a song is kept under.
pub fn cache_kind(model: &str) -> String {
    format!("emission-{model}")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use pf_align::vocab::{self, BLANK, WORD_GAP};

    /// An emission over `path` (a token each 20 ms step): that token likely.
    pub(crate) fn emission_for(path: &[u32]) -> Emission {
        let mut logits = Vec::new();
        for &p in path {
            for v in 0..vocab::SIZE as u32 {
                logits.push(if v == p { 8.0 } else { 0.0 });
            }
        }
        Emission::from_logits(vocab::SIZE, logits)
    }

    /// `text` heard from step `at`, a letter a step, a gap between words; blanks elsewhere.
    pub(crate) fn heard(path: &mut [u32], at: usize, text: &str) {
        let mut t = at;
        for (i, word) in text.split(' ').enumerate() {
            if i > 0 {
                path[t] = WORD_GAP;
                t += 1;
            }
            for c in word.chars() {
                path[t] = vocab::token(c).unwrap();
                t += 2;
            }
        }
    }

    fn word(text: &str, start_ms: u64, end_ms: u64) -> Word {
        Word {
            text: text.into(),
            start_ms,
            end_ms,
            source: WordSource::Matched,
            confidence: 0.95,
            sung: None,
        }
    }

    fn phrase(words: Vec<Word>) -> Phrase {
        Phrase {
            text: words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            start_ms: words[0].start_ms,
            end_ms: words.last().unwrap().end_ms,
            words,
        }
    }

    #[test]
    fn sure_words_move_to_where_their_letters_were_heard() {
        // 6 s: "paper lantern" heard from 2 s.
        let mut path = vec![BLANK; 300];
        heard(&mut path, 100, "paper lantern");
        let e = emission_for(&path);
        let p = phrase(vec![word("Paper", 2_150, 2_400), word("lantern", 2_500, 3_000)]);
        let a = align(&[p], &e, None, 6_000, false);
        let words = &a.phrases[0].words;
        assert_eq!(words[0].start_ms, 2_000);
        // P a p e r: the r at step 108; the gap, then l at step 111.
        assert_eq!(words[1].start_ms, 2_220);
        assert!(words[0].end_ms <= words[1].start_ms);
        assert!(words.iter().all(|w| w.source == WordSource::Aligned));
        assert_eq!(a.report.aligned, 2);
        assert_eq!(a.report.mean_move_ms, (150.0 + 280.0) / 2.0);
        assert_eq!(a.phrases[0].start_ms, 2_000);
        // Sounds for each: "paper" is P EY | P ER, from its letters.
        let paper = a.sounds[0].as_ref().unwrap();
        assert_eq!(paper.len(), 2);
        assert_eq!(paper[0].phones[0].start_ms, 2_000);
        assert_eq!(paper[1].start_ms, 2_080);
        assert_eq!(
            a.report.sentence().unwrap(),
            "Word timing found on this computer for 2 of 2 words (average change 215 ms)."
        );
    }

    #[test]
    fn unsure_lines_and_words_keep_their_timing() {
        // Nothing heard at all: every line unsure, kept as it was.
        let e = emission_for(&vec![BLANK; 300]);
        let p = phrase(vec![word("Paper", 2_150, 2_400), word("lantern", 2_500, 3_000)]);
        let a = align(std::slice::from_ref(&p), &e, None, 6_000, false);
        assert_eq!(a.phrases[0], p);
        assert_eq!(a.report.aligned, 0);
        assert!(a.report.sentence().is_none());
        assert!(a.sounds.iter().all(Option::is_none));
    }

    #[test]
    fn a_kept_word_that_no_longer_fits_moves_just_inside_its_neighbours() {
        // "one two three" heard at 1 s; "two" kept (only its letters are muffled) but its old
        // time is after "three".
        let mut path = vec![BLANK; 200];
        heard(&mut path, 50, "one");
        heard(&mut path, 80, "three");
        let e = emission_for(&path);
        let p = phrase(vec![
            word("one", 1_000, 1_200),
            word("two", 2_500, 2_600),
            word("three", 1_600, 1_900),
        ]);
        let a = align(&[p], &e, None, 4_000, false);
        let w = &a.phrases[0].words;
        assert_eq!(w[0].start_ms, 1_000);
        assert_eq!(w[2].start_ms, 1_600);
        assert!(
            w[1].start_ms > w[0].start_ms && w[1].start_ms < w[2].start_ms,
            "{w:?}"
        );
        assert_ne!(w[1].source, WordSource::Aligned);
        assert!(w[0].end_ms <= w[1].start_ms && w[1].end_ms <= w[2].start_ms);
    }

    #[test]
    fn an_unsung_lead_vowel_is_dropped() {
        // "afraid" sung as "fraid": only f r a i d heard.
        let mut path = vec![BLANK; 200];
        heard(&mut path, 60, "fraid");
        let e = emission_for(&path);
        let p = phrase(vec![word("afraid", 1_150, 1_500)]);
        let a = align(&[p], &e, None, 4_000, false);
        let w = &a.phrases[0].words[0];
        assert_eq!(w.sung.as_deref(), Some("fraid"));
        assert_eq!(w.start_ms, 1_200);
        assert_eq!(a.report.elided, 1);
        let sounds = a.sounds[0].as_ref().unwrap();
        assert_eq!(sounds.len(), 1, "one syllable: fraid");
        assert_eq!(sounds[0].phones[0].start_ms, 1_200);
        // Sung with its vowel: kept.
        let mut path = vec![BLANK; 200];
        heard(&mut path, 58, "afraid");
        let a = align(
            &[phrase(vec![word("afraid", 1_150, 1_500)])],
            &emission_for(&path),
            None,
            4_000,
            false,
        );
        assert_eq!(a.phrases[0].words[0].sung, None);
        assert_eq!(a.phrases[0].words[0].start_ms, 1_160);
        assert_eq!(rest_at("Afraid!", "fraid"), Some(1));
        assert_eq!(rest_at("ghost", "fraid"), None);
    }

    #[test]
    fn lyrics_with_no_times_are_aligned_across_the_whole_song() {
        // Two lines, no times (all at 0), heard at 3 s and 7 s of 10 s.
        let mut path = vec![BLANK; 500];
        heard(&mut path, 150, "paper lantern");
        heard(&mut path, 350, "snowy rooftops");
        let e = emission_for(&path);
        let lines = vec![
            phrase(vec![word("Paper", 0, 0), word("lantern", 0, 0)]),
            phrase(vec![word("Snowy", 0, 0), word("rooftops", 0, 0)]),
        ];
        let a = align(&lines, &e, None, 10_000, true);
        assert_eq!(a.phrases[0].start_ms, 3_000);
        assert_eq!(a.phrases[1].start_ms, 7_000);
        assert_eq!(a.report.aligned, 4);
    }
}
