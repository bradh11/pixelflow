//! Size limits that keep tiny show files from forcing huge allocations.

use crate::{Generator, ShapeSource, Show};

/// Most pixels a single prop may have.
pub const MAX_PROP_NODES: u32 = 1_000_000;
/// Most pixels all props in a show may have, combined.
pub const MAX_SHOW_PIXELS: u64 = 10_000_000;
/// Most points a star may have.
pub const MAX_STAR_POINTS: u32 = 100;
/// Most null pixels a single port slot may have.
pub const MAX_NULL_PIXELS: u32 = 1_000;
/// Most a sequence's lights may be moved against its music, either way, in milliseconds.
pub const MAX_SEQUENCE_OFFSET_MS: i32 = 10_000;
/// Most submodels and faces one prop may have.
pub const MAX_REGIONS_PER_PROP: usize = 1_000;
/// Most pixel entries all of one prop's submodels and faces may name together (a run counts
/// its pixels, up to the prop's size).
pub const MAX_REGION_ENTRIES: u64 = 10_000_000;

/// Returns a plain-language sentence for every limit the show breaks (and for sequences listed
/// twice under one id, which a show file must never have).
pub(crate) fn check_limits(show: &Show) -> Vec<String> {
    let mut problems = Vec::new();
    let mut sequence_ids = std::collections::HashSet::new();
    for sequence in &show.sequences {
        if sequence.offset_ms.unsigned_abs() > MAX_SEQUENCE_OFFSET_MS.unsigned_abs() {
            problems.push(format!(
                "The sequence '{}' moves its lights {} ms against its music, but PixelFlow allows at most {MAX_SEQUENCE_OFFSET_MS} ms either way.",
                sequence.name, sequence.offset_ms
            ));
        }
        if !sequence_ids.insert(sequence.id) {
            problems.push(format!(
                "The sequence '{}' has the same id as another sequence in the show.",
                sequence.name
            ));
        }
    }
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
        if let ShapeSource::Generator(Generator::Star { points, .. }) = &prop.shape
            && *points > MAX_STAR_POINTS
        {
            problems.push(format!(
                "The star '{}' has {points} points, but PixelFlow supports at most {MAX_STAR_POINTS}.",
                prop.name
            ));
            continue;
        }
        if let ShapeSource::Generator(Generator::Tree { strings, .. }) = &prop.shape
            && *strings > MAX_PROP_NODES
        {
            problems.push(format!(
                "The tree '{}' has {strings} strings, but PixelFlow supports at most {MAX_PROP_NODES}.",
                prop.name
            ));
            continue;
        }
        // `node_count()` saturates at u32::MAX, so compute the real count here.
        let nodes = match &prop.shape {
            ShapeSource::Generator(Generator::Matrix { columns, rows, .. }) => {
                u64::from(*columns) * u64::from(*rows)
            }
            ShapeSource::Generator(Generator::Tree {
                strings,
                nodes_per_string,
                ..
            }) => u64::from(*strings) * u64::from(*nodes_per_string),
            _ => u64::from(prop.node_count()),
        };
        total += nodes;
        if prop.regions.len() > MAX_REGIONS_PER_PROP {
            problems.push(format!(
                "The prop '{}' has {} submodels and faces, but PixelFlow supports at most {MAX_REGIONS_PER_PROP} per prop.",
                prop.name,
                prop.regions.len()
            ));
        } else {
            let entries: u64 = prop
                .regions
                .iter()
                .map(|r| r.entry_count(prop.node_count()))
                .sum();
            if entries > MAX_REGION_ENTRIES {
                problems.push(format!(
                    "The submodels and faces of '{}' list {entries} pixels in all, but PixelFlow supports at most {MAX_REGION_ENTRIES} per prop.",
                    prop.name
                ));
            }
        }
        if nodes > u64::from(MAX_PROP_NODES) {
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
