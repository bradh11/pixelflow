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
fn validate_reports_wiring_errors_and_exits_1() {
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(demo_show()).unwrap()).unwrap();
    doc["controllers"][0]["ports"][0]["maxPixels"] = 100.into();
    let path = std::env::temp_dir().join(format!("pixelflow-overloaded-{}.json", std::process::id()));
    std::fs::write(&path, doc.to_string()).unwrap();

    let output = pixelflow(&["validate", path.to_str().unwrap()]);
    let text = stdout(&output);
    std::fs::remove_file(&path).ok();
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("1 error, 0 warnings"), "{text}");
    assert!(
        text.contains("Port 1 on 'Main FPP' is over capacity by 151 pixels (251 of 100)."),
        "{text}"
    );
    assert!(text.contains("Fix: Move a prop to another port"), "{text}");
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
