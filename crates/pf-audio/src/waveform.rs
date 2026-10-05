//! Loudness over time, for drawing a song under the timeline.

use crate::decode::open_decoder;
use crate::error::AudioError;
use rodio::Source;
use serde::Serialize;
use std::path::Path;

/// Samples per block when scanning (per channel-interleaved sample).
const BLOCK: usize = 1024;

/// A song's length and peak loudness in equal slices of time.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Waveform {
    pub duration_ms: u64,
    /// Peak level (0.0–1.0) in each slice, first to last.
    pub peaks: Vec<f32>,
}

/// Decodes the whole file at `path` and returns its peaks in `slices` equal slices.
pub fn waveform(path: &Path, slices: usize) -> Result<Waveform, AudioError> {
    let decoder = open_decoder(path)?;
    // The rate and channel count are read once: music files keep them for their whole length
    // (a file that changed mid-way would get a slightly wrong duration, nothing worse).
    let rate = u64::from(decoder.sample_rate().get());
    let channels = u64::from(decoder.channels().get());
    let mut blocks = Vec::new();
    let (mut peak, mut count, mut total) = (0.0f32, 0usize, 0u64);
    for sample in decoder {
        peak = peak.max(sample.abs());
        count += 1;
        total += 1;
        if count == BLOCK {
            blocks.push(peak);
            (peak, count) = (0.0, 0);
        }
    }
    if count > 0 {
        blocks.push(peak);
    }
    let duration_ms = total * 1000 / (rate * channels).max(1);
    let slices = slices.max(1);
    let peaks = (0..slices)
        .map(|i| {
            let from = i * blocks.len() / slices;
            let to = ((i + 1) * blocks.len() / slices).max(from + 1).min(blocks.len());
            blocks
                .get(from..to)
                .map_or(0.0, |b| b.iter().copied().fold(0.0, f32::max))
                .min(1.0)
        })
        .collect();
    Ok(Waveform { duration_ms, peaks })
}
