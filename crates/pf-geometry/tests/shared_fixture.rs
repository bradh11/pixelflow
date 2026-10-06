//! The shapes in `fixtures/shapes.json` and where pf-geometry puts their pixels. The app's
//! TypeScript mirror (`app/src/lib/geometry.ts`) checks itself against the same file, so the
//! layout editor and the engine always agree.
//!
//! To add a case, add its `name` and `shape` with `"positions": []` and run this test with
//! `PF_UPDATE_FIXTURES=1`; check the new positions by hand before committing them.

use pf_geometry::local_positions;
use pf_model::ShapeSource;
use serde_json::{Value, json};
use std::path::Path;

#[test]
fn shapes_put_their_pixels_where_the_shared_fixture_says() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shapes.json");
    let mut cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let update = std::env::var_os("PF_UPDATE_FIXTURES").is_some();
    for case in &mut cases {
        let name = case["name"].as_str().unwrap().to_string();
        let shape: ShapeSource =
            serde_json::from_value(case["shape"].clone()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let ours: Vec<[f32; 3]> = local_positions(&shape).iter().map(|p| [p.x, p.y, p.z]).collect();
        if update {
            // Rounded so the file stays readable; both sides compare within 1e-4.
            let r = |v: f32| (f64::from(v) * 1e6).round() / 1e6;
            case["positions"] = json!(
                ours.iter()
                    .map(|p| [r(p[0]), r(p[1]), r(p[2])])
                    .collect::<Vec<_>>()
            );
            continue;
        }
        let want: Vec<[f32; 3]> = serde_json::from_value(case["positions"].clone()).unwrap();
        assert_eq!(ours.len(), want.len(), "{name}");
        for (i, (a, b)) in ours.iter().zip(&want).enumerate() {
            let off = (0..3).map(|k| (a[k] - b[k]).abs()).fold(0.0, f32::max);
            assert!(off < 1e-4, "{name} pixel {i}: {a:?} vs {b:?}");
        }
    }
    if update {
        // One line per case keeps the file short enough to read and diff.
        let lines: Vec<String> = cases.iter().map(|c| serde_json::to_string(c).unwrap()).collect();
        std::fs::write(&path, format!("[\n  {}\n]\n", lines.join(",\n  "))).unwrap();
    }
}
