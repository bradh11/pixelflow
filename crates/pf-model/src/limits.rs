//! Size limits that keep tiny show files from forcing huge allocations.

use crate::{Generator, ShapeSource, Show};

/// Most pixels a single prop may have.
pub const MAX_PROP_NODES: u32 = 1_000_000;
/// Most pixels all props in a show may have, combined.
pub const MAX_SHOW_PIXELS: u64 = 10_000_000;
/// Most null pixels a single port slot may have.
pub const MAX_NULL_PIXELS: u32 = 1_000;

/// Returns a plain-language sentence for every limit the show breaks.
pub(crate) fn check_limits(show: &Show) -> Vec<String> {
    let mut problems = Vec::new();
    let mut total: u64 = 0;
    for prop in &show.props {
        if let ShapeSource::Generator(Generator::CustomGrid { columns, rows, cells }) = &prop.shape {
            let expected = u64::from(*columns) * u64::from(*rows);
            if cells.len() as u64 != expected {
                problems.push(format!(
                    "The prop '{}' is a custom grid of {columns} columns by {rows} rows, which needs {expected} cells, but it has {}.",
                    prop.name,
                    cells.len()
                ));
                continue;
            }
        }
        let nodes = prop.node_count();
        total += u64::from(nodes);
        if nodes > MAX_PROP_NODES {
            problems.push(format!(
                "The prop '{}' has {nodes} pixels, but PixelFlow supports at most {MAX_PROP_NODES} per prop.",
                prop.name
            ));
        }
    }
    if total > MAX_SHOW_PIXELS {
        problems.push(format!(
            "The show has {total} pixels in total, but PixelFlow supports at most {MAX_SHOW_PIXELS} per show."
        ));
    }
    for controller in &show.controllers {
        for port in &controller.ports {
            for slot in &port.slots {
                if slot.null_pixels > MAX_NULL_PIXELS {
                    problems.push(format!(
                        "A slot on port {} on '{}' has {} null pixels, but PixelFlow supports at most {MAX_NULL_PIXELS} per slot.",
                        port.number, controller.name, slot.null_pixels
                    ));
                }
            }
        }
    }
    problems
}
