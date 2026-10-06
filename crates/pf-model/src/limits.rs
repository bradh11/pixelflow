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
/// Most arms a spinner may have.
pub const MAX_SPINNER_ARMS: u32 = 1_000;
/// Largest hollow middle a spinner may have, in percent (xLights' `Hollow`).
pub const MAX_SPINNER_HOLLOW: u32 = 100;
/// Most layers a layered arch, circle or star may have.
pub const MAX_SHAPE_LAYERS: usize = 1_000;
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
            && let Some(problem) = poly_line_problem(&prop.name, vertices, segments)
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
        if let ShapeSource::Generator(Generator::Spinner {
            arms, hollow, arc, ..
        }) = &prop.shape
            && let Some(problem) = spinner_problem(&prop.name, *arms, *hollow, *arc)
        {
            problems.push(problem);
            continue;
        }
        if let ShapeSource::Generator(Generator::Arch {
            arches,
            arc,
            skew_deg,
            layers,
            hollow,
            ..
        }) = &prop.shape
            && let Some(problem) = arch_problem(&prop.name, *arches, *arc, *skew_deg, layers, *hollow)
        {
            problems.push(problem);
            continue;
        }
        if let ShapeSource::Generator(Generator::Sphere {
            start_latitude,
            end_latitude,
            degrees,
            ..
        }) = &prop.shape
            && let Some(problem) = sphere_problem(&prop.name, *start_latitude, *end_latitude, *degrees)
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
        // A sphere's or cube's sides are walked even when another side is 0, so each is capped.
        let sides: &[(u32, &str)] = match &prop.shape {
            ShapeSource::Generator(Generator::Sphere { columns, rows, .. }) => {
                &[(*columns, "strands around"), (*rows, "pixels per strand")]
            }
            ShapeSource::Generator(Generator::Cube {
                width, height, depth, ..
            }) => &[
                (*width, "pixels across"),
                (*height, "pixels up"),
                (*depth, "pixels deep"),
            ],
            _ => &[],
        };
        if let Some((n, what)) = sides.iter().find(|(n, _)| *n > MAX_PROP_NODES) {
            problems.push(format!(
                "The prop '{}' has {n} {what}, but PixelFlow supports at most {MAX_PROP_NODES}.",
                prop.name
            ));
            continue;
        }
        // `node_count()` saturates at u32::MAX, so compute the real count here.
        let nodes = match &prop.shape {
            ShapeSource::Generator(
                Generator::Matrix { columns, rows, .. } | Generator::Sphere { columns, rows, .. },
            ) => u64::from(*columns) * u64::from(*rows),
            ShapeSource::Generator(Generator::Cube {
                width, height, depth, ..
            }) => u64::from(*width) * u64::from(*height) * u64::from(*depth),
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
            ShapeSource::Generator(Generator::Spinner {
                arms, nodes_per_arm, ..
            }) => u64::from(*arms) * u64::from(*nodes_per_arm),
            ShapeSource::Generator(Generator::Arch {
                nodes,
                arches,
                layers,
                ..
            }) if layers.is_empty() => u64::from(*arches) * u64::from(*nodes),
            ShapeSource::Generator(Generator::WindowFrame {
                top, sides, bottom, ..
            }) => u64::from(*top) + 2 * u64::from(*sides) + u64::from(*bottom),
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
fn poly_line_problem(
    name: &str,
    vertices: &[crate::Vec3],
    segments: &[crate::PolySegment],
) -> Option<String> {
    let mut curves = segments.iter().filter_map(|s| s.curve).flatten();
    let segments = segments.len();
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
    if vertices.iter().any(|v| !v.is_finite()) || curves.any(|v| !v.is_finite()) {
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

/// What's wrong with a spinner's arms, if anything.
fn spinner_problem(name: &str, arms: u32, hollow: u32, arc: f32) -> Option<String> {
    if arms > MAX_SPINNER_ARMS {
        return Some(format!(
            "The spinner '{name}' has {arms} arms, but PixelFlow supports at most {MAX_SPINNER_ARMS}."
        ));
    }
    if hollow > MAX_SPINNER_HOLLOW {
        return Some(format!(
            "The spinner '{name}' has a hollow middle of {hollow}%, but PixelFlow supports at most {MAX_SPINNER_HOLLOW}%."
        ));
    }
    if !(arc > 0.0 && arc <= 360.0) {
        return Some(format!(
            "The spinner '{name}' spreads its arms over {arc}°, but that must be more than 0° and at most 360°."
        ));
    }
    None
}

/// What's wrong with a layer list, if anything (`what` names the prop, like "The arch 'Gate'").
fn layers_problem(what: &str, layers: &[u32]) -> Option<String> {
    if layers.len() > MAX_SHAPE_LAYERS {
        return Some(format!(
            "{what} has {} layers, but PixelFlow supports at most {MAX_SHAPE_LAYERS}.",
            layers.len()
        ));
    }
    if let Some(&big) = layers.iter().find(|&&n| n > MAX_PROP_NODES) {
        return Some(format!(
            "{what} has a layer of {big} pixels, but PixelFlow supports at most {MAX_PROP_NODES}."
        ));
    }
    None
}

/// What's wrong with an arch's settings, if anything.
fn arch_problem(
    name: &str,
    arches: u32,
    arc: f32,
    skew_deg: f32,
    layers: &[u32],
    hollow: u32,
) -> Option<String> {
    if arches > MAX_PROP_NODES {
        return Some(format!(
            "The arch '{name}' is {arches} arches, but PixelFlow supports at most {MAX_PROP_NODES}."
        ));
    }
    if !(1.0..=180.0).contains(&arc) {
        return Some(format!(
            "The arch '{name}' goes {arc}° round, but that must be from 1° to 180°."
        ));
    }
    if !(-180.0..=180.0).contains(&skew_deg) {
        return Some(format!(
            "The arch '{name}' leans {skew_deg}°, but that must be from -180° to 180°."
        ));
    }
    if hollow > 100 {
        return Some(format!(
            "The arch '{name}' has an innermost layer {hollow}% of its size, but that must be at most 100%."
        ));
    }
    layers_problem(&format!("The arch '{name}'"), layers)
}

/// What's wrong with a sphere's latitudes or sweep, if anything.
fn sphere_problem(name: &str, start_latitude: f32, end_latitude: f32, degrees: f32) -> Option<String> {
    for latitude in [start_latitude, end_latitude] {
        if !(-90.0..=90.0).contains(&latitude) {
            return Some(format!(
                "The sphere '{name}' reaches latitude {latitude}°, but latitudes run from -90° to 90°."
            ));
        }
    }
    if !(degrees > 0.0 && degrees <= 360.0) {
        return Some(format!(
            "The sphere '{name}' goes {degrees}° round, but that must be more than 0° and at most 360°."
        ));
    }
    None
}
