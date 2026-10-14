//! DMX fixtures in a drawn frame: the colour drawn on each becomes its channels, and DMX effects
//! set channels directly.
//!
//! Effects draw on a fixture's cells like on any prop's pixels. Once the whole frame is drawn,
//! each fixture's colour becomes its channels (`pf_mapping::FixtureLayout::write`, by the rule in
//! `pf_model::write_channels`), the channels the frame's DMX effects set are put over those, and
//! the colour the channels make is written back to its cells for the previews.
//!
//! The DMX effects playing are applied in the sequence's order, rows first to last and each row's
//! layers bottom to top, so where two set one channel the later row (or higher layer) wins. It all
//! happens on one thread after the frame is drawn, so it is the same however many threads drew
//! the frame. The values are kept in lists made with the renderer (and a target's fixtures the
//! first time a DMX effect plays on it), so a frame makes nothing new.

use crate::audio::RenderContext;
use crate::geometry::SceneGeometry;
use pf_sequence::{DmxValues, EffectParams, Sequence, Target, dmx_value};
use std::collections::HashMap;

/// The DMX effects playing in the frame being drawn, and each fixture's channels they set.
#[derive(Debug, Clone, Default)]
pub(crate) struct DmxState {
    /// Each fixture's channels set this frame (`None`: from its colour), in the geometry's
    /// fixture order.
    values: Vec<Vec<Option<u8>>>,
    /// The fixtures each target a DMX effect played on holds (indices into the geometry's).
    targets: HashMap<Target, Vec<u32>>,
    /// The DMX effects playing: row, layer, and place in the layer.
    playing: Vec<(u32, u32, u32)>,
}

impl DmxState {
    pub fn new(geometry: &SceneGeometry) -> Self {
        Self {
            values: geometry
                .fixtures
                .iter()
                .map(|f| vec![None; f.footprint()])
                .collect(),
            ..Self::default()
        }
    }

    /// Forgets the effects of the frame before.
    pub fn clear(&mut self) {
        self.playing.clear();
    }

    /// Notes a DMX effect playing in this frame (in the sequence's order).
    pub fn play(&mut self, row: usize, layer: u32, place: u32) {
        self.playing.push((row as u32, layer, place));
    }

    /// Turns the drawn frame's fixtures' colour into their channels, with the channels the
    /// frame's DMX effects set.
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
        for &(r, l, e) in &self.playing {
            let row = &seq.rows[r as usize];
            let effect = &row.layers[l as usize].effects[e as usize];
            let EffectParams::Dmx(params) = &effect.params else {
                continue;
            };
            let fixtures = self
                .targets
                .entry(row.target)
                .or_insert_with(|| geometry.fixtures_of(row.target));
            for &f in fixtures.iter() {
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
        for (fixture, values) in geometry.fixtures.iter().zip(&self.values) {
            fixture.write(frame, values);
        }
    }
}
