//! Replies about the open sequence: the document, what edits changed, its files and pictures,
//! and what importing from xLights gives.

use super::*;

pub(super) fn row_id(n: u32) -> String {
    format!("44444444-0000-4000-8000-{n:012}")
}

pub(super) fn effect_id(n: u32) -> String {
    format!("55555555-0000-4000-8000-{n:012}")
}

pub(super) fn track_id(n: u32) -> String {
    format!("66666666-0000-4000-8000-{n:012}")
}

pub(super) fn row(n: u32, target: Value) -> Value {
    json!({ "id": row_id(n), "target": target, "layers": [{ "effects": [] }] })
}

/// An effect as the window's `newEffect` builds it: its kind, with every setting left to the engine.
pub(super) fn effect(n: u32, kind: &Value, start_ms: u64, end_ms: u64) -> Value {
    json!({ "id": effect_id(n), "startMs": start_ms, "endMs": end_ms, "params": { "kind": kind },
            "palette": { "colors": ["#ff0000", "#00ff00"] }, "blend": "normal", "fadeInMs": 0, "fadeOutMs": 0 })
}

fn track(n: u32, name: &str, kind: &str, marks: Value) -> Value {
    json!({ "id": track_id(n), "name": name, "kind": kind, "marks": marks })
}

#[test]
fn sequence_replies() {
    let shell = Shell::new();
    let dir = shell.dir.path().to_path_buf();
    let mut fixtures = Fixtures::new("sequence", shell.stable());
    let region = "88888888-0000-4000-8000-000000000001";
    let mut matrix = prop(
        2,
        "Matrix",
        json!({ "type": "matrix", "columns": 3, "rows": 2, "width": 3, "height": 2 }),
    );
    matrix["regions"] = json!([{ "id": region, "name": "Top row", "kind": "nodes",
        "lines": [[{ "first": 0, "last": 2 }]], "layout": "horizontal", "buffer": "default" }]);
    shell.ok(
        "apply_edits",
        json!({ "edits": [
            { "type": "addProp", "prop": strip(1, "Strip", 4) },
            { "type": "addProp", "prop": matrix },
            { "type": "addGroup", "group": { "id": "22222222-0000-4000-8000-000000000001", "name": "All", "members": [prop_id(1), prop_id(2)] } },
            { "type": "addController", "controller": bench(1, &[1, 2]) },
        ] }),
    );
    fixtures.reply(
        "no sequence is open",
        "sequence.SequenceSnapshot | null",
        &shell.ok("get_sequence_doc", json!({})),
    );

    // A new sequence with its music and a row for each kind of target.
    let song = dir.join("Song.wav");
    write_wav_ms(&song, 1000);
    let rows = json!([
        row(1, json!({ "prop": prop_id(1) })),
        row(2, json!({ "group": "22222222-0000-4000-8000-000000000001" })),
        row(3, json!({ "region": { "prop": prop_id(2), "region": region } })),
    ]);
    let new = shell.ok(
        "new_sequence_doc",
        json!({ "name": "Song", "durationMs": 40_000, "audio": song, "rows": rows }),
    );
    assert_eq!((&new["dirty"], &new["path"]), (&json!(false), &Value::Null));
    fixtures.reply(
        "a new sequence with rows for a prop, a group, and a submodel",
        "sequence.SequenceSnapshot",
        &new,
    );

    // One effect of every kind: the reply carries each with all its settings filled in.
    let kinds: Vec<Value> = pf_sequence::EffectKind::ALL.iter().map(wire).collect();
    let add: Vec<Value> = kinds
        .iter()
        .zip(1u32..)
        .map(|(kind, n)| {
            let start = u64::from(n - 1) * 1000;
            json!({ "type": "addEffect", "row": row_id(1), "layer": 0, "effect": effect(n, kind, start, start + 1000) })
        })
        .collect();
    let added = shell.ok("edit_sequence", json!({ "edits": add }));
    assert_eq!(added["changes"]["effects"].as_array().unwrap().len(), kinds.len());
    assert!(
        !added["issues"].as_array().unwrap().is_empty(),
        "{}",
        added["issues"]
    );
    fixtures.reply(
        "an effect of every kind added, with the settings the engine gives each",
        "sequence.SequenceEditResult",
        &added,
    );
    let undone = shell.ok("undo_sequence", json!({}));
    assert_eq!(undone["changes"]["rows"][0]["id"], row_id(1));
    fixtures.reply(
        "an undo, which sends whole rows",
        "sequence.SequenceEditResult",
        &undone,
    );

    // Timing tracks of every kind, a sung word, and effects with everything an effect can carry.
    let mut sung = json!({ "startMs": 1000, "endMs": 1400, "label": "afraid" });
    sung["sung"] = json!("fraid");
    let tracks = [
        track(
            1,
            "Beats",
            "beats",
            json!([{ "startMs": 0, "endMs": 500, "label": "1" }, { "startMs": 500, "endMs": 1000, "label": "2" }]),
        ),
        track(
            2,
            "Bars",
            "bars",
            json!([{ "startMs": 0, "endMs": 2000, "label": "1" }]),
        ),
        track(
            3,
            "Sections",
            "sections",
            json!([{ "startMs": 0, "endMs": 20_000, "label": "Verse" }]),
        ),
        track(
            4,
            "Lyrics",
            "lyrics",
            json!([{ "startMs": 1000, "endMs": 2000, "label": "afraid of the dark" }]),
        ),
        track(
            5,
            "Lyrics (words)",
            "words",
            json!([sung, { "startMs": 1400, "endMs": 1500, "label": "of" }]),
        ),
        track(
            6,
            "Lyrics (phonemes)",
            "phonemes",
            json!([{ "startMs": 1000, "endMs": 1100, "label": "AI" }]),
        ),
        track(7, "Cues", "custom", json!([])),
    ];
    let mut decorated = effect(100, &json!("chase"), 0, 4000);
    decorated["blend"] = json!("add");
    decorated["fadeInMs"] = json!(250);
    decorated["fadeOutMs"] = json!(500);
    decorated["sparkles"] = json!(50);
    decorated["sparkleColor"] = json!("#ffcc00");
    decorated["musicSparkles"] = json!(true);
    decorated["blur"] = json!(3);
    decorated["hueAdjust"] = json!(0.25);
    decorated["saturationAdjust"] = json!(-0.5);
    decorated["valueAdjust"] = json!(0.1);
    decorated["brightness"] = json!(1.5);
    decorated["contrast"] = json!(0.2);
    decorated["renderStyle"] = json!("perPreview");
    decorated["bufferTransform"] = json!("rotateCw90");
    decorated["params"]["timingTrack"] = json!(track_id(1));
    decorated["curves"] = json!({
        "speed": { "shape": "sine", "from": 1, "to": 5, "cycles": 2 },
        "sparkles": { "shape": "custom", "from": 0, "to": 100, "points": [[0, 0], [0.5, 1], [1, 0]] },
        "blur": { "shape": "timingFade", "from": 0, "to": 14, "fade": 3, "timingTrack": track_id(1) },
        "width": { "shape": "musicTrigger", "from": 0.1, "to": 0.9, "gain": 20, "trigger": 50, "fade": 2 },
    });
    let mut edits: Vec<Value> = tracks
        .iter()
        .map(|track| json!({ "type": "addTimingTrack", "track": track }))
        .collect();
    edits.extend([
        json!({ "type": "addEffect", "row": row_id(1), "layer": 0, "effect": effect(1, &json!("on"), 0, 1000) }),
        json!({ "type": "addEffect", "row": row_id(1), "layer": 0, "effect": effect(2, &json!("twinkle"), 1000, 2000) }),
        json!({ "type": "addEffect", "row": row_id(2), "layer": 1, "effect": decorated }),
    ]);
    let dressed = shell.ok("edit_sequence", json!({ "edits": edits }));
    fixtures.reply(
        "timing tracks of every kind, and an effect with curves and sparkles",
        "sequence.SequenceEditResult",
        &dressed,
    );

    // One batch that changes a bit of everything a reply can list.
    let batch = shell.ok(
        "edit_sequence",
        json!({ "edits": [
            { "type": "updateInfo", "name": "Song (remix)", "audio": song, "durationMs": 45_000, "frameMs": 50 },
            { "type": "addRow", "row": row(4, json!({ "prop": prop_id(2) })), "index": 0 },
            { "type": "removeRow", "id": row_id(3) },
            { "type": "setEffectTiming", "id": effect_id(1), "startMs": 100, "endMs": 900 },
            { "type": "removeEffect", "id": effect_id(2) },
            { "type": "updateTimingTrack", "track": track(7, "Cues", "custom", json!([{ "startMs": 0, "endMs": 100, "label": "go" }])) },
            { "type": "removeTimingTrack", "id": track_id(3) },
            { "type": "moveTimingTrack", "id": track_id(2), "index": 0 },
        ] }),
    );
    let changes = &batch["changes"];
    for key in [
        "rows",
        "removedRows",
        "effects",
        "removedEffects",
        "timingTracks",
        "removedTimingTracks",
    ] {
        assert!(!changes[key].as_array().unwrap().is_empty(), "{key}: {changes}");
    }
    assert!(
        changes["info"].is_object() && changes["rowOrder"].is_array() && changes["trackOrder"].is_array()
    );
    fixtures.reply(
        "an edit that changes a bit of everything",
        "sequence.SequenceEditResult",
        &batch,
    );
    let nothing = shell.ok("edit_sequence", json!({ "edits": [] }));
    assert_eq!(nothing["changed"], false);
    fixtures.reply(
        "an edit that changed nothing",
        "sequence.SequenceEditResult",
        &nothing,
    );

    // Saved, it has a path; the whole document comes back.
    let file = dir.join("Song.pfseq.json");
    let saved = shell.ok("save_sequence_doc_as", json!({ "path": file }));
    assert_eq!(saved["dirty"], false);
    fixtures.reply("a saved sequence, whole", "sequence.SequenceSnapshot", &saved);

    // How an export lays out the channels, and what it wrote.
    fixtures.reply(
        "an export's channel layout",
        "sequence.ExportLayout",
        &shell.ok("sequence_export_layout", json!({})),
    );
    let exported = shell.ok("export_sequence_doc", json!({ "path": dir.join("Song.fseq") }));
    assert!(exported["media"].is_string(), "{exported}");
    fixtures.reply(
        "an exported sequence with music",
        "sequence.ExportSummary",
        &exported,
    );
    // A controller the FPP's output list gave its channels, and one without ports.
    let mut listed = controller(2, "Falcon", "192.0.2.20", json!({ "type": "ddp" }), json!([]));
    listed["sequenceChannels"] = json!({ "start": 101, "count": 30 });
    shell.ok(
        "apply_edits",
        json!({ "edits": [{ "type": "addController", "controller": listed }] }),
    );
    let layout = shell.ok("sequence_export_layout", json!({}));
    assert!(
        layout["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["fromSequenceChannels"] == true),
        "{layout}"
    );
    fixtures.reply(
        "an export's layout with a controller's channels from its FPP",
        "sequence.ExportLayout",
        &layout,
    );

    // Timing tracks from an xLights timing file, one mark of which is past the end.
    let xtiming = dir.join("Vocals.xtiming");
    std::fs::write(
        &xtiming,
        r#"<timing name="Vocals"><EffectLayer><Effect label="Hi there" starttime="0" endtime="1000"/><Effect label="late" starttime="50000" endtime="60000"/></EffectLayer>
<EffectLayer><Effect label="Hi" starttime="0" endtime="400"/><Effect label="there" starttime="400" endtime="1000"/></EffectLayer></timing>"#,
    )
    .unwrap();
    let imported = shell.ok("import_timing_file", json!({ "path": xtiming }));
    assert!(!imported["notes"].as_array().unwrap().is_empty());
    fixtures.reply("a timing file imported", "sequence.TimingImported", &imported);
    fixtures.reply(
        "how many marks a timing export wrote",
        "number",
        &shell.ok(
            "export_timing_track",
            json!({ "id": track_id(1), "path": dir.join("Beats.txt") }),
        ),
    );

    // A sequence imported from xLights keeps how its rows meet, and which channel of its music
    // is how loud it is: read from its own file.
    let blended = dir.join("Blended.pfseq.json");
    let mut doc = saved["sequence"].clone();
    doc["rowBlending"] = json!("blend");
    doc["musicLevelSource"] = json!("leftChannel");
    doc["audio"] = Value::Null;
    std::fs::write(&blended, doc.to_string()).unwrap();
    let opened = shell.ok("open_sequence_doc", json!({ "path": blended }));
    assert_eq!(opened["sequence"]["rowBlending"], "blend");
    assert_eq!(opened["sequence"]["musicLevelSource"], "leftChannel");
    fixtures.reply(
        "a sequence whose rows blend, without music",
        "sequence.SequenceSnapshot",
        &opened,
    );

    // Unsaved work kept when the app quits is offered back by the next run: one sequence that
    // has a file, and one that never had.
    shell.ok(
        "edit_sequence",
        json!({ "edits": [{ "type": "removeRow", "id": row_id(1) }] }),
    );
    let dir = shell.quit();
    let next = Shell::in_dir(dir, pf_ai::MemoryStore::new());
    next.ok(
        "new_sequence_doc",
        json!({ "name": "Untitled", "durationMs": 1000, "audio": null }),
    );
    next.ok(
        "edit_sequence",
        json!({ "edits": [{ "type": "addRow", "row": row(1, json!({ "prop": prop_id(1) })) }] }),
    );
    let dir = next.quit();
    let last = Shell::in_dir(dir, pf_ai::MemoryStore::new());
    let offered = last.ok("sequence_recoveries", json!({}));
    let kept = offered.as_array().unwrap();
    assert_eq!(kept.len(), 2, "{offered}");
    assert!(kept.iter().any(|k| k["path"].is_null()) && kept.iter().any(|k| k["path"].is_string()));
    // (A kept sequence is named after the time and the process that kept it.)
    for (n, recovery) in kept.iter().enumerate() {
        fixtures.swap(recovery["id"].as_str().unwrap(), &format!("1700000000000-1-{n}"));
    }
    fixtures.reply(
        "unsaved sequences an earlier run kept",
        "sequence.SequenceRecovery[]",
        &offered,
    );
    let recovered = last.ok("recover_sequence", json!({ "id": kept[0]["id"] }));
    assert_eq!(recovered["dirty"], true);
    fixtures.finish();
}

/// The smallest GIF with something to see: one red pixel.
const RED_GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\x00\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";

#[test]
fn sequence_file_replies() {
    let shell = Shell::new();
    let dir = shell.dir.path().to_path_buf();
    let mut fixtures = Fixtures::new("sequenceFiles", shell.stable());
    let folder = dir.join("Show");
    std::fs::create_dir_all(folder.join("Music")).unwrap();
    shell.ok(
        "apply_edits",
        json!({ "edits": [{ "type": "addProp", "prop": strip(1, "Strip", 4) }] }),
    );
    shell.ok(
        "save_show_as",
        json!({ "path": folder.join("House.pixelflow.json") }),
    );

    // The sequence's music moves to another folder of the show.
    let music = folder.join("Music").join("Carol.wav");
    write_wav_ms(&music, 500);
    shell.ok(
        "new_sequence_doc",
        json!({ "name": "Carol", "durationMs": 2000, "audio": music }),
    );
    shell.ok(
        "save_sequence_doc_as",
        json!({ "path": folder.join("Carol.pfseq.json") }),
    );
    fixtures.reply(
        "the sequence's music is where it says",
        "types.MissingFile | null",
        &shell.ok("sequence_music_missing", json!({})),
    );
    std::fs::create_dir_all(folder.join("Audio")).unwrap();
    std::fs::rename(&music, folder.join("Audio").join("Carol.wav")).unwrap();
    let missing = shell.ok("sequence_music_missing", json!({}));
    assert_eq!(missing["file"], json!({ "kind": "sequenceDocMusic" }));
    fixtures.reply(
        "the sequence's music has moved",
        "types.MissingFile | null",
        &missing,
    );
    let found = shell.ok("find_sequence_music", json!({}));
    assert!(
        found["found"].is_object() && found["result"].is_object(),
        "{found}"
    );
    fixtures.reply("the sequence's music found again", "sequencer.MusicFound", &found);
    fixtures.reply(
        "nothing to find",
        "sequencer.MusicFound",
        &shell.ok("find_sequence_music", json!({})),
    );

    // A Picture effect names a picture as another computer had it; it's in the show's folder.
    let mut picture = effect(1, &json!("picture"), 0, 2000);
    picture["params"]["file"] = json!(r"C:\Users\someone\xlights\Images\star.gif");
    shell.ok(
        "edit_sequence",
        json!({ "edits": [
            { "type": "addRow", "row": row(1, json!({ "prop": prop_id(1) })) },
            { "type": "addEffect", "row": row_id(1), "layer": 0, "effect": picture },
        ] }),
    );
    let checked = shell.ok("check_sequence_pictures", json!({}));
    assert_eq!(checked["missing"][0]["file"], json!({ "kind": "picture" }));
    assert!(checked["issues"][0]["effect"].is_string(), "{checked}");
    fixtures.reply(
        "a picture that can't be drawn",
        "sequencer.PicturesChecked",
        &checked,
    );
    std::fs::create_dir_all(folder.join("Assets")).unwrap();
    std::fs::write(folder.join("Assets").join("star.gif"), RED_GIF).unwrap();
    let relinked = shell.ok("find_sequence_pictures", json!({}));
    assert_eq!(relinked["found"], json!(["star.gif"]));
    fixtures.reply("a picture found again", "sequencer.PicturesRelinked", &relinked);
    fixtures.reply(
        "the show's pictures",
        "string[]",
        &shell.ok("list_pictures", json!({})),
    );
    // The shaders a Shader effect can use: PixelFlow's own, then the files in the show's shaders
    // folder, one of which can't be run.
    std::fs::create_dir_all(folder.join("shaders")).unwrap();
    std::fs::write(
        folder.join("shaders").join("rings.fs"),
        r#"/*{ "DESCRIPTION": "Rings from a point.", "CREDIT": "A test", "INPUTS": [
            { "NAME": "rate", "LABEL": "Rate", "TYPE": "float", "MIN": 0.0, "MAX": 4.0, "DEFAULT": 1.0 },
            { "NAME": "flip", "TYPE": "bool", "DEFAULT": true },
            { "NAME": "mode", "TYPE": "long", "VALUES": [0, 2], "LABELS": ["Soft", "Hard"], "DEFAULT": 2 },
            { "NAME": "at", "TYPE": "point2D", "DEFAULT": [0.5, 0.5] },
            { "NAME": "tint", "TYPE": "color", "DEFAULT": [1.0, 0.5, 0.0, 1.0] } ] }*/
        void main() { gl_FragColor = vec4(rate, at, flip ? 1.0 : 0.0) * tint * float(mode); }"#,
    )
    .unwrap();
    std::fs::write(
        folder.join("shaders").join("broken.fs"),
        "void main() {\n  nothing();\n}",
    )
    .unwrap();
    let shaders = shell.ok("list_shaders", json!({}));
    let listed = shaders.as_array().unwrap();
    assert_eq!(listed[0]["file"], "pixelflow:aurora");
    assert_eq!(listed[listed.len() - 1]["file"], "shaders/rings.fs");
    assert_eq!(listed[listed.len() - 1]["inputs"].as_array().unwrap().len(), 5);
    fixtures.reply(
        "the shaders an effect can use",
        "sequencer.ShaderInfo[]",
        &shaders,
    );
    let broken = shell.ok("shader_info", json!({ "file": "shaders/broken.fs" }));
    assert_eq!(broken["line"], 2);
    fixtures.reply("a shader file that can't be run", "sequencer.ShaderInfo", &broken);
    fixtures.reply(
        "a shader file that isn't there",
        "sequencer.ShaderInfo",
        &shell.ok("shader_info", json!({ "file": "shaders/gone.fs" })),
    );
    fixtures.reply(
        "a Shader effect that wasn't stopped",
        "string | null",
        &shell.ok("shader_stopped", json!({ "effect": effect_id(1) })),
    );
    // Nothing found: the same reply without an edit (built; a search that finds none of several
    // missing pictures gives it).
    let still = pictures::PicturesRelinked {
        found: Vec::new(),
        result: None,
        missing: vec![pf_engine::MissingFile {
            file: pf_engine::FileRole::Picture,
            name: "star.gif".into(),
            path: r"C:\Users\someone\xlights\Images\star.gif".into(),
            was_at: r"C:\Users\someone\xlights\Images\star.gif".into(),
            owner: "Picture effect at 0:00.000".into(),
            message: "star.gif isn't where it was.".into(),
        }],
        issues: Vec::new(),
        gave_up: true,
    };
    fixtures.reply(
        "a search for pictures that gave up (built)",
        "sequencer.PicturesRelinked",
        &wire(&still),
    );
    fixtures.finish();
}

fn xlights_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("pf-xlights")
        .join("fixtures")
}

#[test]
fn xlights_replies() {
    let shell = Shell::new();
    let dir = shell.dir.path().to_path_buf();
    let from = xlights_fixtures();
    // (The repository's own folder is somewhere else on every computer.)
    let mut fixtures = Fixtures::new("xlights", shell.stable());
    for form in [from.clone(), from.canonicalize().unwrap()] {
        fixtures.swap(&form.display().to_string(), "/fixture/xlights");
        fixtures.swap(&pf_model::path_to_text(&form), "/fixture/xlights");
    }
    let imported = shell.ok("import_xlights", json!({ "folder": from.join("sample-show") }));
    assert!(imported["summary"]["props"].as_u64().unwrap() > 0);
    fixtures.reply(
        "an xLights show folder imported",
        "types.XlightsImported",
        &imported,
    );
    let sequence = shell.ok(
        "import_xlights_sequence",
        json!({ "path": from.join("sequences").join("effects.xsq") }),
    );
    assert!(sequence["summary"]["effects"].as_u64().unwrap() > 0);
    fixtures.reply(
        "an xLights sequence imported onto the show",
        "types.XlightsSequenceImported",
        &sequence,
    );

    // A vendor's package: what's in it, and how it maps onto the show.
    let package = dir.join("Vendor Song.zip");
    write_zip(
        &package,
        &[
            (
                "Vendor/xlights_rgbeffects.xml",
                &std::fs::read(from.join("sample-show").join("xlights_rgbeffects.xml")).unwrap(),
            ),
            (
                "Vendor/Sequences/effects.xsq",
                &std::fs::read(from.join("sequences").join("effects.xsq")).unwrap(),
            ),
            ("Vendor/Music/effects.mp3", b"ID3 not really a song"),
        ],
    );
    let inspected = shell.ok("inspect_xlights_sequence", json!({ "path": package }));
    assert_eq!(
        (&inspected["music"], &inspected["musicFolder"]),
        (&json!("effects.mp3"), &Value::Null)
    );
    fixtures.reply(
        "a vendor package looked into, the show not saved yet",
        "types.VendorInspection",
        &inspected,
    );
    let house = dir.join("House");
    std::fs::create_dir_all(&house).unwrap();
    shell.ok(
        "save_show_as",
        json!({ "path": house.join("House.pixelflow.json") }),
    );
    let mut mapping = inspected["mapping"].clone();
    mapping["items"]["Mega Tree"] = json!([]);
    let mapped = shell.ok(
        "import_xlights_sequence",
        json!({ "path": package, "sequence": inspected["sequence"], "mapping": mapping, "key": inspected["key"], "musicFolder": null }),
    );
    assert!(
        mapped["snapshot"]["sequence"]["audio"].is_string(),
        "{}",
        mapped["snapshot"]["sequence"]["audio"]
    );
    let mut summary = mapped.clone();
    // (The sequence itself is the one above with a row left out: only the report differs.)
    summary["snapshot"]["sequence"]["rows"] = json!([]);
    summary["snapshot"]["sequence"]["timingTracks"] = json!([]);
    summary["snapshot"]["issues"] = json!([]);
    fixtures.reply(
        "a vendor sequence imported with a mapping (its rows left out here)",
        "types.XlightsSequenceImported",
        &summary,
    );
    let again = shell.ok(
        "inspect_xlights_sequence",
        json!({ "path": package, "sequence": inspected["sequence"] }),
    );
    assert!(again["musicFolder"].is_string());
    fixtures.reply(
        "the package again: the mapping remembered, the show saved",
        "types.VendorInspection",
        &again,
    );

    // An xLights mapping file.
    let xmap = dir.join("vendor.xmap");
    shell.ok(
        "write_xmap",
        json!({ "path": xmap, "mapping": { "items": { "Mega Tree": ["Tree", "Pylons"], "Arch/Left": [] } } }),
    );
    fixtures.reply(
        "an xLights mapping file read",
        "types.XmapRead",
        &shell.ok("read_xmap", json!({ "path": xmap })),
    );
    fixtures.finish();
}
