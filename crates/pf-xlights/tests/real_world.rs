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

/// `body` after a document type declaration `doctype` (and an XML declaration).
fn with_doctype(doctype: &str, body: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{doctype}\n{body}")
}

/// A declaration that would expand to gigabytes if it were read, after a comment that hides a
/// bare doctype from a naive check.
fn expanding(root: &str) -> String {
    let big = "A".repeat(10_000);
    format!(
        "<!-- <!DOCTYPE z> --><!DOCTYPE {root} SYSTEM \"a>b\" [<!ENTITY a \"{big}\">]><{root} a=\"{}\"/>",
        "&a;".repeat(10_000)
    )
}

#[test]
fn every_xlights_file_reads_a_bare_doctype_and_refuses_any_other() {
    let layout = "<xrgb><models/><modelGroups/></xrgb>";
    let networks = "<Networks/>";
    let xsq = "<xsequence><head><version>2024.20</version><sequenceDuration>1.000</sequenceDuration></head></xsequence>";
    let xtiming = "<timing name=\"Beats\"><EffectLayer><Effect label=\"1\" starttime=\"0\" endtime=\"500\"/></EffectLayer></timing>";
    let refused = |e: &dyn std::fmt::Display| assert!(e.to_string().contains("document type"), "{e}");

    assert!(pf_xlights::parse_layout(&with_doctype("<!DOCTYPE html>", layout)).is_ok());
    refused(&pf_xlights::parse_layout(&expanding("xrgb")).unwrap_err());
    assert!(pf_xlights::parse_networks(&with_doctype("<!DOCTYPE html>", networks)).is_ok());
    refused(&pf_xlights::parse_networks(&expanding("Networks")).unwrap_err());
    assert!(pf_xlights::sequence::parse_xsq(&with_doctype("<!DOCTYPE html>", xsq)).is_ok());
    refused(&pf_xlights::sequence::parse_xsq(&expanding("xsequence")).unwrap_err());
    assert!(
        pf_xlights::parse_xtiming(&with_doctype("<!DOCTYPE html>", xtiming), 1000, "beats.xtiming").is_ok()
    );
    refused(&pf_xlights::parse_xtiming(&expanding("timing"), 1000, "beats.xtiming").unwrap_err());
}

#[test]
fn files_xlights_wrote_with_an_html_doctype_import() {
    // Some xLights versions write `<!DOCTYPE html>` at the top of both files.
    let imported = fixture("doctype-show");
    assert_eq!(imported.summary.props, 1);
    assert_eq!(imported.summary.controllers, 1);
    assert_eq!(imported.summary.wired, 1);
}
