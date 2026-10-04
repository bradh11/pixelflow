//! Named subsets of a prop's pixels, including singing-face definitions.

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
}

/// What a region covers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum RegionKind {
    /// A plain set of node ranges (an xLights "submodel").
    Nodes { ranges: Vec<NodeRange> },
    /// A singing-face definition.
    Face(FaceDefinition),
}

/// A named subset of a prop's pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub name: String,
    #[serde(flatten)]
    pub kind: RegionKind,
}

impl Region {
    /// Every node range referenced by this region.
    pub fn ranges(&self) -> Vec<NodeRange> {
        match &self.kind {
            RegionKind::Nodes { ranges } => ranges.clone(),
            RegionKind::Face(face) => face
                .mouths
                .values()
                .flatten()
                .chain(&face.eyes_open)
                .chain(&face.eyes_closed)
                .chain(&face.outline)
                .copied()
                .collect(),
        }
    }
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
    fn face_region_lists_all_ranges_and_serializes_phoneme_keys() {
        let mut face = FaceDefinition::default();
        face.mouths.insert(Phoneme::Mbp, vec![NodeRange::new(0, 4)]);
        face.eyes_open.push(NodeRange::new(10, 12));
        let region = Region {
            name: "Face".into(),
            kind: RegionKind::Face(face),
        };
        assert_eq!(region.ranges().len(), 2);
        let json = serde_json::to_value(&region).unwrap();
        assert_eq!(json["kind"], "face");
        assert!(json["mouths"]["MBP"].is_array());
        assert_eq!(serde_json::from_value::<Region>(json).unwrap(), region);
    }
}
