//! One-way import of an xLights sequence (`.xsq`) into an editable PixelFlow sequence.
//!
//! Effects land on rows targeting the show's props and groups by name (the names the layout
//! import keeps), timing tracks come across with their marks, and each xLights effect becomes the
//! PixelFlow effect closest to it, with its settings translated where there's a clear
//! equivalent. Nothing is approximated or left out silently: the report counts what came in
//! exactly, what was approximated (and how), and what was shown as a placeholder.

mod effects;
mod settings;
mod xsq;

pub use settings::{ParsedPalette, Settings, leading_number, parse_palette, unxml_safe};
pub use xsq::{ElementKind, MAX_XSQ_BYTES, XsqEffect, XsqElement, XsqFile, XsqHead, XsqLayer, parse_xsq};

use crate::XlightsError;
use effects::{Fidelity, Tally};
use pf_model::Show;
use pf_sequence::{
    Effect, EffectId, Layer, MAX_DURATION_MS, MAX_EFFECTS, MAX_FRAME_MS, MAX_LAYERS_PER_ROW, MAX_MARKS,
    MAX_ROWS, MAX_TEXT_LEN, MAX_TIMING_TRACKS, MIN_FRAME_MS, Mark, Row, RowId, Sequence, Target, TimingKind,
    TimingTrack,
};
use serde::Serialize;
use std::collections::HashMap;
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
    /// xLights effects not imported (models not in the show, submodels, limits).
    pub skipped: usize,
    pub timing_tracks: usize,
    pub marks: usize,
    /// Marks in lyric tracks (phrases, words, and phonemes).
    pub lyric_marks: usize,
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
    /// Effects cut at the end of the sequence / dropped because they start after it.
    effects_cut: usize,
    effects_after_end: usize,
    effects_too_late: usize,
    no_length: usize,
    random: usize,
    over_effect_limit: usize,
    bad_refs: usize,
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

    /// The span of an effect or mark, clipped to the sequence; `None` (counted) when it has no
    /// length or lies outside the sequence.
    fn span(&mut self, effect: &XsqEffect) -> Option<(u64, u64)> {
        let (Some(start), end) = (self.clock.ms(&effect.start), self.clock.ms(&effect.end)) else {
            self.effects_too_late += 1;
            return None;
        };
        let end = end.unwrap_or(u64::MAX);
        if start >= end {
            self.no_length += 1;
            return None;
        }
        if start >= self.duration_ms {
            self.effects_after_end += 1;
            return None;
        }
        if end > self.duration_ms {
            self.effects_cut += 1;
        }
        Some((start, end.min(self.duration_ms)))
    }

    fn effect(&mut self, x: &XsqEffect) -> Option<Effect> {
        let name = x.name.trim();
        if name == "Random" {
            self.random += 1;
            return None;
        }
        let (start_ms, end_ms) = self.span(x)?;
        if self.summary.effects >= MAX_EFFECTS {
            self.over_effect_limit += 1;
            return None;
        }
        let settings = self.settings_for(x);
        let palette = self.palette_for(x);
        let frame_ms = self.clock.frame_ms as u32;
        let Some(translated) = effects::translate(name, &settings, &palette, end_ms - start_ms, frame_ms)
        else {
            self.tally.record(name, &Fidelity::Skipped);
            self.summary.skipped += 1;
            return None;
        };
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
        })
    }

    fn marks(&mut self, layer: &XsqLayer, budget: usize, lost: &mut usize) -> Vec<Mark> {
        let mut marks = Vec::new();
        for x in &layer.effects {
            let Some((start, end)) = self.span(x) else {
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

/// Builds a PixelFlow sequence from a parsed `.xsq` file for `show` (rows target its props and
/// groups by name). `fallback_name` names the sequence when the file has no song title.
pub fn build_sequence(file: &XsqFile, show: &Show, fallback_name: &str) -> SequenceImport {
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
        effects_cut: 0,
        effects_after_end: 0,
        effects_too_late: 0,
        no_length: 0,
        random: 0,
        over_effect_limit: 0,
        bad_refs: 0,
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
    let mut timing_names: Vec<&str> = Vec::new();
    for element in file.elements.iter().filter(|e| e.kind == ElementKind::Timing) {
        let name = unxml_safe(&element.name);
        timing_names.push(&element.name);
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
            let lyric = element.layers.len() > 1;
            for (i, layer) in element.layers.iter().enumerate().take(3) {
                if i > 0 && layer.effects.is_empty() {
                    continue;
                }
                let (kind, track_name) = match (lyric, i) {
                    (false, _) => (TimingKind::Custom, name.clone()),
                    (true, 0) => (TimingKind::Lyrics, name.clone()),
                    (true, 1) => (TimingKind::Words, format!("{name} (words)")),
                    _ => (TimingKind::Phonemes, format!("{name} (phonemes)")),
                };
                let marks = b.marks(layer, marks_left, &mut marks_lost);
                marks_left -= marks.len();
                tracks.push(TimingTrack::new(track_name, kind, marks));
            }
            let extra: usize = element.layers.iter().skip(3).map(|l| l.effects.len()).sum();
            if extra > 0 {
                b.notes.push(format!(
                    "Timing track \"{name}\" has more than 3 layers; {} on the extra layers {} left out.",
                    plural(extra, "mark"),
                    if extra == 1 { "was" } else { "were" }
                ));
            }
        }
        for mut track in tracks {
            if sequence.timing_tracks.len() >= MAX_TIMING_TRACKS {
                tracks_lost += 1;
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
    let mut unmatched: Vec<(String, usize)> = Vec::new();
    let mut unmatched_empty = 0;
    let mut submodels: Vec<(String, usize)> = Vec::new();
    let mut rows_lost = 0;
    let mut layers_lost = 0;
    let mut row_index: HashMap<Target, usize> = HashMap::new();
    for element in file.elements.iter().filter(|e| e.kind == ElementKind::Model) {
        let count: usize = element.layers.iter().map(|l| l.effects.len()).sum();
        if timing_names.contains(&element.name.as_str()) {
            // xLights reads such an element as the timing track of the same name.
            continue;
        }
        let Some(target) = b.target(&element.name) else {
            let total = count + element.sub_effects;
            let name = unxml_safe(&element.name);
            if let Some(entry) = unmatched.iter_mut().find(|(n, _)| *n == name) {
                entry.1 += total;
            } else if total == 0 {
                unmatched_empty += 1;
            } else {
                unmatched.push((name, total));
            }
            b.summary.skipped += total;
            continue;
        };
        if element.sub_effects > 0 {
            submodels.push((unxml_safe(&element.name), element.sub_effects));
            b.summary.skipped += element.sub_effects;
        }
        // xLights' first layer is drawn on top; PixelFlow draws its last layer on top.
        let mut layers: Vec<Layer> = Vec::with_capacity(element.layers.len().max(1));
        for (i, x_layer) in element.layers.iter().enumerate().rev() {
            if i >= MAX_LAYERS_PER_ROW {
                layers_lost += x_layer.effects.len();
                b.summary.skipped += x_layer.effects.len();
                continue;
            }
            let effects = x_layer.effects.iter().filter_map(|x| b.effect(x)).collect();
            layers.push(Layer { effects });
        }
        if layers.is_empty() {
            layers.push(Layer::default());
        }
        match row_index.get(&target) {
            // The same model twice: xLights adds the second one's layers beneath the first's.
            Some(&at) => {
                let row: &mut Row = &mut sequence.rows[at];
                let room = MAX_LAYERS_PER_ROW.saturating_sub(row.layers.len());
                let extra: Vec<Layer> = layers.into_iter().rev().take(room).rev().collect();
                row.layers.splice(0..0, extra);
            }
            None if sequence.rows.len() >= MAX_ROWS => {
                rows_lost += 1;
                let lost: usize = layers.iter().map(|l| l.effects.len()).sum();
                b.summary.skipped += lost;
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
    b.summary.rows = sequence.rows.len();

    // Notes, most important first.
    let tally_notes = b.tally.notes();
    b.notes.splice(0..0, tally_notes);
    if !unmatched.is_empty() {
        let names: Vec<String> = unmatched
            .iter()
            .map(|(n, c)| format!("{n} ({})", plural(*c, "effect")))
            .collect();
        b.notes.insert(
            0,
            format!(
                "These xLights models aren't in the show, so their effects weren't imported: {}.",
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
    if !submodels.is_empty() {
        let names: Vec<String> = submodels
            .iter()
            .map(|(n, c)| format!("{n} ({})", plural(*c, "effect")))
            .collect();
        b.notes.push(format!(
            "PixelFlow doesn't import effects on submodels, strands, or single nodes yet; these weren't imported: {}.",
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
    if file.items_skipped > 0 {
        b.summary.skipped += file.items_skipped;
        b.notes.push(format!(
            "The sequence is too large to read completely; {} weren't imported.",
            plural(file.items_skipped, "effect or mark")
        ));
    }
    if b.effects_cut > 0 {
        b.notes.push(format!(
            "{} ran past the end of the sequence and {} cut off there.",
            plural(b.effects_cut, "effect or mark"),
            if b.effects_cut == 1 { "was" } else { "were" }
        ));
    }
    let outside = b.effects_after_end + b.effects_too_late;
    if outside > 0 {
        b.summary.skipped += outside;
        b.notes.push(format!(
            "{} started after the end of the sequence, so {} left out.",
            plural(outside, "effect or mark"),
            if outside == 1 { "it was" } else { "they were" }
        ));
    }
    if b.no_length > 0 {
        b.notes.push(format!(
            "{} had no length (xLights skips {} too), so {} left out.",
            plural(b.no_length, "effect or mark"),
            if b.no_length == 1 { "it" } else { "them" },
            if b.no_length == 1 { "it was" } else { "they were" }
        ));
    }
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
/// named after its song, or the file when it has none.
pub fn import_sequence_file(
    path: &Path,
    show: &Show,
    find_audio: impl Fn(&Path, Option<&str>) -> Option<PathBuf>,
) -> Result<SequenceImport, XlightsError> {
    let read_err = |e| XlightsError::Read(path.display().to_string(), e);
    let size = std::fs::metadata(path).map_err(read_err)?.len();
    if size > MAX_XSQ_BYTES as u64 {
        return Err(XlightsError::BadFile(
            "The xLights sequence",
            format!(
                "it is {} MB; PixelFlow reads xLights sequences up to {} MB",
                size / (1024 * 1024),
                MAX_XSQ_BYTES / (1024 * 1024)
            ),
        ));
    }
    let bytes = std::fs::read(path).map_err(read_err)?;
    let text = String::from_utf8_lossy(&bytes);
    let file = parse_xsq(&text)?;
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "xLights Sequence".to_string());
    let mut import = build_sequence(&file, show, &stem);
    let animation = matches!(file.head.sequence_type.trim(), "Animation" | "Effect");
    match (&import.media_file, animation) {
        (Some(media), false) => match find_audio(path, Some(media)) {
            Some(found) => {
                let found = found.display().to_string();
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
                import.sequence.audio = Some(found.display().to_string());
            }
        }
        _ => {}
    }
    Ok(import)
}
