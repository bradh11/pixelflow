//! Where every pixel sits, normalized per target, so effects can draw on any shape.
//!
//! Each target (a prop, a group of props, or a submodel) becomes a **pixel buffer**: its pixels
//! with (u, v) positions scaled to the target's bounding box (0–1, left to right and bottom to
//! top, front view), plus their order along the target. A group lays its members out as its
//! layout says, usually on xLights' minimal grid over the whole group, so a wave sweeps across
//! the whole group (see `styles.rs`, which also lays targets out in the other render styles).
//!
//! A submodel lays its pixels out the way xLights does (`SubModel.cpp`): with the default buffer
//! style each line is a row (or a column, for a vertical submodel), gaps included; "stacked
//! strands" puts every line on the same row; "keep XY" keeps the pixels where they are on the
//! prop; and a sub-buffer is the prop's own buffer cropped to its rectangle and stretched to
//! fill 0–1 again.

use pf_mapping::ChannelMap;
use pf_model::{
    BufferStyle, BufferTransform, GroupId, GroupLayout, GroupMember, LineLayout, PropId, Region, RegionId,
    RegionKind, RenderStyle, Show,
};
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
    /// For a per-model render style: the members, each drawn on its own buffer. Empty when the
    /// effect draws on this buffer.
    pub(crate) parts: Vec<Part>,
}

/// One member of a group drawn on its own (a per-model render style).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Part {
    pub buffer: PixelBuffer,
    /// Each of the part's pixels' place in the group's buffer.
    pub slots: Vec<u32>,
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

    /// The members drawn one by one, for a per-model render style (empty otherwise).
    pub fn parts(&self) -> impl Iterator<Item = &PixelBuffer> {
        self.parts.iter().map(|p| &p.buffer)
    }

    fn empty() -> Self {
        Self {
            pixels: Vec::new(),
            global: Vec::new(),
            columns: 1,
            rows: 1,
            parts: Vec::new(),
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
pub(crate) struct Point {
    pub global: u32,
    pub xy: Option<[f32; 2]>,
}

/// A prop or submodel: a target of its own, or a member of a group.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Member<'a> {
    Prop(&'a PropGeometry),
    Region(&'a PropGeometry, &'a Region),
}

impl Member<'_> {
    /// Its pixels, in its own order.
    pub fn points(&self) -> Vec<Point> {
        match *self {
            Member::Prop(prop) => (0..prop.node_count()).map(|n| prop.point(n)).collect(),
            Member::Region(prop, region) => region_nodes(prop, region)
                .into_iter()
                .map(|n| prop.point(n))
                .collect(),
        }
    }

    /// Its own buffer (the default render style).
    pub fn own_buffer(&self) -> PixelBuffer {
        match *self {
            Member::Prop(_) => build_buffer(&self.points()),
            Member::Region(prop, region) => region_buffer(prop, region),
        }
    }
}

/// A group's members and how it lays them out.
#[derive(Debug, Clone, PartialEq)]
struct GroupGeometry {
    members: Vec<GroupMember>,
    layout: GroupLayout,
    grid_size: u32,
}

/// Every prop's pixel positions and frame location, computed once per show and channel map.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneGeometry {
    pub(crate) props: Vec<PropGeometry>,
    index: HashMap<PropId, usize>,
    groups: HashMap<GroupId, GroupGeometry>,
    /// The layout's area from its origin (layout units), for groups on the whole layout's grid.
    pub(crate) area: [f32; 2],
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
            .map(|g| {
                let geometry = GroupGeometry {
                    members: g.members.clone(),
                    layout: g.layout,
                    grid_size: g.grid_size,
                };
                (g.id, geometry)
            })
            .collect();
        // Without the layout's size, the area out to the farthest prop.
        let area = show.layout_area.map_or_else(
            || {
                props
                    .iter()
                    .flat_map(|p| &p.points[..p.real])
                    .fold([0.0f32, 0.0f32], |[w, h], &[x, y]| [w.max(x), h.max(y)])
            },
            |a| [a.width, a.height],
        );
        Self {
            props,
            index,
            groups,
            area,
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

    /// The props a target draws on (a group's members' props, in member order), each once, as
    /// places in `props`.
    pub(crate) fn target_props(&self, target: Target) -> Vec<usize> {
        let mut seen = HashSet::new();
        let ids: Vec<PropId> = match target {
            Target::Prop(id) | Target::Region { prop: id, .. } => vec![id],
            Target::Group(id) => self
                .groups
                .get(&id)
                .map(|g| g.members.iter().map(GroupMember::prop).collect())
                .unwrap_or_default(),
        };
        ids.into_iter()
            .filter(|id| seen.insert(*id))
            .filter_map(|id| self.index.get(&id).copied())
            .collect()
    }

    /// The pixel buffer for a target, laid out its own way; empty when the target is unknown or
    /// has no pixels.
    pub fn buffer(&self, target: Target) -> PixelBuffer {
        self.styled_buffer(target, RenderStyle::Default, BufferTransform::None)
    }

    /// The pixel buffer for a target in a render style, turned or flipped. Every style of a
    /// target lists the same pixels in the same order.
    pub fn styled_buffer(
        &self,
        target: Target,
        style: RenderStyle,
        transform: BufferTransform,
    ) -> PixelBuffer {
        let mut buffer = match target {
            Target::Prop(id) => match self.prop(id) {
                Some(prop) => self.member_buffer(Member::Prop(prop), style),
                None => PixelBuffer::empty(),
            },
            Target::Group(id) => {
                let Some(group) = self.groups.get(&id) else {
                    return PixelBuffer::empty();
                };
                // Members in order, whole props and submodels mixed, as xLights lists them.
                let members: Vec<Member> = group
                    .members
                    .iter()
                    .filter_map(|member| {
                        let prop = self.prop(member.prop())?;
                        Some(match member {
                            GroupMember::Prop(_) => Member::Prop(prop),
                            GroupMember::Region(r) => Member::Region(prop, prop.region(r.region)?),
                        })
                    })
                    .collect();
                self.group_buffer(&members, group.layout, group.grid_size, style)
            }
            Target::Region { prop, region } => {
                match self.prop(prop).and_then(|p| Some((p, p.region(region)?))) {
                    Some((prop, region)) => self.member_buffer(Member::Region(prop, region), style),
                    None => PixelBuffer::empty(),
                }
            }
        };
        if buffer.is_empty() {
            return PixelBuffer::empty();
        }
        buffer.transform(transform);
        buffer
    }
}

fn finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

/// The prop's nodes a region lights, in the region's order. A sub-buffer takes the prop's
/// pixels inside its rectangle, and a Keep XY submodel its nodes, in wiring order.
fn region_nodes(prop: &PropGeometry, region: &Region) -> Vec<u32> {
    match region.kind {
        RegionKind::SubBuffer { x1, y1, x2, y2 } => sub_buffer_cells(prop, [x1, y1, x2, y2])
            .1
            .into_iter()
            .map(|c| c.node)
            .collect(),
        RegionKind::Nodes {
            buffer: BufferStyle::KeepXy,
            ..
        } => {
            let mut nodes = region.node_list(prop.node_count());
            nodes.sort_unstable();
            nodes
        }
        _ => region.node_list(prop.node_count()),
    }
}

/// A sub-buffer edge pair as a span of buffer cells, the way xLights'
/// `SubModel::initSubbufferRange` computes it: each percent scales to `cells` (the parent
/// buffer's width or height), the start edge rounds, and the end edge truncates (it is passed
/// to `Model::IsNodeInBufferRange` as an int). Both ends are inside.
fn cell_span(a: f32, b: f32, cells: u32) -> (i64, i64) {
    let (lo, hi) = (a.min(b), a.max(b));
    // xLights: `x *= (float)W; x /= 100.0;` (the division in double, stored back as float).
    let scale = |pct: f32| (f64::from(pct * cells as f32) / 100.0) as f32;
    (scale(lo).round() as i64, scale(hi).trunc() as i64)
}

/// One of the prop's pixels in a sub-buffer: its node, its cell on the prop's buffer grid, and
/// its show-wide index.
#[derive(Debug, Clone, Copy)]
struct Cell {
    node: u32,
    col: i64,
    row: i64,
    global: u32,
}

/// The prop's pixels inside a sub-buffer rectangle (percent edges `[x1, y1, x2, y2]`), in
/// wiring order. Each pixel sits in the cell of the prop's buffer grid nearest its (u, v), and
/// is in when that cell is in the rectangle's span on both axes (`Model::IsNodeInBufferRange`).
/// Also returns the prop's whole buffer.
fn sub_buffer_cells(prop: &PropGeometry, edges: [f32; 4]) -> (PixelBuffer, Vec<Cell>) {
    let whole = build_buffer(&(0..prop.node_count()).map(|n| prop.point(n)).collect::<Vec<_>>());
    let [x1, y1, x2, y2] = edges;
    let (cols, rows) = (whole.columns.max(1), whole.rows.max(1));
    let (lo_x, hi_x) = cell_span(x1, x2, cols);
    let (lo_y, hi_y) = cell_span(y1, y2, rows);
    let cell = |at: f32, cells: u32| (at * (cells - 1) as f32).round() as i64;
    let inside = whole
        .pixels
        .iter()
        .zip(&whole.global)
        .enumerate()
        .map(|(n, (px, &global))| Cell {
            node: n as u32,
            col: cell(px.u, cols),
            row: cell(px.v, rows),
            global,
        })
        .filter(|c| (lo_x..=hi_x).contains(&c.col) && (lo_y..=hi_y).contains(&c.row))
        .collect();
    (whole, inside)
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
            // As xLights does: the cells taken, shifted to start at 0, make a buffer just big
            // enough to hold them.
            let (_, cells) = sub_buffer_cells(prop, [*x1, *y1, *x2, *y2]);
            if cells.is_empty() {
                return PixelBuffer::empty();
            }
            let (min_c, max_c) = cells
                .iter()
                .fold((i64::MAX, i64::MIN), |(lo, hi), c| (lo.min(c.col), hi.max(c.col)));
            let (min_r, max_r) = cells
                .iter()
                .fold((i64::MAX, i64::MIN), |(lo, hi), c| (lo.min(c.row), hi.max(c.row)));
            let stretch = |at: i64, lo: i64, hi: i64| {
                if hi <= lo {
                    0.5
                } else {
                    (at - lo) as f32 / (hi - lo) as f32
                }
            };
            let count = u32::try_from(cells.len()).unwrap_or(u32::MAX);
            PixelBuffer {
                pixels: cells
                    .iter()
                    .enumerate()
                    .map(|(i, c)| Pixel {
                        u: stretch(c.col, min_c, max_c),
                        v: stretch(c.row, min_r, max_r),
                        index: i as u32,
                        count,
                    })
                    .collect(),
                global: cells.iter().map(|c| c.global).collect(),
                columns: u32::try_from(max_c - min_c + 1).unwrap_or(1),
                rows: u32::try_from(max_r - min_r + 1).unwrap_or(1),
                parts: Vec::new(),
            }
        }
        // Keep XY and faces: the pixels where they are on the prop, in node order (xLights keeps
        // a Keep XY submodel's nodes in a sorted set, so a backwards line still runs forwards).
        _ => {
            let mut nodes = region.node_list(prop.node_count());
            nodes.sort_unstable();
            build_buffer(&nodes.into_iter().map(|n| prop.point(n)).collect::<Vec<_>>())
        }
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
        parts: Vec::new(),
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
        parts: Vec::new(),
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
    use pf_model::{Generator, Group, NodeRun, Prop, RegionRef, ShapeSource, Transform, Vec3};

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
    fn a_group_shares_one_grid_and_counts_across_members() {
        let mut show = Show::new("t");
        show.props.push(line("A", 3, 0.0));
        show.props.push(line("B", 3, 3.0));
        let mut group = Group::new("G");
        group.members = vec![
            show.props[1].id.into(),
            show.props[0].id.into(),
            show.props[1].id.into(),
        ];
        let gid = group.id;
        show.groups.push(group);
        let geo = geometry(&show);
        let buffer = geo.buffer(Target::Group(gid));
        assert_eq!(buffer.len(), 6, "a repeated member is drawn once");
        // B (x from 2.5 to 3.5) comes first. In xLights units the group spans x from -50 to 350,
        // 401 units: more than the 400-cell grid, so x scales by 400 / 401 (and truncates).
        assert_eq!((buffer.columns, buffer.rows), (400, 1));
        let cells: Vec<u32> = buffer
            .pixels
            .iter()
            .map(|p| (p.u * 399.0).round() as u32)
            .collect();
        assert_eq!(cells, vec![299, 349, 399, 0, 49, 99]);
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
        // In node order, as xLights keeps Keep XY nodes (a sorted set), whatever the lines say.
        assert_eq!(uvs(&buffer), vec![(2, 0.0, 0.5), (5, 0.5, 0.5), (8, 1.0, 0.5)]);
        let (geo, target) = line_with(submodel(
            vec![vec![run(9, 0)]],
            LineLayout::Horizontal,
            BufferStyle::KeepXy,
        ));
        assert_eq!(
            geo.buffer(target).global,
            (0..10).collect::<Vec<u32>>(),
            "a backwards run chases forwards"
        );

        let face = pf_model::FaceDefinition {
            outline: vec![pf_model::NodeRange::new(4, 7)],
            ..Default::default()
        };
        let (geo, target) = line_with(Region::face("Face", face));
        assert_eq!(geo.buffer(target).global, vec![4, 5, 6]);
    }

    /// A `columns` × `rows` matrix whose rough buffer grid is exactly its cells.
    fn grid_with(
        columns: u32,
        rows: u32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    ) -> (SceneGeometry, PropId, Target) {
        let mut show = Show::new("t");
        let mut grid = Prop::new(
            "Grid",
            ShapeSource::Generator(Generator::Matrix {
                columns,
                rows,
                width: columns as f32,
                height: rows as f32,
                wiring: Default::default(),
            }),
        );
        let window = Region {
            kind: RegionKind::SubBuffer { x1, y1, x2, y2 },
            ..Region::nodes("Window", vec![])
        };
        let target = Target::Region {
            prop: grid.id,
            region: window.id,
        };
        let id = grid.id;
        grid.regions.push(window);
        show.props.push(grid);
        (geometry(&show), id, target)
    }

    /// The (column, row) cells a sub-buffer takes, read off the whole prop's buffer.
    fn cells(geo: &SceneGeometry, prop: PropId, target: Target) -> Vec<(u32, u32)> {
        let whole = geo.buffer(Target::Prop(prop));
        let at: HashMap<u32, (u32, u32)> = whole
            .global
            .iter()
            .zip(&whole.pixels)
            .map(|(g, p)| {
                let col = (p.u * (whole.columns - 1) as f32).round() as u32;
                let row = (p.v * (whole.rows - 1) as f32).round() as u32;
                (*g, (col, row))
            })
            .collect();
        let mut out: Vec<(u32, u32)> = geo.buffer(target).global.iter().map(|g| at[g]).collect();
        out.sort();
        out
    }

    #[test]
    fn sub_buffers_crop_the_prop_and_stretch_back_to_fill() {
        // The lower right of a 5 × 5 grid: columns round(2.5)=3 to 5, rows 0 to trunc(2.5)=2.
        let (geo, prop, target) = grid_with(5, 5, 50.0, 0.0, 100.0, 50.0);
        let buffer = geo.buffer(target);
        assert_eq!(buffer.len(), 6);
        assert_eq!((buffer.columns, buffer.rows), (2, 3));
        assert_eq!(
            cells(&geo, prop, target),
            vec![(3, 0), (3, 1), (3, 2), (4, 0), (4, 1), (4, 2)]
        );
        let mut us: Vec<f32> = buffer.pixels.iter().map(|p| p.u).collect();
        us.sort_by(f32::total_cmp);
        us.dedup();
        assert_eq!(us, vec![0.0, 1.0]);
        assert!(buffer.pixels.iter().all(|p| (0.0..=1.0).contains(&p.v)));
        // A thin rectangle still takes the cell its start edge rounds to.
        let (geo, _, target) = grid_with(5, 5, 0.0, 0.0, 10.0, 10.0);
        assert_eq!(geo.buffer(target).len(), 1, "the corner cell (0, 0)");
    }

    #[test]
    fn sub_buffer_edges_follow_xlights() {
        let cases: &[(u32, f32, f32, (u32, u32))] = &[
            // 10 wide: halves share column 5; thirds give 0-3, 3-6, 7-9.
            (10, 0.0, 50.0, (0, 5)),
            (10, 50.0, 100.0, (5, 9)),
            (10, 0.0, 33.0, (0, 3)),
            (10, 33.0, 66.0, (3, 6)),
            (10, 66.0, 100.0, (7, 9)),
            // 7 wide: 3.5 rounds up to 4 for a start, truncates to 3 for an end.
            (7, 0.0, 50.0, (0, 3)),
            (7, 50.0, 100.0, (4, 6)),
            (7, 0.0, 33.0, (0, 2)),
            (7, 33.0, 66.0, (2, 4)),
            (7, 66.0, 100.0, (5, 6)),
            // Reversed edges are swapped first.
            (10, 50.0, 0.0, (0, 5)),
        ];
        for &(width, x1, x2, (lo, hi)) in cases {
            // Rows: 5 tall, bottom half 0-50% gives rows 0-2 (2.5 truncates to 2).
            let (geo, prop, target) = grid_with(width, 5, x1, 0.0, x2, 50.0);
            let expected: Vec<(u32, u32)> = (lo..=hi).flat_map(|c| (0..=2).map(move |r| (c, r))).collect();
            assert_eq!(cells(&geo, prop, target), expected, "{width} wide, {x1}-{x2}%");
            let buffer = geo.buffer(target);
            assert_eq!(
                (buffer.columns, buffer.rows),
                (hi - lo + 1, 3),
                "{width} wide, {x1}-{x2}%: sized to the cells it takes"
            );
        }
        // The top half of 5 rows: 2.5 rounds up to 3 for the start.
        let (geo, prop, target) = grid_with(4, 5, 0.0, 50.0, 100.0, 100.0);
        let rows: HashSet<u32> = cells(&geo, prop, target).into_iter().map(|(_, r)| r).collect();
        assert_eq!(rows, HashSet::from([3, 4]));
    }

    #[test]
    fn a_sub_buffer_stretches_its_cells_to_fill() {
        // Columns 0-5 of 10, rows 0-2 of 5: u steps by 1/5, v by 1/2.
        let (geo, _, target) = grid_with(10, 5, 0.0, 0.0, 50.0, 50.0);
        let buffer = geo.buffer(target);
        let mut us: Vec<f32> = buffer.pixels.iter().map(|p| p.u).collect();
        us.sort_by(f32::total_cmp);
        us.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        let expected: Vec<f32> = (0..=5).map(|c| c as f32 / 5.0).collect();
        assert!(
            us.iter().zip(&expected).all(|(a, b)| (a - b).abs() < 1e-6) && us.len() == 6,
            "{us:?}"
        );
        let mut vs: Vec<f32> = buffer.pixels.iter().map(|p| p.v).collect();
        vs.sort_by(f32::total_cmp);
        vs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        assert_eq!(vs, vec![0.0, 0.5, 1.0]);
        assert!(
            buffer
                .pixels
                .iter()
                .enumerate()
                .all(|(i, p)| p.index == i as u32 && p.count == 18)
        );
    }

    #[test]
    fn groups_draw_members_in_order_with_submodels_mixed_in() {
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
        group.members = vec![
            b_right.into(),
            a.id.into(),
            a_left.into(),
            RegionRef {
                prop: b.id,
                region: RegionId::new(),
            }
            .into(),
        ];
        let gid = group.id;
        show.props = vec![a, b];
        show.groups.push(group);
        let geo = geometry(&show);
        let buffer = geo.buffer(Target::Group(gid));
        assert_eq!(
            buffer.global,
            vec![7, 6, 0, 1, 2, 3],
            "B's right half first, then A; A's left half is already in"
        );
        assert_eq!(geo.target_props(Target::Group(gid)).len(), 2);
        assert!(
            geo.buffer(Target::Region {
                prop: show.props[0].id,
                region: RegionId::new()
            })
            .is_empty()
        );
    }
}
