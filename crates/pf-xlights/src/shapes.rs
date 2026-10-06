//! Editable shapes for imported models: each xLights model type PixelFlow has a shape for is
//! imported as that shape (its parameters read from the model) instead of as measured points,
//! so it can be edited like a prop drawn in PixelFlow.
//!
//! A shape is only used when `pf-geometry` puts every pixel exactly where xLights does (the
//! measured points, in channel order); otherwise the model keeps its measured points. So an
//! import always looks and maps exactly as before, whichever way a model is set up.

use crate::geometry::{Affine, parse_points, rot_from_x_axis, strtod, strtol0};
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
        _ => Vec::new(),
    };
    candidates.into_iter().find(|(g, t)| fits(g, t, points))
}

/// True when the shape, placed by `transform`, puts every pixel on `points` (front view).
fn fits(generator: &Generator, transform: &Transform, points: &[Vec3]) -> bool {
    if generator.node_count() as usize != points.len() || points.is_empty() {
        return false;
    }
    let mut prop = Prop::new("", ShapeSource::Generator(generator.clone()));
    prop.transform = *transform;
    pf_geometry::world_positions(&prop)
        .iter()
        .zip(points)
        .all(|(a, b)| (a.x - b.x).abs() <= TOLERANCE && (a.y - b.y).abs() <= TOLERANCE)
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
    let mirror = m.attr("Dir") == Some("R");
    let tp = three_point(m);
    let generator = Generator::CandyCanes {
        canes,
        nodes_per_cane,
        width: tp.length as f32 * SCALE,
        height: float(m, "Height", 1.0) as f32,
        cane_height: float(m, "CandyCaneHeight", 1.0) as f32,
        reverse: flag(m, "CandyCaneReverse") != mirror,
        sticks: flag(m, "CandyCaneSticks"),
        alternate_nodes: flag(m, "AlternateNodes"),
        skew_deg: if mirror { -skew } else { skew },
    };
    vec![(generator, between_ends(&tp, mirror))]
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
        // Wired from the right: the mirror image of canes hooking the other way.
        let g = imports_as(
            "Candy Canes",
            &with(&CANES, &[("Dir", "R"), ("CandyCaneSkew", "20")]),
        );
        assert!(matches!(
            g,
            Generator::CandyCanes {
                reverse: true,
                skew_deg: -20.0,
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
}
