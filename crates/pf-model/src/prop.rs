//! Props (physical light elements) and groups of props.

use crate::{
    ColorOrder, DEFAULT_GRID_SIZE, GroupId, GroupLayout, PropId, Region, RegionId, RegionRef, ShapeSource,
    Transform,
};
use serde::{Deserialize, Serialize};

/// How a prop's pixels are laid out for an effect drawn on the prop itself, when that isn't the
/// way its shape lays them out.
// xLights draws on a model's own buffer, which for a single line or a poly line is one row from
// the first point drawn to the last, and for a custom model its grid of cells; an import keeps
// that here. A layout whose cells don't fit the prop's pixels any more is passed over, and the
// prop is laid out as its shape is.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "schema",
    schemars(description = "How effects on it alone lay out its pixels.")
)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum OwnLayout {
    /// The shape's own way: on its grid when it has one (a matrix, a tree's strings), else where
    /// its pixels sit seen from the front.
    #[cfg_attr(feature = "schema", schemars(description = "As its shape does."))]
    #[default]
    Shape,
    /// One row with a cell for every pixel in wiring order (`reversed`: the last pixel first),
    /// whichever way the prop runs in the layout.
    #[cfg_attr(
        feature = "schema",
        schemars(description = "One row along the wiring (`reversed`: last pixel first).")
    )]
    Strand {
        #[serde(default, skip_serializing_if = "is_false")]
        reversed: bool,
    },
    /// A grid of `columns` by `rows` with each pixel in the cell given for it: `[column from the
    /// left, row from the bottom]`, in wiring order.
    #[cfg_attr(
        feature = "schema",
        schemars(description = "A grid; `cells` has each pixel as `[column, row up]`.")
    )]
    Cells {
        columns: u32,
        rows: u32,
        cells: Vec<[u32; 2]>,
    },
}

impl OwnLayout {
    fn is_shape(&self) -> bool {
        *self == OwnLayout::Shape
    }

    /// The grid this lays `nodes` pixels on, and each pixel's cell (`[column, row from the
    /// bottom]`, in wiring order). `None` when the shape lays them out, and for cells that
    /// aren't one a pixel, inside the grid.
    pub fn grid(&self, nodes: u32) -> Option<(u32, u32, Vec<[u32; 2]>)> {
        match self {
            OwnLayout::Shape => None,
            OwnLayout::Strand { reversed } => {
                let cells = (0..nodes).map(|n| [if *reversed { nodes - 1 - n } else { n }, 0]);
                (nodes > 0).then(|| (nodes, 1, cells.collect()))
            }
            OwnLayout::Cells { columns, rows, cells } => {
                let fits = cells.len() == nodes as usize
                    && nodes > 0
                    && cells.iter().all(|&[c, r]| c < *columns && r < *rows);
                fits.then(|| (*columns, *rows, cells.clone()))
            }
        }
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

/// A physical light element: an arch, a matrix, a tree, etc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct Prop {
    pub id: PropId,
    pub name: String,
    pub shape: ShapeSource,
    #[serde(default)]
    pub transform: Transform,
    #[serde(default)]
    pub color_order: ColorOrder,
    #[serde(default)]
    pub regions: Vec<Region>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// How effects on the prop itself lay its pixels out, when not as its shape does.
    #[serde(default, skip_serializing_if = "OwnLayout::is_shape")]
    #[cfg_attr(feature = "schema", schemars(description = ""))]
    pub own_layout: OwnLayout,
}

impl Prop {
    /// Creates a prop with a fresh id, identity transform, and RGB color order.
    pub fn new(name: impl Into<String>, shape: ShapeSource) -> Self {
        Self {
            id: PropId::new(),
            name: name.into(),
            shape,
            transform: Transform::default(),
            color_order: ColorOrder::default(),
            regions: Vec::new(),
            tags: Vec::new(),
            own_layout: OwnLayout::Shape,
        }
    }

    pub fn node_count(&self) -> u32 {
        self.shape.node_count()
    }

    pub fn channels_per_pixel(&self) -> u8 {
        self.color_order.channels_per_pixel()
    }

    /// Bytes this prop occupies in the frame buffer.
    pub fn channel_count(&self) -> usize {
        self.node_count() as usize * self.channels_per_pixel() as usize
    }

    pub fn region(&self, id: RegionId) -> Option<&Region> {
        self.regions.iter().find(|r| r.id == id)
    }
}

/// One member of a group: a whole prop, or one of a prop's submodels.
///
/// Serialized as the prop's id (a plain string, as every group member was before submodels)
/// or as `{ "prop": …, "region": … }`, so older show files read unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum GroupMember {
    Prop(PropId),
    Region(RegionRef),
}

impl GroupMember {
    /// The prop this member is (or is part of).
    pub fn prop(&self) -> PropId {
        match self {
            GroupMember::Prop(id) => *id,
            GroupMember::Region(r) => r.prop,
        }
    }
}

impl From<PropId> for GroupMember {
    fn from(id: PropId) -> Self {
        GroupMember::Prop(id)
    }
}

impl From<RegionRef> for GroupMember {
    fn from(r: RegionRef) -> Self {
        GroupMember::Region(r)
    }
}

/// A named, ordered set of props and submodels. Order matters: effects that run along the
/// group (a chase) count pixels member by member, as xLights does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    #[serde(default)]
    pub members: Vec<GroupMember>,
    /// How effects lay the group's pixels out, unless an effect picks its own render style.
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(description = ""))]
    pub layout: GroupLayout,
    /// The most cells along the longer side of the group's grid (the grid layouts and the "per
    /// preview" render style), from [`MIN_GRID_SIZE`](crate::MIN_GRID_SIZE) to
    /// [`MAX_GRID_SIZE`](crate::MAX_GRID_SIZE).
    #[serde(default = "default_grid_size")]
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Most grid cells across (10-4000; 400).")
    )]
    pub grid_size: u32,
}

fn default_grid_size() -> u32 {
    DEFAULT_GRID_SIZE
}

impl Group {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: GroupId::new(),
            name: name.into(),
            members: Vec::new(),
            layout: GroupLayout::default(),
            grid_size: DEFAULT_GRID_SIZE,
        }
    }

    /// The props the group draws on (whole or in part), in member order, each once.
    pub fn props(&self) -> Vec<PropId> {
        let mut out: Vec<PropId> = Vec::new();
        for m in &self.members {
            let id = m.prop();
            if !out.contains(&id) {
                out.push(id);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Generator;

    #[test]
    fn channel_count_uses_color_order() {
        let mut prop = Prop::new("Arch", ShapeSource::Generator(Generator::arch(50, 4.0, 2.0)));
        assert_eq!(prop.channel_count(), 150);
        prop.color_order = ColorOrder::Grbw;
        assert_eq!(prop.channel_count(), 200);
    }

    #[test]
    fn optional_fields_default_when_missing() {
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "name": "Line",
            "shape": { "source": "generator", "type": "line", "nodes": 10, "length": 3.0 }
        }"#;
        let prop: Prop = serde_json::from_str(json).unwrap();
        assert_eq!(prop.color_order, ColorOrder::Rgb);
        assert_eq!(prop.transform, Transform::default());
        assert!(prop.regions.is_empty());
        assert_eq!(prop.own_layout, OwnLayout::Shape);
    }

    #[test]
    fn a_layout_of_its_own_is_saved_only_when_the_prop_has_one() {
        let line = Generator::Line {
            nodes: 4,
            length: 3.0,
        };
        let mut prop = Prop::new("Line", ShapeSource::Generator(line));
        let saved = serde_json::to_value(&prop).unwrap();
        assert!(saved.get("ownLayout").is_none(), "{saved}");

        prop.own_layout = OwnLayout::Strand { reversed: false };
        let saved = serde_json::to_value(&prop).unwrap();
        assert_eq!(saved["ownLayout"], serde_json::json!({ "kind": "strand" }));
        assert_eq!(serde_json::from_value::<Prop>(saved).unwrap(), prop);

        prop.own_layout = OwnLayout::Strand { reversed: true };
        let saved = serde_json::to_value(&prop).unwrap();
        assert_eq!(
            saved["ownLayout"],
            serde_json::json!({ "kind": "strand", "reversed": true })
        );
        assert_eq!(serde_json::from_value::<Prop>(saved).unwrap(), prop);

        prop.own_layout = OwnLayout::Cells {
            columns: 3,
            rows: 2,
            cells: vec![[0, 0], [2, 0], [2, 1], [0, 1]],
        };
        let saved = serde_json::to_value(&prop).unwrap();
        assert_eq!(
            saved["ownLayout"],
            serde_json::json!({ "kind": "cells", "columns": 3, "rows": 2, "cells": [[0, 0], [2, 0], [2, 1], [0, 1]] })
        );
        assert_eq!(serde_json::from_value::<Prop>(saved).unwrap(), prop);
    }

    #[test]
    fn a_strand_is_one_row_along_the_wiring_from_either_end() {
        assert_eq!(OwnLayout::Shape.grid(3), None);
        assert_eq!(
            OwnLayout::Strand { reversed: false }.grid(3),
            Some((3, 1, vec![[0, 0], [1, 0], [2, 0]]))
        );
        assert_eq!(
            OwnLayout::Strand { reversed: true }.grid(3),
            Some((3, 1, vec![[2, 0], [1, 0], [0, 0]]))
        );
        assert_eq!(OwnLayout::Strand { reversed: true }.grid(0), None);
    }

    #[test]
    fn cells_that_no_longer_fit_the_pixels_are_passed_over() {
        let cells = |cells: Vec<[u32; 2]>| OwnLayout::Cells {
            columns: 2,
            rows: 2,
            cells,
        };
        let fits = cells(vec![[0, 0], [1, 1], [1, 1]]);
        assert_eq!(fits.grid(3), Some((2, 2, vec![[0, 0], [1, 1], [1, 1]])));
        // The prop has gained or lost a pixel since.
        assert_eq!(fits.grid(4), None);
        assert_eq!(fits.grid(2), None);
        // A cell outside the grid.
        assert_eq!(cells(vec![[0, 0], [2, 0]]).grid(2), None);
        assert_eq!(cells(vec![[0, 0], [0, 2]]).grid(2), None);
        assert_eq!(cells(Vec::new()).grid(0), None);
    }
}
