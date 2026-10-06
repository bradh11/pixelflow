//! Editable shapes for imported models: each xLights model type PixelFlow has a shape for is
//! imported as that shape (its parameters read from the model) instead of as measured points,
//! so it can be edited like a prop drawn in PixelFlow.
//!
//! A shape is only used when `pf-geometry` puts every pixel exactly where xLights does (the
//! measured points, in channel order); otherwise the model keeps its measured points. So an
//! import always looks and maps exactly as before, whichever way a model is set up.

use crate::geometry::{Affine, custom_cells, parse_points, rot_from_x_axis, strtod, strtol0};
use crate::model::XmlModel;
use pf_model::{Generator, PolySegment, Prop, ShapeSource, Transform, Vec3};
use std::f64::consts::PI;

/// xLights layout units per PixelFlow unit (see `import::LAYOUT_SCALE`).
const SCALE: f32 = 0.01;

/// How far (PixelFlow units) a shape's pixel may be from xLights' and still count as the same.
const TOLERANCE: f32 = 2e-3;

/// A shape and placement that may reproduce a model.
type Candidate = (Generator, Transform);

/// The editable shape for `model` whose pixels land on `points` (xLights' positions in channel
/// order, PixelFlow units), if PixelFlow has one.
pub(crate) fn editable(model: &XmlModel, points: &[Vec3]) -> Option<Candidate> {
    let candidates = match model.display_as.trim() {
        "Poly Line" => poly_line(model),
        "Single Line" => single_line(model, points.len()),
        "Candy Canes" => candy_canes(model),
        "Icicles" => icicles(model),
        "Window Frame" => window_frame(model),
        "Wreath" => wreath(model),
        "Spinner" => spinner(model),
        "Sphere" => sphere(model),
        "Cube" => cube(model),
        "Custom" => custom(model),
        // Same order as xLights' model factory: matrices before trees.
        t if t.contains("Matrix") && !t.contains("MultiPoint") => matrix(model),
        t if t.starts_with("Tree") => tree(model),
        _ => Vec::new(),
    };
    if candidates.is_empty() {
        return None;
    }
    // The real 3D shape: xLights' layout with depth, without its 2D view's tilt.
    let upright: Vec<Vec3> = crate::geometry::upright_positions(model)
        .iter()
        .map(|p| Vec3::new(p[0], p[1], p[2]) * SCALE)
        .collect();
    // It must be the same nodes the import measured; for models xLights doesn't tilt, in the
    // same places in the front view too.
    let tilted = tilted_in_2d(model);
    let same = upright.len() == points.len()
        && (tilted
            || upright
                .iter()
                .zip(points)
                .all(|(a, b)| close(a.x, b.x) && close(a.y, b.y)));
    if !same {
        return None;
    }
    candidates.into_iter().find(|(g, t)| fits(g, t, &upright))
}

/// Trees, spheres and cubes, which xLights' 2D view draws slightly tilted (`SetPerspective2D`).
pub(crate) fn tilted_in_2d(model: &XmlModel) -> bool {
    let t = model.display_as.trim();
    matches!(t, "Sphere" | "Cube") || (t.starts_with("Tree") && !t.contains("Matrix"))
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= TOLERANCE
}

/// True when the shape, placed by `transform`, puts every pixel on `points`, depth included.
fn fits(generator: &Generator, transform: &Transform, points: &[Vec3]) -> bool {
    if generator.node_count() as usize != points.len() || points.is_empty() {
        return false;
    }
    let mut prop = Prop::new("", ShapeSource::Generator(generator.clone()));
    prop.transform = *transform;
    pf_geometry::world_positions(&prop)
        .iter()
        .zip(points)
        .all(|(a, b)| close(a.x, b.x) && close(a.y, b.y) && close(a.z, b.z))
}

/// `pugixml as_int(default)`.
fn int(m: &XmlModel, key: &str, default: i64) -> i64 {
    m.attr(key).map_or(default, strtol0)
}

/// `pugixml as_float(default)`, non-finite values reading as `default`.
fn float(m: &XmlModel, key: &str, default: f64) -> f64 {
    match m.attr(key) {
        None => default,
        Some(v) => Some(strtod(v).unwrap_or(0.0))
            .filter(|f| f.is_finite())
            .unwrap_or(default),
    }
}

/// A named count with its legacy `parmN` fallback, as `ReadAttrWithParmFallback` reads it.
fn parm(m: &XmlModel, key: &str, legacy: &str, default: i64) -> i64 {
    m.attr(key).or_else(|| m.attr(legacy)).map_or(default, strtol0)
}

fn world_pos(m: &XmlModel) -> Vec3 {
    Vec3::new(
        float(m, "WorldPosX", 0.0) as f32,
        float(m, "WorldPosY", 0.0) as f32,
        float(m, "WorldPosZ", 0.0) as f32,
    ) * SCALE
}

fn transform(position: Vec3, rotation_deg: Vec3, scale: Vec3) -> Transform {
    Transform {
        position,
        rotation_deg,
        scale,
    }
}

fn reversed_segments(segments: &[PolySegment]) -> Vec<PolySegment> {
    segments
        .iter()
        .rev()
        .map(|s| PolySegment {
            nodes: s.nodes,
            curve: s.curve.map(|[a, b]| [b, a]),
        })
        .collect()
}

/// `PolyLineModel` with one light per node and no drops: its points (as `PointData` holds them,
/// scaled by the model's `ScaleX/Y/Z` about `WorldPos`), each segment's `SegN` light count, or
/// `NodesPerString` spread over the whole line when the segments have no counts. Wired from the
/// last point back (`Dir="R"`), the line is turned around so its pixels still run first to last.
///
/// Curved segments are imported straight, as they've always been drawn on import.
fn poly_line(m: &XmlModel) -> Vec<Candidate> {
    let n = int(m, "NumPoints", 2).max(2);
    if n as usize > pf_model::MAX_POLY_VERTICES {
        return Vec::new();
    }
    let n = n as usize;
    let raw = parse_points(m.text("PointData", "0.0, 0.0, 0.0, 0.0, 0.0, 0.0"), n);
    // xLights' poly-point bounds: a nearly flat line (under 0.1 tall) is drawn flat at its lowest
    // point (bounds seeded at 100000 and 0, as in xLights).
    let lo_y = raw.iter().fold(100_000.0f64, |lo, p| lo.min(p[1]));
    let hi_y = raw.iter().fold(0.0f64, |hi, p| hi.max(p[1]));
    let flat = (hi_y - lo_y).abs() < 0.1;
    let vertices: Vec<Vec3> = raw
        .iter()
        .map(|p| Vec3::new(p[0] as f32, if flat { lo_y } else { p[1] } as f32, p[2] as f32) * SCALE)
        .collect();
    let spread = m.attr("Seg1").is_none();
    let segments: Vec<PolySegment> = (0..n - 1)
        .map(|i| {
            PolySegment::straight(if spread {
                0
            } else {
                int(m, &format!("Seg{}", i + 1), 0).max(0) as u32
            })
        })
        .collect();
    let spread_nodes = spread.then(|| parm(m, "NodesPerString", "parm2", 0).max(0) as u32);
    let scale = |k: &str| {
        let v = float(m, k, 1.0);
        if v <= 0.0 { 1.0 } else { v as f32 }
    };
    let place = transform(
        world_pos(m),
        Vec3::ZERO,
        Vec3::new(scale("ScaleX"), scale("ScaleY"), scale("ScaleZ")),
    );
    let forward = Generator::PolyLine {
        vertices: vertices.clone(),
        segments: segments.clone(),
        spread_nodes,
    };
    let backward = Generator::PolyLine {
        vertices: vertices.into_iter().rev().collect(),
        segments: reversed_segments(&segments),
        spread_nodes,
    };
    vec![(forward, place), (backward, place)]
}

/// `SingleLineModel`: `nodes` lights from point 1 (`WorldPos`) to point 2 (`X2/Y2/Z2` from it),
/// as a PixelFlow line centered between them and turned to point from one to the other (or the
/// other way round, for a line wired from point 2).
fn single_line(m: &XmlModel, nodes: usize) -> Vec<Candidate> {
    let from = world_pos(m);
    let d = Vec3::new(
        float(m, "X2", 0.0) as f32,
        float(m, "Y2", 0.0) as f32,
        float(m, "Z2", 0.0) as f32,
    ) * SCALE;
    let length = d.length();
    if length.is_nan() || length <= 0.0 || nodes == 0 {
        return Vec::new();
    }
    let line = Generator::Line {
        nodes: nodes as u32,
        length,
    };
    let center = from + d * 0.5;
    let candidate = |d: Vec3| {
        // Rotation about Y then Z turns +X toward `d` (see `pf_geometry::apply_transform`).
        let turn = (-d.z).atan2((d.x * d.x + d.y * d.y).sqrt()).to_degrees();
        let angle = d.y.atan2(d.x).to_degrees();
        (
            line.clone(),
            transform(center, Vec3::new(0.0, turn, angle), Vec3::ONE),
        )
    };
    vec![candidate(d), candidate(d * -1.0)]
}

/// Where a three-point model (Arches, Candy Canes, Icicles) sits, as
/// `ThreePointScreenLocation` places it: `WorldPos` is point 1, and its local x axis runs
/// `length` (xLights units) toward point 2, turned by `turn`.
struct ThreePoint {
    start: [f64; 3],
    length: f64,
    turn: Affine,
}

fn three_point(m: &XmlModel) -> ThreePoint {
    let start = ["WorldPosX", "WorldPosY", "WorldPosZ"].map(|k| float(m, k, 0.0));
    let (x2, y2, z2) = (float(m, "X2", 0.0), float(m, "Y2", 0.0), float(m, "Z2", 0.0));
    // xLights nudges a zero-length model, and turns one drawn right to left about Y.
    let x = if x2 == 0.0 && y2 == 0.0 && z2 == 0.0 {
        0.001
    } else {
        x2
    };
    let swapped = x2 < 0.0;
    let a = if swapped { [-x, -y2, -z2] } else { [x, y2, z2] };
    let mut turn = rot_from_x_axis(a);
    if swapped {
        turn = turn.then(&Affine::rot_y(PI));
    }
    turn = turn.then(&Affine::rot_x(float(m, "RotateX", 0.0).to_radians()));
    ThreePoint {
        start,
        length: (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt(),
        turn,
    }
}

/// The placement of a PixelFlow shape drawn between two ends (origin midway, x along the line)
/// that reproduces a three-point model. `mirror` turns the shape half round about Y, for a
/// model wired from point 2 whose shape is the mirror image of the one PixelFlow draws.
fn between_ends(tp: &ThreePoint, mirror: bool) -> Transform {
    let mid = tp.turn.apply([tp.length / 2.0, 0.0, 0.0]);
    let turn = if mirror {
        tp.turn.then(&Affine::rot_y(PI))
    } else {
        tp.turn
    };
    let at = |i: usize| ((tp.start[i] + mid[i]) * f64::from(SCALE)) as f32;
    transform(Vec3::new(at(0), at(1), at(2)), euler_degrees(&turn.m), Vec3::ONE)
}

/// Rotations about X, then Y, then Z (degrees, as `pf_geometry::apply_transform` applies them)
/// that make the turn `m`, preferring a half turn about Y to half turns about X and Z.
fn euler_degrees(m: &[[f64; 3]; 3]) -> Vec3 {
    let y = (-m[2][0]).clamp(-1.0, 1.0).asin();
    let (mut x, mut z) = if y.cos() > 1e-9 {
        (m[2][1].atan2(m[2][2]), m[1][0].atan2(m[0][0]))
    } else {
        (0.0, (-m[0][1]).atan2(m[1][1]))
    };
    let mut y = y.to_degrees();
    (x, z) = (x.to_degrees(), z.to_degrees());
    if (x.abs() - 180.0).abs() < 1e-6 && y.abs() < 1e-6 {
        (x, y, z) = (0.0, 180.0, z - 180.0);
    }
    // Rounded so float noise doesn't show up as stray hundred-thousandths of a degree.
    let tidy = |d: f64| {
        let d = (d * 1e6).round() / 1e6;
        let d = if d <= -180.0 { d + 360.0 } else { d };
        (if d == 0.0 { 0.0 } else { d }) as f32
    };
    Vec3::new(tidy(x), tidy(y), tidy(z))
}

/// An xLights `"true"` flag.
fn flag(m: &XmlModel, key: &str) -> bool {
    m.attr(key) == Some("true")
}

/// A count PixelFlow can hold, or `None` for a negative or oversized one.
fn count(v: i64) -> Option<u32> {
    u32::try_from(v).ok()
}

/// `CandyCaneModel` with one light per node: its settings, and the canes scaled to the distance
/// between its two points. Wired from the last cane (`Dir="R"`), each cane still runs up its
/// stick first, so the canes are the mirror image of a row with the hooks the other way.
fn candy_canes(m: &XmlModel) -> Vec<Candidate> {
    let (Some(canes), Some(nodes_per_cane)) = (
        count(parm(m, "NumCanes", "parm1", 1)),
        count(parm(m, "NodesPerCane", "parm2", 1)),
    ) else {
        return Vec::new();
    };
    if u64::from(canes) * u64::from(nodes_per_cane) > u64::from(pf_model::MAX_PROP_NODES) {
        return Vec::new();
    }
    let skew = if m.attr("CandyCaneSkew").is_some() {
        int(m, "CandyCaneSkew", 0)
    } else {
        int(m, "Angle", 0)
    } as f32;
    let tp = three_point(m);
    // xLights' `Dir="R"` only wires the canes from the right; their shape is the same.
    let generator = Generator::CandyCanes {
        canes,
        nodes_per_cane,
        width: tp.length as f32 * SCALE,
        height: float(m, "Height", 1.0) as f32,
        cane_height: float(m, "CandyCaneHeight", 1.0) as f32,
        reverse: flag(m, "CandyCaneReverse"),
        sticks: flag(m, "CandyCaneSticks"),
        alternate_nodes: flag(m, "AlternateNodes"),
        skew_deg: skew,
        start_right: m.attr("Dir") == Some("R"),
    };
    vec![(generator, between_ends(&tp, false))]
}

/// `IciclesModel` without shear: its settings, its columns spread over the distance between its
/// two points, and its drops hanging as far as xLights' `Height` scales them (see
/// `Generator::Icicles::drop_height`). Wired from point 2 (`Dir="R"`), it's turned half round.
fn icicles(m: &XmlModel) -> Vec<Candidate> {
    let (Some(strings), Some(lights_per_string)) = (
        count(parm(m, "NumStrings", "parm1", 1)),
        count(parm(m, "NodesPerString", "parm2", 1)),
    ) else {
        return Vec::new();
    };
    if u64::from(strings) * u64::from(lights_per_string) > u64::from(pf_model::MAX_PROP_NODES)
        || float(m, "Shear", 0.0) != 0.0
    {
        return Vec::new();
    }
    // `IciclesModel::ParseDropSizes`: negative drops are left out, and no drops means drops of 5.
    let mut drops: Vec<u32> = m
        .text("DropPattern", "3,4,5,4")
        .split(',')
        .map(strtol0)
        .filter(|&d| d >= 0)
        .map(|d| u32::try_from(d).unwrap_or(u32::MAX))
        .collect();
    if drops.iter().all(|&d| d == 0) {
        drops = vec![5];
    }
    if drops.len() > pf_model::MAX_ICICLE_DROPS || drops.iter().any(|&d| d > pf_model::MAX_ICICLE_DROP_LIGHTS)
    {
        return Vec::new();
    }
    let tp = three_point(m);
    let gaps = pf_geometry::icicle_column_gaps(strings, lights_per_string, &drops).max(1) as f64;
    let longest = drops.iter().copied().max().unwrap_or(1).saturating_sub(1).max(1);
    let spacing = -float(m, "Height", 1.0) * tp.length / gaps * f64::from(SCALE);
    let generator = Generator::Icicles {
        strings,
        lights_per_string,
        drops,
        width: tp.length as f32 * SCALE,
        drop_height: (spacing * f64::from(longest)) as f32,
        alternate_nodes: flag(m, "AlternateNodes"),
    };
    vec![(generator, between_ends(&tp, m.attr("Dir") == Some("R")))]
}

/// Where a boxed model (Window Frame, Wreath, Spinner) sits, as `BoxedScreenLocation` places it:
/// centered on `WorldPos`, scaled by `ScaleX/Y`, then turned by `RotateX`, `RotateY` and
/// `RotateZ` in turn, which PixelFlow's transform does in the same order. (xLights turns by a
/// slightly short pi, a few millionths of a degree off; well within the import tolerance.)
struct Boxed {
    position: Vec3,
    rotation_deg: Vec3,
    scale_x: f64,
    scale_y: f64,
}

fn boxed(m: &XmlModel) -> Boxed {
    // xLights ignores a negative scale and a turn of more than half way round.
    let scale = |k: &str| Some(float(m, k, 1.0)).filter(|&v| v >= 0.0).unwrap_or(1.0);
    let turn = |k: &str| {
        Some(float(m, k, 0.0))
            .filter(|v| (-180.0..=180.0).contains(v))
            .unwrap_or(0.0) as f32
    };
    Boxed {
        position: world_pos(m),
        rotation_deg: Vec3::new(turn("RotateX"), turn("RotateY"), turn("RotateZ")),
        scale_x: scale("ScaleX"),
        scale_y: scale("ScaleY"),
    }
}

/// Where a string starts, from xLights' `Dir` and `StartSide`: (from the left, from the bottom).
fn start_side(m: &XmlModel) -> (bool, bool) {
    (
        m.attr("Dir") != Some("R"),
        m.attr("StartSide").is_none_or(|s| s == "B"),
    )
}

/// A round boxed model's size, folded into `steps` of its own (one xLights unit each) across
/// its radius, and any difference between its height and width kept in its transform.
fn round_placement(b: &Boxed, steps: f64) -> Option<(f32, Transform)> {
    if b.scale_x <= 0.0 {
        return None;
    }
    let radius = (steps * b.scale_x) as f32 * SCALE;
    let squash = Vec3::new(1.0, (b.scale_y / b.scale_x) as f32, 1.0);
    Some((radius, transform(b.position, b.rotation_deg, squash)))
}

/// Where a sphere, cube or tree sits in PixelFlow, upright as xLights' 3D view draws it
/// (`T * Rz * Ry * Rx * S`, which is PixelFlow's transform): the slight tilt of xLights' 2D view
/// is left out. A scale equal in all three directions is folded into the shape's size (`unit` in
/// layout units per xLights unit); otherwise it stays in the transform and `unit` is one xLights
/// unit.
fn solid_placement(m: &XmlModel, b: &Boxed, scale_mul: [f64; 3]) -> Option<(f32, Transform)> {
    let raw = |k: &str, i: usize| {
        let v = float(m, k, 1.0) * scale_mul[i];
        if v < 0.0 || !v.is_finite() { 1.0 } else { v }
    };
    let (sx, sy, sz) = (raw("ScaleX", 0), raw("ScaleY", 1), raw("ScaleZ", 2));
    if sx == sy && sy == sz {
        let unit = (sx as f32) * SCALE;
        (unit > 0.0).then(|| (unit, transform(b.position, b.rotation_deg, Vec3::ONE)))
    } else {
        let scale = Vec3::new(sx as f32, sy as f32, sz as f32);
        Some((SCALE, transform(b.position, b.rotation_deg, scale)))
    }
}

/// xLights' corner names for (from the left, from the bottom).
fn corner(ltor: bool, btot: bool) -> pf_model::Corner {
    use pf_model::Corner::*;
    match (ltor, btot) {
        (true, true) => BottomLeft,
        (false, true) => BottomRight,
        (true, false) => TopLeft,
        (false, false) => TopRight,
    }
}

/// `SphereModel` with one light per node: vertical-matrix strands (strings times strands per
/// string) of pixels round a globe whose radius is `max(columns, rows) / 1.8 / 2` units, between
/// `StartLatitude` and `EndLatitude`, `Degrees` round. xLights zig-zags within each string, which
/// is PixelFlow's zig-zag for one string or an even number of strands per string, and no zig-zag
/// for one strand per string; both are offered and the one that fits is kept.
fn sphere(m: &XmlModel) -> Vec<Candidate> {
    use pf_model::StrandStyle;
    let strings = parm(m, "NumStrings", "parm1", 1);
    let nps = parm(m, "NodesPerString", "parm2", 1);
    let sps = parm(m, "StrandsPerString", "parm3", 1).max(1).min(nps.max(1));
    if strings <= 0 || nps <= 0 {
        return Vec::new();
    }
    let rows = nps / sps;
    let (Some(columns), Some(rows)) = (count(strings.saturating_mul(sps)), count(rows)) else {
        return Vec::new();
    };
    if u64::from(columns) * u64::from(rows) > u64::from(pf_model::MAX_PROP_NODES) || rows == 0 {
        return Vec::new();
    }
    // Files from before xLights' version 8 spheres keep their old size.
    let version = m.attr("versionNumber").unwrap_or("");
    let mut scale_mul = [1.0; 3];
    if version.is_empty() || strtol0(version) < 8 {
        let mx = rows.max(columns);
        let r = rows as f32 / mx as f32;
        scale_mul = [f64::from(r / 1.8), f64::from(r), f64::from(r / 1.8)];
    }
    let Some((unit, place)) = solid_placement(m, &boxed(m), scale_mul) else {
        return Vec::new();
    };
    let radius = (f64::from(columns.max(rows)) / 1.8 / 2.0) as f32 * unit;
    let (ltor, btot) = start_side(m);
    let styles: &[StrandStyle] = if flag(m, "AlternateNodes") {
        &[StrandStyle::AlternatePixel]
    } else if flag(m, "NoZig") {
        &[StrandStyle::NoZigZag]
    } else {
        &[StrandStyle::ZigZag, StrandStyle::NoZigZag]
    };
    styles
        .iter()
        .map(|&strand_style| {
            let g = Generator::Sphere {
                columns,
                rows,
                radius,
                start_latitude: int(m, "StartLatitude", -86) as f32,
                end_latitude: int(m, "EndLatitude", 86) as f32,
                degrees: int(m, "Degrees", 360) as f32,
                start: corner(ltor, btot),
                strand_style,
            };
            (g, place)
        })
        .collect()
}

/// `CubeModel` with one light per node, as a cube (not a cylinder, and without offset rows):
/// cells one xLights unit apart, wired by its start corner, style and strand style. xLights
/// doesn't center a cube with an even count; the position takes up the half step.
fn cube(m: &XmlModel) -> Vec<Candidate> {
    use pf_model::{CubeStart::*, CubeStyle::*, StrandStyle};
    let (w, h, d) = (
        parm(m, "CubeWidth", "parm1", 1),
        parm(m, "CubeHeight", "parm2", 1),
        parm(m, "CubeDepth", "parm3", 1),
    );
    let (Some(width), Some(height), Some(depth)) = (count(w), count(h), count(d)) else {
        return Vec::new();
    };
    if width == 0
        || height == 0
        || depth == 0
        || u64::from(width) * u64::from(height) * u64::from(depth) > u64::from(pf_model::MAX_PROP_NODES)
        || int(m, "CubeShape", 0) == 1
        || (int(m, "CubeRowOffset", 0) != 0 && depth > 1)
    {
        return Vec::new();
    }
    let pick = |key: &str, names: &[&str]| names.iter().position(|n| m.text(key, "") == *n).unwrap_or(0);
    let start = [
        FrontBottomLeft,
        FrontBottomRight,
        FrontTopLeft,
        FrontTopRight,
        BackBottomLeft,
        BackBottomRight,
        BackTopLeft,
        BackTopRight,
    ][pick(
        "Start",
        &[
            "Front Bottom Left",
            "Front Bottom Right",
            "Front Top Left",
            "Front Top Right",
            "Back Bottom Left",
            "Back Bottom Right",
            "Back Top Left",
            "Back Top Right",
        ],
    )];
    let style = [
        VerticalFrontBack,
        VerticalLeftRight,
        HorizontalFrontBack,
        HorizontalLeftRight,
        StackedFrontBack,
        StackedLeftRight,
    ][pick(
        "Style",
        &[
            "Vertical Front/Back",
            "Vertical Left/Right",
            "Horizontal Front/Back",
            "Horizontal Left/Right",
            "Stacked Front/Back",
            "Stacked Left/Right",
        ],
    )];
    let strand_style = [
        StrandStyle::ZigZag,
        StrandStyle::NoZigZag,
        StrandStyle::AlternatePixel,
    ][pick("StrandPerLine", &["Zig Zag", "No Zig Zag", "Aternate Pixel"])];
    let Some((unit, mut place)) = solid_placement(m, &boxed(m), [1.0; 3]) else {
        return Vec::new();
    };
    let half = |n: u32| ((n as f32 - 1.0) / 2.0 - (n / 2) as f32) * unit;
    place.position = pf_geometry::apply_transform(Vec3::new(half(width), half(height), half(depth)), &place);
    let g = Generator::Cube {
        width,
        height,
        depth,
        spacing: unit,
        start,
        style,
        strand_style,
        strand_per_layer: m.text("StrandPerLayer", "FALSE") == "TRUE",
    };
    vec![(g, place)]
}

/// `CustomModel` with one layer: its grid cropped to the occupied cells (xLights centers a
/// custom model on them, as PixelFlow centers its grid), one unit a cell, scaled by `ScaleX/Y`.
/// Numbering with gaps, several layers or a very large grid stay measured.
fn custom(m: &XmlModel) -> Vec<Candidate> {
    let Some(cells) = custom_cells(m.text("CustomModel", ""), m.text("CustomModelCompressed", "")) else {
        return Vec::new();
    };
    let Some(first) = cells.first() else {
        return Vec::new();
    };
    if cells.iter().any(|c| c[3] != first[3]) {
        return Vec::new();
    }
    let (min_r, max_r) = cells
        .iter()
        .fold((i64::MAX, i64::MIN), |(a, b), c| (a.min(c[1]), b.max(c[1])));
    let (min_c, max_c) = cells
        .iter()
        .fold((i64::MAX, i64::MIN), |(a, b), c| (a.min(c[2]), b.max(c[2])));
    let (columns, rows) = (max_c - min_c + 1, max_r - min_r + 1);
    if columns.saturating_mul(rows) > i64::from(pf_model::MAX_PROP_NODES) {
        return Vec::new();
    }
    let mut grid = vec![0u32; (columns * rows) as usize];
    for c in &cells {
        let Ok(value) = u32::try_from(c[0]) else {
            return Vec::new();
        };
        grid[((c[1] - min_r) * columns + c[2] - min_c) as usize] = value;
    }
    let b = boxed(m);
    let scale = Vec3::new(b.scale_x as f32, b.scale_y as f32, b.scale_x as f32) * SCALE;
    let g = Generator::CustomGrid {
        columns: columns as u32,
        rows: rows as u32,
        cells: grid,
    };
    vec![(g, transform(b.position, b.rotation_deg, scale))]
}

/// Strings and strands of a matrix-wired model (`MatrixModel`): (strands, pixels per strand),
/// the zig-zag choices to try, and where wiring starts. xLights zig-zags inside each string, which
/// is PixelFlow's zig-zag for one string or an even number of strands per string, and no zig-zag
/// for one strand per string; both are offered and the one that fits is kept.
struct Strands {
    strands: u32,
    per_strand: u32,
    serpentine: &'static [bool],
    ltor: bool,
    btot: bool,
}

fn strands(m: &XmlModel) -> Option<Strands> {
    let strings = parm(m, "NumStrings", "parm1", 1);
    let nps = parm(m, "NodesPerString", "parm2", 1);
    if strings <= 0 || nps <= 0 || flag(m, "AlternateNodes") {
        return None;
    }
    let sps = parm(m, "StrandsPerString", "parm3", 1).max(1).min(nps);
    let (strands, per_strand) = (count(strings.saturating_mul(sps))?, count(nps / sps)?);
    if per_strand == 0 || u64::from(strands) * u64::from(per_strand) > u64::from(pf_model::MAX_PROP_NODES) {
        return None;
    }
    let (ltor, btot) = start_side(m);
    let serpentine: &'static [bool] = if flag(m, "NoZig") {
        &[false]
    } else {
        &[true, false]
    };
    Some(Strands {
        strands,
        per_strand,
        serpentine,
        ltor,
        btot,
    })
}

/// `MatrixModel` (Horiz Matrix / Vert Matrix) with one light per node, wired from any corner
/// along rows or columns: PixelFlow's matrix. xLights' grid is one unit a step and not centered
/// for an even count; the position takes up the half step, and an even scale folds into the size.
fn matrix(m: &XmlModel) -> Vec<Candidate> {
    use pf_model::{MatrixWiring, Orientation};
    let Some(s) = strands(m) else {
        return Vec::new();
    };
    let vertical = m.display_as.trim() == "Vert Matrix" || m.attr("Vertical") == Some("true");
    let (columns, rows) = if vertical {
        (s.strands, s.per_strand)
    } else {
        (s.per_strand, s.strands)
    };
    let b = boxed(m);
    if b.scale_x <= 0.0 || b.scale_y <= 0.0 {
        return Vec::new();
    }
    let (unit, scale) = if b.scale_x == b.scale_y {
        ((b.scale_x as f32) * SCALE, Vec3::ONE)
    } else {
        (SCALE, Vec3::new(b.scale_x as f32, b.scale_y as f32, 1.0))
    };
    let mut place = transform(b.position, b.rotation_deg, scale);
    let half = |n: u32| ((n as f32 - 1.0) / 2.0 - (n / 2) as f32) * unit;
    place.position = pf_geometry::apply_transform(Vec3::new(half(columns), half(rows), 0.0), &place);
    let size = |n: u32| (n.saturating_sub(1).max(1) as f32) * unit;
    s.serpentine
        .iter()
        .map(|&serpentine| {
            let g = Generator::Matrix {
                columns,
                rows,
                width: size(columns),
                height: size(rows),
                wiring: MatrixWiring {
                    start: corner(s.ltor, s.btot),
                    orientation: if vertical {
                        Orientation::Vertical
                    } else {
                        Orientation::Horizontal
                    },
                    serpentine,
                },
            };
            (g, place)
        })
        .collect()
}

/// `TreeModel` with one light per node and strands running up (vertical strands from the
/// bottom left, no spiral, no first-strand offset): a round tree of `render_ht = 3 × rows` units
/// tall and `render_ht / 1.8` across the base (tapering by `TreeBottomTopRatio`), starting at
/// `-degrees / 2 + TreeRotation`; or a flat (ribbon) tree `2 × rows` tall, `4 (5) × strands` across
/// the base and `0.9 × strands` across the top. Upright: xLights' 2D tilt (`TreePerspective`) is
/// left out.
fn tree(m: &XmlModel) -> Vec<Candidate> {
    use pf_model::TreeStyle;
    if m.text("StrandDir", "Vertical") != "Vertical"
        || float(m, "TreeSpiralRotations", 0.0) as f32 != 0.0
        || int(m, "exportFirstStrand", 0) > 1
    {
        return Vec::new();
    }
    let Some(s) = strands(m) else {
        return Vec::new();
    };
    if !s.ltor || !s.btot {
        return Vec::new();
    }
    let t = m.display_as.trim();
    let degrees = if t == "Tree" {
        match int(m, "TreeType", 0) {
            1 => 0,
            2 => -1,
            _ => int(m, "TreeDegrees", 360),
        }
    } else {
        match t.split_once(' ').map(|(_, rest)| rest) {
            Some("Flat") => 0,
            Some("Ribbon") => -1,
            Some(tok) => strtol0(tok),
            None => 360,
        }
    };
    let Some((unit, mut place)) = solid_placement(m, &boxed(m), [1.0; 3]) else {
        return Vec::new();
    };
    let (bw, bh) = (f64::from(s.strands), f64::from(s.per_strand));
    let (style, height, base, top, start_angle) = if degrees > 0 {
        let render_ht = bh * 3.0;
        let mut radius = render_ht / 1.8 / 2.0;
        let ratio = f64::from(float(m, "TreeBottomTopRatio", 6.0) as f32);
        let mut top = if ratio != 0.0 {
            radius / ratio.abs()
        } else {
            radius
        };
        if ratio < 0.0 {
            std::mem::swap(&mut top, &mut radius);
        }
        let rotation = f64::from(float(m, "TreeRotation", 3.0) as f32);
        (
            TreeStyle::Round,
            render_ht,
            radius,
            top,
            -(degrees as f64) / 2.0 + rotation,
        )
    } else if degrees == -1 {
        (TreeStyle::Ribbon, bh * 2.0, bw / 2.0 * 5.0, bw / 2.0 * 0.9, 0.0)
    } else {
        (TreeStyle::Flat, bh * 2.0, bw / 2.0 * 4.0, bw / 2.0 * 0.9, 0.0)
    };
    let u = f64::from(unit);
    place.position = pf_geometry::apply_transform(Vec3::new(0.0, -(height * u / 2.0) as f32, 0.0), &place);
    s.serpentine
        .iter()
        .map(|&serpentine| {
            let g = Generator::Tree {
                strings: s.strands,
                nodes_per_string: s.per_strand,
                height: (height * u) as f32,
                base_radius: (base * u) as f32,
                top_radius: (top * u) as f32,
                serpentine,
                style,
                degrees: if degrees > 0 { degrees as f32 } else { 360.0 },
                start_angle: start_angle as f32,
            };
            (g, place)
        })
        .collect()
}

/// `WindowFrameModel` with one light per node: its pixel counts, start corner and direction,
/// and its size (xLights spaces the sides' pixels one unit apart and makes the frame two wider
/// than its longer row, before scaling).
fn window_frame(m: &XmlModel) -> Vec<Candidate> {
    let (Some(top), Some(sides), Some(bottom)) = (
        count(parm(m, "TopNodes", "parm1", 0).max(0)),
        count(parm(m, "SideNodes", "parm2", 0).max(0)),
        count(parm(m, "BottomNodes", "parm3", 0).max(0)),
    ) else {
        return Vec::new();
    };
    if u64::from(top) + 2 * u64::from(sides) + u64::from(bottom) > u64::from(pf_model::MAX_PROP_NODES) {
        return Vec::new();
    }
    let start = match start_side(m) {
        (true, true) => pf_model::Corner::BottomLeft,
        (false, true) => pf_model::Corner::BottomRight,
        (true, false) => pf_model::Corner::TopLeft,
        (false, false) => pf_model::Corner::TopRight,
    };
    let counter_clockwise = !matches!(m.text("Rotation", "CW"), "CW" | "Clockwise");
    let b = boxed(m);
    let across = f64::from(top.max(bottom) + 2);
    let up = (i64::from(sides) - 1).max(1) as f64;
    let generator = Generator::WindowFrame {
        top,
        sides,
        bottom,
        width: (across * b.scale_x) as f32 * SCALE,
        height: (up * b.scale_y) as f32 * SCALE,
        start,
        counter_clockwise,
    };
    vec![(generator, transform(b.position, b.rotation_deg, Vec3::ONE))]
}

/// `WreathModel` with one light per node: its lights, where they start and which way they go,
/// and its grid of `lights / 2` steps across the radius. xLights draws a wreath of an odd number
/// of lights a step down and left of its middle, so that one is moved to match.
fn wreath(m: &XmlModel) -> Vec<Candidate> {
    let lights = parm(m, "NumStrings", "parm1", 1)
        .max(0)
        .saturating_mul(parm(m, "NodesPerString", "parm2", 50).max(0));
    let Some(nodes) = count(lights).filter(|&n| n > 0 && n <= pf_model::MAX_PROP_NODES) else {
        return Vec::new();
    };
    let (ltor, btot) = start_side(m);
    let b = boxed(m);
    let Some((radius, mut place)) = round_placement(&b, f64::from((nodes / 2).max(1))) else {
        return Vec::new();
    };
    if nodes % 2 == 1 {
        let step = (b.scale_x as f32) * SCALE;
        place.position = pf_geometry::apply_transform(Vec3::new(-step, -step, 0.0), &place);
    }
    let generator = Generator::Wreath {
        nodes,
        radius,
        start_at_bottom: btot,
        counter_clockwise: ltor != btot,
    };
    vec![(generator, place)]
}

/// `SpinnerModel` with one light per node: its arms (strings times arms per string), pixels per
/// arm, hollow middle, start angle, arc and options, and its size: the outermost pixel is half a
/// unit beyond the arm's last step past the hollow middle, before scaling.
fn spinner(m: &XmlModel) -> Vec<Candidate> {
    let strings = parm(m, "NumStrings", "parm1", 1).max(0);
    let (Some(arms), Some(nodes_per_arm)) = (
        count(strings.saturating_mul(parm(m, "ArmsPerString", "parm3", 1).max(0))),
        count(parm(m, "NodesPerArm", "parm2", 1).max(0)),
    ) else {
        return Vec::new();
    };
    let (hollow, arc) = (int(m, "Hollow", 20), int(m, "Arc", 360));
    if arms > pf_model::MAX_SPINNER_ARMS
        || u64::from(arms) * u64::from(nodes_per_arm) > u64::from(pf_model::MAX_PROP_NODES)
        || !(0..=i64::from(pf_model::MAX_SPINNER_HOLLOW)).contains(&hollow)
        || !(1..=360).contains(&arc)
    {
        return Vec::new();
    }
    let (ltor, btot) = start_side(m);
    let npa = f64::from(nodes_per_arm);
    let steps = npa - 0.5 + hollow as f64 * 2.0 * npa / 100.0;
    let Some((radius, place)) = round_placement(&boxed(m), steps) else {
        return Vec::new();
    };
    let generator = Generator::Spinner {
        arms,
        nodes_per_arm,
        hollow: hollow as u32,
        start_angle: int(m, "StartAngle", 0) as f32,
        arc: arc as f32,
        zig_zag: flag(m, "ZigZag"),
        alternate: flag(m, "Alternate"),
        from_center: !btot,
        clockwise: !ltor,
        radius,
    };
    vec![(generator, place)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::geometry;

    fn model(display_as: &str, attrs: &[(&str, &str)]) -> XmlModel {
        XmlModel {
            name: "m".into(),
            display_as: display_as.into(),
            attrs: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..XmlModel::default()
        }
    }

    /// Today's measured import of the model: each node at the center of its lights.
    fn measured(m: &XmlModel) -> Vec<Vec3> {
        geometry(m)
            .nodes
            .iter()
            .map(|n| {
                let k = n.points.len().max(1) as f32;
                let (x, y) = n.points.iter().fold((0.0, 0.0), |(x, y), p| (x + p[0], y + p[1]));
                Vec3::new(x / k * SCALE, y / k * SCALE, 0.0)
            })
            .collect()
    }

    #[track_caller]
    fn imports_as(display_as: &str, attrs: &[(&str, &str)]) -> Generator {
        let m = model(display_as, attrs);
        let points = measured(&m);
        assert!(!points.is_empty());
        let (g, _) =
            editable(&m, &points).unwrap_or_else(|| panic!("{display_as} {attrs:?} kept its points"));
        g
    }

    #[track_caller]
    fn stays_measured(display_as: &str, attrs: &[(&str, &str)]) {
        let m = model(display_as, attrs);
        assert_eq!(editable(&m, &measured(&m)), None, "{display_as} {attrs:?}");
    }

    #[test]
    fn poly_lines_keep_their_points_and_segment_counts() {
        let g = imports_as(
            "Poly Line",
            &[
                ("NumPoints", "3"),
                ("PointData", "0,0,0,100,0,0,100,50,0"),
                ("Seg1", "10"),
                ("Seg2", "4"),
                ("WorldPosX", "200"),
                ("WorldPosY", "30"),
                ("ScaleX", "1.5"),
                ("ScaleY", "0.5"),
            ],
        );
        let Generator::PolyLine {
            vertices,
            segments,
            spread_nodes,
        } = g
        else {
            panic!("{g:?}")
        };
        assert_eq!(
            vertices,
            [Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 0.5, 0.0)]
        );
        assert_eq!(segments, [PolySegment::straight(10), PolySegment::straight(4)]);
        assert_eq!(spread_nodes, None);
    }

    #[test]
    fn auto_distributed_and_reversed_poly_lines_import_as_poly_lines() {
        let spread = imports_as(
            "Poly Line",
            &[
                ("NumPoints", "4"),
                ("PointData", "0,0,0,30,40,0,60,0,0,90,-20,0"),
                ("NodesPerString", "37"),
            ],
        );
        assert!(matches!(
            spread,
            Generator::PolyLine {
                spread_nodes: Some(37),
                ..
            }
        ));
        let reversed = imports_as(
            "Poly Line",
            &[
                ("NumPoints", "3"),
                ("PointData", "0,0,0,100,0,0,100,50,0"),
                ("Seg1", "10"),
                ("Seg2", "4"),
                ("Dir", "R"),
            ],
        );
        let Generator::PolyLine {
            vertices, segments, ..
        } = reversed
        else {
            panic!()
        };
        assert_eq!(vertices[0], Vec3::new(1.0, 0.5, 0.0));
        assert_eq!(segments[0].nodes, 4);
        // A nearly flat line is drawn flat, as xLights draws it.
        imports_as(
            "Poly Line",
            &[
                ("NumPoints", "2"),
                ("PointData", "0,0.04,0,100,0.0,0"),
                ("Seg1", "5"),
            ],
        );
    }

    #[test]
    fn poly_lines_xlights_lays_out_differently_keep_their_points() {
        // Icicle drops, and a corner given to one side.
        stays_measured(
            "Poly Line",
            &[
                ("NumPoints", "2"),
                ("PointData", "0,0,0,100,0,0"),
                ("Seg1", "5"),
                ("DropPattern", "3,2"),
            ],
        );
        // Two lights per node: each node is drawn at the middle of its lights, which is where
        // a one-light node goes.
        imports_as(
            "Poly Line",
            &[
                ("NumPoints", "2"),
                ("PointData", "0,0,0,100,0,0"),
                ("Seg1", "5"),
                ("LightsPerNode", "2"),
            ],
        );
        stays_measured(
            "Poly Line",
            &[
                ("NumPoints", "3"),
                ("PointData", "0,0,0,100,0,0,100,50,0"),
                ("Seg1", "4"),
                ("Seg2", "4"),
                ("Corner2", "Leading Segment"),
            ],
        );
    }

    #[test]
    fn single_lines_import_as_lines_between_their_two_points() {
        let attrs = [
            ("parm1", "1"),
            ("parm2", "20"),
            ("WorldPosX", "100"),
            ("WorldPosY", "50"),
            ("X2", "300"),
            ("Y2", "-80"),
        ];
        let g = imports_as("Single Line", &attrs);
        assert_eq!(
            g,
            Generator::Line {
                nodes: 20,
                length: (3.0f32 * 3.0 + 0.8 * 0.8).sqrt()
            }
        );
        let mut reversed = attrs.to_vec();
        reversed.push(("Dir", "R"));
        imports_as("Single Line", &reversed);
        let mut tipped = attrs.to_vec();
        tipped.push(("Z2", "120"));
        imports_as("Single Line", &tipped);
        // Two strings wired from the far end each: not one straight run.
        stays_measured(
            "Single Line",
            &[("parm1", "2"), ("parm2", "3"), ("Dir", "R"), ("X2", "300")],
        );
    }

    /// `base` with `more` set, later settings replacing earlier ones as in an XML element.
    fn with<'a>(base: &[(&'a str, &'a str)], more: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        let mut attrs: Vec<_> = base
            .iter()
            .filter(|(k, _)| !more.iter().any(|(m, _)| m == k))
            .copied()
            .collect();
        attrs.extend_from_slice(more);
        attrs
    }

    /// A row of three candy canes 200 wide, sloping up to the right a little.
    const CANES: [(&str, &str); 6] = [
        ("NumCanes", "3"),
        ("NodesPerCane", "18"),
        ("WorldPosX", "300"),
        ("WorldPosY", "80"),
        ("X2", "200"),
        ("Y2", "30"),
    ];

    #[test]
    fn candy_canes_import_as_candy_canes_between_their_two_points() {
        let g = imports_as("Candy Canes", &CANES);
        let Generator::CandyCanes {
            canes,
            nodes_per_cane,
            width,
            height,
            reverse,
            sticks,
            ..
        } = g
        else {
            panic!("{g:?}")
        };
        assert_eq!(
            (canes, nodes_per_cane, height, reverse, sticks),
            (3, 18, 1.0, false, false)
        );
        assert!((width - (2.0f32 * 2.0 + 0.3 * 0.3).sqrt()).abs() < 1e-5);
        // Midway between the two points, turned toward point 2.
        let m = model("Candy Canes", &CANES);
        let (_, t) = editable(&m, &measured(&m)).unwrap();
        assert!((t.position - Vec3::new(4.0, 0.95, 0.0)).length() < 1e-5, "{t:?}");
        assert_eq!((t.rotation_deg.x, t.rotation_deg.y), (0.0, 0.0));
        assert!(
            (t.rotation_deg.z - 0.3f32.atan2(2.0).to_degrees()).abs() < 1e-4,
            "{t:?}"
        );
    }

    #[test]
    fn candy_canes_set_up_every_way_xlights_offers_import_exactly() {
        let variants: [&[(&str, &str)]; 10] = [
            &[("CandyCaneReverse", "true")],
            &[("CandyCaneSticks", "true")],
            &[("AlternateNodes", "true")],
            &[("Height", "1.6"), ("CandyCaneHeight", "0.7")],
            &[("CandyCaneSkew", "12")],
            &[("Angle", "-8")],
            &[("Dir", "R")],
            &[
                ("Dir", "R"),
                ("CandyCaneReverse", "true"),
                ("CandyCaneSkew", "20"),
            ],
            // Drawn right to left, and tipped back.
            &[("X2", "-150"), ("Y2", "-40"), ("RotateX", "25")],
            // In depth, wired from the right, from the old parm attributes.
            &[("Z2", "60"), ("RotateX", "-10"), ("Dir", "R"), ("NumCanes", "2")],
        ];
        for more in variants {
            imports_as("Candy Canes", &with(&CANES, more));
        }
        // Nearly level: xLights draws it level, so it imports level.
        let m = model("Candy Canes", &with(&CANES, &[("Y2", "0.2")]));
        let (_, t) = editable(&m, &measured(&m)).unwrap();
        assert_eq!(t.rotation_deg, Vec3::ZERO);
        // Right to left: a half turn about Y rather than about X and Z.
        let m = model("Candy Canes", &with(&CANES, &[("X2", "-200"), ("Y2", "0")]));
        let (_, t) = editable(&m, &measured(&m)).unwrap();
        assert_eq!(t.rotation_deg, Vec3::new(0.0, 180.0, 0.0));
        // Wired from the right: the same canes, hooks and lean, the first cane on the right.
        let attrs = with(&CANES, &[("Dir", "R"), ("CandyCaneSkew", "20")]);
        let g = imports_as("Candy Canes", &attrs);
        assert!(matches!(
            g,
            Generator::CandyCanes {
                reverse: false,
                skew_deg: 20.0,
                start_right: true,
                ..
            }
        ));
        assert_eq!(placed("Candy Canes", &attrs).rotation_deg.y, 0.0);
        let g = imports_as(
            "Candy Canes",
            &with(&CANES, &[("Dir", "R"), ("CandyCaneReverse", "true")]),
        );
        assert!(matches!(
            g,
            Generator::CandyCanes {
                reverse: true,
                start_right: true,
                ..
            }
        ));
        imports_as("Candy Canes", &[("parm1", "2"), ("parm2", "12"), ("X2", "100")]);
    }

    #[test]
    fn candy_canes_with_dumb_strings_or_several_lights_per_pixel_keep_their_points() {
        stays_measured(
            "Candy Canes",
            &with(&CANES, &[("StringType", "Single Color Red")]),
        );
        stays_measured("Candy Canes", &with(&CANES, &[("LightsPerNode", "3")]));
    }

    /// Two strings of icicles hanging along 300 of slightly sloping gutter.
    const ICICLES: [(&str, &str); 8] = [
        ("NumStrings", "2"),
        ("NodesPerString", "40"),
        ("DropPattern", "3,4,5,4"),
        ("WorldPosX", "100"),
        ("WorldPosY", "500"),
        ("X2", "300"),
        ("Y2", "-20"),
        ("Height", "-0.5"),
    ];

    #[test]
    fn icicles_import_as_icicles_hanging_from_their_line() {
        let g = imports_as("Icicles", &ICICLES);
        let Generator::Icicles {
            strings,
            lights_per_string,
            ref drops,
            width,
            drop_height,
            alternate_nodes,
        } = g
        else {
            panic!("{g:?}")
        };
        assert_eq!((strings, lights_per_string, alternate_nodes), (2, 40, false));
        assert_eq!(drops, &[3, 4, 5, 4]);
        let length = (3.0f32 * 3.0 + 0.2 * 0.2).sqrt();
        assert!((width - length).abs() < 1e-5);
        // 40 pixels fill eleven drops of 3,4,5,4 (the last one partly) on each of two strings:
        // columns 0 to 21. Height -0.5 spaces the pixels half a column apart, and the longest
        // drop, of 5, hangs 4 of those.
        assert!(
            (drop_height - 0.5 * length / 21.0 * 4.0).abs() < 1e-5,
            "{drop_height}"
        );
    }

    #[test]
    fn icicles_set_up_every_way_xlights_offers_import_exactly() {
        let variants: [&[(&str, &str)]; 8] = [
            &[("AlternateNodes", "true")],
            &[("Dir", "R")],
            &[("DropPattern", "2,0,6,1")],
            &[("DropPattern", "")],
            &[("Height", "0.8")],
            &[("X2", "-250"), ("Y2", "60"), ("RotateX", "15")],
            &[("NumStrings", "1"), ("NodesPerString", "3"), ("DropPattern", "5")],
            &[
                ("NumStrings", "3"),
                ("NodesPerString", "7"),
                ("Z2", "40"),
                ("Dir", "R"),
            ],
        ];
        for more in variants {
            imports_as("Icicles", &with(&ICICLES, more));
        }
        imports_as("Icicles", &[("parm1", "2"), ("parm2", "9"), ("X2", "100")]);
    }

    #[test]
    fn sheared_or_dumb_icicles_and_huge_drops_keep_their_points() {
        stays_measured("Icicles", &with(&ICICLES, &[("Shear", "0.3")]));
        stays_measured(
            "Icicles",
            &with(&ICICLES, &[("StringType", "Single Color White")]),
        );
        stays_measured("Icicles", &with(&ICICLES, &[("DropPattern", "3,2000")]));
    }

    /// Where an imported model's prop is placed.
    #[track_caller]
    fn placed(display_as: &str, attrs: &[(&str, &str)]) -> Transform {
        let m = model(display_as, attrs);
        editable(&m, &measured(&m)).unwrap().1
    }

    /// Every way xLights starts a string: from either side, at the top or the bottom.
    const STARTS: [&[(&str, &str)]; 4] = [
        &[],
        &[("Dir", "R")],
        &[("StartSide", "T")],
        &[("Dir", "R"), ("StartSide", "T")],
    ];

    /// A window frame 10 across the top, 6 up each side and 8 across the bottom, stretched.
    const FRAME: [(&str, &str); 7] = [
        ("TopNodes", "10"),
        ("SideNodes", "6"),
        ("BottomNodes", "8"),
        ("WorldPosX", "200"),
        ("WorldPosY", "300"),
        ("ScaleX", "4"),
        ("ScaleY", "5"),
    ];

    #[test]
    fn window_frames_import_as_window_frames_of_the_same_size() {
        let g = imports_as("Window Frame", &FRAME);
        let Generator::WindowFrame {
            top,
            sides,
            bottom,
            width,
            height,
            start,
            counter_clockwise,
        } = g
        else {
            panic!("{g:?}")
        };
        assert_eq!((top, sides, bottom), (10, 6, 8));
        assert_eq!((start, counter_clockwise), (pf_model::Corner::BottomLeft, false));
        // Two wider than the top's ten, and five steps tall, at 4 and 5 units a step.
        assert!((width - 0.48).abs() < 1e-6, "{width}");
        assert!((height - 0.25).abs() < 1e-6, "{height}");
        let t = placed("Window Frame", &FRAME);
        assert_eq!(t, transform(Vec3::new(2.0, 3.0, 0.0), Vec3::ZERO, Vec3::ONE));
    }

    #[test]
    fn window_frames_started_and_turned_every_way_import_exactly() {
        let corners = [
            pf_model::Corner::BottomLeft,
            pf_model::Corner::BottomRight,
            pf_model::Corner::TopLeft,
            pf_model::Corner::TopRight,
        ];
        for (start, corner) in STARTS.iter().zip(corners) {
            for rotation in [None, Some("CCW"), Some("Counter Clockwise"), Some("Clockwise")] {
                let mut attrs = with(&FRAME, start);
                attrs.extend(rotation.map(|r| ("Rotation", r)));
                let g = imports_as("Window Frame", &attrs);
                assert!(
                    matches!(g, Generator::WindowFrame { start: s, counter_clockwise: ccw, .. }
                        if s == corner && ccw == matches!(rotation, Some("CCW" | "Counter Clockwise"))),
                    "{attrs:?}: {g:?}"
                );
            }
        }
        let variants: [&[(&str, &str)]; 7] = [
            &[("RotateZ", "30")],
            &[("RotateX", "20"), ("RotateY", "-15"), ("RotateZ", "100")],
            // One pixel on top sits a step in from the corner, as in xLights.
            &[("TopNodes", "1"), ("BottomNodes", "1")],
            &[("SideNodes", "1"), ("Rotation", "CCW")],
            &[("SideNodes", "0"), ("StartSide", "T")],
            &[("TopNodes", "0"), ("Dir", "R")],
            &[("ScaleY", "0.5"), ("ScaleZ", "3")],
        ];
        for more in variants {
            imports_as("Window Frame", &with(&FRAME, more));
        }
        imports_as("Window Frame", &[("parm1", "4"), ("parm2", "3"), ("parm3", "4")]);
    }

    #[test]
    fn window_frames_with_dumb_strings_keep_their_points() {
        stays_measured(
            "Window Frame",
            &with(&FRAME, &[("StringType", "Single Color Red")]),
        );
    }

    /// A wreath of 30 lights, 3 units a grid step.
    const WREATH: [(&str, &str); 6] = [
        ("NumStrings", "1"),
        ("NodesPerString", "30"),
        ("WorldPosX", "500"),
        ("WorldPosY", "250"),
        ("ScaleX", "3"),
        ("ScaleY", "3"),
    ];

    #[test]
    fn wreaths_import_as_wreaths_on_the_same_grid() {
        let g = imports_as("Wreath", &WREATH);
        // xLights starts at the bottom and goes clockwise unless told otherwise; 15 grid steps
        // of 3 units each across the radius.
        assert_eq!(
            g,
            Generator::Wreath {
                nodes: 30,
                radius: 0.45,
                start_at_bottom: true,
                counter_clockwise: false,
            }
        );
        let t = placed("Wreath", &WREATH);
        assert_eq!(t, transform(Vec3::new(5.0, 2.5, 0.0), Vec3::ZERO, Vec3::ONE));
        let flips = [(true, false), (true, true), (false, true), (false, false)];
        for (start, (bottom, ccw)) in STARTS.iter().zip(flips) {
            let g = imports_as("Wreath", &with(&WREATH, start));
            assert!(
                matches!(g, Generator::Wreath { start_at_bottom: b, counter_clockwise: c, .. }
                    if b == bottom && c == ccw),
                "{start:?}: {g:?}"
            );
        }
    }

    #[test]
    fn wreaths_of_any_count_size_and_turn_import_exactly() {
        let variants: [&[(&str, &str)]; 7] = [
            // An odd count: xLights draws it a grid step down and left of its middle.
            &[("NodesPerString", "7")],
            &[("NodesPerString", "1")],
            &[("NumStrings", "2"), ("NodesPerString", "25")],
            &[("ScaleY", "1.5")],
            &[("NodesPerString", "9"), ("RotateZ", "40"), ("ScaleY", "2")],
            &[("RotateX", "30"), ("RotateY", "20")],
            &[("NodesPerString", "6"), ("Dir", "R")],
        ];
        for more in variants {
            imports_as("Wreath", &with(&WREATH, more));
        }
        let t = placed("Wreath", &with(&WREATH, &[("NodesPerString", "7")]));
        assert!((t.position - Vec3::new(4.97, 2.47, 0.0)).length() < 1e-5, "{t:?}");
        imports_as("Wreath", &[("parm1", "1"), ("parm2", "12")]);
        stays_measured("Wreath", &with(&WREATH, &[("StringType", "Single Color White")]));
    }

    /// Two strings of three arms of ten pixels, a fifth hollow, 5 units a pixel step.
    const SPINNER: [(&str, &str); 8] = [
        ("NumStrings", "2"),
        ("NodesPerArm", "10"),
        ("ArmsPerString", "3"),
        ("Hollow", "20"),
        ("WorldPosX", "100"),
        ("WorldPosY", "400"),
        ("ScaleX", "5"),
        ("ScaleY", "5"),
    ];

    #[test]
    fn spinners_import_as_spinners_of_the_same_size() {
        let g = imports_as("Spinner", &SPINNER);
        let Generator::Spinner {
            arms,
            nodes_per_arm,
            hollow,
            start_angle,
            arc,
            zig_zag,
            alternate,
            from_center,
            clockwise,
            radius,
        } = g
        else {
            panic!("{g:?}")
        };
        assert_eq!((arms, nodes_per_arm, hollow), (6, 10, 20));
        assert_eq!((start_angle, arc), (0.0, 360.0));
        assert_eq!(
            (zig_zag, alternate, from_center, clockwise),
            (false, false, false, false)
        );
        // The outermost pixel is 9.5 steps plus a hollow of 4 out, at 5 units a step.
        assert!((radius - 0.675).abs() < 1e-6, "{radius}");
        let t = placed("Spinner", &SPINNER);
        assert_eq!(t, transform(Vec3::new(1.0, 4.0, 0.0), Vec3::ZERO, Vec3::ONE));
    }

    #[test]
    fn spinners_set_up_every_way_xlights_offers_import_exactly() {
        for start in STARTS {
            imports_as("Spinner", &with(&SPINNER, start));
        }
        let g = imports_as("Spinner", &with(&SPINNER, &[("Dir", "R"), ("StartSide", "T")]));
        assert!(matches!(
            g,
            Generator::Spinner {
                from_center: true,
                clockwise: true,
                ..
            }
        ));
        let variants: [&[(&str, &str)]; 10] = [
            &[("StartAngle", "35")],
            &[("Arc", "180"), ("StartAngle", "-90")],
            &[("Arc", "90"), ("Dir", "R")],
            &[("ZigZag", "true")],
            &[("ZigZag", "true"), ("StartSide", "T")],
            &[("Alternate", "true"), ("StartSide", "T")],
            &[("Hollow", "0")],
            &[("Hollow", "80"), ("ScaleY", "2")],
            &[("RotateZ", "-60"), ("RotateX", "10")],
            // Many arms: their angles drift as xLights' do.
            &[
                ("NumStrings", "1"),
                ("ArmsPerString", "250"),
                ("NodesPerArm", "3"),
            ],
        ];
        for more in variants {
            imports_as("Spinner", &with(&SPINNER, more));
        }
        imports_as("Spinner", &[("parm1", "1"), ("parm2", "8"), ("parm3", "4")]);
    }

    #[test]
    fn spinners_past_pixelflow_limits_or_with_dumb_strings_keep_their_points() {
        stays_measured("Spinner", &with(&SPINNER, &[("Hollow", "150")]));
        stays_measured("Spinner", &with(&SPINNER, &[("Hollow", "-10")]));
        stays_measured("Spinner", &with(&SPINNER, &[("Arc", "0")]));
        stays_measured("Spinner", &with(&SPINNER, &[("Arc", "400")]));
        stays_measured(
            "Spinner",
            &with(&SPINNER, &[("NumStrings", "1"), ("ArmsPerString", "1001")]),
        );
        stays_measured("Spinner", &with(&SPINNER, &[("StringType", "Single Color Red")]));
    }

    /// A version-8 sphere of 6 strings of 12, scaled evenly.
    const SPHERE: [(&str, &str); 8] = [
        ("NumStrings", "6"),
        ("NodesPerString", "12"),
        ("versionNumber", "8"),
        ("WorldPosX", "400"),
        ("WorldPosY", "300"),
        ("ScaleX", "20"),
        ("ScaleY", "20"),
        ("ScaleZ", "20"),
    ];

    #[test]
    fn spheres_import_upright_as_spheres() {
        let g = imports_as("Sphere", &SPHERE);
        // Six strands, each its own string, run the same way: no zig-zag.
        assert!(matches!(
            g,
            Generator::Sphere {
                columns: 6,
                rows: 12,
                start: pf_model::Corner::BottomLeft,
                strand_style: pf_model::StrandStyle::NoZigZag,
                ..
            }
        ));
        // Upright: xLights' 2D tilt is left out, and the depth checked too.
        let t = placed("Sphere", &SPHERE);
        assert_eq!(t.rotation_deg, Vec3::ZERO);
        assert_eq!(t.scale, Vec3::ONE);
        // Deeper than it is tall, and tipped back: kept as it is, depth included.
        let deep = with(&SPHERE, &[("ScaleZ", "40"), ("RotateX", "20")]);
        imports_as("Sphere", &deep);
        let t = placed("Sphere", &deep);
        assert_eq!((t.rotation_deg.x, t.scale.z), (20.0, 40.0));
        // One string zig-zagging over its strands; every start corner; alternating; stretched
        // flat; older files; part way round between other latitudes.
        let one = with(
            &SPHERE,
            &[
                ("NumStrings", "1"),
                ("NodesPerString", "72"),
                ("StrandsPerString", "6"),
            ],
        );
        assert!(matches!(
            imports_as("Sphere", &one),
            Generator::Sphere {
                strand_style: pf_model::StrandStyle::ZigZag,
                ..
            }
        ));
        for start in STARTS {
            imports_as("Sphere", &with(&one, start));
        }
        imports_as("Sphere", &with(&SPHERE, &[("AlternateNodes", "true")]));
        imports_as("Sphere", &with(&SPHERE, &[("ScaleY", "35"), ("RotateZ", "30")]));
        imports_as("Sphere", &with(&SPHERE, &[("versionNumber", "7")]));
        imports_as(
            "Sphere",
            &with(
                &SPHERE,
                &[
                    ("StartLatitude", "-40"),
                    ("EndLatitude", "70"),
                    ("Degrees", "270"),
                ],
            ),
        );
        // Three strands per string zig-zag inside each string only.
        stays_measured(
            "Sphere",
            &with(
                &SPHERE,
                &[
                    ("NumStrings", "2"),
                    ("NodesPerString", "36"),
                    ("StrandsPerString", "3"),
                ],
            ),
        );
        imports_as("Sphere", &with(&SPHERE, &[("ScaleY", "35"), ("RotateX", "20")]));
    }

    /// A 3 × 4 × 2 cube (even width and depth, so xLights doesn't center it), scaled evenly.
    const CUBE: [(&str, &str); 8] = [
        ("CubeWidth", "4"),
        ("CubeHeight", "3"),
        ("CubeDepth", "2"),
        ("WorldPosX", "100"),
        ("WorldPosY", "50"),
        ("ScaleX", "10"),
        ("ScaleY", "10"),
        ("ScaleZ", "10"),
    ];

    #[test]
    fn cubes_import_as_cubes_in_every_style() {
        let g = imports_as("Cube", &CUBE);
        assert!(matches!(
            g,
            Generator::Cube {
                width: 4,
                height: 3,
                depth: 2,
                ..
            }
        ));
        let starts = [
            "Front Bottom Left",
            "Front Top Right",
            "Back Bottom Right",
            "Back Top Left",
        ];
        let styles = [
            "Vertical Front/Back",
            "Vertical Left/Right",
            "Horizontal Front/Back",
            "Horizontal Left/Right",
            "Stacked Front/Back",
            "Stacked Left/Right",
        ];
        for start in starts {
            for style in styles {
                for strand in ["Zig Zag", "No Zig Zag", "Aternate Pixel"] {
                    let attrs = with(
                        &CUBE,
                        &[("Start", start), ("Style", style), ("StrandPerLine", strand)],
                    );
                    imports_as("Cube", &attrs);
                }
            }
        }
        imports_as(
            "Cube",
            &with(&CUBE, &[("StrandPerLayer", "TRUE"), ("RotateZ", "15")]),
        );
        imports_as("Cube", &with(&CUBE, &[("ScaleX", "20")]));
        stays_measured("Cube", &with(&CUBE, &[("CubeShape", "1")]));
        stays_measured("Cube", &with(&CUBE, &[("CubeRowOffset", "1")]));
    }

    /// 4 strings of 20 (even counts, so xLights' grid isn't centered), scaled evenly.
    const MATRIX: [(&str, &str); 7] = [
        ("parm1", "4"),
        ("parm2", "20"),
        ("parm3", "1"),
        ("WorldPosX", "640"),
        ("WorldPosY", "380"),
        ("ScaleX", "8"),
        ("ScaleY", "8"),
    ];

    #[test]
    fn matrices_import_as_matrices_wired_from_any_corner() {
        let g = imports_as("Vert Matrix", &MATRIX);
        assert!(matches!(
            g,
            Generator::Matrix {
                columns: 4,
                rows: 20,
                wiring: pf_model::MatrixWiring {
                    start: pf_model::Corner::BottomLeft,
                    orientation: pf_model::Orientation::Vertical,
                    serpentine: false,
                },
                ..
            }
        ));
        let one = with(&MATRIX, &[("parm1", "1"), ("parm2", "80"), ("parm3", "4")]);
        for display in ["Vert Matrix", "Horiz Matrix"] {
            for start in STARTS {
                imports_as(display, &with(&MATRIX, start));
                let g = imports_as(display, &with(&one, start));
                assert!(matches!(
                    g,
                    Generator::Matrix {
                        wiring: pf_model::MatrixWiring { serpentine: true, .. },
                        ..
                    }
                ));
            }
        }
        imports_as(
            "Horiz Matrix",
            &with(&MATRIX, &[("ScaleY", "3"), ("RotateZ", "20"), ("NoZig", "true")]),
        );
        stays_measured("Vert Matrix", &with(&MATRIX, &[("AlternateNodes", "true")]));
        stays_measured(
            "Vert Matrix",
            &with(&MATRIX, &[("parm1", "2"), ("parm2", "60"), ("parm3", "3")]),
        );
    }

    /// A mega tree: 8 strings of 50 standing up from the bottom left.
    const TREE: [(&str, &str); 7] = [
        ("parm1", "8"),
        ("parm2", "50"),
        ("parm3", "1"),
        ("WorldPosX", "900"),
        ("WorldPosY", "300"),
        ("ScaleX", "3"),
        ("ScaleY", "3"),
    ];

    #[test]
    fn trees_import_as_round_flat_and_ribbon_trees() {
        // Upright, without xLights' 2D tilt, and as deep as xLights' ScaleZ makes it.
        let t = placed("Tree 360", &TREE);
        assert_eq!(t.rotation_deg, Vec3::ZERO);
        assert_eq!(t.scale, Vec3::new(3.0, 3.0, 1.0));
        let g = imports_as("Tree 360", &TREE);
        assert!(matches!(
            g,
            Generator::Tree {
                strings: 8,
                nodes_per_string: 50,
                serpentine: false,
                style: pf_model::TreeStyle::Round,
                ..
            }
        ));
        let Generator::Tree {
            start_angle, degrees, ..
        } = imports_as("Tree 180", &TREE)
        else {
            panic!()
        };
        assert_eq!((degrees, start_angle), (180.0, -87.0));
        let flat = imports_as("Tree Flat", &TREE);
        assert!(matches!(
            flat,
            Generator::Tree {
                style: pf_model::TreeStyle::Flat,
                ..
            }
        ));
        let ribbon = imports_as("Tree Ribbon", &TREE);
        assert!(matches!(
            ribbon,
            Generator::Tree {
                style: pf_model::TreeStyle::Ribbon,
                ..
            }
        ));
        imports_as("Tree", &with(&TREE, &[("TreeType", "0"), ("TreeDegrees", "270")]));
        imports_as(
            "Tree 360",
            &with(&TREE, &[("parm1", "1"), ("parm2", "400"), ("parm3", "8")]),
        );
        imports_as(
            "Tree 360",
            &with(
                &TREE,
                &[
                    ("TreeBottomTopRatio", "-3"),
                    ("TreePerspective", "0.4"),
                    ("TreeRotation", "20"),
                ],
            ),
        );
        stays_measured("Tree 360", &with(&TREE, &[("TreeSpiralRotations", "1.5")]));
        stays_measured("Tree 360", &with(&TREE, &[("StartSide", "T")]));
        stays_measured("Tree 360", &with(&TREE, &[("StrandDir", "Horizontal")]));
    }

    #[test]
    fn custom_models_import_as_custom_grids_cropped_to_their_pixels() {
        let base = [
            ("CustomModel", ",,,;,1,,2;,,,;3,,4,"),
            ("WorldPosX", "300"),
            ("WorldPosY", "200"),
            ("ScaleX", "12"),
            ("ScaleY", "8"),
        ];
        let g = imports_as("Custom", &base);
        // Rows 1 to 3, columns 0 to 3.
        assert_eq!(
            g,
            Generator::CustomGrid {
                columns: 4,
                rows: 3,
                cells: vec![0, 1, 0, 2, 0, 0, 0, 0, 3, 0, 4, 0],
            }
        );
        // A pixel over two squares sits between them, in both.
        imports_as(
            "Custom",
            &with(&base, &[("CustomModel", "1,1,2;3,,4"), ("RotateZ", "45")]),
        );
        imports_as(
            "Custom",
            &with(&base, &[("CustomModelCompressed", "1,0,0;2,0,3;3,2,1")]),
        );
        // Numbering with a gap leaves the channels a gap too; two layers have depth.
        stays_measured("Custom", &with(&base, &[("CustomModel", "1,,5")]));
        stays_measured(
            "Custom",
            &with(&base, &[("CustomModel", "1,2|3,4"), ("Depth", "2")]),
        );
    }
}
