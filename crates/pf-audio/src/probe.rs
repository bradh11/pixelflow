//! How long a music file plays, found quickly: from what the file says about itself (a WAV's
//! header, an M4A's index, an MP3's Xing, Info, or VBRI header, FLAC's stream info) rather than
//! by decoding it all. An MP3 without such a header only has a guess from its first frames'
//! bitrate, so its frames are counted instead (read, not decoded); a file that says nothing is
//! decoded as a last resort.

use crate::decode::open_decoder;
use crate::error::AudioError;
use crate::progress::{CountedFile, Progress, ReadPosition, reported};
use rodio::Source;
use serde::Serialize;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use symphonia::core::codecs::{CODEC_TYPE_MP3, CodecParameters};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// How much of an MP3's start is searched for its first frame.
const MP3_HEAD_BYTES: u64 = 256 * 1024;

/// A music file's length and format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInfo {
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    /// The codec's short name ("mp3", "aac", "flac", "vorbis", "pcm_s16le").
    pub codec: String,
    /// How the length was found.
    pub found_by: FoundBy,
}

/// Where a file's length came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FoundBy {
    /// The file's own header or index.
    Header,
    /// Counting an MP3's frames.
    Frames,
    /// Decoding the whole file.
    Decoding,
}

/// Finds how long the music file at `path` plays, telling `progress` how far it has got when it
/// has to read the whole file (0–1; a header read reports nothing until the final 1).
pub fn probe(path: &Path, progress: &dyn Fn(f32)) -> Result<AudioInfo, AudioError> {
    let progress = Progress::new(progress);
    let shown = path.display().to_string();
    let decode_error = |e: SymphoniaError| AudioError::Decode {
        path: shown.clone(),
        reason: e.to_string(),
    };
    let opened = File::open(path).and_then(CountedFile::new);
    let (file, read) = opened.map_err(|source| AudioError::Open {
        path: shown.clone(),
        source,
    })?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }
    // Gapless, as playback decodes: an MP3's encoder delay and padding aren't counted.
    let options = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let probed = symphonia::default::get_probe()
        .format(&hint, stream, &options, &MetadataOptions::default())
        .map_err(decode_error)?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| decode_error(SymphoniaError::Unsupported("no audio track")))?;
    let params = track.codec_params.clone();
    let track_id = track.id;
    let codec = symphonia::default::get_codecs()
        .get_codec(params.codec)
        .map_or("unknown", |d| d.short_name)
        .to_string();
    let sample_rate = params.sample_rate.unwrap_or(0);
    let channels = params.channels.map_or(0, |c| c.count() as u16);
    let info = |duration_ms, found_by| AudioInfo {
        duration_ms,
        sample_rate,
        channels,
        codec: codec.clone(),
        found_by,
    };

    let trusted = params.codec != CODEC_TYPE_MP3 || mp3_has_length_header(path);
    if trusted && let Some(ms) = header_ms(&params).filter(|&ms| ms > 0) {
        progress.finish();
        return Ok(info(ms, FoundBy::Header));
    }
    if params.codec == CODEC_TYPE_MP3
        && let Some(ms) = count_frames(format.as_mut(), track_id, &params, &read, &progress)
    {
        progress.finish();
        return Ok(info(ms, FoundBy::Frames));
    }
    drop(format);
    let decoded = decoded_ms(path, &progress)?;
    progress.finish();
    Ok(AudioInfo {
        duration_ms: decoded.0,
        sample_rate: decoded.1,
        channels: decoded.2,
        codec,
        found_by: FoundBy::Decoding,
    })
}

/// The length the track's codec parameters give, in ms.
fn header_ms(params: &CodecParameters) -> Option<u64> {
    let frames = params.n_frames?;
    match params.time_base {
        Some(base) => {
            let time = base.calc_time(frames);
            Some(time.seconds * 1000 + (time.frac * 1000.0).round() as u64)
        }
        None => {
            let rate = u64::from(params.sample_rate?);
            (rate > 0).then(|| frames * 1000 / rate)
        }
    }
}

/// Reads an MP3's frames to the end without decoding them and adds up their lengths, in ms.
fn count_frames(
    format: &mut dyn FormatReader,
    track_id: u32,
    params: &CodecParameters,
    read: &ReadPosition,
    progress: &Progress,
) -> Option<u64> {
    let rate = u64::from(params.sample_rate?);
    let mut frames = 0u64;
    let mut packets = 0usize;
    loop {
        match format.next_packet() {
            Ok(packet) => {
                // Untrimmed: the reader trims to its guessed length, which is what's being
                // checked (and a file without the header has no delay or padding to trim).
                if packet.track_id() == track_id {
                    frames += packet.dur;
                }
                packets += 1;
                if packets.is_multiple_of(256) {
                    progress.set(read.fraction());
                }
            }
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymphoniaError::ResetRequired) => break,
            Err(_) if frames > 0 => break,
            Err(_) => return None,
        }
    }
    (rate > 0 && frames > 0).then(|| frames * 1000 / rate)
}

/// Decodes the whole file: its length (ms), rate, and channel count.
fn decoded_ms(path: &Path, progress: &Progress) -> Result<(u64, u32, u16), AudioError> {
    let (decoder, read) = open_decoder(path)?;
    let rate = decoder.sample_rate().get();
    let channels = decoder.channels().get();
    let total = reported(decoder, read, progress, 1.0).count() as u64;
    let ms = total * 1000 / (u64::from(rate) * u64::from(channels)).max(1);
    Ok((ms, rate, channels))
}

/// Whether an MP3's first frame holds a Xing, Info, or VBRI header (which say how many frames
/// the file has). Without one, the reader's length is a guess from the first frames' bitrate,
/// right for a constant bitrate but not a variable one.
fn mp3_has_length_header(path: &Path) -> bool {
    let mut head = Vec::new();
    let Ok(file) = File::open(path) else { return false };
    if file.take(MP3_HEAD_BYTES).read_to_end(&mut head).is_err() {
        return false;
    }
    first_frame_has_length_header(&head)
}

fn first_frame_has_length_header(bytes: &[u8]) -> bool {
    // An ID3v2 tag ahead of the audio: its size is in the header, seven bits a byte.
    let mut at = 0usize;
    if bytes.len() >= 10 && &bytes[..3] == b"ID3" {
        let size = bytes[6..10]
            .iter()
            .fold(0usize, |n, &b| (n << 7) | usize::from(b & 0x7f));
        let footer = if bytes[5] & 0x10 != 0 { 10 } else { 0 };
        at = 10 + size + footer;
    }
    while at + 4 <= bytes.len() {
        let header = u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
        let Some(side_info) = layer3_side_info(header) else {
            at += 1;
            continue;
        };
        let tag = |offset: usize| bytes.get(at + 4 + offset..at + 8 + offset);
        return matches!(tag(side_info), Some(b"Xing" | b"Info")) || tag(32) == Some(b"VBRI");
    }
    false
}

/// The side information's length after an MPEG audio layer 3 frame header, or `None` when
/// `header` isn't one.
fn layer3_side_info(header: u32) -> Option<usize> {
    let sync = header >> 21 == 0x7ff;
    let version = (header >> 19) & 0b11;
    let layer = (header >> 17) & 0b11;
    let bitrate = (header >> 12) & 0b1111;
    let rate = (header >> 10) & 0b11;
    if !sync || version == 0b01 || layer != 0b01 || bitrate == 0 || bitrate == 0b1111 || rate == 0b11 {
        return None;
    }
    let mono = (header >> 6) & 0b11 == 0b11;
    let mpeg1 = version == 0b11;
    Some(match (mpeg1, mono) {
        (true, false) => 32,
        (true, true) | (false, false) => 17,
        (false, true) => 9,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer3_headers_are_told_from_noise() {
        // MPEG-1 layer 3, 128 kbit/s, 44.1 kHz, joint stereo.
        assert_eq!(layer3_side_info(0xfffb_9064), Some(32));
        // Mono.
        assert_eq!(layer3_side_info(0xfffb_90c4), Some(17));
        // MPEG-2, 22.05 kHz, stereo.
        assert_eq!(layer3_side_info(0xfff3_9064), Some(17));
        assert_eq!(layer3_side_info(0x4944_3304), None);
        // A bad bitrate.
        assert_eq!(layer3_side_info(0xfffb_f064), None);
    }

    #[test]
    fn a_xing_header_is_found_after_an_id3_tag() {
        let mut bytes = b"ID3\x04\x00\x00\x00\x00\x00\x05hello".to_vec();
        let frame = |tag: &[u8], at: usize| {
            let mut f = vec![0xff, 0xfb, 0x90, 0x64];
            f.resize(4 + at, 0);
            f.extend_from_slice(tag);
            f.resize(417, 0);
            f
        };
        let mut xing = bytes.clone();
        xing.extend(frame(b"Xing", 32));
        assert!(first_frame_has_length_header(&xing));
        let mut info = bytes.clone();
        info.extend(frame(b"Info", 32));
        assert!(first_frame_has_length_header(&info));
        let mut vbri = bytes.clone();
        vbri.extend(frame(b"VBRI", 32));
        assert!(first_frame_has_length_header(&vbri));
        // Audio straight away: no header.
        bytes.extend(frame(b"\x12\x34\x56\x78", 32));
        assert!(!first_frame_has_length_header(&bytes));
    }
}
