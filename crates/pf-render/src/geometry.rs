//! Where every pixel sits, normalized per target, so effects can draw on any shape.
//!
//! Each target (a prop, or a group of props) becomes a **pixel buffer**: its pixels with (u, v)
//! positions scaled to the target's bounding box (0–1, left to right and bottom to top, front
//! view), plus their order along the target. A group uses the combined bounding box of its
//! members, so a wave sweeps across the whole group.

use pf_mapping::ChannelMap;
use pf_model::{GroupId, PropId, Show};
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
}

/// Every prop's pixel positions and frame location, computed once per show and channel map.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneGeometry {
    pub(crate) props: Vec<PropGeometry>,
    index: HashMap<PropId, usize>,
    groups: HashMap<GroupId, Vec<PropId>>,
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
            // A shape with fewer points than nodes still gets every pixel (at the origin).
            points.resize(nodes, [0.0, 0.0]);
            index.entry(layout.prop).or_insert(props.len());
            props.push(PropGeometry {
                frame_offset: layout.frame_offset,
                channels_per_pixel: layout.channels_per_pixel,
                first_pixel,
                points,
            });
            first_pixel += nodes;
        }
        let groups = show.groups.iter().map(|g| (g.id, g.members.clone())).collect();
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

    /// The pixel buffer for a target; empty when the target is unknown or has no pixels.
    pub fn buffer(&self, target: Target) -> PixelBuffer {
        let members: Vec<&PropGeometry> = match target {
            Target::Prop(id) => self.index.get(&id).map(|&i| &self.props[i]).into_iter().collect(),
            Target::Group(id) => {
                let mut seen = HashSet::new();
                self.groups
                    .get(&id)
                    .into_iter()
                    .flatten()
                    .filter(|m| seen.insert(**m))
                    .filter_map(|m| self.index.get(m).map(|&i| &self.props[i]))
                    .collect()
            }
        };
        build_buffer(&members)
    }
}

fn finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

fn build_buffer(members: &[&PropGeometry]) -> PixelBuffer {
    let count: usize = members.iter().map(|p| p.points.len()).sum();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in members.iter().flat_map(|p| &p.points) {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    }
    let (width, height) = ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0));
    // A flat side (a straight horizontal line has no height) puts every pixel in the middle.
    let extent = width.max(height);
    let flat = |size: f32| size <= extent * 1e-6 || size <= f32::MIN_POSITIVE;
    let (flat_x, flat_y) = (flat(width), flat(height));
    let count_u32 = u32::try_from(count).unwrap_or(u32::MAX);
    let mut pixels = Vec::with_capacity(count);
    let mut global = Vec::with_capacity(count);
    for prop in members {
        for (node, p) in prop.points.iter().enumerate() {
            let u = if flat_x { 0.5 } else { (p[0] - min_x) / width };
            let v = if flat_y { 0.5 } else { (p[1] - min_y) / height };
            pixels.push(Pixel {
                u,
                v,
                index: u32::try_from(pixels.len()).unwrap_or(u32::MAX),
                count: count_u32,
            });
            global.push(u32::try_from(prop.first_pixel + node).unwrap_or(u32::MAX));
        }
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
    use pf_model::{Generator, Group, Prop, ShapeSource, Transform, Vec3};

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
}
