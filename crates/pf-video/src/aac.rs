//! The video's sound: the stretch of music under the video, decoded as playback decodes it (so
//! it lines up with the lights the same way), then encoded as AAC-LC, the sound every MP4 player
//! plays. Any music PixelFlow plays works: MP3, M4A, WAV, FLAC, and Ogg Vorbis.
//!
//! The encoder (oxideav-aac, pure Rust) is slow, so the sound is cut into stretches encoded at
//! once on several threads. Each frame depends on the two stretches of samples it overlaps and on
//! how the frames before chose short or long windows; a stretch's encoder first runs over the
//! [`LEAD_IN`] frames before it (and throws those away), which settles both, so the stretches
//! join into the stream one encoder would have made.

use crate::VideoError;
use oxideav_aac::encoder::{EncoderConfig, FRAME_LEN, StreamEncoder};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// AAC's samples per frame (per channel).
pub const SAMPLES_PER_FRAME: u64 = FRAME_LEN as u64;
/// The encoder's delay: its first frame decodes to silence, so the sound starts one frame in
/// (the MP4 says so, and players skip it).
pub const PRIMING: u64 = SAMPLES_PER_FRAME;
/// Frames each thread encodes at a time (about 3 seconds at 44.1 kHz).
const STRETCH: usize = 128;
/// Frames run through before a stretch, to settle its encoder.
const LEAD_IN: usize = 3;
/// Bits per second for stereo.
pub const BITRATE: u32 = 192_000;
const CHANNELS: usize = 2;

/// Decoded stereo music, interleaved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pcm {
    pub rate: u32,
    pub samples: Vec<i16>,
}

impl Pcm {
    /// Sample pairs.
    pub fn frames(&self) -> u64 {
        (self.samples.len() / CHANNELS) as u64
    }
}

/// The encoded sound, as MP4 stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AacTrack {
    pub rate: u32,
    pub channels: u16,
    /// The decoder's setup (an AudioSpecificConfig).
    pub config: Vec<u8>,
    /// Raw AAC frames, in order (the first is the encoder's delay).
    pub frames: Vec<Vec<u8>>,
    /// How many samples (per channel) of real sound there are after the delay.
    pub samples: u64,
}

/// `count` sample pairs of the music at `path` from sample `first`, decoded as playback decodes
/// it; silence past its end. `stop` cancels (checked as it goes).
pub fn read_music(path: &Path, first: u64, count: u64, stop: &AtomicBool) -> Result<Pcm, VideoError> {
    let frames = pf_audio::StereoFrames::open(path).map_err(|e| VideoError::Music(e.to_string()))?;
    let rate = frames.sample_rate();
    let mut samples = Vec::with_capacity(count as usize * CHANNELS);
    for (n, (left, right)) in frames.skip(first as usize).take(count as usize).enumerate() {
        if n % 65_536 == 0 && stop.load(Ordering::Relaxed) {
            return Err(VideoError::Cancelled);
        }
        samples.push(to_i16(left));
        samples.push(to_i16(right));
    }
    samples.resize(count as usize * CHANNELS, 0);
    Ok(Pcm { rate, samples })
}

fn to_i16(v: f32) -> i16 {
    (v * 32767.0).round().clamp(-32768.0, 32767.0) as i16
}

/// Encodes `pcm` on up to `threads` threads. `done` counts the frames encoded so far (out of
/// [`frame_count`]); `stop` cancels.
pub fn encode(
    pcm: &Pcm,
    threads: usize,
    done: &AtomicU64,
    stop: &AtomicBool,
) -> Result<AacTrack, VideoError> {
    encode_in_stretches(pcm, threads, STRETCH, done, stop)
}

fn encode_in_stretches(
    pcm: &Pcm,
    threads: usize,
    stretch: usize,
    done: &AtomicU64,
    stop: &AtomicBool,
) -> Result<AacTrack, VideoError> {
    let config = EncoderConfig {
        sample_rate: pcm.rate,
        channels: CHANNELS as u8,
        bitrate: BITRATE,
    };
    let unsupported = || {
        VideoError::Music(format!(
            "The music's sample rate ({} Hz) can't go in a video. Convert it to 44.1 or 48 kHz.",
            pcm.rate
        ))
    };
    StreamEncoder::new(config).map_err(|_| unsupported())?;
    let hop = FRAME_LEN * CHANNELS;
    let hops: Vec<&[i16]> = pcm.samples.chunks(hop).collect();
    let stretches = hops.len().div_ceil(stretch).max(1);
    let results: Mutex<Vec<Option<Vec<Vec<u8>>>>> = Mutex::new(vec![None; stretches]);
    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    std::thread::scope(|scope| {
        for _ in 0..threads.clamp(1, stretches) {
            scope.spawn(|| {
                loop {
                    let s = next.fetch_add(1, Ordering::Relaxed);
                    if s >= stretches || stop.load(Ordering::Relaxed) || failed.load(Ordering::Relaxed) {
                        return;
                    }
                    let start = s * stretch;
                    let end = (start + stretch).min(hops.len());
                    match encode_stretch(config, &hops, start, end, end == hops.len(), done, stop) {
                        Ok(frames) => results.lock().unwrap_or_else(|e| e.into_inner())[s] = Some(frames),
                        Err(_) => failed.store(true, Ordering::Relaxed),
                    }
                }
            });
        }
    });
    if stop.load(Ordering::Relaxed) {
        return Err(VideoError::Cancelled);
    }
    if failed.load(Ordering::Relaxed) {
        return Err(VideoError::Encode("The sound couldn't be encoded.".into()));
    }
    let mut frames = Vec::with_capacity(hops.len() + 1);
    for stretch in results.into_inner().unwrap_or_else(|e| e.into_inner()) {
        frames.extend(stretch.ok_or_else(|| VideoError::Encode("The sound couldn't be encoded.".into()))?);
    }
    Ok(AacTrack {
        rate: pcm.rate,
        channels: CHANNELS as u16,
        config: oxideav_aac::asc_writer::aac_lc_asc(pcm.rate, CHANNELS as u8),
        frames,
        samples: pcm.frames(),
    })
}

/// How many AAC frames `pcm` makes: one per 1024 samples, and the delay's.
pub fn frame_count(samples: u64) -> u64 {
    samples.div_ceil(SAMPLES_PER_FRAME) + 1
}

/// Frames for hops `start..end` (and the final flush when `last`), as raw AAC.
fn encode_stretch(
    config: EncoderConfig,
    hops: &[&[i16]],
    start: usize,
    end: usize,
    last: bool,
    done: &AtomicU64,
    stop: &AtomicBool,
) -> Result<Vec<Vec<u8>>, oxideav_aac::Error> {
    let mut encoder = StreamEncoder::new(config)?;
    for hop in &hops[start.saturating_sub(LEAD_IN)..start] {
        encoder.encode_frame(hop)?;
    }
    let mut frames = Vec::with_capacity(end - start + 1);
    for hop in &hops[start..end] {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        frames.push(raw(encoder.encode_frame(hop)?));
        done.fetch_add(1, Ordering::Relaxed);
    }
    if last {
        frames.push(raw(encoder.finish()?));
        done.fetch_add(1, Ordering::Relaxed);
    }
    Ok(frames)
}

/// An ADTS frame without its header: what MP4 stores.
fn raw(mut adts: Vec<u8>) -> Vec<u8> {
    let protection_absent = adts.get(1).is_some_and(|b| b & 1 == 1);
    let header = if protection_absent { 7 } else { 9 };
    adts.drain(..header.min(adts.len()));
    adts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone with clicks in it (so the encoder switches to short windows now and then).
    fn tone(rate: u32, seconds: f32) -> Pcm {
        let n = (rate as f32 * seconds) as usize;
        let mut samples = Vec::with_capacity(n * 2);
        for i in 0..n {
            let t = i as f32 / rate as f32;
            let mut v = (t * 440.0 * std::f32::consts::TAU).sin() * 0.3;
            if i % 9000 < 40 {
                v = 0.9;
            }
            samples.push(to_i16(v));
            samples.push(to_i16(-v));
        }
        Pcm { rate, samples }
    }

    fn sequential(pcm: &Pcm) -> Vec<Vec<u8>> {
        let mut encoder = StreamEncoder::new(EncoderConfig {
            sample_rate: pcm.rate,
            channels: 2,
            bitrate: BITRATE,
        })
        .unwrap();
        let mut frames: Vec<Vec<u8>> = pcm
            .samples
            .chunks(FRAME_LEN * 2)
            .map(|hop| raw(encoder.encode_frame(hop).unwrap()))
            .collect();
        frames.push(raw(encoder.finish().unwrap()));
        frames
    }

    #[test]
    fn stretches_join_into_what_one_encoder_makes() {
        // Stretches of 4 frames: 8 of them, with clicks landing in some lead-ins.
        let pcm = tone(16_000, 2.0);
        let done = AtomicU64::new(0);
        let track = encode_in_stretches(&pcm, 4, 4, &done, &AtomicBool::new(false)).unwrap();
        assert_eq!(track.frames.len() as u64, frame_count(pcm.frames()));
        assert_eq!(done.load(Ordering::Relaxed), track.frames.len() as u64);
        assert_eq!(track.frames, sequential(&pcm));
        assert_eq!(track.samples, 32_000);
        // AAC-LC, 16 kHz (index 8), stereo.
        assert_eq!(track.config, vec![0b0001_0100, 0b0001_0000]);
    }

    #[test]
    fn frames_are_raw_aac() {
        let pcm = tone(48_000, 0.1);
        let track = encode(&pcm, 2, &AtomicU64::new(0), &AtomicBool::new(false)).unwrap();
        for frame in &track.frames {
            // A raw data block opens with a channel pair element, never an ADTS sync word.
            assert_eq!(frame[0] >> 5, 1, "CPE first");
            assert!(!(frame[0] == 0xff && frame[1] & 0xf0 == 0xf0));
        }
    }

    #[test]
    fn decoding_gives_the_tone_back() {
        // Round trip through the crate's own decoder: the 440 Hz tone comes back, one frame late.
        let pcm = tone(48_000, 1.0);
        let track = encode(&pcm, 2, &AtomicU64::new(0), &AtomicBool::new(false)).unwrap();
        let mut decoder = oxideav_aac::decode::StreamDecoder::new();
        let mut out: Vec<i16> = Vec::new();
        for frame in &track.frames {
            // AAC-LC (object type 2), 48 kHz (index 3), stereo, one block per frame.
            let decoded = decoder.decode_raw_data_block(2, 3, 48_000, 2, 1, frame).unwrap();
            out.extend(decoded.pcm);
        }
        // Compare the left channel a quarter second in, past the delay.
        let at = 12_000usize;
        let lag = PRIMING as usize;
        let (mut signal, mut noise) = (0f64, 0f64);
        for i in at..at + 4800 {
            let a = f64::from(pcm.samples[i * 2]);
            let b = f64::from(out[(i + lag) * 2]);
            signal += a * a;
            noise += (a - b) * (a - b);
        }
        let snr = 10.0 * (signal / noise).log10();
        assert!(snr > 20.0, "signal to noise {snr:.1} dB");
    }

    #[test]
    fn a_stop_cancels() {
        let pcm = tone(16_000, 1.0);
        let stop = AtomicBool::new(true);
        assert!(matches!(
            encode(&pcm, 2, &AtomicU64::new(0), &stop),
            Err(VideoError::Cancelled)
        ));
    }

    #[test]
    fn odd_sample_rates_are_refused() {
        let pcm = Pcm {
            rate: 44_000,
            samples: vec![0; 4096],
        };
        let err = encode(&pcm, 1, &AtomicU64::new(0), &AtomicBool::new(false)).unwrap_err();
        assert!(err.to_string().contains("44000 Hz"), "{err}");
    }

    #[test]
    fn samples_convert_to_16_bits() {
        assert_eq!(to_i16(0.0), 0);
        assert_eq!(to_i16(1.0), 32767);
        assert_eq!(to_i16(-1.5), -32768);
    }
}
