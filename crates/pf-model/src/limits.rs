//! Size limits that keep tiny show files from forcing huge allocations.

use crate::{Generator, IssueCode, ShapeSource, Show};

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
/// Most turns a spiral tree may wind round, either way.
pub const MAX_SPIRAL_TURNS: f32 = 100.0;
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

/// A limit a show breaks: what kind, what's wrong, and how to put it right.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LimitProblem {
    pub code: IssueCode,
    pub message: String,
    pub fix: &'static str,
}

/// Returns every limit the show breaks, plus what a show file can't hold at all: sequences
/// listed twice under one id, and numbers that aren't finite (they would save as `null`).
pub(crate) fn check_limits(show: &Show) -> Vec<LimitProblem> {
    let mut problems = Vec::new();
    let mut push = |code, message, fix| problems.push(LimitProblem { code, message, fix });
    let mut sequence_ids = std::collections::HashSet::new();
    for sequence in &show.sequences {
        if sequence.offset_ms.unsigned_abs() > MAX_SEQUENCE_OFFSET_MS.unsigned_abs() {
            push(
                IssueCode::LimitExceeded,
                format!(
                    "The sequence '{}' moves its lights {} ms against its music, but PixelFlow allows at most {MAX_SEQUENCE_OFFSET_MS} ms either way.",
                    sequence.name, sequence.offset_ms
                ),
                "Move the sequence's lights at most 10 seconds against its music.",
            );
        }
        if !sequence_ids.insert(sequence.id) {
            push(
                IssueCode::DuplicateId,
                format!(
                    "The sequence '{}' has the same id as another sequence in the show.",
                    sequence.name
                ),
                "Remove one of the two from the show's sequences, then add it again.",
            );
        }
    }
    let mut total: u64 = 0;
    for prop in &show.props {
        let t = &prop.transform;
        if !(t.position.is_finite() && t.rotation_deg.is_finite() && t.scale.is_finite()) {
            push(
                IssueCode::InvalidTransform,
                format!(
                    "The prop '{}' has a position, rotation or size that isn't a number.",
                    prop.name
                ),
                "Place the prop again on the Layout screen.",
            );
        }
        let nodes = match shape_check(&prop.name, &prop.shape) {
            Ok(nodes) => nodes,
            Err(problem) => {
                push(
                    IssueCode::InvalidShape,
                    problem,
                    "Change the prop's shape settings, or delete the prop.",
                );
                continue;
            }
        };
        total += nodes;
        if prop.regions.len() > MAX_REGIONS_PER_PROP {
            push(
                IssueCode::LimitExceeded,
                format!(
                    "The prop '{}' has {} submodels and faces, but PixelFlow supports at most {MAX_REGIONS_PER_PROP} per prop.",
                    prop.name,
                    prop.regions.len()
                ),
                "Remove some of the prop's submodels and faces.",
            );
        } else {
            let entries: u64 = prop
                .regions
                .iter()
                .map(|r| r.entry_count(prop.node_count()))
                .sum();
            if entries > MAX_REGION_ENTRIES {
                push(
                    IssueCode::LimitExceeded,
                    format!(
                        "The submodels and faces of '{}' list {entries} pixels in all, but PixelFlow supports at most {MAX_REGION_ENTRIES} per prop.",
                        prop.name
                    ),
                    "Remove some of the prop's submodels and faces, or make them smaller.",
                );
            }
        }
        if nodes > u64::from(MAX_PROP_NODES) {
            push(
                IssueCode::LimitExceeded,
                format!(
                    "The prop '{}' has {nodes} pixels, but PixelFlow supports at most {MAX_PROP_NODES} per prop.",
                    prop.name
                ),
                "Reduce the size, or split it into smaller props.",
            );
        }
    }
    if total > MAX_SHOW_PIXELS {
        push(
            IssueCode::LimitExceeded,
            format!(
                "The show has {total} pixels in total, but PixelFlow supports at most {MAX_SHOW_PIXELS} per show."
            ),
            "Remove some props, or make them smaller.",
        );
    }
    for controller in &show.controllers {
        for port in &controller.ports {
            for slot in &port.slots {
                if slot.null_pixels > MAX_NULL_PIXELS {
                    push(
                        IssueCode::LimitExceeded,
                        format!(
                            "A slot on port {} on '{}' has {} null pixels, but PixelFlow supports at most {MAX_NULL_PIXELS} per slot.",
                            port.number, controller.name, slot.null_pixels
                        ),
                        "Use fewer null pixels on the slot (at most 1,000).",
                    );
                }
            }
        }
    }
    problems
}

/// What's wrong with a prop's shape (`name` names the prop), or its real pixel count: `node_count()`
/// stops at `u32::MAX`.
fn shape_check(name: &str, shape: &ShapeSource) -> Result<u64, String> {
    if !numbers_finite(shape) {
        return Err(format!(
            "The prop '{name}' has a shape setting or point that isn't a number."
        ));
    }
    if let ShapeSource::Generator(Generator::CustomGrid { columns, rows, cells }) = shape {
        let expected = u64::from(*columns) * u64::from(*rows);
        if cells.len() as u64 != expected {
            return Err(format!(
                "The prop '{}' is a custom grid of {columns} columns by {rows} rows, which needs {expected} cells, but it has {}.",
                name,
                cells.len()
            ));
        }
    }
    if let ShapeSource::Generator(Generator::Star { points, .. }) = shape
        && *points > MAX_STAR_POINTS
    {
        return Err(format!(
            "The star '{}' has {points} points, but PixelFlow supports at most {MAX_STAR_POINTS}.",
            name
        ));
    }
    if let ShapeSource::Generator(Generator::PolyLine {
        vertices, segments, ..
    }) = shape
        && let Some(problem) = poly_line_problem(name, vertices, segments)
    {
        return Err(problem);
    }
    if let ShapeSource::Generator(Generator::Icicles { drops, .. }) = shape
        && let Some(problem) = icicles_problem(name, drops)
    {
        return Err(problem);
    }
    if let ShapeSource::Generator(Generator::Spinner {
        arms, hollow, arc, ..
    }) = shape
        && let Some(problem) = spinner_problem(name, *arms, *hollow, *arc)
    {
        return Err(problem);
    }
    if let ShapeSource::Generator(Generator::Arch {
        arches,
        arc,
        skew_deg,
        gap,
        layers,
        hollow,
        ..
    }) = shape
        && let Some(problem) = arch_problem(name, *arches, *arc, *skew_deg, *gap, layers, *hollow)
    {
        return Err(problem);
    }
    if let ShapeSource::Generator(Generator::Tree {
        strings,
        strands_per_string,
        spiral_rotations,
        ..
    }) = shape
        && let Some(problem) = tree_problem(name, *strings, *strands_per_string, *spiral_rotations)
    {
        return Err(problem);
    }
    let rings = match shape {
        ShapeSource::Generator(Generator::Circle {
            layers,
            inner_percent,
            ..
        }) => Some(("circle", layers, *inner_percent)),
        ShapeSource::Generator(Generator::Star {
            layers,
            inner_percent,
            ..
        }) => Some(("star", layers, *inner_percent)),
        _ => None,
    };
    if let Some((what, layers, inner_percent)) = rings {
        let what = format!("The {what} '{name}'");
        let problem = if inner_percent > 100 {
            Some(format!(
                "{what} has an innermost layer {inner_percent}% of its size, but that must be at most 100%."
            ))
        } else {
            layers_problem(&what, layers)
        };
        if let Some(problem) = problem {
            return Err(problem);
        }
    }
    if let ShapeSource::Generator(Generator::Sphere {
        start_latitude,
        end_latitude,
        degrees,
        ..
    }) = shape
        && let Some(problem) = sphere_problem(name, *start_latitude, *end_latitude, *degrees)
    {
        return Err(problem);
    }
    // A row of canes or strings is walked even when it has no pixels, so its length is
    // capped like a prop's pixels.
    if let ShapeSource::Generator(
        Generator::Tree { strings, .. }
        | Generator::Icicles { strings, .. }
        | Generator::CandyCanes { canes: strings, .. },
    ) = shape
        && *strings > MAX_PROP_NODES
    {
        return Err(format!(
            "The prop '{}' has {strings} {}, but PixelFlow supports at most {MAX_PROP_NODES}.",
            name,
            if matches!(*shape, ShapeSource::Generator(Generator::CandyCanes { .. })) {
                "canes"
            } else {
                "strings"
            }
        ));
    }
    // A sphere's or cube's sides are walked even when another side is 0, so each is capped.
    let sides: &[(u32, &str)] = match shape {
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
        return Err(format!(
            "The prop '{}' has {n} {what}, but PixelFlow supports at most {MAX_PROP_NODES}.",
            name
        ));
    }
    // `node_count()` saturates at u32::MAX, so compute the real count here.
    let nodes = match shape {
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
        _ => u64::from(shape.node_count()),
    };
    Ok(nodes)
}

/// What's wrong with a generated shape for a prop named `name`, by the same limits a show file is
/// held to (including its pixel count), if anything. The xLights import checks every shape it
/// makes with this, so an odd model keeps its measured points instead of spoiling the show.
pub fn shape_problem(name: &str, shape: &Generator) -> Option<String> {
    match shape_check(name, &ShapeSource::Generator(shape.clone())) {
        Err(problem) => Some(problem),
        Ok(nodes) if nodes > u64::from(MAX_PROP_NODES) => Some(format!(
            "The prop '{name}' has {nodes} pixels, but PixelFlow supports at most {MAX_PROP_NODES} per prop."
        )),
        Ok(_) => None,
    }
}

/// Whether every size, angle and measured point of a shape is a finite number. A poly line's
/// points are checked with its other points ([`poly_line_problem`]). No catch-all arm, so a new
/// shape must say which of its numbers to check.
fn numbers_finite(shape: &ShapeSource) -> bool {
    let generator = match shape {
        ShapeSource::Measured { points, .. } => return points.iter().all(|p| p.is_finite()),
        ShapeSource::Generator(generator) => generator,
    };
    let numbers: Vec<f32> = match generator {
        Generator::Line { length, .. } => vec![*length],
        Generator::Arch {
            width,
            height,
            arc,
            gap,
            skew_deg,
            ..
        } => vec![*width, *height, *arc, *gap, *skew_deg],
        Generator::Circle { radius, .. } | Generator::Wreath { radius, .. } => vec![*radius],
        Generator::Matrix { width, height, .. } | Generator::WindowFrame { width, height, .. } => {
            vec![*width, *height]
        }
        Generator::Tree {
            height,
            base_radius,
            top_radius,
            degrees,
            start_angle,
            spiral_rotations,
            ..
        } => vec![
            *height,
            *base_radius,
            *top_radius,
            *degrees,
            *start_angle,
            *spiral_rotations,
        ],
        Generator::Star {
            outer_radius,
            inner_radius,
            ..
        } => vec![*outer_radius, *inner_radius],
        Generator::CandyCanes {
            width,
            height,
            cane_height,
            skew_deg,
            ..
        } => vec![*width, *height, *cane_height, *skew_deg],
        Generator::Icicles {
            width, drop_height, ..
        } => vec![*width, *drop_height],
        Generator::Spinner {
            start_angle,
            arc,
            radius,
            ..
        } => vec![*start_angle, *arc, *radius],
        Generator::Sphere {
            radius,
            start_latitude,
            end_latitude,
            degrees,
            ..
        } => vec![*radius, *start_latitude, *end_latitude, *degrees],
        Generator::Cube { spacing, .. } => vec![*spacing],
        Generator::PolyLine { .. } | Generator::CustomGrid { .. } => vec![],
    };
    numbers.iter().all(|n| n.is_finite())
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

/// What's wrong with a tree's wiring settings, if anything.
fn tree_problem(name: &str, strings: u32, strands_per_string: u32, spiral_rotations: f32) -> Option<String> {
    if spiral_rotations.is_nan() || spiral_rotations.abs() > MAX_SPIRAL_TURNS {
        return Some(format!(
            "The tree '{name}' winds round {spiral_rotations} times, but PixelFlow supports at most {MAX_SPIRAL_TURNS} turns either way."
        ));
    }
    if strands_per_string > strings {
        return Some(format!(
            "The tree '{name}' restarts its zig-zag every {strands_per_string} strings, but it only has {strings}."
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
    gap: f32,
    layers: &[u32],
    hollow: u32,
) -> Option<String> {
    if !(gap >= 0.0 && gap.is_finite()) {
        return Some(format!(
            "The arch '{name}' has a gap of {gap} between its arches, but that can't be less than 0."
        ));
    }
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
