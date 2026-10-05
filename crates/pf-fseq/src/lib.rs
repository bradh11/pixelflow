//! Reading and writing FPP sequence (`.fseq`) files: the rendered channel data for every frame
//! of a show, as written by xLights and played by FPP.
//!
//! Supports format versions 1 (uncompressed) and 2 (uncompressed, zstd, or zlib blocks, with
//! optional sparse channel ranges). Frames are decompressed one block at a time, so memory
//! stays small even for long shows. [`FseqWriter`] writes version 2.2 files with zstd blocks.

mod error;
mod header;
mod sequence;
mod writer;

pub use error::FseqError;
pub use header::{Compression, Header};
pub use sequence::Sequence;
pub use writer::{FseqWriter, WriteOptions};
