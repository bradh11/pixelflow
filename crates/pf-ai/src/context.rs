//! What the Assistant is given with the user's message, before it asks: the props with their
//! roles and grids, the groups, the open sequence's rows and timing tracks in outline, and the
//! song's analysis. Nearly every job starts by looking these up, one round trip each; given up
//! front they cost none. Long lists are cut, saying how many more there are and which tool lists
//! them all, so a large show stays small.
//!
//! It all rides in the user's message, after the standing instructions and the tool definitions,
//! which never change: a provider's cache keeps those warm across chats, and the conversation's
//! own prefix (this block included) across the requests of a chat.

use crate::diff::target_name;
use crate::draft::Draft;
use crate::knowledge::reference;
use pf_analysis::Analysis;
use pf_model::{Prop, Show};
use pf_sequence::{EffectKind, Sequence};
use serde_json::{Value, json};

/// Props listed, at most (the rest are counted, for `list_props`).
pub const MAX_PROPS: usize = 80;
/// Sequence rows listed, at most (the rest are counted, for `get_open_sequence`).
pub const MAX_ROWS: usize = 80;
/// Members named for each group, at most.
const MAX_MEMBERS: usize = 12;
/// Timing tracks listed, at most.
const MAX_TRACKS: usize = 30;
/// Pictures named, at most.
const MAX_PICTURES: usize = 30;

/// How the song's line starts: what a scenario's check looks for.
pub const SONG_LINE: &str = "Song (as analyze_song answers it):";

/// The most the context may weigh for the made-up stage show and song (bytes): a test holds it.
pub const STAGE_CONTEXT_BUDGET_BYTES: usize = 9_000;

/// A value as JSON, with `<` and `>` escaped so nothing in it (a name, a lyric) can close the
/// context block it sits in.
pub fn data(value: &Value) -> String {
    value.to_string().replace('<', "\\u003c").replace('>', "\\u003e")
}

/// A number of layout units to one decimal place, as it reads in JSON.
fn round1(x: f32) -> f64 {
    (f64::from(x) * 10.0).round() / 10.0
}

/// A prop as one line: name, role, shape, grid (columns x rows, or null), pixels, where it sits
/// across and up, and the faces set up on it.
fn prop_line(prop: &Prop) -> String {
    let p = prop.transform.position;
    let faces: Vec<&str> = prop
        .regions
        .iter()
        .filter(|r| matches!(r.kind, pf_model::RegionKind::Face(_)))
        .map(|r| r.name.as_str())
        .collect();
    data(&json!([
        prop.name,
        crate::run::role_name(prop),
        crate::run::shape_type(prop),
        crate::run::grid(prop).map(|(columns, rows)| format!("{columns}x{rows}")),
        prop.node_count(),
        round1(p.x),
        round1(p.y),
        faces,
    ]))
}

fn props(show: &Show, lines: &mut Vec<String>) {
    lines.push(format!(
        "Props ({}), each [name, role, shape, grid, pixels, x, y, faces]:",
        show.props.len()
    ));
    lines.extend(show.props.iter().take(MAX_PROPS).map(prop_line));
    if show.props.len() > MAX_PROPS {
        lines.push(format!(
            "…and {} more props: list_props lists them all.",
            show.props.len() - MAX_PROPS
        ));
    }
}

fn groups(show: &Show, lines: &mut Vec<String>) {
    if show.groups.is_empty() {
        return;
    }
    let listed: Vec<Value> = show
        .groups
        .iter()
        .map(|g| {
            let mut members: Vec<String> = g
                .members
                .iter()
                .take(MAX_MEMBERS)
                .map(|m| crate::run::member_name(show, m))
                .collect();
            if g.members.len() > MAX_MEMBERS {
                members.push(format!("+{} more", g.members.len() - MAX_MEMBERS));
            }
            json!([g.name, members])
        })
        .collect();
    lines.push(format!("Groups, each [name, members]: {}", data(&json!(listed))));
}

/// The open sequence in outline: each row with its id, what it lights, the effects on each
/// layer, and their kinds; the timing tracks; the pictures a picture effect can show.
fn sequence(show: &Show, doc: &Sequence, images: &[String], lines: &mut Vec<String>) {
    let effects: usize = doc
        .rows
        .iter()
        .flat_map(|r| &r.layers)
        .map(|l| l.effects.len())
        .sum();
    lines.push(format!(
        "Rows ({}, {effects} effects), each [rowId, lights, effects on each layer, kinds]:",
        doc.rows.len()
    ));
    for row in doc.rows.iter().take(MAX_ROWS) {
        let mut kinds: Vec<EffectKind> = Vec::new();
        for effect in row.layers.iter().flat_map(|l| &l.effects) {
            if !kinds.contains(&effect.kind()) {
                kinds.push(effect.kind());
            }
        }
        lines.push(data(&json!([
            row.id,
            target_name(show, row.target),
            row.layers.iter().map(|l| l.effects.len()).collect::<Vec<_>>(),
            kinds.iter().map(reference::key_of).collect::<Vec<_>>(),
        ])));
    }
    if doc.rows.len() > MAX_ROWS {
        lines.push(format!(
            "…and {} more rows: get_open_sequence lists them all.",
            doc.rows.len() - MAX_ROWS
        ));
    }
    let tracks: Vec<Value> = doc
        .timing_tracks
        .iter()
        .take(MAX_TRACKS)
        .map(|t| json!([t.name, t.kind, t.marks.len()]))
        .collect();
    lines.push(format!(
        "Timing tracks, each [name, kind, marks]: {}{}",
        data(&json!(tracks)),
        if doc.timing_tracks.len() > MAX_TRACKS {
            format!(
                " …and {} more: get_open_sequence lists them all.",
                doc.timing_tracks.len() - MAX_TRACKS
            )
        } else {
            String::new()
        }
    ));
    if !images.is_empty() {
        let named: Vec<&String> = images.iter().take(MAX_PICTURES).collect();
        lines.push(format!(
            "Pictures (for picture effects): {}{}",
            data(&json!(named)),
            if images.len() > MAX_PICTURES {
                format!(" …and {} more.", images.len() - MAX_PICTURES)
            } else {
                String::new()
            }
        ));
    }
}

/// The show, the open sequence (with the draft's changes), and the song as analyzed, as lines
/// of the context block. `song` is `None` when there is no song or it couldn't be analyzed.
pub fn describe(draft: &Draft, song: Option<&Analysis>) -> Vec<String> {
    let show = draft.show();
    let mut lines = Vec::new();
    props(show, &mut lines);
    groups(show, &mut lines);
    if let Some(doc) = draft.sequence() {
        sequence(show, doc, &draft.base().images, &mut lines);
        if let Some(analysis) = song {
            let user = draft.base().sequence.as_ref().map(|s| &s.doc);
            lines.push(format!(
                "{SONG_LINE} {}",
                data(&crate::song::describe(analysis, user))
            ));
        }
    }
    lines
}
