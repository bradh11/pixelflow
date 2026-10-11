//! Replies about controllers: finding and importing them, comparing and sending a setup, and
//! moving sequences to and from an FPP. Recorded devices answer on documentation addresses;
//! fake ones listen on this computer.

use super::*;
use pf_devices::testing::{FALCON, FPP, FakeFpp, WLED};

fn ids(changes: &Value) -> Vec<String> {
    changes
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn device_replies() {
    let shell = Shell::new();
    let mut fixtures = Fixtures::new("devices", shell.stable());

    // Finding controllers, and looking at each kind before importing it.
    let found = shell.ok(
        "discover_devices",
        json!({ "hosts": [FPP, WLED], "network": false }),
    );
    assert!(found["devices"].as_array().unwrap().len() >= 3, "{found}");
    fixtures.reply("controllers found", "types.Discovery", &found);
    // A controller an FPP lists that doesn't answer, and one that asks for a password (built:
    // the recorded network has neither).
    let quiet = pf_devices::Discovery {
        devices: Vec::new(),
        silent: vec![pf_devices::SilentPeer {
            address: "192.0.2.77".into(),
            description: "Garage".into(),
            listed_by: "FPP".into(),
        }],
        locked: vec!["192.0.2.78".into()],
    };
    fixtures.reply(
        "controllers that didn't answer, or asked for a password (built)",
        "types.Discovery",
        &wire(&quiet),
    );
    for (name, address) in [
        ("a Falcon", FALCON),
        ("an FPP that only sends to others", FPP),
        ("a WLED", WLED),
    ] {
        let details = shell.ok("inspect_device", json!({ "address": address }));
        fixtures.reply(
            &format!("{name}, looked at before importing"),
            "types.DeviceDetails",
            &details,
        );
    }
    use pf_devices::DeviceInput;
    let inputs = vec![
        DeviceInput::Ddp,
        DeviceInput::Sacn {
            start_universe: 1,
            channels_per_universe: 510,
            universe_count: 4,
        },
        DeviceInput::Unsupported {
            description: "Art-Net".into(),
        },
    ];
    assert_every_variant(
        "device input",
        3,
        inputs.iter().map(|i| match i {
            DeviceInput::Ddp => 0,
            DeviceInput::Sacn { .. } => 1,
            DeviceInput::Unsupported { .. } => 2,
        }),
    );
    fixtures.all_of("types.DeviceInput", Some("type"), &wire(&inputs));

    // A prop named like one of the Falcon's strings is suggested for it; imported, the
    // controller is in the show.
    let details = shell.ok("inspect_device", json!({ "address": FALCON }));
    let string = details["plan"]["props"][0]["name"].clone();
    let mut named = strip(1, "x", 50);
    named["name"] = string;
    shell.ok(
        "apply_edits",
        json!({ "edits": [{ "type": "addProp", "prop": named }] }),
    );
    let suggesting = shell.ok("inspect_device", json!({ "address": FALCON }));
    assert!(
        !suggesting["plan"]["suggested"].as_object().unwrap().is_empty(),
        "{}",
        suggesting["plan"]
    );
    fixtures.reply(
        "a Falcon with a prop of the show suggested for a string",
        "types.DeviceDetails",
        &suggesting,
    );
    shell.ok("undo", json!({}));

    fixtures.reply(
        "which controllers answer",
        "types.ControllerCheck[]",
        &shell.ok(
            "check_controllers",
            json!({ "addresses": ["192.0.2.10", "192.0.2.11", "192.168.1.50", "porch.local"] }),
        ),
    );

    // What an FPP is doing and holds.
    fixtures.reply(
        "an FPP playing",
        "types.PlayerStatus",
        &shell.ok("fpp_status", json!({ "address": FPP })),
    );
    let idle = pf_devices::fpp_player::PlayerStatus {
        state: pf_devices::fpp_player::PlayerState::Idle,
        playlist: None,
        sequence: None,
        seconds_elapsed: 0,
        seconds_remaining: 0,
        next_playlist: Some("Evening".into()),
        next_start: Some("Sunday @ 17:00:00".into()),
        warnings: vec!["Could not reach 192.0.2.20".into()],
    };
    fixtures.reply(
        "an FPP waiting for its schedule, with a warning (built)",
        "types.PlayerStatus",
        &wire(&idle),
    );
    fixtures.reply(
        "an FPP's sequences",
        "types.FppSequence[]",
        &shell.ok("fpp_sequences", json!({ "address": FPP })),
    );
    fixtures.reply(
        "an FPP's sequence file names",
        "string[]",
        &shell.ok("fpp_sequence_names", json!({ "address": FPP })),
    );
    let fake = FakeFpp::start()
        .with_sequence("Show.fseq", 1000)
        .with_duration("Show.fseq", 60_000)
        .with_music("Show.mp3", 500)
        .with_playlist("Main")
        .with_schedule(json!([
            { "enabled": 1, "day": 7, "playlist": "Main", "startTime": "17:00:00", "endTime": "22:00:00",
              "startDate": "2026-12-01", "endDate": "2026-12-31", "repeat": 1, "stopType": 0 },
            { "enabled": 0, "day": 0, "sequence": 1, "playlist": "Show", "startTime": "SunSet", "startTimeOffset": -15,
              "endTime": "23:00:00", "repeat": 0, "stopType": 1 },
            { "enabled": 1, "day": 8, "command": "Volume Set", "startTime": "08:00:00", "endTime": "08:00:00" },
        ]));
    fixtures.swap(fake.address(), DEVICE);
    for folder in ["sequences", "music", "playlists"] {
        let files = shell.ok(
            "fpp_files",
            json!({ "address": fake.address(), "folder": folder }),
        );
        fixtures.reply(&format!("an FPP's {folder} folder"), "types.FppFile[]", &files);
    }
    let schedule = shell.ok("fpp_schedule", json!({ "address": fake.address() }));
    assert_eq!(schedule.as_array().unwrap().len(), 3, "{schedule}");
    fixtures.reply("an FPP's schedule", "types.ScheduleEntry[]", &schedule);
    let software = shell.ok("fpp_software", json!({ "address": fake.address() }));
    assert!(software["update"].is_object());
    fixtures.reply(
        "an FPP with a newer release to offer",
        "types.FppSoftware",
        &software,
    );
    let current = pf_devices::fpp_software::SoftwareReport {
        software: pf_devices::fpp_software::FppSoftware {
            version: "10.2".into(),
            os_build: "v2026-10".into(),
            os_release: "Debian 13".into(),
            platform: "Raspberry Pi".into(),
            bits: None,
            image_prefix: None,
        },
        update: None,
        checked: false,
    };
    fixtures.reply(
        "an FPP whose release list couldn't be read (built)",
        "types.FppSoftware",
        &wire(&current),
    );

    // Setting the show up from what an FPP sends to: nothing in the show yet, then all of it.
    let plan = shell.ok("fpp_setup_plan", json!({ "address": FPP }));
    assert!(!plan["controllers"].as_array().unwrap().is_empty());
    fixtures.reply(
        "what setting up from an FPP would add",
        "types.FppSetupPlan",
        &plan,
    );
    // An FPP with strings of its own is one the recorded network doesn't have: the plan the
    // command's own planner gives for a fake one (built).
    let hat = fake_hat();
    fixtures.swap(hat.address(), "192.0.2.51");
    let http = pf_devices::HttpClient::new(Duration::from_secs(5));
    let device = pf_devices::identify(&http, hat.address(), Some(pf_devices::DeviceKind::Fpp)).unwrap();
    let config = pf_devices::read_config(&http, &device).unwrap();
    let own = pf_devices::plan_fpp_setup(&device, &config, &pf_model::Show::new("House"));
    assert!(own.own.is_some());
    fixtures.reply(
        "setting up from an FPP with strings of its own (built)",
        "types.FppSetupPlan",
        &wire(&own),
    );
    let expected: Vec<Value> = plan["controllers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["address"].clone())
        .collect();
    shell.ok("fpp_set_up_show", json!({ "address": FPP, "expected": expected }));
    let nothing = shell.ok("fpp_setup_plan", json!({ "address": FPP }));
    assert!(!nothing["skipped"].as_array().unwrap().is_empty());
    fixtures.reply(
        "setting up from an FPP whose targets are in the show",
        "types.FppSetupPlan",
        &nothing,
    );
    fixtures.finish();
}

#[test]
fn device_setup_replies() {
    let shell = Shell::new();
    let mut fixtures = Fixtures::new("deviceSetup", shell.stable());
    let fpp = fake_hat();
    let host = fpp.address().to_string();
    fixtures.swap(&host, DEVICE);
    let snapshot = add_device(&shell.webview, &host);
    let plan_of = || shell.ok("plan_device_setup", json!({ "address": host }));

    // The show and the device agree: nothing to take, nothing to send.
    fixtures.reply(
        "a device that matches the show",
        "types.DeviceComparison",
        &shell.ok("compare_device", json!({ "address": host })),
    );
    let same = plan_of();
    assert_eq!(
        (&same["canSend"], same["reason"].is_string()),
        (&json!(false), true)
    );
    fixtures.reply("nothing to send", "types.SendPlan", &same);

    // A string grew on the FPP's own page: comparing offers to take it.
    fpp.state().pixel_strings.as_mut().unwrap()["channelOutputs"][0]["outputs"][0]["virtualStrings"][1]["pixelCount"] =
        json!(60);
    let comparison = shell.ok("compare_device", json!({ "address": host }));
    assert_eq!(ids(&comparison["changes"]), ["port1/string2/pixels"]);
    fixtures.reply(
        "a device that differs from the show",
        "types.DeviceComparison",
        &comparison,
    );
    shell.ok(
        "take_from_device_setup",
        json!({ "address": host, "picks": ["port1/string2/pixels"] }),
    );

    // The show changes instead: the plan says what sending would do, while the FPP is playing.
    fpp.state().play("Carol.fseq");
    resize(&shell.webview, &snapshot, "Gutter", 30);
    let plan = plan_of();
    assert!(
        plan["busy"].is_string() && plan["changes"][0]["warning"].is_string(),
        "{plan}"
    );
    assert_eq!(
        (&plan["canSend"], &plan["restorePoint"]),
        (&json!(true), &Value::Null)
    );
    fixtures.reply(
        "what sending the show's setup would change, on a busy FPP",
        "types.SendPlan",
        &plan,
    );
    let sent = shell.ok(
        "send_device_setup",
        json!({ "address": host, "expected": ids(&plan["changes"]) }),
    );
    assert_eq!(sent["status"], "sent", "{sent}");
    fixtures.reply("a setup that was sent", "types.SendReport", &sent);

    // A string the FPP won't load can't be sent; the copy kept before the first send is offered.
    resize(&shell.webview, &snapshot, "Gutter", 2000);
    let refused = plan_of();
    assert!(
        !refused["problems"].as_array().unwrap().is_empty() && refused["restorePoint"].is_object(),
        "{refused}"
    );
    fixtures.reply(
        "a setup the FPP wouldn't load, with a kept copy to put back",
        "types.SendPlan",
        &refused,
    );

    // A send the FPP answers but doesn't keep: what still differs comes back.
    resize(&shell.webview, &snapshot, "Gutter", 20);
    let plan = plan_of();
    fpp.state().ignore_config_writes = true;
    let mismatch = shell.ok(
        "send_device_setup",
        json!({ "address": host, "expected": ids(&plan["changes"]) }),
    );
    assert_eq!(mismatch["status"], "mismatch", "{mismatch}");
    assert!(!mismatch["mismatches"].as_array().unwrap().is_empty());
    fixtures.reply("a setup that didn't take", "types.SendReport", &mismatch);
    fpp.state().ignore_config_writes = false;

    // Putting the kept copy back.
    let restore = shell.ok("plan_device_restore", json!({ "address": host }));
    assert_eq!(restore["canRestore"], true, "{restore}");
    fixtures.reply(
        "what putting the kept setup back would change",
        "types.RestorePlan",
        &restore,
    );
    let restored = shell.ok(
        "restore_device_setup",
        json!({ "address": host, "expected": ids(&restore["changes"]) }),
    );
    assert_eq!(restored["restored"], true, "{restored}");
    fixtures.reply("a setup put back", "types.RestoreReport", &restored);
    fixtures.finish();
}

#[test]
fn transfer_replies() {
    let shell = Shell::new();
    let dir = shell.dir.path().to_path_buf();
    let mut fixtures = Fixtures::new("transfers", shell.stable());
    let made = write_sequence(&dir);
    let fpp = FakeFpp::start()
        .with_playlist("Main")
        .with_music("song.mp3", 5)
        .with_sequence_file("Medley.fseq", &std::fs::read(&made).unwrap())
        .with_sequence_media("Medley.fseq", r"C:\xLights\Audio\Medley.mp3")
        .with_music_file("Medley.mp3", b"ID3 not really a song")
        .with_sequence_file("Quiet.fseq", &std::fs::read(&made).unwrap())
        .with_sequence_media("Quiet.fseq", r"C:\xLights\Audio\Gone.mp3");
    fixtures.swap(fpp.address(), DEVICE);
    authored(&shell.webview);

    // Sending the open sequence: its music is on the FPP already, under another spelling.
    let music = dir.join("Song.mp3");
    std::fs::write(&music, b"music").unwrap();
    let source = json!({ "kind": "openSequence", "name": "Song" });
    let plan = shell.ok(
        "fpp_send_plan",
        json!({ "address": fpp.address(), "source": source, "music": music }),
    );
    assert_eq!(plan["music"]["fppName"], "song.mp3");
    assert!(!plan["layoutWarnings"].as_array().unwrap().is_empty());
    fixtures.reply(
        "what sending the open sequence would do",
        "types.FppSendPlan",
        &plan,
    );
    let sent = shell.ok(
        "fpp_send",
        json!({ "address": fpp.address(), "request": { "source": source, "music": music, "sequenceName": "Song.fseq",
            "musicName": "song.mp3", "uploadMusic": false, "replaceSequence": false, "replaceMusic": false,
            "playlist": { "kind": "new", "name": "Song" }, "sendId": 1 } }),
    );
    assert!(
        sent["playlist"].is_string() && sent["musicName"].is_string(),
        "{sent}"
    );
    fixtures.reply("a sequence sent, on a new playlist", "types.FppSendResult", &sent);
    // One of the show's files, without music or a playlist.
    let file = json!({ "kind": "file", "path": made });
    let plain = shell.ok(
        "fpp_send_plan",
        json!({ "address": fpp.address(), "source": file, "music": null }),
    );
    assert_eq!(plain["music"], Value::Null);
    fixtures.reply(
        "what sending a sequence file without music would do",
        "types.FppSendPlan",
        &plain,
    );
    let sent = shell.ok(
        "fpp_send",
        json!({ "address": fpp.address(), "request": { "source": file, "music": null, "sequenceName": plain["sequence"]["name"],
            "musicName": null, "uploadMusic": false, "replaceSequence": false, "replaceMusic": false,
            "playlist": { "kind": "none" }, "sendId": 2 } }),
    );
    fixtures.reply("a sequence file sent by itself", "types.FppSendResult", &sent);

    // Downloading: into a folder the user picked, since the show isn't saved.
    let folder = dir.join("Downloads");
    std::fs::create_dir_all(&folder).unwrap();
    shell.app.state::<AppState>().download_folders.add(folder.clone());
    let plan = shell.ok(
        "fpp_download_plan",
        json!({ "address": fpp.address(), "sequence": "Medley.fseq", "folder": folder }),
    );
    assert!(
        plan["music"].is_object() && plan["channelWarning"].is_string(),
        "{plan}"
    );
    fixtures.reply(
        "what downloading a sequence with its music would save",
        "types.FppDownloadPlan",
        &plan,
    );
    let saved = shell.ok(
        "fpp_download",
        json!({ "address": fpp.address(), "request": { "sequence": "Medley.fseq", "music": "Medley.mp3", "folder": folder,
            "sequenceClash": null, "musicClash": null, "downloadId": 1 } }),
    );
    fixtures.reply(
        "a sequence downloaded with its music",
        "types.FppDownloadResult",
        &saved,
    );
    // Its music isn't on the FPP, and the folder has the sequence already.
    std::fs::write(folder.join("sequences/Quiet.fseq"), b"old").unwrap();
    let plan = shell.ok(
        "fpp_download_plan",
        json!({ "address": fpp.address(), "sequence": "Quiet.fseq", "folder": folder }),
    );
    assert!(
        plan["missingMusic"].is_string() && plan["sequence"]["exists"] == true,
        "{plan}"
    );
    fixtures.reply(
        "a download whose music the FPP doesn't have, over a file that's there",
        "types.FppDownloadPlan",
        &plan,
    );
    let saved = shell.ok(
        "fpp_download",
        json!({ "address": fpp.address(), "request": { "sequence": "Quiet.fseq", "music": null, "folder": folder,
            "sequenceClash": "keepBoth", "musicClash": null, "downloadId": 2 } }),
    );
    assert_eq!(saved["musicPath"], Value::Null);
    fixtures.reply(
        "a sequence downloaded beside the one that was there",
        "types.FppDownloadResult",
        &saved,
    );
    fixtures.finish();
}
