//! Measures how closely PixelFlow renders an imported xLights sequence, against the frames
//! xLights itself rendered.
//!
//! The importer labels each effect exact, approximate, or placeholder by its own judgement. This
//! crate checks the result: it imports an xLights show folder and a sequence (`.xsq`), renders
//! the sequence with PixelFlow's renderer through its `.fseq` export, and compares that channel
//! by channel with the `.fseq` xLights rendered from the same sequence on the same layout.
//!
//! - [`compare`] does the import, the render, and the measuring.
//! - [`metrics`] is the arithmetic: error, tolerance, lit agreement, brightness correlation, hue,
//!   and distributions for effects that place things at random.
//! - [`scorecard`] is what comes out: scores per prop, per effect, and per kind of effect, the
//!   ranking of what to fix first, and the audit of the importer's labels.
//! - [`corpus`] runs a list of sequences from a manifest; [`report`] writes the summaries;
//!   [`strips`] draws frames side by side for looking into a difference.
//!
//! `docs/fidelity.md` has the method and its limits.

pub mod compare;
pub mod corpus;
pub mod layout;
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
