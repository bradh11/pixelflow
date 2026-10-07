//! Walks controller → port → slot to place prop pixels on controller channels.

use crate::layout::{OutputSpan, PropLayout};
use crate::universes::PixelRun;
use pf_model::{Controller, Issue, IssueCode, PropId, Show, ValidationReport};
use std::collections::HashMap;

/// Wiring result for one controller.
#[derive(Debug, Clone, Default)]
pub(crate) struct WiredController {
    pub spans: Vec<OutputSpan>,
    pub channel_count: usize,
    /// Every physical pixel in channel order, including null pixels.
    pub runs: Vec<PixelRun>,
}

pub(crate) fn wire_controllers(
    show: &Show,
    props: &[PropLayout],
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
        .map(|c| wire_controller(show, c, props, &index, &mut coverage, report))
        .collect();
    check_coverage(show, &coverage, report);
    wired
}

fn wire_controller(
    show: &Show,
    controller: &Controller,
    props: &[PropLayout],
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
