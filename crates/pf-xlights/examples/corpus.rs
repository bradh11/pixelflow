//! Imports xLights show folders and reports how each came across: how many props are editable
//! shapes and how many keep measured points (by xLights model type), how long the import took,
//! and the import's notes. For checking the importer against real shows:
//!
//! ```text
//! cargo run --release -p pf-xlights --example corpus -- <show folder>... [--notes] [--list]
//! ```
//!
//! `--notes` prints the import's notes; `--list` names the props that kept measured points.
//! ```text
//! ```

use pf_model::ShapeSource;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let show_notes = args.iter().any(|a| a == "--notes");
    let list = args.iter().any(|a| a == "--list");
    let (mut all_shapes, mut all_props) = (0usize, 0usize);
    for dir in args.iter().filter(|a| !a.starts_with("--")) {
        let dir = Path::new(dir);
        let started = Instant::now();
        let result = std::panic::catch_unwind(|| pf_xlights::import_folder(dir));
        let took = started.elapsed();
        let name = dir
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let imported = match result {
            Err(_) => {
                println!("## {name}: PANICKED after {took:.2?}");
                continue;
            }
            Ok(Err(e)) => {
                println!("## {name}: failed: {e}");
                continue;
            }
            Ok(Ok(i)) => i,
        };
        // The xLights type of each model, to group the props by it.
        let layout = std::fs::read_to_string(dir.join("xlights_rgbeffects.xml"))
            .ok()
            .and_then(|s| pf_xlights::parse_layout(&s).ok())
            .unwrap_or_default();
        let types: HashMap<&str, &str> = layout
            .models
            .iter()
            .map(|m| (m.name.as_str(), m.display_as.as_str()))
            .collect();
        let mut by_type: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        let mut measured: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        let mut shapes = 0;
        for prop in &imported.show.props {
            let t = types.get(prop.name.as_str()).copied().unwrap_or("?");
            let t = if t.starts_with("Dmx") { "Dmx*" } else { t };
            let e = by_type.entry(t.to_string()).or_default();
            if matches!(prop.shape, ShapeSource::Generator(_)) {
                e.0 += 1;
                shapes += 1;
            } else {
                e.1 += 1;
                measured.entry(t.to_string()).or_default().push(&prop.name);
            }
        }
        let props = imported.show.props.len();
        all_shapes += shapes;
        all_props += props;
        let check = pf_model::check_show(&imported.show)
            .map(|_| ())
            .map_err(|e| e.to_string());
        println!(
            "## {name}: {props} props, {shapes} editable shapes ({:.0}%), {} pixels, {} notes, {took:.2?}{}",
            percent(shapes, props),
            imported.summary.pixels,
            imported.notes.len(),
            match check {
                Ok(()) => String::new(),
                Err(e) => format!(", DOES NOT SAVE: {e}"),
            }
        );
        for (t, (s, m)) in &by_type {
            println!("  {t:<16} {s:>4} shapes {m:>4} measured");
            if list && let Some(names) = measured.get(t) {
                println!("      {}", names.join(", "));
            }
        }
        if show_notes {
            for note in &imported.notes {
                println!("  - {note}");
            }
        }
    }
    println!(
        "## total: {all_props} props, {all_shapes} editable shapes ({:.0}%)",
        percent(all_shapes, all_props)
    );
}

fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}
