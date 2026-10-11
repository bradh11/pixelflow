//! What the shell sends the window, as fixtures: each scenario runs real commands through the
//! mock runtime and records their replies with the TypeScript type that must accept them.
//! Values no command can be made to give here (a controller that doesn't resolve, a download's
//! progress) are built from the shell's own types instead, and say so.

use super::support::{
    Fixtures, Shell, bench, check_no_strays, controller, declared_events, heard, pin, port, prop, prop_id,
    slot, strip, wire,
};
use super::*;

mod assistant_replies;
mod device_replies;
mod sequence_replies;
mod unions;

/// The files of `replies/`, one per scenario.
const FILES: [&str; 15] = [
    "show",
    "recent",
    "cameraMap",
    "playback",
    "sequence",
    "sequenceFiles",
    "xlights",
    "devices",
    "deviceSetup",
    "transfers",
    "assistant",
    "events",
    "video",
    "unions",
    "rejections",
];

#[test]
fn no_reply_fixture_is_left_over() {
    check_no_strays("replies", &FILES);
}

/// What a fake device's address reads as in the fixtures (its port changes every run).
const DEVICE: &str = "192.0.2.50";

/// A prop of every shape a show can hold, small enough to read.
fn every_shape() -> Vec<Value> {
    let point = |x: f32, y: f32| json!({ "x": x, "y": y, "z": 0 });
    let mut props = vec![
        strip(1, "Line", 4),
        prop(
            2,
            "Arch",
            json!({ "type": "arch", "nodes": 6, "width": 2, "height": 1, "layers": [2, 4] }),
        ),
        prop(
            3,
            "Circle",
            json!({ "type": "circle", "nodes": 6, "radius": 1, "layers": [2, 4] }),
        ),
        prop(
            4,
            "Matrix",
            json!({ "type": "matrix", "columns": 3, "rows": 2, "width": 3, "height": 2,
            "wiring": { "start": "topLeft", "orientation": "vertical", "serpentine": true } }),
        ),
        prop(
            5,
            "Tree",
            json!({ "type": "tree", "strings": 2, "nodesPerString": 3, "height": 2,
            "baseRadius": 1, "topRadius": 0.1 }),
        ),
        prop(
            6,
            "Star",
            json!({ "type": "star", "points": 5, "nodes": 10, "outerRadius": 1, "innerRadius": 0.5,
            "layers": [5, 5] }),
        ),
        prop(
            7,
            "Custom",
            json!({ "type": "customGrid", "columns": 2, "rows": 2, "cells": [1, 0, 2, 3] }),
        ),
        prop(
            8,
            "Roof",
            json!({ "type": "polyLine", "vertices": [point(0.0, 0.0), point(1.0, 1.0), point(2.0, 0.0)],
            "segments": [{ "nodes": 2, "curve": [point(0.2, 0.6), point(0.6, 1.0)] }, { "nodes": 2 }] }),
        ),
        prop(
            9,
            "Roof spread",
            json!({ "type": "polyLine", "vertices": [point(0.0, 0.0), point(1.0, 0.0)],
            "segments": [{ "nodes": 0 }], "spreadNodes": 3 }),
        ),
        prop(
            10,
            "Canes",
            json!({ "type": "candyCanes", "canes": 2, "nodesPerCane": 3, "width": 2, "height": 1,
            "caneHeight": 1, "reverse": false, "sticks": false, "alternateNodes": false, "skewDeg": 0 }),
        ),
        prop(
            11,
            "Icicles",
            json!({ "type": "icicles", "strings": 1, "lightsPerString": 6, "drops": [1, 2, 3],
            "width": 2, "dropHeight": 1, "alternateNodes": false }),
        ),
        prop(
            12,
            "Window",
            json!({ "type": "windowFrame", "top": 2, "sides": 1, "bottom": 2, "width": 1,
            "height": 1, "start": "bottomLeft", "counterClockwise": false }),
        ),
        prop(
            13,
            "Wreath",
            json!({ "type": "wreath", "nodes": 6, "radius": 1, "startAtBottom": false,
            "counterClockwise": false }),
        ),
        prop(
            14,
            "Spinner",
            json!({ "type": "spinner", "arms": 2, "nodesPerArm": 3, "hollow": 20, "startAngle": 0,
            "arc": 360, "zigZag": false, "alternate": false, "fromCenter": true, "clockwise": true, "radius": 1 }),
        ),
        prop(
            15,
            "Sphere",
            json!({ "type": "sphere", "columns": 3, "rows": 2, "radius": 1 }),
        ),
        prop(
            16,
            "Cube",
            json!({ "type": "cube", "width": 2, "height": 2, "depth": 1, "spacing": 0.5 }),
        ),
    ];
    // Positions measured from a video instead of a generator.
    let mut measured = strip(17, "Bush", 2);
    measured["shape"] = json!({ "source": "measured", "provenance": "cameraMap",
        "points": [point(0.0, 0.0), point(0.5, 0.25)] });
    // Layouts of their own for effects, as an import gives a line and a custom model.
    measured["ownLayout"] = json!({ "kind": "strand", "reversed": true });
    let cube = props.iter_mut().find(|p| p["name"] == "Cube").unwrap();
    cube["ownLayout"] =
        json!({ "kind": "cells", "columns": 2, "rows": 2, "cells": [[0, 0], [1, 0], [0, 1], [1, 1]] });
    props.push(measured);
    // Submodels of each kind, and a color order with a white channel.
    props[3]["colorOrder"] = json!("RGBW");
    props[3]["tags"] = json!(["roof"]);
    props[3]["regions"] = json!([
        { "id": "88888888-0000-4000-8000-000000000001", "name": "Top row", "kind": "nodes",
          "lines": [[{ "first": 0, "last": 1 }, null, { "first": 5, "last": 4 }]], "layout": "horizontal", "buffer": "keepXY" },
        { "id": "88888888-0000-4000-8000-000000000002", "name": "Left half", "kind": "subBuffer",
          "x1": 0, "y1": 0, "x2": 50, "y2": 100 },
        { "id": "88888888-0000-4000-8000-000000000003", "name": "Singer", "kind": "face",
          "mouths": { "AI": [{ "start": 0, "end": 2 }], "REST": [{ "start": 2, "end": 3 }] },
          "eyesOpen": [{ "start": 4, "end": 5 }], "eyesClosed": [{ "start": 5, "end": 6 }], "outline": [],
          "colors": { "mouths": { "AI": "#ff0000" }, "eyesOpen": "#00ff00", "eyesClosed": "#0000ff", "outline": "#ffffff" } },
        { "id": "88888888-0000-4000-8000-000000000004", "name": "Plain face", "kind": "face",
          "mouths": {}, "eyesOpen": [], "eyesClosed": [], "outline": [{ "start": 0, "end": 6 }] },
    ]);
    props
}

/// Numbers each shape, so one added to the engine has to be added to [`every_shape`] (and so
/// to the fixtures the window's types are checked with): the matches have no wildcard.
fn shape_index(shape: &pf_model::ShapeSource) -> usize {
    use pf_model::Generator as G;
    match shape {
        pf_model::ShapeSource::Measured { .. } => 0,
        pf_model::ShapeSource::Generator(generator) => match generator {
            G::Line { .. } => 1,
            G::Arch { .. } => 2,
            G::Circle { .. } => 3,
            G::Matrix { .. } => 4,
            G::Tree { .. } => 5,
            G::Star { .. } => 6,
            G::CustomGrid { .. } => 7,
            G::PolyLine { .. } => 8,
            G::CandyCanes { .. } => 9,
            G::Icicles { .. } => 10,
            G::WindowFrame { .. } => 11,
            G::Wreath { .. } => 12,
            G::Spinner { .. } => 13,
            G::Sphere { .. } => 14,
            G::Cube { .. } => 15,
        },
    }
}
const SHAPES: usize = 16;

/// Fails unless `indexes` are exactly `0..count`: every variant of `what` has an example.
fn assert_every_variant(what: &str, count: usize, indexes: impl IntoIterator<Item = usize>) {
    let mut seen: Vec<usize> = indexes.into_iter().collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen,
        (0..count).collect::<Vec<_>>(),
        "not every {what} has an example"
    );
}

/// A show with every shape, groups, a controller of each protocol, and wiring with every
/// setting, as the window's own edits would build it.
fn full_show_edits() -> Vec<Value> {
    let mut edits: Vec<Value> = every_shape()
        .into_iter()
        .map(|prop| json!({ "type": "addProp", "prop": prop }))
        .collect();
    let mut wired = slot(2);
    wired["segment"] = json!({ "start": 0, "end": 3 });
    wired["nullPixels"] = json!(1);
    wired["reverse"] = json!(true);
    wired["brightness"] = json!(80);
    wired["gamma"] = json!(2.2);
    wired["smartReceiver"] = json!(1);
    wired["controllerColorOrder"] = json!("GRB");
    let mut rest = slot(2);
    rest["segment"] = json!({ "start": 3, "end": 6 });
    let mut porch_port = port(1, json!([slot(1), wired, rest]));
    porch_port["maxPixels"] = json!(680);
    porch_port["brightness"] = json!(90);
    porch_port["gamma"] = json!(2.2);
    let mut player = controller(
        2,
        "Player",
        "192.0.2.11",
        json!({ "type": "ddp" }),
        json!([port(1, json!([slot(3), slot(4)]))]),
    );
    player["adapter"] = json!("fpp");
    player["sequenceChannels"] = json!({ "start": 1, "count": 42, "rawDdpOffsets": true });
    let mut plain = controller(3, "Spare", "192.0.2.12", json!({ "type": "ddp" }), json!([]));
    plain["sequenceChannels"] = json!({ "start": 43, "count": 6 });
    edits.extend([
        json!({ "type": "renameShow", "name": "Every shape" }),
        json!({ "type": "addGroup", "group": { "id": "22222222-0000-4000-8000-000000000001", "name": "Roofline",
            "members": [prop_id(1), { "prop": prop_id(4), "region": "88888888-0000-4000-8000-000000000001" }],
            "layout": "horizontalPerModel", "gridSize": 200 } }),
        json!({ "type": "addGroup", "group": { "id": "22222222-0000-4000-8000-000000000002", "name": "Empty", "members": [] } }),
        json!({ "type": "addController", "controller": controller(1, "Porch", "192.0.2.10",
            json!({ "type": "sacn", "startUniverse": 1, "universeSize": 9, "allowPixelStraddle": false, "multicast": true }),
            json!([porch_port, port(2, json!([]))])) }),
        json!({ "type": "addController", "controller": player }),
        json!({ "type": "addController", "controller": plain }),
        json!({ "type": "addController", "controller": controller(4, "Unset", "",
            json!({ "type": "sacn", "startUniverse": null, "universeSize": 510, "allowPixelStraddle": true, "multicast": false }),
            json!([])) }),
    ]);
    edits
}

#[test]
fn show_replies() {
    let shell = Shell::new();
    let root = shell.dir.path().join("Show");
    std::fs::create_dir_all(&root).unwrap();
    let mut fixtures = Fixtures::new("show", shell.stable());
    fixtures.reply(
        "a new show",
        "types.ShowSnapshot",
        &shell.ok("get_snapshot", json!({})),
    );

    // A small show, saved, with a sequence open and files that aren't where it says.
    let entry = json!({ "id": "77777777-0000-4000-8000-000000000001", "name": "Carol",
        "path": root.join("sequences").join("Carol.fseq"), "audio": root.join("music").join("Carol.mp3"), "offsetMs": -40 });
    let silent = json!({ "id": "77777777-0000-4000-8000-000000000002", "name": "Silent",
        "path": root.join("sequences").join("Silent.fseq"), "audio": null, "offsetMs": 0 });
    shell.ok(
        "apply_edits",
        json!({ "edits": [
            { "type": "renameShow", "name": "House" },
            { "type": "addProp", "prop": strip(1, "Gutter", 4) },
            { "type": "addController", "controller": bench(1, &[1]) },
            { "type": "addSequence", "sequence": entry },
            { "type": "addSequence", "sequence": silent },
            { "type": "setBackground", "background": { "path": root.join("photos").join("house.png"), "x": -10, "y": 8,
                "width": 20, "opacity": 0.6 } },
            { "type": "setHouseModel", "houseModel": { "path": root.join("models").join("house.glb"),
                "position": { "x": 0, "y": 0, "z": -3 }, "rotationDeg": { "x": 0, "y": 90, "z": 0 }, "scale": 1.5, "opacity": 0.8 } },
        ] }),
    );
    let file = root.join("House.pixelflow.json");
    shell.ok("save_show_as", json!({ "path": file }));
    shell.ok(
        "new_sequence_doc",
        json!({ "name": "Carol", "durationMs": 1000, "audio": null }),
    );
    let checked = shell.ok("check_files", json!({ "all": true }));
    assert_eq!(checked["missingFiles"].as_array().unwrap().len(), 5, "{checked}");
    assert!(checked["sequenceRevision"].is_number() && checked["path"].is_string());
    fixtures.reply(
        "a saved show whose files have moved, with a sequence open",
        "types.ShowSnapshot",
        &checked,
    );

    // The music is in two other folders of the show now: one is used, the other offered.
    for folder in ["moved", "other"] {
        std::fs::create_dir_all(root.join(folder)).unwrap();
        std::fs::write(root.join(folder).join("Carol.mp3"), b"not really music").unwrap();
    }
    let found = shell.ok("find_missing_files", json!({}));
    assert_eq!(found["found"][0]["also"].as_array().unwrap().len(), 1, "{found}");
    assert!(!found["stillMissing"].as_array().unwrap().is_empty());
    fixtures.reply(
        "files found again, and the ones still missing",
        "types.FilesFound",
        &found,
    );

    // Autosaved versions (their names are their times).
    autosave(&shell.app.state::<AppState>(), "contract fixtures");
    let mut history = shell.ok("list_history", json!({}));
    let id = history[0]["id"].as_str().unwrap().to_string();
    let stamp: String = id.chars().take_while(char::is_ascii_digit).collect();
    fixtures.swap(&id, &id.replacen(&stamp, "1700000000000", 1));
    // (The file is as long as this computer writes the show's paths.)
    pin(&mut history, "sizeBytes", json!(2048));
    fixtures.reply("autosaved versions", "types.HistoryEntry[]", &history);

    // Test patterns.
    fixtures.reply(
        "no test pattern running",
        "types.OutputStatus",
        &shell.ok("output_status", json!({})),
    );
    shell.ok(
        "start_output",
        json!({ "pattern": { "kind": "chase", "color": "ff8000" }, "target": { "type": "prop", "id": prop_id(1) } }),
    );
    // (The controllers are listed once the output thread has sent its first frame.)
    let mut running = wait_until(&shell, "output_status", sending);
    assert_eq!(running["running"], true);
    // How many frames and packets have gone out by now differs from run to run.
    for (key, value) in [
        ("frames", json!(120)),
        ("lateFrames", json!(1)),
        ("achievedFps", json!(39.5)),
        ("packetsSent", json!(120)),
    ] {
        pin(&mut running, key, value);
    }
    fixtures.reply("a test pattern running", "types.OutputStatus", &running);
    // A controller whose address doesn't resolve can't be had without asking the network:
    // the status the engine gives, with that controller's fields set.
    let mut unresolved = shell.app.state::<AppState>().engine().output_status();
    unresolved.frames = 120;
    unresolved.late_frames = 0;
    unresolved.achieved_fps = 40.0;
    unresolved.controllers[0].state = "unresolved";
    unresolved.controllers[0].packets_sent = 0;
    unresolved.controllers[0].send_errors = 3;
    unresolved.controllers[0].last_error = Some("no address found for porch.local".to_string());
    fixtures.reply(
        "a test pattern whose controller can't be reached (built)",
        "types.OutputStatus",
        &wire(&unresolved),
    );
    // The prop goes: the pattern has nothing left to light, and says why it stopped.
    shell.ok(
        "apply_edits",
        json!({ "edits": [{ "type": "removeProp", "id": prop_id(1) }] }),
    );
    let stopped = shell.ok("output_status", json!({}));
    assert!(stopped["stopReason"].is_string(), "{stopped}");
    fixtures.reply(
        "a test pattern that stopped by itself",
        "types.OutputStatus",
        &stopped,
    );
    shell.ok("undo", json!({}));

    // The preview's own window.
    let alone = shell.ok("preview_link", json!({ "note": { "positionMs": 500 } }));
    fixtures.reply(
        "the preview link while the preview is in the main window",
        "types.PreviewLinked",
        &alone,
    );
    shell.ok("open_preview_window", json!({}));
    let preview = shell.app.get_webview_window(preview_window::LABEL).unwrap();
    let linked = call(&preview, "preview_link", json!({ "note": { "toggles": 1 } })).unwrap();
    assert_eq!(linked["open"], true);
    fixtures.reply(
        "the preview link from the preview's own window",
        "types.PreviewLinked",
        &linked,
    );
    shell.ok("close_preview_window", json!({}));

    // Every shape, with the problems validation finds in its wiring.
    shell.ok("new_show", json!({ "name": "Untitled Show" }));
    let shapes: Vec<pf_model::Prop> = serde_json::from_value(Value::Array(every_shape())).unwrap();
    assert_every_variant("shape", SHAPES, shapes.iter().map(|p| shape_index(&p.shape)));
    let full = shell.ok("apply_edits", json!({ "edits": full_show_edits() }));
    assert!(full["issues"].as_array().unwrap().len() > 1, "{}", full["issues"]);
    fixtures.reply(
        "a show of every shape, with its problems",
        "types.ShowSnapshot",
        &full,
    );
    fixtures.finish();
}

#[test]
fn recent_show_replies() {
    // Straight strips only: a thumbnail's dots are whole numbers, and a curve's could round
    // either way on another platform's maths library.
    let shell = Shell::new();
    let dir = shell.dir.path().to_path_buf();
    let mut fixtures = Fixtures::new("recent", shell.stable());
    shell.ok(
        "apply_edits",
        json!({ "edits": [{ "type": "renameShow", "name": "House" }, { "type": "addProp", "prop": strip(1, "Gutter", 50) },
            { "type": "addController", "controller": bench(1, &[1]) }] }),
    );
    shell.ok(
        "save_show_as",
        json!({ "path": dir.join("House.pixelflow.json") }),
    );
    shell.ok("new_show", json!({ "name": "Shed" }));
    let shed = dir.join("Shed.pixelflow.json");
    shell.ok("save_show_as", json!({ "path": shed }));
    std::fs::remove_file(&shed).unwrap();
    let recent = shell.ok("list_recent_shows", json!({}));
    assert_eq!(recent[0]["status"], "missing");
    assert_eq!(recent[0]["thumbnail"], Value::Null);
    assert!(recent[1]["thumbnail"].is_string());
    fixtures.reply("recent shows, one of them gone", "types.RecentShow[]", &recent);
    fixtures.finish();
}

#[test]
fn camera_map_replies() {
    use tauri::ipc::{CallbackFn, InvokeBody};
    let shell = Shell::new();
    let mut fixtures = Fixtures::new("cameraMap", shell.stable());
    shell.ok(
        "apply_edits",
        json!({ "edits": [{ "type": "addProp", "prop": strip(1, "Roof Line", 4) }] }),
    );
    let target = json!({ "type": "prop", "id": prop_id(1) });
    let info = shell.ok("camera_map_target", json!({ "target": target, "base": "four" }));
    fixtures.reply("what a capture covers", "types.CameraMapTargetInfo", &info);

    // Brightness of a video that starts a second before the sequence does.
    let spec = pf_camera_map::CodeSpec::new(4, pf_camera_map::Base::Four);
    let slots = spec.slots();
    let samples: Vec<Value> = (0..600)
        .map(|i| {
            let t = f64::from(i) / 30.0;
            let k = ((t - 1.0) / 0.5).floor();
            let lit = k >= 0.0 && slots[k as usize % slots.len()] == pf_camera_map::Slot::White;
            json!({ "t": t, "v": if lit { 50.0 } else { 10.0 } })
        })
        .collect();
    let sync = shell.ok(
        "camera_map_sync",
        json!({ "samples": samples, "pixels": 4, "base": "four" }),
    );
    fixtures.reply(
        "where the sequence starts in a video",
        "types.CameraMapSync",
        &sync,
    );

    // One frame per slot: the pixels 10 px apart along a row; pixel 2 never lights, and a
    // stray light flashes like pixel 0 (a reflection).
    let (w, h) = (48usize, 16usize);
    let mut bytes = Vec::new();
    for slot in &slots {
        let mut frame = vec![5u8; w * h * 3];
        let mut dot = |x: i32, y: i32, rgb: [u8; 3]| {
            for (dx, dy, f) in [(0, 0, 1.0), (1, 0, 0.5), (-1, 0, 0.5), (0, 1, 0.5), (0, -1, 0.5)] {
                let at = ((y + dy) as usize * w + (x + dx) as usize) * 3;
                for c in 0..3 {
                    frame[at + c] = (f * f64::from(rgb[c])) as u8 + 5;
                }
            }
        };
        for i in [0u32, 1, 3] {
            let rgb = match spec.symbol(*slot, i) {
                pf_camera_map::Symbol::Off => continue,
                pf_camera_map::Symbol::Red => [220, 30, 20],
                pf_camera_map::Symbol::Green => [30, 220, 60],
                pf_camera_map::Symbol::Blue => [20, 60, 220],
                pf_camera_map::Symbol::White => [230, 230, 230],
            };
            dot(9 + 10 * i as i32, 8, rgb);
            if i == 0 {
                dot(9, 13, rgb);
            }
        }
        bytes.extend(frame);
    }
    let header = json!({ "width": w, "height": h, "pixels": 4, "base": "four" }).to_string();
    let mut headers = tauri::http::HeaderMap::new();
    headers.insert(camera_map::FRAMES_HEADER, header.parse().unwrap());
    let decoded: Value = get_ipc_response(
        &shell.webview,
        InvokeRequest {
            cmd: "camera_map_decode".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: shell.webview.url().unwrap(),
            body: InvokeBody::Raw(bytes),
            headers,
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .unwrap()
    .deserialize()
    .unwrap();
    assert!(!decoded["pixels"].as_array().unwrap().is_empty(), "{decoded}");
    fixtures.reply("the pixels found in a capture", "types.DecodedCapture", &decoded);
    let plan = shell.ok(
        "camera_map_plan",
        json!({ "target": target, "pixels": 4, "decoded": decoded, "anchors": [] }),
    );
    fixtures.reply("a capture lined up with the layout", "types.CameraMapPlan", &plan);

    // Everything a capture can find wrong (built: most need a wiring fault a made-up video
    // doesn't have).
    use pf_camera_map::Anomaly;
    let anomalies = vec![
        Anomaly::Missing {
            prop: 0,
            ranges: vec![[2, 2]],
        },
        Anomaly::Duplicate {
            prop: 0,
            node: 0,
            x: 9.0,
            y: 13.0,
        },
        Anomaly::Reversed { prop: 0 },
        Anomaly::Jump { prop: 0, node: 3 },
        Anomaly::ColorOrder {
            prop: 0,
            configured: "RGB".into(),
            suggested: "GRB".into(),
        },
        Anomaly::Unreadable { count: 2 },
    ];
    assert_every_variant(
        "camera-map anomaly",
        6,
        anomalies.iter().map(|a| match a {
            Anomaly::Missing { .. } => 0,
            Anomaly::Duplicate { .. } => 1,
            Anomaly::Reversed { .. } => 2,
            Anomaly::Jump { .. } => 3,
            Anomaly::ColorOrder { .. } => 4,
            Anomaly::Unreadable { .. } => 5,
        }),
    );
    fixtures.all_of("types.CameraMapAnomaly", Some("kind"), &wire(&anomalies));
    // A pixel whose colors were read, one whose weren't, and a light that fits no pixel.
    let capture = pf_camera_map::Decoded {
        width: 48,
        height: 16,
        pixels: vec![
            pf_camera_map::Found {
                index: 0,
                x: 9.0,
                y: 8.0,
                confidence: 0.9,
                brightness: 220.0,
                seen: Some([0, 1, 2]),
            },
            pf_camera_map::Found {
                index: 1,
                x: 19.0,
                y: 8.0,
                confidence: 0.4,
                brightness: 90.0,
                seen: None,
            },
        ],
        duplicates: vec![pf_camera_map::Found {
            index: 0,
            x: 9.0,
            y: 13.0,
            confidence: 0.5,
            brightness: 80.0,
            seen: None,
        }],
        unreadable: vec![pf_camera_map::Unreadable {
            x: 40.0,
            y: 3.0,
            brightness: 60.0,
        }],
    };
    fixtures.reply(
        "a capture with a reflection and a stray light (built)",
        "types.DecodedCapture",
        &wire(&capture),
    );
    fixtures.finish();
}

/// Asks `command` until `done` says its reply is the one waited for.
fn wait_until(shell: &Shell, command: &str, done: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..2000 {
        let reply = shell.ok(command, json!({}));
        if done(&reply) {
            return reply;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("{command} never answered as expected");
}

/// Whether a status lists the controllers it sends to (it does once its thread is going).
fn sending(status: &Value) -> bool {
    status["controllers"].as_array().is_some_and(|c| !c.is_empty())
}

/// A rendered sequence a minute long: 6 channels, 25 ms a frame.
fn write_long_sequence(dir: &Path) -> PathBuf {
    let frames = 2400u32;
    let mut out = b"PSEQ".to_vec();
    out.extend_from_slice(&28u16.to_le_bytes());
    out.extend_from_slice(&[0, 1]);
    out.extend_from_slice(&28u16.to_le_bytes());
    out.extend_from_slice(&6u32.to_le_bytes());
    out.extend_from_slice(&frames.to_le_bytes());
    out.push(25);
    out.extend_from_slice(&[0; 9]);
    out.extend(std::iter::repeat_n(128u8, frames as usize * 6));
    let path = dir.join("show.fseq");
    std::fs::write(&path, out).unwrap();
    path
}

#[test]
fn playback_replies() {
    let shell = Shell::new();
    let dir = shell.dir.path().to_path_buf();
    let mut fixtures = Fixtures::new("playback", shell.stable());
    // Where a playing sequence has got to, and how many packets went out, differ from run to run.
    let steady = |mut status: Value, position: u64| {
        pin(&mut status, "positionMs", json!(position));
        pin(&mut status, "nowMs", json!(position + 10));
        pin(&mut status, "packetsSent", json!(8));
        status
    };
    let playing = |status: &Value| status["state"] == "playing" && sending(status);

    // A rendered sequence with its music beside it, on the show's playlist, and a controller
    // on this computer to play it to.
    let fseq = write_long_sequence(&dir);
    write_wav_ms(&dir.join("show.wav"), 1000);
    let mut controller = bench(1, &[1]);
    controller["sequenceChannels"] = json!({ "start": 1, "count": 6 });
    shell.ok(
        "apply_edits",
        json!({ "edits": [{ "type": "addProp", "prop": strip(1, "Strip", 2) }, { "type": "addController", "controller": controller }] }),
    );
    let added = shell.ok("add_sequence", json!({ "path": fseq }));
    let entry = added["show"]["sequences"][0].clone();
    assert!(entry["audio"].is_string(), "{entry}");

    shell.ok("play_sequence", json!({ "id": entry["id"], "positionMs": 0 }));
    let status = wait_until(&shell, "playback_status", playing);
    assert_eq!(status["sequence"], entry["id"]);
    assert!(status["music"].is_string());
    fixtures.reply(
        "one of the show's sequences playing with its music",
        "types.PlaybackStatus",
        &steady(status, 0),
    );
    shell.ok("set_playback_volume", json!({ "volume": 0.5 }));
    shell.ok("set_playback_speed", json!({ "speed": 0.5 }));
    shell.ok("pause_playback", json!({ "paused": true }));
    let paused = shell.ok("seek_playback", json!({ "positionMs": 500 }));
    assert_eq!(
        (&paused["state"], &paused["volume"], &paused["speed"]),
        (&json!("paused"), &json!(0.5), &json!(0.5))
    );
    fixtures.reply(
        "paused, quieter and at half speed",
        "types.PlaybackStatus",
        &steady(paused, 500),
    );
    shell.ok("stop_playback", json!({}));
    assert_eq!(shell.ok("playback_status", json!({})), Value::Null);

    // A file played by its path, to its end.
    shell.ok("start_playback", json!({ "path": fseq, "positionMs": 59_950 }));
    let mut ended = wait_until(&shell, "playback_status", |status| {
        status["state"] == "ended" && sending(status)
    });
    pin(&mut ended, "packetsSent", json!(8));
    assert_eq!(
        (
            &ended["positionMs"],
            &ended["nowMs"],
            &ended["sequence"],
            &ended["music"]
        ),
        (&json!(60_000), &json!(60_000), &Value::Null, &Value::Null)
    );
    fixtures.reply(
        "a sequence file that has played to its end",
        "types.PlaybackStatus",
        &ended,
    );
    // Playback that a damaged file stopped, and a note about a controller left out: the
    // status the engine gives, with those fields set (built).
    let mut failed = shell.app.state::<AppState>().engine().playback_status().unwrap();
    failed.error = Some("show.fseq ends before its last frame.".to_string());
    failed.notes = vec!["Porch was left out: it has no sequence channels yet.".to_string()];
    for controller in &mut failed.controllers {
        controller.packets_sent = 8;
        controller.state = "degraded";
        controller.send_errors = 2;
        controller.last_error = Some("the network is unreachable".to_string());
    }
    fixtures.reply(
        "playback a damaged file stopped (built)",
        "types.PlaybackStatus",
        &wire(&failed),
    );
    shell.ok("stop_playback", json!({}));

    // The open sequence, playing live and going round.
    shell.ok(
        "new_sequence_doc",
        json!({ "name": "Song", "durationMs": 60_000, "audio": null }),
    );
    shell.ok("set_sequence_doc_loop", json!({ "looping": true }));
    shell.ok("play_sequence_doc", json!({ "positionMs": 0 }));
    let authored = wait_until(&shell, "playback_status", playing);
    assert_eq!(
        (&authored["authored"], &authored["looping"]),
        (&json!(true), &json!(true))
    );
    fixtures.reply(
        "the open sequence playing, round and round",
        "types.PlaybackStatus",
        &steady(authored, 0),
    );
    // An edit to the show can stop a playing file, and says why.
    shell.ok("stop_playback", json!({}));
    shell.ok("start_playback", json!({ "path": fseq, "positionMs": 0 }));
    shell.ok(
        "apply_edits",
        json!({ "edits": [{ "type": "removeController", "id": "33333333-0000-4000-8000-000000000001" }] }),
    );
    let reason = shell.ok("playback_stop_reason", json!({}));
    assert!(reason.is_string(), "{reason}");
    fixtures.reply("why an edit stopped playback", "string | null", &reason);
    fixtures.reply("nothing stopped playback", "string | null", &Value::Null);
    fixtures.reply("nothing is playing", "types.PlaybackStatus | null", &Value::Null);

    // Music: its length and format, its loudness, and the click that lines the preview up.
    let info = shell.ok("probe_audio", json!({ "path": dir.join("show.wav") }));
    fixtures.reply("a music file's length and format", "types.AudioInfo", &info);
    let waveform = shell.ok(
        "audio_waveform",
        json!({ "path": dir.join("show.wav"), "slices": 8 }),
    );
    fixtures.reply("a silent song's loudness", "types.Waveform", &waveform);
    // (A real song's loudness is worked out with maths that differs in its last digits between
    // platforms: the same types, with values chosen here.)
    let loud = pf_audio::Waveform {
        duration_ms: 1000,
        peaks: vec![0.0, 0.25, 0.5, 1.0],
    };
    fixtures.reply("a song's loudness (built)", "types.Waveform", &wire(&loud));
    let lane = vocals::VocalLane::from_track(&pf_analysis::VocalTrack {
        hop_ms: 5.0,
        offset_ms: 2.5,
        energy: vec![0.0, 0.5, 1.0],
        onsets: vec![100, 300],
        consonant_onsets: vec![90, 500],
        ..Default::default()
    });
    fixtures.reply("a song's lead vocal (built)", "types.VocalLane", &wire(&lane));
    fixtures.reply(
        "the sync click, on a silent stopwatch",
        "types.SyncClick",
        &shell.ok("sync_click_start", json!({ "intervalMs": 500 })),
    );
    shell.ok("sync_click_stop", json!({}));
    let click = sync::SyncClick {
        interval_ms: 500,
        output_latency_ms: Some(10.7),
        sample_rate: Some(48_000),
        buffer_frames: Some(512),
    };
    fixtures.reply(
        "the sync click on a sound output (built)",
        "types.SyncClick",
        &wire(&click),
    );
    fixtures.reply(
        "where the sync click is when it isn't playing",
        "number | null",
        &shell.ok("sync_click_position", json!({})),
    );

    // What beat detection finds in a song (built, for the same reason as the loudness).
    use pf_analysis::{
        BarDrums, BarEnergy, Drum, DrumHit, Event, EventKind, Level, Moment, MomentKind, Section, Suggest,
    };
    let analysis = pf_analysis::Analysis {
        duration_ms: 4000,
        tempo_bpm: Some(120.0),
        beats: vec![0, 500, 1000, 1500, 2000, 2500, 3000, 3500],
        bars: vec![0, 2000],
        onsets: vec![0, 250, 500],
        sections: vec![Section {
            start_ms: 0,
            end_ms: 4000,
            energy: 0.75,
            level: Level::High,
            label: "Chorus".into(),
            group: "A".into(),
            confidence: 0.5,
        }],
        events: vec![
            Event {
                time_ms: 2000,
                kind: EventKind::Drop,
                strength: 1.0,
                duration_ms: None,
            },
            Event {
                time_ms: 3000,
                kind: EventKind::Build,
                strength: 0.5,
                duration_ms: Some(1000),
            },
        ],
        bar_energy: vec![
            BarEnergy {
                overall: 0.5,
                low: 0.25,
                mid: 0.5,
                high: 0.75,
            },
            BarEnergy::default(),
        ],
        moments: vec![
            Moment {
                time_ms: 2000,
                end_ms: None,
                kind: MomentKind::Drop,
                strength: 1.0,
                importance: 0.75,
                label: None,
                suggest: Suggest::Burst,
            },
            Moment {
                time_ms: 3000,
                end_ms: Some(4000),
                kind: MomentKind::KeyChange,
                strength: 0.5,
                importance: 0.5,
                label: Some("C→D".into()),
                suggest: Suggest::ColorShift,
            },
        ],
        drums: vec![DrumHit {
            time_ms: 2000,
            drum: Drum::Crash,
            strength: 1.0,
        }],
        bar_drums: vec![
            BarDrums {
                kick: 4,
                snare: 2,
                hat: 8,
                crash: 1,
            },
            BarDrums::default(),
        ],
        confidence: pf_analysis::Confidence {
            tempo: 0.75,
            downbeat: 0.5,
            sections: 0.25,
        },
        ..Default::default()
    };
    fixtures.reply(
        "what beat detection finds in a song (built)",
        "sequence.Analysis",
        &wire(&analysis),
    );
    let quiet = pf_analysis::Analysis {
        duration_ms: 500,
        ..Default::default()
    };
    fixtures.reply(
        "what beat detection finds in silence (built)",
        "sequence.Analysis",
        &wire(&quiet),
    );
    fixtures.finish();
}

/// Events the shell sends while it works: every one it can send has a payload here.
#[test]
fn event_payloads() {
    let shell = Shell::new();
    let dir = shell.dir.path().to_path_buf();
    let mut fixtures = Fixtures::new("events", shell.stable());
    let app = &shell.app;
    let mut sent: Vec<&str> = Vec::new();
    let mut record = |fixtures: &mut Fixtures, name: &str, event: &'static str, ts: &str, payload: &Value| {
        fixtures.event(name, event, ts, payload);
        sent.push(event);
    };

    // The File menu (macOS): each item sends the window its action.
    shell.ok(
        "save_show_as",
        json!({ "path": dir.join("House.pixelflow.json") }),
    );
    let recent = format!(
        "recent:{}",
        pf_model::path_to_text(&dir.join("House.pixelflow.json"))
    );
    let ((), chosen) = heard(app, menu::MENU_EVENT, || {
        for id in [
            "new-show",
            "open-show",
            recent.as_str(),
            "clear-recent",
            "save",
            "save-as",
            "undo",
            "redo",
        ] {
            menu::on_event(app.handle(), tauri::menu::MenuEvent { id: id.into() });
        }
    });
    assert_eq!(chosen.len(), 8, "{chosen:?}");
    // Close Show is only sent while the main window is in front, which a test's never is (built).
    let mut actions = chosen.clone();
    actions.push(wire(&menu::MenuAction::CloseShow));
    for action in &actions {
        let name = format!("the menu's {}", action["action"].as_str().unwrap());
        record(&mut fixtures, &name, menu::MENU_EVENT, "types.MenuAction", action);
    }

    // Long work on a music file: its loudness read through.
    let song = dir.join("Song.wav");
    write_wav_ms(&song, 1000);
    let (_, progress) = heard(app, progress::AUDIO_PROGRESS_EVENT, || {
        shell.ok("audio_waveform", json!({ "path": song, "slices": 4 }))
    });
    assert!(progress.len() >= 2, "{progress:?}");
    // (How much is read by the first report depends on the decoder's buffers.)
    let mut first = progress[0].clone();
    pin(&mut first, "fraction", json!(0.5));
    record(
        &mut fixtures,
        "a waveform's first step",
        progress::AUDIO_PROGRESS_EVENT,
        "types.AudioProgress",
        &first,
    );
    record(
        &mut fixtures,
        "a waveform's last step",
        progress::AUDIO_PROGRESS_EVENT,
        "types.AudioProgress",
        progress.last().unwrap(),
    );

    // An export of the open sequence.
    authored(&shell.webview);
    let fseq = dir.join("Song.fseq");
    let (_, exported) = heard(app, sequencer::EXPORT_PROGRESS_EVENT, || {
        shell.ok("export_sequence_doc", json!({ "path": fseq }))
    });
    record(
        &mut fixtures,
        "an export part way",
        sequencer::EXPORT_PROGRESS_EVENT,
        "sequence.ExportProgress",
        &exported[exported.len() / 2],
    );
    record(
        &mut fixtures,
        "an export's last frame",
        sequencer::EXPORT_PROGRESS_EVENT,
        "sequence.ExportProgress",
        exported.last().unwrap(),
    );

    // A video export (built: an export takes seconds, and the command's cancel counter is
    // shared by every test in the process).
    let mp4 = dir.join("Song.mp4").display().to_string();
    for (stage, fraction) in [
        (pf_video::Stage::Rendering, Some(0.5)),
        (pf_video::Stage::Sound, None),
        (pf_video::Stage::Writing, Some(1.0)),
    ] {
        let step = video::VideoProgress {
            path: mp4.clone(),
            stage,
            label: stage.label(),
            fraction,
        };
        let name = format!("a video export, {} (built)", stage.label().to_lowercase());
        record(
            &mut fixtures,
            &name,
            video::VIDEO_PROGRESS_EVENT,
            "video.VideoProgress",
            &wire(&step),
        );
    }
    // Sending the open sequence to an FPP, and downloading one from it.
    let fpp = pf_devices::testing::FakeFpp::start()
        .with_playlist("Main")
        .with_sequence_file("Medley.fseq", &std::fs::read(write_sequence(&dir)).unwrap())
        .with_sequence_media("Medley.fseq", r"C:\xLights\Audio\Medley.mp3")
        .with_music_file("Medley.mp3", b"ID3 not really a song");
    fixtures.swap(fpp.address(), DEVICE);
    let music = dir.join("Song.mp3");
    std::fs::write(&music, b"not really music").unwrap();
    let source = json!({ "kind": "openSequence", "name": "Song" });
    let (_, steps) = heard(app, fpp_send::FPP_SEND_PROGRESS_EVENT, || {
        shell.ok(
            "fpp_send",
            json!({ "address": fpp.address(), "request": { "source": source, "music": music, "sequenceName": "Song.fseq",
                "musicName": "Song.mp3", "uploadMusic": true, "replaceSequence": false, "replaceMusic": false,
                "playlist": { "kind": "existing", "name": "Main" }, "sendId": 7 } }),
        )
    });
    for step in ["export", "sequence", "music", "commit", "playlist"] {
        let last = steps
            .iter()
            .rev()
            .find(|s| s["step"] == step)
            .unwrap_or_else(|| panic!("no {step} step in {steps:?}"));
        record(
            &mut fixtures,
            &format!("a send's {step} step"),
            fpp_send::FPP_SEND_PROGRESS_EVENT,
            "types.FppSendProgress",
            last,
        );
    }
    shell
        .app
        .state::<AppState>()
        .download_folders
        .add(dir.join("Downloads"));
    let (_, steps) = heard(app, fpp_download::FPP_DOWNLOAD_PROGRESS_EVENT, || {
        shell.ok(
            "fpp_download",
            json!({ "address": fpp.address(), "request": { "sequence": "Medley.fseq", "music": "Medley.mp3",
                "folder": dir.join("Downloads"), "sequenceClash": null, "musicClash": null, "downloadId": 9 } }),
        )
    });
    for step in ["sequence", "music"] {
        let last = steps
            .iter()
            .rev()
            .find(|s| s["step"] == step)
            .unwrap_or_else(|| panic!("no {step} step in {steps:?}"));
        record(
            &mut fixtures,
            &format!("a download's {step} step"),
            fpp_download::FPP_DOWNLOAD_PROGRESS_EVENT,
            "types.FppDownloadProgress",
            last,
        );
    }

    // A picture a preview was waiting for has been read: the event carries nothing.
    let ((), arrived) = heard(app, pictures::PICTURES_EVENT, || {
        use tauri::Emitter;
        app.emit(pictures::PICTURES_EVENT, ()).unwrap();
    });
    record(
        &mut fixtures,
        "a picture arrived",
        pictures::PICTURES_EVENT,
        "null",
        &arrived[0],
    );

    // The alignment model's download (built: it only runs against the real download).
    let downloaded = lyrics::ModelsProgress {
        received: 1_048_576,
        total: 189_118_943,
    };
    record(
        &mut fixtures,
        "the alignment model part way down (built)",
        lyrics::ALIGN_MODELS_PROGRESS_EVENT,
        "assistant.ModelsProgress",
        &wire(&downloaded),
    );

    // Find lyrics and the assistant send theirs in the assistant's scenario.
    sent.extend(assistant_replies::event_payloads(&shell, &mut fixtures));
    assert_eq!(
        sorted_events(&sent),
        declared_events(),
        "an event the shell can send has no fixture"
    );
    fixtures.finish();
}

/// What a video export gives (built: an export takes seconds, the command's cancel counter is
/// shared by every test in the process, and what the dialog can offer depends on the ffmpeg
/// installed on this computer).
#[test]
fn video_replies() {
    let mut fixtures = Fixtures::new("video", Default::default());
    let written = pf_video::VideoSummary {
        frames: 60,
        duration_ms: 2000,
        width: 1280,
        height: 720,
        fps: 30,
        bytes: 181_000,
        video: "H.264 (OpenH264)".into(),
        sound: Some("AAC 192 kb/s".into()),
        elapsed_ms: 900,
        notes: vec!["The layout photo couldn't be read, so the video has none.".into()],
    };
    fixtures.reply(
        "a video that was written (built)",
        "video.VideoSummary",
        &wire(&written),
    );
    let silent = pf_video::VideoSummary {
        sound: None,
        notes: Vec::new(),
        ..written
    };
    fixtures.reply(
        "a video without sound (built)",
        "video.VideoSummary",
        &wire(&silent),
    );
    for (name, ffmpeg, photo) in [
        ("with ffmpeg and a photo", Some("x264"), true),
        ("with neither", None, false),
    ] {
        let choices = video::VideoChoices { ffmpeg, photo };
        fixtures.reply(
            &format!("a video export's choices, {name} (built)"),
            "video.VideoChoices",
            &wire(&choices),
        );
    }
    fixtures.finish();
}

fn sorted_events(events: &[&str]) -> Vec<String> {
    let mut events: Vec<String> = events.iter().map(|e| e.to_string()).collect();
    events.sort();
    events.dedup();
    events
}

/// What commands are rejected with: plain sentences, ready to show.
#[test]
fn rejections() {
    let shell = Shell::new();
    let mut fixtures = Fixtures::new("rejections", shell.stable());
    let mut refuse = |name: &str, command: &str, args: Value| {
        let error = shell.refused(command, args);
        assert!(error.is_string(), "{command}: {error}");
        fixtures.error(name, command, "string", &error);
    };
    refuse("saving a show that has no file yet", "save_show", json!({}));
    refuse(
        "an edit to a prop that's gone",
        "apply_edits",
        json!({ "edits": [{ "type": "removeProp", "id": prop_id(9) }] }),
    );
    refuse(
        "a test pattern with nothing to light",
        "start_output",
        json!({ "pattern": { "kind": "solid", "color": "ff0000" }, "target": { "type": "show" } }),
    );
    refuse(
        "an edit with no sequence open",
        "edit_sequence",
        json!({ "edits": [] }),
    );
    refuse(
        "a version that isn't there",
        "restore_history",
        json!({ "id": "nothing" }),
    );
    refuse(
        "a controller that doesn't answer",
        "inspect_device",
        json!({ "address": "192.0.2.99" }),
    );
    refuse(
        "a search before the show is saved",
        "find_missing_files",
        json!({}),
    );
    refuse(
        "a photo the user never picked",
        "read_image",
        json!({ "path": "/fixture/photo.png" }),
    );
    // A value the engine refuses while reading the edit comes with Tauri's own words in front.
    let mut controller = bench(1, &[]);
    controller["protocol"] = json!({ "type": "sacn", "startUniverse": 1, "universeSize": 600, "allowPixelStraddle": false, "multicast": false });
    refuse(
        "a universe size the engine can't hold",
        "apply_edits",
        json!({ "edits": [{ "type": "addController", "controller": controller }] }),
    );
    shell.ok(
        "new_sequence_doc",
        json!({ "name": "Song", "durationMs": 1000, "audio": null }),
    );
    refuse(
        "an effect that ends before it starts",
        "edit_sequence",
        json!({ "edits": [{ "type": "setEffectTiming", "id": "55555555-0000-4000-8000-000000000001", "startMs": 9, "endMs": 9 }] }),
    );
    refuse(
        "saving a sequence that has no file yet",
        "save_sequence_doc",
        json!({}),
    );
    // The preview's own window is refused everything but what it draws with.
    shell.ok("open_preview_window", json!({}));
    let preview = shell.app.get_webview_window(preview_window::LABEL).unwrap();
    let error = call(&preview, "new_show", json!({ "name": "Changed" })).unwrap_err();
    fixtures.error(
        "the preview's window asking to change the show",
        "new_show",
        "string",
        &error,
    );
    fixtures.finish();
}
