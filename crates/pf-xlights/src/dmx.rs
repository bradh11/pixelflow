//! xLights DMX models as PixelFlow DMX fixtures: floods, moving heads and servos.
//!
//! xLights keeps a DMX model as its channels (`DmxChannelCount`) and which of them do what,
//! each counted from 1 (0 for none):
//!
//! - **Floods** (`DmxGeneral`, `DmxFloodlight`, `DmxFloodArea`): red, green, blue and white
//!   (`DmxRedChannel` and the like) and a shutter (`DmxShutterChannel`: closed below
//!   `DmxShutterOpen`, open from it up, and held at `DmxShutterOnValue`, the value xLights sends
//!   to open it, or at `DmxShutterOpen` where the file has none). One cell.
//! - **Moving heads** (`DmxMovingHeadAdv`, `DmxMovingHead`, `DmxMovingHead3D`): the same colour
//!   and shutter channels, or a colour wheel (`DmxColorType` 1: `DmxColorWheelChannel`, its
//!   colours `DmxColorWheelColor<n>` and the values selecting them `DmxColorWheelDMX<n>`, and
//!   its brightness on `DmxDimmerChannel`) or CMY (`DmxColorType` 2), a dimmer
//!   (`MhDimmerChannel` from xLights 2026; `DmxDimmerChannel` in older files), a gobo
//!   (`DmxGoboChannel`), the beam's width (`DmxBeamWidth`, degrees), and pan and tilt: a
//!   `PanMotor` and a `TiltMotor` element (`ChannelCoarse`, `ChannelFine`, `RangeOfMotion` in
//!   degrees, `OrientHome`: degrees from the value-0 position to Home, `MinLimit` and
//!   `MaxLimit`: degrees from Home, `Reverse`, `UpsideDown`: which way it turns, see
//!   `head_motor`), or in older files `DmxPanChannel`, `DmxPanDegOfRot` and the tilt's likewise.
//!   One cell.
//! - **Servos** (`DmxServo`, `DmxServo3d`): `NumServos` of them, 16-bit unless `Bits16` is 0,
//!   each a `Servo<n>` element (`Channel`, `MinLimit` and `MaxLimit` as values on its 8- or
//!   16-bit scale, `RangeOfMotion` in degrees); one after another from channel 1 when the file
//!   doesn't say. A servo whose `MinLimit` is the larger turns the other way, as xLights turns
//!   it (xLights 2026 sends the same channels whatever a servo's `Reverse` says). A cell each,
//!   where its arm is drawn.
//!
//! Any of them can have presets (`DmxPresetChannel<n>`, `DmxPresetValue<n>`, and a name,
//! `DmxPresetDesc<n>`): xLights holds those channels at those values while an effect other
//! than DMX lights the model, and so does the import, always.
//!
//! A head's pan and tilt, and each servo, rest at Home when nothing moves them, as PixelFlow's own
//! heads and servos do.
//! Every other channel holds 0, as xLights sends a channel nothing sets. Skulls and other DMX
//! models stay as they were (a row of channel dots, not wired).
//!
//! xLights keeps a DMX model's channels as single channels (`StringType="Single Color White"`,
//! which it writes for every DMX model); the import takes them as single channels whatever the
//! file says.

use crate::model::XmlModel;
use pf_model::{
    DmxBeam, DmxChannel, DmxFixture, DmxMotor, DmxRole, DmxShutter, DmxShutterRange, DmxSlot, Rgb,
};
use std::collections::BTreeMap;

/// What kind of xLights DMX model PixelFlow imports a model as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Flood,
    Head,
    Servo,
}

/// The kind of fixture an xLights model becomes, if it becomes one.
pub(crate) fn kind(model: &XmlModel) -> Option<Kind> {
    match model.display_as.trim() {
        "DmxGeneral" | "DmxFloodlight" | "DmxFloodArea" => Some(Kind::Flood),
        "DmxMovingHeadAdv" | "DmxMovingHead" | "DmxMovingHead3D" => Some(Kind::Head),
        "DmxServo" | "DmxServo3d" | "DmxServo3D" => Some(Kind::Servo),
        _ => None,
    }
}

/// The xLights DMX models PixelFlow has no fixture for: they stay a row of channel dots.
pub(crate) fn has_no_equivalent(model: &XmlModel) -> bool {
    let shown = model.display_as.trim();
    shown.starts_with("Dmx") && kind(model).is_none()
}

/// A DMX model as a fixture.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Imported {
    pub fixture: DmxFixture,
    /// How many cells it has: one, or one a servo.
    pub cells: u32,
    /// The file said which channels are its colours (a flood without them holds every channel).
    pub colored: bool,
    /// Hung upside down: the prop is turned half round in the front plane.
    pub upside_down: bool,
    /// A colour wheel whose colours the file doesn't list: its beam is drawn white.
    pub plain_wheel: bool,
    /// A head whose file gives neither pan nor tilt channel.
    pub unmoving: bool,
}

/// The channel (1-based) a value names, if it names one of `count`.
fn channel_in(text: Option<&str>, count: usize) -> Option<usize> {
    let n: usize = text?.trim().parse().ok()?;
    (1..=count).contains(&n).then_some(n)
}

fn number(text: Option<&str>) -> Option<f64> {
    text?.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

fn flag(text: Option<&str>) -> bool {
    matches!(text.map(str::trim), Some("1" | "true" | "TRUE" | "True"))
}

/// The fixture a DMX model of `kind` is, at `address`.
pub(crate) fn fixture(model: &XmlModel, kind: Kind, address: u16) -> Imported {
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
    let mut out = Imported {
        fixture: DmxFixture {
            address,
            channels: Vec::new(),
            layout: None,
            beam: None,
        },
        cells: 1,
        colored: false,
        upside_down: false,
        plain_wheel: false,
        unmoving: false,
    };
    let mut set = |at: Option<usize>, channel: DmxChannel| -> bool {
        match at {
            Some(n) => {
                channels[n - 1] = channel;
                true
            }
            None => false,
        }
    };
    match kind {
        Kind::Flood => {
            out.colored = colors(&mut set, model, count);
            shutter(&mut set, model, count);
        }
        Kind::Head => {
            out.colored = match model.attr("DmxColorType").map(str::trim) {
                Some("1") => {
                    let wheel = channel_in(model.attr("DmxColorWheelChannel"), count);
                    let slots = wheel_slots(model);
                    out.plain_wheel = wheel.is_some() && slots.is_empty();
                    let mut channel = DmxChannel::new(DmxRole::ColorWheel);
                    channel.slots = slots;
                    set(wheel, channel)
                }
                Some("2") => {
                    let mut any = false;
                    for (key, role) in [
                        ("DmxCyanChannel", DmxRole::Cyan),
                        ("DmxMagentaChannel", DmxRole::Magenta),
                        ("DmxYellowChannel", DmxRole::Yellow),
                        ("DmxWhiteChannel", DmxRole::White),
                    ] {
                        any |= set(channel_in(model.attr(key), count), DmxChannel::new(role));
                    }
                    any
                }
                _ => colors(&mut set, model, count),
            };
            // The head's dimmer is `MhDimmerChannel` in files from xLights 2026 and
            // `DmxDimmerChannel` in older ones. A colour wheel's brightness is
            // `DmxDimmerChannel` in either; on a head of other colours with a
            // `MhDimmerChannel`, xLights 2026 leaves `DmxDimmerChannel` at 0.
            let wheel = matches!(model.attr("DmxColorType").map(str::trim), Some("1"));
            let dimmer = channel_in(model.attr("MhDimmerChannel"), count);
            let older = channel_in(model.attr("DmxDimmerChannel"), count);
            set(dimmer.or(older), DmxChannel::new(DmxRole::Dimmer));
            if wheel {
                set(older, DmxChannel::new(DmxRole::Dimmer));
            }
            set(
                channel_in(model.attr("DmxGoboChannel"), count),
                DmxChannel::new(DmxRole::Gobo),
            );
            shutter(&mut set, model, count);
            let mut moving = false;
            for (part, role, flat, flat_range, range) in [
                ("PanMotor", DmxRole::Pan, "DmxPanChannel", "DmxPanDegOfRot", 540.0),
                (
                    "TiltMotor",
                    DmxRole::Tilt,
                    "DmxTiltChannel",
                    "DmxTiltDegOfRot",
                    180.0,
                ),
            ] {
                let fine_role = role.fine().unwrap_or(role);
                let (coarse, fine, motor, upside_down) = match model.parts.get(part) {
                    Some(m) => {
                        let get = |k: &str| m.get(k).map(String::as_str);
                        (
                            channel_in(get("ChannelCoarse"), count),
                            channel_in(get("ChannelFine"), count),
                            head_motor(&get, range),
                            flag(get("UpsideDown")),
                        )
                    }
                    None => {
                        let range = number(model.attr(flat_range)).unwrap_or(range);
                        (
                            channel_in(model.attr(flat), count),
                            None,
                            DmxMotor::centred(range.clamp(1.0, 1080.0) as f32),
                            false,
                        )
                    }
                };
                out.upside_down |= upside_down;
                let mut channel = DmxChannel::new(role);
                channel.motor = Some(motor);
                moving |= set(coarse, channel);
                if coarse.is_some() {
                    set(fine, DmxChannel::new(fine_role));
                }
            }
            out.unmoving = !moving;
            let angle = number(model.attr("DmxBeamWidth")).unwrap_or(f64::from(DmxBeam::default().angle_deg));
            out.fixture.beam = Some(DmxBeam {
                angle_deg: angle.clamp(1.0, 120.0) as f32,
                ..DmxBeam::default()
            });
        }
        Kind::Servo => {
            let servos = number(model.attr("NumServos")).unwrap_or(1.0).clamp(1.0, 64.0) as usize;
            let wide = !matches!(model.attr("Bits16").map(str::trim), Some("0" | "false"));
            let width = if wide { 2 } else { 1 };
            let full = if wide { 65535.0 } else { 255.0 };
            let mut placed = 0;
            for k in 1..=servos {
                let part = model.parts.get(&format!("Servo{k}"));
                let get = |key: &str| part.and_then(|m| m.get(key)).map(String::as_str);
                let at = channel_in(get("Channel"), count).or_else(|| {
                    let n = 1 + (k - 1) * width;
                    (n + width - 1 <= count).then_some(n)
                });
                let Some(at) = at else { continue };
                let mut channel = DmxChannel::new(DmxRole::Servo);
                channel.name = if servos > 1 {
                    format!("Servo {k}")
                } else {
                    "Servo".into()
                };
                channel.motor = Some(servo_motor(&get, full));
                set(Some(at), channel);
                if wide && at < count {
                    let mut fine = DmxChannel::new(DmxRole::ServoFine);
                    fine.name = if servos > 1 {
                        format!("Servo fine {k}")
                    } else {
                        "Servo fine".into()
                    };
                    set(Some(at + 1), fine);
                }
                placed += 1;
            }
            out.cells = placed.max(1);
        }
    }
    presets(&mut channels, model);
    if matches!(kind, Kind::Head | Kind::Servo) {
        // Where nothing moves them, a head and a servo rest at Home (xLights would send their
        // motors 0).
        pf_model::rest_motors_at(&mut channels, 0.0);
    }
    out.fixture.channels = channels;
    out
}

/// Sets the red, green, blue and white channels the file names; whether it named any.
fn colors(set: &mut impl FnMut(Option<usize>, DmxChannel) -> bool, model: &XmlModel, count: usize) -> bool {
    let mut any = false;
    for (key, role) in [
        ("DmxRedChannel", DmxRole::Red),
        ("DmxGreenChannel", DmxRole::Green),
        ("DmxBlueChannel", DmxRole::Blue),
        ("DmxWhiteChannel", DmxRole::White),
    ] {
        any |= set(channel_in(model.attr(key), count), DmxChannel::new(role));
    }
    any
}

/// A value of a channel from the attribute `key`, if the file gives one.
fn level(model: &XmlModel, key: &str) -> Option<u8> {
    number(model.attr(key)).map(|v| v.clamp(0.0, 255.0).round() as u8)
}

/// Sets the shutter channel the file names: closed below the value that opens it
/// (`DmxShutterOpen`), open from it up (open at every value when that is 0), and held at the
/// value xLights sends to open it (`DmxShutterOnValue`; 0, or none, in older files, where it is
/// held at the value that opens it).
fn shutter(set: &mut impl FnMut(Option<usize>, DmxChannel) -> bool, model: &XmlModel, count: usize) {
    let open = level(model, "DmxShutterOpen").unwrap_or(0);
    let held = level(model, "DmxShutterOnValue")
        .filter(|&v| v > 0)
        .unwrap_or(open);
    let mut channel = DmxChannel::new(DmxRole::Strobe).holding(held);
    let range = |from, does| DmxShutterRange { from, does };
    channel.shutter = if open > 0 {
        vec![range(0, DmxShutter::Closed), range(open, DmxShutter::Open)]
    } else {
        vec![range(0, DmxShutter::Open)]
    };
    set(channel_in(model.attr("DmxShutterChannel"), count), channel);
}

/// Holds each preset's channel at its value (`DmxPresetChannel<n>`, `DmxPresetValue<n>`), as
/// xLights does while an effect other than DMX lights the model. A channel with no part of its
/// own takes the preset's name (`DmxPresetDesc<n>`).
fn presets(channels: &mut [DmxChannel], model: &XmlModel) {
    for (key, at) in &model.attrs {
        let Some(n) = key
            .strip_prefix("DmxPresetChannel")
            .filter(|n| n.parse::<u32>().is_ok())
        else {
            continue;
        };
        let (Some(at), Some(value)) = (
            channel_in(Some(at), channels.len()),
            level(model, &format!("DmxPresetValue{n}")),
        ) else {
            continue;
        };
        let channel = &mut channels[at - 1];
        channel.value = value;
        let name = model
            .attr(&format!("DmxPresetDesc{n}"))
            .map(str::trim)
            .filter(|d| !d.is_empty());
        if let (DmxRole::Other, Some(name)) = (channel.role, name) {
            channel.name = name.chars().take(pf_model::MAX_DMX_CHANNEL_NAME).collect();
        }
    }
}

/// A head's pan or tilt motor from its element's attributes (`range`: when it doesn't say).
///
/// xLights turns a head's motor the other way from PixelFlow's own as its value rises: a pan or
/// tilt of +45° from Home is sent as Home less 45°. `Reverse` turns it back, and so does
/// `UpsideDown` (a head hung upside down turns the other way to point the same way), each on its
/// own motor; both together cancel.
fn head_motor<'a>(get: &impl Fn(&str) -> Option<&'a str>, range: f64) -> DmxMotor {
    let range = number(get("RangeOfMotion")).unwrap_or(range).clamp(1.0, 1080.0);
    let home = number(get("OrientHome")).map_or(range / 2.0, |h| h.clamp(0.0, range));
    let limits = match (number(get("MinLimit")), number(get("MaxLimit"))) {
        (Some(a), Some(b)) => Some((a.min(b).min(0.0), a.max(b).max(0.0))),
        _ => None,
    };
    let reverse = flag(get("Reverse")) == flag(get("UpsideDown"));
    let mut motor = DmxMotor {
        range_deg: range as f32,
        home_deg: home as f32,
        min_deg: None,
        max_deg: None,
        reverse,
    };
    if let Some((lo, hi)) = limits {
        let (low, high) = motor.travel();
        motor.min_deg = (lo > low).then_some(lo as f32);
        motor.max_deg = (hi < high).then_some(hi as f32);
    }
    motor
}

/// A servo's motor from its element's attributes: limits as values on a scale up to `full`,
/// Home in the middle of them. xLights turns a servo from its `MinLimit` to its `MaxLimit`, so
/// one whose `MinLimit` is the larger turns the other way.
fn servo_motor<'a>(get: &impl Fn(&str) -> Option<&'a str>, full: f64) -> DmxMotor {
    let range = number(get("RangeOfMotion")).unwrap_or(180.0).clamp(1.0, 1080.0);
    let a = number(get("MinLimit")).unwrap_or(0.0).clamp(0.0, full);
    let b = number(get("MaxLimit")).unwrap_or(full).clamp(0.0, full);
    let (lo, hi) = (a.min(b) / full * range, a.max(b) / full * range);
    let home = (lo + hi) / 2.0;
    let reverse = a > b;
    let mut motor = DmxMotor {
        range_deg: range as f32,
        home_deg: home as f32,
        min_deg: None,
        max_deg: None,
        reverse,
    };
    // Limits in degrees from Home, the right way round for a reversed servo.
    let (low, high) = motor.travel();
    let (from, to) = if reverse {
        (home - hi, home - lo)
    } else {
        (lo - home, hi - home)
    };
    motor.min_deg = (from > low + 1e-9).then_some(from as f32);
    motor.max_deg = (to < high - 1e-9).then_some(to as f32);
    motor
}

/// A colour wheel's colours from the file: `DmxColorWheelColor<n>` (`#rrggbb`) selected by
/// `DmxColorWheelDMX<n>`.
fn wheel_slots(model: &XmlModel) -> Vec<DmxSlot> {
    let mut slots: BTreeMap<u32, DmxSlot> = BTreeMap::new();
    for (key, value) in &model.attrs {
        let Some(n) = key
            .strip_prefix("DmxColorWheelColor")
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let Some(color) = Rgb::from_hex(value.trim()) else {
            continue;
        };
        let Some(dmx) = number(model.attr(&format!("DmxColorWheelDMX{n}"))) else {
            continue;
        };
        slots.insert(
            n,
            DmxSlot {
                value: dmx.clamp(0.0, 255.0).round() as u8,
                color: Some(color),
                name: None,
            },
        );
    }
    slots.into_values().take(pf_model::MAX_DMX_SLOTS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(display_as: &str, attrs: &[(&str, &str)]) -> XmlModel {
        XmlModel {
            name: "Fixture".into(),
            display_as: display_as.into(),
            attrs: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..XmlModel::default()
        }
    }

    fn with_part(mut m: XmlModel, part: &str, attrs: &[(&str, &str)]) -> XmlModel {
        m.parts.insert(
            part.into(),
            attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        m
    }

    fn roles(f: &DmxFixture) -> Vec<DmxRole> {
        f.channels.iter().map(|c| c.role).collect()
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
        assert_eq!(kind(&m), Some(Kind::Flood));
        let found = fixture(&m, Kind::Flood, 9);
        assert!(found.colored);
        assert_eq!(found.fixture.address, 9);
        use DmxRole::*;
        assert_eq!(
            roles(&found.fixture),
            vec![Other, Red, Green, Blue, Other, Strobe]
        );
        let shutter = &found.fixture.channels[5];
        assert_eq!(shutter.value, 200);
        let at = pf_model::Moment::new(0, 25);
        assert!(
            pf_model::shutter_shows(shutter.shutter().unwrap(), 200, at),
            "open at 200"
        );
        assert!(
            pf_model::shutter_shows(shutter.shutter().unwrap(), 255, at),
            "and above"
        );
        assert!(
            !pf_model::shutter_shows(shutter.shutter().unwrap(), 199, at),
            "closed below"
        );
        assert_eq!(found.fixture.beam, None);
    }

    #[test]
    fn a_general_model_without_colors_holds_its_channels_and_skulls_have_no_equivalent() {
        let found = fixture(
            &model("DmxGeneral", &[("DmxChannelCount", "3"), ("DmxRedChannel", "9")]),
            Kind::Flood,
            1,
        );
        assert!(!found.colored, "a color channel past its last isn't one");
        assert_eq!(found.fixture.channels.len(), 3);
        assert_eq!(kind(&model("DmxMovingHeadAdv", &[])), Some(Kind::Head));
        assert_eq!(kind(&model("DmxServo", &[])), Some(Kind::Servo));
        assert_eq!(kind(&model("DmxSkull", &[])), None);
        assert!(has_no_equivalent(&model("DmxSkull", &[])));
        assert!(!has_no_equivalent(&model("Single Line", &[])));
    }

    #[test]
    fn a_16_bit_head_keeps_its_motors_colors_and_shutter_at_its_channels() {
        let m = model(
            "DmxMovingHeadAdv",
            &[
                ("DmxChannelCount", "12"),
                ("DmxColorType", "0"),
                ("DmxRedChannel", "7"),
                ("DmxGreenChannel", "8"),
                ("DmxBlueChannel", "9"),
                ("DmxWhiteChannel", "10"),
                ("DmxDimmerChannel", "6"),
                ("DmxShutterChannel", "11"),
                ("DmxShutterOpen", "8"),
                ("DmxBeamWidth", "25"),
            ],
        );
        let m = with_part(
            m,
            "PanMotor",
            &[
                ("ChannelCoarse", "1"),
                ("ChannelFine", "2"),
                ("RangeOfMotion", "540"),
                ("OrientHome", "270"),
                ("MinLimit", "-180"),
                ("MaxLimit", "180"),
            ],
        );
        let m = with_part(
            m,
            "TiltMotor",
            &[
                ("ChannelCoarse", "3"),
                ("ChannelFine", "4"),
                ("RangeOfMotion", "270"),
                ("OrientHome", "135"),
                ("Reverse", "1"),
                ("UpsideDown", "1"),
            ],
        );
        let found = fixture(&m, Kind::Head, 20);
        use DmxRole::*;
        assert_eq!(
            roles(&found.fixture),
            vec![
                Pan, PanFine, Tilt, TiltFine, Other, Dimmer, Red, Green, Blue, White, Strobe, Other
            ]
        );
        let f = &found.fixture;
        // An upright pan turns the other way from PixelFlow's own as its value rises, as xLights
        // turns it.
        assert_eq!(
            f.channels[0].motor,
            Some(DmxMotor {
                range_deg: 540.0,
                home_deg: 270.0,
                min_deg: Some(-180.0),
                max_deg: Some(180.0),
                reverse: true,
            })
        );
        // Reversed and upside down: the two turn it back twice.
        let tilt = f.channels[2].motor.unwrap();
        assert!(tilt.reverse);
        assert_eq!(
            (tilt.min_deg, tilt.max_deg),
            (None, None),
            "no limits in the file"
        );
        assert_eq!(f.channels[10].value, 8, "the shutter held open");
        // At rest, Home: pan 270° of 540° is the middle (128, 0); the reversed tilt's Home, 135°
        // of 270°, is the middle too.
        let held: Vec<u8> = f.channels[..4].iter().map(|c| c.value).collect();
        assert_eq!(held, [128, 0, 128, 0], "pan and tilt rest at Home");
        assert_eq!(f.beam.unwrap().angle_deg, 25.0);
        assert!(found.colored && found.upside_down && !found.unmoving);
        assert_eq!(f.problem(1), None);
    }

    #[test]
    fn a_head_with_a_color_wheel_or_cmy_or_older_pan_and_tilt() {
        let wheel = model(
            "DmxMovingHead",
            &[
                ("DmxChannelCount", "8"),
                ("DmxColorType", "1"),
                ("DmxColorWheelChannel", "5"),
                ("DmxDimmerChannel", "6"),
                ("DmxColorWheelColor0", "#FFFFFF"),
                ("DmxColorWheelDMX0", "0"),
                ("DmxColorWheelColor1", "#FF0000"),
                ("DmxColorWheelDMX1", "16"),
                ("DmxColorWheelColor2", "not a color"),
                ("DmxColorWheelDMX2", "32"),
                ("DmxPanChannel", "1"),
                ("DmxPanDegOfRot", "630"),
                ("DmxTiltChannel", "3"),
                ("DmxRedChannel", "8"),
            ],
        );
        let found = fixture(&wheel, Kind::Head, 1);
        let f = &found.fixture;
        use DmxRole::*;
        assert_eq!(
            roles(f),
            vec![Pan, Other, Tilt, Other, ColorWheel, Dimmer, Other, Other]
        );
        assert_eq!(
            f.channels[4].slots,
            vec![
                DmxSlot {
                    value: 0,
                    color: Some(Rgb::WHITE),
                    name: None
                },
                DmxSlot {
                    value: 16,
                    color: Some(Rgb::RED),
                    name: None
                },
            ]
        );
        assert_eq!(f.channels[0].motor, Some(DmxMotor::centred(630.0)));
        assert_eq!(f.channels[2].motor, Some(DmxMotor::centred(180.0)));
        assert!(found.colored && !found.plain_wheel);

        let bare = model(
            "DmxMovingHead",
            &[
                ("DmxChannelCount", "4"),
                ("DmxColorType", "1"),
                ("DmxColorWheelChannel", "2"),
            ],
        );
        let found = fixture(&bare, Kind::Head, 1);
        assert!(found.plain_wheel && found.unmoving);

        let cmy = model(
            "DmxMovingHeadAdv",
            &[
                ("DmxChannelCount", "4"),
                ("DmxColorType", "2"),
                ("DmxCyanChannel", "2"),
                ("DmxMagentaChannel", "3"),
                ("DmxYellowChannel", "4"),
            ],
        );
        assert_eq!(
            roles(&fixture(&cmy, Kind::Head, 1).fixture),
            vec![Other, Cyan, Magenta, Yellow]
        );
    }

    #[test]
    fn servos_take_their_channels_and_limits_with_home_between_them() {
        let m = model(
            "DmxServo",
            &[("DmxChannelCount", "4"), ("NumServos", "2"), ("Bits16", "1")],
        );
        let m = with_part(
            m,
            "Servo2",
            &[
                ("Channel", "3"),
                ("MinLimit", "16384"),
                ("MaxLimit", "49151"),
                ("RangeOfMotion", "180"),
            ],
        );
        let found = fixture(&m, Kind::Servo, 1);
        let f = &found.fixture;
        use DmxRole::*;
        assert_eq!(roles(f), vec![Servo, ServoFine, Servo, ServoFine]);
        assert_eq!(found.cells, 2);
        assert_eq!(f.channels[0].name, "Servo 1");
        assert_eq!(f.channels[3].name, "Servo fine 2");
        assert_eq!(
            f.channels[0].motor,
            Some(DmxMotor::centred(180.0)),
            "the whole travel"
        );
        let second = f.channels[2].motor.unwrap();
        assert!((second.home_deg - 90.0).abs() < 0.01);
        assert!((second.min_deg.unwrap() + 45.0).abs() < 0.01);
        assert!((second.max_deg.unwrap() - 45.0).abs() < 0.01);
        // At its limits, the values the file gives.
        let axes = f.axes();
        assert_eq!(axes[1].values(-45.0), (64, Some(0)));
        assert_eq!(axes[1].values(90.0), (191, Some(255)));
        assert_eq!(f.problem(2), None);
        // At rest, Home: the middle of each servo's limits, not the 0 xLights would send.
        let held: Vec<u8> = f.channels.iter().map(|c| c.value).collect();
        assert_eq!(held, [128, 0, 128, 0], "the servos rest at Home");

        let eight = model(
            "DmxServo",
            &[("DmxChannelCount", "3"), ("NumServos", "3"), ("Bits16", "0")],
        );
        let found = fixture(&eight, Kind::Servo, 1);
        assert_eq!(roles(&found.fixture), vec![Servo, Servo, Servo]);
        assert_eq!(found.cells, 3);
    }

    /// DMX models as xLights 2026 writes them: from the public xLights render tests' show
    /// (github.com/derwin12/xlights-tests, `show/xlights_rgbeffects.xml`), as written there.
    const XLIGHTS_2026: &str = r##"<xrgb><models type="rgb_effects">
    <model name="MH1" DisplayAs="DmxMovingHeadAdv" StringType="Single Color White" StartChannel="6682" StartSide="B" Dir="L" Antialias="1" PixelSize="2" Transparency="0" LayoutGroup="Default" versionNumber="8" WorldPosX="-300" WorldPosY="500" WorldPosZ="0" DmxChannelCount="12" ScaleX="1.0" ScaleY="1.0" ScaleZ="1.0" DmxFixture="MH1" DmxColorType="0" DmxRedChannel="6" DmxGreenChannel="7" DmxBlueChannel="8" DmxWhiteChannel="9" MhDimmerChannel="5" DmxShutterChannel="10" DmxShutterOpen="1" DmxShutterOnValue="255">
      <PanMotor ChannelCoarse="1" ChannelFine="2" MinLimit="-180" MaxLimit="180" RangeOfMotion="540.000000" OrientZero="0" OrientHome="90" SlewLimit="0.000000" SpeedChannel="0" Reverse="0" UpsideDown="0" />
      <TiltMotor ChannelCoarse="3" ChannelFine="4" MinLimit="-90" MaxLimit="90" RangeOfMotion="180.000000" OrientZero="0" OrientHome="90" SlewLimit="0.000000" SpeedChannel="0" Reverse="0" UpsideDown="0" />
    </model>
    <model name="MH3" DisplayAs="DmxMovingHead" StringType="Single Color White" StartChannel="6706" StartSide="B" Dir="L" Antialias="1" PixelSize="2" Transparency="0" LayoutGroup="Default" versionNumber="8" WorldPosX="0" WorldPosY="500" WorldPosZ="0" DmxChannelCount="12" ScaleX="1.0" ScaleY="1.0" ScaleZ="1.0" DmxFixture="MH3" DmxStyle="Moving Head Top" DmxColorType="1" DmxColorWheelChannel="6" DmxDimmerChannel="5" MhDimmerChannel="5" DmxShutterChannel="10" DmxShutterOpen="1" DmxShutterOnValue="255" DmxColorWheelColor0="#ffffff" DmxColorWheelDMX0="0" DmxColorWheelColor1="#ff0000" DmxColorWheelDMX1="20" DmxColorWheelColor2="#00ff00" DmxColorWheelDMX2="40" DmxColorWheelColor3="#0000ff" DmxColorWheelDMX3="60" DmxColorWheelColor4="#ffff00" DmxColorWheelDMX4="80">
      <PanMotor ChannelCoarse="1" ChannelFine="2" MinLimit="-180" MaxLimit="180" RangeOfMotion="540.000000" OrientZero="0" OrientHome="90" SlewLimit="0.000000" SpeedChannel="0" Reverse="0" UpsideDown="0" />
      <TiltMotor ChannelCoarse="3" ChannelFine="4" MinLimit="-90" MaxLimit="90" RangeOfMotion="180.000000" OrientZero="0" OrientHome="90" SlewLimit="0.000000" SpeedChannel="0" Reverse="0" UpsideDown="0" />
    </model>
    <model name="DmxGen" DisplayAs="DmxGeneral" StringType="Single Color White" StartChannel="6734" StartSide="B" Dir="L" Antialias="1" PixelSize="2" Transparency="0" LayoutGroup="Default" versionNumber="8" WorldPosX="450" WorldPosY="500" WorldPosZ="0" DmxChannelCount="8" ScaleX="1.0" ScaleY="1.0" ScaleZ="1.0" DmxColorType="0" DmxRedChannel="2" DmxGreenChannel="3" DmxBlueChannel="4" DmxPresetChannel0="8" DmxPresetValue0="200" DmxPresetDesc0="Strobe" />
    <model name="Servos" DisplayAs="DmxServo" StringType="Single Color White" StartChannel="6742" StartSide="B" Dir="L" Antialias="1" PixelSize="2" Transparency="0" LayoutGroup="Default" versionNumber="8" WorldPosX="600" WorldPosY="500" WorldPosZ="0" DmxChannelCount="4" ScaleX="1.0" ScaleY="1.0" ScaleZ="1.0" NumServos="2" Bits16="1">
      <Servo1 Channel="1" MinLimit="1" MaxLimit="65535" RangeOfMotion="180.000000" ServoStyle="Rotate Z" />
      <Servo2 Channel="3" MinLimit="1" MaxLimit="65535" RangeOfMotion="180.000000" ServoStyle="Translate Y" />
    </model>
  </models></xrgb>"##;

    fn from_2026(name: &str) -> Imported {
        let layout = crate::layout::parse_layout(XLIGHTS_2026).unwrap();
        let m = layout.models.iter().find(|m| m.name == name).unwrap();
        fixture(m, kind(m).unwrap(), 1)
    }

    #[test]
    fn a_2026_head_has_its_dimmer_and_sends_its_shutter_its_on_value() {
        let found = from_2026("MH1");
        let f = &found.fixture;
        use DmxRole::*;
        assert_eq!(
            roles(f),
            vec![
                Pan, PanFine, Tilt, TiltFine, Dimmer, Red, Green, Blue, White, Strobe, Other, Other
            ],
            "MhDimmerChannel is the head's dimmer"
        );
        let shutter = &f.channels[9];
        assert_eq!(shutter.value, 255, "held at DmxShutterOnValue");
        let at = pf_model::Moment::new(0, 25);
        let shows = |v| pf_model::shutter_shows(shutter.shutter().unwrap(), v, at);
        assert!(shows(1) && shows(255) && !shows(0), "open from DmxShutterOpen up");
        assert_eq!(f.problem(1), None);

        // A colour wheel's brightness is DmxDimmerChannel; here it is the head's dimmer too.
        let found = from_2026("MH3");
        let f = &found.fixture;
        assert_eq!(roles(f)[..6], [Pan, PanFine, Tilt, TiltFine, Dimmer, ColorWheel]);
        assert_eq!(f.channels[5].slots.len(), 5);
        assert!(found.colored && !found.plain_wheel);
    }

    #[test]
    fn a_head_turns_the_way_xlights_turns_it() {
        // The public render tests' baseline for MH1 (Home 90° of 540° pan and of 180° tilt),
        // "Pan: 45.0;Tilt: 20.0": pan 21, 85 (45°, Home less 45) and tilt 99, 141 (70°).
        let found = from_2026("MH1");
        let axes = found.fixture.axes();
        // xLights drops a 16-bit value's fraction where PixelFlow rounds it: a fine step apart.
        let near = |(coarse, fine): (u8, Option<u8>), sent: (u32, u32)| {
            let ours = i64::from(coarse) * 256 + i64::from(fine.unwrap_or(0));
            let theirs = i64::from(sent.0 * 256 + sent.1);
            (ours - theirs).abs() <= 1
        };
        assert!(near(axes[0].values(45.0), (21, 85)));
        assert!(near(axes[1].values(20.0), (99, 141)));

        // Reversed, or upside down, it turns the other way; both, as it was. (xLights 2026,
        // "Pan: 45.0;Tilt: 20.0" on a pan of 540° and a tilt of 270° with Home in the middle.)
        let turned = |reverse: &str, upside_down: &str| {
            let motor = [
                ("ChannelCoarse", "1"),
                ("ChannelFine", "2"),
                ("RangeOfMotion", "540"),
                ("OrientHome", "270"),
                ("Reverse", reverse),
                ("UpsideDown", upside_down),
            ];
            let m = with_part(
                model("DmxMovingHeadAdv", &[("DmxChannelCount", "2")]),
                "PanMotor",
                &motor,
            );
            fixture(&m, Kind::Head, 1).fixture.axes()[0].values(45.0)
        };
        assert_eq!(turned("0", "0"), (106, Some(170)), "Home less 45°");
        assert_eq!(turned("1", "0"), (149, Some(85)), "Home and 45°");
        assert_eq!(turned("0", "1"), (149, Some(85)), "Home and 45°");
        assert_eq!(turned("1", "1"), (106, Some(170)), "Home less 45°");
    }

    #[test]
    fn a_heads_older_dimmer_is_its_dimmer_only_without_a_2026_one() {
        let head = |attrs: &[(&str, &str)]| {
            let mut all = vec![
                ("DmxChannelCount", "8"),
                ("DmxColorType", "0"),
                ("DmxRedChannel", "6"),
                ("DmxGreenChannel", "7"),
                ("DmxBlueChannel", "8"),
            ];
            all.extend_from_slice(attrs);
            roles(&fixture(&model("DmxMovingHeadAdv", &all), Kind::Head, 1).fixture)
        };
        use DmxRole::*;
        assert_eq!(head(&[("DmxDimmerChannel", "5")])[4], Dimmer, "an older file's");
        // xLights 2026 leaves DmxDimmerChannel at 0 on a head of red, green and blue that has
        // a MhDimmerChannel.
        let both = head(&[("DmxDimmerChannel", "4"), ("MhDimmerChannel", "5")]);
        assert_eq!(both[3..5], [Other, Dimmer]);
    }

    #[test]
    fn presets_hold_their_channels_and_name_them() {
        let found = from_2026("DmxGen");
        let f = &found.fixture;
        use DmxRole::*;
        assert_eq!(
            roles(f),
            vec![Other, Red, Green, Blue, Other, Other, Other, Other]
        );
        assert_eq!(f.channels[7].value, 200);
        assert_eq!(f.channels[7].name, "Strobe");
        assert!(f.channels[..7].iter().all(|c| c.value == 0));

        // On a channel with a part of its own, the preset's value and the part's name.
        let m = model(
            "DmxMovingHeadAdv",
            &[
                ("DmxChannelCount", "3"),
                ("DmxShutterChannel", "2"),
                ("DmxShutterOpen", "1"),
                ("DmxPresetChannel0", "2"),
                ("DmxPresetValue0", "240"),
                ("DmxPresetDesc0", "Open"),
                ("DmxPresetChannel1", "9"),
                ("DmxPresetValue1", "50"),
            ],
        );
        let f = fixture(&m, Kind::Head, 1).fixture;
        assert_eq!(f.channels[1].role, Strobe);
        assert_eq!(f.channels[1].value, 240);
        assert_ne!(f.channels[1].name, "Open");
        assert_eq!(
            f.channels.len(),
            3,
            "a preset past the last channel is no channel"
        );
    }

    #[test]
    fn a_servo_turns_the_other_way_when_its_lower_limit_is_the_larger() {
        // As xLights 2026 writes servos: whole travel, not reversed.
        let found = from_2026("Servos");
        let axes = found.fixture.axes();
        assert!(!found.fixture.channels[0].motor.unwrap().reverse);
        assert_eq!(axes[0].values(-90.0), (0, Some(1)), "MinLimit 1 at the start");

        // The second servo of the corpus's Arm: MinLimit 49151, MaxLimit 16384. xLights sends
        // 49151 at the start of its travel and 16384 at the end, whatever `Reverse` says.
        for reverse in ["0", "1"] {
            let m = with_part(
                model(
                    "DmxServo",
                    &[("DmxChannelCount", "2"), ("NumServos", "1"), ("Bits16", "1")],
                ),
                "Servo1",
                &[
                    ("Channel", "1"),
                    ("MinLimit", "49151"),
                    ("MaxLimit", "16384"),
                    ("RangeOfMotion", "180"),
                    ("Reverse", reverse),
                ],
            );
            let f = fixture(&m, Kind::Servo, 1).fixture;
            let motor = f.channels[0].motor.unwrap();
            assert!(motor.reverse);
            let axes = f.axes();
            let (lo, hi) = motor.limits();
            assert_eq!(axes[0].values(lo), (191, Some(255)), "49151 at the start");
            assert_eq!(axes[0].values(hi), (64, Some(0)), "16384 at the end");
        }
    }
}
