//! The onset envelope (spectral flux) and onset picking.

use realfft::RealFftPlanner;
use realfft::num_complex::Complex;

/// Samples per analysis frame.
pub const FRAME: usize = 1024;
/// Samples between frames.
pub const HOP: usize = 512;

/// How much new sound starts in each frame, plus the timing to turn frames into times.
#[derive(Debug, Clone, PartialEq)]
pub struct OnsetEnvelope {
    /// Spectral flux per frame (frame `n` starts at sample `n * HOP`).
    pub values: Vec<f32>,
    pub sample_rate: u32,
    /// Samples analyzed.
    pub samples: u64,
}

impl OnsetEnvelope {
    /// Seconds between frames.
    pub fn frame_seconds(&self) -> f64 {
        HOP as f64 / f64::from(self.sample_rate.max(1))
    }

    /// The time (ms) a frame stands for: the middle of its window, where a sound that makes it
    /// peak sits.
    pub fn time_ms(&self, frame: usize) -> u64 {
        ((frame * HOP + FRAME / 2) as f64 * 1000.0 / f64::from(self.sample_rate.max(1))).round() as u64
    }

    pub fn duration_ms(&self) -> u64 {
        self.samples * 1000 / u64::from(self.sample_rate.max(1))
    }

    /// Frames per `ms` milliseconds (at least 1).
    pub(crate) fn frames_for_ms(&self, ms: f64) -> usize {
        ((ms / 1000.0) / self.frame_seconds()).round().max(1.0) as usize
    }
}

/// Computes the onset envelope of mono samples, a frame at a time (memory stays small).
pub fn onset_envelope(samples: impl Iterator<Item = f32>, sample_rate: u32) -> OnsetEnvelope {
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(FRAME);
    let window: Vec<f32> = (0..FRAME)
        .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / FRAME as f32).cos())
        .collect();
    let mut input = fft.make_input_vec();
    let mut spectrum: Vec<Complex<f32>> = fft.make_output_vec();
    let mut scratch = fft.make_scratch_vec();
    let mut previous = vec![0.0f32; spectrum.len()];
    let mut current = vec![0.0f32; spectrum.len()];

    let mut buffer = vec![0.0f32; FRAME];
    let mut filled = 0usize;
    let mut samples_seen = 0u64;
    let mut values = Vec::new();
    let mut samples = samples.map(|s| if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 });
    let mut done = false;
    while !done {
        // Fill the window: the first time completely, then a hop at a time.
        while filled < FRAME {
            match samples.next() {
                Some(s) => {
                    buffer[filled] = s;
                    samples_seen += 1;
                }
                None => {
                    done = true;
                    break;
                }
            }
            filled += 1;
        }
        if done {
            // The last, partial frame (padded with silence), if it holds any new samples.
            let new_samples = filled.saturating_sub(FRAME - HOP);
            if values.is_empty() && filled == 0 || !values.is_empty() && new_samples == 0 {
                break;
            }
            buffer[filled..].fill(0.0);
        }
        for ((x, &s), &w) in input.iter_mut().zip(&buffer).zip(&window) {
            *x = s * w;
        }
        if fft
            .process_with_scratch(&mut input, &mut spectrum, &mut scratch)
            .is_err()
        {
            break;
        }
        let mut flux = 0.0f32;
        for ((c, p), bin) in current.iter_mut().zip(&previous).zip(&spectrum) {
            *c = (1.0 + 10.0 * bin.norm()).ln();
            flux += (*c - p).max(0.0);
        }
        // The first frame has nothing before it to compare with, so it can't be an onset (a song
        // that starts loud would otherwise always start with one).
        values.push(if values.is_empty() { 0.0 } else { flux });
        std::mem::swap(&mut previous, &mut current);
        buffer.copy_within(HOP.., 0);
        filled = FRAME - HOP;
    }
    OnsetEnvelope {
        values,
        sample_rate: sample_rate.max(1),
        samples: samples_seen,
    }
}

/// The envelope scaled to unit standard deviation (all zeros for silence).
pub(crate) fn normalized(envelope: &OnsetEnvelope) -> Vec<f32> {
    let v = &envelope.values;
    if v.is_empty() {
        return Vec::new();
    }
    let mean = v.iter().map(|&x| f64::from(x)).sum::<f64>() / v.len() as f64;
    let var = v.iter().map(|&x| (f64::from(x) - mean).powi(2)).sum::<f64>() / v.len() as f64;
    let sd = var.sqrt();
    if sd < 1e-9 {
        return vec![0.0; v.len()];
    }
    v.iter().map(|&x| (f64::from(x) / sd) as f32).collect()
}

/// Frames where a new sound starts: local peaks of the envelope that stand out from a moving
/// average, at least 50 ms apart.
pub fn pick_onsets(envelope: &OnsetEnvelope) -> Vec<usize> {
    let e = normalized(envelope);
    let n = e.len();
    let around_max = envelope.frames_for_ms(30.0);
    let around_mean = envelope.frames_for_ms(100.0);
    let gap = envelope.frames_for_ms(50.0);
    let delta = 0.5f32;
    // Running sums for the moving average.
    let mut prefix = vec![0.0f64; n + 1];
    for (i, &x) in e.iter().enumerate() {
        prefix[i + 1] = prefix[i] + f64::from(x);
    }
    let mut onsets: Vec<usize> = Vec::new();
    for i in 0..n {
        let x = e[i];
        if x <= 0.0 {
            continue;
        }
        let (lo, hi) = (i.saturating_sub(around_max), (i + around_max + 1).min(n));
        if e[lo..hi].iter().any(|&y| y > x) {
            continue;
        }
        let (alo, ahi) = (i.saturating_sub(around_mean), (i + around_mean + 1).min(n));
        let mean = ((prefix[ahi] - prefix[alo]) / (ahi - alo) as f64) as f32;
        if x < mean + delta {
            continue;
        }
        if onsets.last().is_some_and(|&last| i - last < gap) {
            continue;
        }
        onsets.push(i);
    }
    onsets
}
