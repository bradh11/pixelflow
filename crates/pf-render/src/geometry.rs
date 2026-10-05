//! Where every pixel sits, normalized per target, so effects can draw on any shape.
//!
//! Each target (a prop, a group of props, or a submodel) becomes a **pixel buffer**: its pixels
//! with (u, v) positions scaled to the target's bounding box (0–1, left to right and bottom to
//! top, front view), plus their order along the target. A group uses the combined bounding box
//! of its members, so a wave sweeps across the whole group.
//!
//! A submodel lays its pixels out the way xLights does (`SubModel.cpp`): with the default buffer
//! style each line is a row (or a column, for a vertical submodel), gaps included; "stacked
//! strands" puts every line on the same row; "keep XY" keeps the pixels where they are on the
//! prop; and a sub-buffer is the prop's own buffer cropped to its rectangle and stretched to
//! fill 0–1 again.

use pf_mapping::ChannelMap;
use pf_model::{BufferStyle, GroupId, LineLayout, PropId, Region, RegionId, RegionKind, RegionRef, Show};
use pf_sequence::Target;
use std::collections::{HashMap, HashSet};

/// One pixel as an effect sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pixel {
    /// Left (0) to right (1) across the target.
    pub u: f32,
    /// Bottom (0) to top (1).
    pub v: f32,
    /// Position along the target in wiring order (across group members in member order).
    pub index: u32,
    /// Pixels in the target.
    pub count: u32,
}

/// A target's pixels, ready for effects.
#[derive(Debug, Clone, PartialEq)]
pub struct PixelBuffer {
    pub pixels: Vec<Pixel>,
    /// Each pixel's index in the show-wide pixel list (see [`SceneGeometry`]).
    pub(crate) global: Vec<u32>,
    /// Rough columns and rows of pixels (a 50×20 matrix gives about 50 and 20); effects use them
    /// to size things like meteor lanes to the pixels.
    pub columns: u32,
    pub rows: u32,
}

impl PixelBuffer {
    pub fn len(&self) -> usize {
        self.pixels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pixels.is_empty()
    }

    /// Each pixel's index in the show-wide pixel list, in buffer order.
    pub fn show_pixels(&self) -> &[u32] {
        &self.global
    }

    fn empty() -> Self {
        Self {
            pixels: Vec::new(),
            global: Vec::new(),
            columns: 1,
            rows: 1,
        }
    }
}

/// One prop's pixels in the show frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PropGeometry {
    pub frame_offset: usize,
    pub channels_per_pixel: u8,
    /// Index of the prop's first pixel in the show-wide pixel list.
    pub first_pixel: usize,
    /// Front-view positions (x right, y up), in wiring order.
    pub points: Vec<[f32; 2]>,
    /// How many of `points` came from the shape; the rest are padding (see [`SceneGeometry::new`]).
    pub real: usize,
    /// The prop's submodels and faces.
    pub regions: Vec<Region>,
}

impl PropGeometry {
    fn node_count(&self) -> u32 {
        u32::try_from(self.points.len()).unwrap_or(u32::MAX)
    }

    /// Node `n` as a point for [`build_buffer`].
    fn point(&self, n: u32) -> Point {
        let n = n as usize;
        Point {
            global: u32::try_from(self.first_pixel + n).unwrap_or(u32::MAX),
            xy: (n < self.real).then(|| self.points[n]),
        }
    }

    pub(crate) fn region(&self, id: RegionId) -> Option<&Region> {
        self.regions.iter().find(|r| r.id == id)
    }
}

/// A pixel for [`build_buffer`]: its show-wide index and position (`None`: padding, drawn in the
/// middle of the box).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    global: u32,
    xy: Option<[f32; 2]>,
}

/// Every prop's pixel positions and frame location, computed once per show and channel map.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneGeometry {
    pub(crate) props: Vec<PropGeometry>,
    index: HashMap<PropId, usize>,
    groups: HashMap<GroupId, (Vec<PropId>, Vec<RegionRef>)>,
    pub(crate) pixel_count: usize,
    pub(crate) frame_len: usize,
}

impl SceneGeometry {
    pub fn new(show: &Show, map: &ChannelMap) -> Self {
        let mut props = Vec::with_capacity(map.props.len());
        let mut index = HashMap::with_capacity(map.props.len());
        let mut first_pixel = 0;
        for layout in &map.props {
            let Some(prop) = show.prop(layout.prop) else {
                continue;
            };
            let nodes = layout.nodes as usize;
            let mut points: Vec<[f32; 2]> = pf_geometry::world_positions(prop)
                .into_iter()
                .take(nodes)
                .map(|p| [finite(p.x), finite(p.y)])
                .collect();
            // A shape with fewer points than nodes still gets every pixel. The padding doesn't
            // count toward the bounding box (it would stretch it to the origin); those pixels
            // draw at the middle of the box.
            let real = points.len();
            points.resize(nodes, [0.0, 0.0]);
            index.entry(layout.prop).or_insert(props.len());
            props.push(PropGeometry {
                frame_offset: layout.frame_offset,
                channels_per_pixel: layout.channels_per_pixel,
                first_pixel,
                points,
                real,
                regions: prop.regions.clone(),
            });
            first_pixel += nodes;
        }
        let groups = show
            .groups
            .iter()
            .map(|g| (g.id, (g.members.clone(), g.submodels.clone())))
            .collect();
        Self {
            props,
            index,
            groups,
            pixel_count: first_pixel,
            frame_len: map.frame_len,
        }
    }

    /// Show-frame bytes (prop order, RGB/RGBW per pixel).
    pub fn frame_len(&self) -> usize {
        self.frame_len
    }

    /// Pixels in the whole show.
    pub fn pixel_count(&self) -> usize {
        self.pixel_count
    }

    pub(crate) fn prop(&self, id: PropId) -> Option<&PropGeometry> {
        self.index.get(&id).map(|&i| &self.props[i])
    }

    /// The pixel buffer for a target; empty when the target is unknown or has no pixels.
    pub fn buffer(&self, target: Target) -> PixelBuffer {
        match target {
            Target::Prop(id) => match self.prop(id) {
                Some(prop) => {
                    build_buffer(&(0..prop.node_count()).map(|n| prop.point(n)).collect::<Vec<_>>())
                }
                None => PixelBuffer::empty(),
            },
            Target::Group(id) => {
                let Some((members, submodels)) = self.groups.get(&id) else {
                    return PixelBuffer::empty();
                };
                let mut seen = HashSet::new();
                let mut points = Vec::new();
                let mut add = |p: Point| {
                    if seen.insert(p.global) {
                        points.push(p);
                    }
                };
                for prop in members.iter().filter_map(|m| self.prop(*m)) {
                    (0..prop.node_count()).for_each(|n| add(prop.point(n)));
                }
                for member in submodels {
                    let Some(prop) = self.prop(member.prop) else {
                        continue;
                    };
                    let Some(region) = prop.region(member.region) else {
                        continue;
                    };
                    region_nodes(prop, region)
                        .into_iter()
                        .for_each(|n| add(prop.point(n)));
                }
                build_buffer(&points)
            }
            Target::Region { prop, region } => {
                match self.prop(prop).and_then(|p| Some((p, p.region(region)?))) {
                    Some((prop, region)) => region_buffer(prop, region),
                    None => PixelBuffer::empty(),
                }
            }
        }
    }
}

fn finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

/// The prop's nodes a region lights, in the region's order. A sub-buffer takes the prop's
/// pixels inside its rectangle, in wiring order.
fn region_nodes(prop: &PropGeometry, region: &Region) -> Vec<u32> {
    match region.kind {
        RegionKind::SubBuffer { x1, y1, x2, y2 } => {
            let whole = build_buffer(&(0..prop.node_count()).map(|n| prop.point(n)).collect::<Vec<_>>());
            whole
                .pixels
                .iter()
                .enumerate()
                .filter(|(_, px)| in_rect(px, x1, y1, x2, y2))
                .map(|(n, _)| n as u32)
                .collect()
        }
        _ => region.node_list(prop.node_count()),
    }
}

/// True when a pixel of the prop's buffer lies in a sub-buffer rectangle (percentages).
fn in_rect(px: &Pixel, x1: f32, y1: f32, x2: f32, y2: f32) -> bool {
    const EPS: f32 = 1e-3;
    let (u, v) = (px.u * 100.0, px.v * 100.0);
    u >= x1.min(x2) - EPS && u <= x1.max(x2) + EPS && v >= y1.min(y2) - EPS && v <= y1.max(y2) + EPS
}

/// A submodel's (or face's) pixel buffer.
fn region_buffer(prop: &PropGeometry, region: &Region) -> PixelBuffer {
    match &region.kind {
        RegionKind::Nodes {
            lines,
            layout,
            buffer,
        } if *buffer != BufferStyle::KeepXy => {
            grid_buffer(prop, lines, *layout, *buffer == BufferStyle::StackedStrands)
        }
        RegionKind::SubBuffer { x1, y1, x2, y2 } => {
            let whole = build_buffer(&(0..prop.node_count()).map(|n| prop.point(n)).collect::<Vec<_>>());
            let (lo_x, hi_x) = (x1.min(*x2), x1.max(*x2));
            let (lo_y, hi_y) = (y1.min(*y2), y1.max(*y2));
            let stretch = |value: f32, lo: f32, hi: f32| {
                if hi - lo <= f32::EPSILON {
                    0.5
                } else {
                    ((value * 100.0 - lo) / (hi - lo)).clamp(0.0, 1.0)
                }
            };
            let mut pixels = Vec::new();
            let mut global = Vec::new();
            for (px, &g) in whole.pixels.iter().zip(&whole.global) {
                if in_rect(px, *x1, *y1, *x2, *y2) {
                    pixels.push(Pixel {
                        u: stretch(px.u, lo_x, hi_x),
                        v: stretch(px.v, lo_y, hi_y),
                        index: pixels.len() as u32,
                        count: 0,
                    });
                    global.push(g);
                }
            }
            let count = pixels.len() as u32;
            pixels.iter_mut().for_each(|p| p.count = count);
            let share = |n: u32, part: f32| ((n as f32 * part / 100.0).round() as u32).clamp(1, count.max(1));
            PixelBuffer {
                columns: share(whole.columns, hi_x - lo_x),
                rows: share(whole.rows, hi_y - lo_y),
                pixels,
                global,
            }
        }
        // Keep XY and faces: the pixels where they are on the prop.
        _ => build_buffer(
            &region
                .node_list(prop.node_count())
                .into_iter()
                .map(|n| prop.point(n))
                .collect::<Vec<_>>(),
        ),
    }
}

/// Lines laid out as rows (or columns), as xLights' `SubModel::initDefaultBuffer` does: each
/// pixel takes the next spot along its line, a gap skips a spot, and each line starts a new row
/// (column), or the same one when the lines are stacked. A pixel listed twice draws where it
/// was listed last.
fn grid_buffer(
    prop: &PropGeometry,
    lines: &[pf_model::SubmodelLine],
    layout: LineLayout,
    stacked: bool,
) -> PixelBuffer {
    let vertical = layout == LineLayout::Vertical;
    let nodes = prop.node_count();
    let mut spot: HashMap<u32, (i64, i64)> = HashMap::new();
    let mut order = Vec::new();
    let (mut row, mut col, mut max_row, mut max_col) = (0i64, 0i64, 0i64, 0i64);
    for line in lines {
        for item in line {
            match item {
                None => {
                    if vertical {
                        row += 1;
                    } else {
                        col += 1;
                    }
                }
                Some(run) => {
                    for n in run.nodes(nodes) {
                        if spot.insert(n, (col, row)).is_none() {
                            order.push(n);
                        }
                        if vertical {
                            row += 1;
                        } else {
                            col += 1;
                        }
                    }
                }
            }
        }
        if vertical {
            row -= 1;
        } else {
            col -= 1;
        }
        max_row = max_row.max(row);
        max_col = max_col.max(col);
        if stacked {
            (row, col) = (0, 0);
        } else if vertical {
            (row, col) = (0, col + 1);
        } else {
            (row, col) = (row + 1, 0);
        }
    }
    let (width, height) = (max_col + 1, max_row + 1);
    let scale = |at: i64, size: i64| {
        if size <= 1 {
            0.5
        } else {
            at as f32 / (size - 1) as f32
        }
    };
    let count = u32::try_from(order.len()).unwrap_or(u32::MAX);
    let mut pixels = Vec::with_capacity(order.len());
    let mut global = Vec::with_capacity(order.len());
    for (i, n) in order.iter().enumerate() {
        let (c, r) = spot[n];
        pixels.push(Pixel {
            u: scale(c, width),
            v: scale(r, height),
            index: i as u32,
            count,
        });
        global.push(prop.point(*n).global);
    }
    PixelBuffer {
        pixels,
        global,
        columns: u32::try_from(width).unwrap_or(1).max(1),
        rows: u32::try_from(height).unwrap_or(1).max(1),
    }
}

fn build_buffer(points: &[Point]) -> PixelBuffer {
    let count = points.len();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in points.iter().filter_map(|p| p.xy) {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    }
    if min_x > max_x {
        // Nothing but padding: every pixel at the middle.
        (min_x, min_y, max_x, max_y) = (0.0, 0.0, 0.0, 0.0);
    }
    let (width, height) = ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0));
    // A flat side (a straight horizontal line has no height) puts every pixel in the middle.
    let extent = width.max(height);
    let flat = |size: f32| size <= extent * 1e-6 || size <= f32::MIN_POSITIVE;
    let (flat_x, flat_y) = (flat(width), flat(height));
    let count_u32 = u32::try_from(count).unwrap_or(u32::MAX);
    let mut pixels = Vec::with_capacity(count);
    let mut global = Vec::with_capacity(count);
    for point in points {
        let u = match point.xy {
            Some(p) if !flat_x => (p[0] - min_x) / width,
            _ => 0.5,
        };
        let v = match point.xy {
            Some(p) if !flat_y => (p[1] - min_y) / height,
            _ => 0.5,
        };
        pixels.push(Pixel {
            u,
            v,
            index: u32::try_from(pixels.len()).unwrap_or(u32::MAX),
            count: count_u32,
        });
        global.push(point.global);
    }
    let (columns, rows) = resolution(count, width, height, flat_x, flat_y);
    PixelBuffer {
        pixels,
        global,
        columns,
        rows,
    }
}

/// Rough columns × rows for `count` pixels spread over `width` × `height`.
fn resolution(count: usize, width: f32, height: f32, flat_x: bool, flat_y: bool) -> (u32, u32) {
    let n = count.max(1) as f64;
    let (columns, rows) = match (flat_x, flat_y) {
        (true, true) => (1.0, 1.0),
        (true, false) => (1.0, n),
        (false, true) => (n, 1.0),
        (false, false) => {
            let columns = (n * f64::from(width) / f64::from(height))
                .sqrt()
                .round()
                .clamp(1.0, n);
            (columns, (n / columns).round().max(1.0))
        }
    };
    (columns as u32, rows as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::{Generator, Group, NodeRun, Prop, ShapeSource, Transform, Vec3};

    fn line(name: &str, nodes: u32, x: f32) -> Prop {
        let mut prop = Prop::new(
            name,
            ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
        );
        prop.transform = Transform {
            position: Vec3::new(x, 0.0, 0.0),
            ..Transform::default()
        };
        prop
    }

    fn matrix(columns: u32, rows: u32) -> Prop {
        Prop::new(
            "Matrix",
            ShapeSource::Generator(Generator::Matrix {
                columns,
                rows,
                width: 2.0,
                height: 1.0,
                wiring: Default::default(),
            }),
        )
    }

    fn geometry(show: &Show) -> SceneGeometry {
        SceneGeometry::new(show, &pf_mapping::map_show(show).0)
    }

    #[test]
    fn a_prop_fills_its_own_box() {
        let mut show = Show::new("t");
        show.props.push(line("A", 5, 3.0));
        let id = show.props[0].id;
        let buffer = geometry(&show).buffer(Target::Prop(id));
        let us: Vec<f32> = buffer.pixels.iter().map(|p| p.u).collect();
        assert_eq!(us, vec![0.0, 0.25, 0.5, 0.75, 1.0]);
        assert!(
            buffer.pixels.iter().all(|p| p.v == 0.5),
            "a flat line sits mid-height"
        );
        assert_eq!(buffer.pixels[3].index, 3);
        assert_eq!(buffer.pixels[3].count, 5);
        assert_eq!((buffer.columns, buffer.rows), (5, 1));
    }

    #[test]
    fn a_group_shares_one_box_and_counts_across_members() {
        let mut show = Show::new("t");
        show.props.push(line("A", 3, 0.0));
        show.props.push(line("B", 3, 3.0));
        let mut group = Group::new("G");
        group.members = vec![show.props[1].id, show.props[0].id, show.props[1].id];
        let gid = group.id;
        show.groups.push(group);
        let geo = geometry(&show);
        let buffer = geo.buffer(Target::Group(gid));
        assert_eq!(buffer.len(), 6, "a repeated member is drawn once");
        // B (x from 2.5 to 3.5) comes first; the box spans x from -0.5 to 3.5.
        let us: Vec<f32> = buffer.pixels.iter().map(|p| p.u).collect();
        assert_eq!(us, vec![0.75, 0.875, 1.0, 0.0, 0.125, 0.25]);
        assert_eq!(buffer.global, vec![3, 4, 5, 0, 1, 2]);
        assert_eq!(buffer.pixels[5].index, 5);
        assert!(buffer.pixels.iter().all(|p| p.count == 6));
    }

    #[test]
    fn matrices_report_their_columns_and_rows() {
        let mut show = Show::new("t");
        show.props.push(matrix(20, 10));
        let buffer = geometry(&show).buffer(Target::Prop(show.props[0].id));
        assert_eq!((buffer.columns, buffer.rows), (20, 10));
        let (u, v) = buffer
            .pixels
            .iter()
            .fold((0.0f32, 0.0f32), |(u, v), p| (u.max(p.u), v.max(p.v)));
        assert_eq!((u, v), (1.0, 1.0));
    }

    #[test]
    fn unknown_targets_and_single_pixels_are_safe() {
        let mut show = Show::new("t");
        show.props.push(line("Dot", 1, 0.0));
        let geo = geometry(&show);
        assert!(geo.buffer(Target::Prop(PropId::new())).is_empty());
        assert!(geo.buffer(Target::Group(GroupId::new())).is_empty());
        let dot = geo.buffer(Target::Prop(show.props[0].id));
        assert_eq!((dot.pixels[0].u, dot.pixels[0].v), (0.5, 0.5));
        assert_eq!((dot.columns, dot.rows), (1, 1));
        assert_eq!(geo.pixel_count(), 1);
        assert_eq!(geo.frame_len(), 3);
    }

    fn prop_geometry(points: Vec<[f32; 2]>, real: usize) -> PropGeometry {
        PropGeometry {
            frame_offset: 0,
            channels_per_pixel: 3,
            first_pixel: 0,
            points,
            real,
            regions: Vec::new(),
        }
    }

    fn all_points(prop: &PropGeometry) -> Vec<Point> {
        (0..prop.node_count()).map(|n| prop.point(n)).collect()
    }

    #[test]
    fn padded_points_dont_stretch_the_box() {
        // Two real points far from the origin, plus two padding pixels.
        let prop = prop_geometry(vec![[10.0, 5.0], [12.0, 7.0], [0.0, 0.0], [0.0, 0.0]], 2);
        let buffer = build_buffer(&all_points(&prop));
        let uv: Vec<(f32, f32)> = buffer.pixels.iter().map(|p| (p.u, p.v)).collect();
        assert_eq!(uv, vec![(0.0, 0.0), (1.0, 1.0), (0.5, 0.5), (0.5, 0.5)]);
        let nothing = prop_geometry(vec![[0.0, 0.0]; 3], 0);
        assert!(
            build_buffer(&all_points(&nothing))
                .pixels
                .iter()
                .all(|p| (p.u, p.v) == (0.5, 0.5))
        );
    }

    /// A 10-pixel line along x from 0 to 9, with `region` on it.
    fn line_with(region: Region) -> (SceneGeometry, Target) {
        let mut show = Show::new("t");
        let mut prop = Prop::new(
            "Line",
            ShapeSource::Generator(Generator::Line {
                nodes: 10,
                length: 9.0,
            }),
        );
        let target = Target::Region {
            prop: prop.id,
            region: region.id,
        };
        prop.regions.push(region);
        show.props.push(prop);
        (geometry(&show), target)
    }

    fn submodel(lines: Vec<Vec<Option<NodeRun>>>, layout: LineLayout, buffer: BufferStyle) -> Region {
        Region {
            kind: RegionKind::Nodes {
                lines,
                layout,
                buffer,
            },
            ..Region::nodes("Sub", vec![])
        }
    }

    fn uvs(buffer: &PixelBuffer) -> Vec<(u32, f32, f32)> {
        buffer
            .pixels
            .iter()
            .zip(&buffer.global)
            .map(|(p, g)| (*g, p.u, p.v))
            .collect()
    }

    fn run(a: u32, b: u32) -> Option<NodeRun> {
        Some(NodeRun::new(a, b))
    }

    #[test]
    fn default_submodels_put_each_line_on_its_own_row_with_gaps() {
        // Line 1: pixels 1-3; line 2: pixel 6, a gap, then pixels 5-4 backwards.
        let region = submodel(
            vec![vec![run(0, 2)], vec![run(5, 5), None, run(4, 3)]],
            LineLayout::Horizontal,
            BufferStyle::Default,
        );
        let (geo, target) = line_with(region);
        let buffer = geo.buffer(target);
        assert_eq!((buffer.columns, buffer.rows), (4, 2));
        let third = 1.0 / 3.0;
        assert_eq!(
            uvs(&buffer),
            vec![
                (0, 0.0, 0.0),
                (1, third, 0.0),
                (2, 2.0 * third, 0.0),
                (5, 0.0, 1.0),
                (4, 2.0 * third, 1.0),
                (3, 1.0, 1.0),
            ]
        );
        assert!(
            buffer
                .pixels
                .iter()
                .enumerate()
                .all(|(i, p)| p.index == i as u32 && p.count == 6)
        );
    }

    #[test]
    fn vertical_submodels_make_columns_and_stacked_strands_share_one() {
        let lines = vec![vec![run(0, 1)], vec![run(2, 4)]];
        let (geo, target) = line_with(submodel(
            lines.clone(),
            LineLayout::Vertical,
            BufferStyle::Default,
        ));
        let buffer = geo.buffer(target);
        assert_eq!((buffer.columns, buffer.rows), (2, 3));
        assert_eq!(
            uvs(&buffer),
            vec![
                (0, 0.0, 0.0),
                (1, 0.0, 0.5),
                (2, 1.0, 0.0),
                (3, 1.0, 0.5),
                (4, 1.0, 1.0)
            ]
        );

        let (geo, target) = line_with(submodel(
            lines,
            LineLayout::Horizontal,
            BufferStyle::StackedStrands,
        ));
        let buffer = geo.buffer(target);
        assert_eq!((buffer.columns, buffer.rows), (3, 1));
        assert_eq!(
            uvs(&buffer),
            vec![
                (0, 0.0, 0.5),
                (1, 0.5, 0.5),
                (2, 0.0, 0.5),
                (3, 0.5, 0.5),
                (4, 1.0, 0.5)
            ]
        );
    }

    #[test]
    fn keep_xy_submodels_and_faces_keep_real_positions() {
        let (geo, target) = line_with(submodel(
            vec![vec![run(2, 2), run(8, 8)], vec![run(5, 5)]],
            LineLayout::Horizontal,
            BufferStyle::KeepXy,
        ));
        let buffer = geo.buffer(target);
        assert_eq!(uvs(&buffer), vec![(2, 0.0, 0.5), (8, 1.0, 0.5), (5, 0.5, 0.5)]);

        let face = pf_model::FaceDefinition {
            outline: vec![pf_model::NodeRange::new(4, 7)],
            ..Default::default()
        };
        let (geo, target) = line_with(Region::face("Face", face));
        assert_eq!(geo.buffer(target).global, vec![4, 5, 6]);
    }

    #[test]
    fn sub_buffers_crop_the_prop_and_stretch_back_to_fill() {
        let mut show = Show::new("t");
        let mut grid = matrix(5, 5);
        let window = Region {
            kind: RegionKind::SubBuffer {
                x1: 50.0,
                y1: 0.0,
                x2: 100.0,
                y2: 50.0,
            },
            ..Region::nodes("Window", vec![])
        };
        let target = Target::Region {
            prop: grid.id,
            region: window.id,
        };
        grid.regions.push(window);
        show.props.push(grid);
        let buffer = geometry(&show).buffer(target);
        // The lower-right 3×3 of a 5×5 grid (the middle row and column are on the edges).
        assert_eq!(buffer.len(), 9);
        // Half of the prop's rough 7 × 4 (a 5 × 5 grid twice as wide as it is tall).
        assert_eq!((buffer.columns, buffer.rows), (4, 2));
        let mut us: Vec<f32> = buffer.pixels.iter().map(|p| p.u).collect();
        us.sort_by(f32::total_cmp);
        us.dedup();
        assert_eq!(us, vec![0.0, 0.5, 1.0]);
        assert!(buffer.pixels.iter().all(|p| (0.0..=1.0).contains(&p.v)));
    }

    #[test]
    fn groups_draw_their_submodels_once_after_whole_props() {
        let mut show = Show::new("t");
        let mut a = line("A", 4, 0.0);
        let left = Region::nodes("Left", vec![vec![run(0, 1)]]);
        let a_left = RegionRef {
            prop: a.id,
            region: left.id,
        };
        a.regions.push(left);
        let mut b = line("B", 4, 3.0);
        let right = Region::nodes("Right", vec![vec![run(3, 2)]]);
        let b_right = RegionRef {
            prop: b.id,
            region: right.id,
        };
        b.regions.push(right);
        let mut group = Group::new("G");
        group.members = vec![a.id];
        group.submodels = vec![
            a_left,
            b_right,
            RegionRef {
                prop: b.id,
                region: RegionId::new(),
            },
        ];
        let gid = group.id;
        show.props = vec![a, b];
        show.groups.push(group);
        let geo = geometry(&show);
        let buffer = geo.buffer(Target::Group(gid));
        assert_eq!(
            buffer.global,
            vec![0, 1, 2, 3, 7, 6],
            "A's left half is already in"
        );
        assert!(
            geo.buffer(Target::Region {
                prop: show.props[0].id,
                region: RegionId::new()
            })
            .is_empty()
        );
    }
}
