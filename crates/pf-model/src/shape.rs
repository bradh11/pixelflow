//! Prop shapes: parametric generators or measured point sets.

use crate::Vec3;
use serde::{Deserialize, Serialize};

/// Corner of a matrix where the first pixel is wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Corner {
    #[default]
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
}

/// Direction the wiring runs first in a matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    /// Strings run along rows.
    #[default]
    Horizontal,
    /// Strings run along columns.
    Vertical,
}

/// How a matrix's pixels are wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MatrixWiring {
    pub start: Corner,
    pub orientation: Orientation,
    /// When true, every other string runs in the opposite direction (zig-zag).
    pub serpentine: bool,
}

impl Default for MatrixWiring {
    fn default() -> Self {
        Self {
            start: Corner::BottomLeft,
            orientation: Orientation::Horizontal,
            serpentine: true,
        }
    }
}

/// One stretch of a poly line, from one of its points to the next.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolySegment {
    /// Pixels on this stretch (unused while the line spreads its pixels evenly).
    pub nodes: u32,
    /// The two control points of a curved stretch (a cubic Bézier from this point to the next,
    /// as xLights draws curves), in prop-local coordinates; `None` for a straight stretch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve: Option<[Vec3; 2]>,
}

impl PolySegment {
    pub fn straight(nodes: u32) -> Self {
        Self { nodes, curve: None }
    }
}

fn one() -> f32 {
    1.0
}

fn full_turn() -> f32 {
    360.0
}

/// Parametric prop shapes. Positions are produced by `pf-geometry`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Generator {
    /// Straight run of evenly spaced pixels along X, centered on the origin.
    Line { nodes: u32, length: f32 },
    /// Half-ellipse from left to right; origin at the base center.
    Arch { nodes: u32, width: f32, height: f32 },
    /// Ring starting at the top and running clockwise; centered.
    Circle { nodes: u32, radius: f32 },
    /// Grid of pixels wired according to `wiring`; centered.
    Matrix {
        columns: u32,
        rows: u32,
        width: f32,
        height: f32,
        #[serde(default)]
        wiring: MatrixWiring,
    },
    /// Cone of strings running bottom to top; origin at the base center.
    Tree {
        strings: u32,
        nodes_per_string: u32,
        height: f32,
        base_radius: f32,
        top_radius: f32,
        #[serde(default)]
        serpentine: bool,
    },
    /// Star outline with `points` tips, pixels spaced evenly along the outline; centered.
    Star {
        points: u32,
        nodes: u32,
        outer_radius: f32,
        inner_radius: f32,
    },
    /// A line through any number of points, which can bend and curve (xLights' Poly Line).
    /// Pixels run from the first point to the last. Each stretch between two points has its own
    /// pixel count, spaced evenly with half a gap at each end (so the pixels stay evenly spaced
    /// across a corner), unless `spread_nodes` is set: then that many pixels are spread evenly
    /// along the whole line, the first on the first point, as xLights' "auto distribute" does.
    PolyLine {
        vertices: Vec<Vec3>,
        /// One per stretch: `vertices.len() - 1` of them.
        segments: Vec<PolySegment>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spread_nodes: Option<u32>,
    },
    /// A row of candy canes standing between two ends (xLights' Candy Canes), laid out as xLights
    /// does: each cane is a third as wide as it has pixels, two-thirds of its pixels run up the
    /// stick and the rest round the hook, with a gap of two pixels' spacing between canes. The
    /// whole row is scaled so it is `width` wide; the origin is midway between the two ends, at
    /// the foot of the canes. Pixels run cane by cane, left to right, each up its stick and then
    /// round its hook.
    CandyCanes {
        canes: u32,
        nodes_per_cane: u32,
        /// Distance between the two ends the canes stand between.
        width: f32,
        /// xLights' `Height`: scales the canes' height and the size of their hooks (1 is normal).
        #[serde(default = "one")]
        height: f32,
        /// xLights' `CandyCaneHeight`: stretches the canes taller, hooks included (1 is normal).
        #[serde(default = "one")]
        cane_height: f32,
        /// Hooks point left instead of right.
        #[serde(default)]
        reverse: bool,
        /// Straight sticks, no hooks.
        #[serde(default)]
        sticks: bool,
        /// Each cane's pixels go up every other spot and come back down the ones between.
        #[serde(default)]
        alternate_nodes: bool,
        /// How far each cane leans from upright, in degrees (counter-clockwise).
        #[serde(default)]
        skew_deg: f32,
    },
    /// Icicles hanging from a line between two ends (xLights' Icicles). Each string's pixels fill
    /// drops in turn, the drop sizes repeating `drops` from its start (a drop of 0 leaves a gap),
    /// each drop one column right of the last; the columns are spread evenly over `width`. The
    /// origin is midway along the line, and the drops hang below it.
    Icicles {
        strings: u32,
        lights_per_string: u32,
        /// Pixels in each drop, repeating ("3,4,5,4"). No drop with pixels reads as `[5]`.
        drops: Vec<u32>,
        /// Distance between the two ends.
        width: f32,
        /// How far below the line the longest drop's last pixel hangs (pixels in a drop are this
        /// over one less than the longest drop apart; for drops of one, this is the spacing).
        /// Negative makes the drops stand up. From xLights' `Height`, in layout units:
        /// `-Height * length / (columns - 1) * (longest drop - 1)`.
        drop_height: f32,
        /// Each drop's pixels go down every other spot and come back up the ones between.
        #[serde(default)]
        alternate_nodes: bool,
    },
    /// A window frame (xLights' Window Frame): one string that goes once round the frame from
    /// the `start` corner, `top`, `sides` (each) and `bottom` pixels along its edges, spaced as
    /// xLights spaces them. The edge the string starts along takes the corner pixels: starting up
    /// a side, the sides run from the bottom corner to the top one and the top and bottom pixels
    /// sit between them; starting along the top or bottom, those take the corners instead.
    /// `width` is between the two sides and `height` between the top and bottom; centered.
    WindowFrame {
        top: u32,
        sides: u32,
        bottom: u32,
        width: f32,
        height: f32,
        #[serde(default)]
        start: Corner,
        /// Runs counter-clockwise round the frame instead of clockwise.
        #[serde(default)]
        counter_clockwise: bool,
    },
    /// A wreath (xLights' Wreath): a ring like a circle, but each pixel rounded to the nearest
    /// point of a square grid `radius / (nodes / 2)` apart, as xLights places them. Starts at the
    /// top (or bottom) and runs clockwise (or counter-clockwise); centered.
    Wreath {
        nodes: u32,
        radius: f32,
        #[serde(default)]
        start_at_bottom: bool,
        #[serde(default)]
        counter_clockwise: bool,
    },
    /// A spinner (xLights' Spinner): `arms` straight arms radiating from a hollow middle, each
    /// with `nodes_per_arm` pixels a step apart, the first arm pointing down (turned by
    /// `start_angle`) and the rest spread counter-clockwise (or clockwise) over `arc` degrees.
    /// The hollow middle is `hollow` percent of twice an arm's pixels, in steps; `radius` is
    /// from the middle to the outermost pixel. Centered.
    Spinner {
        arms: u32,
        nodes_per_arm: u32,
        /// xLights' `Hollow`, in percent.
        hollow: u32,
        /// Degrees counter-clockwise from straight down to the first arm.
        #[serde(default)]
        start_angle: f32,
        /// Degrees the arms are spread over: 360 is all the way round (the last arm a step short
        /// of the first); less than that puts the last arm at the end of the arc.
        #[serde(default = "full_turn")]
        arc: f32,
        /// Every other arm runs the other way along itself.
        #[serde(default)]
        zig_zag: bool,
        /// Each arm's pixels go out every other spot and come back in on the ones between
        /// (starting in the middle, whichever end `from_center` picks).
        #[serde(default)]
        alternate: bool,
        /// Each arm's pixels start in the middle instead of at its tip.
        #[serde(default)]
        from_center: bool,
        /// The arms follow each other clockwise instead of counter-clockwise.
        #[serde(default)]
        clockwise: bool,
        radius: f32,
    },
    /// Free-form grid. `cells` is row-major starting at the top row;
    /// 0 is an empty cell and n places node n (1-based) in that cell.
    CustomGrid {
        columns: u32,
        rows: u32,
        cells: Vec<u32>,
    },
}

impl Generator {
    /// Number of pixels this generator produces.
    pub fn node_count(&self) -> u32 {
        match self {
            Generator::Line { nodes, .. }
            | Generator::Arch { nodes, .. }
            | Generator::Circle { nodes, .. }
            | Generator::Wreath { nodes, .. }
            | Generator::Star { nodes, .. } => *nodes,
            Generator::WindowFrame {
                top, sides, bottom, ..
            } => top
                .saturating_add(sides.saturating_mul(2))
                .saturating_add(*bottom),
            Generator::Spinner {
                arms, nodes_per_arm, ..
            } => arms.saturating_mul(*nodes_per_arm),
            Generator::Matrix { columns, rows, .. } => columns.saturating_mul(*rows),
            Generator::Tree {
                strings,
                nodes_per_string,
                ..
            } => strings.saturating_mul(*nodes_per_string),
            Generator::CandyCanes {
                canes,
                nodes_per_cane,
                ..
            } => canes.saturating_mul(*nodes_per_cane),
            Generator::Icicles {
                strings,
                lights_per_string,
                ..
            } => strings.saturating_mul(*lights_per_string),
            Generator::CustomGrid { cells, .. } => cells.iter().copied().max().unwrap_or(0),
            Generator::PolyLine {
                segments,
                spread_nodes,
                ..
            } => spread_nodes
                .unwrap_or_else(|| segments.iter().fold(0u32, |sum, s| sum.saturating_add(s.nodes))),
        }
    }
}

/// Where measured positions came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Provenance {
    CameraMap,
    Import,
    Manual,
}

/// The source of a prop's pixel positions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ShapeSource {
    /// Positions computed from parameters.
    Generator(Generator),
    /// Positions captured directly (camera mapping, import), in prop-local coordinates.
    Measured {
        points: Vec<Vec3>,
        provenance: Provenance,
    },
}

impl ShapeSource {
    /// Number of pixels in the shape.
    pub fn node_count(&self) -> u32 {
        match self {
            ShapeSource::Generator(g) => g.node_count(),
            ShapeSource::Measured { points, .. } => u32::try_from(points.len()).unwrap_or(u32::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_counts() {
        let cases = [
            (
                Generator::Line {
                    nodes: 50,
                    length: 10.0,
                },
                50,
            ),
            (
                Generator::Matrix {
                    columns: 20,
                    rows: 10,
                    width: 4.0,
                    height: 2.0,
                    wiring: MatrixWiring::default(),
                },
                200,
            ),
            (
                Generator::Tree {
                    strings: 16,
                    nodes_per_string: 50,
                    height: 5.0,
                    base_radius: 1.0,
                    top_radius: 0.1,
                    serpentine: false,
                },
                800,
            ),
            (
                Generator::CustomGrid {
                    columns: 3,
                    rows: 1,
                    cells: vec![2, 0, 5],
                },
                5,
            ),
        ];
        for (generator, expected) in cases {
            assert_eq!(generator.node_count(), expected, "{generator:?}");
        }
    }

    fn poly(spread_nodes: Option<u32>) -> Generator {
        Generator::PolyLine {
            vertices: vec![Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.0)],
            segments: vec![
                PolySegment::straight(10),
                PolySegment {
                    nodes: 5,
                    curve: Some([Vec3::new(1.5, 0.2, 0.0), Vec3::new(1.5, 0.8, 0.0)]),
                },
            ],
            spread_nodes,
        }
    }

    #[test]
    fn poly_line_counts_its_segments_or_its_spread() {
        assert_eq!(poly(None).node_count(), 15);
        assert_eq!(poly(Some(40)).node_count(), 40);
        let huge = Generator::PolyLine {
            vertices: vec![Vec3::ZERO; 3],
            segments: vec![PolySegment::straight(u32::MAX), PolySegment::straight(2)],
            spread_nodes: None,
        };
        assert_eq!(huge.node_count(), u32::MAX);
    }

    #[test]
    fn poly_line_json_is_camel_case_and_leaves_out_what_is_unset() {
        let json = serde_json::to_value(ShapeSource::Generator(poly(None))).unwrap();
        assert_eq!(json["type"], "polyLine");
        assert_eq!(json["vertices"][1]["x"], 1.0);
        assert_eq!(json["segments"][0], serde_json::json!({ "nodes": 10 }));
        assert_eq!(json["segments"][1]["curve"][0]["x"], 1.5);
        assert!(json.get("spreadNodes").is_none());
        let back: ShapeSource = serde_json::from_value(json).unwrap();
        assert_eq!(back, ShapeSource::Generator(poly(None)));
        let spread = serde_json::to_value(poly(Some(7))).unwrap();
        assert_eq!(spread["spreadNodes"], 7);
    }

    #[test]
    fn candy_canes_and_icicles_count_and_round_trip_their_settings() {
        let canes = Generator::CandyCanes {
            canes: 3,
            nodes_per_cane: 18,
            width: 3.0,
            height: 1.0,
            cane_height: 1.0,
            reverse: true,
            sticks: false,
            alternate_nodes: false,
            skew_deg: 0.0,
        };
        assert_eq!(canes.node_count(), 54);
        let json = serde_json::to_value(ShapeSource::Generator(canes.clone())).unwrap();
        assert_eq!(json["type"], "candyCanes");
        assert_eq!(json["nodesPerCane"], 18);
        assert_eq!(json["caneHeight"], 1.0);
        assert_eq!(json["skewDeg"], 0.0);
        assert_eq!(
            serde_json::from_value::<ShapeSource>(json).unwrap(),
            ShapeSource::Generator(canes)
        );
        // Left-out options read as off, sizes as 1.
        let plain: Generator =
            serde_json::from_str(r#"{ "type": "candyCanes", "canes": 2, "nodesPerCane": 9, "width": 2 }"#)
                .unwrap();
        assert!(matches!(
            plain,
            Generator::CandyCanes {
                height: 1.0,
                cane_height: 1.0,
                reverse: false,
                sticks: false,
                alternate_nodes: false,
                skew_deg: 0.0,
                ..
            }
        ));

        let icicles = Generator::Icicles {
            strings: 2,
            lights_per_string: 80,
            drops: vec![3, 4, 5, 4],
            width: 4.0,
            drop_height: 0.4,
            alternate_nodes: false,
        };
        assert_eq!(icicles.node_count(), 160);
        let json = serde_json::to_value(&icicles).unwrap();
        assert_eq!(json["type"], "icicles");
        assert_eq!(json["lightsPerString"], 80);
        assert_eq!(json["drops"], serde_json::json!([3, 4, 5, 4]));
        assert_eq!(json["dropHeight"].as_f64().unwrap() as f32, 0.4);
        assert_eq!(serde_json::from_value::<Generator>(json).unwrap(), icicles);
        let huge = Generator::Icicles {
            strings: u32::MAX,
            lights_per_string: 2,
            drops: vec![1],
            width: 1.0,
            drop_height: 1.0,
            alternate_nodes: false,
        };
        assert_eq!(huge.node_count(), u32::MAX);
    }

    #[test]
    fn window_frames_wreaths_and_spinners_count_and_round_trip_their_settings() {
        let frame = Generator::WindowFrame {
            top: 10,
            sides: 8,
            bottom: 12,
            width: 2.0,
            height: 1.5,
            start: Corner::TopRight,
            counter_clockwise: true,
        };
        assert_eq!(frame.node_count(), 38);
        let json = serde_json::to_value(ShapeSource::Generator(frame.clone())).unwrap();
        assert_eq!(json["type"], "windowFrame");
        assert_eq!(json["start"], "topRight");
        assert_eq!(json["counterClockwise"], true);
        assert_eq!(
            serde_json::from_value::<ShapeSource>(json).unwrap(),
            ShapeSource::Generator(frame)
        );
        let plain: Generator = serde_json::from_str(
            r#"{ "type": "windowFrame", "top": 1, "sides": 2, "bottom": 3, "width": 1, "height": 1 }"#,
        )
        .unwrap();
        assert!(matches!(
            plain,
            Generator::WindowFrame {
                start: Corner::BottomLeft,
                counter_clockwise: false,
                ..
            }
        ));
        let huge = Generator::WindowFrame {
            top: u32::MAX,
            sides: u32::MAX,
            bottom: 1,
            width: 1.0,
            height: 1.0,
            start: Corner::BottomLeft,
            counter_clockwise: false,
        };
        assert_eq!(huge.node_count(), u32::MAX);

        let wreath = Generator::Wreath {
            nodes: 50,
            radius: 0.8,
            start_at_bottom: true,
            counter_clockwise: false,
        };
        assert_eq!(wreath.node_count(), 50);
        let json = serde_json::to_value(&wreath).unwrap();
        assert_eq!(json["type"], "wreath");
        assert_eq!(json["startAtBottom"], true);
        assert_eq!(serde_json::from_value::<Generator>(json).unwrap(), wreath);

        let spinner = Generator::Spinner {
            arms: 6,
            nodes_per_arm: 20,
            hollow: 20,
            start_angle: 15.0,
            arc: 180.0,
            zig_zag: true,
            alternate: false,
            from_center: true,
            clockwise: false,
            radius: 1.0,
        };
        assert_eq!(spinner.node_count(), 120);
        let json = serde_json::to_value(&spinner).unwrap();
        assert_eq!(json["type"], "spinner");
        assert_eq!(json["nodesPerArm"], 20);
        assert_eq!(json["startAngle"], 15.0);
        assert_eq!(json["zigZag"], true);
        assert_eq!(json["fromCenter"], true);
        assert_eq!(serde_json::from_value::<Generator>(json).unwrap(), spinner);
        // Left out: no turn, a full circle, and every option off.
        let plain: Generator = serde_json::from_str(
            r#"{ "type": "spinner", "arms": 4, "nodesPerArm": 5, "hollow": 0, "radius": 1 }"#,
        )
        .unwrap();
        assert!(matches!(
            plain,
            Generator::Spinner {
                start_angle: 0.0,
                arc: 360.0,
                zig_zag: false,
                alternate: false,
                from_center: false,
                clockwise: false,
                ..
            }
        ));
    }

    #[test]
    fn generator_shape_json_is_flat_and_camel_case() {
        let shape = ShapeSource::Generator(Generator::Arch {
            nodes: 50,
            width: 4.0,
            height: 2.0,
        });
        let json = serde_json::to_value(&shape).unwrap();
        assert_eq!(json["source"], "generator");
        assert_eq!(json["type"], "arch");
        assert_eq!(json["nodes"], 50);
        let back: ShapeSource = serde_json::from_value(json).unwrap();
        assert_eq!(back, shape);
    }

    #[test]
    fn measured_shape_round_trips_and_counts_points() {
        let shape = ShapeSource::Measured {
            points: vec![Vec3::ZERO, Vec3::ONE],
            provenance: Provenance::CameraMap,
        };
        assert_eq!(shape.node_count(), 2);
        let json = serde_json::to_string(&shape).unwrap();
        assert_eq!(serde_json::from_str::<ShapeSource>(&json).unwrap(), shape);
    }

    #[test]
    fn partial_matrix_wiring_fills_defaults() {
        let wiring: MatrixWiring = serde_json::from_str(r#"{ "start": "topLeft" }"#).unwrap();
        assert_eq!(
            wiring,
            MatrixWiring {
                start: Corner::TopLeft,
                ..MatrixWiring::default()
            }
        );
        assert!(wiring.serpentine);
    }

    #[test]
    fn tree_fields_use_camel_case() {
        let json = serde_json::to_value(Generator::Tree {
            strings: 2,
            nodes_per_string: 3,
            height: 1.0,
            base_radius: 1.0,
            top_radius: 0.5,
            serpentine: true,
        })
        .unwrap();
        assert_eq!(json["nodesPerString"], 3);
        assert_eq!(json["baseRadius"], 1.0);
    }
}
