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
        ToolKind::PlaceEffects => crate::arrange::place(draft, input).map_or_else(err, ok),
        ToolKind::RepeatEffects => crate::arrange::repeat(draft, input).map_or_else(err, ok),
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
        Query::EffectKinds => {
            let mut catalog = pf_sequence::effect_catalog();
            if let Some(kind) = input["kind"].as_str() {
                catalog.retain(|info| serde_json::to_value(info.kind).is_ok_and(|k| k == kind));
                if catalog.is_empty() {
                    return err(
                        "There's no effect kind by that name. list_effect_kinds without a kind lists them all.",
                    );
                }
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
            Some(doc) => ok(sequence_summary(show, doc)),
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

fn member_name(show: &Show, member: &GroupMember) -> String {
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

fn shape_type(prop: &pf_model::Prop) -> String {
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
    json!({
        "id": prop.id,
        "name": prop.name,
        "shape": shape_type(prop),
        "pixels": prop.node_count(),
        "position": [p.x, p.y, p.z],
        "wiredTo": wired,
        "groups": groups,
        "submodels": prop.regions.iter().map(|r| json!({ "id": r.id, "name": r.name })).collect::<Vec<_>>(),
    })
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
    json!({
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
    })
    .to_string()
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

fn sequence_summary(show: &Show, doc: &Sequence) -> String {
    json!({
        "name": doc.name,
        "durationMs": doc.duration_ms,
        "frameMs": doc.frame_ms,
        "music": doc.audio,
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
    .to_string()
}

fn effects(show: &Show, doc: &Sequence, input: &Value) -> String {
    let row_filter = input["rowId"].as_str();
    let (from, to) = range(input);
    let mut out = Vec::new();
    for row in &doc.rows {
        if row_filter.is_some_and(|id| row.id.to_string() != id) {
            continue;
        }
        for (layer, l) in row.layers.iter().enumerate() {
            for effect in l.effects.iter().filter(|e| e.end_ms > from && e.start_ms < to) {
                out.push(json!({
                    "rowId": row.id,
                    "lights": target_name(show, row.target),
                    "layer": layer,
                    "effect": effect,
                }));
            }
        }
    }
    to_json(&out)
}
