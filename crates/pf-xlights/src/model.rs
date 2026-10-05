//! `<model>` elements from `xlights_rgbeffects.xml`, with attribute helpers that follow xLights'
//! read rules (named attribute first, then the legacy `parm1..3`, then the type's default).

use std::collections::BTreeMap;

/// One `<model>` as written by xLights: its type, attributes, and controller connection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct XmlModel {
    pub name: String,
    /// The `DisplayAs` type, e.g. "Single Line", "Arches", "Tree 360".
    pub display_as: String,
    pub attrs: BTreeMap<String, String>,
    /// Attributes of the `<ControllerConnection>` child, if any.
    pub connection: BTreeMap<String, String>,
}

impl XmlModel {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(String::as_str)
    }

    /// A number from the named attribute, else the legacy fallback attribute (e.g. `parm1`),
    /// else `default`. Unparseable values count as missing.
    pub fn num(&self, key: &str, legacy: Option<&str>, default: f64) -> f64 {
        let parse = |k: &str| self.attr(k).and_then(|v| v.trim().parse::<f64>().ok());
        parse(key).or_else(|| legacy.and_then(parse)).unwrap_or(default)
    }

    /// [`XmlModel::num`] as a whole number (truncated, like xLights' `atoi`).
    pub fn int(&self, key: &str, legacy: Option<&str>, default: i64) -> i64 {
        let parse = |k: &str| {
            self.attr(k).and_then(|v| {
                let v = v.trim();
                v.parse::<i64>()
                    .ok()
                    .or_else(|| v.parse::<f64>().ok().map(|f| f as i64))
            })
        };
        parse(key).or_else(|| legacy.and_then(parse)).unwrap_or(default)
    }

    /// A boolean attribute ("1", "true", "TRUE" are true).
    pub fn flag(&self, key: &str) -> bool {
        matches!(
            self.attr(key).map(str::trim),
            Some("1" | "true" | "TRUE" | "True")
        )
    }

    /// A string attribute, or `default`.
    pub fn text<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.attr(key).unwrap_or(default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(attrs: &[(&str, &str)]) -> XmlModel {
        XmlModel {
            attrs: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..XmlModel::default()
        }
    }

    #[test]
    fn named_attributes_win_over_legacy_ones() {
        let m = model(&[("NumStrings", "4"), ("parm1", "9"), ("parm2", "50"), ("Odd", "x")]);
        assert_eq!(m.int("NumStrings", Some("parm1"), 1), 4);
        assert_eq!(m.int("NodesPerString", Some("parm2"), 1), 50);
        assert_eq!(m.int("Missing", None, 7), 7);
        assert_eq!(m.int("Odd", None, 7), 7, "unparseable counts as missing");
        assert_eq!(model(&[("X", "12.9")]).int("X", None, 0), 12);
        assert!(model(&[("Z", "TRUE")]).flag("Z") && !model(&[]).flag("Z"));
    }
}
