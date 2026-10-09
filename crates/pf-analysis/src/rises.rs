//! Where the music pushes: impacts, crashes, builds into them, fills, and peaks.
//!
//! - **Impact**: a broadband drum hit (loud in every drum band) or a crash that stands out: its
//!   flux over the strong hits' within 3 s (at least 1.2 but for a crash), plus a point per 3 dB
//!   (up to 9) the music is louder in the bar after than the bar before, a point for a crash,
//!   half for landing on a downbeat, and up to half for loud cymbals; 2½ points or more. Its
//!   strength is against the song's biggest.
//! - **Crash**: a crash cymbal, hard.
//! - **Build**: 1–8 bars over which brightness (the spectral centroid), the treble, snare and
//!   tom density, and loudness rise together (each against its own spread over the song), paired
//!   with the impact that resolves it: the strongest from a bar in to 1½ beats after its end,
//!   where it then ends.
//! - **Fill**: the last two beats before a downbeat with at least three snare or tom hits, and
//!   at least two more than the bars around have there, or three in places the two bars before
//!   left empty (on a sixteenth-note grid).
//! - **Peak**: the four bars that are loudest, brightest, and busiest together ("climax"), and
//!   each loud section's two busiest bars, where they stand out from the song or the section.

use crate::drums::{Drum, Onset, percentile};
use crate::layers::CYMBAL;
use crate::moments::{Found, MomentKind, Song};
use crate::sections::Level;

/// Impacts: how much an impact must stand out (see [`impacts`]): its score, and its flux over
/// the strong hits' around (unless it's a crash). The music's jump counts up to this (dB).
const IMPACT_SCORE: f32 = 2.5;
const STAND_OUT: f32 = 1.2;
const MOST_JUMP_DB: f32 = 9.0;

/// Impacts and crashes.
pub(crate) fn impacts(song: &Song) -> Vec<Found> {
    let l = song.layers;
    let hits: Vec<&Onset> = song.audible().collect();
    let mut found = Vec::new();
    let mut scored: Vec<(f64, f32)> = Vec::new();
    let mut lo = 0;
    for (k, o) in hits.iter().enumerate() {
        while hits[lo].time_s < o.time_s - 3.0 {
            lo += 1;
        }
        let crash = o.is(Drum::Crash);
        if crash {
            let strength = o.drums.iter().find(|d| d.0 == Drum::Crash).map_or(0.0, |d| d.1);
            if strength >= 0.6 {
                found.push(Found::new(MomentKind::Crash, o.time_s, strength));
            }
        }
        if !(crash || o.broadband()) {
            continue;
        }
        let around: Vec<f32> = (lo..hits.len())
            .take_while(|&m| hits[m].time_s <= o.time_s + 3.0)
            .filter(|&m| m != k)
            .map(|m| hits[m].flux)
            .collect();
        let usual = percentile(&around, 80).max(0.5);
        let span = song.bar().min(2.0);
        let jump = song.mean_db(&l.total_db, o.time_s, o.time_s + span)
            - song.mean_db(&l.total_db, o.time_s - span, o.time_s - 0.05);
        let on_downbeat = song.bars.iter().any(|&b| (b - o.time_s).abs() <= 0.07);
        let stands_out = o.flux / usual;
        // One hit like all the others (a click track's) isn't an impact, however much quieter
        // it was before it.
        if !crash && stands_out < STAND_OUT {
            continue;
        }
        let score = stands_out
            + jump.clamp(0.0, MOST_JUMP_DB) / 3.0
            + if crash { 1.0 } else { 0.0 }
            + if on_downbeat { 0.5 } else { 0.0 }
            + (o.level[CYMBAL] + 6.0).clamp(0.0, 6.0) / 12.0;
        if score >= IMPACT_SCORE {
            scored.push((o.time_s, score));
        }
    }
    // Strength against the strongest impact.
    let top = scored.iter().map(|s| s.1).fold(0.0, f32::max).max(1e-6);
    found.extend(
        scored
            .into_iter()
            .map(|(t, score)| Found::new(MomentKind::Impact, t, score / top)),
    );
    found
}

/// Per beat: how bright, how much treble, how many snare and tom hits, and how loud, each
/// scaled to the song's spread (z-scores), averaged.
fn lift(song: &Song, units: &[f64]) -> Vec<f32> {
    let l = song.layers;
    let f = song.features;
    let n = units.len();
    let span = |i: usize| (units[i], units.get(i + 1).copied().unwrap_or(song.end));
    let centroid: Vec<f32> = (0..n)
        .map(|i| {
            let (a, b) = span(i);
            let (x, y) = (l.frame_at(a), l.frame_at(b).max(l.frame_at(a) + 1).min(l.len()));
            let part: Vec<f32> = (x..y)
                .filter(|&k| l.total_db[k] > -60.0)
                .map(|k| l.centroid_hz[k])
                .collect();
            if part.is_empty() {
                0.0
            } else {
                part.iter().sum::<f32>() / part.len() as f32
            }
        })
        .collect();
    let treble: Vec<f32> = (0..n)
        .map(|i| {
            let (a, b) = span(i);
            let (x, y) = (f.frame_at(a), f.frame_at(b).max(f.frame_at(a) + 1).min(f.len()));
            crate::moments::mean_db(&f.bands_db.iter().map(|d| d[2]).collect::<Vec<_>>()[..], x, y)
        })
        .collect();
    let density: Vec<f32> = (0..n)
        .map(|i| {
            let (a, b) = span(i);
            song.onsets
                .iter()
                .filter(|o| o.time_s >= a && o.time_s < b && o.is_snare_or_tom())
                .count() as f32
        })
        .collect();
    let loud: Vec<f32> = (0..n)
        .map(|i| {
            let (a, b) = span(i);
            song.mean_db(&l.total_db, a, b)
        })
        .collect();
    // Against the song's spread, though at least `least` (a song that hardly changes doesn't
    // have its small changes blown up).
    let z = |v: &[f32], least: f32| -> Vec<f32> {
        let mean = v.iter().sum::<f32>() / v.len().max(1) as f32;
        let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / v.len().max(1) as f32).sqrt();
        v.iter().map(|x| (x - mean) / sd.max(least)).collect()
    };
    let parts = [
        z(&centroid, 150.0),
        z(&treble, 2.0),
        z(&density, 0.75),
        z(&loud, 2.0),
    ];
    let raw: Vec<f32> = (0..n)
        .map(|i| parts.iter().map(|p| p[i]).sum::<f32>() / 4.0)
        .collect();
    // Smoothed over two beats.
    (0..n)
        .map(|i| {
            let (a, b) = (i.saturating_sub(1), (i + 1).min(n));
            raw[a..b].iter().sum::<f32>() / (b - a) as f32
        })
        .collect()
}

/// Builds: from where the lift starts to climb to where it peaks, 1–8 bars, rising at least 1
/// (in spreads), the biggest first, not overlapping. Each paired with an impact from `impacts`
/// from a bar after its start to 1½ beats after its end.
pub(crate) fn builds(song: &Song, impacts: &[Found]) -> Vec<Found> {
    let units: Vec<f64> = if song.beats.len() >= 8 {
        song.beats.to_vec()
    } else {
        (0..)
            .map(|i| i as f64 * 0.5)
            .take_while(|&t| t < song.end)
            .collect()
    };
    let n = units.len();
    if n < 8 {
        return Vec::new();
    }
    let lift = lift(song, &units);
    let (shortest, longest) = (4, 32);
    let mut candidates: Vec<(usize, usize, f32)> = Vec::new();
    for start in (1..n - 1).filter(|&j| lift[j + 1] > lift[j] && lift[j] <= lift[j - 1]) {
        let mut stop = start;
        while stop + 1 < n && stop - start < longest && lift[stop + 1] >= lift[stop] - 0.25 {
            stop += 1;
        }
        stop = (start..=stop).fold(start, |best, j| if lift[j] > lift[best] { j } else { best });
        let gain = lift[stop] - lift[start];
        if stop - start >= shortest && gain >= 1.0 {
            candidates.push((start, stop, gain));
        }
    }
    candidates.sort_by(|a, b| b.2.total_cmp(&a.2));
    let mut kept: Vec<(usize, usize, f32)> = Vec::new();
    for c in candidates {
        if kept.iter().all(|k| c.1 <= k.0 || c.0 >= k.1) {
            kept.push(c);
        }
    }
    kept.into_iter()
        .map(|(start, stop, gain)| {
            // It ends where the beat after its peak starts.
            let end = units.get(stop + 1).copied().unwrap_or(song.end);
            // Resolved by the strongest impact from a bar in to just after its end (where it
            // then ends).
            let impact = impacts
                .iter()
                .filter(|i| {
                    i.kind == MomentKind::Impact
                        && i.at >= units[start] + song.bar()
                        && i.at <= end + 1.5 * song.beat
                })
                .max_by(|a, b| a.strength.total_cmp(&b.strength));
            let strength = (gain / 3.0).min(1.0);
            match impact {
                Some(i) => Found::new(MomentKind::Build, units[start], (strength + 0.2).min(1.0))
                    .until(i.at)
                    .labeled("into impact"),
                None => Found::new(MomentKind::Build, units[start], strength).until(end),
            }
        })
        .collect()
}

/// Fills before downbeats.
pub(crate) fn fills(song: &Song) -> Vec<Found> {
    let bars = song.bars;
    if bars.len() < 4 || song.beats.len() < 8 {
        return Vec::new();
    }
    let tail = 2.0 * song.beat;
    let sixteenth = song.beat / 4.0;
    let snares: Vec<&Onset> = song.onsets.iter().filter(|o| o.is_snare_or_tom()).collect();
    // Per bar: the snare and tom hits in its last two beats, as 16th-note slots.
    let tails: Vec<(f64, Vec<f64>, u16)> = (1..bars.len())
        .map(|j| {
            let to = bars[j];
            let from = to - tail;
            let times: Vec<f64> = snares
                .iter()
                .filter(|o| o.time_s >= from - 0.03 && o.time_s < to - 0.03)
                .map(|o| o.time_s)
                .collect();
            let slots = times
                .iter()
                .map(|t| 1u16 << (((t - from) / sixteenth).round().clamp(0.0, 7.0) as u16))
                .fold(0, |a, b| a | b);
            (to, times, slots)
        })
        .collect();
    let counts: Vec<f32> = tails.iter().map(|t| t.1.len() as f32).collect();
    let mut found = Vec::new();
    for (j, (to, times, slots)) in tails.iter().enumerate() {
        let count = times.len() as f32;
        if count < 3.0 {
            continue;
        }
        let around: Vec<f32> = (j.saturating_sub(4)..(j + 5).min(tails.len()))
            .filter(|&k| k != j)
            .map(|k| counts[k])
            .collect();
        let usual = percentile(&around, 50);
        let before = (1..=2)
            .filter_map(|d| j.checked_sub(d))
            .map(|k| tails[k].2)
            .fold(0, |a, b| a | b);
        let new = (slots & !before).count_ones() as f32;
        if count >= usual + 2.0 || new >= 3.0 {
            let strength = ((count - usual) / 6.0 + new / 8.0).clamp(0.0, 1.0);
            found.push(Found::new(MomentKind::Fill, times[0], strength).until(*to));
        }
    }
    found
}

/// How far (in intensity, 0–1) a peak stands over the song's usual (a section's peak, over half
/// this over the section's mean).
const PEAK_OVER: f32 = 0.1;

/// The climax and each loud section's peak.
pub(crate) fn peaks(song: &Song) -> Vec<Found> {
    let bars = song.bar_starts();
    let n = bars.len().min(song.bar_energy.len());
    if n < 4 {
        return Vec::new();
    }
    let intensity: Vec<f32> = (0..n)
        .map(|j| {
            let e = song.bar_energy[j];
            let d = song
                .bar_drums
                .get(j)
                .map_or(0.0, |d| f32::from(d.kick + d.snare) / 8.0);
            0.5 * e.overall + 0.2 * e.high + 0.3 * d.min(1.0)
        })
        .collect();
    let end_of = |j: usize| bars.get(j).copied().unwrap_or(song.end);
    let best = |from: usize, to: usize, width: usize| -> Option<(usize, f32)> {
        (from..=to.saturating_sub(width))
            .filter(|&j| j + width <= to)
            .map(|j| (j, intensity[j..j + width].iter().sum::<f32>() / width as f32))
            .max_by(|a, b| a.1.total_cmp(&b.1))
    };
    let mut found = Vec::new();
    // A song that's as full throughout has no climax.
    let usual = percentile(&intensity, 50);
    let Some((climax, top)) = best(0, n, 4).filter(|&(_, top)| top >= usual + PEAK_OVER) else {
        return found;
    };
    found.push(
        Found::new(MomentKind::Peak, bars[climax], top.max(0.5))
            .until(end_of(climax + 4))
            .labeled("climax"),
    );
    for s in song.sections.iter().filter(|s| s.level == Level::High) {
        let (a, b) = (s.start_ms as f64 / 1000.0, s.end_ms as f64 / 1000.0);
        let from = bars.partition_point(|&t| t < a - 0.05);
        let to = bars.partition_point(|&t| t < b - 0.05).min(n);
        if to < from + 4 || (from < climax + 4 && climax < to) {
            continue;
        }
        let mean = intensity[from..to].iter().sum::<f32>() / (to - from) as f32;
        if let Some((j, level)) = best(from, to, 2).filter(|&(_, level)| level >= mean + PEAK_OVER / 2.0) {
            found.push(
                Found::new(
                    MomentKind::Peak,
                    bars[j],
                    (0.5 * level + (level - mean).max(0.0) * 2.0).min(1.0),
                )
                .until(end_of(j + 2))
                .labeled(s.label.clone()),
            );
        }
    }
    found
}
