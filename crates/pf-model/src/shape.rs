//! Prop shapes: parametric generators or measured point sets.

use crate::Vec3;
use serde::{Deserialize, Serialize};

/// Corner of a matrix where the first pixel is wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum Corner {
    #[default]
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
}

/// Direction the wiring runs first in a matrix: strings along rows (horizontal) or along
/// columns (vertical).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "schema",
    schemars(description = "Strings run along rows or columns.")
)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    /// Strings run along rows.
    #[cfg_attr(feature = "schema", schemars(description = ""))]
    #[default]
    Horizontal,
    /// Strings run along columns.
    #[cfg_attr(feature = "schema", schemars(description = ""))]
    Vertical,
}

/// How a matrix's pixels are wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", default)]
pub struct MatrixWiring {
    pub start: Corner,
    pub orientation: Orientation,
    /// When true, every other string runs in the opposite direction (zig-zag).
    #[cfg_attr(feature = "schema", schemars(description = "Every other string runs back."))]
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

/// How the pixels run along each strand of a sphere or cube (xLights' strand styles): every
/// other strand back the other way (zig-zag), every strand the same way, or each strand's pixels
/// out on every other spot and back on the ones between (alternate pixel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "schema",
    schemars(
        description = "Zig-zag: every other strand runs back; alternatePixel: every other pixel out, the rest back."
    )
)]
#[serde(rename_all = "camelCase")]
pub enum StrandStyle {
    /// Every other strand runs back the other way.
    #[cfg_attr(feature = "schema", schemars(description = ""))]
    #[default]
    ZigZag,
    /// Every strand runs the same way.
    #[cfg_attr(feature = "schema", schemars(description = ""))]
    NoZigZag,
    /// Each strand's pixels go out every other spot and come back on the ones between.
    #[cfg_attr(feature = "schema", schemars(description = ""))]
    AlternatePixel,
}

/// The corner of a cube its first pixel is at (xLights' cube `Start`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(description = "The corner of the first pixel."))]
#[serde(rename_all = "camelCase")]
pub enum CubeStart {
    #[default]
    FrontBottomLeft,
    FrontBottomRight,
    FrontTopLeft,
    FrontTopRight,
    BackBottomLeft,
    BackBottomRight,
    BackTopLeft,
    BackTopRight,
}

/// Which way a cube's strands run and how they stack into layers (xLights' cube `Style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "schema",
    schemars(description = "Which way strands run and layers stack.")
)]
#[serde(rename_all = "camelCase")]
pub enum CubeStyle {
    #[default]
    VerticalFrontBack,
    VerticalLeftRight,
    HorizontalFrontBack,
    HorizontalLeftRight,
    StackedFrontBack,
    StackedLeftRight,
}

/// One stretch of a poly line, from one of its points to the next.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct PolySegment {
    /// Pixels on this stretch (unused while the line spreads its pixels evenly).
    #[cfg_attr(feature = "schema", schemars(description = "Pixels on this stretch."))]
    pub nodes: u32,
    /// The two control points of a curved stretch (a cubic Bézier from this point to the next,
    /// as xLights draws curves), in prop-local coordinates; `None` for a straight stretch.
    #[cfg_attr(
        feature = "schema",
        schemars(description = "Bézier control points of a curved stretch (prop-local).")
    )]
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

fn half_turn() -> f32 {
    180.0
}

fn one_count() -> u32 {
    1
}

/// xLights' default `Hollow` for layered arches.
fn arch_hollow() -> u32 {
    70
}

/// Where a star's pixels start: its top tip, the inner corner at its bottom (a star with an even
/// number of points turns so it has one), or the tip of its bottom left or right leg.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "schema",
    schemars(description = "Top tip, bottom inner corner, or a bottom leg's tip.")
)]
#[serde(rename_all = "camelCase")]
pub enum StarStart {
    /// The top tip.
    #[cfg_attr(feature = "schema", schemars(description = ""))]
    #[default]
    Top,
    /// The inner corner at the bottom (a star with an even number of points turns so it has one).
    #[cfg_attr(feature = "schema", schemars(description = ""))]
    Bottom,
    /// The bottom left tip.
    #[cfg_attr(feature = "schema", schemars(description = ""))]
    LeftLeg,
    /// The bottom right tip.
    #[cfg_attr(feature = "schema", schemars(description = ""))]
    RightLeg,
}

fn half_percent() -> u32 {
    50
}

/// The shape of a tree (xLights' Tree 360 / Flat / Ribbon).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum TreeStyle {
    #[default]
    Round,
    Flat,
    Ribbon,
}

fn south() -> f32 {
    -86.0
}

fn north() -> f32 {
    86.0
}

/// Parametric prop shapes. Positions are produced by `pf-geometry`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Generator {
    /// Straight run of evenly spaced pixels along X, centered on the origin.
    #[cfg_attr(feature = "schema", schemars(description = "Pixels along X, centered."))]
    Line { nodes: u32, length: f32 },
    /// Arches side by side (xLights' Arches), each `width` between its feet and `height` from
    /// its feet to its top: a part of an ellipse `arc` degrees round (180 is a half ellipse),
    /// `nodes` pixels each, spaced evenly by angle from the left foot to the right one, with
    /// `gap` between one arch's right foot and the next one's left foot. Origin at the middle of
    /// the row, level with the feet. With `layers`, one arch of nested layers instead.
    #[cfg_attr(
        feature = "schema",
        schemars(
            description = "`arches` arches in a row, each `width` across its feet and `height` tall, `nodes` pixels each; `layers` makes one arch of nested layers. Origin mid-row at the feet."
        )
    )]
    Arch {
        nodes: u32,
        width: f32,
        height: f32,
        #[serde(default = "one_count")]
        arches: u32,
        /// Degrees of the ellipse each arch goes round, 1–180.
        #[cfg_attr(feature = "schema", schemars(description = "Degrees round, 1–180."))]
        #[serde(default = "half_turn")]
        arc: f32,
        #[serde(default)]
        gap: f32,
        /// How far the arches lean, in degrees (positive leans left).
        #[cfg_attr(feature = "schema", schemars(description = "Lean in degrees, + is left."))]
        #[serde(default)]
        skew_deg: f32,
        /// The first pixel is the last arch's right foot (pixels run right to left).
        #[cfg_attr(feature = "schema", schemars(description = "Wired from the right."))]
        #[serde(default)]
        start_right: bool,
        /// Pixels on each layer, innermost first; empty for plain arches.
        #[cfg_attr(
            feature = "schema",
            schemars(description = "Pixels per layer, innermost first.")
        )]
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        layers: Vec<u32>,
        /// The innermost layer's size, in percent of the outermost.
        #[cfg_attr(
            feature = "schema",
            schemars(description = "Innermost layer's size, % of the outermost.")
        )]
        #[serde(default = "arch_hollow")]
        hollow: u32,
        /// Every other layer runs back the other way.
        #[cfg_attr(feature = "schema", schemars(description = "Every other layer runs back."))]
        #[serde(default)]
        zig_zag: bool,
        /// The pixels start on the innermost layer.
        #[cfg_attr(feature = "schema", schemars(description = "Start on the innermost layer."))]
        #[serde(default)]
        start_inside: bool,
    },
    /// Rings of pixels (xLights' Circle), one unless `layers` lists the pixels on each ring,
    /// innermost first; the rings are evenly spaced from `radius` in to `inner_percent` of it.
    /// Each ring starts at the top (or bottom) and runs clockwise (or counter-clockwise); the
    /// string goes round the outermost ring first, or the innermost with `start_inside`. Pixels
    /// beyond the rings sit in the middle. Centered.
    #[cfg_attr(
        feature = "schema",
        schemars(
            description = "Rings of pixels from the top, clockwise; `layers` lists each ring's pixels, innermost first. Centered."
        )
    )]
    Circle {
        nodes: u32,
        radius: f32,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        layers: Vec<u32>,
        /// The innermost ring's radius, in percent of the outermost.
        #[cfg_attr(
            feature = "schema",
            schemars(description = "Innermost ring's size, % of the outermost.")
        )]
        #[serde(default = "half_percent")]
        inner_percent: u32,
        #[serde(default)]
        start_inside: bool,
        #[serde(default)]
        start_at_bottom: bool,
        #[serde(default)]
        counter_clockwise: bool,
    },
    /// Grid of pixels wired according to `wiring`; centered.
    Matrix {
        columns: u32,
        rows: u32,
        width: f32,
        height: f32,
        #[serde(default)]
        wiring: MatrixWiring,
    },
    /// Strings running bottom to top; origin at the base center. A round tree is a cone: string
    /// `s` stands at `start_angle + s × degrees / strings` round from the front (or
    /// `degrees / (strings - 1)` apart when `degrees` is under 350, so a part tree reaches both
    /// edges, as xLights does). A flat tree fans its strings out in the front view, from
    /// `base_radius` either side at the bottom to `top_radius` at the top; a ribbon tree does the
    /// same with each string the same length, so the slanted ones end lower.
    ///
    /// Wiring follows xLights': the first string is on the `start` side (round trees go round
    /// the other way from the right) and runs from the bottom or the top; with `serpentine`,
    /// every other string runs back, starting afresh every `strands_per_string` strings when
    /// that's set (each physical string folded into that many); with `alternate_nodes`, each
    /// string's pixels go up every other spot and come back down the ones between. A round tree
    /// can spiral `spiral_rotations` times round from the base to the top, as xLights winds it.
    #[cfg_attr(
        feature = "schema",
        schemars(
            description = "Strings from base to top; origin at the base center. Round: a cone round `degrees` from `startAngle`; flat: fanned across the front; ribbon: flat, strings equal length."
        )
    )]
    Tree {
        strings: u32,
        nodes_per_string: u32,
        height: f32,
        base_radius: f32,
        top_radius: f32,
        #[serde(default)]
        serpentine: bool,
        #[serde(default)]
        style: TreeStyle,
        #[serde(default = "full_turn")]
        degrees: f32,
        #[serde(default)]
        start_angle: f32,
        #[serde(default)]
        start: Corner,
        /// Zig-zag starts afresh every this many strings; 0 never.
        #[serde(default)]
        strands_per_string: u32,
        #[cfg_attr(
            feature = "schema",
            schemars(description = "Pixels go up every other spot and back.")
        )]
        #[serde(default)]
        alternate_nodes: bool,
        /// Turns round a round tree from base to top.
        #[serde(default)]
        spiral_rotations: f32,
    },
    /// Star outlines with `points` tips (xLights' Star), pixels spaced evenly along each from the
    /// `start` corner, clockwise (or counter-clockwise). One outline unless `layers` lists the
    /// pixels on each, innermost first: the outlines are nested, the innermost `inner_percent`
    /// of the outermost's size, the string going round the outermost first (or the innermost,
    /// with `start_inside`). Pixels beyond the outlines sit in the middle. Centered.
    #[cfg_attr(
        feature = "schema",
        schemars(
            description = "Star outlines with `points` tips, pixels evenly along each from `start`, clockwise; `layers` lists each outline's pixels, innermost first. Centered."
        )
    )]
    Star {
        points: u32,
        nodes: u32,
        outer_radius: f32,
        inner_radius: f32,
        #[serde(default)]
        start: StarStart,
        #[serde(default)]
        counter_clockwise: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        layers: Vec<u32>,
        /// The innermost outline's size, in percent of the outermost.
        #[cfg_attr(
            feature = "schema",
            schemars(description = "Innermost layer's size, % of the outermost.")
        )]
        #[serde(default = "half_percent")]
        inner_percent: u32,
        #[serde(default)]
        start_inside: bool,
    },
    /// A line through any number of points, which can bend and curve (xLights' Poly Line).
    /// Pixels run from the first point to the last. Each stretch between two points has its own
    /// pixel count, spaced evenly with half a gap at each end (so the pixels stay evenly spaced
    /// across a corner), unless `spread_nodes` is set: then that many pixels are spread evenly
    /// along the whole line, the first on the first point, as xLights' "auto distribute" does.
    #[cfg_attr(
        feature = "schema",
        schemars(
            description = "A line through `vertices` (prop-local), pixels first to last. `segments`: one per stretch with its pixel count; `spreadNodes` spreads that many evenly instead."
        )
    )]
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
    #[cfg_attr(
        feature = "schema",
        schemars(
            description = "A row of canes `width` wide, pixels cane by cane from the left, up each stick then round its hook. Origin mid-row at the feet."
        )
    )]
    CandyCanes {
        canes: u32,
        nodes_per_cane: u32,
        /// Distance between the two ends the canes stand between.
        #[cfg_attr(feature = "schema", schemars(description = "Distance between the ends."))]
        width: f32,
        /// xLights' `Height`: scales the canes' height and the size of their hooks (1 is normal).
        #[cfg_attr(
            feature = "schema",
            schemars(description = "Scales canes and hooks; 1 is normal.")
        )]
        #[serde(default = "one")]
        height: f32,
        /// xLights' `CandyCaneHeight`: stretches the canes taller, hooks included (1 is normal).
        #[cfg_attr(
            feature = "schema",
            schemars(description = "Stretches the canes taller; 1 is normal.")
        )]
        #[serde(default = "one")]
        cane_height: f32,
        /// Hooks point left instead of right.
        #[serde(default)]
        reverse: bool,
        /// Straight sticks, no hooks.
        #[serde(default)]
        sticks: bool,
        /// Each cane's pixels go up every other spot and come back down the ones between.
        #[cfg_attr(
            feature = "schema",
            schemars(description = "Pixels go up every other spot and back.")
        )]
        #[serde(default)]
        alternate_nodes: bool,
        /// How far each cane leans from upright, in degrees (counter-clockwise).
        #[cfg_attr(feature = "schema", schemars(description = "Lean in degrees, + is left."))]
        #[serde(default)]
        skew_deg: f32,
        /// The first cane is the rightmost (xLights' `Dir="R"`); pixels still run up each stick
        /// then round its hook.
        #[cfg_attr(feature = "schema", schemars(description = "Wired from the right."))]
        #[serde(default)]
        start_right: bool,
    },
    /// Icicles hanging from a line between two ends (xLights' Icicles). Each string's pixels fill
    /// drops in turn, the drop sizes repeating `drops` from its start (a drop of 0 leaves a gap),
    /// each drop one column right of the last; the columns are spread evenly over `width`. The
    /// origin is midway along the line, and the drops hang below it.
    #[cfg_attr(
        feature = "schema",
        schemars(
            description = "Icicles below a line `width` wide; each string fills drops sized by `drops` (repeating), a column apart. Origin mid-line."
        )
    )]
    Icicles {
        strings: u32,
        lights_per_string: u32,
        /// Pixels in each drop, repeating ("3,4,5,4"). No drop with pixels reads as `[5]`.
        #[cfg_attr(
            feature = "schema",
            schemars(description = "Pixels per drop, repeating; 0 is a gap.")
        )]
        drops: Vec<u32>,
        /// Distance between the two ends.
        width: f32,
        /// How far below the line the longest drop's last pixel hangs (pixels in a drop are this
        /// over one less than the longest drop apart; for drops of one, this is the spacing).
        /// Negative makes the drops stand up. From xLights' `Height`, in layout units:
        /// `-Height * length / (columns - 1) * (longest drop - 1)`.
        #[cfg_attr(
            feature = "schema",
            schemars(description = "How far the longest drop hangs; negative stands up.")
        )]
        drop_height: f32,
        /// Each drop's pixels go down every other spot and come back up the ones between.
        #[cfg_attr(
            feature = "schema",
            schemars(description = "Pixels go down every other spot and back.")
        )]
        #[serde(default)]
        alternate_nodes: bool,
    },
    /// A window frame (xLights' Window Frame): one string that goes once round the frame from
    /// the `start` corner, `top`, `sides` (each) and `bottom` pixels along its edges, spaced as
    /// xLights spaces them. The edge the string starts along takes the corner pixels: starting up
    /// a side, the sides run from the bottom corner to the top one and the top and bottom pixels
    /// sit between them; starting along the top or bottom, those take the corners instead.
    /// `width` is between the two sides and `height` between the top and bottom; centered.
    #[cfg_attr(
        feature = "schema",
        schemars(
            description = "One string round a frame `width` by `height` from `start`, with `top`, `sides` (each) and `bottom` pixels. Centered."
        )
    )]
    WindowFrame {
        top: u32,
        sides: u32,
        bottom: u32,
        width: f32,
        height: f32,
        #[serde(default)]
        start: Corner,
        /// Runs counter-clockwise round the frame instead of clockwise.
        #[cfg_attr(feature = "schema", schemars(description = "Runs counter-clockwise."))]
        #[serde(default)]
        counter_clockwise: bool,
    },
    /// A wreath (xLights' Wreath): a ring like a circle, but each pixel rounded to the nearest
    /// point of a square grid `radius / (nodes / 2)` apart, as xLights places them. Starts at the
    /// top (or bottom) and runs clockwise (or counter-clockwise); centered.
    #[cfg_attr(
        feature = "schema",
        schemars(description = "A ring of pixels on a square grid, from the top. Centered.")
    )]
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
    #[cfg_attr(
        feature = "schema",
        schemars(
            description = "Arms out from a hollow middle (`hollow` %), the first pointing down turned by `startAngle`, spread over `arc`; `radius` to the outermost pixel. Centered."
        )
    )]
    Spinner {
        arms: u32,
        nodes_per_arm: u32,
        /// xLights' `Hollow`, in percent.
        hollow: u32,
        /// Degrees counter-clockwise from straight down to the first arm.
        #[cfg_attr(
            feature = "schema",
            schemars(description = "Degrees from down to the first arm.")
        )]
        #[serde(default)]
        start_angle: f32,
        /// Degrees the arms are spread over: 360 is all the way round (the last arm a step short
        /// of the first); less than that puts the last arm at the end of the arc.
        #[cfg_attr(feature = "schema", schemars(description = "Degrees the arms spread over."))]
        #[serde(default = "full_turn")]
        arc: f32,
        /// Every other arm runs the other way along itself.
        #[cfg_attr(feature = "schema", schemars(description = "Every other arm runs back."))]
        #[serde(default)]
        zig_zag: bool,
        /// Each arm's pixels go out every other spot and come back in on the ones between
        /// (starting in the middle, whichever end `from_center` picks).
        #[cfg_attr(
            feature = "schema",
            schemars(description = "Pixels go out every other spot and back.")
        )]
        #[serde(default)]
        alternate: bool,
        /// Each arm's pixels start in the middle instead of at its tip.
        #[cfg_attr(feature = "schema", schemars(description = "Pixels start in the middle."))]
        #[serde(default)]
        from_center: bool,
        /// The arms follow each other clockwise instead of counter-clockwise.
        #[cfg_attr(feature = "schema", schemars(description = "Arms follow clockwise."))]
        #[serde(default)]
        clockwise: bool,
        radius: f32,
    },
    /// A globe (xLights' Sphere): `columns` strands of `rows` pixels running from the south
    /// pole toward the north, between `start_latitude` and `end_latitude` (degrees), spread
    /// round `degrees` of the globe, `radius` from its middle. The first column is at the back,
    /// the next ones round the left side to the front (round the right side when `start` is on
    /// the right); a `start` at the top runs the first strand down from the north. Centered.
    #[cfg_attr(
        feature = "schema",
        schemars(
            description = "A globe of `columns` strands of `rows` pixels, south to north between the latitudes, round `degrees` of it. Centered."
        )
    )]
    Sphere {
        columns: u32,
        rows: u32,
        radius: f32,
        #[serde(default = "south")]
        start_latitude: f32,
        #[serde(default = "north")]
        end_latitude: f32,
        /// How far round the globe the columns go; anything less leaves a gap at the back.
        #[cfg_attr(feature = "schema", schemars(description = "Degrees round the globe."))]
        #[serde(default = "full_turn")]
        degrees: f32,
        #[serde(default)]
        start: Corner,
        #[serde(default)]
        strand_style: StrandStyle,
    },
    /// A cube of pixels (xLights' Cube): `width` across, `height` up and `depth` front to back,
    /// `spacing` apart in every direction, wired from the `start` corner in strands and layers
    /// as `style` says. Centered.
    #[cfg_attr(
        feature = "schema",
        schemars(
            description = "A cube of pixels `spacing` apart, wired from `start` as `style` says. Centered."
        )
    )]
    Cube {
        width: u32,
        height: u32,
        depth: u32,
        spacing: f32,
        #[serde(default)]
        start: CubeStart,
        #[serde(default)]
        style: CubeStyle,
        #[serde(default)]
        strand_style: StrandStyle,
        /// Each layer is wired the same way, instead of the next layer starting where the last
        /// one ended.
        #[cfg_attr(
            feature = "schema",
            schemars(description = "Each layer starts on the same side.")
        )]
        #[serde(default)]
        strand_per_layer: bool,
    },
    /// Free-form grid. `cells` is row-major starting at the top row;
    /// 0 is an empty cell and n places node n (1-based) in that cell.
    #[cfg_attr(
        feature = "schema",
        schemars(description = "`cells` row-major from the top row: 0 empty, n is pixel n (1-based).")
    )]
    CustomGrid {
        columns: u32,
        rows: u32,
        cells: Vec<u32>,
    },
}

impl Generator {
    /// A tree of `strings` strings of `nodes_per_string` pixels, each running up from the base,
    /// all the way round from the front, with xLights' wiring options off.
    pub fn tree(
        strings: u32,
        nodes_per_string: u32,
        height: f32,
        base_radius: f32,
        top_radius: f32,
        style: TreeStyle,
    ) -> Self {
        Generator::Tree {
            strings,
            nodes_per_string,
            height,
            base_radius,
            top_radius,
            serpentine: false,
            style,
            degrees: full_turn(),
            start_angle: 0.0,
            start: Corner::BottomLeft,
            strands_per_string: 0,
            alternate_nodes: false,
            spiral_rotations: 0.0,
        }
    }

    /// One plain half-ellipse arch of `nodes` pixels, `width` between its feet and `height` tall.
    pub fn arch(nodes: u32, width: f32, height: f32) -> Self {
        Generator::Arch {
            nodes,
            width,
            height,
            arches: 1,
            arc: half_turn(),
            gap: 0.0,
            skew_deg: 0.0,
            start_right: false,
            layers: Vec::new(),
            hollow: arch_hollow(),
            zig_zag: false,
            start_inside: false,
        }
    }

    /// One ring of `nodes` pixels from the top, clockwise.
    pub fn circle(nodes: u32, radius: f32) -> Self {
        Generator::Circle {
            nodes,
            radius,
            layers: Vec::new(),
            inner_percent: half_percent(),
            start_inside: false,
            start_at_bottom: false,
            counter_clockwise: false,
        }
    }

    /// One star outline of `nodes` pixels from the top tip, clockwise.
    pub fn star(points: u32, nodes: u32, outer_radius: f32, inner_radius: f32) -> Self {
        Generator::Star {
            points,
            nodes,
            outer_radius,
            inner_radius,
            start: StarStart::Top,
            counter_clockwise: false,
            layers: Vec::new(),
            inner_percent: half_percent(),
            start_inside: false,
        }
    }

    /// Number of pixels this generator produces.
    pub fn node_count(&self) -> u32 {
        match self {
            Generator::Arch {
                nodes,
                arches,
                layers,
                ..
            } => {
                if layers.is_empty() {
                    arches.saturating_mul(*nodes)
                } else {
                    *nodes
                }
            }
            Generator::Line { nodes, .. }
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
            Generator::Matrix { columns, rows, .. } | Generator::Sphere { columns, rows, .. } => {
                columns.saturating_mul(*rows)
            }
            Generator::Cube {
                width, height, depth, ..
            } => width.saturating_mul(*height).saturating_mul(*depth),
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum Provenance {
    CameraMap,
    Import,
    Manual,
}

/// The source of a prop's pixel positions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
                    style: TreeStyle::Round,
                    degrees: 360.0,
                    start_angle: 0.0,
                    start: Corner::BottomLeft,
                    strands_per_string: 0,
                    alternate_nodes: false,
                    spiral_rotations: 0.0,
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
            start_right: false,
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
                start_right: false,
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
    fn spheres_and_cubes_count_and_round_trip_their_settings() {
        let sphere = Generator::Sphere {
            columns: 16,
            rows: 25,
            radius: 1.2,
            start_latitude: -80.0,
            end_latitude: 70.0,
            degrees: 270.0,
            start: Corner::TopRight,
            strand_style: StrandStyle::AlternatePixel,
        };
        assert_eq!(sphere.node_count(), 400);
        let json = serde_json::to_value(ShapeSource::Generator(sphere.clone())).unwrap();
        assert_eq!(json["type"], "sphere");
        assert_eq!(json["startLatitude"], -80.0);
        assert_eq!(json["strandStyle"], "alternatePixel");
        assert_eq!(
            serde_json::from_value::<ShapeSource>(json).unwrap(),
            ShapeSource::Generator(sphere)
        );
        // Left out: xLights' -86° to 86°, all the way round, from the bottom left, zig-zag.
        let plain: Generator =
            serde_json::from_str(r#"{ "type": "sphere", "columns": 4, "rows": 5, "radius": 1 }"#).unwrap();
        assert!(matches!(
            plain,
            Generator::Sphere {
                start_latitude: -86.0,
                end_latitude: 86.0,
                degrees: 360.0,
                start: Corner::BottomLeft,
                strand_style: StrandStyle::ZigZag,
                ..
            }
        ));

        let cube = Generator::Cube {
            width: 5,
            height: 4,
            depth: 3,
            spacing: 0.1,
            start: CubeStart::BackTopRight,
            style: CubeStyle::StackedLeftRight,
            strand_style: StrandStyle::NoZigZag,
            strand_per_layer: true,
        };
        assert_eq!(cube.node_count(), 60);
        let json = serde_json::to_value(&cube).unwrap();
        assert_eq!(json["type"], "cube");
        assert_eq!(json["start"], "backTopRight");
        assert_eq!(json["style"], "stackedLeftRight");
        assert_eq!(json["strandStyle"], "noZigZag");
        assert_eq!(json["strandPerLayer"], true);
        assert_eq!(serde_json::from_value::<Generator>(json).unwrap(), cube);
        let plain: Generator = serde_json::from_str(
            r#"{ "type": "cube", "width": 2, "height": 2, "depth": 2, "spacing": 0.5 }"#,
        )
        .unwrap();
        assert!(matches!(
            plain,
            Generator::Cube {
                start: CubeStart::FrontBottomLeft,
                style: CubeStyle::VerticalFrontBack,
                strand_style: StrandStyle::ZigZag,
                strand_per_layer: false,
                ..
            }
        ));
        let huge = Generator::Cube {
            width: u32::MAX,
            height: 2,
            depth: 2,
            spacing: 1.0,
            start: CubeStart::FrontBottomLeft,
            style: CubeStyle::VerticalFrontBack,
            strand_style: StrandStyle::ZigZag,
            strand_per_layer: false,
        };
        assert_eq!(huge.node_count(), u32::MAX);
    }

    #[test]
    fn arches_count_each_arch_or_their_layers_and_round_trip_their_settings() {
        let row = Generator::Arch {
            nodes: 25,
            width: 2.0,
            height: 1.0,
            arches: 4,
            arc: 120.0,
            gap: 0.3,
            skew_deg: -10.0,
            start_right: true,
            layers: vec![],
            hollow: 70,
            zig_zag: false,
            start_inside: false,
        };
        assert_eq!(row.node_count(), 100);
        let json = serde_json::to_value(&row).unwrap();
        assert_eq!(json["type"], "arch");
        assert_eq!(json["skewDeg"], -10.0);
        assert_eq!(json["startRight"], true);
        assert!(json.get("layers").is_none(), "no layers, none written");
        assert_eq!(serde_json::from_value::<Generator>(json).unwrap(), row);

        let layered = Generator::Arch {
            nodes: 60,
            width: 2.0,
            height: 1.0,
            arches: 1,
            arc: 180.0,
            gap: 0.0,
            skew_deg: 0.0,
            start_right: false,
            layers: vec![10, 20, 30],
            hollow: 40,
            zig_zag: true,
            start_inside: true,
        };
        assert_eq!(layered.node_count(), 60, "one arch of layers");
        let json = serde_json::to_value(&layered).unwrap();
        assert_eq!(json["layers"], serde_json::json!([10, 20, 30]));
        assert_eq!(json["startInside"], true);
        assert_eq!(serde_json::from_value::<Generator>(json).unwrap(), layered);

        // Arches saved before these settings are one half-ellipse arch, as they always were.
        let old: Generator =
            serde_json::from_str(r#"{ "type": "arch", "nodes": 50, "width": 4, "height": 2 }"#).unwrap();
        assert_eq!(old, Generator::arch(50, 4.0, 2.0));
        assert!(matches!(
            old,
            Generator::Arch { arches: 1, arc: 180.0, gap: 0.0, skew_deg: 0.0, start_right: false, hollow: 70, zig_zag: false, start_inside: false, ref layers, .. } if layers.is_empty()
        ));
        let mut huge = Generator::arch(2, 1.0, 1.0);
        if let Generator::Arch { arches, .. } = &mut huge {
            *arches = u32::MAX;
        }
        assert_eq!(huge.node_count(), u32::MAX);
    }

    #[test]
    fn circles_and_stars_round_trip_their_layers_and_starts() {
        let circle = Generator::Circle {
            nodes: 60,
            radius: 1.0,
            layers: vec![10, 20, 30],
            inner_percent: 30,
            start_inside: true,
            start_at_bottom: true,
            counter_clockwise: true,
        };
        assert_eq!(circle.node_count(), 60);
        let json = serde_json::to_value(&circle).unwrap();
        assert_eq!(json["innerPercent"], 30);
        assert_eq!(json["startAtBottom"], true);
        assert_eq!(serde_json::from_value::<Generator>(json).unwrap(), circle);
        let star = Generator::Star {
            points: 6,
            nodes: 80,
            outer_radius: 1.0,
            inner_radius: 0.4,
            start: StarStart::LeftLeg,
            counter_clockwise: true,
            layers: vec![30, 50],
            inner_percent: 40,
            start_inside: true,
        };
        let json = serde_json::to_value(&star).unwrap();
        assert_eq!(json["start"], "leftLeg");
        assert_eq!(json["layers"], serde_json::json!([30, 50]));
        assert_eq!(serde_json::from_value::<Generator>(json).unwrap(), star);
        // Saved before these settings: one ring or outline from the top, clockwise.
        let old: Generator =
            serde_json::from_str(r#"{ "type": "circle", "nodes": 50, "radius": 1 }"#).unwrap();
        assert_eq!(old, Generator::circle(50, 1.0));
        assert!(serde_json::to_value(&old).unwrap().get("layers").is_none());
        let old: Generator = serde_json::from_str(
            r#"{ "type": "star", "points": 5, "nodes": 50, "outerRadius": 1, "innerRadius": 0.4 }"#,
        )
        .unwrap();
        assert_eq!(old, Generator::star(5, 50, 1.0, 0.4));
    }

    #[test]
    fn generator_shape_json_is_flat_and_camel_case() {
        let shape = ShapeSource::Generator(Generator::arch(50, 4.0, 2.0));
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
            style: TreeStyle::Flat,
            degrees: 360.0,
            start_angle: 0.0,
            start: Corner::BottomLeft,
            strands_per_string: 0,
            alternate_nodes: false,
            spiral_rotations: 0.0,
        })
        .unwrap();
        assert_eq!(json["nodesPerString"], 3);
        assert_eq!(json["baseRadius"], 1.0);
        assert_eq!(json["style"], "flat");
        assert_eq!(json["startAngle"], 0.0);
        // Trees saved before styles were added are round, all the way round.
        let old: Generator = serde_json::from_str(
            r#"{ "type": "tree", "strings": 2, "nodesPerString": 3, "height": 1, "baseRadius": 1, "topRadius": 0 }"#,
        )
        .unwrap();
        assert!(matches!(
            old,
            Generator::Tree {
                style: TreeStyle::Round,
                degrees: 360.0,
                start_angle: 0.0,
                start: Corner::BottomLeft,
                strands_per_string: 0,
                alternate_nodes: false,
                spiral_rotations: 0.0,
                ..
            }
        ));
        // xLights' wiring options round-trip.
        let mut wired = Generator::tree(14, 50, 6.0, 2.0, 0.3, TreeStyle::Round);
        if let Generator::Tree {
            serpentine,
            start,
            strands_per_string,
            spiral_rotations,
            ..
        } = &mut wired
        {
            (*serpentine, *start, *strands_per_string, *spiral_rotations) = (true, Corner::TopRight, 7, -2.5);
        }
        let json = serde_json::to_value(&wired).unwrap();
        assert_eq!(json["start"], "topRight");
        assert_eq!(json["strandsPerString"], 7);
        assert_eq!(json["spiralRotations"], -2.5);
        assert_eq!(serde_json::from_value::<Generator>(json).unwrap(), wired);
    }
}
