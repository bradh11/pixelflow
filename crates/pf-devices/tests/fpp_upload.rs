//! Sending files to an FPP, end to end over real HTTP against a fake FPP on 127.0.0.1 that
//! behaves like FPP 9.3's PHP (and FPP 10's per-chunk pieces when asked).

use pf_devices::fpp_upload::{
    self, CHUNK_BYTES, DETAIL, LayoutBlock, PlaylistChoice, UploadError, fpp_file_name, keep_both_name,
    layout_warnings, playlist_entry, playlist_name,
};
use pf_devices::testing::{FakeFpp, SECRET_ENDPOINTS, StoredFile};
use pf_devices::{Destination, Http, HttpClient};
use serde_json::json;
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

/// Waits for the fake to notice a dropped connection.
fn settle() {
    std::thread::sleep(Duration::from_millis(150));
}

#[test]
fn staging_puts_the_file_in_the_upload_folder_and_checks_its_size_before_anything_moves() {
    let fpp = FakeFpp::start().with_sequence("Show.fseq", 5);
    let dir = tempfile::tempdir().unwrap();
    let data = pattern(10 * 1024 * 1024 + 123);
    let path = temp_file(&dir, "local.fseq", &data);
    let mut seen = Vec::new();
    let staged = fpp_upload::stage(
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
    {
        let state = fpp.state();
        assert_eq!(
            state.sequences["Show.fseq"].size, 5,
            "nothing replaced while staging"
        );
        assert_eq!(state.upload_bytes("Show.fseq"), data.len() as u64);
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
        assert_eq!(
            state.requests.last().unwrap(),
            "GET /api/files/uploads",
            "size checked last"
        );
        assert!(state.largest_body <= CHUNK_BYTES);
        assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0), "{seen:?}");
        assert_eq!(seen.last(), Some(&(total, total)));
    }
    fpp_upload::commit(&client(), fpp.address(), &staged).unwrap();
    let state = fpp.state();
    assert_eq!(state.sequences["Show.fseq"], StoredFile::of(&data));
    assert_eq!(state.upload_bytes("Show.fseq"), 0);
    assert!(
        state
            .requests
            .contains(&"GET /api/file/move/Show.fseq".to_string())
    );
}

#[test]
fn fpp_10s_separate_chunk_files_are_put_together_too() {
    let fpp = FakeFpp::start();
    fpp.state().chunk_files = true;
    let dir = tempfile::tempdir().unwrap();
    let data = pattern(9 * 1024 * 1024);
    let path = temp_file(&dir, "a.fseq", &data);
    fpp_upload::upload(&client(), fpp.address(), &path, "Show.fseq", &mut |_, _| true).unwrap();
    assert_eq!(fpp.state().sequences["Show.fseq"], StoredFile::of(&data));
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
    fpp_upload::upload(&client(), fpp.address(), &path, "Big.fseq", &mut |_, _| true).unwrap();
    let state = fpp.state();
    assert_eq!(state.sequences["Big.fseq"].size, size as u64);
    assert!(
        state.largest_body <= CHUNK_BYTES,
        "each request carries one chunk"
    );
    let patches = state.requests.iter().filter(|r| r.starts_with("PATCH")).count() as u64;
    assert_eq!(patches, (size as u64).div_ceil(CHUNK_BYTES));
    eprintln!("100 MB upload over loopback took {:?}", started.elapsed());
}

#[test]
fn cancelling_leaves_the_fpp_as_it_was() {
    let fpp = FakeFpp::start().with_sequence("Show.fseq", 5);
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(9 * 1024 * 1024));
    let err = fpp_upload::stage(&client(), fpp.address(), &path, "Show.fseq", &mut |done, _| {
        done < 5 * 1024 * 1024
    })
    .unwrap_err();
    assert_eq!(err, UploadError::Cancelled);
    assert_eq!(
        err.to_string(),
        "The upload was cancelled. Nothing on the FPP was changed."
    );
    settle();
    let state = fpp.state();
    assert_eq!(state.sequences["Show.fseq"].size, 5);
    assert_eq!(state.upload_bytes("Show.fseq"), 0, "{:?}", state.uploads);
    assert!(!state.requests.iter().any(|r| r.contains("/api/file/move/")));
}

#[test]
fn cancelling_on_fpp_10_removes_every_chunk_file() {
    let fpp = FakeFpp::start();
    fpp.state().chunk_files = true;
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(13 * 1024 * 1024));
    let err = fpp_upload::stage(&client(), fpp.address(), &path, "Show.fseq", &mut |done, _| {
        done < 9 * 1024 * 1024
    })
    .unwrap_err();
    assert_eq!(err, UploadError::Cancelled);
    settle();
    let state = fpp.state();
    assert_eq!(state.upload_bytes("Show.fseq"), 0, "{:?}", state.uploads);
    for offset in [0, CHUNK_BYTES, 2 * CHUNK_BYTES] {
        assert!(
            state
                .requests
                .contains(&format!("DELETE /api/file/uploads/Show.fseq.patch.{offset}")),
            "{:?}",
            state.requests
        );
    }
}

#[test]
fn cancelling_after_the_last_chunk_still_leaves_nothing_behind() {
    let fpp = FakeFpp::start().with_sequence("Show.fseq", 5);
    let dir = tempfile::tempdir().unwrap();
    let data = pattern(1000);
    let path = temp_file(&dir, "Show.fseq", &data);
    // Every byte is sent; the user cancels while FPP puts the file together.
    let err = fpp_upload::stage(
        &client(),
        fpp.address(),
        &path,
        "Show.fseq",
        &mut |done, total| done < total,
    )
    .unwrap_err();
    assert_eq!(err, UploadError::Cancelled);
    let state = fpp.state();
    assert_eq!(state.sequences["Show.fseq"].size, 5);
    assert_eq!(state.upload_bytes("Show.fseq"), 0, "{:?}", state.uploads);
    assert!(
        state
            .requests
            .contains(&"DELETE /api/file/uploads/Show.fseq".to_string())
    );
}

#[test]
fn a_dropped_connection_tidies_up_what_was_sent() {
    let fpp = FakeFpp::start();
    fpp.state().read_delay = Duration::from_millis(400);
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(CHUNK_BYTES as usize));
    let quick = HttpClient::for_uploads_with(Duration::from_secs(1), Duration::from_millis(500));
    let started = Instant::now();
    let err = fpp_upload::stage(&quick, fpp.address(), &path, "Show.fseq", &mut |_, _| true).unwrap_err();
    assert!(matches!(err, UploadError::TimedOut { .. }), "{err:?}");
    assert!(err.to_string().contains("stopped answering"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(5), "gave up promptly");
    fpp.state().read_delay = Duration::ZERO;
    settle();
    assert!(
        fpp.state()
            .requests
            .contains(&"DELETE /api/file/uploads/Show.fseq.patch.0".to_string())
    );
}

#[test]
fn what_couldnt_be_tidied_is_reported() {
    let fpp = FakeFpp::start();
    fpp.state().refuse_deletes = true;
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(6 * 1024 * 1024));
    let err = fpp_upload::stage(&client(), fpp.address(), &path, "Show.fseq", &mut |done, _| {
        done < 5 * 1024 * 1024
    })
    .unwrap_err();
    assert!(matches!(err, UploadError::LeftBehind { .. }), "{err:?}");
    let message = err.to_string();
    assert!(message.starts_with("The upload was cancelled."), "{message}");
    // The file left, by its real name in the upload folder.
    assert!(
        message.contains("These may be left in the FPP's File Manager, under Uploads: Show.fseq.patch.0."),
        "{message}"
    );
}

#[test]
fn cancelling_mid_upload_on_fpp_9_3_leaves_uploads_clean_with_no_false_warning() {
    // FPP 9.3 appends chunks to .patch.0, so .patch.<offset> pieces never exist, and it answers
    // "Invalid path…" (not "File Not Found") for a file that isn't there.
    let fpp = FakeFpp::start();
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(13 * 1024 * 1024));
    let err = fpp_upload::stage(&client(), fpp.address(), &path, "Show.fseq", &mut |done, _| {
        done < 9 * 1024 * 1024
    })
    .unwrap_err();
    assert_eq!(err, UploadError::Cancelled, "no false 'may be left' warning");
    let state = fpp.state();
    assert!(state.uploads.is_empty(), "{:?}", state.uploads);
    // Only what the upload folder listed was deleted; the never-whole file wasn't asked for.
    let deletes: Vec<&String> = state
        .requests
        .iter()
        .filter(|r| r.starts_with("DELETE"))
        .collect();
    assert_eq!(deletes, vec!["DELETE /api/file/uploads/Show.fseq.patch.0"]);
    assert!(state.requests.contains(&"GET /api/files/uploads".to_string()));
}

#[test]
fn a_staged_file_that_wont_be_moved_is_removed_cleanly() {
    let fpp = FakeFpp::start().with_sequence("Show.fseq", 5);
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(9 * 1024 * 1024));
    let staged = fpp_upload::stage(&client(), fpp.address(), &path, "Show.fseq", &mut |_, _| true).unwrap();
    assert_eq!(fpp.state().upload_bytes("Show.fseq"), 9 * 1024 * 1024);
    let left = fpp_upload::discard(&client(), fpp.address(), &staged);
    assert!(left.is_empty(), "{left:?}");
    let state = fpp.state();
    assert!(state.uploads.is_empty(), "{:?}", state.uploads);
    assert_eq!(state.sequences["Show.fseq"].size, 5);
}

#[test]
fn tidying_leaves_other_uploads_alone() {
    let fpp = FakeFpp::start();
    {
        let mut state = fpp.state();
        for other in ["Other.mp3", "Other.mp3.patch.0", "Show.fseq.old"] {
            state
                .uploads
                .insert(other.into(), pf_devices::testing::UploadFile::default());
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(6 * 1024 * 1024));
    let err = fpp_upload::stage(&client(), fpp.address(), &path, "Show.fseq", &mut |done, _| {
        done < 5 * 1024 * 1024
    })
    .unwrap_err();
    assert_eq!(err, UploadError::Cancelled);
    let names: Vec<String> = fpp.state().uploads.keys().cloned().collect();
    assert_eq!(names, vec!["Other.mp3", "Other.mp3.patch.0", "Show.fseq.old"]);
}

#[test]
fn more_bytes_held_than_sent_is_not_trusted() {
    // A stale piece the FPP counted too: the file can't be the one that was sent.
    let fpp = FakeFpp::start().with_sequence("Show.fseq", 5);
    fpp.state().extra_held = 10;
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(1000));
    let err = fpp_upload::stage(&client(), fpp.address(), &path, "Show.fseq", &mut |_, _| true).unwrap_err();
    assert!(matches!(err, UploadError::Rejected { .. }), "{err:?}");
    assert!(
        err.to_string().contains("more of Show.fseq than was sent"),
        "{err}"
    );
    let state = fpp.state();
    assert_eq!(state.sequences["Show.fseq"].size, 5);
    assert!(state.uploads.is_empty(), "{:?}", state.uploads);
}

#[test]
fn a_full_disk_is_reported_plainly_and_the_pieces_removed() {
    let fpp = FakeFpp::start().with_free_bytes(1024 * 1024);
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(3 * 1024 * 1024));
    let err = fpp_upload::stage(&client(), fpp.address(), &path, "Show.fseq", &mut |_, _| true).unwrap_err();
    assert!(matches!(err, UploadError::Full { .. }), "{err:?}");
    let message = err.to_string();
    assert!(message.starts_with("The FPP's storage is full"), "{message}");
    assert!(message.contains("Nothing on the FPP was replaced"), "{message}");
    let state = fpp.state();
    assert!(state.sequences.is_empty());
    assert_eq!(state.upload_bytes("Show.fseq"), 0);
}

#[test]
fn a_short_file_put_together_on_a_filling_disk_never_replaces_the_good_one() {
    // FPP 9.x answers "OK" with the full size even when its disk filled while it put the file
    // together; only the upload folder's listing shows the truth.
    let fpp = FakeFpp::start().with_sequence("Show.fseq", 5);
    fpp.state().short_assembly = Some(4096);
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(2 * 1024 * 1024));
    let err = fpp_upload::stage(&client(), fpp.address(), &path, "Show.fseq", &mut |_, _| true).unwrap_err();
    assert!(matches!(err, UploadError::Full { .. }), "{err:?}");
    assert!(err.to_string().contains("storage may be full"), "{err}");
    let state = fpp.state();
    assert_eq!(state.sequences["Show.fseq"].size, 5, "the good file stays");
    assert_eq!(state.upload_bytes("Show.fseq"), 0, "the short file is removed");
    assert!(!state.requests.iter().any(|r| r.contains("/api/file/move/")));
}

#[test]
fn php_warnings_ahead_of_the_answer_are_read_through() {
    let fpp = FakeFpp::start().with_free_bytes(512 * 1024);
    fpp.state().php_warning = Some(
        "<br />\n<b>Warning</b>:  file_put_contents(): Only 0 of 65536 bytes written, possibly out of free disk space in <b>/opt/fpp/www/api/controllers/files.php</b> on line <b>857</b><br />\n".into(),
    );
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", &pattern(2 * 1024 * 1024));
    let err = fpp_upload::stage(&client(), fpp.address(), &path, "Show.fseq", &mut |_, _| true).unwrap_err();
    let message = err.to_string();
    assert!(message.starts_with("The FPP's storage is full"), "{message}");
    let (_, detail) = message.split_once(DETAIL).expect("FPP's own words follow");
    assert!(detail.contains("possibly out of free disk space"), "{detail}");

    // And a normal answer behind a warning still counts.
    let fpp = FakeFpp::start();
    fpp.state().php_warning = Some("<b>Notice</b>: something harmless<br />\n".into());
    fpp_upload::upload(&client(), fpp.address(), &path, "Show.fseq", &mut |_, _| true).unwrap();
    assert!(fpp.state().sequences.contains_key("Show.fseq"));
}

#[test]
fn an_fpp_that_fails_the_upload_says_why() {
    let fpp = FakeFpp::start();
    fpp.state().fail_uploads = Some((
        500,
        r#"{"status":"failed","file":"Show.fseq","error":"Could not lock file for writing"}"#.into(),
    ));
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", b"data");
    let err = fpp_upload::stage(&client(), fpp.address(), &path, "Show.fseq", &mut |_, _| true).unwrap_err();
    let message = err.to_string();
    assert!(
        message.starts_with("The FPP couldn't store Show.fseq."),
        "{message}"
    );
    assert!(
        message.ends_with(&format!("{DETAIL}Could not lock file for writing")),
        "{message}"
    );
}

#[test]
fn fpp_10s_disk_full_answer_is_storage_full() {
    let fpp = FakeFpp::start();
    fpp.state().fail_uploads = Some((500, "Could not write file (disk full?)".into()));
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", b"data");
    let err = fpp_upload::stage(&client(), fpp.address(), &path, "Show.fseq", &mut |_, _| true).unwrap_err();
    assert!(matches!(err, UploadError::Full { .. }), "{err:?}");
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
fn an_unreachable_fpp_is_reported_plainly() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let host = format!("127.0.0.1:{port}");
    let dir = tempfile::tempdir().unwrap();
    let path = temp_file(&dir, "Show.fseq", b"data");
    let err = fpp_upload::stage(&client(), &host, &path, "Show.fseq", &mut |_, _| true).unwrap_err();
    assert!(matches!(err, UploadError::Unreachable { .. }), "{err:?}");
    let message = err.to_string();
    assert!(
        message.starts_with(&format!("Couldn't reach the FPP at {host}")),
        "{message}"
    );
}

#[test]
fn a_missing_local_file_is_reported() {
    let fpp = FakeFpp::start();
    let err = fpp_upload::stage(
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
fn reading_what_is_on_the_fpp_only_reads_and_keeps_its_spelling() {
    let fpp = FakeFpp::start()
        .with_sequence("Show.fseq", 10)
        .with_music("medley.mp3", 10)
        .with_playlist("Main")
        .with_free_bytes(12_345);
    let files = fpp_upload::read_files(&client(), fpp.address()).unwrap();
    assert_eq!(files.sequences, vec!["Show.fseq"]);
    assert_eq!(files.media, vec!["medley.mp3"]);
    assert_eq!(files.playlists, vec!["Main"]);
    assert_eq!(files.free_bytes, Some(12_345));
    assert!(fpp.state().writes().is_empty());
    no_secrets(&fpp);

    let check = files.check_sequence("Show.fseq");
    assert!(check.exists);
    assert_eq!(check.fpp_name.as_deref(), Some("Show.fseq"));
    assert_eq!(check.keep_both_name, "Show (2).fseq");
    let other = files.check_sequence("Other.fseq");
    assert!(!other.exists);
    assert_eq!(other.fpp_name, None);
    // A different spelling is still a clash, and the FPP's own spelling is kept for identity.
    let music = files.check_media("Medley.mp3");
    assert!(music.exists);
    assert_eq!(music.fpp_name.as_deref(), Some("medley.mp3"));
    assert_eq!(music.keep_both_name, "Medley (2).mp3");
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
    // Brackets confuse the cleanup FPP 9.x does with glob().
    assert_eq!(fpp_file_name("Song [Remix]", "mp3"), "Song Remix.mp3");
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
fn a_new_playlist_is_made_only_when_the_fpp_has_none_by_that_name() {
    let fpp = FakeFpp::start();
    let entry = playlist_entry("Show.fseq", Some("Song.mp3"), 125.5);
    let done = fpp_upload::put_on_playlist(
        &client(),
        fpp.address(),
        &PlaylistChoice::New("Show".into()),
        &entry,
    )
    .unwrap();
    assert_eq!(done.as_deref(), Some("Show"));
    let state = fpp.state();
    let playlist = &state.playlists["Show"];
    assert_eq!(playlist["name"], "Show");
    assert_eq!(playlist["mainPlaylist"][0]["type"], "both");
    assert_eq!(playlist["mainPlaylist"][0]["sequenceName"], "Show.fseq");
    assert_eq!(playlist["mainPlaylist"][0]["mediaName"], "Song.mp3");
    assert_eq!(playlist["playlistInfo"]["total_items"], 1);
    assert!(state.requests.contains(&"GET /api/playlists".to_string()));
    assert!(state.requests.contains(&"POST /api/playlist/Show".to_string()));
}

#[test]
fn a_new_playlist_never_replaces_one_that_exists() {
    let entry = playlist_entry("Show.fseq", None, 10.0);
    // Same name, other capitals, and one FPP can't read: each is refused, nothing written.
    for (existing, broken, asked) in [
        ("Show", false, "Show"),
        ("show", false, "Show"),
        ("Show", true, "Show"),
    ] {
        let fpp = FakeFpp::start();
        if broken {
            fpp.state().broken_playlists.push(existing.into());
        } else {
            let fpp_ref = &fpp;
            fpp_ref.state().playlists.insert(
                existing.into(),
                json!({"name": existing, "mainPlaylist": [{"type": "sequence", "sequenceName": "Other.fseq"}]}),
            );
        }
        let err = fpp_upload::put_on_playlist(
            &client(),
            fpp.address(),
            &PlaylistChoice::New(asked.into()),
            &entry,
        )
        .unwrap_err();
        assert!(err.to_string().contains("already has a playlist called"), "{err}");
        assert!(fpp.state().writes().is_empty(), "{:?}", fpp.state().writes());
    }
}

#[test]
fn adding_to_a_playlist_appends_one_item_and_leaves_the_rest_alone() {
    let fpp = FakeFpp::start();
    let original = json!({
        "name": "Main Show", "version": 3, "repeat": 0, "loopCount": 0, "desc": "The real show",
        "random": 0, "empty": false,
        "leadIn": [{"type": "media", "mediaName": "intro.mp3", "enabled": 1}],
        "mainPlaylist": [{"type": "both", "sequenceName": "Wizards.fseq", "mediaName": "Wizards.mp3", "enabled": 1}],
        "leadOut": [{"type": "pause", "duration": 5, "enabled": 1}],
        "playlistInfo": {"total_duration": 300, "total_items": 3}
    });
    fpp.state().playlists.insert("Main Show".into(), original.clone());
    let entry = playlist_entry("Show.fseq", Some("Song.mp3"), 10.0);
    let choice = PlaylistChoice::Existing("Main Show".into());
    fpp_upload::put_on_playlist(&client(), fpp.address(), &choice, &entry).unwrap();
    fpp_upload::put_on_playlist(&client(), fpp.address(), &choice, &entry).unwrap();
    let state = fpp.state();
    let mut expected = original;
    expected["mainPlaylist"].as_array_mut().unwrap().push(entry);
    assert_eq!(
        state.playlists["Main Show"], expected,
        "one item added, nothing else changed"
    );
    let item_posts = state
        .requests
        .iter()
        .filter(|r| *r == "POST /api/playlist/Main%20Show/mainPlaylist/item")
        .count();
    assert_eq!(
        item_posts, 1,
        "sending again doesn't add it twice: {:?}",
        state.requests
    );
    assert!(
        !state
            .requests
            .contains(&"POST /api/playlist/Main%20Show".to_string()),
        "never rewritten whole"
    );
}

#[test]
fn a_playlist_the_fpp_cant_read_is_never_added_to() {
    // FPP would rebuild it with only the new entry.
    let fpp = FakeFpp::start();
    fpp.state().broken_playlists.push("Main".into());
    let entry = playlist_entry("Show.fseq", None, 10.0);
    let err = fpp_upload::put_on_playlist(
        &client(),
        fpp.address(),
        &PlaylistChoice::Existing("Main".into()),
        &entry,
    )
    .unwrap_err();
    assert!(err.to_string().contains("can't read"), "{err}");
    assert!(fpp.state().writes().is_empty());
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
    assert!(err.to_string().contains("no playlist called"), "{err}");
    assert!(fpp.state().writes().is_empty());
}

#[test]
fn a_playlist_name_is_never_a_path() {
    let fpp = FakeFpp::start();
    let entry = playlist_entry("Show.fseq", None, 10.0);
    for name in ["../settings", "a/b", "a\\b", " "] {
        for choice in [
            PlaylistChoice::Existing(name.into()),
            PlaylistChoice::New(name.into()),
        ] {
            assert!(fpp_upload::put_on_playlist(&client(), fpp.address(), &choice, &entry).is_err());
        }
    }
    assert!(fpp.state().requests.is_empty(), "{:?}", fpp.state().requests);
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
fn a_sequence_without_music_is_a_sequence_entry() {
    let entry = playlist_entry("Show.fseq", None, 10.0);
    assert_eq!(entry["type"], "sequence");
    assert!(entry.get("mediaName").is_none());
}

fn falcon(start: u32, channels: u32) -> Destination {
    Destination {
        address: "192.0.2.20".into(),
        description: "Falcon".into(),
        protocol: "DDP".into(),
        channels,
        start_channel: start,
        start_universe: None,
        universe_size: None,
        ddp_raw: false,
        uneven_universes: false,
    }
}

#[test]
fn a_layout_that_matches_the_fpp_has_no_warnings() {
    let blocks = [LayoutBlock {
        name: "Falcon".into(),
        address: "192.0.2.20".into(),
        start: 1,
        count: 6147,
    }];
    assert!(layout_warnings(6147, &blocks, &[falcon(1, 6147)]).is_empty());
    // Nothing to compare with: no warning.
    assert!(layout_warnings(10, &[], &[]).is_empty());
}

#[test]
fn a_layout_that_differs_from_the_fpp_is_said_plainly() {
    let warnings = layout_warnings(5000, &[], &[falcon(1, 6147)]);
    assert_eq!(
        warnings,
        vec![
            "This sequence has 5,000 channels but the FPP sends 6,147. Lights past channel 5,000 will stay dark."
        ]
    );
    let warnings = layout_warnings(7000, &[], &[falcon(1, 6147)]);
    assert_eq!(
        warnings,
        vec![
            "This sequence has 7,000 channels but the FPP only sends 6,147. Channels past 6,147 won't reach any lights."
        ]
    );
    let blocks = [
        LayoutBlock {
            name: "Falcon".into(),
            address: "192.0.2.20".into(),
            start: 101,
            count: 6147,
        },
        LayoutBlock {
            name: "Porch".into(),
            address: "192.0.2.40".into(),
            start: 6248,
            count: 150,
        },
    ];
    let warnings = layout_warnings(6397, &blocks, &[falcon(1, 6147), falcon(6148, 250)]);
    assert!(
        warnings.contains(
            &"Falcon: this sequence puts it at channels 101–6,247, but the FPP sends it channels 1–6,147."
                .to_string()
        ),
        "{warnings:?}"
    );
    assert!(
        warnings
            .contains(&"Porch (192.0.2.40): the FPP doesn't send to it, so it won't light up.".to_string()),
        "{warnings:?}"
    );
}

#[test]
fn the_recorded_fake_cannot_upload() {
    // The recorded-response fake has no uploads; only the real client streams.
    let http = pf_devices::FakeHttp::new();
    let mut body: &[u8] = b"x";
    assert!(http.send_body("PATCH", "h", "/p", &[], &mut body, 1).is_err());
}
