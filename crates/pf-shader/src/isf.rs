//! The header of a shader file: a JSON object in the file's first `/* … */` comment that starts
//! with `{`, naming the inputs the shader takes.

use crate::Error;
use serde_json::Value;

/// Most inputs one shader declares.
const MAX_INPUTS: usize = 64;
/// Most choices one input offers.
const MAX_CHOICES: usize = 256;

/// What a shader file says about itself.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Header {
    pub description: String,
    pub credit: String,
    pub inputs: Vec<Input>,
    /// Passes it asks to be drawn in (0: one, straight to the picture).
    pub passes: usize,
    /// Whether any pass keeps its picture from one frame to the next.
    pub persistent: bool,
    /// Whether it brings picture files of its own.
    pub imported: bool,
}

/// One input a shader declares.
#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    /// Its name in the shader.
    pub name: String,
    /// What to call it on screen (its name when the file gives none).
    pub label: String,
    pub kind: InputKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputKind {
    Float {
        min: f32,
        max: f32,
        default: f32,
    },
    Bool {
        default: bool,
    },
    /// One of a list: `values[i]` is what the shader is given for `labels[i]`.
    Long {
        values: Vec<i32>,
        labels: Vec<String>,
        default: i32,
    },
    Point {
        min: [f32; 2],
        max: [f32; 2],
        default: [f32; 2],
    },
    Color {
        default: [f32; 4],
    },
    /// A button: true for the frame it's pressed on.
    Event,
    Image,
    Audio,
    AudioFft,
}

/// A JSON number, however the file wrote it (`1`, `1.0`, `true`, `"1"`).
fn number(value: Option<&Value>) -> Option<f32> {
    let n = match value? {
        Value::Number(n) => n.as_f64()?,
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::String(s) => s.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    n.is_finite().then_some(n as f32)
}

fn numbers<const N: usize>(value: Option<&Value>) -> Option<[f32; N]> {
    let list = value?.as_array()?;
    let mut out = [0.0; N];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = number(list.get(i))?;
    }
    Some(out)
}

fn text(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// The header comment's JSON, if the file has one.
fn header_text(source: &str) -> Option<&str> {
    let mut rest = source;
    loop {
        let start = rest.find("/*")?;
        let after = &rest[start + 2..];
        let end = after.find("*/")?;
        let body = after[..end].trim();
        if body.starts_with('{') {
            return Some(body);
        }
        rest = &after[end + 2..];
    }
}

/// Reads the header of `source`. A file with none is a shader with no inputs.
pub(crate) fn header(source: &str) -> Result<Header, Error> {
    let Some(json) = header_text(source) else {
        return Ok(Header::default());
    };
    let root: Value = serde_json::from_str(json)
        .map_err(|e| Error::new(0, format!("its header isn't valid JSON ({e})")))?;
    let mut header = Header {
        description: text(root.get("DESCRIPTION")),
        credit: text(root.get("CREDIT")),
        ..Header::default()
    };
    if let Some(passes) = root.get("PASSES").and_then(Value::as_array) {
        header.passes = passes.len();
        header.persistent = passes.iter().any(|p| p.get("PERSISTENT").is_some());
    }
    header.persistent |= root.get("PERSISTENT_BUFFERS").is_some();
    header.imported = root.get("IMPORTED").is_some_and(|v| !v.is_null());
    let inputs = root.get("INPUTS").and_then(Value::as_array);
    for item in inputs.into_iter().flatten() {
        if header.inputs.len() >= MAX_INPUTS {
            return Err(Error::new(
                0,
                format!("it declares more than {MAX_INPUTS} inputs"),
            ));
        }
        let name = text(item.get("NAME"));
        let plain = !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !name.starts_with(|c: char| c.is_ascii_digit());
        if !plain {
            return Err(Error::new(
                0,
                format!("an input's name isn't a name (\"{name}\")"),
            ));
        }
        let kind = match text(item.get("TYPE")).to_ascii_lowercase().as_str() {
            "float" => {
                let min = number(item.get("MIN")).unwrap_or(0.0);
                let max = number(item.get("MAX")).unwrap_or(1.0);
                let (min, max) = if min <= max { (min, max) } else { (max, min) };
                let default = number(item.get("DEFAULT")).unwrap_or(min);
                InputKind::Float { min, max, default }
            }
            "bool" => InputKind::Bool {
                default: number(item.get("DEFAULT")).is_some_and(|n| n != 0.0),
            },
            "long" => {
                let values: Vec<i32> = item
                    .get("VALUES")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .take(MAX_CHOICES)
                    .map(|v| number(Some(v)).unwrap_or(0.0) as i32)
                    .collect();
                let labels: Vec<String> = item
                    .get("LABELS")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .take(MAX_CHOICES)
                    .map(|v| v.as_str().unwrap_or_default().to_string())
                    .collect();
                let default = number(item.get("DEFAULT")).map_or(0, |n| n as i32);
                InputKind::Long {
                    values,
                    labels,
                    default,
                }
            }
            "point2d" => InputKind::Point {
                min: numbers(item.get("MIN")).unwrap_or([0.0, 0.0]),
                max: numbers(item.get("MAX")).unwrap_or([1.0, 1.0]),
                default: numbers(item.get("DEFAULT")).unwrap_or([0.0, 0.0]),
            },
            "color" => InputKind::Color {
                default: numbers(item.get("DEFAULT")).unwrap_or([1.0, 1.0, 1.0, 1.0]),
            },
            "event" => InputKind::Event,
            "image" => InputKind::Image,
            "audio" => InputKind::Audio,
            "audiofft" => InputKind::AudioFft,
            other => {
                return Err(Error::new(
                    0,
                    format!("input \"{name}\" is of a kind this can't give it (\"{other}\")"),
                ));
            }
        };
        if header.inputs.iter().any(|i| i.name == name) {
            return Err(Error::new(0, format!("two inputs are called \"{name}\"")));
        }
        let label = text(item.get("LABEL"));
        header.inputs.push(Input {
            label: if label.is_empty() { name.clone() } else { label },
            name,
            kind,
        });
    }
    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_lists_inputs_of_each_kind_however_the_numbers_are_written() {
        let source = r#"// a note first
/* {
  "DESCRIPTION": "rings", "CREDIT": "nobody",
  "INPUTS": [
    { "NAME": "speed", "TYPE": "float", "MIN": 0, "MAX": 4.0, "DEFAULT": 1 },
    { "NAME": "flip", "TYPE": "bool", "DEFAULT": "1" },
    { "NAME": "mode", "TYPE": "long", "VALUES": [0, 5], "LABELS": ["One", "Two "], "DEFAULT": 5, "LABEL": "Mode: " },
    { "NAME": "at", "TYPE": "point2D", "MIN": [0, 0], "MAX": [1, 1], "DEFAULT": [0.5, 0.25] },
    { "NAME": "tint", "TYPE": "color", "DEFAULT": [1, 0.5, 0, 1] }
  ]
} */
void main() {}"#;
        let h = header(source).unwrap();
        assert_eq!((h.description.as_str(), h.credit.as_str()), ("rings", "nobody"));
        assert_eq!(h.inputs.len(), 5);
        assert_eq!(
            h.inputs[0].kind,
            InputKind::Float {
                min: 0.0,
                max: 4.0,
                default: 1.0
            }
        );
        assert_eq!(h.inputs[1].kind, InputKind::Bool { default: true });
        assert_eq!(h.inputs[2].label, "Mode:");
        assert_eq!(
            h.inputs[2].kind,
            InputKind::Long {
                values: vec![0, 5],
                labels: vec!["One".into(), "Two ".into()],
                default: 5
            }
        );
        assert_eq!(
            h.inputs[3].kind,
            InputKind::Point {
                min: [0.0, 0.0],
                max: [1.0, 1.0],
                default: [0.5, 0.25]
            }
        );
        assert_eq!(
            h.inputs[4].kind,
            InputKind::Color {
                default: [1.0, 0.5, 0.0, 1.0]
            }
        );
        assert_eq!((h.passes, h.persistent, h.imported), (0, false, false));
    }

    #[test]
    fn a_file_with_no_header_has_no_inputs_and_a_broken_one_is_refused() {
        assert_eq!(
            header("/* just a comment */ void main() {}").unwrap(),
            Header::default()
        );
        assert!(header("/* { \"INPUTS\": [ } */").is_err());
        assert!(header(r#"/*{"INPUTS":[{"NAME":"a b","TYPE":"float"}]}*/"#).is_err());
        assert!(
            header(r#"/*{"INPUTS":[{"NAME":"a","TYPE":"float"},{"NAME":"a","TYPE":"bool"}]}*/"#).is_err()
        );
    }

    #[test]
    fn passes_and_kept_pictures_are_noticed() {
        let h = header(
            r#"/*{"PASSES":[{"TARGET":"a","PERSISTENT":true},{}],"IMPORTED":{"x":{"PATH":"x.png"}}}*/"#,
        )
        .unwrap();
        assert_eq!((h.passes, h.persistent, h.imported), (2, true, true));
    }
}
