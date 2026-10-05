//! Size limits on sequence files, so a damaged or hostile file can't exhaust memory or time.

use crate::Sequence;

/// Largest sequence file PixelFlow will read, in bytes.
pub const MAX_SEQUENCE_BYTES: usize = 64 * 1024 * 1024;
/// Longest sequence: 4 hours.
pub const MAX_DURATION_MS: u64 = 4 * 60 * 60 * 1000;
/// Shortest time between frames (100 frames per second).
pub const MIN_FRAME_MS: u32 = 10;
/// Longest time between frames (what an `.fseq` file can store).
pub const MAX_FRAME_MS: u32 = 255;
/// Effects in the whole sequence.
pub const MAX_EFFECTS: usize = 200_000;
/// Rows in the sequence.
pub const MAX_ROWS: usize = 10_000;
/// Layers on one row.
pub const MAX_LAYERS_PER_ROW: usize = 100;
/// Timing tracks in the sequence.
pub const MAX_TIMING_TRACKS: usize = 1_000;
/// Timing marks across all tracks.
pub const MAX_MARKS: usize = 500_000;
/// Colors in one effect's palette.
pub const MAX_PALETTE_COLORS: usize = 32;
/// Characters in a name, label, or file path.
pub const MAX_TEXT_LEN: usize = 4_096;

/// Every size limit the sequence exceeds, in plain language (empty when it fits).
pub fn limit_problems(seq: &Sequence) -> Vec<String> {
    let mut problems = Vec::new();
    if seq.duration_ms > MAX_DURATION_MS {
        problems.push(format!(
            "The sequence is {} long; PixelFlow sequences can be at most 4 hours.",
            crate::format_ms(seq.duration_ms)
        ));
    }
    if !(MIN_FRAME_MS..=MAX_FRAME_MS).contains(&seq.frame_ms) {
        problems.push(format!(
            "The sequence's frames are {} ms apart; use {MIN_FRAME_MS} to {MAX_FRAME_MS} ms (25 ms is 40 frames per second).",
            seq.frame_ms
        ));
    }
    if seq.rows.len() > MAX_ROWS {
        problems.push(format!(
            "The sequence has {} rows; at most {MAX_ROWS} are allowed.",
            seq.rows.len()
        ));
    }
    if let Some(row) = seq.rows.iter().find(|r| r.layers.len() > MAX_LAYERS_PER_ROW) {
        problems.push(format!(
            "A row has {} layers; at most {MAX_LAYERS_PER_ROW} are allowed per row.",
            row.layers.len()
        ));
    }
    let effects = seq.effect_count();
    if effects > MAX_EFFECTS {
        problems.push(format!(
            "The sequence has {effects} effects; at most {MAX_EFFECTS} are allowed."
        ));
    }
    if let Some(effect) = seq
        .effects()
        .find(|e| e.palette.colors.len() > MAX_PALETTE_COLORS)
    {
        problems.push(format!(
            "An effect's palette has {} colors; at most {MAX_PALETTE_COLORS} are allowed.",
            effect.palette.colors.len()
        ));
    }
    if seq.timing_tracks.len() > MAX_TIMING_TRACKS {
        problems.push(format!(
            "The sequence has {} timing tracks; at most {MAX_TIMING_TRACKS} are allowed.",
            seq.timing_tracks.len()
        ));
    }
    let marks: usize = seq.timing_tracks.iter().map(|t| t.marks.len()).sum();
    if marks > MAX_MARKS {
        problems.push(format!(
            "The sequence has {marks} timing marks; at most {MAX_MARKS} are allowed."
        ));
    }
    let too_long = |s: &str| s.chars().count() > MAX_TEXT_LEN;
    let texts = std::iter::once(seq.name.as_str())
        .chain(seq.audio.as_deref())
        .chain(seq.timing_tracks.iter().map(|t| t.name.as_str()))
        .chain(
            seq.timing_tracks
                .iter()
                .flat_map(|t| &t.marks)
                .map(|m| m.label.as_str()),
        );
    if texts.into_iter().any(too_long) {
        problems.push(format!(
            "A name, label, or file path in the sequence is longer than {MAX_TEXT_LEN} characters."
        ));
    }
    problems
}
