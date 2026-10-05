//! What the UI sees after every change.

use pf_mapping::ChannelMap;
use pf_model::{Issue, Show};
use serde::Serialize;

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
}
