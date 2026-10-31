//! DMX fixtures in a drawn frame: the colour drawn on each becomes its channels, and the DMX,
//! Moving Head and Servo effects set channels directly.
//!
//! Effects draw on a fixture's cells like on any prop's pixels. Once the whole frame is drawn,
//! each fixture's colour becomes its channels (`pf_mapping::FixtureLayout::write`, by the rule in
//! `pf_model::write_channels`), the channels the frame's channel effects set are put over those,
//! and the colour the channels make is written back to its cells for the previews.
//!
//! - The DMX effect sets channels by number.
//! - The Moving Head effect sets each head's pan and tilt (and their fine channels) and, unless
//!   it takes its colour from the layers below, its colour and dimmer channels (or its colour
//!   wheel): the colour it lights the head in becomes channels by the same rule as a drawn one.
//! - The Servo effect sets each servo's channel (and its fine channel).
//!
//! They are applied in the sequence's order, rows first to last and each row's layers bottom to
//! top, so where two set one channel the later row (or higher layer) wins. It all happens on one
//! thread after the frame is drawn, so it is the same however many threads drew the frame. The
//! values are kept in lists made with the renderer (and a target's fixtures the first time a
//! channel effect plays on it), so a frame makes nothing new.

use crate::audio::RenderContext;
use crate::color::Colors;
use crate::geometry::SceneGeometry;
use crate::heads::{FanPlace, fan_place, head_offset, servo_offset, strobe_lit};
use pf_model::{DmxAxis, DmxRole};
use pf_sequence::{
    DmxParams, DmxValues, Effect, EffectParams, HeadColor, MovingHeadParams, Sequence, ServoParams, Target,
    dmx_value,
};
use std::collections::HashMap;

/// The steps a speed curve is added up in, from the effect's start to the moment drawn.
const SPEED_STEPS: u32 = 64;

/// The channel effects playing in the frame being drawn, and each fixture's channels they set.
#[derive(Debug, Clone, Default)]
pub(crate) struct DmxState {
    /// Each fixture's channels set this frame (`None`: from its colour), in the geometry's
    /// fixture order.
    values: Vec<Vec<Option<u8>>>,
    /// Each fixture's motors (see `pf_model::axes`).
    axes: Vec<Vec<DmxAxis>>,
    /// The fixtures each target a channel effect played on holds (indices into the geometry's).
    targets: HashMap<Target, Vec<u32>>,
    /// The channel effects playing: row, layer, and place in the layer.
    playing: Vec<(u32, u32, u32)>,
    /// Room for one fixture's cells and channels while a head's colour becomes its channels.
    cells: Vec<u8>,
    channels: Vec<u8>,
}

impl DmxState {
    pub fn new(geometry: &SceneGeometry) -> Self {
        let fixtures = &geometry.fixtures;
        Self {
            values: fixtures.iter().map(|f| vec![None; f.footprint()]).collect(),
            axes: fixtures.iter().map(|f| pf_model::axes(&f.channels)).collect(),
            cells: vec![0; fixtures.iter().map(|f| f.cells as usize * 3).max().unwrap_or(0)],
            channels: vec![0; fixtures.iter().map(|f| f.footprint()).max().unwrap_or(0)],
            ..Self::default()
        }
    }

    /// Forgets the effects of the frame before.
    pub fn clear(&mut self) {
        self.playing.clear();
    }

    /// Notes a channel effect playing in this frame (in the sequence's order).
    pub fn play(&mut self, row: usize, layer: u32, place: u32) {
        self.playing.push((row as u32, layer, place));
    }

    /// Turns the drawn frame's fixtures' colour into their channels, with the channels the
    /// frame's channel effects set.
    pub fn write(
        &mut self,
        geometry: &SceneGeometry,
        seq: &Sequence,
        t_ms: u64,
        cx: &RenderContext,
        frame: &mut [u8],
    ) {
        if geometry.fixtures.is_empty() {
            return;
        }
        for values in &mut self.values {
            values.fill(None);
        }
        for i in 0..self.playing.len() {
            let (r, l, e) = self.playing[i];
            let row = &seq.rows[r as usize];
            let effect = &row.layers[l as usize].effects[e as usize];
            let fixtures = self
                .targets
                .entry(row.target)
                .or_insert_with(|| geometry.fixtures_of(row.target));
            let fixtures = std::mem::take(fixtures);
            match &effect.params {
                EffectParams::Dmx(p) => self.set_channels(&fixtures, effect, p, t_ms, cx),
                EffectParams::MovingHead(p) => self.point_heads(geometry, &fixtures, effect, p, t_ms, cx),
                EffectParams::Servo(p) => self.turn_servos(&fixtures, effect, p, t_ms, cx),
                _ => {}
            }
            self.targets.insert(row.target, fixtures);
        }
        for (fixture, values) in geometry.fixtures.iter().zip(&self.values) {
            fixture.write(frame, values);
        }
    }

    /// A DMX effect: its channels, by number, on each fixture.
    fn set_channels(
        &mut self,
        fixtures: &[u32],
        effect: &Effect,
        params: &DmxParams,
        t_ms: u64,
        cx: &RenderContext,
    ) {
        for &f in fixtures {
            let values = &mut self.values[f as usize];
            for (&channel, &value) in &params.channels.0 {
                if let Some(slot) = values.get_mut(usize::from(channel) - 1) {
                    *slot = Some(value);
                }
            }
            for (key, curve) in &effect.curves {
                let channel = key.strip_prefix("channels.").and_then(DmxValues::channel_of);
                if let Some(slot) = channel.and_then(|c| values.get_mut(usize::from(c) - 1)) {
                    *slot = Some(dmx_value(cx.curve_value(effect, curve, t_ms)));
                }
            }
        }
    }

    /// A Moving Head effect: each head of the target pointed, and lit, in member order.
    fn point_heads(
        &mut self,
        geometry: &SceneGeometry,
        fixtures: &[u32],
        effect: &Effect,
        params: &MovingHeadParams,
        t_ms: u64,
        cx: &RenderContext,
    ) {
        let params = sanitized(params);
        let is_head = |axes: &[DmxAxis]| {
            axes.iter()
                .any(|a| matches!(a.role, DmxRole::Pan | DmxRole::Tilt))
        };
        let count = fixtures
            .iter()
            .filter(|&&f| is_head(&self.axes[f as usize]))
            .count();
        let colors = Colors::new(&effect.palette.colors);
        let fade = fade_level(effect, t_ms);
        let mut head = 0;
        for &f in fixtures {
            if !is_head(&self.axes[f as usize]) {
                continue;
            }
            let FanPlace { index, place, right } = fan_place(head, count, params.fans);
            // Each head runs `delay` behind the one before it in its fan.
            let delay = f64::from(number(effect, "delay", params.delay, t_ms, cx));
            let at = behind(effect, t_ms, index as f64 * delay);
            let pan = f64::from(number(effect, "pan", params.pan, at, cx));
            let tilt = f64::from(number(effect, "tilt", params.tilt, at, cx));
            let width = f64::from(number(effect, "width", params.width, at, cx));
            let height = f64::from(number(effect, "height", params.height, at, cx));
            let fan_pan = f64::from(number(effect, "fanPan", params.fan_pan, at, cx));
            let fan_tilt = f64::from(number(effect, "fanTilt", params.fan_tilt, at, cx));
            let cycles = cycles(effect, params.speed, at, cx);
            let (dp, dt) = head_offset(params.pattern, cycles, width, height, params.seed, index as u32);
            // Mirrored, the right half of a fan points and moves the other way across.
            let across = if params.mirror && right { -1.0 } else { 1.0 };
            let pan = across * (pan + dp) + place * fan_pan;
            let tilt = tilt + place * fan_tilt + dt;
            let values = &mut self.values[f as usize];
            for axis in &self.axes[f as usize] {
                let degrees = match axis.role {
                    DmxRole::Pan => pan,
                    DmxRole::Tilt => tilt,
                    _ => continue,
                };
                set_axis(values, axis, degrees);
            }
            if params.color != HeadColor::Drawn {
                let color = match params.color {
                    HeadColor::Wash => colors.ramp(effect.progress(at)),
                    HeadColor::PerHead => colors.get(head as u64),
                    _ => colors.get(0),
                };
                let seconds = t_ms.saturating_sub(effect.start_ms) as f64 / 1000.0;
                let strobe = f64::from(number(effect, "strobe", params.strobe, t_ms, cx));
                let level = if strobe_lit(strobe, seconds) {
                    number(effect, "dimmer", params.dimmer, at, cx).clamp(0.0, 100.0) / 100.0 * fade
                } else {
                    0.0
                };
                self.light(geometry, f as usize, color, level);
            }
            head += 1;
        }
    }

    /// Sets fixture `f`'s colour and dimmer channels (or its colour wheel) to what `color` at
    /// `level` makes, by the rule a colour drawn on it follows.
    fn light(&mut self, geometry: &SceneGeometry, f: usize, color: [f32; 3], level: f32) {
        let fixture = &geometry.fixtures[f];
        let cells = &mut self.cells[..fixture.cells as usize * 3];
        let byte = |v: f32| (v * level * 255.0).round().clamp(0.0, 255.0) as u8;
        for cell in cells.as_chunks_mut::<3>().0 {
            *cell = color.map(byte);
        }
        let channels = &mut self.channels[..fixture.footprint()];
        pf_model::write_channels(&fixture.channels, cells, channels);
        let values = &mut self.values[f];
        for ((slot, channel), &value) in values.iter_mut().zip(&fixture.channels).zip(channels.iter()) {
            if channel.from_color() {
                *slot = Some(value);
            }
        }
    }

    /// A Servo effect: each servo of the target (or only the one it names, of each fixture)
    /// turned, fanned out in order.
    fn turn_servos(
        &mut self,
        fixtures: &[u32],
        effect: &Effect,
        params: &ServoParams,
        t_ms: u64,
        cx: &RenderContext,
    ) {
        let params = sanitized_servo(params);
        let chosen = |nth: usize| params.servo == 0 || nth + 1 == params.servo as usize;
        let servos = |axes: &[DmxAxis]| {
            let mut nth = 0;
            axes.iter()
                .filter(|a| a.role == DmxRole::Servo)
                .filter(|_| {
                    nth += 1;
                    chosen(nth - 1)
                })
                .count()
        };
        let count: usize = fixtures.iter().map(|&f| servos(&self.axes[f as usize])).sum();
        let middle = (count.max(1) - 1) as f64 / 2.0;
        let mut index = 0;
        for &f in fixtures {
            let mut nth = 0;
            for axis in &self.axes[f as usize] {
                if axis.role != DmxRole::Servo {
                    continue;
                }
                nth += 1;
                if !chosen(nth - 1) {
                    continue;
                }
                let delay = f64::from(number(effect, "delay", params.delay, t_ms, cx));
                let at = behind(effect, t_ms, index as f64 * delay);
                let position = f64::from(number(effect, "position", params.position, at, cx));
                let size = f64::from(number(effect, "size", params.size, at, cx));
                let fan = f64::from(number(effect, "fan", params.fan, at, cx));
                let cycles = cycles(effect, params.speed, at, cx);
                let degrees = position
                    + (index as f64 - middle) * fan
                    + servo_offset(params.pattern, cycles, size, params.steps);
                set_axis(&mut self.values[f as usize], axis, degrees);
                index += 1;
            }
        }
    }
}

/// A motor's channels set for `degrees` from Home (its limits keep it within them).
fn set_axis(values: &mut [Option<u8>], axis: &DmxAxis, degrees: f64) {
    let (coarse, fine) = axis.values(degrees);
    if let Some(slot) = values.get_mut(axis.coarse) {
        *slot = Some(coarse);
    }
    if let (Some(at), Some(fine)) = (axis.fine, fine)
        && let Some(slot) = values.get_mut(at)
    {
        *slot = Some(fine);
    }
}

/// The moment `seconds` before `t_ms`, but not before the effect's start: a head running behind
/// waits at the start.
fn behind(effect: &Effect, t_ms: u64, seconds: f64) -> u64 {
    let back = (seconds.max(0.0) * 1000.0).round() as u64;
    t_ms.saturating_sub(back).max(effect.start_ms)
}

/// The number setting `key` at `t_ms`: its curve's value there, or the setting.
fn number(effect: &Effect, key: &str, setting: f32, t_ms: u64, cx: &RenderContext) -> f32 {
    match effect.curves.get(key) {
        Some(curve) => cx.curve_value(effect, curve, t_ms),
        None => setting,
    }
}

/// Times round its movement by `t_ms`: the speed (times a second) added up from the effect's
/// start, following its curve where it has one.
fn cycles(effect: &Effect, speed: f32, t_ms: u64, cx: &RenderContext) -> f64 {
    let elapsed = t_ms.saturating_sub(effect.start_ms) as f64;
    let Some(curve) = effect.curves.get("speed") else {
        return f64::from(speed) * elapsed / 1000.0;
    };
    let step = elapsed / f64::from(SPEED_STEPS);
    let sum: f64 = (0..SPEED_STEPS)
        .map(|k| {
            let at = effect.start_ms as f64 + (f64::from(k) + 0.5) * step;
            f64::from(cx.curve_value(effect, curve, at.round() as u64).clamp(0.0, 10.0))
        })
        .sum();
    sum * step / 1000.0
}

/// How much of its light an effect's fades leave at `t_ms` (0 to 1).
fn fade_level(effect: &Effect, t_ms: u64) -> f32 {
    let mut level = 1.0f32;
    if effect.fade_in_ms > 0 {
        level = level.min(t_ms.saturating_sub(effect.start_ms) as f32 / effect.fade_in_ms as f32);
    }
    if effect.fade_out_ms > 0 {
        level = level.min(effect.end_ms.saturating_sub(t_ms) as f32 / effect.fade_out_ms as f32);
    }
    level.clamp(0.0, 1.0)
}

fn sanitized(params: &MovingHeadParams) -> MovingHeadParams {
    let mut p = *params;
    p.sanitize();
    p
}

fn sanitized_servo(params: &ServoParams) -> ServoParams {
    let mut p = *params;
    p.sanitize();
    p
}
