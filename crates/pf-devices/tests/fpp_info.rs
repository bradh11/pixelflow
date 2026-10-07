//! Reading what's on an FPP and its schedule, over real HTTP against the fake FPP on 127.0.0.1.

use pf_devices::fpp_info::{self, FppFolder, ScheduleKind};
use pf_devices::fpp_player;
use pf_devices::testing::{FakeFpp, SECRET_ENDPOINTS};
use pf_devices::HttpClient;
use serde_json::json;
use std::time::Duration;

fn client() -> HttpClient {
    HttpClient::new(Duration::from_secs(2))
}

/// Reading never changes anything, and never asks for anything that holds passwords.
fn read_only(fpp: &FakeFpp) {
    let state = fpp.state();
    assert!(state.writes().is_empty(), "wrote {:?}", state.writes());
    for request in &state.requests {
        for forbidden in SECRET_ENDPOINTS {
            assert!(!request.contains(forbidden), "requested {request}");
        }
    }
}

fn stocked() -> FakeFpp {
    let fpp = FakeFpp::start()
        .with_sequence("Christmas Medley 2017.fseq", 24_000_000)
        .with_duration("Christmas Medley 2017.fseq", 371_000)
        .with_sequence("Wizards.fseq", 9_000_000)
        .with_music("Christmas Medley 2017.mp3", 8_900_000)
        .with_duration("Christmas Medley 2017.mp3", 371_000)
        .with_music("Wizards.mp3", 4_000_000)
        .with_playlist("Christmas Show");
    fpp.state().playlists.get_mut("Christmas Show").unwrap()["mainPlaylist"] = json!([
        {"type": "both", "sequenceName": "Christmas Medley 2017.fseq", "mediaName": "Christmas Medley 2017.mp3"},
        {"type": "sequence", "sequenceName": "Wizards.fseq"}
    ]);
    fpp.state().playlists.get_mut("Christmas Show").unwrap()["playlistInfo"] =
        json!({"total_duration": 551.5, "total_items": 2});
    fpp
}

#[test]
fn lists_sequences_with_length_size_and_date() {
    let fpp = stocked();
    let files = fpp_info::list(&client(), fpp.address(), FppFolder::Sequences).unwrap();
    let rows: Vec<_> = files
        .iter()
        .map(|f| (f.name.as_str(), f.duration_ms, f.size_bytes, f.modified.as_deref(), f.channels))
        .collect();
    assert_eq!(
        rows,
        vec![
            (
                "Christmas Medley 2017.fseq",
                Some(371_000),
                Some(24_000_000),
                Some("2026-10-06 18:05"),
                Some(6147)
            ),
            // A sequence with no frames has no length to show.
            ("Wizards.fseq", None, Some(9_000_000), Some("2026-10-06 18:05"), Some(6147)),
        ]
    );
    read_only(&fpp);
}

#[test]
fn lists_music_with_its_play_time_and_playlists_with_their_items() {
    let fpp = stocked();
    let music = fpp_info::list(&client(), fpp.address(), FppFolder::Music).unwrap();
    assert_eq!(
        music.iter().map(|f| (f.name.as_str(), f.duration_ms)).collect::<Vec<_>>(),
        vec![("Christmas Medley 2017.mp3", Some(371_000)), ("Wizards.mp3", None)]
    );
    let playlists = fpp_info::list(&client(), fpp.address(), FppFolder::Playlists).unwrap();
    assert_eq!(playlists.len(), 1);
    let show = &playlists[0];
    assert_eq!(show.name, "Christmas Show");
    assert_eq!((show.items, show.duration_ms), (Some(2), Some(551_500)));
    assert!(show.size_bytes.unwrap() > 0);
    assert_eq!(show.modified.as_deref(), Some("2026-10-06 18:05"));
    read_only(&fpp);
}

#[test]
fn a_playlist_without_saved_totals_counts_its_items() {
    let fpp = FakeFpp::start().with_playlist("Porch");
    fpp.state().playlists.get_mut("Porch").unwrap()["mainPlaylist"] =
        json!([{"type": "sequence", "sequenceName": "A.fseq"}]);
    let playlists = fpp_info::list(&client(), fpp.address(), FppFolder::Playlists).unwrap();
    assert_eq!((playlists[0].items, playlists[0].duration_ms), (Some(1), None));
}

#[test]
fn reads_the_schedule_keeping_only_when_and_what() {
    let fpp = FakeFpp::start().with_schedule(json!([
        {"enabled": 1, "sequence": 0, "day": 7, "playlist": "Christmas Show",
         "startTime": "17:30:00", "startTimeOffset": 0, "endTime": "22:00:00", "endTimeOffset": 0,
         "repeat": 1, "startDate": "2026-11-25", "endDate": "2027-01-06", "stopType": 0},
        {"enabled": 0, "sequence": 1, "day": 65_536 + 0x4000 + 0x100, "playlist": "Wizards.fseq",
         "startTime": "SunSet", "startTimeOffset": "15", "endTime": "23:00:00",
         "repeat": 1000, "startDate": "", "endDate": "", "stopType": 1},
        {"enabled": 1, "day": 7, "playlist": "", "command": "URL Command",
         "args": ["http://user:secret@192.0.2.50/x", "GET", ""],
         "startTime": "06:00:00", "endTime": "06:00:00", "repeat": 0, "stopType": 0}
    ]));
    let entries = fpp_info::schedule(&client(), fpp.address()).unwrap();
    assert_eq!(entries.len(), 3);
    let first = &entries[0];
    assert!(first.enabled);
    assert_eq!((first.kind, first.name.as_str(), first.day), (ScheduleKind::Playlist, "Christmas Show", 7));
    assert_eq!((first.start_time.as_str(), first.end_time.as_str()), ("17:30:00", "22:00:00"));
    assert_eq!((first.start_date.as_str(), first.end_date.as_str()), ("2026-11-25", "2027-01-06"));
    assert_eq!((first.repeat, first.stop_type), (1, 0));
    let second = &entries[1];
    assert!(!second.enabled);
    assert_eq!((second.kind, second.start_time.as_str(), second.start_offset), (ScheduleKind::Sequence, "SunSet", 15));
    assert_eq!((second.repeat, second.stop_type, second.day), (1000, 1, 0x14100));
    // A command entry keeps its name only: its arguments (which can hold anything) are dropped.
    assert_eq!((entries[2].kind, entries[2].name.as_str()), (ScheduleKind::Command, "URL Command"));
    assert!(!serde_json::to_string(&entries).unwrap().contains("secret"));
    read_only(&fpp);
}

#[test]
fn no_schedule_is_an_empty_list() {
    let fpp = FakeFpp::start();
    assert!(fpp_info::schedule(&client(), fpp.address()).unwrap().is_empty());
}

#[test]
fn an_unreachable_output_target_shows_in_the_status_warnings() {
    let fpp = FakeFpp::start().with_unreachable_target("DDP", "192.0.2.20", "Falcon_F16V5_B9F5");
    let status = fpp_player::status(&client(), fpp.address()).unwrap();
    assert_eq!(status.warnings, vec!["Cannot Ping DDP Channel Data Target 192.0.2.20 Falcon_F16V5_B9F5"]);
    read_only(&fpp);
}
