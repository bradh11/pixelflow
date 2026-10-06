//! Named subsets of a prop's pixels: submodels (an arch segment, a star ring, a window of a
//! matrix) and singing faces. They follow xLights' submodels and face definitions.

use crate::{PropId, RegionId, Rgb};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Half-open range of node indices `[start, end)`, 0-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRange {
    pub start: u32,
    pub end: u32,
}

impl NodeRange {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, node: u32) -> bool {
        self.start <= node && node < self.end
    }

    /// True when the range is well-formed and lies inside a prop with `node_count` pixels.
    pub fn fits_within(&self, node_count: u32) -> bool {
        self.start <= self.end && self.end <= node_count
    }
}

/// A run of pixels on a submodel line: nodes `first` to `last` (0-based, both included), in
/// that order, so `first > last` runs backwards (xLights' `10-1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeRun {
    pub first: u32,
    pub last: u32,
}

impl NodeRun {
    pub fn new(first: u32, last: u32) -> Self {
        Self { first, last }
    }

    /// One pixel.
    pub fn single(node: u32) -> Self {
        Self::new(node, node)
    }

    pub fn len(&self) -> u64 {
        u64::from(self.first.abs_diff(self.last)) + 1
    }

    /// Always false: a run has at least one pixel.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// The lowest and highest node in the run.
    pub fn bounds(&self) -> (u32, u32) {
        (self.first.min(self.last), self.first.max(self.last))
    }

    /// The run's nodes in order, leaving out any at or past `node_count` (as xLights does).
    pub fn nodes(&self, node_count: u32) -> impl Iterator<Item = u32> {
        // Clamp to the prop first, so a damaged run never walks billions of nodes.
        let (lo, hi) = self.bounds();
        let hi = hi.min(node_count.saturating_sub(1));
        let len = if node_count == 0 || lo > hi {
            0
        } else {
            hi - lo + 1
        };
        let forward = self.first <= self.last;
        (0..len).map(move |k| if forward { lo + k } else { lo + len - 1 - k })
    }
}

/// One line of a submodel: runs of pixels, with `None` for an empty spot (xLights' blank or `0`
/// entry), which leaves a gap in the submodel's buffer.
pub type SubmodelLine = Vec<Option<NodeRun>>;

/// How a submodel's lines are laid out for effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineLayout {
    /// Each line is a row, the first at the bottom; pixels run left to right.
    #[default]
    Horizontal,
    /// Each line is a column, the first on the left; pixels run bottom to top.
    Vertical,
}

/// How effects see a submodel's pixels (xLights' submodel "buffer style").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BufferStyle {
    /// Lines side by side (rows or columns), in the order they're listed.
    #[default]
    Default,
    /// The pixels where they really are on the prop, cropped to the submodel.
    #[serde(rename = "keepXY")]
    KeepXy,
    /// Every line on top of the others, starting at the same spot: each line shows the same
    /// part of the effect (a star's rings all doing the same thing, for example).
    StackedStrands,
}

/// Mouth shapes (Preston Blair phoneme set) used by singing faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Phoneme {
    Ai,
    E,
    Etc,
    Fv,
    L,
    Mbp,
    O,
    Rest,
    U,
    Wq,
}

impl Phoneme {
    pub const ALL: [Phoneme; 10] = [
        Phoneme::Ai,
        Phoneme::E,
        Phoneme::Etc,
        Phoneme::Fv,
        Phoneme::L,
        Phoneme::Mbp,
        Phoneme::O,
        Phoneme::Rest,
        Phoneme::U,
        Phoneme::Wq,
    ];

    /// The name xLights (and Papagayo) use: `AI`, `etc`, `rest`, ...
    pub fn xlights_name(self) -> &'static str {
        match self {
            Phoneme::Ai => "AI",
            Phoneme::E => "E",
            Phoneme::Etc => "etc",
            Phoneme::Fv => "FV",
            Phoneme::L => "L",
            Phoneme::Mbp => "MBP",
            Phoneme::O => "O",
            Phoneme::Rest => "rest",
            Phoneme::U => "U",
            Phoneme::Wq => "WQ",
        }
    }

    /// Reads a phoneme name, ignoring case (`AI`, `etc`, `Rest`, `wq`).
    pub fn from_name(name: &str) -> Option<Phoneme> {
        let name = name.trim();
        Phoneme::ALL
            .into_iter()
            .find(|p| p.xlights_name().eq_ignore_ascii_case(name))
    }
}

/// Colors a face was designed with (xLights' "custom colors"). A feature without one is white.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceColors {
    #[serde(default)]
    pub mouths: BTreeMap<Phoneme, Rgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eyes_open: Option<Rgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eyes_closed: Option<Rgb>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Rgb>,
}

/// Maps face features to node ranges on a prop.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceDefinition {
    #[serde(default)]
    pub mouths: BTreeMap<Phoneme, Vec<NodeRange>>,
    #[serde(default)]
    pub eyes_open: Vec<NodeRange>,
    #[serde(default)]
    pub eyes_closed: Vec<NodeRange>,
    #[serde(default)]
    pub outline: Vec<NodeRange>,
    /// The face's own colors; without them the Faces effect uses its palette.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<FaceColors>,
}

impl FaceDefinition {
    /// Every node range the face uses.
    pub fn ranges(&self) -> impl Iterator<Item = &NodeRange> {
        self.mouths
            .values()
            .flatten()
            .chain(&self.eyes_open)
            .chain(&self.eyes_closed)
            .chain(&self.outline)
    }
}

/// What a region covers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RegionKind {
    /// A submodel made of lines of pixels.
    Nodes {
        #[serde(default)]
        lines: Vec<SubmodelLine>,
        #[serde(default)]
        layout: LineLayout,
        #[serde(default)]
        buffer: BufferStyle,
    },
    /// A submodel that is a rectangle of the prop: percentages (0–100) of the prop's width (left
    /// to right) and height (bottom to top).
    SubBuffer { x1: f32, y1: f32, x2: f32, y2: f32 },
    /// A singing-face definition.
    Face(FaceDefinition),
}

fn new_region_id() -> RegionId {
    RegionId::new()
}

/// A named subset of a prop's pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    /// Sequence rows and groups point at a region by id, so renaming it keeps them.
    #[serde(default = "new_region_id")]
    pub id: RegionId,
    pub name: String,
    #[serde(flatten)]
    pub kind: RegionKind,
}

impl Region {
    /// A submodel of lines with the default layout (rows) and buffer style.
    pub fn nodes(name: impl Into<String>, lines: Vec<SubmodelLine>) -> Self {
        Self {
            id: RegionId::new(),
            name: name.into(),
            kind: RegionKind::Nodes {
                lines,
                layout: LineLayout::default(),
                buffer: BufferStyle::default(),
            },
        }
    }

    pub fn face(name: impl Into<String>, face: FaceDefinition) -> Self {
        Self {
            id: RegionId::new(),
            name: name.into(),
            kind: RegionKind::Face(face),
        }
    }

    /// True for submodels (lines or a rectangle), false for faces.
    pub fn is_submodel(&self) -> bool {
        !matches!(self.kind, RegionKind::Face(_))
    }

    /// "submodel" or "face", for messages.
    pub fn kind_word(&self) -> &'static str {
        if self.is_submodel() { "submodel" } else { "face" }
    }

    /// The lowest and highest node of every run or range the region names (nothing for a
    /// rectangle, which picks pixels by position).
    pub fn node_bounds(&self) -> Vec<(u32, u32)> {
        match &self.kind {
            RegionKind::Nodes { lines, .. } => {
                lines.iter().flatten().flatten().map(NodeRun::bounds).collect()
            }
            RegionKind::SubBuffer { .. } => Vec::new(),
            RegionKind::Face(face) => face
                .ranges()
                .filter(|r| !r.is_empty())
                .map(|r| (r.start.min(r.end), r.start.max(r.end) - 1))
                .collect(),
        }
    }

    /// How many pixel entries the region names, each run counted up to `node_count` (a size
    /// check: a damaged file can't make PixelFlow walk billions of nodes).
    pub fn entry_count(&self, node_count: u32) -> u64 {
        let clamp = |n: u64| n.min(u64::from(node_count));
        match &self.kind {
            RegionKind::Nodes { lines, .. } => lines
                .iter()
                .flatten()
                .map(|item| item.map_or(1, |run| clamp(run.len())))
                .sum(),
            RegionKind::SubBuffer { .. } => 0,
            RegionKind::Face(face) => face.ranges().map(|r| clamp(u64::from(r.len()))).sum(),
        }
    }

    /// Every node the region lights, each once, in order of first appearance (nodes at or past
    /// `node_count` are left out). A rectangle needs pixel positions, so it gives nothing here.
    pub fn node_list(&self, node_count: u32) -> Vec<u32> {
        let mut seen = vec![false; node_count as usize];
        let mut out = Vec::new();
        let mut add = |n: u32| {
            if let Some(s) = seen.get_mut(n as usize)
                && !*s
            {
                *s = true;
                out.push(n);
            }
        };
        match &self.kind {
            RegionKind::Nodes { lines, .. } => {
                for run in lines.iter().flatten().flatten() {
                    run.nodes(node_count).for_each(&mut add);
                }
            }
            RegionKind::SubBuffer { .. } => {}
            RegionKind::Face(face) => {
                for range in face.ranges() {
                    (range.start..range.end.min(node_count)).for_each(&mut add);
                }
            }
        }
        out
    }

    /// Why the region can't be used as it is, in plain language (bounds are checked against
    /// the prop separately).
    pub fn problem(&self) -> Option<String> {
        if self.name.trim().is_empty() {
            return Some(format!("A {} has no name.", self.kind_word()));
        }
        if let RegionKind::SubBuffer { x1, y1, x2, y2 } = self.kind
            && [x1, y1, x2, y2]
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=100.0).contains(v))
        {
            return Some(format!(
                "The submodel '{}' covers a rectangle outside the prop; its edges must be 0% to 100%.",
                self.name
            ));
        }
        None
    }
}

/// A region of one prop, as a group member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionRef {
    pub prop: PropId,
    pub region: RegionId,
}

/// Reads one line of an xLights-style pixel list (`1-10,0,15,20-12`): 1-based pixel numbers and
/// ranges (either direction), with a blank or `0` entry for an empty spot. Returns the line, or
/// why it can't be read.
pub fn parse_line(text: &str) -> Result<SubmodelLine, String> {
    let mut line = Vec::new();
    for part in text.split(',') {
        let part = part.trim();
        if part.is_empty() || part == "0" {
            line.push(None);
            continue;
        }
        let number = |s: &str| -> Result<u32, String> {
            let s = s.trim();
            match s.parse::<u32>() {
                Ok(n) if n >= 1 => Ok(n - 1),
                _ => Err(format!(
                    "'{part}' isn't a pixel number or range; use numbers from 1, like 1-10 or 15."
                )),
            }
        };
        let run = match part.split_once('-') {
            Some((a, b)) => NodeRun::new(number(a)?, number(b)?),
            None => NodeRun::single(number(part)?),
        };
        line.push(Some(run));
    }
    // A blank line has no spots at all.
    if line.iter().all(Option::is_none) && text.trim().is_empty() {
        line.clear();
    }
    Ok(line)
}

/// Writes a line the way [`parse_line`] reads it (1-based; `0` for an empty spot).
pub fn format_line(line: &[Option<NodeRun>]) -> String {
    line.iter()
        .map(|item| match item {
            None => "0".to_string(),
            Some(run) if run.first == run.last => (run.first + 1).to_string(),
            Some(run) => format!("{}-{}", run.first + 1, run.last + 1),
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_range_basics() {
        let r = NodeRange::new(2, 5);
        assert_eq!(r.len(), 3);
        assert!(r.contains(2) && r.contains(4) && !r.contains(5));
        assert!(r.fits_within(5));
        assert!(!r.fits_within(4));
        assert!(!NodeRange::new(5, 2).fits_within(10));
        assert!(NodeRange::new(3, 3).is_empty());
    }

    #[test]
    fn runs_walk_either_way_and_stop_at_the_prop() {
        let nodes = |r: NodeRun, n| r.nodes(n).collect::<Vec<_>>();
        assert_eq!(nodes(NodeRun::new(2, 5), 10), vec![2, 3, 4, 5]);
        assert_eq!(nodes(NodeRun::new(5, 2), 10), vec![5, 4, 3, 2]);
        assert_eq!(nodes(NodeRun::new(8, 12), 10), vec![8, 9]);
        assert_eq!(nodes(NodeRun::new(12, 8), 10), vec![9, 8]);
        assert_eq!(nodes(NodeRun::single(10), 10), Vec::<u32>::new());
        assert_eq!(nodes(NodeRun::new(0, u32::MAX), 3), vec![0, 1, 2]);
        assert_eq!(NodeRun::new(7, 3).len(), 5);
        assert_eq!(NodeRun::new(7, 3).bounds(), (3, 7));
    }

    #[test]
    fn lines_read_and_write_like_xlights() {
        let line = parse_line("1-10, 0,15,,20-12").unwrap();
        assert_eq!(
            line,
            vec![
                Some(NodeRun::new(0, 9)),
                None,
                Some(NodeRun::single(14)),
                None,
                Some(NodeRun::new(19, 11)),
            ]
        );
        assert_eq!(format_line(&line), "1-10,0,15,0,20-12");
        assert_eq!(parse_line("").unwrap(), vec![]);
        assert_eq!(parse_line("  ").unwrap(), vec![]);
        for bad in ["a", "1-x", "-3", "1.5"] {
            let err = parse_line(bad).unwrap_err();
            assert!(err.contains("isn't a pixel number or range"), "{bad}: {err}");
        }
    }

    #[test]
    fn region_node_lists_skip_repeats_and_out_of_range_pixels() {
        let region = Region::nodes(
            "Ring",
            vec![
                vec![Some(NodeRun::new(3, 1)), None, Some(NodeRun::single(8))],
                vec![Some(NodeRun::new(2, 4)), Some(NodeRun::single(50))],
            ],
        );
        assert_eq!(region.node_list(10), vec![3, 2, 1, 8, 4]);
        assert_eq!(region.node_bounds(), vec![(1, 3), (8, 8), (2, 4), (50, 50)]);
        assert_eq!(region.entry_count(10), 3 + 1 + 1 + 3 + 1);
        assert!(region.is_submodel());
    }

    #[test]
    fn face_region_lists_all_ranges_and_serializes_phoneme_keys() {
        let mut face = FaceDefinition::default();
        face.mouths.insert(Phoneme::Mbp, vec![NodeRange::new(0, 4)]);
        face.eyes_open.push(NodeRange::new(10, 12));
        face.colors = Some(FaceColors {
            eyes_open: Some(Rgb::new(0, 0, 255)),
            ..FaceColors::default()
        });
        let region = Region::face("Face", face);
        assert_eq!(region.node_list(20), vec![0, 1, 2, 3, 10, 11]);
        assert_eq!(region.node_bounds(), vec![(0, 3), (10, 11)]);
        assert!(!region.is_submodel());
        let json = serde_json::to_value(&region).unwrap();
        assert_eq!(json["kind"], "face");
        assert!(json["mouths"]["MBP"].is_array());
        assert_eq!(json["colors"]["eyesOpen"], "#0000ff");
        assert_eq!(serde_json::from_value::<Region>(json).unwrap(), region);
    }

    #[test]
    fn submodels_serialize_with_their_layout_and_buffer_style() {
        let mut region = Region::nodes("Left", vec![vec![Some(NodeRun::new(0, 4)), None]]);
        region.kind = RegionKind::Nodes {
            lines: vec![vec![Some(NodeRun::new(0, 4)), None]],
            layout: LineLayout::Vertical,
            buffer: BufferStyle::KeepXy,
        };
        let json = serde_json::to_value(&region).unwrap();
        assert_eq!(json["kind"], "nodes");
        assert_eq!(json["layout"], "vertical");
        assert_eq!(json["buffer"], "keepXY");
        assert_eq!(
            json["lines"],
            serde_json::json!([[{ "first": 0, "last": 4 }, null]])
        );
        assert_eq!(serde_json::from_value::<Region>(json).unwrap(), region);

        let window = Region {
            id: RegionId::new(),
            name: "Window".into(),
            kind: RegionKind::SubBuffer {
                x1: 25.0,
                y1: 0.0,
                x2: 75.0,
                y2: 50.0,
            },
        };
        let json = serde_json::to_value(&window).unwrap();
        assert_eq!(json["kind"], "subBuffer");
        assert_eq!(json["x2"], 75.0);
        assert_eq!(serde_json::from_value::<Region>(json).unwrap(), window);
    }

    #[test]
    fn region_problems_are_explained() {
        assert_eq!(Region::nodes("Ok", vec![]).problem(), None);
        assert!(
            Region::nodes(" ", vec![])
                .problem()
                .unwrap()
                .contains("has no name")
        );
        let off = Region {
            id: RegionId::new(),
            name: "Off".into(),
            kind: RegionKind::SubBuffer {
                x1: -5.0,
                y1: 0.0,
                x2: 50.0,
                y2: f32::NAN,
            },
        };
        assert!(off.problem().unwrap().contains("0% to 100%"));
    }

    #[test]
    fn phoneme_names_follow_xlights() {
        assert_eq!(Phoneme::from_name("etc"), Some(Phoneme::Etc));
        assert_eq!(Phoneme::from_name("AI"), Some(Phoneme::Ai));
        assert_eq!(Phoneme::from_name(" Rest "), Some(Phoneme::Rest));
        assert_eq!(Phoneme::from_name("xyz"), None);
        assert_eq!(Phoneme::Wq.xlights_name(), "WQ");
    }
}
