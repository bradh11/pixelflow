//! Controllers, their ports, and the props wired to each port.

use crate::{ControllerId, NodeRange, PropId};
use serde::{Deserialize, Serialize};

/// Which device adapter manages the controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum AdapterKind {
    Fpp,
    Falcon,
    Wled,
    #[default]
    Generic,
}

/// Channels carried by each sACN universe: any number from 1 to 512 (510, exactly 170 RGB
/// pixels, unless the controller says otherwise). Serialized as a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u16")]
pub struct UniverseSize(u16);

impl UniverseSize {
    /// The most channels an sACN universe carries.
    pub const MAX: u16 = 512;
    /// 510 channels: exactly 170 RGB pixels, so RGB pixels never straddle universes.
    pub const CHANNELS_510: UniverseSize = UniverseSize(510);
    pub const CHANNELS_512: UniverseSize = UniverseSize(Self::MAX);

    /// `None` unless `channels` is 1–512.
    pub fn new(channels: u32) -> Option<UniverseSize> {
        u16::try_from(channels)
            .ok()
            .filter(|c| (1..=Self::MAX).contains(c))
            .map(UniverseSize)
    }

    pub fn channels(self) -> u16 {
        self.0
    }
}

impl Default for UniverseSize {
    fn default() -> Self {
        Self::CHANNELS_510
    }
}

impl TryFrom<u32> for UniverseSize {
    type Error = String;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        UniverseSize::new(value).ok_or_else(|| {
            format!("A universe carries 1 to 512 channels, so {value} channels per universe won't work.")
        })
    }
}

impl From<UniverseSize> for u16 {
    fn from(size: UniverseSize) -> u16 {
        size.channels()
    }
}

#[cfg(feature = "schema")]
impl schemars::JsonSchema for UniverseSize {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "UniverseSize".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "Channels carried by each sACN universe, 1–512 (510 is exactly 170 RGB pixels).",
            "type": "integer",
            "minimum": 1,
            "maximum": 512
        })
    }
}

/// sACN (E1.31) output settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Protocol {
    Sacn(SacnConfig),
    Ddp,
}

/// A prop (or a segment of one) wired to a port, in wiring order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct Port {
    /// Physical port number as printed on the controller (1-based).
    pub number: u16,
    /// Most pixels (including null pixels) the port can drive, if known. Counted as boards count
    /// it, in RGB pixels (three channels each), so an RGBW pixel uses 1⅓. When the port feeds
    /// smart receivers, they share this one limit (as in xLights).
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

/// Where a controller's data sits in a rendered sequence (`.fseq`): channels
/// `start..start + count`, counting from 1. Known when the controller was added from an FPP's
/// output list (or, later, an xLights import); used to play sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct SequenceChannels {
    pub start: u32,
    pub count: u32,
    /// The FPP sends this controller's DDP packets with absolute channel numbers (its output is
    /// set to "DDP Raw Channel Numbers"), so the first packet's offset is `start - 1`, not 0.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub raw_ddp_offsets: bool,
}

/// A pixel controller on the network.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    #[serde(default)]
    pub sequence_channels: Option<SequenceChannels>,
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
            sequence_channels: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn universe_size_serializes_as_number_and_rejects_other_values() {
        assert_eq!(serde_json::to_string(&UniverseSize::CHANNELS_512).unwrap(), "512");
        assert_eq!(
            serde_json::from_str::<UniverseSize>("510").unwrap(),
            UniverseSize::CHANNELS_510
        );
        for size in [1, 15, 270, 426, 512] {
            let parsed = serde_json::from_str::<UniverseSize>(&size.to_string()).unwrap();
            assert_eq!(parsed.channels(), size);
            assert_eq!(serde_json::to_string(&parsed).unwrap(), size.to_string());
        }
        for size in ["0", "513", "70000"] {
            let err = serde_json::from_str::<UniverseSize>(size)
                .unwrap_err()
                .to_string();
            assert!(
                err.contains(&format!(
                    "A universe carries 1 to 512 channels, so {size} channels"
                )),
                "{err}"
            );
        }
        assert!(serde_json::from_str::<UniverseSize>("-1").is_err());
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
