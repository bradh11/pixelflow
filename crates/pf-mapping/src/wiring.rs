//! Walks controller → port → slot to place prop pixels on controller channels.

use crate::layout::{FixtureLayout, OutputSpan, PropLayout};
use crate::universes::PixelRun;
use pf_model::{ColorOrder, Controller, Issue, IssueCode, PropId, Protocol, Show, ValidationReport};
use std::collections::HashMap;

/// Channels in a DMX universe.
pub const DMX_UNIVERSE: usize = 512;

/// Wiring result for one controller.
#[derive(Debug, Clone, Default)]
pub(crate) struct WiredController {
    pub spans: Vec<OutputSpan>,
    pub channel_count: usize,
    /// Every physical pixel in channel order, including null pixels.
    pub runs: Vec<PixelRun>,
    /// A DMX controller's universes (one a port, 512 channels each); `None` for a pixel
    /// controller.
    pub dmx_universes: Option<usize>,
}

pub(crate) fn wire_controllers(
    show: &Show,
    props: &[PropLayout],
    fixtures: &[Option<FixtureLayout>],
    report: &mut ValidationReport,
) -> Vec<WiredController> {
    // First occurrence wins, matching `Show::prop`.
    let mut index: HashMap<PropId, usize> = HashMap::with_capacity(props.len());
    for (i, p) in props.iter().enumerate() {
        index.entry(p.prop).or_insert(i);
    }
    let mut coverage: Vec<Vec<u8>> = props.iter().map(|p| vec![0; p.nodes as usize]).collect();
    let wired = show
        .controllers
        .iter()
        .map(|c| {
            if c.is_dmx() {
                wire_dmx_controller(show, c, fixtures, &index, &mut coverage, report)
            } else {
                wire_controller(show, c, props, fixtures, &index, &mut coverage, report)
            }
        })
        .collect();
    check_coverage(show, &coverage, report);
    wired
}

fn wire_controller(
    show: &Show,
    controller: &Controller,
    props: &[PropLayout],
    fixtures: &[Option<FixtureLayout>],
    index: &HashMap<PropId, usize>,
    coverage: &mut [Vec<u8>],
    report: &mut ValidationReport,
) -> WiredController {
    let mut out = WiredController::default();
    for port in &controller.ports {
        let mut load = PortLoad::default();
        for slot in &port.slots {
            let Some(&i) = index.get(&slot.prop) else {
                continue;
            };
            let prop = &show.props[i];
            if prop.dmx.is_some() {
                if fixtures[i].is_some() {
                    wrong_kind(report, &prop.name, controller, true);
                    coverage[i].fill(1);
                }
                continue;
            }
            let layout = &props[i];
            let range = slot.node_range(layout.nodes);
            if !range.fits_within(layout.nodes) {
                continue;
            }
            let cpp = layout.channels_per_pixel;
            if slot.null_pixels > 0 {
                out.runs.push(PixelRun {
                    pixels: slot.null_pixels,
                    channels_per_pixel: cpp,
                });
                out.channel_count += slot.null_pixels as usize * cpp as usize;
            }
            if !range.is_empty() {
                let span = OutputSpan {
                    prop: prop.id,
                    port: port.number,
                    controller_channel: out.channel_count,
                    frame_offset: layout.frame_offset + range.start as usize * cpp as usize,
                    pixels: range.len(),
                    channels_per_pixel: cpp,
                    reverse: slot.reverse,
                    color_order: prop.color_order,
                    brightness: slot.brightness.unwrap_or(port.brightness),
                    gamma: slot.gamma.unwrap_or(port.gamma),
                };
                out.channel_count += span.byte_len();
                out.spans.push(span);
                out.runs.push(PixelRun {
                    pixels: range.len(),
                    channels_per_pixel: cpp,
                });
                for node in range.start..range.end {
                    let hits = &mut coverage[i][node as usize];
                    *hits = hits.saturating_add(1);
                }
            }
            let pixels = u64::from(slot.null_pixels) + u64::from(range.len());
            load.channels += pixels * u64::from(cpp);
            load.wide |= pixels > 0 && cpp > 3;
            if let Some(r) = slot.smart_receiver
                && !load.receivers.contains(&r)
            {
                load.receivers.push(r);
            }
        }
        if let Some(max) = port.max_pixels {
            check_capacity(controller, port.number, max, &load, report);
        }
    }
    out
}

/// A DMX controller: each port one universe of 512 channels, each fixture at its address.
/// Fixtures may share channels (a warning: the later one's are sent); pixel props aren't sent.
fn wire_dmx_controller(
    show: &Show,
    controller: &Controller,
    fixtures: &[Option<FixtureLayout>],
    index: &HashMap<PropId, usize>,
    coverage: &mut [Vec<u8>],
    report: &mut ValidationReport,
) -> WiredController {
    let usb = controller.protocol == Protocol::DmxUsb;
    let ports = if usb {
        controller.ports.len().min(1)
    } else {
        controller.ports.len()
    };
    if usb && controller.ports.len() > 1 {
        report.push(
            Issue::error(
                IssueCode::DmxUniverses,
                format!(
                    "'{}' is a USB DMX interface, which sends one universe, but it has {} ports: only port {} is sent.",
                    controller.name,
                    controller.ports.len(),
                    controller.ports[0].number
                ),
            )
            .with_fix("Move the fixtures to the first port, then remove the others."),
        );
    }
    let mut out = WiredController {
        channel_count: ports * DMX_UNIVERSE,
        runs: vec![PixelRun {
            pixels: (ports * DMX_UNIVERSE) as u32,
            channels_per_pixel: 1,
        }],
        dmx_universes: Some(ports),
        ..WiredController::default()
    };
    for (n, port) in controller.ports.iter().take(ports).enumerate() {
        let base = n * DMX_UNIVERSE;
        // Channels used on this universe so far, and by which fixture.
        let mut used: Vec<(usize, usize, &str)> = Vec::new();
        for slot in &port.slots {
            let Some(&i) = index.get(&slot.prop) else {
                continue;
            };
            let prop = &show.props[i];
            let Some(fixture) = &fixtures[i] else {
                if prop.dmx.is_none() {
                    wrong_kind(report, &prop.name, controller, false);
                    coverage[i].fill(1);
                }
                continue;
            };
            let start = usize::from(fixture.address) - 1;
            let end = start + fixture.footprint();
            if let Some(&(_, _, other)) = used.iter().find(|&&(s, e, _)| start < e && s < end) {
                report.push(
                    Issue::warning(
                        IssueCode::DmxChannelsOverlap,
                        format!(
                            "'{}' and '{other}' share channels on port {} of '{}', so the later one's values are sent there.",
                            prop.name, port.number, controller.name
                        ),
                    )
                    .with_fix("Give one of them a start address after the other's last channel."),
                );
            }
            used.push((start, end, &prop.name));
            out.spans.push(OutputSpan {
                prop: prop.id,
                port: port.number,
                controller_channel: base + start,
                frame_offset: fixture.dmx_offset,
                pixels: fixture.footprint() as u32,
                channels_per_pixel: 1,
                reverse: false,
                color_order: ColorOrder::Rgb,
                brightness: 100,
                gamma: 1.0,
            });
            for hits in coverage[i].iter_mut() {
                *hits = hits.saturating_add(1);
            }
        }
    }
    // Spans in channel order; fixtures sharing channels keep their wiring order, so the later
    // one is copied last.
    out.spans.sort_by_key(|s| s.controller_channel);
    if ports == 0 {
        report.push(
            Issue::warning(
                IssueCode::DmxUniverses,
                format!(
                    "The DMX controller '{}' has no universes, so nothing is sent to it.",
                    controller.name
                ),
            )
            .with_fix("Add a universe (a port) to it on the Wiring screen."),
        );
    }
    out
}

/// A fixture on a pixel controller, or a pixel prop on a DMX one: not sent.
fn wrong_kind(report: &mut ValidationReport, prop: &str, controller: &Controller, fixture: bool) {
    let (message, fix) = if fixture {
        (
            format!(
                "The DMX fixture '{prop}' is wired to '{}', a pixel controller, so it isn't sent.",
                controller.name
            ),
            "Wire it to a DMX controller (a DMX node or a USB DMX interface).",
        )
    } else {
        (
            format!(
                "The prop '{prop}' is wired to '{}', a DMX controller, so it isn't sent: DMX controllers drive fixtures.",
                controller.name
            ),
            "Wire it to a pixel controller.",
        )
    };
    report.push(Issue::warning(IssueCode::WrongControllerKind, message).with_fix(fix));
}

/// Pixels on one port, across any smart receivers it feeds.
#[derive(Default)]
struct PortLoad {
    channels: u64,
    /// Some pixels carry more than 3 channels (RGBW).
    wide: bool,
    /// The smart receivers the port feeds, in wiring order.
    receivers: Vec<u8>,
}

/// Port limits are counted the way the boards (and xLights) count them: in channels, three per
/// pixel, so an RGBW pixel takes the time of 1⅓ RGB pixels. Smart receivers on a port share the
/// port's one limit, as in xLights: their pixels are added up and checked together. Over the limit
/// is a warning, not an error: PixelFlow still sends every channel; the board just can't drive (or
/// refresh) them all.
fn check_capacity(
    controller: &Controller,
    port: u16,
    max: u32,
    load: &PortLoad,
    report: &mut ValidationReport,
) {
    let pixels = load.channels.div_ceil(3);
    if pixels <= u64::from(max) {
        return;
    }
    let shared = match load.receivers.as_slice() {
        [] => String::new(),
        [one] => format!(", on smart receiver {}", receiver_name(*one)),
        many => format!(
            ", shared by smart receivers {}",
            many.iter()
                .map(|r| receiver_name(*r))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let counting = if load.wide {
        ", counting each RGBW pixel as 1⅓ because it carries 4 channels"
    } else {
        ""
    };
    report.push(
        Issue::warning(
            IssueCode::PortOverCapacity,
            format!(
                "Port {port} on '{}' is over capacity by {} pixels ({pixels} of {max}{shared}{counting}).",
                controller.name,
                pixels - u64::from(max)
            ),
        )
        .with_fix("Move a prop to another port, or raise the port's pixel limit."),
    );
}

/// DDP tells the controller one pixel type (RGB or RGBW) for the whole stream, and some
/// controllers, like WLED, decode every pixel that way: a DDP controller wired to both kinds
/// shows one of them garbled. sACN carries plain channels, so it doesn't matter there.
pub(crate) fn check_ddp_pixel_types(show: &Show, wired: &[WiredController], report: &mut ValidationReport) {
    for (controller, wired) in show.controllers.iter().zip(wired) {
        if !matches!(controller.protocol, Protocol::Ddp) {
            continue;
        }
        let rgbw = wired.spans.iter().any(|s| s.channels_per_pixel == 4);
        let rgb = wired.spans.iter().any(|s| s.channels_per_pixel == 3);
        if rgb && rgbw {
            report.push(
                Issue::warning(
                    IssueCode::MixedPixelTypes,
                    format!(
                        "'{}' gets both RGB and RGBW props over DDP, which sends one pixel type for \
                         everything, so controllers like WLED will show some of them in the wrong colors.",
                        controller.name
                    ),
                )
                .with_fix("Wire the RGBW props to a controller of their own, or send this controller sACN."),
            );
        }
    }
}

/// Smart receivers are lettered on the boards: 1 is A, 2 is B, …
fn receiver_name(receiver: u8) -> String {
    match receiver {
        1..=26 => char::from(b'A' + receiver - 1).to_string(),
        other => other.to_string(),
    }
}

fn check_coverage(show: &Show, coverage: &[Vec<u8>], report: &mut ValidationReport) {
    for (prop, hits) in show.props.iter().zip(coverage) {
        let nodes = hits.len();
        let unassigned = hits.iter().filter(|&&h| h == 0).count();
        let doubled = hits.iter().filter(|&&h| h > 1).count();
        if nodes > 0 && unassigned == nodes {
            report.push(
                Issue::warning(
                    IssueCode::UnassignedNodes,
                    format!("The prop '{}' is not wired to any controller port.", prop.name),
                )
                .with_fix("Drag the prop onto a controller port."),
            );
        } else if unassigned > 0 {
            report.push(
                Issue::warning(
                    IssueCode::UnassignedNodes,
                    format!(
                        "{unassigned} of {nodes} pixels on '{}' are not wired to any port.",
                        prop.name
                    ),
                )
                .with_fix("Add a slot for the remaining pixels."),
            );
        }
        if doubled > 0 {
            report.push(
                Issue::error(
                    IssueCode::NodeAssignedTwice,
                    format!(
                        "{doubled} pixels on '{}' are wired to more than one port slot.",
                        prop.name
                    ),
                )
                .with_fix("Remove the duplicate slot or narrow its pixel range."),
            );
        }
    }
}
