//! The moments that make a show dramatic, ranked: impacts, stops and restarts, breakdowns,
//! builds, fills, peaks, holds, key changes, shouts, drops, crashes, and section changes.
//!
//! Each detector ([`crate::stops`], [`crate::rises`], [`crate::key`], and shouts here) finds
//! moments with a strength (0–1, how clearly it is that kind of moment). They're then ranked
//! song-wide by **importance** (0–1): the strength, how much that kind of moment matters to a
//! show, how rare it is in this song, how much the music changes across it, and how near a
//! section boundary it falls. Moments of the same instant (an impact with its crash on a new
//! section) become one, a little more important. Each carries a treatment hint ([`Suggest`]).

use crate::drums::{Onset, percentile};
use crate::layers::Layers;
use crate::{Analysis, Event, EventKind, Section};
use pf_sequence::{Mark, TimingKind, TimingTrack};
use serde::Serialize;

/// What kind of moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MomentKind {
    /// A big hit: a broadband drum hit or crash with the music jumping up.
    Impact,
    /// The band cuts out (`label`: "full", "held", or "stop-time").
    Stop,
    /// The band comes back in after a stop or breakdown.
    Restart,
    /// The drums drop out or thin down while the music goes on.
    Breakdown,
    /// Brightness, noise, drum density, or loudness rising over 1–8 bars.
    Build,
    /// A drum fill leading into a downbeat.
    Fill,
    /// The loudest, busiest stretch (`label` "climax"), or a section's.
    Peak,
    /// A note or chord held with little else going on.
    Hold,
    KeyChange,
    /// A word or vocal burst that lands with the band.
    Shout,
    Drop,
    Crash,
    SectionChange,
}

impl MomentKind {
    pub fn word(self) -> &'static str {
        match self {
            MomentKind::Impact => "impact",
            MomentKind::Stop => "stop",
            MomentKind::Restart => "restart",
            MomentKind::Breakdown => "breakdown",
            MomentKind::Build => "build",
            MomentKind::Fill => "fill",
            MomentKind::Peak => "peak",
            MomentKind::Hold => "hold",
            MomentKind::KeyChange => "key_change",
            MomentKind::Shout => "shout",
            MomentKind::Drop => "drop",
            MomentKind::Crash => "crash",
            MomentKind::SectionChange => "section_change",
        }
    }

    /// How much this kind of moment matters to a show, 0–1.
    fn weight(self) -> f32 {
        match self {
            MomentKind::Stop | MomentKind::KeyChange | MomentKind::Drop | MomentKind::Shout => 0.9,
            MomentKind::Impact | MomentKind::Breakdown | MomentKind::Peak => 0.8,
            MomentKind::Restart | MomentKind::Build => 0.7,
            MomentKind::Hold => 0.6,
            MomentKind::Fill | MomentKind::Crash | MomentKind::SectionChange => 0.5,
        }
    }

    /// How lights might treat it.
    pub fn suggest(self) -> Suggest {
        match self {
            MomentKind::Impact => Suggest::Hit,
            MomentKind::Stop => Suggest::Blackout,
            MomentKind::Restart | MomentKind::Drop => Suggest::Burst,
            MomentKind::Breakdown => Suggest::Minimal,
            MomentKind::Build => Suggest::Ramp,
            MomentKind::Fill => Suggest::Chase,
            MomentKind::Peak => Suggest::Full,
            MomentKind::Hold => Suggest::Sustain,
            MomentKind::KeyChange => Suggest::ColorShift,
            MomentKind::Shout => Suggest::WordPop,
            MomentKind::Crash => Suggest::Flash,
            MomentKind::SectionChange => Suggest::Change,
        }
    }
}

/// A treatment hint for a moment, from a fixed vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Suggest {
    /// Everything hits at once.
    Hit,
    /// Lights out (or nearly) until it comes back.
    Blackout,
    /// Everything back in at once.
    Burst,
    /// Few props, soft and slow.
    Minimal,
    /// Brightness and speed climbing to the end.
    Ramp,
    /// A quick run across props.
    Chase,
    /// Every prop, full and fast.
    Full,
    /// Hold a look steady.
    Sustain,
    /// A new palette from here.
    ColorShift,
    /// A prop pops with the word.
    WordPop,
    /// A white flash.
    Flash,
    /// A new look from here.
    Change,
}

impl Suggest {
    pub fn word(self) -> &'static str {
        match self {
            Suggest::Hit => "hit",
            Suggest::Blackout => "blackout",
            Suggest::Burst => "burst",
            Suggest::Minimal => "minimal",
            Suggest::Ramp => "ramp",
            Suggest::Chase => "chase",
            Suggest::Full => "full",
            Suggest::Sustain => "sustain",
            Suggest::ColorShift => "color-shift",
            Suggest::WordPop => "word-pop",
            Suggest::Flash => "flash",
            Suggest::Change => "change",
        }
    }
}

/// A moment in the song.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Moment {
    pub time_ms: u64,
    /// Where it ends, for one that lasts (a breakdown, a build, a stop's gap).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ms: Option<u64>,
    pub kind: MomentKind,
    /// How clearly it is this kind of moment, 0–1.
    pub strength: f32,
    /// How much it matters in this song, 0–1 (see the module docs).
    pub importance: f32,
    /// A word, a section's name, a key change ("C→D"), or a kind of stop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub suggest: Suggest,
}

/// A moment as a detector finds it (times in s).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Found {
    pub at: f64,
    pub end: Option<f64>,
    pub kind: MomentKind,
    pub strength: f32,
    pub label: Option<String>,
}

impl Found {
    pub fn new(kind: MomentKind, at: f64, strength: f32) -> Self {
        Self {
            at,
            end: None,
            kind,
            strength: strength.clamp(0.0, 1.0),
            label: None,
        }
    }

    pub fn until(mut self, end: f64) -> Self {
        self.end = Some(end.max(self.at));
        self
    }

    pub fn labeled(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// What the detectors work from (times in s).
pub(crate) struct Song<'a> {
    pub layers: &'a Layers,
    pub features: &'a crate::features::Features,
    /// Every percussive onset.
    pub onsets: &'a [Onset],
    pub beats: &'a [f64],
    pub bars: &'a [f64],
    pub sections: &'a [Section],
    pub bar_drums: &'a [crate::BarDrums],
    pub bar_energy: &'a [crate::BarEnergy],
    pub end: f64,
    /// Seconds per beat (0.5 without a tempo).
    pub beat: f64,
    pub levels: Levels,
}

impl Song<'_> {
    /// Bar starts, or every 2 s without bars.
    pub fn bar_starts(&self) -> Vec<f64> {
        if self.bars.len() >= 2 {
            self.bars.to_vec()
        } else {
            (0..)
                .map(|i| i as f64 * 2.0)
                .take_while(|&t| t < self.end)
                .collect()
        }
    }

    /// Seconds per bar.
    pub fn bar(&self) -> f64 {
        4.0 * self.beat
    }

    /// The power mean (dB) of `series` over `from..to` (s).
    pub fn mean_db(&self, series: &[f32], from: f64, to: f64) -> f32 {
        let l = self.layers;
        let (a, b) = (l.frame_at(from.max(0.0)), l.frame_at(to.max(0.0)));
        mean_db(series, a, b.max(a + 1).min(series.len()))
    }

    /// The audible percussive onsets (within 30 dB of their band's loud level).
    pub fn audible(&self) -> impl Iterator<Item = &Onset> {
        self.onsets
            .iter()
            .filter(|o| o.level.iter().copied().fold(f32::MIN, f32::max) >= -30.0)
    }
}

/// dB of the mean power of `series[from..to]` (-100 for none).
pub(crate) fn mean_db(series: &[f32], from: usize, to: usize) -> f32 {
    let part = series.get(from..to.min(series.len())).unwrap_or_default();
    if part.is_empty() {
        return -100.0;
    }
    let sum: f64 = part.iter().map(|&d| 10f64.powf(f64::from(d) / 10.0)).sum();
    (10.0 * (sum / part.len() as f64).max(1e-10).log10()) as f32
}

/// How loud the music around each moment is, to compare a moment with: per half second, the
/// 90th percentile of each layer's level, and the 75th percentile of those over the 16 s
/// around (so a passage of stops is still measured against the music, not its own gaps).
pub(crate) struct Levels {
    block: usize,
    pub total: Vec<f32>,
    pub percussive: Vec<f32>,
    pub harmonic: Vec<f32>,
    /// The song's typical playing level (the median block), less 15 dB: quieter than that around
    /// is an intro, a fade, or a quiet song's quiet part.
    pub playing_total: f32,
    pub playing_percussive: f32,
}

impl Levels {
    pub fn new(layers: &Layers) -> Self {
        let block = layers.frames_for(0.5);
        let around = |series: &[f32]| -> (Vec<f32>, f32) {
            let blocks: Vec<f32> = series.chunks(block).map(|c| percentile(c, 90)).collect();
            let around = (0..blocks.len())
                .map(|b| percentile(&blocks[b.saturating_sub(16)..(b + 17).min(blocks.len())], 75))
                .collect();
            (around, percentile(&blocks, 50) - 15.0)
        };
        let (total, playing_total) = around(&layers.total_db);
        let (percussive, playing_percussive) = around(&layers.percussive_db);
        let (harmonic, _) = around(&layers.harmonic_db);
        Self {
            block,
            total,
            percussive,
            harmonic,
            playing_total,
            playing_percussive,
        }
    }

    /// The block frame `f` is in.
    pub fn at(&self, f: usize) -> usize {
        (f / self.block.max(1)).min(self.total.len().saturating_sub(1))
    }
}

/// Moments of these kinds within this long (s) of each other are one.
const SAME_INSTANT_S: f64 = 0.15;
const MERGED: [MomentKind; 5] = [
    MomentKind::Impact,
    MomentKind::Crash,
    MomentKind::SectionChange,
    MomentKind::Drop,
    MomentKind::Restart,
];
/// Moments of a kind kept per minute of song, at most (the strongest).
fn per_minute(kind: MomentKind) -> f64 {
    match kind {
        MomentKind::Impact | MomentKind::Crash | MomentKind::Stop => 4.0,
        MomentKind::Fill | MomentKind::Restart | MomentKind::Shout => 3.0,
        MomentKind::Hold | MomentKind::Build | MomentKind::Breakdown | MomentKind::Drop => 2.0,
        MomentKind::Peak | MomentKind::KeyChange | MomentKind::SectionChange => 100.0,
    }
}

/// The existing accents as moments: hits as impacts, drops, breaks as stops, and builds.
pub(crate) fn from_events(events: &[Event]) -> Vec<Found> {
    events
        .iter()
        .map(|e| {
            let at = e.time_ms as f64 / 1000.0;
            let kind = match e.kind {
                EventKind::Hit => MomentKind::Impact,
                EventKind::Drop => MomentKind::Drop,
                EventKind::Break => MomentKind::Stop,
                EventKind::Build => MomentKind::Build,
            };
            // A hit is a little less sure to be an impact than one found as such.
            let strength = if kind == MomentKind::Impact {
                0.8 * e.strength
            } else {
                e.strength
            };
            let mut found = Found::new(kind, at, strength);
            if kind == MomentKind::Stop {
                found = found.labeled("full");
            }
            match e.duration_ms {
                Some(d) => found.until(at + d as f64 / 1000.0),
                None => found,
            }
        })
        .collect()
}

/// Whether two moments overlap in time (a point counts as `SAME_INSTANT_S` long).
fn overlap(a: &Found, b: &Found) -> bool {
    let span = |f: &Found| {
        (
            f.at,
            f.end.unwrap_or(f.at + SAME_INSTANT_S).max(f.at + SAME_INSTANT_S),
        )
    };
    let (a, b) = (span(a), span(b));
    a.0 < b.1 && b.0 < a.1
}

/// `found` (detected, then mapped from the accents: those duplicate a detected moment of the same
/// kind are left out) ranked into moments, in time order.
pub(crate) fn rank(detected: Vec<Found>, mapped: Vec<Found>, analysis: &Analysis) -> Vec<Moment> {
    let mut found = detected;
    for m in mapped {
        if !found.iter().any(|f| f.kind == m.kind && overlap(f, &m)) {
            found.push(m);
        }
    }
    let duration = analysis.duration_ms as f64 / 1000.0;
    let minutes = duration / 60.0;
    // The strongest few of each kind per minute (as many as a minute's for a shorter song).
    found.sort_by(|a, b| b.strength.total_cmp(&a.strength));
    let mut counts: Vec<(MomentKind, usize)> = Vec::new();
    found.retain(|f| {
        let slot = match counts.iter_mut().find(|(k, _)| *k == f.kind) {
            Some(slot) => slot,
            None => {
                counts.push((f.kind, 0));
                counts.last_mut().unwrap_or_else(|| unreachable!())
            }
        };
        slot.1 += 1;
        slot.1 as f64 <= (per_minute(f.kind) * minutes.max(1.0)).ceil()
    });
    let count = |kind: MomentKind| counts.iter().find(|(k, _)| *k == kind).map_or(0, |c| c.1) as f64;
    let beat = analysis
        .tempo_bpm
        .filter(|t| *t > 0.0)
        .map_or(0.5, |t| 60.0 / f64::from(t));
    let boundaries: Vec<f64> = analysis
        .sections()
        .iter()
        .skip(1)
        .map(|s| s.start_ms as f64 / 1000.0)
        .collect();
    let energy = |from: f64, to: f64| -> Option<f32> {
        let (a, b) = (
            from.max(0.0) as usize,
            (to.max(0.0).ceil() as usize).min(analysis.energy.len()),
        );
        (b > a).then(|| analysis.energy[a..b].iter().sum::<f32>() / (b - a) as f32)
    };
    let mut ranked: Vec<(Found, f32)> = found
        .into_iter()
        .map(|f| {
            // A hook is shouted again and again: that's what makes it one.
            let rarity = if f.kind == MomentKind::Shout {
                0.5
            } else {
                1.0 - ((count(f.kind) / minutes.max(0.5)) / 4.0).min(1.0) as f32
            };
            let contrast = match f.end {
                // A span: inside it against before it.
                Some(end) if end - f.at > 2.0 * beat => energy(f.at - 4.0, f.at)
                    .zip(energy(f.at, end.min(f.at + 4.0)))
                    .map_or(0.0, |(b, a)| ((b - a).abs() / 0.4).min(1.0)),
                _ => energy(f.at - 2.0, f.at)
                    .zip(energy(f.at, f.at + 2.0))
                    .map_or(0.0, |(b, a)| ((a - b).abs() / 0.4).min(1.0)),
            };
            // A fill leads into a boundary; everything else starts on one.
            let at = if f.kind == MomentKind::Fill {
                f.end.unwrap_or(f.at)
            } else {
                f.at
            };
            let near = boundaries
                .iter()
                .map(|&b| (b - at).abs())
                .fold(f64::MAX, f64::min);
            let boundary = (1.0 - ((near - 0.5 * beat) / (1.5 * beat)).max(0.0)).clamp(0.0, 1.0) as f32;
            let importance =
                0.35 * f.strength + 0.2 * f.kind.weight() + 0.15 * rarity + 0.15 * contrast + 0.15 * boundary;
            (f, importance)
        })
        .collect();
    // One moment per instant: the kind that matters most (a drop over an impact over a crash),
    // as important as the most important of them and a little more for each it stands for.
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut kept: Vec<(Found, f32)> = Vec::new();
    for (f, importance) in ranked {
        let same = MERGED.contains(&f.kind).then(|| {
            kept.iter_mut().find(|(k, _)| {
                MERGED.contains(&k.kind)
                    && k.end.is_none()
                    && f.end.is_none()
                    && (k.at - f.at).abs() <= SAME_INSTANT_S
            })
        });
        match same.flatten() {
            Some((k, i)) => {
                *i += 0.05;
                let label = k.label.take().or(f.label.clone());
                if f.kind.weight() > k.kind.weight() {
                    *k = f;
                }
                k.label = label;
            }
            None => kept.push((f, importance)),
        }
    }
    let ms = |s: f64| (s.max(0.0) * 1000.0).round() as u64;
    let round2 = |x: f32| (x.clamp(0.0, 1.0) * 100.0).round() / 100.0;
    let mut moments: Vec<Moment> = kept
        .into_iter()
        .map(|(f, importance)| Moment {
            time_ms: ms(f.at).min(analysis.duration_ms),
            end_ms: f.end.map(|e| ms(e).min(analysis.duration_ms)),
            kind: f.kind,
            strength: round2(f.strength),
            importance: round2(importance),
            label: f.label,
            suggest: f.kind.suggest(),
        })
        .collect();
    moments.sort_by_key(|m| (m.time_ms, m.kind as u8));
    moments
}

/// Section changes: every section start but the first, as strong as the energy changes there
/// and as sure as the sections are.
pub(crate) fn section_changes(sections: &[Section]) -> Vec<Found> {
    sections
        .windows(2)
        .map(|w| {
            let change = (w[1].energy - w[0].energy).abs();
            Found::new(
                MomentKind::SectionChange,
                w[1].start_ms as f64 / 1000.0,
                0.5 * (change / 0.4).min(1.0) + 0.5 * w[1].confidence,
            )
            .labeled(w[1].label.clone())
        })
        .collect()
}

/// What a song's shouts are found from, kept with the analysis for lyrics that come later.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShoutCues {
    /// Seconds between `voice_db` values, and the first one's time (s).
    pub frame_s: f64,
    pub offset_s: f64,
    /// The power (dB) where the voice sits, of the song's sustained (harmonic) part.
    pub voice_db: Vec<f32>,
    /// Audible drum hits: time (s) and strength (0–1).
    pub hits: Vec<(f64, f32)>,
    /// Stops: from the last hit before to the hit after (s).
    pub gaps: Vec<(f64, f64)>,
}

impl ShoutCues {
    fn voice(&self, from: f64, to: f64) -> f32 {
        if self.frame_s <= 0.0 {
            return -100.0;
        }
        let frame = |s: f64| ((s - self.offset_s) / self.frame_s).round().max(0.0) as usize;
        let (a, b) = (frame(from), frame(to));
        mean_db(&self.voice_db, a, b.max(a + 1))
    }

    /// How much the voice band jumps at `t` (dB): the 0.2 s after against the 0.4 s before.
    fn burst(&self, t: f64) -> f32 {
        self.voice(t, t + 0.2) - self.voice(t - 0.45, t - 0.05)
    }

    /// The strongest hit within `reach` (s) of `t`.
    fn hit_near(&self, t: f64, reach: f64) -> f32 {
        let from = self.hits.partition_point(|h| h.0 < t - reach);
        self.hits[from..]
            .iter()
            .take_while(|h| h.0 <= t + reach)
            .map(|h| h.1)
            .fold(0.0, f32::max)
    }

    fn at_stop(&self, t: f64) -> bool {
        self.gaps.iter().any(|&(a, b)| t >= a - 0.15 && t <= b + 0.15)
    }
}

/// Shouts without lyrics: strong drum hits where the voice band jumps at least 8 dB and stands
/// out from the voice around it; the clearest two per minute.
pub(crate) fn vocal_bursts(cues: &ShoutCues, duration: f64) -> Vec<Found> {
    let loud = percentile(&cues.voice_db, 80);
    let mut found: Vec<Found> = cues
        .hits
        .iter()
        .filter(|h| h.1 >= 0.6)
        .filter_map(|&(t, s)| {
            let burst = cues.burst(t);
            (burst >= 8.0 && cues.voice(t, t + 0.25) >= loud)
                .then(|| Found::new(MomentKind::Shout, t, 0.4 * s + 0.3 * (burst / 16.0).min(1.0)))
        })
        .collect();
    found.sort_by(|a, b| b.strength.total_cmp(&a.strength));
    let mut kept: Vec<Found> = Vec::new();
    for f in found {
        if kept.len() as f64 >= (2.0 * (duration / 60.0).max(1.0)).ceil() {
            break;
        }
        if kept.iter().all(|k| (k.at - f.at).abs() > 2.0) {
            kept.push(f);
        }
    }
    kept
}

/// Words too common to be a hook.
const COMMON_WORDS: &[&str] = &[
    "the", "and", "you", "your", "are", "for", "with", "that", "this", "but", "not", "all", "can", "was",
    "were", "have", "has", "what", "who", "when", "where", "why", "how", "gonna", "wanna", "gotta", "got",
    "get", "its", "it's", "i'm", "don't", "can't", "won't", "ain't", "she", "her", "him", "his", "they",
    "them", "our", "out", "now", "just", "like", "from", "into", "there", "then", "than", "yeah", "ooh",
    "oh", "ah", "uh", "hey", "baby", "know", "say", "said", "one", "will", "would", "could", "let", "come",
    "came", "see", "way", "too", "yes", "own", "off", "over", "some", "she's", "he's", "you're", "we're",
    "they're", "i'll", "you'll", "i've",
];

/// A word as compared: lowercase letters, digits, and apostrophes.
fn plain(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric() || *c == '\'')
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .trim_matches('\'')
        .to_string()
}

/// The word as shown: without the punctuation around it, capitalized.
fn shown(word: &str) -> String {
    let word = word.trim_matches(|c: char| !c.is_alphanumeric());
    let mut chars = word.chars();
    chars
        .next()
        .map_or(String::new(), |first| first.to_uppercase().chain(chars).collect())
}

/// Shouts from the song's sung words (`words`, labeled marks; `lines`, the lyric lines, to tell a
/// word sung on its own): the words repeated most (the hook, and the title's words) and words on
/// their own, scored by whether they land with a drum hit or in a stop, the voice jumping, and
/// an exclamation mark; the best three per minute, at least 2 s apart.
pub(crate) fn shouts_from_words(
    cues: &ShoutCues,
    words: &[Mark],
    lines: &[Mark],
    title: Option<&str>,
    duration: f64,
) -> Vec<Found> {
    let title: Vec<String> = title
        .map(|t| t.split_whitespace().map(plain).filter(|w| w.len() >= 3).collect())
        .unwrap_or_default();
    let content = |w: &str| w.chars().count() >= 3 && !COMMON_WORDS.contains(&w);
    let mut counts: Vec<(String, usize)> = Vec::new();
    for m in words {
        let w = plain(&m.label);
        if !content(&w) {
            continue;
        }
        match counts.iter_mut().find(|(c, _)| *c == w) {
            Some((_, n)) => *n += 1,
            None => counts.push((w, 1)),
        }
    }
    let most = counts.iter().map(|c| c.1).max().unwrap_or(1).max(2) as f32;
    let words_in_line = |t: u64| {
        lines
            .iter()
            .find(|l| t >= l.start_ms && t < l.end_ms.max(l.start_ms + 1))
            .map(|l| l.label.split_whitespace().count())
    };
    let mut found: Vec<Found> = words
        .iter()
        .filter_map(|m| {
            let w = plain(&m.label);
            if !content(&w) {
                return None;
            }
            let repeats = counts.iter().find(|(c, _)| *c == w).map_or(1, |c| c.1);
            let hook = if repeats >= 3 { repeats as f32 / most } else { 0.0 };
            let in_title = title.contains(&w);
            let alone = words_in_line(m.start_ms).is_some_and(|n| n <= 2);
            if hook < 0.3 && !in_title && !alone {
                return None;
            }
            let t = m.start_ms as f64 / 1000.0;
            let hit = cues.hit_near(t, 0.1);
            let stop = cues.at_stop(t);
            let burst = (cues.burst(t) / 12.0).clamp(0.0, 1.0);
            let exclaimed = m.label.trim_end().ends_with('!');
            let score = 0.25 * hook
                + 0.2 * hit
                + 0.15 * f32::from(u8::from(stop))
                + 0.15 * burst
                + 0.15 * f32::from(u8::from(alone))
                + 0.05 * f32::from(u8::from(exclaimed))
                + 0.1 * f32::from(u8::from(in_title));
            (score >= 0.4).then(|| Found::new(MomentKind::Shout, t, score.min(1.0)).labeled(shown(&m.label)))
        })
        .collect();
    // The best few per minute, at least 2 s apart (a hook sung twice in a row is one shout).
    found.sort_by(|a, b| b.strength.total_cmp(&a.strength));
    let mut kept: Vec<Found> = Vec::new();
    for f in found {
        if kept.len() as f64 >= (3.0 * (duration / 60.0).max(1.0)).ceil() {
            break;
        }
        if kept.iter().all(|k| (k.at - f.at).abs() >= 2.0) {
            kept.push(f);
        }
    }
    kept
}

impl Analysis {
    /// The moments with shouts from the song's sung words (`words`: a words track's marks;
    /// `lines`: its lyric lines; `title`: the song's title, if known) in place of those found
    /// without them, ranked again.
    pub fn moments_with_words(&self, words: &[Mark], lines: &[Mark], title: Option<&str>) -> Vec<Moment> {
        let duration = self.duration_ms as f64 / 1000.0;
        let shouts = shouts_from_words(&self.shout_cues, words, lines, title, duration);
        if shouts.is_empty() {
            return self.moments.clone();
        }
        let mut found: Vec<Found> = self
            .moments
            .iter()
            .filter(|m| m.kind != MomentKind::Shout)
            .map(|m| Found {
                at: m.time_ms as f64 / 1000.0,
                end: m.end_ms.map(|e| e as f64 / 1000.0),
                kind: m.kind,
                strength: m.strength,
                label: m.label.clone(),
            })
            .collect();
        found.extend(shouts);
        rank(found, Vec::new(), self)
    }

    /// The most important `most` moments, most important first.
    pub fn top_moments(&self, most: usize) -> Vec<&Moment> {
        let mut top: Vec<&Moment> = self.moments.iter().collect();
        top.sort_by(|a, b| {
            b.importance
                .total_cmp(&a.importance)
                .then(a.time_ms.cmp(&b.time_ms))
        });
        top.truncate(most);
        top
    }

    /// The beat (ms), as the accents track has it.
    fn beat_ms(&self) -> u64 {
        self.tempo_bpm
            .filter(|t| *t > 0.0)
            .map_or(500, |t| (60_000.0 / t).round() as u64)
            .clamp(100, 1000)
    }

    /// The moments as a "Moments" timing track, labeled with their kind (and label: "Shout:
    /// Ghostbusters"): the most important first, each where nothing more important is; one that
    /// lasts runs on until something more important, one that doesn't for half a beat.
    pub fn moments_track(&self) -> TimingTrack {
        moments_track(&self.moments, self.beat_ms(), self.duration_ms)
    }

    /// The notable drum hits as a "Drums" timing track (Kick, Snare, Crash), each a quarter of a
    /// beat (cut short by the next).
    pub fn drums_track(&self) -> TimingTrack {
        let len = (self.beat_ms() / 4).max(40);
        let hits: Vec<_> = self.drums.iter().filter(|h| h.drum != crate::Drum::Hat).collect();
        let mut marks: Vec<Mark> = Vec::new();
        for (i, h) in hits.iter().enumerate() {
            if marks.last().is_some_and(|m| m.start_ms == h.time_ms) {
                continue;
            }
            let next = hits[i + 1..]
                .iter()
                .map(|n| n.time_ms)
                .find(|&t| t > h.time_ms)
                .unwrap_or(u64::MAX);
            let end = (h.time_ms + len)
                .min(next)
                .min(self.duration_ms.max(h.time_ms + 1));
            if end > h.time_ms {
                marks.push(Mark::new(h.time_ms, end, h.drum.word()));
            }
        }
        TimingTrack::new("Drums", TimingKind::Custom, marks)
    }
}

/// Moments to show on the Moments track, per minute of song at most.
const TRACK_MOMENTS_PER_MINUTE: f64 = 8.0;

/// See [`Analysis::moments_track`] (`moments` with the song's beat and length, ms).
pub fn moments_track(moments: &[Moment], beat_ms: u64, duration_ms: u64) -> TimingTrack {
    let most = ((duration_ms as f64 / 60_000.0) * TRACK_MOMENTS_PER_MINUTE).ceil() as usize;
    let mut order: Vec<&Moment> = moments.iter().collect();
    order.sort_by(|a, b| {
        b.importance
            .total_cmp(&a.importance)
            .then(a.time_ms.cmp(&b.time_ms))
    });
    let mut taken: Vec<Mark> = Vec::new();
    for m in order.into_iter().take(most.max(1)) {
        let start = m.time_ms;
        let wanted = m
            .end_ms
            .filter(|&e| e > start)
            .unwrap_or(start + (beat_ms / 2).max(1));
        // Up to the next mark already taken; nothing if it starts inside one.
        if taken.iter().any(|t| start >= t.start_ms && start < t.end_ms) {
            continue;
        }
        let end = taken
            .iter()
            .map(|t| t.start_ms)
            .filter(|&s| s > start)
            .fold(wanted, u64::min)
            .min(duration_ms.max(start + 1));
        if end <= start {
            continue;
        }
        let kind = m.kind.word().replace('_', " ");
        let mut label: String = kind[..1].to_uppercase() + &kind[1..];
        if let Some(l) = &m.label {
            label = format!("{label}: {l}");
        }
        taken.push(Mark::new(start, end, label));
    }
    taken.sort_by_key(|m| m.start_ms);
    TimingTrack::new("Moments", TimingKind::Custom, taken)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moment(time_ms: u64, end_ms: Option<u64>, kind: MomentKind, importance: f32) -> Moment {
        Moment {
            time_ms,
            end_ms,
            kind,
            strength: importance,
            importance,
            label: (kind == MomentKind::Shout).then(|| "Ghostbusters".to_string()),
            suggest: kind.suggest(),
        }
    }

    #[test]
    fn the_moments_track_keeps_the_most_important_where_they_overlap() {
        let moments = [
            moment(10_000, Some(20_000), MomentKind::Breakdown, 0.6),
            moment(12_000, None, MomentKind::Shout, 0.9),
            moment(15_000, None, MomentKind::Fill, 0.3),
            moment(30_100, None, MomentKind::Crash, 0.2),
            moment(30_000, None, MomentKind::KeyChange, 0.8),
        ];
        let track = moments_track(&moments, 500, 60_000);
        let marks: Vec<(u64, u64, &str)> = track
            .marks
            .iter()
            .map(|m| (m.start_ms, m.end_ms, m.label.as_str()))
            .collect();
        assert_eq!(
            marks,
            [
                (10_000, 12_000, "Breakdown"),
                (12_000, 12_250, "Shout: Ghostbusters"),
                (15_000, 15_250, "Fill"),
                (30_000, 30_250, "Key change"),
            ]
        );
    }

    #[test]
    fn words_that_land_with_the_band_are_shouts() {
        // Hits every 2 s; the voice jumps at 8 s.
        let frame_s = 0.0116;
        let voice_db: Vec<f32> = (0..2000)
            .map(|i| {
                if (8.0..9.0).contains(&(i as f64 * frame_s)) {
                    -10.0
                } else {
                    -40.0
                }
            })
            .collect();
        let cues = ShoutCues {
            frame_s,
            offset_s: 0.0,
            voice_db,
            hits: (0..10).map(|i| (i as f64 * 2.0, 0.9)).collect(),
            gaps: Vec::new(),
        };
        let word = |ms: u64, w: &str| Mark::new(ms, ms + 300, w);
        let words = [
            word(1_000, "who"),
            word(1_300, "lights"),
            word(1_600, "the"),
            word(1_900, "porch?"),
            word(4_000, "Snowblasters!"),
            word(6_000, "Snowblasters!"),
            word(8_000, "Snowblasters!"),
            word(10_500, "quiet"),
        ];
        let lines = [
            Mark::new(1_000, 2_200, "Who lights the porch?"),
            Mark::new(4_000, 4_500, "Snowblasters!"),
            Mark::new(6_000, 6_500, "Snowblasters!"),
            Mark::new(8_000, 8_500, "Snowblasters!"),
            Mark::new(10_500, 11_000, "quiet night"),
        ];
        let found = shouts_from_words(&cues, &words, &lines, Some("Snowblasters"), 20.0);
        let times: Vec<f64> = found.iter().map(|f| f.at).collect();
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found.iter().all(|f| f.label.as_deref() == Some("Snowblasters")));
        // The one where the voice jumps, on a hit, is the strongest.
        assert_eq!(times[0], 8.0, "{found:?}");
    }
}
