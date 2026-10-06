//! What a draft changes, worked out by comparing the show (and sequence) before and after, so the
//! proposal card shows exactly what applying would do (an edit undone by a later one shows
//! nothing; a prop changed twice shows once).

use pf_model::{Controller, Group, GroupMember, Prop, Protocol, SequenceEntry, Show};
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
    /// What it is (for added items) or what changed ("name: "A" → "B"), in full.
    pub details: Vec<String>,
    /// What deserves a careful look: where light data will be sent, files the assistant chose.
    pub warnings: Vec<String>,
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

/// Compares two shows (and, when both are given, two versions of the open sequence).
pub fn diff(before: &Show, after: &Show, sequences: Option<(&Sequence, &Sequence)>) -> Diff {
    let mut changes = Vec::new();
    show_settings(before, after, &mut changes);
    keyed(
        &before.props,
        &after.props,
        Keys {
            id: |p: &Prop| p.id.to_string(),
            name: |p: &Prop| p.name.clone(),
            lines: None::<fn(&Prop) -> Vec<String>>,
            added: Some(prop_lines),
            warn: no_warnings,
        },
        Section::Prop,
        &mut changes,
    );
    let group_lines = |g: &Group| vec![format!("members: {}", member_names(after, g))];
    keyed(
        &before.groups,
        &after.groups,
        Keys {
            id: |g: &Group| g.id.to_string(),
            name: |g: &Group| g.name.clone(),
            lines: Some(group_lines),
            added: Some(group_lines),
            warn: no_warnings,
        },
        Section::Group,
        &mut changes,
    );
    let known: BTreeSet<&str> = before.controllers.iter().map(|c| c.address.as_str()).collect();
    let controller_lines = |c: &Controller| controller_lines(c, after);
    keyed(
        &before.controllers,
        &after.controllers,
        Keys {
            id: |c: &Controller| c.id.to_string(),
            name: |c: &Controller| c.name.clone(),
            lines: Some(controller_lines),
            added: Some(controller_lines),
            warn: |old: Option<&Controller>, new: &Controller| controller_warnings(old, new, &known),
        },
        Section::Controller,
        &mut changes,
    );
    keyed(
        &before.sequences,
        &after.sequences,
        Keys {
            id: |s: &SequenceEntry| s.id.to_string(),
            name: |s: &SequenceEntry| s.name.clone(),
            lines: None::<fn(&SequenceEntry) -> Vec<String>>,
            added: Some(|s: &SequenceEntry| {
                vec![
                    format!("file: {}", s.path),
                    format!("music: {}", s.audio.as_deref().unwrap_or("none")),
                    format!("offset: {} ms", s.offset_ms),
                ]
            }),
            warn: |old: Option<&SequenceEntry>, new: &SequenceEntry| {
                let mut out = Vec::new();
                if old.is_none_or(|o| o.path != new.path) {
                    out.push(chosen_file(&new.path));
                }
                if let Some(audio) = &new.audio
                    && old.is_none_or(|o| o.audio.as_ref() != Some(audio))
                {
                    out.push(chosen_file(audio));
                }
                out
            },
        },
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
            warnings: Vec::new(),
        });
    }
    if let Some((old, new)) = sequences {
        sequence(old, new, after, &mut changes);
    }
    Diff { changes }
}

fn no_warnings<T>(_: Option<&T>, _: &T) -> Vec<String> {
    Vec::new()
}

/// A warning for a file path the assistant set.
fn chosen_file(path: &str) -> String {
    format!("Points at a file the assistant chose: {path}")
}

fn prop_lines(prop: &Prop) -> Vec<String> {
    let shape = json(&prop.shape);
    let kind = shape["type"]
        .as_str()
        .map(words)
        .unwrap_or_else(|| "measured points".to_string());
    let p = prop.transform.position;
    vec![
        format!("kind: {kind}"),
        format!("pixels: {}", prop.node_count()),
        format!(
            "position: x {}, y {}, z {}",
            number(p.x),
            number(p.y),
            number(p.z)
        ),
    ]
}

fn number(value: f32) -> String {
    show_value(&json(&f64::from(value)))
}

fn prop_name(show: &Show, id: pf_model::PropId) -> String {
    show.props
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "a missing prop".into())
}

fn member_names(show: &Show, group: &Group) -> String {
    if group.members.is_empty() {
        return "none".into();
    }
    group
        .members
        .iter()
        .map(|m| match m {
            GroupMember::Prop(id) => prop_name(show, *id),
            GroupMember::Region(r) => target_name(
                show,
                Target::Region {
                    prop: r.prop,
                    region: r.region,
                },
            ),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A controller in full: where its data goes, how, and what's on each port.
fn controller_lines(c: &Controller, show: &Show) -> Vec<String> {
    let protocol = match &c.protocol {
        Protocol::Ddp => "DDP".to_string(),
        Protocol::Sacn(sacn) => {
            let from = match sacn.start_universe {
                Some(u) => format!("universes from {u}"),
                None => "universes assigned automatically".to_string(),
            };
            let multicast = if sacn.multicast { ", multicast" } else { "" };
            format!(
                "sACN, {from} ({} channels each){multicast}",
                sacn.universe_size.channels()
            )
        }
    };
    let mut lines = vec![format!("address: {}", c.address), format!("protocol: {protocol}")];
    if c.adapter != pf_model::AdapterKind::Generic {
        lines.push(format!(
            "device type: {}",
            words(json(&c.adapter).as_str().unwrap_or_default())
        ));
    }
    for port in &c.ports {
        let names: Vec<String> = port.slots.iter().map(|s| prop_name(show, s.prop)).collect();
        let props = if names.is_empty() {
            "nothing".to_string()
        } else {
            names.join(", ")
        };
        lines.push(format!("port {}: {props}", port.number));
    }
    if let Some(channels) = &c.sequence_channels {
        lines.push(format!(
            "sequence channels: {}–{}",
            channels.start,
            u64::from(channels.start) + u64::from(channels.count).saturating_sub(1)
        ));
    }
    lines
}

/// Where light data will now be sent, and whether that's somewhere new.
fn controller_warnings(old: Option<&Controller>, new: &Controller, known: &BTreeSet<&str>) -> Vec<String> {
    let mut out = Vec::new();
    let is_new = !known.contains(new.address.as_str());
    match old {
        None if is_new => out.push(format!("Sends light data to a new address: {}", new.address)),
        None => out.push(format!("Sends light data to {}", new.address)),
        Some(old) if old.address != new.address => out.push(format!(
            "Sends light data to {}{} (was {})",
            if is_new { "a new address: " } else { "" },
            new.address,
            old.address
        )),
        Some(_) => {}
    }
    if let Some(old) = old
        && (old.protocol != new.protocol
            || old.ports != new.ports
            || old.sequence_channels != new.sequence_channels)
    {
        out.push("Changes what this controller is sent".to_string());
    }
    out
}

fn show_settings(before: &Show, after: &Show, out: &mut Vec<Change>) {
    let mut details = Vec::new();
    let mut warnings = Vec::new();
    if before.name != after.name {
        details.push(format!("name: \"{}\" → \"{}\"", before.name, after.name));
    }
    let (a, b) = (json(&before.settings), json(&after.settings));
    field_changes(&a, &b, &mut Vec::new(), &mut details, 0);
    let paths = [
        (
            "background photo",
            before.background.as_ref().map(|b| b.path.as_str()),
            after.background.as_ref().map(|b| b.path.as_str()),
            json(&before.background),
            json(&after.background),
        ),
        (
            "house model",
            before.house_model.as_ref().map(|m| m.path.as_str()),
            after.house_model.as_ref().map(|m| m.path.as_str()),
            json(&before.house_model),
            json(&after.house_model),
        ),
    ];
    for (label, old_path, new_path, x, y) in paths {
        match (old_path, new_path) {
            (None, Some(path)) => details.push(format!("{label}: added ({path})")),
            (Some(_), None) => details.push(format!("{label}: removed")),
            (Some(_), Some(_)) if x != y => {
                let mut inner = Vec::new();
                field_changes(&x, &y, &mut vec![label.to_string()], &mut inner, 0);
                details.extend(inner);
            }
            _ => {}
        }
        if let Some(path) = new_path
            && old_path != Some(path)
        {
            warnings.push(chosen_file(path));
        }
    }
    if !details.is_empty() {
        out.push(Change {
            section: Section::Show,
            action: Action::Changed,
            name: after.name.clone(),
            id: None,
            details,
            warnings,
        });
    }
}

fn json<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// How to compare and describe one kind of item.
struct Keys<I, N, L, A, W> {
    id: I,
    name: N,
    /// Describes an item line by line ("key: value"); changed items list the lines that differ.
    /// Without it, changes are listed field by field.
    lines: Option<L>,
    /// Describes an added item.
    added: Option<A>,
    /// Warnings for an added (old `None`) or changed item.
    warn: W,
}

/// Items matched by id: added, removed, or changed (with what changed).
fn keyed<T, I, N, L, A, W>(
    before: &[T],
    after: &[T],
    keys: Keys<I, N, L, A, W>,
    section: Section,
    out: &mut Vec<Change>,
) where
    T: Serialize,
    I: Fn(&T) -> String,
    N: Fn(&T) -> String,
    L: Fn(&T) -> Vec<String>,
    A: Fn(&T) -> Vec<String>,
    W: Fn(Option<&T>, &T) -> Vec<String>,
{
    let old: HashMap<String, &T> = before.iter().map(|item| ((keys.id)(item), item)).collect();
    let new: HashMap<String, &T> = after.iter().map(|item| ((keys.id)(item), item)).collect();
    for item in after {
        let key = (keys.id)(item);
        match old.get(&key) {
            None => out.push(Change {
                section,
                action: Action::Added,
                name: (keys.name)(item),
                id: Some(key),
                details: keys.added.as_ref().map(|a| a(item)).unwrap_or_default(),
                warnings: (keys.warn)(None, item),
            }),
            Some(previous) => {
                let (a, b) = (json(*previous), json(item));
                if a == b {
                    continue;
                }
                let mut details = Vec::new();
                match &keys.lines {
                    Some(lines) => {
                        let (old_name, new_name) = ((keys.name)(previous), (keys.name)(item));
                        if old_name != new_name {
                            details.push(format!("name: \"{old_name}\" → \"{new_name}\""));
                        }
                        details.extend(line_changes(&lines(previous), &lines(item)));
                    }
                    None => field_changes(&a, &b, &mut Vec::new(), &mut details, 0),
                }
                out.push(Change {
                    section,
                    action: Action::Changed,
                    name: (keys.name)(item),
                    id: Some(key),
                    details,
                    warnings: (keys.warn)(Some(previous), item),
                });
            }
        }
    }
    for item in before {
        let key = (keys.id)(item);
        if !new.contains_key(&key) {
            out.push(Change {
                section,
                action: Action::Removed,
                name: (keys.name)(item),
                id: Some(key),
                details: Vec::new(),
                warnings: Vec::new(),
            });
        }
    }
}

/// "key: value" lines that differ, as "key: old → new" (in the new lines' order, then removed).
fn line_changes(old: &[String], new: &[String]) -> Vec<String> {
    let split = |line: &String| match line.split_once(": ") {
        Some((key, value)) => (key.to_string(), value.to_string()),
        None => (line.clone(), String::new()),
    };
    let old: Vec<(String, String)> = old.iter().map(split).collect();
    let new: Vec<(String, String)> = new.iter().map(split).collect();
    let mut out = Vec::new();
    for (key, value) in &new {
        match old.iter().find(|(k, _)| k == key) {
            Some((_, was)) if was == value => {}
            Some((_, was)) => out.push(format!("{key}: {was} → {value}")),
            None => out.push(format!("{key}: none → {value}")),
        }
    }
    for (key, was) in &old {
        if !new.iter().any(|(k, _)| k == key) {
            out.push(format!("{key}: {was} → none"));
        }
    }
    out
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
            warnings: after
                .audio
                .as_deref()
                .filter(|audio| before.audio.as_deref() != Some(*audio))
                .map(chosen_file)
                .into_iter()
                .collect(),
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
                warnings: Vec::new(),
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
                    warnings: Vec::new(),
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
                warnings: Vec::new(),
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
            warnings: Vec::new(),
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
            warnings: Vec::new(),
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
            details,
            warnings: Vec::new(),
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
            warnings: Vec::new(),
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
                warnings: Vec::new(),
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
                    warnings: Vec::new(),
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
                warnings: Vec::new(),
            });
        }
    }
    let kept = |doc: &Sequence, other: &HashMap<pf_sequence::TimingTrackId, &pf_sequence::TimingTrack>| {
        doc.timing_tracks
            .iter()
            .filter(|t| other.contains_key(&t.id))
            .map(|t| t.id)
            .collect::<Vec<_>>()
    };
    if kept(before, &new_tracks) != kept(after, &old_tracks) {
        out.push(Change {
            section: Section::Sequence,
            action: Action::Changed,
            name: "Timing track order".into(),
            id: None,
            details: vec![
                after
                    .timing_tracks
                    .iter()
                    .map(|t| t.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            ],
            warnings: Vec::new(),
        });
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

    fn controller(name: &str, address: &str, prop: &Prop) -> pf_model::Controller {
        let mut c = pf_model::Controller::new(name, address, pf_model::Protocol::Ddp);
        let mut port = pf_model::Port::new(1);
        port.slots.push(pf_model::PortSlot::new(prop.id));
        c.ports.push(port);
        c
    }

    #[test]
    fn an_added_controller_shows_where_its_data_goes() {
        let mut before = Show::new("Show");
        let roof = line("Roofline");
        before.props.push(roof.clone());
        before.controllers.push(controller("Bench", "10.0.0.5", &roof));
        let mut after = before.clone();
        let mut falcon = controller("Falcon 2", "203.0.113.9", &roof);
        falcon.protocol = pf_model::Protocol::Sacn(pf_model::SacnConfig {
            start_universe: Some(20),
            ..Default::default()
        });
        after.controllers.push(falcon);
        let d = diff(&before, &after, None);
        let added = &d.changes[0];
        assert_eq!(
            (added.section, added.action),
            (Section::Controller, Action::Added)
        );
        assert_eq!(
            added.details,
            [
                "address: 203.0.113.9",
                "protocol: sACN, universes from 20 (510 channels each)",
                "port 1: Roofline",
            ]
        );
        assert_eq!(added.warnings, ["Sends light data to a new address: 203.0.113.9"]);

        // One at an address already in the show is still flagged (data goes there), but not as new.
        let mut again = before.clone();
        again.controllers.push(controller("Bench 2", "10.0.0.5", &roof));
        let d = diff(&before, &again, None);
        assert_eq!(d.changes[0].warnings, ["Sends light data to 10.0.0.5"]);
    }

    #[test]
    fn a_changed_controller_lists_every_change_and_flags_new_addresses() {
        let mut before = Show::new("Show");
        let roof = line("Roofline");
        let arch = line("Arch");
        before.props = vec![roof.clone(), arch.clone()];
        before.controllers.push(controller("Bench", "10.0.0.5", &roof));
        let mut after = before.clone();
        let c = &mut after.controllers[0];
        c.address = "203.0.113.9".into();
        let mut port = pf_model::Port::new(2);
        port.slots.push(pf_model::PortSlot::new(arch.id));
        c.ports.push(port);
        c.sequence_channels = Some(pf_model::SequenceChannels {
            start: 1,
            count: 90,
            raw_ddp_offsets: false,
        });
        let d = diff(&before, &after, None);
        assert_eq!(
            d.changes[0].details,
            [
                "address: 10.0.0.5 → 203.0.113.9",
                "port 2: none → Arch",
                "sequence channels: none → 1–90",
            ]
        );
        assert_eq!(
            d.changes[0].warnings,
            [
                "Sends light data to a new address: 203.0.113.9 (was 10.0.0.5)",
                "Changes what this controller is sent",
            ]
        );
    }

    #[test]
    fn added_props_groups_and_files_show_their_details() {
        let before = Show::new("Show");
        let mut after = before.clone();
        let mut arch = Prop::new(
            "Arch 1",
            ShapeSource::Generator(Generator::Arch {
                nodes: 50,
                width: 2.0,
                height: 1.0,
            }),
        );
        arch.transform.position = pf_model::Vec3::new(1.5, 0.0, -2.0);
        after.props.push(arch.clone());
        let mut group = Group::new("Arches");
        group.members.push(arch.id.into());
        after.groups.push(group);
        let mut entry = pf_model::SequenceEntry::new("Wizards", "/shows/wizards.fseq");
        entry.audio = Some("/music/wizards.mp3".into());
        after.sequences.push(entry);
        after.background = Some(pf_model::Background::new("/photos/house.jpg", 0.0, 0.0, 10.0));
        let d = diff(&before, &after, None);
        let by = |s: Section| d.changes.iter().find(|c| c.section == s).unwrap();
        assert_eq!(
            by(Section::Prop).details,
            ["kind: arch", "pixels: 50", "position: x 1.5, y 0, z -2"]
        );
        assert_eq!(by(Section::Group).details, ["members: Arch 1"]);
        assert_eq!(
            by(Section::Playlist).details,
            [
                "file: /shows/wizards.fseq",
                "music: /music/wizards.mp3",
                "offset: 0 ms"
            ]
        );
        assert_eq!(
            by(Section::Playlist).warnings,
            [
                "Points at a file the assistant chose: /shows/wizards.fseq",
                "Points at a file the assistant chose: /music/wizards.mp3"
            ]
        );
        assert_eq!(
            by(Section::Show).details,
            ["background photo: added (/photos/house.jpg)"]
        );
        assert_eq!(
            by(Section::Show).warnings,
            ["Points at a file the assistant chose: /photos/house.jpg"]
        );
    }

    #[test]
    fn every_detail_of_a_changed_item_is_listed() {
        let mut before = Show::new("Show");
        before.props.push(line("A"));
        let mut after = before.clone();
        let p = &mut after.props[0];
        p.name = "B".into();
        p.transform.position = pf_model::Vec3::new(1.0, 2.0, 3.0);
        p.transform.rotation_deg = pf_model::Vec3::new(4.0, 5.0, 6.0);
        p.transform.scale = pf_model::Vec3::new(2.0, 2.0, 2.0);
        let d = diff(&before, &after, None);
        assert_eq!(d.changes[0].details.len(), 10, "{:?}", d.changes[0].details);
        assert!(!d.changes[0].details.iter().any(|x| x.starts_with("and ")));
    }

    #[test]
    fn timing_track_order_is_a_change() {
        let show = Show::new("Show");
        let mut before = Sequence::new("Song", 60_000);
        before.timing_tracks = vec![
            TimingTrack::new("Beats", TimingKind::Beats, vec![]),
            TimingTrack::new("Bars", TimingKind::Bars, vec![]),
        ];
        let mut after = before.clone();
        after.timing_tracks.reverse();
        let d = diff(&show, &show, Some((&before, &after)));
        assert_eq!(d.changes.len(), 1);
        assert_eq!(d.changes[0].name, "Timing track order");
        assert_eq!(d.changes[0].details, ["Bars, Beats"]);
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
