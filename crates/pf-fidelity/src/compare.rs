//! Rendering a sequence in PixelFlow and measuring it against xLights' own frames.
//!
//! [`Prepared::open`] imports the layout and the sequence, exports the sequence with PixelFlow's
//! `.fseq` export (to memory), and opens the reference. Both sides are then `.fseq` data in the
//! same channel space, read back pixel by pixel through one table ([`Slot`]): what is compared is
//! what PixelFlow would write against what xLights wrote, channel for channel.

use crate::chance::{self, Chance};
use crate::layout::{ComparisonShow, comparison_show};
use crate::look::{Grid, Look, LookStats, Parts, Scratch, next_to_seeds};
use crate::metrics::{Limits, PIXELFLOW, Sample, Stats, XLIGHTS};
use crate::scorecard::{
    AuditLimits, Class, Coverage, EffectScore, Gap, Label, Method, Music, OffsetError, Onset, PropScore,
    ReferenceInfo, Scorecard, SequenceInfo, Source, Timing,
};
use crate::{FidelityError, TOOL};
use pf_model::{PropId, Show};
use pf_render::export::{ExportError, export_fseq, export_layout};
use pf_render::{AudioSource, Pictures, SceneGeometry};
use pf_sequence::{Effect, EffectParams, Sequence, Target, TendrilMovement};
use pf_xlights::sequence::{Fidelity, SequenceImport, Verdict};
use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

/// xLights effects that place things at random: no two renders light the same pixels, so they
/// are compared by what they show over a window, not pixel by pixel.
const STOCHASTIC: [&str; 14] = [
    "Candle",
    "Circles",
    "Fire",
    "Fireworks",
    "Life",
    "Lightning",
    "Lines",
    "Liquid",
    "Meteors",
    "Shape",
    "Snowflakes",
    "Snowstorm",
    "Strobe",
    "Twinkle",
];

/// An effect whose sparkles alone can change more than this share of its lit pixels is compared
/// as a stochastic one.
const MOSTLY_SPARKLES: f64 = 0.15;
/// How many more times PixelFlow renders a sequence, each from other seeds, to see what chance
/// does to the effects that draw on it. One would say how far chance moved an effect between
/// two renders, which may be the two that happen to be most alike; two give three pairs to take
/// the least alike of. Each one more is steadier, and takes as long again to render and read.
pub const OTHER_SEEDS: u32 = 2;
/// Frames either side of each frame the timing check tries.
const OFFSETS: i64 = 2;
/// Most frames the timing check reads the neighbors of.
const OFFSET_SAMPLES: u32 = 1500;

/// What to compare.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// The xLights show folder the reference was rendered on.
    pub show: &'a Path,
    pub xsq: &'a Path,
    /// The `.fseq` xLights rendered from `xsq`.
    pub fseq: &'a Path,
}

/// How to compare.
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub limits: Limits,
    pub audit: AuditLimits,
    /// Compares PixelFlow's frame at a time with xLights' frame this many later (0: the same
    /// moment). For trying out a timing difference the scorecard's check points at.
    pub frame_offset: i64,
    /// Leaves the music unread: effects that follow it are then set apart, not scored.
    pub skip_music: bool,
    /// What the scorecard calls the sequence (its file's name otherwise).
    pub label: Option<String>,
    /// Judges every effect moment by moment against xLights' render alone, whatever chance
    /// decides in it: the numbers from before effects that draw on chance were held against
    /// PixelFlow's own renders from other seeds.
    pub by_moment: bool,
    /// How many more renders from other seeds to hold such effects against ([`OTHER_SEEDS`]
    /// when `None`).
    pub seeds: Option<u32>,
}

/// Where a pixel's channels are in both files.
#[derive(Debug, Clone, Copy)]
struct Slot {
    /// The pixel's first channel, counting from 0.
    channel: u32,
    /// Where red, green, and blue are among the pixel's channels.
    at: [u8; 3],
}

/// One prop's place in the comparison.
#[derive(Debug, Clone)]
struct PropEntry {
    name: String,
    /// Its first pixel in the show-wide pixel list.
    first: usize,
    pixels: usize,
    measured: usize,
    /// First and last channel compared, counting from 1.
    channels: Option<(u32, u32)>,
    source: Source,
    /// Where its pixels are on its own grid, in the show-wide list's order.
    grid: Rc<Grid>,
}

/// One imported effect and the pixels it can be measured on.
#[derive(Debug, Clone)]
pub struct EffectEntry {
    pub target: Target,
    pub target_name: String,
    /// xLights' name for the effect.
    pub kind: String,
    pub pixelflow_kind: String,
    pub label: Label,
    pub notes: Vec<String>,
    pub class: Class,
    pub class_reason: Option<String>,
    /// The share of its lit pixels that random sparkles may make differ.
    pub sparkles: f64,
    /// What chance decides in it, and so how its look is compared.
    pub chance: Chance,
    pub chance_reason: Option<&'static str>,
    /// Another effect that draws on chance plays on some of its pixels while it does: the
    /// picture there changes from seed to seed, whatever this effect draws.
    pub shares_chance: bool,
    pub start_ms: u64,
    pub end_ms: u64,
    /// The row's layer, counting from the bottom.
    pub layer: usize,
    /// Pixels in its target.
    pub pixels: usize,
    /// Those both sides have, as places in the show-wide pixel list.
    measured: Rc<[u32]>,
    /// Where each of those is on the target's own grid.
    grid: Rc<Grid>,
    /// How the importer expects it to look.
    pub expected: Verdict,
}

/// What of the reference isn't compared (see `Prepared::dark_runs`).
struct LeftOut {
    /// For each prop, the frames it's left out on.
    frames: Vec<Vec<bool>>,
    /// Props the reference leaves dark for long while PixelFlow lights them.
    dark: usize,
    /// When the file was saved part-way through rendering: the first frame it's missing light
    /// on, and how many props were cut short.
    cut_short: Option<(u32, usize)>,
}

/// A target's pixels: how many it has, those both sides have (as places in the show-wide pixel
/// list), and where each of those is on the target's own grid.
type TargetPixels = (usize, Rc<[u32]>, Rc<Grid>);

/// One pair of renders over an effect's pixel-frames, alone and layered: the look moment by
/// moment and the parts to hold against other pairs, where there were frames to look at.
type Looked = [Option<(Look, Parts)>; 2];

/// Both renderings, open and lined up.
pub struct Prepared {
    options: Options,
    label: String,
    comparison: ComparisonShow,
    import: SequenceImport,
    geometry: SceneGeometry,
    pixelflow: pf_fseq::Sequence<Cursor<Vec<u8>>>,
    /// The same sequence rendered from other seeds, for effects that draw on chance.
    others: Vec<pf_fseq::Sequence<Cursor<Vec<u8>>>>,
    reference: pf_fseq::Sequence,
    /// By place in the show-wide pixel list; `None` where either side has no data.
    slots: Vec<Option<Slot>>,
    props: Vec<PropEntry>,
    effects: Vec<EffectEntry>,
    music: Music,
    notes: Vec<String>,
}

/// The name people know a target by.
fn target_name(show: &Show, target: Target) -> String {
    let prop_name = |id: PropId| show.prop(id).map_or("(unknown prop)", |p| p.name.as_str());
    match target {
        Target::Prop(id) => prop_name(id).to_string(),
        Target::Group(id) => show
            .groups
            .iter()
            .find(|g| g.id == id)
            .map_or_else(|| "(unknown group)".to_string(), |g| g.name.clone()),
        Target::Region { prop, region } => {
            let region = show.region(prop, region).map_or("?", |(_, r)| r.name.as_str());
            format!("{}/{region}", prop_name(prop))
        }
    }
}

/// The share of an effect's lit pixels its sparkles alone can make differ: each lit pixel
/// flashes for 5 frames in every `208 - sparkles`, at its own random moment, so about that share
/// flash on each side, on different pixels.
fn sparkle_share(sparkles: u32) -> f64 {
    let sparkles = sparkles.min(pf_sequence::MAX_SPARKLES);
    if sparkles == 0 {
        0.0
    } else {
        (10.0 / f64::from(208 - sparkles)).min(1.0)
    }
}

/// How an effect can be compared, why when it isn't pixel by pixel, and the share of its lit
/// pixels that sparkles may make differ.
fn class_of(kind: &str, effect: &Effect, music: Music) -> (Class, Option<String>, f64) {
    let sparkles = sparkle_share(effect.sparkles);
    let class = |class, why: &str| (class, Some(why.to_string()), sparkles);
    if pf_render::audio::effect_follows_music(effect) && music != Music::Read {
        return class(Class::NoMusic, "it follows the music, which wasn't read");
    }
    if STOCHASTIC.contains(&kind) {
        return class(Class::Stochastic, "it places things at random");
    }
    if matches!(&effect.params, EffectParams::Tendril(p) if p.movement == TendrilMovement::Random) {
        return class(Class::Stochastic, "it moves at random");
    }
    if effect.curves.contains_key("sparkles") || sparkles > MOSTLY_SPARKLES {
        return class(Class::Stochastic, "it is mostly sparkles");
    }
    (Class::Deterministic, None, sparkles)
}

impl Prepared {
    /// Imports, renders, and opens the reference.
    pub fn open(request: Request, options: Options) -> Result<Self, FidelityError> {
        let reference = pf_fseq::Sequence::open(request.fseq).map_err(|source| FidelityError::Reference {
            path: request.fseq.to_path_buf(),
            source,
        })?;
        let comparison = comparison_show(request.show)?;
        let show = &comparison.show;
        let mut notes = Vec::new();

        let find_music = |sequence: &Path, media: Option<&str>| -> Option<PathBuf> {
            if options.skip_music {
                None
            } else {
                pf_audio::find_audio(sequence, media)
            }
        };
        let import = pf_xlights::import_sequence_file(request.xsq, show, find_music)?;
        let seq = &import.sequence;
        let (music, audio) = match &seq.audio {
            None if import.media_file.is_some() => (Music::Missing, AudioSource::none()),
            None => (Music::None, AudioSource::none()),
            Some(file) => {
                match pf_analysis::audio_track_file(Path::new(file), seq.frame_ms, &|| false, &|_| {}) {
                    Ok(track) => (Music::Read, AudioSource::ready(Arc::new(track))),
                    Err(e) => {
                        notes.push(format!("The music couldn't be read ({e})."));
                        (Music::Missing, AudioSource::none())
                    }
                }
            }
        };
        let pictures = Pictures::new(|file| std::fs::read(file).map_err(|e| e.to_string()));
        let render = |seq: &Sequence| {
            let exported = export_fseq(
                show,
                &comparison.map,
                seq,
                &audio,
                &pictures,
                Cursor::new(Vec::new()),
                0,
                |_, _| true,
            );
            let (bytes, summary) = match exported {
                Ok((out, summary)) => (out.into_inner(), summary),
                Err(ExportError::NoChannels) => return Err(FidelityError::NoChannels),
                Err(e) => return Err(FidelityError::Render(e.to_string())),
            };
            let frames = pf_fseq::Sequence::from_reader(Cursor::new(bytes))
                .map_err(|e| FidelityError::Render(e.to_string()))?;
            Ok((frames, summary))
        };
        let (pixelflow, summary) = render(seq)?;
        // The same sequence from other seeds, when any of its effects draws on chance: what
        // PixelFlow's own renders disagree on is chance, and isn't held against the import.
        let draws_on_chance = seq.effects().any(|e| chance::chance(e).chance != Chance::Nothing);
        let mut others = Vec::new();
        if draws_on_chance && !options.by_moment {
            for draw in 1..=u64::from(options.seeds.unwrap_or(OTHER_SEEDS)) {
                let mut again = seq.clone();
                for effect in again
                    .rows
                    .iter_mut()
                    .flat_map(|r| &mut r.layers)
                    .flat_map(|l| &mut l.effects)
                {
                    effect.id = chance::reseeded(effect.id, draw);
                }
                others.push(render(&again)?.0);
            }
        }

        let (slots, mut props) = lay_out(&comparison, summary.channels, &reference);
        let geometry = SceneGeometry::new(show, &comparison.map);
        for (entry, layout) in props.iter_mut().zip(&comparison.map.props) {
            let buffer = geometry.buffer(Target::Prop(layout.prop));
            let mut at = vec![(0.0, 0.0); entry.pixels];
            for (&g, px) in buffer.show_pixels().iter().zip(&buffer.pixels) {
                if let Some(place) = (g as usize).checked_sub(entry.first).and_then(|k| at.get_mut(k)) {
                    *place = (px.u, px.v);
                }
            }
            entry.grid = Rc::new(Grid::new(&at, buffer.columns, buffer.rows));
        }
        let labels: HashMap<_, _> = import.effects.iter().map(|e| (e.id, e)).collect();
        let mut measured_of: HashMap<Target, TargetPixels> = HashMap::new();
        let mut effects = Vec::new();
        for row in &seq.rows {
            let (pixels, measured, grid) = measured_of
                .entry(row.target)
                .or_insert_with(|| {
                    let buffer = geometry.buffer(row.target);
                    // Each measured pixel once, with its place on the target's own grid.
                    let mut measured: Vec<(u32, (f32, f32))> = buffer
                        .show_pixels()
                        .iter()
                        .zip(&buffer.pixels)
                        .map(|(&g, px)| (g, (px.u, px.v)))
                        .filter(|&(g, _)| slots.get(g as usize).is_some_and(Option::is_some))
                        .collect();
                    measured.sort_by_key(|&(g, _)| g);
                    measured.dedup_by_key(|&mut (g, _)| g);
                    let at: Vec<(f32, f32)> = measured.iter().map(|&(_, at)| at).collect();
                    let grid = Grid::new(&at, buffer.columns, buffer.rows);
                    (
                        buffer.len(),
                        measured.iter().map(|&(g, _)| g).collect(),
                        Rc::new(grid),
                    )
                })
                .clone();
            for (layer, effect) in row
                .layers
                .iter()
                .enumerate()
                .flat_map(|(i, layer)| layer.effects.iter().map(move |e| (i, e)))
            {
                let imported = labels.get(&effect.id);
                let kind =
                    imported.map_or_else(|| effect.kind().label().to_string(), |i| i.xlights_name.clone());
                let (label, effect_notes) = match imported.map(|i| &i.fidelity) {
                    Some(Fidelity::Exact) => (Label::Exact, Vec::new()),
                    Some(Fidelity::Approximate(why)) => (Label::Approximate, why.clone()),
                    Some(Fidelity::Placeholder) => (Label::Placeholder, Vec::new()),
                    Some(Fidelity::Skipped) | None => (
                        Label::Approximate,
                        vec!["the importer gave it no label".to_string()],
                    ),
                };
                let (class, class_reason, sparkles) = class_of(&kind, effect, music);
                let declared = chance::chance(effect);
                let expected = imported.map_or(Verdict::Close, |i| i.verdict);
                effects.push(EffectEntry {
                    target: row.target,
                    target_name: target_name(show, row.target),
                    kind,
                    pixelflow_kind: effect.kind().label().to_string(),
                    label,
                    notes: effect_notes,
                    class,
                    class_reason,
                    sparkles,
                    chance: declared.chance,
                    chance_reason: declared.why,
                    shares_chance: false,
                    start_ms: effect.start_ms,
                    end_ms: effect.end_ms,
                    layer,
                    pixels,
                    measured: measured.clone(),
                    grid: grid.clone(),
                    expected,
                });
            }
        }
        // Which effects share pixels, while they play, with one that draws on chance. The same
        // two targets are asked about again and again, so each answer is kept.
        let mut meet: HashMap<(usize, usize), bool> = HashMap::new();
        let mut meets = |a: &Rc<[u32]>, b: &Rc<[u32]>| {
            let key = (
                Rc::as_ptr(a).cast::<u32>() as usize,
                Rc::as_ptr(b).cast::<u32>() as usize,
            );
            *meet.entry(key).or_insert_with(|| {
                // Both lists are in order.
                let (mut i, mut j) = (0, 0);
                while i < a.len() && j < b.len() {
                    match a[i].cmp(&b[j]) {
                        std::cmp::Ordering::Less => i += 1,
                        std::cmp::Ordering::Greater => j += 1,
                        std::cmp::Ordering::Equal => return true,
                    }
                }
                false
            })
        };
        let shares: Vec<bool> = effects
            .iter()
            .enumerate()
            .map(|(i, effect)| {
                effects.iter().enumerate().any(|(j, other)| {
                    j != i
                        && other.chance != Chance::Nothing
                        && other.start_ms < effect.end_ms
                        && effect.start_ms < other.end_ms
                        && meets(&effect.measured, &other.measured)
                })
            })
            .collect();
        for (effect, shares) in effects.iter_mut().zip(shares) {
            effect.shares_chance = shares;
        }
        let label = options.label.clone().unwrap_or_else(|| {
            request
                .xsq
                .file_stem()
                .map_or_else(|| "sequence".to_string(), |s| s.to_string_lossy().into_owned())
        });
        Ok(Self {
            options,
            label,
            comparison,
            import,
            geometry,
            pixelflow,
            others,
            reference,
            slots,
            props,
            effects,
            music,
            notes,
        })
    }

    pub fn show(&self) -> &Show {
        &self.comparison.show
    }

    pub fn sequence(&self) -> &Sequence {
        &self.import.sequence
    }

    pub fn geometry(&self) -> &SceneGeometry {
        &self.geometry
    }

    /// The imported effects, in the order the scorecard lists them.
    pub fn effects(&self) -> &[EffectEntry] {
        &self.effects
    }

    /// Pixels in the show (the length [`Prepared::read_frame`] fills).
    pub fn pixel_count(&self) -> usize {
        self.slots.len()
    }

    /// Whether both sides have the show pixel `pixel`.
    pub fn measured(&self, pixel: usize) -> bool {
        self.slots.get(pixel).is_some_and(Option::is_some)
    }

    /// xLights' frame for PixelFlow's frame `frame` (the one playing at the same moment, moved
    /// by the frame offset), when the reference has it.
    fn reference_frame(&self, frame: u32) -> Option<u32> {
        let t = u64::from(frame) * u64::from(self.import.sequence.frame_ms);
        let header = self.reference.header();
        let at = i64::try_from(t / u64::from(header.step_ms)).ok()? + self.options.frame_offset;
        u32::try_from(at).ok().filter(|&f| f < header.frames)
    }

    /// Both sides' colors for every show pixel at PixelFlow's frame `frame` (black where a side
    /// has no data). False when the reference doesn't reach that moment.
    pub fn read_frame(
        &mut self,
        frame: u32,
        pixelflow: &mut [[u8; 3]],
        xlights: &mut [[u8; 3]],
    ) -> Result<bool, FidelityError> {
        let Some(at) = self.reference_frame(frame) else {
            return Ok(false);
        };
        if frame >= self.pixelflow.header().frames {
            return Ok(false);
        }
        let mut ours = vec![0u8; self.pixelflow.header().channels as usize];
        let mut theirs = vec![0u8; self.reference.header().channels as usize];
        self.pixelflow
            .read_frame(frame, &mut ours)
            .map_err(|e| FidelityError::Render(e.to_string()))?;
        self.reference
            .read_frame(at, &mut theirs)
            .map_err(FidelityError::ReferenceFrame)?;
        decode(&self.slots, &ours, pixelflow);
        decode(&self.slots, &theirs, xlights);
        Ok(true)
    }

    /// Where the reference looks unrendered: for each prop, the frames of every stretch at least
    /// `dark_run_ms` long that the reference leaves the whole prop dark while PixelFlow lights it
    /// on at least half the frames. xLights saves whatever it has rendered so far, so a file can
    /// hold black where the sequence has effects; scoring that would blame the effects.
    ///
    /// A stretch that runs to the end of the file says more: xLights was still rendering that
    /// prop when the file was saved, and what the file holds for it before it goes dark needn't
    /// be the finished picture either. In a real show's file, two props showed their own row's
    /// effects alone, frame for frame, for the minute before they went dark for good, where up
    /// to then they had shown them under a group's; other props had gone dark at the start of
    /// that minute. How far back the unfinished part goes can't be read from one prop. The
    /// earliest moment any prop is missing light it never gets back is as far as the file
    /// shows it, so every prop cut short is left out from that moment on.
    fn dark_runs(&mut self) -> Result<LeftOut, FidelityError> {
        let frames = self.pixelflow.header().frames;
        let frame_ms = self.import.sequence.frame_ms.max(1);
        let lit = self.options.limits.lit;
        let needed = u64::from(self.options.limits.dark_run_ms).div_ceil(u64::from(frame_ms));
        let mut left_out = vec![vec![false; frames as usize]; self.props.len()];
        if self.options.limits.dark_run_ms == 0 {
            return Ok(LeftOut {
                frames: left_out,
                dark: 0,
                cut_short: None,
            });
        }
        let count = self.slots.len();
        let (mut ours, mut theirs) = (vec![[0u8; 3]; count], vec![[0u8; 3]; count]);
        // For each prop, each frame: whether the reference has it, and whether each side lights
        // any of its pixels.
        let mut seen = vec![vec![(false, false, false); frames as usize]; self.props.len()];
        for frame in 0..frames {
            if !self.read_frame(frame, &mut ours, &mut theirs)? {
                continue;
            }
            for (p, prop) in self.props.iter().enumerate() {
                let range = prop.first..prop.first + prop.pixels;
                let any = |colors: &[[u8; 3]]| colors[range.clone()].iter().flatten().any(|&c| c > lit);
                seen[p][frame as usize] = (true, any(&ours), any(&theirs));
            }
        }
        for (p, prop) in self.props.iter().enumerate() {
            if prop.measured == 0 {
                continue;
            }
            let mut start = 0;
            while start < frames as usize {
                let dark = |f: usize| seen[p][f].0 && !seen[p][f].2;
                if !dark(start) {
                    start += 1;
                    continue;
                }
                let end = (start..frames as usize)
                    .find(|&f| !dark(f))
                    .unwrap_or(frames as usize);
                let ours_lit = (start..end).filter(|&f| seen[p][f].1).count();
                if (end - start) as u64 >= needed && ours_lit * 2 >= end - start {
                    left_out[p][start..end].fill(true);
                }
                start = end;
            }
        }
        let dark = left_out.iter().filter(|frames| frames.contains(&true)).count();
        // The props dark to the end of the file, each with the first frame of that last stretch
        // PixelFlow lights it on: the first light the file doesn't have.
        let last = (0..frames as usize)
            .rev()
            .find(|&f| seen.first().is_some_and(|prop| prop[f].0));
        let cut: Vec<(usize, usize)> = (0..self.props.len())
            .filter_map(|p| {
                let last = last.filter(|&f| left_out[p][f])?;
                let start = (0..last).rev().find(|&f| !left_out[p][f]).map_or(0, |f| f + 1);
                Some((p, (start..=last).find(|&f| seen[p][f].1)?))
            })
            .collect();
        let first = cut.iter().map(|&(_, missing)| missing).min();
        if let (Some(first), Some(last)) = (first, last) {
            for &(p, _) in &cut {
                left_out[p][first..=last].fill(true);
            }
        }
        Ok(LeftOut {
            frames: left_out,
            dark,
            cut_short: first.map(|first| (first as u32, cut.len())),
        })
    }

    /// Measures every frame and fills in the scorecard.
    pub fn score(&mut self) -> Result<Scorecard, FidelityError> {
        let LeftOut {
            frames: left_out,
            dark: dark_props,
            cut_short,
        } = self.dark_runs()?;
        let limits = self.options.limits;
        let seq = &self.import.sequence;
        let frame_ms = seq.frame_ms.max(1);
        let frames = self.pixelflow.header().frames;
        let count = self.slots.len();
        let measurable: Vec<usize> = (0..count).filter(|&g| self.slots[g].is_some()).collect();
        // Each pixel's prop, and whether the pixel counts on the frame being measured.
        let mut prop_of = vec![0usize; count];
        for (p, prop) in self.props.iter().enumerate() {
            prop_of[prop.first..prop.first + prop.pixels].fill(p);
        }
        let mut live = vec![false; count];

        let mut ours = vec![0u8; self.pixelflow.header().channels as usize];
        let mut theirs = vec![0u8; self.reference.header().channels as usize];
        let mut neighbor = theirs.clone();
        let mut colors = [vec![[0u8; 3]; count], vec![[0u8; 3]; count]];
        let mut shifted = vec![[0u8; 3]; count];
        let mut samples = vec![Sample::default(); count];
        let mut was_lit = vec![[false; 2]; count];
        let mut cover = vec![0u16; count];

        let mut prop_stats = vec![Stats::new(frame_ms, &limits); self.props.len()];
        let mut prop_looks: Vec<LookStats> = self
            .props
            .iter()
            .map(|prop| LookStats::new(prop.grid.clone(), frame_ms, &limits))
            .collect();
        let mut reversed = vec![0u64; self.props.len()];
        let mut unattributed = Stats::new(frame_ms, &limits);
        let mut solo = vec![Stats::new(frame_ms, &limits); self.effects.len()];
        let mut layered = vec![Stats::new(frame_ms, &limits); self.effects.len()];
        // Each effect's look, alone and layered: summed while it plays, worked out when it ends.
        let mut looking: Vec<Option<[LookStats; 2]>> = vec![None; self.effects.len()];
        let mut looks: Vec<Looked> = vec![[None; 2]; self.effects.len()];
        let mut scratch = [Scratch::default(), Scratch::default()];
        // An effect that draws on chance is read between more renders than PixelFlow's first and
        // xLights': each of PixelFlow's other seeds against xLights' (`with_xlights` of these),
        // then PixelFlow's renders against one another. A render is numbered from PixelFlow's
        // first (0) through its other seeds to xLights' (the last).
        let others = self.others.len();
        let xlights = others + 1;
        let mut pairs: Vec<(usize, usize)> = (1..=others).map(|seed| (seed, xlights)).collect();
        let with_xlights = pairs.len();
        pairs.extend((0..=others).flat_map(|a| (a + 1..=others).map(move |b| (a, b))));
        let mut seeding: Vec<Vec<[LookStats; 2]>> = vec![Vec::new(); self.effects.len()];
        let mut seeded: Vec<Vec<Looked>> = vec![Vec::new(); self.effects.len()];
        let mut other = vec![0u8; self.pixelflow.header().channels as usize];
        let mut drawn_by = vec![vec![[0u8; 3]; count]; others];
        // Each effect's share of the pixel-frames it was lit on: a pixel under three effects
        // counts a third for each.
        let mut shares = vec![0f64; self.effects.len()];
        // Frames from each effect's start to the first one a side lights its pixels on; kept
        // only for effects that start on dark pixels.
        let mut onsets: Vec<Option<[Option<u32>; 2]>> = vec![None; self.effects.len()];

        // Each effect's frames: those whose moment is within it.
        let span = |e: &EffectEntry| {
            let first = e.start_ms.div_ceil(u64::from(frame_ms));
            let end = e.end_ms.div_ceil(u64::from(frame_ms));
            (first, end)
        };
        let mut starting: Vec<usize> = (0..self.effects.len()).collect();
        starting.sort_by_key(|&i| span(&self.effects[i]).0);
        let mut next = 0;
        let mut active: Vec<usize> = Vec::new();

        let stride = frames.div_ceil(OFFSET_SAMPLES).max(1);
        let mut offsets: BTreeMap<i64, (u64, u64)> = BTreeMap::new();
        let (mut compared, mut missing) = (0u32, 0u32);

        for frame in 0..frames {
            let at = u64::from(frame);
            active.retain(|&i| {
                let playing = span(&self.effects[i]).1 > at;
                if !playing && let Some(stats) = looking[i].take() {
                    looks[i] = stats.map(LookStats::looks);
                    seeded[i] = std::mem::take(&mut seeding[i])
                        .into_iter()
                        .map(|pair| pair.map(LookStats::looks))
                        .collect();
                }
                playing
            });
            let mut started = Vec::new();
            while next < starting.len() && span(&self.effects[starting[next]]).0 <= at {
                let i = starting[next];
                next += 1;
                if span(&self.effects[i]).1 > at {
                    active.push(i);
                    started.push(i);
                    // Held against PixelFlow's other seeds: an effect that draws on chance, and
                    // (where others are on its pixels) one that shares them with such an effect.
                    let effect = &self.effects[i];
                    let draws = effect.chance != Chance::Nothing;
                    let held = others > 0 && (draws || effect.shares_chance);
                    let stats = || {
                        let stats = LookStats::new(effect.grid.clone(), frame_ms, &limits);
                        if held { stats.over(effect.chance) } else { stats }
                    };
                    looking[i] = Some([stats(), stats()]);
                    if held {
                        seeding[i] = pairs.iter().map(|_| [stats(), stats()]).collect();
                    }
                }
            }
            let Some(theirs_at) = self.reference_frame(frame) else {
                missing += 1;
                was_lit.fill([false; 2]);
                continue;
            };
            compared += 1;
            self.pixelflow
                .read_frame(frame, &mut ours)
                .map_err(|e| FidelityError::Render(e.to_string()))?;
            self.reference
                .read_frame(theirs_at, &mut theirs)
                .map_err(FidelityError::ReferenceFrame)?;
            decode(&self.slots, &ours, &mut colors[PIXELFLOW]);
            decode(&self.slots, &theirs, &mut colors[XLIGHTS]);
            if active.iter().any(|&i| !seeding[i].is_empty()) {
                for (frames, drawn) in self.others.iter_mut().zip(&mut drawn_by) {
                    frames
                        .read_frame(frame, &mut other)
                        .map_err(|e| FidelityError::Render(e.to_string()))?;
                    decode(&self.slots, &other, drawn);
                }
            }
            for &g in &measurable {
                live[g] = !left_out[prop_of[g]][frame as usize];
                samples[g] = Sample::of(colors[PIXELFLOW][g], colors[XLIGHTS][g], &limits);
            }

            // The timing check: the same frame against xLights' frames around it.
            if frame % stride == 0 {
                for k in -OFFSETS..=OFFSETS {
                    let Some(other) = u32::try_from(i64::from(theirs_at) + k)
                        .ok()
                        .filter(|&f| f < self.reference.header().frames)
                    else {
                        continue;
                    };
                    let against = if k == 0 {
                        &colors[XLIGHTS]
                    } else {
                        self.reference
                            .read_frame(other, &mut neighbor)
                            .map_err(FidelityError::ReferenceFrame)?;
                        decode(&self.slots, &neighbor, &mut shifted);
                        &shifted
                    };
                    let pixels = measurable.iter().filter(|&&g| live[g]);
                    let sum: u64 = pixels
                        .clone()
                        .map(|&g| abs_difference(colors[PIXELFLOW][g], against[g]))
                        .sum();
                    let entry = offsets.entry(k).or_default();
                    entry.0 += sum;
                    entry.1 += pixels.count() as u64 * 3;
                }
            }

            for &i in &started {
                let dark = self.effects[i]
                    .measured
                    .iter()
                    .all(|&g| was_lit[g as usize] == [false; 2]);
                if dark {
                    onsets[i] = Some([None; 2]);
                }
            }

            cover.fill(0);
            for &i in &active {
                for &g in self.effects[i].measured.iter() {
                    cover[g as usize] += 1;
                }
            }
            for &i in &active {
                let first = span(&self.effects[i]).0;
                let mut lit = [false; 2];
                let mut look = looking[i].as_mut();
                for (k, &g) in self.effects[i].measured.iter().enumerate() {
                    if !live[g as usize] {
                        continue;
                    }
                    let sample = &samples[g as usize];
                    let drawn = [colors[PIXELFLOW][g as usize], colors[XLIGHTS][g as usize]];
                    layered[i].add(sample);
                    shares[i] += 1.0 / f64::from(cover[g as usize]);
                    if let Some([alone, all]) = &mut look {
                        all.add(&mut scratch[1], frame, k, drawn);
                        if cover[g as usize] == 1 {
                            alone.add(&mut scratch[0], frame, k, drawn);
                        }
                    }
                    if cover[g as usize] == 1 {
                        solo[i].add(sample);
                    }
                    lit[PIXELFLOW] |= sample.lit_in(PIXELFLOW);
                    lit[XLIGHTS] |= sample.lit_in(XLIGHTS);
                }
                layered[i].end_frame();
                solo[i].end_frame();
                if let Some([alone, all]) = &mut look {
                    alone.end_frame(&mut scratch[0], frame);
                    all.end_frame(&mut scratch[1], frame);
                }
                // The same pixels between each other two renders.
                let draws = self.effects[i].chance != Chance::Nothing;
                for ([alone, all], &(a, b)) in seeding[i].iter_mut().zip(&pairs) {
                    let from = |render: usize, g: usize| match render {
                        0 => colors[PIXELFLOW][g],
                        seed if seed <= others => drawn_by[seed - 1][g],
                        _ => colors[XLIGHTS][g],
                    };
                    for (k, &g) in self.effects[i].measured.iter().enumerate() {
                        let g = g as usize;
                        if !live[g] {
                            continue;
                        }
                        let drawn = [from(a, g), from(b, g)];
                        all.add(&mut scratch[1], frame, k, drawn);
                        // Alone on a pixel, only what the effect itself draws can be chance.
                        if draws && cover[g] == 1 {
                            alone.add(&mut scratch[0], frame, k, drawn);
                        }
                    }
                    alone.end_frame(&mut scratch[0], frame);
                    all.end_frame(&mut scratch[1], frame);
                }
                if let Some(onset) = &mut onsets[i] {
                    for side in [PIXELFLOW, XLIGHTS] {
                        if lit[side] && onset[side].is_none() {
                            onset[side] = Some((at - first) as u32);
                        }
                    }
                }
            }

            for (p, prop) in self.props.iter().enumerate() {
                if prop.measured == 0 || left_out[p][frame as usize] {
                    continue;
                }
                let range = prop.first..prop.first + prop.pixels;
                let lit = range.clone().any(|g| self.slots[g].is_some() && samples[g].lit());
                if !lit {
                    continue;
                }
                for g in range.clone() {
                    if self.slots[g].is_some() {
                        prop_stats[p].add(&samples[g]);
                        let drawn = [colors[PIXELFLOW][g], colors[XLIGHTS][g]];
                        prop_looks[p].add(&mut scratch[0], frame, g - prop.first, drawn);
                    }
                }
                prop_stats[p].end_frame();
                prop_looks[p].end_frame(&mut scratch[0], frame);
                if prop.measured == prop.pixels {
                    reversed[p] += range
                        .clone()
                        .zip(range.rev())
                        .map(|(g, mirror)| abs_difference(colors[PIXELFLOW][g], colors[XLIGHTS][mirror]))
                        .sum::<u64>();
                }
            }
            for &g in &measurable {
                if !live[g] {
                    was_lit[g] = [false; 2];
                    continue;
                }
                if cover[g] == 0 && samples[g].lit() {
                    unattributed.add(&samples[g]);
                }
                was_lit[g] = [samples[g].lit_in(PIXELFLOW), samples[g].lit_in(XLIGHTS)];
            }
            unattributed.end_frame();
        }

        for (stats, look) in looking.into_iter().zip(&mut looks) {
            if let Some(stats) = stats {
                *look = stats.map(LookStats::looks);
            }
        }
        for (stats, held) in seeding.into_iter().zip(&mut seeded) {
            if !stats.is_empty() {
                *held = stats.into_iter().map(|pair| pair.map(LookStats::looks)).collect();
            }
        }
        // The look an effect is judged on, alone (0) or layered (1), and the one moment by
        // moment when that is another: an effect that draws on chance is judged next to
        // PixelFlow's other seeds, and so is one layered with such an effect.
        let judged = |index: usize, basis: usize| -> (Option<Look>, Option<Look>) {
            let Some((by_moment, parts)) = looks[index][basis] else {
                return (None, None);
            };
            let draws = self.effects[index].chance != Chance::Nothing;
            if seeded[index].is_empty() || (basis == 0 && !draws) {
                return (Some(by_moment), None);
            }
            let (mut between, mut within) = (vec![parts], Vec::new());
            for (n, pair) in seeded[index].iter().enumerate() {
                if let Some((_, parts)) = pair[basis] {
                    if n < with_xlights {
                        between.push(parts);
                    } else {
                        within.push(parts);
                    }
                }
            }
            // What chance decides in the picture: the effect's own moments, or no more than
            // what it or its neighbors draw.
            let chance = self.effects[index].chance.max(Chance::Drawn);
            (
                Some(next_to_seeds(&by_moment, chance, &between, &within)),
                Some(by_moment),
            )
        };
        let mut overall = Stats::new(frame_ms, &limits);
        for stats in &prop_stats {
            overall.merge(stats);
        }
        let props: Vec<PropScore> = self
            .props
            .iter()
            .zip(&prop_stats)
            .zip(&reversed)
            .zip(prop_looks)
            .map(|(((prop, stats), &reversed), look)| PropScore {
                name: prop.name.clone(),
                pixels: prop.pixels,
                measured_pixels: prop.measured,
                channels: prop.channels,
                on: prop.source,
                look: look.finish(),
                scores: stats.scores(),
                reversed_mae: (prop.measured == prop.pixels && stats.samples() > 0)
                    .then(|| round(reversed as f64 / stats.samples() as f64 / 3.0)),
            })
            .collect();
        let effects: Vec<EffectScore> = self
            .effects
            .iter()
            .enumerate()
            .map(|(index, e)| {
                let (look_solo, look_solo_by_moment) = judged(index, 0);
                let (look_layered, look_layered_by_moment) = judged(index, 1);
                EffectScore {
                    index,
                    kind: e.kind.clone(),
                    pixelflow_kind: e.pixelflow_kind.clone(),
                    target: e.target_name.clone(),
                    start_ms: e.start_ms,
                    end_ms: e.end_ms,
                    layer: e.layer,
                    label: e.label,
                    notes: e.notes.clone(),
                    class: e.class,
                    class_reason: e.class_reason.clone(),
                    sparkles: round(e.sparkles),
                    pixels: e.pixels,
                    measured_pixels: e.measured.len(),
                    solo: solo[index].scores(),
                    layered: layered[index].scores(),
                    share: round(shares[index] * f64::from(frame_ms) / 1000.0),
                    onset: onsets[index].and_then(|o| {
                        Some(Onset {
                            pixelflow: o[PIXELFLOW]?,
                            xlights: o[XLIGHTS]?,
                        })
                    }),
                    expected: e.expected,
                    chance: e.chance,
                    chance_reason: e.chance_reason.map(str::to_string),
                    look_solo,
                    look_layered,
                    look_solo_by_moment,
                    look_layered_by_moment,
                    look: None,
                    solo_stats: solo[index].clone(),
                    layered_stats: layered[index].clone(),
                }
            })
            .collect();

        let offsets: Vec<OffsetError> = offsets
            .into_iter()
            .filter(|(_, (_, n))| *n > 0)
            .map(|(offset, (sum, n))| OffsetError {
                offset: offset + self.options.frame_offset,
                mae: round(sum as f64 / n as f64),
            })
            .collect();
        let best_offset = offsets
            .iter()
            .min_by(|a, b| a.mae.total_cmp(&b.mae).then(a.offset.abs().cmp(&b.offset.abs())))
            .map_or(self.options.frame_offset, |o| o.offset);
        let mut onset_deltas: BTreeMap<i64, usize> = BTreeMap::new();
        for onset in effects.iter().filter_map(|e| e.onset) {
            *onset_deltas
                .entry(i64::from(onset.xlights) - i64::from(onset.pixelflow))
                .or_default() += 1;
        }
        let header = self.reference.header();
        let timing = Timing {
            frame_ms,
            reference_step_ms: header.step_ms,
            frames,
            frames_compared: compared,
            frames_missing: missing,
            frame_offset: self.options.frame_offset,
            offsets,
            best_offset,
            onset_deltas,
        };

        // The stretches left out, and how much of what could be measured they are.
        let mut gaps = Vec::new();
        let (mut left, mut whole) = (0u64, 0u64);
        for ((prop, frames_out), stats) in self.props.iter().zip(&left_out).zip(&prop_stats) {
            let mut start = 0;
            while start < frames_out.len() {
                if !frames_out[start] {
                    start += 1;
                    continue;
                }
                let end = (start..frames_out.len())
                    .find(|&f| !frames_out[f])
                    .unwrap_or(frames_out.len());
                gaps.push(Gap {
                    prop: prop.name.clone(),
                    start_ms: start as u64 * u64::from(frame_ms),
                    end_ms: end as u64 * u64::from(frame_ms),
                });
                left += (end - start) as u64 * prop.measured as u64;
                start = end;
            }
            whole += stats.samples();
        }
        let left_out_share = if left + whole == 0 {
            0.0
        } else {
            round(left as f64 / (left + whole) as f64)
        };

        let measured_on = |source: Source| -> usize {
            self.props
                .iter()
                .filter(|p| p.source == source)
                .map(|p| p.measured)
                .sum()
        };
        let coverage = Coverage {
            props: self.props.len(),
            props_measured: self.props.iter().filter(|p| p.measured > 0).count(),
            pixels: count,
            pixels_measured: measurable.len(),
            pixels_on_controllers: measured_on(Source::Controller),
            pixels_on_stand_ins: measured_on(Source::StandIn),
            unmeasured_props: self
                .props
                .iter()
                .filter(|p| p.measured == 0)
                .map(|p| p.name.clone())
                .collect(),
            effects: effects.len(),
            effects_measured: effects.iter().filter(|e| e.layered.is_some()).count(),
            effects_measured_alone: effects.iter().filter(|e| e.solo.is_some()).count(),
        };
        let mut notes = self.notes.clone();
        if header.step_ms != frame_ms {
            notes.push(format!(
                "The sequence's frames are {frame_ms} ms apart and the reference's {} ms: each PixelFlow \
                 frame is compared with the reference frame playing at its moment.",
                header.step_ms
            ));
        }
        if missing > 0 {
            notes.push(format!(
                "The reference is shorter: {missing} of {frames} frames have nothing to compare with."
            ));
        }
        let share = format!("{:.0}% of what could be measured", left_out_share * 100.0);
        if !gaps.is_empty() {
            notes.push(format!(
                "The reference is dark for {} s or more on {dark_props} props while PixelFlow lights \
                 them{}: xLights saves a file with whatever it has rendered so far, so these stretches \
                 look unrendered and are left out.",
                limits.dark_run_ms / 1000,
                if cut_short.is_none() {
                    format!(", {share}")
                } else {
                    String::new()
                },
            ));
        }
        if let Some((first, props)) = cut_short {
            notes.push(format!(
                "The reference was cut short: {} dark to its end while PixelFlow lights {}, from {} at \
                 the earliest. xLights was still rendering there when it saved the file, and what it \
                 holds from that moment on may be some of a prop's rows without the rest, so {} left \
                 out from then. In all, {share} is left out.",
                if props == 1 {
                    "1 prop is".to_string()
                } else {
                    format!("{props} props are")
                },
                if props == 1 { "it" } else { "them" },
                pf_sequence::format_ms(u64::from(first) * u64::from(frame_ms)),
                if props == 1 { "it is" } else { "all of them are" },
            ));
        }
        // xLights rounds a file's channels up to a multiple of four.
        let wanted = self.comparison.channels;
        let sparse = self.reference.stored_ranges().as_slice() != std::slice::from_ref(&(0..header.channels));
        if !sparse && header.channels != wanted && header.channels != wanted.next_multiple_of(4) {
            notes.push(format!(
                "The reference holds {} channels and the layout's props end at channel {wanted}: the \
                 layout has changed since xLights rendered it (or it was rendered on another), and \
                 props whose channels moved can't match.",
                header.channels
            ));
        }
        if !coverage.unmeasured_props.is_empty() {
            notes.push(format!(
                "{} of {} props have no channels in the reference (or none PixelFlow could place), so \
                 they aren't measured.",
                coverage.unmeasured_props.len(),
                coverage.props
            ));
        }
        Ok(Scorecard::assemble(
            SequenceInfo {
                label: self.label.clone(),
                name: seq.name.clone(),
                duration_ms: seq.duration_ms,
                frame_ms,
                import: self.import.summary,
                import_notes: self.import.all_notes(),
                music: self.music,
            },
            ReferenceInfo {
                channels: header.channels,
                frames: header.frames,
                step_ms: header.step_ms,
                producer: header.producer.clone(),
                sparse,
                left_out: gaps,
                left_out_share,
            },
            Method {
                tool: TOOL.to_string(),
                limits,
                audit: self.options.audit,
                matches: crate::look::MATCHES,
                close: crate::look::CLOSE,
                by_moment: self.options.by_moment,
                other_seeds: self.others.len(),
                little: crate::look::LITTLE,
                set_aside: self.comparison.set_aside,
                stand_in_controllers: self.comparison.stand_ins.len(),
            },
            timing,
            coverage,
            overall.scores(),
            unattributed.scores(),
            props,
            effects,
            notes,
        ))
    }
}

/// Imports, renders, and scores one sequence against its reference.
pub fn compare(request: Request, options: Options) -> Result<Scorecard, FidelityError> {
    Prepared::open(request, options)?.score()
}

fn round(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

fn abs_difference(a: [u8; 3], b: [u8; 3]) -> u64 {
    a.into_iter().zip(b).map(|(x, y)| u64::from(x.abs_diff(y))).sum()
}

/// Each measured pixel's color in `frame` (the rest stay as they are: black).
fn decode(slots: &[Option<Slot>], frame: &[u8], out: &mut [[u8; 3]]) {
    for (slot, rgb) in slots.iter().zip(out) {
        if let Some(slot) = slot {
            let at = slot.channel as usize;
            *rgb = slot.at.map(|c| frame[at + usize::from(c)]);
        }
    }
}

/// Finds every show pixel's channels in PixelFlow's export (`exported` channels) and keeps
/// those the reference holds too.
fn lay_out(
    comparison: &ComparisonShow,
    exported: u32,
    reference: &pf_fseq::Sequence,
) -> (Vec<Option<Slot>>, Vec<PropEntry>) {
    let (show, map) = (&comparison.show, &comparison.map);
    let stored = reference.stored_ranges();
    let both = |first: u64, channels: u64| {
        first + channels <= u64::from(exported)
            && stored
                .iter()
                .any(|r| u64::from(r.start) <= first && first + channels <= u64::from(r.end))
    };
    let mut props = Vec::with_capacity(map.props.len());
    let mut first = 0;
    for layout in &map.props {
        props.push(PropEntry {
            name: show
                .prop(layout.prop)
                .map_or_else(String::new, |p| p.name.clone()),
            first,
            pixels: layout.nodes as usize,
            measured: 0,
            channels: None,
            source: Source::None,
            grid: Rc::new(Grid::new(&[], 1, 1)),
        });
        first += layout.nodes as usize;
    }
    let mut slots = vec![None; first];
    for block in export_layout(show, map).blocks {
        let Some(at) = show.controllers.iter().position(|c| c.id == block.controller) else {
            continue;
        };
        let source = if comparison.stand_ins.contains(&block.controller) {
            Source::StandIn
        } else {
            Source::Controller
        };
        for span in &map.controllers[at].spans {
            let cpp = usize::from(span.channels_per_pixel);
            let Some(p) = map.props.iter().position(|l| l.prop == span.prop) else {
                continue;
            };
            if cpp < 3 {
                continue;
            }
            let order = pf_output::wire_order(span.color_order);
            let mut at = [0u8; 3];
            for (wire, &canonical) in order.iter().enumerate().take(cpp) {
                if let Some(slot) = at.get_mut(usize::from(canonical)) {
                    *slot = wire as u8;
                }
            }
            let first_node = (span.frame_offset - map.props[p].frame_offset) / cpp;
            let pixels = span.pixels as usize;
            for k in 0..pixels {
                let wire = if span.reverse { pixels - 1 - k } else { k };
                let controller_channel = span.controller_channel + wire * cpp;
                if controller_channel + cpp > block.count as usize {
                    continue;
                }
                let channel = u64::from(block.start) - 1 + controller_channel as u64;
                let prop = &mut props[p];
                let slot = &mut slots[prop.first + first_node + k];
                if slot.is_some() || !both(channel, cpp as u64) {
                    continue;
                }
                *slot = Some(Slot {
                    channel: channel as u32,
                    at,
                });
                prop.measured += 1;
                prop.source = source;
                let (low, high) = (channel as u32 + 1, (channel + cpp as u64) as u32);
                prop.channels = Some(
                    prop.channels
                        .map_or((low, high), |(a, b)| (a.min(low), b.max(high))),
                );
            }
        }
    }
    (slots, props)
}
