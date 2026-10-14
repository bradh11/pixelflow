//! xLights DMX floods as PixelFlow DMX fixtures.
//!
//! xLights keeps a DMX model as its channels (`DmxChannelCount`) and, on a flood, which of them
//! carry red, green, blue and white (`DmxRedChannel` and the like, counted from 1, 0 for none)
//! and which is its shutter (`DmxShutterChannel`, held open at `DmxShutterOpen`). A flood-type
//! model (`DmxGeneral`, `DmxFloodlight`, `DmxFloodArea`) becomes a one-cell fixture with those
//! channels; the rest of its channels hold 0. Moving heads, servos and the other DMX models stay
//! as they were (a row of channel dots, not wired).

use crate::model::XmlModel;
use pf_model::{DmxChannel, DmxFixture, DmxRole};

/// The xLights DMX models PixelFlow imports as floods.
pub(crate) fn is_flood(model: &XmlModel) -> bool {
    matches!(
        model.display_as.trim(),
        "DmxGeneral" | "DmxFloodlight" | "DmxFloodArea"
    )
}

/// The channel (1-based) an attribute names, if it names one of `count`.
fn channel(model: &XmlModel, key: &str, count: usize) -> Option<usize> {
    let n: usize = model.text(key, "0").trim().parse().ok()?;
    (1..=count).contains(&n).then_some(n)
}

/// The fixture a flood-type model is, at `address`, and whether its file said which channels are
/// its colors (when it doesn't, every channel holds 0 until it's set up).
pub(crate) fn fixture(model: &XmlModel, address: u16) -> (DmxFixture, bool) {
    let count: usize = model
        .text("DmxChannelCount", "1")
        .trim()
        .parse::<usize>()
        .unwrap_or(1)
        .clamp(1, pf_model::MAX_DMX_CHANNELS);
    let mut channels: Vec<DmxChannel> = (1..=count)
        .map(|n| {
            let mut c = DmxChannel::new(DmxRole::Other);
            c.name = format!("Channel {n}");
            c
        })
        .collect();
    let mut colored = false;
    for (key, role) in [
        ("DmxRedChannel", DmxRole::Red),
        ("DmxGreenChannel", DmxRole::Green),
        ("DmxBlueChannel", DmxRole::Blue),
        ("DmxWhiteChannel", DmxRole::White),
    ] {
        if let Some(n) = channel(model, key, count) {
            channels[n - 1] = DmxChannel::new(role);
            colored = true;
        }
    }
    if let Some(n) = channel(model, "DmxShutterChannel", count) {
        let open = model
            .text("DmxShutterOpen", "0")
            .trim()
            .parse::<f64>()
            .unwrap_or(0.0)
            .clamp(0.0, 255.0);
        channels[n - 1] = DmxChannel::new(DmxRole::Strobe).holding(open.round() as u8);
    }
    (
        DmxFixture {
            address,
            channels,
            layout: None,
        },
        colored,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(display_as: &str, attrs: &[(&str, &str)]) -> XmlModel {
        XmlModel {
            name: "Flood".into(),
            display_as: display_as.into(),
            attrs: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..XmlModel::default()
        }
    }

    #[test]
    fn a_flood_keeps_its_color_and_shutter_channels() {
        let m = model(
            "DmxFloodlight",
            &[
                ("DmxChannelCount", "6"),
                ("DmxRedChannel", "2"),
                ("DmxGreenChannel", "3"),
                ("DmxBlueChannel", "4"),
                ("DmxShutterChannel", "6"),
                ("DmxShutterOpen", "200"),
            ],
        );
        assert!(is_flood(&m));
        let (f, colored) = fixture(&m, 9);
        assert!(colored);
        assert_eq!(f.address, 9);
        let roles: Vec<DmxRole> = f.channels.iter().map(|c| c.role).collect();
        use DmxRole::*;
        assert_eq!(roles, vec![Other, Red, Green, Blue, Other, Strobe]);
        assert_eq!(f.channels[5].value, 200);
    }

    #[test]
    fn a_general_model_without_colors_holds_its_channels_and_heads_arent_floods() {
        let (f, colored) = fixture(
            &model("DmxGeneral", &[("DmxChannelCount", "3"), ("DmxRedChannel", "9")]),
            1,
        );
        assert!(!colored, "a color channel past its last isn't one");
        assert_eq!(f.channels.len(), 3);
        assert!(!is_flood(&model("DmxMovingHead", &[])));
        assert!(!is_flood(&model("DmxServo", &[])));
    }
}
