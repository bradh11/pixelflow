//! "Send to FPP": puts a sequence (the open one, exported for the FPP, or a `.fseq` already on
//! the show's playlist) and its music on an FPP, optionally on one of its playlists.
//!
//! Only [`fpp_send`] changes the FPP, and the window calls it only from the Send button.
//! [`fpp_send_plan`] and [`fpp_sequence_names`] only read.
//!
//! A send is all or nothing up to one commit point: every file is uploaded to the FPP's upload
//! folder and checked first, and only then moved into place. Cancel, or any failure, before that
//! point leaves the FPP as it was. Nothing is ever replaced unless the user chose to replace that
//! very file: names are checked again just before the move.

use crate::{AppState, PathArg, Reply, message};
use pf_devices::fpp_upload::{self, FppFiles, LayoutBlock, NameCheck, PlaylistChoice, Staged, UploadError};
use pf_devices::{Destination, DeviceError, Http};
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

/// A `.fseq` file's header, or a plain refusal when it isn't one.
fn fseq_header(path: &Path) -> Reply<pf_fseq::Header> {
    pf_fseq::Sequence::open(path)
        .map(|s| s.header().clone())
        .map_err(|_| {
            format!(
                "{} isn't a sequence file the FPP can play.",
                path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into_owned()
                )
            )
        })
}

/// The music to send as a full path. Only the open sequence's own music may be named relative
/// to it (resolved as it is for playing, by `open_music`); a show file's music is always full.
pub(crate) fn resolve_music(
    source: &SendSource,
    music: Option<PathArg>,
    open_music: impl FnOnce() -> Option<PathBuf>,
) -> Reply<Option<PathArg>> {
    let Some(music) = music else {
        return Ok(None);
    };
    if music.is_absolute() {
        return Ok(Some(music));
    }
    match source {
        SendSource::OpenSequence { .. } => Ok(Some(PathArg(open_music().unwrap_or(music.0)))),
        SendSource::File { .. } => Err(format!(
            "PixelFlow can't find the music {}. Choose it again.",
            music.display()
        )),
    }
}

/// How the sequence lays out its channels, for comparing with the FPP's outputs.
#[derive(Debug, Clone, Default)]
pub(crate) struct Shape {
    pub channels: u32,
    pub blocks: Vec<LayoutBlock>,
}

/// Before sending: the names the files would have on the FPP and whether they clash (with the
/// clashing file's exact name there), its playlists, its free space, and anything about the
/// channel layout that doesn't match the FPP's outputs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SendPlan {
    pub sequence: NameCheck,
    pub music: Option<NameCheck>,
    pub playlists: Vec<String>,
    /// A name for a new playlist (the sequence's).
    pub new_playlist_name: String,
    /// Free space on the FPP, when it says.
    pub free_bytes: Option<u64>,
    pub layout_warnings: Vec<String>,
}

pub(crate) fn plan(
    files: &FppFiles,
    source: &SendSource,
    music: Option<&Path>,
    shape: &Shape,
    outputs: Option<&[Destination]>,
) -> Reply<SendPlan> {
    let sequence = files.check_sequence(&source.fpp_name());
    let music = music.map(music_fpp_name).transpose()?;
    let stem = sequence.name.trim_end_matches(".fseq");
    let layout_warnings = match outputs {
        Some(outputs) => fpp_upload::layout_warnings(shape.channels, &shape.blocks, outputs),
        None => vec!["Couldn't read the FPP's outputs, so the channel layout wasn't checked.".to_string()],
    };
    Ok(SendPlan {
        new_playlist_name: fpp_upload::playlist_name(stem),
        music: music.map(|name| files.check_media(&name)),
        sequence,
        playlists: files.playlists.clone(),
        free_bytes: files.free_bytes,
        layout_warnings,
    })
}

/// What the user chose in the Send dialog.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SendRequest {
    pub source: SendSource,
    /// The music file on this computer, if the sequence has music.
    pub music: Option<PathArg>,
    /// The sequence's file name on the FPP: the planned one, the keep-both one, or (to replace)
    /// the FPP's own spelling of the file it replaces.
    pub sequence_name: String,
    /// The music's file name on the FPP, when there's music (likewise; or the FPP's own spelling
    /// of the copy to use).
    pub music_name: Option<String>,
    /// False to use the copy of the music already on the FPP.
    pub upload_music: bool,
    /// The user chose to replace the FPP's file of that name. Without it, nothing is replaced.
    #[serde(default)]
    pub replace_sequence: bool,
    #[serde(default)]
    pub replace_music: bool,
    pub playlist: PlaylistChoice,
    /// Tells this send's progress events apart from any other's.
    #[serde(default)]
    pub send_id: u64,
}

/// How far a send has got: sent as [`FPP_SEND_PROGRESS_EVENT`] when the step or its percentage
/// changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SendProgress {
    pub send_id: u64,
    /// "export", "sequence", "music", then "commit" (moving into place: no cancelling from here
    /// on) and "playlist".
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

fn device_error(e: DeviceError) -> String {
    match e {
        DeviceError::Unreachable { address, reason } => {
            UploadError::Unreachable { address, reason }.to_string()
        }
        other => other.to_string(),
    }
}

/// The name a file goes by on the FPP. One that replaces or reuses an FPP file is that file's
/// exact name (checked to be a plain file name); otherwise it's tidied to one FPP keeps as is.
fn fpp_target(name: &str, extension: &str, exact: bool) -> Reply<String> {
    if !exact {
        return Ok(fpp_upload::fpp_file_name(name, extension));
    }
    let ends_right = name
        .rsplit_once('.')
        .is_some_and(|(_, e)| e.eq_ignore_ascii_case(extension));
    if fpp_upload::is_safe_fpp_name(name) && ends_right {
        Ok(name.to_string())
    } else {
        Err(format!(
            "PixelFlow can't replace or use {name} on the FPP because of the characters in its name. Choose Keep both instead."
        ))
    }
}

fn clash_message(name: &str) -> String {
    format!(
        "The FPP now has a file called {name} that wasn't there when you chose what to send, so nothing was replaced. Check again and choose what to do."
    )
}

/// What a send writes, and what it may replace.
struct Targets<'a> {
    sequence: &'a str,
    replace_sequence: bool,
    /// The music's name, whether it's uploaded (else the FPP's copy is used), and whether
    /// replacing is allowed.
    music: Option<(&'a str, bool, bool)>,
}

/// Refuses when writing would replace a file the user didn't choose to replace (a clash is found
/// whatever the capitals, and replacing needs the exact file), or when the FPP's copy of the
/// music to use has gone.
fn check_targets(files: &FppFiles, targets: &Targets) -> Reply<()> {
    let clash = |list: &[String], name: &str, replace: bool| -> Reply<()> {
        match list.iter().find(|f| f.eq_ignore_ascii_case(name)) {
            Some(found) if !(replace && found == name) => Err(clash_message(found)),
            _ => Ok(()),
        }
    };
    clash(&files.sequences, targets.sequence, targets.replace_sequence)?;
    match targets.music {
        Some((name, true, replace)) => clash(&files.media, name, replace),
        Some((name, false, _)) if !files.media.iter().any(|f| f == name) => Err(format!(
            "The FPP no longer has {name}. Check again and choose what to do."
        )),
        _ => Ok(()),
    }
}

/// Removes staged files that won't be moved into place; `error` says what happened, plus where
/// something couldn't be removed.
fn discard_all(http: &dyn Http, host: &str, staged: &[Staged], error: String) -> String {
    let left: Vec<String> = staged
        .iter()
        .flat_map(|s| fpp_upload::discard(http, host, s))
        .collect();
    if left.is_empty() {
        error
    } else {
        format!(
            "{error} These may be left in the FPP's File Manager, under Uploads: {}. You can delete them there.",
            left.join(", ")
        )
    }
}

/// Sends the sequence (and music, and playlist entry) to the FPP at `host`, reporting progress,
/// until done or until `cancels` moves past `started` (honoured up to the commit point).
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
    let cancelled = || UploadError::Cancelled.to_string();
    // Names come from the window: tidied, or the FPP's exact name when replacing or reusing.
    let sequence_name = fpp_target(&request.sequence_name, "fseq", request.replace_sequence)?;
    let music = request.music.as_deref().filter(|_| request.music_name.is_some());
    let upload_music = request.upload_music;
    let music_name = match (music, &request.music_name) {
        (Some(path), Some(name)) => {
            let tidy = music_fpp_name(path)?;
            let extension = tidy.rsplit_once('.').map_or("mp3", |(_, e)| e);
            let exact = request.replace_music || !upload_music;
            Some(fpp_target(name, extension, exact)?)
        }
        _ => None,
    };
    let playlist = match &request.playlist {
        PlaylistChoice::None => PlaylistChoice::None,
        PlaylistChoice::Existing(name) => PlaylistChoice::Existing(name.clone()),
        PlaylistChoice::New(name) => PlaylistChoice::New(fpp_upload::playlist_name(name)),
    };
    let targets = Targets {
        sequence: &sequence_name,
        replace_sequence: request.replace_sequence,
        music: music_name
            .as_deref()
            .map(|name| (name, upload_music, request.replace_music)),
    };

    let send_id = request.send_id;
    let mut last: Option<(&'static str, u32)> = None;
    let mut progress = |step: &'static str, done: u64, total: u64| {
        let percent = u32::try_from(done.saturating_mul(100) / total.max(1)).unwrap_or(100);
        if last != Some((step, percent)) {
            last = Some((step, percent));
            report(SendProgress {
                send_id,
                step,
                percent,
                done,
                total,
            });
        }
    };

    // The sequence file: exported (its music recorded under its exact name on the FPP).
    let from_file = matches!(prepared, Prepared::File(_));
    let _temp;
    let (fseq, header) = match prepared {
        Prepared::Export(job) => {
            let temp = TempFolder::new()?;
            let path = temp.0.join(&sequence_name);
            _temp = temp;
            let job = job.with_music_named(music_name.as_deref());
            job.run(&path, |done, total| {
                progress("export", u64::from(done), u64::from(total));
                going()
            })
            .map_err(|e| if going() { message(e) } else { cancelled() })?;
            let header = fseq_header(&path)?;
            (path, header)
        }
        Prepared::File(path) => {
            let header = fseq_header(&path)?;
            (path, header)
        }
    };
    if !going() {
        return Err(cancelled());
    }

    // Before anything is sent: nothing unapproved would be replaced, and there's room. FPP puts
    // each upload together by copying its received pieces, so for a moment it holds the biggest
    // file twice.
    let files = fpp_upload::read_files(http, host).map_err(device_error)?;
    check_targets(&files, &targets)?;
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let fseq_bytes = size(&fseq);
    let music_bytes = if upload_music { music.map_or(0, size) } else { 0 };
    let needed = fseq_bytes + music_bytes + fseq_bytes.max(music_bytes);
    fpp_upload::ensure_room(files.free_bytes, needed).map_err(|e| e.to_string())?;

    // Phase one: every file into the FPP's upload folder, checked whole. Nothing replaced yet.
    let mut staged: Vec<Staged> = Vec::new();
    let sequence = fpp_upload::stage(http, host, &fseq, &sequence_name, &mut |done, total| {
        progress("sequence", done, total);
        going()
    })
    .map_err(|e| e.to_string())?;
    staged.push(sequence);
    if let (true, Some(path), Some(name)) = (upload_music, music, &music_name) {
        let song = fpp_upload::stage(http, host, path, name, &mut |done, total| {
            progress("music", done, total);
            going()
        });
        match song {
            Ok(song) => staged.insert(0, song),
            Err(e) => return Err(discard_all(http, host, &staged, e.to_string())),
        }
    }
    if !going() {
        return Err(discard_all(http, host, &staged, cancelled()));
    }
    // The names again, just before anything moves: a file may have appeared meanwhile.
    let files = match fpp_upload::read_files(http, host) {
        Ok(files) => files,
        Err(e) => return Err(discard_all(http, host, &staged, device_error(e))),
    };
    if let Err(e) = check_targets(&files, &targets) {
        return Err(discard_all(http, host, &staged, e));
    }

    // Phase two, the commit point: music first, so the sequence never names music that isn't
    // there. Cancel no longer applies.
    progress("commit", 0, 1);
    let mut placed: Vec<String> = Vec::new();
    for (i, file) in staged.iter().enumerate() {
        if let Err(e) = fpp_upload::commit(http, host, file) {
            let rest = discard_all(http, host, &staged[i + 1..], e.to_string());
            return Err(if placed.is_empty() {
                rest
            } else {
                format!("Only part of the send finished: {}. {rest}", placed.join("; "))
            });
        }
        let replaced = files
            .sequences
            .iter()
            .chain(&files.media)
            .any(|f| *f == file.name);
        placed.push(if replaced {
            format!("{} is on the FPP now (it replaced the one there)", file.name)
        } else {
            format!("{} is on the FPP now", file.name)
        });
    }

    let mut notes = Vec::new();
    progress("playlist", 0, 1);
    let seconds = header.duration_ms() as f64 / 1000.0;
    let entry = fpp_upload::playlist_entry(&sequence_name, music_name.as_deref(), seconds);
    let on_playlist = match fpp_upload::put_on_playlist(http, host, &playlist, &entry) {
        Ok(name) => name,
        Err(e) => {
            // The files are on the FPP; only the playlist didn't work.
            notes.push(format!(
                "{e} The sequence is on the FPP; add it to a playlist on FPP's Playlists page."
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
    // A file sent as it is names the music it was exported with.
    if from_file && play_name == sequence_name {
        match (&music_name, header.media.as_deref()) {
            (Some(name), Some(own)) if own != name => notes.push(format!(
                "This sequence file names its music as {own}, but the music is on the FPP as {name}. Play it from a playlist so this music plays with it."
            )),
            (Some(name), None) => notes.push(format!(
                "This sequence file doesn't name its music. Play it from a playlist so {name} plays with it."
            )),
            (None, Some(own)) => notes.push(format!(
                "This sequence file still names its own music ({own}). If the FPP has a file by that name, it plays when the sequence is started on its own."
            )),
            _ => {}
        }
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

/// The open sequence's channel layout, with each block's controller address.
fn open_shape(state: &AppState) -> Reply<Shape> {
    let engine = state.engine();
    let layout = engine.sequence_export().map_err(message)?.layout();
    let controllers = &engine.show().controllers;
    let blocks = layout
        .blocks
        .iter()
        .filter_map(|block| {
            let controller = controllers.iter().find(|c| c.id == block.controller)?;
            Some(LayoutBlock {
                name: block.name.clone(),
                address: controller.address.clone(),
                start: block.start,
                count: block.count,
            })
        })
        .collect();
    Ok(Shape {
        channels: layout.channels,
        blocks,
    })
}

/// Reads what sending would do: the names on the FPP and whether they clash, its playlists, its
/// free space, and how its outputs compare with the sequence's channels (changes nothing).
#[tauri::command]
pub(crate) async fn fpp_send_plan(
    state: State<'_, AppState>,
    address: String,
    source: SendSource,
    music: Option<PathArg>,
) -> Reply<SendPlan> {
    let music = resolve_music(&source, music, || state.engine().sequence_music())?;
    let shape = match &source {
        SendSource::OpenSequence { .. } => Some(open_shape(&state)?),
        SendSource::File { .. } => None,
    };
    let http = Arc::clone(&state.devices.read_http);
    off_thread(move || {
        let shape = match (&source, shape) {
            (SendSource::File { path }, _) => Shape {
                channels: fseq_header(path)?.channels,
                blocks: Vec::new(),
            },
            (_, shape) => shape.unwrap_or_default(),
        };
        let files = fpp_upload::read_files(http.as_ref(), &address).map_err(device_error)?;
        let outputs = pf_devices::fpp::read_config(http.as_ref(), &address)
            .ok()
            .map(|c| c.destinations);
        plan(&files, &source, music.as_deref(), &shape, outputs.as_deref())
    })
    .await
}

/// Sends a sequence to an FPP, sending [`FPP_SEND_PROGRESS_EVENT`] events as it goes;
/// [`cancel_fpp_send`] stops it before its commit point. Changes the FPP: only ever called from
/// the Send button.
#[tauri::command]
pub(crate) async fn fpp_send<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    address: String,
    mut request: SendRequest,
) -> Reply<SendResult> {
    request.music = resolve_music(&request.source, request.music.take(), || {
        state.engine().sequence_music()
    })?;
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

/// Cancels the sends running now (they stop before their commit point).
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

    /// A real `.fseq` of about `len` bytes, naming `media` as its music.
    fn fseq(dir: &tempfile::TempDir, name: &str, len: usize, media: Option<&str>) -> PathBuf {
        let path = dir.path().join(name);
        let channels = 300u32;
        let frames = u32::try_from(len / channels as usize).unwrap().max(1);
        let mut options = pf_fseq::WriteOptions::new(channels, frames, 50);
        options.media = media.map(str::to_string);
        let out = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
        let mut writer = pf_fseq::FseqWriter::new(out, options).unwrap();
        for f in 0..frames {
            let frame: Vec<u8> = (0..channels).map(|c| ((c + f * 7) % 251) as u8).collect();
            writer.write_frame(&frame).unwrap();
        }
        writer.finish().unwrap();
        path
    }

    fn request(source: SendSource, music: Option<&Path>, playlist: PlaylistChoice) -> SendRequest {
        SendRequest {
            sequence_name: source.fpp_name(),
            music_name: music.map(|m| music_fpp_name(m).unwrap()),
            music: music.map(|m| PathArg(m.to_path_buf())),
            upload_music: true,
            replace_sequence: false,
            replace_music: false,
            source,
            playlist,
            send_id: 7,
        }
    }

    fn from_file(path: &Path) -> SendSource {
        SendSource::File {
            path: PathArg(path.to_path_buf()),
        }
    }

    fn run(fpp: &FakeFpp, fseq: &Path, request: &SendRequest) -> Reply<SendResult> {
        send(
            &client(),
            fpp.address(),
            Prepared::File(fseq.to_path_buf()),
            request,
            &AtomicU64::new(0),
            0,
            |_| {},
        )
    }

    fn files() -> FppFiles {
        FppFiles {
            sequences: vec!["Christmas Medley.fseq".into()],
            media: vec!["medley.mp3".into()],
            playlists: vec!["Main".into()],
            free_bytes: Some(10),
        }
    }

    #[test]
    fn a_plan_names_the_files_and_finds_clashes_with_the_fpps_spelling() {
        let source = SendSource::OpenSequence {
            name: "Christmas Medley".into(),
        };
        let plan = plan(
            &files(),
            &source,
            Some(Path::new("/m/Medley.MP3")),
            &Shape::default(),
            Some(&[]),
        )
        .unwrap();
        assert_eq!(plan.sequence.name, "Christmas Medley.fseq");
        assert!(plan.sequence.exists);
        assert_eq!(plan.sequence.fpp_name.as_deref(), Some("Christmas Medley.fseq"));
        assert_eq!(plan.sequence.keep_both_name, "Christmas Medley (2).fseq");
        let music = plan.music.unwrap();
        assert_eq!(music.name, "Medley.mp3");
        assert!(music.exists, "a clash whatever the capitals");
        assert_eq!(
            music.fpp_name.as_deref(),
            Some("medley.mp3"),
            "the FPP's own spelling"
        );
        assert_eq!(plan.playlists, vec!["Main"]);
        assert_eq!(plan.new_playlist_name, "Christmas Medley");
        assert_eq!(plan.free_bytes, Some(10));
        assert!(plan.layout_warnings.is_empty());

        let source = from_file(Path::new("/shows/Wizards in Winter.fseq"));
        let err = super::plan(
            &files(),
            &source,
            Some(Path::new("/m/notes.txt")),
            &Shape::default(),
            None,
        )
        .unwrap_err();
        assert!(err.contains("can't play notes.txt"), "{err}");
    }

    #[test]
    fn a_plan_says_when_the_channels_dont_match_the_fpp() {
        let outputs = [Destination {
            address: "192.0.2.20".into(),
            description: "Falcon".into(),
            protocol: "DDP".into(),
            channels: 6147,
            start_channel: 1,
            start_universe: None,
            universe_size: None,
            ddp_raw: false,
            uneven_universes: false,
        }];
        let shape = Shape {
            channels: 5000,
            blocks: vec![],
        };
        let source = from_file(Path::new("/a.fseq"));
        let plan = plan(&files(), &source, None, &shape, Some(&outputs)).unwrap();
        assert_eq!(
            plan.layout_warnings,
            vec![
                "This sequence has 5,000 channels but the FPP sends 6,147. Lights past channel 5,000 will stay dark."
            ]
        );
        let plan = super::plan(&files(), &source, None, &shape, None).unwrap();
        assert_eq!(
            plan.layout_warnings,
            vec!["Couldn't read the FPP's outputs, so the channel layout wasn't checked."]
        );
    }

    #[test]
    fn a_file_and_its_music_go_up_onto_a_new_playlist() {
        let fpp = FakeFpp::start().with_playlist("Main");
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "Wizards.fseq", 300_000, Some("Wizards.mp3"));
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
        assert_eq!(state.playlists["Wizards"]["mainPlaylist"][0]["duration"], 50.0);
        let steps: Vec<&str> = events.iter().map(|e| e.step).collect();
        let commit = steps.iter().position(|s| *s == "commit").unwrap();
        assert!(
            steps[..commit].contains(&"sequence") && steps[..commit].contains(&"music"),
            "{steps:?}"
        );
        assert_eq!(steps.last(), Some(&"playlist"));
        assert!(events.iter().all(|e| e.send_id == 7));
        // Both files were whole in the upload folder before either moved.
        let moves: Vec<usize> = state
            .requests
            .iter()
            .enumerate()
            .filter(|(_, r)| r.starts_with("GET /api/file/move/"))
            .map(|(i, _)| i)
            .collect();
        let last_patch = state
            .requests
            .iter()
            .rposition(|r| r.starts_with("PATCH"))
            .unwrap();
        assert!(moves.iter().all(|m| *m > last_patch), "{:?}", state.requests);
    }

    #[test]
    fn nothing_is_replaced_without_the_users_say_so() {
        let fpp = FakeFpp::start().with_sequence("Show.fseq", 3);
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "Show.fseq", 1000, None);
        let err = run(
            &fpp,
            &fseq,
            &request(from_file(&fseq), None, PlaylistChoice::None),
        )
        .unwrap_err();
        assert_eq!(err, clash_message("Show.fseq"));
        assert!(fpp.state().writes().is_empty(), "{:?}", fpp.state().writes());
        assert_eq!(fpp.state().sequences["Show.fseq"].size, 3);
    }

    #[test]
    fn a_name_that_appears_while_sending_is_never_replaced() {
        let fpp = FakeFpp::start();
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "Show.fseq", 1000, None);
        let music = file(&dir, "Song.mp3", 6 * 1024 * 1024);
        let mut appeared = false;
        let request = request(from_file(&fseq), Some(&music), PlaylistChoice::None);
        let err = send(
            &client(),
            fpp.address(),
            Prepared::File(fseq.clone()),
            &request,
            &AtomicU64::new(0),
            0,
            |p| {
                if p.step == "music" && !appeared {
                    // Someone uploads a sequence by that name (xLights, say) meanwhile.
                    appeared = true;
                    fpp.state()
                        .sequences
                        .insert("show.fseq".into(), StoredFile::default());
                }
            },
        )
        .unwrap_err();
        assert_eq!(err, clash_message("show.fseq"));
        let state = fpp.state();
        assert!(!state.requests.iter().any(|r| r.contains("/api/file/move/")));
        assert_eq!(
            state.upload_bytes("Show.fseq") + state.upload_bytes("Song.mp3"),
            0,
            "{:?}",
            state.uploads
        );
        assert!(state.music.is_empty());
    }

    #[test]
    fn replacing_needs_the_fpps_exact_file() {
        let fpp = FakeFpp::start()
            .with_sequence("show.fseq", 3)
            .with_music("song.mp3", 3);
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "Show.fseq", 1000, None);
        let music = file(&dir, "Song.mp3", 1000);
        let mut request = request(from_file(&fseq), Some(&music), PlaylistChoice::None);
        request.replace_sequence = true;
        request.replace_music = true;
        // "Show.fseq" isn't the FPP's file: replacing it would only add a second one.
        let err = run(&fpp, &fseq, &request).unwrap_err();
        assert_eq!(err, clash_message("show.fseq"));
        // With the FPP's own spelling, the very file is replaced.
        request.sequence_name = "show.fseq".into();
        request.music_name = Some("song.mp3".into());
        let result = run(&fpp, &fseq, &request).unwrap();
        assert_eq!(result.sequence_name, "show.fseq");
        let state = fpp.state();
        assert_eq!(state.sequences.len(), 1);
        assert_eq!(
            state.sequences["show.fseq"].size,
            std::fs::metadata(&fseq).unwrap().len()
        );
        assert_eq!(state.music["song.mp3"].size, 1000);
    }

    #[test]
    fn music_already_on_the_fpp_is_used_by_its_exact_name() {
        let fpp = FakeFpp::start().with_music("medley.mp3", 7);
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "Wizards.fseq", 1000, Some("medley.mp3"));
        let music = file(&dir, "Medley.mp3", 1000);
        let mut request = request(
            from_file(&fseq),
            Some(&music),
            PlaylistChoice::New("Wizards".into()),
        );
        request.upload_music = false;
        request.music_name = Some("medley.mp3".into());
        let result = run(&fpp, &fseq, &request).unwrap();
        assert_eq!(result.music_name.as_deref(), Some("medley.mp3"));
        let state = fpp.state();
        assert_eq!(state.music["medley.mp3"].size, 7, "the FPP's copy is kept");
        assert!(!state.writes().iter().any(|w| w.contains("medley.mp3")));
        assert_eq!(
            state.playlists["Wizards"]["mainPlaylist"][0]["mediaName"],
            "medley.mp3"
        );

        // Gone since the dialog checked: refused, not sent with a name that doesn't exist.
        let fpp = FakeFpp::start();
        let err = run(&fpp, &fseq, &request).unwrap_err();
        assert!(err.starts_with("The FPP no longer has medley.mp3"), "{err}");
        assert!(fpp.state().writes().is_empty());
    }

    #[test]
    fn keeping_both_sends_under_the_new_names() {
        let fpp = FakeFpp::start()
            .with_sequence("Show.fseq", 3)
            .with_music("Song.mp3", 3);
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "Show.fseq", 1000, Some("Song.mp3"));
        let music = file(&dir, "Song.mp3", 1000);
        let mut request = request(from_file(&fseq), Some(&music), PlaylistChoice::None);
        request.sequence_name = "Show (2).fseq".into();
        request.music_name = Some("Song (2).mp3".into());
        let result = run(&fpp, &fseq, &request).unwrap();
        let state = fpp.state();
        assert_eq!(
            (state.sequences["Show.fseq"].size, state.music["Song.mp3"].size),
            (3, 3)
        );
        assert!(state.sequences.contains_key("Show (2).fseq"));
        assert_eq!(state.music["Song (2).mp3"].size, 1000);
        assert_eq!(
            result.notes,
            vec![
                "This sequence file names its music as Song.mp3, but the music is on the FPP as Song (2).mp3. Play it from a playlist so this music plays with it."
            ]
        );
    }

    #[test]
    fn a_file_sent_without_music_says_it_still_names_its_own() {
        let fpp = FakeFpp::start();
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "Show.fseq", 1000, Some("Old Song.mp3"));
        let result = run(
            &fpp,
            &fseq,
            &request(from_file(&fseq), None, PlaylistChoice::None),
        )
        .unwrap();
        assert_eq!(
            result.notes,
            vec![
                "This sequence file still names its own music (Old Song.mp3). If the FPP has a file by that name, it plays when the sequence is started on its own."
            ]
        );
    }

    #[test]
    fn a_file_that_isnt_a_sequence_is_refused() {
        let fpp = FakeFpp::start();
        let dir = tempfile::tempdir().unwrap();
        let fake = file(&dir, "notes.fseq", 1000);
        let err = run(
            &fpp,
            &fake,
            &request(from_file(&fake), None, PlaylistChoice::None),
        )
        .unwrap_err();
        assert_eq!(err, "notes.fseq isn't a sequence file the FPP can play.");
        assert!(fpp.state().requests.is_empty());
    }

    #[test]
    fn names_from_the_window_never_become_paths() {
        let fpp = FakeFpp::start();
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "a.fseq", 1000, None);
        let mut request = request(
            from_file(&fseq),
            None,
            PlaylistChoice::New("../../settings".into()),
        );
        request.sequence_name = "../../etc/passwd".into();
        let result = run(&fpp, &fseq, &request).unwrap();
        assert_eq!(result.sequence_name, "etc passwd.fseq");
        assert_eq!(result.playlist.as_deref(), Some("settings"));
        // Replacing takes the name as it is: only a plain file name will do.
        request.replace_sequence = true;
        request.sequence_name = "../../etc/passwd.fseq".into();
        let err = run(&fpp, &fseq, &request).unwrap_err();
        assert!(err.contains("because of the characters in its name"), "{err}");
    }

    #[test]
    fn not_enough_room_stops_before_sending() {
        // Room for the file once, but FPP briefly needs it twice while putting it together.
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "Show.fseq", 1000, None);
        let size = std::fs::metadata(&fseq).unwrap().len();
        let fpp = FakeFpp::start().with_free_bytes(size * 3 / 2);
        let err = run(
            &fpp,
            &fseq,
            &request(from_file(&fseq), None, PlaylistChoice::None),
        )
        .unwrap_err();
        assert!(err.starts_with("There isn't room on the FPP"), "{err}");
        assert!(fpp.state().writes().is_empty());
    }

    #[test]
    fn cancelling_before_the_commit_point_leaves_the_fpp_as_it_was() {
        let fpp = FakeFpp::start()
            .with_sequence("Show.fseq", 3)
            .with_music("Song.mp3", 3);
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "Show.fseq", 1000, None);
        let music = file(&dir, "Song.mp3", 9 * 1024 * 1024);
        let mut request = request(from_file(&fseq), Some(&music), PlaylistChoice::New("Show".into()));
        request.replace_sequence = true;
        request.replace_music = true;
        let cancels = AtomicU64::new(0);
        // Cancelled while the music goes up: the sequence is already in the upload folder.
        let err = send(
            &client(),
            fpp.address(),
            Prepared::File(fseq.clone()),
            &request,
            &cancels,
            0,
            |p| {
                if p.step == "music" && p.percent >= 50 {
                    cancels.fetch_add(1, Ordering::AcqRel);
                }
            },
        )
        .unwrap_err();
        assert_eq!(err, "The upload was cancelled. Nothing on the FPP was changed.");
        std::thread::sleep(Duration::from_millis(150));
        let state = fpp.state();
        assert_eq!(
            (state.sequences["Show.fseq"].size, state.music["Song.mp3"].size),
            (3, 3)
        );
        assert!(state.playlists.is_empty());
        assert_eq!(
            state.upload_bytes("Show.fseq") + state.upload_bytes("Song.mp3"),
            0,
            "{:?}",
            state.uploads
        );
    }

    #[test]
    fn cancelling_after_everything_is_staged_leaves_uploads_clean() {
        // FPP 9.3 behaviour in the fake: pieces were appended and are gone once the file is put
        // together, and deleting a missing file answers "Invalid path…".
        let fpp = FakeFpp::start().with_sequence("Show.fseq", 3);
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "Show.fseq", 1000, None);
        let music = file(&dir, "Song.mp3", 9 * 1024 * 1024);
        let mut request = request(from_file(&fseq), Some(&music), PlaylistChoice::None);
        request.replace_sequence = true;
        let cancels = AtomicU64::new(0);
        let err = send(
            &client(),
            fpp.address(),
            Prepared::File(fseq.clone()),
            &request,
            &cancels,
            0,
            |p| {
                if p.step == "music" && p.percent == 100 {
                    cancels.fetch_add(1, Ordering::AcqRel);
                }
            },
        )
        .unwrap_err();
        assert_eq!(err, "The upload was cancelled. Nothing on the FPP was changed.");
        let state = fpp.state();
        assert!(state.uploads.is_empty(), "{:?}", state.uploads);
        assert_eq!(state.sequences["Show.fseq"].size, 3);
        assert!(state.music.is_empty());
    }

    #[test]
    fn what_couldnt_be_removed_is_named() {
        let fpp = FakeFpp::start();
        fpp.state().refuse_deletes = true;
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "Show.fseq", 1000, None);
        let cancels = AtomicU64::new(0);
        let err = send(
            &client(),
            fpp.address(),
            Prepared::File(fseq.clone()),
            &request(from_file(&fseq), None, PlaylistChoice::None),
            &cancels,
            0,
            |p| {
                if p.step == "sequence" && p.percent == 100 {
                    cancels.fetch_add(1, Ordering::AcqRel);
                }
            },
        )
        .unwrap_err();
        assert_eq!(
            err,
            "The upload was cancelled. Nothing on the FPP was changed. These may be left in the FPP's File Manager, under Uploads: Show.fseq. You can delete them there."
        );
    }

    #[test]
    fn cancel_after_the_commit_point_finishes_the_send() {
        let fpp = FakeFpp::start();
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "Show.fseq", 1000, None);
        let cancels = AtomicU64::new(0);
        let result = send(
            &client(),
            fpp.address(),
            Prepared::File(fseq.clone()),
            &request(from_file(&fseq), None, PlaylistChoice::None),
            &cancels,
            0,
            |p| {
                if p.step == "commit" {
                    cancels.fetch_add(1, Ordering::AcqRel);
                }
            },
        )
        .unwrap();
        assert_eq!(result.sequence_name, "Show.fseq");
        assert!(fpp.state().sequences.contains_key("Show.fseq"));
    }

    #[test]
    fn a_failure_after_the_commit_point_says_whats_on_the_fpp_now() {
        let fpp = FakeFpp::start().with_music("Song.mp3", 3);
        fpp.state().fail_move = Some("Show.fseq".into());
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "Show.fseq", 1000, None);
        let music = file(&dir, "Song.mp3", 1000);
        let mut request = request(from_file(&fseq), Some(&music), PlaylistChoice::None);
        request.replace_music = true;
        let err = run(&fpp, &fseq, &request).unwrap_err();
        assert!(
            err.starts_with(
                "Only part of the send finished: Song.mp3 is on the FPP now (it replaced the one there). The FPP couldn't store Show.fseq."
            ),
            "{err}"
        );
        assert_eq!(
            fpp.state().upload_bytes("Show.fseq"),
            0,
            "the unmoved file is removed"
        );
    }

    #[test]
    fn an_unreachable_fpp_is_said_plainly() {
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let host = format!("127.0.0.1:{port}");
        let dir = tempfile::tempdir().unwrap();
        let fseq = fseq(&dir, "Show.fseq", 1000, None);
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

    #[test]
    fn relative_music_is_only_the_open_sequences_own() {
        let open = SendSource::OpenSequence { name: "A".into() };
        let resolved = resolve_music(&open, Some(PathArg("Song.mp3".into())), || {
            Some("/doc/Song.mp3".into())
        })
        .unwrap();
        assert_eq!(resolved.unwrap().0, PathBuf::from("/doc/Song.mp3"));
        let file = from_file(Path::new("/shows/A.fseq"));
        let err = resolve_music(&file, Some(PathArg("Song.mp3".into())), || panic!("not asked")).unwrap_err();
        assert!(err.contains("can't find the music Song.mp3"), "{err}");
        let full = resolve_music(&file, Some(PathArg("/m/Song.mp3".into())), || None).unwrap();
        assert_eq!(full.unwrap().0, PathBuf::from("/m/Song.mp3"));
    }
}
