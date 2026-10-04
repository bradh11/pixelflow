//! Brightness and gamma lookup tables.

/// A 256-entry table mapping an 8-bit channel value through gamma, then brightness.
///
/// `brightness` is a percentage (values above 100 are treated as 100). A gamma that is
/// not a positive finite number is treated as 1.0.
pub fn build_lut(brightness: u8, gamma: f32) -> [u8; 256] {
    let scale = f32::from(brightness.min(100)) / 100.0;
    let gamma = if gamma.is_finite() && gamma > 0.0 {
        gamma
    } else {
        1.0
    };
    let mut lut = [0u8; 256];
    for (value, out) in lut.iter_mut().enumerate() {
        let x = value as f32 / 255.0;
        *out = (x.powf(gamma) * scale * 255.0).round() as u8;
    }
    lut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_brightness_linear_gamma_is_identity() {
        let lut = build_lut(100, 1.0);
        assert!(lut.iter().enumerate().all(|(i, &v)| v as usize == i));
    }

    #[test]
    fn brightness_scales_and_gamma_curves() {
        assert_eq!(build_lut(50, 1.0)[255], 128);
        assert_eq!(build_lut(0, 1.0)[255], 0);
        assert_eq!(build_lut(100, 2.2)[128], 56);
        assert_eq!(build_lut(100, 2.2)[255], 255);
    }

    #[test]
    fn invalid_gamma_and_brightness_are_clamped() {
        assert_eq!(build_lut(100, f32::NAN), build_lut(100, 1.0));
        assert_eq!(build_lut(100, -1.0), build_lut(100, 1.0));
        assert_eq!(build_lut(250, 1.0), build_lut(100, 1.0));
    }
}
