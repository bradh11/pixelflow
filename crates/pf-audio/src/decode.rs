//! Opening music files for decoding.

use crate::error::AudioError;
use rodio::Decoder;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Opens `path` for decoding. Built from the `File` itself, so the decoder knows the file's length
/// and can jump backwards (an MP3 or WAV read as a plain stream can only go forwards) and find
/// an M4A's index at the end of the file.
pub(crate) fn open_decoder(path: &Path) -> Result<Decoder<BufReader<File>>, AudioError> {
    let shown = path.display().to_string();
    let file = File::open(path).map_err(|source| AudioError::Open {
        path: shown.clone(),
        source,
    })?;
    Decoder::try_from(file).map_err(|e| AudioError::Decode {
        path: shown,
        reason: e.to_string(),
    })
}
