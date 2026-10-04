//! Props (physical light elements) and groups of props.

use crate::{ColorOrder, GroupId, PropId, Region, ShapeSource, Transform};
use serde::{Deserialize, Serialize};

/// A physical light element: an arch, a matrix, a tree, etc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

/// A named set of props.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    #[serde(default)]
    pub members: Vec<PropId>,
}

impl Group {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: GroupId::new(),
            name: name.into(),
            members: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Generator;

    #[test]
    fn channel_count_uses_color_order() {
        let mut prop = Prop::new(
            "Arch",
            ShapeSource::Generator(Generator::Arch {
                nodes: 50,
                width: 4.0,
                height: 2.0,
            }),
        );
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
