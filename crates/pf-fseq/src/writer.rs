//! Writing version 2.2 sequence files (the version xLights writes and FPP reads): zstd-compressed
//! blocks of frames, no sparse ranges. Version 2.2 is the one whose block count has 12 bits; a 2.0
//! reader would only see the low 8 (255 blocks, about a minute at 25 ms and 10 frames per block).

use crate::error::FseqError;
use crate::header::MAX_CHANNELS;
use std::io::{self, Seek, SeekFrom, Write};

/// Output errors are write errors (the shared `Io` variant talks about reading).
fn cant(reason: impl Into<String>) -> FseqError {
    FseqError::CantWrite(reason.into())
}

fn w<T>(result: io::Result<T>) -> Result<T, FseqError> {
    result.map_err(FseqError::Write)
}

/// Most blocks a version 2 header can list (the count is 12 bits).
const MAX_BLOCKS: u32 = 4095;
/// Largest block the writer makes before compression. Readers (PixelFlow's included) refuse
/// blocks over 256 MB; FPP is happiest with small ones.
const MAX_BLOCK_BYTES: u64 = 64 * 1024 * 1024;
/// Longest text kept in a variable header (the media file name, the producer).
const MAX_HEADER_TEXT: usize = 1024;

/// What to write: the frame layout and the file's descriptive headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOptions {
    /// Channels in every frame.
    pub channels: u32,
    /// Frames the file will hold (exactly this many must be written).
    pub frames: u32,
    /// Time between frames, 1–255 ms.
    pub step_ms: u32,
    /// The music file name (the `mf` header), so FPP plays the right song.
    pub media: Option<String>,
    /// The program writing the file (the `sp` header).
    pub producer: Option<String>,
    /// Frames per compressed block (raised automatically for long sequences, since a file lists
    /// at most 4095 blocks, and lowered for huge frames).
    pub frames_per_block: u32,
    /// zstd level (1 = fastest).
    pub compression_level: i32,
    /// The header's unique id (FPP uses it to tell files apart); 0 is fine.
    pub unique_id: u64,
}

impl WriteOptions {
    /// Options for `frames` frames of `channels` channels, `step_ms` apart, 10 frames per block.
    pub fn new(channels: u32, frames: u32, step_ms: u32) -> Self {
        Self {
            channels,
            frames,
            step_ms,
            media: None,
            producer: None,
            frames_per_block: 10,
            compression_level: 1,
            unique_id: 0,
        }
    }
}

/// Writes a sequence frame by frame. Frames are compressed a block at a time, so memory stays
/// small; the header (with the block list) is filled in by [`FseqWriter::finish`].
pub struct FseqWriter<W: Write + Seek> {
    out: W,
    options: WriteOptions,
    start: u64,
    frames_per_block: u32,
    header_len: usize,
    blocks: Vec<(u32, u32)>,
    pending: Vec<u8>,
    pending_frames: u32,
    written: u32,
}

/// A variable header: length (including these 4 bytes), 2-letter code, NUL-terminated text.
fn variable(code: &[u8; 2], text: &str) -> Vec<u8> {
    let mut end = text.len().min(MAX_HEADER_TEXT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let text = &text.as_bytes()[..end];
    let mut out = ((text.len() + 5) as u16).to_le_bytes().to_vec();
    out.extend_from_slice(code);
    out.extend_from_slice(text);
    out.push(0);
    out
}

impl<W: Write + Seek> FseqWriter<W> {
    /// Starts a file at the current position of `out`.
    pub fn new(mut out: W, options: WriteOptions) -> Result<Self, FseqError> {
        if options.channels == 0 || options.channels > MAX_CHANNELS {
            return Err(cant(format!(
                "it has {} channels per frame (1 to {MAX_CHANNELS} are possible)",
                options.channels
            )));
        }
        if !(1..=255).contains(&options.step_ms) {
            return Err(cant(format!(
                "its frames are {} ms apart (1 to 255 ms are possible)",
                options.step_ms
            )));
        }
        if options.frames == 0 {
            return Err(cant("it has no frames"));
        }
        let by_size = (MAX_BLOCK_BYTES / u64::from(options.channels)).max(1) as u32;
        let mut frames_per_block = options.frames_per_block.clamp(1, by_size);
        if options.frames.div_ceil(frames_per_block) > MAX_BLOCKS {
            frames_per_block = options.frames.div_ceil(MAX_BLOCKS);
        }
        if u64::from(frames_per_block) * u64::from(options.channels) > 4 * MAX_BLOCK_BYTES {
            return Err(cant(
                "it is too long for frames this large (it would need blocks over 256 MB)",
            ));
        }
        let block_count = options.frames.div_ceil(frames_per_block) as usize;
        let vars = Self::variables(&options).len();
        // Padded to a multiple of 4 bytes like xLights (readers stop at the zero padding).
        let header_len = (32 + block_count * 8 + vars).next_multiple_of(4);
        if header_len > usize::from(u16::MAX) {
            return Err(cant("its header would be longer than 64 KB"));
        }
        let start = w(out.stream_position())?;
        w(out.write_all(&vec![0u8; header_len]))?;
        Ok(Self {
            out,
            frames_per_block,
            header_len,
            blocks: Vec::with_capacity(block_count),
            pending: Vec::with_capacity(frames_per_block as usize * options.channels as usize),
            pending_frames: 0,
            written: 0,
            start,
            options,
        })
    }

    fn variables(options: &WriteOptions) -> Vec<u8> {
        let mut vars = Vec::new();
        if let Some(media) = options.media.as_deref().filter(|m| !m.is_empty()) {
            vars.extend(variable(b"mf", media));
        }
        if let Some(producer) = options.producer.as_deref().filter(|p| !p.is_empty()) {
            vars.extend(variable(b"sp", producer));
        }
        vars
    }

    /// Frames per block actually used.
    pub fn frames_per_block(&self) -> u32 {
        self.frames_per_block
    }

    /// Adds the next frame (exactly `channels` bytes).
    pub fn write_frame(&mut self, frame: &[u8]) -> Result<(), FseqError> {
        if frame.len() != self.options.channels as usize {
            return Err(cant(format!(
                "a frame has {} channels, not {}",
                frame.len(),
                self.options.channels
            )));
        }
        if self.written >= self.options.frames {
            return Err(cant(format!(
                "more than the {} frames it was started with were written",
                self.options.frames
            )));
        }
        self.pending.extend_from_slice(frame);
        self.pending_frames += 1;
        self.written += 1;
        if self.pending_frames == self.frames_per_block {
            self.flush_block()?;
        }
        Ok(())
    }

    fn flush_block(&mut self) -> Result<(), FseqError> {
        if self.pending_frames == 0 {
            return Ok(());
        }
        let packed = w(zstd::bulk::compress(
            &self.pending,
            self.options.compression_level,
        ))?;
        let len = u32::try_from(packed.len()).map_err(|_| cant("a compressed block is over 4 GB"))?;
        w(self.out.write_all(&packed))?;
        let first = self.written - self.pending_frames;
        self.blocks.push((first, len));
        self.pending.clear();
        self.pending_frames = 0;
        Ok(())
    }

    /// Writes the last block and the header, and hands back the output.
    pub fn finish(mut self) -> Result<W, FseqError> {
        if self.written != self.options.frames {
            return Err(cant(format!(
                "only {} of its {} frames were written",
                self.written, self.options.frames
            )));
        }
        self.flush_block()?;
        let header = self.header();
        debug_assert_eq!(header.len(), self.header_len);
        let end = w(self.out.stream_position())?;
        w(self.out.seek(SeekFrom::Start(self.start)))?;
        w(self.out.write_all(&header))?;
        w(self.out.seek(SeekFrom::Start(end)))?;
        w(self.out.flush())?;
        Ok(self.out)
    }

    fn header(&self) -> Vec<u8> {
        let o = &self.options;
        let block_count = self.blocks.len();
        let variable_start = 32 + block_count * 8;
        let mut out = Vec::with_capacity(self.header_len);
        out.extend_from_slice(b"PSEQ");
        out.extend_from_slice(&(self.header_len as u16).to_le_bytes());
        out.extend_from_slice(&[2, 2]); // version 2.2 (minor, major): 12-bit block count
        out.extend_from_slice(&(variable_start as u16).to_le_bytes());
        out.extend_from_slice(&o.channels.to_le_bytes());
        out.extend_from_slice(&o.frames.to_le_bytes());
        out.push(o.step_ms as u8);
        out.push(0); // flags
        out.push(1 | (((block_count >> 8) as u8 & 0x0F) << 4)); // zstd + high bits of block count
        out.push(block_count as u8);
        out.push(0); // no sparse ranges
        out.push(0); // flags
        out.extend_from_slice(&o.unique_id.to_le_bytes());
        for &(first, len) in &self.blocks {
            out.extend_from_slice(&first.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
        }
        out.extend(Self::variables(o));
        out.resize(self.header_len, 0);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_headers_are_nul_terminated_and_capped() {
        assert_eq!(variable(b"mf", "a.mp3"), b"\x0a\x00mfa.mp3\x00".to_vec());
        let long = "é".repeat(600); // 1200 bytes; cut at a character boundary
        let v = variable(b"mf", &long);
        assert_eq!(v.len(), 4 + 1024 + 1);
    }
}
