//! The assistant's tools: one per engine edit (show [`Edit`]s and open-sequence
//! [`SequenceEdit`]s), generated from the edit types' JSON Schemas so they follow the engine as
//! it changes; read-only query tools; and the draft tools (review, start over, propose).
//!
//! There is deliberately no tool that saves or exports files, sends to controllers, starts
//! output or playback, or talks to devices: those stay behind the user's own clicks.

use crate::provider::ToolSpec;
use pf_engine::{Edit, SequenceEdit};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

/// What running a tool does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolKind {
    /// Drafts one show edit; `tag` is the edit's `type`.
    ShowEdit {
        tag: String,
    },
    /// Drafts one edit to the open sequence.
    SequenceEdit {
        tag: String,
    },
    Query(Query),
    ReviewDraft,
    ResetDraft,
    Propose,
}

/// Read-only questions about the draft show and sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Query {
    Overview,
    ListProps,
    GetProp,
    ListGroups,
    GetGroup,
    ListControllers,
    GetController,
    ListPlaylist,
    Selection,
    EffectKinds,
    OpenSequence,
    SequenceEffects,
    TimingMarks,
}

#[derive(Debug, Clone)]
pub struct Tool {
    pub spec: ToolSpec,
    pub kind: ToolKind,
}

/// Every tool the assistant has, in a fixed order (so a provider's cached prefix stays valid).
#[derive(Debug, Clone)]
pub struct Toolbox {
    tools: Vec<Tool>,
}

impl Default for Toolbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Toolbox {
    pub fn new() -> Self {
        let mut tools = query_tools();
        tools.extend(show_edit_tools());
        tools.extend(sequence_edit_tools());
        tools.extend(draft_tools());
        Self { tools }
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.iter().map(|t| t.spec.clone()).collect()
    }

    pub fn find(&self, name: &str) -> Option<&Tool> {
        self.tools.iter().find(|t| t.spec.name == name)
    }

    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }
}

/// "addProp" → "add_prop".
fn snake(tag: &str) -> String {
    let mut out = String::new();
    for c in tag.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// "addProp" → "Add prop".
fn sentence(tag: &str) -> String {
    let words = snake(tag).replace('_', " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => words,
    }
}

/// The tool name for an edit tag.
pub fn show_tool_name(tag: &str) -> String {
    format!("show_{}", snake(tag))
}

pub fn sequence_tool_name(tag: &str) -> String {
    format!("sequence_{}", snake(tag))
}

/// Every `#/$defs/...` a schema refers to, followed through the definitions.
fn referenced_defs(schema: &Value, defs: &Map<String, Value>) -> BTreeSet<String> {
    fn walk(value: &Value, found: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                if let Some(Value::String(r)) = map.get("$ref")
                    && let Some(name) = r.strip_prefix("#/$defs/")
                {
                    found.push(name.to_string());
                }
                map.values().for_each(|v| walk(v, found));
            }
            Value::Array(items) => items.iter().for_each(|v| walk(v, found)),
            _ => {}
        }
    }
    let mut seen = BTreeSet::new();
    let mut queue = Vec::new();
    walk(schema, &mut queue);
    while let Some(name) = queue.pop() {
        if seen.insert(name.clone())
            && let Some(def) = defs.get(&name)
        {
            walk(def, &mut queue);
        }
    }
    seen
}

/// One tool per variant of an internally tagged (`"type"`) enum's schema: the variant's fields
/// become the tool's input (the tag is implied by the tool), with the definitions it uses.
fn variant_tools(
    root: Value,
    name: impl Fn(&str) -> String,
    describe: impl Fn(&str, Option<&str>) -> String,
    kind: impl Fn(String) -> ToolKind,
) -> Vec<Tool> {
    let defs = root["$defs"].as_object().cloned().unwrap_or_default();
    let variants = root["oneOf"].as_array().cloned().unwrap_or_default();
    variants
        .into_iter()
        .filter_map(|mut variant| {
            let tag = variant["properties"]["type"]["const"].as_str()?.to_string();
            let object = variant.as_object_mut()?;
            let doc = object
                .remove("description")
                .and_then(|d| d.as_str().map(str::to_string));
            if let Some(Value::Object(properties)) = object.get_mut("properties") {
                properties.remove("type");
            }
            if let Some(Value::Array(required)) = object.get_mut("required") {
                required.retain(|r| r != "type");
            }
            object.insert("type".into(), json!("object"));
            object.entry("properties").or_insert_with(|| json!({}));
            let used = referenced_defs(&variant, &defs);
            if !used.is_empty() {
                let picked: Map<String, Value> = used
                    .into_iter()
                    .filter_map(|name| defs.get(&name).map(|d| (name, d.clone())))
                    .collect();
                variant["$defs"] = Value::Object(picked);
            }
            Some(Tool {
                spec: ToolSpec {
                    name: name(&tag),
                    description: describe(&tag, doc.as_deref()),
                    input_schema: variant,
                },
                kind: kind(tag),
            })
        })
        .collect()
}

fn hints(tag: &str) -> &'static str {
    if tag.starts_with("add") {
        " A new item needs a new random UUID (version 4) as its id; existing ids come from the list and get tools."
    } else if tag.starts_with("update") {
        " Send the whole item as it should be: get it first, then change only what you mean to change."
    } else {
        ""
    }
}

/// One tool per show [`Edit`] variant.
pub fn show_edit_tools() -> Vec<Tool> {
    let root = serde_json::to_value(schemars::schema_for!(Edit)).unwrap_or(Value::Null);
    variant_tools(
        root,
        show_tool_name,
        |tag, doc| {
            format!(
                "{}{} Changes your draft of the show only; the user reviews the proposal before anything changes.{}",
                doc.map(str::to_string).unwrap_or_else(|| sentence(tag)),
                if doc.is_some_and(|d| d.ends_with('.')) {
                    ""
                } else {
                    "."
                },
                hints(tag)
            )
        },
        |tag| ToolKind::ShowEdit { tag },
    )
}

/// One tool per open-sequence [`SequenceEdit`] variant.
pub fn sequence_edit_tools() -> Vec<Tool> {
    let root = serde_json::to_value(schemars::schema_for!(SequenceEdit)).unwrap_or(Value::Null);
    variant_tools(
        root,
        sequence_tool_name,
        |tag, doc| {
            format!(
                "{}{} Changes your draft of the open sequence only (fails when no sequence is open).{}",
                doc.map(|d| d.replace('\n', " ")).unwrap_or_else(|| sentence(tag)),
                if doc.is_some_and(|d| d.ends_with('.')) {
                    ""
                } else {
                    "."
                },
                hints(tag)
            )
        },
        |tag| ToolKind::SequenceEdit { tag },
    )
}

/// Turns a show edit tool call back into the engine's edit (its input plus the implied tag).
pub fn show_edit(tag: &str, input: &Value) -> Result<Edit, String> {
    serde_json::from_value(tagged(tag, input)?).map_err(|e| format!("That input doesn't fit this edit: {e}"))
}

pub fn sequence_edit(tag: &str, input: &Value) -> Result<SequenceEdit, String> {
    serde_json::from_value(tagged(tag, input)?).map_err(|e| format!("That input doesn't fit this edit: {e}"))
}

fn tagged(tag: &str, input: &Value) -> Result<Value, String> {
    let mut object = match input {
        Value::Object(map) => map.clone(),
        Value::Null => Map::new(),
        _ => return Err("The tool input must be a JSON object.".to_string()),
    };
    object.insert("type".into(), json!(tag));
    Ok(Value::Object(object))
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required })
}

fn query(name: &str, description: &str, input_schema: Value, q: Query) -> Tool {
    Tool {
        spec: ToolSpec {
            name: name.into(),
            description: description.into(),
            input_schema,
        },
        kind: ToolKind::Query(q),
    }
}

fn id_input(what: &str) -> Value {
    object(
        json!({ "id": { "type": "string", "description": format!("The {what}'s id.") } }),
        &["id"],
    )
}

fn time_range() -> Value {
    json!({
        "fromMs": { "type": "integer", "minimum": 0, "description": "Only from this time (ms)." },
        "toMs": { "type": "integer", "minimum": 0, "description": "Only up to this time (ms)." }
    })
}

/// Read-only tools. They answer from the draft, so they include the assistant's own changes.
fn query_tools() -> Vec<Tool> {
    vec![
        query(
            "get_show_overview",
            "The show at a glance (with your draft changes): its name, frame rate, how many props, groups, controllers, and playlist sequences, its problems, the open sequence, and what the user has selected.",
            object(json!({}), &[]),
            Query::Overview,
        ),
        query(
            "list_props",
            "Every prop: id, name, shape type, pixel count, position, the controller port it's wired to, and its groups. Optionally only props whose name contains some text.",
            object(json!({ "nameContains": { "type": "string" } }), &[]),
            Query::ListProps,
        ),
        query(
            "get_prop",
            "One prop in full (as show_update_prop takes it).",
            id_input("prop"),
            Query::GetProp,
        ),
        query(
            "list_groups",
            "Every group: id, name, and its members by name.",
            object(json!({}), &[]),
            Query::ListGroups,
        ),
        query(
            "get_group",
            "One group in full (as show_update_group takes it).",
            id_input("group"),
            Query::GetGroup,
        ),
        query(
            "list_controllers",
            "Every controller: id, name, address, protocol, and what's wired to each port.",
            object(json!({}), &[]),
            Query::ListControllers,
        ),
        query(
            "get_controller",
            "One controller in full (as show_update_controller takes it).",
            id_input("controller"),
            Query::GetController,
        ),
        query(
            "list_playlist",
            "The show's playlist: rendered sequences (.fseq) in play order.",
            object(json!({}), &[]),
            Query::ListPlaylist,
        ),
        query(
            "get_selection",
            "What the user has selected right now: props (layout) and effects (sequence editor), and the playhead.",
            object(json!({}), &[]),
            Query::Selection,
        ),
        query(
            "list_effect_kinds",
            "Every effect kind for sequences, with its settings: keys, ranges, defaults, and choices.",
            object(json!({}), &[]),
            Query::EffectKinds,
        ),
        query(
            "get_open_sequence",
            "The sequence open in the editor (with your draft changes): name, length, music, rows (what each lights, layers, effect counts), and timing tracks.",
            object(json!({}), &[]),
            Query::OpenSequence,
        ),
        query(
            "list_sequence_effects",
            "Effects in the open sequence, in full: optionally one row only, and only those overlapping a time range.",
            object(
                {
                    let mut p = time_range();
                    p["rowId"] = json!({ "type": "string", "description": "Only this row." });
                    p
                },
                &[],
            ),
            Query::SequenceEffects,
        ),
        query(
            "get_timing_marks",
            "A timing track's marks (start, end, label), optionally only in a time range.",
            object(
                {
                    let mut p = time_range();
                    p["trackId"] = json!({ "type": "string" });
                    p
                },
                &["trackId"],
            ),
            Query::TimingMarks,
        ),
    ]
}

fn draft_tools() -> Vec<Tool> {
    vec![
        Tool {
            spec: ToolSpec {
                name: "review_draft".into(),
                description: "Lists everything your draft changes so far, compared with the user's show and sequence."
                    .into(),
                input_schema: object(json!({}), &[]),
            },
            kind: ToolKind::ReviewDraft,
        },
        Tool {
            spec: ToolSpec {
                name: "reset_draft".into(),
                description: "Throws your draft changes away and starts again from the user's show and sequence.".into(),
                input_schema: object(json!({}), &[]),
            },
            kind: ToolKind::ResetDraft,
        },
        Tool {
            spec: ToolSpec {
                name: "propose_changes".into(),
                description: "Shows your draft to the user as a proposal: your summary, a list of every change, and a preview, with Apply and Discard buttons. Call it once, when the draft is ready. Nothing changes unless the user presses Apply, which applies the whole draft as one undo step."
                    .into(),
                input_schema: object(
                    json!({ "summary": { "type": "string", "description": "One or two plain sentences saying what the changes do." } }),
                    &["summary"],
                ),
            },
            kind: ToolKind::Propose,
        },
    ]
}
