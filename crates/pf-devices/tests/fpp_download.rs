//! Downloading from an FPP, end to end over real HTTP against a fake FPP on 127.0.0.1 that
//! answers file downloads as FPP 9.5.3's `GetFile()` does.

use pf_devices::HttpClient;
use pf_devices::fpp_download::{self, DownloadError};
use pf_devices::fpp_info::FppFolder;
use pf_devices::testing::{FakeFpp, SECRET_ENDPOINTS};
use std::time::{Duration, Instant};

fn client() -> HttpClient {
    HttpClient::for_uploads_with(Duration::from_secs(2), Duration::from_secs(10))
}

/// Bytes that differ from position to position, so a misplaced piece shows.
fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 % 251) as u8).collect()
}

/// What's in `dir`, hidden files included.
fn files_in(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Only GETs of the listings, a sequence's details, and file downloads; nothing that returns
/// passwords.
fn read_only(fpp: &FakeFpp) {
    let state = fpp.state();
    assert_eq!(state.writes(), Vec::<String>::new());
    for request in &state.requests {
        for forbidden in SECRET_ENDPOINTS {
            assert!(!request.contains(forbidden), "requested {request}");
        }
        assert!(
            request.starts_with("GET /api/files/")
                || request.starts_with("GET /api/file/sequences/")
                || request.starts_with("GET /api/file/music/")
                || (request.starts_with("GET /api/sequence/") && request.ends_with("/meta")),
            "requested {request}"
        );
    }
}

#[test]
fn reads_the_sequence_and_finds_its_music_from_the_mf_path() {
    let fpp = FakeFpp::start()
        .with_sequence_file("Wizards in Winter.fseq", &pattern(4000))
        .with_sequence_media(
            "Wizards in Winter.fseq",
            r"C:\Users\Me\xLights\Audio\wizards in winter.MP3",
        )
        .with_music_file("Wizards in Winter.mp3", &pattern(900))
        .with_music_file("Other.mp3", &pattern(10));
    let found = fpp_download::read_sequence(&client(), fpp.address(), "Wizards in Winter.fseq").unwrap();
    assert_eq!(found.file.size_bytes, Some(4000));
    assert_eq!(found.file.channels, Some(6147));
    assert_eq!(
        found.media.as_deref(),
        Some(r"C:\Users\Me\xLights\Audio\wizards in winter.MP3")
    );
    let music = found.music.unwrap();
    assert_eq!(
        (music.name.as_str(), music.size_bytes),
        ("Wizards in Winter.mp3", Some(900))
    );

    // Music the FPP doesn't have, or none named.
    let fpp = fpp
        .with_sequence_file("Silent.fseq", &pattern(10))
        .with_sequence_media("Wizards in Winter.fseq", "/Shows/Gone.mp3");
    let found = fpp_download::read_sequence(&client(), fpp.address(), "Wizards in Winter.fseq").unwrap();
    assert_eq!(
        (found.media.as_deref(), found.music),
        (Some("/Shows/Gone.mp3"), None)
    );
    let silent = fpp_download::read_sequence(&client(), fpp.address(), "Silent.fseq").unwrap();
    assert_eq!((silent.media, silent.music), (None, None));

    assert_eq!(
        fpp_download::read_sequence(&client(), fpp.address(), "Nope.fseq"),
        Err(DownloadError::Gone {
            name: "Nope.fseq".into()
        })
    );
    read_only(&fpp);
}

#[test]
fn streams_a_file_whole_with_progress_and_places_it() {
    let data = pattern(300_000);
    let fpp = FakeFpp::start().with_sequence_file("Medley 2017.fseq", &data);
    let dir = tempfile::tempdir().unwrap();
    let mut seen = Vec::new();
    let got = fpp_download::download(
        &client(),
        fpp.address(),
        FppFolder::Sequences,
        "Medley 2017.fseq",
        dir.path(),
        Some(data.len() as u64),
        &mut |done| {
            seen.push(done);
            true
        },
    )
    .unwrap();
    assert_eq!(got.bytes, data.len() as u64);
    assert!(seen.len() > 2, "progress as it arrives: {seen:?}");
    assert!(seen.windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(seen.last(), Some(&(data.len() as u64)));
    // Waiting, whole, in a hidden file until placed.
    assert_eq!(files_in(dir.path()).len(), 1);
    assert!(files_in(dir.path())[0].starts_with('.'));
    let to = dir.path().join("Medley 2017.fseq");
    got.place(&to).unwrap();
    assert_eq!(std::fs::read(&to).unwrap(), data);
    assert_eq!(files_in(dir.path()), vec!["Medley 2017.fseq"]);
    assert_eq!(
        fpp.state().requests,
        vec!["GET /api/file/sequences/Medley%202017.fseq"]
    );
    read_only(&fpp);
}

#[test]
fn a_download_not_placed_leaves_nothing() {
    let fpp = FakeFpp::start().with_music_file("Song.mp3", &pattern(5000));
    let dir = tempfile::tempdir().unwrap();
    let got = fpp_download::download(
        &client(),
        fpp.address(),
        FppFolder::Music,
        "Song.mp3",
        dir.path(),
        None,
        &mut |_| true,
    )
    .unwrap();
    drop(got);
    assert_eq!(files_in(dir.path()), Vec::<String>::new());
}

#[test]
fn cancelling_a_slow_download_stops_it_and_leaves_nothing() {
    let fpp = FakeFpp::start().with_sequence_file("Big.fseq", &pattern(2_000_000));
    fpp.state().download_delay = Duration::from_millis(20);
    let dir = tempfile::tempdir().unwrap();
    let started = Instant::now();
    let result = fpp_download::download(
        &client(),
        fpp.address(),
        FppFolder::Sequences,
        "Big.fseq",
        dir.path(),
        Some(2_000_000),
        &mut |done| done < 100_000,
    );
    assert_eq!(result.unwrap_err(), DownloadError::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "stopped early, not after the whole file ({:?})",
        started.elapsed()
    );
    assert_eq!(files_in(dir.path()), Vec::<String>::new());
}

#[test]
fn a_download_cut_off_partway_fails_and_leaves_nothing() {
    let fpp = FakeFpp::start().with_sequence_file("Cut.fseq", &pattern(500_000));
    fpp.state().cut_downloads_at = Some(200_000);
    let dir = tempfile::tempdir().unwrap();
    let error = fpp_download::download(
        &client(),
        fpp.address(),
        FppFolder::Sequences,
        "Cut.fseq",
        dir.path(),
        Some(500_000),
        &mut |_| true,
    )
    .unwrap_err();
    assert!(matches!(error, DownloadError::Unreachable { .. }), "{error:?}");
    assert!(error.to_string().contains("Nothing was saved."), "{error}");
    assert_eq!(files_in(dir.path()), Vec::<String>::new());
}

#[test]
fn a_file_gone_from_the_fpp_says_so_and_leaves_nothing() {
    let fpp = FakeFpp::start();
    let dir = tempfile::tempdir().unwrap();
    let error = fpp_download::download(
        &client(),
        fpp.address(),
        FppFolder::Music,
        "Gone.mp3",
        dir.path(),
        None,
        &mut |_| true,
    )
    .unwrap_err();
    assert_eq!(
        error,
        DownloadError::Gone {
            name: "Gone.mp3".into()
        }
    );
    assert_eq!(files_in(dir.path()), Vec::<String>::new());
}

#[test]
fn names_with_paths_are_never_requested() {
    let fpp = FakeFpp::start();
    let dir = tempfile::tempdir().unwrap();
    for name in ["../settings", "../../config/settings", ".htaccess"] {
        let error = fpp_download::download(
            &client(),
            fpp.address(),
            FppFolder::Sequences,
            name,
            dir.path(),
            None,
            &mut |_| true,
        )
        .unwrap_err();
        assert!(matches!(error, DownloadError::BadName { .. }), "{error:?}");
    }
    assert_eq!(fpp.state().requests, Vec::<String>::new());
    assert_eq!(files_in(dir.path()), Vec::<String>::new());
}
