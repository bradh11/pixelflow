//! Controllers, their ports, and the props wired to each port.

use crate::{ControllerId, NodeRange, PropId};
use serde::{Deserialize, Serialize};

/// Which device adapter manages the controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AdapterKind {
    Fpp,
    Wled,
    #[default]
    Generic,
}

/// Channels carried by each sACN universe. Serialized as the number `510` or `512`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub enum UniverseSize {
    /// 510 channels: exactly 170 RGB pixels, so RGB pixels never straddle universes.
    #[default]
    Channels510,
    Channels512,
}

impl UniverseSize {
    pub fn channels(self) -> u16 {
        match self {
            UniverseSize::Channels510 => 510,
            UniverseSize::Channels512 => 512,
        }
    }
}

impl TryFrom<u16> for UniverseSize {
    type Error = String;
    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            510 => Ok(UniverseSize::Channels510),
            512 => Ok(UniverseSize::Channels512),
            other => Err(format!("universe size must be 510 or 512, got {other}")),
        }
    }
}

impl From<UniverseSize> for u16 {
    fn from(size: UniverseSize) -> u16 {
        size.channels()
    }
}

/// sACN (E1.31) output settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SacnConfig {
    /// Pinned first universe. `None` lets PixelFlow assign universes automatically.
    #[serde(default)]
    pub start_universe: Option<u16>,
    #[serde(default)]
    pub universe_size: UniverseSize,
    /// When false, a pixel's channels are never split across two universes.
    #[serde(default)]
    pub allow_pixel_straddle: bool,
    #[serde(default)]
    pub multicast: bool,
}

/// Network protocol a controller receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Protocol {
    Sacn(SacnConfig),
    Ddp,
}

/// A prop (or a segment of one) wired to a port, in wiring order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortSlot {
    pub prop: PropId,
    /// Nodes of the prop on this slot. `None` means the whole prop.
    #[serde(default)]
    pub segment: Option<NodeRange>,
    /// Unused physical pixels before this slot's first node.
    #[serde(default)]
    pub null_pixels: u32,
    /// Pixels run from the segment's last node to its first.
    #[serde(default)]
    pub reverse: bool,
    /// Brightness override in percent (0–100).
    #[serde(default)]
    pub brightness: Option<u8>,
    #[serde(default)]
    pub gamma: Option<f32>,
    /// Smart/differential receiver index, when the port feeds receivers.
    #[serde(default)]
    pub smart_receiver: Option<u8>,
}

impl PortSlot {
    /// A slot carrying the whole prop with no overrides.
    pub fn new(prop: PropId) -> Self {
        Self {
            prop,
            segment: None,
            null_pixels: 0,
            reverse: false,
            brightness: None,
            gamma: None,
            smart_receiver: None,
        }
    }

    /// The prop nodes this slot carries, given the prop's node count.
    pub fn node_range(&self, prop_nodes: u32) -> NodeRange {
        self.segment.unwrap_or(NodeRange::new(0, prop_nodes))
    }
}

/// A physical output port on a controller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Port {
    /// Physical port number as printed on the controller (1-based).
    pub number: u16,
    /// Most pixels (including null pixels) the port can drive, if known.
    #[serde(default)]
    pub max_pixels: Option<u32>,
    /// Brightness in percent (0–100).
    #[serde(default = "default_brightness")]
    pub brightness: u8,
    #[serde(default = "default_gamma")]
    pub gamma: f32,
    #[serde(default)]
    pub slots: Vec<PortSlot>,
}

fn default_brightness() -> u8 {
    100
}

fn default_gamma() -> f32 {
    1.0
}

impl Port {
    pub fn new(number: u16) -> Self {
        Self {
            number,
            max_pixels: None,
            brightness: default_brightness(),
            gamma: default_gamma(),
            slots: Vec::new(),
        }
    }
}

/// A pixel controller on the network.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Controller {
    pub id: ControllerId,
    pub name: String,
    /// IP address or hostname.
    pub address: String,
    #[serde(default)]
    pub adapter: AdapterKind,
    pub protocol: Protocol,
    #[serde(default)]
    pub ports: Vec<Port>,
}

impl Controller {
    pub fn new(name: impl Into<String>, address: impl Into<String>, protocol: Protocol) -> Self {
        Self {
            id: ControllerId::new(),
            name: name.into(),
            address: address.into(),
            adapter: AdapterKind::default(),
            protocol,
            ports: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn universe_size_serializes_as_number_and_rejects_other_values() {
        assert_eq!(serde_json::to_string(&UniverseSize::Channels512).unwrap(), "512");
        assert_eq!(
            serde_json::from_str::<UniverseSize>("510").unwrap(),
            UniverseSize::Channels510
        );
        assert!(serde_json::from_str::<UniverseSize>("500").is_err());
    }

    #[test]
    fn protocol_json_shapes() {
        let sacn = Protocol::Sacn(SacnConfig {
            start_universe: Some(10),
            ..SacnConfig::default()
        });
        let json = serde_json::to_value(sacn).unwrap();
        assert_eq!(json["type"], "sacn");
        assert_eq!(json["startUniverse"], 10);
        assert_eq!(json["universeSize"], 510);
        assert_eq!(serde_json::to_value(Protocol::Ddp).unwrap()["type"], "ddp");
        assert_eq!(serde_json::from_value::<Protocol>(json).unwrap(), sacn);
    }

    #[test]
    fn slot_node_range_defaults_to_whole_prop() {
        let mut slot = PortSlot::new(PropId::new());
        assert_eq!(slot.node_range(50), NodeRange::new(0, 50));
        slot.segment = Some(NodeRange::new(10, 20));
        assert_eq!(slot.node_range(50), NodeRange::new(10, 20));
    }

    #[test]
    fn port_defaults_when_fields_missing() {
        let port: Port = serde_json::from_str(r#"{ "number": 3 }"#).unwrap();
        assert_eq!(port, Port::new(3));
    }
}
