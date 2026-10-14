//! What a DMX effect sets a fixture's channels to.
//!
//! The DMX effect sets channels of the DMX fixtures it plays on directly: channel `n` (1-based,
//! counted from the fixture's own start address) to a value from 0 to 255. Only the channels it
//! names are set; the rest come from the colour drawn on the fixture as usual. A curve changes
//! one of them over the effect: its key is `channels.` and the channel's number.

use crate::settings::{SettingField, SettingRange};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The highest channel a DMX effect can set (a whole universe).
pub const MAX_DMX_EFFECT_CHANNEL: u16 = 512;

/// The channels a DMX effect sets, by number (1-based, from the fixture's address), to 0-255.
/// Saved as an object keyed by the channel's number: `{ "1": 255, "7": 200 }`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct DmxValues(pub BTreeMap<u16, u8>);

/// Read through the text of each key: inside an effect's `params` (tagged by its `kind`), the keys
/// arrive as text, which a number key wouldn't take.
impl<'de> Deserialize<'de> for DmxValues {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = BTreeMap::<String, u8>::deserialize(deserializer)?;
        raw.into_iter()
            .map(|(key, value)| match key.trim().parse::<u16>() {
                Ok(channel) => Ok((channel, value)),
                Err(_) => Err(serde::de::Error::custom(format!(
                    "\"{key}\" isn't a channel number"
                ))),
            })
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(DmxValues)
    }
}

impl DmxValues {
    pub fn new(values: impl IntoIterator<Item = (u16, u8)>) -> Self {
        Self(values.into_iter().collect())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, channel: u16) -> Option<u8> {
        self.0.get(&channel).copied()
    }

    pub fn set(&mut self, channel: u16, value: u8) {
        self.0.insert(channel, value);
    }

    /// The channel a curve's `part` (what follows `channels.` in its key) names, if it's one.
    pub fn channel_of(part: &str) -> Option<u16> {
        let n: u16 = part.parse().ok()?;
        (1..=MAX_DMX_EFFECT_CHANNEL).contains(&n).then_some(n)
    }
}

/// A value from a curve, as a channel holds it.
pub fn dmx_value(value: f32) -> u8 {
    // Saturating: NaN becomes 0.
    value.round().clamp(0.0, 255.0) as u8
}

impl SettingField for DmxValues {
    /// Drops channels outside 1-512.
    fn sanitize(&mut self, _range: &SettingRange, _default: Self) {
        self.0.retain(|&n, _| (1..=MAX_DMX_EFFECT_CHANNEL).contains(&n));
    }

    fn problem(&self, _range: &SettingRange) -> Option<String> {
        self.0
            .keys()
            .find(|&&n| !(1..=MAX_DMX_EFFECT_CHANNEL).contains(&n))
            .map(|n| format!("sets channel {n}; channels are 1 to {MAX_DMX_EFFECT_CHANNEL}"))
    }

    fn set_part(&mut self, part: &str, value: f32) -> bool {
        match Self::channel_of(part) {
            Some(n) => {
                self.0.insert(n, dmx_value(value));
                true
            }
            None => false,
        }
    }

    fn part(&self, part: &str) -> Option<f32> {
        Self::channel_of(part).and_then(|n| self.get(n)).map(f32::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_save_by_channel_number() {
        let values = DmxValues::new([(1, 255), (7, 200)]);
        let json = serde_json::to_value(&values).unwrap();
        assert_eq!(json, serde_json::json!({ "1": 255, "7": 200 }));
        assert_eq!(serde_json::from_value::<DmxValues>(json).unwrap(), values);
    }

    #[test]
    fn a_curve_sets_one_channel_by_its_number() {
        let mut values = DmxValues::default();
        assert!(values.set_part("7", 127.6));
        assert_eq!(values.get(7), Some(128));
        assert!(values.set_part("8", 400.0), "clamped");
        assert_eq!(values.get(8), Some(255));
        assert!(!values.set_part("0", 1.0));
        assert!(!values.set_part("513", 1.0));
        assert!(!values.set_part("red", 1.0));
        assert_eq!(values.part("7"), Some(128.0));
        assert_eq!(values.part("9"), None);
    }

    #[test]
    fn curves_drive_one_channel_and_the_effect_round_trips() {
        use crate::{Effect, EffectKind, EffectParams};
        let range = Effect::curve_range(EffectKind::Dmx, "channels.7").unwrap();
        assert_eq!((range.min, range.max, range.whole), (0.0, 255.0, true));
        assert!(Effect::curve_range(EffectKind::Dmx, "channels.0").is_none());
        assert!(Effect::curve_range(EffectKind::Dmx, "channels.red").is_none());
        assert!(Effect::curve_range(EffectKind::On, "channels.1").is_none());
        let mut params = EffectParams::default_for(EffectKind::Dmx);
        assert!(params.set_number("channels.3", 42.0));
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "kind": "dmx", "channels": { "3": 42 } })
        );
        assert_eq!(serde_json::from_value::<EffectParams>(json).unwrap(), params);
    }

    #[test]
    fn channels_outside_a_universe_are_dropped() {
        let mut values = DmxValues::new([(0, 1), (3, 2), (600, 3)]);
        assert!(values.problem(&SettingRange::DmxChannels).is_some());
        values.sanitize(&SettingRange::DmxChannels, DmxValues::default());
        assert_eq!(values, DmxValues::new([(3, 2)]));
        assert_eq!(values.problem(&SettingRange::DmxChannels), None);
    }
}
