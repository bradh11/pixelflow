//! Synthetic shows: a believable mix of props, groups, and controllers at any pixel count, built in
//! code so nothing large is committed and every run of a tier gets the same show.
//!
//! A show is laid out the way real displays grow: a house's worth of outlines, arches, trees, and
//! small props, and the rest of the pixels in matrices (large displays are mostly panel walls).
//! Props are wired to string controllers (16 ports of up to 600 pixels) and to matrix controllers
//! of their own, over a mix of sACN and DDP. Every controller's address is the loopback address:
//! the benchmark never sends anywhere else (see [`crate::sink`]).

use pf_model::{
    ColorOrder, Controller, ControllerId, Generator, Group, GroupId, GroupLayout, GroupMember, MatrixWiring,
    NodeRange, PolySegment, Port, PortSlot, Prop, PropId, Protocol, SacnConfig, ShapeSource, Show, TreeStyle,
    Vec3,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Most pixels on one port of a string controller (about what a port drives at 40 frames a second).
pub const PORT_PIXELS: u32 = 600;
/// Ports on a string controller.
pub const PORTS_PER_CONTROLLER: u16 = 16;
/// A matrix this large or larger gets a controller of its own (so does a show's largest).
const OWN_CONTROLLER_PIXELS: u32 = 4_000;
/// A matrix this large or larger is a panel wall: always DDP.
const PANEL_WALL_PIXELS: u32 = 16_000;
/// Where every synthetic controller lives. The runner gives each its own port there.
pub const LOOPBACK: &str = "127.0.0.1";

/// The named tiers.
pub const TIERS: [u32; 3] = [20_000, 100_000, 200_000];

/// What kind of prop this is: it decides the prop's shape, where it stands, and which effects a
/// synthetic sequence plays on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    /// Roof lines, eaves, and window frames.
    Outline,
    Arch,
    Tree,
    /// Stars, circles, and candy canes.
    Accent,
    Matrix,
}

impl Category {
    pub const ALL: [Category; 5] = [
        Category::Outline,
        Category::Arch,
        Category::Tree,
        Category::Accent,
        Category::Matrix,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Outline => "Outlines and lines",
            Category::Arch => "Arches",
            Category::Tree => "Trees",
            Category::Accent => "Stars, circles, canes",
            Category::Matrix => "Matrices",
        }
    }
}

/// One prop of a synthetic show.
#[derive(Debug, Clone, PartialEq)]
pub struct PropInfo {
    pub id: PropId,
    pub category: Category,
    pub pixels: u32,
}

/// Which props a synthetic group holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GroupKind {
    WholeHouse,
    Of(Category),
}

/// One group of a synthetic show.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupInfo {
    pub id: GroupId,
    pub kind: GroupKind,
    pub pixels: u32,
}

/// A synthetic show and what it's made of.
#[derive(Debug, Clone)]
pub struct SyntheticShow {
    pub show: Show,
    /// The show's props, in show order.
    pub props: Vec<PropInfo>,
    pub groups: Vec<GroupInfo>,
}

/// What a synthetic show is made of, for reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Composition {
    pub pixels: u32,
    pub props: usize,
    pub categories: Vec<CategoryCount>,
    /// The matrices by size, largest first.
    pub matrices: Vec<MatrixCount>,
    pub groups: Vec<GroupCount>,
    pub controllers: usize,
    pub sacn_controllers: usize,
    pub sacn_pixels: u32,
    pub ddp_controllers: usize,
    pub ddp_pixels: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryCount {
    pub category: Category,
    pub props: usize,
    pub pixels: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatrixCount {
    pub columns: u32,
    pub rows: u32,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupCount {
    pub name: String,
    pub layout: String,
    pub members: usize,
    pub pixels: u32,
}

/// An id that is the same every time: `kind` tells props from groups and so on, `n` counts.
pub(crate) fn fixed_id(kind: u8, n: u64) -> Uuid {
    Uuid::from_u128((0x7066_6265_6e63_6800_u128 << 64) | (u128::from(kind) << 56) | u128::from(n))
}

/// How a show's pixels are shared out: each kind's share, the matrices taking the rest.
struct Mix {
    outline: f32,
    arch: f32,
    tree: f32,
    accent: f32,
}

/// A house only has so many roof lines, so the larger the show, the more of it is matrix.
fn mix(pixels: u32) -> Mix {
    if pixels <= 50_000 {
        Mix {
            outline: 0.22,
            arch: 0.05,
            tree: 0.25,
            accent: 0.06,
        }
    } else if pixels <= 150_000 {
        Mix {
            outline: 0.10,
            arch: 0.03,
            tree: 0.17,
            accent: 0.03,
        }
    } else {
        Mix {
            outline: 0.06,
            arch: 0.02,
            tree: 0.12,
            accent: 0.02,
        }
    }
}

/// A prop on its way into the show: its shape, and how much room it takes in the layout.
struct Piece {
    name: String,
    category: Category,
    shape: Generator,
    width: f32,
    height: f32,
    /// The shape's origin is its middle (else the middle of its base).
    centered: bool,
}

#[derive(Default)]
struct Pieces {
    list: Vec<Piece>,
    /// How many of each kind of thing there are so far, for their names.
    named: Vec<(&'static str, u32)>,
}

impl Pieces {
    fn push(
        &mut self,
        category: Category,
        what: &'static str,
        shape: Generator,
        size: (f32, f32),
        centered: bool,
    ) {
        let n = match self.named.iter_mut().find(|(name, _)| *name == what) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                self.named.push((what, 1));
                1
            }
        };
        self.list.push(Piece {
            name: format!("{what} {n}"),
            category,
            shape,
            width: size.0,
            height: size.1,
            centered,
        });
    }
}

/// A straight run of pixels 30 mm apart.
fn line(nodes: u32) -> (Generator, (f32, f32)) {
    let length = nodes as f32 * 0.03;
    (Generator::Line { nodes, length }, (length, 0.3))
}

/// Roof lines, peaked eaves, and window frames, in turn, up to `budget` pixels.
fn outlines(pieces: &mut Pieces, budget: u32) -> u32 {
    let mut used = 0;
    for turn in 0u32.. {
        let left = budget - used;
        match turn % 4 {
            0 | 2 if left >= 200 => {
                let (shape, size) = line(if turn % 4 == 0 { 200 } else { 150 });
                used += shape.node_count();
                pieces.push(Category::Outline, "Roof line", shape, size, true);
            }
            1 if left >= 200 => {
                let shape = Generator::PolyLine {
                    vertices: vec![
                        Vec3::new(-2.5, 0.0, 0.0),
                        Vec3::new(0.0, 1.5, 0.0),
                        Vec3::new(2.5, 0.0, 0.0),
                    ],
                    segments: vec![PolySegment::straight(100), PolySegment::straight(100)],
                    spread_nodes: None,
                };
                used += 200;
                pieces.push(Category::Outline, "Eave", shape, (5.0, 1.5), false);
            }
            3 if left >= 120 => {
                let shape = Generator::WindowFrame {
                    top: 30,
                    sides: 30,
                    bottom: 30,
                    width: 1.2,
                    height: 1.5,
                    start: Default::default(),
                    counter_clockwise: false,
                };
                used += 120;
                pieces.push(Category::Outline, "Window", shape, (1.2, 1.5), true);
            }
            _ => break,
        }
    }
    used
}

/// Rows of four 50-pixel arches.
fn arches(pieces: &mut Pieces, budget: u32) -> u32 {
    let count = budget / 200;
    for _ in 0..count {
        let Generator::Arch {
            nodes,
            width,
            height,
            arc,
            skew_deg,
            start_right,
            layers,
            hollow,
            zig_zag,
            start_inside,
            ..
        } = Generator::arch(50, 1.2, 0.6)
        else {
            unreachable!("an arch")
        };
        let shape = Generator::Arch {
            nodes,
            width,
            height,
            arches: 4,
            arc,
            gap: 0.1,
            skew_deg,
            start_right,
            layers,
            hollow,
            zig_zag,
            start_inside,
        };
        pieces.push(Category::Arch, "Arches", shape, (5.1, 0.6), false);
    }
    count * 200
}

/// Mega trees for most of the budget, then medium round trees and flat mini trees in turn.
fn trees(pieces: &mut Pieces, budget: u32, large: bool) -> u32 {
    let (strings, nodes) = if large { (48, 150) } else { (32, 100) };
    let mega = strings * nodes;
    // At least one mega tree when it leaves room for the others.
    let megas = (budget * 6 / 10 / mega).max(u32::from(mega * 10 <= budget * 7));
    let mut used = 0;
    for _ in 0..megas {
        let shape = Generator::tree(strings, nodes, 6.0, 2.0, 0.1, TreeStyle::Round);
        pieces.push(Category::Tree, "Mega tree", shape, (4.0, 6.0), false);
        used += mega;
    }
    for turn in 0u32.. {
        let left = budget - used;
        if turn.is_multiple_of(2) && left >= 800 {
            let shape = Generator::tree(16, 50, 2.5, 0.8, 0.05, TreeStyle::Round);
            pieces.push(Category::Tree, "Tree", shape, (1.6, 2.5), false);
            used += 800;
        } else if left >= 200 {
            let shape = Generator::tree(8, 25, 1.0, 0.4, 0.02, TreeStyle::Flat);
            pieces.push(Category::Tree, "Mini tree", shape, (0.8, 1.0), false);
            used += 200;
        } else {
            break;
        }
    }
    used
}

/// Stars, circles, and rows of candy canes, in turn.
fn accents(pieces: &mut Pieces, budget: u32) -> u32 {
    let mut used = 0;
    for turn in 0u32.. {
        let left = budget - used;
        match turn % 3 {
            0 if left >= 150 => {
                let shape = Generator::star(5, 150, 0.5, 0.2);
                pieces.push(Category::Accent, "Star", shape, (1.0, 1.0), true);
                used += 150;
            }
            1 if left >= 100 => {
                let shape = Generator::circle(100, 0.4);
                pieces.push(Category::Accent, "Circle", shape, (0.8, 0.8), true);
                used += 100;
            }
            2 if left >= 180 => {
                let shape = Generator::CandyCanes {
                    canes: 6,
                    nodes_per_cane: 30,
                    width: 2.4,
                    height: 1.0,
                    cane_height: 1.0,
                    reverse: false,
                    sticks: false,
                    alternate_nodes: false,
                    skew_deg: 0.0,
                    start_right: false,
                };
                pieces.push(Category::Accent, "Candy canes", shape, (2.4, 1.0), false);
                used += 180;
            }
            _ => break,
        }
    }
    used
}

/// Matrix sizes, largest first, and the most of each a show has.
const MATRIX_SIZES: [(u32, u32, u32); 6] = [
    (256, 128, 2),
    (192, 96, 2),
    (128, 64, 4),
    (96, 48, 4),
    (64, 32, 4),
    (48, 16, 8),
];

/// Matrices of several sizes: each size takes up to six tenths of what the larger ones left, and
/// the smallest whatever it can of the rest.
fn matrices(pieces: &mut Pieces, budget: u32) -> u32 {
    let mut used = 0;
    for (i, (columns, rows, most)) in MATRIX_SIZES.into_iter().enumerate() {
        let pixels = columns * rows;
        let left = budget - used;
        let share = if i + 1 == MATRIX_SIZES.len() {
            left
        } else {
            (u64::from(left) * 6 / 10) as u32
        };
        for _ in 0..most.min(share / pixels) {
            // Panels are 10 mm apart; pixel-node matrices 25 mm.
            let pitch = if columns >= 128 { 0.01 } else { 0.025 };
            let size = (columns as f32 * pitch, rows as f32 * pitch);
            let shape = Generator::Matrix {
                columns,
                rows,
                width: size.0,
                height: size.1,
                wiring: MatrixWiring::default(),
            };
            pieces.push(Category::Matrix, "Matrix", shape, size, true);
            used += pixels;
        }
    }
    used
}

/// Stands the pieces in the layout: each kind in a band of its own, the yard at the bottom and
/// the roof lines at the top, packed left to right in rows.
fn place(pieces: &[Piece], pixels: u32) -> Vec<Vec3> {
    let scene_width = 18.0 + 12.0 * (pixels as f32 / 20_000.0).sqrt();
    let mut positions = vec![Vec3::ZERO; pieces.len()];
    let mut band_y = 0.0f32;
    let bands = [
        Category::Tree,
        Category::Arch,
        Category::Accent,
        Category::Matrix,
        Category::Outline,
    ];
    for band in bands {
        let (mut x, mut y, mut row_height) = (0.0f32, band_y, 0.0f32);
        for (i, piece) in pieces.iter().enumerate().filter(|(_, p)| p.category == band) {
            if x > 0.0 && x + piece.width > scene_width {
                x = 0.0;
                y += row_height + 0.4;
                row_height = 0.0;
            }
            let base = if piece.centered { y + piece.height / 2.0 } else { y };
            positions[i] = Vec3::new(x + piece.width / 2.0, base, 0.0);
            x += piece.width + 0.4;
            row_height = row_height.max(piece.height);
        }
        band_y = y + row_height + 0.8;
    }
    positions
}

/// String controllers being filled port by port.
struct Strings {
    controllers: Vec<Controller>,
    /// Room left on the port being filled (none: the next pixel starts a new port).
    room: u32,
    slots: u32,
}

impl Strings {
    /// Once the port being filled is full, starts a new one, on a new controller when the last
    /// one has all its ports.
    fn make_room(&mut self) {
        if self.room > 0 {
            return;
        }
        if self
            .controllers
            .last()
            .is_none_or(|c| c.ports.len() == usize::from(PORTS_PER_CONTROLLER))
        {
            let n = self.controllers.len();
            // Two sACN controllers for every DDP one.
            let (protocol, kind) = if n % 3 == 2 {
                (Protocol::Ddp, "DDP")
            } else {
                (Protocol::Sacn(SacnConfig::default()), "sACN")
            };
            let mut controller = Controller::new(format!("Strings {} ({kind})", n + 1), LOOPBACK, protocol);
            controller.id = ControllerId(fixed_id(3, n as u64));
            self.controllers.push(controller);
        }
        // Every other controller runs dimmed with a gamma curve, as outdoor strings often do.
        let dim = self.controllers.len().is_multiple_of(2);
        let controller = self.controllers.last_mut().expect("a controller");
        let mut port = Port::new(controller.ports.len() as u16 + 1);
        if dim {
            port.brightness = 60;
            port.gamma = 2.2;
        }
        controller.ports.push(port);
        self.room = PORT_PIXELS;
    }

    /// Wires a prop's pixels, carrying on to the next port (and controller) as each fills.
    fn wire(&mut self, prop: PropId, pixels: u32) {
        let mut start = 0;
        while start < pixels {
            self.make_room();
            let take = self.room.min(pixels - start);
            let mut slot = PortSlot::new(prop);
            if take < pixels {
                slot.segment = Some(NodeRange::new(start, start + take));
            }
            // Now and then a string is wired from its far end.
            self.slots += 1;
            slot.reverse = self.slots.is_multiple_of(5);
            let port = self.controllers.last_mut().and_then(|c| c.ports.last_mut());
            port.expect("a port").slots.push(slot);
            self.room -= take;
            start += take;
        }
    }
}

/// The synthetic show with `pixels` pixels: the same show every time.
pub fn synthetic_show(pixels: u32) -> SyntheticShow {
    let share = mix(pixels);
    let part = |f: f32| (pixels as f32 * f) as u32;
    let mut pieces = Pieces::default();
    let mut used = outlines(&mut pieces, part(share.outline));
    used += arches(&mut pieces, part(share.arch));
    used += trees(&mut pieces, part(share.tree), pixels > 50_000);
    used += accents(&mut pieces, part(share.accent));
    used += matrices(&mut pieces, pixels.saturating_sub(used));
    // Whatever is left over lines the yard, so the show has exactly the pixels asked for.
    while used < pixels {
        let left = pixels - used;
        let nodes = if left <= 300 { left } else { 250 };
        let (shape, size) = line(nodes);
        pieces.push(Category::Outline, "Yard line", shape, size, true);
        used += nodes;
    }

    let positions = place(&pieces.list, pixels);
    let mut show = Show::new(format!("Synthetic {}", tier_label(pixels)));
    let mut props = Vec::new();
    for (i, (piece, position)) in pieces.list.into_iter().zip(positions).enumerate() {
        let mut prop = Prop::new(piece.name, ShapeSource::Generator(piece.shape));
        prop.id = PropId(fixed_id(1, i as u64));
        prop.transform.position = position;
        // Strings and strips are a mix of RGB and GRB; matrices are RGB.
        if matches!(piece.category, Category::Arch | Category::Accent) {
            prop.color_order = ColorOrder::Grb;
        }
        props.push(PropInfo {
            id: prop.id,
            category: piece.category,
            pixels: prop.node_count(),
        });
        show.props.push(prop);
    }

    let mut groups = Vec::new();
    let kinds = [
        (
            GroupKind::Of(Category::Outline),
            "All Outlines",
            GroupLayout::MinimalGrid,
        ),
        (
            GroupKind::Of(Category::Arch),
            "All Arches",
            GroupLayout::HorizontalPerModel,
        ),
        (
            GroupKind::Of(Category::Tree),
            "All Trees",
            GroupLayout::PerModelDefault,
        ),
        (
            GroupKind::Of(Category::Accent),
            "All Accents",
            GroupLayout::VerticalPerModel,
        ),
        (
            GroupKind::Of(Category::Matrix),
            "All Matrices",
            GroupLayout::MinimalGrid,
        ),
        (GroupKind::WholeHouse, "Whole House", GroupLayout::MinimalGrid),
    ];
    for (n, (kind, name, layout)) in kinds.into_iter().enumerate() {
        let members: Vec<&PropInfo> = props
            .iter()
            .filter(|p| match kind {
                GroupKind::WholeHouse => true,
                GroupKind::Of(category) => p.category == category,
            })
            .collect();
        if members.is_empty() {
            continue;
        }
        let mut group = Group::new(name);
        group.id = GroupId(fixed_id(2, n as u64));
        group.layout = layout;
        group.members = members.iter().map(|p| GroupMember::Prop(p.id)).collect();
        groups.push(GroupInfo {
            id: group.id,
            kind,
            pixels: members.iter().map(|p| p.pixels).sum(),
        });
        show.groups.push(group);
    }

    let mut strings = Strings {
        controllers: Vec::new(),
        room: 0,
        slots: 0,
    };
    let mut own = Vec::new();
    for prop in &props {
        // The largest matrix (they are made largest first) has its own controller in any show.
        let matrix = prop.category == Category::Matrix;
        if matrix && (prop.pixels >= OWN_CONTROLLER_PIXELS || own.is_empty()) {
            // Panel walls are DDP; the matrices between take turns.
            let ddp = prop.pixels >= PANEL_WALL_PIXELS || own.len().is_multiple_of(2);
            let (protocol, kind) = if ddp {
                (Protocol::Ddp, "DDP")
            } else {
                (Protocol::Sacn(SacnConfig::default()), "sACN")
            };
            let n = own.len();
            let mut controller = Controller::new(format!("Panels {} ({kind})", n + 1), LOOPBACK, protocol);
            controller.id = ControllerId(fixed_id(4, n as u64));
            let mut port = Port::new(1);
            port.slots.push(PortSlot::new(prop.id));
            controller.ports.push(port);
            own.push(controller);
        } else {
            strings.wire(prop.id, prop.pixels);
        }
    }
    show.controllers = strings.controllers;
    show.controllers.extend(own);
    SyntheticShow { show, props, groups }
}

/// "20k" for 20,000 pixels; the number itself when it isn't a round thousand.
pub fn tier_label(pixels: u32) -> String {
    if pixels.is_multiple_of(1000) {
        format!("{}k", pixels / 1000)
    } else {
        pixels.to_string()
    }
}

/// Reads a tier: "20k", "200K", or a plain pixel count.
pub fn parse_tier(text: &str) -> Result<u32, String> {
    let text = text.trim().to_ascii_lowercase();
    let (digits, scale) = match text.strip_suffix('k') {
        Some(digits) => (digits, 1000),
        None => (text.as_str(), 1),
    };
    let pixels = digits
        .parse::<u32>()
        .ok()
        .and_then(|n| n.checked_mul(scale))
        .ok_or_else(|| format!("'{text}' isn't a pixel count; use something like 20k, 100k, or 200k"))?;
    if !(500..=2_000_000).contains(&pixels) {
        return Err(format!(
            "{pixels} pixels is outside what the benchmark builds (500 to 2,000,000)"
        ));
    }
    Ok(pixels)
}

impl SyntheticShow {
    pub fn pixels(&self) -> u32 {
        self.props.iter().map(|p| p.pixels).sum()
    }

    pub fn composition(&self) -> Composition {
        let categories = Category::ALL
            .into_iter()
            .map(|category| {
                let of: Vec<&PropInfo> = self.props.iter().filter(|p| p.category == category).collect();
                CategoryCount {
                    category,
                    props: of.len(),
                    pixels: of.iter().map(|p| p.pixels).sum(),
                }
            })
            .collect();
        let mut matrices: Vec<MatrixCount> = Vec::new();
        for prop in &self.show.props {
            let ShapeSource::Generator(Generator::Matrix { columns, rows, .. }) = prop.shape else {
                continue;
            };
            match matrices
                .iter_mut()
                .find(|m| (m.columns, m.rows) == (columns, rows))
            {
                Some(found) => found.count += 1,
                None => matrices.push(MatrixCount {
                    columns,
                    rows,
                    count: 1,
                }),
            }
        }
        matrices.sort_by_key(|m| std::cmp::Reverse(m.columns * m.rows));
        let groups = self
            .show
            .groups
            .iter()
            .zip(&self.groups)
            .map(|(group, info)| GroupCount {
                name: group.name.clone(),
                layout: format!("{:?}", group.layout),
                members: group.members.len(),
                pixels: info.pixels,
            })
            .collect();
        let wired = |controller: &&Controller| -> u32 {
            controller
                .ports
                .iter()
                .flat_map(|p| &p.slots)
                .map(|slot| {
                    let nodes = self.show.prop(slot.prop).map_or(0, Prop::node_count);
                    slot.node_range(nodes).len()
                })
                .sum()
        };
        let (sacn, ddp): (Vec<&Controller>, Vec<&Controller>) = self
            .show
            .controllers
            .iter()
            .partition(|c| matches!(c.protocol, Protocol::Sacn(_)));
        Composition {
            pixels: self.pixels(),
            props: self.props.len(),
            categories,
            matrices,
            groups,
            controllers: self.show.controllers.len(),
            sacn_controllers: sacn.len(),
            sacn_pixels: sacn.iter().map(wired).sum(),
            ddp_controllers: ddp.len(),
            ddp_pixels: ddp.iter().map(wired).sum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tier_has_exactly_its_pixels_and_no_problems() {
        for pixels in [1_500, 20_000, 100_000, 200_000] {
            let made = synthetic_show(pixels);
            assert_eq!(made.pixels(), pixels);
            let report = pf_model::validate_show(&made.show);
            assert!(!report.has_errors(), "{pixels}: {:?}", report.issues);
            let (map, report) = pf_mapping::map_show(&made.show);
            assert!(!report.has_errors(), "{pixels}: {:?}", report.issues);
            // Every pixel is wired: the frame and the controllers carry three channels each.
            assert_eq!(map.frame_len, pixels as usize * 3);
            let composition = made.composition();
            assert_eq!(composition.sacn_pixels + composition.ddp_pixels, pixels);
        }
    }

    #[test]
    fn the_named_tiers_mix_every_kind_of_prop_and_both_protocols() {
        for pixels in TIERS {
            let composition = synthetic_show(pixels).composition();
            for count in &composition.categories {
                assert!(count.props > 0, "{pixels}: no {:?}", count.category);
            }
            assert!(composition.matrices.len() >= 3, "{pixels}: several matrix sizes");
            assert!(composition.sacn_controllers > 0 && composition.ddp_controllers > 0);
            assert_eq!(composition.groups.len(), 6);
            let whole = composition.groups.last().unwrap();
            assert_eq!((whole.name.as_str(), whole.pixels), ("Whole House", pixels));
        }
    }

    #[test]
    fn a_tier_is_the_same_show_every_time_and_only_ever_on_loopback() {
        let (a, b) = (synthetic_show(20_000), synthetic_show(20_000));
        assert_eq!(a.show, b.show);
        for controller in &a.show.controllers {
            assert_eq!(controller.address, LOOPBACK);
            if let Protocol::Sacn(sacn) = controller.protocol {
                assert!(!sacn.multicast, "multicast would leave the machine");
            }
        }
    }

    #[test]
    fn tiers_read_as_thousands_or_plain_counts() {
        assert_eq!(parse_tier("20k"), Ok(20_000));
        assert_eq!(parse_tier(" 200K "), Ok(200_000));
        assert_eq!(parse_tier("1500"), Ok(1_500));
        assert!(parse_tier("lots").is_err());
        assert!(parse_tier("12").is_err());
        assert_eq!(tier_label(100_000), "100k");
        assert_eq!(tier_label(1_234), "1234");
    }
}
