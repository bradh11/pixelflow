//! One-way import of an xLights show folder into a PixelFlow show.
//!
//! Reads `xlights_networks.xml` (controllers and their channel ranges) and
//! `xlights_rgbeffects.xml` (props, their exact pixel positions, and how they're wired), and
//! builds a show whose channel layout matches xLights', so sequences rendered by xLights play on
//! the right pixels. Nothing is guessed silently: anything approximated or skipped is reported.
//!
//! Sequences (`.xsq`) import too ([`sequence`]): as editable PixelFlow sequences on that show,
//! with each xLights effect translated to the closest PixelFlow effect and a report of what
//! didn't come across exactly.

mod channels;
mod error;
mod geometry;
mod import;
mod layout;
mod model;
mod networks;
pub mod sequence;

pub use channels::{ChannelRequest, Resolved, resolve};
pub use error::XlightsError;
pub use geometry::{Geometry, XNode, geometry};
pub use import::{ImportSummary, XlightsImport, build_show};
pub use layout::{XGroup, XLayout, parse_layout};
pub use model::XmlModel;
pub use networks::{XController, XOutput, parse_networks};
pub use sequence::{SequenceImport, SequenceImportSummary, build_sequence, import_sequence_file};

use std::path::Path;

/// Imports the xLights show in `dir` (its `xlights_rgbeffects.xml`, plus `xlights_networks.xml`
/// when present). The show is named after the folder.
pub fn import_folder(dir: &Path) -> Result<XlightsImport, XlightsError> {
    let layout_path = dir.join("xlights_rgbeffects.xml");
    if !layout_path.is_file() {
        return Err(XlightsError::NotAShowFolder(dir.display().to_string()));
    }
    let read = |path: &Path| {
        std::fs::read_to_string(path).map_err(|e| XlightsError::Read(path.display().to_string(), e))
    };
    let layout = parse_layout(&read(&layout_path)?)?;
    let networks_path = dir.join("xlights_networks.xml");
    let (controllers, missing_networks) = if networks_path.is_file() {
        (parse_networks(&read(&networks_path)?)?, false)
    } else {
        (Vec::new(), true)
    };
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "xLights Show".to_string());
    let mut result = build_show(&name, &controllers, &layout, geometry);
    if missing_networks {
        result.notes.insert(
            0,
            "There's no xlights_networks.xml in this folder, so no controllers were imported.".to_string(),
        );
    }
    Ok(result)
}
