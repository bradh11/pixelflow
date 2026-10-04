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
    let index: HashMap<PropId, usize> = props.iter().enumerate().map(|(i, p)| (p.prop, i)).collect();
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
        let mut port_pixels: u64 = 0;
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
            port_pixels += u64::from(slot.null_pixels) + u64::from(range.len());
        }
        if let Some(max) = port.max_pixels
            && port_pixels > u64::from(max)
        {
            report.push(
                Issue::error(
                    IssueCode::PortOverCapacity,
                    format!(
                        "Port {} on '{}' is over capacity by {} pixels ({} of {}).",
                        port.number,
                        controller.name,
                        port_pixels - u64::from(max),
                        port_pixels,
                        max
                    ),
                )
                .with_fix("Move a prop to another port, or raise the port's pixel limit."),
            );
        }
    }
    out
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
