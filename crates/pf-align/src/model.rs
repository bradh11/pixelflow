//! The speech model that hears the letters: wav2vec2-base-960h (Facebook AI, Apache-2.0),
//! English letters every 20 ms from 16 kHz sound, run by tract (pure Rust ONNX, nothing to
//! install). Its weights are stored half-size (16-bit) and worked out at full precision: the
//! 8-bit and 4-bit versions published beside it don't run correctly in tract.

use crate::AlignError;
use crate::ctc::Emission;
use crate::vocab;
use std::path::Path;
use tract_onnx::prelude::*;
use tract_onnx::tract_core::floats::FloatPrecisionTranslator;

/// Hears letters in sound.
pub trait Acoustic: Send + Sync {
    /// How likely each letter is every 20 ms of `samples` (mono, 16 kHz): a step per 320
    /// samples, the first covering the first 400.
    fn emit(&self, samples: &[f32]) -> Result<Emission, AlignError>;
}

/// The steps the model gives for `samples` samples.
pub fn frames_for(samples: usize) -> usize {
    if samples < 400 {
        0
    } else {
        (samples - 400) / 320 + 1
    }
}

/// wav2vec2, loaded and ready.
pub struct Wav2Vec2 {
    plan: std::sync::Arc<TypedRunnableModel>,
}

impl Wav2Vec2 {
    /// Loads the model from its ONNX file (16-bit weights worked out at 32-bit).
    pub fn load(path: &Path) -> Result<Self, AlignError> {
        let failed = |e: TractError| AlignError::Model(format!("{e:#}"));
        let mut model = tract_onnx::onnx()
            .model_for_path(path)
            .map_err(failed)?
            .into_typed()
            .map_err(failed)?
            .into_decluttered()
            .map_err(failed)?;
        model
            .transform(&FloatPrecisionTranslator::new(
                f16::datum_type(),
                f32::datum_type(),
            ))
            .map_err(failed)?;
        let plan = model
            .into_optimized()
            .map_err(failed)?
            .into_runnable()
            .map_err(failed)?;
        Ok(Self { plan })
    }
}

/// `samples` made zero-mean and unit-variance, as the model was trained on.
fn normalized(samples: &[f32]) -> Vec<f32> {
    let n = samples.len().max(1) as f64;
    let mean = samples.iter().map(|&x| f64::from(x)).sum::<f64>() / n;
    let var = samples
        .iter()
        .map(|&x| (f64::from(x) - mean).powi(2))
        .sum::<f64>()
        / n;
    let scale = 1.0 / (var + 1e-7).sqrt();
    samples
        .iter()
        .map(|&x| ((f64::from(x) - mean) * scale) as f32)
        .collect()
}

impl Acoustic for Wav2Vec2 {
    fn emit(&self, samples: &[f32]) -> Result<Emission, AlignError> {
        if frames_for(samples.len()) == 0 {
            return Ok(Emission::new(vocab::SIZE, Vec::new()));
        }
        let failed = |e: TractError| AlignError::Model(format!("{e:#}"));
        let input = tract_ndarray::Array2::from_shape_vec((1, samples.len()), normalized(samples))
            .map_err(|e| AlignError::Model(e.to_string()))?
            .into_tensor();
        let out = self.plan.run(tvec!(input.into())).map_err(failed)?;
        let logits = out[0].to_plain_array_view::<f32>().map_err(failed)?;
        let shape = logits.shape().to_vec();
        if shape.len() != 3 || shape[2] != vocab::SIZE {
            return Err(AlignError::Model(format!("unexpected output shape {shape:?}")));
        }
        Ok(Emission::from_logits(
            vocab::SIZE,
            logits.iter().copied().collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_follow_the_models_strides() {
        assert_eq!(frames_for(399), 0);
        assert_eq!(frames_for(400), 1);
        assert_eq!(frames_for(16_000), 49);
        assert_eq!(frames_for(320 * 100 + 80), 100);
        let x = normalized(&[1.0, 3.0, 1.0, 3.0]);
        assert!(x.iter().all(|v| (v.abs() - 1.0).abs() < 1e-3), "{x:?}");
    }
}
