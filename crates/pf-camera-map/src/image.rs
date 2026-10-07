//! Frames as the decoder sees them: linear RGB floats, row by row from the top left.

use std::fmt;

/// An RGB image (0–255 per channel, as floats so averaged frames keep their precision).
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    /// `width * height * 3` values, row by row from the top left.
    pub rgb: Vec<f32>,
}

/// Why raw frame bytes couldn't be read as images.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageError(pub String);

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ImageError {}

/// The largest frame the decoder accepts (4K); the window sends smaller ones.
const MAX_PIXELS: usize = 3840 * 2160;

impl Image {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            rgb: vec![0.0; width * height * 3],
        }
    }

    /// Splits `bytes` (RGB, 3 bytes per pixel) into `count` images of `width` × `height`.
    pub fn split_rgb8(
        bytes: &[u8],
        width: usize,
        height: usize,
        count: usize,
    ) -> Result<Vec<Image>, ImageError> {
        if width == 0 || height == 0 || width.saturating_mul(height) > MAX_PIXELS {
            return Err(ImageError(format!(
                "Frames of {width} × {height} can't be decoded."
            )));
        }
        let each = width * height * 3;
        if bytes.len() != each.saturating_mul(count) {
            return Err(ImageError(format!(
                "Expected {count} frames of {width} × {height}, got {} bytes.",
                bytes.len()
            )));
        }
        Ok(bytes
            .chunks_exact(each)
            .map(|chunk| Image {
                width,
                height,
                rgb: chunk.iter().map(|&b| f32::from(b)).collect(),
            })
            .collect())
    }

    /// The colour at (`x`, `y`), which must be inside the image.
    pub fn at(&self, x: usize, y: usize) -> [f32; 3] {
        let i = (y * self.width + x) * 3;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }

    /// The per-pixel mean of `images` (all the same size), or `None` when there are none.
    pub fn mean(images: &[&Image]) -> Option<Image> {
        let first = images.first()?;
        let mut out = Image::new(first.width, first.height);
        for image in images {
            for (o, v) in out.rgb.iter_mut().zip(&image.rgb) {
                *o += v;
            }
        }
        let n = images.len() as f32;
        out.rgb.iter_mut().for_each(|v| *v /= n);
        Some(out)
    }

    /// `self - other`, per channel (may go negative: noise around the background).
    pub fn minus(&self, other: &Image) -> Image {
        Image {
            width: self.width,
            height: self.height,
            rgb: self.rgb.iter().zip(&other.rgb).map(|(a, b)| a - b).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_raw_bytes_into_frames() {
        let bytes: Vec<u8> = (0..24).collect();
        let images = Image::split_rgb8(&bytes, 2, 2, 2).unwrap();
        assert_eq!(images.len(), 2);
        assert_eq!(images[1].at(1, 1), [21.0, 22.0, 23.0]);
        assert!(Image::split_rgb8(&bytes, 2, 2, 3).is_err());
        assert!(Image::split_rgb8(&bytes, 0, 2, 1).is_err());
    }

    #[test]
    fn mean_and_difference() {
        let a = Image {
            width: 1,
            height: 1,
            rgb: vec![10.0, 20.0, 30.0],
        };
        let b = Image {
            width: 1,
            height: 1,
            rgb: vec![30.0, 20.0, 10.0],
        };
        assert_eq!(Image::mean(&[&a, &b]).unwrap().rgb, vec![20.0, 20.0, 20.0]);
        assert_eq!(a.minus(&b).rgb, vec![-20.0, 0.0, 20.0]);
        assert!(Image::mean(&[]).is_none());
    }
}
