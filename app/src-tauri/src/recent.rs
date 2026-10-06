//! Shows opened lately, for the start page, the show menu, and File → Open Recent.
//!
//! The list lives in `recent.json` in the app's config folder, with a small picture of each
//! show's layout beside it (in `recent-thumbs/`). Only the shell writes it, and only when a show
//! has really been opened, saved, or restored: the window can read the list, take shows off it,
//! or clear it, but never put a path on it. Opening a show from the list goes through
//! `open_show` like any other open.

use crate::pickers::{Pick, PickKind};
use crate::{AppState, Reply};
use pf_engine::ShowSnapshot;
use pf_model::{Show, path_from_text, path_to_text};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Runtime, State};

/// How many shows the list keeps.
pub(crate) const RECENT_LIMIT: usize = 10;
/// How long the list waits to hear whether its shows are still there (a dead network drive
/// can take far longer; those shows are then shown as they are, not as missing).
const PRESENCE_WAIT: Duration = Duration::from_millis(800);
const LIST_FILE: &str = "recent.json";
const THUMBS: &str = "recent-thumbs";
/// Thumbnails larger than this are ignored (the shell only writes small ones).
const MAX_THUMB_BYTES: u64 = 64 * 1024;
/// A list file larger than this isn't one the shell wrote, and is read as empty.
pub(crate) const MAX_LIST_BYTES: u64 = 256 * 1024;
/// At most this many dots are drawn in a thumbnail.
const MAX_DOTS: usize = 1500;
/// The thumbnail's size, in its own units.
const THUMB_W: f32 = 320.0;
const THUMB_H: f32 = 200.0;
const THUMB_PAD: f32 = 12.0;

/// One show as `recent.json` keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    /// The show file, as path text (see `pf_model::path_to_text`: nothing is lost).
    path: String,
    name: String,
    /// When it was last opened or saved, in milliseconds since 1970.
    opened_at: u64,
    #[serde(default)]
    props: usize,
    #[serde(default)]
    pixels: u64,
    #[serde(default)]
    controllers: usize,
    /// The file name of its thumbnail in `recent-thumbs/`.
    #[serde(default)]
    thumbnail: Option<String>,
}

/// Whether a recent show's file is still where the list says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Presence {
    Here,
    Missing,
    /// Its drive didn't answer in time.
    Unknown,
}

/// A recent show, for the window.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentShow {
    pub path: String,
    pub name: String,
    pub opened_at: u64,
    pub props: usize,
    pub pixels: u64,
    pub controllers: usize,
    /// A small SVG picture of the layout, when there is one.
    pub thumbnail: Option<String>,
    pub status: Presence,
}

/// What to remember about a show just opened or saved.
pub(crate) struct Visit {
    pub path: PathBuf,
    pub name: String,
    pub props: usize,
    pub pixels: u64,
    pub controllers: usize,
    /// Front-view pixel positions (x, y pairs) for the thumbnail.
    pub points: Vec<f32>,
}

impl Visit {
    /// The visit for a saved show's snapshot; `None` when it has no file.
    pub(crate) fn of(snapshot: &ShowSnapshot) -> Option<Self> {
        let path = path_from_text(snapshot.path.as_deref()?);
        Some(Self {
            path,
            name: snapshot.show.name.clone(),
            props: snapshot.summary.props,
            pixels: snapshot.summary.pixels,
            controllers: snapshot.summary.controllers,
            points: front_view(&snapshot.show),
        })
    }
}

/// Every pixel's front-view position (x, y pairs), at most about `MAX_DOTS * 4` of them.
fn front_view(show: &Show) -> Vec<f32> {
    let total: usize = show.props.iter().map(|p| p.node_count() as usize).sum();
    let step = (total / (MAX_DOTS * 4)).max(1);
    let mut points = Vec::new();
    for prop in &show.props {
        for p in pf_geometry::world_positions(prop).into_iter().step_by(step) {
            points.push(p.x);
            points.push(p.y);
        }
    }
    points
}

/// The recent shows list and where it's kept (`None`: nowhere, for this session only).
pub(crate) struct RecentShows {
    dir: Option<PathBuf>,
    entries: Mutex<Option<Vec<Stored>>>,
}

impl RecentShows {
    pub(crate) fn new(dir: Option<PathBuf>) -> Self {
        Self {
            dir,
            entries: Mutex::new(None),
        }
    }

    /// A list kept for this session only.
    #[cfg(test)]
    pub(crate) fn in_memory() -> Self {
        Self::new(None)
    }

    /// The list, read from disk the first time.
    fn entries(&self) -> MutexGuard<'_, Option<Vec<Stored>>> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if entries.is_none() {
            *entries = Some(self.read());
        }
        entries
    }

    fn read(&self) -> Vec<Stored> {
        let Some(dir) = &self.dir else {
            return Vec::new();
        };
        let Some(text) = read_small(&dir.join(LIST_FILE), MAX_LIST_BYTES) else {
            return Vec::new();
        };
        let mut list: Vec<Stored> = serde_json::from_str(&text).unwrap_or_default();
        let mut seen = HashSet::new();
        list.retain(|s| !s.path.is_empty() && seen.insert(s.path.clone()));
        list.truncate(RECENT_LIMIT);
        list
    }

    /// Writes the list (all at once: a failed write leaves the old file) and removes
    /// thumbnails nothing refers to any more. Called with the list locked, like every change
    /// to the thumbnails, so none is removed while it's being written or put on the list.
    fn write(&self, list: &[Stored]) {
        let Some(dir) = &self.dir else { return };
        if let Err(error) = write_atomic(
            &dir.join(LIST_FILE),
            &serde_json::to_vec_pretty(list).unwrap_or_default(),
        ) {
            log::warn!("couldn't keep the recent shows list: {error}");
            return;
        }
        let used: HashSet<&str> = list.iter().filter_map(|s| s.thumbnail.as_deref()).collect();
        if let Ok(files) = std::fs::read_dir(dir.join(THUMBS)) {
            for file in files.flatten() {
                let name = file.file_name();
                if !name.to_str().is_some_and(|n| used.contains(n)) {
                    let _ = std::fs::remove_file(file.path());
                }
            }
        }
    }

    /// Puts the show at the top of the list (once), keeping at most [`RECENT_LIMIT`].
    pub(crate) fn record(&self, visit: Visit, now: u64) {
        let path = path_to_text(&visit.path);
        let svg = thumbnail_svg(&visit.points);
        let mut entries = self.entries();
        let thumbnail = svg.and_then(|svg| self.write_thumbnail(&path, &svg));
        let list = entries.get_or_insert_with(Vec::new);
        list.retain(|s| s.path != path);
        list.insert(
            0,
            Stored {
                path,
                name: visit.name,
                opened_at: now,
                props: visit.props,
                pixels: visit.pixels,
                controllers: visit.controllers,
                thumbnail,
            },
        );
        list.truncate(RECENT_LIMIT);
        self.write(list);
    }

    /// Writes a show's thumbnail (with the list locked: see [`Self::write`]).
    fn write_thumbnail(&self, path: &str, svg: &str) -> Option<String> {
        let dir = self.dir.as_ref()?.join(THUMBS);
        let name = format!("{:016x}.svg", stable_hash(path));
        std::fs::create_dir_all(&dir).ok()?;
        match write_atomic(&dir.join(&name), svg.as_bytes()) {
            Ok(()) => Some(name),
            Err(error) => {
                log::warn!("couldn't keep a recent show's picture: {error}");
                None
            }
        }
    }

    /// Takes a show off the list (nothing happens when it isn't on it).
    pub(crate) fn forget(&self, path: &str) {
        let mut entries = self.entries();
        let list = entries.get_or_insert_with(Vec::new);
        let before = list.len();
        list.retain(|s| s.path != path);
        if list.len() != before {
            self.write(list);
        }
    }

    /// Empties the list.
    pub(crate) fn clear(&self) {
        let mut entries = self.entries();
        let list = entries.get_or_insert_with(Vec::new);
        list.clear();
        self.write(list);
    }

    /// The shows' paths and names, newest first, without looking at the disk (for menus).
    pub(crate) fn names(&self) -> Vec<(String, String)> {
        self.entries()
            .iter()
            .flatten()
            .map(|s| (s.path.clone(), s.name.clone()))
            .collect()
    }

    /// The list, newest first, with each show's thumbnail and whether its file is still there.
    /// Reads the disk: call it away from the window and the engine.
    pub(crate) fn list(&self) -> Vec<RecentShow> {
        let stored = self.entries().clone().unwrap_or_default();
        let presence = presence_of(&stored, PRESENCE_WAIT);
        stored
            .into_iter()
            .zip(presence)
            .map(|(s, status)| RecentShow {
                thumbnail: s.thumbnail.as_deref().and_then(|name| self.read_thumbnail(name)),
                path: s.path,
                name: s.name,
                opened_at: s.opened_at,
                props: s.props,
                pixels: s.pixels,
                controllers: s.controllers,
                status,
            })
            .collect()
    }

    fn read_thumbnail(&self, name: &str) -> Option<String> {
        // Only names the shell gave out: no folders.
        if name.contains(['/', '\\']) || name.starts_with('.') {
            return None;
        }
        let path = self.dir.as_ref()?.join(THUMBS).join(name);
        if std::fs::metadata(&path).ok()?.len() > MAX_THUMB_BYTES {
            return None;
        }
        let svg = std::fs::read_to_string(path).ok()?;
        svg.starts_with("<svg").then_some(svg)
    }
}

/// Whether each show's file is there, asking about all of them at once and waiting at most
/// `wait` (a drive that hasn't answered by then counts as unknown).
fn presence_of(stored: &[Stored], wait: Duration) -> Vec<Presence> {
    let (tx, rx) = mpsc::channel();
    for (i, s) in stored.iter().enumerate() {
        let tx = tx.clone();
        let path = path_from_text(&s.path);
        // A check that can't start leaves the show as unknown.
        let _ = std::thread::Builder::new()
            .name("pixelflow-recent-check".into())
            .spawn(move || {
                let here = std::fs::metadata(&path).is_ok_and(|m| m.is_file());
                let _ = tx.send((i, if here { Presence::Here } else { Presence::Missing }));
            });
    }
    drop(tx);
    let mut presence = vec![Presence::Unknown; stored.len()];
    let deadline = Instant::now() + wait;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok((i, p)) => presence[i] = p,
            Err(_) => break,
        }
    }
    presence
}

/// Milliseconds since 1970.
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// FNV-1a: the same name for the same path on every run and every Rust version.
fn stable_hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Writes `bytes` to `path` all at once (a temporary file of its own, then a rename): a crash
/// or a failed write leaves the old file, and a reader never sees half of it. For the shell's
/// small lists, which can be rebuilt, so not flushed to the disk first like a show is.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    let n = WRITES.fetch_add(1, Ordering::Relaxed);
    let temp = dir.join(format!(".{name}.{}.{n}.tmp", std::process::id()));
    let written = std::fs::write(&temp, bytes).and_then(|()| std::fs::rename(&temp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

/// The text of a small file the shell keeps; `None` when it can't be read or is larger than
/// `limit` (not one the shell wrote).
pub(crate) fn read_small(path: &Path, limit: u64) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > limit {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// A small picture of the layout's pixels seen from the front, as SVG; `None` with no pixels.
pub(crate) fn thumbnail_svg(points: &[f32]) -> Option<String> {
    let pairs: Vec<(f32, f32)> = points
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[x, y]| (x, y))
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .collect();
    if pairs.is_empty() {
        return None;
    }
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for &(x, y) in &pairs {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    let span_x = (max_x - min_x).max(1e-3);
    let span_y = (max_y - min_y).max(1e-3);
    let scale = ((THUMB_W - 2.0 * THUMB_PAD) / span_x).min((THUMB_H - 2.0 * THUMB_PAD) / span_y);
    // Centred, with y up as in the layout.
    let off_x = (THUMB_W - span_x * scale) / 2.0;
    let off_y = (THUMB_H - span_y * scale) / 2.0;
    let mut seen = HashSet::new();
    let mut dots = String::new();
    for &(x, y) in &pairs {
        let px = ((x - min_x) * scale + off_x).round() as i32;
        let py = (THUMB_H - ((y - min_y) * scale + off_y)).round() as i32;
        if seen.len() >= MAX_DOTS {
            break;
        }
        if seen.insert((px, py)) {
            let _ = write!(dots, "M{px} {py}h0");
        }
    }
    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {THUMB_W} {THUMB_H}\">\
         <path d=\"{dots}\" fill=\"none\" stroke=\"#fcd34d\" stroke-width=\"4\" stroke-linecap=\"round\"/></svg>"
    ))
}

/// The recent shows, newest first, each with its thumbnail and whether it's still there.
#[tauri::command]
pub(crate) async fn list_recent_shows(state: State<'_, AppState>) -> Reply<Vec<RecentShow>> {
    let list = Arc::clone(&state.recent);
    tauri::async_runtime::spawn_blocking(move || list.list())
        .await
        .map_err(|_| "Something went wrong reading your recent shows.".to_string())
}

/// Takes a show off the recent list (its file is left alone).
#[tauri::command]
pub(crate) async fn forget_recent_show<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    path: String,
) -> Reply<()> {
    let list = Arc::clone(&state.recent);
    let _ = tauri::async_runtime::spawn_blocking(move || list.forget(&path)).await;
    crate::menu::refresh_recent(&app, &state.recent);
    Ok(())
}

/// Empties the recent list.
#[tauri::command]
pub(crate) async fn clear_recent_shows<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
) -> Reply<()> {
    let list = Arc::clone(&state.recent);
    let _ = tauri::async_runtime::spawn_blocking(move || list.clear()).await;
    crate::menu::refresh_recent(&app, &state.recent);
    Ok(())
}

/// For a recent show that isn't where the list says: asks where it is now (the system's open
/// dialog, starting where it was), opens the file chosen like any other open, and puts it on
/// the list in place of the old entry. `None` when cancelled.
#[tauri::command]
pub(crate) async fn locate_recent_show<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    path: String,
) -> Reply<Option<ShowSnapshot>> {
    let Some((_, name)) = state.recent.names().into_iter().find(|(p, _)| *p == path) else {
        return Err("That show isn't on your recent list any more.".into());
    };
    let was = path_from_text(&path);
    let mut request = Pick::of(PickKind::Show);
    request.title = Some(format!("Where is {name} now?"));
    request.first = was.parent().map(Path::to_path_buf);
    let Some(chosen) = crate::pickers::pick(&app, &state, request).await? else {
        return Ok(None);
    };
    let moved = chosen != was;
    let snapshot = crate::open_show_at(&app, &state, chosen).await?;
    if moved {
        let list = Arc::clone(&state.recent);
        let _ = tauri::async_runtime::spawn_blocking(move || list.forget(&path)).await;
        crate::menu::refresh_recent(&app, &state.recent);
    }
    Ok(Some(snapshot))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visit(path: &Path, name: &str) -> Visit {
        Visit {
            path: path.to_path_buf(),
            name: name.into(),
            props: 2,
            pixels: 100,
            controllers: 1,
            points: vec![0.0, 0.0, 1.0, 1.0],
        }
    }

    #[test]
    fn newest_first_without_repeats_and_at_most_ten() {
        let dir = tempfile::tempdir().unwrap();
        let recent = RecentShows::new(Some(dir.path().join("config")));
        for i in 0..12 {
            recent.record(
                visit(&dir.path().join(format!("{i}.json")), &format!("Show {i}")),
                i,
            );
        }
        recent.record(visit(&dir.path().join("5.json"), "Show 5 again"), 99);
        let names: Vec<String> = recent.names().into_iter().map(|(_, n)| n).collect();
        assert_eq!(names.len(), RECENT_LIMIT);
        assert_eq!(names[0], "Show 5 again");
        assert_eq!(names[1], "Show 11");
        assert_eq!(names.iter().filter(|n| n.starts_with("Show 5")).count(), 1);
    }

    #[test]
    fn the_list_survives_a_restart_and_keeps_counts_and_time() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        let show = dir.path().join("house.pixelflow.json");
        std::fs::write(&show, "{}").unwrap();
        RecentShows::new(Some(config.clone())).record(visit(&show, "House"), 1234);
        let list = RecentShows::new(Some(config)).list();
        assert_eq!(list.len(), 1);
        let entry = &list[0];
        assert_eq!(entry.name, "House");
        assert_eq!(entry.path, path_to_text(&show));
        assert_eq!(entry.opened_at, 1234);
        assert_eq!((entry.props, entry.pixels, entry.controllers), (2, 100, 1));
        assert_eq!(entry.status, Presence::Here);
        assert!(entry.thumbnail.as_deref().unwrap().starts_with("<svg"));
    }

    #[test]
    fn a_show_that_is_gone_stays_on_the_list_marked_missing() {
        let dir = tempfile::tempdir().unwrap();
        let recent = RecentShows::new(Some(dir.path().to_path_buf()));
        recent.record(visit(&dir.path().join("gone.json"), "Gone"), 1);
        let list = recent.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].status, Presence::Missing);
    }

    #[test]
    fn forget_and_clear_remove_entries_and_their_pictures() {
        let dir = tempfile::tempdir().unwrap();
        let recent = RecentShows::new(Some(dir.path().to_path_buf()));
        let a = dir.path().join("a.json");
        let b = dir.path().join("b.json");
        recent.record(visit(&a, "A"), 1);
        recent.record(visit(&b, "B"), 2);
        assert_eq!(std::fs::read_dir(dir.path().join(THUMBS)).unwrap().count(), 2);
        recent.forget(&path_to_text(&a));
        assert_eq!(recent.names(), vec![(path_to_text(&b), "B".to_string())]);
        assert_eq!(std::fs::read_dir(dir.path().join(THUMBS)).unwrap().count(), 1);
        recent.clear();
        assert!(recent.names().is_empty());
        assert_eq!(std::fs::read_dir(dir.path().join(THUMBS)).unwrap().count(), 0);
        assert!(
            RecentShows::new(Some(dir.path().to_path_buf()))
                .names()
                .is_empty()
        );
    }

    #[cfg(unix)]
    #[test]
    fn paths_that_are_not_utf8_are_kept_exactly() {
        use std::os::unix::ffi::OsStringExt;
        let dir = tempfile::tempdir().unwrap();
        let show = dir
            .path()
            .join(std::ffi::OsString::from_vec(b"Caf\xe9.pixelflow.json".to_vec()));
        // Some disks (APFS) refuse such names, so the file itself isn't made.
        RecentShows::new(Some(dir.path().join("c"))).record(visit(&show, "Café"), 1);
        let list = RecentShows::new(Some(dir.path().join("c"))).list();
        assert_eq!(path_from_text(&list[0].path), show);
        assert_eq!(list[0].status, Presence::Missing);
    }

    #[test]
    fn a_damaged_list_file_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(LIST_FILE), "not json").unwrap();
        assert!(RecentShows::new(Some(dir.path().to_path_buf())).list().is_empty());
    }

    #[test]
    fn thumbnails_are_never_read_from_outside_their_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("secret.svg"), "<svg>secret</svg>").unwrap();
        let recent = RecentShows::new(Some(dir.path().join("config")));
        assert_eq!(recent.read_thumbnail("../secret.svg"), None);
    }

    #[test]
    fn the_thumbnail_fits_the_layout_and_draws_each_spot_once() {
        assert_eq!(thumbnail_svg(&[]), None);
        let svg = thumbnail_svg(&[0.0, 0.0, 10.0, 5.0, 10.0, 5.0]).unwrap();
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"), "{svg}");
        // Two distinct dots, the lower-left one at the bottom (y up).
        assert_eq!(svg.matches('M').count(), 2, "{svg}");
        assert!(svg.contains("M12 174h0"), "{svg}");
        assert!(svg.contains("M308 26h0"), "{svg}");
        let many: Vec<f32> = (0..20_000).flat_map(|i| [i as f32, (i % 97) as f32]).collect();
        let svg = thumbnail_svg(&many).unwrap();
        assert!(svg.len() < MAX_THUMB_BYTES as usize, "{}", svg.len());
    }

    #[test]
    fn a_listed_thumbnail_is_never_removed_by_a_change_made_at_the_same_time() {
        let dir = tempfile::tempdir().unwrap();
        let recent = Arc::new(RecentShows::new(Some(dir.path().to_path_buf())));
        let show = dir.path().join("house.json");
        let busy = |work: fn(&RecentShows, &Path)| {
            let (recent, show) = (Arc::clone(&recent), show.clone());
            std::thread::spawn(move || {
                for _ in 0..300 {
                    work(&recent, &show);
                }
            })
        };
        let threads = [
            busy(|r, s| r.record(visit(s, "House"), 1)),
            busy(|r, s| r.forget(&path_to_text(s))),
            // Another show's save tidies the thumbnails while House is off the list.
            busy(|r, s| r.record(visit(&s.with_file_name("shed.json"), "Shed"), 2)),
        ];
        let mut broken = 0;
        while threads.iter().any(|t| !t.is_finished()) {
            // Looked at between changes: every thumbnail on the list is there.
            let entries = recent.entries();
            for s in entries.iter().flatten() {
                if let Some(name) = &s.thumbnail
                    && !dir.path().join(THUMBS).join(name).exists()
                {
                    broken += 1;
                }
            }
        }
        assert_eq!(broken, 0, "listed thumbnails missing");
    }

    #[test]
    fn a_list_file_too_big_to_be_ours_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let huge = format!(
            "[{}]",
            vec!["{\"path\":\"/a\",\"name\":\"A\",\"openedAt\":1}"; 20_000].join(",")
        );
        std::fs::write(dir.path().join(LIST_FILE), huge).unwrap();
        assert!(
            RecentShows::new(Some(dir.path().to_path_buf()))
                .names()
                .is_empty()
        );
    }

    #[test]
    fn presence_doesnt_wait_forever() {
        let start = Instant::now();
        let stored = vec![Stored {
            path: "/nowhere/at/all.json".into(),
            name: "X".into(),
            opened_at: 0,
            props: 0,
            pixels: 0,
            controllers: 0,
            thumbnail: None,
        }];
        assert_eq!(
            presence_of(&stored, Duration::from_millis(500)),
            vec![Presence::Missing]
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
