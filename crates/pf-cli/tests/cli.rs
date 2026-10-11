//! Runs the `pixelflow` binary against example show files.

use std::path::PathBuf;
use std::process::{Command, Output};

fn demo_show() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/shows/demo.pixelflow.json")
}

fn pixelflow(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pixelflow"))
        .args(args)
        .output()
        .expect("failed to run pixelflow")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn validate_demo_show_reports_summary_and_no_problems() {
    let output = pixelflow(&["validate", demo_show().to_str().unwrap()]);
    let text = stdout(&output);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("Demo House"), "{text}");
    assert!(
        text.contains("4 props · 1,150 pixels · 2 controllers · 7 universes"),
        "{text}"
    );
    assert!(text.contains("No problems found."), "{text}");
}

#[test]
fn validate_reports_a_full_port_as_a_warning_and_errors_with_exit_1() {
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(demo_show()).unwrap()).unwrap();
    doc["controllers"][0]["ports"][0]["maxPixels"] = 100.into();
    let path = std::env::temp_dir().join(format!("pixelflow-overloaded-{}.json", std::process::id()));
    std::fs::write(&path, doc.to_string()).unwrap();

    let output = pixelflow(&["validate", path.to_str().unwrap()]);
    let text = stdout(&output);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(text.contains("0 errors, 1 warning"), "{text}");
    assert!(
        text.contains("Port 1 on 'Main FPP' is over capacity by 151 pixels (251 of 100)."),
        "{text}"
    );
    assert!(text.contains("Fix: Move a prop to another port"), "{text}");

    doc["controllers"][0]["ports"][0]["brightness"] = 200.into();
    std::fs::write(&path, doc.to_string()).unwrap();
    let output = pixelflow(&["validate", path.to_str().unwrap()]);
    let text = stdout(&output);
    std::fs::remove_file(&path).ok();
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("1 error, 1 warning"), "{text}");
}

#[test]
fn map_prints_ports_props_and_addresses() {
    let output = pixelflow(&["map", demo_show().to_str().unwrap()]);
    let text = stdout(&output);
    assert!(output.status.success(), "{text}");
    assert!(
        text.contains("Main FPP  192.168.1.50  sACN universes 1–7 (unicast)"),
        "{text}"
    );
    assert!(text.contains("Porch WLED  192.168.1.60  DDP"), "{text}");
    let arch = text.lines().find(|l| l.contains("Garage Arch")).unwrap();
    assert!(arch.contains("U1:1 → U1:150"), "{arch}");
    let star = text.lines().find(|l| l.contains("Porch Star")).unwrap();
    assert!(star.contains("@0 → @399"), "{star}");
}

#[test]
fn map_json_is_machine_readable() {
    let output = pixelflow(&["map", "--json", demo_show().to_str().unwrap()]);
    let map: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(map["frameLen"], (50 + 800 + 200) * 3 + 100 * 4);
    assert_eq!(map["controllers"][1]["addressing"]["type"], "ddp");
}

#[test]
fn missing_file_exits_2_with_message() {
    let output = pixelflow(&["validate", "/no/such/show.json"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not read /no/such/show.json"));
}

#[test]
fn validate_rejects_huge_null_pixel_counts_quickly() {
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(demo_show()).unwrap()).unwrap();
    doc["controllers"][0]["ports"][0]["slots"][0]["nullPixels"] = 4_294_967_295u64.into();
    let path = std::env::temp_dir().join(format!("pixelflow-hugenull-{}.json", std::process::id()));
    std::fs::write(&path, doc.to_string()).unwrap();

    let started = std::time::Instant::now();
    let output = pixelflow(&["validate", path.to_str().unwrap()]);
    std::fs::remove_file(&path).ok();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("at most 1000"), "{stderr}");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

fn loopback_show(address: &str) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "name": "Bench",
        "props": [{
            "id": "11111111-0000-4000-8000-000000000001",
            "name": "Strip",
            "colorOrder": "GRB",
            "shape": { "source": "generator", "type": "line", "nodes": 2, "length": 1.0 }
        }],
        "controllers": [{
            "id": "33333333-0000-4000-8000-000000000001",
            "name": "Bench WLED",
            "address": address,
            "protocol": { "type": "ddp" },
            "ports": [{ "number": 1, "slots": [{ "prop": "11111111-0000-4000-8000-000000000001" }] }]
        }]
    })
}

#[test]
fn test_pattern_sends_ddp_frames_to_a_controller() {
    let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    receiver
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let address = receiver.local_addr().unwrap().to_string();
    let show = loopback_show(&address);
    let path = std::env::temp_dir().join(format!("pixelflow-bench-{}.json", std::process::id()));
    std::fs::write(&path, show.to_string()).unwrap();

    let child = std::thread::spawn({
        let path = path.clone();
        move || {
            pixelflow(&[
                "test-pattern",
                path.to_str().unwrap(),
                "--pattern",
                "solid",
                "--color",
                "ff0000",
                "--seconds",
                "0.5",
                "--bind",
                "127.0.0.1",
            ])
        }
    });
    // Skip any all-zero packets; the first lit frame must be red.
    let mut buf = [0u8; 64];
    let n = loop {
        let (n, _) = receiver.recv_from(&mut buf).unwrap();
        if buf[10..n].iter().any(|&b| b != 0) {
            break n;
        }
    };
    let output = child.join().unwrap();
    std::fs::remove_file(&path).ok();

    // Red in GRB order is (0, 255, 0) per pixel.
    assert_eq!(&buf[10..n], &[0, 255, 0, 0, 255, 0]);
    let text = stdout(&output);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("Sent "), "{text}");
    assert!(text.contains("Bench WLED"), "{text}");
}

#[cfg(unix)]
#[test]
fn ctrl_c_ends_a_test_pattern_early_and_still_blacks_out() {
    let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    receiver
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let address = receiver.local_addr().unwrap().to_string();
    let path = std::env::temp_dir().join(format!("pixelflow-ctrlc-{}.json", std::process::id()));
    std::fs::write(&path, loopback_show(&address).to_string()).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_pixelflow"))
        .args([
            "test-pattern",
            path.to_str().unwrap(),
            "--pattern",
            "solid",
            "--seconds",
            "60",
            "--bind",
            "127.0.0.1",
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut buf = [0u8; 64];
    loop {
        let (n, _) = receiver.recv_from(&mut buf).unwrap();
        if buf[10..n].iter().any(|&b| b != 0) {
            break;
        }
    }
    let started = std::time::Instant::now();
    let killed = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    // Read until the blackout: three all-zero frames in a row.
    let mut black = 0;
    while black < 3 {
        let (n, _) = receiver.recv_from(&mut buf).unwrap();
        black = if buf[10..n].iter().all(|&b| b == 0) {
            black + 1
        } else {
            0
        };
    }
    let output = child.wait_with_output().unwrap();
    std::fs::remove_file(&path).ok();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "stopped early"
    );
    let text = stdout(&output);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("Sent "), "{text}");
}

#[test]
fn test_pattern_rejects_a_sync_universe_outside_1_to_63999() {
    let path = std::env::temp_dir().join(format!("pixelflow-sync-{}.json", std::process::id()));
    std::fs::write(&path, loopback_show("127.0.0.1:4048").to_string()).unwrap();
    let output = pixelflow(&[
        "test-pattern",
        path.to_str().unwrap(),
        "--sync-universe",
        "64000",
        "--bind",
        "127.0.0.1",
    ]);
    std::fs::remove_file(&path).ok();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("--sync-universe must be between 1 and 63999, not 64000"),
        "{stderr}"
    );
}

#[test]
fn test_pattern_refuses_a_target_with_no_pixels() {
    let mut show = loopback_show("127.0.0.1:4048");
    show["controllers"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": "33333333-0000-4000-8000-000000000002",
            "name": "Spare",
            "address": "127.0.0.1:4049",
            "protocol": { "type": "ddp" },
            "ports": []
        }));
    let path = std::env::temp_dir().join(format!("pixelflow-empty-{}.json", std::process::id()));
    std::fs::write(&path, show.to_string()).unwrap();
    let output = pixelflow(&[
        "test-pattern",
        path.to_str().unwrap(),
        "--target",
        "controller:Spare",
        "--bind",
        "127.0.0.1",
    ]);
    std::fs::remove_file(&path).ok();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("'controller:Spare' has no pixels to light"),
        "{stderr}"
    );
}

#[test]
fn test_pattern_refuses_shows_with_errors() {
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(demo_show()).unwrap()).unwrap();
    doc["controllers"][0]["ports"][0]["brightness"] = 200.into();
    let path = std::env::temp_dir().join(format!("pixelflow-refuse-{}.json", std::process::id()));
    std::fs::write(&path, doc.to_string()).unwrap();
    let output = pixelflow(&[
        "test-pattern",
        path.to_str().unwrap(),
        "--seconds",
        "0",
        "--bind",
        "127.0.0.1",
    ]);
    std::fs::remove_file(&path).ok();
    let text = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("Fix these problems before sending output."),
        "{text}"
    );
}

#[test]
fn test_pattern_rejects_a_non_finite_duration_without_panicking() {
    let show = loopback_show("127.0.0.1:4048");
    let path = std::env::temp_dir().join(format!("pixelflow-inf-{}.json", std::process::id()));
    std::fs::write(&path, show.to_string()).unwrap();
    let output = pixelflow(&[
        "test-pattern",
        path.to_str().unwrap(),
        "--seconds",
        "inf",
        "--bind",
        "127.0.0.1",
    ]);
    std::fs::remove_file(&path).ok();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("--seconds must be a number of seconds between 0 and 86400"),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn an_xlights_sequence_imports_onto_a_show_and_saves_as_a_sequence_file() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pf-xlights/fixtures");
    let dir = std::env::temp_dir();
    let show = dir.join(format!("pixelflow-xsq-show-{}.json", std::process::id()));
    let saved = dir.join(format!("pixelflow-xsq-{}.pfseq.json", std::process::id()));
    let imported = pixelflow(&[
        "xlights",
        fixtures.join("sample-show").to_str().unwrap(),
        "--save",
        show.to_str().unwrap(),
    ]);
    assert!(imported.status.success(), "{}", stdout(&imported));
    let output = pixelflow(&[
        "xlights-sequence",
        fixtures.join("sequences/effects.xsq").to_str().unwrap(),
        "--show",
        show.to_str().unwrap(),
        "--save",
        saved.to_str().unwrap(),
    ]);
    let text = stdout(&output);
    let json = std::fs::read_to_string(&saved).unwrap_or_default();
    std::fs::remove_file(&show).ok();
    std::fs::remove_file(&saved).ok();
    assert!(output.status.success(), "{text}");
    assert!(
        text.contains("Effects: 0:20.000 long, 8 rows, 22 effects (16 exact, 6 approximated, 0 placeholders"),
        "{text}"
    );
    assert!(
        text.contains("Text (1 effect) approximated: 'default' font shown in PixelFlow's pixel font."),
        "{text}"
    );
    assert!(text.contains("Saved"), "{text}");
    let doc: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(doc["name"], "Effects");
    assert_eq!(doc["rows"].as_array().unwrap().len(), 8);
}

#[test]
fn a_sequence_auto_maps_and_its_mapping_saves_as_an_xmap_that_imports_the_same() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pf-xlights/fixtures");
    let dir = std::env::temp_dir();
    let show = dir.join(format!("pixelflow-map-show-{}.json", std::process::id()));
    let xmap = dir.join(format!("pixelflow-map-{}.xmap", std::process::id()));
    let imported = pixelflow(&[
        "xlights",
        fixtures.join("sample-show").to_str().unwrap(),
        "--save",
        show.to_str().unwrap(),
    ]);
    assert!(imported.status.success(), "{}", stdout(&imported));
    let sequence = fixtures.join("sequences/effects.xsq");
    let mapped = pixelflow(&[
        "xlights-sequence",
        sequence.to_str().unwrap(),
        "--show",
        show.to_str().unwrap(),
        "--auto-map",
        "--save-xmap",
        xmap.to_str().unwrap(),
    ]);
    let again = pixelflow(&[
        "xlights-sequence",
        sequence.to_str().unwrap(),
        "--show",
        show.to_str().unwrap(),
        "--xmap",
        xmap.to_str().unwrap(),
    ]);
    let written = std::fs::read_to_string(&xmap).unwrap_or_default();
    std::fs::remove_file(&show).ok();
    std::fs::remove_file(&xmap).ok();
    let (text, text_again) = (stdout(&mapped), stdout(&again));
    assert!(mapped.status.success(), "{text}");
    assert!(again.status.success(), "{text_again}");
    assert!(
        text.contains("Mapped 8 of 11 items (82% of 29 effects)"),
        "{text}"
    );
    assert!(
        text.contains("Santa's Sleigh & Reindeer (model, 1 effect)"),
        "{text}"
    );
    assert!(text.contains("[exact 1.00]"), "{text}");
    assert!(written.starts_with("false\n"), "{written}");
    // The same effects come in either way.
    assert!(
        text.contains("8 rows, 22 effects (16 exact, 6 approximated"),
        "{text}"
    );
    assert!(
        text_again.contains("8 rows, 22 effects (16 exact, 6 approximated"),
        "{text_again}"
    );
}

#[test]
fn an_xlights_import_saved_with_save_opens_again() {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pf-xlights/fixtures/sample-show");
    let path = std::env::temp_dir().join(format!("pixelflow-xlights-{}.json", std::process::id()));
    let output = pixelflow(&[
        "xlights",
        folder.to_str().unwrap(),
        "--save",
        path.to_str().unwrap(),
    ]);
    let text = stdout(&output);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("Saved"), "{text}");
    let validated = pixelflow(&["validate", path.to_str().unwrap()]);
    std::fs::remove_file(&path).ok();
    assert!(
        stdout(&validated).contains("sample-show"),
        "{}",
        stdout(&validated)
    );
}

#[test]
fn a_closed_output_pipe_is_a_clean_exit() {
    // `pixelflow map show | head -1`: the reader goes away before anything is printed.
    for args in [vec!["map"], vec!["map", "--json"], vec!["validate"]] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_pixelflow"))
            .args(&args)
            .arg(demo_show())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("failed to run pixelflow");
        drop(child.stdout.take());
        let output = child.wait_with_output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(stderr, "", "{args:?}");
        assert_eq!(output.status.code(), Some(0), "{args:?}");
    }
}

#[test]
fn malformed_json_exits_2_with_a_message() {
    let path = std::env::temp_dir().join(format!("pixelflow-malformed-{}.json", std::process::id()));
    std::fs::write(&path, "{ \"schemaVersion\": 11, ").unwrap();
    for command in ["validate", "map"] {
        let output = pixelflow(&[command, path.to_str().unwrap()]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{command}: {stderr}");
        assert!(stderr.contains("could not load"), "{command}: {stderr}");
        assert!(output.stdout.is_empty(), "{command}");
    }
    std::fs::remove_file(&path).ok();
}

/// The demo show with `edit` applied, saved to a temporary file named after `tag`.
fn edited_demo(tag: &str, edit: impl FnOnce(&mut serde_json::Value)) -> PathBuf {
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(demo_show()).unwrap()).unwrap();
    edit(&mut doc);
    let path = std::env::temp_dir().join(format!("pixelflow-{tag}-{}.json", std::process::id()));
    std::fs::write(&path, doc.to_string()).unwrap();
    path
}

#[test]
fn map_json_exits_0_even_when_the_show_has_errors() {
    // `map` describes the wiring; `validate` is the command that fails on errors.
    let path = edited_demo("map-errors", |doc| {
        doc["controllers"][0]["ports"][0]["brightness"] = 200.into();
    });
    let mapped = pixelflow(&["map", "--json", path.to_str().unwrap()]);
    let validated = pixelflow(&["validate", path.to_str().unwrap()]);
    std::fs::remove_file(&path).ok();
    assert_eq!(mapped.status.code(), Some(0));
    let map: serde_json::Value = serde_json::from_slice(&mapped.stdout).unwrap();
    assert!(map["frameLen"].as_u64().unwrap() > 0);
    assert_eq!(validated.status.code(), Some(1));
}

#[test]
fn validate_lists_errors_before_warnings() {
    // The model's checks run first and find a warning (a prop with no pixels); the wiring
    // checks find an error after it (a prop wired twice). The error is printed first.
    let path = edited_demo("order", |doc| {
        let mut empty = doc["props"][0].clone();
        empty["id"] = "11111111-0000-4000-8000-0000000000ee".into();
        empty["name"] = "Empty".into();
        empty["shape"]["nodes"] = 0.into();
        doc["props"].as_array_mut().unwrap().push(empty);
        let twice = doc["controllers"][0]["ports"][0]["slots"][0].clone();
        doc["controllers"][1]["ports"][0]["slots"]
            .as_array_mut()
            .unwrap()
            .push(twice);
    });
    let output = pixelflow(&["validate", path.to_str().unwrap()]);
    std::fs::remove_file(&path).ok();
    let text = stdout(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    let error = text.find("  error").expect(&text);
    let warning = text.find("  warning").expect(&text);
    assert!(error < warning, "{text}");
}
