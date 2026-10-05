//! Random access to a sequence's frames.

use crate::error::{FseqError, corrupt};
use crate::header::{Compression, Header, Layout, read_layout};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// An open sequence. Reading a frame decompresses (and keeps) only the block that holds it, so
/// playing frames in order decompresses each block once.
pub struct Sequence<R = BufReader<File>> {
    reader: R,
    layout: Layout,
    /// The most recently decompressed block: (index into `layout.blocks`, its frame data).
    cached: Option<(usize, Vec<u8>)>,
    /// Reused buffer for uncompressed frames.
    scratch: Vec<u8>,
}

impl Sequence {
    /// Opens a sequence file and reads its headers.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, FseqError> {
        Self::from_reader(BufReader::new(File::open(path)?))
    }
}

impl<R: Read + Seek> Sequence<R> {
    /// Reads a sequence from any seekable source (a file, or bytes in memory).
    pub fn from_reader(mut reader: R) -> Result<Self, FseqError> {
        reader.seek(SeekFrom::Start(0))?;
        let layout = read_layout(&mut reader)?;
        Ok(Self {
            reader,
            layout,
            cached: None,
            scratch: Vec::new(),
        })
    }

    pub fn header(&self) -> &Header {
        &self.layout.header
    }

    /// Fills `out` (exactly `header().channels` bytes) with frame `frame`'s channel data.
    /// Channels outside a sparse file's ranges are set to 0.
    pub fn read_frame(&mut self, frame: u32, out: &mut [u8]) -> Result<(), FseqError> {
        let header = &self.layout.header;
        if out.len() != header.channels as usize {
            return Err(corrupt(format!(
                "a frame has {} channels, not {}",
                header.channels,
                out.len()
            )));
        }
        if frame >= header.frames {
            return Err(corrupt(format!(
                "there is no frame {frame} (it has {})",
                header.frames
            )));
        }
        let frame_len = self.layout.frame_len();
        let bytes: &[u8] = if header.compression == Compression::None {
            let offset = self.layout.data_offset + u64::from(frame) * frame_len as u64;
            self.scratch.resize(frame_len, 0);
            self.reader.seek(SeekFrom::Start(offset))?;
            self.reader
                .read_exact(&mut self.scratch)
                .map_err(|_| corrupt("the file ends before its last frame"))?;
            &self.scratch
        } else {
            let index = self.block_for(frame);
            if self.cached.as_ref().is_none_or(|(i, _)| *i != index) {
                let data = self.decompress(index)?;
                self.cached = Some((index, data));
            }
            let start = (frame - self.layout.blocks[index].first_frame) as usize * frame_len;
            let (_, data) = self.cached.as_ref().expect("cached above");
            data.get(start..start + frame_len)
                .ok_or_else(|| corrupt(format!("frame {frame} is missing from its block")))?
        };
        if self.layout.ranges.is_empty() {
            out.copy_from_slice(bytes);
        } else {
            out.fill(0);
            let mut at = 0;
            for range in &self.layout.ranges {
                let (start, count) = (range.start as usize, range.count as usize);
                out[start..start + count].copy_from_slice(&bytes[at..at + count]);
                at += count;
            }
        }
        Ok(())
    }

    /// The block holding `frame` (the last block starting at or before it).
    fn block_for(&self, frame: u32) -> usize {
        self.layout.blocks.partition_point(|b| b.first_frame <= frame) - 1
    }

    /// Frames stored in block `index`.
    fn frames_in(&self, index: usize) -> u32 {
        let first = self.layout.blocks[index].first_frame;
        let end = self
            .layout
            .blocks
            .get(index + 1)
            .map_or(self.layout.header.frames, |b| b.first_frame);
        end - first
    }

    fn decompress(&mut self, index: usize) -> Result<Vec<u8>, FseqError> {
        let block = self.layout.blocks[index];
        let expected = self.frames_in(index) as usize * self.layout.frame_len();
        let mut compressed = vec![0u8; block.len as usize];
        self.reader.seek(SeekFrom::Start(block.offset))?;
        self.reader
            .read_exact(&mut compressed)
            .map_err(|_| corrupt("the file ends inside a compressed block"))?;
        let data = match self.layout.header.compression {
            Compression::Zstd => zstd::bulk::decompress(&compressed, expected)
                .map_err(|e| corrupt(format!("a zstd block won't decompress ({e})")))?,
            Compression::Zlib => {
                let mut data = Vec::with_capacity(expected);
                flate2::read::ZlibDecoder::new(compressed.as_slice())
                    .take(expected as u64)
                    .read_to_end(&mut data)
                    .map_err(|e| corrupt(format!("a zlib block won't decompress ({e})")))?;
                data
            }
            Compression::None => unreachable!("uncompressed frames are read directly"),
        };
        if data.len() < expected {
            return Err(corrupt(format!(
                "a block holds {} bytes but should hold {expected}",
                data.len()
            )));
        }
        Ok(data)
    }
}
