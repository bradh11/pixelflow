//! Cisco's OpenH264 video encoder in the app (see `pf_video::cisco`): what Settings and the
//! export dialog show about it, the download (only ever after the user's click), turning it off
//! and on, and removing it. PixelFlow ships no H.264 encoder of its own.

use crate::Reply;
use crate::progress::Throttle;
use pf_video::cisco::{self, CodecError, State as CodecState, Store};
use pf_video::{Encoder, VideoError};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Runtime, State};

/// The event the encoder's download reports its progress with.
pub(crate) const H264_DOWNLOAD_PROGRESS_EVENT: &str = "h264-download-progress";

/// Fetches the encoder into its store, telling its second argument the bytes so far and the
/// bytes in all, until done or its third says to stop.
type Fetch =
    dyn Fn(&Store, &dyn Fn(u64, u64), &dyn Fn() -> bool) -> Result<PathBuf, CodecError> + Send + Sync;

/// The encoder: where it's kept, the download in progress, and how it's fetched.
pub(crate) struct H264State {
    /// None when the app has no data folder.
    store: Option<Store>,
    /// The download in progress: set to stop it.
    downloading: Mutex<Option<Arc<AtomicBool>>>,
    fetch: Arc<Fetch>,
    /// What exports encode with in the shell's tests: OpenH264 compiled in, nothing downloaded.
    #[cfg(test)]
    stand_in: Option<Encoder>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl H264State {
    /// The encoder for this computer, kept in the app's data folder, from Cisco's server.
    pub(crate) fn live(data_dir: Option<PathBuf>) -> Self {
        Self {
            store: data_dir.map(|dir| Store::new(&dir)),
            downloading: Mutex::default(),
            fetch: Arc::new(Store::download),
            #[cfg(test)]
            stand_in: None,
        }
    }

    /// Kept in `store`, fetched by `fetch` instead of from Cisco.
    #[cfg(test)]
    pub(crate) fn with_fetch(store: Option<Store>, fetch: Arc<Fetch>) -> Self {
        Self {
            store,
            downloading: Mutex::default(),
            fetch,
            stand_in: None,
        }
    }

    /// Exports encode with OpenH264 compiled into the tests (nothing is downloaded).
    #[cfg(test)]
    pub(crate) fn compiled_in() -> Self {
        Self {
            stand_in: Encoder::compiled_in(),
            ..Self::with_fetch(None, Arc::new(|_, _, _| Err(CodecError::Offline)))
        }
    }

    /// What an export with the built-in encoder encodes with: Cisco's library, when it's here
    /// and on (checked again as the export loads it).
    pub(crate) fn encoder(&self) -> Result<Encoder, CodecError> {
        #[cfg(test)]
        if let Some(encoder) = &self.stand_in {
            return Ok(encoder.clone());
        }
        let store = self.store.as_ref().ok_or(CodecError::Unsupported)?;
        store.ready().map(Encoder::OpenH264)
    }

    /// After an export: a library found changed since it was checked is removed, so the next
    /// export asks to download it again.
    pub(crate) fn after_export(&self, error: &VideoError) {
        if let (VideoError::Codec(CodecError::Changed), Some(store)) = (error, &self.store) {
            log::warn!("Cisco's OpenH264 library changed since it was checked; removing it");
            if let Err(e) = store.remove() {
                log::warn!("removing the changed OpenH264 library: {e:?}");
            }
        }
    }

    fn view(&self) -> H264Encoder {
        let binary = self.store.as_ref().and_then(Store::binary);
        let (state, problem) = match &self.store {
            None => (
                CodecState::Unsupported,
                Some("PixelFlow has no folder on this computer to keep the encoder in.".to_string()),
            ),
            Some(store) => {
                let state = store.state();
                let problem = (state == CodecState::Unsupported).then(|| CodecError::Unsupported.to_string());
                (state, problem)
            }
        };
        H264Encoder {
            state,
            downloading: lock(&self.downloading).is_some(),
            version: cisco::VERSION,
            host: cisco::HOST,
            url: binary.map(|b| b.url),
            bytes: binary.map_or(0, |b| b.download_bytes),
            attribution: cisco::ATTRIBUTION,
            licence: cisco::LICENCE,
            licence_url: cisco::LICENCE_URL,
            problem,
        }
    }
}

/// Cisco's OpenH264 encoder, as Settings and the export dialog show it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct H264Encoder {
    pub state: CodecState,
    pub downloading: bool,
    /// Cisco's release used.
    pub version: &'static str,
    /// Cisco's server.
    pub host: &'static str,
    /// Where it's downloaded from; none on a system Cisco makes it for none.
    pub url: Option<&'static str>,
    /// The download's size, about.
    pub bytes: u64,
    /// The line Cisco's licence asks to be shown where it's turned on and off.
    pub attribution: &'static str,
    /// Cisco's licence for its binary, word for word.
    pub licence: &'static str,
    pub licence_url: &'static str,
    /// Why it can't be used on this computer, when it can't.
    pub problem: Option<String>,
}

/// How far the encoder's download has got.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct H264Progress {
    pub received: u64,
    pub total: u64,
}

/// Marks the download as over when dropped.
struct Downloading<'a>(&'a Mutex<Option<Arc<AtomicBool>>>);

impl Drop for Downloading<'_> {
    fn drop(&mut self) {
        *lock(self.0) = None;
    }
}

/// Cisco's OpenH264 encoder: whether it's here and on, and what Cisco's licence asks be shown.
#[tauri::command]
pub(crate) async fn h264_encoder(h264: State<'_, H264State>) -> Reply<H264Encoder> {
    Ok(h264.view())
}

/// Downloads Cisco's OpenH264 encoder for this computer from Cisco's server (only ever when the
/// user has clicked to), checks it against its pinned SHA-256, keeps it, and turns it on.
/// Reports its progress ([`H264_DOWNLOAD_PROGRESS_EVENT`]); [`cancel_h264_download`] stops it,
/// keeping nothing.
#[tauri::command]
pub(crate) async fn download_h264_encoder<R: Runtime>(
    app: AppHandle<R>,
    h264: State<'_, H264State>,
) -> Reply<H264Encoder> {
    let Some(store) = h264.store.clone() else {
        return Err(h264.view().problem.unwrap_or_default());
    };
    let stop = {
        let mut downloading = lock(&h264.downloading);
        if downloading.is_some() {
            return Err("The encoder is already downloading.".to_string());
        }
        let stop = Arc::new(AtomicBool::new(false));
        *downloading = Some(Arc::clone(&stop));
        stop
    };
    let marker = Downloading(&h264.downloading);
    let fetch = Arc::clone(&h264.fetch);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let throttle = Throttle::default();
        fetch(
            &store,
            &|received, total| {
                let fraction = received as f32 / total.max(1) as f32;
                if throttle.due(fraction, Instant::now()) {
                    // A window that's gone can't show progress; the download carries on.
                    let _ = app.emit(H264_DOWNLOAD_PROGRESS_EVENT, H264Progress { received, total });
                }
            },
            &|| stop.load(Ordering::Acquire),
        )
    })
    .await
    .map_err(|_| "Something went wrong downloading the encoder.".to_string())?;
    drop(marker);
    match result {
        Ok(path) => {
            log::info!("kept Cisco's OpenH264 {} at {}", cisco::VERSION, path.display());
            Ok(h264.view())
        }
        Err(CodecError::Cancelled) => Err(CodecError::Cancelled.to_string()),
        Err(error) => {
            log::warn!("downloading Cisco's OpenH264: {error:?}");
            Err(error.to_string())
        }
    }
}

/// Stops the encoder's download (nothing half-downloaded is kept).
#[tauri::command]
pub(crate) async fn cancel_h264_download(h264: State<'_, H264State>) -> Reply<()> {
    if let Some(stop) = lock(&h264.downloading).as_ref() {
        stop.store(true, Ordering::Release);
    }
    Ok(())
}

/// Turns Cisco's encoder on or off for video export (kept across runs).
#[tauri::command]
pub(crate) async fn set_h264_encoder_enabled(h264: State<'_, H264State>, on: bool) -> Reply<H264Encoder> {
    let store = h264
        .store
        .as_ref()
        .ok_or_else(|| h264.view().problem.unwrap_or_default())?;
    store.set_enabled(on).map_err(|e| {
        log::warn!("turning OpenH264 {}: {e:?}", if on { "on" } else { "off" });
        e.to_string()
    })?;
    Ok(h264.view())
}

/// Deletes Cisco's encoder from the app's data folder.
#[tauri::command]
pub(crate) async fn remove_h264_encoder(h264: State<'_, H264State>) -> Reply<H264Encoder> {
    if lock(&h264.downloading).is_some() {
        return Err("Wait for the download to finish, or stop it, first.".to_string());
    }
    if let Some(store) = &h264.store {
        store.remove().map_err(|e| {
            log::warn!("removing OpenH264: {e:?}");
            "The encoder couldn't be removed. If a video is exporting, wait for it to finish, then try again."
                .to_string()
        })?;
    }
    Ok(h264.view())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{call, files_under, heard};
    use pf_video::cisco::Binary;
    use serde_json::{Value, json};
    use tauri::test::MockRuntime;
    use tauri::{App, Listener, Manager, WebviewWindow, WebviewWindowBuilder};

    /// "not really a library", compressed with bzip2.
    const PACKED: &[u8] = &[
        0x42, 0x5a, 0x68, 0x39, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59, 0xd9, 0x37, 0xe5, 0x47, 0x00, 0x00, 0x06,
        0x11, 0x80, 0x40, 0x00, 0x32, 0x25, 0x94, 0x20, 0x20, 0x00, 0x22, 0x01, 0xa6, 0x8d, 0x08, 0x06, 0x9a,
        0x68, 0x52, 0xa8, 0x23, 0x1b, 0x79, 0x27, 0xb8, 0x05, 0x87, 0xc5, 0xdc, 0x91, 0x4e, 0x14, 0x24, 0x36,
        0x4d, 0xf9, 0x51, 0xc0,
    ];
    const FAKE: Binary = Binary {
        os: "test",
        arch: "test",
        file: "libfake.dylib",
        url: "https://ciscobinary.openh264.org/libfake.dylib.bz2",
        sha256: "8c94411e7c8b87bd238af6ff286f5e356e89b0b393364163b58e2ca433a40fe5",
        bytes: 20,
        download_bytes: 55,
    };

    /// Hands out one byte per read, so a download reports its progress many times.
    struct Trickle(&'static [u8]);

    impl std::io::Read for Trickle {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            match (self.0.split_first(), buf.first_mut()) {
                (Some((byte, rest)), Some(slot)) => {
                    *slot = *byte;
                    self.0 = rest;
                    Ok(1)
                }
                _ => Ok(0),
            }
        }
    }

    fn app_with(state: H264State) -> (App<MockRuntime>, WebviewWindow<MockRuntime>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let app = crate::testing::builder(dir.path())
            .manage(state)
            .build(crate::context())
            .unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        (app, webview, dir)
    }

    fn state_of(webview: &WebviewWindow<MockRuntime>) -> (String, bool) {
        let view = call(webview, "h264_encoder", json!({})).unwrap();
        (
            view["state"].as_str().unwrap().to_string(),
            view["downloading"].as_bool().unwrap(),
        )
    }

    #[test]
    fn the_download_reports_its_bytes_stops_keeping_nothing_and_can_be_turned_off_and_removed() {
        let data = tempfile::tempdir().unwrap();
        let store = Store::for_binary(data.path(), Some(&FAKE));
        let fetch: Arc<Fetch> = Arc::new(|store, progress, stop| {
            store.install_from(&mut Trickle(PACKED), PACKED.len() as u64, progress, stop)
        });
        let (app, webview, _dir) = app_with(H264State::with_fetch(Some(store.clone()), fetch));
        let view = call(&webview, "h264_encoder", json!({})).unwrap();
        assert_eq!(view["state"], "missing");
        assert_eq!(view["url"], FAKE.url);
        assert_eq!(view["bytes"], 55);
        assert_eq!(
            view["attribution"],
            "OpenH264 Video Codec provided by Cisco Systems, Inc."
        );
        assert!(
            view["licence"]
                .as_str()
                .unwrap()
                .contains("AVC/H.264 Patent Portfolio License")
        );
        assert_eq!(view["problem"], Value::Null);
        let events = heard(&app, H264_DOWNLOAD_PROGRESS_EVENT);
        // Nothing is downloading: stopping does nothing, now or to the download after it.
        call(&webview, "cancel_h264_download", json!({})).unwrap();

        // As the first bytes are reported: it's downloading, a second download and a removal
        // are refused, and then it's stopped.
        let during: Arc<Mutex<Vec<Value>>> = Arc::default();
        let seen = Arc::clone(&during);
        let watcher = webview.clone();
        app.once_any(H264_DOWNLOAD_PROGRESS_EVENT, move |_| {
            let refused = |command: &str| call(&watcher, command, json!({})).unwrap_err();
            *lock(&seen) = vec![
                json!(state_of(&watcher)),
                refused("download_h264_encoder"),
                refused("remove_h264_encoder"),
            ];
            call(&watcher, "cancel_h264_download", json!({})).unwrap();
        });
        let stopped = call(&webview, "download_h264_encoder", json!({})).unwrap_err();
        assert_eq!(stopped, "Stopped.");
        assert_eq!(
            *lock(&during),
            [
                json!(["missing", true]),
                json!("The encoder is already downloading."),
                json!("Wait for the download to finish, or stop it, first."),
            ]
        );
        assert_eq!(
            std::mem::take(&mut *lock(&events)),
            [json!({ "received": 1, "total": 55 })]
        );
        assert_eq!(files_under(data.path()), Vec::<String>::new());
        assert_eq!(state_of(&webview), ("missing".to_string(), false));

        // The stop is spent: the next download runs to its last byte, keeps the library, and
        // turns it on.
        let done = call(&webview, "download_h264_encoder", json!({})).unwrap();
        assert_eq!(
            (&done["state"], &done["downloading"]),
            (&json!("ready"), &json!(false))
        );
        let sent = lock(&events).clone();
        assert_eq!(sent.last().unwrap(), &json!({ "received": 55, "total": 55 }));
        assert_eq!(files_under(data.path()), ["openh264/2.6.0/libfake.dylib"]);

        // Off, then on again; an export is refused while it's off.
        let off = call(&webview, "set_h264_encoder_enabled", json!({ "on": false })).unwrap();
        assert_eq!(off["state"], "off");
        let h264 = app.state::<H264State>();
        assert_eq!(
            h264.encoder().unwrap_err().to_string(),
            "Cisco's OpenH264 video encoder is turned off in Settings, under Video export."
        );
        let on = call(&webview, "set_h264_encoder_enabled", json!({ "on": true })).unwrap();
        assert_eq!(on["state"], "ready");
        assert_eq!(
            h264.encoder().unwrap(),
            Encoder::OpenH264(data.path().join("openh264/2.6.0/libfake.dylib"))
        );

        // Removed: gone, and the next export would ask again.
        let removed = call(&webview, "remove_h264_encoder", json!({})).unwrap();
        assert_eq!(removed["state"], "missing");
        assert_eq!(files_under(data.path()), Vec::<String>::new());
        assert!(matches!(h264.encoder(), Err(CodecError::Missing)));
    }

    #[test]
    fn a_failed_download_says_why_in_words_and_keeps_nothing() {
        let data = tempfile::tempdir().unwrap();
        let store = Store::for_binary(data.path(), Some(&FAKE));
        let fetch: Arc<Fetch> = Arc::new(|_, _, _| Err(CodecError::Offline));
        let (_app, webview, _dir) = app_with(H264State::with_fetch(Some(store), fetch));
        let error = call(&webview, "download_h264_encoder", json!({})).unwrap_err();
        assert_eq!(
            error,
            "PixelFlow couldn't reach Cisco's server (ciscobinary.openh264.org). Check that this computer is online, then try again."
        );
        assert_eq!(state_of(&webview), ("missing".to_string(), false));
        assert_eq!(files_under(data.path()), Vec::<String>::new());
    }

    #[test]
    fn a_system_cisco_makes_nothing_for_says_so() {
        let data = tempfile::tempdir().unwrap();
        let store = Store::for_binary(data.path(), None);
        let fetch: Arc<Fetch> = Arc::new(|store, _, _| store.ready());
        let (_app, webview, _dir) = app_with(H264State::with_fetch(Some(store), fetch));
        let view = call(&webview, "h264_encoder", json!({})).unwrap();
        assert_eq!(view["state"], "unsupported");
        assert_eq!(view["url"], Value::Null);
        assert!(
            view["problem"]
                .as_str()
                .unwrap()
                .starts_with("Cisco doesn't make its OpenH264 encoder for this system")
        );
        // Without a data folder, it says that instead.
        let (_app, webview, _dir) = app_with(H264State::live(None));
        let view = call(&webview, "h264_encoder", json!({})).unwrap();
        assert_eq!(view["state"], "unsupported");
        assert_eq!(
            call(&webview, "download_h264_encoder", json!({})).unwrap_err(),
            "PixelFlow has no folder on this computer to keep the encoder in."
        );
    }

    #[test]
    fn a_library_changed_since_it_was_checked_is_removed_after_the_export_refuses_it() {
        let data = tempfile::tempdir().unwrap();
        let store = Store::for_binary(data.path(), Some(&FAKE));
        store
            .install_from(&mut Trickle(PACKED), 55, &|_, _| {}, &|| false)
            .unwrap();
        let h264 = H264State::with_fetch(Some(store.clone()), Arc::new(|_, _, _| Err(CodecError::Offline)));
        h264.after_export(&VideoError::Cancelled);
        assert_eq!(store.state(), CodecState::Ready);
        h264.after_export(&VideoError::Codec(CodecError::Changed));
        assert_eq!(store.state(), CodecState::Missing);
        assert_eq!(files_under(data.path()), Vec::<String>::new());
    }
}
