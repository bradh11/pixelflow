//! The browser's stand-ins against the engine. `MemoryBackend` and `MemorySequencer`
//! (`app/src/api`) do in TypeScript what the engine does, for the browser demo and the window's
//! own tests; nothing else says they still agree with it. Each scenario here is a script of
//! commands, run against the real engine through the mock runtime; what each step answered and
//! left behind is written to `app/src/api/contracts/parity/<scenario>.json`. `parity.test.ts`
//! runs the same scripts against the stand-ins and compares.
//!
//! The files fail a test when stale; `PIXELFLOW_UPDATE_CONTRACTS=1` rewrites them.

use super::contracts::support::{
    Shell, bench, check_file, check_no_strays, controller, in_dir, port, prop, prop_id, slot, strip,
};
use super::*;
use pf_devices::testing::{FALCON, FPP};

/// A whole prop, controller, or effect to a line: the files are mostly what the show and the
/// sequence look like after each step.
const WIDE: usize = 400;

/// The files of `parity/`, one per scenario.
const SCENARIOS: [&str; 19] = [
    "showEdits",
    "showSaving",
    "showRejectedEdits",
    "showCleanup",
    "showWiring",
    "showPlaylist",
    "showNoOps",
    "sampleShow",
    "deviceImport",
    "fppSetup",
    "sequenceEdits",
    "sequenceGestures",
    "sequenceRejectedEdits",
    "sequenceRows",
    "sequenceSaving",
    "sequenceTracks",
    "sequenceRowBlending",
    "sequenceMusicLevel",
    "sequenceInShow",
];

#[test]
fn no_parity_fixture_is_left_over() {
    check_no_strays("parity", &SCENARIOS);
}

/// What a snapshot of the show or the open sequence says about undo and saving.
fn flags(snapshot: &Value) -> Value {
    if snapshot.is_null() {
        return Value::Null;
    }
    json!({
        "revision": snapshot["revision"], "path": snapshot["path"], "dirty": snapshot["dirty"],
        "canUndo": snapshot["canUndo"], "canRedo": snapshot["canRedo"],
    })
}

/// What a step answered, besides the state it left (recorded apart): a rejection's message, an
/// edit's list of changes (only the parts that list something), or a reply that isn't a snapshot
/// (a plan, a device's details).
fn reply_of(reply: Result<Value, Value>) -> Value {
    match reply {
        Err(error) => json!({ "error": error }),
        Ok(reply) if reply.get("changes").is_some() => {
            let changes: serde_json::Map<String, Value> = reply["changes"]
                .as_object()
                .unwrap()
                .iter()
                .filter(|(_, part)| !part.is_null() && part.as_array().is_none_or(|list| !list.is_empty()))
                .map(|(name, part)| (name.clone(), part.clone()))
                .collect();
            json!({ "changed": reply["changed"], "changes": changes })
        }
        Ok(reply) if reply.get("channelMap").is_some() || reply.get("sequence").is_some() => json!({}),
        Ok(reply) => json!({ "value": reply }),
    }
}

/// Runs `steps` against the engine and checks `parity/<file>.json`. `devices` are the recorded
/// controllers the stand-in is told about, as the engine sees them before the show has anything.
fn scenario(file: &str, about: &str, devices: &[&str], steps: Vec<(&str, Value)>) {
    let shell = Shell::new();
    let devices: Vec<Value> = devices
        .iter()
        .map(|address| shell.ok("inspect_device", json!({ "address": address })))
        .collect();
    // The show, what's worked out from it, and the sequence are written only where a step
    // changed them: `…Now` is how each stands from that step on. An edit to the sequence says
    // what it changed, so the whole document is written only after the last of a run of them.
    let mut now: std::collections::BTreeMap<&str, Value> = Default::default();
    let mut state_after = |step: &mut Value, whole_sequence: bool| {
        let snapshot = shell.ok("get_snapshot", json!({}));
        let open = shell.ok("get_sequence_doc", json!({}));
        step["show"] = flags(&snapshot);
        step["sequence"] = flags(&open);
        for (name, value) in [
            ("showNow", &snapshot["show"]),
            ("channelMapNow", &snapshot["channelMap"]),
            ("summaryNow", &snapshot["summary"]),
            ("issuesNow", &snapshot["issues"]),
            ("sequenceNow", &open["sequence"]),
            ("sequenceIssuesNow", &open["issues"]),
        ] {
            let written = name != "sequenceNow" || whole_sequence;
            if written && now.get(name) != Some(value) {
                now.insert(name, value.clone());
                step[name] = value.clone();
            }
        }
    };
    let mut start = json!({});
    state_after(&mut start, true);
    let mut recorded = Vec::new();
    for (i, (command, args)) in steps.iter().enumerate() {
        let reply = reply_of(shell.call(command, in_dir(args, shell.dir.path())));
        let run_goes_on = steps.get(i + 1).is_some_and(|next| next.0 == *command);
        let whole_sequence = reply.get("changes").is_none() || !run_goes_on;
        let mut step = json!({ "command": command, "args": args, "reply": reply });
        state_after(&mut step, whole_sequence);
        recorded.push(step);
    }
    let whole = json!({ "about": about, "devices": devices, "start": start, "steps": recorded });
    check_file(&format!("parity/{file}.json"), &shell.stable().value(whole), WIDE);
}

fn edits(edits: Value) -> (&'static str, Value) {
    ("apply_edits", json!({ "edits": edits }))
}

fn plain(command: &'static str) -> (&'static str, Value) {
    (command, json!({}))
}

fn group(n: u32, name: &str, members: Value) -> Value {
    json!({ "id": format!("22222222-0000-4000-8000-{n:012}"), "name": name, "members": members })
}

fn group_id(n: u32) -> String {
    format!("22222222-0000-4000-8000-{n:012}")
}

fn controller_id(n: u32) -> String {
    format!("33333333-0000-4000-8000-{n:012}")
}

fn entry(n: u32, name: &str) -> Value {
    json!({ "id": entry_id(n), "name": name, "path": format!("/fixture/sequences/{name}.fseq"),
            "audio": format!("/fixture/music/{name}.mp3"), "offsetMs": 0 })
}

fn entry_id(n: u32) -> String {
    format!("77777777-0000-4000-8000-{n:012}")
}

#[test]
fn show_edits_undo_and_redo() {
    scenario(
        "showEdits",
        "Edits to the show are undo steps, one per batch; a new edit forgets what was undone.",
        &[],
        vec![
            edits(json!([
                { "type": "addProp", "prop": strip(1, "Gutter", 50) },
                { "type": "addProp", "prop": strip(2, "Fence", 30) },
                { "type": "addController", "controller": bench(1, &[1, 2]) },
            ])),
            edits(json!([{ "type": "renameShow", "name": "House" }, { "type": "setFrameRate", "fps": 20 }])),
            edits(
                json!([{ "type": "addGroup", "group": group(1, "Front", json!([prop_id(1), prop_id(2)])) }]),
            ),
            plain("undo"),
            plain("undo"),
            plain("redo"),
            edits(json!([{ "type": "removeProp", "id": prop_id(2) }])),
            plain("redo"),
            plain("undo"),
            plain("undo"),
            plain("undo"),
            plain("undo"),
            plain("redo"),
        ],
    );
}

#[test]
fn show_saving_and_unsaved_changes() {
    let file = "/fixture/House.pixelflow.json";
    scenario(
        "showSaving",
        "Whether the show has unsaved changes, through saving, undoing past a save, and starting or opening another.",
        &[],
        vec![
            edits(json!([{ "type": "renameShow", "name": "House" }])),
            plain("save_show"),
            ("save_show_as", json!({ "path": file })),
            edits(json!([{ "type": "addProp", "prop": strip(1, "Gutter", 50) }])),
            plain("undo"),
            plain("save_show"),
            plain("redo"),
            ("new_show", json!({ "name": "Shed" })),
            plain("undo"),
            plain("save_show"),
            ("open_show", json!({ "path": file })),
            plain("undo"),
            edits(json!([{ "type": "renameShow", "name": "House 2" }])),
            ("open_show", json!({ "path": file })),
        ],
    );
}

#[test]
fn show_edits_that_are_refused() {
    let photo = |change: Value| {
        let mut background =
            json!({ "path": "/fixture/house.png", "x": 0, "y": 0, "width": 10, "opacity": 1 });
        for (key, value) in change.as_object().unwrap() {
            background[key] = value.clone();
        }
        edits(json!([{ "type": "setBackground", "background": background }]))
    };
    let model = |change: Value| {
        let mut house = json!({ "path": "/fixture/house.glb", "position": { "x": 0, "y": 0, "z": 0 },
            "rotationDeg": { "x": 0, "y": 0, "z": 0 }, "scale": 1, "opacity": 1 });
        for (key, value) in change.as_object().unwrap() {
            house[key] = value.clone();
        }
        edits(json!([{ "type": "setHouseModel", "houseModel": house }]))
    };
    let gone = prop_id(99);
    let mut wide = bench(2, &[]);
    wide["protocol"] = json!({ "type": "sacn", "startUniverse": 1, "universeSize": 600, "allowPixelStraddle": false, "multicast": false });
    scenario(
        "showRejectedEdits",
        "Edits the engine refuses, in its words, and what they leave behind: nothing.",
        &[],
        vec![
            edits(json!([
                { "type": "addProp", "prop": strip(1, "Gutter", 50) },
                { "type": "addController", "controller": bench(1, &[1]) },
                { "type": "addGroup", "group": group(1, "Front", json!([prop_id(1)])) },
                { "type": "addSequence", "sequence": entry(1, "Carol") },
            ])),
            edits(json!([{ "type": "addProp", "prop": strip(1, "Again", 10) }])),
            edits(json!([{ "type": "addController", "controller": bench(1, &[]) }])),
            edits(json!([{ "type": "addGroup", "group": group(1, "Again", json!([])) }])),
            edits(json!([{ "type": "addSequence", "sequence": entry(1, "Again") }])),
            edits(json!([{ "type": "updateProp", "prop": strip(99, "Nobody", 10) }])),
            edits(json!([{ "type": "removeProp", "id": gone }])),
            edits(json!([{ "type": "updateGroup", "group": group(99, "Nobody", json!([])) }])),
            edits(json!([{ "type": "removeGroup", "id": group_id(99) }])),
            edits(json!([{ "type": "updateController", "controller": bench(99, &[]) }])),
            edits(json!([{ "type": "removeController", "id": controller_id(99) }])),
            edits(json!([{ "type": "updateSequence", "sequence": entry(99, "Nobody") }])),
            edits(json!([{ "type": "removeSequence", "id": entry_id(99) }])),
            edits(json!([{ "type": "moveSequence", "id": entry_id(99), "index": 0 }])),
            // The first edit of a batch is good, the second isn't: neither is kept.
            edits(json!([{ "type": "renameShow", "name": "Changed" }, { "type": "removeProp", "id": gone }])),
            photo(json!({ "path": "  " })),
            photo(json!({ "x": 10_000_000 })),
            photo(json!({ "width": 0 })),
            photo(json!({ "width": 10_000_000 })),
            photo(json!({ "opacity": 1.5 })),
            model(json!({ "path": "" })),
            model(json!({ "scale": 0 })),
            model(json!({ "opacity": -0.5 })),
            edits(json!([{ "type": "addController", "controller": wide }])),
            plain("undo"),
            plain("undo"),
        ],
    );
}

#[test]
fn removing_things_from_the_show_cleans_up() {
    let (top, left) = (
        "88888888-0000-4000-8000-000000000001",
        "88888888-0000-4000-8000-000000000002",
    );
    let nodes = |id: &str, name: &str| {
        json!({ "id": id, "name": name, "kind": "nodes", "lines": [[{ "first": 0, "last": 2 }]],
                "layout": "horizontal", "buffer": "default" })
    };
    let mut matrix = prop(
        1,
        "Matrix",
        json!({ "type": "matrix", "columns": 3, "rows": 2, "width": 3, "height": 2,
                 "wiring": { "start": "bottomLeft", "orientation": "horizontal", "serpentine": false } }),
    );
    matrix["regions"] = json!([nodes(top, "Top"), nodes(left, "Left")]);
    let mut without_top = matrix.clone();
    without_top["regions"] = json!([nodes(left, "Left")]);
    let mut half = slot(2);
    half["segment"] = json!({ "start": 0, "end": 10 });
    let wired = controller(
        1,
        "Porch",
        "192.0.2.10",
        json!({ "type": "ddp" }),
        json!([port(1, json!([slot(1), half])), port(2, json!([slot(2)]))]),
    );
    let members = json!([prop_id(1), { "prop": prop_id(1), "region": top }, prop_id(2), { "prop": prop_id(1), "region": left }]);
    scenario(
        "showCleanup",
        "A submodel or prop that goes away leaves the groups and ports it was in; undo brings it all back.",
        &[],
        vec![
            edits(json!([
                { "type": "addProp", "prop": matrix },
                { "type": "addProp", "prop": strip(2, "Gutter", 20) },
                { "type": "addController", "controller": wired },
                { "type": "addGroup", "group": group(1, "Everything", members) },
            ])),
            edits(json!([{ "type": "updateProp", "prop": without_top }])),
            edits(json!([{ "type": "removeProp", "id": prop_id(1) }])),
            plain("undo"),
            plain("undo"),
            edits(json!([{ "type": "removeProp", "id": prop_id(2) }])),
            edits(json!([{ "type": "removeController", "id": controller_id(1) }])),
            edits(json!([{ "type": "removeGroup", "id": group_id(1) }])),
            plain("undo"),
            plain("undo"),
            plain("undo"),
        ],
    );
}

#[test]
fn wiring_becomes_channels_and_universes() {
    let mut white = strip(2, "Eaves", 100);
    white["colorOrder"] = json!("RGBW");
    let mut first = slot(2);
    first["segment"] = json!({ "start": 0, "end": 60 });
    first["nullPixels"] = json!(2);
    first["reverse"] = json!(true);
    first["brightness"] = json!(50);
    first["gamma"] = json!(2.0);
    let mut rest = slot(2);
    rest["segment"] = json!({ "start": 60, "end": 100 });
    let sacn = |start: Value, size: u32, straddle: bool| json!({ "type": "sacn", "startUniverse": start, "universeSize": size, "allowPixelStraddle": straddle, "multicast": false });
    let mut porch = controller(
        1,
        "Porch",
        "192.0.2.10",
        sacn(Value::Null, 510, false),
        json!([port(1, json!([slot(1), first])), port(2, json!([rest]))]),
    );
    let garage = controller(
        2,
        "Garage",
        "192.0.2.11",
        sacn(json!(1), 100, true),
        json!([port(1, json!([slot(3)]))]),
    );
    let shed = controller(
        3,
        "Shed",
        "192.0.2.12",
        json!({ "type": "ddp" }),
        json!([port(1, json!([slot(4)]))]),
    );
    let first_plan = edits(json!([
        { "type": "addProp", "prop": strip(1, "Gutter", 50) },
        { "type": "addProp", "prop": white },
        { "type": "addProp", "prop": strip(3, "Fence", 40) },
        { "type": "addProp", "prop": strip(4, "Door", 10) },
        { "type": "addProp", "prop": strip(5, "Spare", 5) },
        { "type": "addController", "controller": porch.clone() },
        { "type": "addController", "controller": garage },
        { "type": "addController", "controller": shed },
    ]));
    porch["protocol"] = sacn(Value::Null, 30, false);
    let small_universes = edits(json!([{ "type": "updateController", "controller": porch.clone() }]));
    porch["protocol"] = json!({ "type": "ddp" });
    scenario(
        "showWiring",
        "Props wired to ports become controller channels and sACN universes, numbered round the ones that are pinned.",
        &[],
        vec![
            first_plan,
            small_universes,
            edits(json!([{ "type": "updateController", "controller": porch }])),
            edits(json!([{ "type": "removeProp", "id": prop_id(1) }])),
            plain("undo"),
        ],
    );
}

#[test]
fn the_playlist_keeps_its_order() {
    let mut louder = entry(2, "Wizards");
    louder["offsetMs"] = json!(-120);
    louder["audio"] = Value::Null;
    scenario(
        "showPlaylist",
        "Sequences on the show's playlist are added, moved, changed, and removed.",
        &[],
        vec![
            edits(json!([
                { "type": "addSequence", "sequence": entry(1, "Carol") },
                { "type": "addSequence", "sequence": entry(2, "Wizards") },
                { "type": "addSequence", "sequence": entry(3, "Medley") },
            ])),
            edits(json!([{ "type": "moveSequence", "id": entry_id(1), "index": 99 }])),
            edits(json!([{ "type": "moveSequence", "id": entry_id(3), "index": 0 }])),
            edits(json!([{ "type": "updateSequence", "sequence": louder }])),
            edits(json!([{ "type": "removeSequence", "id": entry_id(3) }])),
            plain("undo"),
            plain("undo"),
            plain("redo"),
        ],
    );
}

#[test]
fn edits_that_change_nothing_are_not_changes() {
    scenario(
        "showNoOps",
        "An edit that leaves the show as it was is no undo step and nothing to save.",
        &[],
        vec![
            edits(
                json!([{ "type": "addProp", "prop": strip(1, "Gutter", 50) }, { "type": "addSequence", "sequence": entry(1, "Carol") }]),
            ),
            ("save_show_as", json!({ "path": "/fixture/House.pixelflow.json" })),
            edits(json!([])),
            edits(json!([{ "type": "renameShow", "name": "Untitled Show" }])),
            edits(json!([{ "type": "setFrameRate", "fps": 40 }])),
            edits(json!([{ "type": "updateProp", "prop": strip(1, "Gutter", 50) }])),
            edits(json!([{ "type": "moveSequence", "id": entry_id(1), "index": 0 }])),
            edits(json!([{ "type": "setBackground", "background": null }])),
            edits(json!([{ "type": "setHouseModel", "houseModel": null }])),
            // Changed and changed back in one batch.
            edits(
                json!([{ "type": "renameShow", "name": "House" }, { "type": "renameShow", "name": "Untitled Show" }]),
            ),
            plain("undo"),
            plain("redo"),
        ],
    );
}

#[test]
fn the_sample_show_is_the_same_show() {
    scenario(
        "sampleShow",
        "The demo show built into the app: the same props, channels, and universes on both sides.",
        &[],
        vec![
            plain("open_sample_show"),
            edits(json!([{ "type": "renameShow", "name": "My House" }])),
            plain("undo"),
            plain("undo"),
        ],
    );
}

#[test]
fn importing_a_controller() {
    scenario(
        "deviceImport",
        "Looking at a controller and importing it: starter props, names that are taken, and a controller an FPP listed being filled in.",
        &[FPP, FALCON],
        vec![
            ("inspect_device", json!({ "address": FALCON })),
            ("import_device", json!({ "address": FALCON })),
            ("inspect_device", json!({ "address": FALCON })),
            plain("undo"),
            (
                "import_fpp_destination",
                json!({ "address": FPP, "destination": FALCON, "protocol": "DDP" }),
            ),
            (
                "import_fpp_destination",
                json!({ "address": FPP, "destination": FALCON, "protocol": "DDP" }),
            ),
            (
                "import_fpp_destination",
                json!({ "address": FPP, "destination": "192.0.2.77", "protocol": "DDP" }),
            ),
            (
                "import_fpp_destination",
                json!({ "address": FPP, "destination": FALCON, "protocol": "sACN unicast" }),
            ),
            ("inspect_device", json!({ "address": FALCON })),
            ("import_device", json!({ "address": FALCON })),
            ("import_device", json!({ "address": FPP })),
            ("inspect_device", json!({ "address": "192.0.2.99" })),
            plain("undo"),
            plain("undo"),
        ],
    );
}

#[test]
fn setting_the_show_up_from_an_fpp() {
    scenario(
        "fppSetup",
        "What an FPP sends to becomes the show's controllers in one step, and only as it was shown.",
        &[FPP, FALCON],
        vec![
            ("fpp_setup_plan", json!({ "address": FPP })),
            (
                "fpp_set_up_show",
                json!({ "address": FPP, "expected": ["192.0.2.77"] }),
            ),
            ("fpp_set_up_show", json!({ "address": FPP, "expected": [FALCON] })),
            ("fpp_setup_plan", json!({ "address": FPP })),
            ("fpp_set_up_show", json!({ "address": FPP, "expected": [] })),
            plain("undo"),
            ("fpp_setup_plan", json!({ "address": FPP })),
        ],
    );
}

fn row_id(n: u32) -> String {
    format!("44444444-0000-4000-8000-{n:012}")
}

fn effect_id(n: u32) -> String {
    format!("55555555-0000-4000-8000-{n:012}")
}

fn track_id(n: u32) -> String {
    format!("66666666-0000-4000-8000-{n:012}")
}

fn row(n: u32, prop: u32) -> Value {
    json!({ "id": row_id(n), "target": { "prop": prop_id(prop) }, "layers": [{ "effects": [] }] })
}

/// An effect as the window's `newEffect` builds it.
fn effect(n: u32, kind: &str, start_ms: u64, end_ms: u64) -> Value {
    json!({ "id": effect_id(n), "startMs": start_ms, "endMs": end_ms, "params": { "kind": kind },
            "palette": { "colors": ["#ffffff"] }, "blend": "normal", "fadeInMs": 0, "fadeOutMs": 0 })
}

fn sequence_edits(edits: Value) -> (&'static str, Value) {
    ("edit_sequence", json!({ "edits": edits }))
}

fn dragging(gesture: &str, edits: Value) -> (&'static str, Value) {
    ("edit_sequence", json!({ "edits": edits, "gesture": gesture }))
}

fn timing(id: u32, start_ms: u64, end_ms: u64) -> Value {
    json!({ "type": "setEffectTiming", "id": effect_id(id), "startMs": start_ms, "endMs": end_ms })
}

/// Two strips, and a new sequence with a row for each.
fn two_rows() -> Vec<(&'static str, Value)> {
    vec![
        edits(
            json!([{ "type": "addProp", "prop": strip(1, "Gutter", 4) }, { "type": "addProp", "prop": strip(2, "Fence", 4) }]),
        ),
        (
            "new_sequence_doc",
            json!({ "name": "Song", "durationMs": 20_000, "audio": null, "rows": [row(1, 1), row(2, 2)] }),
        ),
    ]
}

fn add_effect(row: u32, layer: u32, effect: Value) -> Value {
    json!({ "type": "addEffect", "row": row_id(row), "layer": layer, "effect": effect })
}

#[test]
fn sequence_edits_undo_and_redo() {
    let mut replaced = effect(1, "twinkle", 200, 800);
    replaced["blend"] = json!("add");
    replaced["fadeInMs"] = json!(100);
    replaced["palette"] = json!({ "colors": ["#ff0000", "#00ff00"] });
    let mut steps = two_rows();
    steps.extend([
        sequence_edits(json!([
            add_effect(1, 0, effect(1, "on", 0, 1000)),
            add_effect(1, 0, effect(2, "chase", 1000, 2000)),
            add_effect(1, 1, effect(3, "fire", 0, 2000)),
        ])),
        sequence_edits(json!([timing(1, 100, 900)])),
        sequence_edits(json!([{ "type": "setEffectParams", "id": effect_id(2), "params": { "kind": "chase", "speed": 4, "bounce": true } }])),
        sequence_edits(json!([{ "type": "moveEffect", "id": effect_id(2), "row": row_id(2), "layer": 0, "startMs": 2000, "endMs": 3000 }])),
        sequence_edits(json!([{ "type": "moveEffect", "id": effect_id(3), "row": row_id(2), "layer": 1, "startMs": 0, "endMs": 500 }])),
        sequence_edits(json!([{ "type": "updateEffect", "effect": replaced }])),
        sequence_edits(json!([{ "type": "removeEffect", "id": effect_id(3) }])),
        sequence_edits(json!([{ "type": "updateInfo", "name": "Carol", "audio": null, "durationMs": 30_000, "frameMs": 50 }])),
        plain("undo_sequence"),
        plain("undo_sequence"),
        plain("undo_sequence"),
        plain("redo_sequence"),
        sequence_edits(json!([timing(1, 0, 400)])),
        plain("redo_sequence"),
    ]);
    steps.extend((0..8).map(|_| plain("undo_sequence")));
    steps.push(plain("redo_sequence"));
    scenario(
        "sequenceEdits",
        "Effects are added, timed, set, moved between rows and layers, replaced, and removed, each batch one undo step.",
        &[],
        steps,
    );
}

#[test]
fn a_drag_is_one_undo_step() {
    let mut steps = two_rows();
    steps.extend([
        sequence_edits(json!([
            add_effect(1, 0, effect(1, "on", 0, 1000)),
            add_effect(2, 0, effect(2, "on", 0, 1000))
        ])),
        dragging("drag-1", json!([timing(1, 100, 1100)])),
        dragging("drag-1", json!([timing(1, 200, 1200)])),
        dragging("drag-1", json!([timing(1, 300, 1300)])),
        plain("undo_sequence"),
        plain("redo_sequence"),
        // The same gesture after a redo starts a step of its own.
        dragging("drag-1", json!([timing(1, 400, 1400)])),
        plain("undo_sequence"),
        plain("undo_sequence"),
        plain("redo_sequence"),
        // Another edit in between ends the gesture.
        dragging("drag-2", json!([timing(2, 100, 1100)])),
        sequence_edits(json!([timing(1, 0, 500)])),
        dragging("drag-2", json!([timing(2, 200, 1200)])),
        // A move that changes nothing doesn't end it.
        dragging("drag-2", json!([timing(2, 200, 1200)])),
        dragging("drag-2", json!([timing(2, 300, 1300)])),
        // A move that's refused doesn't either.
        dragging("drag-2", json!([timing(2, 500, 500)])),
        dragging("drag-2", json!([timing(2, 400, 1400)])),
        dragging("drag-3", json!([timing(2, 500, 1500)])),
        plain("undo_sequence"),
        plain("undo_sequence"),
        plain("undo_sequence"),
        plain("undo_sequence"),
        plain("undo_sequence"),
        plain("undo_sequence"),
    ]);
    scenario(
        "sequenceGestures",
        "Edits that carry the same gesture id, one after another, undo together.",
        &[],
        steps,
    );
}

#[test]
fn sequence_edits_that_are_refused() {
    let nobody = effect_id(99);
    let track =
        |n: u32, name: &str| json!({ "id": track_id(n), "name": name, "kind": "custom", "marks": [] });
    let mut fast = effect(5, "chase", 0, 1000);
    fast["params"]["speed"] = json!(99);
    let mut steps = vec![
        sequence_edits(json!([])),
        plain("undo_sequence"),
        plain("redo_sequence"),
        (
            "new_sequence_doc",
            json!({ "name": "Long", "durationMs": 5 * 60 * 60 * 1000, "audio": null }),
        ),
        (
            "new_sequence_doc",
            json!({ "name": "Twice", "durationMs": 1000, "audio": null, "rows": [row(1, 1), row(1, 2)] }),
        ),
    ];
    steps.extend(two_rows());
    steps.extend([
        sequence_edits(json!([add_effect(1, 0, effect(1, "on", 0, 1000)), { "type": "addTimingTrack", "track": track(1, "Cues") }])),
        sequence_edits(json!([{ "type": "addRow", "row": row(1, 1) }])),
        sequence_edits(json!([{ "type": "removeRow", "id": row_id(99) }])),
        sequence_edits(json!([{ "type": "moveRow", "id": row_id(99), "index": 0 }])),
        sequence_edits(json!([{ "type": "addLayer", "row": row_id(99) }])),
        sequence_edits(json!([{ "type": "removeLayer", "row": row_id(1), "layer": 5 }])),
        sequence_edits(json!([{ "type": "removeLayer", "row": row_id(99), "layer": 0 }])),
        sequence_edits(json!([add_effect(99, 0, effect(2, "on", 0, 1000))])),
        sequence_edits(json!([add_effect(1, 3, effect(2, "on", 0, 1000))])),
        sequence_edits(json!([add_effect(1, 0, effect(2, "on", 1000, 1000))])),
        sequence_edits(json!([add_effect(1, 0, effect(1, "on", 2000, 3000))])),
        sequence_edits(json!([add_effect(1, 0, effect(2, "on", 0, 1000)), add_effect(2, 0, effect(2, "on", 0, 1000))])),
        sequence_edits(json!([{ "type": "updateEffect", "effect": effect(99, "on", 0, 1000) }])),
        sequence_edits(json!([{ "type": "updateEffect", "effect": effect(1, "on", 500, 100) }])),
        sequence_edits(json!([timing(99, 0, 1000)])),
        sequence_edits(json!([timing(1, 9, 9)])),
        sequence_edits(json!([{ "type": "setEffectParams", "id": nobody, "params": { "kind": "on" } }])),
        sequence_edits(json!([{ "type": "moveEffect", "id": nobody, "row": row_id(1), "layer": 0, "startMs": 0, "endMs": 1000 }])),
        sequence_edits(json!([{ "type": "moveEffect", "id": effect_id(1), "row": row_id(99), "layer": 0, "startMs": 0, "endMs": 1000 }])),
        sequence_edits(json!([{ "type": "moveEffect", "id": effect_id(1), "row": row_id(2), "layer": 4, "startMs": 0, "endMs": 1000 }])),
        sequence_edits(json!([{ "type": "moveEffect", "id": effect_id(1), "row": row_id(2), "layer": 0, "startMs": 700, "endMs": 200 }])),
        sequence_edits(json!([{ "type": "removeEffect", "id": nobody }])),
        sequence_edits(json!([add_effect(1, 0, fast)])),
        sequence_edits(json!([{ "type": "setEffectParams", "id": effect_id(1), "params": { "kind": "chase", "speed": -1 } }])),
        sequence_edits(json!([{ "type": "addTimingTrack", "track": track(1, "Again") }])),
        sequence_edits(json!([{ "type": "updateTimingTrack", "track": track(99, "Nobody") }])),
        sequence_edits(json!([{ "type": "removeTimingTrack", "id": track_id(99) }])),
        sequence_edits(json!([{ "type": "renameTimingTrack", "id": track_id(1), "name": "   " }])),
        sequence_edits(json!([{ "type": "renameTimingTrack", "id": track_id(99), "name": "Nobody" }])),
        sequence_edits(json!([{ "type": "moveTimingTrack", "id": track_id(99), "index": 0 }])),
        // The first edit of a batch is good, the second isn't: neither is kept.
        sequence_edits(json!([timing(1, 100, 900), { "type": "removeRow", "id": row_id(99) }])),
        plain("undo_sequence"),
        plain("undo_sequence"),
    ]);
    scenario(
        "sequenceRejectedEdits",
        "Sequence edits the engine refuses, in its words, and what they leave behind: nothing.",
        &[],
        steps,
    );
}

#[test]
fn rows_and_layers_keep_their_order() {
    let mut steps = two_rows();
    steps.extend([
        sequence_edits(json!([{ "type": "addRow", "row": row(3, 1), "index": 0 }, { "type": "addRow", "row": row(4, 2) }])),
        sequence_edits(json!([{ "type": "addRow", "row": row(5, 1), "index": 99 }])),
        sequence_edits(json!([{ "type": "moveRow", "id": row_id(3), "index": 99 }])),
        sequence_edits(json!([{ "type": "moveRow", "id": row_id(5), "index": 0 }])),
        sequence_edits(json!([
            add_effect(1, 0, effect(1, "on", 0, 1000)),
            { "type": "addLayer", "row": row_id(1) },
            { "type": "addLayer", "row": row_id(1), "index": 0 },
            add_effect(1, 2, effect(2, "on", 0, 1000)),
        ])),
        sequence_edits(json!([{ "type": "removeLayer", "row": row_id(1), "layer": 1 }])),
        sequence_edits(json!([{ "type": "removeRow", "id": row_id(1) }])),
        plain("undo_sequence"),
        plain("undo_sequence"),
        plain("undo_sequence"),
        plain("undo_sequence"),
        plain("redo_sequence"),
        plain("redo_sequence"),
        plain("redo_sequence"),
        plain("redo_sequence"),
    ]);
    scenario(
        "sequenceRows",
        "Rows and layers are added at a place, moved, and removed with what's on them; undo puts the order back.",
        &[],
        steps,
    );
}

#[test]
fn sequence_saving_and_unsaved_changes() {
    let file = "/fixture/Song.pfseq.json";
    let mut steps = two_rows();
    steps.extend([
        plain("save_sequence_doc"),
        sequence_edits(json!([add_effect(1, 0, effect(1, "on", 0, 1000))])),
        ("save_sequence_doc_as", json!({ "path": file })),
        sequence_edits(json!([timing(1, 100, 900)])),
        plain("undo_sequence"),
        plain("save_sequence_doc"),
        plain("redo_sequence"),
        plain("close_sequence_doc"),
        sequence_edits(json!([timing(1, 0, 500)])),
        plain("save_sequence_doc"),
        ("open_sequence_doc", json!({ "path": file })),
        plain("undo_sequence"),
        (
            "new_sequence_doc",
            json!({ "name": "Other", "durationMs": 1000, "audio": "  " }),
        ),
        plain("undo_sequence"),
    ]);
    scenario(
        "sequenceSaving",
        "Whether the open sequence has unsaved changes, through saving, undoing past a save, closing, and opening.",
        &[],
        steps,
    );
}

#[test]
fn timing_tracks_are_edited_like_rows() {
    let track = |n: u32, name: &str, kind: &str, marks: Value| json!({ "id": track_id(n), "name": name, "kind": kind, "marks": marks });
    let mut steps = two_rows();
    steps.extend([
        sequence_edits(json!([
            { "type": "addTimingTrack", "track": track(1, "Beats", "beats", json!([{ "startMs": 0, "endMs": 500, "label": "1" }])) },
            { "type": "addTimingTrack", "track": track(2, "Lyrics", "lyrics", json!([])) },
            { "type": "addTimingTrack", "track": track(3, "Cues", "custom", json!([])) },
        ])),
        sequence_edits(json!([{ "type": "moveTimingTrack", "id": track_id(1), "index": 99 }])),
        sequence_edits(json!([{ "type": "renameTimingTrack", "id": track_id(2), "name": "  Words  " }])),
        sequence_edits(json!([{ "type": "updateTimingTrack", "track": track(3, "Cues", "custom", json!([{ "startMs": 1000, "endMs": 2000, "label": "go" }])) }])),
        sequence_edits(json!([{ "type": "removeTimingTrack", "id": track_id(2) }])),
        plain("undo_sequence"),
        plain("undo_sequence"),
        plain("undo_sequence"),
        plain("undo_sequence"),
        plain("redo_sequence"),
    ]);
    scenario(
        "sequenceTracks",
        "Timing tracks are added, moved, renamed, replaced, and removed; undo puts them back in order.",
        &[],
        steps,
    );
}

#[test]
fn how_rows_meet_the_rows_above_is_an_edit_like_any_other() {
    let blending = |how: &str| sequence_edits(json!([{ "type": "setRowBlending", "rowBlending": how }]));
    let mut steps = two_rows();
    steps.extend([
        blending("blend"),
        blending("blend"),
        blending("replace"),
        plain("undo_sequence"),
        plain("undo_sequence"),
        plain("redo_sequence"),
    ]);
    scenario(
        "sequenceRowBlending",
        "How rows meet the rows above them is set, set again to the same, and undone.",
        &[],
        steps,
    );
}

#[test]
fn which_channel_is_how_loud_the_music_is_is_an_edit_like_any_other() {
    let level =
        |from: &str| sequence_edits(json!([{ "type": "setMusicLevelSource", "musicLevelSource": from }]));
    let file = "/fixture/Song.pfseq.json";
    let mut steps = two_rows();
    steps.extend([
        level("louderChannel"),
        level("leftChannel"),
        level("leftChannel"),
        ("save_sequence_doc_as", json!({ "path": file })),
        plain("undo_sequence"),
        plain("redo_sequence"),
        plain("close_sequence_doc"),
        ("open_sequence_doc", json!({ "path": file })),
    ]);
    scenario(
        "sequenceMusicLevel",
        "Which channel of the music is how loud it is: set, set again to the same, saved, undone, and opened again.",
        &[],
        steps,
    );
}

#[test]
fn an_exported_sequence_goes_on_the_playlist() {
    scenario(
        "sequenceInShow",
        "An exported sequence is added to the show's playlist under the sequence's name, once per file.",
        &[],
        vec![
            (
                "add_sequence_doc_to_show",
                json!({ "path": "/fixture/sequences/Song.fseq" }),
            ),
            (
                "new_sequence_doc",
                json!({ "name": "Song", "durationMs": 1000, "audio": "/fixture/music/Song.mp3" }),
            ),
            (
                "add_sequence_doc_to_show",
                json!({ "path": "/fixture/sequences/Song.fseq" }),
            ),
            (
                "add_sequence_doc_to_show",
                json!({ "path": "/fixture/sequences/Song.fseq" }),
            ),
            (
                "add_sequence_doc_to_show",
                json!({ "path": "/fixture/sequences/Song again.fseq" }),
            ),
            sequence_edits(
                json!([{ "type": "updateInfo", "name": "  ", "audio": null, "durationMs": 1000, "frameMs": 25 }]),
            ),
            (
                "add_sequence_doc_to_show",
                json!({ "path": "/fixture/sequences/Unnamed.fseq" }),
            ),
            (
                "add_sequence_doc_to_show",
                json!({ "path": "/fixture/sequences/Song.fseq" }),
            ),
            plain("undo"),
            plain("undo"),
        ],
    );
}
