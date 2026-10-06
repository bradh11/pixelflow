//! Sending files to an FPP, end to end over real HTTP against a fake FPP on 127.0.0.1.

use pf_devices::fpp_upload::{
    self, CHUNK_BYTES, PlaylistChoice, UploadError, fpp_file_name, keep_both_name, playlist_entry,
    playlist_name,
};
use pf_devices::testing::{FakeFpp, SECRET_ENDPOINTS, StoredFile};
use pf_devices::{Http, HttpClient};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn client() -> HttpClient {
    HttpClient::for_uploads_with(Duration::from_secs(2), Duration::from_secs(10))
}

/// Bytes that differ from position to position, so a misplaced chunk changes the checksum.
fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 % 251) as u8).collect()
}

fn temp_file(dir: &tempfile::TempDir, name: &str, data: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::File::create(&path).unwrap().write_all(data).unwrap();
    path
}

fn no_secrets(fpp: &FakeFpp) {
    for request in &fpp.state().requests {
        for forbidden in SECRET_ENDPOINTS {
            assert!(!request.contains(forbidden), "requested {request}");
        }
    }
}

#[test]
fn a_sequence_goes_up_in_chunks_and_is_moved_into_place() {
    let fpp = FakeFpp::start();
    let dir = tempfile::tempdir().unwrap();
    let data = pattern(10 * 1024 * 1024 + 123);
    let path = temp_file(&dir, "local.fseq", &data);
    let mut seen = Vec::new();
    fpp_upload::upload(
        &client(),
        fpp.address(),
        &path,
        "Show.fseq",
        &mut |done, total| {
            seen.push((done, total));
            true
        },
    )
    .unwrap();

    let state = fpp.state();
    assert_eq!(state.sequences.get("Show.fseq"), Some(&StoredFile::of(&data)));
    assert!(state.uploaded.is_empty(), "moved out of the upload folder");
    assert!(state.largest_body <= CHUNK_BYTES);
    let total = data.len() as u64;
    assert_eq!(
        state.writes(),
        vec![
            format!("PATCH /api/file/uploads Show.fseq@0+{CHUNK_BYTES}"),
            format!("PATCH /api/file/uploads Show.fseq@{CHUNK_BYTES}+{CHUNK_BYTES}"),
            format!(
                "PATCH /api/file/uploads Show.fseq@{}+{}",
                2 * CHUNK_BYTES,
                total - 2 * CHUNK_BYTES
            ),
        ]
    );
    assert!(
        state
            .requests
            .contains(&"GET /api/file/move/Show.fseq".to_string())
    );
    // Progress only goes forward and ends at the whole file.
    assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0), "{seen:?}");
    assert_eq!(seen.last(), Some(&(total, total)));
    assert!(seen.len() > 3, "more often than once per chunk");
}

#[test]
fn names_are_encoded_in_urls_and_sent_as_is_in_headers() {
    let fpp = FakeFpp::start();
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "a.mp3", b"music");
    fpp_upload::upload(
        &client(),
        fpp.address(),
        &path,
        "Jingle Bells (Live).mp3",
        &mut |_, _| true,
    )
    .unwrap();
    let state = fpp.state();
    assert!(state.music.contains_key("Jingle Bells (Live).mp3"));
    assert!(
        state
            .requests
            .contains(&"GET /api/file/move/Jingle%20Bells%20%28Live%29.mp3".to_string()),
        "{:?}",
        state.requests
    );
}

#[test]
fn uploading_over_a_file_replaces_it() {
    let fpp = FakeFpp::start().with_sequence("Show.fseq", 5);
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(1000));
    fpp_upload::upload(&client(), fpp.address(), &path, "Show.fseq", &mut |_, _| true).unwrap();
    assert_eq!(fpp.state().sequences["Show.fseq"], StoredFile::of(&pattern(1000)));
}

#[test]
fn a_hundred_megabyte_sequence_streams_without_trouble() {
    let fpp = FakeFpp::start();
    let dir = tempfile::tempdir().unwrap();
    let size = 100 * 1024 * 1024 + 7;
    let path = dir.path().join("big.fseq");
    {
        // Written in pieces, so the test itself never holds 100 MB either.
        let mut file = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
        let block = pattern(1024 * 1024);
        for _ in 0..100 {
            file.write_all(&block).unwrap();
        }
        file.write_all(&block[..7]).unwrap();
    }
    let started = Instant::now();
    let mut last = 0;
    fpp_upload::upload(&client(), fpp.address(), &path, "Big.fseq", &mut |done, _| {
        last = done;
        true
    })
    .unwrap();
    let state = fpp.state();
    assert_eq!(state.sequences["Big.fseq"].size, size as u64);
    assert_eq!(last, size as u64);
    assert!(
        state.largest_body <= CHUNK_BYTES,
        "each request carries one chunk"
    );
    let patches = state.requests.iter().filter(|r| r.starts_with("PATCH")).count() as u64;
    assert_eq!(patches, (size as u64).div_ceil(CHUNK_BYTES));
    eprintln!("100 MB upload over loopback took {:?}", started.elapsed());
}

#[test]
fn cancelling_stops_the_upload_leaves_the_old_file_and_tidies_up() {
    let fpp = FakeFpp::start().with_sequence("Show.fseq", 5);
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(9 * 1024 * 1024));
    let err = fpp_upload::upload(&client(), fpp.address(), &path, "Show.fseq", &mut |done, _| {
        done < 5 * 1024 * 1024
    })
    .unwrap_err();
    assert_eq!(err, UploadError::Cancelled);
    assert_eq!(err.to_string(), "The upload was cancelled.");
    // Give the fake a moment to notice the dropped connection.
    std::thread::sleep(Duration::from_millis(100));
    let state = fpp.state();
    assert_eq!(
        state.sequences["Show.fseq"].size, 5,
        "the FPP's copy is untouched"
    );
    assert_eq!(
        state.partial_bytes("Show.fseq"),
        None,
        "the partial upload was removed"
    );
    assert!(
        state
            .requests
            .contains(&"DELETE /api/file/uploads/Show.fseq.patch.0".to_string()),
        "{:?}",
        state.requests
    );
    assert!(!state.requests.iter().any(|r| r.contains("/api/file/move/")));
}

#[test]
fn a_full_disk_is_reported_plainly_and_the_partial_file_removed() {
    let fpp = FakeFpp::start().with_free_bytes(1024 * 1024);
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(3 * 1024 * 1024));
    let err = fpp_upload::upload(&client(), fpp.address(), &path, "Show.fseq", &mut |_, _| true).unwrap_err();
    assert!(matches!(err, UploadError::Full { .. }), "{err:?}");
    let message = err.to_string();
    assert!(message.contains("storage is full"), "{message}");
    assert!(message.contains("Show.fseq"), "{message}");
    let state = fpp.state();
    assert!(state.sequences.is_empty());
    assert_eq!(state.partial_bytes("Show.fseq"), None);
}

#[test]
fn not_enough_room_is_found_before_sending_anything() {
    let free = Some(2 * 1024 * 1024);
    let err = fpp_upload::ensure_room(free, 3 * 1024 * 1024).unwrap_err();
    assert!(matches!(err, UploadError::NoRoom { .. }), "{err:?}");
    let message = err.to_string();
    assert!(
        message.contains("3.0 MB") && message.contains("2.0 MB"),
        "{message}"
    );
    assert!(fpp_upload::ensure_room(free, 1024).is_ok());
    assert!(
        fpp_upload::ensure_room(None, u64::MAX).is_ok(),
        "unknown free space doesn't block"
    );
}

#[test]
fn an_fpp_that_fails_the_upload_says_so() {
    let fpp = FakeFpp::start();
    fpp.state().fail_uploads = Some(500);
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", b"data");
    let err = fpp_upload::upload(&client(), fpp.address(), &path, "Show.fseq", &mut |_, _| true).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("couldn't store Show.fseq"), "{message}");
    assert!(message.contains("500"), "{message}");
}

#[test]
fn an_unreachable_fpp_is_reported_plainly() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let host = format!("127.0.0.1:{port}");
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", b"data");
    let err = fpp_upload::upload(&client(), &host, &path, "Show.fseq", &mut |_, _| true).unwrap_err();
    assert!(matches!(err, UploadError::Unreachable { .. }), "{err:?}");
    let message = err.to_string();
    assert!(
        message.starts_with(&format!("Couldn't reach the FPP at {host}")),
        "{message}"
    );
}

#[test]
fn a_stalled_fpp_times_out() {
    let fpp = FakeFpp::start();
    fpp.state().read_delay = Duration::from_millis(400);
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(CHUNK_BYTES as usize));
    let quick = HttpClient::for_uploads_with(Duration::from_secs(1), Duration::from_millis(500));
    let started = Instant::now();
    let err = fpp_upload::upload(&quick, fpp.address(), &path, "Show.fseq", &mut |_, _| true).unwrap_err();
    assert!(matches!(err, UploadError::TimedOut { .. }), "{err:?}");
    assert!(err.to_string().contains("stopped answering"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(5), "gave up promptly");
}

#[test]
fn a_missing_local_file_is_reported() {
    let fpp = FakeFpp::start();
    let err = fpp_upload::upload(
        &client(),
        fpp.address(),
        std::path::Path::new("/nowhere/Show.fseq"),
        "Show.fseq",
        &mut |_, _| true,
    )
    .unwrap_err();
    assert!(err.to_string().starts_with("Couldn't read Show.fseq"), "{err}");
    assert!(fpp.state().requests.is_empty());
}

#[test]
fn reading_what_is_on_the_fpp_only_reads() {
    let fpp = FakeFpp::start()
        .with_sequence("Show.fseq", 10)
        .with_music("Song.mp3", 10)
        .with_playlist("Main")
        .with_free_bytes(12_345);
    let files = fpp_upload::read_files(&client(), fpp.address()).unwrap();
    assert_eq!(files.sequences, vec!["Show.fseq"]);
    assert_eq!(files.media, vec!["Song.mp3"]);
    assert_eq!(files.playlists, vec!["Main"]);
    assert_eq!(files.free_bytes, Some(12_345));
    assert!(fpp.state().writes().is_empty());
    no_secrets(&fpp);

    let check = files.check_sequence("Show.fseq");
    assert!(check.exists);
    assert_eq!(check.keep_both_name, "Show (2).fseq");
    assert!(!files.check_sequence("Other.fseq").exists);
    assert!(files.check_media("Song.mp3").exists);
}

#[test]
fn sequence_names_takes_one_request() {
    let fpp = FakeFpp::start()
        .with_sequence("A.fseq", 1)
        .with_sequence("B.fseq", 1);
    let names = fpp_upload::sequence_names(&client(), fpp.address()).unwrap();
    assert_eq!(names, vec!["A.fseq", "B.fseq"]);
    assert_eq!(fpp.state().requests, vec!["GET /api/sequence"]);
}

#[test]
fn file_names_are_ones_fpp_keeps_as_they_are() {
    assert_eq!(
        fpp_file_name("Christmas Medley 2017", "fseq"),
        "Christmas Medley 2017.fseq"
    );
    assert_eq!(
        fpp_file_name("Rock'n \"Roll\"/Mix?", "fseq"),
        "Rockn Roll Mix.fseq"
    );
    assert_eq!(fpp_file_name("Café ✓", "fseq"), "Caf.fseq");
    assert_eq!(fpp_file_name("..a..b..", "mp3"), "a.b.mp3");
    assert_eq!(fpp_file_name("   ", "fseq"), "Sequence.fseq");
    assert_eq!(fpp_file_name("Song.MP3", "mp3"), "Song.mp3");
    assert_eq!(fpp_file_name("Song.mp3", "mp3"), "Song.mp3");
    assert_eq!(playlist_name("Rock'n Roll (2025)!"), "Rockn Roll 2025");
    assert_eq!(playlist_name("✓"), "PixelFlow");
}

#[test]
fn keep_both_picks_the_next_free_number() {
    let taken = ["Show.fseq", "Show (2).fseq"];
    assert_eq!(
        keep_both_name("Show.fseq", |n| taken.contains(&n)),
        "Show (3).fseq"
    );
    assert_eq!(keep_both_name("Song.mp3", |_| false), "Song (2).mp3");
    assert_eq!(keep_both_name("README", |_| false), "README (2)");
}

#[test]
fn music_fpp_can_play_is_recognised() {
    for name in ["a.mp3", "a.OGG", "a.m4a", "a.wav", "a.flac", "a.aac"] {
        assert!(fpp_upload::is_music(name), "{name}");
    }
    for name in ["a.fseq", "a.txt", "mp3"] {
        assert!(!fpp_upload::is_music(name), "{name}");
    }
}

#[test]
fn a_new_playlist_is_made_with_the_sequence_and_its_music() {
    let fpp = FakeFpp::start();
    let http = client();
    let entry = playlist_entry("Show.fseq", Some("Song.mp3"), 125.5);
    let done = fpp_upload::put_on_playlist(&http, fpp.address(), &PlaylistChoice::New("Show".into()), &entry)
        .unwrap();
    assert_eq!(done.as_deref(), Some("Show"));
    let state = fpp.state();
    let playlist = &state.playlists["Show"];
    assert_eq!(playlist["name"], "Show");
    assert_eq!(playlist["mainPlaylist"][0]["type"], "both");
    assert_eq!(playlist["mainPlaylist"][0]["sequenceName"], "Show.fseq");
    assert_eq!(playlist["mainPlaylist"][0]["mediaName"], "Song.mp3");
    assert_eq!(playlist["mainPlaylist"][0]["enabled"], 1);
    assert_eq!(playlist["playlistInfo"]["total_items"], 1);
    assert_eq!(playlist["empty"], false);
    assert!(state.requests.contains(&"POST /api/playlist/Show".to_string()));
}

#[test]
fn a_sequence_without_music_is_a_sequence_entry() {
    let entry = playlist_entry("Show.fseq", None, 10.0);
    assert_eq!(entry["type"], "sequence");
    assert!(entry.get("mediaName").is_none());
}

#[test]
fn adding_to_an_existing_playlist_appends_once() {
    let fpp = FakeFpp::start().with_playlist("Main Show");
    let http = client();
    let entry = playlist_entry("Show.fseq", Some("Song.mp3"), 10.0);
    let choice = PlaylistChoice::Existing("Main Show".into());
    fpp_upload::put_on_playlist(&http, fpp.address(), &choice, &entry).unwrap();
    fpp_upload::put_on_playlist(&http, fpp.address(), &choice, &entry).unwrap();
    let state = fpp.state();
    let main = state.playlists["Main Show"]["mainPlaylist"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(main.len(), 1, "sending again doesn't add it twice");
    assert!(
        state
            .requests
            .contains(&"POST /api/playlist/Main%20Show/mainPlaylist/item".to_string()),
        "{:?}",
        state.requests
    );
}

#[test]
fn a_new_playlist_whose_name_is_taken_is_added_to_instead() {
    let fpp = FakeFpp::start().with_playlist("Show");
    fpp.state().playlists.get_mut("Show").unwrap()["mainPlaylist"] =
        serde_json::json!([{"type": "sequence", "sequenceName": "Other.fseq"}]);
    let entry = playlist_entry("Show.fseq", None, 10.0);
    fpp_upload::put_on_playlist(
        &client(),
        fpp.address(),
        &PlaylistChoice::New("Show".into()),
        &entry,
    )
    .unwrap();
    let main = fpp.state().playlists["Show"]["mainPlaylist"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(main.len(), 2, "the other sequence stays: {main:?}");
}

#[test]
fn no_playlist_means_no_playlist_requests() {
    let fpp = FakeFpp::start();
    let entry = playlist_entry("Show.fseq", None, 10.0);
    let done = fpp_upload::put_on_playlist(&client(), fpp.address(), &PlaylistChoice::None, &entry).unwrap();
    assert_eq!(done, None);
    assert!(fpp.state().requests.is_empty());
}

#[test]
fn a_playlist_name_is_never_a_path() {
    let fpp = FakeFpp::start();
    let entry = playlist_entry("Show.fseq", None, 10.0);
    for name in ["../settings", "a/b", "a\\b", " "] {
        for choice in [PlaylistChoice::Existing(name.into()), PlaylistChoice::New(name.into())] {
            assert!(fpp_upload::put_on_playlist(&client(), fpp.address(), &choice, &entry).is_err());
        }
    }
    assert!(fpp.state().requests.is_empty(), "{:?}", fpp.state().requests);
}

#[test]
fn a_missing_existing_playlist_is_an_error() {
    let fpp = FakeFpp::start();
    let entry = playlist_entry("Show.fseq", None, 10.0);
    let err = fpp_upload::put_on_playlist(
        &client(),
        fpp.address(),
        &PlaylistChoice::Existing("Gone".into()),
        &entry,
    )
    .unwrap_err();
    assert!(err.to_string().contains("Gone"), "{err}");
}

#[test]
fn the_recorded_fake_cannot_upload() {
    // The recorded-response fake has no uploads; only the real client streams.
    let http = pf_devices::FakeHttp::new();
    let mut body: &[u8] = b"x";
    assert!(http.send_body("PATCH", "h", "/p", &[], &mut body, 1).is_err());
}
