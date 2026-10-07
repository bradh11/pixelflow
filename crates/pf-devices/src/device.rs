//! What discovery reports about a device.

use serde::{Deserialize, Serialize};

/// The kinds of controller PixelFlow understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    Fpp,
    Falcon,
    Wled,
}

/// How a device was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FoundBy {
    /// Answered FPP's MultiSync discovery packet.
    Ping,
    /// Its web page was recognized during the network sweep.
    WebSweep,
    /// Advertised itself over mDNS.
    Mdns,
    /// Listed by an FPP as a MultiSync peer or output destination.
    FppPeer,
    /// Entered by the user.
    Manual,
}

/// A controller found on the network.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    /// IP address (or the hostname the user typed).
    pub address: String,
    pub kind: DeviceKind,
    pub name: String,
    pub model: String,
    pub firmware: String,
    /// Operating mode as the device reports it (FPP only, e.g. "player"; not set for other kinds).
    pub mode: Option<String>,
    pub found_by: Vec<FoundBy>,
}
