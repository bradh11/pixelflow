//! Per-controller output plans built from a show and its channel map.

use crate::lut::build_lut;
use pf_mapping::{Addressing, ChannelMap, UniverseSpan};
use pf_model::{ColorOrder, ControllerId, Show};
use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs};

/// Standard sACN (E1.31) UDP port.
pub const SACN_PORT: u16 = 5568;
/// Standard DDP UDP port.
pub const DDP_PORT: u16 = 4048;

/// DDP data type for 8-bit RGB pixels.
const DDP_TYPE_RGB24: u8 = 0x0B;
/// DDP data type for 8-bit RGBW pixels.
const DDP_TYPE_RGBW32: u8 = 0x1B;

/// One run of prop pixels to copy from the frame into a controller's channel buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatherSpan {
    pub frame_offset: usize,
    pub controller_channel: usize,
    pub pixels: u32,
    pub channels_per_pixel: u8,
    pub reverse: bool,
    /// Wire channel `j` takes canonical (RGB/RGBW) channel `order[j]`.
    pub order: [u8; 4],
    /// Index into [`OutputPlan::luts`].
    pub lut: usize,
}

/// How a controller's channels are addressed on the wire.
#[derive(Debug, Clone, PartialEq)]
pub enum Wire {
    Sacn {
        universes: Vec<UniverseSpan>,
        multicast: bool,
    },
    Ddp {
        data_type: u8,
    },
}

/// Everything needed to send one controller's channels.
#[derive(Debug, Clone, PartialEq)]
pub struct ControllerPlan {
    pub id: ControllerId,
    pub name: String,
    /// Unicast destination (sACN unicast or DDP), or why it could not be resolved.
    /// Multicast sACN does not use it.
    pub destination: Result<SocketAddr, String>,
    pub channel_count: usize,
    pub spans: Vec<GatherSpan>,
    pub wire: Wire,
}

/// The output plan for a whole show.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputPlan {
    pub frame_len: usize,
    pub frame_rate: u16,
    pub controllers: Vec<ControllerPlan>,
    /// Distinct brightness/gamma lookup tables referenced by [`GatherSpan::lut`].
    pub luts: Vec<[u8; 256]>,
}

/// Wire channel order for a color order: wire channel `j` carries canonical channel `order[j]`.
pub fn wire_order(order: ColorOrder) -> [u8; 4] {
    match order {
        ColorOrder::Rgb | ColorOrder::Rgbw => [0, 1, 2, 3],
        ColorOrder::Rbg => [0, 2, 1, 3],
        ColorOrder::Grb | ColorOrder::Grbw => [1, 0, 2, 3],
        ColorOrder::Gbr => [1, 2, 0, 3],
        ColorOrder::Brg => [2, 0, 1, 3],
        ColorOrder::Bgr => [2, 1, 0, 3],
    }
}

/// Builds the output plan. Controller addresses are resolved here, once; a controller
/// whose address cannot be resolved keeps the reason in [`ControllerPlan::destination`].
pub fn build_plan(show: &Show, map: &ChannelMap) -> OutputPlan {
    let mut luts: Vec<[u8; 256]> = Vec::new();
    let mut lut_index: HashMap<(u8, u32), usize> = HashMap::new();
    let controllers = show
        .controllers
        .iter()
        .zip(&map.controllers)
        .map(|(controller, output)| {
            let spans: Vec<GatherSpan> = output
                .spans
                .iter()
                .map(|span| {
                    let key = (span.brightness, span.gamma.to_bits());
                    let lut = *lut_index.entry(key).or_insert_with(|| {
                        luts.push(build_lut(span.brightness, span.gamma));
                        luts.len() - 1
                    });
                    GatherSpan {
                        frame_offset: span.frame_offset,
                        controller_channel: span.controller_channel,
                        pixels: span.pixels,
                        channels_per_pixel: span.channels_per_pixel,
                        reverse: span.reverse,
                        order: wire_order(span.color_order),
                        lut,
                    }
                })
                .collect();
            let (wire, port) = match &output.addressing {
                Addressing::Sacn { universes, multicast } => (
                    Wire::Sacn {
                        universes: universes.clone(),
                        multicast: *multicast,
                    },
                    SACN_PORT,
                ),
                Addressing::Ddp => {
                    let all_rgbw = !spans.is_empty() && spans.iter().all(|s| s.channels_per_pixel == 4);
                    let data_type = if all_rgbw { DDP_TYPE_RGBW32 } else { DDP_TYPE_RGB24 };
                    (Wire::Ddp { data_type }, DDP_PORT)
                }
            };
            ControllerPlan {
                id: controller.id,
                name: controller.name.clone(),
                destination: resolve(&controller.address, port),
                channel_count: output.channel_count,
                spans,
                wire,
            }
        })
        .collect();
    OutputPlan {
        frame_len: map.frame_len,
        frame_rate: show.settings.frame_rate,
        controllers,
        luts,
    }
}

/// Resolves `ip`, `ip:port`, `host`, or `host:port` to an IPv4 socket address.
fn resolve(address: &str, default_port: u16) -> Result<SocketAddr, String> {
    if let Ok(addr) = address.parse::<SocketAddr>() {
        return Ok(addr);
    }
    let found = if address.contains(':') {
        address.to_socket_addrs()
    } else {
        (address, default_port).to_socket_addrs()
    };
    found
        .map_err(|e| format!("could not resolve '{address}': {e}"))?
        .find(SocketAddr::is_ipv4)
        .ok_or_else(|| format!("'{address}' has no IPv4 address"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::{Controller, Generator, Port, PortSlot, Prop, Protocol, SacnConfig, ShapeSource};

    fn prop(nodes: u32, order: ColorOrder) -> Prop {
        let mut p = Prop::new(
            "P",
            ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
        );
        p.color_order = order;
        p
    }

    #[test]
    fn wire_orders_permute_canonical_channels() {
        assert_eq!(wire_order(ColorOrder::Grb), [1, 0, 2, 3]);
        assert_eq!(wire_order(ColorOrder::Bgr), [2, 1, 0, 3]);
        assert_eq!(wire_order(ColorOrder::Grbw), [1, 0, 2, 3]);
    }

    #[test]
    fn resolves_ips_ports_and_reports_failures() {
        assert_eq!(resolve("10.0.0.5", 4048), Ok("10.0.0.5:4048".parse().unwrap()));
        assert_eq!(
            resolve("10.0.0.5:9000", 4048),
            Ok("10.0.0.5:9000".parse().unwrap())
        );
        assert_eq!(resolve("localhost", 5568).map(|a| a.port()), Ok(5568));
        assert!(resolve("no-such-host.invalid", 4048).is_err());
    }

    #[test]
    fn plans_share_luts_and_pick_ddp_data_type() {
        let mut show = Show::new("t");
        let rgb = prop(2, ColorOrder::Grb);
        let rgbw = prop(2, ColorOrder::Grbw);
        let mut ddp = Controller::new("D", "10.0.0.9", Protocol::Ddp);
        let mut port = Port::new(1);
        port.slots = vec![PortSlot::new(rgbw.id)];
        ddp.ports = vec![port];
        let mut sacn = Controller::new("S", "10.0.0.8", Protocol::Sacn(SacnConfig::default()));
        let mut port = Port::new(1);
        let mut dim = PortSlot::new(rgb.id);
        dim.brightness = Some(50);
        port.slots = vec![PortSlot::new(rgb.id), dim];
        sacn.ports = vec![port];
        show.props = vec![rgb, rgbw];
        show.controllers = vec![ddp, sacn];
        let (map, _) = pf_mapping::map_show(&show);

        let plan = build_plan(&show, &map);
        assert_eq!(plan.frame_rate, 40);
        assert_eq!(plan.frame_len, 2 * 3 + 2 * 4);
        assert_eq!(plan.luts.len(), 2);
        assert_eq!(plan.controllers[0].wire, Wire::Ddp { data_type: 0x1B });
        assert_eq!(
            plan.controllers[0].destination,
            Ok("10.0.0.9:4048".parse().unwrap())
        );
        let sacn = &plan.controllers[1];
        assert_eq!(sacn.destination, Ok("10.0.0.8:5568".parse().unwrap()));
        assert_eq!(sacn.spans[0].order, [1, 0, 2, 3]);
        assert_ne!(sacn.spans[0].lut, sacn.spans[1].lut);
        assert!(matches!(&sacn.wire, Wire::Sacn { universes, multicast: false } if universes.len() == 1));
    }
}
