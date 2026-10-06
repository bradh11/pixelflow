//! Size limits that keep tiny show files from forcing huge allocations.

use crate::{Generator, ShapeSource, Show};

/// Most pixels a single prop may have.
pub const MAX_PROP_NODES: u32 = 1_000_000;
/// Most pixels all props in a show may have, combined.
pub const MAX_SHOW_PIXELS: u64 = 10_000_000;
/// Most points a star may have.
pub const MAX_STAR_POINTS: u32 = 100;
/// Most points a poly line may have.
pub const MAX_POLY_VERTICES: usize = 1_000;
/// Most drops an icicle drop pattern may list.
pub const MAX_ICICLE_DROPS: usize = 1_000;
/// Most pixels one icicle drop may have.
pub const MAX_ICICLE_DROP_LIGHTS: u32 = 1_000;
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
        if let ShapeSource::Generator(Generator::PolyLine {
            vertices, segments, ..
        }) = &prop.shape
            && let Some(problem) = poly_line_problem(&prop.name, vertices, segments.len())
        {
            problems.push(problem);
            continue;
        }
        if let ShapeSource::Generator(Generator::Icicles { drops, .. }) = &prop.shape
            && let Some(problem) = icicles_problem(&prop.name, drops)
        {
            problems.push(problem);
            continue;
        }
        // A row of canes or strings is walked even when it has no pixels, so its length is
        // capped like a prop's pixels.
        if let ShapeSource::Generator(
            Generator::Tree { strings, .. }
            | Generator::Icicles { strings, .. }
            | Generator::CandyCanes { canes: strings, .. },
        ) = &prop.shape
            && *strings > MAX_PROP_NODES
        {
            problems.push(format!(
                "The prop '{}' has {strings} {}, but PixelFlow supports at most {MAX_PROP_NODES}.",
                prop.name,
                if matches!(prop.shape, ShapeSource::Generator(Generator::CandyCanes { .. })) {
                    "canes"
                } else {
                    "strings"
                }
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
            ShapeSource::Generator(Generator::CandyCanes {
                canes,
                nodes_per_cane,
                ..
            }) => u64::from(*canes) * u64::from(*nodes_per_cane),
            ShapeSource::Generator(Generator::Icicles {
                strings,
                lights_per_string,
                ..
            }) => u64::from(*strings) * u64::from(*lights_per_string),
            ShapeSource::Generator(Generator::PolyLine {
                segments,
                spread_nodes: None,
                ..
            }) => segments.iter().map(|s| u64::from(s.nodes)).sum(),
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

/// What's wrong with a poly line's points, if anything.
fn poly_line_problem(name: &str, vertices: &[crate::Vec3], segments: usize) -> Option<String> {
    let n = vertices.len();
    if n < 2 {
        return Some(format!(
            "The poly line '{name}' has {n} point{}, but it needs at least 2.",
            if n == 1 { "" } else { "s" }
        ));
    }
    if n > MAX_POLY_VERTICES {
        return Some(format!(
            "The poly line '{name}' has {n} points, but PixelFlow supports at most {MAX_POLY_VERTICES}."
        ));
    }
    if segments != n - 1 {
        return Some(format!(
            "The poly line '{name}' has {n} points, so it needs {} stretches between them, but it has {segments}.",
            n - 1
        ));
    }
    if vertices.iter().any(|v| !v.is_finite()) {
        return Some(format!("The poly line '{name}' has a point that isn't a number."));
    }
    None
}

/// What's wrong with an icicle drop pattern, if anything.
fn icicles_problem(name: &str, drops: &[u32]) -> Option<String> {
    if drops.len() > MAX_ICICLE_DROPS {
        return Some(format!(
            "The icicles '{name}' list {} drops in their pattern, but PixelFlow supports at most {MAX_ICICLE_DROPS}.",
            drops.len()
        ));
    }
    if let Some(&big) = drops.iter().find(|&&d| d > MAX_ICICLE_DROP_LIGHTS) {
        return Some(format!(
            "The icicles '{name}' have a drop of {big} pixels, but PixelFlow supports at most {MAX_ICICLE_DROP_LIGHTS} per drop."
        ));
    }
    if drops.iter().all(|&d| d == 0) {
        return Some(format!(
            "The icicles '{name}' need at least one drop with pixels in their drop pattern."
        ));
    }
    None
}
