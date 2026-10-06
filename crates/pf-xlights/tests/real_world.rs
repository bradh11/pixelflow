//! Quirks of real xLights show folders, each reproduced by a small fixture of our own.

use pf_model::{Generator, ShapeSource};
use pf_xlights::import_folder;
use std::path::Path;

fn fixture(name: &str) -> pf_xlights::XlightsImport {
    import_folder(&Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn curved_poly_lines_import_curved_with_their_pixels_on_the_curves() {
    // Both lines used to import as measured points drawn as if their curves were straight.
    let imported = fixture("curved-poly-line-show");
    assert!(
        imported.notes.iter().all(|n| !n.contains("curved")),
        "{:#?}",
        imported.notes
    );
    let layout = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/curved-poly-line-show/xlights_rgbeffects.xml"),
    )
    .unwrap();
    let layout = pf_xlights::parse_layout(&layout).unwrap();
    for prop in &imported.show.props {
        let ShapeSource::Generator(Generator::PolyLine { segments, .. }) = &prop.shape else {
            panic!("{} isn't an editable poly line: {:?}", prop.name, prop.shape)
        };
        assert!(segments.iter().any(|s| s.curve.is_some()), "{}", prop.name);
        let model = layout.models.iter().find(|m| m.name == prop.name).unwrap();
        let xlights = pf_xlights::upright_positions(model);
        for (a, b) in pf_geometry::world_positions(prop).iter().zip(&xlights) {
            assert!(
                (a.x - b[0] * 0.01).abs() < 2e-3 && (a.y - b[1] * 0.01).abs() < 2e-3,
                "{}",
                prop.name
            );
        }
    }
    // The bike frame's first curve bulges up and left of its straight line: its middle pixel
    // (the 6th of 10) is well off the line from (-100, 0) to (0, 60).
    let frame = &imported.show.props[0];
    let p = pf_geometry::world_positions(frame)[5];
    let (x, y) = (p.x * 100.0 - 400.0, p.y * 100.0 - 300.0);
    assert!(y - (x + 100.0) * 0.6 > 10.0, "({x}, {y}) is on the straight line");
    // The garland hangs below its flat line.
    let garland = &imported.show.props[1];
    assert!(pf_geometry::world_positions(garland)[5].y < 5.9);
}

#[test]
fn files_xlights_wrote_with_an_html_doctype_import() {
    // Some xLights versions write `<!DOCTYPE html>` at the top of both files.
    let imported = fixture("doctype-show");
    assert_eq!(imported.summary.props, 1);
    assert_eq!(imported.summary.controllers, 1);
    assert_eq!(imported.summary.wired, 1);
}
