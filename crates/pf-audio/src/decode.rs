//! Opening music files for decoding.

use crate::error::AudioError;
use crate::progress::{CountedFile, ReadPosition};
use rodio::Decoder;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// A decoder reading a music file, keeping count of how far into the file it has read.
pub(crate) type FileDecoder = Decoder<BufReader<CountedFile>>;

/// Opens `path` for decoding, with where in the file the decoder is (for progress). Given the
/// file's length, so the decoder can jump backwards (an MP3 or WAV read as a plain stream can
/// only go forwards) and find an M4A's index at the end of the file.
pub(crate) fn open_decoder(path: &Path) -> Result<(FileDecoder, ReadPosition), AudioError> {
    let shown = path.display().to_string();
    let opened = File::open(path).and_then(CountedFile::new);
    let (file, position) = opened.map_err(|source| AudioError::Open {
        path: shown.clone(),
        source,
    })?;
    let len = file.len();
    let decoder = Decoder::builder()
        .with_data(BufReader::new(file))
        .with_byte_len(len)
        .with_seekable(true)
        .build()
        .map_err(|e| AudioError::Decode {
            path: shown,
            reason: e.to_string(),
        })?;
    Ok((decoder, position))
}
