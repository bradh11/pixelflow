//! FPP player status, sequences, and user-initiated playback control, against recorded responses.

use pf_devices::fpp_player::{self, PlayerState};
use pf_devices::testing::{FPP, assert_no_secret_endpoints, network};

#[test]
fn status_reports_what_is_playing_and_whats_next() {
    let http = network();
    let status = fpp_player::status(&http, FPP).unwrap();
    assert_eq!(status.state, PlayerState::Playing);
    assert_eq!(status.playlist.as_deref(), Some("Christmas Medley 2017.fseq"));
    assert_eq!(status.sequence.as_deref(), Some("Christmas Medley 2017.fseq"));
    assert_eq!((status.seconds_elapsed, status.seconds_remaining), (109, 456));
    assert_eq!(
        status.next_playlist.as_deref(),
        Some("Christmas Medley 2017.fseq")
    );
    assert_eq!(
        status.next_start.as_deref(),
        Some("Mon Oct  5 @ 06:48 PM - (Everyday)")
    );
    assert_eq!(
        status.warnings,
        vec!["Cannot Ping DDP Channel Data Target 192.0.2.20 Falcon_F16V5_B9F5"]
    );
    assert_no_secret_endpoints(&http);
}

#[test]
fn an_idle_player_has_nothing_current() {
    let idle = r#"{"status_name": "idle", "current_playlist": {"playlist": ""}, "current_sequence": "",
        "seconds_elapsed": "0", "seconds_remaining": "0", "next_playlist": {"playlist": "No playlist scheduled.", "start_time": ""}}"#;
    let http = network().with_get(FPP, "/api/fppd/status", idle);
    let status = fpp_player::status(&http, FPP).unwrap();
    assert_eq!(status.state, PlayerState::Idle);
    assert_eq!(
        (status.playlist, status.sequence, status.next_playlist),
        (None, None, None)
    );
    assert!(status.warnings.is_empty());
}

#[test]
fn player_states_are_recognized() {
    for (name, state) in [
        ("paused", PlayerState::Paused),
        ("stopping gracefully", PlayerState::Stopping),
        ("stopping gracefully after loop", PlayerState::Stopping),
        ("testing", PlayerState::Other),
    ] {
        let body = format!(r#"{{"status_name": "{name}"}}"#);
        let http = network().with_get(FPP, "/api/fppd/status", &body);
        assert_eq!(fpp_player::status(&http, FPP).unwrap().state, state, "{name}");
    }
}

#[test]
fn sequences_list_their_length_and_channels() {
    let http = network();
    let sequences = fpp_player::sequences(&http, FPP).unwrap();
    assert_eq!(sequences.len(), 1);
    let medley = &sequences[0];
    assert_eq!(medley.name, "Christmas Medley 2017");
    assert_eq!(
        (medley.frames, medley.step_ms, medley.channels),
        (11332, 50, 6148)
    );
    assert_eq!(medley.duration_ms(), 566_600);
    assert!(
        http.requests()
            .contains(&format!("GET {FPP}/api/sequence/Christmas%20Medley%202017/meta")),
        "names are URL-encoded: {:?}",
        http.requests()
    );
    assert_no_secret_endpoints(&http);
}

#[test]
fn a_sequence_without_metadata_is_still_listed() {
    let http = network()
        .with_get(FPP, "/api/sequence", r#"["Medley", "Broken"]"#)
        .with_get(
            FPP,
            "/api/sequence/Medley/meta",
            r#"{"NumFrames": 10, "StepTime": 25, "ChannelCount": 30}"#,
        );
    let sequences = fpp_player::sequences(&http, FPP).unwrap();
    assert_eq!(sequences.len(), 2);
    assert_eq!((sequences[1].name.as_str(), sequences[1].frames), ("Broken", 0));
}

#[test]
fn playback_control_sends_fpp_commands() {
    let start = r#"{"command":"Start Playlist","args":["Christmas Medley 2017.fseq","false","false"]}"#;
    let stop_now = r#"{"command":"Stop Now","args":[]}"#;
    let stop_gracefully = r#"{"command":"Stop Gracefully","args":["false"]}"#;
    let http = network()
        .with_post(FPP, "/api/command", start, "Playlist Starting")
        .with_post(FPP, "/api/command", stop_now, "Stopped")
        .with_post(FPP, "/api/command", stop_gracefully, "Stopping");
    fpp_player::start(&http, FPP, "Christmas Medley 2017.fseq").unwrap();
    fpp_player::stop(&http, FPP, false).unwrap();
    fpp_player::stop(&http, FPP, true).unwrap();
    let posts: Vec<_> = http
        .requests()
        .into_iter()
        .filter(|r| r.starts_with("POST"))
        .collect();
    assert_eq!(posts.len(), 3);
    assert_no_secret_endpoints(&http);
}

#[test]
fn reading_never_sends_commands() {
    let http = network();
    fpp_player::status(&http, FPP).unwrap();
    fpp_player::sequences(&http, FPP).unwrap();
    assert!(
        http.requests().iter().all(|r| r.starts_with("GET")),
        "{:?}",
        http.requests()
    );
}
