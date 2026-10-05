//! Writing sequences and reading them back with the reader.

use pf_fseq::{Compression, FseqError, FseqWriter, Sequence, WriteOptions};
use std::io::Cursor;

fn value(frame: u32, channel: u32) -> u8 {
    (frame.wrapping_mul(31) ^ channel.wrapping_mul(7)) as u8
}

fn frame(f: u32, channels: u32) -> Vec<u8> {
    (0..channels).map(|c| value(f, c)).collect()
}

fn write(options: WriteOptions) -> Vec<u8> {
    let (channels, frames) = (options.channels, options.frames);
    let mut writer = FseqWriter::new(Cursor::new(Vec::new()), options).unwrap();
    for f in 0..frames {
        writer.write_frame(&frame(f, channels)).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn read_back(bytes: Vec<u8>) -> Sequence<Cursor<Vec<u8>>> {
    Sequence::from_reader(Cursor::new(bytes)).unwrap()
}

fn check_all_frames(seq: &mut Sequence<Cursor<Vec<u8>>>) {
    let (channels, frames) = (seq.header().channels, seq.header().frames);
    let mut out = vec![0u8; channels as usize];
    let order: Vec<u32> = (0..frames).rev().chain(0..frames).collect();
    for f in order {
        seq.read_frame(f, &mut out).unwrap();
        assert_eq!(out, frame(f, channels), "frame {f}");
    }
}

#[test]
fn round_trips_through_the_reader_with_headers() {
    let mut options = WriteOptions::new(30, 23, 25);
    options.frames_per_block = 5;
    options.media = Some("Carol of the Bells.mp3".into());
    options.producer = Some("PixelFlow 0.1.0".into());
    let bytes = write(options);
    assert_eq!(&bytes[..4], b"PSEQ");
    assert_eq!(
        (bytes[6], bytes[7]),
        (2, 2),
        "version 2.2 (minor, major), like xLights and FPP"
    );
    let header_len = usize::from(u16::from_le_bytes([bytes[4], bytes[5]]));
    assert_eq!(header_len % 4, 0, "the header is padded to a multiple of 4 bytes");
    let mut seq = read_back(bytes);
    let h = seq.header().clone();
    assert_eq!((h.version, h.channels, h.frames, h.step_ms), ((2, 2), 30, 23, 25));
    assert_eq!(h.compression, Compression::Zstd);
    assert_eq!(h.media.as_deref(), Some("Carol of the Bells.mp3"));
    assert_eq!(h.producer.as_deref(), Some("PixelFlow 0.1.0"));
    check_all_frames(&mut seq);
}

#[test]
fn headers_are_optional_and_a_single_frame_works() {
    let mut seq = read_back(write(WriteOptions::new(3, 1, 50)));
    assert_eq!(seq.header().media, None);
    assert_eq!(seq.header().producer, None);
    check_all_frames(&mut seq);
}

#[test]
fn long_sequences_use_bigger_blocks_to_fit_the_block_list() {
    let mut options = WriteOptions::new(6, 9000, 10);
    options.frames_per_block = 1;
    let writer = FseqWriter::new(Cursor::new(Vec::new()), options.clone()).unwrap();
    assert_eq!(writer.frames_per_block(), 3, "9000 frames in at most 4095 blocks");
    let bytes = write(options);
    // The block count's high bits live in the compression byte.
    let blocks = usize::from(bytes[21]) | (usize::from(bytes[20] >> 4) << 8);
    assert_eq!(blocks, 3000);
    check_all_frames(&mut read_back(bytes));
}

/// Block index entry `i`: (first frame, compressed length).
fn index_entry(bytes: &[u8], i: usize) -> (u32, u32) {
    let at = 32 + i * 8;
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    (word(at), word(at + 4))
}

/// The exact bytes FPP's reader expects (FPP's docs/FSEQ_Sequence_File_Format.txt), checked by
/// hand against FPP's V2FSEQFile::writeHeader and an xLights 2024 export.
#[test]
fn the_header_is_laid_out_byte_for_byte_like_fpp_writes_it() {
    let mut options = WriteOptions::new(4, 3, 25);
    options.media = Some("a.mp3".into());
    options.producer = Some("PF".into());
    options.unique_id = 0x0807_0605_0403_0201;
    let bytes = write(options);
    let block_len = (bytes.len() - 60) as u32;
    let mut expected = Vec::new();
    expected.extend_from_slice(b"PSEQ");
    expected.extend_from_slice(&[60, 0]); // channel data offset: 57 header bytes padded to 60
    expected.extend_from_slice(&[2, 2]); // version 2.2 (minor, major), as xLights 2024 writes
    expected.extend_from_slice(&[40, 0]); // first variable header: 32 + one 8-byte index entry
    expected.extend_from_slice(&[4, 0, 0, 0]); // channels per frame
    expected.extend_from_slice(&[3, 0, 0, 0]); // frames
    expected.push(25); // step time (ms)
    expected.push(0); // flags
    expected.push(0x01); // zstd; block count's high 4 bits (0) in the top nibble
    expected.push(1); // block count, low 8 bits
    expected.push(0); // no sparse ranges: the whole channel space is stored
    expected.push(0); // flags
    expected.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]); // unique id
    expected.extend_from_slice(&[0, 0, 0, 0]); // block 0: first frame
    expected.extend_from_slice(&block_len.to_le_bytes()); // block 0: compressed length
    expected.extend_from_slice(&[10, 0, b'm', b'f', b'a', b'.', b'm', b'p', b'3', 0]);
    expected.extend_from_slice(&[7, 0, b's', b'p', b'P', b'F', 0]);
    expected.extend_from_slice(&[0, 0, 0]); // padding to a multiple of 4
    assert_eq!(&bytes[..60], &expected[..]);
    // One zstd frame holding the three frames back to back.
    let raw = zstd::bulk::decompress(&bytes[60..], 1024).unwrap();
    assert_eq!(raw, [frame(0, 4), frame(1, 4), frame(2, 4)].concat());
}

/// FPP's and xLights' writer end the first block after frame 9 whatever the block size, so a
/// player (or a MultiSync remote) has frame 0 as soon as possible.
#[test]
fn the_first_block_holds_the_first_ten_frames() {
    let options = WriteOptions::new(6, 100_000, 25);
    let writer = FseqWriter::new(Cursor::new(Vec::new()), options.clone()).unwrap();
    assert_eq!(
        writer.frames_per_block(),
        25,
        "the 99,990 frames after the first block fit 4094 more blocks"
    );
    let bytes = write(options);
    let blocks = usize::from(bytes[21]) | (usize::from(bytes[20] >> 4) << 8);
    assert_eq!(blocks, 4001);
    assert_eq!(index_entry(&bytes, 0).0, 0);
    assert_eq!(index_entry(&bytes, 1).0, 10);
    assert_eq!(index_entry(&bytes, 2).0, 35);
    assert_eq!(index_entry(&bytes, 4000).0, 99_985);
    let mut seq = read_back(bytes);
    let mut out = vec![0u8; 6];
    for f in [0, 9, 10, 34, 35, 50_000, 99_999] {
        seq.read_frame(f, &mut out).unwrap();
        assert_eq!(out, frame(f, 6), "frame {f}");
    }

    // Smaller blocks than that (huge frames) start no differently.
    let bytes = write({
        let mut o = WriteOptions::new(6, 20, 25);
        o.frames_per_block = 4;
        o
    });
    let firsts: Vec<u32> = (0..5).map(|i| index_entry(&bytes, i).0).collect();
    assert_eq!(firsts, [0, 4, 8, 12, 16]);
}

#[test]
fn huge_frames_get_small_blocks() {
    let options = WriteOptions::new(20_000_000, 2, 25);
    let writer = FseqWriter::new(Cursor::new(Vec::new()), options).unwrap();
    assert_eq!(
        writer.frames_per_block(),
        3,
        "blocks stay under 64 MB before compression"
    );
}

#[test]
fn files_can_start_part_way_into_the_output() {
    let mut out = Cursor::new(b"junk".to_vec());
    out.set_position(4);
    let mut writer = FseqWriter::new(out, WriteOptions::new(3, 2, 25)).unwrap();
    writer.write_frame(&frame(0, 3)).unwrap();
    writer.write_frame(&frame(1, 3)).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    assert_eq!(&bytes[..8], b"junkPSEQ");
    check_all_frames(&mut read_back(bytes[4..].to_vec()));
}

#[test]
fn misuse_is_explained() {
    let start = |o: WriteOptions| {
        FseqWriter::new(Cursor::new(Vec::new()), o)
            .err()
            .unwrap()
            .to_string()
    };
    assert!(start(WriteOptions::new(0, 1, 25)).contains("0 channels"));
    assert!(start(WriteOptions::new(3, 1, 300)).contains("300 ms apart"));
    assert!(start(WriteOptions::new(3, 0, 25)).contains("no frames"));

    let mut writer = FseqWriter::new(Cursor::new(Vec::new()), WriteOptions::new(3, 2, 25)).unwrap();
    let err = writer.write_frame(&[1, 2]).unwrap_err();
    assert!(matches!(err, FseqError::CantWrite(_)));
    assert_eq!(
        err.to_string(),
        "Can't write this sequence: a frame has 2 channels, not 3."
    );
    writer.write_frame(&[1, 2, 3]).unwrap();
    let err = writer.finish().err().unwrap();
    assert!(err.to_string().contains("only 1 of its 2 frames"), "{err}");

    let mut writer = FseqWriter::new(Cursor::new(Vec::new()), WriteOptions::new(1, 1, 25)).unwrap();
    writer.write_frame(&[1]).unwrap();
    assert!(
        writer
            .write_frame(&[1])
            .unwrap_err()
            .to_string()
            .contains("more than the 1")
    );
}
