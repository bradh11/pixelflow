//! "Download from FPP": saves a sequence on an FPP, and the music its `mf` header names, into the
//! show's folder (`sequences/` and `music/`), or a folder the user chose when the show isn't
//! saved yet.
//!
//! Only GETs reach the FPP (its file listings, the sequence's details, and the two files; see
//! `pf_devices::fpp_download`). Files are written only inside that folder, under the names the
//! FPP listed, and only once whole: each arrives in a hidden temporary file, and both are moved
//! into place (music first) once both are in. Cancel, or any failure while downloading, leaves
//! nothing behind. A file already in the folder is replaced only when the user chose Replace.

use crate::{AppState, PathArg, Reply};
use pf_devices::Http;
use pf_devices::fpp_download::{self, Clash, DownloadError, Downloaded, RemoteSequence};
use pf_devices::fpp_info::FppFolder;
use pf_model::path_to_text;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

/// The event a download reports its progress with (see [`DownloadProgress`]).
pub(crate) const FPP_DOWNLOAD_PROGRESS_EVENT: &str = "fpp-download-progress";

/// Folders the user picked to download into this session (shell dialogs only): with an unsaved
/// show, a download may write only into one of these.
#[derive(Default)]
pub(crate) struct ChosenFolders(Mutex<HashSet<PathBuf>>);

impl ChosenFolders {
    pub(crate) fn add(&self, folder: PathBuf) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(folder);
    }

    fn contains(&self, folder: &Path) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(folder)
    }
}

/// Where downloads go: the saved show's folder (whatever the window says), else a folder the
/// user picked in the shell's dialog.
fn download_folder(state: &AppState, chosen: Option<&Path>) -> Reply<PathBuf> {
    if let Some(folder) = state.engine().show_path().and_then(Path::parent) {
        return Ok(folder.to_path_buf());
    }
    match chosen {
        Some(folder) if folder.is_absolute() && state.download_folders.contains(folder) => {
            Ok(folder.to_path_buf())
        }
        _ => Err("Choose a folder to save the sequence in first (your show isn't saved yet).".to_string()),
    }
}

/// A number with thousands separators: 6,147.
fn grouped(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A plain warning when the sequence's channels don't match the show's.
pub(crate) fn channel_warning(sequence: Option<u32>, show: u32) -> Option<String> {
    let sequence = sequence?;
    if sequence == show {
        return None;
    }
    let has = if show == 0 {
        "no channels wired to controllers yet".to_string()
    } else {
        grouped(show)
    };
    Some(format!(
        "This sequence uses {} channels; your show has {has}. Its preview won't light the right props until your layout matches.",
        grouped(sequence)
    ))
}

/// A file to download, and whether the folder has one by that name already.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalName {
    pub name: String,
    pub size_bytes: Option<u64>,
    /// The folder it goes in (the show folder's `sequences` or `music`).
    pub folder: String,
    pub exists: bool,
    /// The name Keep both saves it under.
    pub keep_both_name: String,
}

fn local_name(folder: &Path, name: &str, size: Option<u64>) -> LocalName {
    let taken = |n: &str| folder.join(n).exists();
    LocalName {
        name: name.to_string(),
        size_bytes: size,
        folder: path_to_text(folder),
        exists: taken(name),
        keep_both_name: pf_devices::fpp_upload::keep_both_name(name, taken),
    }
}

/// Before downloading: what will be saved where, what's in the way, and whether the sequence's
/// channels match the show's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadPlan {
    pub folder: String,
    pub sequence: LocalName,
    pub music: Option<LocalName>,
    /// The music file the sequence names, when the FPP doesn't have it.
    pub missing_music: Option<String>,
    pub channels: Option<u32>,
    pub show_channels: u32,
    pub channel_warning: Option<String>,
}

pub(crate) fn plan(remote: &RemoteSequence, folder: &Path, show_channels: u32) -> DownloadPlan {
    let missing_music = match (&remote.media, &remote.music) {
        (Some(mf), None) => fpp_download::media_file_name(mf).map(String::from),
        _ => None,
    };
    DownloadPlan {
        folder: path_to_text(folder),
        sequence: local_name(
            &folder.join("sequences"),
            &remote.file.name,
            remote.file.size_bytes,
        ),
        music: remote
            .music
            .as_ref()
            .map(|m| local_name(&folder.join("music"), &m.name, m.size_bytes)),
        missing_music,
        channels: remote.file.channels,
        show_channels,
        channel_warning: channel_warning(remote.file.channels, show_channels),
    }
}

/// What the user chose in the Download dialog.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadRequest {
    /// The sequence's name on the FPP, with `.fseq`.
    pub sequence: String,
    /// The music's name on the FPP, to download with it.
    pub music: Option<String>,
    /// A folder the user picked (only used while the show isn't saved).
    pub folder: Option<PathArg>,
    /// What to do about a file of that name already in the folder; none chosen, nothing is
    /// replaced.
    #[serde(default)]
    pub sequence_clash: Option<Clash>,
    #[serde(default)]
    pub music_clash: Option<Clash>,
    /// Tells this download's progress events apart from any other's.
    #[serde(default)]
    pub download_id: u64,
}

/// How far a download has got, sent as [`FPP_DOWNLOAD_PROGRESS_EVENT`] when its percentage
/// changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadProgress {
    pub download_id: u64,
    /// "sequence" or "music".
    pub step: &'static str,
    pub percent: u32,
    pub done: u64,
    pub total: u64,
}

/// What was saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadResult {
    pub sequence_path: String,
    pub music_path: Option<String>,
}

/// Downloads the sequence (and music) from the FPP at `host` into `folder`'s `sequences` and
/// `music` subfolders, until done or until `cancels` moves past `started`.
pub(crate) fn download(
    http: &dyn Http,
    host: &str,
    folder: &Path,
    request: &DownloadRequest,
    cancels: &AtomicU64,
    started: u64,
    mut report: impl FnMut(DownloadProgress),
) -> Reply<DownloadResult> {
    let going = || cancels.load(Ordering::Acquire) == started;
    let text = |e: DownloadError| e.to_string();
    // Only what the FPP lists now, by its exact name.
    let remote = fpp_download::read_sequence(http, host, &request.sequence).map_err(text)?;
    let music = match &request.music {
        Some(name) => match &remote.music {
            Some(m) if &m.name == name => Some(m.clone()),
            _ => return Err(text(DownloadError::Gone { name: name.clone() })),
        },
        None => None,
    };
    let sequences = folder.join("sequences");
    let songs = folder.join("music");
    let make = |dir: &Path| {
        std::fs::create_dir_all(dir).map_err(|e| {
            format!(
                "Couldn't make the folder {} ({e}). Nothing was saved.",
                dir.display()
            )
        })
    };
    make(&sequences)?;
    if music.is_some() {
        make(&songs)?;
    }
    // Checked before anything arrives, and again just before anything is moved into place.
    fpp_download::target_path(&sequences, &remote.file.name, request.sequence_clash).map_err(text)?;
    if let Some(m) = &music {
        fpp_download::target_path(&songs, &m.name, request.music_clash).map_err(text)?;
    }

    let download_id = request.download_id;
    let mut last: Option<(&'static str, u32)> = None;
    let mut progress = |step: &'static str, done: u64, total: u64| {
        let percent = u32::try_from(done.saturating_mul(100) / total.max(1))
            .unwrap_or(100)
            .min(100);
        if last != Some((step, percent)) {
            last = Some((step, percent));
            report(DownloadProgress {
                download_id,
                step,
                percent,
                done,
                total,
            });
        }
    };
    let mut fetch = |which: FppFolder, step: &'static str, name: &str, into: &Path, size: Option<u64>| {
        let total = size.unwrap_or(0);
        fpp_download::download(http, host, which, name, into, size, &mut |done| {
            progress(step, done, total.max(done));
            going()
        })
        .map_err(text)
    };
    let sequence: Downloaded = fetch(
        FppFolder::Sequences,
        "sequence",
        &remote.file.name,
        &sequences,
        remote.file.size_bytes,
    )?;
    let song: Option<(Downloaded, String)> = match &music {
        Some(m) => Some((
            fetch(FppFolder::Music, "music", &m.name, &songs, m.size_bytes)?,
            m.name.clone(),
        )),
        None => None,
    };
    if !going() {
        return Err(text(DownloadError::Cancelled));
    }

    // Both are whole: into place, music first, so the sequence never names music that isn't
    // there. Names are checked again (a file may have appeared meanwhile).
    let sequence_to =
        fpp_download::target_path(&sequences, &remote.file.name, request.sequence_clash).map_err(text)?;
    let music_path = match song {
        Some((file, name)) => {
            let to = fpp_download::target_path(&songs, &name, request.music_clash).map_err(text)?;
            file.place(&to).map_err(text)?;
            Some(to)
        }
        None => None,
    };
    sequence.place(&sequence_to).map_err(text)?;
    Ok(DownloadResult {
        sequence_path: path_to_text(&sequence_to),
        music_path: music_path.as_deref().map(path_to_text),
    })
}

async fn off_thread<T: Send + 'static>(work: impl FnOnce() -> Reply<T> + Send + 'static) -> Reply<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "Something went wrong talking to the FPP.".to_string())?
}

/// Reads what downloading `sequence` would save, and where (changes nothing anywhere).
#[tauri::command]
pub(crate) async fn fpp_download_plan(
    state: State<'_, AppState>,
    address: String,
    sequence: String,
    folder: Option<PathArg>,
) -> Reply<DownloadPlan> {
    let folder = download_folder(&state, folder.as_deref())?;
    let show_channels = state.engine().show_channels();
    let http = Arc::clone(&state.devices.read_http);
    off_thread(move || {
        let remote =
            fpp_download::read_sequence(http.as_ref(), &address, &sequence).map_err(|e| e.to_string())?;
        Ok(plan(&remote, &folder, show_channels))
    })
    .await
}

/// Downloads a sequence (and its music) from an FPP, sending [`FPP_DOWNLOAD_PROGRESS_EVENT`]
/// events as it goes; [`cancel_fpp_download`] stops it. Only reads from the FPP.
#[tauri::command]
pub(crate) async fn fpp_download<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    address: String,
    request: DownloadRequest,
) -> Reply<DownloadResult> {
    let folder = download_folder(&state, request.folder.as_deref())?;
    let started = state.download_cancels.load(Ordering::Acquire);
    let http = Arc::clone(&state.devices.upload_http);
    let handle = app.clone();
    off_thread(move || {
        let state = handle.state::<AppState>();
        download(
            http.as_ref(),
            &address,
            &folder,
            &request,
            &state.download_cancels,
            started,
            |progress| {
                // A window that's gone can't show progress; the download carries on.
                let _ = handle.emit(FPP_DOWNLOAD_PROGRESS_EVENT, progress);
            },
        )
    })
    .await
}

/// Cancels the downloads running now (nothing they fetched is kept).
#[tauri::command]
pub(crate) async fn cancel_fpp_download(state: State<'_, AppState>) -> Reply<()> {
    state.download_cancels.fetch_add(1, Ordering::AcqRel);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_devices::HttpClient;
    use pf_devices::testing::FakeFpp;
    use std::time::Duration;

    fn client() -> HttpClient {
        HttpClient::for_uploads_with(Duration::from_secs(1), Duration::from_secs(10))
    }

    fn pattern(len: usize, seed: u8) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8 ^ seed).collect()
    }

    fn fpp() -> FakeFpp {
        FakeFpp::start()
            .with_sequence_file("Wizards.fseq", &pattern(200_000, 1))
            .with_sequence_media("Wizards.fseq", r"C:\Users\Me\Shows\Audio\Wizards.mp3")
            .with_music_file("Wizards.mp3", &pattern(50_000, 2))
    }

    fn request(music: Option<&str>) -> DownloadRequest {
        DownloadRequest {
            sequence: "Wizards.fseq".into(),
            music: music.map(String::from),
            folder: None,
            sequence_clash: None,
            music_clash: None,
            download_id: 4,
        }
    }

    fn run(fpp: &FakeFpp, folder: &Path, request: &DownloadRequest) -> Reply<DownloadResult> {
        download(
            &client(),
            fpp.address(),
            folder,
            request,
            &AtomicU64::new(0),
            0,
            |_| {},
        )
    }

    /// Every file under `dir`, relative, hidden ones included.
    fn tree(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.path().is_dir() {
                out.extend(tree(&entry.path()).into_iter().map(|n| format!("{name}/{n}")));
            } else {
                out.push(name);
            }
        }
        out.sort();
        out
    }

    #[test]
    fn channel_counts_that_differ_are_warned_about_plainly() {
        assert_eq!(
            channel_warning(Some(6147), 1462).as_deref(),
            Some(
                "This sequence uses 6,147 channels; your show has 1,462. Its preview won't light the right props until your layout matches."
            )
        );
        assert!(
            channel_warning(Some(1_234_567), 0)
                .unwrap()
                .contains("1,234,567 channels; your show has no channels")
        );
        assert_eq!(channel_warning(Some(512), 512), None);
        assert_eq!(channel_warning(None, 512), None);
    }

    #[test]
    fn a_plan_names_the_files_where_they_go_and_what_is_in_the_way() {
        let fpp = fpp();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("music")).unwrap();
        std::fs::write(dir.path().join("music/Wizards.mp3"), b"mine").unwrap();
        let remote = fpp_download::read_sequence(&client(), fpp.address(), "Wizards.fseq").unwrap();
        let plan = plan(&remote, dir.path(), 1462);
        assert_eq!(plan.sequence.name, "Wizards.fseq");
        assert_eq!(plan.sequence.size_bytes, Some(200_000));
        assert!(!plan.sequence.exists);
        assert_eq!(plan.sequence.folder, path_to_text(&dir.path().join("sequences")));
        let music = plan.music.unwrap();
        assert!(music.exists);
        assert_eq!(music.keep_both_name, "Wizards (2).mp3");
        assert_eq!(plan.missing_music, None);
        assert_eq!(plan.channels, Some(6147));
        assert!(
            plan.channel_warning
                .unwrap()
                .starts_with("This sequence uses 6,147 channels; your show has 1,462.")
        );

        fpp.state()
            .sequence_media
            .insert("Wizards.fseq".into(), "/Volumes/Show/Gone.ogg".into());
        let remote = fpp_download::read_sequence(&client(), fpp.address(), "Wizards.fseq").unwrap();
        let plan = super::plan(&remote, dir.path(), 6147);
        assert_eq!(
            (plan.music, plan.missing_music.as_deref()),
            (None, Some("Gone.ogg"))
        );
        assert_eq!(plan.channel_warning, None);
    }

    #[test]
    fn downloads_the_sequence_and_its_music_into_the_show_folder() {
        let fpp = fpp();
        let dir = tempfile::tempdir().unwrap();
        let mut seen = Vec::new();
        let result = download(
            &client(),
            fpp.address(),
            dir.path(),
            &request(Some("Wizards.mp3")),
            &AtomicU64::new(0),
            0,
            |p| seen.push((p.step, p.percent)),
        )
        .unwrap();
        assert_eq!(
            result.sequence_path,
            path_to_text(&dir.path().join("sequences/Wizards.fseq"))
        );
        assert_eq!(
            result.music_path,
            Some(path_to_text(&dir.path().join("music/Wizards.mp3")))
        );
        assert_eq!(
            std::fs::read(dir.path().join("sequences/Wizards.fseq")).unwrap(),
            pattern(200_000, 1)
        );
        assert_eq!(
            std::fs::read(dir.path().join("music/Wizards.mp3")).unwrap(),
            pattern(50_000, 2)
        );
        assert_eq!(
            tree(dir.path()),
            vec!["music/Wizards.mp3", "sequences/Wizards.fseq"]
        );
        assert!(
            seen.contains(&("sequence", 100)) && seen.contains(&("music", 100)),
            "{seen:?}"
        );
        assert!(
            seen.iter()
                .any(|&(step, p)| step == "sequence" && p > 0 && p < 100),
            "{seen:?}"
        );
        // Only reads.
        assert_eq!(fpp.state().writes(), Vec::<String>::new());
        let requests = fpp.state().requests.clone();
        assert!(requests.contains(&"GET /api/file/sequences/Wizards.fseq".to_string()));
        assert!(requests.contains(&"GET /api/file/music/Wizards.mp3".to_string()));
        assert!(requests.iter().all(|r| r.starts_with("GET ")), "{requests:?}");
    }

    #[test]
    fn a_name_clash_is_replaced_kept_both_or_left_alone() {
        let fpp = fpp();
        let dir = tempfile::tempdir().unwrap();
        let seq = dir.path().join("sequences/Wizards.fseq");
        std::fs::create_dir_all(seq.parent().unwrap()).unwrap();
        std::fs::write(&seq, b"mine").unwrap();

        // Nothing chosen: refused, and nothing written.
        let error = run(&fpp, dir.path(), &request(None)).unwrap_err();
        assert!(
            error.contains("There's now a file called Wizards.fseq"),
            "{error}"
        );
        assert_eq!(tree(dir.path()), vec!["sequences/Wizards.fseq"]);
        assert_eq!(std::fs::read(&seq).unwrap(), b"mine");

        // Keep both: saved as Wizards (2).fseq.
        let mut keep = request(None);
        keep.sequence_clash = Some(Clash::KeepBoth);
        let result = run(&fpp, dir.path(), &keep).unwrap();
        assert_eq!(
            result.sequence_path,
            path_to_text(&dir.path().join("sequences/Wizards (2).fseq"))
        );
        assert_eq!(std::fs::read(&seq).unwrap(), b"mine");

        // Replace.
        let mut replace = request(None);
        replace.sequence_clash = Some(Clash::Replace);
        run(&fpp, dir.path(), &replace).unwrap();
        assert_eq!(std::fs::read(&seq).unwrap(), pattern(200_000, 1));
        assert_eq!(
            tree(dir.path()),
            vec!["sequences/Wizards (2).fseq", "sequences/Wizards.fseq"]
        );
    }

    #[test]
    fn cancelling_keeps_nothing_not_even_the_sequence_that_had_finished() {
        let fpp = fpp();
        fpp.state().download_delay = Duration::from_millis(15);
        let dir = tempfile::tempdir().unwrap();
        let cancels = Arc::new(AtomicU64::new(0));
        let canceller = {
            let cancels = Arc::clone(&cancels);
            move |p: DownloadProgress| {
                if p.step == "music" && p.percent >= 50 {
                    cancels.fetch_add(1, Ordering::AcqRel);
                }
            }
        };
        let error = download(
            &client(),
            fpp.address(),
            dir.path(),
            &request(Some("Wizards.mp3")),
            &cancels,
            0,
            canceller,
        )
        .unwrap_err();
        assert_eq!(error, "The download was cancelled. Nothing was saved.");
        // The folders may have been made, but hold nothing, not even a hidden part.
        assert_eq!(tree(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn a_failure_partway_leaves_no_partial_file() {
        let fpp = fpp();
        fpp.state().cut_downloads_at = Some(100_000);
        let dir = tempfile::tempdir().unwrap();
        let error = run(&fpp, dir.path(), &request(Some("Wizards.mp3"))).unwrap_err();
        assert!(error.contains("Nothing was saved."), "{error}");
        assert_eq!(tree(dir.path()), Vec::<String>::new());
    }

    #[test]
    fn music_the_fpp_no_longer_names_is_refused_before_anything_is_fetched() {
        let fpp = fpp();
        let dir = tempfile::tempdir().unwrap();
        let error = run(&fpp, dir.path(), &request(Some("../../config/settings"))).unwrap_err();
        assert!(error.contains("no longer has"), "{error}");
        assert!(
            fpp.state()
                .requests
                .iter()
                .all(|r| !r.starts_with("GET /api/file/")),
            "{:?}",
            fpp.state().requests
        );
        assert_eq!(tree(dir.path()), Vec::<String>::new());
    }
}
