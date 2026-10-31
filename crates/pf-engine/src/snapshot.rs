//! What the UI sees after every change.

use crate::files::MissingFile;
use pf_mapping::ChannelMap;
use pf_model::{Issue, PropId, Show};
use serde::{Deserialize, Serialize};

/// Headline numbers for the show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub props: usize,
    pub pixels: u64,
    pub controllers: usize,
    pub universes: usize,
}

/// The engine's full state, sent to the UI after every change.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowSnapshot {
    /// Increases on every change; lets the UI ignore stale updates.
    pub revision: u64,
    /// Where the show is saved, if it has been.
    pub path: Option<String>,
    /// True when there are changes since the last save or open.
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub show: Show,
    /// Structural and wiring problems, errors first.
    pub issues: Vec<Issue>,
    pub channel_map: ChannelMap,
    pub summary: Summary,
    /// The open sequence's revision, if one is open: undo or redo of a change made to the show
    /// and the sequence together changes it too (the UI then fetches the sequence again).
    pub sequence_revision: Option<u64>,
    /// The show's files (sequences, music, the photo, the house model) that aren't where it
    /// says they are.
    pub missing_files: Vec<MissingFile>,
    /// False until every file the show refers to has been looked at (see
    /// [`crate::Engine::file_check`]): only files looked at can be called missing.
    pub files_checked: bool,
}

/// Where a prop's pixels are drawn in the preview (front view, or 3D) and where their colors sit
/// in a live frame.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewProp {
    pub prop: PropId,
    pub frame_offset: usize,
    pub channels_per_pixel: u8,
    /// x, y pairs (x, y, z triples in the 3D preview), one per pixel, in wiring order.
    pub points: Vec<f32>,
    /// A DMX flood: each of its pixels (cells) is drawn as a wash of its color.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flood: bool,
    /// A moving head's beam or servos' arms, pointed by channels in the frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<PreviewMotion>,
}

/// How a moving head or servos move, for the previews: where their motors' values are in a live
/// frame, and how each turns. A head's beam starts at its first cell; servo `k` turns at cell `k`
/// (the last cell for any beyond).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewMotion {
    /// The fixture's own frame in the layout (see `pf_model::Basis`): the prop's rotation, its
    /// numbers widened from the engine's so the window reads exactly the same ones.
    pub basis: pf_model::Basis,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beam: Option<PreviewBeam>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servos: Vec<PreviewAxis>,
}

/// A moving head's beam: its pan and tilt (each left out when the head has none), its width in
/// degrees (or its zoom channel's, when it has one) and how far it is drawn, in layout units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewBeam {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan: Option<PreviewAxis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tilt: Option<PreviewAxis>,
    pub angle_deg: f64,
    pub length: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<PreviewZoom>,
}

impl PreviewBeam {
    /// How wide the beam is, in degrees, with the frame `frame` (`None`: at rest).
    pub fn width(&self, frame: Option<&[u8]>) -> f64 {
        self.zoom.as_ref().map_or(self.angle_deg, |z| z.angle(frame))
    }
}

/// A zoom channel in a live frame: where its value is, the value it rests at, and the beam's
/// width at value 0 and at 255 (in degrees, widened from the engine's so the window reads exactly
/// the same numbers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewZoom {
    pub at: usize,
    pub rest: u8,
    pub from_deg: f64,
    pub to_deg: f64,
}

impl PreviewZoom {
    /// How wide it makes the beam, in degrees, with the frame `frame` (`None`: at rest).
    pub fn angle(&self, frame: Option<&[u8]>) -> f64 {
        let value = frame.map_or(self.rest, |f| f.get(self.at).copied().unwrap_or(0));
        self.from_deg + (self.to_deg - self.from_deg) * f64::from(value) / 255.0
    }
}

/// One motor in a live frame: where its value is (and its fine value, for a 16-bit motor), the
/// values it rests at when nothing plays, and how it turns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewAxis {
    pub at: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fine_at: Option<usize>,
    pub rest: [u8; 2],
    pub motor: pf_model::DmxMotor,
}

impl PreviewAxis {
    /// Where it points, in degrees from Home, with the frame `frame` (`None`: at rest).
    pub fn degrees(&self, frame: Option<&[u8]>) -> f64 {
        let at = |i: usize, rest: u8| frame.map_or(rest, |f| f.get(i).copied().unwrap_or(0));
        let coarse = at(self.at, self.rest[0]);
        let fine = self.fine_at.map(|i| at(i, self.rest[1]));
        self.motor.degrees(coarse, fine)
    }
}

impl PreviewProp {
    /// A prop's preview entry: its pixels' `points` and, from the channel map, where its colors
    /// are and how a moving head or servos move.
    pub fn of(prop: &pf_model::Prop, map: &pf_mapping::ChannelMap, points: Vec<f32>) -> Option<Self> {
        let layout = map.prop_layout(prop.id)?;
        let motion = prop
            .dmx
            .as_ref()
            .zip(map.fixture(prop.id))
            .and_then(|(dmx, fixture)| motion_of(prop, dmx, fixture));
        Some(Self {
            prop: prop.id,
            frame_offset: layout.frame_offset,
            channels_per_pixel: layout.channels_per_pixel,
            points,
            flood: prop.dmx.is_some() && motion.is_none(),
            motion,
        })
    }
}

/// How a fixture moves in the previews, if it has motors.
fn motion_of(
    prop: &pf_model::Prop,
    dmx: &pf_model::DmxFixture,
    fixture: &pf_mapping::FixtureLayout,
) -> Option<PreviewMotion> {
    let axes = dmx.axes();
    if axes.is_empty() {
        return None;
    }
    let held = |i: usize| dmx.channels.get(i).map_or(0, |c| c.value);
    let preview = |axis: &pf_model::DmxAxis| PreviewAxis {
        at: fixture.dmx_offset + axis.coarse,
        fine_at: axis.fine.map(|f| fixture.dmx_offset + f),
        rest: [held(axis.coarse), axis.fine.map_or(0, held)],
        motor: axis.motor,
    };
    let of_role = |role: pf_model::DmxRole| axes.iter().find(|a| a.role == role).map(preview);
    let zoom = dmx.channels.iter().enumerate().find_map(|(i, c)| {
        c.zoom().map(|z| PreviewZoom {
            at: fixture.dmx_offset + i,
            rest: c.value,
            from_deg: f64::from(z.from_deg),
            to_deg: f64::from(z.to_deg),
        })
    });
    let beam = dmx.beam().map(|b| PreviewBeam {
        pan: of_role(pf_model::DmxRole::Pan),
        tilt: of_role(pf_model::DmxRole::Tilt),
        angle_deg: f64::from(b.angle_deg),
        length: f64::from(b.length),
        zoom,
    });
    Some(PreviewMotion {
        basis: pf_geometry::rotation_basis(&prop.transform).map(f64::from),
        beam,
        servos: axes
            .iter()
            .filter(|a| a.role == pf_model::DmxRole::Servo)
            .map(preview)
            .collect(),
    })
}
