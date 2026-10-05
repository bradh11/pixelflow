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
