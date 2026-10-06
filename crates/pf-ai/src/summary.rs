//! A sequence proposal at a glance: what it adds, changes, and removes in each section of the
//! song, and a timeline of the draft (rows of colored effects) to look at before applying.

use crate::diff::target_name;
use pf_model::{Rgb, Show};
use pf_sequence::{Effect, EffectId, Sequence, TimingKind};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};

/// What a proposal does in one section of the song.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionSummary {
    pub label: String,
    pub start_ms: u64,
    pub end_ms: u64,
    /// Rows with an effect added, changed, or removed here.
    pub rows: usize,
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
    /// The kinds of effect added or changed here ("Twinkle"), in order of first use.
    pub kinds: Vec<String>,
}

/// One effect on the timeline.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEffect {
    pub start_ms: u64,
    pub end_ms: u64,
    /// The effect's first color, "#rrggbb".
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineRow {
    pub name: String,
    pub effects: Vec<TimelineEffect>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineSection {
    pub label: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// The draft sequence drawn small: its rows that have effects, in order.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    pub duration_ms: u64,
    pub sections: Vec<TimelineSection>,
    pub rows: Vec<TimelineRow>,
    /// Rows with effects left out to keep it small.
    pub more_rows: usize,
}

/// Rows drawn at most, and effects per row.
const MAX_TIMELINE_ROWS: usize = 48;
const MAX_ROW_EFFECTS: usize = 400;

/// The song's sections: the draft's first Sections track, or the whole sequence as one.
fn sections_of(doc: &Sequence) -> Vec<TimelineSection> {
    let marks = doc
        .timing_tracks
        .iter()
        .find(|t| t.kind == TimingKind::Sections && !t.marks.is_empty())
        .map(|t| &t.marks);
    match marks {
        Some(marks) => marks
            .iter()
            .enumerate()
            .map(|(i, m)| TimelineSection {
                label: if m.label.trim().is_empty() {
                    format!("Section {}", i + 1)
                } else {
                    m.label.clone()
                },
                start_ms: m.start_ms,
                end_ms: m.end_ms,
            })
            .collect(),
        None => vec![TimelineSection {
            label: "Whole sequence".into(),
            start_ms: 0,
            end_ms: doc.duration_ms,
        }],
    }
}

fn hex(color: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

/// Every effect with its row's index, by id.
fn effects(doc: &Sequence) -> HashMap<EffectId, (usize, &Effect)> {
    doc.rows
        .iter()
        .enumerate()
        .flat_map(|(r, row)| {
            row.layers
                .iter()
                .flat_map(move |l| l.effects.iter().map(move |e| (e.id, (r, e))))
        })
        .collect()
}

/// What changed in each section (an effect counts where it starts).
pub fn section_summaries(before: &Sequence, after: &Sequence) -> Vec<SectionSummary> {
    let sections = sections_of(after);
    let old = effects(before);
    let new = effects(after);
    let row_id = |doc: &Sequence, r: usize| doc.rows[r].id;
    let mut out: Vec<SectionSummary> = sections
        .iter()
        .map(|s| SectionSummary {
            label: s.label.clone(),
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            rows: 0,
            added: 0,
            changed: 0,
            removed: 0,
            kinds: Vec::new(),
        })
        .collect();
    let mut rows: Vec<BTreeSet<pf_sequence::RowId>> = vec![BTreeSet::new(); out.len()];
    let section_at = |ms: u64| {
        sections
            .iter()
            .position(|s| ms >= s.start_ms && ms < s.end_ms)
            .or_else(|| (!sections.is_empty()).then(|| sections.len() - 1))
    };
    let mut note = |ms: u64, row, kind: Option<&str>, which: fn(&mut SectionSummary) -> &mut usize| {
        if let Some(i) = section_at(ms) {
            *which(&mut out[i]) += 1;
            rows[i].insert(row);
            if let Some(kind) = kind
                && !out[i].kinds.iter().any(|k| k == kind)
            {
                out[i].kinds.push(kind.to_string());
            }
        }
    };
    // In time order, so each section's kinds read in the order they first appear.
    let mut now: Vec<(&EffectId, &(usize, &Effect))> = new.iter().collect();
    now.sort_by_key(|(id, (r, e))| (e.start_ms, *r, **id));
    for (id, (r, effect)) in now {
        let label = Some(effect.kind().label());
        match old.get(id) {
            None => note(effect.start_ms, row_id(after, *r), label, |s| &mut s.added),
            Some((_, was)) if was != effect => {
                note(effect.start_ms, row_id(after, *r), label, |s| &mut s.changed)
            }
            Some(_) => {}
        }
    }
    for (id, (r, effect)) in &old {
        if !new.contains_key(id) {
            note(effect.start_ms, row_id(before, *r), None, |s| &mut s.removed);
        }
    }
    for (summary, rows) in out.iter_mut().zip(rows) {
        summary.rows = rows.len();
    }
    out
}

/// The draft sequence as a timeline.
pub fn timeline(doc: &Sequence, show: &Show) -> Timeline {
    let lit: Vec<_> = doc
        .rows
        .iter()
        .filter(|row| row.layers.iter().any(|l| !l.effects.is_empty()))
        .collect();
    let rows = lit
        .iter()
        .take(MAX_TIMELINE_ROWS)
        .map(|row| {
            let mut effects: Vec<TimelineEffect> = row
                .layers
                .iter()
                .flat_map(|l| &l.effects)
                .map(|e| TimelineEffect {
                    start_ms: e.start_ms,
                    end_ms: e.end_ms,
                    color: e
                        .palette
                        .colors
                        .first()
                        .map_or_else(|| "#ffffff".into(), |c| hex(*c)),
                })
                .collect();
            effects.sort_by_key(|e| (e.start_ms, e.end_ms));
            effects.truncate(MAX_ROW_EFFECTS);
            TimelineRow {
                name: target_name(show, row.target),
                effects,
            }
        })
        .collect();
    Timeline {
        duration_ms: doc.duration_ms,
        sections: sections_of(doc),
        rows,
        more_rows: lit.len().saturating_sub(MAX_TIMELINE_ROWS),
    }
}
