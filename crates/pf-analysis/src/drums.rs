//! The drums, hit by hit: where the percussive part of the song starts a new sound
//! ([`crate::layers`]), and which drum it is from how loud each drum band peaks (against how
//! loud the song's hits get there: their 90th percentile) and how long the cymbal band rings.
//!
//! | drum | what gives it away |
//! |---|---|
//! | kick | the 35–130 Hz band within 8 dB of the song's loud hits there |
//! | snare (or clap) | the 2–6 kHz rattle within 4 dB, with the 140–420 Hz body, and the cymbal band not much louder |
//! | crash | the cymbal band (6–16 kHz) within 3 dB, rising 6 dB or more over what was there, ringing over ⅓ s |
//! | hi-hat | the cymbal band within 15 dB, dying away sooner, not a snare |
//!
//! A hit can be more than one (a kick with a hi-hat). Every hit is kept for finding fills,
//! stops, and breakdowns; only the notable ones (crashes, and kicks and snares much harder than
//! those around them) are listed in the analysis, with each bar's counts ([`BarDrums`]).

use crate::layers::{BODY, CYMBAL, KICK, Layers, RATTLE};
use serde::Serialize;

/// How early (s) a hit may be and still count as on the bar line after it.
const EARLY_S: f64 = 0.05;

/// A drum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Drum {
    Kick,
    Snare,
    Hat,
    Crash,
}

impl Drum {
    pub fn word(self) -> &'static str {
        match self {
            Drum::Kick => "Kick",
            Drum::Snare => "Snare",
            Drum::Hat => "Hat",
            Drum::Crash => "Crash",
        }
    }
}

/// One drum hit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrumHit {
    pub time_ms: u64,
    pub drum: Drum,
    /// How hard, 0–1 (1 = as loud as that drum gets in the song).
    pub strength: f32,
}

/// How many of each drum a bar has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BarDrums {
    pub kick: u16,
    pub snare: u16,
    pub hat: u16,
    pub crash: u16,
}

/// A new percussive sound: when, how strong (its flux over the song's spread), how much each
/// drum band rose (dB), and the drums it was taken for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Onset {
    pub frame: usize,
    pub time_s: f64,
    pub flux: f32,
    pub rise: [f32; 4],
    /// Each band's peak less how loud the song's hits get there (dB; their 90th percentile is 0).
    pub level: [f32; 4],
    pub drums: Vec<(Drum, f32)>,
}

impl Onset {
    pub fn is(&self, drum: Drum) -> bool {
        self.drums.iter().any(|(d, _)| *d == drum)
    }

    /// Whether it's in the bar from `from` to `to` (s): a hit a touch early for the bar line is
    /// the bar's.
    pub fn in_bar(&self, from: f64, to: f64) -> bool {
        let t = self.time_s + EARLY_S;
        t >= from && t < to
    }

    /// A snare or a tom: a snare, or a loud body over the cymbals without a kick.
    pub fn is_snare_or_tom(&self) -> bool {
        self.is(Drum::Snare)
            || (self.level[BODY] >= -6.0 && self.level[BODY] >= self.level[CYMBAL] && !self.is(Drum::Kick))
    }

    /// How hard, 0–1: its loudest band against that band's loud level, and its flux.
    pub fn strength(&self) -> f32 {
        let loudest = self.level.iter().copied().fold(f32::MIN, f32::max);
        (0.5 * (loudest + 12.0) / 12.0 + 0.5 * self.flux / 10.0).clamp(0.0, 1.0)
    }

    /// Broadband: loud in every band.
    pub fn broadband(&self) -> bool {
        self.level.iter().all(|&l| l >= -8.0)
    }
}

/// Peaks of `values` (frames `frame_s` apart) standing out from the ~200 ms around them, at
/// least 50 ms apart, as (frame, value over the song's spread).
pub(crate) fn peaks(values: &[f32], frame_s: f64) -> Vec<(usize, f32)> {
    let n = values.len();
    if n == 0 {
        return Vec::new();
    }
    let mean = values.iter().map(|&x| f64::from(x)).sum::<f64>() / n as f64;
    let sd = (values.iter().map(|&x| (f64::from(x) - mean).powi(2)).sum::<f64>() / n as f64).sqrt();
    if sd < 1e-9 {
        return Vec::new();
    }
    let e: Vec<f32> = values.iter().map(|&x| (f64::from(x) / sd) as f32).collect();
    let frames = |s: f64| ((s / frame_s).round() as usize).max(1);
    let (around_max, around_mean, gap) = (frames(0.03), frames(0.1), frames(0.05));
    let mut prefix = vec![0.0f64; n + 1];
    for (i, &x) in e.iter().enumerate() {
        prefix[i + 1] = prefix[i] + f64::from(x);
    }
    let mut out: Vec<(usize, f32)> = Vec::new();
    for i in 0..n {
        let x = e[i];
        if x <= 0.0 {
            continue;
        }
        let (lo, hi) = (i.saturating_sub(around_max), (i + around_max + 1).min(n));
        if e[lo..hi].iter().any(|&y| y > x) {
            continue;
        }
        let (alo, ahi) = (i.saturating_sub(around_mean), (i + around_mean + 1).min(n));
        let local = ((prefix[ahi] - prefix[alo]) / (ahi - alo) as f64) as f32;
        if x < local + 0.5 || out.last().is_some_and(|&(last, _)| i - last < gap) {
            continue;
        }
        out.push((i, x));
    }
    out
}

/// The `p`th percentile of `values` (0 for none).
pub(crate) fn percentile(values: &[f32], p: usize) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    let at = (sorted.len() * p / 100).min(sorted.len() - 1);
    *sorted.select_nth_unstable_by(at, f32::total_cmp).1
}

/// How loud (dB under the song's loud hits in that band) each drum must be: the kick in the kick
/// band; a snare in the rattle band, with its body; a crash or a hi-hat in the cymbal band.
const KICK_DB: f32 = -8.0;
const SNARE_DB: f32 = -4.0;
const BODY_DB: f32 = -10.0;
const CRASH_DB: f32 = -3.0;
const HAT_DB: f32 = -15.0;
/// A snare's cymbal band is at most this much louder (against its loud level) than its rattle.
const HAT_OVER_DB: f32 = 6.0;
/// A crash rings at least this long (s); a hi-hat doesn't. Either rises at least this much (dB)
/// over what the cymbal band had before.
const CRASH_RING_S: f64 = 0.35;
const CYMBAL_RISE_DB: f32 = 6.0;

/// Every percussive onset in the song, with the drums it was taken for.
pub(crate) fn onsets(layers: &Layers) -> Vec<Onset> {
    let n = layers.len();
    if n == 0 {
        return Vec::new();
    }
    let (before, after) = (layers.frames_for(0.046), layers.frames_for(0.035));
    let ring_limit = layers.frames_for(1.5);
    // Each onset (with its peak in each band for its level, for now), and how long its cymbals
    // ring.
    let mut raw: Vec<(Onset, f64)> = peaks(&layers.percussive_flux, layers.frame_seconds())
        .into_iter()
        .map(|(f, flux)| {
            let pre: [f32; 4] = std::array::from_fn(|b| {
                (f.saturating_sub(before)..f)
                    .map(|i| layers.drums_db[i][b])
                    .fold(f32::MAX, f32::min)
            });
            let peak: [f32; 4] = std::array::from_fn(|b| {
                (f..(f + after).min(n))
                    .map(|i| layers.drums_db[i][b])
                    .fold(f32::MIN, f32::max)
            });
            let rise: [f32; 4] = std::array::from_fn(|b| peak[b] - pre[b].min(peak[b]).max(-100.0));
            // How long (s) the cymbal band rings.
            let ring_from = (f..(f + after).min(n))
                .max_by(|&x, &y| layers.cymbal_db[x].total_cmp(&layers.cymbal_db[y]))
                .unwrap_or(f);
            let top = layers.cymbal_db[ring_from];
            let under = (f.saturating_sub(before)..f)
                .map(|i| layers.cymbal_db[i])
                .fold(f32::MAX, f32::min)
                .min(top);
            // Until 10 dB under its peak, or halfway back down to what was there before (a hit
            // over a steady wash, a riser's noise, doesn't ring of its own).
            let ring = if top - under < CYMBAL_RISE_DB {
                0.0
            } else {
                let floor = (top - 10.0).max(under + 0.5 * (top - under));
                (ring_from..(ring_from + ring_limit).min(n))
                    .find(|&i| layers.cymbal_db[i] < floor)
                    .map_or(ring_limit, |i| i - ring_from) as f64
                    * layers.frame_seconds()
            };
            let onset = Onset {
                frame: f,
                time_s: layers.time_s(f),
                flux,
                rise,
                level: peak,
                drums: Vec::new(),
            };
            (onset, ring)
        })
        .collect();
    // How loud the song's hits get in each band.
    let loud: [f32; 4] =
        std::array::from_fn(|b| percentile(&raw.iter().map(|r| r.0.level[b]).collect::<Vec<_>>(), 90));
    for (onset, ring) in &mut raw {
        let ring = *ring;
        onset.level = std::array::from_fn(|b| onset.level[b] - loud[b]);
        let (level, rise) = (onset.level, onset.rise);
        let strength = |b: usize| ((level[b] + 12.0) / 12.0).clamp(0.0, 1.0);
        let rose = |b: usize| rise[b] >= 6.0;
        let mut drums = Vec::new();
        if rose(KICK) && level[KICK] >= KICK_DB {
            drums.push((Drum::Kick, strength(KICK)));
        }
        let snare = rose(RATTLE)
            && level[RATTLE] >= SNARE_DB
            && level[BODY] >= BODY_DB
            && level[CYMBAL] - level[RATTLE] <= HAT_OVER_DB;
        if snare {
            drums.push((Drum::Snare, strength(RATTLE)));
        }
        let crash =
            rose(CYMBAL) && level[CYMBAL] >= CRASH_DB && ring >= CRASH_RING_S && level[RATTLE] >= -10.0;
        if crash {
            drums.push((Drum::Crash, strength(CYMBAL)));
        } else if !snare && rose(CYMBAL) && level[CYMBAL] >= HAT_DB && ring < CRASH_RING_S {
            drums.push((Drum::Hat, strength(CYMBAL)));
        }
        onset.drums = drums;
    }
    raw.into_iter().map(|(onset, _)| onset).collect()
}

/// Each bar's drum counts (bars start at `bars`, s; the last runs to `end`).
pub(crate) fn bar_drums(onsets: &[Onset], bars: &[f64], end: f64) -> Vec<BarDrums> {
    (0..bars.len())
        .map(|i| {
            let (from, to) = (bars[i], bars.get(i + 1).copied().unwrap_or(end));
            let mut counts = BarDrums::default();
            for o in onsets.iter().filter(|o| o.in_bar(from, to)) {
                for (drum, _) in &o.drums {
                    let slot = match drum {
                        Drum::Kick => &mut counts.kick,
                        Drum::Snare => &mut counts.snare,
                        Drum::Hat => &mut counts.hat,
                        Drum::Crash => &mut counts.crash,
                    };
                    *slot = slot.saturating_add(1);
                }
            }
            counts
        })
        .collect()
}

/// Notable kicks and snares per minute, at most.
const ACCENTS_PER_MINUTE: f64 = 6.0;
/// Crashes listed per minute, at most, and how close together (s).
const CRASHES_PER_MINUTE: f64 = 6.0;
const CRASH_GAP_S: f64 = 1.0;

/// The drum hits worth listing: crashes, and kicks and snares at least 3 dB harder than most of
/// their kind in the 8 s around them; the strongest few per minute, in time order.
pub(crate) fn notable(onsets: &[Onset], duration_s: f64) -> Vec<DrumHit> {
    let minutes = (duration_s / 60.0).max(1.0 / 60.0);
    let hit = |o: &Onset, drum: Drum, strength: f32| DrumHit {
        time_ms: (o.time_s * 1000.0).round() as u64,
        drum,
        strength: (strength.clamp(0.0, 1.0) * 100.0).round() / 100.0,
    };
    let mut crashes: Vec<(f32, DrumHit)> = onsets
        .iter()
        .filter_map(|o| {
            o.drums
                .iter()
                .find(|(d, _)| *d == Drum::Crash)
                .map(|&(_, s)| (s, hit(o, Drum::Crash, s)))
        })
        .filter(|(s, _)| *s >= 0.5)
        .collect();
    crashes.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut kept: Vec<DrumHit> = Vec::new();
    for (_, c) in crashes {
        if kept.len() as f64 >= CRASHES_PER_MINUTE * minutes {
            break;
        }
        if kept
            .iter()
            .all(|k| (k.time_ms as f64 - c.time_ms as f64).abs() >= CRASH_GAP_S * 1000.0)
        {
            kept.push(c);
        }
    }
    let mut accents: Vec<(f32, DrumHit)> = Vec::new();
    for drum in [Drum::Kick, Drum::Snare] {
        let band = if drum == Drum::Kick { KICK } else { RATTLE };
        let of: Vec<&Onset> = onsets.iter().filter(|o| o.is(drum)).collect();
        let mut lo = 0;
        for (i, o) in of.iter().enumerate() {
            while of[lo].time_s < o.time_s - 4.0 {
                lo += 1;
            }
            let around: Vec<f32> = (lo..of.len())
                .take_while(|&j| of[j].time_s <= o.time_s + 4.0)
                .filter(|&j| j != i)
                .map(|j| of[j].level[band])
                .collect();
            if around.len() < 3 {
                continue;
            }
            let over = o.level[band] - percentile(&around, 75);
            let strength = o.drums.iter().find(|(d, _)| *d == drum).map_or(0.0, |d| d.1);
            if over >= 3.0 && strength >= 0.6 {
                accents.push((over, hit(o, drum, strength)));
            }
        }
    }
    accents.sort_by(|a, b| b.0.total_cmp(&a.0));
    accents.truncate((ACCENTS_PER_MINUTE * minutes).ceil() as usize);
    kept.extend(accents.into_iter().map(|(_, h)| h));
    kept.sort_by_key(|h| (h.time_ms, h.drum as u8));
    kept
}
