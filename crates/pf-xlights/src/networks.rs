//! Controllers from `xlights_networks.xml`, with the absolute channel ranges xLights gives them.
//!
//! xLights numbers channels cumulatively in file order: each controller's outputs (universes, or
//! one DDP block) follow the previous controller's, using each output's `MaxChannels`.

use crate::error::XlightsError;
use roxmltree::{Document, Node};

/// One output of a controller: an sACN/Art-Net universe or a DDP block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XOutput {
    /// Universe number for E1.31/Art-Net (0 for DDP and others).
    pub universe: u32,
    /// First absolute channel (1-based).
    pub start: u32,
    pub channels: u32,
}

/// A controller and its absolute channel range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XController {
    pub name: String,
    /// IP address (empty for serial and null controllers).
    pub ip: String,
    /// Output protocol as xLights names it: "DDP", "E131", "ArtNet", "Null", "DMX", …
    pub protocol: String,
    /// `Ethernet`, `Serial`, or `Null`.
    pub kind: String,
    /// False when xLights has the controller set to inactive.
    pub active: bool,
    /// DDP packets carry absolute channel numbers (`KeepChannelNumbers`).
    pub keep_channel_numbers: bool,
    pub outputs: Vec<XOutput>,
}

impl XController {
    /// First absolute channel (1-based).
    pub fn start(&self) -> u32 {
        self.outputs.first().map_or(1, |o| o.start)
    }

    /// Channels across all outputs.
    pub fn channels(&self) -> u32 {
        self.outputs
            .iter()
            .map(|o| o.channels)
            .fold(0u32, u32::saturating_add)
    }

    /// Whether the absolute channel `channel` (1-based) belongs to this controller.
    pub fn contains(&self, channel: u32) -> bool {
        let start = self.start();
        channel >= start && u64::from(channel) < u64::from(start) + u64::from(self.channels())
    }
}

fn attr<'a>(node: Node<'a, '_>, key: &str) -> &'a str {
    node.attribute(key).unwrap_or("").trim()
}

fn number(node: Node<'_, '_>, key: &str) -> u32 {
    attr(node, key)
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map_or(0, |v| v.min(f64::from(u32::MAX)) as u32)
}

/// Reads every controller in file order and assigns absolute channels.
pub fn parse_networks(xml: &str) -> Result<Vec<XController>, XlightsError> {
    let doc =
        Document::parse(xml).map_err(|e| XlightsError::BadFile("xlights_networks.xml", e.to_string()))?;
    let root = doc.root_element();
    let mut controllers = Vec::new();
    let mut next = 1u32;
    let mut add_output = |universe: u32, channels: u32| {
        let output = XOutput {
            universe,
            start: next,
            channels,
        };
        next = next.saturating_add(channels);
        output
    };
    for child in root.children().filter(Node::is_element) {
        match child.tag_name().name() {
            "Controller" => {
                let kind = attr(child, "Type");
                if !matches!(kind, "Ethernet" | "Serial" | "Null") {
                    continue;
                }
                let active = match child.attribute("ActiveState") {
                    Some(state) => state.trim() != "Inactive",
                    None => attr(child, "Active") != "0",
                };
                let networks: Vec<Node> = child
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "network")
                    .collect();
                let protocol = match attr(child, "Protocol") {
                    "" => networks.first().map_or("", |n| attr(*n, "NetworkType")),
                    p => p,
                };
                let ip = match attr(child, "IP") {
                    "" if kind == "Ethernet" => networks.first().map_or("", |n| attr(*n, "ComPort")),
                    "" => "",
                    ip => ip,
                };
                let outputs = networks
                    .iter()
                    .map(|n| add_output(number(*n, "BaudRate"), number(*n, "MaxChannels")))
                    .collect();
                controllers.push(XController {
                    name: attr(child, "Name").to_string(),
                    ip: ip.to_string(),
                    protocol: protocol.to_string(),
                    kind: kind.to_string(),
                    active,
                    keep_channel_numbers: networks.iter().any(|n| attr(*n, "KeepChannelNumbers") == "1"),
                    outputs,
                });
            }
            "network" => {
                // Legacy format: outputs directly under the root. Consecutive universes of the same
                // type and address form one controller, as xLights' converter does.
                let protocol = attr(child, "NetworkType").to_string();
                let ip = attr(child, "ComPort").to_string();
                let universe = number(child, "BaudRate");
                let count = number(child, "NumUniverses").max(1);
                let channels = number(child, "MaxChannels");
                let joins = controllers.last().is_some_and(|c: &XController| {
                    c.kind == "Legacy"
                        && c.protocol == protocol
                        && c.ip == ip
                        && c.outputs
                            .last()
                            .is_some_and(|o| o.universe.checked_add(1) == Some(universe))
                });
                let outputs: Vec<XOutput> = (0..count.min(64_000))
                    .map(|i| add_output(universe.saturating_add(i), channels))
                    .collect();
                if joins {
                    controllers.last_mut().expect("checked").outputs.extend(outputs);
                } else {
                    let name = match attr(child, "Description") {
                        "" => format!("{protocol} {ip}"),
                        d => d.to_string(),
                    };
                    controllers.push(XController {
                        name,
                        ip,
                        protocol,
                        kind: "Legacy".to_string(),
                        active: attr(child, "Enabled") != "No",
                        keep_channel_numbers: false,
                        outputs,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(controllers)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETWORKS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Networks>
  <Controller Id="1" Name="Falcon_F16V5_B9F5" Type="Ethernet" IP="192.0.2.20" Protocol="DDP" Vendor="Falcon" Model="F16V5" ActiveState="Active">
    <network NetworkType="DDP" ComPort="192.0.2.20" BaudRate="1" MaxChannels="6147" KeepChannelNumbers="0"/>
  </Controller>
  <Controller Id="2" Name="Arches" Type="Ethernet" IP="192.0.2.30" Protocol="E131" ActiveState="Active">
    <network NetworkType="E131" ComPort="192.0.2.30" BaudRate="10" MaxChannels="512"/>
    <network NetworkType="E131" ComPort="192.0.2.30" BaudRate="11" MaxChannels="512"/>
  </Controller>
  <Controller Id="3" Name="Spare" Type="Null" ActiveState="Inactive">
    <network NetworkType="NULL" MaxChannels="100"/>
  </Controller>
</Networks>"#;

    #[test]
    fn channels_are_cumulative_in_file_order() {
        let controllers = parse_networks(NETWORKS).unwrap();
        let summary: Vec<_> = controllers
            .iter()
            .map(|c| {
                (
                    c.name.as_str(),
                    c.protocol.as_str(),
                    c.start(),
                    c.channels(),
                    c.active,
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("Falcon_F16V5_B9F5", "DDP", 1, 6147, true),
                ("Arches", "E131", 6148, 1024, true),
                ("Spare", "NULL", 7172, 100, false),
            ]
        );
        assert_eq!(
            controllers[1].outputs[1],
            XOutput {
                universe: 11,
                start: 6660,
                channels: 512
            }
        );
        assert!(controllers[0].contains(6147) && !controllers[0].contains(6148));
        assert_eq!(controllers[0].ip, "192.0.2.20");
    }

    #[test]
    fn legacy_networks_group_consecutive_universes() {
        let xml = r#"<Networks>
            <network NetworkType="E131" ComPort="192.0.2.5" BaudRate="1" MaxChannels="510" NumUniverses="2" Description="Porch"/>
            <network NetworkType="E131" ComPort="192.0.2.5" BaudRate="3" MaxChannels="510"/>
            <network NetworkType="E131" ComPort="192.0.2.6" BaudRate="1" MaxChannels="510"/>
        </Networks>"#;
        let controllers = parse_networks(xml).unwrap();
        assert_eq!(controllers.len(), 2);
        assert_eq!(
            (controllers[0].name.as_str(), controllers[0].outputs.len()),
            ("Porch", 3)
        );
        assert_eq!(controllers[1].start(), 1531);
    }

    #[test]
    fn huge_universe_numbers_dont_overflow() {
        let xml = r#"<Networks>
            <network NetworkType="E131" ComPort="192.0.2.5" BaudRate="4294967295" MaxChannels="510" NumUniverses="2"/>
            <network NetworkType="E131" ComPort="192.0.2.5" BaudRate="99999999999" MaxChannels="510"/>
        </Networks>"#;
        let controllers = parse_networks(xml).unwrap();
        let universes: Vec<u32> = controllers
            .iter()
            .flat_map(|c| c.outputs.iter().map(|o| o.universe))
            .collect();
        assert_eq!(universes, [u32::MAX, u32::MAX, u32::MAX]);
    }

    #[test]
    fn broken_files_are_reported() {
        assert!(matches!(
            parse_networks("<Networks"),
            Err(XlightsError::BadFile(..))
        ));
    }
}
