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
//! 4. **Cyan, magenta and yellow** (a fixture that mixes colour by taking it out of a white lamp)
//!    are what red, green and blue leave out: cyan is 255 less the red, and so on.
//! 5. **A colour wheel** with its colours listed (its slots) takes the slot nearest the colour
//!    drawn (at full brightness: the dimmer carries the brightness). Where nothing is drawn it
//!    keeps the value it holds, so it doesn't turn for nothing.
//! 6. Every other channel (UV, strobe, a mode or macro, pan, tilt, zoom and servos) holds the
//!    value set on the fixture, unless a DMX effect sets it. A moving head's pan and tilt, and a
//!    servo, turn as their `motor` says (see `motion.rs`); a zoom widens a head's beam as its
//!    `zoom` says.
//!
//! What the fixture shows at a moment is the colour its channels make ([`emitted_colors`]),
//! unless its shutter is closed or its strobe is between flashes then ([`shutter_open`], see
//! `shutter.rs`).

use crate::motion::{DmxBeam, DmxMotor, DmxSlot, DmxZoom, MAX_DMX_SLOTS};
use crate::shutter::{DmxShutterRange, Moment, USUAL_SHUTTER, shutter_problem, shutter_shows};
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
    /// Strobe or shutter: held at its value; what its values do (closed, open, a strobe) is its
    /// `shutter`.
    Strobe,
    /// A mode, macro or program channel: held at its value.
    #[default]
    Fixed,
    /// A moving head's pan (turning round) and tilt (leaning), each with its fine channel for
    /// a 16-bit head: held at their value unless a DMX effect sets them.
    Pan,
    PanFine,
    Tilt,
    TiltFine,
    /// A colour wheel: from the colour drawn when its slots are listed, else held.
    ColorWheel,
    Gobo,
    /// Anything else: held at its value.
    Other,
    /// Colour taken out of a white lamp: what the colour drawn leaves out of red, green, blue.
    Cyan,
    Magenta,
    Yellow,
    /// A servo's position, and its fine channel for a 16-bit servo.
    Servo,
    ServoFine,
    /// How fast a head moves: held at its value.
    Speed,
    /// How wide a head's beam is: held at its value, widening the beam as its `zoom` says.
    Zoom,
}

impl DmxRole {
    pub const ALL: [DmxRole; 23] = [
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
        DmxRole::Cyan,
        DmxRole::Magenta,
        DmxRole::Yellow,
        DmxRole::Servo,
        DmxRole::ServoFine,
        DmxRole::Speed,
        DmxRole::Zoom,
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
            DmxRole::Cyan => "Cyan",
            DmxRole::Magenta => "Magenta",
            DmxRole::Yellow => "Yellow",
            DmxRole::Servo => "Servo",
            DmxRole::ServoFine => "Servo fine",
            DmxRole::Speed => "Speed",
            DmxRole::Zoom => "Zoom",
        }
    }

    /// A channel whose value comes from the colour drawn on the fixture (or one cell of it).
    pub fn from_color(self) -> bool {
        matches!(
            self,
            DmxRole::Dimmer
                | DmxRole::Red
                | DmxRole::Green
                | DmxRole::Blue
                | DmxRole::White
                | DmxRole::Amber
                | DmxRole::Cyan
                | DmxRole::Magenta
                | DmxRole::Yellow
        )
    }

    /// A channel that belongs to one cell (its colour), rather than to the whole fixture.
    pub fn per_cell(self) -> bool {
        matches!(
            self,
            DmxRole::Red
                | DmxRole::Green
                | DmxRole::Blue
                | DmxRole::White
                | DmxRole::Amber
                | DmxRole::Cyan
                | DmxRole::Magenta
                | DmxRole::Yellow
        )
    }

    /// A motor's channel: a head's pan or tilt, or a servo (each with a `motor`).
    pub fn is_motor(self) -> bool {
        matches!(self, DmxRole::Pan | DmxRole::Tilt | DmxRole::Servo)
    }

    /// A channel that moves the fixture: a motor's, or its fine channel. Blackout leaves these
    /// where they are, so stopping never swings a head.
    pub fn moves(self) -> bool {
        matches!(
            self,
            DmxRole::Pan
                | DmxRole::PanFine
                | DmxRole::Tilt
                | DmxRole::TiltFine
                | DmxRole::Servo
                | DmxRole::ServoFine
        )
    }

    /// The fine channel's role for a motor's role.
    pub fn fine(self) -> Option<DmxRole> {
        match self {
            DmxRole::Pan => Some(DmxRole::PanFine),
            DmxRole::Tilt => Some(DmxRole::TiltFine),
            DmxRole::Servo => Some(DmxRole::ServoFine),
            _ => None,
        }
    }

    /// A channel that holds slots (a colour wheel or a gobo wheel).
    pub fn has_slots(self) -> bool {
        matches!(self, DmxRole::ColorWheel | DmxRole::Gobo)
    }
}

fn is_zero_u8(v: &u8) -> bool {
    *v == 0
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

/// One channel of a fixture, in the order the fixture takes them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// out from the colour instead. A motor's held value is where it rests.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub value: u8,
    /// How a pan, tilt or servo channel turns with its value (left out: the usual for its role,
    /// see [`DmxMotor::default_for`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motor: Option<DmxMotor>,
    /// A colour wheel's colours, or a gobo wheel's gobos, by the value that selects each.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<DmxSlot>,
    /// What a strobe channel's values do: closed, open or a strobe, by range (left out: the usual,
    /// see [`USUAL_SHUTTER`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shutter: Vec<DmxShutterRange>,
    /// How a zoom channel widens the beam (left out: the usual, see [`DmxZoom::default`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<DmxZoom>,
}

impl DmxChannel {
    pub fn new(role: DmxRole) -> Self {
        Self {
            name: role.label().to_string(),
            role,
            cell: 0,
            value: 0,
            motor: role.is_motor().then(|| DmxMotor::default_for(role)),
            slots: Vec::new(),
            shutter: Vec::new(),
            zoom: None,
        }
    }

    /// Its motor, or the usual one for its role (a motor's channel only).
    pub fn motor(&self) -> Option<DmxMotor> {
        self.role
            .is_motor()
            .then(|| self.motor.unwrap_or_else(|| DmxMotor::default_for(self.role)))
    }

    /// What its values do, for a strobe channel: its own ranges, or the usual ones.
    pub fn shutter(&self) -> Option<&[DmxShutterRange]> {
        (self.role == DmxRole::Strobe).then(|| {
            if self.shutter.is_empty() {
                USUAL_SHUTTER
            } else {
                &self.shutter[..]
            }
        })
    }

    /// How it widens the beam, for a zoom channel: its own, or the usual.
    pub fn zoom(&self) -> Option<DmxZoom> {
        (self.role == DmxRole::Zoom).then(|| self.zoom.unwrap_or_default())
    }

    /// Its value comes from the colour drawn: a colour or dimmer channel, or a colour wheel with
    /// its colours listed.
    pub fn from_color(&self) -> bool {
        self.role.from_color() || self.is_color_wheel()
    }

    /// A colour wheel with its colours listed.
    pub fn is_color_wheel(&self) -> bool {
        self.role == DmxRole::ColorWheel && self.slots.iter().any(|s| s.color.is_some())
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// A moving head's beam, as the previews draw it (left out: the usual, for a head).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beam: Option<DmxBeam>,
}

/// One motor of a fixture: a head's pan or tilt, or a servo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DmxAxis {
    pub role: DmxRole,
    /// Its channel, and its fine channel for a 16-bit motor (indices into the fixture's).
    pub coarse: usize,
    pub fine: Option<usize>,
    pub motor: DmxMotor,
}

impl DmxAxis {
    /// The values it takes for `degrees` from Home: its channel's, and its fine channel's (if it
    /// has one).
    pub fn values(&self, degrees: f64) -> (u8, Option<u8>) {
        let (coarse, fine) = self.motor.values(degrees, self.fine.is_some());
        (coarse, self.fine.map(|_| fine))
    }

    /// Where it is, in degrees from Home, with the fixture's channels at `dmx`.
    pub fn degrees(&self, dmx: &[u8]) -> f64 {
        let at = |i: usize| dmx.get(i).copied().unwrap_or(0);
        self.motor.degrees(at(self.coarse), self.fine.map(at))
    }
}

/// A fixture's motors, in the order of their channels: each pan, tilt and servo channel with its
/// fine channel. The k-th fine channel of a kind belongs to the k-th motor of that kind,
/// wherever it is (heads often put both fine channels after both coarse ones).
pub fn axes(channels: &[DmxChannel]) -> Vec<DmxAxis> {
    let mut out = Vec::new();
    for (i, c) in channels.iter().enumerate() {
        let Some(motor) = c.motor() else { continue };
        let nth = channels[..i].iter().filter(|o| o.role == c.role).count();
        let fine = c.role.fine().and_then(|fine| {
            channels
                .iter()
                .enumerate()
                .filter(|(_, o)| o.role == fine)
                .nth(nth)
                .map(|(j, _)| j)
        });
        out.push(DmxAxis {
            role: c.role,
            coarse: i,
            fine,
            motor,
        });
    }
    out
}

/// Sets each motor's held values (its channel's, and its fine channel's) to `degrees` from Home:
/// where it rests when nothing moves it.
pub fn rest_motors_at(channels: &mut [DmxChannel], degrees: f64) {
    for axis in axes(channels) {
        let (coarse, fine) = axis.values(degrees);
        channels[axis.coarse].value = coarse;
        if let (Some(at), Some(v)) = (axis.fine, fine) {
            channels[at].value = v;
        }
    }
}

impl DmxFixture {
    /// A fixture at `address` laid out as the library's `layout` with `cells` cells.
    pub fn from_layout(layout: &DmxLayout, cells: u32, address: u16) -> Self {
        let mut fixture = Self {
            address,
            channels: layout.channels(cells),
            layout: Some(layout.id.to_string()),
            beam: None,
        };
        if fixture.is_head() {
            fixture.beam = Some(DmxBeam::default());
        }
        fixture
    }

    /// Its motors (see [`axes`]).
    pub fn axes(&self) -> Vec<DmxAxis> {
        axes(&self.channels)
    }

    /// A moving head: it has a pan or a tilt channel.
    pub fn is_head(&self) -> bool {
        self.channels
            .iter()
            .any(|c| matches!(c.role, DmxRole::Pan | DmxRole::Tilt))
    }

    /// Its beam as drawn: its own, or the usual one for a head.
    pub fn beam(&self) -> Option<DmxBeam> {
        self.is_head().then(|| self.beam.unwrap_or_default())
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
        for c in &self.channels {
            if let Some(problem) = c.motor.and_then(|m| m.problem()) {
                return Some(format!("The channel '{}': {problem}", c.name));
            }
            if let Some(problem) = shutter_problem(&c.shutter).or_else(|| c.zoom.and_then(|z| z.problem())) {
                return Some(format!("The channel '{}': {problem}", c.name));
            }
            if c.slots.len() > MAX_DMX_SLOTS {
                return Some(format!(
                    "The channel '{}' lists {} slots; a wheel has at most {MAX_DMX_SLOTS}.",
                    c.name,
                    c.slots.len()
                ));
            }
            if let Some(slot) = c.slots.iter().find(|s| {
                s.name
                    .as_ref()
                    .is_some_and(|n| n.chars().count() > MAX_DMX_CHANNEL_NAME)
            }) {
                return Some(format!(
                    "The slot at {} of '{}' has a name longer than {MAX_DMX_CHANNEL_NAME} characters.",
                    slot.value, c.name
                ));
            }
        }
        if let Some(problem) = self.beam.and_then(|b| b.problem()) {
            return Some(problem);
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
    /// Made for several cells (a bar, or several servos); the others are usually one.
    pub multi_cell: bool,
    /// What kind of fixture it is for.
    pub kind: DmxKind,
}

/// What kind of fixture a library layout is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DmxKind {
    /// A flood, par or bar: light only.
    Flood,
    /// A moving head: light, pointed by its pan and tilt.
    Head,
    /// Servos, one a cell: they move something, and make no light.
    Servo,
}

impl DmxLayout {
    /// Its channels for a fixture of `cells` cells, named by role (with the cell's number when
    /// there are several). Motors rest at Home.
    pub fn channels(&self, cells: u32) -> Vec<DmxChannel> {
        let mut out = self.channels_as_listed(cells);
        rest_motors_at(&mut out, 0.0);
        out
    }

    fn channels_as_listed(&self, cells: u32) -> Vec<DmxChannel> {
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
    Amber as A, Blue as B, Dimmer as D, Fixed as M, Green as G, Pan as P, PanFine as PF, Red as R,
    Speed as SP, Strobe as S, Tilt as T, TiltFine as TF, Uv as U, White as W,
};

/// The library of common flood, par, bar, moving-head and servo layouts, named by their channels
/// (generic layouts many fixtures share, not any one maker's).
pub const DMX_LAYOUTS: &[DmxLayout] = &[
    DmxLayout {
        id: "rgb",
        label: "3 channels: red, green, blue",
        head: &[],
        cell: &[R, G, B],
        tail: &[],
        multi_cell: false,
        kind: DmxKind::Flood,
    },
    DmxLayout {
        id: "dimmer-rgb",
        label: "4 channels: dimmer, red, green, blue",
        head: &[D],
        cell: &[R, G, B],
        tail: &[],
        multi_cell: false,
        kind: DmxKind::Flood,
    },
    DmxLayout {
        id: "rgbw",
        label: "4 channels: red, green, blue, white",
        head: &[],
        cell: &[R, G, B, W],
        tail: &[],
        multi_cell: false,
        kind: DmxKind::Flood,
    },
    DmxLayout {
        id: "dimmer-rgb-strobe",
        label: "5 channels: dimmer, red, green, blue, strobe",
        head: &[D],
        cell: &[R, G, B],
        tail: &[S],
        multi_cell: false,
        kind: DmxKind::Flood,
    },
    DmxLayout {
        id: "rgbwa",
        label: "5 channels: red, green, blue, white, amber",
        head: &[],
        cell: &[R, G, B, W, A],
        tail: &[],
        multi_cell: false,
        kind: DmxKind::Flood,
    },
    DmxLayout {
        id: "dimmer-rgbw-strobe",
        label: "6 channels: dimmer, red, green, blue, white, strobe",
        head: &[D],
        cell: &[R, G, B, W],
        tail: &[S],
        multi_cell: false,
        kind: DmxKind::Flood,
    },
    DmxLayout {
        id: "rgbwa-uv",
        label: "6 channels: red, green, blue, white, amber, UV",
        head: &[],
        cell: &[R, G, B, W, A, U],
        tail: &[],
        multi_cell: false,
        kind: DmxKind::Flood,
    },
    DmxLayout {
        id: "dimmer-rgbw-strobe-mode",
        label: "7 channels: dimmer, red, green, blue, white, strobe, mode",
        head: &[D],
        cell: &[R, G, B, W],
        tail: &[S, M],
        multi_cell: false,
        kind: DmxKind::Flood,
    },
    DmxLayout {
        id: "dimmer-rgbwa-uv-strobe",
        label: "8 channels: dimmer, red, green, blue, white, amber, UV, strobe",
        head: &[D],
        cell: &[R, G, B, W, A, U],
        tail: &[S],
        multi_cell: false,
        kind: DmxKind::Flood,
    },
    DmxLayout {
        id: "bar-rgb",
        label: "Bar: red, green, blue for each cell",
        head: &[],
        cell: &[R, G, B],
        tail: &[],
        multi_cell: true,
        kind: DmxKind::Flood,
    },
    DmxLayout {
        id: "bar-dimmer-strobe-rgb",
        label: "Bar: dimmer, strobe, then red, green, blue for each cell",
        head: &[D, S],
        cell: &[R, G, B],
        tail: &[],
        multi_cell: true,
        kind: DmxKind::Flood,
    },
    DmxLayout {
        id: "bar-rgbw",
        label: "Bar: red, green, blue, white for each cell",
        head: &[],
        cell: &[R, G, B, W],
        tail: &[],
        multi_cell: true,
        kind: DmxKind::Flood,
    },
    DmxLayout {
        id: "head-rgbw-16",
        label: "Moving head, 11 channels: pan, pan fine, tilt, tilt fine, speed, dimmer, strobe, red, green, blue, white",
        head: &[P, PF, T, TF, SP, D, S],
        cell: &[R, G, B, W],
        tail: &[],
        multi_cell: false,
        kind: DmxKind::Head,
    },
    DmxLayout {
        id: "head-rgbw-16-split",
        label: "Moving head, 11 channels: pan, tilt, pan fine, tilt fine, speed, dimmer, strobe, red, green, blue, white",
        head: &[P, T, PF, TF, SP, D, S],
        cell: &[R, G, B, W],
        tail: &[],
        multi_cell: false,
        kind: DmxKind::Head,
    },
    DmxLayout {
        id: "head-rgbw-8",
        label: "Moving head, 8 channels: pan, tilt, speed, dimmer, red, green, blue, white",
        head: &[P, T, SP, D],
        cell: &[R, G, B, W],
        tail: &[],
        multi_cell: false,
        kind: DmxKind::Head,
    },
    DmxLayout {
        id: "head-spot-16",
        label: "Moving head, 9 channels: pan, pan fine, tilt, tilt fine, speed, dimmer, strobe, color wheel, gobo",
        head: &[P, PF, T, TF, SP, D, S, DmxRole::ColorWheel, DmxRole::Gobo],
        cell: &[],
        tail: &[],
        multi_cell: false,
        kind: DmxKind::Head,
    },
    DmxLayout {
        id: "servo-8",
        label: "Servos, 1 channel each: position",
        head: &[],
        cell: &[DmxRole::Servo],
        tail: &[],
        multi_cell: true,
        kind: DmxKind::Servo,
    },
    DmxLayout {
        id: "servo-16",
        label: "Servos, 2 channels each: position, fine",
        head: &[],
        cell: &[DmxRole::Servo, DmxRole::ServoFine],
        tail: &[],
        multi_cell: true,
        kind: DmxKind::Servo,
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

/// The slot with a colour that `distance` puts nearest (the first of equals), if any has one.
fn nearest_slot(slots: &[DmxSlot], distance: impl Fn(&DmxSlot) -> i32) -> Option<&DmxSlot> {
    slots
        .iter()
        .filter(|s| s.color.is_some())
        .min_by_key(|s| distance(s))
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
            // What's left out of a white lamp: cyan takes out red, and so on.
            DmxRole::Cyan => 255 - scale(cell_rgb(channel.cell)[0], gain),
            DmxRole::Magenta => 255 - scale(cell_rgb(channel.cell)[1], gain),
            DmxRole::Yellow => 255 - scale(cell_rgb(channel.cell)[2], gain),
            // A wheel turns to the colour nearest what's drawn (at full), and stays put in the dark.
            _ if channel.is_color_wheel() => {
                if peak == 0 {
                    channel.value
                } else {
                    let drawn = cell_rgb(0).map(|v| i32::from(scale(v, (255, u32::from(peak)))));
                    nearest_slot(&channel.slots, |s| {
                        let c = s.color.map_or([0; 3], |c| [c.r, c.g, c.b].map(i32::from));
                        (0..3).map(|k| (c[k] - drawn[k]).pow(2)).sum::<i32>()
                    })
                    .map_or(channel.value, |s| s.value)
                }
            }
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

/// Whether the fixture's light shows at `at` with its channels at `dmx`: its first strobe
/// channel's shutter is open, or flashing then (see `shutter.rs`). Without a strobe channel it
/// always shows.
pub fn shutter_open(channels: &[DmxChannel], dmx: &[u8], at: Moment) -> bool {
    channels
        .iter()
        .enumerate()
        .find_map(|(i, c)| {
            c.shutter()
                .map(|ranges| (ranges, dmx.get(i).copied().unwrap_or(0)))
        })
        .is_none_or(|(ranges, value)| shutter_shows(ranges, value, at))
}

/// The colour each cell shows for the fixture's `dmx` channel values, three bytes a cell, into
/// `cells`, with its shutter open. Allocation-free.
pub fn emitted_colors(channels: &[DmxChannel], dmx: &[u8], cells: &mut [u8]) {
    let value = |i: usize| dmx.get(i).copied().unwrap_or(0);
    let dimmer = channels.iter().position(|c| c.role == DmxRole::Dimmer).map(value);
    // Servos with no dimmer make no light of their own.
    let dark = dimmer.is_none() && channels.iter().any(|c| c.role == DmxRole::Servo);
    for (cell, rgb) in cells.as_chunks_mut::<3>().0.iter_mut().enumerate() {
        let cell = cell as u32;
        let (mut r, mut g, mut b, mut lit) = (0u32, 0u32, 0u32, false);
        // A lamp with colour taken out of it: white, less what its cyan, magenta and yellow hold.
        let (mut cmy, mut taken) = (false, [0u32; 3]);
        for (i, c) in channels.iter().enumerate() {
            let v = u32::from(value(i));
            let mine = c.cell == cell;
            match c.role {
                DmxRole::Red if mine => (r, lit) = (r + v, true),
                DmxRole::Green if mine => (g, lit) = (g + v, true),
                DmxRole::Blue if mine => (b, lit) = (b + v, true),
                DmxRole::White if mine => (r, g, b, lit) = (r + v, g + v, b + v, true),
                DmxRole::Amber if mine => (r, g, lit) = (r + v, g + u32::from(half(v as u8)), true),
                DmxRole::Cyan if mine => (cmy, taken[0]) = (true, v),
                DmxRole::Magenta if mine => (cmy, taken[1]) = (true, v),
                DmxRole::Yellow if mine => (cmy, taken[2]) = (true, v),
                // Ultraviolet glows a faint violet on what it lights.
                DmxRole::Uv => (r, b, lit) = (r + v / 4, b + v / 2, true),
                // A colour wheel shows the colour of the slot nearest its value.
                _ if c.is_color_wheel() => {
                    let slot = nearest_slot(&c.slots, |s| (i32::from(s.value) - v as i32).abs());
                    if let Some(color) = slot.and_then(|s| s.color) {
                        (r, g, b, lit) = (
                            r + u32::from(color.r),
                            g + u32::from(color.g),
                            b + u32::from(color.b),
                            true,
                        );
                    }
                }
                _ => {}
            }
        }
        if cmy {
            (r, g, b, lit) = (r + 255 - taken[0], g + 255 - taken[1], b + 255 - taken[2], true);
        }
        let level = u32::from(dimmer.unwrap_or(255));
        let (r, g, b) = match (lit, dark) {
            (true, _) => (r, g, b),
            (false, true) => (0, 0, 0),
            (false, false) => (255, 255, 255),
        };
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
            beam: None,
        };
        assert!(empty.problem(1).is_some());
        let mut head = DmxFixture::from_layout(dmx_layout("head-rgbw-16").unwrap(), 1, 1);
        assert_eq!(head.problem(1), None);
        head.channels[0].motor.as_mut().unwrap().range_deg = 0.0;
        assert!(head.problem(1).unwrap().contains("'Pan'"));
        let mut head = DmxFixture::from_layout(dmx_layout("head-rgbw-16").unwrap(), 1, 1);
        head.beam = Some(DmxBeam {
            angle_deg: 0.0,
            length: 6.0,
        });
        assert!(head.problem(1).unwrap().contains("1° to 120°"));
        let mut spot = DmxFixture::from_layout(dmx_layout("head-spot-16").unwrap(), 1, 1);
        spot.channels[7].slots = (0..65)
            .map(|v| DmxSlot {
                value: v,
                color: None,
                name: None,
            })
            .collect();
        assert!(spot.problem(1).unwrap().contains("at most 64"));
    }

    #[test]
    fn library_heads_rest_at_home_with_a_beam_and_servos_one_a_cell() {
        let head = DmxFixture::from_layout(dmx_layout("head-rgbw-16").unwrap(), 1, 1);
        assert!(head.is_head());
        assert_eq!(head.beam, Some(DmxBeam::default()));
        let held: Vec<u8> = head.channels.iter().map(|c| c.value).collect();
        assert_eq!(
            held,
            vec![128, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0],
            "pan and tilt at Home"
        );
        let axes = head.axes();
        assert_eq!(axes.len(), 2);
        assert_eq!(
            (axes[0].role, axes[0].coarse, axes[0].fine),
            (DmxRole::Pan, 0, Some(1))
        );
        assert_eq!(
            (axes[1].role, axes[1].coarse, axes[1].fine),
            (DmxRole::Tilt, 2, Some(3))
        );
        assert_eq!(axes[0].motor, DmxMotor::centred(540.0));
        assert_eq!(axes[1].motor, DmxMotor::centred(270.0));
        // Both fine channels after both coarse ones: still each its own motor's.
        let split = DmxFixture::from_layout(dmx_layout("head-rgbw-16-split").unwrap(), 1, 1);
        let axes = split.axes();
        assert_eq!((axes[0].coarse, axes[0].fine), (0, Some(2)));
        assert_eq!((axes[1].coarse, axes[1].fine), (1, Some(3)));
        assert_eq!(axes[0].values(90.0), (170, Some(170)));
        let eight = DmxFixture::from_layout(dmx_layout("head-rgbw-8").unwrap(), 1, 1);
        assert_eq!(eight.axes()[0].fine, None);
        assert_eq!(eight.axes()[0].values(90.0), (170, None));

        let servos = DmxFixture::from_layout(dmx_layout("servo-16").unwrap(), 3, 1);
        assert!(!servos.is_head());
        assert_eq!(servos.beam, None);
        assert_eq!(servos.channels.len(), 6);
        assert_eq!(servos.channels[2].name, "Servo 2");
        assert_eq!(servos.channels[3].name, "Servo fine 2");
        let axes = servos.axes();
        assert_eq!(axes.len(), 3);
        assert_eq!((axes[1].coarse, axes[1].fine), (2, Some(3)));
        assert_eq!(servos.channels[2].value, 128, "at Home");
        // A pan channel set up by hand, with no motor written: the usual pan.
        let bare = DmxChannel {
            motor: None,
            ..DmxChannel::new(DmxRole::Pan)
        };
        assert_eq!(bare.motor(), Some(DmxMotor::centred(540.0)));
        assert_eq!(DmxChannel::new(DmxRole::Red).motor(), None);
    }

    #[test]
    fn cyan_magenta_and_yellow_are_what_the_colour_leaves_out() {
        let channels = vec![
            DmxChannel::new(DmxRole::Dimmer),
            DmxChannel::new(DmxRole::Cyan),
            DmxChannel::new(DmxRole::Magenta),
            DmxChannel::new(DmxRole::Yellow),
        ];
        let mut out = [0; 4];
        // Orange at half: dimmer half; the colour at full takes some green and all blue out.
        write_channels(&channels, &[128, 64, 0], &mut out);
        assert_eq!(out, [128, 0, 127, 255]);
        let mut back = [0; 3];
        emitted_colors(&channels, &out, &mut back);
        assert_eq!(back, [128, 64, 0]);
    }

    #[test]
    fn a_colour_wheel_turns_to_the_nearest_colour_and_keeps_still_in_the_dark() {
        let slot = |value: u8, hex: &str| DmxSlot {
            value,
            color: crate::Rgb::from_hex(hex),
            name: None,
        };
        let mut wheel = DmxChannel::new(DmxRole::ColorWheel).holding(40);
        wheel.slots = vec![
            slot(0, "#ffffff"),
            slot(20, "#ff0000"),
            slot(40, "#00ff00"),
            slot(60, "#0000ff"),
        ];
        let channels = vec![DmxChannel::new(DmxRole::Dimmer), wheel];
        let mut out = [0; 2];
        write_channels(&channels, &[100, 10, 0], &mut out);
        assert_eq!(
            out,
            [100, 20],
            "dim red: the red slot, the dimmer at its brightness"
        );
        write_channels(&channels, &[0, 0, 0], &mut out);
        assert_eq!(out, [0, 40], "dark: the wheel stays where it is");
        let mut back = [0; 3];
        emitted_colors(&channels, &[255, 23], &mut back);
        assert_eq!(back, [255, 0, 0], "the slot nearest 23 is red");
        emitted_colors(&channels, &[128, 55], &mut back);
        assert_eq!(back, [0, 0, 128]);
        // Without colours listed it's a held channel, and the head shows white.
        let plain = vec![
            DmxChannel::new(DmxRole::Dimmer),
            DmxChannel::new(DmxRole::ColorWheel).holding(9),
        ];
        assert!(!plain[1].from_color());
        write_channels(&plain, &[255, 0, 0], &mut out);
        assert_eq!(out, [255, 9]);
        emitted_colors(&plain, &out, &mut back);
        assert_eq!(back, [255, 255, 255]);
    }

    #[test]
    fn servos_make_no_light_and_hold_their_position() {
        let servos = layout("servo-8", 2);
        let mut out = [0; 2];
        write_channels(&servos, &[255, 255, 255, 255, 255, 255], &mut out);
        assert_eq!(out, [128, 128], "they rest at Home, whatever the colour");
        let mut cells = [9; 6];
        emitted_colors(&servos, &out, &mut cells);
        assert_eq!(cells, [0; 6]);
    }

    #[test]
    fn a_fixture_shows_its_light_while_its_shutter_is_open_or_flashing() {
        use crate::shutter::{DmxShutter, DmxShutterRange};
        let at = Moment::new(25, 25);
        // A library flood: its strobe holds 0, open, as most fixtures' strobe channels are.
        let flood = layout("dimmer-rgb-strobe", 1);
        assert!(shutter_open(&flood, &[255, 255, 0, 0, 0], at));
        assert!(
            !shutter_open(&flood, &[255, 255, 0, 0, 255], at),
            "strobing, between flashes"
        );
        assert!(
            shutter_open(&flood, &[255, 255, 0, 0, 255], Moment::new(0, 25)),
            "a flash"
        );
        // A fixture's own ranges: closed below 128.
        let mut head = flood.clone();
        head[4].shutter = vec![
            DmxShutterRange {
                from: 0,
                does: DmxShutter::Closed,
            },
            DmxShutterRange {
                from: 128,
                does: DmxShutter::Open,
            },
        ];
        assert!(!shutter_open(&head, &[255, 255, 0, 0, 0], at));
        assert!(shutter_open(&head, &[255, 255, 0, 0, 200], at));
        // Without a strobe channel, always.
        assert!(shutter_open(&layout("rgb", 1), &[0, 0, 0], at));
        // Ranges out of order can't be used.
        let mut fixture = DmxFixture::from_layout(dmx_layout("dimmer-rgb-strobe").unwrap(), 1, 1);
        fixture.channels[4].shutter = vec![
            DmxShutterRange {
                from: 9,
                does: DmxShutter::Closed,
            },
            DmxShutterRange {
                from: 3,
                does: DmxShutter::Open,
            },
        ];
        assert!(fixture.problem(1).unwrap().contains("Strobe"));
    }

    #[test]
    fn a_zoom_channel_holds_its_value_and_widens_the_beam() {
        let zoom = DmxChannel::new(DmxRole::Zoom).holding(40);
        assert_eq!(zoom.zoom(), Some(DmxZoom::default()));
        assert_eq!(DmxChannel::new(DmxRole::Strobe).zoom(), None);
        let channels = vec![DmxChannel::new(DmxRole::Red), zoom];
        let mut out = [0; 2];
        write_channels(&channels, &[255, 0, 0], &mut out);
        assert_eq!(out, [255, 40]);
        let mut fixture = DmxFixture {
            address: 1,
            channels,
            layout: None,
            beam: None,
        };
        fixture.channels[1].zoom = Some(DmxZoom {
            from_deg: 0.5,
            to_deg: 30.0,
        });
        assert!(fixture.problem(1).unwrap().contains("Zoom"));
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
