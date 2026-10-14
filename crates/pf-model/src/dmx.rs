//! DMX fixtures: floods, pars and bars driven channel by channel.
//!
//! A fixture is a prop with a [`DmxFixture`]: its shape gives where its light comes from (one
//! cell for a flood or a par, a row of cells for a multi-cell bar), so effects draw on it like on
//! any prop. The colour drawn on its cells becomes its channels by one rule
//! ([`write_channels`]), and the channels become the colour it shows ([`emitted_colors`]), which
//! the previews draw.
//!
//! The rule, per fixture:
//!
//! 1. **Dimmer.** With a dimmer channel, the dimmer carries the brightness: it is the brightest
//!    channel of any cell's colour, and each cell's colour is scaled up so that brightest channel
//!    is full. Floods have 8-bit colour channels: a dim colour carried in them loses its hue
//!    (orange at 5% is 13, 5, 0) and a fade steps; through the dimmer the hue stays exact and the
//!    fade is as smooth as the fixture's dimmer. Without a dimmer channel, the colour channels
//!    carry the brightness themselves.
//! 2. **White** is the part of the colour all three of red, green and blue share (their
//!    smallest), taken out of them: white LEDs make a whiter white than red, green and blue
//!    together. A cell with a white channel and no red, green or blue gives it its brightest
//!    channel.
//! 3. **Amber** stands for the colour 255, 128, 0: after white, it is the most of that the colour
//!    holds (the smaller of red and twice green), taken out of red and green.
//! 4. Every other channel (UV, strobe, a mode or macro, and the moving-head channels) holds the
//!    value set on the fixture, unless a DMX effect sets it.

use serde::{Deserialize, Serialize};

/// The most channels one fixture may have: a whole DMX universe.
pub const MAX_DMX_CHANNELS: usize = 512;
/// The most cells (separately coloured light sources) one fixture may have.
pub const MAX_DMX_CELLS: u32 = 64;
/// The longest a channel's name may be.
pub const MAX_DMX_CHANNEL_NAME: usize = 40;

/// What one DMX channel of a fixture does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum DmxRole {
    /// Master dimmer (intensity) of the whole fixture.
    Dimmer,
    Red,
    Green,
    Blue,
    White,
    Amber,
    /// Ultraviolet: held at its value.
    Uv,
    /// Strobe or shutter: held at its value.
    Strobe,
    /// A mode, macro or program channel: held at its value.
    #[default]
    Fixed,
    /// Moving-head channels, held at their value for now.
    Pan,
    PanFine,
    Tilt,
    TiltFine,
    ColorWheel,
    Gobo,
    /// Anything else: held at its value.
    Other,
}

impl DmxRole {
    pub const ALL: [DmxRole; 16] = [
        DmxRole::Dimmer,
        DmxRole::Red,
        DmxRole::Green,
        DmxRole::Blue,
        DmxRole::White,
        DmxRole::Amber,
        DmxRole::Uv,
        DmxRole::Strobe,
        DmxRole::Fixed,
        DmxRole::Pan,
        DmxRole::PanFine,
        DmxRole::Tilt,
        DmxRole::TiltFine,
        DmxRole::ColorWheel,
        DmxRole::Gobo,
        DmxRole::Other,
    ];

    /// The name people see.
    pub fn label(self) -> &'static str {
        match self {
            DmxRole::Dimmer => "Dimmer",
            DmxRole::Red => "Red",
            DmxRole::Green => "Green",
            DmxRole::Blue => "Blue",
            DmxRole::White => "White",
            DmxRole::Amber => "Amber",
            DmxRole::Uv => "UV",
            DmxRole::Strobe => "Strobe",
            DmxRole::Fixed => "Mode",
            DmxRole::Pan => "Pan",
            DmxRole::PanFine => "Pan fine",
            DmxRole::Tilt => "Tilt",
            DmxRole::TiltFine => "Tilt fine",
            DmxRole::ColorWheel => "Color wheel",
            DmxRole::Gobo => "Gobo",
            DmxRole::Other => "Other",
        }
    }

    /// A channel whose value comes from the colour drawn on the fixture (or one cell of it).
    pub fn from_color(self) -> bool {
        matches!(
            self,
            DmxRole::Dimmer | DmxRole::Red | DmxRole::Green | DmxRole::Blue | DmxRole::White | DmxRole::Amber
        )
    }

    /// A channel that belongs to one cell (its colour), rather than to the whole fixture.
    pub fn per_cell(self) -> bool {
        matches!(
            self,
            DmxRole::Red | DmxRole::Green | DmxRole::Blue | DmxRole::White | DmxRole::Amber
        )
    }
}

fn is_zero_u8(v: &u8) -> bool {
    *v == 0
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

/// One channel of a fixture, in the order the fixture takes them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct DmxChannel {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub role: DmxRole,
    /// The cell a colour channel belongs to (0: the first); unused for the other roles.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub cell: u32,
    /// The value it holds when nothing sets it (0-255); colour and dimmer channels are worked
    /// out from the colour instead.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub value: u8,
}

impl DmxChannel {
    pub fn new(role: DmxRole) -> Self {
        Self {
            name: role.label().to_string(),
            role,
            cell: 0,
            value: 0,
        }
    }

    pub fn in_cell(mut self, cell: u32) -> Self {
        self.cell = cell;
        self
    }

    pub fn holding(mut self, value: u8) -> Self {
        self.value = value;
        self
    }
}

/// What makes a prop a DMX fixture: where it starts in its universe, and its channels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct DmxFixture {
    /// The DMX start address (1-512) set on the fixture.
    pub address: u16,
    /// Its channels in the order it takes them, from the start address on.
    pub channels: Vec<DmxChannel>,
    /// The library layout it was made from (see [`DMX_LAYOUTS`]), when it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
}

impl DmxFixture {
    /// A fixture at `address` laid out as the library's `layout` with `cells` cells.
    pub fn from_layout(layout: &DmxLayout, cells: u32, address: u16) -> Self {
        Self {
            address,
            channels: layout.channels(cells),
            layout: Some(layout.id.to_string()),
        }
    }

    /// Channels it takes from its address on.
    pub fn footprint(&self) -> usize {
        self.channels.len()
    }

    /// The last address it uses.
    pub fn last_address(&self) -> usize {
        usize::from(self.address) + self.footprint().saturating_sub(1)
    }

    /// How many cells its colour channels name (at least one).
    pub fn cells(&self) -> u32 {
        self.channels
            .iter()
            .filter(|c| c.role.per_cell())
            .map(|c| c.cell.saturating_add(1))
            .max()
            .unwrap_or(1)
    }

    /// Why this fixture can't be used on a prop with `nodes` pixels (its cells), in plain words.
    pub fn problem(&self, nodes: u32) -> Option<String> {
        if self.channels.is_empty() {
            return Some("A DMX fixture needs at least one channel.".into());
        }
        if self.channels.len() > MAX_DMX_CHANNELS {
            return Some(format!(
                "A DMX fixture has at most {MAX_DMX_CHANNELS} channels; this one has {}.",
                self.channels.len()
            ));
        }
        if !(1..=512).contains(&self.address) {
            return Some(format!(
                "A DMX address is 1 to 512, so {} won't work.",
                self.address
            ));
        }
        if self.last_address() > 512 {
            return Some(format!(
                "Its {} channels from address {} run past 512, the end of the universe.",
                self.footprint(),
                self.address
            ));
        }
        if nodes == 0 || nodes > MAX_DMX_CELLS {
            return Some(format!(
                "A DMX fixture has 1 to {MAX_DMX_CELLS} cells (light positions)."
            ));
        }
        if let Some(c) = self
            .channels
            .iter()
            .find(|c| c.role.per_cell() && c.cell >= nodes)
        {
            return Some(format!(
                "The channel '{}' is for cell {}, but the fixture has {nodes}.",
                c.name,
                c.cell + 1
            ));
        }
        if let Some(c) = self
            .channels
            .iter()
            .find(|c| c.name.chars().count() > MAX_DMX_CHANNEL_NAME)
        {
            return Some(format!(
                "The channel name '{}…' is longer than {MAX_DMX_CHANNEL_NAME} characters.",
                c.name.chars().take(12).collect::<String>()
            ));
        }
        None
    }
}

/// A channel layout from the library: some channels for the whole fixture (`head`), the same
/// channels for each cell (`cell`), then more for the whole fixture (`tail`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DmxLayout {
    pub id: &'static str,
    pub label: &'static str,
    pub head: &'static [DmxRole],
    pub cell: &'static [DmxRole],
    pub tail: &'static [DmxRole],
    /// Made for several cells (a bar); the others are usually one.
    pub multi_cell: bool,
}

impl DmxLayout {
    /// Its channels for a fixture of `cells` cells, named by role (with the cell's number when
    /// there are several).
    pub fn channels(&self, cells: u32) -> Vec<DmxChannel> {
        let cells = cells.max(1);
        let mut out: Vec<DmxChannel> = self.head.iter().map(|&r| DmxChannel::new(r)).collect();
        for cell in 0..cells {
            for &role in self.cell {
                let mut channel = DmxChannel::new(role);
                if cells > 1 {
                    channel.name = format!("{} {}", role.label(), cell + 1);
                }
                if role.per_cell() {
                    channel.cell = cell;
                }
                out.push(channel);
            }
        }
        out.extend(self.tail.iter().map(|&r| DmxChannel::new(r)));
        out
    }

    /// Channels a fixture of `cells` cells takes.
    pub fn footprint(&self, cells: u32) -> usize {
        self.head.len() + self.cell.len() * cells.max(1) as usize + self.tail.len()
    }
}

use DmxRole::{
    Amber as A, Blue as B, Dimmer as D, Fixed as M, Green as G, Red as R, Strobe as S, Uv as U, White as W,
};

/// The library of common flood, par and bar layouts, named by their channels.
pub const DMX_LAYOUTS: &[DmxLayout] = &[
    DmxLayout {
        id: "rgb",
        label: "3 channels: red, green, blue",
        head: &[],
        cell: &[R, G, B],
        tail: &[],
        multi_cell: false,
    },
    DmxLayout {
        id: "dimmer-rgb",
        label: "4 channels: dimmer, red, green, blue",
        head: &[D],
        cell: &[R, G, B],
        tail: &[],
        multi_cell: false,
    },
    DmxLayout {
        id: "rgbw",
        label: "4 channels: red, green, blue, white",
        head: &[],
        cell: &[R, G, B, W],
        tail: &[],
        multi_cell: false,
    },
    DmxLayout {
        id: "dimmer-rgb-strobe",
        label: "5 channels: dimmer, red, green, blue, strobe",
        head: &[D],
        cell: &[R, G, B],
        tail: &[S],
        multi_cell: false,
    },
    DmxLayout {
        id: "rgbwa",
        label: "5 channels: red, green, blue, white, amber",
        head: &[],
        cell: &[R, G, B, W, A],
        tail: &[],
        multi_cell: false,
    },
    DmxLayout {
        id: "dimmer-rgbw-strobe",
        label: "6 channels: dimmer, red, green, blue, white, strobe",
        head: &[D],
        cell: &[R, G, B, W],
        tail: &[S],
        multi_cell: false,
    },
    DmxLayout {
        id: "rgbwa-uv",
        label: "6 channels: red, green, blue, white, amber, UV",
        head: &[],
        cell: &[R, G, B, W, A, U],
        tail: &[],
        multi_cell: false,
    },
    DmxLayout {
        id: "dimmer-rgbw-strobe-mode",
        label: "7 channels: dimmer, red, green, blue, white, strobe, mode",
        head: &[D],
        cell: &[R, G, B, W],
        tail: &[S, M],
        multi_cell: false,
    },
    DmxLayout {
        id: "dimmer-rgbwa-uv-strobe",
        label: "8 channels: dimmer, red, green, blue, white, amber, UV, strobe",
        head: &[D],
        cell: &[R, G, B, W, A, U],
        tail: &[S],
        multi_cell: false,
    },
    DmxLayout {
        id: "bar-rgb",
        label: "Bar: red, green, blue for each cell",
        head: &[],
        cell: &[R, G, B],
        tail: &[],
        multi_cell: true,
    },
    DmxLayout {
        id: "bar-dimmer-strobe-rgb",
        label: "Bar: dimmer, strobe, then red, green, blue for each cell",
        head: &[D, S],
        cell: &[R, G, B],
        tail: &[],
        multi_cell: true,
    },
    DmxLayout {
        id: "bar-rgbw",
        label: "Bar: red, green, blue, white for each cell",
        head: &[],
        cell: &[R, G, B, W],
        tail: &[],
        multi_cell: true,
    },
];

/// The library layout called `id`.
pub fn dmx_layout(id: &str) -> Option<&'static DmxLayout> {
    DMX_LAYOUTS.iter().find(|l| l.id == id)
}

/// Half of an amber value, as green holds it (amber stands for 255, 128, 0).
fn half(a: u8) -> u8 {
    a.div_ceil(2)
}

/// `value` times `num / den`, rounded, at most 255.
fn scale(value: u8, (num, den): (u32, u32)) -> u8 {
    ((u32::from(value) * num + den / 2) / den).min(255) as u8
}

/// What a fixture's channels hold for the colour drawn on its cells: `cells` holds each cell's
/// red, green and blue (three bytes a cell), `out` gets one byte per channel. Channels that
/// don't come from the colour get their held value. Allocation-free.
pub fn write_channels(channels: &[DmxChannel], cells: &[u8], out: &mut [u8]) {
    let cell_rgb = |cell: u32| -> [u8; 3] {
        let at = cell as usize * 3;
        cells.get(at..at + 3).map_or([0; 3], |c| [c[0], c[1], c[2]])
    };
    let has = |role: DmxRole| channels.iter().any(|c| c.role == role);
    let dimmed = has(DmxRole::Dimmer);
    let peak = (0..(cells.len() / 3) as u32)
        .map(|c| cell_rgb(c).into_iter().max().unwrap_or(0))
        .max()
        .unwrap_or(0);
    // With a dimmer, each colour is brought up so the brightest channel of the fixture is full.
    let gain = if dimmed && peak > 0 {
        (255, u32::from(peak))
    } else {
        (1, 1)
    };
    for (channel, byte) in channels.iter().zip(out.iter_mut()) {
        *byte = match channel.role {
            DmxRole::Dimmer => peak,
            role if role.per_cell() => {
                let cell = channel.cell;
                let [r, g, b] = cell_rgb(cell).map(|v| scale(v, gain));
                let in_cell = |role: DmxRole| channels.iter().any(|c| c.role == role && c.cell == cell);
                let rgb = in_cell(DmxRole::Red) || in_cell(DmxRole::Green) || in_cell(DmxRole::Blue);
                let white = match (in_cell(DmxRole::White), rgb) {
                    (false, _) => 0,
                    (true, true) => r.min(g).min(b),
                    (true, false) => r.max(g).max(b),
                };
                let (r, g, b) = if rgb {
                    (r - white, g - white, b - white)
                } else {
                    (r, g, b)
                };
                let amber = if in_cell(DmxRole::Amber) {
                    r.min(g.saturating_mul(2))
                } else {
                    0
                };
                match role {
                    DmxRole::Red => r - amber,
                    DmxRole::Green => g - half(amber).min(g),
                    DmxRole::Blue => b,
                    DmxRole::White => white,
                    _ => amber,
                }
            }
            _ => channel.value,
        };
    }
}

/// The colour each cell shows for the fixture's `dmx` channel values, three bytes a cell, into
/// `cells`. Allocation-free.
pub fn emitted_colors(channels: &[DmxChannel], dmx: &[u8], cells: &mut [u8]) {
    let value = |i: usize| dmx.get(i).copied().unwrap_or(0);
    let dimmer = channels.iter().position(|c| c.role == DmxRole::Dimmer).map(value);
    for (cell, rgb) in cells.as_chunks_mut::<3>().0.iter_mut().enumerate() {
        let cell = cell as u32;
        let (mut r, mut g, mut b, mut lit) = (0u32, 0u32, 0u32, false);
        for (i, c) in channels.iter().enumerate() {
            let v = u32::from(value(i));
            let mine = c.cell == cell;
            match c.role {
                DmxRole::Red if mine => (r, lit) = (r + v, true),
                DmxRole::Green if mine => (g, lit) = (g + v, true),
                DmxRole::Blue if mine => (b, lit) = (b + v, true),
                DmxRole::White if mine => (r, g, b, lit) = (r + v, g + v, b + v, true),
                DmxRole::Amber if mine => (r, g, lit) = (r + v, g + u32::from(half(v as u8)), true),
                // Ultraviolet glows a faint violet on what it lights.
                DmxRole::Uv => (r, b, lit) = (r + v / 4, b + v / 2, true),
                _ => {}
            }
        }
        let level = u32::from(dimmer.unwrap_or(255));
        let (r, g, b) = if lit { (r, g, b) } else { (255, 255, 255) };
        let out = |v: u32| ((v.min(255) * level + 127) / 255) as u8;
        *rgb = [out(r), out(g), out(b)];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(id: &str, cells: u32) -> Vec<DmxChannel> {
        dmx_layout(id).unwrap().channels(cells)
    }

    fn channels_for(id: &str, cells: &[u8]) -> Vec<u8> {
        let channels = layout(id, (cells.len() / 3) as u32);
        let mut out = vec![0; channels.len()];
        write_channels(&channels, cells, &mut out);
        out
    }

    fn shown(id: &str, cells: &[u8]) -> Vec<u8> {
        let channels = layout(id, (cells.len() / 3) as u32);
        let mut dmx = vec![0; channels.len()];
        write_channels(&channels, cells, &mut dmx);
        let mut back = vec![0; cells.len()];
        emitted_colors(&channels, &dmx, &mut back);
        back
    }

    #[test]
    fn library_layouts_have_the_channel_counts_their_names_say() {
        for (id, count) in [
            ("rgb", 3),
            ("dimmer-rgb", 4),
            ("rgbw", 4),
            ("dimmer-rgb-strobe", 5),
            ("rgbwa", 5),
            ("dimmer-rgbw-strobe", 6),
            ("rgbwa-uv", 6),
            ("dimmer-rgbw-strobe-mode", 7),
            ("dimmer-rgbwa-uv-strobe", 8),
        ] {
            let l = dmx_layout(id).unwrap();
            assert_eq!(l.channels(1).len(), count, "{id}");
            assert_eq!(l.footprint(1), count, "{id}");
            assert!(l.label.starts_with(&format!("{count} channels")), "{id}");
        }
        let bar = dmx_layout("bar-dimmer-strobe-rgb").unwrap().channels(4);
        assert_eq!(bar.len(), 2 + 12);
        assert_eq!(bar[2].name, "Red 1");
        assert_eq!(bar[13].name, "Blue 4");
        assert_eq!(bar[13].cell, 3);
        let ids: std::collections::HashSet<_> = DMX_LAYOUTS.iter().map(|l| l.id).collect();
        assert_eq!(ids.len(), DMX_LAYOUTS.len(), "ids are unique");
    }

    #[test]
    fn plain_rgb_floods_carry_the_colour_as_drawn() {
        assert_eq!(channels_for("rgb", &[200, 100, 0]), vec![200, 100, 0]);
        assert_eq!(shown("rgb", &[200, 100, 0]), vec![200, 100, 0]);
    }

    #[test]
    fn the_dimmer_carries_the_brightness_and_the_colour_keeps_its_hue() {
        // Orange at about 5%: through the colour channels alone it would be 13, 5, 0.
        assert_eq!(channels_for("dimmer-rgb", &[13, 6, 0]), vec![13, 255, 118, 0]);
        assert_eq!(channels_for("dimmer-rgb", &[0, 0, 0]), vec![0, 0, 0, 0]);
        assert_eq!(channels_for("dimmer-rgb", &[255, 0, 255]), vec![255, 255, 0, 255]);
        // What it shows comes back as drawn (within rounding).
        for color in [
            [13u8, 6, 0],
            [200, 100, 50],
            [1, 1, 1],
            [255, 255, 255],
            [0, 0, 0],
        ] {
            let back = shown("dimmer-rgb", &color);
            for (a, b) in back.iter().zip(color) {
                assert!(a.abs_diff(b) <= 1, "{color:?} came back as {back:?}");
            }
        }
    }

    #[test]
    fn white_takes_the_part_all_three_share_and_amber_the_orange() {
        // Pale blue: white 100 and blue's extra.
        assert_eq!(channels_for("rgbw", &[100, 100, 250]), vec![0, 0, 150, 100]);
        // Orange is all amber.
        assert_eq!(channels_for("rgbwa", &[255, 128, 0]), vec![0, 0, 0, 0, 255]);
        // Warm white: white, then amber for the warmth.
        assert_eq!(channels_for("rgbwa", &[255, 200, 100]), vec![0, 22, 0, 100, 155]);
        for color in [[255u8, 128, 0], [255, 200, 100], [10, 200, 30], [255, 255, 255]] {
            assert_eq!(shown("rgbwa", &color), color.to_vec(), "{color:?}");
        }
    }

    #[test]
    fn held_channels_keep_their_value_whatever_the_colour() {
        let mut channels = layout("dimmer-rgbw-strobe-mode", 1);
        channels[5].value = 0;
        channels[6].value = 201;
        let mut out = vec![9; 7];
        write_channels(&channels, &[255, 0, 0], &mut out);
        assert_eq!(out, vec![255, 255, 0, 0, 0, 0, 201]);
    }

    #[test]
    fn a_bar_shares_one_dimmer_set_by_its_brightest_cell() {
        // Cell 1 full red, cell 2 half green.
        let out = channels_for("bar-dimmer-strobe-rgb", &[255, 0, 0, 0, 128, 0]);
        assert_eq!(out, vec![255, 0, 255, 0, 0, 0, 128, 0]);
        let out = channels_for("bar-dimmer-strobe-rgb", &[0, 0, 100, 0, 50, 0]);
        assert_eq!(out, vec![100, 0, 0, 0, 255, 0, 128, 0]);
        assert_eq!(
            shown("bar-dimmer-strobe-rgb", &[0, 0, 100, 0, 50, 0]),
            vec![0, 0, 100, 0, 50, 0]
        );
    }

    #[test]
    fn a_white_only_or_dimmer_only_fixture_follows_the_brightest_channel() {
        let white = vec![DmxChannel::new(DmxRole::White)];
        let mut out = [0];
        write_channels(&white, &[30, 90, 10], &mut out);
        assert_eq!(out, [90]);
        let dimmer = vec![DmxChannel::new(DmxRole::Dimmer)];
        write_channels(&dimmer, &[30, 90, 10], &mut out);
        assert_eq!(out, [90]);
        let mut cell = [0; 3];
        emitted_colors(&dimmer, &[90], &mut cell);
        assert_eq!(cell, [90, 90, 90]);
    }

    #[test]
    fn fixture_problems_are_explained() {
        let ok = DmxFixture::from_layout(dmx_layout("dimmer-rgb").unwrap(), 1, 1);
        assert_eq!(ok.problem(1), None);
        let mut far = ok.clone();
        far.address = 510;
        assert!(far.problem(1).unwrap().contains("past 512"));
        far.address = 509;
        assert_eq!(far.problem(1), None);
        let mut zero = ok.clone();
        zero.address = 0;
        assert!(zero.problem(1).unwrap().contains("1 to 512"));
        let bar = DmxFixture::from_layout(dmx_layout("bar-rgb").unwrap(), 3, 1);
        assert_eq!(bar.cells(), 3);
        assert!(bar.problem(2).unwrap().contains("cell 3"));
        let empty = DmxFixture {
            address: 1,
            channels: vec![],
            layout: None,
        };
        assert!(empty.problem(1).is_some());
    }

    #[test]
    fn channels_save_only_what_they_set() {
        let json = serde_json::to_value(DmxChannel::new(DmxRole::Red)).unwrap();
        assert_eq!(json, serde_json::json!({ "name": "Red", "role": "red" }));
        let json = serde_json::to_value(DmxChannel::new(DmxRole::Fixed).holding(7).in_cell(2)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "name": "Mode", "role": "fixed", "cell": 2, "value": 7 })
        );
        let back: DmxChannel = serde_json::from_value(json).unwrap();
        assert_eq!(back.value, 7);
    }
}
