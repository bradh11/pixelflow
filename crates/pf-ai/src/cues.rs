//! Director-level cues: the assistant says what a moment should do ("a hit here, a blackout
//! through the stop, a ramp into the drop") and `stage_cue` expands each cue, the same way every
//! time, into finished effects on the draft.
//!
//! - **Where**: on the rows that light the cue's props (each prop's own row, else the last group
//!   row made only of cue props), plus any later row over them that plays during the cue and
//!   would otherwise cover it; on a layer above every effect there during the cue (never the
//!   bottom one), so the section looks stay underneath.
//! - **What**: by each prop's role in the layout (outlines, trees, matrices, arches, windows,
//!   round props, talking props) and where it sits (left to right, bottom to top).
//! - **When**: from the song's tempo: a hit dips to dark for ½ beat (when that time is free)
//!   and decays over 1–2 beats; a ramp climbs bar by bar. Nothing overlaps on a layer, nothing
//!   reaches past the sequence.
//!
//! The result is ordinary effects the user can edit. One call stages many cues, or every moment
//! `analyze_song` lists from some importance with its suggested treatment, as one draft step:
//! all of it, or (with the reason) none of it.

use crate::draft::Draft;
use crate::song::find_track;
use pf_analysis::{Analysis, Moment, MomentKind, Suggest};
use pf_engine::SequenceEdit;
use pf_model::{Generator, PropId, RegionKind, RenderStyle, ShapeSource, Show};
use pf_sequence::{
    Effect, EffectKind, EffectParams, Layer, MAX_LAYERS_PER_ROW, Mark, Palette, Rgb, Sequence, Target,
    TimingKind, TimingTrack, TimingTrackId, format_ms,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// The cues, as `stage_cue` takes them.
pub const CUES: [&str; 13] = [
    "hit",
    "blackout",
    "ramp",
    "sweep",
    "chase",
    "call_response",
    "word_pop",
    "sing",
    "minimal",
    "full",
    "sustain",
    "color_shift",
    "breathe",
];

/// Which way a sweep travels (and a chase steps): the Wipe effect's directions.
pub const DIRECTIONS: [&str; 6] = ["leftToRight", "rightToLeft", "up", "down", "centerOut", "edgesIn"];

/// Words `targets` takes besides group and prop names.
pub const TARGET_WORDS: [&str; 12] = [
    "all", "left", "right", "top", "bottom", "outlines", "trees", "matrices", "arches", "windows", "rounds",
    "faces",
];

/// Effects one call may add, at most.
pub const MAX_STAGED: usize = 8_000;
/// Cues one call may give, at most.
const MAX_CUES: usize = 200;
/// From this intensity a cue gets the biggest treatment (accents, the whole house, singing).
const BIG: f32 = 0.85;
/// The intensity of a cue that names none (a moment's is its importance).
const DEFAULT_INTENSITY: f32 = 0.8;
/// The beat without a tempo to go by (120 BPM).
const DEFAULT_BEAT_MS: u64 = 500;

/// A ramp's colors, cool to hot.
const HEAT: [&str; 4] = ["#1e3cff", "#8a2be2", "#ff4500", "#ffd700"];
/// A peak's colors.
const FESTIVE: [&str; 3] = ["#ff2a2a", "#ffffff", "#2a6bff"];
/// A breakdown's.
const DEEP: [&str; 1] = ["#1e3a8a"];
/// A breath's.
const WARM: [&str; 1] = ["#ffc880"];
/// A color shift's, without colors given.
const SHIFT: [&str; 2] = ["#ff2a2a", "#2a6bff"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cue {
    Hit,
    Blackout,
    Ramp,
    Sweep,
    Chase,
    CallResponse,
    WordPop,
    Sing,
    Minimal,
    Full,
    Sustain,
    ColorShift,
    Breathe,
}

const ALL_CUES: [Cue; 13] = [
    Cue::Hit,
    Cue::Blackout,
    Cue::Ramp,
    Cue::Sweep,
    Cue::Chase,
    Cue::CallResponse,
    Cue::WordPop,
    Cue::Sing,
    Cue::Minimal,
    Cue::Full,
    Cue::Sustain,
    Cue::ColorShift,
    Cue::Breathe,
];

impl Cue {
    fn word(self) -> &'static str {
        CUES[ALL_CUES.iter().position(|c| *c == self).unwrap_or(0)]
    }

    fn parse(word: &str) -> Option<Self> {
        CUES.iter().position(|c| *c == word).map(|i| ALL_CUES[i])
    }

    /// How the proposal's summary counts it: one, and many.
    fn names(word: &str) -> (&'static str, &'static str) {
        match word {
            "hit" => ("hit", "hits"),
            "blackout" => ("blackout", "blackouts"),
            "ramp" => ("ramp", "ramps"),
            "sweep" => ("sweep", "sweeps"),
            "chase" => ("chase", "chases"),
            "call_response" => ("call and response", "calls and responses"),
            "word_pop" => ("word pop", "word pops"),
            "sing" => ("singing part", "singing parts"),
            "minimal" => ("minimal look", "minimal looks"),
            "full" => ("full look", "full looks"),
            "sustain" => ("sustain", "sustains"),
            "color_shift" => ("color shift", "color shifts"),
            _ => ("breath", "breaths"),
        }
    }

    /// The cue for a moment's suggested treatment.
    fn suggested(suggest: Suggest) -> Self {
        match suggest {
            Suggest::Hit | Suggest::Burst | Suggest::Flash => Cue::Hit,
            Suggest::Blackout => Cue::Blackout,
            Suggest::Minimal => Cue::Minimal,
            Suggest::Ramp => Cue::Ramp,
            Suggest::Chase => Cue::Chase,
            Suggest::Full => Cue::Full,
            Suggest::Sustain => Cue::Sustain,
            Suggest::ColorShift => Cue::ColorShift,
            Suggest::WordPop => Cue::WordPop,
            Suggest::Change => Cue::Sweep,
        }
    }

    /// Accents over a moment, staged after the cues that last (so they go above them).
    fn is_point(self) -> bool {
        matches!(self, Cue::Hit | Cue::WordPop)
    }
}

/// The cue `stageMoments` stages for a moment that suggests `suggest` ("hit", "blackout", ...).
pub fn suggested_cue(suggest: Suggest) -> &'static str {
    Cue::suggested(suggest).word()
}

/// "Staged 14 cues: 6 hits, 3 word pops, 2 blackouts" from cues counted by kind (most first),
/// or `None` without any.
pub fn summary(counts: &BTreeMap<String, usize>) -> Option<String> {
    let total: usize = counts.values().sum();
    if total == 0 {
        return None;
    }
    let mut kinds: Vec<(&String, &usize)> = counts.iter().filter(|(_, n)| **n > 0).collect();
    kinds.sort_by(|a, b| {
        let order = |w: &str| CUES.iter().position(|c| *c == w).unwrap_or(CUES.len());
        b.1.cmp(a.1).then(order(a.0).cmp(&order(b.0)))
    });
    let listed: Vec<String> = kinds
        .iter()
        .map(|(word, n)| {
            let (one, many) = Cue::names(word);
            format!("{n} {}", if **n == 1 { one } else { many })
        })
        .collect();
    Some(format!(
        "Staged {total} {}: {}",
        if total == 1 { "cue" } else { "cues" },
        listed.join(", ")
    ))
}

/// What a prop does in a show, by its shape (or its name, for shapes that don't say).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Role {
    Outline,
    Tree,
    Matrix,
    Arch,
    Window,
    Round,
    Other,
}

fn role_by_name(name: &str) -> Option<Role> {
    let name = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
    if has(&["tree"]) {
        Some(Role::Tree)
    } else if has(&["matrix", "panel", "screen", "p5", "p10"]) {
        Some(Role::Matrix)
    } else if has(&["arch"]) {
        Some(Role::Arch)
    } else if has(&["window", "door"]) {
        Some(Role::Window)
    } else if has(&[
        "star",
        "wreath",
        "spinner",
        "snowflake",
        "circle",
        "ball",
        "globe",
    ]) {
        Some(Role::Round)
    } else if has(&[
        "roof", "eave", "outline", "line", "icicle", "gutter", "path", "fence", "peak",
    ]) {
        Some(Role::Outline)
    } else {
        None
    }
}

pub(crate) fn role_of(prop: &pf_model::Prop) -> Role {
    let by_shape = match &prop.shape {
        ShapeSource::Generator(g) => match g {
            Generator::Tree { .. } => Some(Role::Tree),
            Generator::Matrix { .. } | Generator::Cube { .. } | Generator::Sphere { .. } => {
                Some(Role::Matrix)
            }
            Generator::Arch { .. } => Some(Role::Arch),
            Generator::WindowFrame { .. } => Some(Role::Window),
            Generator::Circle { .. }
            | Generator::Wreath { .. }
            | Generator::Star { .. }
            | Generator::Spinner { .. } => Some(Role::Round),
            Generator::Icicles { .. } | Generator::CandyCanes { .. } => Some(Role::Outline),
            // Lines, polylines, and custom grids are anything: the name says more.
            Generator::Line { .. } | Generator::PolyLine { .. } => {
                Some(role_by_name(&prop.name).unwrap_or(Role::Outline))
            }
            Generator::CustomGrid { .. } => role_by_name(&prop.name),
        },
        ShapeSource::Measured { .. } => role_by_name(&prop.name),
    };
    by_shape.unwrap_or(Role::Other)
}

/// Where a prop sits and what it is.
#[derive(Debug, Clone)]
struct PropInfo {
    name: String,
    /// The corners of its pixels' bounding box in the layout (x, y).
    min: [f32; 2],
    max: [f32; 2],
    role: Role,
    /// Its first face, if it has one.
    face: Option<String>,
    /// It can sing: a face, or a name that says so.
    talking: bool,
}

impl PropInfo {
    fn center(&self) -> [f32; 2] {
        [
            (self.min[0] + self.max[0]) / 2.0,
            (self.min[1] + self.max[1]) / 2.0,
        ]
    }
}

fn prop_info(prop: &pf_model::Prop) -> PropInfo {
    let points = pf_geometry::world_positions(prop);
    let p = prop.transform.position;
    let (mut min, mut max) = ([p.x, p.y], [p.x, p.y]);
    if let Some(first) = points.first() {
        min = [first.x, first.y];
        max = min;
        for q in &points {
            min = [min[0].min(q.x), min[1].min(q.y)];
            max = [max[0].max(q.x), max[1].max(q.y)];
        }
    }
    let face = prop.regions.iter().find_map(|r| match &r.kind {
        RegionKind::Face(_) => Some(r.name.clone()),
        _ => None,
    });
    let name = prop.name.to_lowercase();
    let talking = face.is_some()
        || ["face", "sing", "mouth", "carol"]
            .iter()
            .any(|w| name.contains(w));
    PropInfo {
        name: prop.name.clone(),
        min,
        max,
        role: role_of(prop),
        face,
        talking,
    }
}

/// A time a cue is at: milliseconds, a moment (`"m3"`, as `analyze_song` lists it), or a mark
/// (`"Sections:4"`).
#[derive(Debug, Clone)]
struct When {
    at: u64,
    /// Where the moment or mark ends, if it lasts.
    end: Option<u64>,
    moment: Option<Moment>,
}

/// One cue, read and checked.
#[derive(Debug, Clone)]
struct Spec {
    cue: Cue,
    at: u64,
    until: Option<u64>,
    /// The cue's props, in layout order (left to right).
    props: Vec<PropId>,
    /// call_response's other side.
    with: Vec<PropId>,
    /// No targets were named: the whole show.
    everything: bool,
    intensity: f32,
    colors: Vec<Rgb>,
    direction: &'static str,
    pattern: Option<String>,
    track: Option<TimingTrackId>,
    /// End a ramp, or follow a blackout, with a hit (`None`: as the music says).
    hit: Option<bool>,
}

/// The effects being staged, on a copy of the draft's sequence kept in step with them.
struct Stager<'a> {
    show: &'a Show,
    seq: Sequence,
    props: BTreeMap<PropId, PropInfo>,
    /// Which props each row lights.
    covers: Vec<BTreeSet<PropId>>,
    /// The layout's bounding box (x, y).
    min: [f32; 2],
    max: [f32; 2],
    beat: u64,
    moments: Vec<Moment>,
    analysis: Option<&'a Analysis>,
    edits: Vec<SequenceEdit>,
    rows_used: BTreeSet<usize>,
    /// Hits and word pops staged (times), so two never land on the same instant.
    hits: Vec<u64>,
    counts: BTreeMap<String, usize>,
    skipped: usize,
    notes: Vec<String>,
}

fn color(text: &str) -> Option<Rgb> {
    serde_json::from_value(json!(text)).ok()
}

fn colors(list: &[&str]) -> Vec<Rgb> {
    list.iter().filter_map(|c| color(c)).collect()
}

/// An effect of `kind` with `settings` (the rest at their defaults), from `start` to `end`.
fn make(kind: EffectKind, settings: Value, start: u64, end: u64, palette: &[Rgb]) -> Effect {
    let mut object = match settings {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    let tag = serde_json::to_value(kind).unwrap_or(Value::Null);
    object.insert("kind".into(), tag);
    let mut params = serde_json::from_value::<EffectParams>(Value::Object(object))
        .unwrap_or_else(|_| EffectParams::default_for(kind));
    params.sanitize();
    let mut effect = Effect::new(kind, start, end).with_params(params);
    if !palette.is_empty() {
        effect.palette = Palette::new(palette.to_vec());
    }
    effect
}

/// `stage_cue`: expands the cues (and, with `stageMoments`, a cue for each listed moment) into
/// effects on the draft, as one step. `analysis` is the song's, for its tempo, moments, and
/// timing (without it, 120 BPM and times in ms only).
pub fn stage(draft: &mut Draft, analysis: Option<&Analysis>, input: &Value) -> Result<String, String> {
    if draft.sequence().is_none() {
        return Err(
            "No sequence is open. Offer ask_for_song so the user can pick a song for a new one.".into(),
        );
    }
    known(input, &["cues", "stageMoments"], "stage_cue")?;
    let given = match &input["cues"] {
        Value::Null => Vec::new(),
        Value::Array(items) => items.clone(),
        _ => return Err("cues must be a list of cues.".into()),
    };
    let auto = &input["stageMoments"];
    if given.is_empty() && auto.is_null() {
        return Err("Give cues, or stageMoments to stage the song's moments.".into());
    }
    if given.len() > MAX_CUES {
        return Err(format!("Give at most {MAX_CUES} cues in one call."));
    }
    let mut work = draft.clone();
    let words: Vec<&str> = given.iter().filter_map(|c| c["cue"].as_str()).collect();
    let steps = !auto.is_null()
        || words
            .iter()
            .any(|w| ["ramp", "chase", "call_response", "breathe"].contains(w));
    if let Some(analysis) = analysis
        && steps
    {
        crate::song::add_timing(Some(analysis), &mut work, &["beats".into(), "bars".into()])?;
    }
    let sings = !auto.is_null() || words.iter().any(|w| ["sing", "word_pop"].contains(w));
    if sings && let Some(doc) = work.sequence() {
        let tracks = &doc.timing_tracks;
        let has_syllables = tracks.iter().any(crate::lyrics::tracks::is_syllables);
        let has_phonemes = tracks
            .iter()
            .any(|t| t.kind == TimingKind::Phonemes && !t.marks.is_empty());
        if crate::lyrics::tracks::words_track(tracks).is_some() && !(has_syllables && has_phonemes) {
            crate::song::add_timing(analysis, &mut work, &["syllables".into(), "phonemes".into()])?;
        }
    }
    let base = work.base().sequence.as_ref().map(|s| s.doc.clone());
    let mut stager = Stager::new(&work, analysis, base.as_ref())?;
    let mut specs = Vec::new();
    for (i, cue) in given.iter().enumerate() {
        specs.push(stager.spec(cue).map_err(|e| {
            format!(
                "Nothing was staged: cue {} ({}): {e}",
                i + 1,
                cue["cue"].as_str().unwrap_or("?")
            )
        })?);
    }
    if !auto.is_null() {
        specs.extend(stager.moment_specs(auto)?);
    }
    // Cues that last first, then the accents over them, so the accents go above.
    specs.sort_by_key(|s| s.cue.is_point());
    for spec in &specs {
        if stager.run(spec)? {
            *stager.counts.entry(spec.cue.word().to_string()).or_default() += 1;
        }
        if stager.edits.len() > MAX_STAGED {
            return Err(format!(
                "Nothing was staged: that's more than {MAX_STAGED} effects in one call. Stage fewer cues at a time, or fewer targets."
            ));
        }
    }
    if stager.edits.is_empty() {
        let why = stager.notes.first().cloned().unwrap_or_else(|| {
            "the cues' props have no rows in the sequence, or their times are taken".into()
        });
        return Err(format!("Nothing was staged: {why}"));
    }
    let added = stager.edits.len();
    let rows = stager.rows_used.len();
    let (counts, skipped, notes) = (stager.counts.clone(), stager.skipped, stager.notes.clone());
    work.edit_sequence_batch(stager.edits)
        .map_err(|e| e.to_string())?;
    work.staged(&counts);
    *draft = work;
    let mut out = format!(
        "{} ({added} effects on {rows} rows) in your draft.",
        summary(&counts).unwrap_or_default()
    );
    if skipped > 0 {
        out.push_str(&format!(
            " Skipped {skipped}: a hit or word pop already lands within half a beat."
        ));
    }
    for note in notes.iter().take(5) {
        out.push(' ');
        out.push_str(note);
    }
    Ok(out)
}

/// Refuses fields `what` doesn't take (a misspelled option would otherwise be ignored).
fn known(value: &Value, keys: &[&str], what: &str) -> Result<(), String> {
    let unknown: Vec<&str> = value
        .as_object()
        .map(|o| {
            o.keys()
                .map(String::as_str)
                .filter(|k| !keys.contains(k))
                .collect()
        })
        .unwrap_or_default();
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{what} has no {}. It takes {}.",
            unknown.join(", "),
            keys.join(", ")
        ))
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// The moment kinds, as `analyze_song` names them.
const MOMENT_KINDS: [MomentKind; 13] = [
    MomentKind::Impact,
    MomentKind::Stop,
    MomentKind::Restart,
    MomentKind::Breakdown,
    MomentKind::Build,
    MomentKind::Fill,
    MomentKind::Peak,
    MomentKind::Hold,
    MomentKind::KeyChange,
    MomentKind::Shout,
    MomentKind::Drop,
    MomentKind::Crash,
    MomentKind::SectionChange,
];

impl<'a> Stager<'a> {
    fn new(
        draft: &'a Draft,
        analysis: Option<&'a Analysis>,
        user: Option<&Sequence>,
    ) -> Result<Self, String> {
        let seq = draft.sequence().cloned().ok_or("No sequence is open.")?;
        if seq.rows.is_empty() {
            return Err("The open sequence has no rows to stage cues on.".into());
        }
        let show = draft.show();
        let props: BTreeMap<PropId, PropInfo> = show.props.iter().map(|p| (p.id, prop_info(p))).collect();
        let covers = seq
            .rows
            .iter()
            .map(|row| match row.target {
                Target::Prop(id) | Target::Region { prop: id, .. } => BTreeSet::from([id]),
                Target::Group(id) => show
                    .groups
                    .iter()
                    .find(|g| g.id == id)
                    .map(|g| g.members.iter().map(|m| m.prop()).collect())
                    .unwrap_or_default(),
            })
            .collect();
        let mut min = [f32::MAX; 2];
        let mut max = [f32::MIN; 2];
        for info in props.values() {
            min = [min[0].min(info.min[0]), min[1].min(info.min[1])];
            max = [max[0].max(info.max[0]), max[1].max(info.max[1])];
        }
        if props.is_empty() {
            (min, max) = ([0.0; 2], [1.0; 2]);
        }
        let beat = analysis
            .and_then(|a| a.tempo_bpm)
            .filter(|t| *t > 0.0)
            .map_or(DEFAULT_BEAT_MS, |t| (60_000.0 / t).round() as u64)
            .clamp(200, 1500);
        Ok(Self {
            show,
            seq,
            props,
            covers,
            min,
            max,
            beat,
            moments: analysis
                .map(|a| crate::song::ranked_moments(a, user))
                .unwrap_or_default(),
            analysis,
            edits: Vec::new(),
            rows_used: BTreeSet::new(),
            hits: Vec::new(),
            counts: BTreeMap::new(),
            skipped: 0,
            notes: Vec::new(),
        })
    }

    fn bar(&self) -> u64 {
        4 * self.beat
    }

    fn duration(&self) -> u64 {
        self.seq.duration_ms
    }

    fn track(&self, key: &str) -> Option<&TimingTrack> {
        find_track(&self.seq.timing_tracks, key)
    }

    fn words(&self) -> Option<&TimingTrack> {
        crate::lyrics::tracks::words_track(&self.seq.timing_tracks)
    }

    // ---- reading cues ----

    fn when(&self, value: &Value, field: &str) -> Result<Option<When>, String> {
        let at = |at: u64| When {
            at,
            end: None,
            moment: None,
        };
        match value {
            Value::Null => Ok(None),
            Value::Number(n) => n
                .as_u64()
                .map(|ms| Some(at(ms)))
                .ok_or_else(|| format!("{field} must be a whole number of ms.")),
            Value::String(text) => {
                let text = text.trim();
                if let Ok(ms) = text.parse::<u64>() {
                    return Ok(Some(at(ms)));
                }
                if let Some(i) = text
                    .strip_prefix(['m', 'M'])
                    .and_then(|i| i.parse::<usize>().ok())
                {
                    let moment = self.moments.get(i).ok_or_else(|| {
                        if self.moments.is_empty() {
                            "there are no moments: the sequence has no analyzed song.".to_string()
                        } else {
                            format!(
                                "there's no moment {i}; analyze_song lists moments 0 to {}.",
                                self.moments.len() - 1
                            )
                        }
                    })?;
                    return Ok(Some(When {
                        at: moment.time_ms,
                        end: moment.end_ms,
                        moment: Some(moment.clone()),
                    }));
                }
                if let Some((name, index)) = text.rsplit_once(':')
                    && let Ok(index) = index.trim().parse::<usize>()
                {
                    let track = self
                        .track(name.trim())
                        .ok_or_else(|| format!("there's no timing track \"{}\".", name.trim()))?;
                    let mark = track.marks.get(index).ok_or_else(|| {
                        format!(
                            "\"{}\" has {}.",
                            track.name,
                            plural(track.marks.len(), "mark", "marks")
                        )
                    })?;
                    return Ok(Some(When {
                        at: mark.start_ms,
                        end: Some(mark.end_ms),
                        moment: None,
                    }));
                }
                Err(format!(
                    "{field} is ms, \"m3\" (a moment from analyze_song), or \"Track:5\" (a mark)."
                ))
            }
            _ => Err(format!("{field} must be ms or text.")),
        }
    }

    /// Props in layout order: left to right, then bottom to top.
    fn spatial(&self, props: impl IntoIterator<Item = PropId>) -> Vec<PropId> {
        let mut out: Vec<PropId> = props.into_iter().filter(|p| self.props.contains_key(p)).collect();
        out.sort_by(|a, b| {
            let (pa, pb) = (&self.props[a], &self.props[b]);
            let (ca, cb) = (pa.center(), pb.center());
            ca[0]
                .total_cmp(&cb[0])
                .then(ca[1].total_cmp(&cb[1]))
                .then(pa.name.cmp(&pb.name))
                .then(a.cmp(b))
        });
        out.dedup();
        out
    }

    fn targets(&self, value: &Value, field: &str) -> Result<(Vec<PropId>, bool), String> {
        let names: Vec<&str> = match value {
            Value::Null => Vec::new(),
            Value::String(name) => vec![name.as_str()],
            Value::Array(items) => items
                .iter()
                .map(|v| v.as_str().ok_or(format!("{field} must be a list of names.")))
                .collect::<Result<_, _>>()?,
            _ => return Err(format!("{field} must be a list of names.")),
        };
        if names.is_empty() {
            return Ok((self.spatial(self.props.keys().copied()), true));
        }
        let mid = [
            (self.min[0] + self.max[0]) / 2.0,
            (self.min[1] + self.max[1]) / 2.0,
        ];
        let mut found = Vec::new();
        for name in names {
            let key = name.trim().to_lowercase();
            let pick = |keep: &dyn Fn(&PropInfo) -> bool| -> Vec<PropId> {
                self.props
                    .iter()
                    .filter(|(_, info)| keep(info))
                    .map(|(id, _)| *id)
                    .collect()
            };
            let role = |r: Role| move |i: &PropInfo| i.role == r;
            let props = match key.as_str() {
                "all" | "everything" => pick(&|_| true),
                "left" => pick(&|i| i.center()[0] < mid[0]),
                "right" => pick(&|i| i.center()[0] >= mid[0]),
                "top" => pick(&|i| i.center()[1] >= mid[1]),
                "bottom" => pick(&|i| i.center()[1] < mid[1]),
                "outlines" => pick(&role(Role::Outline)),
                "trees" => pick(&role(Role::Tree)),
                "matrices" => pick(&role(Role::Matrix)),
                "arches" => pick(&role(Role::Arch)),
                "windows" => pick(&role(Role::Window)),
                "rounds" => pick(&role(Role::Round)),
                "faces" => pick(&|i| i.talking),
                _ => {
                    let group = self
                        .show
                        .groups
                        .iter()
                        .find(|g| g.name.to_lowercase() == key || g.id.to_string() == key);
                    let prop = self
                        .show
                        .props
                        .iter()
                        .find(|p| p.name.to_lowercase() == key || p.id.to_string() == key);
                    match (group, prop) {
                        (Some(g), _) => g.members.iter().map(|m| m.prop()).collect(),
                        (None, Some(p)) => vec![p.id],
                        (None, None) => {
                            return Err(format!(
                                "there's no group or prop \"{name}\"; {field} takes group and prop names, or {}.",
                                TARGET_WORDS.join(", ")
                            ));
                        }
                    }
                }
            };
            if props.is_empty() {
                return Err(format!("no props are {name}."));
            }
            found.extend(props);
        }
        Ok((self.spatial(found), false))
    }

    fn spec(&self, value: &Value) -> Result<Spec, String> {
        known(
            value,
            &[
                "cue",
                "at",
                "until",
                "targets",
                "with",
                "intensity",
                "colors",
                "direction",
                "match",
                "track",
                "hit",
            ],
            "a cue",
        )?;
        let word = value["cue"].as_str().unwrap_or_default();
        let cue = Cue::parse(word).ok_or_else(|| format!("cue is one of {}.", CUES.join(", ")))?;
        let when = self
            .when(&value["at"], "at")?
            .ok_or("give at: when the cue happens.")?;
        if when.at >= self.duration() {
            return Err(format!(
                "at ({}) is past the end of the sequence ({}).",
                format_ms(when.at),
                format_ms(self.duration())
            ));
        }
        let until = match self.when(&value["until"], "until")? {
            Some(until) if until.at <= when.at => {
                return Err("until must come after at.".into());
            }
            Some(until) => Some(until.at.min(self.duration())),
            None => when.end.filter(|end| *end > when.at),
        };
        let (props, everything) = self.targets(&value["targets"], "targets")?;
        let (with, _) = match &value["with"] {
            Value::Null => (Vec::new(), false),
            other => self.targets(other, "with")?,
        };
        let intensity = match &value["intensity"] {
            Value::Null => when.moment.as_ref().map_or(DEFAULT_INTENSITY, |m| m.importance),
            v => v.as_f64().ok_or("intensity is 0 to 1.")? as f32,
        }
        .clamp(0.0, 1.0);
        let colors = match &value["colors"] {
            Value::Null => Vec::new(),
            Value::Array(items) => items
                .iter()
                .map(|c| {
                    c.as_str()
                        .and_then(color)
                        .ok_or_else(|| format!("{c} isn't a color; use \"#rrggbb\"."))
                })
                .collect::<Result<_, _>>()?,
            _ => return Err("colors must be a list of \"#rrggbb\".".into()),
        };
        let direction = match value["direction"].as_str() {
            None => DIRECTIONS[0],
            Some(d) => DIRECTIONS
                .into_iter()
                .find(|x| *x == d)
                .ok_or_else(|| format!("direction is one of {}.", DIRECTIONS.join(", ")))?,
        };
        let track = match value["track"].as_str() {
            None => None,
            Some(key) => Some(
                self.track(key)
                    .ok_or_else(|| format!("there's no timing track \"{key}\"."))?
                    .id,
            ),
        };
        let pattern = value["match"]
            .as_str()
            .map(str::to_string)
            .or_else(|| when.moment.as_ref().and_then(|m| m.label.clone()));
        Ok(Spec {
            cue,
            at: when.at,
            until,
            props,
            with,
            everything,
            intensity,
            colors,
            direction,
            pattern,
            track,
            hit: value["hit"].as_bool(),
        })
    }

    /// A cue for each moment `analyze_song` lists from `minImportance` (of `kinds`), with the
    /// treatment it suggests.
    fn moment_specs(&self, options: &Value) -> Result<Vec<Spec>, String> {
        known(options, &["minImportance", "kinds"], "stageMoments")?;
        if self.moments.is_empty() {
            return Err("stageMoments needs the song's moments: the sequence has no analyzed song.".into());
        }
        let min = options["minImportance"].as_f64().unwrap_or(0.5) as f32;
        let kinds: Option<Vec<MomentKind>> = match &options["kinds"] {
            Value::Null => None,
            Value::Array(items) => Some(
                items
                    .iter()
                    .map(|k| {
                        let word = k.as_str().unwrap_or_default();
                        MOMENT_KINDS
                            .into_iter()
                            .find(|m| m.word() == word)
                            .ok_or_else(|| {
                                let all: Vec<&str> = MOMENT_KINDS.iter().map(|m| m.word()).collect();
                                format!("there's no moment kind \"{word}\"; kinds are {}.", all.join(", "))
                            })
                    })
                    .collect::<Result<_, _>>()?,
            ),
            _ => return Err("stageMoments.kinds must be a list of moment kinds.".into()),
        };
        let all = self.spatial(self.props.keys().copied());
        let picked: Vec<&Moment> = self
            .moments
            .iter()
            .filter(|m| m.importance >= min && m.time_ms < self.duration())
            .filter(|m| kinds.as_ref().is_none_or(|k| k.contains(&m.kind)))
            .collect();
        let mut specs: Vec<Spec> = picked
            .iter()
            .map(|m| Spec {
                cue: Cue::suggested(m.suggest),
                at: m.time_ms,
                until: m
                    .end_ms
                    .filter(|e| *e > m.time_ms)
                    .map(|e| e.min(self.duration())),
                props: all.clone(),
                with: Vec::new(),
                everything: true,
                intensity: m.importance.clamp(0.0, 1.0),
                colors: Vec::new(),
                direction: DIRECTIONS[0],
                pattern: m.label.clone().filter(|_| m.kind == MomentKind::Shout),
                track: None,
                hit: None,
            })
            .collect();
        // A ramp ends on a hit of its own only where no staged moment lands.
        let points: Vec<u64> = specs.iter().filter(|s| s.cue.is_point()).map(|s| s.at).collect();
        for spec in specs.iter_mut().filter(|s| s.cue == Cue::Ramp) {
            if let Some(end) = spec.until
                && points.iter().any(|p| p.abs_diff(end) <= self.beat / 2)
            {
                spec.hit = Some(false);
            }
        }
        // Spans in time order, then accents by importance (its intensity), most first.
        specs.sort_by(|a, b| {
            let point = (a.cue.is_point(), b.cue.is_point());
            point.0.cmp(&point.1).then(match point {
                (true, true) => b.intensity.total_cmp(&a.intensity).then(a.at.cmp(&b.at)),
                _ => a.at.cmp(&b.at),
            })
        });
        Ok(specs)
    }

    // ---- placing ----

    fn active(&self, row: usize, span: (u64, u64)) -> bool {
        self.seq.rows[row]
            .layers
            .iter()
            .flat_map(|l| &l.effects)
            .any(|e| e.start_ms < span.1 && span.0 < e.end_ms)
    }

    /// The rows that show `props` during `span`: each prop's own row (else the last group row
    /// made only of these props), and every later row over it, made only of these props, that
    /// plays then (it would cover the cue). In row order.
    fn rows_for(&self, props: &[PropId], span: (u64, u64)) -> Vec<usize> {
        let set: BTreeSet<PropId> = props.iter().copied().collect();
        let mut chosen = BTreeSet::new();
        for p in &set {
            let own = self
                .seq
                .rows
                .iter()
                .rposition(|r| r.target == Target::Prop(*p))
                .or_else(|| {
                    (0..self.covers.len())
                        .rev()
                        .find(|&i| self.covers[i].contains(p) && self.covers[i].is_subset(&set))
                });
            let Some(own) = own else {
                continue;
            };
            chosen.insert(own);
            for i in own + 1..self.covers.len() {
                if self.covers[i].contains(p) && self.covers[i].is_subset(&set) && self.active(i, span) {
                    chosen.insert(i);
                }
            }
        }
        chosen.into_iter().collect()
    }

    /// The layer a cue playing during `span` goes on in `row`: above every effect there then,
    /// and never the bottom one (the section looks' layer).
    fn layer_for(&self, row: usize, span: (u64, u64)) -> usize {
        let layers = &self.seq.rows[row].layers;
        let top = layers
            .iter()
            .rposition(|l| l.effects.iter().any(|e| e.start_ms < span.1 && span.0 < e.end_ms));
        match top {
            Some(i) => i + 1,
            None => layers.len().min(1),
        }
    }

    /// Whether nothing plays during `span` on `row`'s layers from `layer` up.
    fn free(&self, row: usize, layer: usize, span: (u64, u64)) -> bool {
        self.seq.rows[row]
            .layers
            .iter()
            .skip(layer)
            .flat_map(|l| &l.effects)
            .all(|e| !(e.start_ms < span.1 && span.0 < e.end_ms))
    }

    /// Adds `effect` on `row`'s `layer` (a new layer on top if it's past the top), cut to the
    /// sequence. Leaves it out if it would be shorter than a frame or overlap anything there.
    fn put(&mut self, row: usize, layer: usize, mut effect: Effect) -> bool {
        effect.end_ms = effect.end_ms.min(self.duration());
        if effect.end_ms <= effect.start_ms + u64::from(self.seq.frame_ms) {
            return false;
        }
        let length = u32::try_from(effect.duration_ms()).unwrap_or(u32::MAX);
        effect.fade_out_ms = effect.fade_out_ms.min(length);
        effect.fade_in_ms = effect.fade_in_ms.min(length - effect.fade_out_ms);
        let r = &mut self.seq.rows[row];
        let layer = layer.min(r.layers.len());
        if layer >= MAX_LAYERS_PER_ROW
            || r.layers
                .get(layer)
                .is_some_and(|l| l.effects.iter().any(|e| e.overlaps(&effect)))
        {
            return false;
        }
        if layer == r.layers.len() {
            r.layers.push(Layer::default());
        }
        r.layers[layer].effects.push(effect.clone());
        self.edits.push(SequenceEdit::AddEffect {
            row: r.id,
            layer,
            effect,
        });
        self.rows_used.insert(row);
        true
    }

    /// The role of what a row lights (a group's most common role).
    fn row_role(&self, row: usize) -> Role {
        let roles: Vec<Role> = self.covers[row]
            .iter()
            .filter_map(|p| self.props.get(p).map(|i| i.role))
            .collect();
        let count = |r: Role| roles.iter().filter(|x| **x == r).count();
        roles
            .iter()
            .copied()
            .max_by_key(|r| count(*r))
            .unwrap_or(Role::Other)
    }

    /// Where a row's props sit along `direction`, 0–1 across the cue's props (`within`).
    fn extent(&self, row: usize, direction: &str, within: ([f32; 2], [f32; 2])) -> (f32, f32) {
        let (lo, hi) = within;
        let infos: Vec<&PropInfo> = self.covers[row]
            .iter()
            .filter_map(|p| self.props.get(p))
            .collect();
        if infos.is_empty() {
            return (0.0, 1.0);
        }
        let min = [
            infos.iter().map(|i| i.min[0]).fold(f32::MAX, f32::min),
            infos.iter().map(|i| i.min[1]).fold(f32::MAX, f32::min),
        ];
        let max = [
            infos.iter().map(|i| i.max[0]).fold(f32::MIN, f32::max),
            infos.iter().map(|i| i.max[1]).fold(f32::MIN, f32::max),
        ];
        let norm = |v: f32, axis: usize| {
            let span = hi[axis] - lo[axis];
            if span <= f32::EPSILON {
                0.0
            } else {
                ((v - lo[axis]) / span).clamp(0.0, 1.0)
            }
        };
        let (x0, x1) = (norm(min[0], 0), norm(max[0], 0));
        let (y0, y1) = (norm(min[1], 1), norm(max[1], 1));
        // Distance from the middle, 0 at the center to 1 at the farthest corner.
        let far =
            |x: f32, y: f32| (((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt() / 0.5f32.hypot(0.5)).min(1.0);
        let near = far(0.5f32.clamp(x0, x1), 0.5f32.clamp(y0, y1));
        let farthest = far(x0, y0).max(far(x1, y0)).max(far(x0, y1)).max(far(x1, y1));
        match direction {
            "rightToLeft" => (1.0 - x1, 1.0 - x0),
            "up" => (y0, y1),
            "down" => (1.0 - y1, 1.0 - y0),
            "centerOut" => (near, farthest),
            "edgesIn" => (1.0 - farthest, 1.0 - near),
            _ => (x0, x1),
        }
    }

    /// The bounding box of `props`.
    fn bounds(&self, props: &[PropId]) -> ([f32; 2], [f32; 2]) {
        let mut min = [f32::MAX; 2];
        let mut max = [f32::MIN; 2];
        for info in props.iter().filter_map(|p| self.props.get(p)) {
            min = [min[0].min(info.min[0]), min[1].min(info.min[1])];
            max = [max[0].max(info.max[0]), max[1].max(info.max[1])];
        }
        if min[0] > max[0] {
            return (self.min, self.max);
        }
        (min, max)
    }

    /// `props` in order along `direction`.
    fn along(&self, props: &[PropId], direction: &str) -> Vec<PropId> {
        let within = self.bounds(props);
        let mut order: Vec<(f32, PropId)> = props
            .iter()
            .map(|p| {
                let info = &self.props[p];
                let (lo, hi) = self.extent_of(info, direction, within);
                ((lo + hi) / 2.0, *p)
            })
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        order.into_iter().map(|(_, p)| p).collect()
    }

    fn extent_of(&self, info: &PropInfo, direction: &str, within: ([f32; 2], [f32; 2])) -> (f32, f32) {
        let (lo, hi) = within;
        let c = info.center();
        let norm = |v: f32, axis: usize| {
            let span = hi[axis] - lo[axis];
            if span <= f32::EPSILON {
                0.5
            } else {
                ((v - lo[axis]) / span).clamp(0.0, 1.0)
            }
        };
        let (x, y) = (norm(c[0], 0), norm(c[1], 1));
        let d = (((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt() / 0.5f32.hypot(0.5)).min(1.0);
        let at = match direction {
            "rightToLeft" => 1.0 - x,
            "up" => y,
            "down" => 1.0 - y,
            "centerOut" => d,
            "edgesIn" => 1.0 - d,
            _ => x,
        };
        (at, at)
    }

    fn palette(&self, spec: &Spec, fallback: &[&str]) -> Vec<Rgb> {
        if spec.colors.is_empty() {
            colors(fallback)
        } else {
            spec.colors.clone()
        }
    }

    // ---- the cues ----

    /// Stages one cue. False when it made nothing (skipped).
    fn run(&mut self, spec: &Spec) -> Result<bool, String> {
        let before = self.edits.len();
        match spec.cue {
            Cue::Hit => {
                if !self.hit(spec, spec.at, &spec.props, true) {
                    self.skipped += 1;
                    return Ok(false);
                }
            }
            Cue::Blackout => self.blackout(spec),
            Cue::Ramp => self.ramp(spec),
            Cue::Sweep => self.sweep(spec),
            Cue::Chase => self.chase(spec),
            Cue::CallResponse => self.call_response(spec),
            Cue::WordPop => {
                if !self.word_pop(spec) {
                    self.skipped += 1;
                    return Ok(false);
                }
            }
            Cue::Sing => self.sing(spec)?,
            Cue::Minimal => self.minimal(spec),
            Cue::Full => self.full(spec),
            Cue::Sustain => self.sustain(spec),
            Cue::ColorShift => self.color_shift(spec),
            Cue::Breathe => self.breathe(spec),
        }
        Ok(self.edits.len() > before)
    }

    /// A hit at `t` on `props`: dark for ½ beat before (when `dip` and that time is free), then
    /// an Impact with a punch, decaying over 1–2 beats by intensity; the biggest add a strobe on
    /// outlines and lightning on matrices. False when another hit already lands within ½ beat.
    fn hit(&mut self, spec: &Spec, t: u64, props: &[PropId], dip: bool) -> bool {
        let b = self.beat;
        if self.hits.iter().any(|h| h.abs_diff(t) < b / 2) || t >= self.duration() {
            return false;
        }
        let decay = b + (b as f32 * spec.intensity) as u64;
        let span = (t, t + decay);
        let dip_ms = b / 2;
        let palette = spec.colors.clone();
        let hit_color = if palette.is_empty() { "white" } else { "palette" };
        let big = spec.intensity >= BIG;
        let mut any = false;
        for row in self.rows_for(props, span) {
            let layer = self.layer_for(row, span);
            let role = self.row_role(row);
            // Free: nothing above the looks' layer then (another cue's decay, say).
            if dip && t >= dip_ms && self.free(row, 1, (t - dip_ms, t)) {
                let mut off = make(EffectKind::Off, json!({}), t - dip_ms, t, &[]);
                off.fade_in_ms = u32::try_from(dip_ms / 2).unwrap_or(0);
                self.put(row, layer, off);
            }
            let bloom = if role == Role::Matrix { b as f32 / 3.0 } else { 0.0 };
            let impact = make(
                EffectKind::Impact,
                json!({ "decay": "punch", "color": hit_color, "colorShift": palette.len() > 1, "bloom": bloom }),
                t,
                t + decay,
                &palette,
            );
            any |= self.put(row, layer, impact);
            if big {
                let accent = match role {
                    Role::Outline | Role::Window | Role::Arch => Some(make(
                        EffectKind::Strobe,
                        json!({ "rate": 20.0, "density": 0.5 }),
                        t,
                        t + b / 2,
                        &[Rgb::WHITE],
                    )),
                    Role::Matrix => Some(make(
                        EffectKind::Lightning,
                        json!({ "density": 6.0, "branches": 0.5, "glow": 0.4 }),
                        t,
                        t + b,
                        &[Rgb::WHITE],
                    )),
                    _ => None,
                };
                if let Some(accent) = accent {
                    self.put(row, layer + 1, accent);
                }
            }
        }
        if any {
            self.hits.push(t);
        }
        any
    }

    /// Off from `at` to `until` (a beat without one); with `until`, a hit there brings it back.
    fn blackout(&mut self, spec: &Spec) {
        let end = spec.until.unwrap_or(spec.at + self.beat);
        for row in self.rows_for(&spec.props, (spec.at, end)) {
            let layer = self.layer_for(row, (spec.at, end));
            self.put(row, layer, make(EffectKind::Off, json!({}), spec.at, end, &[]));
        }
        if spec.until.is_some() && spec.hit != Some(false) {
            self.hit(spec, end, &spec.props, false);
        }
    }

    /// Whether a moment that lands hard (an impact, drop, restart, or crash) is at `t`.
    fn impact_at(&self, t: u64) -> bool {
        self.analysis.is_some_and(|a| {
            a.moments.iter().any(|m| {
                matches!(
                    m.kind,
                    MomentKind::Impact | MomentKind::Drop | MomentKind::Restart | MomentKind::Crash
                ) && m.time_ms.abs_diff(t) <= self.beat / 2
            })
        })
    }

    /// A build from `at` to `until` (2 bars without one), a step up each bar (up to 4): pulses on
    /// the beat (chases on outlines and arches) getting brighter and faster, through colors from
    /// cool to hot; then a hit at `until` when one lands there (or `hit`).
    fn ramp(&mut self, spec: &Spec) {
        let end = spec
            .until
            .unwrap_or(spec.at + 2 * self.bar())
            .min(self.duration());
        let length = end - spec.at;
        let steps = ((length as f64 / self.bar() as f64).round() as u64).clamp(1, 4);
        let heat = if spec.colors.len() >= 2 {
            spec.colors.clone()
        } else {
            colors(&HEAT)
        };
        let beats = self.track("Beats").map(|t| t.id);
        for row in self.rows_for(&spec.props, (spec.at, end)) {
            let layer = self.layer_for(row, (spec.at, end));
            let role = self.row_role(row);
            for k in 0..steps {
                let from = spec.at + length * k / steps;
                let to = spec.at + length * (k + 1) / steps;
                let x = if steps == 1 {
                    1.0
                } else {
                    k as f32 / (steps - 1) as f32
                };
                let tint = heat[((x * (heat.len() - 1) as f32).round() as usize).min(heat.len() - 1)];
                let level = 0.35 + 0.65 * (k + 1) as f32 / steps as f32;
                let effect = match (role, beats) {
                    (Role::Outline | Role::Arch | Role::Window, _) => make(
                        EffectKind::Chase,
                        json!({ "speed": 1.0 + 2.5 * k as f32, "bands": 2 + k, "width": 0.4 }),
                        from,
                        to,
                        &[tint],
                    ),
                    (_, Some(track)) => make(
                        EffectKind::Pulse,
                        json!({
                            "source": "marks",
                            "timingTrack": track,
                            "shape": if 2 * k >= steps { "saw" } else { "sine" },
                            "min": 0.1 * k as f32 / steps as f32,
                            "max": level,
                        }),
                        from,
                        to,
                        &[tint],
                    ),
                    (_, None) => make(
                        EffectKind::On,
                        json!({ "startLevel": level - 0.65 / steps as f32, "endLevel": level }),
                        from,
                        to,
                        &[tint],
                    ),
                };
                self.put(row, layer, effect);
            }
        }
        let lands = spec.until.is_some() && self.impact_at(end);
        if spec.hit == Some(true) || (spec.hit.is_none() && lands) {
            let big = Spec {
                intensity: spec.intensity.max(0.9),
                ..spec.clone()
            };
            self.hit(&big, end, &spec.props, false);
        }
    }

    /// A wipe across the layout in `direction`, on then off, each prop's when the sweep reaches
    /// it (a group row: across the whole group, per preview).
    fn sweep(&mut self, spec: &Spec) {
        let end = spec.until.unwrap_or(spec.at + 2 * self.beat).min(self.duration());
        let length = end - spec.at;
        let travel = length * 3 / 5;
        let trail = length - travel;
        let within = self.bounds(&spec.props);
        let palette = self.palette(spec, &["#ffffff"]);
        for row in self.rows_for(&spec.props, (spec.at, end)) {
            let group = matches!(self.seq.rows[row].target, Target::Group(_));
            let (from, to) = if group {
                (spec.at, end)
            } else {
                let (lo, hi) = self.extent(row, spec.direction, within);
                (
                    spec.at + (travel as f32 * lo) as u64,
                    spec.at + (travel as f32 * hi) as u64 + trail,
                )
            };
            let mut wipe = make(
                EffectKind::Wipe,
                json!({ "direction": spec.direction, "mode": "onOff", "duration": 50.0, "softness": 0.3 }),
                from,
                to,
                &palette,
            );
            if group {
                wipe.render_style = RenderStyle::PerPreview;
            }
            let layer = self.layer_for(row, (spec.at, end));
            self.put(row, layer, wipe);
        }
    }

    /// The marks of `track` starting from `from` to `to`, else of `fallback` tracks (the first
    /// with at least `least` there), else a grid every `step` ms.
    fn steps(
        &self,
        track: Option<TimingTrackId>,
        fallback: &[&str],
        least: usize,
        from: u64,
        to: u64,
        step: u64,
    ) -> Vec<(u64, u64)> {
        let marks_of = |t: &TimingTrack| -> Vec<(u64, u64)> {
            t.marks
                .iter()
                .filter(|m: &&Mark| m.start_ms >= from && m.start_ms < to)
                .map(|m| (m.start_ms, m.end_ms.min(to)))
                .filter(|(s, e)| e > s)
                .collect()
        };
        if let Some(track) = track.and_then(|id| self.seq.timing_tracks.iter().find(|t| t.id == id)) {
            return marks_of(track);
        }
        for name in fallback {
            if let Some(track) = self.track(name) {
                let marks = marks_of(track);
                if marks.len() >= least {
                    return marks;
                }
            }
        }
        let step = step.max(50);
        (0..)
            .map(|k| from + k * step)
            .take_while(|t| *t < to)
            .map(|t| (t, (t + step).min(to)))
            .collect()
    }

    /// Props lit one at a time in layout order (`direction`), stepping on `track`'s marks, else
    /// the drum hits, else eighth notes; each flash fades before that prop's next turn.
    fn chase(&mut self, spec: &Spec) {
        let end = spec.until.unwrap_or(spec.at + self.bar()).min(self.duration());
        let steps = self.steps(spec.track, &["Drums"], 3, spec.at, end, self.beat / 2);
        let order = self.along(&spec.props, spec.direction);
        if order.is_empty() || steps.is_empty() {
            return;
        }
        let palette = self.palette(spec, &["#ffffff"]);
        let n = order.len();
        let cue_span = (spec.at, end + self.beat);
        let mut layers: BTreeMap<usize, usize> = BTreeMap::new();
        for (k, &(start, stop)) in steps.iter().enumerate() {
            let prop = order[k % n];
            let next_turn = steps.get(k + n).map_or(u64::MAX, |s| s.0);
            let length = (2 * (stop - start))
                .max(self.beat / 4)
                .min(next_turn.saturating_sub(start));
            let tint = palette[k % palette.len()];
            for row in self.rows_for(&[prop], cue_span) {
                let layer = *layers.entry(row).or_insert_with(|| self.layer_for(row, cue_span));
                let flash = make(
                    EffectKind::Impact,
                    json!({ "decay": "exponential", "color": "palette" }),
                    start,
                    start + length,
                    &[tint],
                );
                self.put(row, layer, flash);
            }
        }
    }

    /// Two sides taking turns on each mark of `track` (bars, else beats): `targets` and `with`,
    /// or the left and right halves of `targets`.
    fn call_response(&mut self, spec: &Spec) {
        let end = spec
            .until
            .unwrap_or(spec.at + 2 * self.bar())
            .min(self.duration());
        let (a, b) = if spec.with.is_empty() {
            let half = spec.props.len().div_ceil(2);
            (spec.props[..half].to_vec(), spec.props[half..].to_vec())
        } else {
            (spec.props.clone(), spec.with.clone())
        };
        let marks = self.steps(spec.track, &["Bars", "Beats"], 2, spec.at, end, self.beat);
        let palette = self.palette(spec, &["#ffffff"]);
        let span = (spec.at, end);
        let mut layers: BTreeMap<usize, usize> = BTreeMap::new();
        for (k, &(start, stop)) in marks.iter().enumerate() {
            let side = if k % 2 == 0 { &a } else { &b };
            let tint = palette[(k % 2).min(palette.len() - 1)];
            for row in self.rows_for(side, span) {
                let layer = *layers.entry(row).or_insert_with(|| self.layer_for(row, span));
                let answer = make(
                    EffectKind::Impact,
                    json!({ "decay": "linear", "color": "palette" }),
                    start,
                    stop,
                    &[tint],
                );
                self.put(row, layer, answer);
            }
        }
    }

    /// `props` singing from `from` to `to`, one effect per sung stretch (from the Vocals track)
    /// in that time: Faces on a prop with a face (on the phonemes), Sing on the rest (on the
    /// syllables). False without sung words.
    fn sing_on(&mut self, props: &[PropId], from: u64, to: u64, palette: &[Rgb]) -> bool {
        let tracks = &self.seq.timing_tracks;
        let Some(words) = crate::lyrics::tracks::words_track(tracks).map(|t| t.id) else {
            return false;
        };
        let syllables = tracks
            .iter()
            .find(|t| crate::lyrics::tracks::is_syllables(t) && !t.marks.is_empty())
            .map_or(words, |t| t.id);
        let phonemes = tracks
            .iter()
            .find(|t| t.kind == TimingKind::Phonemes && !t.marks.is_empty())
            .map_or(words, |t| t.id);
        let stretches: Vec<(u64, u64)> = self
            .track(crate::lyrics::tracks::VOCALS_TRACK)
            .map(|v| {
                v.marks
                    .iter()
                    .map(|m| (m.start_ms.max(from), m.end_ms.min(to)))
                    .filter(|(s, e)| e > s)
                    .collect()
            })
            .filter(|s: &Vec<(u64, u64)>| !s.is_empty())
            .unwrap_or_else(|| vec![(from, to)]);
        let mut any = false;
        for &p in props {
            let face = self.props.get(&p).and_then(|i| i.face.clone());
            for row in self.rows_for(&[p], (from, to)) {
                let layer = self.layer_for(row, (from, to));
                for &(start, stop) in &stretches {
                    let effect = match &face {
                        Some(face) => make(
                            EffectKind::Faces,
                            json!({ "face": face, "timingTrack": phonemes, "eyes": "auto" }),
                            start,
                            stop,
                            palette,
                        ),
                        None => make(
                            EffectKind::Sing,
                            json!({ "mode": "mouth", "timingTrack": syllables, "min": 0.05 }),
                            start,
                            stop,
                            palette,
                        ),
                    };
                    any |= self.put(row, layer, effect);
                }
            }
        }
        any
    }

    /// A pop on each sung word matching `match` (a moment's word) from ½ beat before `at` to
    /// `until` (else a beat after): an Impact on the targets lasting the word or a beat; the
    /// biggest pop the whole house and set the talking props singing it. Without sung words, a
    /// pop at `at`. False when a hit already lands there.
    fn word_pop(&mut self, spec: &Spec) -> bool {
        let b = self.beat;
        let from = spec.at.saturating_sub(b / 2);
        let to = spec.until.unwrap_or(spec.at + b).min(self.duration());
        let mut spans: Vec<(u64, u64)> = Vec::new();
        if let (Some(pattern), Some(words)) = (&spec.pattern, self.words()) {
            let marks: Vec<&Mark> = words
                .marks
                .iter()
                .filter(|m| m.start_ms >= from && m.start_ms < to)
                .collect();
            if let Ok(found) = crate::arrange::matching(&marks, pattern) {
                spans = found
                    .into_iter()
                    .map(|(first, last)| (marks[first].start_ms, marks[last].end_ms))
                    .collect();
            }
        }
        if spans.is_empty() {
            if spec.until.is_some() && spec.pattern.is_some() {
                self.notes.push(format!(
                    "No sung words match \"{}\" there, so the pop is at {}.",
                    spec.pattern.as_deref().unwrap_or_default(),
                    format_ms(spec.at)
                ));
            }
            spans.push((spec.at, spec.at + b / 2));
        }
        let big = spec.intensity >= BIG;
        let props = if big {
            self.spatial(self.props.keys().copied())
        } else {
            spec.props.clone()
        };
        let talking: Vec<PropId> = props
            .iter()
            .copied()
            .filter(|p| self.props.get(p).is_some_and(|i| i.talking))
            .collect();
        let palette = spec.colors.clone();
        let mut any = false;
        for (start, stop) in spans {
            if self.hits.iter().any(|h| h.abs_diff(start) < b / 2) {
                continue;
            }
            let length = (stop - start).max(b);
            let span = (start, start + length);
            for row in self.rows_for(&props, span) {
                let layer = self.layer_for(row, span);
                let pop = make(
                    EffectKind::Impact,
                    json!({ "decay": "exponential", "color": if palette.is_empty() { "white" } else { "palette" } }),
                    start,
                    start + length,
                    &palette,
                );
                any |= self.put(row, layer, pop);
            }
            if big && !talking.is_empty() {
                self.sing_on(&talking, start, stop.max(start + b / 2), &[]);
            }
            self.hits.push(start);
        }
        any
    }

    /// Talking props (or the targets named) singing from `at` to `until` (the last sung word).
    fn sing(&mut self, spec: &Spec) -> Result<(), String> {
        let words = self.words().ok_or(
            "there are no sung words. Lyrics come from Find lyrics, beside Detect beats: offer it to the user.",
        )?;
        let last = words
            .marks
            .iter()
            .map(|m| m.end_ms)
            .max()
            .unwrap_or(self.duration());
        let props: Vec<PropId> = if spec.everything {
            spec.props
                .iter()
                .copied()
                .filter(|p| self.props.get(p).is_some_and(|i| i.talking))
                .collect()
        } else {
            spec.props.clone()
        };
        if props.is_empty() {
            return Err(
                "no props have faces or sing in their names: name the props to sing in targets.".into(),
            );
        }
        let end = spec.until.unwrap_or(last).min(self.duration());
        if end > spec.at {
            self.sing_on(&props, spec.at, end, &spec.colors);
        }
        Ok(())
    }

    /// A breakdown: a few props (trees first, then matrices and round props, nearest the
    /// middle) breathe softly with the bass; the rest go dark.
    fn minimal(&mut self, spec: &Spec) {
        let end = spec
            .until
            .unwrap_or(spec.at + 2 * self.bar())
            .min(self.duration());
        let span = (spec.at, end);
        let keep_n = (spec.props.len() / 5).clamp(1, 3);
        let mid = (self.min[0] + self.max[0]) / 2.0;
        let mut ranked = spec.props.clone();
        ranked.sort_by(|a, b| {
            let rank = |p: &PropId| match self.props[p].role {
                Role::Tree => 0,
                Role::Matrix => 1,
                Role::Round => 2,
                _ => 3,
            };
            let off = |p: &PropId| (self.props[p].center()[0] - mid).abs();
            rank(a).cmp(&rank(b)).then(off(a).total_cmp(&off(b)))
        });
        let keep: BTreeSet<PropId> = ranked.into_iter().take(keep_n).collect();
        let palette = self.palette(spec, &DEEP);
        let fade = u32::try_from(self.beat / 2).unwrap_or(0);
        for row in self.rows_for(&spec.props, span) {
            let layer = self.layer_for(row, span);
            let mut off = make(EffectKind::Off, json!({}), spec.at, end, &[]);
            off.fade_in_ms = fade;
            if self.covers[row].is_subset(&keep) {
                off.fade_in_ms = 0;
                self.put(row, layer, off);
                let pulse = make(
                    EffectKind::Pulse,
                    json!({ "source": "bass", "min": 0.05, "max": 0.3 + 0.4 * spec.intensity, "attack": 60.0, "release": 800.0 }),
                    spec.at,
                    end,
                    &palette,
                );
                self.put(row, layer + 1, pulse);
            } else {
                self.put(row, layer, off);
            }
        }
    }

    /// The peak look: meters on matrices, spirals on trees, chases on outlines and arches, bass
    /// pulses on the rest, opened by a hit when it's big.
    fn full(&mut self, spec: &Spec) {
        let end = spec
            .until
            .unwrap_or(spec.at + 4 * self.bar())
            .min(self.duration());
        let span = (spec.at, end);
        let palette = self.palette(spec, &FESTIVE);
        let speed = 0.5 + spec.intensity;
        for row in self.rows_for(&spec.props, span) {
            let layer = self.layer_for(row, span);
            let effect = match self.row_role(row) {
                Role::Matrix => make(
                    EffectKind::VuMeter,
                    json!({ "meter": "spectrogram", "bars": 16 }),
                    spec.at,
                    end,
                    &palette,
                ),
                Role::Tree => make(
                    EffectKind::Spiral,
                    json!({ "count": 4, "speed": speed, "thickness": 0.45, "twist": 1.5 }),
                    spec.at,
                    end,
                    &palette,
                ),
                Role::Outline | Role::Window => make(
                    EffectKind::Chase,
                    json!({ "speed": 2.0 * speed, "bands": 4, "width": 0.5 }),
                    spec.at,
                    end,
                    &palette,
                ),
                Role::Arch => make(
                    EffectKind::Chase,
                    json!({ "speed": 1.5 * speed, "bands": 1, "width": 0.4, "bounce": true }),
                    spec.at,
                    end,
                    &palette,
                ),
                Role::Round | Role::Other => make(
                    EffectKind::Pulse,
                    json!({ "source": "bass", "min": 0.3, "max": 1.0, "attack": 20.0, "release": 300.0 }),
                    spec.at,
                    end,
                    &palette,
                ),
            };
            self.put(row, layer, effect);
        }
        if spec.intensity >= 0.8 {
            self.hit(spec, spec.at, &spec.props, false);
        }
    }

    /// A hold: the look as it is for the first part, then a slow fade to dark by `until`.
    fn sustain(&mut self, spec: &Spec) {
        let end = spec
            .until
            .unwrap_or(spec.at + 2 * self.bar())
            .min(self.duration());
        let from = spec.at + (end - spec.at) * 2 / 5;
        for row in self.rows_for(&spec.props, (from, end)) {
            let layer = self.layer_for(row, (from, end));
            let mut off = make(EffectKind::Off, json!({}), from, end, &[]);
            off.fade_in_ms = u32::try_from(end - from).unwrap_or(u32::MAX);
            self.put(row, layer, off);
        }
    }

    /// A change to new colors travelling across the props in `direction` over a beat.
    fn color_shift(&mut self, spec: &Spec) {
        let end = spec
            .until
            .unwrap_or(spec.at + 2 * self.bar())
            .min(self.duration());
        let palette = self.palette(spec, &SHIFT);
        let order = self.along(&spec.props, spec.direction);
        let n = order.len().max(1) as u64;
        for (k, prop) in order.iter().enumerate() {
            let start = spec.at + self.beat * k as u64 / n;
            for row in self.rows_for(&[*prop], (start, end)) {
                let layer = self.layer_for(row, (spec.at, end));
                let shift = make(
                    EffectKind::ColorShift,
                    json!({ "ease": "smooth", "duration": 30.0 }),
                    start,
                    end,
                    &palette,
                );
                self.put(row, layer, shift);
            }
        }
    }

    /// A pulse with the bass (or on `track`'s marks) from `at` to `until`.
    fn breathe(&mut self, spec: &Spec) {
        let end = spec
            .until
            .unwrap_or(spec.at + 2 * self.bar())
            .min(self.duration());
        let palette = self.palette(spec, &WARM);
        let settings = match spec.track {
            Some(track) => {
                json!({ "source": "marks", "timingTrack": track, "shape": "sine", "min": 0.15, "max": 0.6 + 0.4 * spec.intensity })
            }
            None => {
                json!({ "source": "bass", "min": 0.15, "max": 0.6 + 0.4 * spec.intensity, "attack": 40.0, "release": 500.0 })
            }
        };
        for row in self.rows_for(&spec.props, (spec.at, end)) {
            let layer = self.layer_for(row, (spec.at, end));
            let pulse = make(EffectKind::Pulse, settings.clone(), spec.at, end, &palette);
            self.put(row, layer, pulse);
        }
    }
}
