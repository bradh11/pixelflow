//! Props (physical light elements) and groups of props.

use crate::{ColorOrder, GroupId, PropId, Region, RegionId, RegionRef, ShapeSource, Transform};
use serde::{Deserialize, Serialize};

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
}

impl Group {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: GroupId::new(),
            name: name.into(),
            members: Vec::new(),
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
    }
}
