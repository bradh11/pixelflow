//! Reviewing a sequence against its song, as a lighting designer watches a run-through: the
//! sequence is drawn every [`STEP_MS`] (with its music, for the effects that follow it), each
//! frame summed up (how bright, how much is lit, its colors, how much changed since the frame
//! before, each prop's brightness), and the run-through scored on:
//!
//! - **moments**: each of the song's top moments (impacts, drops, shouts, stops, restarts, peaks,
//!   builds' ends) shows within a beat: brightness or the lit share jumps, many pixels light at
//!   once, or the motion spikes; a stop goes dark.
//! - **contrast**: the biggest hits come out of something darker in the half beat before.
//! - **energy**: the show's intensity (brightness lifted by motion) follows the music bar by bar,
//!   and no section is much hotter or colder than its music.
//! - **consistency**: sections that repeat look alike (colors, effects per prop role, which
//!   props are lit), and different sections look different.
//! - **variety**: no prop holds the same effects for more than 16 bars; the show doesn't flash
//!   much of the time.
//! - **safety**: flashes over the whole show stay under 3 a second (sustained over 2 s).
//! - **lyrics**: hook words (shouts, the title's words) show when they're sung.
//! - **dead air**: nothing goes dark while the music plays, outside stops and breaks.
//! - **restraint**: ordinary bars don't hit as hard as the biggest moment.
//!
//! The result is a score, a score per criterion, and a short punch list of fixes, each with its
//! time and, where tools can make it, the tool calls that would. Everything is worked out the
//! same way every time.

use crate::cues::{Role, role_of, suggested_cue};
use crate::provider::Cancel;
use pf_analysis::{Analysis, Moment, MomentKind};
use pf_model::{PropId, Show};
use pf_render::AudioSource;
use pf_sequence::{Sequence, Target, format_ms};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;

/// How often the sequence is drawn (ms), at the least.
pub const STEP_MS: u64 = 50;
/// Fixes in the punch list, at most.
pub const MAX_FIXES: usize = 12;
/// Top moments checked, at most.
pub const TOP_MOMENTS: usize = 12;
/// Moments from this importance are top moments.
const TOP_IMPORTANCE: f32 = 0.5;
/// Stretches of the song drawn side by side (fixed, so the result doesn't depend on the machine).
const CHUNKS: usize = 8;
/// Color bins: twelve hues, then white (and pale colors).
const HUES: usize = 13;
/// A pixel whose brightest channel is this bright (0–1) is lit.
const LIT: f32 = 0.1;
/// A pixel changing this much from one frame to the next jumps.
const JUMP: f32 = 0.25;
/// A rise this big at a moment is a visible emphasis.
const EMPHASIS: f32 = 0.12;
/// A sung hook word needs less: a pop on a few props shows.
const WORD_EMPHASIS: f32 = 0.06;
/// Bars a prop may hold the same effects before it's stale.
const STALE_BARS: u64 = 16;
/// Flashes a second over the whole show that are too many, kept up over [`FLASH_WINDOW_MS`].
const FLASH_HZ: f32 = 3.0;
const FLASH_WINDOW_MS: u64 = 2_000;
/// The share of the song flashing that tires the eye.
const FLASHY_SHARE: f32 = 0.12;
/// Music this loud (0–1) is playing.
const PLAYING: f32 = 0.2;
/// A color for each part of the song (by its group letter), for filling dark stretches.
const PART_COLORS: [&str; 6] = ["#ffc880", "#2a6bff", "#ff2a2a", "#2aff8a", "#b02aff", "#ffd700"];
/// Words too common to be hooks.
const COMMON: [&str; 24] = [
    "the", "and", "you", "your", "for", "are", "but", "not", "all", "with", "that", "this", "was", "his",
    "her", "she", "him", "they", "them", "what", "who", "its", "it's", "from",
];

/// What's reviewed: a sequence on a show, with its song.
pub struct Subject<'a> {
    pub show: &'a Show,
    pub doc: &'a Sequence,
    /// The user's sequence as it was before any draft: its own sections, and moments numbered
    /// as `analyze_song` numbers them.
    pub user: Option<&'a Sequence>,
    pub analysis: Option<&'a Analysis>,
    /// The music, for the effects that follow it.
    pub audio: &'a AudioSource,
}

/// One fix in the punch list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fix {
    #[serde(skip)]
    pub at_ms: u64,
    /// When, as `m:ss.mmm`.
    pub at: String,
    /// What's wrong, in a plain sentence.
    pub what: String,
    /// The tool calls that fix it, in order, when tools can.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<Call>,
    /// How to fix it otherwise (or what to do after).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// A tool call: the tool's name and its input.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Call {
    pub tool: &'static str,
    pub input: Value,
}

/// Each criterion's score, 0–100 (`None`: it doesn't apply, e.g. no lyrics).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Criteria {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moments: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contrast: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub energy: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consistency: Option<u32>,
    pub variety: u32,
    pub safety: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lyrics: Option<u32>,
    pub dead_air: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restraint: Option<u32>,
}

impl Criteria {
    /// The overall score: each criterion weighted, those that don't apply left out.
    fn overall(&self) -> u32 {
        let weighted = [
            (self.moments, 25.0),
            (self.contrast, 10.0),
            (self.energy, 15.0),
            (self.consistency, 10.0),
            (Some(self.variety), 10.0),
            (Some(self.safety), 10.0),
            (self.lyrics, 5.0),
            (Some(self.dead_air), 10.0),
            (self.restraint, 5.0),
        ];
        let (sum, weight) = weighted
            .iter()
            .filter_map(|(score, w)| score.map(|s| (f64::from(s) * w, *w)))
            .fold((0.0, 0.0), |(a, b), (s, w)| (a + s, b + w));
        let score = (sum / weight.max(1.0)).round() as u32;
        // Unsafe flashing caps everything.
        if self.safety < 100 { score.min(60) } else { score }
    }
}

/// A review: the score, each criterion's, a one-line summary, and the punch list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub score: u32,
    /// "Review: 92/100 · all 12 top moments emphasised · flashes safe".
    pub summary: String,
    pub criteria: Criteria,
    pub fixes: Vec<Fix>,
    /// Fixes left off the list.
    #[serde(skip_serializing_if = "is_zero")]
    pub more_fixes: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// Top moments emphasised, of those checked.
    #[serde(skip)]
    pub top_moments: (usize, usize),
    #[serde(skip)]
    pub flashes_safe: bool,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// What the proposal card shows of a review.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewView {
    pub score: u32,
    pub line: String,
    /// The fixes left, each "m:ss.mmm what".
    pub items: Vec<String>,
}

impl Review {
    /// The review as the model reads it (compact JSON).
    pub fn to_model(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn view(&self) -> ReviewView {
        let mut items: Vec<String> = self
            .fixes
            .iter()
            .map(|f| format!("{} {}", f.at, f.what))
            .collect();
        if self.more_fixes > 0 {
            items.push(format!("…and {} more.", self.more_fixes));
        }
        ReviewView {
            score: self.score,
            line: self.summary.clone(),
            items,
        }
    }
}

/// Reviews the subject's sequence. `Err` when stopped.
pub fn review(subject: &Subject<'_>, cancel: &Cancel) -> Result<Review, String> {
    let mut notes = Vec::new();
    let audio = wait_for_music(subject.audio, cancel)?;
    if subject.audio.has_music() && audio.track().is_none() {
        notes.push("The music wasn't ready, so effects that follow it were drawn as in silence.".into());
    }
    let frames = draw(subject.show, subject.doc, &audio, cancel)?;
    let song = Song::new(subject);
    if subject.analysis.is_none() {
        notes.push(
            "Without the song's analysis, only flashing, variety, and dark stretches were checked.".into(),
        );
    }
    let mut r = Reviewer {
        subject,
        frames,
        hooks: hooks(subject.doc, &song.moments),
        song,
        fixes: Fixes::default(),
        criteria: Criteria::default(),
        top: (0, 0),
        safe: true,
    };
    r.moments();
    r.energy();
    r.consistency();
    r.variety();
    r.lyrics();
    r.dead_air();
    r.restraint();
    let score = r.criteria.overall();
    let (shown, of) = r.top;
    let mut summary = format!("Review: {score}/100");
    match (shown, of) {
        (_, 0) => {}
        (1, 1) => summary.push_str(" · the top moment emphasised"),
        (_, 1) => summary.push_str(" · the top moment not emphasised"),
        (s, n) if s == n => summary.push_str(&format!(" · all {n} top moments emphasised")),
        (s, n) => summary.push_str(&format!(" · {s} of {n} top moments emphasised")),
    }
    summary.push_str(if r.safe {
        " · flashes safe"
    } else {
        " · flashing too fast"
    });
    let (fixes, more_fixes) = r.fixes.list();
    Ok(Review {
        score,
        summary,
        criteria: r.criteria,
        fixes,
        more_fixes,
        notes,
        top_moments: (shown, of),
        flashes_safe: r.safe,
    })
}

/// The music once it's worked out (it's being worked out in the background when a sequence is
/// opened), giving up after a while: the review then draws as in silence.
fn wait_for_music(audio: &AudioSource, cancel: &Cancel) -> Result<AudioSource, String> {
    for _ in 0..600 {
        if !audio.has_music() || audio.track().is_some() {
            break;
        }
        cancel.check().map_err(|_| "Stopped.".to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Ok(audio.clone())
}

// ---- drawing ----

/// The sequence drawn every `step` ms, each frame summed up.
struct Frames {
    step: u64,
    /// Mean brightness over every pixel, 0–1 (so it counts how much is lit, too).
    level: Vec<f32>,
    /// The share of pixels lit.
    lit: Vec<f32>,
    /// The mean change in brightness since the frame before.
    motion: Vec<f32>,
    /// The share of pixels that jumped brighter, and darker, since the frame before.
    up: Vec<f32>,
    down: Vec<f32>,
    /// Brightness by color bin (twelve hues, then white), over every pixel.
    hues: Vec<[f32; HUES]>,
    /// Each prop's mean brightness: frame × props + prop.
    prop_level: Vec<f32>,
    props: Vec<PropId>,
}

impl Frames {
    fn len(&self) -> usize {
        self.level.len()
    }

    /// The frames from `from` up to (not including) `to`.
    fn span(&self, from: u64, to: u64) -> Range<usize> {
        let a = (from.div_ceil(self.step) as usize).min(self.len());
        let b = (to.div_ceil(self.step) as usize).min(self.len());
        a..b.max(a)
    }

    fn mean(&self, values: &[f32], range: Range<usize>) -> f32 {
        let n = range.len();
        if n == 0 {
            return 0.0;
        }
        values[range].iter().sum::<f32>() / n as f32
    }

    /// Intensity: brightness lifted by motion (a still look counts half).
    fn intensity(&self, i: usize) -> f32 {
        self.level[i] * (0.5 + 0.5 * (self.motion[i] / 0.05).min(1.0))
    }

    fn prop(&self, frame: usize, prop: usize) -> f32 {
        self.prop_level[frame * self.props.len() + prop]
    }
}

/// One stretch of frames, drawn on its own.
struct Part {
    level: Vec<f32>,
    lit: Vec<f32>,
    motion: Vec<f32>,
    up: Vec<f32>,
    down: Vec<f32>,
    hues: Vec<[f32; HUES]>,
    prop_level: Vec<f32>,
}

/// A prop's pixels in the frame: its byte offset, bytes per pixel, and pixel count.
type Pixels = (usize, usize, usize);

fn draw(show: &Show, doc: &Sequence, audio: &AudioSource, cancel: &Cancel) -> Result<Frames, String> {
    let step = STEP_MS.max(u64::from(doc.frame_ms));
    let layout: Vec<(PropId, Pixels)> = pf_engine::preview_props_of(show)
        .into_iter()
        .map(|p| {
            let n = p.points.len() / 2;
            (
                p.prop,
                (p.frame_offset, usize::from(p.channels_per_pixel.max(1)), n),
            )
        })
        .collect();
    let n = doc.duration_ms.div_ceil(step) as usize;
    let chunk = n.div_ceil(CHUNKS).max(1);
    let pixels: Vec<Pixels> = layout.iter().map(|(_, p)| *p).collect();
    let parts: Vec<Option<Part>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..n)
            .step_by(chunk)
            .map(|start| {
                let range = start..(start + chunk).min(n);
                let pixels = &pixels;
                scope.spawn(move || draw_part(show, doc, audio, pixels, step, range, cancel))
            })
            .collect();
        handles.into_iter().map(|h| h.join().ok().flatten()).collect()
    });
    cancel.check().map_err(|_| "Stopped.".to_string())?;
    let mut frames = Frames {
        step,
        level: Vec::with_capacity(n),
        lit: Vec::with_capacity(n),
        motion: Vec::with_capacity(n),
        up: Vec::with_capacity(n),
        down: Vec::with_capacity(n),
        hues: Vec::with_capacity(n),
        prop_level: Vec::with_capacity(n * layout.len()),
        props: layout.iter().map(|(id, _)| *id).collect(),
    };
    for part in parts {
        let part = part.ok_or("The sequence couldn't be drawn for the review.")?;
        frames.level.extend(part.level);
        frames.lit.extend(part.lit);
        frames.motion.extend(part.motion);
        frames.up.extend(part.up);
        frames.down.extend(part.down);
        frames.hues.extend(part.hues);
        frames.prop_level.extend(part.prop_level);
    }
    Ok(frames)
}

fn draw_part(
    show: &Show,
    doc: &Sequence,
    audio: &AudioSource,
    pixels: &[Pixels],
    step: u64,
    range: Range<usize>,
    cancel: &Cancel,
) -> Option<Part> {
    let mut renderer = pf_engine::DraftRenderer::new(show);
    renderer.set_audio(audio.clone());
    let total: usize = pixels.iter().map(|p| p.2).sum();
    let count = range.len();
    let mut part = Part {
        level: Vec::with_capacity(count),
        lit: Vec::with_capacity(count),
        motion: Vec::with_capacity(count),
        up: Vec::with_capacity(count),
        down: Vec::with_capacity(count),
        hues: Vec::with_capacity(count),
        prop_level: Vec::with_capacity(count * pixels.len()),
    };
    let mut before = vec![0.0f32; total];
    let mut now = vec![0.0f32; total];
    // The frame before the first, for the change into it.
    let first = range.start.saturating_sub(1);
    for i in first..range.end {
        if cancel.is_cancelled() {
            return None;
        }
        let frame = renderer.frame(doc, i as u64 * step);
        let mut hues = [0.0f32; HUES];
        let mut lit = 0usize;
        let mut k = 0;
        let mut props = Vec::with_capacity(pixels.len());
        for &(offset, bytes, n) in pixels {
            let mut sum = 0.0;
            for p in 0..n {
                let at = offset + p * bytes;
                let rgb = frame.get(at..at + 3).unwrap_or(&[0, 0, 0]);
                let (v, bin) = brightness_and_bin(rgb);
                now[k] = v;
                sum += v;
                if let Some(bin) = bin {
                    hues[bin] += v;
                    lit += 1;
                }
                k += 1;
            }
            props.push(if n == 0 { 0.0 } else { sum / n as f32 });
        }
        if i < range.start {
            std::mem::swap(&mut before, &mut now);
            continue;
        }
        let scale = 1.0 / total.max(1) as f32;
        let (mut level, mut motion, mut up, mut down) = (0.0, 0.0, 0usize, 0usize);
        for (v, b) in now.iter().zip(&before) {
            level += v;
            if i > 0 {
                let d = v - b;
                motion += d.abs();
                up += usize::from(d >= JUMP);
                down += usize::from(d <= -JUMP);
            }
        }
        part.level.push(level * scale);
        part.lit.push(lit as f32 * scale);
        part.motion.push(motion * scale);
        part.up.push(up as f32 * scale);
        part.down.push(down as f32 * scale);
        part.hues.push(hues.map(|h| h * scale));
        part.prop_level.extend(props);
        std::mem::swap(&mut before, &mut now);
    }
    Some(part)
}

/// A pixel's brightness (0–1: halfway between its brightest channel and their mean, so white
/// is brighter than a full red), and its color bin when it's lit.
fn brightness_and_bin(rgb: &[u8]) -> (f32, Option<usize>) {
    let (r, g, b) = (f32::from(rgb[0]), f32::from(rgb[1]), f32::from(rgb[2]));
    let max = r.max(g).max(b);
    let v = (0.5 * max + (r + g + b) / 6.0) / 255.0;
    if max / 255.0 < LIT {
        return (v, None);
    }
    let min = r.min(g).min(b);
    let d = max - min;
    if d / max < 0.25 {
        return (v, Some(HUES - 1));
    }
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (v, Some(((h * 2.0) as usize).min(HUES - 2)))
}

// ---- the song ----

/// A section of the song: its label, its name (counted when the label repeats: "Chorus 2"), its
/// group (the same letter is the same music), and when.
#[derive(Debug, Clone)]
struct Section {
    label: String,
    name: String,
    group: String,
    start: u64,
    end: u64,
}

/// Where sections are, in words: "Chorus 1 (0:45.430–1:02.032)", or "Chorus (3 times, from
/// 0:45.430)".
fn where_(sections: &[&Section]) -> String {
    match sections {
        [one] => format!("{} ({}–{})", one.name, format_ms(one.start), format_ms(one.end)),
        many => format!(
            "{} ({} times, from {})",
            crate::song::label_root(&many[0].label),
            many.len(),
            format_ms(many[0].start)
        ),
    }
}

/// What the review knows of the song.
struct Song {
    beat: u64,
    /// Each bar: start, end, and the music's energy (0–1).
    bars: Vec<(u64, u64, f32)>,
    /// The song's moments, as `analyze_song` ranks them.
    moments: Vec<Moment>,
    sections: Vec<Section>,
    /// How loud each second is (0–1).
    loudness: Vec<f32>,
    analyzed: bool,
}

impl Song {
    fn new(subject: &Subject<'_>) -> Self {
        let Some(analysis) = subject.analysis else {
            return Self {
                beat: 500,
                bars: Vec::new(),
                moments: Vec::new(),
                sections: Vec::new(),
                loudness: Vec::new(),
                analyzed: false,
            };
        };
        let beat = analysis
            .tempo_bpm
            .filter(|t| *t > 0.0)
            .map_or(500, |t| (60_000.0 / t).round() as u64)
            .clamp(200, 1500);
        let end = subject.doc.duration_ms.min(analysis.duration_ms.max(1));
        let bars = analysis
            .bars
            .iter()
            .enumerate()
            .filter(|(_, b)| **b < end)
            .map(|(i, &b)| {
                let next = analysis.bars.get(i + 1).copied().unwrap_or(b + 4 * beat).min(end);
                let energy = analysis.bar_energy.get(i).map_or(0.0, |e| e.overall);
                (b, next, energy)
            })
            .filter(|(a, b, _)| b > a)
            .collect();
        Self {
            beat,
            bars,
            moments: crate::song::ranked_moments(analysis, subject.user),
            sections: sections(analysis, subject.user, end),
            loudness: analysis.energy.clone(),
            analyzed: true,
        }
    }

    fn bar(&self) -> u64 {
        4 * self.beat
    }

    /// Whether music plays at `t` (always, without an analysis or its loudness).
    fn playing(&self, t: u64) -> bool {
        !self.analyzed
            || self.loudness.is_empty()
            || self
                .loudness
                .get((t / 1000) as usize)
                .is_some_and(|e| *e >= PLAYING)
    }
}

/// The song's sections: the user's own (from their Sections track, grouped by name) when they
/// have them, else the analysis's.
fn sections(analysis: &Analysis, user: Option<&Sequence>, end: u64) -> Vec<Section> {
    // (label, group, start, end)
    let found: Vec<(String, String, u64, u64)> = match user.and_then(crate::align::user_sections) {
        Some(track) => {
            let mut roots: Vec<String> = Vec::new();
            track
                .marks
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let label = if m.label.trim().is_empty() {
                        format!("Section {}", i + 1)
                    } else {
                        m.label.trim().to_string()
                    };
                    let root = crate::song::label_root(&label).to_string();
                    let at = roots.iter().position(|r| *r == root).unwrap_or_else(|| {
                        roots.push(root);
                        roots.len() - 1
                    });
                    let group = char::from(b'A' + (at % 26) as u8).to_string();
                    (label, group, m.start_ms, m.end_ms)
                })
                .collect()
        }
        None => analysis
            .sections()
            .into_iter()
            .map(|s| (s.label, s.group, s.start_ms, s.end_ms))
            .collect(),
    };
    let mut seen: HashMap<String, usize> = HashMap::new();
    found
        .iter()
        .filter(|s| s.2 < end)
        .map(|(label, group, start, stop)| {
            let repeats = found.iter().filter(|s| s.0 == *label).count() > 1;
            let n = seen.entry(label.clone()).or_default();
            *n += 1;
            Section {
                label: label.clone(),
                name: if repeats {
                    format!("{label} {n}")
                } else {
                    label.clone()
                },
                group: group.clone(),
                start: *start,
                end: (*stop).min(end),
            }
        })
        .collect()
}

/// Moments that should show as a jump (the rest of the top kinds go dark: stops).
fn lands(kind: MomentKind) -> bool {
    matches!(
        kind,
        MomentKind::Impact
            | MomentKind::Drop
            | MomentKind::Shout
            | MomentKind::Restart
            | MomentKind::Peak
            | MomentKind::Build
    )
}

/// A moment's kind in words ("impact", "build's end").
fn kind_name(kind: MomentKind) -> &'static str {
    match kind {
        MomentKind::Build => "build's end",
        MomentKind::KeyChange => "key change",
        MomentKind::SectionChange => "section change",
        other => other.word(),
    }
}

/// The hook words, and when each is sung: the words shouted at moments, and the title's words
/// sung at least twice (none without sung words).
fn hooks(doc: &Sequence, moments: &[Moment]) -> Vec<(String, Vec<(u64, u64)>)> {
    let Some(words) = crate::lyrics::tracks::words_track(&doc.timing_tracks) else {
        return Vec::new();
    };
    let keep = |w: &String| w.chars().count() >= 3 && !COMMON.contains(&w.as_str());
    let sung: Vec<String> = words.marks.iter().map(|m| plain(&m.label)).collect();
    let shouted = moments
        .iter()
        .filter(|m| m.kind == MomentKind::Shout)
        .filter_map(|m| m.label.as_deref())
        .flat_map(str::split_whitespace)
        .map(plain);
    let title = doc
        .name
        .split_whitespace()
        .map(plain)
        .filter(|w| sung.iter().filter(|s| *s == w).count() >= 2);
    let hooks: BTreeSet<String> = shouted.chain(title).filter(keep).collect();
    hooks
        .into_iter()
        .map(|word| {
            let times = words
                .marks
                .iter()
                .zip(&sung)
                .filter(|(_, s)| **s == word)
                .map(|(m, _)| (m.start_ms, m.end_ms))
                .collect();
            (word, times)
        })
        .filter(|(_, times): &(String, Vec<(u64, u64)>)| !times.is_empty())
        .collect()
}

/// Lowercase letters and digits only ("Ghostbusters!" → "ghostbusters").
fn plain(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric() || *c == '\'')
        .flat_map(char::to_lowercase)
        .collect()
}

// ---- the punch list ----

/// A `stage_cue` call for `cues`.
fn stage(cues: Vec<Value>) -> Call {
    Call {
        tool: "stage_cue",
        input: json!({ "cues": cues }),
    }
}

/// Fixes by how much they matter (lower first), in the order found.
#[derive(Default)]
struct Fixes(Vec<(u8, Fix)>);

impl Fixes {
    fn add(&mut self, rank: u8, at_ms: u64, what: String, calls: Vec<Call>, hint: Option<String>) {
        self.0.push((
            rank,
            Fix {
                at_ms,
                at: format_ms(at_ms),
                what,
                calls,
                hint,
            },
        ));
    }

    fn cue(&mut self, rank: u8, at_ms: u64, what: String, cue: Value) {
        self.cues(rank, at_ms, what, vec![cue]);
    }

    fn cues(&mut self, rank: u8, at_ms: u64, what: String, cues: Vec<Value>) {
        self.add(rank, at_ms, what, vec![stage(cues)], None);
    }

    /// The list, most important first and capped, and how many were left off.
    fn list(mut self) -> (Vec<Fix>, usize) {
        self.0.sort_by_key(|(rank, _)| *rank);
        let more = self.0.len().saturating_sub(MAX_FIXES);
        (self.0.into_iter().take(MAX_FIXES).map(|(_, f)| f).collect(), more)
    }
}

/// How much fixes matter, most first.
mod rank {
    pub const SAFETY: u8 = 0;
    pub const MOMENT: u8 = 1;
    pub const FLAT: u8 = 2;
    pub const DEAD: u8 = 3;
    pub const ENERGY: u8 = 4;
    pub const LOOK: u8 = 5;
    pub const LYRICS: u8 = 6;
    pub const RESTRAINT: u8 = 7;
    pub const VARIETY: u8 = 8;
}

fn percent(x: f32) -> u32 {
    (x.clamp(0.0, 1.0) * 100.0).round() as u32
}

struct Reviewer<'a> {
    subject: &'a Subject<'a>,
    frames: Frames,
    song: Song,
    fixes: Fixes,
    criteria: Criteria,
    /// The hook words, and when each is sung.
    hooks: Vec<(String, Vec<(u64, u64)>)>,
    /// Top moments emphasised, of those checked.
    top: (usize, usize),
    safe: bool,
}

impl Reviewer<'_> {
    fn beat(&self) -> u64 {
        self.song.beat
    }

    fn half(&self) -> usize {
        ((self.beat() / 2 / self.frames.step) as usize).max(1)
    }

    /// How hard the show rises at frame `i`: brightness or the lit share up from the lowest of
    /// the half beat before, or the share of pixels jumping brighter at once.
    fn rise_at(&self, i: usize) -> f32 {
        let f = &self.frames;
        let back = i.saturating_sub(self.half())..i;
        let low = |v: &[f32]| v[back.clone()].iter().copied().fold(v[i], f32::min);
        (f.level[i] - low(&f.level))
            .max(f.lit[i] - low(&f.lit))
            .max(f.up[i])
    }

    /// The biggest rise from `from` to `to`, or a motion spike well above the motion around.
    fn emphasis(&self, from: u64, to: u64) -> f32 {
        let f = &self.frames;
        let span = f.span(from, to + 1);
        if span.is_empty() {
            return 0.0;
        }
        let around = f.span(from.saturating_sub(2 * self.song.bar()), to + 2 * self.song.bar());
        let mut motions: Vec<f32> = f.motion[around].to_vec();
        motions.sort_by(f32::total_cmp);
        let usual = motions.get(motions.len() / 2).copied().unwrap_or(0.0);
        span.map(|i| self.rise_at(i).max(2.0 * (f.motion[i] - 2.0 * usual)))
            .fold(0.0, f32::max)
    }

    /// How big the treatment at `t` is: the rise, the motion, and how much is lit.
    fn size(&self, t: u64) -> f32 {
        let f = &self.frames;
        let b = self.beat();
        let after = f.span(t, t + b);
        let peak = |v: &[f32]| v[after.clone()].iter().copied().fold(0.0, f32::max);
        self.emphasis(t.saturating_sub(b / 4), t + b / 2) + peak(&f.motion) + 0.5 * peak(&f.lit)
    }

    /// Whether the show goes dark at `t` (a quarter beat starting within a half beat of it, much
    /// darker than the beat before).
    fn goes_dark(&self, t: u64) -> bool {
        let f = &self.frames;
        let b = self.beat();
        let before = f.mean(
            &f.level,
            f.span(t.saturating_sub(3 * b / 2), t.saturating_sub(b / 2)),
        );
        let quarter = (b / 4).max(f.step);
        let mut darkest = f32::MAX;
        let mut s = t.saturating_sub(b / 4);
        while s <= t + b / 2 {
            let span = f.span(s, s + quarter);
            if !span.is_empty() {
                darkest = darkest.min(f.mean(&f.level, span));
            }
            s += f.step;
        }
        darkest <= 0.03f32.max(0.35 * before)
    }

    // ---- moments and contrast ----

    fn moments(&mut self) {
        let duration = self.subject.doc.duration_ms;
        let top: Vec<(usize, Moment)> = self
            .song
            .moments
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                (lands(m.kind) || m.kind == MomentKind::Stop)
                    && m.importance >= TOP_IMPORTANCE
                    && m.time_ms < duration
            })
            .take(TOP_MOMENTS)
            .map(|(i, m)| (i, m.clone()))
            .collect();
        if top.is_empty() {
            return;
        }
        let b = self.beat();
        let (mut shown, mut weight, mut got) = (0, 0.0, 0.0);
        let mut hits: Vec<(usize, Moment)> = Vec::new();
        for (i, m) in &top {
            // A build lands at its end.
            let at = if m.kind == MomentKind::Build {
                m.end_ms.unwrap_or(m.time_ms).min(duration.saturating_sub(1))
            } else {
                m.time_ms
            };
            let ok = if m.kind == MomentKind::Stop {
                self.goes_dark(at)
            } else if m.kind == MomentKind::Peak {
                let f = &self.frames;
                let end = m.end_ms.unwrap_or(at + 4 * self.song.bar());
                let during = f.span(at, end);
                let mut all: Vec<f32> = (0..f.len()).map(|k| f.intensity(k)).collect();
                all.sort_by(f32::total_cmp);
                let p75 = all.get(all.len() * 3 / 4).copied().unwrap_or(0.0);
                let mean = during.clone().map(|k| f.intensity(k)).sum::<f32>() / during.len().max(1) as f32;
                self.emphasis(at.saturating_sub(b), at + b) >= EMPHASIS || (mean >= p75 && mean > 0.05)
            } else {
                self.emphasis(at.saturating_sub(b), at + b) >= EMPHASIS
            };
            weight += m.importance;
            if ok {
                shown += 1;
                got += m.importance;
                if lands(m.kind) && m.kind != MomentKind::Peak && m.kind != MomentKind::Build {
                    hits.push((*i, m.clone()));
                }
                continue;
            }
            let name = kind_name(m.kind);
            let word = m
                .label
                .as_deref()
                .filter(|_| m.kind == MomentKind::Shout)
                .map(|w| format!(" \"{w}\""))
                .unwrap_or_default();
            let what = if m.kind == MomentKind::Stop {
                format!("The {name} (m{i}) stays lit: the band cuts out, so the lights should too.")
            } else {
                format!("The {name}{word} (m{i}) doesn't show: nothing jumps within a beat of it.")
            };
            let cue = json!({ "cue": suggested_cue(m.suggest), "at": format!("m{i}") });
            self.fixes.cue(rank::MOMENT, at, what, cue);
        }
        self.top = (shown, top.len());
        self.criteria.moments = Some(percent(got / weight.max(f32::EPSILON)));
        self.contrast(&hits);
    }

    /// The biggest hits shown: each should come out of something darker in the half beat
    /// before.
    fn contrast(&mut self, hits: &[(usize, Moment)]) {
        let biggest = &hits[..hits.len().min(6)];
        if biggest.is_empty() {
            return;
        }
        let b = self.beat();
        let mut flat = 0;
        for (i, m) in biggest {
            let t = m.time_ms;
            let f = &self.frames;
            let before = f.mean(&f.level, f.span(t.saturating_sub(b / 2), t));
            let after = f.level[f.span(t, t + b)].iter().copied().fold(0.0, f32::max);
            if after > 0.05 && before >= 0.8 * after {
                flat += 1;
                let what = format!(
                    "Flat into the {} (m{i}): it's as bright just before, so the hit has nothing to land from.",
                    kind_name(m.kind)
                );
                let cue =
                    json!({ "cue": "blackout", "at": t.saturating_sub(b / 2), "until": t, "hit": false });
                self.fixes.cue(rank::FLAT, t, what, cue);
            }
        }
        self.criteria.contrast = Some(percent(1.0 - flat as f32 / biggest.len() as f32));
    }

    // ---- energy ----

    fn energy(&mut self) {
        let f = &self.frames;
        let bars: Vec<(u64, f32, f32)> = self
            .song
            .bars
            .iter()
            .map(|&(a, b, music)| {
                let span = f.span(a, b);
                let n = span.len().max(1) as f32;
                let show = span.map(|i| f.intensity(i)).sum::<f32>() / n;
                (a, music, show)
            })
            .collect();
        if bars.len() < 8 {
            return;
        }
        let music: Vec<f32> = bars.iter().map(|b| b.1).collect();
        let show: Vec<f32> = bars.iter().map(|b| b.2).collect();
        let r = correlation(&music, &show);
        let (music_n, show_n) = (normalized(&music), normalized(&show));
        // Sections of the same music together, in the order they first come.
        let mut groups: Vec<(String, Vec<&Section>, Vec<usize>)> = Vec::new();
        for s in &self.song.sections {
            let idx: Vec<usize> = (0..bars.len())
                .filter(|&i| bars[i].0 >= s.start && bars[i].0 < s.end)
                .collect();
            if idx.len() < 2 {
                continue;
            }
            match groups.iter_mut().find(|g| g.0 == s.group) {
                Some(g) => {
                    g.1.push(s);
                    g.2.extend(idx);
                }
                None => groups.push((s.group.clone(), vec![s], idx)),
            }
        }
        let mean = |v: &[f32], idx: &[usize]| idx.iter().map(|&i| v[i]).sum::<f32>() / idx.len() as f32;
        let mut flagged_bars = 0usize;
        let mut flagged: Vec<usize> = Vec::new();
        let mut fixes = Vec::new();
        for (g, (_, sections, idx)) in groups.iter().enumerate() {
            let (m, sh) = (mean(&music_n, idx), mean(&show_n, idx));
            let (cue, problem) = if sh - m > 0.4 && m < 0.5 {
                ("minimal", "is lit like a loud part, but the music is quiet there")
            } else if m - sh > 0.4 && m > 0.5 {
                ("full", "is dim for how loud the music is")
            } else {
                continue;
            };
            flagged.push(g);
            flagged_bars += idx.len();
            fixes.push((
                sections[0].start,
                format!("{} {problem}.", where_(sections)),
                cue,
                sections.clone(),
            ));
        }
        // A louder part of the song that's dimmer than a quieter one (a chorus under its verse).
        'pairs: for (a, loud) in groups.iter().enumerate() {
            for (b, quiet) in groups.iter().enumerate() {
                let (loud_music, quiet_music) = (mean(&music, &loud.2), mean(&music, &quiet.2));
                let (loud_show, quiet_show) = (mean(&show, &loud.2), mean(&show, &quiet.2));
                if a != b
                    && !flagged.contains(&a)
                    && loud_music > quiet_music + 0.15
                    && loud_show + 0.05 < quiet_show
                {
                    flagged.push(a);
                    fixes.push((
                        loud.1[0].start,
                        format!(
                            "{} is dimmer than {}, though its music is louder.",
                            where_(&loud.1),
                            quiet.1[0].name
                        ),
                        "full",
                        loud.1.clone(),
                    ));
                    break 'pairs;
                }
            }
        }
        // Music that hardly changes has nothing to follow.
        let (lo, hi) = music
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), &m| (lo.min(m), hi.max(m)));
        let follows = if hi - lo < 0.05 {
            1.0
        } else {
            ((r - 0.1) / 0.6).clamp(0.0, 1.0)
        };
        let calm = 1.0 - (flagged_bars as f32 / bars.len() as f32).min(1.0);
        let pairs = if flagged.is_empty() { 1.0 } else { 0.9 };
        self.criteria.energy = Some(percent((0.6 * follows + 0.4 * calm) * pairs));
        for (at, what, cue, sections) in fixes.into_iter().take(3) {
            // Every time the part comes, so it stays the same part.
            let mut cues: Vec<Value> = sections
                .iter()
                .take(6)
                .map(|s| {
                    let mut c = json!({ "cue": cue, "at": s.start, "until": s.end });
                    if cue == "full" {
                        c["intensity"] = json!(0.7);
                    }
                    c
                })
                .collect();
            let spans: Vec<(u64, u64)> = sections.iter().map(|s| (s.start, s.end)).collect();
            cues.extend(self.restaged(&spans));
            self.fixes.cues(rank::ENERGY, at, what, cues);
        }
    }

    /// The point cues (hits, word pops) of the moments `stageMoments` stages within `spans`:
    /// staged again with a cue that lasts over them, they stay on top of it.
    fn restaged(&self, spans: &[(u64, u64)]) -> Vec<Value> {
        let mut cues: Vec<Value> = self
            .song
            .moments
            .iter()
            .enumerate()
            .filter(|(_, m)| m.importance >= TOP_IMPORTANCE)
            .filter(|(_, m)| spans.iter().any(|(a, z)| (*a..*z).contains(&m.time_ms)))
            .filter_map(|(i, m)| {
                let cue = suggested_cue(m.suggest);
                matches!(cue, "hit" | "word_pop").then(|| json!({ "cue": cue, "at": format!("m{i}") }))
            })
            .collect();
        // The hook words sung there pop again too.
        for (word, times) in &self.hooks {
            for &(a, z) in spans {
                if let Some(&(first, _)) = times.iter().find(|(t, _)| (a..z).contains(t)) {
                    cues.push(json!({ "cue": "word_pop", "at": first, "until": z, "match": word }));
                }
            }
        }
        cues
    }

    // ---- consistency ----

    fn consistency(&mut self) {
        let bar = self.song.bar();
        let sections: Vec<Section> = self
            .song
            .sections
            .iter()
            .filter(|s| s.end.saturating_sub(s.start) >= 2 * bar)
            .cloned()
            .collect();
        if sections.len() < 2 {
            return;
        }
        let looks: Vec<Look> = sections.iter().map(|s| self.look(s.start, s.end)).collect();
        let mut flags = 0;
        let mut first: BTreeMap<&str, usize> = BTreeMap::new();
        for (i, s) in sections.iter().enumerate() {
            match first.get(s.group.as_str()) {
                Some(&j) => {
                    let d = looks[i].distance(&looks[j]);
                    if d > 0.5 && flags < 3 {
                        flags += 1;
                        let (a, b) = (&sections[j], s);
                        let mut calls = vec![Call {
                            tool: "repeat_effects",
                            input: json!({ "fromMs": a.start, "toMs": a.end.min(a.start + (b.end - b.start)), "startsMs": [b.start], "replace": true }),
                        }];
                        // The copy replaces what was there: its own moments go back on top.
                        let moments = self.restaged(&[(b.start, b.end)]);
                        if !moments.is_empty() {
                            calls.push(stage(moments));
                        }
                        self.fixes.add(
                            rank::LOOK,
                            b.start,
                            format!(
                                "{} looks unlike {} though it's the same music: bring its look back, with a variation.",
                                b.name, a.name
                            ),
                            calls,
                            Some("Then change its colors or speed a little.".into()),
                        );
                    }
                }
                None => {
                    first.insert(s.group.as_str(), i);
                }
            }
        }
        // Different music should look different.
        let firsts: Vec<usize> = first.values().copied().collect();
        'pairs: for (n, &i) in firsts.iter().enumerate() {
            for &j in &firsts[n + 1..] {
                let (a, b) = (&sections[i.min(j)], &sections[i.max(j)]);
                if crate::song::label_root(&a.label) == crate::song::label_root(&b.label) {
                    continue;
                }
                let (la, lb) = (&looks[i.min(j)], &looks[i.max(j)]);
                if la.lit && lb.lit && la.distance(lb) < 0.12 {
                    flags += 1;
                    // New colors every time that part comes, so it stays the same part.
                    let colors = contrasting(la.hue_peak());
                    let spans: Vec<(u64, u64)> = sections
                        .iter()
                        .filter(|s| s.group == b.group)
                        .take(6)
                        .map(|s| (s.start, s.end))
                        .collect();
                    let mut cues: Vec<Value> = spans
                        .iter()
                        .map(|(at, until)| json!({ "cue": "color_shift", "at": at, "until": until, "colors": colors }))
                        .collect();
                    cues.extend(self.restaged(&spans));
                    self.fixes.cues(
                        rank::LOOK,
                        b.start,
                        format!(
                            "{} looks the same as {}: different parts of the song should look different.",
                            b.name, a.name
                        ),
                        cues,
                    );
                    break 'pairs;
                }
            }
        }
        self.criteria.consistency = Some(100u32.saturating_sub(25 * flags));
    }

    /// A stretch's look: its colors, the effects on each role's props, and which roles are lit.
    fn look(&self, from: u64, to: u64) -> Look {
        let f = &self.frames;
        let span = f.span(from, to);
        let mut hues = [0.0f32; HUES];
        for i in span.clone() {
            for (h, v) in hues.iter_mut().zip(f.hues[i]) {
                *h += v;
            }
        }
        let total: f32 = hues.iter().sum();
        if total > 0.0 {
            hues = hues.map(|h| h / total);
        }
        let roles = self.roles();
        let mut levels: BTreeMap<Role, (f32, usize)> = BTreeMap::new();
        for (p, prop) in f.props.iter().enumerate() {
            let role = roles.get(prop).copied().unwrap_or(Role::Other);
            let mean = span.clone().map(|i| f.prop(i, p)).sum::<f32>() / span.len().max(1) as f32;
            let entry = levels.entry(role).or_default();
            entry.0 += mean;
            entry.1 += 1;
        }
        let mut kinds = BTreeSet::new();
        let doc = self.subject.doc;
        for row in &doc.rows {
            let row_roles: BTreeSet<Role> = self
                .covers(row.target)
                .iter()
                .map(|p| roles.get(p).copied().unwrap_or(Role::Other))
                .collect();
            for e in row.layers.iter().flat_map(|l| &l.effects) {
                // Effects that play a good part of the stretch.
                let overlap = e.end_ms.min(to).saturating_sub(e.start_ms.max(from));
                if overlap * 4 >= (to - from).min(4 * self.song.bar()) {
                    for role in &row_roles {
                        kinds.insert(format!("{role:?}:{:?}", e.kind()));
                    }
                }
            }
        }
        Look {
            hues,
            light: total > 0.0,
            lit: total > 0.0 && f.mean(&f.level, span) > 0.02,
            levels: levels
                .into_iter()
                .map(|(r, (sum, n))| (r, sum / n.max(1) as f32))
                .collect(),
            kinds,
        }
    }

    fn roles(&self) -> HashMap<PropId, Role> {
        self.subject
            .show
            .props
            .iter()
            .map(|p| (p.id, role_of(p)))
            .collect()
    }

    /// The props a row lights.
    fn covers(&self, target: Target) -> Vec<PropId> {
        match target {
            Target::Prop(id) | Target::Region { prop: id, .. } => vec![id],
            Target::Group(id) => self
                .subject
                .show
                .groups
                .iter()
                .find(|g| g.id == id)
                .map(|g| g.members.iter().map(|m| m.prop()).collect())
                .unwrap_or_default(),
        }
    }

    // ---- variety, flashing, and safety ----

    fn variety(&mut self) {
        let mut score = 100u32;
        // Each prop's effects (by what they are, not when), from the rows that light it.
        let doc = self.subject.doc;
        let mut kinds: HashMap<String, u32> = HashMap::new();
        let mut on: HashMap<PropId, Vec<(u64, u64, u32)>> = HashMap::new();
        for row in &doc.rows {
            let props = self.covers(row.target);
            for e in row.layers.iter().flat_map(|l| &l.effects) {
                let key = serde_json::to_string(&(&e.params, &e.palette, e.blend)).unwrap_or_default();
                let next = kinds.len() as u32;
                let id = *kinds.entry(key).or_insert(next);
                for p in &props {
                    on.entry(*p).or_default().push((e.start_ms, e.end_ms, id));
                }
            }
        }
        let names: HashMap<PropId, &str> = self
            .subject
            .show
            .props
            .iter()
            .map(|p| (p.id, p.name.as_str()))
            .collect();
        let index: HashMap<PropId, usize> = self
            .frames
            .props
            .iter()
            .enumerate()
            .map(|(i, p)| (*p, i))
            .collect();
        let long = STALE_BARS * self.song.bar();
        let mut stale: BTreeMap<(u64, u64), Vec<&str>> = BTreeMap::new();
        for (prop, effects) in &on {
            for (a, b) in held(effects).into_iter().filter(|(a, b)| b - a > long) {
                let lit = index.get(prop).is_some_and(|&p| {
                    let span = self.frames.span(a, b);
                    let n = span.len().max(1) as f32;
                    span.map(|i| self.frames.prop(i, p)).sum::<f32>() / n > 0.05
                });
                if lit {
                    stale
                        .entry((a, b))
                        .or_default()
                        .push(names.get(prop).copied().unwrap_or("a prop"));
                }
            }
        }
        let mut runs: Vec<((u64, u64), Vec<&str>)> = stale.into_iter().collect();
        runs.sort_by(|x, y| (y.0.1 - y.0.0).cmp(&(x.0.1 - x.0.0)).then(x.0.cmp(&y.0)));
        score = score.saturating_sub(10 * runs.len().min(5) as u32);
        let bar = self.song.bar();
        for ((a, b), mut props) in runs.into_iter().take(2) {
            props.sort_unstable();
            let named = match props.len() {
                1 => props[0].to_string(),
                n if n <= 3 => props.join(", "),
                n => format!("{} and {} more props", props[..2].join(", "), n - 2),
            };
            let mid = a + (b - a) / 2 / bar * bar;
            let targets: Vec<&str> = props.iter().take(8).copied().collect();
            self.fixes.cue(
                rank::VARIETY,
                a,
                format!(
                    "{named} hold the same effect for {} bars ({}–{}): change it partway.",
                    (b - a) / bar,
                    format_ms(a),
                    format_ms(b)
                ),
                json!({ "cue": "color_shift", "at": mid, "until": b, "targets": targets }),
            );
        }
        // Flashes over the whole show.
        let f = &self.frames;
        let mut flips: Vec<(usize, i8)> = Vec::new();
        for i in 1..f.len() {
            let d = f.level[i] - f.level[i - 1];
            let field = if d > 0.0 { f.up[i] } else { f.down[i] };
            if d.abs() >= 0.1 && field >= 0.25 && f.level[i].min(f.level[i - 1]) <= 0.8 {
                let sign = if d > 0.0 { 1 } else { -1 };
                // A change over several frames in one direction is one change.
                if flips.last().is_some_and(|&(j, s)| s == sign && j + 1 == i) {
                    flips.last_mut().unwrap().0 = i;
                } else {
                    flips.push((i, sign));
                }
            }
        }
        // Alternations only: each change the other way from the one before; two make a flash.
        let mut turns: Vec<usize> = Vec::new();
        let mut last = 0i8;
        for (i, sign) in &flips {
            if *sign != last {
                turns.push(*i);
                last = *sign;
            }
        }
        let window = (FLASH_WINDOW_MS / f.step) as usize;
        let most = (FLASH_HZ * FLASH_WINDOW_MS as f32 / 1000.0) as usize;
        let mut unsafe_spans: Vec<(usize, usize, usize)> = Vec::new();
        let mut first = 0;
        for (k, &i) in turns.iter().enumerate() {
            while turns[first] + window <= i {
                first += 1;
            }
            let flashes = (k + 1 - first) / 2;
            if flashes > most {
                let from = turns[first];
                match unsafe_spans.last_mut() {
                    Some(span) if from <= span.1 => {
                        span.1 = i;
                        span.2 = span.2.max(flashes);
                    }
                    _ => unsafe_spans.push((from, i, flashes)),
                }
            }
        }
        self.safe = unsafe_spans.is_empty();
        self.criteria.safety = if self.safe { 100 } else { 0 };
        for &(a, b, flashes) in unsafe_spans.iter().take(2) {
            let (from, to) = (a as u64 * f.step, (b as u64 + 1) * f.step);
            let rate = flashes as f32 * 1000.0 / FLASH_WINDOW_MS as f32;
            self.fixes.add(
                rank::SAFETY,
                from,
                format!(
                    "The whole show flashes about {rate:.0} times a second ({}–{}): over 3 a second can trigger seizures in people sensitive to flashing.",
                    format_ms(from),
                    format_ms(to)
                ),
                Vec::new(),
                Some(format!(
                    "Slow the flashing there to under 3 a second, or flash only some props: list_sequence_effects from {from} to {to} ms, then change or remove the flashing effects."
                )),
            );
        }
        let playing = (0..f.len())
            .filter(|&i| self.song.playing(i as u64 * f.step))
            .count();
        let flashing = flips.len() as f32 / playing.max(1) as f32;
        if flashing > FLASHY_SHARE {
            score = score.saturating_sub(30);
            self.fixes.add(
                rank::VARIETY,
                0,
                format!(
                    "The show flashes in {:.0}% of the song: that tires the eye. Keep flashes for the moments that matter.",
                    flashing * 100.0
                ),
                Vec::new(),
                None,
            );
        }
        self.criteria.variety = score;
    }

    // ---- lyrics ----

    fn lyrics(&mut self) {
        let duration = self.subject.doc.duration_ms;
        let b = self.beat();
        let mut sung: Vec<(&str, u64, u64)> = self
            .hooks
            .iter()
            .flat_map(|(word, times)| times.iter().map(move |&(a, z)| (word.as_str(), a, z)))
            .filter(|(_, a, _)| *a < duration)
            .collect();
        sung.sort_by_key(|s| s.1);
        let mut missed: BTreeMap<&str, Vec<(u64, u64)>> = BTreeMap::new();
        let (mut checked, mut shown) = (0, 0);
        for &(word, a, z) in sung.iter().take(40) {
            checked += 1;
            if self.emphasis(a.saturating_sub(b / 4), a + b / 2) >= WORD_EMPHASIS {
                shown += 1;
            } else {
                missed.entry(word).or_default().push((a, z));
            }
        }
        if checked == 0 {
            return;
        }
        self.criteria.lyrics = Some(percent(shown as f32 / checked as f32));
        let mut missed: Vec<(&str, Vec<(u64, u64)>)> = missed.into_iter().collect();
        missed.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
        let doc = self.subject.doc;
        for (word, times) in missed.into_iter().take(2) {
            let (from, to) = (times[0].0, times[times.len() - 1].1 + b);
            let sung_times = sung.iter().filter(|s| s.0 == word).count();
            self.fixes.cue(
                rank::LYRICS,
                from,
                format!(
                    "The hook word \"{word}\" doesn't show {} of the {sung_times} times it's sung.",
                    times.len()
                ),
                json!({ "cue": "word_pop", "at": from, "until": to.min(doc.duration_ms), "match": word }),
            );
        }
    }

    // ---- dead air ----

    fn dead_air(&mut self) {
        let f = &self.frames;
        let b = self.beat();
        // Stops and breaks may go dark.
        let mut quiet: Vec<(u64, u64)> = self
            .song
            .moments
            .iter()
            .filter(|m| matches!(m.kind, MomentKind::Stop | MomentKind::Breakdown))
            .map(|m| (m.time_ms.saturating_sub(b), m.end_ms.unwrap_or(m.time_ms + b) + b))
            .collect();
        quiet.extend(
            self.song
                .sections
                .iter()
                .filter(|s| crate::song::label_root(&s.label).eq_ignore_ascii_case("break"))
                .map(|s| (s.start, s.end)),
        );
        let excused = |t: u64| quiet.iter().any(|(a, z)| (*a..*z).contains(&t));
        let shortest = (2 * b).max(1000);
        let mut spans: Vec<(u64, u64)> = Vec::new();
        let mut start: Option<u64> = None;
        let (mut dark_ms, mut music_ms) = (0u64, 0u64);
        for i in 0..=f.len() {
            let t = i as u64 * f.step;
            let counts = i < f.len() && self.song.playing(t) && !excused(t);
            if counts {
                music_ms += f.step;
            }
            let dark = counts && f.lit[i] < 0.01;
            match (dark, start) {
                (true, None) => start = Some(t),
                (false, Some(s)) => {
                    if t - s >= shortest {
                        spans.push((s, t));
                        dark_ms += t - s;
                    }
                    start = None;
                }
                _ => {}
            }
        }
        let share = dark_ms as f32 / music_ms.max(1) as f32;
        self.criteria.dead_air = percent(1.0 - 3.0 * share);
        let Some(&(a, z)) = spans
            .iter()
            .max_by(|x, y| (x.1 - x.0).cmp(&(y.1 - y.0)).then(y.0.cmp(&x.0)))
        else {
            return;
        };
        let what = if spans.len() == 1 {
            format!(
                "Nothing is lit from {} to {} while the music plays.",
                format_ms(a),
                format_ms(z)
            )
        } else {
            format!(
                "Nothing is lit for {:.0} s of music, in {} stretches (the longest {}–{}).",
                dark_ms as f64 / 1000.0,
                spans.len(),
                format_ms(a),
                format_ms(z)
            )
        };
        // Every stretch (the longest, if there are very many), in time order.
        let mut filled = spans.clone();
        filled.sort_by(|x, y| (y.1 - y.0).cmp(&(x.1 - x.0)).then(x.0.cmp(&y.0)));
        filled.truncate(24);
        filled.sort_unstable();
        // Cut at the sections, each in its part's color: the same music looks the same.
        let mut cues = Vec::new();
        for (a, z) in filled {
            let mut cuts: Vec<u64> = vec![a, z];
            cuts.extend(
                self.song
                    .sections
                    .iter()
                    .map(|s| s.start)
                    .filter(|t| (a + 1..z).contains(t)),
            );
            cuts.sort_unstable();
            for pair in cuts.windows(2) {
                let mut cue = json!({ "cue": "breathe", "at": pair[0], "until": pair[1] });
                if let Some(s) = self
                    .song
                    .sections
                    .iter()
                    .find(|s| (s.start..s.end).contains(&pair[0]))
                {
                    let n = s
                        .group
                        .bytes()
                        .next()
                        .map_or(0, |b| usize::from(b.saturating_sub(b'A')));
                    cue["colors"] = json!([PART_COLORS[n % PART_COLORS.len()]]);
                }
                cues.push(cue);
            }
        }
        self.fixes.cues(rank::DEAD, spans[0].0, what, cues);
    }

    // ---- restraint ----

    fn restraint(&mut self) {
        let duration = self.subject.doc.duration_ms;
        let landing: Vec<(usize, &Moment)> = self
            .song
            .moments
            .iter()
            .enumerate()
            .filter(|(_, m)| lands(m.kind) && m.kind != MomentKind::Build && m.time_ms < duration)
            .collect();
        let Some(&(top_i, top)) = landing.first() else {
            return;
        };
        let b = self.beat();
        let big = self.size(top.time_ms);
        let near_top: Vec<u64> = self
            .song
            .moments
            .iter()
            .take(TOP_MOMENTS)
            .map(|m| m.time_ms)
            .collect();
        let ordinary: Vec<u64> = self
            .song
            .bars
            .iter()
            .map(|b| b.0)
            .filter(|t| near_top.iter().all(|m| m.abs_diff(*t) > b))
            .collect();
        if ordinary.len() < 4 {
            return;
        }
        let as_big = ordinary
            .iter()
            .filter(|&&t| {
                let s = self.size(t);
                s >= EMPHASIS && s >= 0.9 * big
            })
            .count();
        let share = as_big as f32 / ordinary.len() as f32;
        self.criteria.restraint = Some(percent(1.0 - (share - 0.1).max(0.0) / 0.5));
        if share > 0.25 {
            self.fixes.cue(
                rank::RESTRAINT,
                top.time_ms,
                format!(
                    "{as_big} ordinary bars hit as hard as the song's biggest moment, the {} (m{top_i}): hold back elsewhere and make it the biggest.",
                    kind_name(top.kind)
                ),
                json!({ "cue": "hit", "at": format!("m{top_i}"), "intensity": 1.0 }),
            );
        }
    }
}

/// A stretch's look, for comparing sections.
struct Look {
    /// Its colors (shares of its light by color bin).
    hues: [f32; HUES],
    /// It has any light at all, and it's lit (more than a glimmer).
    light: bool,
    lit: bool,
    /// How bright each role's props are.
    levels: BTreeMap<Role, f32>,
    /// The effects on each role's props ("Tree:Spiral").
    kinds: BTreeSet<String>,
}

impl Look {
    /// 0 (alike) to 1 (nothing in common).
    fn distance(&self, other: &Look) -> f32 {
        let (hue, kinds, levels) = self.parts(other);
        (0.5 * hue + 0.3 * kinds + 0.2 * levels).clamp(0.0, 1.0)
    }

    /// How far apart the colors, the effects, and the roles' brightness are, each 0–1.
    fn parts(&self, other: &Look) -> (f32, f32, f32) {
        let hue = if self.light && other.light {
            1.0 - self
                .hues
                .iter()
                .zip(&other.hues)
                .map(|(a, b)| a.min(*b))
                .sum::<f32>()
        } else if self.light == other.light {
            0.0
        } else {
            1.0
        };
        let union = self.kinds.union(&other.kinds).count();
        let kinds = if union == 0 {
            0.0
        } else {
            1.0 - self.kinds.intersection(&other.kinds).count() as f32 / union as f32
        };
        let roles: Vec<f32> = self
            .levels
            .iter()
            .map(|(r, a)| {
                let b = other.levels.get(r).copied().unwrap_or(0.0);
                (a - b).abs() / a.max(b).max(0.15)
            })
            .collect();
        let levels = roles.iter().sum::<f32>() / roles.len().max(1) as f32;
        (hue, kinds, levels.min(1.0))
    }

    /// The color bin with the most light.
    fn hue_peak(&self) -> usize {
        self.hues
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1).then(b.0.cmp(&a.0)))
            .map_or(HUES - 1, |(i, _)| i)
    }
}

/// Two colors far from color bin `bin` (warm against cool), as "#rrggbb".
fn contrasting(bin: usize) -> Vec<String> {
    let base = if bin >= HUES - 1 { 0.0 } else { bin as f32 * 30.0 };
    [150.0, 210.0]
        .iter()
        .map(|turn| {
            let h = (base + turn) % 360.0;
            let x = 1.0 - ((h / 60.0) % 2.0 - 1.0).abs();
            let (r, g, b) = match (h / 60.0) as u32 {
                0 => (1.0, x, 0.0),
                1 => (x, 1.0, 0.0),
                2 => (0.0, 1.0, x),
                3 => (0.0, x, 1.0),
                4 => (x, 0.0, 1.0),
                _ => (1.0, 0.0, x),
            };
            let byte = |c: f32| (c * 255.0).round() as u8;
            format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
        })
        .collect()
}

/// The stretches where the same effects (by what they are) play, unchanged and not none:
/// `effects` are (start, end, what).
fn held(effects: &[(u64, u64, u32)]) -> Vec<(u64, u64)> {
    let mut changes: BTreeMap<u64, Vec<(u32, i32)>> = BTreeMap::new();
    for &(a, b, id) in effects {
        changes.entry(a).or_default().push((id, 1));
        changes.entry(b).or_default().push((id, -1));
    }
    let mut playing: BTreeMap<u32, i32> = BTreeMap::new();
    let mut runs = Vec::new();
    let mut since: Option<u64> = None;
    for (t, list) in changes {
        let before = playing.clone();
        for (id, d) in list {
            let n = playing.entry(id).or_default();
            *n += d;
            if *n == 0 {
                playing.remove(&id);
            }
        }
        if playing == before {
            continue;
        }
        if let Some(s) = since.take()
            && !before.is_empty()
        {
            runs.push((s, t));
        }
        if !playing.is_empty() {
            since = Some(t);
        }
    }
    runs
}

/// Pearson's correlation (0 when either doesn't vary).
fn correlation(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len()) as f32;
    if n < 2.0 {
        return 0.0;
    }
    let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        sab += (x - ma) * (y - mb);
        saa += (x - ma).powi(2);
        sbb += (y - mb).powi(2);
    }
    if saa <= f32::EPSILON || sbb <= f32::EPSILON {
        return 0.0;
    }
    sab / (saa * sbb).sqrt()
}

/// Values scaled so their 10th percentile is 0 and their 90th is 1.
fn normalized(values: &[f32]) -> Vec<f32> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    let at = |p: usize| sorted[(sorted.len() * p / 100).min(sorted.len() - 1)];
    let (lo, hi) = (at(10), at(90));
    values
        .iter()
        .map(|v| {
            if hi - lo <= 1e-4 {
                0.5
            } else {
                ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
            }
        })
        .collect()
}
