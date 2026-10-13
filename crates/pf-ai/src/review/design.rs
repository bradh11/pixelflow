//! The review's design half: a draft judged as a show designer judges one, beyond whether it
//! keeps time with the song.
//!
//! - **props**: every prop has a part. Props that never light are named.
//! - **dark space**: something rests. A show with every prop lit nearly all the time has nowhere
//!   to go.
//! - **palette**: a section holds to a few colors.
//! - **changes**: something changes where the song does, at each new section.
//! - **scale**: the song's biggest moments light most of the house.
//! - **fit**: effects suit the props they're on (an effect drawn on a grid shows only a slice on a
//!   single run of pixels; a singing face needs a face), going by the targets each effect's guide
//!   gives looks for.
//! - **text**: words can be read: tall enough for their prop, on long enough, and not scrolling
//!   too fast.
//! - **flicker**: effects that flash faster than a frame-by-frame run-through can see (a shimmer
//!   over a large part of the display) are caught from their settings.
//!
//! Each finding names the props, sections, and times, and where a tool call would fix it, gives
//! the call.

use super::{Call, EMPHASIS, HUES, Reviewer, Section, kind_name, lands, percent, rank, where_};
use crate::knowledge::{self, Suit};
use pf_model::{Generator, Prop, PropId, RegionKind, ShapeSource};
use pf_sequence::{Effect, EffectKind, EffectParams, Row, Target, format_ms};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// A prop this bright (0–1, its mean) is lit.
const PROP_LIT: f32 = 0.04;
/// A prop lit less than this share of the time the music plays has no part.
const NO_PART: f32 = 0.02;
/// The share of props lit that counts as the whole house.
const WHOLE_HOUSE: f32 = 0.9;
/// A color on this many props through a section (one prop for a tenth of it) is one of its colors.
const A_COLOR: f32 = 0.1;
/// The most colors a section holds before it reads as a rainbow.
const SECTION_COLORS: usize = 4;
/// Two looks this close are the same look.
const SAME_LOOK: f32 = 0.12;
/// The share of props a big moment should light.
const BIG_MOMENT_PROPS: f32 = 0.5;
/// Moments from this importance are the big ones.
const BIG_IMPORTANCE: f32 = 0.75;
/// Whole-prop flickers a second, from settings, that are too fast over a large area.
const FLICKER_HZ: f32 = 3.0;
/// The share of the show's pixels that is a large area.
const LARGE_AREA: f32 = 0.25;

/// Effects on a prop they don't suit: what's wrong, the prop, its row, and the effects' kind.
type Misfit = (String, String, String, String);
/// The most effects a fix names one by one; more are chosen by kind and time.
const IDS_IN_A_FIX: usize = 40;

/// An effect flashing too fast over a large area.
struct Flicker {
    from: u64,
    to: u64,
    hz: f32,
    /// What it lights, by name.
    target: String,
    id: String,
    kind: EffectKind,
}

/// Words that can't be read: when, on what, why, the effect, and the settings that fix it (when
/// settings can).
struct Unreadable {
    at: u64,
    prop: String,
    why: String,
    id: String,
    settings: Option<serde_json::Value>,
}

/// What a prop gives an effect to draw on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Canvas {
    /// One run of pixels: a line, an arch, a ring. One row tall.
    Run,
    /// Columns and rows: a matrix, a tree.
    Grid { columns: u32, rows: u32 },
    /// Something else (a custom shape, a frame, a spinner): not judged.
    Other,
}

/// The columns and rows of `prop`'s grid, when it has one words can stand on.
pub(super) fn grid(prop: &Prop) -> Option<(u32, u32)> {
    match canvas(prop) {
        Canvas::Grid { columns, rows } => Some((columns, rows)),
        _ => None,
    }
}

fn canvas(prop: &Prop) -> Canvas {
    let ShapeSource::Generator(shape) = &prop.shape else {
        return Canvas::Other;
    };
    match shape {
        Generator::Line { .. } | Generator::PolyLine { .. } | Generator::Wreath { .. } => Canvas::Run,
        Generator::Arch { arches, layers, .. } if *arches <= 1 && layers.is_empty() => Canvas::Run,
        Generator::Circle { layers, .. } if layers.is_empty() => Canvas::Run,
        Generator::Matrix { columns, rows, .. } => Canvas::Grid {
            columns: *columns,
            rows: *rows,
        },
        Generator::Tree {
            strings,
            nodes_per_string,
            ..
        } => Canvas::Grid {
            columns: *strings,
            rows: *nodes_per_string,
        },
        _ => Canvas::Other,
    }
}

/// A color bin as "#rrggbb": the middle of its slice of the color wheel, or white.
fn bin_color(bin: usize) -> String {
    if bin >= HUES - 1 {
        return "#ffffff".into();
    }
    let h = bin as f32 * 30.0 + 15.0;
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
}

/// Names in a row: "A", "A and B", "A, B, and 3 more".
fn named(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => one.to_string(),
        [a, b] => format!("{a} and {b}"),
        [a, b, c] => format!("{a}, {b}, and {c}"),
        [a, b, rest @ ..] => format!("{a}, {b}, and {} more", rest.len()),
    }
}

use knowledge::suits_a_run;

/// The targets `kind`'s guide gives looks for, in words.
fn suited_to(kind: EffectKind) -> String {
    let suits: BTreeSet<Suit> = knowledge::effect_guide(kind)
        .map(|g| g.looks.iter().flat_map(|l| l.suits.clone()).collect())
        .unwrap_or_default();
    suits.iter().map(|s| s.word()).collect::<Vec<_>>().join(", ")
}

/// How many times a second an effect flashes, and how much of its target flashes together
/// (0–1), when its settings alone say so: faster than the run-through's frames can show.
fn flicker(effect: &Effect, frame_ms: u32) -> Option<(f32, f32)> {
    let frames_a_second = 1000.0 / frame_ms.max(1) as f32;
    match &effect.params {
        // Every other frame is the other color.
        EffectParams::On(on) if on.shimmer => Some((frames_a_second / 2.0, 1.0)),
        // The whole prop on and off (or, always on, from one color to the next); at a frame
        // or less a flicker it simply stays lit.
        EffectParams::Shimmer(shimmer)
            if shimmer.rate < frames_a_second && (shimmer.duty < 1.0 || effect.palette.colors.len() > 1) =>
        {
            Some((shimmer.rate, 1.0))
        }
        // A share of the pixels, all on and off together.
        EffectParams::Strobe(strobe) => Some((strobe.rate, strobe.density)),
        _ => None,
    }
}

impl Reviewer<'_> {
    /// The frames while music plays (all of them, without the song's loudness).
    fn playing_frames(&self) -> Vec<usize> {
        let f = &self.frames;
        (0..f.len())
            .filter(|&i| self.song.playing(i as u64 * f.step))
            .collect()
    }

    fn prop_name(&self, id: PropId) -> &str {
        self.subject
            .show
            .props
            .iter()
            .find(|p| p.id == id)
            .map_or("a prop", |p| p.name.as_str())
    }

    /// The rows that light `prop`: its own, then its groups'.
    fn rows_of(&self, prop: PropId) -> Vec<&Row> {
        let doc = self.subject.doc;
        let mut rows: Vec<&Row> = doc
            .rows
            .iter()
            .filter(|r| r.target == Target::Prop(prop))
            .collect();
        rows.extend(
            doc.rows
                .iter()
                .filter(|r| matches!(r.target, Target::Group(_)) && self.covers(r.target).contains(&prop)),
        );
        rows
    }

    /// The sections, or without any, the song in stretches of eight bars.
    fn stretches(&self) -> Vec<Section> {
        if !self.song.sections.is_empty() {
            return self.song.sections.clone();
        }
        let length = 8 * self.song.bar();
        let duration = self.subject.doc.duration_ms;
        (0..duration.div_ceil(length.max(1)))
            .map(|i| {
                let (start, end) = (i * length, ((i + 1) * length).min(duration));
                Section {
                    label: String::new(),
                    name: format!("{}–{}", format_ms(start), format_ms(end)),
                    group: i.to_string(),
                    start,
                    end,
                }
            })
            .collect()
    }

    // ---- props ----

    /// Every prop has a part: those that never light are named, with their rows.
    pub(super) fn props(&mut self) {
        let f = &self.frames;
        let playing = self.playing_frames();
        if f.props.len() < 2 || playing.is_empty() {
            return;
        }
        let unused: Vec<PropId> = f
            .props
            .iter()
            .enumerate()
            .filter(|(p, _)| {
                let lit = playing.iter().filter(|&&i| f.prop(i, *p) > PROP_LIT).count();
                (lit as f32) < NO_PART * playing.len() as f32
            })
            .map(|(_, id)| *id)
            .collect();
        self.criteria.props = Some(percent(1.0 - unused.len() as f32 / f.props.len() as f32));
        if unused.is_empty() {
            return;
        }
        let names: Vec<&str> = unused.iter().map(|id| self.prop_name(*id)).collect();
        let rows: Vec<String> = unused
            .iter()
            .filter_map(|id| self.rows_of(*id).first().map(|r| r.id.to_string()))
            .collect();
        let what = format!(
            "{} of the {} props never light{}: {}. Every prop wants a part, if only in the loud sections.",
            unused.len(),
            f.props.len(),
            if unused.len() * 2 >= f.props.len() {
                " (half the display or more)"
            } else {
                ""
            },
            named(&names)
        );
        let hint = if rows.is_empty() {
            "They have no rows in the sequence: sequence_add_row gives each one, then place effects on them."
                .to_string()
        } else {
            format!(
                "Their rows: {}. Give each a look with place_effects (get_guide topic `roles` says what suits which prop).",
                rows.join(", ")
            )
        };
        self.fixes.add(rank::PROPS, 0, what, Vec::new(), Some(hint));
    }

    // ---- dark space ----

    /// Something rests: the whole house isn't lit nearly all the time.
    pub(super) fn dark_space(&mut self) {
        let f = &self.frames;
        let playing = self.playing_frames();
        if f.props.len() < 3 || playing.is_empty() {
            return;
        }
        let whole = playing
            .iter()
            .filter(|&&i| {
                let lit = (0..f.props.len()).filter(|&p| f.prop(i, p) > PROP_LIT).count();
                lit as f32 >= WHOLE_HOUSE * f.props.len() as f32
            })
            .count();
        let share = whole as f32 / playing.len() as f32;
        self.criteria.dark_space = Some(percent(1.0 - (share - 0.5).max(0.0) / 0.5));
        if share <= 0.7 {
            return;
        }
        let what = format!(
            "Every prop is lit for {:.0}% of the song: with nothing ever dark, nothing can come in. Rest most of the house through the quiet parts, and some of it through the verses.",
            share * 100.0
        );
        // The quiet parts of the song, to thin out: every section under four tenths of the
        // song's loudest, else the quietest one.
        let mut quiet: Vec<(f32, &Section)> = self
            .song
            .sections
            .iter()
            .filter(|s| s.end - s.start >= 2 * self.song.bar())
            .map(|s| {
                let bars: Vec<f32> = self
                    .song
                    .bars
                    .iter()
                    .filter(|b| (s.start..s.end).contains(&b.0))
                    .map(|b| b.2)
                    .collect();
                (bars.iter().sum::<f32>() / bars.len().max(1) as f32, s)
            })
            .collect();
        quiet.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.start.cmp(&b.1.start)));
        let keep = quiet.iter().filter(|q| q.0 < 0.4).count().clamp(1, 4);
        quiet.truncate(keep);
        quiet.sort_by_key(|q| q.1.start);
        if quiet.is_empty() {
            self.fixes.add(
                rank::DARK,
                0,
                what,
                Vec::new(),
                Some("Take a group out for a stretch: change_effects with remove, or an off effect on its row.".into()),
            );
            return;
        }
        let cues = quiet
            .iter()
            .map(|(_, s)| json!({ "cue": "minimal", "at": s.start, "until": s.end }))
            .collect();
        self.fixes.cues(rank::DARK, quiet[0].1.start, what, cues);
    }

    // ---- palette ----

    /// A section holds to a few colors: the colors its effects were given, each counted by the
    /// props it plays on (so a small prop in a sixth color counts, as it does to the eye).
    pub(super) fn palette(&mut self) {
        let doc = self.subject.doc;
        let bar = self.song.bar();
        let mut busy: Vec<(usize, Vec<usize>, Section)> = Vec::new();
        let mut judged = 0;
        for section in self.stretches() {
            let length = section.end.saturating_sub(section.start);
            if length == 0 {
                continue;
            }
            let mut weight = [0.0f32; HUES];
            for row in &doc.rows {
                let props = self.covers(row.target).len().max(1) as f32;
                for effect in row.layers.iter().flat_map(|l| &l.effects) {
                    let overlap = effect
                        .end_ms
                        .min(section.end)
                        .saturating_sub(effect.start_ms.max(section.start));
                    // Effects that play a fair part of the section, not an accent passing through.
                    if overlap < bar.min(length / 4).max(1) {
                        continue;
                    }
                    let mut bins: Vec<usize> = effect
                        .palette
                        .colors
                        .iter()
                        .filter_map(|c| super::brightness_and_bin(&[c.r, c.g, c.b]).1)
                        .collect();
                    bins.sort_unstable();
                    bins.dedup();
                    for bin in bins {
                        weight[bin] += props * overlap as f32 / length as f32;
                    }
                }
            }
            let mut colors: Vec<(usize, f32)> = weight
                .iter()
                .enumerate()
                .filter(|(_, w)| **w >= A_COLOR)
                .map(|(bin, w)| (bin, *w))
                .collect();
            if colors.is_empty() {
                continue;
            }
            judged += 1;
            colors.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            if colors.len() > SECTION_COLORS {
                busy.push((colors.len(), colors.iter().map(|c| c.0).collect(), section));
            }
        }
        if judged == 0 {
            return;
        }
        self.criteria.palette = Some(percent(1.0 - busy.len() as f32 / judged as f32));
        busy.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.start.cmp(&b.2.start)));
        for (count, bins, section) in busy.into_iter().take(2) {
            let keep: Vec<String> = bins.iter().take(3).map(|b| bin_color(*b)).collect();
            let what = format!(
                "{} uses {count} colors at once: a section reads best held to two or three.",
                where_(&[&section])
            );
            let call = Call {
                tool: "change_effects",
                input: json!({ "fromMs": section.start, "toMs": section.end, "change": { "colors": keep } }),
            };
            self.fixes.add(
                rank::PALETTE,
                section.start,
                what,
                vec![call],
                Some(
                    "Those are the three it uses most; choose your own (get_guide topic `palettes`).".into(),
                ),
            );
        }
    }

    // ---- changes ----

    /// Something changes where the song does: each new section looks different from the one
    /// before it, or opens with something that shows.
    pub(super) fn changes(&mut self) {
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
        let b = self.beat();
        let (mut boundaries, mut same) = (0, Vec::new());
        for pair in sections.windows(2) {
            let (before, after) = (&pair[0], &pair[1]);
            if before.group == after.group || after.start != before.end {
                continue;
            }
            let last = self.look(before.end.saturating_sub(4 * bar).max(before.start), before.end);
            let first = self.look(after.start, (after.start + 4 * bar).min(after.end));
            // Dark into dark isn't a look to compare (dead air says so).
            if !last.lit && !first.lit {
                continue;
            }
            boundaries += 1;
            let marked = self.emphasis(after.start.saturating_sub(b / 2), after.start + b) >= EMPHASIS;
            if last.lit && first.lit && last.distance(&first) < SAME_LOOK && !marked {
                same.push((before.clone(), after.clone(), first.hue_peak()));
            }
        }
        if boundaries == 0 {
            return;
        }
        self.criteria.changes = Some(percent(1.0 - same.len() as f32 / boundaries as f32));
        for (before, after, hue) in same.into_iter().take(3) {
            if self.looked.contains(&after.start) {
                continue;
            }
            let what = format!(
                "Nothing changes at {} ({}): it looks the same as the {} before it. Change something at every new section: colors, effects, or which props play.",
                after.name,
                format_ms(after.start),
                before.name
            );
            let colors = super::contrasting(hue);
            let cues = vec![
                json!({ "cue": "sweep", "at": after.start }),
                json!({ "cue": "color_shift", "at": after.start, "until": after.end, "colors": colors }),
            ];
            self.fixes.cues(rank::CHANGE, after.start, what, cues);
        }
    }

    // ---- scale ----

    /// The song's biggest moments light most of the house.
    pub(super) fn scale(&mut self) {
        let f = &self.frames;
        let duration = self.subject.doc.duration_ms;
        let b = self.beat();
        if f.props.len() < 3 {
            return;
        }
        let big: Vec<(usize, pf_analysis::Moment)> = self
            .song
            .moments
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                lands(m.kind)
                    && !matches!(
                        m.kind,
                        pf_analysis::MomentKind::Build | pf_analysis::MomentKind::Peak
                    )
                    && m.importance >= BIG_IMPORTANCE
                    && m.time_ms < duration
            })
            .take(3)
            .map(|(i, m)| (i, m.clone()))
            .collect();
        if big.is_empty() {
            return;
        }
        let mut got = 0.0;
        let mut small = Vec::new();
        for (i, m) in &big {
            let span = f.span(m.time_ms.saturating_sub(b / 4), m.time_ms + b);
            let most = span
                .map(|k| (0..f.props.len()).filter(|&p| f.prop(k, p) > PROP_LIT).count())
                .max()
                .unwrap_or(0);
            let share = most as f32 / f.props.len() as f32;
            got += (share / BIG_MOMENT_PROPS).min(1.0);
            if share < BIG_MOMENT_PROPS {
                small.push((*i, m.clone(), most));
            }
        }
        self.criteria.scale = Some(percent(got / big.len() as f32));
        for (i, m, most) in small {
            let what = format!(
                "The {} (m{i}), one of the song's biggest moments, lights only {most} of the {} props: the big moments want the whole house.",
                kind_name(m.kind),
                f.props.len()
            );
            self.fixes.cue(
                rank::SCALE,
                m.time_ms,
                what,
                json!({ "cue": "hit", "at": format!("m{i}"), "intensity": 1.0, "targets": ["all"] }),
            );
        }
    }

    // ---- fit ----

    /// Effects suit the props they're on, and words can be read.
    pub(super) fn fit(&mut self) {
        let doc = self.subject.doc;
        let show = self.subject.show;
        let pixels: usize = show.props.iter().map(|p| p.node_count() as usize).sum();
        // (what's wrong, the prop, the row, the kind) → the effects, as (start, end).
        let mut misfits: BTreeMap<Misfit, Vec<(u64, u64)>> = BTreeMap::new();
        let mut unreadable: Vec<Unreadable> = Vec::new();
        let mut flickers: Vec<Flicker> = Vec::new();
        let (mut all_ms, mut misfit_ms, mut texts, mut bad_texts) = (0u64, 0u64, 0usize, 0usize);
        for row in &doc.rows {
            let prop = match row.target {
                Target::Prop(id) => show.props.iter().find(|p| p.id == id),
                _ => None,
            };
            let lights: usize = self
                .covers(row.target)
                .iter()
                .filter_map(|id| show.props.iter().find(|p| p.id == *id))
                .map(|p| p.node_count() as usize)
                .sum();
            for effect in row.layers.iter().flat_map(|l| &l.effects) {
                let kind = effect.kind();
                let length = effect.duration_ms();
                all_ms += length;
                if let Some((hz, share)) = flicker(effect, doc.frame_ms)
                    && hz > FLICKER_HZ
                    && length >= 1_000
                    && lights as f32 * share >= LARGE_AREA * pixels.max(1) as f32
                {
                    let name = crate::diff::target_name(show, row.target);
                    flickers.push(Flicker {
                        from: effect.start_ms,
                        to: effect.end_ms,
                        hz,
                        target: name.trim_start_matches("group ").to_string(),
                        id: effect.id.to_string(),
                        kind,
                    });
                }
                let Some(prop) = prop else {
                    continue;
                };
                let on = canvas(prop);
                let single_look = effect.render_style == pf_model::RenderStyle::Default;
                let mut problem = None;
                if kind == EffectKind::Faces
                    && !prop.regions.iter().any(|r| matches!(r.kind, RegionKind::Face(_)))
                {
                    problem = Some("has no face, so a Faces effect has nothing to sing with; the sing effect sings on any prop".to_string());
                } else if on == Canvas::Run && single_look && suits_a_run(kind) == Some(false) {
                    problem = Some(format!(
                        "is a single run of pixels, one row tall, so {} shows only a slice of itself there (its guide has looks for: {})",
                        kind.label(),
                        suited_to(kind)
                    ));
                }
                if problem.is_some() {
                    misfit_ms += length;
                }
                // Words on a run of pixels are said once, by the check on words below.
                if let Some(problem) = problem.filter(|_| kind != EffectKind::Text) {
                    misfits
                        .entry((
                            problem,
                            prop.name.clone(),
                            row.id.to_string(),
                            crate::knowledge::reference::key_of(&kind),
                        ))
                        .or_default()
                        .push((effect.start_ms, effect.end_ms));
                }
                if let EffectParams::Text(text) = &effect.params {
                    texts += 1;
                    if let Some(why) = text_problem(text, effect, on) {
                        bad_texts += 1;
                        unreadable.push(Unreadable {
                            at: effect.start_ms,
                            prop: prop.name.clone(),
                            why,
                            id: effect.id.to_string(),
                            settings: match on {
                                Canvas::Grid { columns, rows } => {
                                    super::text::readable(text, effect, columns, rows)
                                }
                                _ => None,
                            },
                        });
                    }
                }
            }
        }
        if all_ms > 0 {
            self.criteria.fit = percent(1.0 - misfit_ms as f32 / all_ms as f32);
        }
        if texts > 0 {
            self.criteria.text = Some(percent(1.0 - bad_texts as f32 / texts as f32));
        }
        let mut worst: Vec<_> = misfits.into_iter().collect();
        worst.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.1[0].0.cmp(&b.1[0].0)));
        for ((problem, prop, row, kind), times) in worst.into_iter().take(3) {
            let first = times.iter().map(|t| t.0).min().unwrap_or(0);
            let what = if times.len() == 1 {
                format!("{prop} {problem} (at {}).", format_ms(first))
            } else {
                format!(
                    "{prop} {problem} ({} effects, from {}).",
                    times.len(),
                    format_ms(first)
                )
            };
            let hint = format!(
                "Give {prop} an effect made for it instead, or move these to a prop they suit: change_effects with {{\"rowIds\": [\"{row}\"], \"kinds\": [\"{kind}\"]}} and moveTo or remove."
            );
            self.fixes.add(rank::FIT, first, what, Vec::new(), Some(hint));
        }
        unreadable.sort_by(|a, b| (a.at, &a.prop).cmp(&(b.at, &b.prop)));
        for words in unreadable.into_iter().take(3) {
            let what = format!(
                "The words on {} at {} can't be read: {}",
                words.prop,
                format_ms(words.at),
                words.why
            );
            // The settings that make them readable, when settings can; otherwise it's a choice.
            let (calls, hint) = match words.settings {
                Some(settings) => (
                    vec![Call {
                        tool: "change_effects",
                        input: json!({ "ids": [words.id], "change": { "settings": settings } }),
                    }],
                    None,
                ),
                None => (
                    Vec::new(),
                    Some("get_guide effects: [\"text\"] has what fits.".to_string()),
                ),
            };
            self.fixes.add(rank::TEXT, words.at, what, calls, hint);
        }
        flickers.sort_by_key(|f| f.from);
        if let Some(first) = flickers.first() {
            self.safe = false;
            self.criteria.safety = 0;
            let more = match flickers.len() - 1 {
                0 => String::new(),
                1 => ", and 1 more stretch like it".to_string(),
                n => format!(", and {n} more stretches like it"),
            };
            let what = format!(
                "{} flickers about {:.0} times a second over a large part of the display ({}–{}{more}): over 3 a second can trigger seizures in people sensitive to flashing.",
                first.target,
                first.hz,
                format_ms(first.from),
                format_ms(first.to)
            );
            // One call for each kind of effect doing it: an On's shimmer off, the others slowed.
            let mut kinds: Vec<EffectKind> = Vec::new();
            for flicker in &flickers {
                if !kinds.contains(&flicker.kind) {
                    kinds.push(flicker.kind);
                }
            }
            let calls = kinds
                .into_iter()
                .map(|kind| {
                    let these: Vec<&Flicker> = flickers.iter().filter(|f| f.kind == kind).collect();
                    let settings = if kind == EffectKind::On {
                        json!({ "shimmer": false })
                    } else {
                        json!({ "rate": 2 })
                    };
                    let mut input = if these.len() <= IDS_IN_A_FIX {
                        json!({ "ids": these.iter().map(|f| f.id.as_str()).collect::<Vec<_>>() })
                    } else {
                        let starts = these.iter().map(|f| f.from);
                        json!({
                            "kinds": [crate::knowledge::reference::key_of(&kind)],
                            "fromMs": starts.clone().min().unwrap_or(0),
                            "toMs": starts.max().unwrap_or(0) + 1,
                        })
                    };
                    input["change"] = json!({ "settings": settings });
                    Call {
                        tool: "change_effects",
                        input,
                    }
                })
                .collect();
            self.fixes.add(
                rank::SAFETY,
                first.from,
                what,
                calls,
                Some(
                    "That stops it, or slows it to 2 a second. A faster flicker is only for one small prop, for a moment."
                        .into(),
                ),
            );
        }
    }
}

/// Why the words of a Text effect can't be read on what it's drawn on, if they can't.
fn text_problem(text: &pf_sequence::TextParams, effect: &Effect, on: Canvas) -> Option<String> {
    let words = text.text.trim();
    if words.is_empty() {
        return None;
    }
    match on {
        Canvas::Run => {
            Some("it is one row of pixels, and letters need rows to stand on. Put words on a matrix.".into())
        }
        Canvas::Grid { rows, .. } if rows < super::text::SMALLEST_LETTERS => Some(format!(
            "it is {rows} pixels tall, and the smallest letters need {}.",
            super::text::SMALLEST_LETTERS
        )),
        Canvas::Grid { columns, rows } => super::text::problem(text, effect, columns, rows),
        Canvas::Other => None,
    }
}
