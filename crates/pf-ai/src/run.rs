//! Running one tool call against the draft. Queries read the draft; edit tools change it (each
//! checked exactly as the engine checks edits); nothing here reaches the engine, a file, the
//! network, or a device.

use crate::diff::{effect_name, target_name};
use crate::draft::Draft;
use crate::provider::ToolCall;
use crate::song::Song;
use crate::tools::{Query, ToolKind, Toolbox, sequence_edit, show_edit};
use pf_model::{GroupMember, Show};
use pf_sequence::{Sequence, format_ms};
use serde::Serialize;
use serde_json::{Value, json};

/// The longest tool answer sent back to the model; longer ones are cut, saying so.
pub const MAX_RESULT_CHARS: usize = 40_000;

/// What running a tool came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// An answer for the model.
    Answer { content: String, is_error: bool },
    /// The model asked to show its draft to the user.
    Propose { summary: String },
    /// The model asked the user to choose a song for a new sequence.
    AskForSong,
    /// The model asked for these groups of tools (each one of [`crate::tools::GROUPS`]).
    UseTools { groups: Vec<String> },
}

fn ok(content: impl Into<String>) -> Outcome {
    Outcome::Answer {
        content: cap(content.into()),
        is_error: false,
    }
}

fn err(content: impl Into<String>) -> Outcome {
    Outcome::Answer {
        content: cap(content.into()),
        is_error: true,
    }
}

fn cap(mut text: String) -> String {
    if text.len() > MAX_RESULT_CHARS {
        let mut end = MAX_RESULT_CHARS;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n…(cut short: ask for less, e.g. with a filter or a time range)");
    }
    text
}

fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

/// Runs one call. Unknown tools (anything outside the toolbox, like saving, output, or devices)
/// are refused with an explanation, and nothing happens.
pub fn run_tool(toolbox: &Toolbox, call: &ToolCall, draft: &mut Draft, song: &mut Song<'_>) -> Outcome {
    let Some(tool) = toolbox.find(&call.name) else {
        return err(format!(
            "There is no tool called \"{}\". You can only read the show and draft changes for the user to review: you can't save or export files, send to controllers, start output or playback, or contact devices. If the user wants one of those, tell them where to do it in PixelFlow.",
            call.name.chars().take(64).collect::<String>()
        ));
    };
    if let Some(problem) = &call.input_error {
        return err(problem.clone());
    }
    let input = &call.input;
    match &tool.kind {
        ToolKind::ShowEdit { tag } => match show_edit(tag, input) {
            Ok(edit) => match draft.edit_show(edit) {
                Ok(()) => ok("Done (in your draft)."),
                Err(e) => err(e.to_string()),
            },
            Err(e) => err(e),
        },
        ToolKind::SequenceEdit { tag } => match sequence_edit(tag, input) {
            Ok(edit) => match draft.edit_sequence(edit) {
                Ok(()) => ok("Done (in your draft)."),
                Err(e) => err(e.to_string()),
            },
            Err(e) => err(e),
        },
        ToolKind::Guide => crate::knowledge::answer(input).map_or_else(err, ok),
        ToolKind::AnalyzeSong => match song.analysis(draft) {
            Ok(analysis) => {
                let user = draft.base().sequence.as_ref().map(|s| &s.doc);
                ok(crate::song::describe(&analysis, user).to_string())
            }
            Err(e) => err(e),
        },
        ToolKind::AddSongTiming => {
            let wanted: Vec<String> = match input["tracks"].as_array() {
                Some(names) => names
                    .iter()
                    .filter_map(|n| n.as_str().map(str::to_string))
                    .collect(),
                None => crate::song::DEFAULT_TRACKS.map(String::from).to_vec(),
            };
            // Syllables and phonemes come from the words: the analysis only if it's there.
            let analysis = if crate::song::needs_analysis(&wanted) {
                song.analysis(draft).map(Some)
            } else {
                Ok(song.analyzed())
            };
            match analysis.and_then(|analysis| crate::song::add_timing(analysis.as_deref(), draft, &wanted)) {
                Ok(tracks) => ok(tracks.to_string()),
                Err(e) => err(e),
            }
        }
        ToolKind::PlaceEffects => with_song_tracks(input, draft, song, |draft| {
            batched(input, |item| crate::arrange::place(draft, item))
        }),
        ToolKind::RepeatEffects => {
            batched(input, |item| crate::arrange::repeat(draft, item)).map_or_else(err, ok)
        }
        ToolKind::ChangeEffects => with_song_tracks(input, draft, song, |draft| {
            batched(input, |item| crate::rearrange::change(draft, item))
        }),
        ToolKind::StageCue => {
            // The song gives the tempo, moments, and beats; without one, cues go by ms at 120 BPM.
            let analysis = match song.music {
                Some(_) => match song.analysis(draft) {
                    Ok(analysis) => Some(analysis),
                    Err(e) => return err(e),
                },
                None => None,
            };
            crate::cues::stage(draft, analysis.as_deref(), input).map_or_else(err, ok)
        }
        ToolKind::AskForSong => Outcome::AskForSong,
        ToolKind::UseTools => {
            let keys: Vec<&str> = crate::tools::GROUPS.iter().map(|(g, _)| *g).collect();
            let asked = match crate::arrange::text_list(&input["groups"], "groups") {
                Ok(asked) if !asked.is_empty() => asked,
                _ => return err(format!("Name the groups you need: {}.", keys.join(", "))),
            };
            match asked.iter().find(|g| !keys.contains(&g.as_str())) {
                Some(unknown) => err(format!(
                    "There's no group \"{}\". Groups: {}.",
                    unknown.chars().take(40).collect::<String>(),
                    keys.join(", ")
                )),
                None => Outcome::UseTools { groups: asked },
            }
        }
        ToolKind::ReviewDraft => review_draft(draft, song),
        ToolKind::ResetDraft => {
            draft.reset();
            ok("Your draft is back to the user's show and sequence.")
        }
        ToolKind::Propose => {
            let summary = input["summary"].as_str().unwrap_or_default().trim().to_string();
            if summary.is_empty() {
                return err("Give a short summary of the changes.");
            }
            if draft.diff().is_empty() {
                return err(
                    "Your draft has no changes to propose. Make the changes first with the show_ and sequence_ tools.",
                );
            }
            Outcome::Propose { summary }
        }
        ToolKind::Query(query) => run_query(*query, input, draft),
    }
}

/// Items one batched call (`place_effects`, `change_effects`, `repeat_effects`) may hold.
pub const MAX_ITEMS: usize = 60;

/// How many `items` a call has, when it has a list of them.
pub fn items_in(input: &Value) -> Option<usize> {
    input.get("items").and_then(Value::as_array).map(Vec::len)
}

/// Runs `one` on each of `input`'s `items`, each taking the call's other fields as its
/// defaults; or once on `input` itself, when it has no items. Each item is done, or refused
/// with its reason, on its own: the answer says which, item by item, so only a refused one
/// need be sent again. Refused outright only when every item is.
fn batched(input: &Value, mut one: impl FnMut(&Value) -> Result<String, String>) -> Result<String, String> {
    let Some(items) = input.get("items") else {
        return one(input);
    };
    let items = match items {
        Value::Array(items) if !items.is_empty() && items.len() <= MAX_ITEMS => items,
        _ => {
            return Err(format!(
                "items must be a list of 1 to {MAX_ITEMS} objects, each taking this tool's other fields."
            ));
        }
    };
    let mut defaults = input.as_object().cloned().unwrap_or_default();
    defaults.remove("items");
    let mut lines = Vec::new();
    let mut done = 0;
    for (i, item) in items.iter().enumerate() {
        let answer = match item {
            Value::Object(fields) => {
                let mut merged = defaults.clone();
                merged.extend(fields.clone());
                one(&Value::Object(merged))
            }
            _ => Err("each item must be an object of this tool's fields.".to_string()),
        };
        match answer {
            Ok(answer) => {
                done += 1;
                lines.push(format!("{}. {answer}", i + 1));
            }
            Err(problem) => lines.push(format!(
                "{}. Refused, nothing done for this one: {problem}",
                i + 1
            )),
        }
    }
    let answer = format!("{done} of {} items done.\n{}", items.len(), lines.join("\n"));
    if done == 0 { Err(answer) } else { Ok(answer) }
}

/// The song's own tracks, by the names `add_song_timing` gives them ("Beats", "Sections", ...):
/// the ones made from the analysis, not from the words.
fn song_track(name: &str) -> Option<&'static str> {
    crate::song::TRACK_CHOICES
        .into_iter()
        .filter(|choice| !crate::song::needs_words(choice))
        .find(|choice| choice.eq_ignore_ascii_case(name.trim()))
}

/// Every timing track `input` names to cut at or follow (`track` and `timingTrack`, anywhere).
fn tracks_named(input: &Value, out: &mut Vec<String>) {
    match input {
        Value::Object(map) => {
            for (key, value) in map {
                match value {
                    Value::String(name) if key == "track" || key == "timingTrack" => out.push(name.clone()),
                    other => tracks_named(other, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| tracks_named(item, out)),
        _ => {}
    }
}

/// Runs `run` after adding to the draft any of the song's tracks that `input` names but the
/// sequence lacks (placing on "Sections" with no Sections track yet adds it first, as
/// `add_song_timing` would), saying so in the answer. Without a song nothing is added, and the
/// tool says what's missing as it always does.
fn with_song_tracks(
    input: &Value,
    draft: &mut Draft,
    song: &mut Song<'_>,
    run: impl FnOnce(&mut Draft) -> Result<String, String>,
) -> Outcome {
    let mut named = Vec::new();
    tracks_named(input, &mut named);
    let missing: Vec<String> = match draft.sequence() {
        Some(doc) => {
            let mut missing: Vec<String> = Vec::new();
            for name in named {
                if let Some(choice) = song_track(&name)
                    && crate::song::find_track(&doc.timing_tracks, &name).is_none()
                    && !missing.iter().any(|m| m == choice)
                {
                    missing.push(choice.to_string());
                }
            }
            missing
        }
        None => Vec::new(),
    };
    let mut said = String::new();
    if !missing.is_empty()
        && song.music.is_some()
        && let Ok(analysis) = song.analysis(draft)
        && let Ok(Value::Array(tracks)) = crate::song::add_timing(Some(&analysis), draft, &missing)
    {
        let names: Vec<String> = tracks
            .iter()
            .filter(|t| t["added"] == true)
            .filter_map(|t| t["name"].as_str().map(|n| format!("\"{n}\"")))
            .collect();
        if !names.is_empty() {
            said = format!(
                "Added the song's {} {} to your draft. ",
                names.join(", "),
                if names.len() == 1 { "track" } else { "tracks" }
            );
        }
    }
    match run(draft) {
        Ok(answer) => ok(format!("{said}{answer}")),
        Err(problem) => err(format!("{said}{problem}")),
    }
}

/// `review_draft`: the review of the draft's sequence against the song (see [`crate::review`]),
/// then what the draft changes.
fn review_draft(draft: &mut Draft, song: &mut Song<'_>) -> Outcome {
    let changes = draft.diff().describe();
    if draft.sequence().is_none() {
        return ok(changes);
    }
    // Without a song (or one that can't be read), the parts that need none are reviewed.
    let analysis = song.music.and_then(|_| song.analysis(draft).ok());
    match draft.review(analysis.as_deref(), song.cancel) {
        Ok(Some(review)) => ok(format!("Review: {}\n\nChanges: {changes}", review.to_model())),
        Ok(None) => ok(changes),
        Err(e) => err(e),
    }
}

fn run_query(query: Query, input: &Value, draft: &Draft) -> Outcome {
    let show = draft.show();
    match query {
        Query::Overview => ok(overview(draft)),
        Query::ListProps => {
            let filter = input["nameContains"].as_str().map(str::to_lowercase);
            let props: Vec<Value> = show
                .props
                .iter()
                .filter(|p| filter.as_ref().is_none_or(|f| p.name.to_lowercase().contains(f)))
                .map(|p| prop_summary(show, p))
                .collect();
            ok(to_json(&props))
        }
        Query::GetProp => by_id(input, "prop", |id| {
            show.props.iter().find(|p| p.id.to_string() == id).map(to_json)
        }),
        Query::ListGroups => {
            let groups: Vec<Value> = show
                .groups
                .iter()
                .map(|g| {
                    json!({
                        "id": g.id,
                        "name": g.name,
                        "members": g.members.iter().map(|m| member_name(show, m)).collect::<Vec<_>>(),
                    })
                })
                .collect();
            ok(to_json(&groups))
        }
        Query::GetGroup => by_id(input, "group", |id| {
            show.groups.iter().find(|g| g.id.to_string() == id).map(to_json)
        }),
        Query::ListControllers => {
            let controllers: Vec<Value> = show
                .controllers
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "name": c.name,
                        "address": c.address,
                        "protocol": c.protocol,
                        "ports": c.ports.iter().map(|port| json!({
                            "number": port.number,
                            "props": port.slots.iter().map(|s| prop_name(show, s.prop)).collect::<Vec<_>>(),
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect();
            ok(to_json(&controllers))
        }
        Query::GetController => by_id(input, "controller", |id| {
            show.controllers
                .iter()
                .find(|c| c.id.to_string() == id)
                .map(to_json)
        }),
        Query::ListPlaylist => ok(to_json(&show.sequences)),
        Query::Selection => ok(selection(draft)),
        Query::EffectKinds if input["kind"].as_str() == Some("shader") => {
            let shaders = &draft.base().shaders;
            match input["file"].as_str() {
                Some(file) => match shaders.iter().find(|s| s.file == file) {
                    Some(shader) => ok(to_json(shader)),
                    None => err("There's no shader by that file. list_effect_kinds with kind \"shader\" and no file lists them."),
                },
                // Briefly: how to use one, what each looks like, and what its inputs are called.
                None => ok(json!({
                    "guide": SHADER_GUIDE,
                    "kind": pf_sequence::effect_catalog().into_iter().find(|i| i.kind == pf_sequence::EffectKind::Shader),
                    "shaders": shaders
                        .iter()
                        .map(|s| match &s.problem {
                            Some(problem) => json!({ "file": s.file, "name": s.name, "cantBeDrawn": problem }),
                            None => json!({
                                "file": s.file,
                                "name": s.name,
                                "looks": s.description,
                                "inputs": s.inputs.iter().map(|i| i.name()).collect::<Vec<_>>(),
                            }),
                        })
                        .collect::<Vec<_>>(),
                })
                .to_string()),
            }
        }
        Query::EffectKinds => {
            let mut catalog = pf_sequence::effect_catalog();
            let Some(kind) = input["kind"].as_str() else {
                // Every kind in full is more than one answer holds: each one's settings by key.
                let kinds: Vec<Value> = catalog
                    .iter()
                    .map(|info| {
                        json!({
                            "kind": info.kind,
                            "label": info.label,
                            "description": info.description,
                            "settings": info.settings.iter().map(|s| s.key).collect::<Vec<_>>(),
                        })
                    })
                    .collect();
                return ok(to_json(&kinds));
            };
            catalog.retain(|info| serde_json::to_value(info.kind).is_ok_and(|k| k == kind));
            if catalog.is_empty() {
                return err(format!(
                    "There's no effect kind \"{}\". The kinds: {}.",
                    kind.chars().take(40).collect::<String>(),
                    crate::knowledge::reference::kind_keys().join(", ")
                ));
            }
            ok(to_json(&catalog))
        }
        Query::ShapeSettings => {
            let shape = input["type"].as_str().unwrap_or_default();
            match crate::tools::shape_settings(shape) {
                Some(schema) => ok(schema.to_string()),
                None => err(format!(
                    "There's no prop shape by that name. Shapes: {}.",
                    crate::tools::shape_types().join(", ")
                )),
            }
        }
        Query::OpenSequence => match draft.sequence() {
            Some(doc) => ok(with_pictures(sequence_summary(show, doc), draft).to_string()),
            None => ok("No sequence is open in the editor."),
        },
        Query::SequenceEffects => match draft.sequence() {
            Some(doc) => ok(effects(show, doc, input)),
            None => err("No sequence is open in the editor."),
        },
        Query::TimingMarks => match draft.sequence() {
            Some(doc) => {
                let id = input["trackId"].as_str().unwrap_or_default();
                let Some(track) = doc.timing_tracks.iter().find(|t| t.id.to_string() == id) else {
                    return err("There's no timing track with that id. get_open_sequence lists them.");
                };
                let (from, to) = range(input);
                let marks: Vec<_> = track
                    .marks
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| m.end_ms > from && m.start_ms < to)
                    .map(|(i, m)| json!({ "index": i, "startMs": m.start_ms, "endMs": m.end_ms, "label": m.label }))
                    .collect();
                ok(to_json(&marks))
            }
            None => err("No sequence is open in the editor."),
        },
    }
}

fn by_id(input: &Value, what: &str, find: impl Fn(&str) -> Option<String>) -> Outcome {
    let id = input["id"].as_str().unwrap_or_default();
    find(id).map(ok).unwrap_or_else(|| {
        err(format!(
            "There's no {what} with that id. The list_ tools give ids."
        ))
    })
}

fn range(input: &Value) -> (u64, u64) {
    (
        input["fromMs"].as_u64().unwrap_or(0),
        input["toMs"].as_u64().unwrap_or(u64::MAX),
    )
}

fn prop_name(show: &Show, id: pf_model::PropId) -> String {
    show.props
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "(missing prop)".into())
}

pub(crate) fn member_name(show: &Show, member: &GroupMember) -> String {
    match member {
        GroupMember::Prop(id) => prop_name(show, *id),
        GroupMember::Region(r) => target_name(
            show,
            pf_sequence::Target::Region {
                prop: r.prop,
                region: r.region,
            },
        ),
    }
}

pub(crate) fn shape_type(prop: &pf_model::Prop) -> String {
    let value = serde_json::to_value(&prop.shape).unwrap_or(Value::Null);
    value["type"]
        .as_str()
        .or_else(|| value["source"].as_str())
        .unwrap_or("custom")
        .to_string()
}

fn prop_summary(show: &Show, prop: &pf_model::Prop) -> Value {
    let wired: Vec<String> = show
        .controllers
        .iter()
        .flat_map(|c| {
            c.ports
                .iter()
                .filter(|port| port.slots.iter().any(|s| s.prop == prop.id))
                .map(move |port| format!("{} port {}", c.name, port.number))
        })
        .collect();
    let groups: Vec<&str> = show
        .groups
        .iter()
        .filter(|g| g.members.iter().any(|m| m.prop() == prop.id))
        .map(|g| g.name.as_str())
        .collect();
    let p = prop.transform.position;
    let mut summary = json!({
        "id": prop.id,
        "name": prop.name,
        "shape": shape_type(prop),
        "role": role_name(prop),
        "pixels": prop.node_count(),
        "position": [p.x, p.y, p.z],
        "wiredTo": wired,
        "groups": groups,
        "submodels": prop.regions.iter().map(|r| json!({ "id": r.id, "name": r.name })).collect::<Vec<_>>(),
    });
    if let Some((columns, rows)) = grid(prop) {
        summary["grid"] = json!(format!("{columns}x{rows}"));
    }
    let faces: Vec<&str> = prop
        .regions
        .iter()
        .filter(|r| matches!(r.kind, pf_model::RegionKind::Face(_)))
        .map(|r| r.name.as_str())
        .collect();
    if !faces.is_empty() {
        summary["faces"] = json!(faces);
    }
    summary
}

/// The part PixelFlow gives a prop in a show ("outline", "tree", "matrix", ...).
pub(crate) fn role_name(prop: &pf_model::Prop) -> String {
    format!("{:?}", crate::cues::role_of(prop)).to_lowercase()
}

/// The grid effects draw on for the shapes that are one: columns across and rows up (a tree's
/// strings and the pixels up each). `None` for a shape that is a single run or an outline.
pub(crate) fn grid(prop: &pf_model::Prop) -> Option<(u32, u32)> {
    use pf_model::{Generator, ShapeSource};
    let ShapeSource::Generator(shape) = &prop.shape else {
        return None;
    };
    match shape {
        Generator::Matrix { columns, rows, .. } | Generator::Sphere { columns, rows, .. } => {
            Some((*columns, *rows))
        }
        Generator::Tree {
            strings,
            nodes_per_string,
            ..
        } => Some((*strings, *nodes_per_string)),
        Generator::CandyCanes {
            canes,
            nodes_per_cane,
            ..
        } => Some((*canes, *nodes_per_cane)),
        _ => None,
    }
}

/// The most pictures listed (the rest are counted).
const MAX_PICTURES: usize = 30;

/// Said with the list of shaders: where the craft of using them is (the Shader effect's guide).
const SHADER_GUIDE: &str = "Set a shader effect's settings.file to one of these, and its settings.inputs by the names given (with a file, list_effect_kinds gives each input's type, range, default, and choices). get_guide with effects [\"shader\"] says which props each suits, how inputs and curves on them are written, and ready-made looks.";

/// Adds the pictures in the show's images folder to a summary, as a Picture effect's `file`
/// names them: nothing when there are none.
fn with_pictures(mut summary: Value, draft: &Draft) -> Value {
    let images = &draft.base().images;
    if !images.is_empty() {
        summary["pictures"] = json!(images.iter().take(MAX_PICTURES).collect::<Vec<_>>());
        if images.len() > MAX_PICTURES {
            summary["morePictures"] = json!(images.len() - MAX_PICTURES);
        }
    }
    summary
}

fn overview(draft: &Draft) -> String {
    let show = draft.show();
    let report = pf_model::validate_show(show);
    let problems: Vec<&str> = report
        .issues
        .iter()
        .take(20)
        .map(|i| i.message.as_str())
        .collect();
    let summary = json!({
        "name": show.name,
        "frameRate": show.settings.frame_rate,
        "props": show.props.len(),
        "pixels": show.props.iter().map(|p| u64::from(p.node_count())).sum::<u64>(),
        "groups": show.groups.len(),
        "controllers": show.controllers.len(),
        "playlist": show.sequences.len(),
        "hasBackgroundPhoto": show.background.is_some(),
        "problems": problems,
        "openSequence": draft.sequence().map(|s| json!({ "name": s.name, "length": format_ms(s.duration_ms) })),
        "draftChanges": draft.diff().changes.len(),
    });
    with_pictures(summary, draft).to_string()
}

fn selection(draft: &Draft) -> String {
    let show = draft.show();
    let context = &draft.base().context;
    let props: Vec<Value> = context
        .selected_props
        .iter()
        .map(|id| json!({ "id": id, "name": prop_name(show, *id) }))
        .collect();
    let effects: Vec<Value> = match draft.sequence() {
        Some(doc) => context
            .selected_effects
            .iter()
            .filter_map(|id| {
                doc.rows.iter().find_map(|row| {
                    row.layers
                        .iter()
                        .flat_map(|l| &l.effects)
                        .find(|e| e.id == *id)
                        .map(|e| json!({ "id": e.id, "rowId": row.id, "what": effect_name(e, row, show) }))
                })
            })
            .collect(),
        None => Vec::new(),
    };
    json!({
        "screen": context.screen,
        "props": props,
        "effects": effects,
        "playheadMs": context.playhead_ms,
    })
    .to_string()
}

fn sequence_summary(show: &Show, doc: &Sequence) -> Value {
    json!({
        "name": doc.name,
        "durationMs": doc.duration_ms,
        "frameMs": doc.frame_ms,
        "music": doc.audio,
        "musicLevelSource": doc.music_level_source,
        "rows": doc.rows.iter().map(|row| json!({
            "id": row.id,
            "lights": target_name(show, row.target),
            "target": row.target,
            "layers": row.layers.iter().map(|l| l.effects.len()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "timingTracks": doc.timing_tracks.iter().map(|t| json!({
            "id": t.id, "name": t.name, "kind": t.kind, "marks": t.marks.len(),
        })).collect::<Vec<_>>(),
    })
}

fn effects(show: &Show, doc: &Sequence, input: &Value) -> String {
    let row_filter = input["rowId"].as_str();
    let (from, to) = range(input);
    let mut out = Vec::new();
    // Row by row: what it lights, each effect briefly, and each layer's count and kinds.
    let mut brief = Vec::new();
    let mut outline = Vec::new();
    let mut count = 0;
    for row in &doc.rows {
        if row_filter.is_some_and(|id| row.id.to_string() != id) {
            continue;
        }
        let mut lines = Vec::new();
        let mut layers = Vec::new();
        for (layer, l) in row.layers.iter().enumerate() {
            let playing: Vec<&pf_sequence::Effect> = l
                .effects
                .iter()
                .filter(|e| e.end_ms > from && e.start_ms < to)
                .collect();
            for effect in &playing {
                out.push(json!({
                    "rowId": row.id,
                    "lights": target_name(show, row.target),
                    "layer": layer,
                    "effect": effect,
                }));
                lines.push(json!([
                    effect.id,
                    layer,
                    effect.kind(),
                    effect.start_ms,
                    effect.end_ms
                ]));
            }
            if let (Some(first), Some(last)) = (playing.first(), playing.last()) {
                let mut kinds: Vec<pf_sequence::EffectKind> = Vec::new();
                for effect in &playing {
                    if !kinds.contains(&effect.kind()) {
                        kinds.push(effect.kind());
                    }
                }
                layers.push(json!({
                    "layer": layer,
                    "effects": playing.len(),
                    "kinds": kinds,
                    "fromMs": first.start_ms,
                    "toMs": last.end_ms,
                }));
            }
        }
        if lines.is_empty() {
            continue;
        }
        count += lines.len();
        let lights = target_name(show, row.target);
        brief.push(json!({ "rowId": row.id, "lights": lights, "effects": lines }));
        outline.push(json!({ "rowId": row.id, "lights": lights, "layers": layers }));
    }
    let full = to_json(&out);
    let asked = input["brief"].as_bool().unwrap_or(false);
    if !asked && full.len() <= MAX_RESULT_CHARS {
        return full;
    }
    // Too many to give in full (or asked for briefly): each effect in a line, row by row; and
    // when even that is too long, each row's layers in a line.
    let narrow = "ask for one row or a shorter time to see settings";
    let briefly = json!({
        "rows": brief,
        "each": "[id, layer, kind, startMs, endMs]",
        "note": if asked {
            format!("{count} effects.")
        } else {
            format!("{count} effects: too many to give in full ({narrow}).")
        },
    })
    .to_string();
    if briefly.len() <= MAX_RESULT_CHARS {
        return briefly;
    }
    json!({
        "rows": outline,
        "note": format!("{count} effects: too many to list ({narrow}, or brief: true for each one's id, kind, and times)."),
    })
    .to_string()
}
