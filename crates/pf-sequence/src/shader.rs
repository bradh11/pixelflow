//! What a Shader effect sets its shader file's inputs to.
//!
//! Every other effect's settings are a fixed table (see [`crate::settings`]). A shader's inputs
//! come from its file: which there are, what each holds, and its range. So the effect keeps
//! them by name ([`ShaderInputs`]), and only the ones that were set: an input left out is as the
//! file has it, which is how a shader looks as its author meant it until something is changed.
//!
//! This crate never reads a shader file, so it can't say whether a name is one of the file's
//! inputs or a number is inside its range. It keeps what could be an input (a plain name, a
//! number that is one) and no more than a shader may declare. Whoever has the file holds the
//! rest against it: the renderer draws with each value brought inside the file's range and
//! ignores a name the file doesn't have, and edits are checked where the file is known.
//!
//! A curve changes one number of them over the effect: its key is `inputs.` and the input's
//! name, with `.x` or `.y` after a point's (see [`ShaderInputs::set_part`]).

use crate::settings::{SettingField, SettingRange};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The most inputs a Shader effect sets (as many as a shader file may declare).
pub const MAX_SHADER_INPUTS: usize = 64;
/// The longest an input's name is.
pub const MAX_SHADER_INPUT_NAME: usize = 100;
/// The furthest from zero an input's number may be (as a shader file's own ranges are kept).
pub const MAX_SHADER_NUMBER: f32 = 1.0e9;

/// What one input of a shader is set to.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum ShaderValue {
    /// A switch.
    Bool(bool),
    /// A number; for a choice, the number the shader is given for the one chosen.
    Number(f32),
    /// A point: across, then up.
    Point([f32; 2]),
    /// A color: red, green, blue, and alpha, each 0 to 1.
    Color([f32; 4]),
}

impl ShaderValue {
    /// The numbers a shader is given for it (a switch as 1 or 0).
    pub fn numbers(&self) -> ([f32; 4], usize) {
        match *self {
            ShaderValue::Bool(on) => ([f32::from(u8::from(on)), 0.0, 0.0, 0.0], 1),
            ShaderValue::Number(n) => ([n, 0.0, 0.0, 0.0], 1),
            ShaderValue::Point([x, y]) => ([x, y, 0.0, 0.0], 2),
            ShaderValue::Color(c) => (c, 4),
        }
    }
}

/// The inputs a Shader effect sets, by the name its shader file gives each.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct ShaderInputs(pub BTreeMap<String, ShaderValue>);

/// Whether `name` could be an input's: a name as GLSL writes one, of a sensible length.
pub fn is_shader_input_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_SHADER_INPUT_NAME
        && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        && !name.as_bytes()[0].is_ascii_digit()
}

/// An input's name and which of its numbers a curve's `part` means: `rate` is the number
/// `rate`; `at.x` and `at.y` are the two of the point `at`.
fn part_of(part: &str) -> Option<(&str, Option<usize>)> {
    let (name, which) = match part.rsplit_once('.') {
        Some((name, "x")) => (name, Some(0)),
        Some((name, "y")) => (name, Some(1)),
        Some(_) => return None,
        None => (part, None),
    };
    is_shader_input_name(name).then_some((name, which))
}

impl ShaderInputs {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&ShaderValue> {
        self.0.get(name)
    }

    /// Sets input `name`.
    pub fn set(&mut self, name: impl Into<String>, value: ShaderValue) {
        self.0.insert(name.into(), value);
    }

    /// Whether `part` (what follows `inputs.` in a curve's key) names a number an input could
    /// hold.
    pub fn is_part(part: &str) -> bool {
        part_of(part).is_some()
    }
}

/// One number that isn't one, or is past [`MAX_SHADER_NUMBER`], as it's kept: nothing for what
/// isn't a number (the shader then takes its file's own value there).
fn kept(n: f32) -> f32 {
    if n.is_nan() {
        n
    } else {
        n.clamp(-MAX_SHADER_NUMBER, MAX_SHADER_NUMBER)
    }
}

impl SettingField for ShaderInputs {
    /// Drops what couldn't be an input (a name that isn't one, a number that isn't one) and any
    /// past the most a shader declares, and brings absurd numbers inside what a file's ranges
    /// can be. One coordinate of a point may be no number: the file's own, which is what a curve
    /// on the other one leaves it at.
    fn sanitize(&mut self, _range: &SettingRange, _default: Self) {
        self.0.retain(|name, value| {
            if !is_shader_input_name(name) {
                return false;
            }
            match value {
                ShaderValue::Bool(_) => true,
                ShaderValue::Number(n) => {
                    *n = kept(*n);
                    !n.is_nan()
                }
                ShaderValue::Point(at) => {
                    *at = at.map(kept);
                    at.iter().any(|v| !v.is_nan())
                }
                ShaderValue::Color(color) => {
                    *color = color.map(|v| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) });
                    true
                }
            }
        });
        while self.0.len() > MAX_SHADER_INPUTS {
            self.0.pop_last();
        }
    }

    fn problem(&self, _range: &SettingRange) -> Option<String> {
        if self.0.len() > MAX_SHADER_INPUTS {
            return Some(format!(
                "sets {} inputs; a shader has at most {MAX_SHADER_INPUTS}",
                self.0.len()
            ));
        }
        self.0.iter().find_map(|(name, value)| {
            if !is_shader_input_name(name) {
                let shown: String = name.chars().take(40).collect();
                return Some(format!(
                    "names an input \"{shown}\", which isn't a name a shader can give one"
                ));
            }
            let (numbers, count) = value.numbers();
            let numbers = &numbers[..count];
            if numbers.iter().any(|n| !n.is_finite()) {
                return Some(format!("sets \"{name}\" to something that isn't a usable number"));
            }
            let most = match value {
                ShaderValue::Color(_) => 1.0,
                _ => MAX_SHADER_NUMBER,
            };
            let least = if most == 1.0 { 0.0 } else { -most };
            numbers
                .iter()
                .find(|n| !(least..=most).contains(*n))
                .map(|n| format!("sets \"{name}\" to {n}; use {least} to {most}"))
        })
    }

    /// Sets the number `part` names (see [`part_of`]). A number input becomes `value`; one
    /// coordinate of a point becomes `value` and the other stays (a point not set yet has no
    /// number there, so the shader takes its file's own). An input set to a switch or a color
    /// isn't a number, and stays as it is.
    fn set_part(&mut self, part: &str, value: f32) -> bool {
        let Some((name, which)) = part_of(part) else {
            return false;
        };
        match (which, self.0.get_mut(name)) {
            (None, Some(ShaderValue::Number(n))) => *n = value,
            (Some(k), Some(ShaderValue::Point(at))) => at[k] = value,
            (_, Some(_)) => return false,
            (None, None) => {
                if self.0.len() >= MAX_SHADER_INPUTS {
                    return false;
                }
                self.0.insert(name.to_string(), ShaderValue::Number(value));
            }
            (Some(k), None) => {
                if self.0.len() >= MAX_SHADER_INPUTS {
                    return false;
                }
                let mut at = [f32::NAN; 2];
                at[k] = value;
                self.0.insert(name.to_string(), ShaderValue::Point(at));
            }
        }
        true
    }

    fn part(&self, part: &str) -> Option<f32> {
        let (name, which) = part_of(part)?;
        match (which, self.0.get(name)?) {
            (None, ShaderValue::Number(n)) => Some(*n),
            (Some(k), ShaderValue::Point(at)) => Some(at[k]),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(json: &str) -> ShaderInputs {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn each_kind_of_value_reads_and_writes_as_plain_json() {
        let json = r#"{"at":[0.5,0.25],"flip":true,"mode":2.0,"rate":1.5,"tint":[1.0,0.5,0.0,1.0]}"#;
        let set = inputs(json);
        assert_eq!(set.get("flip"), Some(&ShaderValue::Bool(true)));
        assert_eq!(set.get("rate"), Some(&ShaderValue::Number(1.5)));
        assert_eq!(set.get("mode"), Some(&ShaderValue::Number(2.0)));
        assert_eq!(set.get("at"), Some(&ShaderValue::Point([0.5, 0.25])));
        assert_eq!(set.get("tint"), Some(&ShaderValue::Color([1.0, 0.5, 0.0, 1.0])));
        assert_eq!(serde_json::to_string(&set).unwrap(), json);
        // What is neither is refused when the file is read.
        for bad in [
            r#"{"a":"text"}"#,
            r#"{"a":[1,2,3]}"#,
            r#"{"a":null}"#,
            r#"{"a":{}}"#,
        ] {
            assert!(serde_json::from_str::<ShaderInputs>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn what_couldnt_be_an_input_is_dropped_and_said() {
        let range = SettingRange::ShaderInputs;
        let mut set = inputs(r#"{"rate":1e30,"2fast":1,"a b":1,"tint":[2,-1,0.5,1],"ok":true}"#);
        set.set("gone", ShaderValue::Number(f32::NAN));
        let problem = set.problem(&range).unwrap();
        assert!(
            problem.contains("\"2fast\"") && problem.contains("isn't a name"),
            "{problem}"
        );
        set.sanitize(&range, ShaderInputs::default());
        assert_eq!(
            serde_json::to_string(&set).unwrap(),
            r#"{"ok":true,"rate":1000000000.0,"tint":[1.0,0.0,0.5,1.0]}"#
        );
        assert_eq!(set.problem(&range), None);
        // Each kind of problem in plain words.
        let said = |json: &str| inputs(json).problem(&range).unwrap();
        assert_eq!(
            said(r#"{"rate":2e9}"#),
            "sets \"rate\" to 2000000000; use -1000000000 to 1000000000"
        );
        assert_eq!(
            said(r#"{"tint":[0,0,1.5,1]}"#),
            "sets \"tint\" to 1.5; use 0 to 1"
        );
        let mut many = ShaderInputs::default();
        for k in 0..70 {
            many.set(format!("in{k:02}"), ShaderValue::Number(k as f32));
        }
        assert!(many.problem(&range).unwrap().starts_with("sets 70 inputs"));
        many.sanitize(&range, ShaderInputs::default());
        assert_eq!(many.0.len(), MAX_SHADER_INPUTS);
        assert!(many.get("in00").is_some() && many.get("in69").is_none());
    }

    #[test]
    fn a_curve_sets_one_number_of_them() {
        let mut set = inputs(r#"{"rate":1.5,"at":[0.5,0.25],"flip":true,"tint":[1,1,1,1]}"#);
        assert!(set.set_part("rate", 2.0) && set.set_part("at.y", 0.75));
        assert_eq!(
            (set.part("rate"), set.part("at.x"), set.part("at.y")),
            (Some(2.0), Some(0.5), Some(0.75))
        );
        // One not set yet is made: a point's other coordinate stays the file's own.
        assert!(set.set_part("glow", 0.3) && set.set_part("from.x", 0.1));
        assert_eq!(set.get("glow"), Some(&ShaderValue::Number(0.3)));
        let Some(ShaderValue::Point(from)) = set.get("from") else {
            panic!("a point")
        };
        assert!(from[0] == 0.1 && from[1].is_nan());
        // A point with one coordinate is kept as it is; with none, it's nothing.
        set.set("lost", ShaderValue::Point([f32::NAN; 2]));
        set.sanitize(&SettingRange::ShaderInputs, ShaderInputs::default());
        assert!(set.get("from").is_some() && set.get("lost").is_none());
        // What isn't a number isn't changed, and neither is what isn't a name.
        for part in ["flip", "tint", "rate.x", "at", "at.z", "a b", "", ".x", "tint.r"] {
            assert!(!set.set_part(part, 1.0), "{part}");
        }
        assert_eq!(set.get("flip"), Some(&ShaderValue::Bool(true)));
        assert!(ShaderInputs::is_part("rate") && ShaderInputs::is_part("at.x"));
        assert!(!ShaderInputs::is_part("at.z") && !ShaderInputs::is_part("9lives"));
    }
}
