//! CTC forced alignment: placing known tokens on a model's steps by the most likely path.
//!
//! The model scores every token at every step ([`Emission`]: log-probabilities, a step every
//! [`FRAME_MS`]). A path through the steps says one token each step: the blank (nothing new), or
//! a token of the transcript, in order, each said for one step or more, a blank needed between
//! two of the same ("LL"). The Viterbi trellis (as in torchaudio's `forced_align`) finds the
//! likeliest path over states blank, t₁, blank, t₂, … blank; each token's steps are its
//! [`Span`], scored by how likely the model found it there.

use crate::vocab;
use std::ops::Range;

/// The time between two steps of the model (ms): 320 samples at 16 kHz.
pub const FRAME_MS: f64 = 20.0;
/// The most trellis cells (steps × states) worked out in one alignment.
pub const MAX_CELLS: usize = 250_000_000;

/// How likely each token is at each step: log-probabilities, a row of [`vocab::SIZE`] per
/// step, step `i` heard from `i * FRAME_MS`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Emission {
    pub vocab: usize,
    pub log_probs: Vec<f32>,
}

const MAGIC: &[u8; 4] = b"PFEM";

impl Emission {
    /// From log-probabilities, a row of `vocab` per step.
    pub fn new(vocab: usize, log_probs: Vec<f32>) -> Self {
        Self { vocab, log_probs }
    }

    /// From a model's scores (logits), each row turned into log-probabilities.
    pub fn from_logits(vocab: usize, mut logits: Vec<f32>) -> Self {
        for row in logits.chunks_mut(vocab.max(1)) {
            let top = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let sum: f32 = row.iter().map(|&x| (x - top).exp()).sum();
            let log_sum = top + sum.ln();
            for x in row.iter_mut() {
                *x -= log_sum;
            }
        }
        Self::new(vocab, logits)
    }

    pub fn frames(&self) -> usize {
        self.log_probs.len().checked_div(self.vocab).unwrap_or(0)
    }

    /// How likely `token` is at step `t` (log).
    pub fn at(&self, t: usize, token: u32) -> f32 {
        self.log_probs[t * self.vocab + token as usize]
    }

    /// The emission's steps `range`.
    pub fn frames_in(&self, range: Range<usize>) -> &[f32] {
        &self.log_probs[range.start * self.vocab..range.end * self.vocab]
    }

    /// Joins `next` on after this one.
    pub fn extend(&mut self, next: &Emission) {
        if self.vocab == 0 {
            self.vocab = next.vocab;
        }
        self.log_probs.extend_from_slice(&next.log_probs);
    }

    /// The emission as bytes, for a cache file.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + self.log_probs.len() * 4);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(self.vocab as u32).to_le_bytes());
        out.extend_from_slice(&(self.frames() as u32).to_le_bytes());
        for v in &self.log_probs {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    /// An emission read back from [`Emission::to_bytes`]; `None` when the bytes aren't one.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let (magic, rest) = bytes.split_at_checked(4)?;
        let (vocab, rest) = rest.split_at_checked(4)?;
        let (frames, rest) = rest.split_at_checked(4)?;
        let vocab = u32::from_le_bytes(vocab.try_into().ok()?) as usize;
        let frames = u32::from_le_bytes(frames.try_into().ok()?) as usize;
        if magic != MAGIC || vocab == 0 || rest.len() != frames.checked_mul(vocab)?.checked_mul(4)? {
            return None;
        }
        let log_probs = rest
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&c| f32::from_le_bytes(c))
            .collect();
        Some(Self::new(vocab, log_probs))
    }
}

/// Where one token of the transcript was said: steps `start..end` of the emission, and how
/// likely the model found it there (0–1, the mean probability over those steps).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub score: f32,
}

/// From the state before (stay, one back, two back).
const STAY: u8 = 0;
const STEP: u8 = 1;
const SKIP: u8 = 2;

/// Places `tokens` (none the blank) on the emission's steps `frames` by the likeliest CTC path:
/// a [`Span`] per token, in order, steps counted from the emission's start. `None` when there
/// are too few steps for the tokens, too many cells to work out ([`MAX_CELLS`]), or no tokens.
pub fn force_align(emission: &Emission, frames: Range<usize>, tokens: &[u32]) -> Option<Vec<Span>> {
    let frames = frames.start..frames.end.min(emission.frames());
    let t_count = frames.len();
    let n = tokens.len();
    if n == 0
        || tokens
            .iter()
            .any(|&t| t == vocab::BLANK || t as usize >= emission.vocab)
    {
        return None;
    }
    let repeats = tokens.windows(2).filter(|w| w[0] == w[1]).count();
    let states = 2 * n + 1;
    if t_count < n + repeats || t_count.checked_mul(states)? > MAX_CELLS {
        return None;
    }
    // State s: even a blank, odd token (s - 1) / 2.
    let label = |s: usize| {
        if s.is_multiple_of(2) {
            vocab::BLANK
        } else {
            tokens[s / 2]
        }
    };
    let mut back = vec![STAY; t_count * states];
    let mut score = vec![f32::NEG_INFINITY; states];
    let mut next = vec![f32::NEG_INFINITY; states];
    let first = frames.start;
    score[0] = emission.at(first, vocab::BLANK);
    score[1] = emission.at(first, tokens[0]);
    for t in 1..t_count {
        let at = first + t;
        // Only states that can still reach the end in time, and be reached by now.
        let lowest = states.saturating_sub(2 * (t_count - t));
        let highest = (2 * t + 1).min(states - 1);
        next.fill(f32::NEG_INFINITY);
        for s in lowest..=highest {
            let mut best = score[s];
            let mut from = STAY;
            if s >= 1 && score[s - 1] > best {
                best = score[s - 1];
                from = STEP;
            }
            if s >= 2 && s % 2 == 1 && tokens[s / 2] != tokens[s / 2 - 1] && score[s - 2] > best {
                best = score[s - 2];
                from = SKIP;
            }
            if best > f32::NEG_INFINITY {
                next[s] = best + emission.at(at, label(s));
                back[t * states + s] = from;
            }
        }
        std::mem::swap(&mut score, &mut next);
    }
    // Ending on the last token, or the blank after it.
    let mut s = if score[states - 1] >= score[states - 2] {
        states - 1
    } else {
        states - 2
    };
    if score[s] == f32::NEG_INFINITY {
        return None;
    }
    let mut path = vec![0usize; t_count];
    for t in (0..t_count).rev() {
        path[t] = s;
        if t > 0 {
            s -= usize::from(back[t * states + s]);
        }
    }
    let mut spans: Vec<Option<Span>> = vec![None; n];
    let mut sums = vec![0.0f32; n];
    for (t, &s) in path.iter().enumerate() {
        if s.is_multiple_of(2) {
            continue;
        }
        let k = s / 2;
        sums[k] += emission.at(first + t, tokens[k]).exp();
        let span = spans[k].get_or_insert(Span {
            start: first + t,
            end: first + t,
            score: 0.0,
        });
        span.end = first + t + 1;
    }
    spans
        .into_iter()
        .zip(sums)
        .map(|(span, sum)| {
            span.map(|mut s| {
                s.score = sum / (s.end - s.start) as f32;
                s
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An emission over `path` (a token each step): that token likely, the rest not.
    fn emission_for(path: &[u32], sure: f32) -> Emission {
        let mut logits = Vec::new();
        for &p in path {
            for v in 0..vocab::SIZE as u32 {
                logits.push(if v == p { sure } else { 0.0 });
            }
        }
        Emission::from_logits(vocab::SIZE, logits)
    }

    const A: u32 = 7;
    const B: u32 = 24;
    const L: u32 = 15;
    const BL: u32 = vocab::BLANK;

    #[test]
    fn log_probabilities_sum_to_one() {
        let e = emission_for(&[A, BL], 4.0);
        assert_eq!(e.frames(), 2);
        for t in 0..2 {
            let total: f32 = (0..vocab::SIZE as u32).map(|v| e.at(t, v).exp()).sum();
            assert!((total - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn a_known_path_is_found_again() {
        // blank blank A A blank B blank: A at steps 2–3, B at 5.
        let e = emission_for(&[BL, BL, A, A, BL, B, BL], 8.0);
        let spans = force_align(&e, 0..e.frames(), &[A, B]).unwrap();
        assert_eq!((spans[0].start, spans[0].end), (2, 4));
        assert_eq!((spans[1].start, spans[1].end), (5, 6));
        assert!(spans.iter().all(|s| s.score > 0.9), "{spans:?}");
        // Within a range of the steps: counted from the emission's start.
        let e = emission_for(&[A, A, BL, BL, B, B, BL], 8.0);
        let spans = force_align(&e, 2..7, &[B]).unwrap();
        assert_eq!((spans[0].start, spans[0].end), (4, 6));
    }

    #[test]
    fn a_doubled_letter_needs_a_blank_between() {
        // L blank L: two Ls, not one held.
        let e = emission_for(&[L, BL, L, BL], 8.0);
        let spans = force_align(&e, 0..4, &[L, L]).unwrap();
        assert_eq!((spans[0].start, spans[0].end), (0, 1));
        assert_eq!((spans[1].start, spans[1].end), (2, 3));
        // Two steps can't hold L blank L.
        assert!(force_align(&e, 0..2, &[L, L]).is_none());
        // Without the blank the model says one L held, but two are forced: each gets a step.
        let e = emission_for(&[L, L, L, BL], 8.0);
        let spans = force_align(&e, 0..4, &[L, L]).unwrap();
        assert!(spans[0].end <= spans[1].start, "{spans:?}");
    }

    #[test]
    fn a_letter_the_model_never_heard_is_squeezed_in_unsure() {
        // "AB" where only B was heard: A gets a step, scored low.
        let e = emission_for(&[BL, BL, BL, B, B, BL], 8.0);
        let spans = force_align(&e, 0..6, &[A, B]).unwrap();
        assert_eq!(spans[0].end - spans[0].start, 1);
        assert!(spans[0].score < 0.01, "{spans:?}");
        assert_eq!(spans[1].start, 3);
        assert!(spans[1].score > 0.9);
    }

    #[test]
    fn too_few_steps_or_no_tokens_give_nothing() {
        let e = emission_for(&[A, B], 8.0);
        assert!(force_align(&e, 0..2, &[A, B, A]).is_none());
        assert!(force_align(&e, 0..2, &[]).is_none());
        assert!(force_align(&e, 0..2, &[BL]).is_none());
        // A range past the end is cut to it.
        assert_eq!(force_align(&e, 0..99, &[A, B]).unwrap().len(), 2);
    }

    #[test]
    fn emissions_come_back_from_bytes() {
        let e = emission_for(&[A, BL, B], 3.0);
        assert_eq!(Emission::from_bytes(&e.to_bytes()), Some(e.clone()));
        let mut bytes = e.to_bytes();
        bytes.pop();
        assert_eq!(Emission::from_bytes(&bytes), None);
        assert_eq!(Emission::from_bytes(b"nope"), None);
        let mut joined = Emission::default();
        joined.extend(&e);
        joined.extend(&e);
        assert_eq!(joined.frames(), 6);
        assert_eq!(joined.frames_in(3..4), e.frames_in(0..1));
    }
}
