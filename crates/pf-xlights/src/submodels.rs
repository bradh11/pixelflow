//! A model's `<subModel>` and `<faceInfo>` children as PixelFlow regions, following xLights'
//! `SubModel.cpp` (line and range parsing, buffer styles, sub-buffers) and
//! `Model::UpdateFaceInfoNodes` (face node lists).

use crate::import::list;
use pf_model::{
    BufferStyle, FaceColors, FaceDefinition, LineLayout, NodeRange, NodeRun, Phoneme, Region, RegionId,
    RegionKind, Rgb, SubmodelLine,
};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// An element's attributes, as written.
pub type Attrs = BTreeMap<String, String>;

/// What didn't come across exactly, gathered over every model for a few notes.
#[derive(Debug, Default)]
pub(crate) struct RegionNotes {
    past_end: Vec<String>,
    unreadable: Vec<String>,
    styles: Vec<String>,
    rectangles: Vec<String>,
    picture_faces: Vec<String>,
    channel_faces: Vec<String>,
    states: Vec<String>,
    one_color: Vec<String>,
    colors: Vec<String>,
    repeated: Vec<String>,
    renamed: Vec<String>,
    unnamed: usize,
}

impl RegionNotes {
    pub fn into_notes(self, notes: &mut Vec<String>) {
        let mut add = |items: &[String], text: &str| {
            if !items.is_empty() {
                notes.push(text.replace("{}", &list(items)));
            }
        };
        add(
            &self.past_end,
            "These submodels and faces list pixels past the end of their prop; xLights skips those pixels, and so does PixelFlow: {}.",
        );
        add(
            &self.unreadable,
            "Some entries in these submodels aren't pixel numbers, so they were read as empty spots: {}.",
        );
        add(
            &self.styles,
            "These submodels use a buffer style PixelFlow doesn't have, so they use the default style: {}.",
        );
        add(
            &self.rectangles,
            "These submodels' rectangles reach outside their prop, so they were trimmed to its edges: {}.",
        );
        add(
            &self.picture_faces,
            "PixelFlow doesn't support picture faces yet, so these weren't imported: {}.",
        );
        add(
            &self.channel_faces,
            "PixelFlow doesn't support channel-based (Coro) faces yet, so these weren't imported: {}.",
        );
        add(
            &self.states,
            "PixelFlow doesn't import states yet, so these weren't imported: {}.",
        );
        add(
            &self.one_color,
            "PixelFlow draws each part of a face in one color, so these faces' second and third colors became their first: {}.",
        );
        add(
            &self.colors,
            "Some colors in these faces couldn't be read, so they show white: {}.",
        );
        add(
            &self.repeated,
            "These props have two submodels or faces with the same name; only the first was imported: {}.",
        );
        add(
            &self.renamed,
            "These faces share a name with a submodel on the same prop, so \"(face)\" was added to their names: {}.",
        );
        if self.unnamed > 0 {
            notes.push(format!(
                "{} without a name {} left out.",
                crate::sequence::plural(self.unnamed, "submodel"),
                if self.unnamed == 1 { "was" } else { "were" }
            ));
        }
    }
}

/// The leading whole number of `text`, like C's `strtol` (0 when there isn't one).
fn strtol(text: &str) -> i64 {
    let t = text.trim_start();
    let (sign, digits) = match t.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, t.strip_prefix('+').unwrap_or(t)),
    };
    let end = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
    digits[..end]
        .parse::<i64>()
        .map(|n| sign * n)
        .unwrap_or(if end > 0 { i64::MAX } else { 0 })
}

/// True when an entry has nothing but a number or `a-b` range in it.
fn tidy_entry(entry: &str) -> bool {
    let entry = entry.trim();
    let number = |s: &str| !s.trim().is_empty() && s.trim().chars().all(|c| c.is_ascii_digit());
    entry.is_empty()
        || match entry.split_once('-') {
            Some((a, b)) => number(a) && number(b),
            None => number(entry),
        }
}

/// One xLights submodel line (`1-10,0,15`), as xLights reads it: `0` or a blank is an empty
/// spot, `a-b` runs either way, and pixels past the prop are skipped (they leave no spot).
/// Returns the line and whether anything was skipped or unreadable.
fn read_line(text: &str, nodes: u32) -> (SubmodelLine, bool, bool) {
    let mut line = Vec::new();
    let (mut past_end, mut unreadable) = (false, false);
    for entry in text.split(',') {
        unreadable |= !tidy_entry(entry);
        let (start, end) = match entry.find('-') {
            Some(at) => (strtol(&entry[..at]), strtol(&entry[at + 1..])),
            None => {
                let n = strtol(entry);
                (n, n)
            }
        };
        if start <= 0 {
            line.push(None);
            continue;
        }
        let (first, last) = (start - 1, (end - 1).max(0));
        let limit = i64::from(nodes) - 1;
        let clamp = |n: i64| n.min(limit);
        if first.min(last) > limit {
            past_end = true;
            continue;
        }
        if first.max(last) > limit {
            past_end = true;
        }
        line.push(Some(NodeRun::new(clamp(first) as u32, clamp(last) as u32)));
    }
    (line, past_end, unreadable)
}

/// A face feature's node list (`1-4,9`), as `Model::UpdateFaceInfoNodes` reads it, as sorted
/// ranges. Returns the ranges and whether some pixels were past the prop.
fn read_nodes(text: &str, nodes: u32, into: &mut BTreeSet<u32>) -> bool {
    let mut past_end = false;
    for entry in text.split(',') {
        let (mut start, mut end) = match entry.find('-') {
            Some(at) => (strtol(&entry[..at]), strtol(&entry[at + 1..])),
            None => {
                let n = strtol(entry);
                (n, n)
            }
        };
        if end < start {
            std::mem::swap(&mut start, &mut end);
        }
        let (from, to) = ((start - 1).max(0), end - 1);
        if to >= i64::from(nodes) {
            past_end = true;
        }
        let to = to.min(i64::from(nodes) - 1);
        if from <= to {
            into.extend(from as u32..=to as u32);
        }
    }
    past_end
}

/// Sorted nodes as half-open ranges.
fn ranges(nodes: &BTreeSet<u32>) -> Vec<NodeRange> {
    let mut out: Vec<NodeRange> = Vec::new();
    for &n in nodes {
        match out.last_mut() {
            Some(r) if r.end == n => r.end += 1,
            _ => out.push(NodeRange::new(n, n + 1)),
        }
    }
    out
}

fn submodel(attrs: &Attrs, nodes: u32, label: &str, notes: &mut RegionNotes) -> RegionKind {
    let get = |k: &str| attrs.get(k).map(String::as_str);
    if get("type").unwrap_or("ranges") != "ranges" {
        let parts: Vec<f32> = get("subBuffer")
            .unwrap_or("")
            .split('x')
            .map(|p| {
                p.trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .unwrap_or(0.0)
            })
            .collect();
        let at = |i: usize, default: f32| {
            if parts.len() > i && !get("subBuffer").unwrap_or("").is_empty() {
                parts[i]
            } else {
                default
            }
        };
        let (mut x1, mut y1, mut x2, mut y2) = (at(0, 0.0), at(1, 0.0), at(2, 100.0), at(3, 100.0));
        if x1 > x2 {
            std::mem::swap(&mut x1, &mut x2);
        }
        if y1 > y2 {
            std::mem::swap(&mut y1, &mut y2);
        }
        let edges = [x1, y1, x2, y2];
        if edges.iter().any(|v| !(0.0..=100.0).contains(v)) {
            notes.rectangles.push(label.to_string());
        }
        let c = |v: f32| v.clamp(0.0, 100.0);
        return RegionKind::SubBuffer {
            x1: c(x1),
            y1: c(y1),
            x2: c(x2),
            y2: c(y2),
        };
    }
    let layout = if get("layout").unwrap_or("vertical") == "vertical" {
        LineLayout::Vertical
    } else {
        LineLayout::Horizontal
    };
    // xLights reads and writes `bufferstyle` (`XmlNodeKeys::BufferStyleAttribute`); attribute
    // names are case-sensitive, so the camelCase spelling is only a fallback for hand-made files.
    let style = get("bufferstyle").or_else(|| get("bufferStyle"));
    let buffer = match style.unwrap_or("Default") {
        "Default" => BufferStyle::Default,
        "Keep XY" => BufferStyle::KeepXy,
        "Stacked Strands" => BufferStyle::StackedStrands,
        _ => {
            notes.styles.push(label.to_string());
            BufferStyle::Default
        }
    };
    let (mut past_end, mut unreadable) = (false, false);
    let mut lines = Vec::new();
    for i in 0.. {
        let Some(text) = get(&format!("line{i}")) else {
            break;
        };
        let (line, past, bad) = read_line(text, nodes);
        past_end |= past;
        unreadable |= bad;
        lines.push(line);
    }
    if past_end {
        notes.past_end.push(label.to_string());
    }
    if unreadable {
        notes.unreadable.push(label.to_string());
    }
    RegionKind::Nodes {
        lines,
        layout,
        buffer,
    }
}

/// xLights' face feature names for a PixelFlow feature, main one first.
fn variants(base: &str) -> [String; 3] {
    if base.starts_with("Eyes-") {
        [base.to_string(), format!("{base}2"), format!("{base}3")]
    } else {
        [base.to_string(), format!("{base}2"), String::new()]
    }
}

fn face(attrs: &Attrs, nodes: u32, label: &str, notes: &mut RegionNotes) -> FaceDefinition {
    let custom = attrs.get("CustomColors").map(String::as_str) == Some("1");
    let (mut past_end, mut one_color, mut bad_color) = (false, false, false);
    let mut color = |key: &str| -> Option<Rgb> {
        let text = attrs.get(&format!("{key}-Color"))?.trim();
        if text.is_empty() {
            return None;
        }
        let parsed = Rgb::from_hex(text);
        bad_color |= parsed.is_none();
        parsed
    };
    // One feature: its nodes (all variants merged) and its main color.
    let mut feature = |base: &str| -> (Vec<NodeRange>, Option<Rgb>) {
        let mut set = BTreeSet::new();
        let names = variants(base);
        let main = if custom { color(&names[0]) } else { None };
        for (i, name) in names.iter().enumerate().filter(|(_, n)| !n.is_empty()) {
            let Some(text) = attrs.get(name).filter(|t| !t.trim().is_empty()) else {
                continue;
            };
            past_end |= read_nodes(text, nodes, &mut set);
            if i > 0 && custom && color(name).unwrap_or(Rgb::WHITE) != main.unwrap_or(Rgb::WHITE) {
                one_color = true;
            }
        }
        (ranges(&set), main)
    };
    let mut def = FaceDefinition::default();
    let mut colors = FaceColors::default();
    for phoneme in Phoneme::ALL {
        let (found, c) = feature(&format!("Mouth-{}", phoneme.xlights_name()));
        if !found.is_empty() {
            def.mouths.insert(phoneme, found);
        }
        if let Some(c) = c {
            colors.mouths.insert(phoneme, c);
        }
    }
    (def.eyes_open, colors.eyes_open) = feature("Eyes-Open");
    (def.eyes_closed, colors.eyes_closed) = feature("Eyes-Closed");
    (def.outline, colors.outline) = feature("FaceOutline");
    if custom {
        def.colors = Some(colors);
    }
    if past_end {
        notes.past_end.push(label.to_string());
    }
    if one_color {
        notes.one_color.push(label.to_string());
    }
    if bad_color {
        notes.colors.push(label.to_string());
    }
    def
}

/// The regions for one model with `nodes` pixels: its submodels (in file order), then its
/// NodeRange faces. Names stay unique on the prop.
pub(crate) fn regions(
    model: &str,
    submodels: &[Attrs],
    faces: &[Attrs],
    states: &[String],
    nodes: u32,
    notes: &mut RegionNotes,
) -> Vec<Region> {
    let mut out: Vec<Region> = Vec::new();
    let mut names = HashSet::new();
    for attrs in submodels {
        let name = attrs.get("name").map(|n| n.trim()).unwrap_or("");
        if name.is_empty() {
            notes.unnamed += 1;
            continue;
        }
        let label = format!("{model}/{name}");
        if !names.insert(name.to_lowercase()) {
            notes.repeated.push(label);
            continue;
        }
        let kind = submodel(attrs, nodes, &label, notes);
        out.push(Region {
            id: RegionId::new(),
            name: name.to_string(),
            kind,
        });
    }
    for attrs in faces {
        let kind = attrs.get("Type").map(|t| t.trim()).unwrap_or("SingleNode");
        let name = attrs
            .get("Name")
            .map(|n| n.trim())
            .filter(|n| !n.is_empty())
            .unwrap_or(kind);
        let label = format!("{name} (on {model})");
        match kind {
            "NodeRange" => {}
            "SingleNode" | "Coro" => {
                notes.channel_faces.push(label);
                continue;
            }
            _ => {
                notes.picture_faces.push(label);
                continue;
            }
        }
        let mut region_name = name.to_string();
        if names.contains(&region_name.to_lowercase()) {
            region_name = format!("{name} (face)");
            notes.renamed.push(label.clone());
        }
        if !names.insert(region_name.to_lowercase()) {
            notes.repeated.push(label);
            continue;
        }
        let def = face(attrs, nodes, &label, notes);
        out.push(Region::face(region_name, def));
    }
    notes
        .states
        .extend(states.iter().map(|s| format!("{s} (on {model})")));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(pairs: &[(&str, &str)]) -> Attrs {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn run(a: u32, b: u32) -> Option<NodeRun> {
        Some(NodeRun::new(a, b))
    }

    #[test]
    fn lines_read_like_xlights() {
        assert_eq!(
            read_line("1-5,0,,9,12-10", 20),
            (vec![run(0, 4), None, None, run(8, 8), run(11, 9)], false, false)
        );
        // Past the end: trimmed, or skipped without leaving a spot.
        assert_eq!(
            read_line("8-12,15,3", 10),
            (vec![run(7, 9), run(2, 2)], true, false)
        );
        assert_eq!(read_line("12-8", 10), (vec![run(9, 7)], true, false));
        // Not numbers: read as xLights' strtol would.
        assert_eq!(read_line("x,4a", 10), (vec![None, run(3, 3)], false, true));
        assert_eq!(read_line("5-", 10), (vec![run(4, 0)], false, true));
        assert_eq!(read_line("", 10), (vec![None], false, false));
    }

    #[test]
    fn each_buffer_style_and_layout_is_kept() {
        let mut notes = RegionNotes::default();
        let regions = regions(
            "Arches",
            &[
                attrs(&[
                    ("name", "Arch 1"),
                    ("layout", "horizontal"),
                    ("type", "ranges"),
                    ("line0", "1-50"),
                ]),
                attrs(&[
                    ("name", "Tops"),
                    ("type", "ranges"),
                    ("bufferstyle", "Stacked Strands"),
                    ("line0", "20-30"),
                    ("line1", "70-80"),
                    ("line2", "120-130"),
                ]),
                attrs(&[
                    ("name", "Ends"),
                    ("layout", "horizontal"),
                    ("bufferstyle", "Keep XY"),
                    ("line0", "1-5,46-50"),
                ]),
                attrs(&[("name", "Odd"), ("bufferstyle", "Wavy"), ("line0", "1")]),
                attrs(&[
                    ("name", "Window"),
                    ("type", "subbuffer"),
                    ("subBuffer", "50x100x0x50"),
                ]),
                attrs(&[
                    ("name", "Wide"),
                    ("type", "subbuffer"),
                    ("subBuffer", "-10x0x120x100"),
                ]),
                attrs(&[("name", "All"), ("type", "subbuffer")]),
                attrs(&[("name", "tops"), ("line0", "1")]),
                attrs(&[("name", " "), ("line0", "1")]),
            ],
            &[],
            &[],
            150,
            &mut notes,
        );
        let kinds: Vec<(&str, &RegionKind)> = regions.iter().map(|r| (r.name.as_str(), &r.kind)).collect();
        assert_eq!(
            kinds,
            vec![
                (
                    "Arch 1",
                    &RegionKind::Nodes {
                        lines: vec![vec![run(0, 49)]],
                        layout: LineLayout::Horizontal,
                        buffer: BufferStyle::Default
                    }
                ),
                (
                    "Tops",
                    &RegionKind::Nodes {
                        lines: vec![vec![run(19, 29)], vec![run(69, 79)], vec![run(119, 129)]],
                        layout: LineLayout::Vertical,
                        buffer: BufferStyle::StackedStrands
                    }
                ),
                (
                    "Ends",
                    &RegionKind::Nodes {
                        lines: vec![vec![run(0, 4), run(45, 49)]],
                        layout: LineLayout::Horizontal,
                        buffer: BufferStyle::KeepXy
                    }
                ),
                (
                    "Odd",
                    &RegionKind::Nodes {
                        lines: vec![vec![run(0, 0)]],
                        layout: LineLayout::Vertical,
                        buffer: BufferStyle::Default
                    }
                ),
                (
                    "Window",
                    &RegionKind::SubBuffer {
                        x1: 0.0,
                        y1: 50.0,
                        x2: 50.0,
                        y2: 100.0
                    }
                ),
                (
                    "Wide",
                    &RegionKind::SubBuffer {
                        x1: 0.0,
                        y1: 0.0,
                        x2: 100.0,
                        y2: 100.0
                    }
                ),
                (
                    "All",
                    &RegionKind::SubBuffer {
                        x1: 0.0,
                        y1: 0.0,
                        x2: 100.0,
                        y2: 100.0
                    }
                ),
            ]
        );
        let mut text = Vec::new();
        notes.into_notes(&mut text);
        let text = text.join("\n");
        for expected in [
            "buffer style PixelFlow doesn't have, so they use the default style: Arches/Odd.",
            "rectangles reach outside their prop, so they were trimmed to its edges: Arches/Wide.",
            "same name; only the first was imported: Arches/tops.",
            "1 submodel without a name was left out.",
        ] {
            assert!(text.contains(expected), "{expected}\n{text}");
        }
    }

    /// Attributes exactly as xLights writes them (`BaseSerializingVisitor::WriteSubmodels`:
    /// `name` first, then sorted, with the buffer style as all-lowercase `bufferstyle`).
    #[test]
    fn real_xlights_attribute_casing_keeps_the_buffer_style() {
        let mut notes = RegionNotes::default();
        let regions = regions(
            "Star",
            &[
                attrs(&[
                    ("name", "Rings"),
                    ("bufferstyle", "Stacked Strands"),
                    ("layout", "horizontal"),
                    ("line0", "1-10"),
                    ("line1", "11-20"),
                    ("type", "ranges"),
                ]),
                attrs(&[
                    ("name", "Points"),
                    ("bufferstyle", "Keep XY"),
                    ("layout", "vertical"),
                    ("line0", "1,5,9"),
                    ("type", "ranges"),
                ]),
                attrs(&[
                    ("name", "Middle"),
                    ("bufferstyle", "Default"),
                    ("layout", "horizontal"),
                    ("subBuffer", "25x25x75x75"),
                    ("type", "subbuffer"),
                ]),
                // Older hand-edited files: the camelCase spelling still reads.
                attrs(&[("name", "Old"), ("bufferStyle", "Keep XY"), ("line0", "2")]),
            ],
            &[],
            &[],
            20,
            &mut notes,
        );
        let style = |i: usize| match &regions[i].kind {
            RegionKind::Nodes { buffer, .. } => *buffer,
            other => panic!("{other:?}"),
        };
        assert_eq!(style(0), BufferStyle::StackedStrands);
        assert_eq!(style(1), BufferStyle::KeepXy);
        assert!(matches!(regions[2].kind, RegionKind::SubBuffer { x1, .. } if x1 == 25.0));
        assert_eq!(style(3), BufferStyle::KeepXy);
        let mut text = Vec::new();
        notes.into_notes(&mut text);
        assert!(text.is_empty(), "{text:?}");
    }

    #[test]
    fn node_range_faces_keep_their_features_and_colors() {
        let mut notes = RegionNotes::default();
        let regions = regions(
            "Matrix",
            &[attrs(&[("name", "Singer"), ("line0", "1")])],
            &[
                attrs(&[
                    ("Name", "Singer"),
                    ("Type", "NodeRange"),
                    ("CustomColors", "1"),
                    ("Mouth-AI", "1-4"),
                    ("Mouth-AI-Color", "#FF0000"),
                    ("Mouth-AI2", "6"),
                    ("Mouth-AI2-Color", "#0000FF"),
                    ("Mouth-rest", "9-10"),
                    ("Eyes-Open", "62-61,79-80"),
                    ("Eyes-Open-Color", "#00FF00"),
                    ("Eyes-Closed", "63,78"),
                    ("Eyes-Closed-Color", "green"),
                    ("FaceOutline", "21-40,95"),
                ]),
                attrs(&[
                    ("Name", "Pictures"),
                    ("Type", "Matrix"),
                    ("Mouth-AI-EyesOpen", "ai.png"),
                ]),
                attrs(&[("Name", "Old"), ("Type", "Coro"), ("Mouth-AI", "Node 1")]),
            ],
            &["Lights".to_string()],
            80,
            &mut notes,
        );
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[1].name, "Singer (face)");
        let RegionKind::Face(face) = &regions[1].kind else {
            panic!("a face")
        };
        assert_eq!(
            face.mouths[&Phoneme::Ai],
            vec![NodeRange::new(0, 4), NodeRange::new(5, 6)]
        );
        assert_eq!(face.mouths[&Phoneme::Rest], vec![NodeRange::new(8, 10)]);
        assert!(!face.mouths.contains_key(&Phoneme::O));
        assert_eq!(
            face.eyes_open,
            vec![NodeRange::new(60, 62), NodeRange::new(78, 80)]
        );
        assert_eq!(
            face.eyes_closed,
            vec![NodeRange::new(62, 63), NodeRange::new(77, 78)]
        );
        assert_eq!(face.outline, vec![NodeRange::new(20, 40)]);
        let colors = face.colors.as_ref().unwrap();
        assert_eq!(colors.mouths[&Phoneme::Ai], Rgb::new(255, 0, 0));
        assert_eq!(colors.eyes_open, Some(Rgb::new(0, 255, 0)));
        assert_eq!(colors.eyes_closed, None);
        assert_eq!(colors.outline, None);

        let mut text = Vec::new();
        notes.into_notes(&mut text);
        let text = text.join("\n");
        for expected in [
            "pixels past the end of their prop; xLights skips those pixels, and so does PixelFlow: Singer (on Matrix).",
            "picture faces yet, so these weren't imported: Pictures (on Matrix).",
            "channel-based (Coro) faces yet, so these weren't imported: Old (on Matrix).",
            "states yet, so these weren't imported: Lights (on Matrix).",
            "second and third colors became their first: Singer (on Matrix).",
            "couldn't be read, so they show white: Singer (on Matrix).",
            "\"(face)\" was added to their names: Singer (on Matrix).",
        ] {
            assert!(text.contains(expected), "{expected}\n{text}");
        }
    }

    #[test]
    fn faces_without_custom_colors_use_the_palette() {
        let mut notes = RegionNotes::default();
        let regions = regions(
            "Tree",
            &[],
            &[attrs(&[
                ("Name", "Face1"),
                ("Type", "NodeRange"),
                ("Mouth-O", "3"),
                ("Mouth-O-Color", "#ff0000"),
            ])],
            &[],
            10,
            &mut notes,
        );
        let RegionKind::Face(face) = &regions[0].kind else {
            panic!("a face")
        };
        assert_eq!(face.colors, None);
        assert_eq!(face.mouths[&Phoneme::O], vec![NodeRange::new(2, 3)]);
    }
}
