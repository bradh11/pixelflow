//! What the window sends the shell. `commandCalls.json` is every call the window's API objects
//! make, recorded by `commandCalls.test.ts` with arguments of the window's own types; here each
//! is held against the command it names:
//!
//! - **by name**: every key sent is a parameter of the command, and every parameter that isn't
//!   an `Option` is sent. Tauri ignores a key it doesn't know and reads a missing optional one as
//!   `None`, so a renamed optional argument would otherwise go unnoticed. Tauri has no
//!   reflection: the parameters are read from the shell's own source files.
//! - **by running it**: the command is called through the mock runtime with the recorded
//!   arguments, and must get past reading them (it may fail later: no such file, nothing open).
//! - **inside each argument**: a field the window sends in an object must be one the Rust type
//!   reads (serde drops unknown fields without a word).

use super::support::{FIXTURE_DIR, Shell, contracts_dir, declared_events, shell_sources, without_tests};
use super::*;
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, BTreeSet};
use tauri::ipc::{CallbackFn, InvokeBody};

#[derive(Debug, Clone, PartialEq)]
struct Param {
    /// As the window names it (camelCase).
    key: String,
    ty: String,
}

impl Param {
    fn optional(&self) -> bool {
        self.ty.starts_with("Option<")
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Command {
    name: String,
    params: Vec<Param>,
    /// Takes the request itself (raw bytes and headers) instead of named arguments.
    raw: bool,
}

/// What Tauri hands a command itself: never sent by the window.
const INJECTED: [&str; 7] = [
    "State",
    "AppHandle",
    "Window",
    "WebviewWindow",
    "Webview",
    "Request",
    "Channel",
];

fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for ch in name.chars() {
        if ch == '_' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// The end of the bracket that opens at `open` (`(`…`)` or `<`…`>`), minding nesting and `->`.
fn closing(text: &str, open: usize) -> usize {
    let bytes = text.as_bytes();
    let (start, end) = if bytes[open] == b'(' {
        (b'(', b')')
    } else {
        (b'<', b'>')
    };
    let mut depth = 0;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        if b == start {
            depth += 1;
        } else if b == end && !(end == b'>' && bytes[i - 1] == b'-') {
            depth -= 1;
            if depth == 0 {
                return i;
            }
        }
    }
    panic!(
        "an unclosed bracket in {}",
        &text[open..(open + 60).min(text.len())]
    );
}

/// `text` cut at the commas that aren't inside brackets.
fn top_level(text: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut depth = 0;
    let mut previous = ' ';
    for ch in text.chars() {
        match ch {
            '(' | '[' | '<' => depth += 1,
            ')' | ']' => depth -= 1,
            '>' if previous != '-' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(String::new());
                previous = ch;
                continue;
            }
            _ => {}
        }
        parts.last_mut().unwrap().push(ch);
        previous = ch;
    }
    parts
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Every `#[tauri::command]` function in `source`, with its parameters as written.
fn parse_commands(file: &str, source: &str) -> Vec<Command> {
    let mut commands = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find("#[tauri::command") {
        rest = &rest[at..];
        let attribute = &rest[..=rest.find(']').unwrap()];
        assert_eq!(
            attribute, "#[tauri::command]",
            "{file}: this checker reads plain #[tauri::command] only: teach it what {attribute} changes"
        );
        let start = rest.find("fn ").unwrap() + 3;
        let name_end = start + rest[start..].find(['<', '(']).unwrap();
        let name = rest[start..name_end].trim().to_string();
        let open = if rest.as_bytes()[name_end] == b'<' {
            closing(rest, name_end) + 1
        } else {
            name_end
        };
        assert_eq!(rest.as_bytes()[open], b'(', "{file}: {name}'s parameters");
        let close = closing(rest, open);
        let mut command = Command {
            name,
            params: Vec::new(),
            raw: false,
        };
        for param in top_level(&rest[open + 1..close]) {
            let (name, ty) = param.split_once(':').unwrap_or_else(|| panic!("{file}: {param}"));
            let ty = ty.split_whitespace().collect::<Vec<_>>().join(" ");
            let outer = ty
                .split('<')
                .next()
                .unwrap()
                .rsplit("::")
                .next()
                .unwrap()
                .to_string();
            if INJECTED.contains(&outer.as_str()) {
                command.raw |= outer == "Request";
                continue;
            }
            command.params.push(Param {
                key: camel(name.trim().trim_start_matches("mut ").trim()),
                ty,
            });
        }
        commands.push(command);
        rest = &rest[close..];
    }
    commands
}

/// The commands the shell's sources define, by name.
fn defined() -> BTreeMap<String, Command> {
    let mut commands = BTreeMap::new();
    for (file, source) in shell_sources() {
        for command in parse_commands(&file, without_tests(&source)) {
            let name = command.name.clone();
            assert!(
                commands.insert(name.clone(), command).is_none(),
                "two commands are called {name}"
            );
        }
    }
    commands
}

/// The commands the window may call: the names in the shell's `generate_handler!` list.
fn registered() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for (_, source) in shell_sources() {
        let source = without_tests(&source);
        let Some(at) = source.find("generate_handler![") else {
            continue;
        };
        let list = &source[at + "generate_handler![".len()..];
        let list = &list[..list.find(']').unwrap()];
        names.extend(
            top_level(list)
                .iter()
                .map(|path| path.rsplit("::").next().unwrap().to_string()),
        );
    }
    assert!(
        !names.is_empty(),
        "the shell's generate_handler! list wasn't found"
    );
    names
}

/// One call the window makes, as `commandCalls.test.ts` recorded it.
#[derive(Debug, Clone, serde::Deserialize)]
struct Call {
    /// The API method that made it ("backend.applyEdits").
    api: String,
    command: String,
    /// The named arguments (none for a call that sends raw bytes).
    #[serde(default)]
    args: serde_json::Map<String, Value>,
    /// The headers of a call that sends raw bytes.
    #[serde(default)]
    headers: Option<BTreeMap<String, String>>,
}

/// An event an API method listens for.
#[derive(Debug, serde::Deserialize)]
struct Listen {
    api: String,
    event: String,
}

#[derive(Debug, serde::Deserialize)]
struct Recording {
    calls: Vec<Call>,
    listens: Vec<Listen>,
}

fn recording() -> Recording {
    let path = contracts_dir().join("commandCalls.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

/// Registered commands the window's API objects never call, and why.
const NEVER_CALLED: [(&str, &str); 0] = [];

/// What's wrong with `call`'s argument names, if anything.
fn name_problems(
    call: &Call,
    commands: &BTreeMap<String, Command>,
    registered: &BTreeSet<String>,
) -> Vec<String> {
    let Some(command) = commands.get(&call.command) else {
        return vec![format!(
            "{} calls `{}`, which the shell doesn't define",
            call.api, call.command
        )];
    };
    let mut problems = Vec::new();
    if !registered.contains(&call.command) {
        problems.push(format!(
            "{} calls `{}`, which isn't in the shell's list of commands",
            call.api, call.command
        ));
    }
    if command.raw != call.headers.is_some() {
        problems.push(format!(
            "{} and `{}` disagree on whether the call carries raw bytes",
            call.api, call.command
        ));
    }
    let takes: Vec<&str> = command.params.iter().map(|p| p.key.as_str()).collect();
    for key in call.args.keys() {
        if !takes.contains(&key.as_str()) {
            problems.push(format!(
                "{} sends `{key}`, which `{}` doesn't take (it takes: {})",
                call.api,
                call.command,
                if takes.is_empty() {
                    "nothing".to_string()
                } else {
                    takes.join(", ")
                }
            ));
        }
    }
    for param in command.params.iter().filter(|p| !p.optional()) {
        if !call.args.contains_key(&param.key) {
            problems.push(format!(
                "{} doesn't send `{}`, which `{}` needs",
                call.api, param.key, call.command
            ));
        }
    }
    problems
}

#[test]
fn every_call_names_a_command_and_its_parameters() {
    let (commands, registered, recording) = (defined(), registered(), recording());
    let mut problems = Vec::new();
    for name in &registered {
        if !commands.contains_key(name) {
            problems.push(format!(
                "`{name}` is in the shell's list of commands, but its #[tauri::command] wasn't found"
            ));
        }
    }
    for name in commands.keys() {
        if !registered.contains(name) {
            problems.push(format!(
                "`{name}` is a #[tauri::command] the shell's list of commands leaves out"
            ));
        }
    }
    let called: BTreeSet<&str> = recording.calls.iter().map(|c| c.command.as_str()).collect();
    for name in &registered {
        let excused = NEVER_CALLED.iter().any(|(never, _)| never == name);
        match (called.contains(name.as_str()), excused) {
            (false, false) => problems.push(format!(
                "no API method calls `{name}`: call it in commandCalls.test.ts, or list it in NEVER_CALLED with the reason"
            )),
            (true, true) => problems.push(format!("`{name}` is called, so it doesn't belong in NEVER_CALLED")),
            _ => {}
        }
    }
    for call in &recording.calls {
        problems.extend(name_problems(call, &commands, &registered));
    }
    // An optional parameter no call ever sends is one the window may have under another name.
    for command in commands.values().filter(|c| called.contains(c.name.as_str())) {
        for param in &command.params {
            let sent = recording
                .calls
                .iter()
                .any(|call| call.command == command.name && call.args.contains_key(&param.key));
            if !sent {
                problems.push(format!(
                    "no recorded call sends `{}` to `{}`: record one that does",
                    param.key, command.name
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Commands that can't be run even against fakes, and why. Their names are still checked.
const NOT_RUN: [(&str, &str); 4] = [
    ("open_device_page", "opens the system's browser"),
    ("download_alignment_models", "downloads from the internet"),
    (
        "video_export_choices",
        "runs the ffmpeg installed on this computer",
    ),
    (
        "cancel_video_export",
        "bumps a counter every test in the process shares, so it would stop another test's export",
    ),
];

/// The address the window's recorded calls give a controller (the recorded FPP's).
const RECORDED_ADDRESS: &str = "192.0.2.10";

/// `value` with the recorded folder moved into `dir` and the recorded controller's address
/// replaced by `device`'s: nothing a call does can then land outside the test's own folder, or
/// reach past this computer.
fn at_home(value: &Value, dir: &Path, device: &str) -> Value {
    match value {
        Value::String(text) if text.starts_with(FIXTURE_DIR) => {
            let below = text[FIXTURE_DIR.len()..].trim_start_matches('/');
            json!(pf_model::path_to_text(
                &below
                    .split('/')
                    .fold(dir.to_path_buf(), |path, part| path.join(part))
            ))
        }
        Value::String(text) if text == RECORDED_ADDRESS => json!(device),
        Value::Array(items) => Value::Array(items.iter().map(|v| at_home(v, dir, device)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), at_home(v, dir, device)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Whether a rejection is Tauri's own, from before the command ran: it couldn't read an
/// argument, the command isn't there, or a state it needs isn't.
fn before_the_command(error: &Value) -> bool {
    error.as_str().is_some_and(|text| {
        text.starts_with("invalid args ")
            || text.contains(" missing required key ")
            || (text.starts_with("Command ") && text.ends_with(" not found"))
            || text.starts_with("state not managed ")
    })
}

/// Runs `call` in a new app (nothing open, nothing saved) and says what Tauri made of it.
fn run(call: &Call, device: &str) -> Result<(), Value> {
    let shell = Shell::new();
    // A dialog asked for while one is showing is answered as cancelled: none is ever shown.
    let state = shell.app.state::<AppState>();
    let _showing = state.dialog.take().unwrap();
    let args = at_home(&Value::Object(call.args.clone()), shell.dir.path(), device);
    let (body, headers) = match &call.headers {
        Some(sent) => {
            let mut headers = tauri::http::HeaderMap::new();
            for (name, value) in sent {
                headers.insert(
                    tauri::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                    value.parse().unwrap(),
                );
            }
            (InvokeBody::Raw(vec![0; 16]), headers)
        }
        None => (InvokeBody::Json(args), Default::default()),
    };
    get_ipc_response(
        &shell.webview,
        InvokeRequest {
            cmd: call.command.clone(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: shell.webview.url().unwrap(),
            body,
            headers,
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|_| ())
}

#[test]
fn every_call_is_one_its_command_can_read() {
    let recording = recording();
    let fpp = pf_devices::testing::FakeFpp::start();
    let calls: Vec<&Call> = recording
        .calls
        .iter()
        .filter(|call| !NOT_RUN.iter().any(|(name, _)| *name == call.command))
        .collect();
    // Each call gets an app of its own, so a few threads share them out.
    let problems: Vec<String> = std::thread::scope(|scope| {
        let workers: Vec<_> = calls
            .chunks(calls.len().div_ceil(4))
            .map(|share| {
                let address = fpp.address();
                scope.spawn(move || {
                    share
                        .iter()
                        .filter_map(|call| match run(call, address) {
                            Err(error) if before_the_command(&error) => {
                                Some(format!("{} → `{}`: {error}", call.api, call.command))
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().unwrap())
            .collect()
    });
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    let commands = defined();
    for (name, why) in NOT_RUN {
        assert!(
            commands.contains_key(name),
            "`{name}` ({why}) isn't a command any more"
        );
    }
}

/// The paths of every member of `value` that isn't nothing (`null`, `false`, `0`, `""`, `[]`,
/// `{}`: a field left at such a value reads the same sent or not).
fn members(value: &Value, path: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    let nothing = |v: &Value| match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
    };
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                path.push(key.clone());
                if !nothing(v) {
                    out.push(path.clone());
                }
                members(v, path, out);
                path.pop();
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                path.push(i.to_string());
                members(v, path, out);
                path.pop();
            }
        }
        _ => {}
    }
}

/// `value` without the object member at `path`.
fn without(value: &Value, path: &[String]) -> Value {
    let mut value = value.clone();
    let mut at = &mut value;
    for step in &path[..path.len() - 1] {
        at = match at {
            Value::Array(items) => &mut items[step.parse::<usize>().unwrap()],
            other => &mut other[step.as_str()],
        };
    }
    at.as_object_mut().unwrap().remove(path.last().unwrap());
    value
}

/// The fields `sent` has that reading it as a `T` ignores: taking one out neither stops it
/// being read nor changes what was read.
fn unread<T: DeserializeOwned + std::fmt::Debug>(sent: &Value) -> Vec<String> {
    let Ok(whole) = serde_json::from_value::<T>(sent.clone()) else {
        // (Running the command reports a value that can't be read at all.)
        return Vec::new();
    };
    let whole = format!("{whole:?}");
    let mut paths = Vec::new();
    members(sent, &mut Vec::new(), &mut paths);
    paths
        .into_iter()
        .filter(|path| {
            serde_json::from_value::<T>(without(sent, path)).is_ok_and(|less| format!("{less:?}") == whole)
        })
        .map(|path| path.join("."))
        .collect()
}

/// The fields of an argument of type `ty` (as its command declares it) that the type doesn't
/// read; `None` for a type this checker hasn't been told about.
fn unread_in(ty: &str, sent: &Value) -> Option<Vec<String>> {
    use pf_engine::{FileRole, PatternSpec, SequenceEdit, TargetSpec};
    use pf_model::PropId;
    Some(match ty {
        "Vec<Edit>" => unread::<Vec<Edit>>(sent),
        "Vec<SequenceEdit>" => unread::<Vec<SequenceEdit>>(sent),
        "PatternSpec" => unread::<PatternSpec>(sent),
        "TargetSpec" => unread::<TargetSpec>(sent),
        "FileRole" | "Option<FileRole>" => unread::<Option<FileRole>>(sent),
        "Option<Vec<Row>>" => unread::<Option<Vec<pf_sequence::Row>>>(sent),
        "Option<BTreeMap<String, PropId>>" => unread::<Option<BTreeMap<String, PropId>>>(sent),
        "Vec<Sample>" => unread::<Vec<pf_camera_map::Sample>>(sent),
        "Decoded" => unread::<pf_camera_map::Decoded>(sent),
        "SendSource" => unread::<fpp_send::SendSource>(sent),
        "SendRequest" => unread::<fpp_send::SendRequest>(sent),
        "DownloadRequest" => unread::<fpp_download::DownloadRequest>(sent),
        "VideoRequest" => unread::<video::VideoRequest>(sent),
        "Mapping" | "Option<Mapping>" => unread::<Option<pf_xlights::vendor::Mapping>>(sent),
        "Option<UiContext>" => unread::<Option<pf_ai::UiContext>>(sent),
        "Choice" => unread::<pf_ai::lyrics::Choice>(sent),
        // Read whole, whatever is in it.
        "Option<Value>" => Vec::new(),
        // One value each: there's nothing inside to go unread.
        "String"
        | "bool"
        | "u32"
        | "u64"
        | "usize"
        | "i64"
        | "f32"
        | "PathArg"
        | "ApiKey"
        | "Base"
        | "FppFolder"
        | "PickKind"
        | "ProviderId"
        | "SequenceId"
        | "TimingTrackId"
        | "pf_sequence::TimingTrackId"
        | "Option<String>"
        | "Option<bool>"
        | "Option<PathArg>"
        | "Option<ProviderId>"
        | "Vec<String>"
        | "Vec<u32>" => Vec::new(),
        _ => return None,
    })
}

#[test]
fn every_field_the_window_sends_is_one_the_shell_reads() {
    let (commands, recording) = (defined(), recording());
    let mut problems = Vec::new();
    for call in &recording.calls {
        let Some(command) = commands.get(&call.command) else {
            continue;
        };
        for param in &command.params {
            let Some(sent) = call.args.get(&param.key) else {
                continue;
            };
            match unread_in(&param.ty, sent) {
                Some(unread) => problems.extend(unread.into_iter().map(|field| {
                    format!(
                        "{} sends `{}.{field}` to `{}`, but {} doesn't read it (or it's sent at the value \
                         the shell assumes when it's left out: record another)",
                        call.api, param.key, call.command, param.ty
                    )
                })),
                None => problems.push(format!(
                    "`{}` takes a {}: say in unread_in() how to read one",
                    call.command, param.ty
                )),
            }
        }
    }
    problems.sort();
    problems.dedup();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The call that carries raw bytes (video frames) describes them in a header, which the command
/// reads as JSON: the header's name and fields are the ones it reads.
#[test]
fn the_header_of_a_call_with_raw_bytes_is_one_the_shell_reads() {
    let recording = recording();
    let raw: Vec<&Call> = recording
        .calls
        .iter()
        .filter(|call| call.headers.is_some())
        .collect();
    assert!(!raw.is_empty(), "no recorded call carries raw bytes");
    for call in raw {
        assert_eq!(
            call.command, "camera_map_decode",
            "say here how `{}` reads its header",
            call.command
        );
        let headers = call.headers.as_ref().unwrap();
        assert_eq!(
            headers.keys().collect::<Vec<_>>(),
            [camera_map::FRAMES_HEADER],
            "{}",
            call.api
        );
        let sent: Value = serde_json::from_str(&headers[camera_map::FRAMES_HEADER]).unwrap();
        if let Err(error) = serde_json::from_value::<camera_map::FramesInfo>(sent.clone()) {
            panic!(
                "{} sends a header the shell can't read ({error}): {sent}",
                call.api
            );
        }
        assert_eq!(
            unread::<camera_map::FramesInfo>(&sent),
            Vec::<String>::new(),
            "{}",
            call.api
        );
    }
}

/// Every edit, every sequence edit, and each variant of the other unions the window sends is
/// in the recording: the matches below have no wildcard, so a variant added to the engine has
/// to be sent by `commandCalls.test.ts` before this passes.
#[test]
fn every_variant_the_shell_takes_is_sent() {
    use pf_engine::{FileRole, SequenceEdit, TargetSpec};
    let recording = recording();
    let sent = |command: &str, key: &str| -> Vec<Value> {
        recording
            .calls
            .iter()
            .filter(|call| call.command == command)
            .filter_map(|call| call.args.get(key).cloned())
            .collect()
    };
    let read = |values: Vec<Value>| -> Vec<Value> {
        values
            .into_iter()
            .flat_map(|v| match v {
                Value::Array(items) => items,
                other => vec![other],
            })
            .collect()
    };
    fn typed<T: DeserializeOwned>(values: Vec<Value>) -> Vec<T> {
        values
            .into_iter()
            .map(|v| serde_json::from_value(v.clone()).unwrap_or_else(|e| panic!("{v}: {e}")))
            .collect()
    }
    fn covered(what: &str, count: usize, indexes: impl IntoIterator<Item = usize>) {
        let seen: BTreeSet<usize> = indexes.into_iter().collect();
        let missing: Vec<usize> = (0..count).filter(|i| !seen.contains(i)).collect();
        assert!(
            missing.is_empty(),
            "commandCalls.test.ts sends no {what} for variant(s) {missing:?} (numbered as in the match in this test)"
        );
    }

    let edits: Vec<Edit> = typed(read(sent("apply_edits", "edits")));
    covered(
        "show edit",
        17,
        edits.iter().map(|edit| match edit {
            Edit::RenameShow { .. } => 0,
            Edit::SetFrameRate { .. } => 1,
            Edit::AddProp { .. } => 2,
            Edit::UpdateProp { .. } => 3,
            Edit::RemoveProp { .. } => 4,
            Edit::AddGroup { .. } => 5,
            Edit::UpdateGroup { .. } => 6,
            Edit::RemoveGroup { .. } => 7,
            Edit::AddController { .. } => 8,
            Edit::UpdateController { .. } => 9,
            Edit::RemoveController { .. } => 10,
            Edit::AddSequence { .. } => 11,
            Edit::UpdateSequence { .. } => 12,
            Edit::RemoveSequence { .. } => 13,
            Edit::MoveSequence { .. } => 14,
            Edit::SetBackground { .. } => 15,
            Edit::SetHouseModel { .. } => 16,
        }),
    );
    let edits: Vec<SequenceEdit> = typed(read(sent("edit_sequence", "edits")));
    covered(
        "sequence edit",
        27,
        edits.iter().map(|edit| match edit {
            SequenceEdit::UpdateInfo { .. } => 0,
            SequenceEdit::AddRow { .. } => 1,
            SequenceEdit::RemoveRow { .. } => 2,
            SequenceEdit::MoveRow { .. } => 3,
            SequenceEdit::AddLayer { .. } => 4,
            SequenceEdit::RemoveLayer { .. } => 5,
            SequenceEdit::AddEffect { .. } => 6,
            SequenceEdit::UpdateEffect { .. } => 7,
            SequenceEdit::SetEffectTiming { .. } => 8,
            SequenceEdit::SetEffectParams { .. } => 9,
            SequenceEdit::MoveEffect { .. } => 10,
            SequenceEdit::RemoveEffect { .. } => 11,
            SequenceEdit::AddTimingTrack { .. } => 12,
            SequenceEdit::UpdateTimingTrack { .. } => 13,
            SequenceEdit::RemoveTimingTrack { .. } => 14,
            SequenceEdit::RenameTimingTrack { .. } => 15,
            SequenceEdit::MoveTimingTrack { .. } => 16,
            SequenceEdit::AddMarks { .. } => 17,
            SequenceEdit::SetMark { .. } => 18,
            SequenceEdit::RemoveMarks { .. } => 19,
            SequenceEdit::SplitMark { .. } => 20,
            SequenceEdit::MergeMarks { .. } => 21,
            SequenceEdit::GenerateMarks { .. } => 22,
            SequenceEdit::CopyMarks { .. } => 23,
            SequenceEdit::SpreadLyrics { .. } => 24,
            SequenceEdit::LabelMarks { .. } => 25,
            SequenceEdit::BreakIntoWords { .. } => 26,
        }),
    );
    let mut targets = sent("start_output", "target");
    targets.extend(sent("camera_map_target", "target"));
    let targets: Vec<TargetSpec> = typed(targets);
    covered(
        "test pattern target",
        5,
        targets.iter().map(|target| match target {
            TargetSpec::Show => 0,
            TargetSpec::Prop { .. } => 1,
            TargetSpec::Group { .. } => 2,
            TargetSpec::Controller { .. } => 3,
            TargetSpec::Port { .. } => 4,
        }),
    );
    let mut roles = sent("locate_file", "file");
    roles.extend(sent("find_missing_files", "file"));
    let roles: Vec<FileRole> = typed(roles);
    covered(
        "show file",
        4,
        roles.iter().filter_map(|role| match role {
            FileRole::Sequence { .. } => Some(0),
            FileRole::Music { .. } => Some(1),
            FileRole::Photo => Some(2),
            FileRole::HouseModel => Some(3),
            // The open sequence's own music and pictures have commands of their own.
            FileRole::SequenceDocMusic | FileRole::Picture => None,
        }),
    );
    let sources: Vec<fpp_send::SendSource> = typed(sent("fpp_send_plan", "source"));
    covered(
        "thing to send to an FPP",
        2,
        sources.iter().map(|source| match source {
            fpp_send::SendSource::OpenSequence { .. } => 0,
            fpp_send::SendSource::File { .. } => 1,
        }),
    );
    let requests: Vec<fpp_send::SendRequest> = typed(sent("fpp_send", "request"));
    covered(
        "playlist choice",
        3,
        requests.iter().map(|request| match request.playlist {
            pf_devices::fpp_upload::PlaylistChoice::None => 0,
            pf_devices::fpp_upload::PlaylistChoice::Existing(_) => 1,
            pf_devices::fpp_upload::PlaylistChoice::New(_) => 2,
        }),
    );
    let choices: Vec<pf_ai::lyrics::Choice> = typed(sent("choose_lyrics", "choice"));
    covered(
        "lyrics choice",
        2,
        choices.iter().map(|choice| match choice {
            pf_ai::lyrics::Choice::Candidate(_) => 0,
            pf_ai::lyrics::Choice::Pasted(_) => 1,
        }),
    );
}

/// The events the window listens for while its calls are out are ones the shell declares.
#[test]
fn every_event_the_window_listens_for_is_one_the_shell_sends() {
    let declared = declared_events();
    let listens = recording().listens;
    for Listen { api, event } in &listens {
        assert!(
            declared.contains(event),
            "{api} listens for `{event}`, which the shell never sends"
        );
    }
    for event in &declared {
        assert!(
            listens.iter().any(|l| l.event == *event),
            "the shell sends `{event}`, which no API method listens for"
        );
    }
}

#[test]
fn the_checks_catch_a_bad_call() {
    let source = r#"
        /// Plays.
        #[tauri::command]
        pub(crate) async fn play_it<R: Runtime>(
            app: AppHandle<R>,
            state: State<'_, AppState>,
            path: PathArg,
            position_ms: u64,
            mut music: Option<PathArg>,
            use_props: Option<BTreeMap<String, PropId>>,
        ) -> Reply<Option<PlaybackStatus>> {
            todo!()
        }

        #[tauri::command]
        pub(crate) fn decode(request: Request<'_>) -> Reply<()> {}
    "#;
    let commands = parse_commands("made up", source);
    let param = |key: &str, ty: &str| Param {
        key: key.into(),
        ty: ty.into(),
    };
    assert_eq!(
        commands,
        vec![
            Command {
                name: "play_it".into(),
                params: vec![
                    param("path", "PathArg"),
                    param("positionMs", "u64"),
                    param("music", "Option<PathArg>"),
                    param("useProps", "Option<BTreeMap<String, PropId>>"),
                ],
                raw: false,
            },
            Command {
                name: "decode".into(),
                params: vec![],
                raw: true,
            },
        ]
    );
    let commands: BTreeMap<String, Command> = commands.into_iter().map(|c| (c.name.clone(), c)).collect();
    let registered: BTreeSet<String> = ["play_it".to_string()].into();
    let call = |command: &str, args: Value| Call {
        api: "made.up".into(),
        command: command.into(),
        args: args.as_object().unwrap().clone(),
        headers: None,
    };
    let good = call("play_it", json!({ "path": "/a", "positionMs": 0 }));
    assert_eq!(name_problems(&good, &commands, &registered), Vec::<String>::new());
    // A renamed optional argument: Tauri would read `music` as none and say nothing.
    let renamed = call("play_it", json!({ "path": "/a", "positionMs": 0, "audio": "/b" }));
    assert_eq!(
        name_problems(&renamed, &commands, &registered),
        ["made.up sends `audio`, which `play_it` doesn't take (it takes: path, positionMs, music, useProps)"]
    );
    let missing = call("play_it", json!({ "path": "/a", "position_ms": 0 }));
    assert_eq!(name_problems(&missing, &commands, &registered).len(), 2);
    assert_eq!(
        name_problems(&call("nope", json!({})), &commands, &registered),
        ["made.up calls `nope`, which the shell doesn't define"]
    );
    assert!(name_problems(&call("decode", json!({})), &commands, &registered).len() == 2);

    // Run for real: a wrong type, a missing argument, and a command that isn't there are
    // Tauri's to refuse; anything the command itself says is not.
    let fpp = pf_devices::testing::FakeFpp::start();
    let ran = |command: &str, args: Value| run(&call(command, args), fpp.address());
    for (command, args) in [
        ("new_show", json!({ "name": 7 })),
        ("new_show", json!({})),
        (
            "apply_edits",
            json!({ "edits": [{ "type": "renameTheShow", "name": "x" }] }),
        ),
        ("no_such_command", json!({})),
    ] {
        let error = ran(command, args.clone()).unwrap_err();
        assert!(before_the_command(&error), "{command} {args}: {error}");
    }
    let error = ran("save_show", json!({})).unwrap_err();
    assert!(!before_the_command(&error), "{error}");
    assert!(
        ran("new_show", json!({ "name": "House", "extra": true })).is_ok(),
        "Tauri ignores a key it doesn't know"
    );

    // Inside an argument: a field serde would drop without a word.
    let request = json!({ "width": 1280, "height": 720, "fps": 30, "startMs": 0, "endMs": 500, "photo": true,
        "pixelSize": 1.5, "glow": 0.4, "ffmpeg": true });
    assert_eq!(unread::<video::VideoRequest>(&request), Vec::<String>::new());
    let mut renamed = request.clone();
    let glow = renamed.as_object_mut().unwrap().remove("glow").unwrap();
    renamed["glowAmount"] = glow;
    assert_eq!(unread::<video::VideoRequest>(&renamed), ["glowAmount"]);
    let edits = json!([{ "type": "addRow", "row": { "id": "44444444-0000-4000-8000-000000000001",
        "target": { "prop": "11111111-0000-4000-8000-000000000001" }, "layers": [{ "effects": [], "muted": true }] }, "at": 2 }]);
    assert_eq!(
        unread::<Vec<pf_engine::SequenceEdit>>(&edits),
        ["0.at", "0.row.layers.0.muted"]
    );
    assert_eq!(unread_in("SomethingNew", &json!({})), None);
    assert_eq!(
        at_home(
            &json!({ "path": "/fixture/a/b.json", "address": RECORDED_ADDRESS, "n": 1 }),
            Path::new("tmp"),
            "here"
        )["address"],
        "here"
    );
}
