//! Editable shapes for imported models: each xLights model type PixelFlow has a shape for is
//! imported as that shape (its parameters read from the model) instead of as measured points,
//! so it can be edited like a prop drawn in PixelFlow.
//!
//! A shape is only used when `pf-geometry` puts every pixel exactly where xLights does (the
//! measured points, in channel order); otherwise the model keeps its measured points. So an
//! import always looks and maps exactly as before, whichever way a model is set up.

use crate::geometry::{parse_points, strtod, strtol0};
use crate::model::XmlModel;
use pf_model::{Generator, PolySegment, Prop, ShapeSource, Transform, Vec3};

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
}
