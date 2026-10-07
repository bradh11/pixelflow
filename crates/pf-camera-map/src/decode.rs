//! Reading pixel numbers from one averaged image per slot.

use crate::code::{Base, CodeSpec, Slot};
use crate::detect::{Gray, find_blobs, noise_threshold};
use crate::image::{Image, ImageError};
use serde::{Deserialize, Serialize};

/// A pixel found and read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    /// The pixel's place in the sequence (wiring order across the target).
    pub index: u32,
    /// Its centre in the images, in pixels from the top left.
    pub x: f64,
    pub y: f64,
    /// How clearly every slot read, 0–1.
    pub confidence: f32,
    /// Its peak brightness when white, above the background (0–255).
    pub brightness: f32,
    /// The colour the camera saw when the pixel was sent red, green, and blue (0 red, 1 green,
    /// 2 blue): `[0, 1, 2]` when its colour order is right. `None` when unclear.
    pub seen: Option<[u8; 3]>,
}

/// A lit spot that didn't read as a pixel number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unreadable {
    pub x: f64,
    pub y: f64,
    pub brightness: f32,
}

/// What a capture decoded to.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decoded {
    pub width: usize,
    pub height: usize,
    /// One per pixel number read, in pixel order.
    pub pixels: Vec<Found>,
    /// Further spots that read as a number already found (usually reflections).
    pub duplicates: Vec<Found>,
    pub unreadable: Vec<Unreadable>,
}

/// Background brightness changes smaller than this (0–255) are never taken for a pixel.
const MIN_LIGHT: f32 = 12.0;
/// A slot's frames may be shifted by up to this many pixels (a hand-held or bumped camera).
const MAX_SHIFT: i32 = 3;
/// Readings less clear than this aren't trusted.
const MIN_CONFIDENCE: f32 = 0.15;

/// Finds the pixels in `images` (one per slot of `spec`, in slot order, all one size) and reads
/// each one's number.
pub fn decode(images: &[Image], spec: &CodeSpec) -> Result<Decoded, ImageError> {
    let slots = spec.slots();
    if images.len() != slots.len() {
        return Err(ImageError(format!(
            "Expected {} slot images, got {}.",
            slots.len(),
            images.len()
        )));
    }
    let (width, height) = (images[0].width, images[0].height);
    if images
        .iter()
        .any(|i| i.width != width || i.height != height || i.rgb.len() != width * height * 3)
    {
        return Err(ImageError("The slot images differ in size.".into()));
    }
    let of =
        |kind: fn(&Slot) -> bool| -> Vec<usize> { (0..slots.len()).filter(|&k| kind(&slots[k])).collect() };
    let darks = of(|s| matches!(s, Slot::Dark));
    let whites = of(|s| matches!(s, Slot::White));

    let background = Image::mean(&darks.iter().map(|&k| &images[k]).collect::<Vec<_>>())
        .ok_or_else(|| ImageError("No dark slots.".into()))?;
    let lit: Vec<Image> = images.iter().map(|i| i.minus(&background)).collect();

    // Line every slot up with the first white one (the camera may have moved a little).
    let anchor = luma(&lit[whites[0]]);
    let rough = find_blobs(&anchor, noise_threshold(&anchor, MIN_LIGHT));
    let aligned: Vec<Image> = lit
        .iter()
        .enumerate()
        .map(|(k, image)| {
            if matches!(slots[k], Slot::Dark) || k == whites[0] {
                return image.clone();
            }
            let (dx, dy) = best_shift(&rough, &luma(image));
            shifted(image, dx, dy)
        })
        .collect();

    let white = Image::mean(&whites.iter().map(|&k| &aligned[k]).collect::<Vec<_>>())
        .ok_or_else(|| ImageError("No white slots.".into()))?;
    let white_luma = luma(&white);
    let blobs = find_blobs(&white_luma, noise_threshold(&white_luma, MIN_LIGHT));

    let mut read: Vec<Found> = Vec::new();
    let mut unreadable = Vec::new();
    for blob in &blobs {
        let colour = |image: &Image| -> [f32; 3] {
            let (mut sum, mut total) = ([0.0f32; 3], 0.0f32);
            for &(i, w) in &blob.support {
                for (c, s) in sum.iter_mut().enumerate() {
                    *s += image.rgb[i * 3 + c] * w;
                }
                total += w;
            }
            sum.map(|s| s / total.max(1e-6))
        };
        let white_seen = colour(&white);
        let reference: Vec<[f32; 3]> = (0..3u8)
            .map(|c| colour(&aligned[slots.iter().position(|s| *s == Slot::Reference(c)).unwrap_or(0)]))
            .collect();
        let templates: Vec<[f32; 3]> = match spec.base {
            Base::Four => vec![[0.0; 3], reference[0], reference[1], reference[2]],
            Base::Two => vec![[0.0; 3], white_seen],
        };
        let scale = norm(white_seen).max(1.0);
        let mut code = Vec::new();
        let mut confidence = 1.0f32;
        for (k, slot) in slots.iter().enumerate() {
            if matches!(slot, Slot::Digit(_) | Slot::Check(_)) {
                let (digit, clarity) = classify(colour(&aligned[k]), &templates, scale);
                code.push(digit);
                confidence = confidence.min(clarity);
            }
        }
        match spec.read(&code) {
            Some(index) if confidence >= MIN_CONFIDENCE => read.push(Found {
                index,
                x: blob.x,
                y: blob.y,
                confidence,
                brightness: blob.peak,
                seen: seen_order(&reference, white_seen),
            }),
            _ => unreadable.push(Unreadable {
                x: blob.x,
                y: blob.y,
                brightness: blob.peak,
            }),
        }
    }

    // One spot per number: the brightest; the others are most likely reflections.
    read.sort_by(|a, b| a.index.cmp(&b.index).then(b.brightness.total_cmp(&a.brightness)));
    let mut pixels: Vec<Found> = Vec::new();
    let mut duplicates = Vec::new();
    for found in read {
        if pixels.last().is_some_and(|p| p.index == found.index) {
            duplicates.push(found);
        } else {
            pixels.push(found);
        }
    }
    Ok(Decoded {
        width,
        height,
        pixels,
        duplicates,
        unreadable,
    })
}

/// The brightest channel of each pixel.
fn luma(image: &Image) -> Gray {
    Gray {
        width: image.width,
        height: image.height,
        v: image
            .rgb
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| p[0].max(p[1]).max(p[2]))
            .collect(),
    }
}

/// The shift (within ±[`MAX_SHIFT`]) that best lines `map` up with the blobs found in the anchor.
fn best_shift(blobs: &[crate::detect::Blob], map: &Gray) -> (i32, i32) {
    let (w, h) = (map.width as i32, map.height as i32);
    let mut best = ((0, 0), f32::MIN);
    for dy in -MAX_SHIFT..=MAX_SHIFT {
        for dx in -MAX_SHIFT..=MAX_SHIFT {
            let mut score = 0.0;
            for blob in blobs {
                for &(i, wgt) in &blob.support {
                    let (x, y) = ((i % map.width) as i32 + dx, (i / map.width) as i32 + dy);
                    if x >= 0 && y >= 0 && x < w && y < h {
                        score += wgt * map.v[(y * w + x) as usize];
                    }
                }
            }
            // Prefer no shift on a tie (an all-dark slot).
            let score = score - 1e-3 * (dx.abs() + dy.abs()) as f32;
            if score > best.1 {
                best = ((dx, dy), score);
            }
        }
    }
    best.0
}

/// `image` moved by (-`dx`, -`dy`), so what was at (x + dx, y + dy) is now at (x, y).
fn shifted(image: &Image, dx: i32, dy: i32) -> Image {
    if dx == 0 && dy == 0 {
        return image.clone();
    }
    let (w, h) = (image.width as i32, image.height as i32);
    let mut out = Image::new(image.width, image.height);
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = (x + dx, y + dy);
            if sx >= 0 && sy >= 0 && sx < w && sy < h {
                let (from, to) = (((sy * w + sx) * 3) as usize, ((y * w + x) * 3) as usize);
                out.rgb[to..to + 3].copy_from_slice(&image.rgb[from..from + 3]);
            }
        }
    }
    out
}

fn norm(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// The template `v` matches best (allowing for brightness changing between slots, as a phone's
/// exposure does), and how clearly: 0 when two match equally, 1 when only one could.
fn classify(v: [f32; 3], templates: &[[f32; 3]], scale: f32) -> (u8, f32) {
    let mut errors: Vec<(u8, f32)> = templates
        .iter()
        .enumerate()
        .map(|(d, t)| {
            let tt = t[0] * t[0] + t[1] * t[1] + t[2] * t[2];
            let a = if tt > 1e-6 {
                ((v[0] * t[0] + v[1] * t[1] + v[2] * t[2]) / tt).clamp(0.35, 2.5)
            } else {
                0.0
            };
            let error = norm([v[0] - a * t[0], v[1] - a * t[1], v[2] - a * t[2]]);
            (d as u8, error / scale)
        })
        .collect();
    errors.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (best, second) = (errors[0], errors[1]);
    (best.0, ((second.1 - best.1) / (second.1 + 0.05)).clamp(0.0, 1.0))
}

/// Which camera colour each reference (red, green, blue) looked most like, relative to how the
/// camera sees the pixel's white; `None` unless that's each colour once.
fn seen_order(reference: &[[f32; 3]], white: [f32; 3]) -> Option<[u8; 3]> {
    let mut seen = [0u8; 3];
    for (c, r) in reference.iter().enumerate() {
        let balanced: Vec<f32> = (0..3).map(|k| r[k] / white[k].max(1.0)).collect();
        let (top, value) = balanced
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(k, v)| (k as u8, *v))?;
        if value <= 0.05 {
            return None;
        }
        seen[c] = top;
    }
    let mut sorted = seen;
    sorted.sort_unstable();
    (sorted == [0, 1, 2]).then_some(seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_shape_not_brightness() {
        let templates = [
            [0.0; 3],
            [200.0, 30.0, 10.0],
            [40.0, 220.0, 80.0],
            [10.0, 50.0, 230.0],
        ];
        assert_eq!(classify([100.0, 15.0, 5.0], &templates, 250.0).0, 1);
        assert_eq!(classify([60.0, 300.0, 110.0], &templates, 250.0).0, 2);
        assert_eq!(classify([3.0, 2.0, 4.0], &templates, 250.0).0, 0);
        let (digit, clarity) = classify([10.0, 50.0, 230.0], &templates, 250.0);
        assert_eq!(digit, 3);
        assert!(clarity > 0.8);
    }

    #[test]
    fn seen_order_spots_swapped_colours() {
        let white = [200.0, 200.0, 200.0];
        let right = [[200.0, 20.0, 5.0], [20.0, 210.0, 30.0], [5.0, 30.0, 190.0]];
        assert_eq!(seen_order(&right, white), Some([0, 1, 2]));
        // Wired GRB but set up as RGB: red shows green and green shows red.
        let swapped = [right[1], right[0], right[2]];
        assert_eq!(seen_order(&swapped, white), Some([1, 0, 2]));
        assert_eq!(seen_order(&[right[0], right[0], right[2]], white), None);
    }

    #[test]
    fn wrong_number_of_slots_is_an_error() {
        let spec = CodeSpec::new(10, Base::Four);
        assert!(decode(&[Image::new(4, 4)], &spec).is_err());
    }
}
