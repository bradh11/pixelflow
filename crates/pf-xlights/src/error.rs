//! Import errors, written for people.

#[derive(Debug, thiserror::Error)]
pub enum XlightsError {
    #[error("Could not read {0}: {1}")]
    Read(String, std::io::Error),
    #[error("{0} isn't a valid xLights file: {1}")]
    BadFile(&'static str, String),
    #[error("{0} doesn't look like an xLights show folder (no xlights_rgbeffects.xml).")]
    NotAShowFolder(String),
}
