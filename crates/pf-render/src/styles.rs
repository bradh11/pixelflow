//! Render styles: how a target's pixels are laid out for an effect, the way xLights lays out a
//! render buffer (`ModelGroup::InitRenderBufferNodes` and `Model::InitRenderBufferNodes`).
//!
//! xLights draws every effect on a grid of cells. A group's own layout is usually its **minimal
//! grid**: each pixel in the cell where it sits in the layout, with the grid scaled so its longer
//! side has up to the group's grid size (400) cells, cut down to the pixels. A whole-house group
//! is then about 400 cells across with most cells empty, so sizes in cells (a shape's thickness,
//! a fan's blades) are as fine as xLights draws them. The other styles lay the members out
//! one by one: each a column ("horizontal per model"), side by side ("horizontal stack"), on top
//! of each other ("overlay"), or each drawn on its own ("per model").
//!
//! Positions are worked out in xLights' layout units (a PixelFlow layout unit is 100 of them, as
//! the xLights import places props), so a group lands on the same grid as in xLights. Each pixel's
//! cell `x` of a `width`-cell grid becomes `u = x / (width - 1)` (0.5 for a single cell), so
//! effects find it back in the same cell. Every style of one target lists the same pixels in the
//! same order; only where they are on the grid differs.

use crate::geometry::{Member, Members, Pixel, PixelBuffer, Point, SceneGeometry};
use pf_model::{BufferTransform, GroupLayout, MAX_GRID_SIZE, MIN_GRID_SIZE, RenderStyle};
use std::collections::HashMap;
use std::sync::Arc;

/// xLights layout units per PixelFlow layout unit (the xLights import's scale).
pub(crate) const XLIGHTS_UNITS: f32 = 100.0;

/// A member's pixels and where they sit on a grid: `(column, row)` per pixel (`None`: a padding
/// pixel with no position, drawn in the middle), and the grid's size.
struct Cells {
    at: Vec<Option<(i64, i64)>>,
    width: i64,
    height: i64,
}

/// One cell per pixel at `u` and `v` (0–1), for `width` × `height` cells.
fn unit(at: i64, cells: i64) -> f32 {
    if cells <= 1 {
        0.5
    } else {
        at as f32 / (cells - 1) as f32
    }
}

/// The cell a buffer pixel falls in (as effects find it).
fn cell_of(px: &Pixel, columns: u32, rows: u32) -> (i64, i64) {
    let place = |at: f32, cells: u32| -> i64 {
        if cells <= 1 || !at.is_finite() {
            0
        } else {
            (at.clamp(0.0, 1.0) * (cells - 1) as f32).round() as i64
        }
    };
    (place(px.u, columns), place(px.v, rows))
}

/// A buffer of `points` (in order, counted across all of them) placed on `cells`.
fn on_cells(points: &[Point], cells: &Cells) -> PixelBuffer {
    let count = u32::try_from(points.len()).unwrap_or(u32::MAX);
    let (width, height) = (cells.width.max(1), cells.height.max(1));
    PixelBuffer {
        pixels: cells
            .at
            .iter()
            .enumerate()
            .map(|(i, at)| {
                let (u, v) = at.map_or((0.5, 0.5), |(x, y)| (unit(x, width), unit(y, height)));
                Pixel {
                    u,
                    v,
                    index: i as u32,
                    count,
                }
            })
            .collect(),
        global: points.iter().map(|p| p.global).collect(),
        columns: u32::try_from(width).unwrap_or(u32::MAX),
        rows: u32::try_from(height).unwrap_or(u32::MAX),
        parts: Vec::new(),
        members: None,
    }
}

/// The real positions of `points` in xLights units, and their bounding box (`None` when every
/// point is padding).
fn xlights_positions(points: &[Point]) -> (Vec<Option<[f32; 2]>>, Option<[f32; 4]>) {
    let at: Vec<Option<[f32; 2]>> = points
        .iter()
        .map(|p| p.xy.map(|[x, y]| [x * XLIGHTS_UNITS, y * XLIGHTS_UNITS]))
        .collect();
    let bounds = at.iter().flatten().fold(None, |b: Option<[f32; 4]>, &[x, y]| {
        Some(match b {
            None => [x, y, x, y],
            Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
        })
    });
    (at, bounds)
}

/// The group grid (`ModelGroup::RebuildBuffers`): each pixel in the cell where it is, the grid
/// scaled down so neither side has more than `grid_size` cells. `area` is the layout's size in
/// xLights units for the whole-layout grid, `None` for the minimal grid (cut down to the pixels).
fn grid_cells(points: &[Point], grid_size: u32, area: Option<[f32; 2]>) -> Cells {
    let (at, bounds) = xlights_positions(points);
    let Some([mut min_x, mut min_y, mut max_x, mut max_y]) = bounds else {
        return Cells {
            at: vec![None; points.len()],
            width: 1,
            height: 1,
        };
    };
    // Off the layout's bottom or left edge: everything shifts up or right to start at 0.
    let (shift_x, shift_y) = (min_x.min(0.0), min_y.min(0.0));
    (min_x, max_x, min_y, max_y) = (min_x - shift_x, max_x - shift_x, min_y - shift_y, max_y - shift_y);
    let g = grid_size as f32;
    let scale: f32 = match area {
        None => {
            if max_y - min_y + 1.0 < g && max_x - min_x + 1.0 < g {
                1.0
            } else {
                (g / (max_y - min_y + 1.0)).min(g / (max_x - min_x + 1.0))
            }
        }
        Some([w, h]) => {
            (min_x, min_y, max_x, max_y) = (0.0, 0.0, max_x.max(w), max_y.max(h));
            if max_y < g && max_x < g {
                1.0
            } else {
                (g / max_y).min(g / max_x)
            }
        }
    };
    let scale = f64::from(scale);
    let place = |v: f32, shift: f32, min: f32| (f64::from(v - shift - min) * scale) as i64;
    let at: Vec<Option<(i64, i64)>> = at
        .iter()
        .map(|p| p.map(|[x, y]| (place(x, shift_x, min_x), place(y, shift_y, min_y))))
        .collect();
    let (mut width, mut height) = at
        .iter()
        .flatten()
        .fold((0, 0), |(w, h), &(x, y)| (w.max(x + 1), h.max(y + 1)));
    if let Some([w, h]) = area {
        width = width.max((f64::from(w) * scale) as i64);
        height = height.max((f64::from(h) * scale) as i64);
    }
    Cells { at, width, height }
}

/// "Per Preview" (`Model::InitRenderBufferNodes`): each pixel in the cell nearest where it is.
/// A group (`grid_size` given) scales down to its grid size; a prop or submodel whose pixels are
/// close together spreads them out (about four empty cells per pixel, up to 400 across).
fn preview_cells(points: &[Point], grid_size: Option<u32>) -> Cells {
    let (at, bounds) = xlights_positions(points);
    let Some([min_x, min_y, max_x, max_y]) = bounds else {
        return Cells {
            at: vec![None; points.len()],
            width: 1,
            height: 1,
        };
    };
    let (span_x, span_y) = (max_x - min_x, max_y - min_y);
    let mut factor = 1.0f32;
    if let Some(g) = grid_size.map(|g| g as f32)
        && (span_x > g || span_y > g)
    {
        factor = (span_x / g).max(span_y / g).max(1.0);
    }
    if span_x > 2048.0 || span_y > 2048.0 {
        // xLights keeps any buffer under about 2048 cells across.
        factor = (span_x / 2048.0).max(span_y / 2048.0);
    }
    if grid_size.is_none() && factor == 1.0 && points.len() as f64 * 5.0 > f64::from(span_x * span_y) {
        let (dx, dy) = (
            if span_x == 0.0 { 0.01 } else { span_x },
            if span_y == 0.0 { 0.01 } else { span_y },
        );
        let across = (points.len() as f32 * 5.0 * (dx / dy)).sqrt();
        factor = dx / across;
        if (dx / factor).max(dy / factor) > 400.0 {
            factor = dx.max(dy) / 400.0;
        }
    }
    let (off_x, off_y) = (min_x / factor, min_y / factor);
    let at: Vec<Option<(i64, i64)>> = at
        .iter()
        .map(|p| {
            p.map(|[x, y]| {
                (
                    (x / factor - off_x).round() as i64,
                    (y / factor - off_y).round() as i64,
                )
            })
        })
        .collect();
    let (width, height) = at
        .iter()
        .flatten()
        .fold((0, 0), |(w, h), &(x, y)| (w.max(x + 1), h.max(y + 1)));
    Cells { at, width, height }
}

/// Every pixel in a row, in order.
fn line_cells(count: usize) -> Cells {
    Cells {
        at: (0..count as i64).map(|x| Some((x, 0))).collect(),
        width: count as i64,
        height: 1,
    }
}

impl SceneGeometry {
    /// A prop's or submodel's buffer in `style`: its own layout, per preview, single line, or as
    /// one cell. Group styles draw as its own layout; per-model styles as the style they name.
    pub(crate) fn member_buffer(&self, member: Member, style: RenderStyle) -> PixelBuffer {
        let style = style.per_model().unwrap_or(style);
        let points = || member.points();
        match style {
            RenderStyle::PerPreview => {
                let points = points();
                on_cells(&points, &preview_cells(&points, None))
            }
            RenderStyle::SingleLine => {
                let points = points();
                on_cells(&points, &line_cells(points.len()))
            }
            RenderStyle::AsPixel => {
                let points = points();
                let cells = Cells {
                    at: vec![Some((0, 0)); points.len()],
                    width: 1,
                    height: 1,
                };
                on_cells(&points, &cells)
            }
            _ => member.own_buffer(),
        }
    }

    /// A group's buffer in `style` (the group's layout for the default style).
    pub(crate) fn group_buffer(
        &self,
        members: &[Member],
        layout: GroupLayout,
        grid_size: u32,
        style: RenderStyle,
    ) -> PixelBuffer {
        let grid_size = grid_size.clamp(MIN_GRID_SIZE, MAX_GRID_SIZE);
        // Each member's pixels, then the group's: each pixel once, in its first member.
        let lists: Vec<Vec<Point>> = members.iter().map(Member::points).collect();
        let mut place: HashMap<u32, usize> = HashMap::new();
        let mut points = Vec::new();
        let mut owner = Vec::new();
        for (m, list) in lists.iter().enumerate() {
            for (k, p) in list.iter().enumerate() {
                if let std::collections::hash_map::Entry::Vacant(e) = place.entry(p.global) {
                    e.insert(points.len());
                    points.push(*p);
                    owner.push((m, k));
                }
            }
        }
        let style = match style {
            RenderStyle::Default => layout.style(),
            style => Some(style),
        };
        let grid = || {
            let area = (layout == GroupLayout::Grid).then(|| self.area.map(|a| a * XLIGHTS_UNITS));
            grid_cells(&points, grid_size, area)
        };
        // Members' own buffers, and where each of the group's pixels is on its member's.
        let own = || -> (Vec<PixelBuffer>, Vec<(i64, i64)>) {
            let buffers: Vec<PixelBuffer> = members.iter().map(Member::own_buffer).collect();
            let spots: Vec<HashMap<u32, (i64, i64)>> = buffers
                .iter()
                .map(|b| {
                    b.global
                        .iter()
                        .zip(&b.pixels)
                        .map(|(&g, px)| (g, cell_of(px, b.columns, b.rows)))
                        .collect()
                })
                .collect();
            let at = points
                .iter()
                .zip(&owner)
                .map(|(p, &(m, _))| spots[m].get(&p.global).copied().unwrap_or((0, 0)))
                .collect();
            (buffers, at)
        };
        let models = members.len() as i64;
        let cells = match style {
            None => grid(),
            Some(RenderStyle::PerPreview) => preview_cells(&points, Some(grid_size)),
            Some(RenderStyle::SingleLine) => line_cells(points.len()),
            Some(RenderStyle::AsPixel) => Cells {
                at: vec![Some((0, 0)); points.len()],
                width: 1,
                height: 1,
            },
            Some(RenderStyle::HorizontalPerModel | RenderStyle::VerticalPerModel) => {
                let longest = lists.iter().map(Vec::len).max().unwrap_or(0) as i64;
                let across = style == Some(RenderStyle::HorizontalPerModel);
                Cells {
                    at: owner
                        .iter()
                        .map(|&(m, k)| {
                            Some(if across {
                                (m as i64, k as i64)
                            } else {
                                (k as i64, m as i64)
                            })
                        })
                        .collect(),
                    width: if across { models } else { longest },
                    height: if across { longest } else { models },
                }
            }
            Some(
                s @ (RenderStyle::HorizontalStack
                | RenderStyle::VerticalStack
                | RenderStyle::HorizontalStackScaled
                | RenderStyle::VerticalStackScaled
                | RenderStyle::OverlayCentered
                | RenderStyle::OverlayScaled),
            ) => {
                let (buffers, at) = own();
                let size = |b: &PixelBuffer| (i64::from(b.columns.max(1)), i64::from(b.rows.max(1)));
                let (max_w, max_h) = buffers
                    .iter()
                    .map(size)
                    .fold((0, 0), |(w, h), (bw, bh)| (w.max(bw), h.max(bh)));
                // Where each member starts (stacks) and the whole buffer's size.
                let mut starts = Vec::with_capacity(buffers.len());
                let (width, height) = match s {
                    RenderStyle::HorizontalStack => {
                        let mut x = 0;
                        for b in &buffers {
                            starts.push(x);
                            x += size(b).0;
                        }
                        (x, max_h)
                    }
                    RenderStyle::VerticalStack => {
                        let mut y = 0;
                        for b in &buffers {
                            starts.push(y);
                            y += size(b).1;
                        }
                        (max_w, y)
                    }
                    RenderStyle::HorizontalStackScaled => (max_w * models, max_h),
                    RenderStyle::VerticalStackScaled => (max_w, max_h * models),
                    _ => (max_w, max_h),
                };
                let at = at
                    .iter()
                    .zip(&owner)
                    .map(|(&(x, y), &(m, _))| {
                        let (bw, bh) = size(&buffers[m]);
                        let stretch =
                            |at: i64, to: i64, from: i64| (at as f64 * (to as f64 / from as f64)) as i64;
                        Some(match s {
                            RenderStyle::HorizontalStack => (x + starts[m], y),
                            RenderStyle::VerticalStack => (x, y + starts[m]),
                            RenderStyle::HorizontalStackScaled => {
                                let each = width / models.max(1);
                                (stretch(x, each, bw) + each * m as i64, stretch(y, height, bh))
                            }
                            RenderStyle::VerticalStackScaled => {
                                let each = height / models.max(1);
                                (stretch(x, width, bw), stretch(y, each, bh) + each * m as i64)
                            }
                            _ if (bw, bh) == (width, height) => (x, y),
                            RenderStyle::OverlayScaled => (stretch(x, width, bw), stretch(y, height, bh)),
                            _ => (x + (width - bw) / 2, y + (height - bh) / 2),
                        })
                    })
                    .collect();
                Cells { at, width, height }
            }
            Some(RenderStyle::SingleLineModelAsPixel) => Cells {
                at: owner.iter().map(|&(m, _)| Some((m as i64, 0))).collect(),
                width: models,
                height: 1,
            },
            Some(RenderStyle::DefaultModelAsPixel) => {
                // Per preview, then each member's pixels all in the cell at their middle.
                let mut cells = preview_cells(&points, Some(grid_size));
                let mut sums = vec![(0i64, 0i64, 0i64); members.len()];
                for (at, &(m, _)) in cells.at.iter().zip(&owner) {
                    if let Some((x, y)) = at {
                        sums[m] = (sums[m].0 + x, sums[m].1 + y, sums[m].2 + 1);
                    }
                }
                for (at, &(m, _)) in cells.at.iter_mut().zip(&owner) {
                    let (x, y, n) = sums[m];
                    if n > 0 {
                        *at = Some((x / n, y / n));
                    }
                }
                cells
            }
            Some(
                RenderStyle::PerModelDefault
                | RenderStyle::PerModelPerPreview
                | RenderStyle::PerModelSingleLine,
            ) => {
                // Drawn member by member, each on its own buffer; the group's own grid stays for
                // anything that looks at the whole group.
                let per = style.and_then(RenderStyle::per_model).unwrap_or_default();
                let mut buffer = on_cells(&points, &grid());
                buffer.parts = members
                    .iter()
                    .enumerate()
                    .filter_map(|(m, &member)| {
                        let mut part = self.member_buffer(member, per);
                        // Pixels in an earlier member are drawn there.
                        let keep: Vec<(usize, u32)> = part
                            .global
                            .iter()
                            .enumerate()
                            .filter_map(|(i, g)| {
                                let slot = *place.get(g)?;
                                (owner[slot].0 == m).then_some((i, slot as u32))
                            })
                            .collect();
                        if keep.is_empty() {
                            return None;
                        }
                        if keep.len() < part.pixels.len() {
                            part.pixels = keep.iter().map(|&(i, _)| part.pixels[i]).collect();
                            part.global = keep.iter().map(|&(i, _)| part.global[i]).collect();
                        }
                        Some(crate::geometry::Part {
                            buffer: part,
                            slots: keep.into_iter().map(|(_, slot)| slot).collect(),
                        })
                    })
                    .collect();
                buffer.members = Some(members_of(&owner, members.len(), &buffer));
                return buffer;
            }
            // Only the styles above lay out a group.
            Some(_) => grid(),
        };
        let mut buffer = on_cells(&points, &cells);
        buffer.members = Some(members_of(&owner, members.len(), &buffer));
        buffer
    }
}

/// Which member each of a group buffer's pixels belongs to (`owner`: each pixel's member and
/// place in it).
fn members_of(owner: &[(usize, usize)], count: usize, buffer: &PixelBuffer) -> Arc<Members> {
    let of = owner.iter().map(|&(m, _)| m as u32).collect();
    Arc::new(Members::new(of, count, &buffer.pixels))
}

impl PixelBuffer {
    /// Turns or flips the buffer (and each part of a per-model one).
    pub(crate) fn transform(&mut self, transform: BufferTransform) {
        if transform == BufferTransform::None {
            return;
        }
        for px in &mut self.pixels {
            // On the unit square, as on a 2 × 2 grid: a cell at x goes to 1 - x.
            let (u, v) = transform.apply(f64::from(px.u), f64::from(px.v), 2.0, 2.0);
            (px.u, px.v) = (u as f32, v as f32);
        }
        if transform.turns() {
            std::mem::swap(&mut self.columns, &mut self.rows);
        }
        if let Some(members) = &self.members {
            self.members = Some(Arc::new(members.moved(&self.pixels)));
        }
        for part in &mut self.parts {
            part.buffer.transform(transform);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::{Generator, Group, LayoutArea, Prop, ShapeSource, Show, Transform, Vec3};
    use pf_sequence::Target;

    fn points(xy: &[[f32; 2]]) -> Vec<Point> {
        xy.iter()
            .enumerate()
            .map(|(i, &[x, y])| Point {
                global: i as u32,
                // In PixelFlow units: xLights units / 100.
                xy: Some([x / XLIGHTS_UNITS, y / XLIGHTS_UNITS]),
            })
            .collect()
    }

    fn cells(c: &Cells) -> Vec<(i64, i64)> {
        c.at.iter().map(|a| a.unwrap()).collect()
    }

    #[test]
    fn a_small_minimal_grid_keeps_layout_units() {
        // Spans under the grid size: one cell per xLights unit, from the lowest pixel.
        let c = grid_cells(&points(&[[100.0, 50.0], [110.5, 50.0], [150.0, 80.9]]), 400, None);
        assert_eq!(cells(&c), vec![(0, 0), (10, 0), (50, 30)]);
        assert_eq!((c.width, c.height), (51, 31));
    }

    #[test]
    fn a_large_minimal_grid_scales_to_the_grid_size() {
        // 0-1000 across, 0-300 up: scale = 400 / 1001, so x = trunc(x * 0.3996).
        let c = grid_cells(
            &points(&[[0.0, 0.0], [500.0, 300.0], [1000.0, 150.0], [-0.0, 299.0]]),
            400,
            None,
        );
        assert_eq!(cells(&c), vec![(0, 0), (199, 119), (399, 59), (0, 119)]);
        assert_eq!((c.width, c.height), (400, 120));
    }

    #[test]
    fn the_whole_layout_grid_starts_at_the_origin_and_covers_the_area() {
        // A 1000 × 500 layout, grid 400: scale = min(400/500, 400/1000) = 0.4.
        let c = grid_cells(
            &points(&[[100.0, 100.0], [300.0, 200.0]]),
            400,
            Some([1000.0, 500.0]),
        );
        assert_eq!(cells(&c), vec![(40, 40), (120, 80)]);
        assert_eq!((c.width, c.height), (400, 200));
        // Off the left edge: shifted to start at 0.
        let c = grid_cells(&points(&[[-50.0, 10.0], [50.0, 10.0]]), 400, Some([100.0, 100.0]));
        assert_eq!(cells(&c), vec![(0, 10), (100, 10)]);
        assert_eq!((c.width, c.height), (101, 100));
    }

    #[test]
    fn per_preview_scales_a_group_by_its_span() {
        // 0-1000 across, grid 400: factor 2.5, cells rounded.
        let c = preview_cells(&points(&[[0.0, 0.0], [501.0, 100.0], [1000.0, 0.0]]), Some(400));
        assert_eq!(cells(&c), vec![(0, 0), (200, 40), (400, 0)]);
        assert_eq!((c.width, c.height), (401, 41));
    }

    #[test]
    fn per_preview_spreads_out_a_dense_prop() {
        // 10 pixels over 9 units (and 0.01 high, for a flat line): dense, so spread out to
        // sqrt(10 * 5 * 9 / 0.01) = 212.1 cells across, every pixel 23.6 cells apart.
        let xy: Vec<[f32; 2]> = (0..10).map(|i| [i as f32, 0.0]).collect();
        let c = preview_cells(&points(&xy), None);
        assert_eq!(c.at[1], Some((24, 0)));
        assert_eq!(c.at[9], Some((212, 0)));
        assert_eq!((c.width, c.height), (213, 1));
    }

    /// Two lines in a group, at known places in xLights units: A, 5 pixels from (-200, 0) to
    /// (200, 0), then B, 3 pixels from (-100, 200) to (100, 200).
    fn two_lines(layout: GroupLayout, grid_size: u32) -> (SceneGeometry, Show) {
        let line = |name: &str, nodes: u32, length: f32, y: f32| {
            let mut prop = Prop::new(name, ShapeSource::Generator(Generator::Line { nodes, length }));
            prop.transform = Transform {
                position: Vec3::new(0.0, y, 0.0),
                ..Transform::default()
            };
            prop
        };
        let mut show = Show::new("t");
        show.props = vec![line("A", 5, 4.0, 0.0), line("B", 3, 2.0, 2.0)];
        // A 1000 x 500 layout.
        show.layout_area = Some(LayoutArea {
            width: 10.0,
            height: 5.0,
        });
        let mut group = Group::new("G");
        group.members = vec![show.props[0].id.into(), show.props[1].id.into()];
        group.layout = layout;
        group.grid_size = grid_size;
        show.groups.push(group);
        (SceneGeometry::new(&show, &pf_mapping::map_show(&show).0), show)
    }

    fn group_of(show: &Show) -> Target {
        Target::Group(show.groups[0].id)
    }

    /// A buffer's size and each pixel's cell.
    fn laid_out(buffer: &PixelBuffer) -> ((u32, u32), Vec<(i64, i64)>) {
        let at = buffer
            .pixels
            .iter()
            .map(|px| cell_of(px, buffer.columns, buffer.rows))
            .collect();
        ((buffer.columns, buffer.rows), at)
    }

    fn styled(layout: GroupLayout, grid_size: u32, style: RenderStyle) -> ((u32, u32), Vec<(i64, i64)>) {
        let (geo, show) = two_lines(layout, grid_size);
        laid_out(&geo.styled_buffer(group_of(&show), style, BufferTransform::None))
    }

    /// A's five cells then B's three: A's along the bottom row, B's along row `y`.
    fn rows(a: &[i64], b: &[i64], y: i64) -> Vec<(i64, i64)> {
        a.iter()
            .map(|&x| (x, 0))
            .chain(b.iter().map(|&x| (x, y)))
            .collect()
    }

    #[test]
    fn group_grids_match_xlights() {
        use GroupLayout::{Grid, MinimalGrid};
        let default = RenderStyle::Default;
        // 401 x 201 units: under a 1000-cell grid, one cell per unit.
        assert_eq!(
            styled(MinimalGrid, 1000, default),
            ((401, 201), rows(&[0, 100, 200, 300, 400], &[100, 200, 300], 200))
        );
        // Over 400: scaled by 400 / 401, positions truncated (99.75 is cell 99).
        assert_eq!(
            styled(MinimalGrid, 400, default),
            ((400, 200), rows(&[0, 99, 199, 299, 399], &[99, 199, 299], 199))
        );
        assert_eq!(
            styled(MinimalGrid, 100, default),
            ((100, 50), rows(&[0, 24, 49, 74, 99], &[24, 49, 74], 49))
        );
        // The whole 1000 x 500 layout from the origin (after shifting A's left end to 0): scaled
        // by 400 / 1000.
        assert_eq!(
            styled(Grid, 400, default),
            ((400, 200), rows(&[0, 40, 80, 120, 160], &[40, 80, 120], 80))
        );
        // Per preview: scaled by the span over the grid size (400 / 100), positions rounded.
        assert_eq!(
            styled(MinimalGrid, 100, RenderStyle::PerPreview),
            ((101, 51), rows(&[0, 25, 50, 75, 100], &[25, 50, 75], 50))
        );
        assert_eq!(
            styled(MinimalGrid, 400, RenderStyle::PerPreview),
            ((401, 201), rows(&[0, 100, 200, 300, 400], &[100, 200, 300], 200))
        );
    }

    #[test]
    fn group_styles_lay_members_out_as_xlights_does() {
        use RenderStyle::*;
        let layout = |style| styled(GroupLayout::MinimalGrid, 400, style);
        let each = |a: fn(i64) -> (i64, i64), b: fn(i64) -> (i64, i64)| -> Vec<(i64, i64)> {
            (0..5).map(a).chain((0..3).map(b)).collect()
        };
        assert_eq!(layout(SingleLine), ((8, 1), (0..8).map(|x| (x, 0)).collect()));
        assert_eq!(layout(AsPixel), ((1, 1), vec![(0, 0); 8]));
        // A is the first column (or row), B the second, a cell per pixel from the start.
        assert_eq!(layout(HorizontalPerModel), ((2, 5), each(|k| (0, k), |k| (1, k))));
        assert_eq!(layout(VerticalPerModel), ((5, 2), each(|k| (k, 0), |k| (k, 1))));
        // The members' own buffers (5 x 1 and 3 x 1) side by side, or one above the other.
        assert_eq!(
            layout(HorizontalStack),
            ((8, 1), each(|k| (k, 0), |k| (5 + k, 0)))
        );
        assert_eq!(layout(VerticalStack), ((5, 2), each(|k| (k, 0), |k| (k, 1))));
        // Scaled: each member gets 5 cells; B's stretch by 5 / 3 (and truncate).
        assert_eq!(
            layout(HorizontalStackScaled),
            ((10, 1), rows(&[0, 1, 2, 3, 4], &[5, 6, 8], 0))
        );
        assert_eq!(
            layout(VerticalStackScaled),
            ((5, 2), rows(&[0, 1, 2, 3, 4], &[0, 1, 3], 1))
        );
        // On top of A's 5 x 1: B centered a cell in, or stretched.
        assert_eq!(
            layout(OverlayCentered),
            ((5, 1), rows(&[0, 1, 2, 3, 4], &[1, 2, 3], 0))
        );
        assert_eq!(
            layout(OverlayScaled),
            ((5, 1), rows(&[0, 1, 2, 3, 4], &[0, 1, 3], 0))
        );
        assert_eq!(
            layout(SingleLineModelAsPixel),
            ((2, 1), each(|_| (0, 0), |_| (1, 0)))
        );
        // Per preview (401 x 201 here), each member in the cell at its middle.
        assert_eq!(
            layout(DefaultModelAsPixel),
            ((401, 201), each(|_| (200, 0), |_| (200, 200)))
        );
    }

    #[test]
    fn per_model_styles_draw_each_member_on_its_own() {
        let (geo, show) = two_lines(GroupLayout::MinimalGrid, 400);
        let parts = |style| {
            let buffer = geo.styled_buffer(group_of(&show), style, BufferTransform::None);
            assert_eq!(buffer.len(), 8);
            buffer
                .parts
                .iter()
                .map(|p| (laid_out(&p.buffer).0, p.slots.clone()))
                .collect::<Vec<_>>()
        };
        let (a, b) = (vec![0, 1, 2, 3, 4], vec![5, 6, 7]);
        assert_eq!(
            parts(RenderStyle::PerModelDefault),
            vec![((5, 1), a.clone()), ((3, 1), b.clone())]
        );
        assert_eq!(
            parts(RenderStyle::PerModelSingleLine),
            vec![((5, 1), a.clone()), ((3, 1), b.clone())]
        );
        // Each line on its own per preview: its pixels close together, so spread out to 400 across.
        assert_eq!(
            parts(RenderStyle::PerModelPerPreview),
            vec![((401, 1), a), ((401, 1), b)]
        );
        // A group laid out per model draws that way by default.
        let (geo, show) = two_lines(GroupLayout::PerModelDefault, 400);
        assert_eq!(geo.buffer(group_of(&show)).parts.len(), 2);
        let line = geo.styled_buffer(group_of(&show), RenderStyle::SingleLine, BufferTransform::None);
        assert!(line.parts.is_empty());
    }

    #[test]
    fn a_group_layout_is_its_default_style_and_props_ignore_group_styles() {
        let (geo, show) = two_lines(GroupLayout::HorizontalPerModel, 400);
        assert_eq!(laid_out(&geo.buffer(group_of(&show))).0, (2, 5));
        let a = Target::Prop(show.props[0].id);
        let own = laid_out(&geo.buffer(a));
        assert_eq!(own.0, (5, 1));
        for style in [
            RenderStyle::HorizontalPerModel,
            RenderStyle::OverlayScaled,
            RenderStyle::PerModelDefault,
        ] {
            assert_eq!(laid_out(&geo.styled_buffer(a, style, BufferTransform::None)), own);
        }
        let line = geo.styled_buffer(a, RenderStyle::PerModelSingleLine, BufferTransform::None);
        assert_eq!(laid_out(&line).0, (5, 1));
    }

    #[test]
    fn transforms_turn_the_grid() {
        let (geo, show) = two_lines(GroupLayout::MinimalGrid, 400);
        let turned = geo.styled_buffer(group_of(&show), RenderStyle::Default, BufferTransform::RotateCw90);
        let (size, at) = laid_out(&turned);
        // 400 x 200 turns to 200 x 400; the cell at (x, y) goes to (199 - y, x).
        assert_eq!(size, (200, 400));
        assert_eq!(at[1], (199, 99));
        assert_eq!(at[5], (0, 99));
        let flipped = geo.styled_buffer(
            group_of(&show),
            RenderStyle::SingleLine,
            BufferTransform::FlipHorizontal,
        );
        assert_eq!(laid_out(&flipped).1[0], (7, 0));
    }
}
