//! An effect as the Assistant (and a guide's look) describes one: a kind, its settings, colors,
//! and anything else an effect can have, by name, with the rest left at its default. Everything
//! is checked against the effect catalog before anything is drafted, and what doesn't fit is
//! answered with what would: the setting's range, the choices, the kind's own settings.
//!
//! ```json
//! { "kind": "bars", "settings": { "count": 6, "speed": 0.4 }, "colors": ["#ff0000", "#ffffff"],
//!   "blend": "max", "fadeInMs": 300, "sparkles": 40, "blur": 1, "brightness": 0.8,
//!   "renderStyle": "perModelDefault", "bufferTransform": "flipHorizontal",
//!   "curves": { "speed": { "shape": "ramp", "from": 0.2, "to": 2 } } }
//! ```
//!
//! A timing track (a `timingTrack` setting, or a curve's) may be given by name.

use crate::knowledge::reference::{self, key_of};
use pf_model::RenderStyle;
use pf_sequence::{
    Blend, CurveShape, Effect, EffectKind, MAX_PALETTE_COLORS, Rgb, SettingValue, TimingTrack,
};
use serde_json::{Map, Value, json};

/// What an effect description may hold besides `kind` and `settings`.
pub const EFFECT_WIDE: [&str; 16] = [
    "colors",
    "blend",
    "fadeInMs",
    "fadeOutMs",
    "sparkles",
    "sparkleColor",
    "musicSparkles",
    "blur",
    "brightness",
    "contrast",
    "hueAdjust",
    "saturationAdjust",
    "valueAdjust",
    "renderStyle",
    "bufferTransform",
    "curves",
];

/// What a description is checked against besides the catalog.
#[derive(Debug, Clone, Copy, Default)]
pub struct Context<'a> {
    /// The sequence's timing tracks, for the ones named.
    pub tracks: &'a [TimingTrack],
    /// The show's pictures, as a Picture effect's `file` names them (`None`: not checked).
    pub pictures: Option<&'a [String]>,
}

fn list(names: impl IntoIterator<Item = impl AsRef<str>>) -> String {
    names
        .into_iter()
        .map(|n| n.as_ref().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A timing track given by id or by name, as its id.
fn track_id(value: &Value, what: &str, cx: &Context<'_>) -> Result<Value, String> {
    let Some(key) = value.as_str() else {
        return Err(format!("{what} is a timing track's name or id."));
    };
    match crate::song::find_track(cx.tracks, key) {
        Some(track) => Ok(json!(track.id)),
        None if cx.tracks.is_empty() => Err(format!(
            "{what} is \"{key}\", but the sequence has no timing tracks: add_song_timing makes the song's."
        )),
        None => Err(format!(
            "{what} is \"{key}\", and there's no such timing track. The sequence has: {}.",
            list(cx.tracks.iter().map(|t| &t.name))
        )),
    }
}

/// One setting's value, checked against its range or choices (a named timing track as its id).
fn setting(kind: EffectKind, key: &str, value: &Value, cx: &Context<'_>) -> Result<Value, String> {
    let info = reference::info(kind);
    let Some(spec) = info.settings.iter().find(|s| s.key == key) else {
        let keys: Vec<&str> = info.settings.iter().map(|s| s.key).collect();
        return Err(if keys.is_empty() {
            format!("The {} effect has no settings, so no \"{key}\".", info.label)
        } else {
            format!(
                "The {} effect has no setting \"{key}\". Its settings: {}.",
                info.label,
                keys.join(", ")
            )
        });
    };
    // Ranges are said as the catalog holds them (0.1, not the 0.10000000149 an f32 widens to).
    let out_of =
        |min: &dyn std::fmt::Display, max: &dyn std::fmt::Display, default: &dyn std::fmt::Display| {
            format!("{key} is {value}; use {min} to {max} (default {default}).")
        };
    match &spec.value {
        SettingValue::Number {
            min, max, default, ..
        } => {
            let n = value
                .as_f64()
                .ok_or_else(|| format!("{key} is a number, {min} to {max}."))?;
            // Compared as the setting holds it, so 0.1 is inside a range that starts at 0.1.
            if !(*min..=*max).contains(&(n as f32)) {
                return Err(out_of(min, max, default));
            }
        }
        SettingValue::Int {
            min, max, default, ..
        } => {
            let n = value
                .as_u64()
                .or_else(|| {
                    value
                        .as_f64()
                        .filter(|f| f.fract() == 0.0 && *f >= 0.0)
                        .map(|f| f as u64)
                })
                .ok_or_else(|| format!("{key} is a whole number, {min} to {max}."))?;
            if !(u64::from(*min)..=u64::from(*max)).contains(&n) {
                return Err(out_of(min, max, default));
            }
            return Ok(json!(n));
        }
        SettingValue::Bool { .. } => {
            if !value.is_boolean() {
                return Err(format!("{key} is true or false."));
            }
        }
        SettingValue::Choice { options, .. } => {
            if !options.iter().any(|o| value.as_str() == Some(o.value)) {
                return Err(format!(
                    "{key} is {value}; choose from {}.",
                    list(options.iter().map(|o| o.value))
                ));
            }
        }
        SettingValue::Face { .. } | SettingValue::Text { .. } => {
            if !value.is_string() {
                return Err(format!("{key} is text."));
            }
        }
        SettingValue::Image { .. } => {
            let file = value
                .as_str()
                .ok_or_else(|| format!("{key} is a picture's name."))?;
            if let Some(pictures) = cx.pictures
                && !file.is_empty()
                && !pictures.iter().any(|p| p == file)
            {
                return Err(if pictures.is_empty() {
                    format!(
                        "{key} is \"{file}\", but this show has no pictures yet: the user adds them to the show's images folder."
                    )
                } else {
                    format!(
                        "{key} is \"{file}\", which isn't one of the show's pictures: {}.",
                        list(pictures.iter().take(30))
                    )
                });
            }
        }
        SettingValue::TimingTrack { .. } => {
            if !value.is_null() {
                return track_id(value, key, cx);
            }
        }
    }
    Ok(value.clone())
}

/// One curve, checked: a setting the kind's curves can drive, a real shape, and its track by id.
fn curve(kind: EffectKind, key: &str, value: &Value, cx: &Context<'_>) -> Result<Value, String> {
    if Effect::curve_range(kind, key).is_none() {
        let mut keys = reference::curve_keys(kind);
        keys.extend(reference::effect_wide_curve_keys());
        return Err(format!(
            "A curve can't drive \"{key}\" on a {} effect. Curves can drive: {}.",
            reference::info(kind).label,
            keys.join(", ")
        ));
    }
    let Some(object) = value.as_object() else {
        return Err(format!(
            "The curve on {key} is an object: {{\"shape\", \"from\", \"to\"}}."
        ));
    };
    let shapes: Vec<String> = CurveShape::ALL.iter().map(key_of).collect();
    let shape = object.get("shape").and_then(Value::as_str).unwrap_or_default();
    let Some(shape) = CurveShape::ALL.into_iter().find(|s| key_of(s) == shape) else {
        return Err(format!(
            "The curve on {key} needs a shape: one of {}.",
            shapes.join(", ")
        ));
    };
    for end in ["from", "to"] {
        if !object.get(end).is_some_and(Value::is_number) {
            return Err(format!(
                "The curve on {key} needs from and to: the setting's value at its two ends."
            ));
        }
    }
    let mut out = object.clone();
    match object.get("timingTrack").filter(|t| !t.is_null()) {
        Some(track) => {
            out.insert(
                "timingTrack".into(),
                track_id(track, &format!("The curve on {key}'s timingTrack"), cx)?,
            );
        }
        None if shape.follows_marks() => {
            return Err(format!(
                "A {} curve follows a timing track: give the curve on {key} a timingTrack{}.",
                key_of(&shape),
                if cx.tracks.is_empty() {
                    " (add_song_timing makes the song's)".to_string()
                } else {
                    format!(" (one of {})", list(cx.tracks.iter().map(|t| &t.name)))
                }
            ));
        }
        None => {}
    }
    Ok(Value::Object(out))
}

/// One effect-wide setting, checked where a wrong value needs more than its range said back.
fn effect_wide(key: &str, value: &Value) -> Result<Value, String> {
    let one_of = |names: Vec<String>| {
        if names.iter().any(|n| value.as_str() == Some(n)) {
            Ok(value.clone())
        } else {
            Err(format!("{key} is {value}; choose from {}.", names.join(", ")))
        }
    };
    match key {
        "blend" => one_of(Blend::ALL.iter().map(|b| b.key().to_string()).collect()),
        "renderStyle" => one_of(RenderStyle::ALL.iter().map(key_of).collect()),
        "bufferTransform" => one_of(reference::buffer_transform_keys()),
        "sparkleColor" => {
            if serde_json::from_value::<Rgb>(value.clone()).is_err() {
                return Err(format!("{key} is {value}, which isn't a color; use \"#rrggbb\"."));
            }
            Ok(value.clone())
        }
        "musicSparkles" => value
            .is_boolean()
            .then(|| value.clone())
            .ok_or_else(|| format!("{key} is true or false.")),
        _ => value
            .is_number()
            .then(|| value.clone())
            .ok_or_else(|| format!("{key} is a number.")),
    }
}

fn colors(value: &Value) -> Result<Value, String> {
    let Some(items) = value.as_array() else {
        return Err("colors is a list of colors, each \"#rrggbb\".".into());
    };
    if items.len() > MAX_PALETTE_COLORS {
        return Err(format!("An effect takes at most {MAX_PALETTE_COLORS} colors."));
    }
    for item in items {
        if !item.is_string() || serde_json::from_value::<Rgb>(item.clone()).is_err() {
            return Err(format!("{item} isn't a color; use \"#rrggbb\"."));
        }
    }
    Ok(json!({ "colors": items }))
}

/// Writes what `spec` says onto `effect` (an effect as JSON). `fresh`: the description is of a
/// new effect, so it names its kind; otherwise it changes one that exists, and can't.
fn write(
    effect: &mut Map<String, Value>,
    kind: EffectKind,
    spec: &Value,
    cx: &Context<'_>,
    fresh: bool,
) -> Result<(), String> {
    let Some(given) = spec.as_object() else {
        return Err("An effect is an object: {\"kind\", \"settings\", \"colors\", ...}.".into());
    };
    for (key, value) in given {
        match key.as_str() {
            "kind" if fresh => {}
            "settings" => {
                let Some(settings) = value.as_object() else {
                    if value.is_null() {
                        continue;
                    }
                    return Err("settings is an object of the kind's settings by key.".into());
                };
                let params = effect
                    .entry("params")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .ok_or("the effect has no settings to change")?;
                for (name, v) in settings {
                    params.insert(name.clone(), setting(kind, name, v, cx)?);
                }
            }
            "colors" => {
                effect.insert("palette".into(), colors(value)?);
            }
            "curves" => {
                let Some(curves) = value.as_object() else {
                    return Err("curves is an object of curves by setting key.".into());
                };
                let kept = effect
                    .entry("curves")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .ok_or("the effect's curves can't be changed")?;
                for (name, v) in curves {
                    if v.is_null() {
                        kept.remove(name);
                    } else {
                        kept.insert(name.clone(), curve(kind, name, v, cx)?);
                    }
                }
                // An effect with no curves has no `curves` at all.
                if kept.is_empty() {
                    effect.remove("curves");
                }
            }
            name if EFFECT_WIDE.contains(&name) => {
                effect.insert(key.clone(), effect_wide(name, value)?);
            }
            other => {
                let mut takes = vec!["settings"];
                if fresh {
                    takes.insert(0, "kind");
                }
                takes.extend(EFFECT_WIDE);
                return Err(format!(
                    "Nothing was drafted: an effect has no \"{other}\". It takes {}.",
                    takes.join(", ")
                ));
            }
        }
    }
    Ok(())
}

/// The effect `json` says, or what about it is out of range.
fn read(json: Map<String, Value>) -> Result<Effect, String> {
    let given = Value::Object(json);
    let effect: Effect =
        serde_json::from_value(given.clone()).map_err(|e| format!("That effect doesn't fit: {e}."))?;
    // A setting left at its neutral value isn't written back (a brightness of 1, a curve's one
    // cycle): that isn't a name PixelFlow doesn't know.
    const NEUTRAL: [&str; 6] = ["cycles", "points", "gain", "trigger", "fade", "timingTrack"];
    let ignored: Vec<String> =
        crate::tools::ignored_keys(&given, &serde_json::to_value(&effect).unwrap_or(Value::Null))
            .into_iter()
            .filter(|path| {
                let name = path.rsplit('.').next().unwrap_or(path);
                !EFFECT_WIDE.contains(&name) && !(path.starts_with("curves.") && NEUTRAL.contains(&name))
            })
            .collect();
    if !ignored.is_empty() {
        return Err(crate::tools::ignored_message(&ignored));
    }
    if let Some(problem) = effect.setting_problem() {
        return Err(format!("{problem}."));
    }
    Ok(effect)
}

/// The effect kind `spec` names, or the kinds there are.
pub fn kind_of(spec: &Value) -> Result<EffectKind, String> {
    let kind = spec["kind"].as_str().unwrap_or_default();
    reference::kind_named(kind).ok_or_else(|| {
        format!(
            "There's no effect kind \"{kind}\"; choose from {}.",
            reference::kind_keys().join(", ")
        )
    })
}

/// The effect `spec` describes (its id new, its time a placeholder for the caller to set).
pub fn parse(spec: &Value, cx: &Context<'_>) -> Result<Effect, String> {
    if !spec.is_object() {
        return Err("An effect is an object: {\"kind\", \"settings\", \"colors\", ...}.".into());
    }
    let kind = kind_of(spec)?;
    let Value::Object(mut effect) =
        serde_json::to_value(Effect::new(kind, 0, 1)).map_err(|e| e.to_string())?
    else {
        return Err("That effect couldn't be made.".into());
    };
    write(&mut effect, kind, spec, cx, true)?;
    read(effect)
}

/// `effect` with what `changes` says changed: settings and curves by key (a curve of `null` is
/// taken off), and anything else whole. Its kind, id, and time stay.
pub fn changed(effect: &Effect, changes: &Value, cx: &Context<'_>) -> Result<Effect, String> {
    let Value::Object(mut json) = serde_json::to_value(effect).map_err(|e| e.to_string())? else {
        return Err("That effect couldn't be changed.".into());
    };
    write(&mut json, effect.kind(), changes, cx, false)?;
    read(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_sequence::{EffectParams, Mark, TimingKind};

    fn tracks() -> Vec<TimingTrack> {
        vec![
            TimingTrack::new("Beats", TimingKind::Beats, vec![Mark::new(0, 500, "1")]),
            TimingTrack::new("Bars", TimingKind::Bars, Vec::new()),
        ]
    }

    #[test]
    fn a_description_becomes_an_effect_with_everything_it_names() {
        let tracks = tracks();
        let cx = Context {
            tracks: &tracks,
            pictures: None,
        };
        let spec = json!({
            "kind": "chase",
            "settings": { "speed": 2, "bands": 3, "timingTrack": "beats" },
            "colors": ["#ff0000", "#ffffff"],
            "blend": "max", "fadeInMs": 200, "fadeOutMs": 300,
            "sparkles": 40, "sparkleColor": "#ffd700", "musicSparkles": true, "blur": 2,
            "brightness": 0.8, "contrast": 0.2, "hueAdjust": 0.1, "saturationAdjust": -0.2, "valueAdjust": 0.1,
            "renderStyle": "perModelDefault", "bufferTransform": "flipHorizontal",
            "curves": {
                "speed": { "shape": "ramp", "from": 0.5, "to": 4 },
                "brightness": { "shape": "timingFade", "from": 0.3, "to": 1, "fade": 8, "timingTrack": "Bars" },
            },
        });
        let effect = parse(&spec, &cx).unwrap();
        let EffectParams::Chase(chase) = &effect.params else {
            panic!("{:?}", effect.params)
        };
        assert_eq!((chase.speed, chase.bands), (2.0, 3));
        assert_eq!(
            chase.timing_track,
            Some(tracks[0].id),
            "a track by name, any case"
        );
        assert_eq!(effect.palette.colors.len(), 2);
        assert_eq!(effect.blend, Blend::Max);
        assert_eq!((effect.fade_in_ms, effect.fade_out_ms), (200, 300));
        assert_eq!(
            (effect.sparkles, effect.music_sparkles, effect.blur),
            (40, true, 2)
        );
        assert_eq!(effect.sparkle_color, Rgb::new(255, 215, 0));
        assert_eq!((effect.brightness, effect.contrast), (0.8, 0.2));
        assert_eq!(
            (effect.hue_adjust, effect.saturation_adjust, effect.value_adjust),
            (0.1, -0.2, 0.1)
        );
        assert_eq!(effect.render_style, RenderStyle::PerModelDefault);
        assert_eq!(effect.buffer_transform, pf_model::BufferTransform::FlipHorizontal);
        assert_eq!(effect.curves["speed"].to, 4.0);
        assert_eq!(effect.curves["brightness"].timing_track, Some(tracks[1].id));
        // A kind alone is a whole effect.
        assert_eq!(
            parse(&json!({ "kind": "twinkle" }), &cx).unwrap().params,
            EffectParams::default_for(EffectKind::Twinkle)
        );
    }

    #[test]
    fn what_doesnt_fit_is_answered_with_what_would() {
        let tracks = tracks();
        let cx = Context {
            tracks: &tracks,
            pictures: Some(&[]),
        };
        let refused = |spec: Value| parse(&spec, &cx).unwrap_err();
        let kind = refused(json!({ "kind": "sparkle" }));
        assert!(
            kind.contains("no effect kind \"sparkle\"") && kind.contains("twinkle"),
            "{kind}"
        );
        let key = refused(json!({ "kind": "chase", "settings": { "sped": 2 } }));
        assert!(
            key.contains("no setting \"sped\"") && key.contains("speed, width, bands"),
            "{key}"
        );
        let range = refused(json!({ "kind": "chase", "settings": { "speed": 70 } }));
        assert_eq!(range, "speed is 70; use 0 to 50 (default 1).");
        // A range's own ends are inside it, and are said as they're written.
        let low = json!({ "kind": "picture", "settings": { "file": "", "playSpeed": 0.1 } });
        assert!(parse(&low, &cx).is_ok());
        let under = refused(json!({ "kind": "picture", "settings": { "playSpeed": 0.05 } }));
        assert_eq!(under, "playSpeed is 0.05; use 0.1 to 10 (default 1).");
        let whole = refused(json!({ "kind": "chase", "settings": { "bands": 2.5 } }));
        assert_eq!(whole, "bands is a whole number, 1 to 1000.");
        let choice = refused(json!({ "kind": "chase", "settings": { "direction": "left" } }));
        assert_eq!(choice, "direction is \"left\"; choose from forward, reverse.");
        let flag = refused(json!({ "kind": "chase", "settings": { "bounce": "yes" } }));
        assert_eq!(flag, "bounce is true or false.");
        let track = refused(json!({ "kind": "chase", "settings": { "timingTrack": "Claps" } }));
        assert!(
            track.contains("no such timing track") && track.contains("Beats, Bars"),
            "{track}"
        );
        let picture = refused(json!({ "kind": "picture", "settings": { "file": "images/santa.gif" } }));
        assert!(picture.contains("no pictures yet"), "{picture}");
        let blend = refused(json!({ "kind": "on", "blend": "screen" }));
        assert!(blend.contains("choose from normal, add"), "{blend}");
        let style = refused(json!({ "kind": "on", "renderStyle": "perModel" }));
        assert!(style.contains("perModelDefault"), "{style}");
        let sparkles = refused(json!({ "kind": "on", "sparkles": 500 }));
        assert_eq!(sparkles, "Sparkles is 500; use 0 to 200.");
        let color = refused(json!({ "kind": "on", "colors": ["red"] }));
        assert!(color.contains("#rrggbb"), "{color}");
        let unknown = refused(json!({ "kind": "on", "colour": "#ff0000" }));
        assert!(
            unknown.contains("no \"colour\"") && unknown.contains("colors"),
            "{unknown}"
        );
        let curve_key = refused(
            json!({ "kind": "chase", "curves": { "direction": { "shape": "ramp", "from": 0, "to": 1 } } }),
        );
        assert!(
            curve_key.contains("Curves can drive: speed, width, bands, sparkles"),
            "{curve_key}"
        );
        let shape = refused(
            json!({ "kind": "chase", "curves": { "speed": { "shape": "wobble", "from": 0, "to": 1 } } }),
        );
        assert!(shape.contains("one of ramp, sine"), "{shape}");
        let ends = refused(json!({ "kind": "chase", "curves": { "speed": { "shape": "ramp", "to": 1 } } }));
        assert!(ends.contains("needs from and to"), "{ends}");
        let far = refused(
            json!({ "kind": "chase", "curves": { "speed": { "shape": "ramp", "from": 0, "to": 90 } } }),
        );
        assert_eq!(far, "Speed's curve ends at 90; use 0 to 50.");
        let trackless = refused(
            json!({ "kind": "chase", "curves": { "speed": { "shape": "timingFade", "from": 0, "to": 9 } } }),
        );
        assert!(
            trackless.contains("give the curve on speed a timingTrack (one of Beats, Bars)"),
            "{trackless}"
        );
    }

    #[test]
    fn changes_keep_what_they_dont_name_and_take_curves_off() {
        let tracks = tracks();
        let cx = Context {
            tracks: &tracks,
            pictures: None,
        };
        let effect = parse(
            &json!({
                "kind": "bars", "settings": { "count": 6, "speed": 2 }, "colors": ["#ff0000"], "sparkles": 20,
                "curves": { "speed": { "shape": "ramp", "from": 0, "to": 2 } },
            }),
            &cx,
        )
        .unwrap();
        let after = changed(
            &effect,
            &json!({ "settings": { "speed": 1 }, "colors": ["#00ff00", "#ffffff"], "curves": { "speed": null }, "blur": 3 }),
            &cx,
        )
        .unwrap();
        assert_eq!(after.id, effect.id);
        let EffectParams::Bars(bars) = &after.params else {
            panic!()
        };
        assert_eq!((bars.count, bars.speed), (6, 1.0), "count stays, speed changes");
        assert_eq!(after.palette.colors.len(), 2);
        assert_eq!((after.sparkles, after.blur), (20, 3));
        assert!(after.curves.is_empty());
        // A change can't turn an effect into another kind, or use a setting its kind lacks.
        let kind = changed(&effect, &json!({ "kind": "chase" }), &cx).unwrap_err();
        assert!(kind.contains("no \"kind\""), "{kind}");
        let key = changed(&effect, &json!({ "settings": { "bands": 3 } }), &cx).unwrap_err();
        assert!(key.contains("The Bars effect has no setting \"bands\""), "{key}");
    }
}
