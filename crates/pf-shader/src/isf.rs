//! The header of a shader file: a JSON object in the file's first `/* … */` comment that starts
//! with `{`, naming the inputs the shader takes.

use crate::Error;
use serde_json::Value;

/// Most inputs one shader declares.
pub const MAX_INPUTS: usize = 64;
/// Most choices one input offers.
pub const MAX_CHOICES: usize = 256;
/// Most characters kept of a description, a credit, and a label.
const MAX_DESCRIPTION: usize = 1000;
const MAX_CREDIT: usize = 200;
const MAX_LABEL: usize = 100;
/// The furthest from zero a number in a header may be: far past any range a shader has a use
/// for, and small enough that arithmetic on two of them stays a number.
const MOST: f32 = 1.0e9;

/// What a shader file says about itself.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Header {
    pub description: String,
    pub credit: String,
    pub inputs: Vec<Input>,
    /// Passes it asks to be drawn in (0: one, straight to the picture).
    pub passes: usize,
    /// Whether any pass draws to a picture of its own for a later pass to read.
    pub targets: bool,
    /// Whether any pass keeps its picture from one frame to the next.
    pub persistent: bool,
    /// Whether it brings picture files of its own.
    pub imported: bool,
}

impl Header {
    /// What the shader needs that PixelFlow doesn't give a shader yet, in plain words (`None`:
    /// nothing). Such a shader is refused, not drawn without it.
    pub fn unsupported(&self) -> Option<String> {
        if self.persistent {
            return Some(
                "it keeps its picture from one frame to the next (a persistent buffer), which isn't supported yet"
                    .into(),
            );
        }
        if self.passes > 1 || self.targets {
            return Some("it draws in several passes (PASSES), which isn't supported yet".into());
        }
        if self.imported {
            return Some("it brings picture files of its own (IMPORTED), which isn't supported yet".into());
        }
        self.inputs.iter().find_map(|input| {
            let what = match input.kind {
                InputKind::Image => "a picture (an image input)",
                InputKind::Audio | InputKind::AudioFft => "the music (an audio input)",
                _ => return None,
            };
            Some(format!(
                "its input \"{}\" is {what}, which isn't supported yet",
                input.name
            ))
        })
    }
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
    n.is_finite().then_some((n as f32).clamp(-MOST, MOST))
}

fn numbers<const N: usize>(value: Option<&Value>) -> Option<[f32; N]> {
    let list = value?.as_array()?;
    let mut out = [0.0; N];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = number(list.get(i))?;
    }
    Some(out)
}

/// A JSON string, trimmed, and at most `most` characters of it.
fn text(value: Option<&Value>, most: usize) -> String {
    let text = value.and_then(Value::as_str).unwrap_or_default().trim();
    text.chars().take(most).collect()
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
        description: text(root.get("DESCRIPTION"), MAX_DESCRIPTION),
        credit: text(root.get("CREDIT"), MAX_CREDIT),
        ..Header::default()
    };
    if let Some(passes) = root.get("PASSES").and_then(Value::as_array) {
        // `"PERSISTENT": false` says nothing is kept, as leaving it out does.
        let set = |pass: &Value, key: &str| {
            pass.get(key)
                .is_some_and(|v| !v.is_null() && v.as_bool() != Some(false))
        };
        header.passes = passes.len();
        header.targets = passes.iter().any(|p| set(p, "TARGET"));
        header.persistent = passes.iter().any(|p| set(p, "PERSISTENT"));
    }
    header.persistent |= root.get("PERSISTENT_BUFFERS").is_some_and(|v| !v.is_null());
    header.imported = root.get("IMPORTED").is_some_and(|v| !v.is_null());
    let inputs = root.get("INPUTS").and_then(Value::as_array);
    for item in inputs.into_iter().flatten() {
        if header.inputs.len() >= MAX_INPUTS {
            return Err(Error::new(
                0,
                format!("it declares more than {MAX_INPUTS} inputs"),
            ));
        }
        let name = text(item.get("NAME"), MAX_LABEL + 1);
        let plain = !name.is_empty()
            && name.len() <= MAX_LABEL
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !name.starts_with(|c: char| c.is_ascii_digit());
        if !plain {
            return Err(Error::new(
                0,
                format!("an input's name isn't a name (\"{name}\")"),
            ));
        }
        let kind = match text(item.get("TYPE"), MAX_LABEL).to_ascii_lowercase().as_str() {
            "float" => {
                let min = number(item.get("MIN")).unwrap_or(0.0);
                let max = number(item.get("MAX")).unwrap_or(1.0);
                let (min, max) = if min <= max { (min, max) } else { (max, min) };
                let default = number(item.get("DEFAULT")).unwrap_or(min).clamp(min, max);
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
                    .map(|v| text(Some(v), MAX_LABEL))
                    .collect();
                // The first choice, when the file's own isn't one of them.
                let default = number(item.get("DEFAULT"))
                    .map(|n| n as i32)
                    .filter(|n| values.is_empty() || values.contains(n))
                    .or(values.first().copied())
                    .unwrap_or(0);
                InputKind::Long {
                    values,
                    labels,
                    default,
                }
            }
            "point2d" => {
                let a: [f32; 2] = numbers(item.get("MIN")).unwrap_or([0.0, 0.0]);
                let b: [f32; 2] = numbers(item.get("MAX")).unwrap_or([1.0, 1.0]);
                let min = [a[0].min(b[0]), a[1].min(b[1])];
                let max = [a[0].max(b[0]), a[1].max(b[1])];
                let at: [f32; 2] = numbers(item.get("DEFAULT")).unwrap_or(min);
                InputKind::Point {
                    min,
                    max,
                    default: [at[0].clamp(min[0], max[0]), at[1].clamp(min[1], max[1])],
                }
            }
            "color" => InputKind::Color {
                default: numbers(item.get("DEFAULT"))
                    .unwrap_or([1.0, 1.0, 1.0, 1.0])
                    .map(|v: f32| v.clamp(0.0, 1.0)),
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
        let label = text(item.get("LABEL"), MAX_LABEL);
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
                labels: vec!["One".into(), "Two".into()],
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
        assert_eq!(h.unsupported(), None);
    }

    #[test]
    fn defaults_outside_their_range_and_absurd_numbers_are_brought_inside() {
        let h = header(
            r#"/*{"INPUTS":[
                {"NAME":"a","TYPE":"float","MIN":2,"MAX":1,"DEFAULT":9},
                {"NAME":"b","TYPE":"float","MIN":-1e30,"MAX":1e30},
                {"NAME":"c","TYPE":"long","VALUES":[3,4],"LABELS":["x","y"],"DEFAULT":7},
                {"NAME":"d","TYPE":"point2D","MIN":[1,1],"MAX":[0,0],"DEFAULT":[5,-5]},
                {"NAME":"e","TYPE":"color","DEFAULT":[2,-1,0.5,1]}
            ]}*/"#,
        )
        .unwrap();
        let kinds: Vec<&InputKind> = h.inputs.iter().map(|i| &i.kind).collect();
        assert_eq!(
            *kinds[0],
            InputKind::Float {
                min: 1.0,
                max: 2.0,
                default: 2.0
            }
        );
        assert_eq!(
            *kinds[1],
            InputKind::Float {
                min: -1.0e9,
                max: 1.0e9,
                default: -1.0e9
            }
        );
        assert!(matches!(kinds[2], InputKind::Long { default: 3, .. }));
        assert_eq!(
            *kinds[3],
            InputKind::Point {
                min: [0.0, 0.0],
                max: [1.0, 1.0],
                default: [1.0, 0.0]
            }
        );
        assert_eq!(
            *kinds[4],
            InputKind::Color {
                default: [1.0, 0.0, 0.5, 1.0]
            }
        );
        // A description is kept to a length a panel can show.
        let long = format!(r#"/*{{"DESCRIPTION":"{}"}}*/"#, "x".repeat(5000));
        assert_eq!(header(&long).unwrap().description.len(), 1000);
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
        assert!(h.unsupported().unwrap().contains("from one frame to the next"));
        // Each thing a shader can need that isn't given yet is named.
        let need = |json: &str| header(json).unwrap().unsupported().unwrap_or_default();
        assert!(need(r#"/*{"PASSES":[{"TARGET":"a"},{}]}*/"#).contains("several passes"));
        assert!(need(r#"/*{"PASSES":[{"TARGET":"a"}]}*/"#).contains("several passes"));
        assert!(need(r#"/*{"IMPORTED":{"x":{"PATH":"x.png"}}}*/"#).contains("picture files of its own"));
        assert!(need(r#"/*{"INPUTS":[{"NAME":"in","TYPE":"image"}]}*/"#).contains("\"in\" is a picture"));
        assert!(need(r#"/*{"INPUTS":[{"NAME":"fft","TYPE":"audioFFT"}]}*/"#).contains("the music"));
        // One pass straight to the picture is no different from none.
        assert_eq!(need(r#"/*{"PASSES":[{}]}*/"#), "");
        assert_eq!(need(r#"/*{"PASSES":[{"PERSISTENT":false}]}*/"#), "");
    }
}
