//! What a draft changes, worked out by comparing the show (and sequence) before and after, so the
//! proposal card shows exactly what applying would do (an edit undone by a later one shows
//! nothing; a prop changed twice shows once).

use pf_model::Show;
use pf_sequence::{Effect, EffectId, Row, Sequence, Target, format_ms};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

/// Which part of the show a change is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Section {
    Show,
    Prop,
    Group,
    Controller,
    Playlist,
    Sequence,
    Row,
    Effect,
    TimingTrack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Added,
    Removed,
    Changed,
}

/// One line of the proposal card.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub section: Section,
    pub action: Action,
    /// What it is, in words ("Roofline", "Twinkle on Mega Tree at 0:30.000–0:45.000").
    pub name: String,
    /// The item's id, when it has one.
    pub id: Option<String>,
    /// For changed items: what changed ("name: "A" → "B"").
    pub details: Vec<String>,
}

/// Every change, show first, then the open sequence.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub changes: Vec<Change>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Props added or changed (to highlight in the preview).
    pub fn touched_props(&self) -> Vec<String> {
        self.changes
            .iter()
            .filter(|c| c.section == Section::Prop && c.action != Action::Removed)
            .filter_map(|c| c.id.clone())
            .collect()
    }

    /// A short plain-text list, for the model to check its own draft.
    pub fn describe(&self) -> String {
        if self.changes.is_empty() {
            return "The draft has no changes yet.".to_string();
        }
        let mut out = String::new();
        for change in &self.changes {
            let action = match change.action {
                Action::Added => "Add",
                Action::Removed => "Remove",
                Action::Changed => "Change",
            };
            out.push_str(&format!("- {action} {:?}: {}", change.section, change.name));
            if !change.details.is_empty() {
                out.push_str(&format!(" ({})", change.details.join("; ")));
            }
            out.push('\n');
        }
        out
    }
}

/// Details listed per changed item before "and N more".
const MAX_DETAILS: usize = 6;

/// Compares two shows (and, when both are given, two versions of the open sequence).
pub fn diff(before: &Show, after: &Show, sequences: Option<(&Sequence, &Sequence)>) -> Diff {
    let mut changes = Vec::new();
    show_settings(before, after, &mut changes);
    keyed(
        &before.props,
        &after.props,
        |p| p.id.to_string(),
        |p| p.name.clone(),
        Section::Prop,
        &mut changes,
    );
    keyed(
        &before.groups,
        &after.groups,
        |g| g.id.to_string(),
        |g| g.name.clone(),
        Section::Group,
        &mut changes,
    );
    keyed(
        &before.controllers,
        &after.controllers,
        |c| c.id.to_string(),
        |c| c.name.clone(),
        Section::Controller,
        &mut changes,
    );
    keyed(
        &before.sequences,
        &after.sequences,
        |s| s.id.to_string(),
        |s| s.name.clone(),
        Section::Playlist,
        &mut changes,
    );
    let order = |show: &Show| show.sequences.iter().map(|s| s.id).collect::<Vec<_>>();
    let same_set =
        order(before).iter().collect::<BTreeSet<_>>() == order(after).iter().collect::<BTreeSet<_>>();
    if same_set && order(before) != order(after) {
        changes.push(Change {
            section: Section::Playlist,
            action: Action::Changed,
            name: "Playlist order".into(),
            id: None,
            details: vec![
                after
                    .sequences
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            ],
        });
    }
    if let Some((old, new)) = sequences {
        sequence(old, new, after, &mut changes);
    }
    Diff { changes }
}

fn show_settings(before: &Show, after: &Show, out: &mut Vec<Change>) {
    let mut details = Vec::new();
    if before.name != after.name {
        details.push(format!("name: \"{}\" → \"{}\"", before.name, after.name));
    }
    let (a, b) = (json(&before.settings), json(&after.settings));
    field_changes(&a, &b, &mut Vec::new(), &mut details, 0);
    for (label, x, y) in [
        (
            "background photo",
            json(&before.background),
            json(&after.background),
        ),
        ("house model", json(&before.house_model), json(&after.house_model)),
    ] {
        match (x.is_null(), y.is_null()) {
            (true, false) => details.push(format!("{label}: added")),
            (false, true) => details.push(format!("{label}: removed")),
            (false, false) if x != y => {
                let mut inner = Vec::new();
                field_changes(&x, &y, &mut vec![label.to_string()], &mut inner, 0);
                details.extend(inner);
            }
            _ => {}
        }
    }
    if !details.is_empty() {
        out.push(Change {
            section: Section::Show,
            action: Action::Changed,
            name: after.name.clone(),
            id: None,
            details: trim(details),
        });
    }
}

fn json<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// Items matched by id: added, removed, or changed (with what changed).
fn keyed<T: Serialize>(
    before: &[T],
    after: &[T],
    id: impl Fn(&T) -> String,
    name: impl Fn(&T) -> String,
    section: Section,
    out: &mut Vec<Change>,
) {
    let old: HashMap<String, &T> = before.iter().map(|item| (id(item), item)).collect();
    let new: HashMap<String, &T> = after.iter().map(|item| (id(item), item)).collect();
    for item in after {
        let key = id(item);
        match old.get(&key) {
            None => out.push(Change {
                section,
                action: Action::Added,
                name: name(item),
                id: Some(key),
                details: Vec::new(),
            }),
            Some(previous) => {
                let (a, b) = (json(*previous), json(item));
                if a != b {
                    let mut details = Vec::new();
                    field_changes(&a, &b, &mut Vec::new(), &mut details, 0);
                    out.push(Change {
                        section,
                        action: Action::Changed,
                        name: name(item),
                        id: Some(key),
                        details: trim(details),
                    });
                }
            }
        }
    }
    for item in before {
        let key = id(item);
        if !new.contains_key(&key) {
            out.push(Change {
                section,
                action: Action::Removed,
                name: name(item),
                id: Some(key),
                details: Vec::new(),
            });
        }
    }
}

fn trim(mut details: Vec<String>) -> Vec<String> {
    if details.len() > MAX_DETAILS {
        let more = details.len() - (MAX_DETAILS - 1);
        details.truncate(MAX_DETAILS - 1);
        details.push(format!("and {more} more"));
    }
    details
}

/// "colorOrder" → "color order".
fn words(key: &str) -> String {
    let mut out = String::new();
    for (i, c) in key.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push(' ');
            out.push(c.to_ascii_lowercase());
        } else if c == '_' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

fn show_value(value: &Value) -> String {
    match value {
        Value::Null => "none".into(),
        Value::String(s) => format!("\"{s}\""),
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() => {
                let rounded = (f * 1000.0).round() / 1000.0;
                format!("{rounded}")
            }
            _ => n.to_string(),
        },
        Value::Bool(b) => if *b { "on" } else { "off" }.into(),
        Value::Array(items) => format!("{} items", items.len()),
        Value::Object(_) => "set".into(),
    }
}

/// Lists what differs between two JSON values, as "path: old → new" (a few levels deep; lists
/// are summarized by size).
fn field_changes(before: &Value, after: &Value, path: &mut Vec<String>, out: &mut Vec<String>, depth: usize) {
    if before == after {
        return;
    }
    let label = || {
        if path.is_empty() {
            "value".to_string()
        } else {
            path.join(" ")
        }
    };
    match (before, after) {
        (Value::Object(a), Value::Object(b)) if depth < 4 => {
            // A different kind of thing (another shape type, say): one line, not every field.
            if let (Some(x), Some(y)) = (kind_tag(a), kind_tag(b))
                && x != y
            {
                out.push(format!("{}: {x} → {y}", label()));
                return;
            }
            let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            for key in keys {
                if key == "id" {
                    continue;
                }
                let x = a.get(key).unwrap_or(&Value::Null);
                let y = b.get(key).unwrap_or(&Value::Null);
                path.push(words(key));
                field_changes(x, y, path, out, depth + 1);
                path.pop();
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                out.push(format!("{}: {} → {} items", label(), a.len(), b.len()));
            } else {
                out.push(format!("{} changed", label()));
            }
        }
        (Value::Object(_), Value::Object(_)) => out.push(format!("{} changed", label())),
        _ => out.push(format!(
            "{}: {} → {}",
            label(),
            show_value(before),
            show_value(after)
        )),
    }
}

/// The tag of an internally tagged value (`type`, `kind`, or `source`).
fn kind_tag(object: &serde_json::Map<String, Value>) -> Option<String> {
    ["type", "kind", "source"]
        .iter()
        .find_map(|tag| object.get(*tag).and_then(Value::as_str).map(words))
}

/// What a row lights, by name.
pub fn target_name(show: &Show, target: Target) -> String {
    match target {
        Target::Prop(id) => show
            .props
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "a missing prop".into()),
        Target::Group(id) => show
            .groups
            .iter()
            .find(|g| g.id == id)
            .map(|g| format!("group {}", g.name))
            .unwrap_or_else(|| "a missing group".into()),
        Target::Region { prop, region } => {
            let Some(p) = show.props.iter().find(|p| p.id == prop) else {
                return "a missing prop".into();
            };
            match p.region(region) {
                Some(r) => format!("{} › {}", p.name, r.name),
                None => format!("{} › a missing submodel", p.name),
            }
        }
    }
}

/// An effect, in words.
pub fn effect_name(effect: &Effect, row: &Row, show: &Show) -> String {
    format!(
        "{} on {} at {}–{}",
        effect.params.kind().label(),
        target_name(show, row.target),
        format_ms(effect.start_ms),
        format_ms(effect.end_ms)
    )
}

fn effects_by_id(doc: &Sequence) -> HashMap<EffectId, (&Row, usize, &Effect)> {
    let mut out = HashMap::new();
    for row in &doc.rows {
        for (layer, l) in row.layers.iter().enumerate() {
            for effect in &l.effects {
                out.insert(effect.id, (row, layer, effect));
            }
        }
    }
    out
}

fn sequence(before: &Sequence, after: &Sequence, show: &Show, out: &mut Vec<Change>) {
    let mut details = Vec::new();
    if before.name != after.name {
        details.push(format!("name: \"{}\" → \"{}\"", before.name, after.name));
    }
    if before.duration_ms != after.duration_ms {
        details.push(format!(
            "length: {} → {}",
            format_ms(before.duration_ms),
            format_ms(after.duration_ms)
        ));
    }
    if before.audio != after.audio {
        details.push(format!(
            "music: {} → {}",
            show_value(&json(&before.audio)),
            show_value(&json(&after.audio))
        ));
    }
    if before.frame_ms != after.frame_ms {
        details.push(format!(
            "frame time: {} ms → {} ms",
            before.frame_ms, after.frame_ms
        ));
    }
    if !details.is_empty() {
        out.push(Change {
            section: Section::Sequence,
            action: Action::Changed,
            name: after.name.clone(),
            id: None,
            details,
        });
    }

    // Rows: added and removed (a row's effects are listed as effects).
    let row_name = |row: &Row| target_name(show, row.target);
    let old_rows: HashMap<_, _> = before.rows.iter().map(|r| (r.id, r)).collect();
    let new_rows: HashMap<_, _> = after.rows.iter().map(|r| (r.id, r)).collect();
    for row in &after.rows {
        match old_rows.get(&row.id) {
            None => out.push(Change {
                section: Section::Row,
                action: Action::Added,
                name: row_name(row),
                id: Some(row.id.to_string()),
                details: Vec::new(),
            }),
            Some(old) if old.target != row.target || old.layers.len() != row.layers.len() => {
                let mut details = Vec::new();
                if old.target != row.target {
                    details.push(format!("lights: {} → {}", row_name(old), row_name(row)));
                }
                if old.layers.len() != row.layers.len() {
                    details.push(format!("layers: {} → {}", old.layers.len(), row.layers.len()));
                }
                out.push(Change {
                    section: Section::Row,
                    action: Action::Changed,
                    name: row_name(row),
                    id: Some(row.id.to_string()),
                    details,
                });
            }
            Some(_) => {}
        }
    }
    for row in &before.rows {
        if !new_rows.contains_key(&row.id) {
            out.push(Change {
                section: Section::Row,
                action: Action::Removed,
                name: row_name(row),
                id: Some(row.id.to_string()),
                details: Vec::new(),
            });
        }
    }
    let row_order = |doc: &Sequence| doc.rows.iter().map(|r| r.id).collect::<Vec<_>>();
    let kept = |doc: &Sequence, other: &HashMap<pf_sequence::RowId, &Row>| {
        row_order(doc)
            .into_iter()
            .filter(|id| other.contains_key(id))
            .collect::<Vec<_>>()
    };
    if kept(before, &new_rows) != kept(after, &old_rows) {
        out.push(Change {
            section: Section::Sequence,
            action: Action::Changed,
            name: "Row order".into(),
            id: None,
            details: Vec::new(),
        });
    }

    // Effects, wherever they are.
    let old_effects = effects_by_id(before);
    let new_effects = effects_by_id(after);
    let mut added: Vec<_> = new_effects
        .iter()
        .filter(|(id, _)| !old_effects.contains_key(id))
        .collect();
    added.sort_by_key(|(_, (row, layer, e))| (row_position(after, row), *layer, e.start_ms));
    for (id, (row, _, effect)) in added {
        out.push(Change {
            section: Section::Effect,
            action: Action::Added,
            name: effect_name(effect, row, show),
            id: Some(id.to_string()),
            details: Vec::new(),
        });
    }
    let mut changed: Vec<_> = new_effects
        .iter()
        .filter_map(|(id, new)| old_effects.get(id).map(|old| (id, old, new)))
        .filter(|(_, old, new)| old.0.id != new.0.id || old.1 != new.1 || old.2 != new.2)
        .collect();
    changed.sort_by_key(|(_, _, (row, layer, e))| (row_position(after, row), *layer, e.start_ms));
    for (id, (old_row, old_layer, old), (row, layer, effect)) in changed {
        let mut details = Vec::new();
        if old_row.id != row.id {
            details.push(format!("moved to {}", row_name(row)));
        } else if old_layer != layer {
            details.push(format!("layer: {} → {}", old_layer + 1, layer + 1));
        }
        if (old.start_ms, old.end_ms) != (effect.start_ms, effect.end_ms) {
            details.push(format!(
                "time: {}–{} → {}–{}",
                format_ms(old.start_ms),
                format_ms(old.end_ms),
                format_ms(effect.start_ms),
                format_ms(effect.end_ms)
            ));
        }
        let (mut a, mut b) = (json(*old), json(*effect));
        for v in [&mut a, &mut b] {
            if let Some(o) = v.as_object_mut() {
                o.remove("startMs");
                o.remove("endMs");
            }
        }
        field_changes(&a, &b, &mut Vec::new(), &mut details, 0);
        out.push(Change {
            section: Section::Effect,
            action: Action::Changed,
            name: effect_name(effect, row, show),
            id: Some(id.to_string()),
            details: trim(details),
        });
    }
    let mut removed: Vec<_> = old_effects
        .iter()
        .filter(|(id, _)| !new_effects.contains_key(id))
        .collect();
    removed.sort_by_key(|(_, (row, layer, e))| (row_position(before, row), *layer, e.start_ms));
    for (id, (row, _, effect)) in removed {
        out.push(Change {
            section: Section::Effect,
            action: Action::Removed,
            name: effect_name(effect, row, show),
            id: Some(id.to_string()),
            details: Vec::new(),
        });
    }

    // Timing tracks.
    let old_tracks: HashMap<_, _> = before.timing_tracks.iter().map(|t| (t.id, t)).collect();
    let new_tracks: HashMap<_, _> = after.timing_tracks.iter().map(|t| (t.id, t)).collect();
    for track in &after.timing_tracks {
        match old_tracks.get(&track.id) {
            None => out.push(Change {
                section: Section::TimingTrack,
                action: Action::Added,
                name: track.name.clone(),
                id: Some(track.id.to_string()),
                details: vec![format!("{} marks", track.marks.len())],
            }),
            Some(old) if *old != track => {
                let mut details = Vec::new();
                if old.name != track.name {
                    details.push(format!("name: \"{}\" → \"{}\"", old.name, track.name));
                }
                if old.marks.len() != track.marks.len() {
                    details.push(format!("marks: {} → {}", old.marks.len(), track.marks.len()));
                } else if old.marks != track.marks {
                    details.push("marks moved or relabeled".into());
                }
                if old.kind != track.kind {
                    details.push("kind changed".into());
                }
                out.push(Change {
                    section: Section::TimingTrack,
                    action: Action::Changed,
                    name: track.name.clone(),
                    id: Some(track.id.to_string()),
                    details,
                });
            }
            Some(_) => {}
        }
    }
    for track in &before.timing_tracks {
        if !new_tracks.contains_key(&track.id) {
            out.push(Change {
                section: Section::TimingTrack,
                action: Action::Removed,
                name: track.name.clone(),
                id: Some(track.id.to_string()),
                details: Vec::new(),
            });
        }
    }
}

fn row_position(doc: &Sequence, row: &Row) -> usize {
    doc.rows.iter().position(|r| r.id == row.id).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::{Generator, Group, Prop, ShapeSource};
    use pf_sequence::{EffectKind, Layer, TimingKind, TimingTrack};

    fn line(name: &str) -> Prop {
        Prop::new(
            name,
            ShapeSource::Generator(Generator::Line {
                nodes: 10,
                length: 1.0,
            }),
        )
    }

    #[test]
    fn nothing_changed_means_an_empty_diff() {
        let mut show = Show::new("Show");
        show.props.push(line("A"));
        assert!(diff(&show, &show.clone(), None).is_empty());
    }

    #[test]
    fn props_are_added_removed_and_changed_by_id() {
        let mut before = Show::new("Show");
        let a = line("A");
        let b = line("B");
        before.props = vec![a.clone(), b.clone()];
        let mut after = before.clone();
        after.props.retain(|p| p.id != b.id);
        after.props[0].name = "Roofline".into();
        let c = line("C");
        after.props.push(c.clone());
        let d = diff(&before, &after, None);
        let summary: Vec<_> = d.changes.iter().map(|c| (c.action, c.name.as_str())).collect();
        assert_eq!(
            summary,
            [
                (Action::Changed, "Roofline"),
                (Action::Added, "C"),
                (Action::Removed, "B")
            ]
        );
        assert_eq!(d.changes[0].details, ["name: \"A\" → \"Roofline\""]);
        assert_eq!(d.touched_props(), [a.id.to_string(), c.id.to_string()]);
    }

    #[test]
    fn a_different_shape_is_one_line_not_every_field() {
        let mut before = Show::new("Show");
        before.props.push(line("A"));
        let mut after = before.clone();
        after.props[0].shape = ShapeSource::Generator(Generator::Arch {
            nodes: 50,
            width: 2.0,
            height: 1.0,
        });
        let d = diff(&before, &after, None);
        assert_eq!(d.changes[0].details, ["shape: line → arch"]);
    }

    #[test]
    fn show_settings_groups_and_playlist_order() {
        let mut before = Show::new("Show");
        before.sequences = vec![
            pf_model::SequenceEntry::new("One", "/a.fseq"),
            pf_model::SequenceEntry::new("Two", "/b.fseq"),
        ];
        let mut after = before.clone();
        after.name = "Christmas".into();
        after.settings.frame_rate = 20;
        after.sequences.reverse();
        after.groups.push(Group::new("Roof"));
        let d = diff(&before, &after, None);
        assert_eq!(d.changes[0].section, Section::Show);
        assert_eq!(
            d.changes[0].details,
            ["name: \"Show\" → \"Christmas\"", "frame rate: 40 → 20"]
        );
        assert_eq!(d.changes[1].section, Section::Group);
        assert_eq!(d.changes[2].name, "Playlist order");
        assert_eq!(d.changes[2].details, ["Two, One"]);
    }

    #[test]
    fn sequence_effects_rows_and_tracks() {
        let mut show = Show::new("Show");
        let tree = line("Tree");
        show.props.push(tree.clone());
        let mut before = Sequence::new("Song", 60_000);
        let mut row = Row::new(Target::Prop(tree.id));
        let on = Effect::new(EffectKind::On, 0, 1000);
        row.layers = vec![Layer {
            effects: vec![on.clone()],
        }];
        before.rows.push(row);
        let mut after = before.clone();
        after.rows[0].layers[0].effects[0].end_ms = 2000;
        let added = Effect::new(EffectKind::On, 5000, 6000);
        after.rows[0].layers[0].effects.push(added);
        after
            .timing_tracks
            .push(TimingTrack::new("Beats", TimingKind::Beats, vec![]));
        let d = diff(&show, &show, Some((&before, &after)));
        let lines: Vec<_> = d
            .changes
            .iter()
            .map(|c| (c.section, c.action, c.name.clone()))
            .collect();
        assert_eq!(
            lines,
            [
                (
                    Section::Effect,
                    Action::Added,
                    "On on Tree at 0:05.000–0:06.000".to_string()
                ),
                (
                    Section::Effect,
                    Action::Changed,
                    "On on Tree at 0:00.000–0:02.000".to_string()
                ),
                (Section::TimingTrack, Action::Added, "Beats".to_string()),
            ]
        );
        assert_eq!(
            d.changes[1].details,
            ["time: 0:00.000–0:01.000 → 0:00.000–0:02.000"]
        );
    }
}
