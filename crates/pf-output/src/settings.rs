//! Output settings that are not part of the show file.

use uuid::Uuid;

/// Runtime sACN identity and synchronization settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputSettings {
    /// sACN source name shown by receivers (at most 63 bytes are sent).
    pub source_name: String,
    /// sACN priority, 0–200 (receivers prefer the highest).
    pub priority: u8,
    /// When set, data packets carry this sync universe and a sync packet follows each frame.
    pub sync_universe: Option<u16>,
    /// sACN component identifier; stable for the life of one output session.
    pub cid: [u8; 16],
}

impl Default for OutputSettings {
    fn default() -> Self {
        Self {
            source_name: "PixelFlow".to_string(),
            priority: 100,
            sync_universe: None,
            cid: *Uuid::new_v4().as_bytes(),
        }
    }
}
