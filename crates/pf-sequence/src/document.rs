//! The sequence document: rows of layered effects, and timing tracks.

use crate::{Effect, EffectId, RowId, TimingTrackId};
use pf_model::{GroupId, PropId, RegionId};
use serde::{Deserialize, Serialize};

/// Schema version written by this build.
///
/// Policy (same as the show file): bump this for **every** change to the sequence file format,
/// even an additive one, so an older PixelFlow refuses a newer file instead of silently dropping
/// what it doesn't know on save. Add the migration in `io.rs` in the same change.
///
/// History: 1 = initial format; 2 = rows can target a submodel (`{ "region": { "prop", "region" } }`).
pub const CURRENT_SCHEMA_VERSION: u32 = 2;

fn default_frame_ms() -> u32 {
    25
}

/// An authored sequence: effects on the show's props and groups, timed to music.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sequence {
    pub schema_version: u32,
    pub name: String,
    /// The music file the sequence is timed to.
    #[serde(default)]
    pub audio: Option<String>,
    pub duration_ms: u64,
    /// Time between frames (25 ms = 40 frames per second).
    #[serde(default = "default_frame_ms")]
    pub frame_ms: u32,
    #[serde(default)]
    pub timing_tracks: Vec<TimingTrack>,
    /// Drawn in order: a later row covers an earlier one where both light the same pixels.
    #[serde(default)]
    pub rows: Vec<Row>,
}

impl Sequence {
    /// An empty sequence at the current schema version, 40 frames per second.
    pub fn new(name: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            name: name.into(),
            audio: None,
            duration_ms,
            frame_ms: default_frame_ms(),
            timing_tracks: Vec::new(),
            rows: Vec::new(),
        }
    }

    /// Frames needed to cover the whole duration (the last one may be partial).
    pub fn frame_count(&self) -> u64 {
        self.duration_ms.div_ceil(u64::from(self.frame_ms.max(1)))
    }

    /// Every effect in the sequence.
    pub fn effects(&self) -> impl Iterator<Item = &Effect> {
        self.rows.iter().flat_map(|r| &r.layers).flat_map(|l| &l.effects)
    }

    /// Pulls every effect setting into the range its kind allows (see
    /// [`crate::EffectParams::sanitize`]). Opening a file does this, so a value JSON can't hold
    /// (an overflowing number reads as infinity) never makes a file impossible to save and reopen.
    pub fn sanitize_settings(&mut self) {
        for effect in self
            .rows
            .iter_mut()
            .flat_map(|r| &mut r.layers)
            .flat_map(|l| &mut l.effects)
        {
            effect.params.sanitize();
        }
    }

    pub fn effect_count(&self) -> usize {
        self.rows
            .iter()
            .flat_map(|r| &r.layers)
            .map(|l| l.effects.len())
            .sum()
    }

    pub fn row(&self, id: RowId) -> Option<&Row> {
        self.rows.iter().find(|r| r.id == id)
    }

    pub fn row_mut(&mut self, id: RowId) -> Option<&mut Row> {
        self.rows.iter_mut().find(|r| r.id == id)
    }

    /// Where an effect lives: (row index, layer index, effect index).
    pub fn locate_effect(&self, id: EffectId) -> Option<(usize, usize, usize)> {
        self.rows.iter().enumerate().find_map(|(r, row)| {
            row.layers
                .iter()
                .enumerate()
                .find_map(|(l, layer)| layer.effects.iter().position(|e| e.id == id).map(|e| (r, l, e)))
        })
    }

    pub fn effect(&self, id: EffectId) -> Option<&Effect> {
        let (r, l, e) = self.locate_effect(id)?;
        Some(&self.rows[r].layers[l].effects[e])
    }

    pub fn timing_track(&self, id: TimingTrackId) -> Option<&TimingTrack> {
        self.timing_tracks.iter().find(|t| t.id == id)
    }
}

/// What a row lights: one prop, a group of props treated as one canvas, or one of a prop's
/// submodels (or faces).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Target {
    Prop(PropId),
    Group(GroupId),
    Region { prop: PropId, region: RegionId },
}

impl Target {
    /// The prop a prop or submodel row belongs to (`None` for a group).
    pub fn prop(self) -> Option<PropId> {
        match self {
            Target::Prop(id) | Target::Region { prop: id, .. } => Some(id),
            Target::Group(_) => None,
        }
    }
}

/// A row on the timeline: layers of effects on one target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: RowId,
    pub target: Target,
    /// Drawn bottom (first, index 0) to top (last); the opposite of xLights' numbering. Each
    /// effect's [`crate::Blend`] mixes with the layers below it on this row only.
    #[serde(default)]
    pub layers: Vec<Layer>,
}

impl Row {
    /// A row with one empty layer.
    pub fn new(target: Target) -> Self {
        Self {
            id: RowId::new(),
            target,
            layers: vec![Layer::default()],
        }
    }
}

/// Effects that take turns on a row (they shouldn't overlap in time).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    #[serde(default)]
    pub effects: Vec<Effect>,
}

/// What a timing track marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimingKind {
    Beats,
    Bars,
    Sections,
    Lyrics,
    Words,
    Phonemes,
    Custom,
}

/// Marks in time (beats, bars, lyric lines) to line effects up with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingTrack {
    pub id: TimingTrackId,
    pub name: String,
    pub kind: TimingKind,
    #[serde(default)]
    pub marks: Vec<Mark>,
}

impl TimingTrack {
    pub fn new(name: impl Into<String>, kind: TimingKind, marks: Vec<Mark>) -> Self {
        Self {
            id: TimingTrackId::new(),
            name: name.into(),
            kind,
            marks,
        }
    }
}

/// One mark: a span of time with an optional label (a lyric, a bar number).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mark {
    pub start_ms: u64,
    pub end_ms: u64,
    #[serde(default)]
    pub label: String,
}

impl Mark {
    pub fn new(start_ms: u64, end_ms: u64, label: impl Into<String>) -> Self {
        Self {
            start_ms,
            end_ms,
            label: label.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EffectKind;

    #[test]
    fn targets_serialize_as_kind_and_id() {
        let id = PropId(uuid::Uuid::nil());
        let json = serde_json::to_value(Target::Prop(id)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "prop": "00000000-0000-0000-0000-000000000000" })
        );
        assert_eq!(serde_json::from_value::<Target>(json).unwrap(), Target::Prop(id));
        let region = Target::Region {
            prop: id,
            region: RegionId(uuid::Uuid::nil()),
        };
        let json = serde_json::to_value(region).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "region": {
                "prop": "00000000-0000-0000-0000-000000000000",
                "region": "00000000-0000-0000-0000-000000000000"
            } })
        );
        assert_eq!(serde_json::from_value::<Target>(json).unwrap(), region);
        assert_eq!(region.prop(), Some(id));
        assert_eq!(Target::Group(GroupId(uuid::Uuid::nil())).prop(), None);
    }

    #[test]
    fn frame_count_covers_the_whole_duration() {
        let mut seq = Sequence::new("s", 1000);
        assert_eq!(seq.frame_count(), 40);
        seq.duration_ms = 1001;
        assert_eq!(seq.frame_count(), 41);
    }

    #[test]
    fn effects_can_be_found_by_id() {
        let mut seq = Sequence::new("s", 1000);
        let mut row = Row::new(Target::Prop(PropId::new()));
        let effect = Effect::new(EffectKind::On, 0, 500);
        let id = effect.id;
        row.layers.push(Layer {
            effects: vec![Effect::new(EffectKind::Off, 0, 10), effect],
        });
        seq.rows.push(row);
        assert_eq!(seq.locate_effect(id), Some((0, 1, 1)));
        assert_eq!(seq.effect(id).unwrap().kind(), EffectKind::On);
        assert_eq!(seq.effect_count(), 2);
        assert!(seq.effect(EffectId::new()).is_none());
    }
}
