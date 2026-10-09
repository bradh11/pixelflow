//! Where the band drops out: stops (it cuts out for a moment, then comes back in), breakdowns
//! (the drums thin out or drop away for bars while the music goes on), and holds (a note or
//! chord held with little else going on).
//!
//! - **Stop**: between two drum hits, for ¼ beat or more once the first has died away, either
//!   everything falls 18 dB under the music around (**full**: silence), or the drums fall 10 dB
//!   under the drums around and 6 dB under the rest (**held**: a chord or a voice rings on), which
//!   also has to be a gap the drums don't usually leave (a beat or more, and over 2½ times the
//!   song's usual gap between hits). A full stop lasts up to 4 bars, a held one up to 2 (longer
//!   is a breakdown). The stop is at the last hit before the gap; the hit after it is a
//!   **restart**. Three or more stops a few bars apart, each a short stab then a gap, are one
//!   **stop-time** passage. The drums must have been playing (three hits in the bar before).
//! - **Breakdown**: two or more bars whose drums are 10 dB under the song's usual (or have a
//!   quarter of its kicks, and are 4 dB under) while the rest of the music is no more than
//!   15 dB under its usual, with drums before and after. Its depth is how far the drums fall.
//! - **Hold**: a bar or more where the drums are out, the sustained part plays steadily, and
//!   the harmony stays the same.

use crate::drums::{Onset, percentile};
use crate::moments::{Found, MomentKind, Song, mean_db};

/// A full stop: everything this far (dB) under the music around.
const STOP_DB: f32 = 18.0;
/// How far (dB) under its usual the sustained part may be in a held stop.
const HELD_DB: f32 = 15.0;
/// The drums are out (in a held stop or a hold): this far (dB) under the drums around, and this
/// far under the sustained part (a held chord leaks a little into the percussive part).
const DRUMS_OUT_DB: f32 = 10.0;
const UNDER_REST_DB: f32 = 6.0;
/// A breakdown: the drums this far (dB) under their usual, or a quarter of the kicks and this
/// far under.
const BREAKDOWN_DB: f32 = 10.0;
const THIN_DB: f32 = 4.0;
/// Stops at most this many bars apart can be one stop-time passage.
const STOP_TIME_BARS: f64 = 2.5;

/// A stop found between two hits (s), before stop-time passages are put together.
struct Gap {
    stop: f64,
    restart: f64,
    held: bool,
    strength: f32,
    restart_strength: f32,
}

/// Stops (and stop-time passages) and the restarts after them; also each stop's gap (from the
/// last hit before to the hit after), for shouts.
pub(crate) fn stops(song: &Song) -> (Vec<Found>, Vec<(f64, f64)>) {
    let l = song.layers;
    let levels = &song.levels;
    let hits: Vec<&Onset> = song.audible().collect();
    let gaps_between: Vec<f32> = hits
        .windows(2)
        .map(|w| (w[1].time_s - w[0].time_s) as f32)
        .collect();
    let usual = percentile(&gaps_between, 50) as f64;
    let shortest = (0.25 * song.beat).max(0.12);
    // Full stops: silence between any two sounds. Held stops: no drums between two drum hits.
    let drums: Vec<&Onset> = hits.iter().copied().filter(|o| !o.drums.is_empty()).collect();
    let mut gaps: Vec<Gap> = Vec::new();
    for (held, between) in [(false, &hits), (true, &drums)] {
        let longest = if held { 2.0 } else { 4.0 } * song.bar();
        for w in between.windows(2) {
            let (a, b) = (w[0], w[1]);
            let gap = b.time_s - a.time_s;
            if gap < shortest
                || gap > longest
                || gaps.iter().any(|g| g.stop < b.time_s && a.time_s < g.restart)
            {
                continue;
            }
            // Where the music around is playing, not an intro or a fade.
            let block = levels.at(a.frame);
            if levels.total[block] < levels.playing_total
                || levels.percussive[block] < levels.playing_percussive
            {
                continue;
            }
            // The gap, after the hit has had a moment to die away.
            let from = a.time_s + (0.3 * gap).min(0.15);
            let to = b.time_s - 0.02;
            if to - from < shortest {
                continue;
            }
            let total = song.mean_db(&l.total_db, from, to);
            let percussive = song.mean_db(&l.percussive_db, from, to);
            let harmonic = song.mean_db(&l.harmonic_db, from, to);
            let found = if held {
                percussive < levels.percussive[block] - DRUMS_OUT_DB
                    && percussive < harmonic - UNDER_REST_DB
                    && harmonic >= levels.harmonic[block] - HELD_DB
                    && gap >= song.beat.max(2.5 * usual)
            } else {
                total < levels.total[block] - STOP_DB
            };
            if !found {
                continue;
            }
            let depth = if held {
                (levels.percussive[block] - percussive) / 36.0
            } else {
                (levels.total[block] - total) / 36.0
            };
            let length = (gap / (2.0 * song.beat)).min(1.0) as f32;
            gaps.push(Gap {
                stop: a.time_s,
                restart: b.time_s,
                held,
                strength: (0.5 * depth.min(1.0) + 0.5 * length).clamp(0.0, 1.0),
                restart_strength: (0.5 + b.flux / 20.0).min(1.0),
            });
        }
    }
    gaps.sort_by(|a, b| a.stop.total_cmp(&b.stop));
    // The band was playing: three drum hits or more in the bar before (not an intro's lone hit).
    let playing_before = |t: f64| {
        drums
            .iter()
            .filter(|o| o.time_s >= t - song.bar() && o.time_s < t - 0.03)
            .count()
            >= 3
    };
    let spans = gaps.iter().map(|g| (g.stop, g.restart)).collect();
    // Stop-time: three or more stops close together, each re-entry a short stab.
    let mut found = Vec::new();
    let mut i = 0;
    while i < gaps.len() {
        let mut j = i;
        while j + 1 < gaps.len()
            && gaps[j + 1].stop - gaps[j].stop <= STOP_TIME_BARS * song.bar()
            && gaps[j + 1].stop - gaps[j].restart <= song.beat
        {
            j += 1;
        }
        if !playing_before(gaps[i].stop) {
            i = j + 1;
            continue;
        }
        if j - i >= 2 {
            let strongest = gaps[i..=j].iter().map(|g| g.strength).fold(0.0, f32::max);
            found.push(
                Found::new(MomentKind::Stop, gaps[i].stop, (strongest + 0.2).min(1.0))
                    .until(gaps[j].restart)
                    .labeled("stop-time"),
            );
            found.push(Found::new(
                MomentKind::Restart,
                gaps[j].restart,
                gaps[j].restart_strength,
            ));
        } else {
            for g in &gaps[i..=j] {
                found.push(
                    Found::new(MomentKind::Stop, g.stop, g.strength)
                        .until(g.restart)
                        .labeled(if g.held { "held" } else { "full" }),
                );
                found.push(Found::new(MomentKind::Restart, g.restart, g.restart_strength));
            }
        }
        i = j + 1;
    }
    (found, spans)
}

/// Breakdowns, with a restart where the drums come back.
pub(crate) fn breakdowns(song: &Song) -> Vec<Found> {
    let l = song.layers;
    let bars = song.bar_starts();
    let n = bars.len();
    if n < 4 {
        return Vec::new();
    }
    let span = |j: usize| (bars[j], bars.get(j + 1).copied().unwrap_or(song.end));
    let level = |series: &[f32], j: usize| {
        let (a, b) = span(j);
        song.mean_db(series, a, b)
    };
    let total: Vec<f32> = (0..n).map(|j| level(&l.total_db, j)).collect();
    let drums: Vec<f32> = (0..n).map(|j| level(&l.percussive_db, j)).collect();
    let rest: Vec<f32> = (0..n).map(|j| level(&l.harmonic_db, j)).collect();
    let kicks: Vec<f32> = (0..n)
        .map(|j| {
            let (a, b) = span(j);
            song.onsets
                .iter()
                .filter(|o| o.in_bar(a, b) && o.is(crate::Drum::Kick))
                .count() as f32
        })
        .collect();
    // The usual: the median of the bars where the music plays.
    let loud = percentile(&total, 50) - 12.0;
    let playing: Vec<usize> = (0..n).filter(|&j| total[j] >= loud).collect();
    let pick = |v: &[f32]| percentile(&playing.iter().map(|&j| v[j]).collect::<Vec<_>>(), 50);
    let (usual_drums, usual_rest, usual_kicks) = (pick(&drums), pick(&rest), pick(&kicks));
    let thin: Vec<bool> = (0..n)
        .map(|j| {
            let fewer_kicks = usual_kicks >= 2.0 && kicks[j] <= 0.25 * usual_kicks;
            let drums_down =
                drums[j] <= usual_drums - BREAKDOWN_DB || (fewer_kicks && drums[j] <= usual_drums - THIN_DB);
            drums_down && rest[j] >= usual_rest - 15.0
        })
        .collect();
    let has_drums = |j: usize| !thin[j] && drums[j] >= usual_drums - THIN_DB;
    let (Some(first), Some(last)) = (
        (0..n).find(|&j| has_drums(j)),
        (0..n).rev().find(|&j| has_drums(j)),
    ) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut j = first;
    while j < last {
        if !thin[j] {
            j += 1;
            continue;
        }
        let start = j;
        // A single bar back to the drums (a fill, a stab) doesn't end it.
        while j < last && (thin[j] || (j + 1 < last && thin[j + 1])) {
            j += 1;
        }
        let bars_long = j - start;
        if bars_long >= 2 {
            let mean = (start..j).map(|k| drums[k]).sum::<f32>() / bars_long as f32;
            let depth = ((usual_drums - mean) / 24.0).clamp(0.0, 1.0);
            let (from, to) = (bars[start], span(j - 1).1);
            found.push(
                Found::new(
                    MomentKind::Breakdown,
                    from,
                    0.6 * depth + 0.4 * (bars_long as f32 / 8.0).min(1.0),
                )
                .until(to)
                .labeled(format!("{bars_long} bars")),
            );
            // The drums back: the strongest hit within half a beat of the end.
            let back = song
                .audible()
                .filter(|o| (o.time_s - to).abs() <= 0.5 * song.beat)
                .max_by(|a, b| a.flux.total_cmp(&b.flux));
            let at = back.map_or(to, |o| o.time_s);
            found.push(Found::new(MomentKind::Restart, at, (0.4 + 0.6 * depth).min(1.0)));
        }
        j += 1;
    }
    found
}

/// Holds: a bar or more (1.5 s without a tempo) without a drum hit, where the sustained part
/// plays, stays within 3 dB, and is well over the percussive part, and the harmony stays the
/// same.
pub(crate) fn holds(song: &Song) -> Vec<Found> {
    let l = song.layers;
    let f = song.features;
    let levels = &song.levels;
    if l.is_empty() || f.is_empty() {
        return Vec::new();
    }
    let quarter = l.frames_for(0.25);
    let half_s = 0.5;
    let playing = percentile(&l.harmonic_db, 50) - 20.0;
    // Frames within 50 ms of a drum hit.
    let mut hit = vec![false; l.len()];
    let reach = l.frames_for(0.05);
    for o in song.audible().filter(|o| !o.drums.is_empty()) {
        hit[o.frame.saturating_sub(reach)..(o.frame + reach + 1).min(l.len())].fill(true);
    }
    let held: Vec<bool> = (0..l.len())
        .map(|i| {
            let t = l.time_s(i);
            let (now, then) = (f.frame_at(t), f.frame_at(t - half_s));
            let same_harmony = cosine(&f.chroma[now], &f.chroma[then]) >= 0.85;
            !hit[i]
                && l.percussive_db[i] < l.harmonic_db[i] - UNDER_REST_DB
                && l.harmonic_db[i] >= playing
                && i >= quarter
                && (l.harmonic_db[i] - l.harmonic_db[i - quarter]).abs() < 3.0
                && same_harmony
        })
        .collect();
    let shortest = if song.bars.len() >= 2 { song.bar() } else { 1.5 };
    let mut found = Vec::new();
    let mut i = 0;
    while i < held.len() {
        if !held[i] {
            i += 1;
            continue;
        }
        let start = i;
        // A flicker (a frame or two) doesn't end it; a drum hit does.
        let slack = 3;
        while i < held.len() && (held[i] || held[i..(i + slack).min(held.len())].iter().any(|&h| h)) {
            i += 1;
        }
        let (from, to) = (l.time_s(start) - 0.25, l.time_s(i - 1));
        if to - from >= shortest {
            // Nothing but silence after it.
            let at_end = song.end - to < 1.0
                || song.mean_db(&l.total_db, to + 0.3, song.end) < levels.playing_total - 10.0;
            let level = mean_db(&l.harmonic_db, start, i) - playing;
            let mut hold = Found::new(
                MomentKind::Hold,
                from.max(0.0),
                0.4 + 0.3 * ((to - from) / (2.0 * shortest)).min(1.0) as f32
                    + 0.3 * (level / 20.0).clamp(0.0, 1.0),
            )
            .until(to);
            if at_end {
                hold = hold.labeled("end");
            }
            found.push(hold);
        }
    }
    found
}

fn cosine(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm = |v: &[f32; 12]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
    let n = norm(a) * norm(b);
    if n < 1e-6 { 0.0 } else { dot / n }
}
