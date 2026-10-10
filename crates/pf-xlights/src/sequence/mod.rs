//! One-way import of an xLights sequence (`.xsq`) into an editable PixelFlow sequence.
//!
//! Effects land on rows targeting the show's props and groups by name (the names the layout
//! import keeps), timing tracks come across with their marks, and each xLights effect becomes the
//! PixelFlow effect closest to it, with its settings translated where there's a clear
//! equivalent. Nothing is approximated or left out silently: the report counts what came in
//! exactly, what was approximated (and how), and what was shown as a placeholder.

mod curves;
mod effects;
mod settings;
mod xsq;

pub use settings::{ParsedPalette, Settings, leading_number, parse_palette, unxml_safe};
pub use xsq::{
    ElementKind, MAX_XSQ_BYTES, XsqEffect, XsqElement, XsqFile, XsqHead, XsqLayer, XsqSubmodelLayer,
    parse_xsq,
};

use crate::XlightsError;
use crate::vendor::Mapping;
use effects::{Fidelity, Tally};
use pf_model::Show;
use pf_sequence::{
    Effect, EffectId, EffectParams, Layer, MAX_DURATION_MS, MAX_EFFECTS, MAX_FRAME_MS, MAX_LAYERS_PER_ROW,
    MAX_MARKS, MAX_ROWS, MAX_TEXT_LEN, MAX_TIMING_TRACKS, MIN_FRAME_MS, Mark, Row, RowId, Sequence, Target,
    TimingKind, TimingTrack, TimingTrackId,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Most names listed in one note.
const MAX_LISTED: usize = 20;
/// Frame time xLights falls back to when the file doesn't give a usable one.
const DEFAULT_FRAME_MS: u32 = 50;

/// Counts for the import report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceImportSummary {
    pub rows: usize,
    /// Effects imported (exact + approximate + placeholders).
    pub effects: usize,
    /// The same effect in PixelFlow, with every setting that changes its look translated.
    pub exact: usize,
    /// The closest PixelFlow effect, or the same effect with some settings left out.
    pub approximate: usize,
    /// No PixelFlow equivalent yet: kept as a dim fill in the effect's first color.
    pub placeholders: usize,
    /// xLights effects not imported (models or submodels not in the show, strands and nodes,
    /// outside the sequence, limits). Timing marks are counted in `marks_skipped`.
    pub skipped: usize,
    pub timing_tracks: usize,
    pub marks: usize,
    /// Marks in lyric tracks (phrases, words, and phonemes).
    pub lyric_marks: usize,
    /// Timing marks not imported (outside the sequence, limits).
    pub marks_skipped: usize,
}

/// The imported sequence and a plain-language report of anything not imported exactly.
#[derive(Debug, Clone, PartialEq)]
pub struct SequenceImport {
    pub sequence: Sequence,
    pub summary: SequenceImportSummary,
    pub notes: Vec<String>,
    /// The music file named in the sequence, as written (often a path on another computer).
    pub media_file: Option<String>,
}

/// `items` joined with commas, cut short after [`MAX_LISTED`].
pub(crate) fn list(items: &[String]) -> String {
    if items.len() <= MAX_LISTED {
        return items.join(", ");
    }
    format!(
        "{}, and {} more",
        items[..MAX_LISTED].join(", "),
        items.len() - MAX_LISTED
    )
}

pub(crate) fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// `text` cut to PixelFlow's longest name, counting how many were cut.
fn bounded(text: String, cut: &mut usize) -> String {
    if text.chars().count() <= MAX_TEXT_LEN {
        return text;
    }
    *cut += 1;
    text.chars().take(MAX_TEXT_LEN).collect()
}

/// Times in the file, converted to milliseconds and rounded to frames as xLights does.
struct Clock {
    fixed_point: bool,
    frame_ms: u64,
}

impl Clock {
    /// Milliseconds, rounded to the nearest frame; `None` when it's not a time or it's beyond
    /// PixelFlow's longest sequence.
    fn ms(&self, written: &str) -> Option<u64> {
        let value = leading_number(written).unwrap_or(0.0);
        let ms = if self.fixed_point { value } else { value * 1000.0 };
        if !ms.is_finite() || ms > MAX_DURATION_MS as f64 + self.frame_ms as f64 {
            return None;
        }
        let ms = ms.max(0.0).round() as u64;
        Some((ms + self.frame_ms / 2) / self.frame_ms * self.frame_ms)
    }
}

/// Effects (or marks) that don't fit the sequence's time span.
#[derive(Debug, Default)]
struct Drops {
    /// Ran past the end and were cut there.
    cut: usize,
    /// Started after the end (or beyond PixelFlow's longest sequence): left out.
    outside: usize,
    /// No length (xLights drops these too): left out.
    no_length: usize,
}

impl Drops {
    /// Notes about these drops; `what` is "effect" or "mark".
    fn notes(&self, what: &str, notes: &mut Vec<String>) {
        if self.cut > 0 {
            notes.push(format!(
                "{} ran past the end of the sequence and {} cut off there.",
                plural(self.cut, what),
                if self.cut == 1 { "was" } else { "were" }
            ));
        }
        if self.outside > 0 {
            notes.push(format!(
                "{} started after the end of the sequence, so {} left out.",
                plural(self.outside, what),
                if self.outside == 1 { "it was" } else { "they were" }
            ));
        }
        if self.no_length > 0 {
            notes.push(format!(
                "{} had no length (xLights skips {} too), so {} left out.",
                plural(self.no_length, what),
                if self.no_length == 1 { "it" } else { "them" },
                if self.no_length == 1 {
                    "it was"
                } else {
                    "they were"
                }
            ));
        }
    }
}

/// `fidelity` with one more thing that differs.
fn with_note(fidelity: Fidelity, note: String) -> Fidelity {
    match fidelity {
        Fidelity::Approximate(mut reasons) => {
            reasons.push(note);
            Fidelity::Approximate(reasons)
        }
        _ => Fidelity::Approximate(vec![note]),
    }
}

/// The face a Faces effect names, among the row's faces (`faces`, as PixelFlow named them).
/// "Default" (blank here) is the model's first face in name order, as xLights keeps its faces in
/// a sorted map. A face that clashed with a submodel's name was imported as "<name> (face)"; an
/// effect naming the original gets that face.
fn face_named(wanted: &str, faces: &[&str]) -> String {
    const RENAMED: &str = " (face)";
    let wanted = wanted.trim();
    if wanted.is_empty() {
        return faces
            .iter()
            .min_by_key(|f| f.strip_suffix(RENAMED).unwrap_or(f))
            .map_or_else(String::new, |f| f.to_string());
    }
    if faces.iter().any(|f| f.trim().eq_ignore_ascii_case(wanted)) {
        return wanted.to_string();
    }
    let renamed = format!("{wanted}{RENAMED}");
    faces
        .iter()
        .find(|f| f.trim().eq_ignore_ascii_case(&renamed))
        .map_or_else(|| wanted.to_string(), |f| f.to_string())
}

/// Builds the import while reading the file.
struct Builder<'a> {
    file: &'a XsqFile,
    clock: Clock,
    duration_ms: u64,
    targets: HashMap<&'a str, Target>,
    settings: HashMap<usize, Settings>,
    palettes: HashMap<usize, ParsedPalette>,
    tally: Tally,
    summary: SequenceImportSummary,
    notes: Vec<String>,
    names_cut: usize,
    effect_drops: Drops,
    mark_drops: Drops,
    random: usize,
    over_effect_limit: usize,
    bad_refs: usize,
    /// The track a Faces effect sings to, by xLights timing track name: its phonemes when it
    /// has them, else its words, else its lyrics. A track with none of those isn't here: xLights
    /// keeps the mouth at rest on it.
    face_tracks: HashMap<String, TimingTrackId>,
    /// Every timing track's xLights name.
    timing_tracks: HashSet<String>,
    /// The track a Shape effect fires on, by xLights timing track name: tracks with one layer
    /// (xLights fires shapes only on those).
    mark_tracks: HashMap<String, TimingTrackId>,
}

impl<'a> Builder<'a> {
    fn target(&self, name: &str) -> Option<Target> {
        self.targets
            .get(name)
            .or_else(|| self.targets.get(unxml_safe(name).as_str()))
            .copied()
    }

    fn settings_for(&mut self, effect: &XsqEffect) -> Settings {
        let Some(written) = &effect.settings_ref else {
            return Settings::parse(&effect.inline_settings);
        };
        match leading_number(written)
            .filter(|i| *i >= 0.0 && i.fract() == 0.0)
            .map(|i| i as usize)
            .filter(|&i| i < self.file.effect_db.len())
        {
            Some(i) => self
                .settings
                .entry(i)
                .or_insert_with(|| Settings::parse(&self.file.effect_db[i]))
                .clone(),
            None => {
                self.bad_refs += 1;
                Settings::default()
            }
        }
    }

    fn palette_for(&mut self, effect: &XsqEffect) -> ParsedPalette {
        let Some(written) = &effect.palette_ref else {
            return ParsedPalette {
                brightness: 100.0,
                ..ParsedPalette::default()
            };
        };
        match leading_number(written)
            .filter(|i| *i >= 0.0 && i.fract() == 0.0)
            .map(|i| i as usize)
            .filter(|&i| i < self.file.palettes.len())
        {
            Some(i) => self
                .palettes
                .entry(i)
                .or_insert_with(|| parse_palette(&self.file.palettes[i]))
                .clone(),
            None => {
                self.bad_refs += 1;
                ParsedPalette {
                    brightness: 100.0,
                    ..ParsedPalette::default()
                }
            }
        }
    }

    /// The span of an effect (or a timing mark, when `mark`), clipped to the sequence; `None`
    /// (counted) when it has no length or lies outside the sequence.
    fn span(&mut self, effect: &XsqEffect, mark: bool) -> Option<(u64, u64)> {
        let drops = if mark {
            &mut self.mark_drops
        } else {
            &mut self.effect_drops
        };
        let (Some(start), end) = (self.clock.ms(&effect.start), self.clock.ms(&effect.end)) else {
            drops.outside += 1;
            return None;
        };
        let end = end.unwrap_or(u64::MAX);
        if start >= end {
            drops.no_length += 1;
            return None;
        }
        if start >= self.duration_ms {
            drops.outside += 1;
            return None;
        }
        if end > self.duration_ms {
            drops.cut += 1;
        }
        Some((start, end.min(self.duration_ms)))
    }

    /// One effect, on a row whose props have the faces `faces` (by name, in show order).
    fn effect(&mut self, x: &XsqEffect, faces: &[&str]) -> Option<Effect> {
        let name = x.name.trim();
        if name == "Random" {
            self.random += 1;
            return None;
        }
        let (start_ms, end_ms) = self.span(x, false)?;
        if self.summary.effects >= MAX_EFFECTS {
            self.over_effect_limit += 1;
            return None;
        }
        let mut settings = self.settings_for(x);
        effects::adjust_for_version(name, &mut settings, &self.file.head.version);
        let palette = self.palette_for(x);
        let frame_ms = self.clock.frame_ms as u32;
        let Some(mut translated) = effects::translate(name, &settings, &palette, end_ms - start_ms, frame_ms)
        else {
            self.tally.record(name, &Fidelity::Skipped);
            self.summary.skipped += 1;
            return None;
        };
        if let EffectParams::Faces(params) = &mut translated.params {
            params.face = face_named(&params.face, faces);
            let wanted = unxml_safe(settings.text("E_CHOICE_Faces_TimingTrack", "").trim());
            params.timing_track = self.face_tracks.get(wanted.as_str()).copied();
            if params.timing_track.is_none() && !wanted.is_empty() {
                let missing = if self.timing_tracks.contains(&wanted) {
                    "its timing track has no lyrics, so the mouth stays at rest, as in xLights"
                } else {
                    "its timing track isn't in the sequence, so the mouth stays at rest"
                }
                .to_string();
                translated.fidelity = with_note(translated.fidelity, missing);
            }
        }
        if let EffectParams::Shape(params) = &mut translated.params
            && settings.flag("E_CHECKBOX_Shape_FireTiming", false)
        {
            let wanted = unxml_safe(settings.text("E_CHOICE_Shape_FireTimingTrack", "").trim());
            params.timing_track = self.mark_tracks.get(wanted.as_str()).copied();
            if params.timing_track.is_none() && !wanted.is_empty() {
                let missing = if self.timing_tracks.contains(&wanted) {
                    "its timing track has more than one layer, so shapes are shown as a steady stream"
                } else {
                    "its timing track isn't in the sequence, so shapes are shown as a steady stream"
                }
                .to_string();
                translated.fidelity = with_note(translated.fidelity, missing);
            }
        }
        if let EffectParams::Chase(params) = &mut translated.params {
            let wanted = unxml_safe(settings.text("E_CHOICE_SingleStrand_TimingTrack", "").trim());
            params.timing_track = self.mark_tracks.get(wanted.as_str()).copied();
            if params.timing_track.is_none() && !wanted.is_empty() {
                let missing = if self.timing_tracks.contains(&wanted) {
                    "its timing track has more than one layer, so it chases at its speed"
                } else {
                    "its timing track isn't in the sequence, so it chases at its speed"
                }
                .to_string();
                translated.fidelity = with_note(translated.fidelity, missing);
            }
        }
        if let EffectParams::VuMeter(params) = &mut translated.params
            && params.meter.uses_marks()
        {
            let wanted = unxml_safe(settings.text("E_CHOICE_VUMeter_TimingTrack", "").trim());
            params.timing_track = self.mark_tracks.get(wanted.as_str()).copied();
            if params.timing_track.is_none() && !wanted.is_empty() {
                let missing = if self.timing_tracks.contains(&wanted) {
                    "its timing track has more than one layer, so it shows nothing"
                } else {
                    "its timing track isn't in the sequence, so it shows nothing"
                }
                .to_string();
                translated.fidelity = with_note(translated.fidelity, missing);
            }
        }
        for (key, track) in std::mem::take(&mut translated.curve_tracks) {
            let found = self.mark_tracks.get(unxml_safe(&track).as_str()).copied();
            if let Some(curve) = translated.curves.get_mut(&key) {
                curve.timing_track = found;
            }
            if found.is_none() {
                translated.fidelity = with_note(
                    translated.fidelity,
                    "a setting's timing track isn't in the sequence, so the setting sits at its middle"
                        .into(),
                );
            }
        }
        self.tally.record(name, &translated.fidelity);
        self.summary.effects += 1;
        match translated.fidelity {
            Fidelity::Exact => self.summary.exact += 1,
            Fidelity::Approximate(_) => self.summary.approximate += 1,
            Fidelity::Placeholder => self.summary.placeholders += 1,
            Fidelity::Skipped => {}
        }
        Some(Effect {
            id: EffectId::new(),
            start_ms,
            end_ms,
            params: translated.params,
            palette: translated.palette,
            blend: translated.blend,
            fade_in_ms: translated.fade_in_ms,
            fade_out_ms: translated.fade_out_ms,
            sparkles: translated.sparkles,
            sparkle_color: translated.sparkle_color,
            music_sparkles: translated.music_sparkles,
            blur: translated.blur,
            render_style: translated.render_style,
            buffer_transform: translated.buffer_transform,
            curves: translated.curves,
        })
    }

    fn marks(&mut self, layer: &XsqLayer, budget: usize, lost: &mut usize) -> Vec<Mark> {
        let mut marks = Vec::new();
        for x in &layer.effects {
            let Some((start, end)) = self.span(x, true) else {
                continue;
            };
            if marks.len() >= budget {
                *lost += 1;
                continue;
            }
            let label = bounded(unxml_safe(&x.name), &mut self.names_cut);
            marks.push(Mark::new(start, end, label));
        }
        marks
    }
}

/// Adds a row of `layers` on `target` to the sequence, or (the same model twice) adds them
/// beneath the existing row's layers, as xLights does.
fn place_row(
    sequence: &mut Sequence,
    row_index: &mut HashMap<Target, usize>,
    rows_lost: &mut usize,
    summary: &mut SequenceImportSummary,
    target: Target,
    layers: Vec<Layer>,
) {
    match row_index.get(&target) {
        Some(&at) => {
            let row: &mut Row = &mut sequence.rows[at];
            let room = MAX_LAYERS_PER_ROW.saturating_sub(row.layers.len());
            let extra: Vec<Layer> = layers.into_iter().rev().take(room).rev().collect();
            row.layers.splice(0..0, extra);
        }
        None if sequence.rows.len() >= MAX_ROWS => {
            *rows_lost += 1;
            summary.skipped += layers.iter().map(|l| l.effects.len()).sum::<usize>();
        }
        None => {
            row_index.insert(target, sequence.rows.len());
            sequence.rows.push(Row {
                id: RowId::new(),
                target,
                layers,
            });
        }
    }
}

/// Names with counts, in first-seen order, each name once.
#[derive(Debug, Default)]
struct Named {
    entries: Vec<(String, usize)>,
    index: HashMap<String, usize>,
}

impl Named {
    fn add(&mut self, name: String, count: usize) {
        match self.index.get(&name) {
            Some(&at) => self.entries[at].1 += count,
            None => {
                self.index.insert(name.clone(), self.entries.len());
                self.entries.push((name, count));
            }
        }
    }

    fn contains(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// "A (2 effects), B (1 effect)", cut short after [`MAX_LISTED`].
    fn list(&self, what: &str) -> String {
        let names: Vec<String> = self
            .entries
            .iter()
            .map(|(n, c)| format!("{n} ({})", plural(*c, what)))
            .collect();
        list(&names)
    }
}

/// True when every mark of `inner` lies within a mark of `outer` (as lyric words lie within
/// phrases, and phonemes within words). An empty `inner` doesn't count as nested.
fn nested(inner: &XsqLayer, outer: &XsqLayer, clock: &Clock) -> bool {
    let span = |e: &XsqEffect| Some((clock.ms(&e.start)?, clock.ms(&e.end)?));
    let mut outer: Vec<(u64, u64)> = outer.effects.iter().filter_map(span).collect();
    outer.sort_unstable();
    !inner.effects.is_empty()
        && inner.effects.iter().all(|e| {
            span(e).is_some_and(|(start, end)| {
                let at = outer.partition_point(|&(s, _)| s <= start);
                // Marks on one layer don't overlap, so only the last one starting before can
                // hold it.
                at > 0 && end <= outer[at - 1].1
            })
        })
}

/// A lyric track as xLights' Papagayo import makes it: labelled phrases on the first layer,
/// words within them on the second, and (optionally) phonemes within the words on the third.
fn is_lyric_track(element: &XsqElement, clock: &Clock) -> bool {
    let layers = &element.layers;
    layers.len() >= 2
        && layers[0].effects.iter().any(|e| !e.name.trim().is_empty())
        && nested(&layers[1], &layers[0], clock)
        && layers
            .get(2)
            .is_none_or(|l| l.effects.is_empty() || nested(l, &layers[1], clock))
}

/// The show's prop, group, or submodel (`"Prop/Submodel"`) called `name`. A prop and a group of
/// the same name: the prop.
fn target_named(show: &Show, by_name: &HashMap<&str, Target>, name: &str) -> Option<Target> {
    if let Some(target) = by_name.get(name) {
        return Some(*target);
    }
    name.match_indices('/').find_map(|(at, _)| {
        let (prop, region) = (&name[..at], &name[at + 1..]);
        let Some(Target::Prop(id)) = by_name.get(prop) else {
            return None;
        };
        let found = show
            .prop(*id)?
            .regions
            .iter()
            .find(|r| r.is_submodel() && r.name == region)?;
        Some(Target::Region {
            prop: *id,
            region: found.id,
        })
    })
}

/// Effect layers by xLights' layer number (missing numbers are empty layers; the same number
/// twice is one layer). Effects on layers past PixelFlow's limit are counted in `lost`.
fn numbered_layers<'f>(layers: &[&'f XsqSubmodelLayer], lost: &mut usize) -> Vec<Vec<&'f XsqEffect>> {
    let mut x_layers: Vec<Vec<&XsqEffect>> = Vec::new();
    for sub in layers {
        if sub.layer >= MAX_LAYERS_PER_ROW {
            *lost += sub.effects.len();
            continue;
        }
        if x_layers.len() <= sub.layer {
            x_layers.resize(sub.layer + 1, Vec::new());
        }
        x_layers[sub.layer].extend(&sub.effects);
    }
    x_layers
}

/// `layers` grouped by name, in the order each name first appears.
fn by_name(layers: &[XsqSubmodelLayer]) -> Vec<(&str, Vec<&XsqSubmodelLayer>)> {
    let mut out: Vec<(&str, Vec<&XsqSubmodelLayer>)> = Vec::new();
    for layer in layers {
        match out.iter_mut().find(|(n, _)| *n == layer.name) {
            Some((_, list)) => list.push(layer),
            None => out.push((&layer.name, vec![layer])),
        }
    }
    out
}

/// Builds a PixelFlow sequence from a parsed `.xsq` file for `show` (rows target its props and
/// groups by name). `fallback_name` names the sequence when the file has no song title.
pub fn build_sequence(file: &XsqFile, show: &Show, fallback_name: &str) -> SequenceImport {
    build(file, show, fallback_name, None)
}

/// [`build_sequence`] with each model, group, submodel, and strand's effects going where
/// `mapping` says (see [`Mapping`]), as xLights' Import Effects does: an item mapped to several
/// targets is copied to each, and items mapped to the same target are layered on it in the
/// sequence's order, the first on top. Items the mapping leaves out are skipped (and reported).
pub fn build_sequence_mapped(
    file: &XsqFile,
    show: &Show,
    fallback_name: &str,
    mapping: &Mapping,
) -> SequenceImport {
    build(file, show, fallback_name, Some(mapping))
}

fn build(file: &XsqFile, show: &Show, fallback_name: &str, mapping: Option<&Mapping>) -> SequenceImport {
    let mut notes = file.notes.clone();

    let frame_ms = match leading_number(&file.head.timing) {
        Some(ms) if ms >= 1.0 && ms.is_finite() => {
            let ms = ms as u32;
            let clamped = ms.clamp(MIN_FRAME_MS, MAX_FRAME_MS);
            if clamped != ms {
                notes.push(format!(
                    "The sequence's frames are {ms} ms apart; PixelFlow uses {MIN_FRAME_MS} to {MAX_FRAME_MS} ms, so it plays at {clamped} ms."
                ));
            }
            clamped
        }
        _ => {
            notes.push(format!(
                "The sequence doesn't say how far apart its frames are, so it plays at {DEFAULT_FRAME_MS} ms (xLights does the same)."
            ));
            DEFAULT_FRAME_MS
        }
    };
    let clock = Clock {
        fixed_point: file.fixed_point,
        frame_ms: u64::from(frame_ms),
    };

    // Duration: the file's (seconds), else the end of the last effect or mark.
    let written = leading_number(&file.head.duration)
        .filter(|s| s.is_finite() && *s > 0.0)
        .map(|s| (s * 1000.0).round());
    let duration_ms = match written {
        Some(ms) if ms <= MAX_DURATION_MS as f64 => ms as u64,
        Some(_) => {
            notes.push(
                "The sequence is longer than 4 hours, PixelFlow's longest; everything after 4 hours was left out."
                    .to_string(),
            );
            MAX_DURATION_MS
        }
        None => {
            let last = file
                .elements
                .iter()
                .flat_map(|e| &e.layers)
                .flat_map(|l| &l.effects)
                .filter_map(|e| clock.ms(&e.end))
                .max()
                .unwrap_or(0);
            notes.push(format!(
                "The sequence doesn't give its length, so it ends with its last effect ({}).",
                pf_sequence::format_ms(last)
            ));
            last
        }
    }
    .max(u64::from(frame_ms));

    let mut targets: HashMap<&str, Target> = HashMap::new();
    for group in &show.groups {
        targets.insert(group.name.as_str(), Target::Group(group.id));
    }
    for prop in &show.props {
        targets.insert(prop.name.as_str(), Target::Prop(prop.id));
    }

    let mut b = Builder {
        file,
        clock,
        duration_ms,
        targets,
        settings: HashMap::new(),
        palettes: HashMap::new(),
        tally: Tally::default(),
        summary: SequenceImportSummary::default(),
        notes: Vec::new(),
        names_cut: 0,
        effect_drops: Drops::default(),
        mark_drops: Drops::default(),
        random: 0,
        over_effect_limit: 0,
        bad_refs: 0,
        face_tracks: HashMap::new(),
        timing_tracks: HashSet::new(),
        mark_tracks: HashMap::new(),
    };

    let mut sequence = Sequence::new(fallback_name, duration_ms);
    sequence.frame_ms = frame_ms;
    let song = unxml_safe(file.head.song.trim());
    if !song.trim().is_empty() {
        sequence.name = song.trim().to_string();
    }
    sequence.name = bounded(std::mem::take(&mut sequence.name), &mut b.names_cut);

    // Timing tracks.
    let mut marks_left = MAX_MARKS;
    let mut marks_lost = 0;
    let mut tracks_lost = 0;
    let mut timing_names: HashSet<&str> = HashSet::new();
    for element in file.elements.iter().filter(|e| e.kind == ElementKind::Timing) {
        let name = unxml_safe(&element.name);
        timing_names.insert(&element.name);
        b.timing_tracks.insert(name.clone());
        let mut tracks = Vec::new();
        let interval = element
            .fixed
            .as_deref()
            .and_then(leading_number)
            .filter(|n| n.is_finite() && *n >= 1.0);
        if let Some(interval) = interval {
            // A mark every `interval` ms, rounded to frames (at least one frame), like xLights.
            let frame = u64::from(frame_ms);
            let step =
                (((interval.min(MAX_DURATION_MS as f64) as u64) + frame / 2) / frame * frame).max(frame);
            let count = duration_ms.div_ceil(step) as usize;
            let kept = count.min(marks_left);
            marks_lost += count - kept;
            marks_left -= kept;
            let marks = (0..kept as u64)
                .map(|i| Mark::new(i * step, ((i + 1) * step).min(duration_ms), ""))
                .collect();
            tracks.push(TimingTrack::new(name.clone(), TimingKind::Custom, marks));
        } else {
            let lyric = is_lyric_track(element, &b.clock);
            for (i, layer) in element.layers.iter().enumerate() {
                if i > 0 && layer.effects.is_empty() {
                    continue;
                }
                let (kind, track_name) = match (lyric, i) {
                    (_, 0) if !lyric => (TimingKind::Custom, name.clone()),
                    (true, 0) => (TimingKind::Lyrics, name.clone()),
                    (true, 1) => (TimingKind::Words, format!("{name} (words)")),
                    (true, 2) => (TimingKind::Phonemes, format!("{name} (phonemes)")),
                    _ => (TimingKind::Custom, format!("{name} layer {}", i + 1)),
                };
                let marks = b.marks(layer, marks_left, &mut marks_lost);
                marks_left -= marks.len();
                tracks.push(TimingTrack::new(track_name, kind, marks));
            }
        }
        let sings = [TimingKind::Phonemes, TimingKind::Words, TimingKind::Lyrics]
            .iter()
            .find_map(|kind| tracks.iter().find(|t| t.kind == *kind))
            .map(|t| t.id);
        let room = MAX_TIMING_TRACKS.saturating_sub(sequence.timing_tracks.len());
        if let Some(id) = sings
            && tracks.iter().take(room).any(|t| t.id == id)
        {
            b.face_tracks.insert(name.clone(), id);
        }
        if (interval.is_some() || element.layers.len() == 1)
            && let Some(first) = tracks.first()
            && room > 0
        {
            b.mark_tracks.insert(name.clone(), first.id);
        }
        for mut track in tracks {
            if sequence.timing_tracks.len() >= MAX_TIMING_TRACKS {
                tracks_lost += 1;
                marks_lost += track.marks.len();
                continue;
            }
            track.name = bounded(track.name, &mut b.names_cut);
            let lyric = matches!(
                track.kind,
                TimingKind::Lyrics | TimingKind::Words | TimingKind::Phonemes
            );
            b.summary.marks += track.marks.len();
            if lyric {
                b.summary.lyric_marks += track.marks.len();
            }
            sequence.timing_tracks.push(track);
        }
    }
    if marks_lost > 0 {
        b.summary.marks_skipped += marks_lost;
        b.notes.push(format!(
            "The sequence has more timing marks than PixelFlow's limit of {MAX_MARKS}; {} were left out.",
            plural(marks_lost, "mark")
        ));
    }
    if tracks_lost > 0 {
        b.notes.push(format!(
            "The sequence has more timing tracks than PixelFlow's limit of {MAX_TIMING_TRACKS}; {} were left out.",
            plural(tracks_lost, "track")
        ));
    }
    b.summary.timing_tracks = sequence.timing_tracks.len();

    // Rows.
    let mut unmatched = Named::default();
    let mut unmatched_empty = 0;
    let mut shadowed = Named::default();
    let mut strands: Vec<(String, usize)> = Vec::new();
    let mut missing_submodels = Named::default();
    // Props a mapping names that aren't in the show.
    let mut missing_targets = Named::default();
    let mut rows_lost = 0;
    let mut layers_lost = 0;
    let mut row_index: HashMap<Target, usize> = HashMap::new();
    let mut other_elements = 0;
    let mut other_effects = 0;
    for element in &file.elements {
        let count: usize = element.layers.iter().map(|l| l.effects.len()).sum();
        let below = element.sub_effects + element.submodel_effects();
        match element.kind {
            ElementKind::Timing => continue,
            ElementKind::Other => {
                other_elements += 1;
                other_effects += count + below;
                b.summary.skipped += count + below;
                continue;
            }
            ElementKind::Model => {}
        }
        if timing_names.contains(element.name.as_str()) {
            // xLights reads such an element as the timing track of the same name.
            shadowed.add(unxml_safe(&element.name), count + below);
            b.summary.skipped += count + below;
            continue;
        }
        let model_layers: Vec<Vec<&XsqEffect>> = element
            .layers
            .iter()
            .map(|l| l.effects.iter().collect())
            .collect();
        // The model's rows, then those of its submodels (drawn over the model, as in xLights),
        // in the order they first appear.
        let mut rows: Vec<(Target, Vec<Vec<&XsqEffect>>)> = Vec::new();
        if let Some(mapping) = mapping {
            let element_name = unxml_safe(&element.name);
            // Where `item` goes in the show (each target once, in the mapping's order).
            let mut targets_of = |item: &str| -> Vec<Target> {
                let names = mapping
                    .targets(item)
                    .or_else(|| mapping.targets(&unxml_safe(item)))
                    .unwrap_or_default();
                let mut found: Vec<Target> = Vec::new();
                for name in names {
                    match target_named(show, &b.targets, name) {
                        Some(t) if !found.contains(&t) => found.push(t),
                        Some(_) => {}
                        None => missing_targets.add(name.clone(), 0),
                    }
                }
                found
            };
            let targets = targets_of(&element.name);
            if targets.is_empty() && count > 0 {
                unmatched.add(element_name.clone(), count);
                b.summary.skipped += count;
            }
            for target in targets {
                rows.push((target, model_layers.clone()));
            }
            for layers in [&element.submodels, &element.strands] {
                for (name, subs) in by_name(layers) {
                    let lost: usize = subs.iter().map(|s| s.effects.len()).sum();
                    let targets = targets_of(&format!("{}/{name}", element.name));
                    if targets.is_empty() {
                        missing_submodels.add(format!("{element_name}/{}", unxml_safe(name)), lost);
                        b.summary.skipped += lost;
                        continue;
                    }
                    let mut lost_here = 0;
                    let x_layers = numbered_layers(&subs, &mut lost_here);
                    layers_lost += lost_here;
                    b.summary.skipped += lost_here;
                    for target in targets {
                        rows.push((target, x_layers.clone()));
                    }
                }
            }
            let strand_effects: usize = element.strands.iter().map(|s| s.effects.len()).sum();
            let nodes = element.sub_effects.saturating_sub(strand_effects);
            if nodes > 0 {
                strands.push((element_name, nodes));
                b.summary.skipped += nodes;
            }
        } else {
            let Some(target) = b.target(&element.name) else {
                let total = count + below;
                let name = unxml_safe(&element.name);
                if total == 0 && !unmatched.contains(&name) {
                    unmatched_empty += 1;
                } else {
                    unmatched.add(name, total);
                }
                b.summary.skipped += total;
                continue;
            };
            if element.sub_effects > 0 {
                strands.push((unxml_safe(&element.name), element.sub_effects));
                b.summary.skipped += element.sub_effects;
            }
            rows.push((target, model_layers));
            for (name, subs) in by_name(&element.submodels) {
                let region = target.prop().and_then(|prop| {
                    let wanted = unxml_safe(name);
                    let found = show
                        .prop(prop)?
                        .regions
                        .iter()
                        .find(|r| r.is_submodel() && (r.name == name || r.name == wanted))?;
                    Some(Target::Region {
                        prop,
                        region: found.id,
                    })
                });
                let Some(region) = region else {
                    let lost: usize = subs.iter().map(|s| s.effects.len()).sum();
                    missing_submodels.add(
                        format!("{}/{}", unxml_safe(&element.name), unxml_safe(name)),
                        lost,
                    );
                    b.summary.skipped += lost;
                    continue;
                };
                let mut lost_here = 0;
                let x_layers = numbered_layers(&subs, &mut lost_here);
                layers_lost += lost_here;
                b.summary.skipped += lost_here;
                rows.push((region, x_layers));
            }
        }
        for (target, x_layers) in rows {
            // The faces a Faces effect on this row can name (a model's, or its submodel's model's).
            let faces: Vec<&str> = target
                .prop()
                .and_then(|id| show.prop(id))
                .map(|prop| {
                    prop.regions
                        .iter()
                        .filter(|r| matches!(r.kind, pf_model::RegionKind::Face(_)))
                        .map(|r| r.name.as_str())
                        .collect()
                })
                .unwrap_or_default();
            // xLights' first layer is drawn on top; PixelFlow draws its last layer on top.
            let mut layers: Vec<Layer> = Vec::with_capacity(x_layers.len().max(1));
            for (i, x_layer) in x_layers.iter().enumerate().rev() {
                if i >= MAX_LAYERS_PER_ROW {
                    layers_lost += x_layer.len();
                    b.summary.skipped += x_layer.len();
                    continue;
                }
                let effects = x_layer.iter().filter_map(|x| b.effect(x, &faces)).collect();
                layers.push(Layer { effects });
            }
            if layers.is_empty() {
                layers.push(Layer::default());
            }
            place_row(
                &mut sequence,
                &mut row_index,
                &mut rows_lost,
                &mut b.summary,
                target,
                layers,
            );
        }
    }
    b.summary.rows = sequence.rows.len();

    // Notes, most important first.
    let tally_notes = b.tally.notes();
    b.notes.splice(0..0, tally_notes);
    if !unmatched.is_empty() {
        b.notes.insert(
            0,
            if mapping.is_some() {
                format!(
                    "These xLights models weren't mapped to anything, so their effects weren't imported: {}.",
                    unmatched.list("effect")
                )
            } else {
                format!(
                    "These xLights models aren't in the show, so their effects weren't imported: {}.",
                    unmatched.list("effect")
                )
            },
        );
    }
    if !missing_targets.is_empty() {
        let names: Vec<String> = missing_targets.entries.iter().map(|(n, _)| n.clone()).collect();
        b.notes.insert(
            0,
            format!(
                "The mapping names props that aren't in the show, so nothing went to them: {}.",
                list(&names)
            ),
        );
    }
    if unmatched_empty > 0 {
        b.notes.push(format!(
            "{} in the sequence {} in the show; {} no effects, so nothing was lost.",
            plural(unmatched_empty, "model"),
            if unmatched_empty == 1 { "isn't" } else { "aren't" },
            if unmatched_empty == 1 {
                "it had"
            } else {
                "they had"
            }
        ));
    }
    if !shadowed.is_empty() {
        b.notes.push(format!(
            "These models have the same name as a timing track, so xLights reads them as that track and their effects weren't imported: {}.",
            shadowed.list("effect")
        ));
    }
    if other_elements > 0 {
        b.notes.push(format!(
            "{} in the sequence {} neither a model nor a timing track, so {} left out ({}).",
            plural(other_elements, "element"),
            if other_elements == 1 { "is" } else { "are" },
            if other_elements == 1 {
                "it was"
            } else {
                "they were"
            },
            plural(other_effects, "effect")
        ));
    }
    if !missing_submodels.is_empty() {
        b.notes.push(if mapping.is_some() {
            format!(
                "These submodels and strands weren't mapped to anything, so their effects weren't imported: {}.",
                missing_submodels.list("effect")
            )
        } else {
            format!(
                "These submodels aren't in the show, so their effects weren't imported: {}.",
                missing_submodels.list("effect")
            )
        });
    }
    if !strands.is_empty() && mapping.is_some() {
        let names: Vec<String> = strands
            .iter()
            .map(|(n, c)| format!("{n} ({})", plural(*c, "effect")))
            .collect();
        b.notes.push(format!(
            "PixelFlow doesn't import effects on single nodes yet; these weren't imported: {}.",
            list(&names)
        ));
    } else if !strands.is_empty() {
        let names: Vec<String> = strands
            .iter()
            .map(|(n, c)| format!("{n} ({})", plural(*c, "effect")))
            .collect();
        b.notes.push(format!(
            "PixelFlow doesn't import effects on strands or single nodes yet; these weren't imported: {}.",
            list(&names)
        ));
    }
    if rows_lost > 0 {
        b.notes.push(format!(
            "The sequence has more models than PixelFlow's limit of {MAX_ROWS} rows; {} weren't imported.",
            plural(rows_lost, "model")
        ));
    }
    if layers_lost > 0 {
        b.notes.push(format!(
            "Some models have more than {MAX_LAYERS_PER_ROW} layers; {} on the extra layers weren't imported.",
            plural(layers_lost, "effect")
        ));
    }
    if b.over_effect_limit > 0 {
        b.summary.skipped += b.over_effect_limit;
        b.notes.push(format!(
            "The sequence has more effects than PixelFlow's limit of {MAX_EFFECTS}; {} weren't imported.",
            plural(b.over_effect_limit, "effect")
        ));
    }
    if file.effects_unread + file.marks_unread > 0 {
        b.summary.skipped += file.effects_unread;
        b.summary.marks_skipped += file.marks_unread;
        b.notes.push(format!(
            "The sequence is too large to read completely; {} and {} weren't imported.",
            plural(file.effects_unread, "effect"),
            plural(file.marks_unread, "timing mark")
        ));
    }
    b.summary.skipped += b.effect_drops.outside;
    b.summary.marks_skipped += b.mark_drops.outside;
    b.effect_drops.notes("effect", &mut b.notes);
    b.mark_drops.notes("timing mark", &mut b.notes);
    if b.random > 0 {
        b.notes.push(format!(
            "{} xLights' Random effect (xLights doesn't load {} either), so {} left out.",
            if b.random == 1 {
                "1 effect is"
            } else {
                "Some effects are"
            },
            if b.random == 1 { "it" } else { "them" },
            if b.random == 1 { "it was" } else { "they were" }
        ));
    }
    if b.bad_refs > 0 {
        b.notes.push(format!(
            "{} referred to settings or colors missing from the file, so {} defaults.",
            plural(b.bad_refs, "effect"),
            if b.bad_refs == 1 { "it uses" } else { "they use" }
        ));
    }
    if b.names_cut > 0 {
        b.notes.push(format!(
            "{} longer than {MAX_TEXT_LEN} characters {} shortened.",
            plural(b.names_cut, "name or label"),
            if b.names_cut == 1 { "was" } else { "were" }
        ));
    }
    notes.extend(b.notes);

    let media = file.head.media_file.trim();
    SequenceImport {
        sequence,
        summary: b.summary,
        notes,
        media_file: (!media.is_empty()).then(|| media.to_string()),
    }
}

/// Imports the `.xsq` file at `path` for `show`. `find_audio` locates its music from the
/// sequence's path and the file it names (PixelFlow's `pf_audio::find_audio`); the sequence is
/// named after its song, or the file when it has none. Its pictures are looked for near it (see
/// [`find_pictures`]).
pub fn import_sequence_file(
    path: &Path,
    show: &Show,
    find_audio: impl Fn(&Path, Option<&str>) -> Option<PathBuf>,
) -> Result<SequenceImport, XlightsError> {
    let file = read_sequence_file(path)?;
    let mut import = build_sequence(&file, show, &name_from_path(path));
    find_music(&mut import, &file, path, find_audio);
    find_pictures(&mut import, Some(path));
    Ok(import)
}

/// The folders an xLights sequence's pictures are looked for in: the sequence's own, then the
/// ones above it up to the show folder (the one with the layout), or three up when none is.
fn picture_folders(sequence: &Path) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    for folder in sequence.ancestors().skip(1).take(4) {
        if folder.as_os_str().is_empty() {
            break;
        }
        folders.push(folder.to_path_buf());
        if folder.join("xlights_rgbeffects.xml").is_file() {
            break;
        }
    }
    folders
}

/// Points the import's Picture effects at their files on this computer (the sequence was read
/// from `path`): each where the sequence says, else by its name in the xLights show folder or
/// under it with the old show folder's part of the path removed, the way the layout's photo is
/// found. One that isn't found keeps what the sequence says, and a note names it.
pub(crate) fn find_pictures(import: &mut SequenceImport, path: Option<&Path>) {
    let folders = path.map(picture_folders).unwrap_or_default();
    let mut found: HashMap<String, Option<String>> = HashMap::new();
    let mut lost: Vec<String> = Vec::new();
    let effects = import
        .sequence
        .rows
        .iter_mut()
        .flat_map(|row| &mut row.layers)
        .flat_map(|layer| &mut layer.effects);
    for effect in effects {
        let EffectParams::Picture(p) = &mut effect.params else {
            continue;
        };
        if p.file.is_empty() {
            continue;
        }
        let here = found.entry(p.file.clone()).or_insert_with(|| {
            folders
                .iter()
                .find_map(|folder| crate::background::find(&p.file, folder))
                .map(|at| pf_model::path_to_text(&std::path::absolute(&at).unwrap_or(at)))
                .filter(|text| text.chars().count() <= MAX_TEXT_LEN)
        });
        match here {
            Some(text) => p.file = text.clone(),
            None => {
                let name = pf_model::file_name_of(&p.file);
                if !lost.contains(&name) {
                    lost.push(name);
                }
            }
        }
    }
    if !lost.is_empty() {
        let (what, them) = if lost.len() == 1 {
            ("a picture", "it")
        } else {
            ("pictures", "them")
        };
        import.notes.insert(
            0,
            format!(
                "Couldn't find {what} near the sequence ({}); choose {them} in the Picture effects' settings.",
                list(&lost)
            ),
        );
    }
}

/// The too-large message for a sequence of `size` bytes.
pub(crate) fn too_large(size: u64) -> XlightsError {
    XlightsError::BadFile(
        "The xLights sequence",
        format!(
            "it is {} MB; PixelFlow reads xLights sequences up to {} MB",
            size / (1024 * 1024),
            MAX_XSQ_BYTES / (1024 * 1024)
        ),
    )
}

/// Reads and parses the `.xsq` file at `path`.
pub(crate) fn read_sequence_file(path: &Path) -> Result<XsqFile, XlightsError> {
    let read_err = |e| XlightsError::Read(path.display().to_string(), e);
    let size = std::fs::metadata(path).map_err(read_err)?.len();
    if size > MAX_XSQ_BYTES as u64 {
        return Err(too_large(size));
    }
    let bytes = std::fs::read(path).map_err(read_err)?;
    parse_xsq(&String::from_utf8_lossy(&bytes))
}

/// A sequence's name from its file's: the name without its extension.
pub(crate) fn name_from_path(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "xLights Sequence".to_string())
}

/// Gives the import the music its sequence (read from `path`) names, found with `find_audio`
/// (see [`import_sequence_file`]), or a note that it couldn't be found.
pub(crate) fn find_music(
    import: &mut SequenceImport,
    file: &XsqFile,
    path: &Path,
    find_audio: impl Fn(&Path, Option<&str>) -> Option<PathBuf>,
) {
    let animation = matches!(file.head.sequence_type.trim(), "Animation" | "Effect");
    match (&import.media_file, animation) {
        (Some(media), false) => match find_audio(path, Some(media)) {
            Some(found) => {
                let found = pf_model::path_to_text(&found);
                if found.chars().count() <= MAX_TEXT_LEN {
                    import.sequence.audio = Some(found);
                }
            }
            None => import.notes.insert(
                0,
                format!(
                    "Couldn't find the music ({}) near the sequence; choose it in the sequence's settings.",
                    media.rsplit(['/', '\\']).next().unwrap_or(media)
                ),
            ),
        },
        (None, false) if !file.head.sequence_type.trim().is_empty() => {
            if let Some(found) = find_audio(path, None) {
                import.sequence.audio = Some(pf_model::path_to_text(&found));
            }
        }
        _ => {}
    }
}
