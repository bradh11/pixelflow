//! Measures how an xLights sequence imported into PixelFlow looks next to the frames xLights
//! itself rendered.
//!
//! Fidelity is about the look: the right props lit, in the right colors, moving the same way at
//! the same speed, to the same rhythm. It isn't pixel-for-pixel agreement, and it isn't whether
//! PixelFlow builds an effect the way xLights does. This crate imports an xLights show folder
//! and a sequence (`.xsq`), renders the sequence with PixelFlow's renderer through its `.fseq`
//! export, and compares that with the `.fseq` xLights rendered from the same sequence on the
//! same layout. Everything is worked out from the two renders.
//!
//! - [`compare`] does the import, the render, and the measuring.
//! - [`look`] is the look score: coverage, brightness, color, pattern, motion, and rhythm, the
//!   composite of the six, and the verdict it earns.
//! - [`scorecard`] is what comes out: how much of the sequence matches, is close, is different,
//!   or has no equivalent; what to improve first and which part of its look is off; and how the
//!   importer's labels held up.
//! - [`labels`] holds the importer's table of expected looks against a scorecard.
//! - [`metrics`] is the pixel-for-pixel arithmetic the scorecard keeps as diagnostics: error,
//!   tolerance, lit agreement, brightness correlation, hue, and distributions.
//! - [`corpus`] runs a list of sequences from a manifest; [`report`] writes the summaries;
//!   [`strips`] draws frames side by side for looking into a difference.
//!
//! `docs/fidelity.md` has the method and its limits.

pub mod compare;
pub mod corpus;
pub mod labels;
pub mod layout;
pub mod look;
pub mod metrics;
pub mod report;
pub mod scorecard;
pub mod strips;

pub use compare::{Options, Prepared, Request, compare};
pub use scorecard::{CorpusCard, Scorecard};

use std::io;
use std::path::PathBuf;

/// The tool's name and version, as scorecards record it.
pub const TOOL: &str = concat!("pf-fidelity ", env!("CARGO_PKG_VERSION"));

#[derive(Debug, thiserror::Error)]
pub enum FidelityError {
    #[error("{0} isn't an xLights show folder (it has no xlights_rgbeffects.xml).")]
    NotAShowFolder(PathBuf),
    #[error("Could not read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("{0}")]
    Xlights(#[from] pf_xlights::XlightsError),
    #[error("Could not open the reference {path}: {source}")]
    Reference {
        path: PathBuf,
        source: pf_fseq::FseqError,
    },
    #[error("Could not read a frame of the reference: {0}")]
    ReferenceFrame(pf_fseq::FseqError),
    #[error("None of the layout's props have channels, so there is nothing to compare.")]
    NoChannels,
    #[error("PixelFlow could not render the sequence: {0}")]
    Render(String),
    #[error("{path}: {reason}")]
    Manifest { path: PathBuf, reason: String },
    #[error("Could not write {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
    #[error("{0}")]
    Strip(String),
}
