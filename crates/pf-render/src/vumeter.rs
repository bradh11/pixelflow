//! The VU Meter effect, as xLights draws it (`VUMeterEffect` in
//! `src-core/effects/VUMeterEffect.cpp`): bars, levels, shapes, and flashes worked out each frame
//! from the music (its peak, xLights' `FrameData::max` and `min`, and its note spectrogram) or
//! from a timing track's marks, drawn on the target's grid with xLights' drawing code.
//!
//! Many types carry something from frame to frame (falling bars and peaks, the last trigger, the
//! color index, the bar lit), so the meter is worked out frame by frame from the effect's first
//! frame (see `sim.rs`), as xLights renders it. Frames count from the start of the sequence, as
//! xLights' `curPeriod` does. Where xLights compares a mark's start with the frame's start time
//! (its marks always sit on frame boundaries), a mark starting anywhere within the frame counts.
//! Without the music, the types that read it draw nothing, as in xLights.

use crate::audio::{Audio, RenderContext};
use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, Rng, Shade, hash};
use crate::geometry::Pixel;
use crate::raster::{Raster, grid_size};
use crate::sim::Context;
use pf_sequence::{Mark, VuMeterParams, VuMeterShape, VuMeterType};
use std::collections::VecDeque;
use std::f64::consts::PI;

const RANDOM: u64 = 0x7C_E7;

/// xLights' `LogarithmicScale`: how many notes each bar takes with a logarithmic X axis.
const LOG_X: [f64; 127] = [
    18.17223207,
    10.63007432,
    7.542157755,
    5.850152051,
    4.779922266,
    4.041366691,
    3.500791064,
    3.087916561,
    2.76223549,
    2.498745944,
    2.281176321,
    2.098478794,
    1.942887898,
    1.808785359,
    1.692005705,
    1.589395049,
    1.498521512,
    1.41747975,
    1.344755739,
    1.279131202,
    1.219614742,
    1.165391374,
    1.115784947,
    1.070229785,
    1.028249009,
    0.989437767,
    0.95345013,
    0.919988749,
    0.88879661,
    0.859650427,
    0.832355277,
    0.80674024,
    0.782654809,
    0.759965938,
    0.738555574,
    0.718318607,
    0.699161143,
    0.680999044,
    0.663756696,
    0.647365955,
    0.631765247,
    0.616898794,
    0.602715949,
    0.589170617,
    0.576220758,
    0.563827948,
    0.551956999,
    0.540575628,
    0.529654157,
    0.519165264,
    0.509083745,
    0.49938632,
    0.490051447,
    0.481059168,
    0.472390962,
    0.46402962,
    0.455959129,
    0.448164573,
    0.440632038,
    0.433348529,
    0.426301898,
    0.419480775,
    0.412874503,
    0.406473089,
    0.40026715,
    0.394247867,
    0.388406942,
    0.382736565,
    0.377229373,
    0.371878421,
    0.366677153,
    0.361619376,
    0.356699232,
    0.351911178,
    0.347249965,
    0.34271062,
    0.338288424,
    0.3339789,
    0.329777796,
    0.325681071,
    0.321684884,
    0.317785577,
    0.31397967,
    0.310263847,
    0.306634947,
    0.303089955,
    0.299625994,
    0.296240317,
    0.2929303,
    0.289693435,
    0.286527323,
    0.28342967,
    0.280398278,
    0.277431045,
    0.274525954,
    0.271681075,
    0.268894553,
    0.266164612,
    0.263489545,
    0.260867715,
    0.258297548,
    0.255777532,
    0.253306213,
    0.250882193,
    0.248504127,
    0.24617072,
    0.243880727,
    0.241632946,
    0.239426222,
    0.237259439,
    0.235131523,
    0.233041437,
    0.230988182,
    0.228970792,
    0.226988336,
    0.225039915,
    0.223124658,
    0.221241727,
    0.21939031,
    0.217569623,
    0.215778906,
    0.214017426,
    0.212284472,
    0.210579358,
    0.208901417,
    0.207250005,
    0.0,
];

/// `GetLogSum(to)`: the whole number of notes the first `to` bars take.
fn log_sum(to: usize) -> i32 {
    LOG_X.iter().take(to.min(127)).sum::<f64>() as i32
}

/// xLights' `ApplyGain`: the level boosted by `gain` percent, at most 1.
fn gained(value: f32, gain: f32) -> f32 {
    ((100.0 + gain) * value / 100.0).min(1.0)
}

/// What the meter carries from frame to frame (xLights' `VUMeterRenderCache`).
#[derive(Debug, Clone)]
pub(crate) struct Meter {
    last_values: Vec<f32>,
    last_peaks: Vec<f32>,
    pause: Vec<i32>,
    /// The spectrogram lines drawn in recent frames, oldest first (the last is this frame's when
    /// `drew_line`).
    lines: VecDeque<Vec<(i32, i32)>>,
    drew_line: bool,
    /// The last frame something triggered (`_lasttimingmark`).
    last_mark: i64,
    /// A jump's height, a pulse's frames left, or a Level Shape's size (`_lastsize`).
    last_size: f32,
    /// What the jumps and pulses draw this frame (they fall after drawing).
    shown: f32,
    /// The bar lit (`_lastsize` again in xLights, as a whole number).
    bar: i32,
    colour: i64,
    count: i64,
    direction: i32,
    /// Whether this frame has a timing event (Timing Event Color).
    present: bool,
}

impl Default for Meter {
    fn default() -> Self {
        Self {
            last_values: Vec::new(),
            last_peaks: Vec::new(),
            pause: Vec::new(),
            lines: VecDeque::new(),
            drew_line: false,
            last_mark: -1,
            last_size: 0.0,
            shown: 0.0,
            bar: 0,
            colour: -1,
            count: 0,
            direction: 1,
            present: false,
        }
    }
}

/// Bars, limited to the grid's width for most types (xLights' `usebars`).
fn bars_used(p: &VuMeterParams, width: i32) -> i32 {
    let bars = p.bars.max(1) as i32;
    match p.meter {
        VuMeterType::TimingEventJump
        | VuMeterType::TimingEventPulse
        | VuMeterType::TimingEventPulseColor
        | VuMeterType::TimingEventJump100 => bars,
        _ => bars.min(width.max(1)),
    }
}

/// Whether a mark's label passes the filter (xLights' `Effect::FilteredIn`, without regular
/// expressions): one of its words (split at `: ;,`) is the filter, or the filter lists it.
fn filtered_in(mark: &Mark, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    if mark.label.is_empty() {
        return false;
    }
    mark.label
        .split([':', ' ', ';', ','])
        .any(|w| !w.is_empty() && w == filter)
        || format!(";{filter};").contains(&format!(";{};", mark.label))
}

/// The marks a meter reads, and the frame they're read in.
struct Marks<'a> {
    marks: &'a [Mark],
    filter: &'a str,
    frame_ms: u64,
}

impl Marks<'_> {
    /// The frame's span (ms).
    fn span(&self, frame: u64) -> (u64, u64) {
        (frame * self.frame_ms, (frame + 1) * self.frame_ms)
    }

    /// The first mark the frame is in (`GetTimingEvent`).
    fn event(&self, frame: u64) -> Option<&Mark> {
        let (from, to) = self.span(frame);
        self.marks
            .iter()
            .take_while(|m| m.start_ms < to)
            .find(|m| m.end_ms > from && filtered_in(m, self.filter))
    }

    /// Whether `mark` starts in the frame.
    fn starts_in(&self, mark: &Mark, frame: u64) -> bool {
        let (from, to) = self.span(frame);
        (from..to).contains(&mark.start_ms)
    }

    /// The frame's event, when it starts in the frame.
    fn starting(&self, frame: u64) -> Option<&Mark> {
        self.event(frame).filter(|m| self.starts_in(m, frame))
    }
}

/// The loudest note from just above `start` up to `end` (as xLights' note types read them).
fn note_level(notes: &[f32], start: u32, end: u32) -> f32 {
    notes
        .iter()
        .enumerate()
        .filter(|&(i, _)| i as u32 > start && i as u32 <= end)
        .fold(0.0f32, |m, (_, &v)| m.max(v))
}

/// The notes from `start` to `end`, the lower first.
fn note_range(p: &VuMeterParams) -> (u32, u32) {
    let (a, b) = (p.start_note.min(126), p.end_note.min(126));
    (a.min(b), a.max(b))
}

impl Meter {
    /// Moves on to frame `frame` (counted from the start of the sequence).
    pub fn step(&mut self, p: &VuMeterParams, frame: u64, cx: &Context, world: &RenderContext) {
        let (width, height) = grid_size(cx.canvas);
        let usebars = bars_used(p, width);
        let audio = world.audio;
        let sensitivity = p.sensitivity as f32 / 100.0;
        let level = |a: Audio| gained(a.peak(frame), p.gain);
        let (start, end) = note_range(p);
        let notes = |a: Audio| a.notes(frame).map(|n| gained(note_level(&n, start, end), p.gain));
        let marks = world.marks(p.timing_track).map(|marks| Marks {
            marks,
            filter: &p.filter,
            frame_ms: u64::from(cx.frame_ms.max(1)),
        });
        let colors = cx.colors.len() as i64;
        let f = frame as i64;
        use VuMeterType as T;
        match p.meter {
            T::Spectrogram | T::SpectrogramPeak | T::SpectrogramLine | T::SpectrogramCircleLine => {
                self.spectrogram(p, frame, audio, width, height);
            }
            T::Pulse => {
                // Any mark at all, whatever the filter (`RenderPulseFrame`).
                if let Some(m) = &marks {
                    let (from, to) = m.span(frame);
                    if m.marks.iter().any(|mark| (from..to).contains(&mark.start_ms)) {
                        self.last_mark = f;
                    }
                }
            }
            T::LevelPulse => {
                if let Some(a) = audio
                    && level(a) > sensitivity
                {
                    self.last_mark = f;
                }
            }
            T::NoteLevelPulse => {
                if let Some(level) = audio.and_then(notes)
                    && level > sensitivity
                {
                    self.last_mark = f;
                }
            }
            T::LevelJump | T::LevelJump100 | T::NoteLevelJump | T::NoteLevelJump100 => {
                let now = match p.meter {
                    T::LevelJump | T::LevelJump100 => audio.map(level),
                    _ => audio.and_then(notes),
                };
                if let Some(now) = now
                    && now > sensitivity
                {
                    self.last_mark = f;
                    let full = matches!(p.meter, T::LevelJump100 | T::NoteLevelJump100);
                    self.last_size = if full { 1.0 } else { now };
                }
            }
            T::LevelPulseColor | T::LevelColor => {
                if let Some(a) = audio
                    && level(a) > sensitivity
                {
                    if self.last_mark != f - 1 {
                        self.colour = (self.colour + 1) % colors.max(1);
                    }
                    self.last_mark = f;
                }
            }
            T::LevelShape => {
                if let Some(a) = audio {
                    let scaling = p.sensitivity as f32 / 100.0 * 7.0;
                    let (w, h) = (width as f32 / 2.0, height as f32 / 2.0);
                    let max_size = h.min(w) * scaling;
                    let size = max_size * level(a);
                    if p.slow_falls && size < self.last_size {
                        self.last_size -= max_size.min(h.max(w)) / 20.0;
                        self.last_size = self.last_size.max(size);
                    } else {
                        self.last_size = size;
                    }
                }
            }
            T::LevelBar | T::LevelRandomBar | T::NoteLevelBar | T::NoteLevelRandomBar => {
                let now = match p.meter {
                    T::LevelBar | T::LevelRandomBar => audio.map(level),
                    _ => audio.and_then(notes),
                };
                if let Some(now) = now
                    && now > sensitivity
                {
                    self.colour = (self.colour + 1) % colors.max(1);
                    let random = matches!(p.meter, T::LevelRandomBar | T::NoteLevelRandomBar);
                    self.next_bar(usebars, random, false, cx.seed, frame);
                }
            }
            T::TimingEventTimedSweep
            | T::TimingEventTimedSweep2
            | T::TimingEventAlternateTimedSweep
            | T::TimingEventAlternateTimedSweep2
            | T::TimingEventChaseFromMiddle
            | T::TimingEventChaseToMiddle => {
                if let Some(m) = &marks
                    && m.starting(frame).is_some()
                {
                    self.count += 1;
                }
            }
            T::TimingEventColor => {
                if let Some(m) = &marks {
                    if m.starting(frame).is_some() {
                        self.colour = (self.colour + 1) % colors.max(1);
                    }
                    self.present = m.event(frame).is_some();
                    self.colour = self.colour.max(0);
                }
            }
            T::TimingEventJump | T::TimingEventJump100 => {
                let jump100 = p.meter == T::TimingEventJump100;
                if let Some(m) = &marks
                    && (jump100 || audio.is_some())
                {
                    if m.starting(frame).is_some() {
                        self.last_size = match audio {
                            Some(a) if !jump100 => level(a),
                            _ => 1.0,
                        };
                    }
                    self.shown = self.last_size;
                    if self.last_size > 0.0 {
                        self.last_size = (self.last_size - 1.0 / usebars as f32).max(0.0);
                    }
                } else {
                    self.shown = 0.0;
                }
            }
            T::TimingEventPulse | T::TimingEventPulseColor => {
                if let Some(m) = &marks {
                    if m.starting(frame).is_some() {
                        self.last_size = usebars as f32;
                        if p.meter == T::TimingEventPulseColor {
                            self.colour = (self.colour + 1) % colors.max(1);
                        }
                    }
                    self.shown = self.last_size;
                    if self.last_size > 0.0 {
                        self.last_size -= 1.0;
                    }
                } else {
                    self.shown = 0.0;
                }
            }
            T::TimingEventBar | T::TimingEventBarBounce | T::TimingEventRandomBar | T::TimingEventBars => {
                if let Some(m) = &marks {
                    if m.starting(frame).is_some() {
                        self.colour = (self.colour + 1) % colors.max(1);
                        let random = p.meter == T::TimingEventRandomBar;
                        let bounce = p.meter == T::TimingEventBarBounce;
                        self.next_bar(usebars, random, bounce, cx.seed, frame);
                    }
                    self.colour = self.colour.max(0);
                }
            }
            // The rest only read the music or marks of the frame they draw.
            T::VolumeBars
            | T::Waveform
            | T::On
            | T::ColorOn
            | T::DominantFrequencyColor
            | T::DominantFrequencyColorGradient
            | T::IntensityWave
            | T::TimingEventSpike
            | T::TimingEventSweep
            | T::TimingEventSweep2
            | T::NoteOn => {}
        }
    }

    /// The next bar to light: the next one along (back to the first after the last), the next
    /// in a bounce, or a random other one.
    fn next_bar(&mut self, bars: i32, random: bool, bounce: bool, seed: u64, frame: u64) {
        if random && bars > 2 {
            let mut rng = Rng::new(hash(seed, RANDOM, frame));
            let was = self.bar;
            while self.bar == was {
                self.bar = 1 + (rng.unit() * f64::from(bars)) as i32;
            }
            if self.bar > bars {
                self.bar = 1;
            }
        } else if bounce {
            self.bar += self.direction;
            if self.bar > bars || self.bar == 0 {
                self.direction = -self.direction;
                self.bar += self.direction * 2;
            }
        } else {
            self.bar += 1;
            if self.bar > bars {
                self.bar = 1;
            }
        }
    }

    /// The spectrogram's falling values and peaks, and its line (`RenderSpectrogramFrame`).
    fn spectrogram(&mut self, p: &VuMeterParams, frame: u64, audio: Option<Audio>, width: i32, height: i32) {
        use VuMeterType as T;
        let peak = p.meter != T::Spectrogram;
        let line = matches!(p.meter, T::SpectrogramLine | T::SpectrogramCircleLine);
        let keep = (p.sensitivity / 10) as usize;
        while self.lines.len() > keep {
            self.lines.pop_front();
        }
        self.drew_line = false;
        let Some(vu) = audio.and_then(|a| a.notes(frame)) else {
            return;
        };
        if peak {
            if self.last_values.is_empty() {
                self.last_values = vu.to_vec();
                self.last_peaks = vu.to_vec();
                self.pause = vec![0; vu.len()];
            } else {
                let hold = p.sensitivity as i32;
                for ((old, &new), pause) in self.last_peaks.iter_mut().zip(&vu).zip(&mut self.pause) {
                    if new < *old {
                        if *pause == 0 {
                            *old = (*old - 0.05).max(new);
                        }
                        *pause = (*pause - 1).max(0);
                    } else {
                        *old = new;
                        *pause = hold;
                    }
                }
            }
        }
        if p.slow_falls {
            if self.last_values.is_empty() {
                self.last_values = vu.to_vec();
            } else {
                for (old, &new) in self.last_values.iter_mut().zip(&vu) {
                    *old = if new < *old { (*old - 0.05).max(new) } else { new };
                }
            }
        } else {
            self.last_values = vu.to_vec();
        }
        if line {
            let (points, _) = self.line_points(p, width, height);
            if !points.is_empty() {
                self.lines.push_back(points);
                self.drew_line = true;
            }
        }
    }

    /// Each bar's height and peak (0–1, the height with the gain applied) for the spectrogram as
    /// it stands, and how many bars and grid columns per bar.
    fn spectrogram_bars(&self, p: &VuMeterParams) -> (Vec<(f32, f32)>, i32) {
        let peak = p.meter != VuMeterType::Spectrogram;
        let (start, end) = note_range(p);
        let values = &self.last_values;
        let datapoints = values.len().min((end - start + 1) as usize) as i32;
        // The spectrogram types take all the bars asked for, up to one a note.
        let usebars = (p.bars.max(1) as i32).min(datapoints).max(1);
        let per = datapoints as f32 / usebars as f32;
        let gain = if p.meter == VuMeterType::SpectrogramCircleLine {
            p.gain
        } else {
            1.0
        };
        let mut it = start as usize;
        let mut bars = Vec::with_capacity(usebars as usize);
        for j in 0..usebars as usize {
            let take = if p.log_x {
                log_sum(j + 1) - log_sum(j)
            } else {
                per as i32
            };
            let (mut f, mut pk) = (0.0f32, 0.0f32);
            for _ in 0..take {
                let Some(&v) = values.get(it) else { break };
                f = f.max(v);
                if peak {
                    pk = pk.max(self.last_peaks.get(it).copied().unwrap_or(0.0));
                }
                // Don't run off the end.
                if it + 1 < values.len() {
                    it += 1;
                }
            }
            bars.push((gained(f, gain), pk));
        }
        (bars, usebars)
    }

    /// The spectrogram's line through the bars' heights (or around a circle), and the flat line
    /// a single bar draws instead.
    fn line_points(&self, p: &VuMeterParams, width: i32, height: i32) -> (Vec<(i32, i32)>, Option<[i32; 4]>) {
        let (bars, usebars) = self.spectrogram_bars(p);
        let (tx, ty) = (p.x_offset as i32 * width / 100, p.y_offset as i32 * height / 100);
        let cols = if p.x_offset as i32 == 0 {
            (width as f32 / usebars as f32).max(1.0)
        } else {
            1.0
        };
        let mut points = Vec::new();
        let mut flat = None;
        if p.meter == VuMeterType::SpectrogramCircleLine {
            let per = 360.0 / f64::from(usebars);
            let at = |vector: f64, angle: f64| {
                let a = angle.to_radians();
                (
                    (f64::from(width / 2 + tx) + vector * a.sin()) as i32,
                    (f64::from(height / 2 + ty) + vector * a.cos()) as i32,
                )
            };
            let mut first = 0.0;
            for (j, &(f, _)) in bars.iter().enumerate() {
                let vector = f64::from(width.min(height) as f32 * f);
                let angle = per / 2.0 + j as f64 * per;
                if j == 0 {
                    first = vector;
                }
                points.push(at(vector, angle));
                if j + 1 == bars.len() && j > 0 {
                    points.push(at(first, angle + per));
                }
            }
        } else {
            for (j, &(f, _)) in bars.iter().enumerate() {
                let colheight = (height as f32 * f) as i32;
                let mid = (cols * j as f32 + cols / 2.0) as i32;
                points.push((mid, colheight));
                if usebars == 1 {
                    flat = Some([0, colheight, (cols - 1.0) as i32, colheight]);
                }
            }
        }
        (points, flat)
    }
}

/// The meter's frame, drawn on the target's grid.
pub struct VuMeter {
    raster: Raster,
}

impl VuMeter {
    /// The meter at `frame` (counted from the start of the sequence), as it stands.
    pub(crate) fn new(
        meter: &Meter,
        p: &VuMeterParams,
        frame: u64,
        cx: &Context,
        world: &RenderContext,
    ) -> Self {
        let mut raster = Raster::new(cx.canvas);
        Draw {
            raster: &mut raster,
            meter,
            p,
            frame,
            colors: cx.colors,
            audio: world.audio,
            marks: world.marks(p.timing_track).map(|marks| Marks {
                marks,
                filter: &p.filter,
                frame_ms: u64::from(cx.frame_ms.max(1)),
            }),
        }
        .draw();
        Self { raster }
    }
}

impl Shade for VuMeter {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        self.raster.at(px)
    }
}

/// One frame being drawn.
struct Draw<'a> {
    raster: &'a mut Raster,
    meter: &'a Meter,
    p: &'a VuMeterParams,
    frame: u64,
    colors: Colors,
    audio: Option<Audio<'a>>,
    marks: Option<Marks<'a>>,
}

/// xLights' `GetMultiColorBlend(n, false, …, reserve)`: the palette (less its last `reserve`
/// colors) as a ramp.
fn blend(colors: &Colors, n: f32, reserve: usize) -> [f32; 3] {
    let count = colors.len().saturating_sub(reserve);
    if count <= 1 {
        return colors.get(0);
    }
    let n = if n >= 1.0 { 0.99999 } else { n.max(0.0) };
    let at = n * (count - 1) as f32;
    let i = at.floor() as usize;
    let (a, b) = (colors.get(i as u64), colors.get(((i + 1) % count) as u64));
    let t = at - i as f32;
    [0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * t)
}

/// A color at xLights' 8-bit alpha (`color.alpha = v`, truncated).
fn with_alpha(c: [f32; 3], alpha: f32) -> Rgba {
    Rgba::with_alpha(c, (alpha * 255.0).clamp(0.0, 255.0).trunc() / 255.0)
}

impl Draw<'_> {
    fn width(&self) -> i32 {
        self.raster.width
    }

    fn height(&self) -> i32 {
        self.raster.height
    }

    fn fill(&mut self, c: Rgba) {
        for y in 0..self.height() {
            for x in 0..self.width() {
                self.raster.set(x, y, c);
            }
        }
    }

    fn column(&mut self, x: i32, c: Rgba) {
        for y in 0..self.height() {
            self.raster.set(x, y, c);
        }
    }

    /// Rows from the bottom up to `level` of the height, in the palette from bottom to top.
    fn rise(&mut self, level: f32) {
        let h = self.height();
        let mut y = 0;
        while (y as f32) < level * h as f32 {
            let c = Rgba::opaque(blend(&self.colors, y as f32 / h as f32, 0));
            for x in 0..self.width() {
                self.raster.set(x, y, c);
            }
            y += 1;
        }
    }

    fn color(&self, index: i64) -> [f32; 3] {
        self.colors.get(index.max(0) as u64)
    }

    fn draw(mut self) {
        let p = self.p;
        let frame = self.frame;
        let f = frame as i64;
        let meter = self.meter;
        let usebars = bars_used(p, self.width());
        let level = self.audio.map(|a| gained(a.peak(frame), p.gain));
        let (start, end) = note_range(p);
        use VuMeterType as T;
        match p.meter {
            T::Spectrogram | T::SpectrogramPeak | T::SpectrogramLine | T::SpectrogramCircleLine => {
                self.spectrogram();
            }
            T::VolumeBars => {
                let Some(audio) = self.audio else { return };
                let first = f - i64::from(usebars);
                let cols = (self.width() as f32 / usebars as f32).max(0.001);
                for x in 0..self.width() {
                    let i = first + (x as f32 / cols) as i64;
                    if i > 0 {
                        let level = gained(audio.peak(i as u64), p.gain);
                        let colheight = (self.height() as f32 * level) as i32;
                        for y in 0..colheight {
                            let c = blend(&self.colors, y as f32 / self.height() as f32, 0);
                            self.raster.set(x, y, Rgba::opaque(c));
                        }
                    }
                }
            }
            T::Waveform => {
                let Some(audio) = self.audio else { return };
                let (w, h) = (self.width(), self.height());
                let ty = p.y_offset as i32 * h / 2 / 100;
                let cols = w as f32 / usebars as f32;
                let first = f - i64::from(usebars);
                let mut x = 0i32;
                for i in 0..i64::from(usebars) {
                    if first + i >= 0 {
                        let at = (first + i) as u64;
                        let (high, low) = (gained(audio.peak(at), p.gain), gained(audio.trough(at), p.gain));
                        let s = ((1.0 - low) * h as f32 / 2.0) as i32;
                        let e = (((1.0 + high) * h as f32 / 2.0) as i32).max(s).min(h);
                        let mut j = 0;
                        while (j as f32) < cols {
                            for y in s..e {
                                let c = blend(&self.colors, y as f32 / h as f32, 0);
                                self.raster.set(x, y + ty, Rgba::opaque(c));
                            }
                            x += 1;
                            j += 1;
                        }
                    } else {
                        x = (x as f32 + cols) as i32;
                    }
                }
            }
            T::On => {
                if let Some(level) = level {
                    self.fill(with_alpha(self.color(0), level));
                }
            }
            T::ColorOn => {
                if let Some(level) = level {
                    self.fill(Rgba::opaque(blend(&self.colors, level, 0)));
                }
            }
            T::DominantFrequencyColor | T::DominantFrequencyColorGradient => {
                let Some(vu) = self.audio.and_then(|a| a.notes(frame)) else {
                    return;
                };
                let sensitivity = p.sensitivity as f32 / 100.0;
                let mut note = None;
                let mut loudest = -1000.0f32;
                for (i, &v) in vu.iter().enumerate().take(end as usize + 1).skip(start as usize) {
                    if v > sensitivity && v > loudest {
                        loudest = v;
                        note = Some(i as u32);
                    }
                }
                if let Some(note) = note {
                    let span = (end - start + 1) as f32;
                    let c = if p.meter == T::DominantFrequencyColorGradient {
                        blend(&self.colors, (note - start) as f32 / span, 0)
                    } else {
                        let i = ((note - start) as f32 * self.colors.len() as f32 / span) as i64;
                        self.color(i)
                    };
                    self.fill(Rgba::opaque(c));
                }
            }
            T::Pulse => {
                if self.marks.is_some() && meter.last_mark >= 0 {
                    let level = 1.0 - (f - meter.last_mark) as f32 / usebars as f32;
                    if level > 0.0 {
                        self.fill(with_alpha(self.color(0), level));
                    }
                }
            }
            T::IntensityWave => {
                let Some(audio) = self.audio else { return };
                let first = f - i64::from(usebars);
                let cols = self.width() / usebars;
                let mut x = 0;
                for i in 0..i64::from(usebars) {
                    if first + i >= 0 {
                        let level = gained(audio.peak((first + i) as u64), p.gain);
                        let c = if self.colors.len() < 2 {
                            with_alpha(self.color(0), level)
                        } else {
                            Rgba::opaque(blend(&self.colors, 1.0 - level, 0))
                        };
                        for _ in 0..cols {
                            self.column(x, c);
                            x += 1;
                        }
                    } else {
                        x += cols;
                    }
                }
            }
            T::LevelPulse | T::NoteLevelPulse | T::LevelPulseColor => {
                let has_input = match p.meter {
                    T::NoteLevelPulse => self.audio.and_then(|a| a.notes(frame)).is_some(),
                    _ => self.audio.is_some(),
                };
                let fade = i64::from(usebars);
                if has_input && fade > 0 && f - meter.last_mark < fade {
                    let level = 1.0 - (f - meter.last_mark) as f32 / fade as f32;
                    if level > 0.0 {
                        let index = if p.meter == T::LevelPulseColor {
                            meter.colour
                        } else {
                            0
                        };
                        self.fill(with_alpha(self.color(index), level));
                    }
                }
            }
            T::LevelColor => {
                if self.audio.is_some() && meter.colour >= 0 {
                    self.fill(Rgba::opaque(self.color(meter.colour)));
                }
            }
            T::LevelJump | T::LevelJump100 | T::NoteLevelJump | T::NoteLevelJump100 => {
                let has_input = match p.meter {
                    T::LevelJump | T::LevelJump100 => self.audio.is_some(),
                    _ => self.audio.and_then(|a| a.notes(frame)).is_some(),
                };
                let fade = i64::from(usebars);
                if has_input && fade > 0 && f - meter.last_mark < fade {
                    let size = meter.last_size;
                    let level = size - size * (f - meter.last_mark) as f32 / fade as f32;
                    if level > 0.0 {
                        self.rise(level);
                    }
                }
            }
            T::LevelShape => {
                if self.audio.is_some() {
                    self.level_shape(usebars);
                }
            }
            T::LevelBar | T::LevelRandomBar | T::NoteLevelBar | T::NoteLevelRandomBar => {
                // Only while there's music (xLights' frame data).
                if self.audio.and_then(|a| a.notes(frame)).is_some() {
                    self.bar(usebars, meter.bar - 1, meter.colour);
                }
            }
            T::TimingEventBar | T::TimingEventBarBounce | T::TimingEventRandomBar => {
                if self.marks.is_some() {
                    self.bar(usebars, meter.bar - 1, meter.colour);
                }
            }
            T::TimingEventBars => {
                if self.marks.is_some() {
                    let mut index = meter.colour;
                    for i in 0..usebars {
                        let c = Rgba::opaque(self.color(index));
                        let (from, to) = self.bar_span(usebars, i);
                        for x in from..to {
                            self.column(x, c);
                        }
                        index = (index + 1) % self.colors.len() as i64;
                    }
                }
            }
            T::TimingEventSpike | T::TimingEventSweep | T::TimingEventSweep2 => {
                let Some(marks) = self.marks.take() else { return };
                let first = f - i64::from(usebars);
                let cols = self.width() / usebars;
                let h = self.height();
                let mut x = 0;
                for i in 0..i64::from(usebars) {
                    if first + i >= 0 && marks.starting((first + i) as u64).is_some() {
                        for j in 0..cols {
                            for y in 0..h {
                                let c = match p.meter {
                                    T::TimingEventSweep => blend(&self.colors, y as f32 / h as f32, 0),
                                    T::TimingEventSweep2 => blend(&self.colors, j as f32 / cols as f32, 0),
                                    _ => blend(&self.colors, 0.0, 0),
                                };
                                self.raster.set(x, y, Rgba::opaque(c));
                            }
                            x += 1;
                        }
                    } else {
                        x += cols;
                    }
                }
            }
            T::TimingEventTimedSweep
            | T::TimingEventTimedSweep2
            | T::TimingEventAlternateTimedSweep
            | T::TimingEventAlternateTimedSweep2
            | T::TimingEventChaseFromMiddle
            | T::TimingEventChaseToMiddle => self.timed(usebars),
            T::TimingEventColor => {
                if self.marks.is_some() {
                    let c = self.color(meter.colour);
                    let fill = if meter.present {
                        Rgba::opaque(c)
                    } else {
                        Rgba::with_alpha(c, ((p.sensitivity * 255) / 100) as f32 / 255.0)
                    };
                    self.fill(fill);
                }
            }
            T::TimingEventJump | T::TimingEventJump100 => {
                if meter.shown > 0.0 {
                    self.rise(meter.shown);
                }
            }
            T::TimingEventPulse | T::TimingEventPulseColor => {
                if meter.shown > 0.0 {
                    let index = if p.meter == T::TimingEventPulseColor {
                        meter.colour
                    } else {
                        0
                    };
                    let alpha = meter.shown / usebars as f32;
                    let c = with_alpha(self.color(index), alpha);
                    let h = self.height() as f32;
                    let mut y = 0;
                    while (y as f32) < h * meter.shown {
                        for x in 0..self.width() {
                            self.raster.set(x, y, c);
                        }
                        y += 1;
                    }
                }
            }
            T::NoteOn => {
                if let Some(vu) = self.audio.and_then(|a| a.notes(frame)) {
                    let level = gained(note_level(&vu, start, end), p.gain);
                    self.fill(with_alpha(self.color(0), level));
                }
            }
        }
    }

    /// Bar `i` of `bars`: its columns (`startx` to `endx`, xLights' whole-number division).
    fn bar_span(&self, bars: i32, i: i32) -> (i32, i32) {
        let w = self.width();
        let each = w / bars.max(1);
        (each * i, (each * (i + 1)).min(w))
    }

    fn bar(&mut self, bars: i32, bar: i32, colour: i64) {
        if bar < 0 {
            return;
        }
        let c = Rgba::opaque(self.color(colour));
        let (from, to) = self.bar_span(bars, bar);
        for x in from..to {
            self.column(x, c);
        }
    }

    /// The timed sweeps and chases: a band of the palette moving across over the mark the frame
    /// is in.
    fn timed(&mut self, usebars: i32) {
        use VuMeterType as T;
        let Some(marks) = self.marks.take() else { return };
        let Some(mark) = marks.event(self.frame) else {
            return;
        };
        let (w, h) = (self.width(), self.height());
        let frame_ms = marks.frame_ms as f64;
        let length = ((mark.end_ms - mark.start_ms) as f64 / frame_ms).max(1.0);
        let at = (self.frame * marks.frame_ms).saturating_sub(mark.start_ms) / marks.frame_ms;
        let p = self.p;
        let bars = f64::from(usebars);
        let distance = match p.meter {
            T::TimingEventTimedSweep | T::TimingEventAlternateTimedSweep => f64::from(w) + 2.0 * bars,
            T::TimingEventTimedSweep2 | T::TimingEventAlternateTimedSweep2 => f64::from(w) - bars,
            T::TimingEventChaseFromMiddle => f64::from((w + 2 * usebars) / 2),
            _ => f64::from(w + 2 * usebars),
        };
        let mut start = (distance / length * at as f64) as i32;
        if matches!(
            p.meter,
            T::TimingEventTimedSweep | T::TimingEventAlternateTimedSweep
        ) {
            start -= usebars;
        }
        let flipped = matches!(
            p.meter,
            T::TimingEventAlternateTimedSweep | T::TimingEventAlternateTimedSweep2
        ) && self.meter.count % 2 == 0;
        for x in 0..usebars {
            let c = Rgba::opaque(blend(&self.colors, x as f32 / usebars as f32, 0));
            for y in 0..h {
                match p.meter {
                    T::TimingEventChaseFromMiddle => {
                        self.raster.set(w / 2 - (x + start), y, c);
                        self.raster.set(w / 2 + (x + start), y, c);
                    }
                    T::TimingEventChaseToMiddle => {
                        self.raster.set(w - (x + start) - 1, y, c);
                        self.raster.set(x + start, y, c);
                    }
                    _ if flipped => self.raster.set(w - (x + start) - 1, y, c),
                    _ => self.raster.set(x + start, y, c),
                }
            }
        }
    }

    fn spectrogram(&mut self) {
        use VuMeterType as T;
        let p = self.p;
        let meter = self.meter;
        if self.audio.and_then(|a| a.notes(self.frame)).is_none() {
            return;
        }
        let peak = p.meter != T::Spectrogram;
        let line = matches!(p.meter, T::SpectrogramLine | T::SpectrogramCircleLine);
        let (w, h) = (self.width(), self.height());
        // Earlier frames' lines, fading out.
        let keep = (p.sensitivity / 10) as i32;
        let earlier = meter.lines.len() - usize::from(meter.drew_line);
        if keep > 0 {
            let first = self.color(0);
            let mut alpha = 255;
            for points in meter.lines.iter().take(earlier) {
                if points.len() > 1 {
                    alpha -= 255 / keep;
                    let c = Rgba::with_alpha(first, alpha.max(0) as f32 / 255.0);
                    for pair in points.windows(2) {
                        line_over(self.raster, pair[0], pair[1], c);
                    }
                }
            }
        }
        if line {
            let (points, flat) = meter.line_points(p, w, h);
            let c = Rgba::opaque(self.color(0));
            for pair in points.windows(2) {
                self.raster.line(pair[0].0, pair[0].1, pair[1].0, pair[1].1, c);
            }
            if let Some([x0, y0, x1, y1]) = flat {
                self.raster.line(x0, y0, x1, y1, c);
            }
            return;
        }
        let (bars, usebars) = meter.spectrogram_bars(p);
        let tx = p.x_offset as i32 * w / 100;
        let cols = if p.x_offset as i32 == 0 {
            (w as f32 / usebars as f32).max(1.0)
        } else {
            1.0
        };
        // Peaks take the last color; the bars the rest.
        let peak_color = if self.colors.len() > 1 {
            self.color(self.colors.len() as i64 - 1)
        } else {
            self.color(0)
        };
        let mut x = tx;
        for (j, &(f, pk)) in bars.iter().enumerate() {
            let colheight = (h as f32 * f) as i32;
            let peakheight = (h as f32 * pk) as i32;
            let limit = (j + 1) as f32 * cols;
            while (x as f32) < limit {
                for y in 0..h {
                    if y < colheight {
                        let c = blend(&self.colors, y as f32 / h as f32, usize::from(peak));
                        self.raster.set(x, y, Rgba::opaque(c));
                    }
                    if peak {
                        if y >= peakheight {
                            self.raster.set(x, y, Rgba::opaque(peak_color));
                            break;
                        }
                    } else if y >= colheight {
                        break;
                    }
                }
                x += 1;
            }
        }
    }

    /// `RenderLevelShapeFrame`: the shape at the meter's size, around the middle.
    fn level_shape(&mut self, usebars: i32) {
        use VuMeterShape as S;
        let p = self.p;
        let size = self.meter.last_size;
        let (w, h) = (self.width(), self.height());
        let points = usebars.min(99) / 25 + 4;
        let (tx, ty) = (p.x_offset as i32 * w / 2 / 100, p.y_offset as i32 * h / 2 / 100);
        let (cx, cy) = ((w as f64 / 2.0) as i32 + tx, (h as f64 / 2.0) as i32 + ty);
        let first = self.color(0);
        let colors = self.colors;
        let ramp = |d: f32| Rgba::opaque(blend(&colors, d, 0));
        let r = &mut *self.raster;
        // The outline at its size, with fainter ones either side (`_ ± 1` at half, `± 2` at a
        // quarter).
        let glow = |r: &mut Raster, draw: &dyn Fn(&mut Raster, f32, Rgba)| {
            for (offset, alpha) in [(-2.0, 0.25), (2.0, 0.25), (-1.0, 0.5), (1.0, 0.5), (0.0, 1.0)] {
                draw(r, size + offset, with_alpha(first, alpha));
            }
        };
        match p.shape {
            S::Circle => glow(r, &|r, s, c| draw_circle(r, cx, cy, s, c)),
            S::FilledCircle => {
                let mut x = 0;
                while x as f32 <= size {
                    draw_circle(r, cx, cy, x as f32, ramp(x as f32 / size));
                    x += 1;
                }
            }
            S::Square => {
                // The fainter boxes are the box grown and shrunk by a pixel or two.
                let half = f64::from(size) / 2.0;
                let (sx, ex) = ((f64::from(cx) - half) as i32, (f64::from(cx) + half) as i32);
                let (sy, ey) = ((f64::from(cy) - half) as i32, (f64::from(cy) + half) as i32);
                for (d, alpha) in [(2, 0.25), (-2, 0.25), (1, 0.5), (-1, 0.5), (0, 1.0)] {
                    draw_box(r, sx - d, ex + d, sy - d, ey + d, with_alpha(first, alpha));
                }
            }
            S::FilledSquare => {
                let half = f64::from(size) / 2.0;
                let (sx, ex) = ((f64::from(cx) - half) as i32, (f64::from(cx) + half) as i32);
                let (sy, ey) = ((f64::from(cy) - half) as i32, (f64::from(cy) + half) as i32);
                let mut x = 0;
                while f64::from(x) <= half {
                    let c = ramp((f64::from(x) / half) as f32);
                    draw_box(r, sx + x, ex - x, sy + x, ey - x, c);
                    x += 1;
                }
            }
            S::Diamond => glow(r, &|r, s, c| draw_diamond(r, cx, cy, s as i32, c)),
            S::FilledDiamond => {
                let mut x = 0;
                while x as f32 <= size {
                    draw_diamond(r, cx, cy, x, ramp(x as f32 / size));
                    x += 1;
                }
            }
            S::Star => glow(r, &|r, s, c| draw_star(r, cx, cy, s, c, points)),
            S::FilledStar => filled_halves(size, |x, d| draw_star(r, cx, cy, x, ramp(d), points)),
            S::Tree => glow(r, &|r, s, c| draw_outline(r, cx, cy, f64::from(s), c, &TREE)),
            S::FilledTree => {
                filled_halves(size, |x, d| draw_outline(r, cx, cy, f64::from(x), ramp(d), &TREE))
            }
            S::Crucifix => glow(r, &|r, s, c| draw_outline(r, cx, cy, f64::from(s), c, &CROSS)),
            S::FilledCrucifix => {
                filled_halves(size, |x, d| {
                    draw_outline(r, cx, cy, f64::from(x), ramp(d), &CROSS)
                });
            }
            S::Present => glow(r, &|r, s, c| draw_outline(r, cx, cy, f64::from(s), c, &PRESENT)),
            S::FilledPresent => {
                let mut x = 0;
                while x as f32 <= size {
                    draw_outline(r, cx, cy, f64::from(x), ramp(x as f32 / size), &PRESENT);
                    x += 1;
                }
            }
            S::CandyCane => glow(r, &|r, s, c| draw_candy_cane(r, cx, cy, f64::from(s), c)),
            S::Snowflake => glow(r, &|r, s, c| draw_snowflake(r, cx, cy, f64::from(s), points, c)),
            S::Heart => glow(r, &|r, s, c| draw_heart(r, cx, cy, f64::from(s), c)),
            S::FilledHeart => {
                let mut x = 0;
                while x as f32 <= size {
                    draw_heart(r, cx, cy, f64::from(x), ramp(x as f32 / size));
                    x += 1;
                }
            }
        }
    }
}

/// The filled shapes drawn every half pixel out to `size` (`for (x = 0; x <= size; x += 0.5f)`),
/// each at its share of the size.
fn filled_halves(size: f32, mut draw: impl FnMut(f32, f32)) {
    let mut x = 0.0f32;
    while x <= size {
        draw(x, x / size);
        x += 0.5;
    }
}

/// `DrawLine` with alpha (`SetPixel(…, useAlpha)`): what's there shows through.
fn line_over(raster: &mut Raster, (x0, y0): (i32, i32), (x1, y1): (i32, i32), c: Rgba) {
    let mut lit = Raster::new(Canvas {
        columns: raster.width as u32,
        rows: raster.height as u32,
    });
    lit.line(x0, y0, x1, y1, c);
    for y in 0..raster.height {
        for x in 0..raster.width {
            let top = lit.get(x, y);
            if top.a > 0.0 {
                raster.cover(x, y, top);
            }
        }
    }
}

/// `DrawCircle`: points every half pixel across, then down, so the circle has no gaps.
fn draw_circle(r: &mut Raster, cx: i32, cy: i32, radius: f32, c: Rgba) {
    if radius <= 0.0 {
        return;
    }
    let squared = radius * radius;
    let (w, h) = (r.width as f32, r.height as f32);
    let mut x = cx as f32 - radius;
    while x <= cx as f32 + radius {
        if x >= 0.0 && x < w {
            let zz = squared - (x - cx as f32) * (x - cx as f32);
            if zz >= 0.0 {
                let y = zz.sqrt() as i32;
                for yy in [y + cy, -y + cy] {
                    if yy >= 0 && (yy as f32) < h {
                        r.set(x as i32, yy, c);
                    }
                }
            }
        }
        x += 0.5;
    }
    let mut y = cy as f32 - radius;
    while y <= cy as f32 + radius {
        if y >= 0.0 && y < h {
            let zz = squared - (y - cy as f32) * (y - cy as f32);
            if zz >= 0.0 {
                let x = zz.sqrt() as i32;
                for xx in [x + cx, -x + cx] {
                    if xx >= 0 && (xx as f32) < w {
                        r.set(xx, y as i32, c);
                    }
                }
            }
        }
        y += 0.5;
    }
}

/// `DrawBox`: an outline from `(sx, sy)` to `(ex, ey)`, both included.
fn draw_box(r: &mut Raster, sx: i32, ex: i32, sy: i32, ey: i32, c: Rgba) {
    for x in sx..=ex {
        if x == sx || x == ex {
            for y in sy..=ey {
                r.set(x, y, c);
            }
        } else {
            r.set(x, sy, c);
            r.set(x, ey, c);
        }
    }
}

/// `DrawDiamond`.
fn draw_diamond(r: &mut Raster, cx: i32, cy: i32, size: i32, c: Rgba) {
    for x in -size..=size {
        let y = size - x.abs();
        r.set(x + cx, y + cy, c);
        r.set(x + cx, -y + cy, c);
    }
}

/// `DrawStar`: lines from each point to the inner corners either side.
fn draw_star(r: &mut Raster, cx: i32, cy: i32, radius: f32, c: Rgba, points: i32) {
    if radius <= 0.0 {
        return;
    }
    let offset = match points {
        5 => 90.0 - 360.0 / 5.0,
        6 => 30.0,
        7 => 90.0 - 360.0 / 7.0,
        _ => 0.0,
    };
    let radius = f64::from(radius);
    let inner = radius / 2.618034;
    let increment = 360.0 / f64::from(points);
    let at = |r: f64, degrees: f64| {
        let a = (offset + degrees) * (PI / 180.0);
        (
            (r * a.cos() + f64::from(cx)) as i32,
            (r * a.sin() + f64::from(cy)) as i32,
        )
    };
    let mut degrees = 0.0;
    while degrees < 361.0 {
        if degrees > 360.0 {
            degrees = 360.0;
        }
        let outer = at(radius, degrees);
        for side in [increment / 2.0, -increment / 2.0] {
            let corner = at(inner, degrees + side);
            r.line(corner.0, corner.1, outer.0, outer.1, c);
        }
        degrees += increment;
    }
}

/// `DrawSnowflake`: lines through the middle, `sides × 2` of them.
fn draw_snowflake(r: &mut Raster, cx: i32, cy: i32, radius: f64, sides: i32, c: Rgba) {
    if radius < 0.0 {
        return;
    }
    let increment = 360.0 / f64::from(sides * 2);
    let mut angle = 0.0f64;
    for _ in 0..sides * 2 {
        let a = angle * PI / 180.0;
        let b = (180.0 + angle) * PI / 180.0;
        let end = |a: f64| {
            (
                (radius * a.cos()).round() as i32 + cx,
                (radius * a.sin()).round() as i32 + cy,
            )
        };
        let (p1, p2) = (end(a), end(b));
        r.line(p1.0, p1.1, p2.0, p2.1, c);
        angle += increment;
    }
}

/// `DrawHeart` (one pixel thick).
fn draw_heart(r: &mut Raster, cx: i32, cy: i32, radius: f64, c: Rgba) {
    if radius < 0.0 {
        return;
    }
    let step = 0.01;
    let mut x = -2.0f64;
    while x <= 2.0 {
        let y1 = (1.0 - (x.abs() - 1.0) * (x.abs() - 1.0)).sqrt();
        let y2 = (1.0 - x.abs()).acos() - PI;
        let xx = (x * radius / 2.0).round() + f64::from(cx);
        let (mut a, mut b) = (
            y1 * radius / 2.0 + f64::from(cy),
            y2 * radius / 2.0 + f64::from(cy),
        );
        r.set(xx as i32, a.round() as i32, c);
        r.set(xx as i32, b.round() as i32, c);
        if x + step > 2.0 || x == -2.0 + step {
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            let mut z = a;
            while z < b {
                r.set(xx as i32, z.round() as i32, c);
                z += 0.5;
            }
        }
        x += step;
    }
}

/// `DrawCandycane` (one pixel thick).
fn draw_candy_cane(r: &mut Raster, cx: i32, cy: i32, radius: f64, c: Rgba) {
    if radius < 0.0 {
        return;
    }
    let y1 = (f64::from(cy) + radius / 6.0).round() as i32;
    let y2 = (f64::from(cy) - radius / 2.0).round() as i32;
    let x = (f64::from(cx) + radius / 2.0).round() as i32;
    r.line(x, y1, x, y2, c);
    let hook = radius / 3.0;
    for degrees in 0..180 {
        let a = f64::from(degrees).to_radians();
        let x = ((hook - 0.75) * a.cos() + f64::from(cx) + radius / 6.0).round() as i32;
        let y = ((hook - 0.75) * a.sin() + f64::from(y1)).round() as i32;
        r.set(x, y, c);
    }
}

/// A shape drawn as lines between points on its own grid: `(x, y)` pairs, the grid's middle,
/// and how wide and tall it is.
struct Outline {
    lines: &'static [[i32; 4]],
    middle: (f64, f64),
    size: (f64, f64),
}

const TREE: Outline = Outline {
    lines: &[
        [3, 0, 5, 0],
        [5, 0, 5, 3],
        [3, 0, 3, 3],
        [0, 3, 8, 3],
        [0, 3, 2, 6],
        [8, 3, 6, 6],
        [1, 6, 2, 6],
        [6, 6, 7, 6],
        [1, 6, 3, 9],
        [7, 6, 5, 9],
        [2, 9, 3, 9],
        [5, 9, 6, 9],
        [6, 9, 4, 11],
        [2, 9, 4, 11],
    ],
    middle: (4.0, 4.0),
    size: (11.0, 11.0),
};

const CROSS: Outline = Outline {
    lines: &[
        [2, 0, 2, 6],
        [2, 6, 0, 6],
        [0, 6, 0, 7],
        [0, 7, 2, 7],
        [2, 7, 2, 10],
        [2, 10, 3, 10],
        [3, 10, 3, 7],
        [3, 7, 5, 7],
        [5, 7, 5, 6],
        [5, 6, 3, 6],
        [3, 6, 3, 0],
        [3, 0, 2, 0],
    ],
    middle: (2.5, 6.5),
    size: (7.0, 10.0),
};

const PRESENT: Outline = Outline {
    lines: &[
        [0, 0, 0, 9],
        [0, 9, 10, 9],
        [10, 9, 10, 0],
        [10, 0, 0, 0],
        [5, 0, 5, 9],
        [5, 9, 2, 11],
        [2, 11, 2, 9],
        [5, 9, 8, 11],
        [8, 11, 8, 9],
    ],
    middle: (5.0, 5.5),
    size: (7.0, 10.0),
};

/// `DrawTree`, `DrawCrucifix`, `DrawPresent` (one pixel thick).
fn draw_outline(r: &mut Raster, cx: i32, cy: i32, radius: f64, c: Rgba, shape: &Outline) {
    if radius < 0.0 {
        return;
    }
    let at = |x: i32, y: i32| {
        (
            ((f64::from(x) - shape.middle.0) / shape.size.0 * radius).round() as i32,
            ((f64::from(y) - shape.middle.1) / shape.size.1 * radius).round() as i32,
        )
    };
    for &[x1, y1, x2, y2] in shape.lines {
        let (a, b) = (at(x1, y1), at(x2, y2));
        r.line(cx + a.0, cy + a.1, cx + b.0, cy + b.1, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logarithmic_bars_take_xlights_note_counts() {
        assert_eq!(log_sum(0), 0);
        assert_eq!(log_sum(1), 18);
        assert_eq!(log_sum(2) - log_sum(1), 10);
        assert_eq!(log_sum(127), log_sum(200));
    }

    #[test]
    fn filters_match_a_word_of_the_label() {
        let mark = |label: &str| Mark::new(0, 10, label);
        assert!(filtered_in(&mark("anything"), ""));
        assert!(filtered_in(&mark("kick, snare"), "snare"));
        assert!(!filtered_in(&mark("kicks"), "kick"));
        assert!(!filtered_in(&mark(""), "kick"));
        assert!(filtered_in(&mark("hat"), "kick;hat"));
    }

    #[test]
    fn the_palette_blends_as_xlights_does() {
        let colors = Colors::new(&[
            pf_sequence::Rgb::RED,
            pf_sequence::Rgb::BLUE,
            pf_sequence::Rgb::GREEN,
        ]);
        assert_eq!(blend(&colors, 0.0, 0), [1.0, 0.0, 0.0]);
        assert_eq!(blend(&colors, 0.5, 0), [0.0, 0.0, 1.0]);
        assert_eq!(
            blend(&colors, 0.5, 1),
            [0.5, 0.0, 0.5],
            "the last color kept back"
        );
        let top = blend(&colors, 1.0, 0);
        assert!(top[1] > 0.99 && top[2] < 0.01);
    }
}
