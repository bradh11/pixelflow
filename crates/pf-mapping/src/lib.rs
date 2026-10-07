//! Automatic channel mapping.
//!
//! [`map_show`] turns a show's props and wiring into:
//! - a frame-buffer layout (where each prop's pixels live in the show-wide frame),
//! - per-controller output spans (which frame bytes go to which controller channels),
//! - sACN universe assignments,
//!
//! plus a [`ValidationReport`] of wiring problems. It is a pure function of the show.

mod layout;
mod universes;
mod wiring;

pub use layout::{
    Addressing, ChannelAddress, ChannelMap, ControllerOutput, OutputSpan, PixelLocation, PropLayout,
    UniverseSpan,
};

use pf_model::{Show, ValidationReport};

/// Computes the channel map for a show and reports wiring problems.
///
/// Slots that reference missing props or out-of-range segments are skipped here;
/// `pf_model::validate_show` reports those.
pub fn map_show(show: &Show) -> (ChannelMap, ValidationReport) {
    let mut report = ValidationReport::default();
    let props = layout::prop_layouts(show);
    let frame_len = props.last().map_or(0, |p| p.frame_offset + p.byte_len());

    let wired = wiring::wire_controllers(show, &props, &mut report);
    wiring::check_ddp_pixel_types(show, &wired, &mut report);
    let runs: Vec<&[universes::PixelRun]> = wired.iter().map(|w| w.runs.as_slice()).collect();
    let addressing = universes::assign(show, &runs, &mut report);

    let controllers = show
        .controllers
        .iter()
        .zip(wired)
        .zip(addressing)
        .map(|((controller, wired), addressing)| ControllerOutput {
            controller: controller.id,
            channel_count: wired.channel_count,
            addressing,
            spans: wired.spans,
        })
        .collect();

    let map = ChannelMap {
        frame_len,
        props,
        controllers,
    };
    (map, report)
}
