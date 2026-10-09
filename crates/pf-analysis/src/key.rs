//! Key changes: the key of a stretch is the major or minor key whose profile (Krumhansl and
//! Kessler's) best matches its chroma; the key changes at a bar where the 8–32 bars after it
//! fit a key at least two sharps or flats away from the 8–32 before's clearly better than any
//! key near the old one (a relative major or minor has the same notes; a neighbouring key all
//! but one), and the bars before fit the old key clearly better than the new. A bridge that
//! wanders off and comes back isn't a change: the bars after it are still mostly in the old key.

use crate::moments::{Found, MomentKind, Song};

const MAJOR: [f32; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const MINOR: [f32; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];
const NAMES: [&str; 12] = ["C", "C♯", "D", "E♭", "E", "F", "F♯", "G", "A♭", "A", "B♭", "B"];
/// Bars compared either side of a change, at least and at most.
const FEWEST_BARS: usize = 8;
const MOST_BARS: usize = 32;
/// How well a key must fit, and how much better than the other side's.
const FIT: f32 = 0.6;
const MARGIN: f32 = 0.15;
/// Keys this many sharps or flats apart, at least: neighbouring keys share all but one note, so
/// a section leaning on the IV or V chord reads as one.
const FIFTHS_APART: usize = 2;

/// A key: its tonic (0 = C) and whether it's minor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Key {
    pub tonic: usize,
    pub minor: bool,
}

impl Key {
    pub fn name(self) -> String {
        format!("{}{}", NAMES[self.tonic], if self.minor { "m" } else { "" })
    }

    /// How many sharps or flats apart the two keys' notes are (0: the same notes, as a key and
    /// its relative major or minor; 1: neighbours, as C and G).
    fn distance(self, other: Key) -> usize {
        // Place on the circle of fifths, by the relative major.
        let fifths = |k: Key| (if k.minor { (k.tonic + 3) % 12 } else { k.tonic } * 7) % 12;
        let d = fifths(self).abs_diff(fifths(other));
        d.min(12 - d)
    }
}

fn correlation(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let mean = |v: &[f32; 12]| v.iter().sum::<f32>() / 12.0;
    let (ma, mb) = (mean(a), mean(b));
    let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
    for i in 0..12 {
        let (x, y) = (a[i] - ma, b[i] - mb);
        num += x * y;
        da += x * x;
        db += y * y;
    }
    if da * db < 1e-12 {
        0.0
    } else {
        num / (da * db).sqrt()
    }
}

/// How well `chroma` fits `key` (-1–1).
pub(crate) fn fit(chroma: &[f32; 12], key: Key) -> f32 {
    let profile = if key.minor { &MINOR } else { &MAJOR };
    let rotated: [f32; 12] = std::array::from_fn(|i| profile[(i + 12 - key.tonic) % 12]);
    correlation(chroma, &rotated)
}

/// How well `chroma` fits the best key near `key` (fewer than [`FIFTHS_APART`] sharps or flats
/// away: itself, its relative, its neighbours).
fn near_fit(chroma: &[f32; 12], key: Key) -> f32 {
    all_keys()
        .filter(|k| k.distance(key) < FIFTHS_APART)
        .map(|k| fit(chroma, k))
        .fold(f32::MIN, f32::max)
}

fn all_keys() -> impl Iterator<Item = Key> {
    (0..24).map(|k| Key {
        tonic: k % 12,
        minor: k >= 12,
    })
}

/// The key `chroma` fits best, and how well.
pub(crate) fn best_key(chroma: &[f32; 12]) -> (Key, f32) {
    all_keys()
        .map(|key| (key, fit(chroma, key)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((
            Key {
                tonic: 0,
                minor: false,
            },
            0.0,
        ))
}

/// Key changes: wherever the bars either side are clearly in different keys, the clearest of
/// those close together, moved onto a section start within two bars.
pub(crate) fn key_changes(song: &Song) -> Vec<Found> {
    let f = song.features;
    let bars = song.bar_starts();
    let n = bars.len();
    if n < 2 * FEWEST_BARS || f.is_empty() {
        return Vec::new();
    }
    let chroma: Vec<[f32; 12]> = (0..n)
        .map(|j| {
            let (a, b) = (bars[j], bars.get(j + 1).copied().unwrap_or(song.end));
            let (x, y) = (f.frame_at(a), f.frame_at(b).max(f.frame_at(a) + 1).min(f.len()));
            let mut sum = [0.0f32; 12];
            for c in &f.chroma[x..y] {
                sum.iter_mut().zip(c).for_each(|(s, v)| *s += v);
            }
            sum
        })
        .collect();
    let total = |from: usize, to: usize| {
        let mut sum = [0.0f32; 12];
        for c in &chroma[from..to] {
            sum.iter_mut().zip(c).for_each(|(s, v)| *s += v);
        }
        sum
    };
    let starts: Vec<usize> = song
        .sections
        .iter()
        .skip(1)
        .map(|s| bars.partition_point(|&t| t < s.start_ms as f64 / 1000.0 - 0.1))
        .collect();
    let mut found: Vec<(usize, f32, Key, Key)> = Vec::new();
    for c in FEWEST_BARS..=n - FEWEST_BARS {
        let before = total(c.saturating_sub(MOST_BARS), c);
        let after = total(c, (c + MOST_BARS).min(n));
        let (old, old_fit) = best_key(&before);
        let (new, new_fit) = best_key(&after);
        if old.distance(new) < FIFTHS_APART || old_fit < FIT || new_fit < FIT {
            continue;
        }
        let margin_after = new_fit - near_fit(&after, old);
        let margin_before = old_fit - fit(&before, new);
        if margin_after < MARGIN || margin_before < MARGIN {
            continue;
        }
        found.push((c, margin_after + margin_before, old, new));
    }
    // The clearest of those within 8 bars of each other.
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut kept: Vec<(usize, f32, Key, Key)> = Vec::new();
    for c in found {
        if kept.iter().all(|k| k.0.abs_diff(c.0) >= FEWEST_BARS) {
            kept.push(c);
        }
    }
    kept.into_iter()
        .map(|(c, score, old, new)| {
            // Exactly where: the bar from which the bars lean to the new key rather than the old
            // (the latest, where a chord in both keys leaves it open), then the section start
            // within two bars, if there is one.
            let lean: Vec<f32> = chroma.iter().map(|b| fit(b, old) - fit(b, new)).collect();
            let split = |k: usize| {
                lean[k.saturating_sub(FEWEST_BARS)..k].iter().sum::<f32>()
                    - lean[k..(k + FEWEST_BARS).min(n)].iter().sum::<f32>()
            };
            let c = (c.saturating_sub(4).max(1)..=(c + 4).min(n - 1))
                .fold(c, |best, k| if split(k) >= split(best) { k } else { best });
            let c = starts
                .iter()
                .copied()
                .filter(|s| s.abs_diff(c) <= 2)
                .min_by_key(|s| s.abs_diff(c))
                .unwrap_or(c);
            Found::new(MomentKind::KeyChange, bars[c], score / 0.6).labeled(format!(
                "{}→{}",
                old.name(),
                new.name()
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scale_names_its_key() {
        // The C major scale's notes, the tonic and fifth strongest.
        let mut c = [0.0f32; 12];
        for (pc, w) in [
            (0, 3.0),
            (2, 1.0),
            (4, 2.0),
            (5, 1.0),
            (7, 2.5),
            (9, 1.0),
            (11, 1.0),
        ] {
            c[pc] = w;
        }
        let (key, fit) = best_key(&c);
        assert_eq!(
            key,
            Key {
                tonic: 0,
                minor: false
            }
        );
        assert!(fit > 0.7);
        let d: [f32; 12] = std::array::from_fn(|i| c[(i + 10) % 12]);
        assert_eq!(best_key(&d).0.name(), "D");
        let key = |tonic, minor| Key { tonic, minor };
        assert_eq!(
            key(9, true).distance(key(0, false)),
            0,
            "A minor is the relative of C major"
        );
        assert_eq!(key(0, false).distance(key(7, false)), 1);
        assert_eq!(key(0, false).distance(key(2, false)), 2);
        assert_eq!(key(0, false).distance(key(1, false)), 5);
    }
}
