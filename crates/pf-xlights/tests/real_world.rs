//! Quirks of real xLights show folders, each reproduced by a small fixture of our own.

use pf_mapping::ChannelAddress;
use pf_model::{Generator, Protocol, ShapeSource};
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
fn models_with_odd_layer_lists_import_measured_quickly_and_the_show_opens() {
    // Each model's exact shape would hold a layer bigger than a prop may be: imported as shapes,
    // the whole show couldn't be opened. One layer is written as nearly 2^63.
    let started = std::time::Instant::now();
    let imported = fixture("odd-layers-show");
    assert!(started.elapsed().as_secs_f32() < 1.0, "{:?}", started.elapsed());
    assert_eq!(imported.summary.props, 4);
    for prop in &imported.show.props {
        assert!(
            matches!(prop.shape, ShapeSource::Measured { .. }),
            "{}",
            prop.name
        );
    }
    pf_model::check_show(&imported.show).expect("the imported show opens");
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

#[test]
fn controllers_with_odd_universe_sizes_import_with_their_wiring() {
    // Universes of 15, 270 and 426 channels, with start channels given as a universe ("#2:13",
    // "#192.0.2.63:20:424"), a controller ("!Mid:265") and the end of another prop (">Fence:1").
    // Each of the first three props crosses from one universe into the next.
    let imported = fixture("odd-universe-show");
    assert!(
        imported
            .notes
            .iter()
            .all(|n| !n.contains("per universe") && !n.contains("wired") && !n.contains("aren't on any")),
        "{:#?}",
        imported.notes
    );
    assert_eq!((imported.summary.controllers, imported.summary.wired), (3, 4));
    let show = &imported.show;
    let sizes: Vec<_> = show
        .controllers
        .iter()
        .map(|c| match c.protocol {
            Protocol::Sacn(cfg) => (c.name.as_str(), cfg.start_universe, cfg.universe_size.channels()),
            Protocol::Ddp => panic!("{} is sACN", c.name),
        })
        .collect();
    assert_eq!(
        sizes,
        [
            ("Tiny", Some(1), 15),
            ("Mid", Some(10), 270),
            ("Wide", Some(20), 426)
        ]
    );

    let (map, report) = pf_mapping::map_show(show);
    assert!(!report.has_errors(), "{report:?}");
    let addresses = |name: &str| -> Vec<(String, u16, u16)> {
        let prop = show.props.iter().find(|p| p.name == name).unwrap();
        (0..prop.node_count())
            .map(|node| {
                let found = map.locate(prop.id, node);
                assert_eq!(found.len(), 1, "{name} node {node}");
                let controller = show
                    .controllers
                    .iter()
                    .find(|c| c.id == found[0].controller)
                    .unwrap();
                let ChannelAddress::Sacn { universe, channel } = found[0].address else {
                    panic!("sACN")
                };
                (controller.name.clone(), universe, channel)
            })
            .collect()
    };
    let at = |c: &str, list: &[(u16, u16)]| -> Vec<(String, u16, u16)> {
        list.iter().map(|&(u, ch)| (c.to_string(), u, ch)).collect()
    };
    assert_eq!(addresses("Window"), at("Tiny", &[(2, 13), (3, 1)]));
    assert_eq!(
        addresses("Gutter"),
        at("Mid", &[(10, 265), (10, 268), (11, 1), (11, 4)])
    );
    assert_eq!(addresses("Fence"), at("Wide", &[(20, 424), (21, 1), (21, 4)]));
    assert_eq!(addresses("Post"), at("Wide", &[(21, 7), (21, 10)]));
}
