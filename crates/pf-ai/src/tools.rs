//! The assistant's tools: one per engine edit (show [`Edit`]s and open-sequence
//! [`SequenceEdit`]s), generated from the edit types' JSON Schemas so they follow the engine as
//! it changes; read-only query tools; and the draft tools (review, start over, propose).
//!
//! There is deliberately no tool that saves or exports files, sends to controllers, starts
//! output or playback, or talks to devices: those stay behind the user's own clicks. Nor is
//! there one that looks at the disk for the show's files or points the show at files found
//! there (see [`FILE_OPERATIONS`]).

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
    /// Reads the open sequence's song (tempo, bars, sections, accents, energy).
    AnalyzeSong,
    /// Adds the song's timing tracks to the draft.
    AddSongTiming,
    /// One effect on many rows, along timing marks.
    PlaceEffects,
    /// Copies a stretch of effects to other times.
    RepeatEffects,
    /// Offers the user a button to start a new sequence from a song.
    AskForSong,
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
    ShapeSettings,
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
        tools.extend(song_tools());
        tools.extend(draft_tools());
        compact_large_unions(&mut tools);
        share_large_definitions(&mut tools);
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

/// The engine's operations on the files a show refers to: checking whether they're there,
/// searching the disk for missing ones, and pointing the show (or the open sequence) at files
/// found or located. They read the disk and are the user's to start, so the assistant never gets
/// them as tools, even should one become an edit (an edit whose tag, in snake case, is one of
/// these is left out of the toolbox).
pub const FILE_OPERATIONS: &[&str] = &[
    "check_files",
    "find_missing_files",
    "use_found_files",
    "locate_file",
    "relink_file",
    "sequence_music_missing",
    "find_sequence_music",
    "use_found_sequence_music",
    "locate_sequence_music",
    "relink_sequence_music",
];

/// Whether the edit `tag` ("relinkFile") is one of the [`FILE_OPERATIONS`].
pub fn is_file_operation(tag: &str) -> bool {
    FILE_OPERATIONS.contains(&snake(tag).as_str())
}

/// The tool name for an edit tag.
pub fn show_tool_name(tag: &str) -> String {
    format!("show_{}", snake(tag))
}

pub fn sequence_tool_name(tag: &str) -> String {
    format!("sequence_{}", snake(tag))
}

/// The most the whole tool block may weigh, as JSON sent to a provider (bytes). Providers cache
/// it with the system prompt; past this it crowds the conversation out.
pub const TOOL_BUDGET_BYTES: usize = 64_000;
/// How much of [`TOOL_BUDGET_BYTES`] stays free for new tools, edits, and options.
pub const TOOL_HEADROOM_BYTES: usize = 12_000;
/// The most one tool may weigh: a large union belongs behind a lookup tool instead (see
/// [`compact_large_unions`]).
pub const ONE_TOOL_BUDGET_BYTES: usize = 8_000;

/// The prop shapes' full schema (every generator with its settings).
fn generator_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(pf_model::Generator)).unwrap_or(Value::Null)
}

/// Every prop shape's `type`, in order.
pub fn shape_types() -> Vec<String> {
    generator_schema()["oneOf"]
        .as_array()
        .map(|variants| {
            variants
                .iter()
                .filter_map(|v| v["properties"]["type"]["const"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// One prop shape's full settings (a JSON Schema with the definitions it uses), or `None` for a
/// shape that doesn't exist. What `shape_settings` answers.
pub fn shape_settings(shape: &str) -> Option<Value> {
    let root = generator_schema();
    let defs = root["$defs"].as_object().cloned().unwrap_or_default();
    let mut variant = root["oneOf"]
        .as_array()?
        .iter()
        .find(|v| v["properties"]["type"]["const"] == shape)?
        .clone();
    let used = referenced_defs(&variant, &defs);
    if !used.is_empty() {
        variant["$defs"] = Value::Object(
            used.into_iter()
                .filter_map(|name| defs.get(&name).map(|d| (name, d.clone())))
                .collect(),
        );
    }
    tidy(&mut variant);
    Some(variant)
}

/// Every effect kind's `kind`, in order.
fn effect_kinds() -> Vec<Value> {
    pf_sequence::EffectKind::ALL
        .iter()
        .filter_map(|kind| serde_json::to_value(kind).ok())
        .collect()
}

/// Replaces the two largest unions, every prop shape (`Generator`) and every effect kind's
/// settings (`EffectParams`), with their tag and a pointer to the tool that spells one out
/// (`shape_settings`, `list_effect_kinds`). Together they were over 17 KB of every request. The
/// engine still checks every shape and setting on use, and says what doesn't fit.
fn compact_large_unions(tools: &mut [Tool]) {
    let shapes = shape_types();
    let kinds = effect_kinds();
    for tool in tools.iter_mut() {
        let Some(defs) = tool
            .spec
            .input_schema
            .get_mut("$defs")
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        if defs.contains_key("Generator") {
            defs.insert(
                "Generator".into(),
                json!({
                    "type": "object",
                    "description": "A generated shape: `type` plus that shape's settings. Call shape_settings for one shape's settings and defaults.",
                    "properties": { "type": { "enum": shapes } },
                    "required": ["type"],
                }),
            );
        }
        if defs.contains_key("EffectParams") {
            defs.insert(
                "EffectParams".into(),
                json!({
                    "type": "object",
                    "description": "`kind` plus any of its settings (missing ones take their defaults). list_effect_kinds lists each kind's settings.",
                    "properties": { "kind": { "enum": kinds } },
                    "required": ["kind"],
                }),
            );
        }
    }
}

/// Drops `format` annotations, defaults that are their type's empty value, and empty
/// descriptions, and writes a choice of plain names as an `enum` (as
/// [`share_large_definitions`] does for the tools themselves).
fn tidy(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if map.get("format").is_some_and(Value::is_string) {
                map.remove("format");
            }
            if map.get("default").is_some_and(is_empty_value) {
                map.remove("default");
            }
            if map.get("description").is_some_and(|d| d.as_str() == Some("")) {
                map.remove("description");
            }
            map.values_mut().for_each(tidy);
            let names: Option<Vec<Value>> = map.get("oneOf").and_then(Value::as_array).and_then(|items| {
                items
                    .iter()
                    .map(|item| {
                        let item = item.as_object()?;
                        (item.len() == 2 && item.get("type") == Some(&json!("string")))
                            .then(|| item.get("const").cloned())
                            .flatten()
                    })
                    .collect()
            });
            if let Some(names) = names {
                map.remove("oneOf");
                map.insert("type".into(), json!("string"));
                map.insert("enum".into(), Value::Array(names));
            }
        }
        Value::Array(items) => items.iter_mut().for_each(tidy),
        _ => {}
    }
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

/// Large definitions spelled out in full in one tool only (the one that adds such a thing); every
/// other tool that takes one refers to it there, so a request doesn't carry the same 10 KB schema
/// four times. The reference still accepts the item (as any object): the engine checks it on use.
const SHARED_DEFINITIONS: &[(&str, &str, &str)] = &[
    ("Prop", "show_add_prop", "prop"),
    ("Controller", "show_add_controller", "controller"),
    ("Group", "show_add_group", "group"),
    ("Effect", "sequence_add_effect", "effect"),
    ("EffectParams", "sequence_add_effect", "effect.params"),
];

/// A value that is its type's empty one: `false`, `0`, `null`, `""`, `[]` or `{}`.
fn is_empty_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
    }
}

/// Rewrites `$ref`s to shared definitions outside their owning tool, then drops the definitions
/// those tools no longer use. Also drops `format` annotations (number widths), which don't
/// constrain anything here, defaults that are the type's empty value (`false`, `0`, `null`,
/// nothing), which say no more than leaving the field out does, and empty descriptions; and
/// writes a choice of plain names as an `enum`.
fn share_large_definitions(tools: &mut [Tool]) {
    fn rewrite(
        value: &mut Value,
        owner_of: &dyn Fn(&str) -> Option<(&'static str, &'static str)>,
        tool: &str,
    ) {
        match value {
            Value::Object(map) => {
                if map.get("format").is_some_and(Value::is_string) {
                    map.remove("format");
                }
                if map.get("default").is_some_and(is_empty_value) {
                    map.remove("default");
                }
                // An empty description (set to keep a Rust doc comment out of the schema) says nothing.
                if map.get("description").is_some_and(|d| d.as_str() == Some("")) {
                    map.remove("description");
                }
                if let Some(Value::String(r)) = map.get("$ref")
                    && let Some(name) = r.strip_prefix("#/$defs/")
                    && let Some((owner, field)) = owner_of(name)
                    && owner != tool
                {
                    *value = json!({
                        "type": "object",
                        "description": format!("A {name}, exactly as `{field}` in the {owner} tool's input."),
                    });
                    return;
                }
                map.values_mut().for_each(|v| rewrite(v, owner_of, tool));
                // A choice of plain names (variants documented in Rust only) as the short `enum`.
                let names: Option<Vec<Value>> =
                    map.get("oneOf").and_then(Value::as_array).and_then(|items| {
                        items
                            .iter()
                            .map(|item| {
                                let item = item.as_object()?;
                                (item.len() == 2 && item.get("type") == Some(&json!("string")))
                                    .then(|| item.get("const").cloned())
                                    .flatten()
                            })
                            .collect()
                    });
                if let Some(names) = names {
                    map.remove("oneOf");
                    map.insert("type".into(), json!("string"));
                    map.insert("enum".into(), Value::Array(names));
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|v| rewrite(v, owner_of, tool)),
            _ => {}
        }
    }
    let owner_of = |name: &str| {
        SHARED_DEFINITIONS
            .iter()
            .find(|(def, _, _)| *def == name)
            .map(|(_, owner, field)| (*owner, *field))
    };
    for tool in tools.iter_mut() {
        let name = tool.spec.name.clone();
        let schema = &mut tool.spec.input_schema;
        let Some(defs) = schema.get("$defs").and_then(Value::as_object).cloned() else {
            rewrite(schema, &owner_of, &name);
            continue;
        };
        rewrite(schema, &owner_of, &name);
        let mut body = schema.clone();
        if let Some(object) = body.as_object_mut() {
            object.remove("$defs");
        }
        let mut rewritten_defs = Map::new();
        for (def, mut definition) in defs {
            rewrite(&mut definition, &owner_of, &name);
            rewritten_defs.insert(def, definition);
        }
        let used = referenced_defs(&body, &rewritten_defs);
        let kept: Map<String, Value> = rewritten_defs
            .into_iter()
            .filter(|(def, _)| used.contains(def))
            .collect();
        if let Some(object) = schema.as_object_mut() {
            if kept.is_empty() {
                object.remove("$defs");
            } else {
                object.insert("$defs".into(), Value::Object(kept));
            }
        }
    }
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
            if is_file_operation(&tag) {
                return None;
            }
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

/// A tool's description: what the edit does, from its doc comment (or its name). What every edit
/// tool has in common (drafts only, ids, whole-item updates) is said once, in the system prompt.
fn describe(prefix: &str, tag: &str, doc: Option<&str>) -> String {
    let what = doc.map(|d| d.replace('\n', " ")).unwrap_or_else(|| sentence(tag));
    let stop = if what.ends_with('.') { "" } else { "." };
    format!("{prefix}: {what}{stop}")
}

/// One tool per show [`Edit`] variant.
pub fn show_edit_tools() -> Vec<Tool> {
    let root = serde_json::to_value(schemars::schema_for!(Edit)).unwrap_or(Value::Null);
    variant_tools(
        root,
        show_tool_name,
        |tag, doc| describe("Draft change to the show", tag, doc),
        |tag| ToolKind::ShowEdit { tag },
    )
}

/// One tool per open-sequence [`SequenceEdit`] variant.
pub fn sequence_edit_tools() -> Vec<Tool> {
    let root = serde_json::to_value(schemars::schema_for!(SequenceEdit)).unwrap_or(Value::Null);
    variant_tools(
        root,
        sequence_tool_name,
        |tag, doc| describe("Draft change to the open sequence", tag, doc),
        |tag| ToolKind::SequenceEdit { tag },
    )
}

/// Turns a show edit tool call back into the engine's edit (its input plus the implied tag).
/// Refuses an edit that read back without some of the input's settings, naming them.
pub fn show_edit(tag: &str, input: &Value) -> Result<Edit, String> {
    parse_whole(tag, input)
}

pub fn sequence_edit(tag: &str, input: &Value) -> Result<SequenceEdit, String> {
    parse_whole(tag, input)
}

fn parse_whole<T: serde::de::DeserializeOwned + serde::Serialize>(
    tag: &str,
    input: &Value,
) -> Result<T, String> {
    let tagged = tagged(tag, input)?;
    let edit: T = serde_json::from_value(tagged.clone())
        .map_err(|e| format!("That input doesn't fit this edit: {e}"))?;
    let ignored = ignored_keys(&tagged, &serde_json::to_value(&edit).unwrap_or(Value::Null));
    if ignored.is_empty() {
        Ok(edit)
    } else {
        Err(ignored_message(&ignored))
    }
}

/// Where in `input` there are settings that `parsed` (the same value read into its type and
/// written back) doesn't have: names the type doesn't know, which serde would silently drop.
/// A null, or a `false` flag that isn't written back, isn't counted.
pub fn ignored_keys(input: &Value, parsed: &Value) -> Vec<String> {
    fn walk(input: &Value, parsed: &Value, path: &str, out: &mut Vec<String>) {
        match (input, parsed) {
            (Value::Object(given), Value::Object(kept)) => {
                for (key, value) in given {
                    let here = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    match kept.get(key) {
                        Some(back) => walk(value, back, &here, out),
                        None if matches!(value, Value::Null | Value::Bool(false)) => {}
                        None => out.push(here),
                    }
                }
            }
            (Value::Array(given), Value::Array(kept)) => {
                for (i, (value, back)) in given.iter().zip(kept).enumerate() {
                    walk(value, back, &format!("{path}[{i}]"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(input, parsed, "", &mut out);
    out
}

/// "PixelFlow doesn't know these settings…", with where to look them up.
pub fn ignored_message(keys: &[String]) -> String {
    let mut hints = Vec::new();
    if keys.iter().any(|k| k.contains("shape")) {
        hints.push("shape_settings(type) for a prop shape's");
    }
    if keys
        .iter()
        .any(|k| k.contains("params") || k.contains("settings"))
    {
        hints.push("list_effect_kinds(kind) for an effect's");
    }
    let hint = if hints.is_empty() {
        "Check the tool's input schema".to_string()
    } else {
        format!("Look the real names up: {}", hints.join("; "))
    };
    format!(
        "Nothing was drafted: PixelFlow doesn't know {} ({}), and would have ignored {}. {hint}.",
        if keys.len() == 1 {
            "this setting"
        } else {
            "these settings"
        },
        keys.iter().take(12).cloned().collect::<Vec<_>>().join(", "),
        if keys.len() == 1 { "it" } else { "them" },
    )
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
            "Effect kinds for sequences, with their settings: keys, ranges, defaults, and choices. All of them, or one `kind`.",
            object(json!({ "kind": { "type": "string" } }), &[]),
            Query::EffectKinds,
        ),
        query(
            "shape_settings",
            "One prop shape's settings in full (a JSON Schema with defaults), for a prop's `shape`.",
            object(
                json!({ "type": { "type": "string", "description": "The shape's `type`, e.g. \"tree\"." } }),
                &["type"],
            ),
            Query::ShapeSettings,
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
            "A timing track's marks (start, end, label: a beat number, a lyric line or word), optionally only in a time range.",
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

fn tool(name: &str, description: &str, input_schema: Value, kind: ToolKind) -> Tool {
    Tool {
        spec: ToolSpec {
            name: name.into(),
            description: description.into(),
            input_schema,
        },
        kind,
    }
}

/// Starting a sequence from a song, reading the song, and placing many effects in one call.
fn song_tools() -> Vec<Tool> {
    let ms = || json!({ "type": "integer", "minimum": 0 });
    let ids = || json!({ "type": "array", "items": { "type": "string" } });
    vec![
        tool(
            "ask_for_song",
            "Shows the user a Choose a song button that starts a new, unsaved sequence from a song of theirs, with a row per prop and group. Use it when no sequence is open (or they want a new one), then end your turn: their next message says when it's open.",
            object(json!({}), &[]),
            ToolKind::AskForSong,
        ),
        tool(
            "analyze_song",
            "The open sequence's song: tempo, bar start times; sections (Intro, Verse, Chorus …; the same group letter is the same music; energy 0–1), from the user's Sections track when sectionsFrom is \"user\"; accents to land on (hit, drop, break, build; forMs for breaks and builds), likewise; barEnergy and barBass, a digit per bar (0 quiet to 9 full); confidence; lyrics (lines, vocalsMs) if any.",
            object(json!({}), &[]),
            ToolKind::AnalyzeSong,
        ),
        tool(
            "add_song_timing",
            "Adds the song's timing tracks to the draft (Beats labeled 1–4, numbered Bars, labeled Sections, Onsets, and Accents: hits, drops, breaks, builds), reusing ones already there; answers their ids.",
            object(
                json!({ "tracks": { "type": "array", "items": { "enum": crate::song::TRACK_CHOICES }, "description": "Default: beats, bars, sections, accents." } }),
                &[],
            ),
            ToolKind::AddSongTiming,
        ),
        tool(
            "place_effects",
            "Puts one effect on many rows from fromMs to toMs: one each, or cut at a timing track's marks (`track`: id or name; `match`: only marks with this word, phrase, or regex, a lyrics track's words; `marksEach` marks per effect) and shared out by `spread`: together, alternate (neighbours take turns), sweep (one row after another), build (rows join one by one). Refused where it overlaps effects on that layer, unless replace.",
            object(
                json!({
                    "rowIds": ids(),
                    "fromMs": ms(),
                    "toMs": ms(),
                    "effect": {
                        "type": "object",
                        "description": "kind and settings as list_effect_kinds gives them; colors as \"#rrggbb\".",
                        "properties": {
                            "kind": { "type": "string" },
                            "settings": { "type": "object" },
                            "colors": { "type": "array", "items": { "type": "string" } },
                            "blend": { "enum": pf_sequence::Blend::ALL.map(pf_sequence::Blend::key) },
                            "fadeInMs": ms(),
                            "fadeOutMs": ms(),
                        },
                        "required": ["kind"],
                    },
                    "track": { "type": "string" },
                    "match": { "type": "string" },
                    "marksEach": { "type": "integer", "minimum": 1 },
                    "spread": { "enum": crate::arrange::SPREADS },
                    "layer": { "type": "integer", "minimum": 0, "description": "Default 0, the bottom; one past the top adds a layer." },
                    "replace": { "type": "boolean" },
                }),
                &["rowIds", "fromMs", "toMs", "effect"],
            ),
            ToolKind::PlaceEffects,
        ),
        tool(
            "repeat_effects",
            "Copies every effect starting from fromMs to toMs (on all rows, or rowIds) to each time in startsMs, keeping rows, layers, and lengths. Refused where a copy overlaps effects, unless replace.",
            object(
                json!({
                    "fromMs": ms(),
                    "toMs": ms(),
                    "startsMs": { "type": "array", "items": ms() },
                    "rowIds": ids(),
                    "replace": { "type": "boolean" },
                }),
                &["fromMs", "toMs", "startsMs"],
            ),
            ToolKind::RepeatEffects,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tagged_enum(tags: &[&str]) -> Value {
        let variants: Vec<Value> = tags
            .iter()
            .map(|tag| {
                json!({
                    "type": "object",
                    "properties": { "type": { "type": "string", "const": tag } },
                    "required": ["type"],
                })
            })
            .collect();
        json!({ "oneOf": variants })
    }

    #[test]
    fn only_empty_defaults_are_dropped() {
        let mut tools = vec![Tool {
            spec: ToolSpec {
                name: "t".into(),
                description: "A tool for the test.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "flag": { "type": "boolean", "default": false },
                        "angle": { "type": "number", "default": 0.0 },
                        "count": { "type": "integer", "default": 0 },
                        "list": { "type": "array", "default": [] },
                        "maybe": { "type": ["string", "null"], "default": null },
                        "on": { "type": "boolean", "default": true },
                        "turn": { "type": "number", "default": 360.0 },
                    },
                }),
            },
            kind: ToolKind::ResetDraft,
        }];
        share_large_definitions(&mut tools);
        let props = &tools[0].spec.input_schema["properties"];
        for field in ["flag", "angle", "count", "list", "maybe"] {
            assert!(props[field].get("default").is_none(), "{field}");
        }
        assert_eq!(props["on"]["default"], true);
        assert_eq!(props["turn"]["default"], 360.0);
    }

    #[test]
    fn an_edit_that_works_on_files_never_becomes_a_tool() {
        let tags = ["renameShow", "relinkFile", "useFoundFiles", "relinkSequenceMusic"];
        let tools = variant_tools(
            tagged_enum(&tags),
            show_tool_name,
            |tag, doc| describe("Draft change to the show", tag, doc),
            |tag| ToolKind::ShowEdit { tag },
        );
        let names: Vec<&str> = tools.iter().map(|t| t.spec.name.as_str()).collect();
        assert_eq!(names, ["show_rename_show"]);
        assert!(is_file_operation("relinkFile"));
        assert!(is_file_operation("checkFiles"));
        assert!(!is_file_operation("setBackground"));
    }
}
