//! The measurements: how far apart two drawings of the same pixels are.
//!
//! Each pixel of each frame becomes a [`Sample`] (worked out once), which is added to the
//! [`Stats`] of everything it belongs to: its prop, and every effect lit on it at that moment.
//! [`Stats::scores`] turns the sums into the numbers the scorecard shows.

use serde::Serialize;

/// PixelFlow's side of a pair.
pub const PIXELFLOW: usize = 0;
/// xLights' side of a pair.
pub const XLIGHTS: usize = 1;

/// Bins of the color histogram: four levels each of red, green, and blue.
const BINS: usize = 64;

/// Where the lines are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    /// A pixel matches when every channel is within this much (of 255).
    pub tolerance: u8,
    /// A pixel is lit when its brightest channel is above this.
    pub lit: u8,
    /// Hue is compared where both pixels are lit and each has at least this much color
    /// (brightest channel minus dimmest): a grey pixel's hue means nothing.
    pub chroma: u8,
    /// How long the windows are that distributions are compared over, in milliseconds.
    pub window_ms: u32,
    /// A prop the reference leaves dark for at least this long (milliseconds) while PixelFlow
    /// lights it most of that time is taken as not rendered there, and left out (0: never).
    /// Props it leaves dark that way to the end of the file are left out from the moment the
    /// first of them is missing light: the file was saved before they were rendered.
    pub dark_run_ms: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            tolerance: 16,
            lit: 24,
            chroma: 32,
            window_ms: 1000,
            dark_run_ms: 20_000,
        }
    }
}

/// A color's hue in degrees (0 for greys).
fn hue(rgb: [u8; 3]) -> f32 {
    let [r, g, b] = rgb.map(f32::from);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let chroma = max - min;
    if chroma <= 0.0 {
        return 0.0;
    }
    let sixth = if max == r {
        ((g - b) / chroma).rem_euclid(6.0)
    } else if max == g {
        (b - r) / chroma + 2.0
    } else {
        (r - g) / chroma + 4.0
    };
    sixth * 60.0
}

/// One pixel in one frame, as PixelFlow and xLights each drew it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sample {
    /// |difference| summed over red, green, and blue.
    abs: u16,
    /// Difference squared, summed likewise.
    sq: u32,
    within: bool,
    lit: [bool; 2],
    /// Brightness: the brightest channel.
    level: [u8; 2],
    /// Histogram bin of each color.
    bin: [u8; 2],
    /// Degrees between the two hues, where both have one.
    hue: Option<f32>,
}

impl Sample {
    pub fn of(pixelflow: [u8; 3], xlights: [u8; 3], limits: &Limits) -> Self {
        let mut sample = Sample {
            within: true,
            ..Sample::default()
        };
        for (a, b) in pixelflow.into_iter().zip(xlights) {
            let d = a.abs_diff(b);
            sample.abs += u16::from(d);
            sample.sq += u32::from(d) * u32::from(d);
            sample.within &= d <= limits.tolerance;
        }
        let mut colored = true;
        for (side, rgb) in [pixelflow, xlights].into_iter().enumerate() {
            let (max, min) = (rgb[0].max(rgb[1]).max(rgb[2]), rgb[0].min(rgb[1]).min(rgb[2]));
            sample.level[side] = max;
            sample.lit[side] = max > limits.lit;
            sample.bin[side] = (rgb[0] >> 6) << 4 | (rgb[1] >> 6) << 2 | rgb[2] >> 6;
            colored &= sample.lit[side] && max - min >= limits.chroma;
        }
        if colored {
            let apart = (hue(pixelflow) - hue(xlights)).abs();
            sample.hue = Some(apart.min(360.0 - apart));
        }
        sample
    }

    /// Whether either side lights the pixel.
    pub fn lit(&self) -> bool {
        self.lit[PIXELFLOW] || self.lit[XLIGHTS]
    }

    pub fn lit_in(&self, side: usize) -> bool {
        self.lit[side]
    }
}

/// The frame being added.
#[derive(Debug, Clone, Copy, Default)]
struct Frame {
    samples: u64,
    level: [u64; 2],
}

/// Pairs of numbers, for how they move together.
#[derive(Debug, Clone, Copy, Default)]
struct Series {
    points: u64,
    n: f64,
    x: f64,
    y: f64,
    xx: f64,
    yy: f64,
    xy: f64,
}

impl Series {
    /// Adds a point that counts for `weight`.
    fn add(&mut self, weight: f64, x: f64, y: f64) {
        self.points += 1;
        self.n += weight;
        self.x += weight * x;
        self.y += weight * y;
        self.xx += weight * x * x;
        self.yy += weight * y * y;
        self.xy += weight * x * y;
    }

    fn merge(&mut self, other: &Series) {
        self.points += other.points;
        self.n += other.n;
        self.x += other.x;
        self.y += other.y;
        self.xx += other.xx;
        self.yy += other.yy;
        self.xy += other.xy;
    }

    /// Pearson's correlation; `None` with too few points, or when either side barely moves
    /// (under half a brightness level).
    fn correlation(&self) -> Option<f64> {
        if self.points < 3 {
            return None;
        }
        let vx = self.xx / self.n - (self.x / self.n).powi(2);
        let vy = self.yy / self.n - (self.y / self.n).powi(2);
        if vx < 0.25 || vy < 0.25 {
            return None;
        }
        let cov = self.xy / self.n - self.x / self.n * (self.y / self.n);
        Some((cov / (vx * vy).sqrt()).clamp(-1.0, 1.0))
    }
}

/// One window of frames: what was lit, how brightly, in which colors.
#[derive(Debug, Clone, Copy)]
struct Window {
    samples: u64,
    level: [u64; 2],
    lit: [u64; 2],
    colors: [[u32; BINS]; 2],
}

impl Default for Window {
    fn default() -> Self {
        Self {
            samples: 0,
            level: [0; 2],
            lit: [0; 2],
            colors: [[0; BINS]; 2],
        }
    }
}

impl Window {
    /// How far apart the two sides' colors are: half the summed difference of the lit pixels'
    /// histograms (0 the same colors in the same shares, 1 none in common).
    fn color_distance(&self) -> f64 {
        match (self.lit[PIXELFLOW], self.lit[XLIGHTS]) {
            (0, 0) => 0.0,
            (0, _) | (_, 0) => 1.0,
            (a, b) => {
                let (a, b) = (a as f64, b as f64);
                self.colors[PIXELFLOW]
                    .iter()
                    .zip(&self.colors[XLIGHTS])
                    .map(|(&x, &y)| (f64::from(x) / a - f64::from(y) / b).abs())
                    .sum::<f64>()
                    / 2.0
            }
        }
    }
}

/// Windows compared by what they hold rather than pixel by pixel.
#[derive(Debug, Clone, Copy)]
struct Windows {
    /// Frames in a window.
    len: u32,
    /// Frames in the window being filled.
    frames: u32,
    current: Window,
    /// What the windows closed so far weigh; the distances below are weighted by it.
    weight: f64,
    level: f64,
    lit: f64,
    colors: f64,
}

impl Windows {
    /// Closes the window being filled, each of its samples counting for `weight`.
    fn close(&mut self, weight: u64) {
        let window = std::mem::take(&mut self.current);
        self.frames = 0;
        if window.samples == 0 {
            return;
        }
        let samples = window.samples as f64;
        let apart = |pair: [u64; 2]| pair[PIXELFLOW].abs_diff(pair[XLIGHTS]) as f64 / samples;
        let n = samples * weight as f64;
        self.weight += n;
        self.level += n * apart(window.level) / 255.0;
        self.lit += n * apart(window.lit);
        self.colors += n * window.color_distance();
    }
}

/// Sums over a set of samples: a prop's, or an effect's. Every sample counts for the length of
/// its frame, so sums from sequences with different frame times add up fairly.
#[derive(Debug, Clone)]
pub struct Stats {
    /// What each sample added here counts for: the frame time in milliseconds.
    weight: u64,
    samples: u64,
    /// Pixels times milliseconds.
    time: u64,
    abs: u64,
    sq: u64,
    within: u64,
    lit: [u64; 2],
    lit_both: u64,
    level: [u64; 2],
    hue: f64,
    hue_time: u64,
    frame: Frame,
    /// Each frame's mean brightness, PixelFlow's against xLights'.
    series: Series,
    windows: Windows,
}

impl Stats {
    /// Empty sums for samples from frames `frame_ms` long.
    pub fn new(frame_ms: u32, limits: &Limits) -> Self {
        let frame_ms = frame_ms.max(1);
        Self {
            weight: u64::from(frame_ms),
            samples: 0,
            time: 0,
            abs: 0,
            sq: 0,
            within: 0,
            lit: [0; 2],
            lit_both: 0,
            level: [0; 2],
            hue: 0.0,
            hue_time: 0,
            frame: Frame::default(),
            series: Series::default(),
            windows: Windows {
                len: (limits.window_ms / frame_ms).max(1),
                frames: 0,
                current: Window::default(),
                weight: 0.0,
                level: 0.0,
                lit: 0.0,
                colors: 0.0,
            },
        }
    }

    pub fn add(&mut self, s: &Sample) {
        let w = self.weight;
        self.samples += 1;
        self.time += w;
        self.abs += w * u64::from(s.abs);
        self.sq += w * u64::from(s.sq);
        self.within += w * u64::from(s.within);
        self.lit_both += w * u64::from(s.lit[PIXELFLOW] && s.lit[XLIGHTS]);
        self.frame.samples += 1;
        self.windows.current.samples += 1;
        for side in [PIXELFLOW, XLIGHTS] {
            let level = u64::from(s.level[side]);
            self.level[side] += w * level;
            self.frame.level[side] += level;
            self.windows.current.level[side] += level;
            if s.lit[side] {
                self.lit[side] += w;
                self.windows.current.lit[side] += 1;
                self.windows.current.colors[side][usize::from(s.bin[side])] += 1;
            }
        }
        if let Some(hue) = s.hue {
            self.hue += w as f64 * f64::from(hue);
            self.hue_time += w;
        }
    }

    /// Ends the frame being added (one without samples doesn't count).
    pub fn end_frame(&mut self) {
        let frame = std::mem::take(&mut self.frame);
        if frame.samples == 0 {
            return;
        }
        let n = frame.samples as f64;
        self.series.add(
            self.weight as f64,
            frame.level[PIXELFLOW] as f64 / n,
            frame.level[XLIGHTS] as f64 / n,
        );
        self.windows.frames += 1;
        if self.windows.frames >= self.windows.len {
            self.windows.close(self.weight);
        }
    }

    /// Ends the frame and the window being added.
    fn close(&mut self) {
        self.end_frame();
        self.windows.close(self.weight);
    }

    /// Adds another set's sums to these.
    pub fn merge(&mut self, other: &Stats) {
        let mut other = other.clone();
        other.close();
        self.close();
        self.samples += other.samples;
        self.time += other.time;
        self.abs += other.abs;
        self.sq += other.sq;
        self.within += other.within;
        self.lit_both += other.lit_both;
        self.hue += other.hue;
        self.hue_time += other.hue_time;
        for side in [PIXELFLOW, XLIGHTS] {
            self.lit[side] += other.lit[side];
            self.level[side] += other.level[side];
        }
        self.series.merge(&other.series);
        self.windows.weight += other.windows.weight;
        self.windows.level += other.windows.level;
        self.windows.lit += other.windows.lit;
        self.windows.colors += other.windows.colors;
    }

    /// Pixel-frames added.
    pub fn samples(&self) -> u64 {
        self.samples
    }

    /// The scores of what was added; `None` when nothing was.
    pub fn scores(&self) -> Option<Scores> {
        if self.samples == 0 {
            return None;
        }
        let mut done = self.clone();
        done.close();
        let n = done.time as f64;
        let union = done.lit[PIXELFLOW] + done.lit[XLIGHTS] - done.lit_both;
        let weight = done.windows.weight.max(1.0);
        let distribution = Distribution {
            level: round(done.windows.level / weight),
            lit: round(done.windows.lit / weight),
            colors: round(done.windows.colors / weight),
            distance: round((done.windows.level + done.windows.lit + done.windows.colors) / weight / 3.0),
        };
        Some(Scores {
            pixel_seconds: round(n / 1000.0),
            mae: round(done.abs as f64 / n / 3.0),
            rmse: round((done.sq as f64 / n / 3.0).sqrt()),
            within_tolerance: round(done.within as f64 / n),
            lit_iou: if union == 0 {
                1.0
            } else {
                round(done.lit_both as f64 / union as f64)
            },
            brightness_correlation: done.series.correlation().map(round),
            hue_difference: (done.hue_time > 0).then(|| round(done.hue / done.hue_time as f64)),
            level: Pair {
                pixelflow: round(done.level[PIXELFLOW] as f64 / n),
                xlights: round(done.level[XLIGHTS] as f64 / n),
            },
            lit: Pair {
                pixelflow: round(done.lit[PIXELFLOW] as f64 / n),
                xlights: round(done.lit[XLIGHTS] as f64 / n),
            },
            distribution,
        })
    }
}

/// Four decimal places: plenty for a score, and it keeps the files readable.
fn round(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// A number for each side.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Pair {
    pub pixelflow: f64,
    pub xlights: f64,
}

/// How far apart the two sides are window by window, whichever pixels did the work (each 0–1).
/// The fair measure of an effect that places things at random.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Distribution {
    /// Difference in mean brightness, as a share of full brightness.
    pub level: f64,
    /// Difference in the share of pixels lit.
    pub lit: f64,
    /// Distance between the lit pixels' color histograms.
    pub colors: f64,
    /// The mean of the three.
    pub distance: f64,
}

/// What the scorecard shows for a set of samples.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scores {
    /// How much was measured: pixels times seconds.
    pub pixel_seconds: f64,
    /// Mean absolute error per channel, 0–255.
    pub mae: f64,
    /// Root mean square error per channel, 0–255.
    pub rmse: f64,
    /// Share of pixels within the tolerance on every channel, 0–1.
    pub within_tolerance: f64,
    /// "Is the right thing lit": pixels both light over pixels either lights (1 when neither
    /// lights any).
    pub lit_iou: f64,
    /// How the two brightnesses rise and fall together over time, -1 to 1; `None` when either
    /// stays level.
    pub brightness_correlation: Option<f64>,
    /// Mean degrees between the hues (0–180) where both are lit and colored; `None` when
    /// nowhere is.
    pub hue_difference: Option<f64>,
    /// Mean brightness (the brightest channel), 0–255.
    pub level: Pair,
    /// Share of pixels lit, 0–1.
    pub lit: Pair,
    pub distribution: Distribution,
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [u8; 3] = [255, 0, 0];
    const GREEN: [u8; 3] = [0, 255, 0];
    const BLUE: [u8; 3] = [0, 0, 255];
    const BLACK: [u8; 3] = [0, 0, 0];

    fn sample(pixelflow: [u8; 3], xlights: [u8; 3]) -> Sample {
        Sample::of(pixelflow, xlights, &Limits::default())
    }

    /// Sums for 50 ms frames, with windows `window_ms` long.
    fn stats(window_ms: u32) -> Stats {
        Stats::new(
            50,
            &Limits {
                window_ms,
                ..Limits::default()
            },
        )
    }

    /// One frame of `pairs`, scored at 50 ms a frame.
    fn scores(pairs: &[([u8; 3], [u8; 3])]) -> Scores {
        let mut stats = stats(1000);
        for &(a, b) in pairs {
            stats.add(&sample(a, b));
        }
        stats.scores().unwrap()
    }

    #[test]
    fn the_same_pixels_score_perfectly() {
        let s = scores(&[(RED, RED), (BLACK, BLACK), ([10, 200, 30], [10, 200, 30])]);
        assert_eq!(
            (s.mae, s.rmse, s.within_tolerance, s.lit_iou),
            (0.0, 0.0, 1.0, 1.0)
        );
        assert_eq!(s.hue_difference, Some(0.0));
        assert_eq!(s.distribution.distance, 0.0);
        assert_eq!(s.pixel_seconds, 0.15);
    }

    #[test]
    fn error_is_per_channel_and_rms_weighs_big_misses_more() {
        // Differences of 20, 10, and 0: 30 over three channels, squares 500.
        let s = scores(&[([255, 0, 0], [235, 10, 0])]);
        assert_eq!(s.mae, 10.0);
        assert_eq!(s.rmse, 12.9099);
        // One channel is past the tolerance of 16, so the pixel doesn't match.
        assert_eq!(s.within_tolerance, 0.0);
        let s = scores(&[([255, 0, 0], [239, 16, 0]), (RED, RED)]);
        assert_eq!(s.within_tolerance, 1.0);
        assert_eq!(s.mae, 5.3333);
    }

    #[test]
    fn nothing_added_has_no_scores() {
        assert_eq!(stats(1000).scores(), None);
    }

    #[test]
    fn lit_agreement_is_what_both_light_over_what_either_does() {
        // PixelFlow lights pixels 1 and 2, xLights 2 and 3; the fourth is dark in both.
        let s = scores(&[(RED, BLACK), (RED, RED), (BLACK, RED), (BLACK, BLACK)]);
        assert_eq!(s.lit_iou, 0.3333);
        assert_eq!((s.lit.pixelflow, s.lit.xlights), (0.5, 0.5));
        // Dim pixels under the threshold don't count as lit.
        let s = scores(&[([20, 20, 20], BLACK)]);
        assert_eq!((s.lit_iou, s.lit.pixelflow), (1.0, 0.0));
    }

    #[test]
    fn hue_difference_goes_the_short_way_round_and_skips_greys() {
        assert_eq!(scores(&[(RED, GREEN)]).hue_difference, Some(120.0));
        // 350 degrees against 10: 20 apart, not 340.
        let s = scores(&[([255, 0, 43], [255, 43, 0])]);
        assert!((s.hue_difference.unwrap() - 20.2).abs() < 0.2, "{s:?}");
        assert_eq!(scores(&[([200, 200, 200], RED)]).hue_difference, None);
        assert_eq!(scores(&[(BLACK, RED)]).hue_difference, None);
    }

    #[test]
    fn brightness_correlation_follows_the_two_over_time() {
        let run = |xlights: fn(u8) -> u8| {
            let mut stats = stats(1000);
            for frame in 0..10u8 {
                let level = frame * 20;
                stats.add(&sample([level, 0, 0], [xlights(level), 0, 0]));
                stats.end_frame();
            }
            stats.scores().unwrap().brightness_correlation
        };
        assert_eq!(run(|level| level), Some(1.0));
        // Dimmer, but rising and falling together.
        assert_eq!(run(|level| level / 2 + 3), Some(1.0));
        assert_eq!(run(|level| 200 - level), Some(-1.0));
        // xLights stays level: nothing to follow.
        assert_eq!(run(|_| 100), None);
    }

    #[test]
    fn a_distribution_ignores_which_pixels_did_the_work() {
        // A twinkle: the same number lit in the same color, on different pixels.
        let s = scores(&[(RED, BLACK), (BLACK, RED), (RED, BLACK), (BLACK, RED)]);
        assert_eq!(s.mae, 85.0);
        assert_eq!(s.lit_iou, 0.0);
        assert_eq!(s.distribution.distance, 0.0);
        // The same pixels in another color: only the colors differ.
        let s = scores(&[(RED, BLUE), (RED, BLUE)]);
        assert_eq!(
            (s.distribution.level, s.distribution.lit, s.distribution.colors),
            (0.0, 0.0, 1.0)
        );
        assert_eq!(s.distribution.distance, 0.3333);
        // Half as many lit, at the same brightness each.
        let s = scores(&[(RED, RED), (BLACK, RED)]);
        assert_eq!(
            (s.distribution.level, s.distribution.lit, s.distribution.colors),
            (0.5, 0.5, 0.0)
        );
    }

    #[test]
    fn distributions_are_compared_window_by_window() {
        // Lit in the first window on one side and in the second on the other: over the whole
        // effect they'd look alike, window by window they don't.
        let mut stats = stats(100);
        for (a, b) in [(RED, BLACK), (RED, BLACK), (BLACK, RED), (BLACK, RED)] {
            stats.add(&sample(a, b));
            stats.end_frame();
        }
        let s = stats.scores().unwrap();
        assert_eq!((s.distribution.level, s.distribution.lit), (1.0, 1.0));
        assert_eq!(s.distribution.colors, 1.0);
    }

    #[test]
    fn merged_sums_score_like_one_set() {
        let frames: Vec<Vec<([u8; 3], [u8; 3])>> = (0..12u8)
            .map(|f| vec![([f * 20, 0, 0], [f * 10, 5, 0]), (RED, BLUE), (BLACK, GREEN)])
            .collect();
        let mut whole = stats(200);
        let (mut first, mut second) = (stats(200), stats(200));
        for (i, frame) in frames.iter().enumerate() {
            let part = if i < 8 { &mut first } else { &mut second };
            for &(a, b) in frame {
                whole.add(&sample(a, b));
                part.add(&sample(a, b));
            }
            whole.end_frame();
            part.end_frame();
        }
        first.merge(&second);
        assert_eq!(first.scores(), whole.scores());
        assert_eq!(first.samples(), 36);
    }

    #[test]
    fn sums_from_different_frame_times_add_up_by_time() {
        // A second of red against black at 50 ms a frame, and a second matching at 20 ms: half
        // the time is wrong, though the faster sequence has more frames.
        let limits = Limits::default();
        let (mut slow, mut fast) = (Stats::new(50, &limits), Stats::new(20, &limits));
        for _ in 0..20 {
            slow.add(&sample(RED, BLACK));
            slow.end_frame();
        }
        for _ in 0..50 {
            fast.add(&sample(RED, RED));
            fast.end_frame();
        }
        slow.merge(&fast);
        let s = slow.scores().unwrap();
        assert_eq!((s.pixel_seconds, s.mae, s.within_tolerance), (2.0, 42.5, 0.5));
        assert_eq!(s.distribution.lit, 0.5);
    }
}
