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
    let mut vars = variable(b"mf", "/Shows/Medley.mp3");
    vars.extend(variable(b"sp", "Test Writer 1.0"));
    let variable_start = 32 + index_entries * 8 + ranges.len() * 6;
    let data_offset = variable_start + vars.len();
    let mut out = Vec::new();
    out.extend_from_slice(b"PSEQ");
    out.extend_from_slice(&(data_offset as u16).to_le_bytes());
    out.extend_from_slice(&[2, 2]);
    out.extend_from_slice(&(variable_start as u16).to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
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
    check_all_frames(v2(16, 7, 1, 3, &ranges), 16, 7, stored);
    check_all_frames(v2(16, 7, 0, 0, &ranges), 16, 7, stored);
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
