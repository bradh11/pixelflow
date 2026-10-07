//! Lining up 2D point sets: scale, rotation, and translation (no mirroring).

use serde::{Deserialize, Serialize};

/// `p ↦ scale · R(angle) · p + (tx, ty)`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Similarity {
    pub scale: f64,
    /// Radians, counter-clockwise.
    pub angle: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Similarity {
    pub fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        let (s, c) = self.angle.sin_cos();
        [
            self.scale * (c * p[0] - s * p[1]) + self.tx,
            self.scale * (s * p[0] + c * p[1]) + self.ty,
        ]
    }

    /// The least-squares similarity taking `from` onto `to` (pairs in order). `None` with fewer
    /// than two pairs or when `from` is a single point.
    pub fn fit(from: &[[f64; 2]], to: &[[f64; 2]]) -> Option<Similarity> {
        let n = from.len().min(to.len());
        if n < 2 {
            return None;
        }
        let centre = |ps: &[[f64; 2]]| {
            let (x, y) = ps[..n].iter().fold((0.0, 0.0), |(x, y), p| (x + p[0], y + p[1]));
            [x / n as f64, y / n as f64]
        };
        let (mf, mt) = (centre(from), centre(to));
        let (mut a, mut b, mut spread) = (0.0, 0.0, 0.0);
        for (f, t) in from.iter().zip(to).take(n) {
            let (fx, fy, tx, ty) = (f[0] - mf[0], f[1] - mf[1], t[0] - mt[0], t[1] - mt[1]);
            a += fx * tx + fy * ty;
            b += fx * ty - fy * tx;
            spread += fx * fx + fy * fy;
        }
        if spread < 1e-12 {
            return None;
        }
        let angle = b.atan2(a);
        let scale = (a * a + b * b).sqrt() / spread;
        let (s, c) = angle.sin_cos();
        Some(Similarity {
            scale,
            angle,
            tx: mt[0] - scale * (c * mf[0] - s * mf[1]),
            ty: mt[1] - scale * (s * mf[0] + c * mf[1]),
        })
    }

    /// Like [`Similarity::fit`], then again without the pairs that fit far worse than the rest
    /// (a misplaced prop in the layout, a misread pixel), twice.
    pub fn fit_robust(from: &[[f64; 2]], to: &[[f64; 2]]) -> Option<Similarity> {
        let mut keep: Vec<usize> = (0..from.len().min(to.len())).collect();
        let mut fit = Similarity::fit(from, to)?;
        for _ in 0..2 {
            let errors: Vec<f64> = keep
                .iter()
                .map(|&i| distance(fit.apply(from[i]), to[i]))
                .collect();
            let mut sorted = errors.clone();
            sorted.sort_by(f64::total_cmp);
            let limit = (3.0 * sorted[sorted.len() / 2]).max(1e-9);
            let next: Vec<usize> = keep
                .iter()
                .zip(&errors)
                .filter(|(_, e)| **e <= limit)
                .map(|(i, _)| *i)
                .collect();
            if next.len() < 2 || next.len() == keep.len() {
                break;
            }
            keep = next;
            let (f, t): (Vec<[f64; 2]>, Vec<[f64; 2]>) = keep.iter().map(|&i| (from[i], to[i])).unzip();
            fit = Similarity::fit(&f, &t)?;
        }
        Some(fit)
    }

    /// Root-mean-square distance between `to` and `from` moved by this.
    pub fn rms(&self, from: &[[f64; 2]], to: &[[f64; 2]]) -> f64 {
        let n = from.len().min(to.len());
        if n == 0 {
            return 0.0;
        }
        let sum: f64 = from
            .iter()
            .zip(to)
            .map(|(f, t)| distance(self.apply(*f), *t).powi(2))
            .sum();
        (sum / n as f64).sqrt()
    }
}

pub(crate) fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KNOWN: Similarity = Similarity {
        scale: 0.02,
        angle: 0.1,
        tx: -5.0,
        ty: 3.0,
    };

    #[test]
    fn two_points_pin_down_a_similarity() {
        let from = [[100.0, -200.0], [400.0, -250.0]];
        let to = from.map(|p| KNOWN.apply(p));
        let fit = Similarity::fit(&from, &to).unwrap();
        assert!((fit.scale - 0.02).abs() < 1e-9 && (fit.angle - 0.1).abs() < 1e-9);
        assert!(fit.rms(&from, &to) < 1e-9);
        assert!(Similarity::fit(&from[..1], &to[..1]).is_none());
        assert!(Similarity::fit(&[[1.0, 1.0], [1.0, 1.0]], &to).is_none());
    }

    #[test]
    fn robust_fit_ignores_a_few_bad_pairs() {
        let from: Vec<[f64; 2]> = (0..40)
            .map(|i| [f64::from(i) * 13.0, f64::from(i % 7) * -29.0])
            .collect();
        let mut to: Vec<[f64; 2]> = from.iter().map(|p| KNOWN.apply(*p)).collect();
        to[3] = [50.0, 50.0];
        to[17] = [-40.0, 9.0];
        let fit = Similarity::fit_robust(&from, &to).unwrap();
        assert!(
            (fit.scale - 0.02).abs() < 1e-6 && (fit.angle - 0.1).abs() < 1e-6,
            "{fit:?}"
        );
    }
}
