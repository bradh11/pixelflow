//! Effect settings as data: every setting's key, label, allowed values, and default, declared once
//! per effect kind next to its settings struct (see `effect.rs`). The same table gives
//!
//! - the settings panel its controls ([`effect_catalog`]),
//! - new effects their defaults (`Default` on each `*Params`),
//! - files their clamping on open ([`crate::EffectParams::sanitize`]), and edits their checks
//!   ([`crate::limit_problems`]),
//! - and the renderer its clamps (it draws [`crate::EffectParams::sanitized`] settings),
//!
//! so none of them can drift apart.

use crate::{EffectKind, EffectParams};
use serde::Serialize;

/// One choice in a list setting: the JSON value and the name people see.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ChoiceOption {
    pub value: &'static str,
    pub label: &'static str,
}

/// What a setting can hold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettingRange {
    /// A number from `min` to `max` (a slider moving by `step`).
    Number {
        min: f32,
        max: f32,
        step: f32,
        unit: Option<&'static str>,
    },
    /// A whole number from `min` to `max`.
    Int {
        min: u32,
        max: u32,
        unit: Option<&'static str>,
    },
    /// On or off.
    Bool,
    /// One of a list.
    Choice(&'static [ChoiceOption]),
}

/// One setting of one effect kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SettingSpec {
    /// The setting's key in the effect's JSON `params` (camelCase).
    pub key: &'static str,
    /// The name people see.
    pub label: &'static str,
    /// The setting's doc comment, line by line (joined into the catalog's description).
    pub doc: &'static [&'static str],
    pub range: SettingRange,
}

impl SettingSpec {
    /// The doc comment as one plain sentence or two.
    pub fn description(&self) -> String {
        self.doc.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ")
    }
}

/// A settings field type: knows how to clamp itself into its range.
pub(crate) trait SettingField: Copy {
    /// Pulls the value into `range`; NaN becomes `default`.
    fn sanitize(&mut self, range: &SettingRange, default: Self);
    /// Why the value is outside `range`, if it is (e.g. "is 70; use 0 to 50").
    fn problem(&self, range: &SettingRange) -> Option<String>;
}

impl SettingField for f32 {
    fn sanitize(&mut self, range: &SettingRange, default: Self) {
        if let SettingRange::Number { min, max, .. } = *range {
            *self = if self.is_nan() {
                default
            } else {
                self.clamp(min, max)
            };
        }
    }

    fn problem(&self, range: &SettingRange) -> Option<String> {
        let SettingRange::Number { min, max, .. } = *range else {
            return None;
        };
        if !self.is_finite() {
            Some(format!("isn't a usable number; use {min} to {max}"))
        } else if !(min..=max).contains(self) {
            Some(format!("is {self}; use {min} to {max}"))
        } else {
            None
        }
    }
}

impl SettingField for u32 {
    fn sanitize(&mut self, range: &SettingRange, _default: Self) {
        if let SettingRange::Int { min, max, .. } = *range {
            *self = (*self).clamp(min, max);
        }
    }

    fn problem(&self, range: &SettingRange) -> Option<String> {
        match *range {
            SettingRange::Int { min, max, .. } if !(min..=max).contains(self) => {
                Some(format!("is {self}; use {min} to {max}"))
            }
            _ => None,
        }
    }
}

impl SettingField for bool {
    fn sanitize(&mut self, _range: &SettingRange, _default: Self) {}

    fn problem(&self, _range: &SettingRange) -> Option<String> {
        None
    }
}

/// A list setting's type (an enum): its options, in menu order.
pub(crate) trait ChoiceSetting: Copy {
    const OPTIONS: &'static [ChoiceOption];
}

/// Implements [`ChoiceSetting`] (and a no-op [`SettingField`]: serde already refuses unknown
/// values) for an enum.
macro_rules! choices {
    ($ty:ty { $($value:literal => $label:literal),* $(,)? }) => {
        impl $crate::settings::ChoiceSetting for $ty {
            const OPTIONS: &'static [$crate::settings::ChoiceOption] = &[
                $($crate::settings::ChoiceOption { value: $value, label: $label }),*
            ];
        }
        impl $crate::settings::SettingField for $ty {
            fn sanitize(&mut self, _range: &$crate::settings::SettingRange, _default: Self) {}
            fn problem(&self, _range: &$crate::settings::SettingRange) -> Option<String> {
                None
            }
        }
    };
}
pub(crate) use choices;

/// The [`SettingRange`] written in an `effect_params!` field.
macro_rules! range {
    ($ty:ty, number($min:expr, $max:expr, $step:expr)) => {
        $crate::settings::SettingRange::Number {
            min: $min,
            max: $max,
            step: $step,
            unit: None,
        }
    };
    ($ty:ty, number($min:expr, $max:expr, $step:expr, $unit:expr)) => {
        $crate::settings::SettingRange::Number {
            min: $min,
            max: $max,
            step: $step,
            unit: Some($unit),
        }
    };
    ($ty:ty, int($min:expr, $max:expr)) => {
        $crate::settings::SettingRange::Int {
            min: $min,
            max: $max,
            unit: None,
        }
    };
    ($ty:ty, int($min:expr, $max:expr, $unit:expr)) => {
        $crate::settings::SettingRange::Int {
            min: $min,
            max: $max,
            unit: Some($unit),
        }
    };
    ($ty:ty, toggle) => {
        $crate::settings::SettingRange::Bool
    };
    ($ty:ty, choice) => {
        $crate::settings::SettingRange::Choice(<$ty as $crate::settings::ChoiceSetting>::OPTIONS)
    };
}
pub(crate) use range;

/// Declares an effect's settings struct from one table: each field's type, default, JSON key,
/// label, and range. Generates the struct (serde, missing settings take defaults), `Default`,
/// `SETTINGS`, `sanitize`, and `setting_problem`.
macro_rules! effect_params {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $(
                $(#[doc = $doc:literal])*
                $field:ident : $ty:ty = $default:expr => $key:literal, $label:literal, $kind:ident $(( $($arg:expr),* ))?;
            )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
        #[serde(default, rename_all = "camelCase")]
        pub struct $name {
            $( $(#[doc = $doc])* pub $field: $ty, )*
        }

        impl Default for $name {
            fn default() -> Self {
                Self { $( $field: $default, )* }
            }
        }

        impl $name {
            /// Every setting in JSON order: key, label, and allowed values. The renderer clamps
            /// to these ranges.
            pub const SETTINGS: &'static [$crate::settings::SettingSpec] = &[
                $(
                    $crate::settings::SettingSpec {
                        key: $key,
                        label: $label,
                        doc: &[$($doc),*],
                        range: $crate::settings::range!($ty, $kind $(( $($arg),* ))?),
                    },
                )*
            ];

            /// Pulls every setting into its range (NaN becomes the default).
            #[allow(unused_mut, unused_variables)]
            pub fn sanitize(&mut self) {
                let defaults = Self::default();
                let mut specs = Self::SETTINGS.iter();
                $(
                    let spec = specs.next().expect("one spec per field");
                    $crate::settings::SettingField::sanitize(&mut self.$field, &spec.range, defaults.$field);
                )*
            }

            /// The first setting outside its range: its spec and why.
            #[allow(unused_mut, unused_variables)]
            pub fn setting_problem(&self) -> Option<(&'static $crate::settings::SettingSpec, String)> {
                let mut specs = Self::SETTINGS.iter();
                $(
                    let spec = specs.next().expect("one spec per field");
                    if let Some(why) = $crate::settings::SettingField::problem(&self.$field, &spec.range) {
                        return Some((spec, why));
                    }
                )*
                None
            }
        }
    };
}
pub(crate) use effect_params;

/// An effect kind as the settings panel shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectInfo {
    pub kind: EffectKind,
    pub label: &'static str,
    pub description: &'static str,
    pub settings: Vec<SettingInfo>,
}

/// One setting as the settings panel shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingInfo {
    /// The key in the effect's `params`.
    pub key: &'static str,
    pub label: &'static str,
    pub description: String,
    #[serde(flatten)]
    pub value: SettingValue,
}

/// A setting's type, range, and default (`type` names the control).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SettingValue {
    Number {
        min: f32,
        max: f32,
        step: f32,
        default: f32,
        #[serde(skip_serializing_if = "Option::is_none")]
        unit: Option<&'static str>,
    },
    Int {
        min: u32,
        max: u32,
        step: u32,
        default: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        unit: Option<&'static str>,
    },
    Bool {
        default: bool,
    },
    Choice {
        default: String,
        options: Vec<ChoiceOption>,
    },
}

/// Every effect kind, in menu order, with its settings, ranges, and defaults.
pub fn effect_catalog() -> Vec<EffectInfo> {
    EffectKind::ALL.into_iter().map(effect_info).collect()
}

fn effect_info(kind: EffectKind) -> EffectInfo {
    let defaults = serde_json::to_value(EffectParams::default_for(kind)).expect("settings serialize");
    let settings = kind
        .settings()
        .iter()
        .map(|spec| {
            let default = &defaults[spec.key];
            let value = match spec.range {
                SettingRange::Number { min, max, step, unit } => SettingValue::Number {
                    min,
                    max,
                    step,
                    default: default.as_f64().expect("a number default") as f32,
                    unit,
                },
                SettingRange::Int { min, max, unit } => SettingValue::Int {
                    min,
                    max,
                    step: 1,
                    default: default.as_u64().expect("a whole-number default") as u32,
                    unit,
                },
                SettingRange::Bool => SettingValue::Bool {
                    default: default.as_bool().expect("an on/off default"),
                },
                SettingRange::Choice(options) => SettingValue::Choice {
                    default: default.as_str().expect("a choice default").to_string(),
                    options: options.to_vec(),
                },
            };
            SettingInfo {
                key: spec.key,
                label: spec.label,
                description: spec.description(),
                value,
            }
        })
        .collect();
    EffectInfo {
        kind,
        label: kind.label(),
        description: kind.description(),
        settings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn every_setting_matches_the_json_its_struct_writes() {
        for kind in EffectKind::ALL {
            let json = serde_json::to_value(EffectParams::default_for(kind)).unwrap();
            let keys: Vec<&str> = json
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .filter(|k| *k != "kind")
                .collect();
            let mut spec_keys: Vec<&str> = kind.settings().iter().map(|s| s.key).collect();
            let mut sorted = keys.clone();
            sorted.sort_unstable();
            spec_keys.sort_unstable();
            assert_eq!(spec_keys, sorted, "{kind:?}");
            for spec in kind.settings() {
                let value = &json[spec.key];
                match spec.range {
                    SettingRange::Number { min, max, step, .. } => {
                        let v = value.as_f64().unwrap() as f32;
                        assert!(
                            min < max && step > 0.0 && (min..=max).contains(&v),
                            "{kind:?}.{}",
                            spec.key
                        );
                    }
                    SettingRange::Int { min, max, .. } => {
                        let v = value.as_u64().unwrap() as u32;
                        assert!(min < max && (min..=max).contains(&v), "{kind:?}.{}", spec.key);
                    }
                    SettingRange::Bool => assert!(value.is_boolean(), "{kind:?}.{}", spec.key),
                    SettingRange::Choice(options) => {
                        assert!(
                            options.iter().any(|o| value.as_str() == Some(o.value)),
                            "{kind:?}.{}",
                            spec.key
                        );
                        // Every option is a value serde accepts, so the panel never offers a bad one.
                        for option in options {
                            let mut params = json.clone();
                            params[spec.key] = Value::from(option.value);
                            assert!(
                                serde_json::from_value::<EffectParams>(params).is_ok(),
                                "{kind:?}.{} = {}",
                                spec.key,
                                option.value
                            );
                        }
                    }
                }
                assert!(
                    !spec.label.is_empty() && !spec.description().is_empty(),
                    "{kind:?}.{}",
                    spec.key
                );
            }
        }
    }

    #[test]
    fn the_catalog_lists_every_kind_with_defaults() {
        let catalog = effect_catalog();
        assert_eq!(catalog.len(), EffectKind::ALL.len());
        let json = serde_json::to_value(&catalog).unwrap();
        let chase = &json[4];
        assert_eq!(chase["kind"], "chase");
        assert_eq!(chase["label"], "Chase");
        let speed = &chase["settings"][0];
        assert_eq!(speed["key"], "speed");
        assert_eq!(speed["type"], "number");
        assert_eq!(speed["default"], 1.0);
        assert_eq!(speed["min"], 0.0);
        assert_eq!(speed["description"], "Trips along the whole prop per second.");
        let bands = &chase["settings"][2];
        assert_eq!(
            (bands["type"].as_str(), bands["step"].as_u64()),
            (Some("int"), Some(1))
        );
        let direction = &chase["settings"][3];
        assert_eq!(direction["type"], "choice");
        assert_eq!(direction["default"], "forward");
        assert_eq!(direction["options"][1]["value"], "reverse");
        assert_eq!(chase["settings"][4]["type"], "bool");
        assert!(
            json[1]["settings"].as_array().unwrap().is_empty(),
            "Off has no settings"
        );
        assert!(catalog.iter().all(|e| !e.description.is_empty()));
    }

    #[test]
    fn sanitize_clamps_into_the_table_and_nan_takes_the_default() {
        let mut params = EffectParams::Chase(crate::ChaseParams {
            speed: f32::INFINITY,
            width: f32::NAN,
            bands: u32::MAX,
            ..Default::default()
        });
        let problem = params.setting_problem().unwrap();
        assert!(problem.contains("Speed"), "{problem}");
        params.sanitize();
        let EffectParams::Chase(p) = params else {
            unreachable!()
        };
        assert_eq!((p.speed, p.width, p.bands), (50.0, 0.2, 1000));
        assert_eq!(EffectParams::Chase(p).setting_problem(), None);
    }
}
