//! The top-level show document.

use crate::{Controller, ControllerId, Group, Prop, PropId, SequenceId};
use serde::{Deserialize, Serialize};

/// Schema version written by this build.
///
/// Policy: bump this for **every** change to the show file format, even an additive one with
/// a no-op migration, so an older PixelFlow refuses a newer file instead of silently dropping
/// fields it doesn't know on save. Add the migration in `io.rs` in the same change.
///
/// History: 1 = initial format; 2 = adds the `falcon` controller adapter; 3 = adds a
/// controller's `sequenceChannels`; 4 = adds the show's `sequences`.
pub const CURRENT_SCHEMA_VERSION: u32 = 4;

/// Show-wide settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowSettings {
    /// Output frames per second (20–100).
    #[serde(default = "default_frame_rate")]
    pub frame_rate: u16,
}

fn default_frame_rate() -> u16 {
    40
}

impl Default for ShowSettings {
    fn default() -> Self {
        Self {
            frame_rate: default_frame_rate(),
        }
    }
}

/// A complete show: layout, groups, and controller wiring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Show {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub settings: ShowSettings,
    #[serde(default)]
    pub props: Vec<Prop>,
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub controllers: Vec<Controller>,
    /// Rendered sequences in the show, in playlist order.
    #[serde(default)]
    pub sequences: Vec<SequenceEntry>,
}

/// A rendered sequence (`.fseq`) in the show, with its music.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceEntry {
    pub id: SequenceId,
    pub name: String,
    /// The `.fseq` file.
    pub path: String,
    /// The music file, when the sequence has one.
    #[serde(default)]
    pub audio: Option<String>,
    /// How far the lights run ahead of the music, in milliseconds (negative: behind).
    #[serde(default)]
    pub offset_ms: i32,
}

impl SequenceEntry {
    pub fn new(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            id: SequenceId::new(),
            name: name.into(),
            path: path.into(),
            audio: None,
            offset_ms: 0,
        }
    }
}

impl Show {
    /// An empty show at the current schema version.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            name: name.into(),
            settings: ShowSettings::default(),
            props: Vec::new(),
            groups: Vec::new(),
            controllers: Vec::new(),
            sequences: Vec::new(),
        }
    }

    pub fn prop(&self, id: PropId) -> Option<&Prop> {
        self.props.iter().find(|p| p.id == id)
    }

    pub fn controller(&self, id: ControllerId) -> Option<&Controller> {
        self.controllers.iter().find(|c| c.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Generator, ShapeSource};

    #[test]
    fn new_show_is_empty_at_current_version() {
        let show = Show::new("Demo");
        assert_eq!(show.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(show.settings.frame_rate, 40);
        assert!(show.props.is_empty());
    }

    #[test]
    fn lookup_by_id() {
        let mut show = Show::new("Demo");
        let prop = Prop::new(
            "Line",
            ShapeSource::Generator(Generator::Line {
                nodes: 5,
                length: 1.0,
            }),
        );
        let id = prop.id;
        show.props.push(prop);
        assert_eq!(show.prop(id).unwrap().name, "Line");
        assert!(show.prop(PropId::new()).is_none());
    }
}
