//! Import errors, written for people.

#[derive(Debug, thiserror::Error)]
pub enum XlightsError {
    #[error("Could not read {0}: {1}")]
    Read(String, std::io::Error),
    #[error("{0} isn't a valid xLights file: {1}")]
    BadFile(&'static str, String),
    /// A timing file (`.xtiming`, Audacity labels) that can't be read, and why.
    #[error("{0} isn't a timing file PixelFlow can read: {1}")]
    BadTimingFile(String, String),
    #[error("{0} doesn't look like an xLights show folder (no xlights_rgbeffects.xml).")]
    NotAShowFolder(String),
    /// A vendor package (zip or folder) that can't be read, and why.
    #[error("{0}")]
    Package(String),
}
