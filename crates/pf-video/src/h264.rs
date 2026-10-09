//! H.264 video with OpenH264 (Cisco's encoder, BSD licensed, built from source), as MP4 wants it:
//! each frame one sample of length-prefixed NAL units, with the stream's parameter sets kept
//! apart for the file's header.

use crate::VideoError;
use crate::yuv::Yuv;
use openh264::OpenH264API;
use openh264::encoder::{
    BitRate, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod, QpRange, RateControlMode,
    VuiConfig,
};

/// One encoded frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    /// NAL units, each after its length (4 bytes, big-endian).
    pub data: Vec<u8>,
    /// Whether playback can start here (an IDR frame).
    pub sync: bool,
}

/// The bitrate aimed for: about 0.15 bits per pixel per frame at 30 fps (9 Mb/s at 1080p) and a
/// little less per frame at 60, where each frame differs less from the last.
pub fn bitrate(width: u32, height: u32, fps: u32) -> u32 {
    let per_frame = if fps > 30 { 0.1 } else { 0.15 };
    (f64::from(width) * f64::from(height) * f64::from(fps) * per_frame) as u32
}

pub struct H264 {
    encoder: Encoder,
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
}

impl H264 {
    pub fn new(width: u32, height: u32, fps: u32) -> Result<Self, VideoError> {
        let config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(bitrate(width, height, fps)))
            .max_frame_rate(FrameRate::from_hz(fps as f32))
            .rate_control_mode(RateControlMode::Bitrate)
            // Every frame is kept, and never blurred past this, whatever the bitrate says.
            .skip_frames(false)
            .qp(QpRange::new(10, 34))
            // A keyframe every two seconds, so players can jump around the video.
            .intra_frame_period(IntraFramePeriod::from_num_frames(fps * 2))
            .vui(VuiConfig::bt709());
        let encoder = Encoder::with_api_config(OpenH264API::from_source(), config)
            .map_err(|e| VideoError::Encode(format!("The video encoder couldn't start: {e}")))?;
        Ok(Self {
            encoder,
            sps: None,
            pps: None,
        })
    }

    /// Encodes the next frame.
    pub fn encode(&mut self, picture: &Yuv) -> Result<Sample, VideoError> {
        let stream = self
            .encoder
            .encode(picture)
            .map_err(|e| VideoError::Encode(format!("The video encoder failed: {e}")))?;
        let sync = stream.frame_type() == FrameType::IDR;
        let mut data = Vec::new();
        for l in 0..stream.num_layers() {
            let Some(layer) = stream.layer(l) else { continue };
            for n in 0..layer.nal_count() {
                let Some(nal) = layer.nal_unit(n).map(without_start_code) else {
                    continue;
                };
                match nal.first().map(|b| b & 0x1f) {
                    None => {}
                    // Parameter sets go in the file's header; access unit delimiters aren't needed.
                    Some(7) => {
                        self.sps.get_or_insert_with(|| nal.to_vec());
                    }
                    Some(8) => {
                        self.pps.get_or_insert_with(|| nal.to_vec());
                    }
                    Some(9) => {}
                    Some(_) => {
                        data.extend_from_slice(&(nal.len() as u32).to_be_bytes());
                        data.extend_from_slice(nal);
                    }
                }
            }
        }
        if data.is_empty() {
            return Err(VideoError::Encode("The video encoder skipped a frame.".into()));
        }
        Ok(Sample { data, sync })
    }

    /// The stream's sequence and picture parameter sets, once the first frame is encoded.
    pub fn parameter_sets(&self) -> Option<(&[u8], &[u8])> {
        Some((self.sps.as_deref()?, self.pps.as_deref()?))
    }
}

/// A NAL unit without the start code before it (`00 00 01` or `00 00 00 01`).
fn without_start_code(nal: &[u8]) -> &[u8] {
    let zeros = nal.iter().take_while(|&&b| b == 0).count();
    if zeros >= 2 && nal.get(zeros) == Some(&1) {
        &nal[zeros + 1..]
    } else {
        nal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::Canvas;

    #[test]
    fn start_codes_are_dropped() {
        assert_eq!(without_start_code(&[0, 0, 0, 1, 0x67, 5]), &[0x67, 5]);
        assert_eq!(without_start_code(&[0, 0, 1, 0x68]), &[0x68]);
        assert_eq!(without_start_code(&[0x65, 1]), &[0x65, 1]);
    }

    #[test]
    fn frames_come_out_length_prefixed_with_a_keyframe_first() {
        let mut encoder = H264::new(64, 48, 30).unwrap();
        let mut canvas = Canvas::new(64, 48);
        let mut samples = Vec::new();
        for n in 0..4u16 {
            canvas.rgb.fill(n * 40);
            samples.push(encoder.encode(&Yuv::from_canvas(&canvas)).unwrap());
        }
        assert!(samples[0].sync);
        assert!(!samples[1].sync);
        let (sps, pps) = encoder.parameter_sets().unwrap();
        assert_eq!(sps[0] & 0x1f, 7);
        assert_eq!(pps[0] & 0x1f, 8);
        for sample in &samples {
            // Walk the NAL units by their lengths: they fill the sample exactly.
            let mut at = 0;
            while at < sample.data.len() {
                let len = u32::from_be_bytes(sample.data[at..at + 4].try_into().unwrap()) as usize;
                let kind = sample.data[at + 4] & 0x1f;
                assert!(matches!(kind, 1 | 5 | 6), "slices (and SEI) only: {kind}");
                at += 4 + len;
            }
            assert_eq!(at, sample.data.len());
        }
    }

    #[test]
    fn bitrates_by_size() {
        assert_eq!(bitrate(1920, 1080, 30), 9_331_200);
        assert_eq!(bitrate(1280, 720, 60), 5_529_600);
    }
}
