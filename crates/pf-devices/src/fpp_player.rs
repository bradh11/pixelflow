//! What an FPP is playing, the sequences stored on it, and playback control.
//!
//! Reading uses only `/api/fppd/status`, `/api/sequence`, and `/api/sequence/<name>/meta` (no
//! credentials in any of them). Control functions ([`start`], [`stop`]) send FPP commands and
//! change what the FPP is doing; call them only when the user asks.

use crate::error::DeviceError;
use crate::fpp::{get_json, int_field, str_field};
use crate::http::Http;
use serde::Serialize;
use serde_json::Value;

/// What the player is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayerState {
    Idle,
    Playing,
    Paused,
    Stopping,
    /// Anything else FPP reports (testing mode, for example).
    Other,
}

/// An FPP's playback status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatus {
    pub state: PlayerState,
    pub playlist: Option<String>,
    pub sequence: Option<String>,
    pub seconds_elapsed: u32,
    pub seconds_remaining: u32,
    /// The next scheduled playlist and when it starts, as FPP describes it.
    pub next_playlist: Option<String>,
    pub next_start: Option<String>,
    /// Problems FPP itself reports, such as an output target it can't reach.
    pub warnings: Vec<String>,
}

/// A sequence stored on an FPP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FppSequence {
    pub name: String,
    pub frames: u32,
    pub step_ms: u32,
    pub channels: u32,
}

impl FppSequence {
    pub fn duration_ms(&self) -> u64 {
        u64::from(self.frames) * u64::from(self.step_ms)
    }
}

fn non_empty(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn seconds(v: &Value, key: &str) -> u32 {
    u32::try_from(int_field(v, key).max(0)).unwrap_or(u32::MAX)
}

/// Reads the FPP's playback status.
pub fn status(http: &dyn Http, host: &str) -> Result<PlayerStatus, DeviceError> {
    let doc = get_json(http, host, "/api/fppd/status")?;
    let state = match str_field(&doc, "status_name") {
        "idle" => PlayerState::Idle,
        "playing" => PlayerState::Playing,
        "paused" => PlayerState::Paused,
        name if name.starts_with("stopping") => PlayerState::Stopping,
        _ => PlayerState::Other,
    };
    let next = doc.get("next_playlist").unwrap_or(&Value::Null);
    // FPP fills the next playlist with a sentence when nothing is scheduled.
    let next_playlist = non_empty(str_field(next, "playlist")).filter(|p| !p.starts_with("No playlist"));
    let warnings = doc
        .get("warnings")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .filter_map(non_empty)
                .collect()
        })
        .unwrap_or_default();
    Ok(PlayerStatus {
        state,
        playlist: doc
            .get("current_playlist")
            .and_then(|p| non_empty(str_field(p, "playlist"))),
        sequence: non_empty(str_field(&doc, "current_sequence")),
        seconds_elapsed: seconds(&doc, "seconds_elapsed"),
        seconds_remaining: seconds(&doc, "seconds_remaining"),
        next_start: next_playlist
            .as_ref()
            .and_then(|_| non_empty(str_field(next, "start_time"))),
        next_playlist,
        warnings,
    })
}

/// Percent-encodes one URL path segment.
pub(crate) fn encode_segment(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Lists the sequences stored on the FPP. A sequence whose details can't be read is still
/// listed, with zero frames.
pub fn sequences(http: &dyn Http, host: &str) -> Result<Vec<FppSequence>, DeviceError> {
    let names = get_json(http, host, "/api/sequence")?;
    let names: Vec<String> = names
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .filter_map(non_empty)
                .collect()
        })
        .unwrap_or_default();
    Ok(names
        .into_iter()
        .map(|name| {
            let meta = get_json(
                http,
                host,
                &format!("/api/sequence/{}/meta", encode_segment(&name)),
            )
            .unwrap_or(Value::Null);
            let number = |key| u32::try_from(int_field(&meta, key).max(0)).unwrap_or(0);
            FppSequence {
                frames: number("NumFrames"),
                step_ms: number("StepTime"),
                channels: number("ChannelCount"),
                name,
            }
        })
        .collect())
}

#[derive(Serialize)]
struct Command<'a> {
    command: &'a str,
    args: &'a [&'a str],
}

fn command(http: &dyn Http, host: &str, command: &str, args: &[&str]) -> Result<(), DeviceError> {
    let body = serde_json::to_string(&Command { command, args }).expect("strings always serialize");
    http.post_json(host, "/api/command", &body).map(|_| ())
}

/// Starts a playlist or sequence (by FPP name, e.g. `"Christmas Medley 2017.fseq"`) once.
/// Changes what the FPP is doing.
pub fn start(http: &dyn Http, host: &str, name: &str) -> Result<(), DeviceError> {
    command(http, host, "Start Playlist", &[name, "false", "false"])
}

/// Stops playback now, or (`gracefully`) at the end of the current sequence.
/// Changes what the FPP is doing.
pub fn stop(http: &dyn Http, host: &str, gracefully: bool) -> Result<(), DeviceError> {
    if gracefully {
        command(http, host, "Stop Gracefully", &["false"])
    } else {
        command(http, host, "Stop Now", &[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_are_percent_encoded() {
        assert_eq!(
            encode_segment("Christmas Medley 2017"),
            "Christmas%20Medley%202017"
        );
        assert_eq!(encode_segment("a/b?c#d&é"), "a%2Fb%3Fc%23d%26%C3%A9");
        assert_eq!(encode_segment("Show_1.fseq"), "Show_1.fseq");
    }
}
