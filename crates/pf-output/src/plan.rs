//! Per-controller output plans built from a show and its channel map.

use crate::lut::build_lut;
use pf_mapping::{Addressing, ChannelMap, UniverseSpan};
use pf_model::{ColorOrder, ControllerId, Protocol, Show};
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
        /// DDP offset of the controller's first channel (0 unless it expects raw channel numbers).
        offset_base: u32,
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
    plan_with(show, map, resolve)
}

/// Builds the output plan without looking up any addresses (no network or DNS): for turning
/// show frames into controller channels offline, such as exporting a sequence. Every
/// [`ControllerPlan::destination`] is an error saying it wasn't resolved.
pub fn build_offline_plan(show: &Show, map: &ChannelMap) -> OutputPlan {
    plan_with(show, map, |_, _| Err("not resolved (offline plan)".to_string()))
}

fn plan_with(
    show: &Show,
    map: &ChannelMap,
    resolve: impl Fn(&str, u16) -> Result<SocketAddr, String>,
) -> OutputPlan {
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
                    (
                        Wire::Ddp {
                            data_type,
                            offset_base: 0,
                        },
                        DDP_PORT,
                    )
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

/// A controller that receives a block of a rendered sequence's channels unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct PassthroughRoute {
    pub id: ControllerId,
    pub name: String,
    pub address: String,
    pub protocol: Protocol,
    /// First sequence channel (0-based) sent to this controller.
    pub start: usize,
    pub count: usize,
    /// DDP offset of this controller's first channel: 0 normally, or `start` when the controller
    /// expects raw channel numbers (FPP's "DDP Raw Channel Numbers" mode).
    pub ddp_offset_base: u32,
}

/// Builds an output plan that sends each route's block of the frame (a whole rendered sequence
/// frame, `frame_len` channels) to its controller as-is: no reordering, brightness, or gamma,
/// because the sequence was already rendered for the controllers.
pub fn build_passthrough_plan(routes: &[PassthroughRoute], frame_len: usize, frame_rate: u16) -> OutputPlan {
    let controllers = routes
        .iter()
        .map(|route| {
            let (wire, port) = match &route.protocol {
                Protocol::Ddp => (
                    Wire::Ddp {
                        data_type: DDP_TYPE_RGB24,
                        offset_base: route.ddp_offset_base,
                    },
                    DDP_PORT,
                ),
                Protocol::Sacn(sacn) => {
                    let size = usize::from(sacn.universe_size.channels());
                    let first = sacn.start_universe.unwrap_or(1);
                    let universes = (0..route.count.div_ceil(size))
                        .map(|i| UniverseSpan {
                            universe: first.saturating_add(u16::try_from(i).unwrap_or(u16::MAX)),
                            controller_channel: i * size,
                            len: u16::try_from(size.min(route.count - i * size)).unwrap_or(u16::MAX),
                        })
                        .collect();
                    (
                        Wire::Sacn {
                            universes,
                            multicast: sacn.multicast,
                        },
                        SACN_PORT,
                    )
                }
            };
            ControllerPlan {
                id: route.id,
                name: route.name.clone(),
                destination: resolve(&route.address, port),
                channel_count: route.count,
                spans: vec![GatherSpan {
                    frame_offset: route.start,
                    controller_channel: 0,
                    pixels: u32::try_from(route.count).unwrap_or(u32::MAX),
                    channels_per_pixel: 1,
                    reverse: false,
                    order: [0, 1, 2, 3],
                    lut: 0,
                }],
                wire,
            }
        })
        .collect();
    OutputPlan {
        frame_len,
        frame_rate,
        controllers,
        luts: vec![build_lut(100, 1.0)],
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
        // An invalid port fails without a DNS lookup.
        let error = resolve("bad:port", 4048).unwrap_err();
        assert!(error.starts_with("could not resolve"), "{error}");
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
        assert_eq!(
            plan.controllers[0].wire,
            Wire::Ddp {
                data_type: 0x1B,
                offset_base: 0
            }
        );
        assert_eq!(
            plan.controllers[0].destination,
            Ok("10.0.0.9:4048".parse().unwrap())
        );
        let sacn = &plan.controllers[1];
        assert_eq!(sacn.destination, Ok("10.0.0.8:5568".parse().unwrap()));
        assert_eq!(sacn.spans[0].order, [1, 0, 2, 3]);
        assert_ne!(sacn.spans[0].lut, sacn.spans[1].lut);
        assert!(matches!(&sacn.wire, Wire::Sacn { universes, multicast: false } if universes.len() == 1));

        // The offline plan is the same apart from addresses, which it never looks up.
        show.controllers[1].address = "no-such-host.invalid".into();
        let offline = build_offline_plan(&show, &map);
        assert_eq!(offline.luts, plan.luts);
        for (a, b) in offline.controllers.iter().zip(&plan.controllers) {
            assert_eq!(
                (&a.spans, &a.wire, a.channel_count),
                (&b.spans, &b.wire, b.channel_count)
            );
            assert_eq!(a.destination, Err("not resolved (offline plan)".to_string()));
        }
    }

    #[test]
    fn passthrough_plans_send_each_block_unchanged() {
        use pf_model::{SacnConfig, UniverseSize};
        let routes = [
            PassthroughRoute {
                id: ControllerId::new(),
                name: "Falcon".into(),
                address: "127.0.0.1".into(),
                protocol: Protocol::Ddp,
                start: 0,
                count: 6147,
                ddp_offset_base: 0,
            },
            PassthroughRoute {
                id: ControllerId::new(),
                name: "Arches".into(),
                address: "127.0.0.2".into(),
                protocol: Protocol::Sacn(SacnConfig {
                    start_universe: Some(10),
                    universe_size: UniverseSize::Channels512,
                    ..SacnConfig::default()
                }),
                start: 6147,
                count: 1100,
                ddp_offset_base: 0,
            },
        ];
        let plan = build_passthrough_plan(&routes, 7247, 20);
        assert_eq!((plan.frame_len, plan.frame_rate, plan.luts.len()), (7247, 20, 1));
        assert!(
            plan.luts[0].iter().enumerate().all(|(i, &v)| usize::from(v) == i),
            "identity LUT"
        );
        let falcon = &plan.controllers[0];
        assert_eq!(falcon.destination, Ok("127.0.0.1:4048".parse().unwrap()));
        assert_eq!((falcon.channel_count, falcon.spans[0].frame_offset), (6147, 0));
        let arches = &plan.controllers[1];
        assert_eq!(arches.spans[0].frame_offset, 6147);
        let Wire::Sacn { universes, .. } = &arches.wire else {
            panic!("sACN")
        };
        let spans: Vec<_> = universes
            .iter()
            .map(|u| (u.universe, u.controller_channel, u.len))
            .collect();
        assert_eq!(spans, vec![(10, 0, 512), (11, 512, 512), (12, 1024, 76)]);

        let mut out = vec![0u8; 1100];
        let frame: Vec<u8> = (0..7247).map(|i| (i % 251) as u8).collect();
        crate::render_controller(&frame, arches, &plan.luts, &mut out);
        assert_eq!(&out[..], &frame[6147..]);
    }

    #[test]
    fn raw_ddp_routes_carry_their_offset_base() {
        let route = PassthroughRoute {
            id: ControllerId::new(),
            name: "Raw".into(),
            address: "127.0.0.1".into(),
            protocol: Protocol::Ddp,
            start: 3000,
            count: 30,
            ddp_offset_base: 3000,
        };
        let plan = build_passthrough_plan(&[route], 4000, 20);
        assert_eq!(
            plan.controllers[0].wire,
            Wire::Ddp {
                data_type: DDP_TYPE_RGB24,
                offset_base: 3000
            }
        );
    }
}
