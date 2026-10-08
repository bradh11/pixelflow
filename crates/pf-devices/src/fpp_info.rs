//! What's stored on an FPP (sequences, music, playlists) and its schedule — read-only.
//!
//! Only these endpoints are read, checked against FPP 9.5.3's PHP:
//! - `/api/files/{sequences,music,playlists}` (`files.php` `GetFiles()`): each file's name, size,
//!   and date, and a music file's play time. It lists one media folder; nothing else.
//! - `/api/sequence/<name>/meta`: a sequence's frames, frame time, and channels.
//! - `/api/playlists` and `/api/playlist/<name>` (`playlist.php`): a playlist's file as saved.
//!   Only its item count and total length are kept.
//! - `/api/schedule` (`schedule.php` `GetSchedule()`): `config/schedule.json` as saved by FPP's
//!   scheduler page, and nothing else (no settings, network, or passwords). Only when, what, and
//!   how each entry stops are kept; a command entry's arguments are dropped unread.

use crate::error::DeviceError;
use crate::fpp::{get_json, int_field, str_field};
use crate::fpp_player::encode_segment;
use crate::fpp_upload::lenient_json;
use crate::http::Http;
use serde::Serialize;
use serde_json::Value;

/// A sequence, music file, or playlist stored on an FPP.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FppFile {
    /// The file's name on the FPP (a sequence's with `.fseq`; a playlist's without `.json`).
    pub name: String,
    pub size_bytes: Option<u64>,
    /// When it was last changed, as `YYYY-MM-DD HH:MM` (the FPP's own clock).
    pub modified: Option<String>,
    pub duration_ms: Option<u64>,
    /// A sequence's channel count.
    pub channels: Option<u32>,
    /// A playlist's item count.
    pub items: Option<u32>,
}

/// Which of the FPP's folders to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FppFolder {
    Sequences,
    Music,
    Playlists,
}

/// FPP's `'m/d/y  h:i A'` file date ("10/06/26  06:05 PM") as `2026-10-06 18:05`.
fn file_date(text: &str) -> Option<String> {
    let mut parts = text.split_whitespace();
    let date: Vec<u32> = parts
        .next()?
        .split('/')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    let (hour, minute) = parts.next()?.split_once(':')?;
    let (hour, minute): (u32, u32) = (hour.parse().ok()?, minute.parse().ok()?);
    let pm = parts.next()?.eq_ignore_ascii_case("PM");
    let [month, day, year] = date[..] else { return None };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || !(1..=12).contains(&hour) || minute > 59 {
        return None;
    }
    let hour = hour % 12 + if pm { 12 } else { 0 };
    Some(format!(
        "{:04}-{month:02}-{day:02} {hour:02}:{minute:02}",
        2000 + year
    ))
}

/// FPP's `human_playtime()` ("03m:45s", "01h:02m:03s"; "Unknown" when it can't tell) in ms.
fn play_time_ms(text: &str) -> Option<u64> {
    let mut total = 0u64;
    let mut any = false;
    for part in text.split(':') {
        let part = part.trim();
        let (number, unit) = part.split_at(part.find(|c: char| !c.is_ascii_digit())?);
        let n: u64 = number.parse().ok()?;
        total += n * match unit {
            "h" => 3600,
            "m" => 60,
            "s" => 1,
            _ => return None,
        };
        any = true;
    }
    any.then_some(total * 1000)
}

fn size_of(file: &Value) -> Option<u64> {
    let size = file.get("sizeBytes")?;
    size.as_u64().or_else(|| size.as_str()?.trim().parse().ok())
}

/// The files `GetFiles()` lists in `folder` (sub-folders left out), by name.
pub(crate) fn listing(http: &dyn Http, host: &str, folder: &str) -> Result<Vec<Value>, DeviceError> {
    let path = format!("/api/files/{folder}");
    let doc = lenient_json(&http.get(host, &path)?);
    let files = doc
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| DeviceError::bad(host, &path, "no file list"))?;
    let mut files: Vec<Value> = files
        .iter()
        .filter(|f| str_field(f, "sizeHuman") != "Directory" && !str_field(f, "name").trim().is_empty())
        .cloned()
        .collect();
    files.sort_by_key(|f| str_field(f, "name").to_lowercase());
    Ok(files)
}

pub(crate) fn base(file: &Value) -> FppFile {
    FppFile {
        name: str_field(file, "name").trim().to_string(),
        size_bytes: size_of(file),
        modified: file_date(str_field(file, "mtime")),
        duration_ms: None,
        channels: None,
        items: None,
    }
}

/// The sequences on the FPP with their length and channels (one request each for those).
pub fn sequences(http: &dyn Http, host: &str) -> Result<Vec<FppFile>, DeviceError> {
    Ok(listing(http, host, "sequences")?
        .iter()
        .filter(|f| str_field(f, "name").to_ascii_lowercase().ends_with(".fseq"))
        .map(|f| {
            let mut file = base(f);
            let stem = &file.name[..file.name.len() - ".fseq".len()];
            if let Ok(meta) = get_json(
                http,
                host,
                &format!("/api/sequence/{}/meta", encode_segment(stem)),
            ) {
                let number = |key| u64::try_from(int_field(&meta, key)).unwrap_or(0);
                let (frames, step) = (number("NumFrames"), number("StepTime"));
                file.duration_ms = (frames > 0 && step > 0).then_some(frames * step);
                file.channels = u32::try_from(number("ChannelCount")).ok().filter(|&c| c > 0);
            }
            file
        })
        .collect())
}

/// The music files on the FPP with their play time, as FPP measures it.
pub fn music(http: &dyn Http, host: &str) -> Result<Vec<FppFile>, DeviceError> {
    Ok(listing(http, host, "music")?
        .iter()
        .map(|f| FppFile {
            duration_ms: play_time_ms(str_field(f, "playtimeSeconds")),
            ..base(f)
        })
        .collect())
}

/// The playlists on the FPP with their item count and length (one request each). Sizes and
/// dates come from the playlist folder's listing, when FPP has it.
pub fn playlists(http: &dyn Http, host: &str) -> Result<Vec<FppFile>, DeviceError> {
    let names = get_json(http, host, "/api/playlists")?;
    let files = listing(http, host, "playlists").unwrap_or_default();
    let mut names: Vec<String> = names
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    names.sort_by_key(|n| n.to_lowercase());
    Ok(names
        .into_iter()
        .map(|name| {
            let listed = files
                .iter()
                .find(|f| str_field(f, "name") == format!("{name}.json"));
            let mut file = listed.map(base).unwrap_or_default();
            file.name = name.clone();
            if let Ok(doc) = get_json(http, host, &format!("/api/playlist/{}", encode_segment(&name))) {
                let info = doc.get("playlistInfo").unwrap_or(&Value::Null);
                let seconds = info
                    .get("total_duration")
                    .and_then(|v| v.as_f64().or_else(|| v.as_str()?.trim().parse().ok()));
                file.duration_ms = seconds.filter(|s| *s > 0.0).map(|s| (s * 1000.0).round() as u64);
                let counted: usize = ["leadIn", "mainPlaylist", "leadOut"]
                    .iter()
                    .filter_map(|section| doc.get(section).and_then(Value::as_array))
                    .map(Vec::len)
                    .sum();
                let listed_items = u32::try_from(int_field(info, "total_items"))
                    .ok()
                    .filter(|&n| n > 0);
                file.items = listed_items.or(u32::try_from(counted).ok());
            }
            file
        })
        .collect())
}

/// Lists one of the FPP's folders (changes nothing).
pub fn list(http: &dyn Http, host: &str, folder: FppFolder) -> Result<Vec<FppFile>, DeviceError> {
    match folder {
        FppFolder::Sequences => sequences(http, host),
        FppFolder::Music => music(http, host),
        FppFolder::Playlists => playlists(http, host),
    }
}

/// What a schedule entry starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ScheduleKind {
    Playlist,
    Sequence,
    /// An FPP command (only its name is kept).
    Command,
}

/// One entry of the FPP's schedule, as FPP's scheduler page saved it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleEntry {
    pub enabled: bool,
    pub kind: ScheduleKind,
    /// The playlist, sequence, or command name.
    pub name: String,
    /// FPP's day code: 0–6 Sunday–Saturday, 7 every day, 8 weekdays, 9 weekends, 10 Mon/Wed/Fri,
    /// 11 Tue/Thu, 12 Sun–Thu, 13 Fri/Sat, 14 odd days, 15 even days, or 0x10000 plus a bit per
    /// day (0x4000 Sunday down to 0x100 Saturday).
    pub day: u32,
    /// `HH:MM:SS`, or SunRise, SunSet, Dawn, Dusk (with the offset in minutes).
    pub start_time: String,
    pub start_offset: i64,
    pub end_time: String,
    pub end_offset: i64,
    /// `YYYY-MM-DD` (year 0000: every year) or a holiday name; empty for no limit.
    pub start_date: String,
    pub end_date: String,
    /// 0 plays once, 1 repeats straight away, otherwise every `repeat / 100` minutes.
    pub repeat: u32,
    /// How it stops at its end time: 0 gracefully, 1 at once, 2 gracefully after the loop.
    pub stop_type: u32,
}

fn bool_field(v: &Value, key: &str) -> bool {
    match v.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(_) => int_field(v, key) != 0,
        None => false,
    }
}

/// Reads the FPP's schedule (changes nothing).
pub fn schedule(http: &dyn Http, host: &str) -> Result<Vec<ScheduleEntry>, DeviceError> {
    let doc = get_json(http, host, "/api/schedule")?;
    // FPP answers [] when there's no schedule file; older versions wrapped it in an object.
    let list = doc
        .as_array()
        .or_else(|| doc.get("entries").and_then(Value::as_array))
        .cloned()
        .unwrap_or_default();
    Ok(list
        .iter()
        .filter(|e| e.is_object())
        .map(|e| {
            let command = str_field(e, "command").trim();
            let (kind, name) = if !command.is_empty() {
                (ScheduleKind::Command, command.to_string())
            } else if bool_field(e, "sequence") {
                (
                    ScheduleKind::Sequence,
                    str_field(e, "playlist").trim().to_string(),
                )
            } else {
                (
                    ScheduleKind::Playlist,
                    str_field(e, "playlist").trim().to_string(),
                )
            };
            let number = |key| u32::try_from(int_field(e, key)).unwrap_or(0);
            ScheduleEntry {
                enabled: bool_field(e, "enabled"),
                kind,
                name,
                day: number("day"),
                start_time: str_field(e, "startTime").trim().to_string(),
                start_offset: int_field(e, "startTimeOffset"),
                end_time: str_field(e, "endTime").trim().to_string(),
                end_offset: int_field(e, "endTimeOffset"),
                start_date: str_field(e, "startDate").trim().to_string(),
                end_date: str_field(e, "endDate").trim().to_string(),
                repeat: number("repeat"),
                stop_type: number("stopType"),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_dates_read_as_24_hour_times() {
        assert_eq!(
            file_date("10/06/26  06:05 PM").as_deref(),
            Some("2026-10-06 18:05")
        );
        assert_eq!(
            file_date("01/02/25  12:00 AM").as_deref(),
            Some("2025-01-02 00:00")
        );
        assert_eq!(
            file_date("01/02/25  12:30 PM").as_deref(),
            Some("2025-01-02 12:30")
        );
        assert_eq!(file_date("yesterday"), None);
        assert_eq!(file_date(""), None);
    }

    #[test]
    fn play_times_read_in_ms() {
        assert_eq!(play_time_ms("03m:45s"), Some(225_000));
        assert_eq!(play_time_ms("01h:02m:03s"), Some(3_723_000));
        assert_eq!(play_time_ms("Unknown"), None);
        assert_eq!(play_time_ms(""), None);
    }
}
