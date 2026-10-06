//! "Send to FPP": puts a sequence (the open one, exported for the FPP, or a `.fseq` already on
//! the show's playlist) and its music on an FPP, optionally on one of its playlists.
//!
//! Only [`fpp_send`] changes the FPP, and the window calls it only from the Send button.
//! [`fpp_send_plan`] and [`fpp_sequence_names`] only read.

use crate::{AppState, PathArg, Reply, message};
use pf_devices::fpp_upload::{self, FppFiles, NameCheck, PlaylistChoice, UploadError};
use pf_devices::{DeviceError, Http};
use pf_engine::SequenceExport;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

/// The event a send reports its progress with (see [`SendProgress`]).
pub(crate) const FPP_SEND_PROGRESS_EVENT: &str = "fpp-send-progress";

/// What to send.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum SendSource {
    /// The open sequence, exported for the FPP; `name` is what to call it there.
    OpenSequence { name: String },
    /// A `.fseq` file as it is (one of the show's sequences).
    File { path: PathArg },
}

impl SendSource {
    /// The sequence's file name on the FPP, before any renaming.
    fn fpp_name(&self) -> String {
        match self {
            SendSource::OpenSequence { name } => fpp_upload::fpp_file_name(name, "fseq"),
            SendSource::File { path } => fpp_upload::fpp_file_name(
                &path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default(),
                "fseq",
            ),
        }
    }
}

/// The music's file name on the FPP, or why FPP can't play it.
fn music_fpp_name(music: &Path) -> Reply<String> {
    let file = music
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !fpp_upload::is_music(&file) {
        return Err(format!(
            "The FPP can't play {file} with a sequence. Choose an mp3, ogg, m4a, wav, or flac file."
        ));
    }
    let (stem, extension) = file.rsplit_once('.').unwrap_or((&file, "mp3"));
    Ok(fpp_upload::fpp_file_name(stem, extension))
}

/// Before sending: the names the files would have on the FPP and whether they're taken, its
/// playlists, and its free space.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SendPlan {
    pub sequence: NameCheck,
    pub music: Option<NameCheck>,
    pub playlists: Vec<String>,
    /// A name for a new playlist (the sequence's).
    pub new_playlist_name: String,
    pub free_bytes: Option<u64>,
}

pub(crate) fn plan(files: &FppFiles, source: &SendSource, music: Option<&Path>) -> Reply<SendPlan> {
    let sequence = files.check_sequence(&source.fpp_name());
    let music = music.map(music_fpp_name).transpose()?;
    let stem = sequence.name.trim_end_matches(".fseq");
    Ok(SendPlan {
        new_playlist_name: fpp_upload::playlist_name(stem),
        music: music.map(|name| files.check_media(&name)),
        sequence,
        playlists: files.playlists.clone(),
        free_bytes: files.free_bytes,
    })
}

/// What the user chose in the Send dialog.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SendRequest {
    pub source: SendSource,
    /// The music file on this computer, if the sequence has music.
    pub music: Option<PathArg>,
    /// The sequence's file name on the FPP (the planned one, or the keep-both one).
    pub sequence_name: String,
    /// The music's file name on the FPP, when there's music.
    pub music_name: Option<String>,
    /// False to use the copy of the music already on the FPP.
    pub upload_music: bool,
    pub playlist: PlaylistChoice,
}

/// How far a send has got: sent as [`FPP_SEND_PROGRESS_EVENT`] when the step or its percentage
/// changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SendProgress {
    /// "export", "sequence", "music", or "playlist".
    pub step: &'static str,
    pub percent: u32,
    pub done: u64,
    pub total: u64,
}

/// What was sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SendResult {
    pub sequence_name: String,
    pub music_name: Option<String>,
    pub playlist: Option<String>,
    /// What "Play it now" starts on the FPP (a playlist or a sequence name).
    pub play_name: String,
    pub notes: Vec<String>,
}

/// The sequence to send: exported now, or a file as it is.
pub(crate) enum Prepared {
    Export(Box<SequenceExport>),
    File(PathBuf),
}

/// A folder for the exported file, removed when the send ends however it ends.
struct TempFolder(PathBuf);

impl TempFolder {
    fn new() -> Reply<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "pixelflow-send-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).map_err(|e| format!("Couldn't make a temporary folder ({e})."))?;
        Ok(Self(path))
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// How long the `.fseq` at `path` plays, in seconds (0 if it can't be read).
fn fseq_seconds(path: &Path) -> f64 {
    pf_fseq::Sequence::open(path).map_or(0.0, |s| s.header().duration_ms() as f64 / 1000.0)
}

fn device_error(e: DeviceError) -> String {
    match e {
        DeviceError::Unreachable { address, reason } => {
            UploadError::Unreachable { address, reason }.to_string()
        }
        other => other.to_string(),
    }
}

/// Sends the sequence (and music, and playlist entry) to the FPP at `host`, reporting progress,
/// until done or until `cancels` moves past `started`.
pub(crate) fn send(
    http: &dyn Http,
    host: &str,
    prepared: Prepared,
    request: &SendRequest,
    cancels: &AtomicU64,
    started: u64,
    mut report: impl FnMut(SendProgress),
) -> Reply<SendResult> {
    let going = || cancels.load(Ordering::Acquire) == started;
    // Names come from the window: only names FPP keeps, never a path.
    let sequence_name = fpp_upload::fpp_file_name(&request.sequence_name, "fseq");
    let music = request.music.as_deref().filter(|_| request.music_name.is_some());
    let music_name = match (music, &request.music_name) {
        (Some(path), Some(name)) => {
            let extension = music_fpp_name(path)?;
            let extension = extension.rsplit_once('.').map_or("mp3", |(_, e)| e);
            Some(fpp_upload::fpp_file_name(name, extension))
        }
        _ => None,
    };
    let playlist = match &request.playlist {
        PlaylistChoice::None => PlaylistChoice::None,
        PlaylistChoice::Existing(name) => PlaylistChoice::Existing(name.clone()),
        PlaylistChoice::New(name) => PlaylistChoice::New(fpp_upload::playlist_name(name)),
    };

    let mut last: Option<(&'static str, u32)> = None;
    let mut progress = |step: &'static str, done: u64, total: u64| {
        let percent = u32::try_from(done.saturating_mul(100) / total.max(1)).unwrap_or(100);
        if last != Some((step, percent)) {
            last = Some((step, percent));
            report(SendProgress {
                step,
                percent,
                done,
                total,
            });
        }
    };

    // The sequence file: exported (its music recorded under the name it has on the FPP).
    let from_file = matches!(prepared, Prepared::File(_));
    let _temp;
    let (fseq, seconds) = match prepared {
        Prepared::Export(job) => {
            let temp = TempFolder::new()?;
            let path = temp.0.join(&sequence_name);
            _temp = temp;
            let job = job.with_music_named(music_name.as_deref());
            let seconds = job.duration_ms() as f64 / 1000.0;
            job.run(&path, |done, total| {
                progress("export", u64::from(done), u64::from(total));
                going()
            })
            .map_err(|e| {
                if going() {
                    message(e)
                } else {
                    UploadError::Cancelled.to_string()
                }
            })?;
            (path, seconds)
        }
        Prepared::File(path) => {
            let seconds = fseq_seconds(&path);
            (path, seconds)
        }
    };
    if !going() {
        return Err(UploadError::Cancelled.to_string());
    }

    // Room for it all, checked before anything is sent.
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let upload_music = request.upload_music && music_name.is_some();
    let needed = size(&fseq) + if upload_music { music.map_or(0, size) } else { 0 };
    let files = fpp_upload::read_files(http, host).map_err(device_error)?;
    fpp_upload::ensure_room(files.free_bytes, needed).map_err(|e| e.to_string())?;

    fpp_upload::upload(http, host, &fseq, &sequence_name, &mut |done, total| {
        progress("sequence", done, total);
        going()
    })
    .map_err(|e| e.to_string())?;
    if let (true, Some(path), Some(name)) = (upload_music, music, &music_name) {
        fpp_upload::upload(http, host, path, name, &mut |done, total| {
            progress("music", done, total);
            going()
        })
        .map_err(|e| e.to_string())?;
    }

    let mut notes = Vec::new();
    progress("playlist", 0, 1);
    let entry = fpp_upload::playlist_entry(&sequence_name, music_name.as_deref(), seconds);
    let on_playlist = match fpp_upload::put_on_playlist(http, host, &playlist, &entry) {
        Ok(name) => name,
        Err(e) => {
            // The files are on the FPP; only the playlist didn't work.
            notes.push(format!(
                "{e} The sequence is on the FPP; add it to a playlist in FPP's Playlists page."
            ));
            None
        }
    };
    progress("playlist", 1, 1);
    // A new playlist holds just this sequence and its music: playing it plays both. Otherwise
    // FPP plays the sequence with the music its file names.
    let play_name = match (&playlist, &on_playlist) {
        (PlaylistChoice::New(_), Some(name)) => name.clone(),
        _ => sequence_name.clone(),
    };
    // A file sent as it is names its music as it was when exported; music stored under another
    // name plays with it only from a playlist.
    if let (true, Some(path), Some(name)) = (from_file, music, &music_name)
        && music_fpp_name(path).ok().as_ref() != Some(name)
        && play_name == sequence_name
    {
        notes.push(format!(
            "The music is on the FPP as {name}. Play the sequence from a playlist so the music plays with it."
        ));
    }
    Ok(SendResult {
        sequence_name,
        music_name,
        playlist: on_playlist,
        play_name,
        notes,
    })
}

async fn off_thread<T: Send + 'static>(work: impl FnOnce() -> Reply<T> + Send + 'static) -> Reply<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "Something went wrong talking to the FPP.".to_string())?
}

/// Reads what sending would do: the names on the FPP and whether they're taken, its playlists,
/// and its free space (changes nothing).
#[tauri::command]
pub(crate) async fn fpp_send_plan(
    state: State<'_, AppState>,
    address: String,
    source: SendSource,
    music: Option<PathArg>,
) -> Reply<SendPlan> {
    let http = Arc::clone(&state.devices.upload_http);
    off_thread(move || {
        let files = fpp_upload::read_files(http.as_ref(), &address).map_err(device_error)?;
        plan(&files, &source, music.as_deref())
    })
    .await
}

/// Sends a sequence to an FPP, sending [`FPP_SEND_PROGRESS_EVENT`] events as it goes;
/// [`cancel_fpp_send`] stops it. Changes the FPP: only ever called from the Send button.
#[tauri::command]
pub(crate) async fn fpp_send<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    address: String,
    request: SendRequest,
) -> Reply<SendResult> {
    let prepared = match &request.source {
        SendSource::OpenSequence { .. } => {
            Prepared::Export(Box::new(state.engine().sequence_export().map_err(message)?))
        }
        SendSource::File { path } => Prepared::File(path.to_path_buf()),
    };
    let started = state.send_cancels.load(Ordering::Acquire);
    let http = Arc::clone(&state.devices.upload_http);
    let handle = app.clone();
    off_thread(move || {
        let state = handle.state::<AppState>();
        send(
            http.as_ref(),
            &address,
            prepared,
            &request,
            &state.send_cancels,
            started,
            |progress| {
                // A window that's gone can't show progress; the send carries on.
                let _ = handle.emit(FPP_SEND_PROGRESS_EVENT, progress);
            },
        )
    })
    .await
}

/// Cancels the sends running now (they stop before their next piece).
#[tauri::command]
pub(crate) async fn cancel_fpp_send(state: State<'_, AppState>) -> Reply<()> {
    state.send_cancels.fetch_add(1, Ordering::AcqRel);
    Ok(())
}

/// The sequence files on an FPP, with `.fseq` (one request; changes nothing).
#[tauri::command]
pub(crate) async fn fpp_sequence_names(state: State<'_, AppState>, address: String) -> Reply<Vec<String>> {
    let http = Arc::clone(&state.devices.http);
    off_thread(move || fpp_upload::sequence_names(http.as_ref(), &address).map_err(|e| e.to_string())).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_devices::HttpClient;
    use pf_devices::testing::{FakeFpp, StoredFile};
    use std::time::Duration;

    fn client() -> HttpClient {
        HttpClient::for_uploads_with(Duration::from_secs(1), Duration::from_secs(10))
    }

    fn file(dir: &tempfile::TempDir, name: &str, len: usize) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, (0..len).map(|i| (i % 251) as u8).collect::<Vec<_>>()).unwrap();
        path
    }

    fn request(source: SendSource, music: Option<&Path>, playlist: PlaylistChoice) -> SendRequest {
        SendRequest {
            sequence_name: source.fpp_name(),
            music_name: music.map(|m| music_fpp_name(m).unwrap()),
            music: music.map(|m| PathArg(m.to_path_buf())),
            upload_music: true,
            source,
            playlist,
        }
    }

    fn from_file(path: &Path) -> SendSource {
        SendSource::File {
            path: PathArg(path.to_path_buf()),
        }
    }

    #[test]
    fn a_plan_names_the_files_and_finds_clashes() {
        let files = FppFiles {
            sequences: vec!["Christmas Medley.fseq".into()],
            media: vec!["medley.mp3".into()],
            playlists: vec!["Main".into()],
            free_bytes: Some(10),
        };
        let source = SendSource::OpenSequence {
            name: "Christmas Medley".into(),
        };
        let plan = plan(&files, &source, Some(Path::new("/m/Medley.MP3"))).unwrap();
        assert_eq!(plan.sequence.name, "Christmas Medley.fseq");
        assert!(plan.sequence.exists);
        assert_eq!(plan.sequence.keep_both_name, "Christmas Medley (2).fseq");
        let music = plan.music.unwrap();
        assert_eq!(music.name, "Medley.mp3");
        assert!(music.exists, "names match whatever their case");
        assert_eq!(plan.playlists, vec!["Main"]);
        assert_eq!(plan.new_playlist_name, "Christmas Medley");
        assert_eq!(plan.free_bytes, Some(10));

        let source = from_file(Path::new("/shows/Wizards in Winter.fseq"));
        let plan = super::plan(&files, &source, None).unwrap();
        assert_eq!(plan.sequence.name, "Wizards in Winter.fseq");
        assert!(!plan.sequence.exists);
        assert!(plan.music.is_none());

        let err = super::plan(&files, &source, Some(Path::new("/m/notes.txt"))).unwrap_err();
        assert!(err.contains("can't play notes.txt"), "{err}");
    }

    #[test]
    fn a_file_and_its_music_go_up_onto_a_new_playlist() {
        let fpp = FakeFpp::start().with_playlist("Main");
        let dir = tempfile::tempdir().unwrap();
        let fseq = file(&dir, "Wizards.fseq", 300_000);
        let music = file(&dir, "Wizards.mp3", 100_000);
        let mut events = Vec::new();
        let result = send(
            &client(),
            fpp.address(),
            Prepared::File(fseq.clone()),
            &request(
                from_file(&fseq),
                Some(&music),
                PlaylistChoice::New("Wizards".into()),
            ),
            &AtomicU64::new(0),
            0,
            |p| events.push(p),
        )
        .unwrap();
        assert_eq!(
            result,
            SendResult {
                sequence_name: "Wizards.fseq".into(),
                music_name: Some("Wizards.mp3".into()),
                playlist: Some("Wizards".into()),
                play_name: "Wizards".into(),
                notes: vec![],
            }
        );
        let state = fpp.state();
        assert_eq!(
            state.sequences["Wizards.fseq"],
            StoredFile::of(&std::fs::read(&fseq).unwrap())
        );
        assert_eq!(
            state.music["Wizards.mp3"],
            StoredFile::of(&std::fs::read(&music).unwrap())
        );
        assert_eq!(
            state.playlists["Wizards"]["mainPlaylist"][0]["mediaName"],
            "Wizards.mp3"
        );
        let steps: Vec<&str> = events.iter().map(|e| e.step).collect();
        assert_eq!(steps.first(), Some(&"sequence"));
        assert!(
            steps.contains(&"music") && steps.last() == Some(&"playlist"),
            "{steps:?}"
        );
        assert!(events.iter().any(|e| e.step == "sequence" && e.percent == 100));
    }

    #[test]
    fn music_already_on_the_fpp_can_be_used_as_it_is() {
        let fpp = FakeFpp::start().with_music("Wizards.mp3", 7);
        let dir = tempfile::tempdir().unwrap();
        let fseq = file(&dir, "Wizards.fseq", 1000);
        let music = file(&dir, "Wizards.mp3", 1000);
        let mut request = request(from_file(&fseq), Some(&music), PlaylistChoice::None);
        request.upload_music = false;
        let result = send(
            &client(),
            fpp.address(),
            Prepared::File(fseq),
            &request,
            &AtomicU64::new(0),
            0,
            |_| {},
        )
        .unwrap();
        assert_eq!(
            result.play_name, "Wizards.fseq",
            "no new playlist: the sequence plays"
        );
        assert_eq!(result.playlist, None);
        let state = fpp.state();
        assert_eq!(state.music["Wizards.mp3"].size, 7, "the FPP's copy is kept");
        assert!(!state.writes().iter().any(|w| w.contains("Wizards.mp3")));
        assert!(!state.writes().iter().any(|w| w.contains("playlist")));
    }

    #[test]
    fn keeping_both_sends_under_the_new_names() {
        let fpp = FakeFpp::start()
            .with_sequence("Show.fseq", 3)
            .with_music("Song.mp3", 3);
        let dir = tempfile::tempdir().unwrap();
        let fseq = file(&dir, "Show.fseq", 1000);
        let music = file(&dir, "Song.mp3", 1000);
        let mut request = request(from_file(&fseq), Some(&music), PlaylistChoice::None);
        request.sequence_name = "Show (2).fseq".into();
        request.music_name = Some("Song (2).mp3".into());
        let result = send(
            &client(),
            fpp.address(),
            Prepared::File(fseq),
            &request,
            &AtomicU64::new(0),
            0,
            |_| {},
        )
        .unwrap();
        let state = fpp.state();
        assert_eq!(
            (state.sequences["Show.fseq"].size, state.music["Song.mp3"].size),
            (3, 3)
        );
        assert_eq!(state.sequences["Show (2).fseq"].size, 1000);
        assert_eq!(state.music["Song (2).mp3"].size, 1000);
        assert_eq!(
            result.notes,
            vec![
                "The music is on the FPP as Song (2).mp3. Play the sequence from a playlist so the music plays with it."
            ]
        );
    }

    #[test]
    fn names_from_the_window_never_become_paths() {
        let fpp = FakeFpp::start();
        let dir = tempfile::tempdir().unwrap();
        let fseq = file(&dir, "a.fseq", 10);
        let mut request = request(
            from_file(&fseq),
            None,
            PlaylistChoice::New("../../settings".into()),
        );
        request.sequence_name = "../../etc/passwd".into();
        let result = send(
            &client(),
            fpp.address(),
            Prepared::File(fseq),
            &request,
            &AtomicU64::new(0),
            0,
            |_| {},
        )
        .unwrap();
        assert_eq!(result.sequence_name, "etc passwd.fseq");
        assert_eq!(result.playlist.as_deref(), Some("settings"));
    }

    #[test]
    fn not_enough_room_stops_before_sending() {
        let fpp = FakeFpp::start().with_free_bytes(100);
        let dir = tempfile::tempdir().unwrap();
        let fseq = file(&dir, "Show.fseq", 1000);
        let request = request(from_file(&fseq), None, PlaylistChoice::None);
        let err = send(
            &client(),
            fpp.address(),
            Prepared::File(fseq),
            &request,
            &AtomicU64::new(0),
            0,
            |_| {},
        )
        .unwrap_err();
        assert!(err.starts_with("There isn't room on the FPP"), "{err}");
        assert!(fpp.state().writes().is_empty());
    }

    #[test]
    fn cancelling_stops_the_send() {
        let fpp = FakeFpp::start().with_sequence("Show.fseq", 3);
        let dir = tempfile::tempdir().unwrap();
        let fseq = file(&dir, "Show.fseq", 9 * 1024 * 1024);
        let music = file(&dir, "Song.mp3", 1000);
        let request = request(from_file(&fseq), Some(&music), PlaylistChoice::New("Show".into()));
        let cancels = AtomicU64::new(0);
        let err = send(
            &client(),
            fpp.address(),
            Prepared::File(fseq),
            &request,
            &cancels,
            0,
            |p| {
                if p.step == "sequence" && p.percent >= 50 {
                    cancels.fetch_add(1, Ordering::AcqRel);
                }
            },
        )
        .unwrap_err();
        assert_eq!(err, "The upload was cancelled.");
        let state = fpp.state();
        assert_eq!(state.sequences["Show.fseq"].size, 3);
        assert!(state.music.is_empty() && state.playlists.is_empty());
    }

    #[test]
    fn an_unreachable_fpp_is_said_plainly() {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let host = format!("127.0.0.1:{port}");
        let dir = tempfile::tempdir().unwrap();
        let fseq = file(&dir, "Show.fseq", 10);
        let request = request(from_file(&fseq), None, PlaylistChoice::None);
        let err = send(
            &client(),
            &host,
            Prepared::File(fseq),
            &request,
            &AtomicU64::new(0),
            0,
            |_| {},
        )
        .unwrap_err();
        assert!(
            err.starts_with(&format!("Couldn't reach the FPP at {host}")),
            "{err}"
        );
    }
}
