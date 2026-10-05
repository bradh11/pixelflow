//! The fixed and variable headers at the start of a sequence file.

use crate::error::{FseqError, corrupt};
use std::io::Read;

/// Largest frame PixelFlow will allocate (bytes of channel data per frame). Far above any real
/// show (10 million RGBW pixels is 40 MB), but stops a damaged header from asking for gigabytes.
pub const MAX_CHANNELS: u32 = 64 * 1024 * 1024;

/// How the frame data is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    Zstd,
    Zlib,
}

/// What a sequence file says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Format version (major, minor), e.g. (2, 2).
    pub version: (u8, u8),
    /// Channels in every frame (the full channel space; sparse files fill the gaps with zeros).
    pub channels: u32,
    pub frames: u32,
    /// Time between frames, in milliseconds (50 = 20 frames per second).
    pub step_ms: u32,
    pub compression: Compression,
    /// The audio file the sequence was made for (the `mf` header), if recorded.
    pub media: Option<String>,
    /// The program that wrote the file (the `sp` header), e.g. "xLights Macintosh 2024.19".
    pub producer: Option<String>,
}

impl Header {
    /// The sequence's running time in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        u64::from(self.frames) * u64::from(self.step_ms)
    }
}

/// A compressed block: frames `first_frame..` stored in `len` bytes starting at `offset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Block {
    pub first_frame: u32,
    pub offset: u64,
    pub len: u32,
}

/// Channels `start..start + count` of the channel space, stored contiguously in each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Range {
    pub start: u32,
    pub count: u32,
}

/// Everything needed to find a frame's data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layout {
    pub header: Header,
    /// Where frame data starts in the file.
    pub data_offset: u64,
    /// Compressed blocks in frame order (empty when uncompressed).
    pub blocks: Vec<Block>,
    /// Sparse ranges (empty means the whole channel space is stored).
    pub ranges: Vec<Range>,
}

impl Layout {
    /// Bytes stored per frame.
    pub fn frame_len(&self) -> usize {
        if self.ranges.is_empty() {
            self.header.channels as usize
        } else {
            self.ranges.iter().map(|r| r.count as usize).sum()
        }
    }
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u24_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], 0])
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

/// Reads the headers (everything before the frame data) from the start of a file.
pub(crate) fn read_layout(mut reader: impl Read) -> Result<Layout, FseqError> {
    let mut fixed = [0u8; 32];
    reader
        .read_exact(&mut fixed[..8])
        .map_err(|_| FseqError::NotFseq)?;
    if &fixed[..4] != b"PSEQ" {
        return Err(FseqError::NotFseq);
    }
    let data_offset = usize::from(u16_at(&fixed, 4));
    let (minor, major) = (fixed[6], fixed[7]);
    if !(1..=2).contains(&major) {
        return Err(FseqError::Unsupported(format!("format version {major}.{minor}")));
    }
    if data_offset < 28 {
        return Err(corrupt("the header is too short"));
    }
    // Everything before the frame data: fixed header, block index, sparse ranges, variable headers.
    let mut head = vec![0u8; data_offset];
    head[..8].copy_from_slice(&fixed[..8]);
    reader
        .read_exact(&mut head[8..])
        .map_err(|_| corrupt("the file ends inside its header"))?;

    let channels = u32_at(&head, 10);
    let frames = u32_at(&head, 14);
    let step_ms = u32::from(head[18]);
    if channels == 0 || channels > MAX_CHANNELS {
        return Err(corrupt(format!("it says each frame has {channels} channels")));
    }
    if step_ms == 0 {
        return Err(corrupt("it says frames are 0 ms apart"));
    }

    let (compression, blocks, ranges, variable_start) = if major == 1 {
        (
            Compression::None,
            Vec::new(),
            Vec::new(),
            usize::from(u16_at(&head, 8)),
        )
    } else {
        if data_offset < 32 {
            return Err(corrupt("the header is too short"));
        }
        let compression = match head[20] & 0x0F {
            0 => Compression::None,
            1 => Compression::Zstd,
            2 => Compression::Zlib,
            other => return Err(FseqError::Unsupported(format!("compression type {other}"))),
        };
        let block_count = usize::from(head[21]) | (usize::from(head[20] >> 4) << 8);
        let range_count = usize::from(head[22]);
        let table_end = 32 + block_count * 8 + range_count * 6;
        if table_end > data_offset {
            return Err(corrupt("its block list runs past the header"));
        }
        let mut blocks = Vec::new();
        let mut offset = data_offset as u64;
        let mut previous_first = None;
        for i in 0..block_count {
            let at = 32 + i * 8;
            let (first_frame, len) = (u32_at(&head, at), u32_at(&head, at + 4));
            if len == 0 {
                // xLights pads the index with empty entries.
                continue;
            }
            if previous_first.is_some_and(|p| first_frame <= p) || first_frame >= frames {
                return Err(corrupt("its blocks are out of order"));
            }
            previous_first = Some(first_frame);
            blocks.push(Block {
                first_frame,
                offset,
                len,
            });
            offset += u64::from(len);
        }
        if compression != Compression::None && blocks.first().is_none_or(|b| b.first_frame != 0) && frames > 0
        {
            return Err(corrupt("its first block doesn't start at frame 0"));
        }
        let mut ranges = Vec::new();
        for i in 0..range_count {
            let at = 32 + block_count * 8 + i * 6;
            let range = Range {
                start: u24_at(&head, at),
                count: u24_at(&head, at + 3),
            };
            if u64::from(range.start) + u64::from(range.count) > u64::from(channels) {
                return Err(corrupt("a channel range runs past the last channel"));
            }
            ranges.push(range);
        }
        // Ranges must be disjoint: the stored bytes per frame can't exceed the channel space.
        // (Overlapping ranges could otherwise claim gigabytes per frame.)
        let total: u64 = ranges.iter().map(|r| u64::from(r.count)).sum();
        let mut sorted = ranges.clone();
        sorted.sort_by_key(|r| r.start);
        let overlaps = sorted
            .windows(2)
            .any(|w| u64::from(w[0].start) + u64::from(w[0].count) > u64::from(w[1].start));
        if total > u64::from(channels) || overlaps {
            return Err(corrupt("its channel ranges overlap"));
        }
        (compression, blocks, ranges, usize::from(u16_at(&head, 8)))
    };

    let (media, producer) = variable_headers(&head, variable_start.max(28));
    Ok(Layout {
        header: Header {
            version: (major, minor),
            channels,
            frames,
            step_ms,
            compression,
            media,
            producer,
        },
        data_offset: data_offset as u64,
        blocks,
        ranges,
    })
}

/// Reads the `mf` (media file) and `sp` (producer) variable headers; ignores anything malformed.
fn variable_headers(head: &[u8], mut at: usize) -> (Option<String>, Option<String>) {
    let (mut media, mut producer) = (None, None);
    while at + 4 <= head.len() {
        let len = usize::from(u16_at(head, at));
        if len < 4 || at + len > head.len() {
            break;
        }
        let text = String::from_utf8_lossy(&head[at + 4..at + len])
            .trim_end_matches('\0')
            .trim()
            .to_string();
        match &head[at + 2..at + 4] {
            b"mf" if !text.is_empty() => media = Some(text),
            b"sp" if !text.is_empty() => producer = Some(text),
            _ => {}
        }
        at += len;
    }
    (media, producer)
}
