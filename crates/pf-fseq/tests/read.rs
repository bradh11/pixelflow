//! Reading generated sequences in every supported layout, and (when `PIXELFLOW_FSEQ` names
//! one) a real sequence file.

use pf_fseq::{Compression, FseqError, Sequence};
use std::io::{Cursor, Write};

/// Channel value for (frame, channel): distinct enough to catch any mix-up.
fn value(frame: u32, channel: u32) -> u8 {
    (frame.wrapping_mul(31) ^ channel.wrapping_mul(7)) as u8
}

fn frame_bytes(frame: u32, channels: &[u32]) -> Vec<u8> {
    channels.iter().map(|&c| value(frame, c)).collect()
}

fn variable(code: &[u8; 2], text: &str) -> Vec<u8> {
    let mut out = ((text.len() + 5) as u16).to_le_bytes().to_vec();
    out.extend_from_slice(code);
    out.extend_from_slice(text.as_bytes());
    out.push(0);
    out
}

/// Writes a version 2 file. `ranges` empty = the whole channel space is stored.
fn v2(channels: u32, frames: u32, compression: u8, frames_per_block: u32, ranges: &[(u32, u32)]) -> Vec<u8> {
    let mut vars = variable(b"mf", "/Shows/Medley.mp3");
    vars.extend(variable(b"sp", "Test Writer 1.0"));
    v2_with(channels, frames, compression, frames_per_block, ranges, vars)
}

/// [`v2`] with the given variable headers. As FPP and xLights write them, a sparse file's
/// channel count is the bytes stored per frame (the ranges' total), not the channel space.
fn v2_with(
    channels: u32,
    frames: u32,
    compression: u8,
    frames_per_block: u32,
    ranges: &[(u32, u32)],
    vars: Vec<u8>,
) -> Vec<u8> {
    let stored: Vec<u32> = if ranges.is_empty() {
        (0..channels).collect()
    } else {
        ranges.iter().flat_map(|&(s, n)| s..s + n).collect()
    };
    let mut blocks: Vec<(u32, Vec<u8>)> = Vec::new();
    if compression != 0 {
        let mut first = 0;
        while first < frames {
            let end = (first + frames_per_block).min(frames);
            let raw: Vec<u8> = (first..end).flat_map(|f| frame_bytes(f, &stored)).collect();
            let packed = match compression {
                1 => zstd::bulk::compress(&raw, 3).unwrap(),
                _ => {
                    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                    e.write_all(&raw).unwrap();
                    e.finish().unwrap()
                }
            };
            blocks.push((first, packed));
            first = end;
        }
    }
    let index_entries = if compression != 0 { blocks.len() + 2 } else { 0 }; // + xLights-style padding
    let variable_start = 32 + index_entries * 8 + ranges.len() * 6;
    let data_offset = variable_start + vars.len();
    let mut out = Vec::new();
    out.extend_from_slice(b"PSEQ");
    out.extend_from_slice(&(data_offset as u16).to_le_bytes());
    out.extend_from_slice(&[2, 2]);
    out.extend_from_slice(&(variable_start as u16).to_le_bytes());
    let channel_count = if ranges.is_empty() {
        channels
    } else {
        stored.len() as u32
    };
    out.extend_from_slice(&channel_count.to_le_bytes());
    out.extend_from_slice(&frames.to_le_bytes());
    out.push(25); // 25 ms
    out.push(0);
    out.push(compression | (((index_entries >> 8) as u8) << 4));
    out.push(index_entries as u8);
    out.push(ranges.len() as u8);
    out.push(0);
    out.extend_from_slice(&0u64.to_le_bytes());
    for (first, packed) in &blocks {
        out.extend_from_slice(&first.to_le_bytes());
        out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    }
    for _ in blocks.len()..index_entries {
        out.extend_from_slice(&[0; 8]);
    }
    for &(s, n) in ranges {
        out.extend_from_slice(&s.to_le_bytes()[..3]);
        out.extend_from_slice(&n.to_le_bytes()[..3]);
    }
    out.extend(vars);
    assert_eq!(out.len(), data_offset);
    if compression == 0 {
        for f in 0..frames {
            out.extend(frame_bytes(f, &stored));
        }
    } else {
        for (_, packed) in blocks {
            out.extend(packed);
        }
    }
    out
}

fn v1(channels: u32, frames: u32) -> Vec<u8> {
    let data_offset = 28u16;
    let mut out = Vec::new();
    out.extend_from_slice(b"PSEQ");
    out.extend_from_slice(&data_offset.to_le_bytes());
    out.extend_from_slice(&[0, 1]);
    out.extend_from_slice(&28u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&frames.to_le_bytes());
    out.push(50);
    out.extend_from_slice(&[0; 9]);
    let all: Vec<u32> = (0..channels).collect();
    for f in 0..frames {
        out.extend(frame_bytes(f, &all));
    }
    out
}

fn check_all_frames(bytes: Vec<u8>, channels: u32, frames: u32, stored: impl Fn(u32) -> bool) {
    let mut seq = Sequence::from_reader(Cursor::new(bytes)).unwrap();
    let mut out = vec![0u8; channels as usize];
    // Out of order on purpose: jumps between blocks must work, not just playing forward.
    let order: Vec<u32> = (0..frames).rev().chain(0..frames).collect();
    for f in order {
        seq.read_frame(f, &mut out).unwrap();
        for c in 0..channels {
            let expected = if stored(c) { value(f, c) } else { 0 };
            assert_eq!(out[c as usize], expected, "frame {f}, channel {c}");
        }
    }
}

#[test]
fn reads_zstd_blocks_in_any_order() {
    let bytes = v2(30, 23, 1, 5, &[]);
    let seq = Sequence::from_reader(Cursor::new(bytes.clone())).unwrap();
    let h = seq.header();
    assert_eq!((h.version, h.channels, h.frames, h.step_ms), ((2, 2), 30, 23, 25));
    assert_eq!(h.compression, Compression::Zstd);
    assert_eq!(h.media.as_deref(), Some("/Shows/Medley.mp3"));
    assert_eq!(h.producer.as_deref(), Some("Test Writer 1.0"));
    assert_eq!(h.duration_ms(), 575);
    check_all_frames(bytes, 30, 23, |_| true);
}

#[test]
fn reads_zlib_and_uncompressed_version_2() {
    check_all_frames(v2(12, 9, 2, 4, &[]), 12, 9, |_| true);
    check_all_frames(v2(12, 9, 0, 0, &[]), 12, 9, |_| true);
}

#[test]
fn sparse_ranges_fill_the_rest_with_zeros() {
    let ranges = [(3, 4), (10, 2)];
    let stored = |c: u32| (3..7).contains(&c) || (10..12).contains(&c);
    // The channel space ends with the last range (channel 12), as FPP's getMaxChannel() says.
    check_all_frames(v2(16, 7, 1, 3, &ranges), 12, 7, stored);
    check_all_frames(v2(16, 7, 0, 0, &ranges), 12, 7, stored);
}

/// FPP's format document's example: one range of 50 channels from channel 5000, so the header's
/// channel count is 50 (what each frame stores), and FPP plays them on channels 5001–5050.
#[test]
fn a_sparse_files_channel_count_is_what_each_frame_stores() {
    for compression in [0, 1, 2] {
        let bytes = v2(5050, 4, compression, 3, &[(5000, 50)]);
        assert_eq!(&bytes[10..14], &50u32.to_le_bytes());
        let seq = Sequence::from_reader(Cursor::new(bytes.clone())).unwrap();
        assert_eq!(seq.header().channels, 5050, "the channel space");
        check_all_frames(bytes, 5050, 4, |c| c >= 5000);
    }
    // xLights writes one range over the whole file.
    check_all_frames(v2(6148, 12, 1, 10, &[(0, 6148)]), 6148, 12, |_| true);
}

#[test]
fn ranges_holding_more_than_a_frame_stores_are_refused() {
    let mut bytes = v2(20, 2, 0, 0, &[(0, 4), (10, 4)]);
    bytes[10..14].copy_from_slice(&6u32.to_le_bytes()); // 8 bytes of ranges, 6 stored
    let err = Sequence::from_reader(Cursor::new(bytes)).err().unwrap();
    assert!(err.to_string().contains("channel ranges"), "{err}");
}

/// FPP writes the header's frame count before the frames, so a file can hold more frames than it
/// declares (a longer last block, even whole blocks past the end). FPP plays the declared ones.
#[test]
fn frames_past_the_declared_count_are_ignored() {
    for compression in [1, 2] {
        let mut bytes = v2(12, 11, compression, 4, &[]); // blocks start at 0, 4, 8
        bytes[14..18].copy_from_slice(&6u32.to_le_bytes()); // declare 6 frames
        let seq = Sequence::from_reader(Cursor::new(bytes.clone())).unwrap();
        assert_eq!(seq.header().frames, 6);
        check_all_frames(bytes, 12, 6, |_| true);
    }
}

/// An extended-data ('ED') variable header: the text lives elsewhere in the file. FPP 2.2 moves
/// headers there when they don't fit the 64 KB header (xLights' embedded show files can do that).
fn extended(code: &[u8; 2], offset: u64, len: u32) -> Vec<u8> {
    let mut out = 18u16.to_le_bytes().to_vec();
    out.extend_from_slice(b"ED");
    out.extend_from_slice(code);
    out.extend_from_slice(&offset.to_le_bytes());
    out.extend_from_slice(&len.to_le_bytes());
    out
}

#[test]
fn extended_media_and_producer_headers_are_read() {
    let media = b"/Users/me/Music/Extended Song.mp3\0";
    let producer = b"xLights Macintosh 2024.19\0";
    let mut vars = extended(b"mf", 0, media.len() as u32);
    vars.extend(extended(b"sp", 0, producer.len() as u32));
    vars.extend(extended(b"XS", 0, 3)); // other extended data is skipped
    let mut bytes = v2_with(12, 5, 1, 2, &[], vars);
    // Like FPP, put the text after the channel data and point the headers at it.
    let at = bytes.windows(4).position(|w| w == b"EDmf").unwrap() + 4;
    let end = bytes.len() as u64;
    bytes[at..at + 8].copy_from_slice(&end.to_le_bytes());
    bytes[at + 18..at + 26].copy_from_slice(&(end + media.len() as u64).to_le_bytes());
    bytes.extend_from_slice(media);
    bytes.extend_from_slice(producer);
    bytes.extend_from_slice(b"xyz");
    let seq = Sequence::from_reader(Cursor::new(bytes.clone())).unwrap();
    assert_eq!(
        seq.header().media.as_deref(),
        Some("/Users/me/Music/Extended Song.mp3")
    );
    assert_eq!(
        seq.header().producer.as_deref(),
        Some("xLights Macintosh 2024.19")
    );
    check_all_frames(bytes, 12, 5, |_| true);

    // Pointing past the end of the file (a damaged header) leaves the media unknown.
    let mut vars = extended(b"mf", 1 << 40, 10);
    vars.extend(variable(b"sp", "Writer"));
    let seq = Sequence::from_reader(Cursor::new(v2_with(12, 5, 1, 2, &[], vars))).unwrap();
    assert_eq!(seq.header().media, None);
    assert_eq!(seq.header().producer.as_deref(), Some("Writer"));
}

#[test]
fn reads_version_1() {
    let bytes = v1(9, 4);
    let seq = Sequence::from_reader(Cursor::new(bytes.clone())).unwrap();
    assert_eq!((seq.header().version.0, seq.header().step_ms), (1, 50));
    check_all_frames(bytes, 9, 4, |_| true);
}

#[test]
fn damaged_and_foreign_files_are_reported_plainly() {
    let err = |bytes: Vec<u8>| Sequence::from_reader(Cursor::new(bytes)).err().unwrap();
    assert!(matches!(
        err(b"<xml>not a sequence</xml>".to_vec()),
        FseqError::NotFseq
    ));
    assert!(matches!(err(b"PSEQ".to_vec()), FseqError::NotFseq));

    let mut huge = v2(12, 3, 1, 2, &[]);
    huge[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(err(huge).to_string().contains("channels"));

    let mut odd = v2(12, 3, 1, 2, &[]);
    odd[20] = (odd[20] & 0xF0) | 7;
    assert_eq!(
        err(odd).to_string(),
        "This sequence uses compression type 7, which PixelFlow can't read yet."
    );

    let mut cut = v2(12, 6, 1, 2, &[]);
    cut.truncate(cut.len() - 3);
    let mut seq = Sequence::from_reader(Cursor::new(cut)).unwrap();
    let mut out = vec![0u8; 12];
    assert!(seq.read_frame(5, &mut out).is_err(), "truncated last block");
    assert!(seq.read_frame(0, &mut out).is_ok(), "earlier blocks still read");
    assert!(seq.read_frame(6, &mut out).is_err(), "past the end");
    assert!(seq.read_frame(0, &mut [0u8; 5]).is_err(), "wrong buffer size");
}

#[test]
fn hostile_headers_fail_without_huge_allocations() {
    // Billions of frames of 64M channels in one block: must be refused, not allocated.
    for compression in [1u8, 2] {
        let mut huge = v2(12, 3, compression, 3, &[]);
        huge[10..14].copy_from_slice(&(64u32 * 1024 * 1024).to_le_bytes());
        huge[14..18].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut seq = Sequence::from_reader(Cursor::new(huge)).unwrap();
        let mut out = vec![0u8; 64 * 1024 * 1024];
        let err = seq.read_frame(0, &mut out).unwrap_err();
        assert!(err.to_string().contains("too large"), "{err}");
    }

    // 255 full-width ranges would claim 255x the channel space per frame.
    let ranges = vec![(0, 40); 255];
    let overlapping = v2(40, 2, 0, 0, &ranges);
    let err = Sequence::from_reader(Cursor::new(overlapping)).err().unwrap();
    assert!(err.to_string().contains("overlap"), "{err}");

    // Adjacent, in-order ranges are fine.
    assert!(Sequence::from_reader(Cursor::new(v2(16, 2, 0, 0, &[(0, 8), (8, 8)]))).is_ok());
}

/// Set `PIXELFLOW_FSEQ=/path/to/show.fseq` to check a real sequence: every frame must decode.
#[test]
fn real_sequence_decodes_when_provided() {
    let Ok(path) = std::env::var("PIXELFLOW_FSEQ") else {
        return;
    };
    let mut seq = Sequence::open(&path).unwrap();
    let header = seq.header().clone();
    let mut out = vec![0u8; header.channels as usize];
    let mut lit_frames = 0;
    for f in 0..header.frames {
        seq.read_frame(f, &mut out).unwrap();
        lit_frames += usize::from(out.iter().any(|&b| b != 0));
    }
    eprintln!(
        "{path}: {header:?}; {lit_frames} of {} frames have light",
        header.frames
    );
    assert!(lit_frames > 0);
}
