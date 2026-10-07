//! Finding lit pixels (blobs) in a brightness map, to sub-pixel accuracy.

/// A single-channel map, row by row from the top left.
#[derive(Debug, Clone)]
pub(crate) struct Gray {
    pub width: usize,
    pub height: usize,
    pub v: Vec<f32>,
}

impl Gray {
    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.v[y * self.width + x]
    }

    /// 3 × 3 box blur (edges use the pixels that exist).
    fn smoothed(&self) -> Gray {
        let (w, h) = (self.width, self.height);
        let mut out = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                let (mut sum, mut n) = (0.0, 0.0);
                for yy in y.saturating_sub(1)..(y + 2).min(h) {
                    for xx in x.saturating_sub(1)..(x + 2).min(w) {
                        sum += self.at(xx, yy);
                        n += 1.0;
                    }
                }
                out[y * w + x] = sum / n;
            }
        }
        Gray {
            width: w,
            height: h,
            v: out,
        }
    }
}

/// A lit spot: its intensity-weighted centre, peak, and the pixels it covers with their weights.
#[derive(Debug, Clone)]
pub(crate) struct Blob {
    pub x: f64,
    pub y: f64,
    pub peak: f32,
    /// (pixel index, weight) pairs.
    pub support: Vec<(usize, f32)>,
}

/// Neighbouring maxima closer than this (pixels) are one blob.
const SUPPRESS: usize = 2;
/// How far from its peak a blob's centre is measured.
const REACH: usize = 5;
/// A blob's support: pixels brighter than this fraction of its peak.
const FLOOR: f32 = 0.3;

/// The noise floor of a map: median plus 8 robust standard deviations (MAD), at least `min`.
pub(crate) fn noise_threshold(map: &Gray, min: f32) -> f32 {
    let mut sample: Vec<f32> = map
        .v
        .iter()
        .step_by((map.v.len() / 50_000).max(1))
        .copied()
        .collect();
    if sample.is_empty() {
        return min;
    }
    let mid = sample.len() / 2;
    let median = *sample.select_nth_unstable_by(mid, f32::total_cmp).1;
    let mut dev: Vec<f32> = sample.iter().map(|v| (v - median).abs()).collect();
    let mad = *dev.select_nth_unstable_by(mid, f32::total_cmp).1;
    (median + 8.0 * 1.4826 * mad).max(min)
}

/// Finds blobs in `map` brighter than `threshold`: local maxima of the smoothed map, merged when
/// two belong to one bright patch, each measured by its weighted centroid over the pixels nearer
/// to it than to any other blob.
pub(crate) fn find_blobs(map: &Gray, threshold: f32) -> Vec<Blob> {
    let (w, h) = (map.width, map.height);
    let smooth = map.smoothed();
    let mut peaks: Vec<(usize, usize, f32)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let v = smooth.at(x, y);
            if v <= threshold {
                continue;
            }
            let i = y * w + x;
            let mut is_max = true;
            'window: for yy in y.saturating_sub(SUPPRESS)..(y + SUPPRESS + 1).min(h) {
                for xx in x.saturating_sub(SUPPRESS)..(x + SUPPRESS + 1).min(w) {
                    let (o, j) = (smooth.at(xx, yy), yy * w + xx);
                    // Ties go to the first pixel, so a flat (saturated) top gives one maximum.
                    if o > v || (o == v && j < i) {
                        is_max = false;
                        break 'window;
                    }
                }
            }
            if is_max {
                peaks.push((x, y, v));
            }
        }
    }
    let peaks = merge_plateaus(&smooth, peaks);
    // Measure each blob around its peak, then again around that centre (a flat-topped blob's
    // first maximum is at the top left of its top).
    let mut blobs = measure(map, &peaks, &peaks.iter().map(|p| (p.0, p.1)).collect::<Vec<_>>());
    for _ in 0..2 {
        let centres: Vec<(usize, usize)> = blobs
            .iter()
            .zip(&peaks)
            .map(|(b, p)| match b {
                Some(b) => (
                    (b.x.round().max(0.0) as usize).min(w - 1),
                    (b.y.round().max(0.0) as usize).min(h - 1),
                ),
                None => (p.0, p.1),
            })
            .collect();
        blobs = measure(map, &peaks, &centres);
    }
    blobs.into_iter().flatten().collect()
}

/// Each blob's weighted centroid over the pixels within reach of its centre that are nearer to
/// it than to any other centre and brighter than [`FLOOR`] of its peak.
fn measure(map: &Gray, peaks: &[(usize, usize, f32)], centres: &[(usize, usize)]) -> Vec<Option<Blob>> {
    let (w, h) = (map.width, map.height);
    let mut owner: Vec<(usize, usize)> = vec![(usize::MAX, usize::MAX); w * h];
    for (k, &(px, py)) in centres.iter().enumerate() {
        for y in py.saturating_sub(REACH)..(py + REACH + 1).min(h) {
            for x in px.saturating_sub(REACH)..(px + REACH + 1).min(w) {
                let d = x.abs_diff(px).pow(2) + y.abs_diff(py).pow(2);
                let slot = &mut owner[y * w + x];
                if d < slot.1 {
                    *slot = (k, d);
                }
            }
        }
    }
    centres
        .iter()
        .zip(peaks)
        .enumerate()
        .map(|(k, (&(px, py), &(_, _, peak)))| {
            let floor = FLOOR * peak;
            let mut support = Vec::new();
            let (mut sx, mut sy, mut sw) = (0.0f64, 0.0f64, 0.0f64);
            for y in py.saturating_sub(REACH)..(py + REACH + 1).min(h) {
                for x in px.saturating_sub(REACH)..(px + REACH + 1).min(w) {
                    let wgt = map.at(x, y) - floor;
                    if wgt <= 0.0 || owner[y * w + x].0 != k {
                        continue;
                    }
                    support.push((y * w + x, wgt));
                    sx += x as f64 * f64::from(wgt);
                    sy += y as f64 * f64::from(wgt);
                    sw += f64::from(wgt);
                }
            }
            (sw > 0.0).then(|| Blob {
                x: sx / sw,
                y: sy / sw,
                peak,
                support,
            })
        })
        .collect()
}

/// Drops a maximum that joins a brighter one without a dip between them (one patch, e.g. a
/// saturated or blurred pixel with a flat top).
fn merge_plateaus(smooth: &Gray, mut peaks: Vec<(usize, usize, f32)>) -> Vec<(usize, usize, f32)> {
    peaks.sort_by(|a, b| b.2.total_cmp(&a.2));
    let mut kept: Vec<(usize, usize, f32)> = Vec::new();
    for p in peaks {
        let joined = kept.iter().any(|k| {
            let (dx, dy) = (k.0 as f64 - p.0 as f64, k.1 as f64 - p.1 as f64);
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > (3 * REACH) as f64 {
                return false;
            }
            let steps = dist.ceil().max(1.0) as usize;
            let lowest = (0..=steps)
                .map(|s| {
                    let f = s as f64 / steps as f64;
                    let x = (p.0 as f64 + dx * f).round() as usize;
                    let y = (p.1 as f64 + dy * f).round() as usize;
                    smooth.at(x, y)
                })
                .fold(f32::MAX, f32::min);
            lowest >= 0.85 * p.2
        });
        if !joined {
            kept.push(p);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spots(width: usize, height: usize, at: &[(f64, f64, f32, f64)]) -> Gray {
        let mut v = vec![0.0; width * height];
        for y in 0..height {
            for x in 0..width {
                for &(cx, cy, peak, sigma) in at {
                    let d2 = (x as f64 - cx).powi(2) + (y as f64 - cy).powi(2);
                    v[y * width + x] += peak * (-d2 / (2.0 * sigma * sigma)).exp() as f32;
                }
            }
        }
        Gray { width, height, v }
    }

    #[test]
    fn finds_separate_spots_to_sub_pixel_accuracy() {
        let truth = [
            (10.3, 12.7, 200.0, 1.5),
            (30.6, 12.1, 80.0, 2.0),
            (20.0, 30.45, 150.0, 1.2),
        ];
        let blobs = find_blobs(&spots(48, 40, &truth), 10.0);
        assert_eq!(blobs.len(), 3);
        for (tx, ty, _, _) in truth {
            let b = blobs
                .iter()
                .min_by(|a, b| {
                    ((a.x - tx).abs() + (a.y - ty).abs()).total_cmp(&((b.x - tx).abs() + (b.y - ty).abs()))
                })
                .unwrap();
            assert!(
                (b.x - tx).abs() < 0.1 && (b.y - ty).abs() < 0.1,
                "{tx},{ty} -> {},{}",
                b.x,
                b.y
            );
        }
    }

    #[test]
    fn a_flat_saturated_top_is_one_blob() {
        let mut map = spots(40, 40, &[(20.0, 20.0, 600.0, 3.0)]);
        map.v.iter_mut().for_each(|v| *v = v.min(255.0));
        let blobs = find_blobs(&map, 10.0);
        assert_eq!(blobs.len(), 1);
        assert!((blobs[0].x - 20.0).abs() < 0.05 && (blobs[0].y - 20.0).abs() < 0.05);
    }

    #[test]
    fn close_spots_with_a_dip_between_stay_apart() {
        let blobs = find_blobs(
            &spots(40, 20, &[(15.0, 10.0, 200.0, 1.2), (21.0, 10.0, 200.0, 1.2)]),
            10.0,
        );
        assert_eq!(blobs.len(), 2);
        let mut xs: Vec<f64> = blobs.iter().map(|b| b.x).collect();
        xs.sort_by(f64::total_cmp);
        assert!((xs[0] - 15.0).abs() < 0.2 && (xs[1] - 21.0).abs() < 0.2, "{xs:?}");
    }

    #[test]
    fn noise_threshold_sits_above_the_noise() {
        let v: Vec<f32> = (0..10_000).map(|i| ((i * 7919) % 11) as f32 - 5.0).collect();
        let t = noise_threshold(
            &Gray {
                width: 100,
                height: 100,
                v,
            },
            3.0,
        );
        assert!(t > 5.0 && t < 60.0, "{t}");
    }
}
