//! Moments lighting should land on: hits (stabs, shouts, crashes that stand out from the music
//! around them), drops (a sharp rise after a quieter stretch), breaks (the music drops out), and
//! builds (energy rising over a few bars).

use crate::energy::BarEnergy;
use crate::onset::{OnsetEnvelope, normalized};
use serde::Serialize;

/// Hits kept per minute of song, at most (the strongest in each minute).
const HITS_PER_MINUTE: usize = 5;
/// Drops, breaks, and builds kept per minute of song, at most (each).
const OTHERS_PER_MINUTE: f64 = 1.5;
/// How much a hit must stand out: its flux as a multiple of the strong onsets' around it, plus
/// how much louder it peaks than they do, a point per `HIT_DB` dB.
const HIT_SCORE: f32 = 2.0;
const HIT_DB: f32 = 2.0;
/// Hits at least this far apart (s).
const HIT_GAP_S: f64 = 0.35;
/// The onsets a hit is compared with: this far (s) either side.
const HIT_CONTEXT_S: f64 = 3.0;
/// A break: at least this much (dB) quieter than the music around it, for at least this long (s).
const BREAK_DEPTH_DB: f32 = 12.0;
const BREAK_MIN_S: f64 = 0.3;
/// A drop: energy (0–1) rising at least this much from the 4 bars before to the 2 bars after.
const DROP_RISE: f32 = 0.25;
/// A build: 2–8 bars of energy (0–1) rising at least this much.
const BUILD_RISE: f32 = 0.2;
const BUILD_BARS: std::ops::RangeInclusive<usize> = 2..=8;

/// What kind of moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EventKind {
    Hit,
    Drop,
    Break,
    Build,
}

impl EventKind {
    pub fn word(self) -> &'static str {
        match self {
            EventKind::Hit => "Hit",
            EventKind::Drop => "Drop",
            EventKind::Break => "Break",
            EventKind::Build => "Build",
        }
    }
}

/// A moment in the song.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub time_ms: u64,
    pub kind: EventKind,
    /// How strong, 0–1 (1 = the strongest of its kind in the song).
    pub strength: f32,
    /// How long it lasts (breaks and builds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

fn round2(x: f32) -> f32 {
    (x.clamp(0.0, 1.0) * 100.0).round() / 100.0
}

/// Keeps the strongest `most` of `found` (time s, score, duration s), scales their strength so
/// the strongest is 1, and turns them into events.
fn keep(mut found: Vec<(f64, f32, Option<f64>)>, most: usize, kind: EventKind) -> Vec<Event> {
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    found.truncate(most);
    let top = found.first().map_or(1.0, |f| f.1).max(1e-6);
    found
        .into_iter()
        .map(|(t, score, duration)| Event {
            time_ms: (t * 1000.0).round() as u64,
            kind,
            strength: round2(score / top),
            duration_ms: duration.map(|d| (d * 1000.0).round() as u64),
        })
        .collect()
}

/// Hits: onsets that stand out from the strong onsets around them (within 3 s), by how much more
/// new sound starts (spectral flux, as a ratio) and how much louder they peak (dB); the strongest
/// few per minute, spaced out.
fn hits(envelope: &OnsetEnvelope, onsets: &[usize]) -> Vec<Event> {
    let e = normalized(envelope);
    let reach = (HIT_CONTEXT_S / envelope.frame_seconds()) as usize;
    let after = envelope.frames_for_ms(90.0);
    let rms_db = |f: usize| 20.0 * envelope.loudness.get(f).copied().unwrap_or(0.0).max(1e-5).log10();
    let peak: Vec<f32> = onsets
        .iter()
        .map(|&i| (i..i + after).map(rms_db).fold(f32::MIN, f32::max))
        .collect();
    // The level the strong onsets around reach (their 80th percentile).
    let strong = |mut v: Vec<f32>| -> Option<f32> {
        let at = (v.len() * 4 / 5).min(v.len().saturating_sub(1));
        (!v.is_empty()).then(|| *v.select_nth_unstable_by(at, f32::total_cmp).1)
    };
    let mut found: Vec<(f64, f32)> = Vec::new();
    let mut lo = 0;
    for (k, &i) in onsets.iter().enumerate() {
        while onsets[lo] + reach < i {
            lo += 1;
        }
        let around: Vec<usize> = (lo..onsets.len())
            .take_while(|&m| onsets[m] <= i + reach)
            .filter(|&m| m != k)
            .collect();
        let flux = strong(around.iter().map(|&m| e[onsets[m]]).collect())
            .unwrap_or(0.5)
            .max(0.5);
        let loud = strong(around.iter().map(|&m| peak[m]).collect()).unwrap_or(peak[k]);
        let score = e[i] / flux + (peak[k] - loud).max(0.0) / HIT_DB;
        if score >= HIT_SCORE {
            found.push((envelope.time_ms(i) as f64 / 1000.0, score));
        }
    }
    // Strongest first, keeping those far enough from a stronger one and few enough per minute.
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut kept: Vec<(f64, f32, Option<f64>)> = Vec::new();
    let mut per_minute: Vec<usize> = Vec::new();
    for (t, score) in found {
        let minute = (t / 60.0) as usize;
        if per_minute.len() <= minute {
            per_minute.resize(minute + 1, 0);
        }
        if per_minute[minute] >= HITS_PER_MINUTE || kept.iter().any(|k| (k.0 - t).abs() < HIT_GAP_S) {
            continue;
        }
        per_minute[minute] += 1;
        kept.push((t, score, None));
    }
    let most = kept.len();
    keep(kept, most, EventKind::Hit)
}

/// Breaks: where the sound drops at least 12 dB below the music around it (the median of the
/// 8 s around) for at least 0.3 s, while the music plays: between the first and last onsets, and
/// from a second after the first beat to a second before the last (`beats`, s).
fn breaks(envelope: &OnsetEnvelope, onsets: &[usize], beats: Option<(f64, f64)>, most: usize) -> Vec<Event> {
    let (Some(&first), Some(&last)) = (onsets.first(), onsets.last()) else {
        return Vec::new();
    };
    let frame = |s: f64| (s.max(0.0) / envelope.frame_seconds()) as usize;
    let (first, last) = match beats {
        Some((a, b)) => (first.max(frame(a + 1.0)), last.min(frame((b - 1.0).max(0.0)))),
        None => (first, last),
    };
    if first >= last {
        return Vec::new();
    }
    let db: Vec<f32> = envelope
        .loudness
        .iter()
        .map(|&r| 20.0 * r.max(1e-5).log10())
        .collect();
    // The median of each half second, then of the 8 s around each.
    let block = envelope.frames_for_ms(500.0);
    let median = |v: &mut Vec<f32>| {
        let mid = v.len() / 2;
        *v.select_nth_unstable_by(mid, f32::total_cmp).1
    };
    let blocks: Vec<f32> = db.chunks(block).map(|c| median(&mut c.to_vec())).collect();
    let around: Vec<f32> = (0..blocks.len())
        .map(|b| median(&mut blocks[b.saturating_sub(8)..(b + 9).min(blocks.len())].to_vec()))
        .collect();
    // Only where the music around is playing (not a quiet intro or a fade-out).
    let playing = median(&mut blocks.clone()) - BREAK_DEPTH_DB;
    let min_frames = envelope.frames_for_ms(BREAK_MIN_S * 1000.0);
    let mut found = Vec::new();
    let mut run: Option<usize> = None;
    for i in first..=last.min(db.len().saturating_sub(1)) + 1 {
        let quiet = i <= last
            && db.get(i).is_some_and(|&d| {
                let reference = around[i / block];
                reference >= playing && d < reference - BREAK_DEPTH_DB
            });
        match (quiet, run) {
            (true, None) => run = Some(i),
            (false, Some(start)) => {
                run = None;
                if i - start >= min_frames {
                    let depth =
                        (start..i).map(|f| around[f / block] - db[f]).sum::<f32>() / (i - start) as f32;
                    let t = envelope.time_ms(start) as f64 / 1000.0;
                    let duration = (i - start) as f64 * envelope.frame_seconds();
                    found.push((t, depth / 30.0, Some(duration)));
                }
            }
            _ => {}
        }
    }
    keep(found, most, EventKind::Break)
}

/// Drops and builds from the bars' energy (the bars start at `bars`, s).
fn drops_and_builds(bars: &[f64], energy: &[BarEnergy], end: f64, most: usize) -> Vec<Event> {
    let n = energy.len().min(bars.len());
    if n < 6 {
        return Vec::new();
    }
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
    // Drops: the kick and bass count, as well as the overall level.
    let punch: Vec<f32> = energy[..n]
        .iter()
        .map(|e| 0.6 * e.overall + 0.4 * e.low)
        .collect();
    let rise: Vec<f32> = (0..n)
        .map(|j| {
            if j < 2 || j + 1 >= n {
                return 0.0;
            }
            let before = mean(&punch[j.saturating_sub(4)..j]);
            let after = mean(&punch[j..(j + 2).min(n)]);
            if before <= 0.65 && after >= 0.6 {
                after - before
            } else {
                0.0
            }
        })
        .collect();
    let drops: Vec<(f64, f32, Option<f64>)> = (0..n)
        .filter(|&j| rise[j] >= DROP_RISE)
        // Sudden: most of the rise in one bar (a steady climb is a build).
        .filter(|&j| punch[j] - punch[j - 1] >= 0.5 * rise[j])
        // The biggest rise of the bars around (the first, if two match).
        .filter(|&j| {
            (j.saturating_sub(2)..j).all(|k| rise[k] < rise[j])
                && (j + 1..(j + 3).min(n)).all(|k| rise[k] <= rise[j])
        })
        .map(|j| (bars[j], rise[j] / 0.6, None))
        .collect();
    // Builds: brightness counts as well as the level (risers, snare rolls).
    let lift: Vec<f32> = energy[..n]
        .iter()
        .map(|e| 0.5 * e.overall + 0.5 * e.high)
        .collect();
    let mut candidates: Vec<(usize, usize, f32)> = Vec::new();
    // Each starting where the energy starts to climb.
    for start in (0..n - 1).filter(|&j| lift[j + 1] > lift[j] + 0.02) {
        let mut stop = start;
        while stop + 1 < n && stop - start < *BUILD_BARS.end() && lift[stop + 1] >= lift[stop] - 0.03 {
            stop += 1;
        }
        // It ends where the energy peaks.
        stop = (start..=stop).fold(start, |best, j| if lift[j] > lift[best] { j } else { best });
        let gain = lift[stop] - lift[start];
        if BUILD_BARS.contains(&(stop - start)) && gain >= BUILD_RISE {
            candidates.push((start, stop, gain));
        }
    }
    candidates.sort_by(|a, b| b.2.total_cmp(&a.2));
    let mut builds: Vec<(usize, usize, f32)> = Vec::new();
    for c in candidates {
        if builds.iter().all(|b| c.1 <= b.0 || c.0 >= b.1) {
            builds.push(c);
        }
    }
    let at = |j: usize| bars.get(j).copied().unwrap_or(end);
    let builds = builds
        .into_iter()
        .map(|(start, stop, gain)| (at(start), gain / 0.5, Some(at(stop) - at(start))))
        .collect();
    let mut out = keep(drops, most, EventKind::Drop);
    out.extend(keep(builds, most, EventKind::Build));
    out
}

/// Every kind of moment in the song, in time order. `beats` and `bars` (s) are the beats and
/// bars, and `energy` the bars' energy (all empty without a beat: then there are no drops or
/// builds).
pub(crate) fn events(
    envelope: &OnsetEnvelope,
    onsets: &[usize],
    beats: &[f64],
    bars: &[f64],
    energy: &[BarEnergy],
) -> Vec<Event> {
    let duration = envelope.duration_ms() as f64 / 1000.0;
    let most = ((duration / 60.0 * OTHERS_PER_MINUTE).ceil() as usize).max(2);
    let mut out = hits(envelope, onsets);
    let music = beats.first().zip(beats.last()).map(|(&a, &b)| (a, b));
    out.extend(breaks(envelope, onsets, music, most));
    out.extend(drops_and_builds(bars, energy, duration, most));
    out.sort_by_key(|e| (e.time_ms, e.kind as u8));
    out
}
