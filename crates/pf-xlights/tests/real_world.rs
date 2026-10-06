//! Quirks of real xLights show folders, each reproduced by a small fixture of our own.

use pf_xlights::import_folder;
use std::path::Path;

fn fixture(name: &str) -> pf_xlights::XlightsImport {
    import_folder(&Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn files_xlights_wrote_with_an_html_doctype_import() {
    // Some xLights versions write `<!DOCTYPE html>` at the top of both files.
    let imported = fixture("doctype-show");
    assert_eq!(imported.summary.props, 1);
    assert_eq!(imported.summary.controllers, 1);
    assert_eq!(imported.summary.wired, 1);
}
