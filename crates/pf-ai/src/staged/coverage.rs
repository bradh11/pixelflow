//! Whether the draft has what the plan says plays where, checked by drawing it: for each part of
//! the song the plan builds, the props it has playing there are drawn every [`SAMPLE_MS`], and a
//! prop lit in less than [`MIN_SHARE`] of the part (leaving out the moments the plan darkens on
//! purpose) is dark there. A part with half of those props dark or more isn't built as the plan
//! has it (a few props joining only for a build are the plan's own business, a part left dark is
//! not): what is missing, with its times, props, and rows, goes back to the step that should have
//! built it, and after the build to the refine.

use crate::draft::Draft;
use crate::provider::Cancel;
use crate::staged::plan::{Plan, World, inside};
use pf_model::PropId;
use pf_sequence::{EffectKind, Target};
use std::collections::{BTreeMap, BTreeSet};

/// How often a part is drawn (ms).
pub const SAMPLE_MS: u64 = 250;
/// A prop lit for less of a part than this (0–1) is missing from it.
pub const MIN_SHARE: f32 = 0.2;
/// A pixel whose brightest channel is this (of 255) is lit: the evaluation's own measure.
const LIT: u8 = 12;
/// Gaps told at once, at most.
const MAX_GAPS: usize = 8;

/// What the plan has playing in one part of the song.
#[derive(Debug, Clone, PartialEq)]
pub struct Expect {
    /// The part's name ("Chorus 3").
    pub part: String,
    pub span: (u64, u64),
    pub props: BTreeSet<PropId>,
    /// Stretches the plan darkens on purpose (its own moments), each with the props it darkens.
    pub excused: Vec<((u64, u64), BTreeSet<PropId>)>,
    /// The plan's look there, in a few words.
    pub look: String,
}

/// A part missing props the plan has playing there.
#[derive(Debug, Clone, PartialEq)]
pub struct Gap {
    pub part: String,
    pub span: (u64, u64),
    /// The props dark there, with the share of the part each is lit.
    pub dark: Vec<(PropId, f32)>,
    pub look: String,
    /// Off effects (blackouts) over the part on rows that light those props: id, start, end.
    pub offs: Vec<(String, u64, u64)>,
}

/// The stretches the plan's moments darken on purpose, inside `span`.
fn excused(plan: &Plan, world: &World, span: (u64, u64)) -> Vec<((u64, u64), BTreeSet<PropId>)> {
    plan.moments
        .iter()
        .map(|m| (m.window(world.bar_ms), m))
        .filter(|((a, b), _)| *a < span.1 && span.0 < *b)
        .map(|(window, m)| {
            let props = if m.lights.is_empty() {
                world.all_props().clone()
            } else {
                m.lights.clone()
            };
            (window, props)
        })
        .collect()
}

/// What the plan has playing in each part of the song inside `scope`.
pub fn planned(plan: &Plan, world: &World, scope: &[(u64, u64)]) -> Vec<Expect> {
    world
        .occurrences()
        .into_iter()
        .filter(|(_, span)| inside(scope, *span))
        .filter_map(|(part, span)| {
            let section = plan.section_at(span).filter(|s| s.plays())?;
            Some(Expect {
                part,
                span,
                props: section.lights.clone(),
                excused: excused(plan, world, span),
                look: section.look.clone(),
            })
        })
        .collect()
}

/// What step `index` of `plan` is to build: the props the plan has playing in each part of its
/// own, among those it lights, and not left to a step for fewer props there (the singing on the
/// face, the words on the matrix). A step over the whole song, or one with only cues, lays no
/// part's look and has none.
pub fn for_step(plan: &Plan, world: &World, index: usize) -> Vec<Expect> {
    let step = &plan.steps[index];
    if step.whole_song() || !step.lays_looks() {
        return Vec::new();
    }
    planned(plan, world, &step.spans)
        .into_iter()
        .filter_map(|mut expect| {
            expect.props.retain(|p| step.lights.contains(p));
            for (j, other) in plan.steps.iter().enumerate() {
                let meets = other
                    .spans
                    .iter()
                    .any(|(a, b)| *a < expect.span.1 && expect.span.0 < *b);
                if j != index && meets && other.lays_looks() && other.lights.len() < step.lights.len() {
                    expect.props.retain(|p| !other.lights.contains(p));
                }
            }
            (!expect.props.is_empty()).then_some(expect)
        })
        .collect()
}

/// The parts of `expects` missing props, drawn from `draft`. Empty when stopped.
pub fn check(draft: &Draft, expects: &[Expect], cancel: &Cancel) -> Vec<Gap> {
    let Some(doc) = draft.sequence() else {
        return Vec::new();
    };
    if expects.is_empty() {
        return Vec::new();
    }
    let show = draft.show();
    let layout: Vec<(PropId, usize, usize, usize)> = pf_engine::preview_props_of(show)
        .into_iter()
        .map(|p| {
            (
                p.prop,
                p.frame_offset,
                usize::from(p.channels_per_pixel.max(1)),
                p.points.len() / 2,
            )
        })
        .collect();
    let mut renderer = pf_engine::DraftRenderer::new(show);
    renderer.set_audio(draft.base().audio.clone());
    renderer.set_pictures(draft.base().pictures.waiting());
    // Each time drawn once, whichever parts want it: the props lit then.
    let mut drawn: BTreeMap<u64, BTreeSet<PropId>> = BTreeMap::new();
    for expect in expects {
        let mut t = expect.span.0 + SAMPLE_MS / 2;
        while t < expect.span.1 {
            if cancel.is_cancelled() {
                return Vec::new();
            }
            drawn.entry(t).or_insert_with(|| {
                let frame = renderer.frame(doc, t);
                layout
                    .iter()
                    .filter(|(_, offset, step, count)| {
                        (0..*count).any(|pixel| {
                            let at = offset + pixel * step;
                            frame
                                .get(at..at + (*step).min(3))
                                .and_then(|c| c.iter().copied().max())
                                .unwrap_or(0)
                                >= LIT
                        })
                    })
                    .map(|(prop, ..)| *prop)
                    .collect()
            });
            t += SAMPLE_MS;
        }
    }
    let mut gaps = Vec::new();
    for expect in expects {
        let mut dark = Vec::new();
        for &prop in &expect.props {
            let samples: Vec<&BTreeSet<PropId>> = drawn
                .range(expect.span.0..expect.span.1)
                .filter(|(t, _)| {
                    !expect
                        .excused
                        .iter()
                        .any(|((a, b), props)| (*a..*b).contains(*t) && props.contains(&prop))
                })
                .map(|(_, lit)| lit)
                .collect();
            if samples.is_empty() {
                continue;
            }
            let share =
                samples.iter().filter(|lit| lit.contains(&prop)).count() as f32 / samples.len() as f32;
            if share < MIN_SHARE {
                dark.push((prop, share));
            }
        }
        if dark.is_empty() || dark.len() * 2 < expect.props.len() {
            continue;
        }
        let offs = offs_over(draft, expect.span, &dark);
        gaps.push(Gap {
            part: expect.part.clone(),
            span: expect.span,
            dark,
            look: expect.look.clone(),
            offs,
        });
    }
    gaps
}

/// The Off effects over `span` on rows that light any of `dark`.
fn offs_over(draft: &Draft, span: (u64, u64), dark: &[(PropId, f32)]) -> Vec<(String, u64, u64)> {
    let Some(doc) = draft.sequence() else {
        return Vec::new();
    };
    let show = draft.show();
    let lights = |target: Target| -> Vec<PropId> {
        match target {
            Target::Prop(id) | Target::Region { prop: id, .. } => vec![id],
            Target::Group(id) => show
                .groups
                .iter()
                .find(|g| g.id == id)
                .map(|g| g.members.iter().map(|m| m.prop()).collect())
                .unwrap_or_default(),
        }
    };
    let mut offs: Vec<(String, u64, u64)> = Vec::new();
    for row in &doc.rows {
        if !lights(row.target)
            .iter()
            .any(|p| dark.iter().any(|(d, _)| d == p))
        {
            continue;
        }
        for effect in row.layers.iter().flat_map(|l| &l.effects) {
            let long = effect
                .end_ms
                .min(span.1)
                .saturating_sub(effect.start_ms.max(span.0));
            if effect.kind() == EffectKind::Off && long > SAMPLE_MS && offs.len() < 6 {
                offs.push((effect.id.to_string(), effect.start_ms, effect.end_ms));
            }
        }
    }
    offs
}

/// The rows of `draft` that light `prop` by itself (its own rows, not its groups').
fn own_rows(draft: &Draft, prop: PropId) -> Vec<String> {
    draft
        .sequence()
        .map(|doc| {
            doc.rows
                .iter()
                .filter(|r| matches!(r.target, Target::Prop(id) if id == prop))
                .map(|r| r.id.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The gaps as lines the model reads: each part, its times, the props dark there with their
/// rows, the plan's look, and any blackout darkening them.
pub fn describe(gaps: &[Gap], world: &World, draft: &Draft) -> String {
    let mut lines: Vec<String> = gaps
        .iter()
        .take(MAX_GAPS)
        .map(|gap| {
            let props: Vec<String> = gap
                .dark
                .iter()
                .map(|(prop, _)| {
                    let rows = own_rows(draft, *prop);
                    if rows.is_empty() {
                        world.prop_name(*prop).to_string()
                    } else {
                        format!("{} (row {})", world.prop_name(*prop), rows.join(", "))
                    }
                })
                .collect();
            let mut line = format!(
                "- {} ({}–{} ms): nothing lights {}",
                gap.part,
                gap.span.0,
                gap.span.1,
                props.join(", ")
            );
            if !gap.look.is_empty() {
                line.push_str(&format!("; the plan has: {}", gap.look));
            }
            if !gap.offs.is_empty() {
                let offs: Vec<String> = gap
                    .offs
                    .iter()
                    .map(|(id, a, b)| format!("{id} ({a}–{b})"))
                    .collect();
                line.push_str(&format!(
                    "; off effects darken them there (remove or shorten them outside the moment): {}",
                    offs.join(", ")
                ));
            }
            line.push('.');
            line
        })
        .collect();
    if gaps.len() > MAX_GAPS {
        lines.push(format!("- …and {} more parts.", gaps.len() - MAX_GAPS));
    }
    lines.join("\n")
}

/// The gaps in a sentence the person reads (no ids).
pub fn sentence(gaps: &[Gap], world: &World) -> String {
    let parts: Vec<String> = gaps
        .iter()
        .take(4)
        .map(|gap| {
            let props: Vec<&str> = gap.dark.iter().map(|(p, _)| world.prop_name(*p)).collect();
            format!("{} ({})", gap.part, props.join(", "))
        })
        .collect();
    format!(
        "Some of the plan is still dark: {}{}.",
        parts.join("; "),
        if gaps.len() > 4 { "; and more" } else { "" }
    )
}
