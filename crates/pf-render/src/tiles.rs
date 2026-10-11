//! Tiles: the show's pixels cut into runs that are each drawn on their own.
//!
//! Everything a frame does to a pixel after its effects are made ready (starting a row, mixing its
//! layers, laying the row over the rows before, writing the bytes) reads and writes that pixel
//! alone. So the show's pixels can be cut into tiles and each tile drawn by whichever thread is
//! free: a pixel goes through the same steps in the same order whichever thread draws its tile.
//!
//! A tile is a run of show pixels: a large prop is cut into several, and small props that follow
//! one another share one. A target's buffer lists its pixels in its own order, so each target
//! keeps a [`Spread`]: for each tile, the runs of its buffer that fall there.

use crate::geometry::{PixelBuffer, SceneGeometry};
use std::ops::Range;

/// Most pixels in a tile. Small enough that a tile's row stays in the processor's nearest cache
/// through all its layers, and that a large show has many more tiles than threads; large enough
/// that a tile is worth handing to a thread.
pub(crate) const TILE_PIXELS: usize = 2048;

/// A run of show pixels drawn as one piece of work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tile {
    /// Its first pixel in the show-wide pixel list, and how many it has.
    pub first: usize,
    pub len: usize,
    /// The props its pixels belong to (places in [`SceneGeometry`]'s props).
    pub props: Range<usize>,
    /// Where its pixels' bytes start in the show frame (see [`Tiles::in_frame_order`]).
    pub bytes: usize,
}

/// The show's tiles, in pixel order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tiles {
    pub tiles: Vec<Tile>,
    /// Whether the props' bytes follow one another in the show frame as their pixels do in the
    /// pixel list, so each tile's bytes are a piece of the frame of its own. A channel map lays
    /// props out that way; for one that doesn't, the frame is written in one go instead.
    pub in_frame_order: bool,
}

impl Tiles {
    pub fn new(geometry: &SceneGeometry) -> Self {
        Self::of(geometry, TILE_PIXELS)
    }

    /// The show cut into tiles of at most `most` pixels.
    pub fn of(geometry: &SceneGeometry, most: usize) -> Self {
        let most = most.max(1);
        let mut tiles: Vec<Tile> = Vec::new();
        // The tile small props are being gathered into.
        let mut open: Option<Tile> = None;
        let mut in_frame_order = true;
        // Where the bytes written so far end.
        let mut written = 0;
        for (i, prop) in geometry.props.iter().enumerate() {
            let count = prop.points.len();
            if count == 0 {
                continue;
            }
            let cpp = usize::from(prop.channels_per_pixel);
            // Props narrower than a color aren't written (see `Renderer::write`).
            let bytes = if cpp >= 3 {
                in_frame_order &= prop.frame_offset >= written;
                written = written.max(prop.frame_offset + count * cpp);
                prop.frame_offset
            } else {
                written
            };
            if count > most {
                tiles.extend(open.take());
                // Even pieces, so none is a sliver.
                let pieces = count.div_ceil(most);
                for piece in 0..pieces {
                    let (from, to) = (piece * count / pieces, (piece + 1) * count / pieces);
                    tiles.push(Tile {
                        first: prop.first_pixel + from,
                        len: to - from,
                        props: i..i + 1,
                        bytes: bytes + if cpp >= 3 { from * cpp } else { 0 },
                    });
                }
                continue;
            }
            match &mut open {
                Some(tile) if tile.len + count <= most => {
                    tile.len += count;
                    tile.props.end = i + 1;
                }
                _ => {
                    tiles.extend(open.take());
                    open = Some(Tile {
                        first: prop.first_pixel,
                        len: count,
                        props: i..i + 1,
                        bytes,
                    });
                }
            }
        }
        tiles.extend(open);
        Self {
            tiles,
            in_frame_order,
        }
    }

    /// The tile show pixel `pixel` is in.
    fn of_pixel(&self, pixel: u32) -> Option<u32> {
        let pixel = pixel as usize;
        let after = self.tiles.partition_point(|tile| tile.first <= pixel);
        let tile = after.checked_sub(1)?;
        (pixel < self.tiles[tile].first + self.tiles[tile].len).then_some(tile as u32)
    }
}

/// The part of a target's buffer that falls in one tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InTile {
    pub tile: u32,
    /// Its runs, as places in [`Spread`]'s runs.
    pub runs: (u32, u32),
    /// How many of the buffer's pixels they hold.
    pub pixels: u32,
}

/// Where a target's buffer falls among the tiles. A row is drawn tile by tile on the pixels its
/// buffer has there, taken in buffer order: its part of a tile is those pixels side by side.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Spread {
    /// The tiles the buffer has pixels in, in tile order.
    pub tiles: Vec<InTile>,
    /// Runs of the buffer (first place, and one past the last) whose pixels share a tile.
    pub runs: Vec<(u32, u32)>,
}

impl Spread {
    /// The spread of a buffer listing the show pixels `global`.
    pub fn new(global: &[u32], tiles: &Tiles) -> Self {
        // Runs of places in the same tile, then tile by tile (a stable sort: buffer order within
        // each tile).
        let mut found: Vec<(u32, u32, u32)> = Vec::new();
        for (place, &pixel) in global.iter().enumerate() {
            // A pixel outside the show is in no tile, and never drawn.
            let Some(tile) = tiles.of_pixel(pixel) else {
                continue;
            };
            let place = place as u32;
            match found.last_mut() {
                Some((last, _, end)) if *last == tile && *end == place => *end = place + 1,
                _ => found.push((tile, place, place + 1)),
            }
        }
        found.sort_by_key(|&(tile, ..)| tile);
        let mut spread = Self::default();
        for (n, &(tile, from, to)) in found.iter().enumerate() {
            spread.runs.push((from, to));
            match spread.tiles.last_mut() {
                Some(last) if last.tile == tile => {
                    last.runs.1 = n as u32 + 1;
                    last.pixels += to - from;
                }
                _ => spread.tiles.push(InTile {
                    tile,
                    runs: (n as u32, n as u32 + 1),
                    pixels: to - from,
                }),
            }
        }
        spread
    }

    /// One tile's runs.
    pub fn runs(&self, of: (u32, u32)) -> &[(u32, u32)] {
        &self.runs[of.0 as usize..of.1 as usize]
    }

    /// For each of the buffer's `len` places: where it is in its tile's part of the buffer
    /// (`u32::MAX`: in no tile).
    fn places(&self, len: usize) -> Vec<u32> {
        let mut places = vec![u32::MAX; len];
        for tile in &self.tiles {
            let mut at = 0;
            for &(from, to) in self.runs(tile.runs) {
                for place in from..to {
                    places[place as usize] = at;
                    at += 1;
                }
            }
        }
        places
    }
}

/// One member of a per-model buffer in one tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PartInTile {
    /// The member (its place in the buffer's parts).
    pub part: u32,
    /// Its runs there, as places in [`PartSpread`]'s runs.
    pub runs: (u32, u32),
}

/// Where the members of a per-model buffer fall among the tiles.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PartSpread {
    /// The tiles any member has pixels in, in tile order, each with its members (places in
    /// `members`).
    tiles: Vec<(u32, u32, u32)>,
    /// A tile's members, in member order.
    members: Vec<PartInTile>,
    /// Runs of a member's own pixels (first, and one past the last) that share a tile.
    runs: Vec<(u32, u32)>,
    /// For each member, each of its pixels' place in its tile's part of the group's buffer
    /// (see [`Spread`]).
    pub at: Vec<Vec<u32>>,
}

impl PartSpread {
    /// The spread of `buffer`'s parts, `spread` being the spread of the target's own buffer
    /// (which lists the same pixels in the same order).
    pub fn new(buffer: &PixelBuffer, spread: &Spread, tiles: &Tiles) -> Self {
        let places = spread.places(buffer.global.len());
        let mut found: Vec<(u32, u32, u32, u32)> = Vec::new();
        let mut at = Vec::with_capacity(buffer.parts.len());
        for (part, member) in buffer.parts.iter().enumerate() {
            let count = member.slots.len().min(member.buffer.pixels.len());
            let mut own = vec![u32::MAX; count];
            for (k, &slot) in member.slots[..count].iter().enumerate() {
                let place = places.get(slot as usize).copied().unwrap_or(u32::MAX);
                let tile = buffer
                    .global
                    .get(slot as usize)
                    .and_then(|&pixel| tiles.of_pixel(pixel));
                let (Some(tile), true) = (tile, place != u32::MAX) else {
                    continue;
                };
                own[k] = place;
                let (part, k) = (part as u32, k as u32);
                match found.last_mut() {
                    Some((t, p, _, end)) if *t == tile && *p == part && *end == k => *end = k + 1,
                    _ => found.push((tile, part, k, k + 1)),
                }
            }
            at.push(own);
        }
        // Tile by tile, each tile's members in order and each member's runs in order.
        found.sort_by_key(|&(tile, ..)| tile);
        let mut out = Self {
            at,
            ..Self::default()
        };
        for (n, &(tile, part, from, to)) in found.iter().enumerate() {
            let n = n as u32;
            out.runs.push((from, to));
            let new_tile = out.tiles.last().is_none_or(|last| last.0 != tile);
            if new_tile {
                let members = out.members.len() as u32;
                out.tiles.push((tile, members, members));
            }
            match out.members.last_mut() {
                Some(last) if !new_tile && last.part == part => last.runs.1 = n + 1,
                _ => {
                    out.members.push(PartInTile {
                        part,
                        runs: (n, n + 1),
                    });
                    if let Some(last) = out.tiles.last_mut() {
                        last.2 += 1;
                    }
                }
            }
        }
        out
    }

    /// The members with pixels in `tile`, in member order.
    pub fn in_tile(&self, tile: u32) -> &[PartInTile] {
        match self.tiles.binary_search_by_key(&tile, |&(t, ..)| t) {
            Ok(found) => {
                let (_, from, to) = self.tiles[found];
                &self.members[from as usize..to as usize]
            }
            Err(_) => &[],
        }
    }

    /// One member's runs in a tile.
    pub fn runs(&self, of: (u32, u32)) -> &[(u32, u32)] {
        &self.runs[of.0 as usize..of.1 as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::{Generator, Group, Prop, RenderStyle, ShapeSource, Show, Transform, Vec3};
    use pf_sequence::Target;

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

    /// Lines of these lengths side by side, in a group that lists them last to first.
    fn scene(lengths: &[u32]) -> (SceneGeometry, Show) {
        let mut show = Show::new("t");
        for (i, &nodes) in lengths.iter().enumerate() {
            show.props.push(line(&format!("P{i}"), nodes, i as f32 * 2.0));
        }
        let mut group = Group::new("All");
        group.members = show.props.iter().rev().map(|p| p.id.into()).collect();
        show.groups.push(group);
        let (map, _) = pf_mapping::map_show(&show);
        (SceneGeometry::new(&show, &map), show)
    }

    #[test]
    fn tiles_cover_every_pixel_once_and_gather_small_props() {
        let (geometry, _) = scene(&[3, 4, 25, 2, 2, 9]);
        let tiles = Tiles::of(&geometry, 10);
        assert!(tiles.in_frame_order);
        let cut: Vec<(usize, usize, Range<usize>)> = tiles
            .tiles
            .iter()
            .map(|t| (t.first, t.len, t.props.clone()))
            .collect();
        // 3 + 4 share a tile; 25 is cut in three; 2 + 2 share one, which 9 more wouldn't fit.
        assert_eq!(
            cut,
            vec![
                (0, 7, 0..2),
                (7, 8, 2..3),
                (15, 8, 2..3),
                (23, 9, 2..3),
                (32, 4, 3..5),
                (36, 9, 5..6),
            ]
        );
        // Each tile's bytes start where its first pixel's do (three bytes a pixel here).
        assert!(tiles.tiles.iter().all(|t| t.bytes == t.first * 3));
        for pixel in 0..45 {
            let tile = &tiles.tiles[tiles.of_pixel(pixel).unwrap() as usize];
            assert!((tile.first..tile.first + tile.len).contains(&(pixel as usize)));
        }
        assert_eq!(tiles.of_pixel(45), None);
    }

    #[test]
    fn props_out_of_order_in_the_frame_are_noticed() {
        let (mut geometry, _) = scene(&[4, 4]);
        assert!(Tiles::new(&geometry).in_frame_order);
        geometry.props[0].frame_offset = 12;
        geometry.props[1].frame_offset = 0;
        assert!(!Tiles::new(&geometry).in_frame_order);
    }

    #[test]
    fn a_spread_lists_each_tiles_part_of_a_buffer_in_buffer_order() {
        let (geometry, show) = scene(&[3, 4, 25]);
        let tiles = Tiles::of(&geometry, 10);
        // The group lists the 25, then the 4, then the 3: its buffer runs against the tiles.
        let buffer = geometry.buffer(Target::Group(show.groups[0].id));
        let spread = Spread::new(&buffer.global, &tiles);
        let part = |t: &InTile| (t.tile, spread.runs(t.runs), t.pixels);
        assert_eq!(
            spread.tiles.iter().map(part).collect::<Vec<_>>(),
            vec![
                (0, &[(25, 32)][..], 7),
                (1, &[(0, 8)][..], 8),
                (2, &[(8, 16)][..], 8),
                (3, &[(16, 25)][..], 9),
            ]
        );
        // Every place is in one tile's part, counted from that part's start.
        let places = spread.places(buffer.global.len());
        assert_eq!(places[25..], [0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(places[8..16], [0, 1, 2, 3, 4, 5, 6, 7]);
        // A pixel outside the show is left out.
        let spread = Spread::new(&[0, 1, 999, 2], &tiles);
        assert_eq!(spread.runs, vec![(0, 2), (3, 4)]);
        assert_eq!(spread.tiles[0].pixels, 3);
        assert_eq!(spread.places(4), [0, 1, u32::MAX, 2]);
    }

    #[test]
    fn a_per_model_buffers_members_are_found_tile_by_tile() {
        let (geometry, show) = scene(&[3, 4, 25]);
        let tiles = Tiles::of(&geometry, 10);
        let target = Target::Group(show.groups[0].id);
        let spread = Spread::new(&geometry.buffer(target).global, &tiles);
        let buffer = geometry.styled_buffer(target, RenderStyle::PerModelDefault, Default::default());
        assert_eq!(buffer.parts.len(), 3);
        let parts = PartSpread::new(&buffer, &spread, &tiles);
        let in_tile = |tile: u32| -> Vec<(u32, Vec<(u32, u32)>)> {
            parts
                .in_tile(tile)
                .iter()
                .map(|m| (m.part, parts.runs(m.runs).to_vec()))
                .collect()
        };
        // The first tile holds the two small props: the group's second and third members.
        assert_eq!(in_tile(0), vec![(1, vec![(0, 4)]), (2, vec![(0, 3)])]);
        assert_eq!(in_tile(1), vec![(0, vec![(0, 8)])]);
        assert_eq!(in_tile(3), vec![(0, vec![(16, 25)])]);
        assert!(in_tile(4).is_empty());
        // In the tile's part of the group's buffer the 4-pixel prop comes first, as the group
        // lists it, though the 3-pixel one comes first in the show.
        assert_eq!(parts.at[1], [0, 1, 2, 3]);
        assert_eq!(parts.at[2], [4, 5, 6]);
        assert_eq!(parts.at[0][8..16], [0, 1, 2, 3, 4, 5, 6, 7]);
    }
}
