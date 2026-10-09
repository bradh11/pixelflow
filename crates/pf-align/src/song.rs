//! A whole song heard letter by letter: the model run over pieces of [`CHUNK_FRAMES`] steps
//! (20 s), each with a second of sound either side for context, several pieces at once, and the
//! steps joined back into one [`Emission`] for the song.

use crate::AlignError;
use crate::ctc::Emission;
use crate::model::{Acoustic, frames_for};
use crate::vocab;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

/// Steps per piece (20 s).
pub const CHUNK_FRAMES: usize = 1_000;
/// Steps of context either side of a piece (1 s), heard but not kept.
pub const CONTEXT_FRAMES: usize = 50;
/// Pieces worked on at once, at most.
pub const MAX_WORKERS: usize = 4;
/// Samples per step, and the samples the first step takes in.
const STRIDE: usize = 320;
const FIELD: usize = 400;

/// What a piece the model couldn't cover says: nothing but blanks.
fn blank_row() -> Vec<f32> {
    (0..vocab::SIZE)
        .map(|v| if v as u32 == vocab::BLANK { 0.0 } else { -30.0 })
        .collect()
}

/// The steps `core` of `samples` (mono, 16 kHz), heard with their context.
fn piece(
    acoustic: &dyn Acoustic,
    samples: &[f32],
    core: std::ops::Range<usize>,
) -> Result<Emission, AlignError> {
    let first = core.start.saturating_sub(CONTEXT_FRAMES);
    let from = first * STRIDE;
    let to = ((core.end + CONTEXT_FRAMES) * STRIDE + (FIELD - STRIDE)).min(samples.len());
    let heard = acoustic.emit(&samples[from..to])?;
    let mut out = Vec::with_capacity(core.len() * vocab::SIZE);
    for g in core {
        let local = g - first;
        if local < heard.frames() && heard.vocab == vocab::SIZE {
            out.extend_from_slice(heard.frames_in(local..local + 1));
        } else {
            out.extend(blank_row());
        }
    }
    Ok(Emission::new(vocab::SIZE, out))
}

/// The song's `samples` (mono, 16 kHz) heard letter by letter, telling `progress` how far it
/// has got (0–1, from this thread) and giving up when `stop` says so.
pub fn emission_of(
    acoustic: &dyn Acoustic,
    samples: &[f32],
    stop: &(dyn Fn() -> bool + Sync),
    progress: &dyn Fn(f32),
) -> Result<Emission, AlignError> {
    let frames = frames_for(samples.len());
    let pieces = frames.div_ceil(CHUNK_FRAMES);
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .clamp(1, MAX_WORKERS)
        .min(pieces.max(1));
    let next = AtomicUsize::new(0);
    let mut results: Vec<Option<Result<Emission, AlignError>>> = (0..pieces).map(|_| None).collect();
    std::thread::scope(|scope| {
        let (sender, heard) = mpsc::channel();
        for _ in 0..workers {
            let sender = sender.clone();
            let next = &next;
            scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= pieces {
                        break;
                    }
                    let result = if stop() {
                        Err(AlignError::Cancelled)
                    } else {
                        let core = i * CHUNK_FRAMES..((i + 1) * CHUNK_FRAMES).min(frames);
                        piece(acoustic, samples, core)
                    };
                    let failed = result.is_err();
                    if sender.send((i, result)).is_err() || failed {
                        // The rest aren't worth hearing.
                        next.store(pieces, Ordering::Relaxed);
                        break;
                    }
                }
            });
        }
        drop(sender);
        let mut done = 0;
        for (i, result) in heard {
            done += usize::from(result.is_ok());
            results[i] = Some(result);
            progress(done as f32 / pieces as f32);
        }
    });
    let mut song = Emission::new(vocab::SIZE, Vec::with_capacity(frames * vocab::SIZE));
    for heard in results {
        match heard {
            Some(Ok(piece)) => song.extend(&piece),
            Some(Err(e)) => return Err(e),
            None => return Err(AlignError::Cancelled),
        }
    }
    Ok(song)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hears "A" wherever the sound is loud, blank elsewhere: a step per 320 samples.
    struct Loudness;

    impl Acoustic for Loudness {
        fn emit(&self, samples: &[f32]) -> Result<Emission, AlignError> {
            let mut logits = Vec::new();
            for f in 0..frames_for(samples.len()) {
                let loud = samples[f * STRIDE..f * STRIDE + FIELD]
                    .iter()
                    .any(|s| s.abs() > 0.5);
                for v in 0..vocab::SIZE as u32 {
                    let hot = if loud { 7 } else { vocab::BLANK };
                    logits.push(if v == hot { 10.0 } else { 0.0 });
                }
            }
            Ok(Emission::from_logits(vocab::SIZE, logits))
        }
    }

    #[test]
    fn pieces_join_up_into_the_songs_steps() {
        // 50 s, loud from 30 s to 30.5 s: spans the second piece's end and the third's start.
        let mut samples = vec![0.0f32; 16_000 * 50];
        for s in &mut samples[16_000 * 30..16_000 * 30 + 8_000] {
            *s = 1.0;
        }
        let reports = std::cell::RefCell::new(Vec::new());
        let e = emission_of(&Loudness, &samples, &|| false, &|f| reports.borrow_mut().push(f)).unwrap();
        assert_eq!(e.frames(), frames_for(samples.len()));
        let loud: Vec<usize> = (0..e.frames()).filter(|&t| e.at(t, 7) > -0.1).collect();
        // Steps whose 400 samples touch the loud half second: 1499 to 1524.
        assert_eq!(loud.first(), Some(&1_499));
        assert_eq!(loud.last(), Some(&1_524));
        assert_eq!(loud.len(), 26);
        let mut reports = reports.into_inner();
        reports.sort_by(f32::total_cmp);
        assert_eq!(reports.last(), Some(&1.0));
        assert_eq!(reports.len(), 3);
    }

    #[test]
    fn stop_stops() {
        let samples = vec![0.0f32; 16_000 * 30];
        let e = emission_of(&Loudness, &samples, &|| true, &|_| {});
        assert!(matches!(e, Err(AlignError::Cancelled)));
        // Too short to hear anything.
        assert_eq!(
            emission_of(&Loudness, &[0.0; 100], &|| false, &|_| {})
                .unwrap()
                .frames(),
            0
        );
    }
}
