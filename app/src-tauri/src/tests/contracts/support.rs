//! What the contract and parity suites share: committed files that fail a test when they're
//! stale, values made the same on every run and every platform, and an app with every state
//! the commands use (nothing in it reaches the network, a sound device, or the keychain).

use super::super::*;
use pf_ai::http::RetryPolicy;
use pf_ai::testing::FakeTransport;
use std::collections::HashMap;
use tauri::{App, WebviewWindowBuilder};

/// Set to rewrite the committed files instead of failing when they're stale.
pub(crate) const UPDATE: &str = "PIXELFLOW_UPDATE_CONTRACTS";

/// Where the temp folder of a run appears to be, in fixtures and in the window's recorded calls.
pub(crate) const FIXTURE_DIR: &str = "/fixture";

/// The folder both sides read: `app/src/api/contracts`.
pub(crate) fn contracts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("src")
        .join("api")
        .join("contracts")
}

/// Compares `value` with the committed file `relative` (under [`contracts_dir`]), or rewrites
/// the file when [`UPDATE`] is set. `line` is the longest line an array or object is kept on.
pub(crate) fn check_file(relative: &str, value: &Value, line: usize) {
    let path = contracts_dir().join(relative);
    let text = pretty(&sorted(value.clone()), line) + "\n";
    // A checkout may have turned the line ends into CRLF.
    let read = || {
        std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n")
    };
    if std::env::var_os(UPDATE).is_some() && read() != text {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
    }
    let saved = read();
    if saved == text {
        return;
    }
    let line = saved
        .lines()
        .zip(text.lines())
        .position(|(old, new)| old != new)
        .unwrap_or_else(|| saved.lines().count().min(text.lines().count()));
    let show = |text: &str| {
        text.lines()
            .nth(line)
            .unwrap_or("(the file ends)")
            .trim()
            .to_string()
    };
    panic!(
        "app/src/api/contracts/{relative} is out of date (line {}: the file has `{}`, the shell now gives `{}`). \
         Run `{UPDATE}=1 cargo test --locked -- contracts parity` in app/src-tauri, then `npx vitest run src/api/contracts` in app, and commit the result.",
        line + 1,
        show(&saved),
        show(&text),
    );
}

/// The longest line an array or object is kept on, unless a file asks for another.
pub(crate) const LINE: usize = 110;

/// JSON with a member per line, except that an array or object that fits on one line (of at
/// most `line` characters) stays on one: fixtures a person can read, and diffs that show what
/// changed.
pub(crate) fn pretty(value: &Value, line: usize) -> String {
    let mut out = String::new();
    write_value(value, 0, 0, line, &mut out);
    out
}

fn flat(value: &Value) -> String {
    match value {
        Value::Array(items) if !items.is_empty() => {
            format!("[{}]", items.iter().map(flat).collect::<Vec<_>>().join(", "))
        }
        Value::Object(map) if !map.is_empty() => {
            let members: Vec<String> = map
                .iter()
                .map(|(key, v)| format!("{}: {}", Value::from(key.as_str()), flat(v)))
                .collect();
            format!("{{ {} }}", members.join(", "))
        }
        other => other.to_string(),
    }
}

/// Writes `value` at `indent`, after `lead` characters already on its line.
fn write_value(value: &Value, indent: usize, lead: usize, line: usize, out: &mut String) {
    let one_line = flat(value);
    let pad = |n: usize, out: &mut String| out.push_str(&" ".repeat(n));
    match value {
        Value::Array(items) if indent + lead + one_line.len() > line => {
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                pad(indent + 2, out);
                write_value(item, indent + 2, 0, line, out);
                out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
            }
            pad(indent, out);
            out.push(']');
        }
        Value::Object(map) if indent + lead + one_line.len() > line => {
            out.push_str("{\n");
            for (i, (key, item)) in map.iter().enumerate() {
                pad(indent + 2, out);
                let key = format!("{}: ", Value::from(key.as_str()));
                out.push_str(&key);
                write_value(item, indent + 2, key.len(), line, out);
                out.push_str(if i + 1 < map.len() { ",\n" } else { "\n" });
            }
            pad(indent, out);
            out.push('}');
        }
        _ => out.push_str(&one_line),
    }
}

/// Fails when `folder` (under [`contracts_dir`]) doesn't hold exactly the `.json` files the
/// tests write: one left over from a scenario that's gone would still be read by the window's
/// side.
pub(crate) fn check_no_strays(folder: &str, written: &[&str]) {
    let mut found: Vec<String> = std::fs::read_dir(contracts_dir().join(folder))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".json"))
                .collect()
        })
        .unwrap_or_default();
    let mut expected: Vec<String> = written.iter().map(|name| format!("{name}.json")).collect();
    expected.sort();
    // While the files are being rewritten, the ones still to come aren't missing.
    if std::env::var_os(UPDATE).is_some() {
        let coming: Vec<String> = expected
            .iter()
            .filter(|name| !found.contains(name))
            .cloned()
            .collect();
        found.extend(coming);
    }
    found.sort();
    assert_eq!(
        found, expected,
        "app/src/api/contracts/{folder} should hold exactly the files the tests write"
    );
}

/// `value` with every object's keys in order.
fn sorted(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().map(|(k, v)| (k, sorted(v))).collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(entries.into_iter().collect())
        }
        other => other,
    }
}

/// What the clock sets: pinned, since no two runs agree on it.
const BY_THE_CLOCK: [&str; 3] = ["savedAtMs", "takenAtMs", "openedAt"];
const PINNED_TIME: u64 = 1_700_000_000_000;

/// Makes values the same on every run and platform. What it changes, and why:
/// - text given to [`Stable::swap`] (a run's temp folder, a fake device's port) reads as its
///   fixed stand-in, with a path's separators as `/`;
/// - ids made at random read as `00000000-0000-4000-a000-…`, numbered as they first appear
///   (ids the tests chose themselves, `…-0000-4000-8000-…`, are kept);
/// - numbers with a fraction are rounded to six places (the last digits of sines and square
///   roots differ between platforms' maths libraries);
/// - times read from the clock ([`BY_THE_CLOCK`]) are pinned.
#[derive(Default)]
pub(crate) struct Stable {
    swaps: Vec<(String, String)>,
    ids: HashMap<String, usize>,
}

impl Stable {
    /// Paths under `dir` read as under [`FIXTURE_DIR`].
    pub(crate) fn dir(dir: &Path) -> Self {
        let mut stable = Self::default();
        let mut forms = vec![dir.to_path_buf()];
        // (macOS hands out temp folders through a link.)
        forms.extend(dir.canonicalize().ok());
        for form in forms {
            stable.add(&form.display().to_string(), FIXTURE_DIR);
            stable.add(&pf_model::path_to_text(&form), FIXTURE_DIR);
        }
        stable
    }

    /// Also reads `from` as `to`.
    pub(crate) fn swap(mut self, from: &str, to: &str) -> Self {
        self.add(from, to);
        self
    }

    fn add(&mut self, from: &str, to: &str) {
        // (Windows writes a path it has resolved with `\\?\` in front, and without elsewhere.)
        for from in [from, from.trim_start_matches(r"\\?\")] {
            if !self.swaps.iter().any(|(have, _)| have == from) {
                self.swaps.push((from.to_string(), to.to_string()));
            }
        }
        // The longest first, so a folder inside another is found whole.
        self.swaps.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
    }

    pub(crate) fn text(&mut self, text: &str) -> String {
        let mut out = text.to_string();
        let mut path = false;
        for (from, to) in &self.swaps {
            if out.contains(from.as_str()) {
                path |= to == FIXTURE_DIR;
                out = out.replace(from.as_str(), to);
            }
        }
        if path {
            out = out.replace('\\', "/");
        }
        self.ids_in(&out)
    }

    fn ids_in(&mut self, text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        while at < bytes.len() {
            if at + 36 <= bytes.len() && text.is_char_boundary(at + 36) && is_uuid(&bytes[at..at + 36]) {
                let id = &text[at..at + 36];
                if &id[8..24] == "-0000-4000-8000-" {
                    out.push_str(id);
                } else {
                    let next = self.ids.len() + 1;
                    let n = *self.ids.entry(id.to_ascii_lowercase()).or_insert(next);
                    out.push_str(&format!("00000000-0000-4000-a000-{n:012}"));
                }
                at += 36;
            } else {
                let ch = text[at..].chars().next().unwrap();
                out.push(ch);
                at += ch.len_utf8();
            }
        }
        out
    }

    pub(crate) fn value(&mut self, value: Value) -> Value {
        self.walk(sorted(value))
    }

    fn walk(&mut self, value: Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.text(&text)),
            Value::Number(number) => match number.as_f64() {
                Some(x) if number.is_f64() && x.fract() != 0.0 => json!((x * 1e6).round() / 1e6),
                _ => Value::Number(number),
            },
            Value::Array(items) => Value::Array(items.into_iter().map(|v| self.walk(v)).collect()),
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(key, v)| {
                        let v = match v {
                            Value::Number(_) if BY_THE_CLOCK.contains(&key.as_str()) => json!(PINNED_TIME),
                            v => self.walk(v),
                        };
                        (self.text(&key), v)
                    })
                    .collect(),
            ),
            other => other,
        }
    }
}

fn is_uuid(bytes: &[u8]) -> bool {
    bytes.iter().enumerate().all(|(i, b)| match i {
        8 | 13 | 18 | 23 => *b == b'-',
        _ => b.is_ascii_hexdigit(),
    })
}

/// Sets every `key` in `value`, however deep, to `to`: for what a run can't repeat (where a
/// playing sequence has got to, how many packets went out).
pub(crate) fn pin(value: &mut Value, key: &str, to: Value) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(|v| pin(v, key, to.clone())),
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if k == key && !v.is_null() {
                    *v = to.clone();
                } else {
                    pin(v, key, to.clone());
                }
            }
        }
        _ => {}
    }
}

/// One file of values the shell sends, each with the TypeScript type that must accept it.
pub(crate) struct Fixtures {
    file: &'static str,
    stable: Stable,
    entries: Vec<Value>,
}

impl Fixtures {
    /// The fixtures of `replies/<file>.json`.
    pub(crate) fn new(file: &'static str, stable: Stable) -> Self {
        Self {
            file,
            stable,
            entries: Vec::new(),
        }
    }

    fn push(&mut self, mut entry: Value, name: &str, ts: &str, value: &Value) {
        assert!(
            !self.entries.iter().any(|e| e["name"] == name),
            "two fixtures are called {name}"
        );
        entry["name"] = json!(name);
        entry["ts"] = json!(ts);
        entry["value"] = value.clone();
        self.entries.push(entry);
    }

    /// A command's reply, which the TypeScript type `ts` must accept.
    pub(crate) fn reply(&mut self, name: &str, ts: &str, value: &Value) {
        self.push(json!({}), name, ts, value);
    }

    /// The payload of `event`.
    pub(crate) fn event(&mut self, name: &str, event: &str, ts: &str, payload: &Value) {
        self.push(json!({ "event": event }), name, ts, payload);
    }

    /// What `command` was rejected with.
    pub(crate) fn error(&mut self, name: &str, command: &str, ts: &str, error: &Value) {
        self.push(json!({ "rejectedBy": command }), name, ts, error);
    }

    /// Every member of the TypeScript union `union`, as the shell sends them: the window's side
    /// checks the union has no member besides these. `tag` names the field that tells the
    /// variants of a union of objects apart.
    pub(crate) fn all_of(&mut self, union: &str, tag: Option<&str>, values: &Value) {
        let mut entry = json!({ "allOf": union });
        if let Some(tag) = tag {
            entry["tag"] = json!(tag);
        }
        self.push(entry, &format!("every {union}"), &format!("({union})[]"), values);
    }

    /// [`Fixtures::all_of`] for a union of objects without a tag, whose variants are told
    /// apart by the keys they have.
    pub(crate) fn all_of_by_keys(&mut self, union: &str, values: &Value) {
        let entry = json!({ "allOf": union, "byKeys": true });
        self.push(entry, &format!("every {union}"), &format!("({union})[]"), values);
    }

    /// Also reads `from` as `to` (see [`Stable::swap`]): for a name made from the clock, or a
    /// fake device's port.
    pub(crate) fn swap(&mut self, from: &str, to: &str) {
        self.stable = std::mem::take(&mut self.stable).swap(from, to);
    }

    /// Checks the committed file (see [`check_file`]).
    pub(crate) fn finish(mut self) {
        let entries = self.stable.value(Value::Array(std::mem::take(&mut self.entries)));
        check_file(&format!("replies/{}.json", self.file), &entries, LINE);
    }
}

/// The app with the assistant's and Find lyrics' states too: a credential store in memory and
/// recorded provider replies.
pub(crate) struct Shell {
    pub app: App<MockRuntime>,
    pub webview: WebviewWindow<MockRuntime>,
    pub dir: tempfile::TempDir,
    pub anthropic: Arc<FakeTransport>,
    pub lrclib: Arc<FakeTransport>,
    pub whisper: Arc<FakeTransport>,
}

impl Shell {
    pub(crate) fn new() -> Self {
        Self::with_keys(pf_ai::MemoryStore::new())
    }

    pub(crate) fn with_keys(store: pf_ai::MemoryStore) -> Self {
        Self::in_dir(tempfile::tempdir().unwrap(), store)
    }

    /// The app as it starts with what an earlier run left in `dir`.
    pub(crate) fn in_dir(dir: tempfile::TempDir, store: pf_ai::MemoryStore) -> Self {
        let app = app_without_window(dir.path());
        let anthropic = Arc::new(FakeTransport::default());
        let openai = Arc::new(FakeTransport::default());
        let lrclib = Arc::new(FakeTransport::default());
        let whisper = Arc::new(FakeTransport::default());
        let providers = pf_ai::Providers {
            anthropic: Arc::new(
                pf_ai::anthropic::Anthropic::new(anthropic.clone()).with_retry(RetryPolicy::immediate()),
            ),
            openai: Arc::new(pf_ai::openai::OpenAi::new(openai).with_retry(RetryPolicy::immediate())),
        };
        app.manage(assistant::AiState::new(
            pf_ai::KeyVault::new(Box::new(store)),
            providers,
        ));
        app.manage(
            lyrics::LyricsState::new(
                pf_ai::lyrics::Services {
                    lrclib: pf_ai::lyrics::lrclib::Lrclib::new(lrclib.clone())
                        .with_retry_delay(Duration::ZERO),
                    transcriber: pf_ai::lyrics::transcribe::Transcriber::new(whisper.clone())
                        .with_retry(RetryPolicy::immediate()),
                    voice: Box::new(|_, _, _| Ok(pf_analysis::VocalTrack::default())),
                    tags: Box::new(|path| pf_audio::read_tags(path).ok()),
                    log: Box::new(|_| {}),
                },
                None,
            )
            .with_models(Some(dir.path().join("data"))),
        );
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        Self {
            app,
            webview,
            dir,
            anthropic,
            lrclib,
            whisper,
        }
    }

    pub(crate) fn call(&self, cmd: &str, args: Value) -> Result<Value, Value> {
        call(&self.webview, cmd, args)
    }

    /// The reply to a call that must succeed.
    pub(crate) fn ok(&self, cmd: &str, args: Value) -> Value {
        self.call(cmd, args)
            .unwrap_or_else(|error| panic!("{cmd} was rejected: {error}"))
    }

    /// What a call that must be refused is rejected with.
    pub(crate) fn refused(&self, cmd: &str, args: Value) -> Value {
        match self.call(cmd, args) {
            Ok(reply) => panic!("{cmd} should have been rejected, but answered {reply}"),
            Err(error) => error,
        }
    }

    /// Quits as the app does (keeping unsaved work), leaving its folder for the next run.
    pub(crate) fn quit(self) -> tempfile::TempDir {
        shut_down(&self.app.state::<AppState>());
        self.dir
    }

    /// Paths under this app's folder read as under [`FIXTURE_DIR`].
    pub(crate) fn stable(&self) -> Stable {
        Stable::dir(self.dir.path())
    }
}

/// The shell's own source files (not its tests), by path below `src`.
pub(crate) fn shell_sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, below: &str, out: &mut Vec<(String, String)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().filter_map(Result::ok).collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if path.is_dir() {
                if !(below.is_empty() && name == "tests") {
                    walk(&path, &format!("{below}{name}/"), out);
                }
            } else if name.ends_with(".rs") {
                out.push((format!("{below}{name}"), std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    let mut sources = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        "",
        &mut sources,
    );
    sources
}

/// The source without its test module (everything from `#[cfg(test)] mod … {` on; a module
/// only named there, `mod testing;`, is in a file of its own).
pub(crate) fn without_tests(source: &str) -> &str {
    const MARK: &str = "#[cfg(test)]";
    source
        .match_indices(MARK)
        .find(|(at, _)| {
            let after = source[at + MARK.len()..].trim_start();
            let inline = match (after.find('{'), after.find(';')) {
                (Some(body), Some(end)) => body < end,
                (body, _) => body.is_some(),
            };
            after.starts_with("mod ") && inline
        })
        .map_or(source, |(at, _)| &source[..at])
}

/// Every event name the shell's sources declare (`const …_EVENT: &str = "…"`), in order.
pub(crate) fn declared_events() -> Vec<String> {
    let events: std::collections::BTreeSet<String> = shell_sources()
        .iter()
        .flat_map(|(_, source)| {
            without_tests(source)
                .lines()
                .filter_map(|line| line.trim().split_once(": &str = \""))
                .filter(|(name, _)| name.ends_with("_EVENT"))
                .map(|(_, value)| value.trim_end_matches("\";").to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(events.len() > 5, "{events:?}");
    events.into_iter().collect()
}

/// `value` with every path under [`FIXTURE_DIR`] moved into `dir`: what a scripted call names
/// then lands in the test's own folder.
pub(crate) fn in_dir(value: &Value, dir: &Path) -> Value {
    match value {
        Value::String(text) if text.starts_with(FIXTURE_DIR) => {
            let below = text[FIXTURE_DIR.len()..].trim_start_matches('/');
            let path = below
                .split('/')
                .fold(dir.to_path_buf(), |path, part| path.join(part));
            json!(pf_model::path_to_text(&path))
        }
        Value::Array(items) => Value::Array(items.iter().map(|v| in_dir(v, dir)).collect()),
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), in_dir(v, dir))).collect()),
        other => other.clone(),
    }
}

// Pieces of a show as the window's own edits carry them, with ids the tests choose.

pub(crate) fn prop_id(n: u32) -> String {
    format!("11111111-0000-4000-8000-{n:012}")
}

pub(crate) fn prop(n: u32, name: &str, shape: Value) -> Value {
    let mut shape = shape;
    shape["source"] = json!("generator");
    json!({
        "id": prop_id(n),
        "name": name,
        "shape": shape,
        "transform": { "position": { "x": n, "y": 0, "z": 0 }, "rotationDeg": { "x": 0, "y": 0, "z": 0 },
                       "scale": { "x": 1, "y": 1, "z": 1 } },
        "colorOrder": "RGB",
        "regions": [],
        "tags": [],
    })
}

/// A straight strip of `nodes` pixels.
pub(crate) fn strip(n: u32, name: &str, nodes: u32) -> Value {
    prop(n, name, json!({ "type": "line", "nodes": nodes, "length": 3 }))
}

pub(crate) fn controller(n: u32, name: &str, address: &str, protocol: Value, ports: Value) -> Value {
    json!({
        "id": format!("33333333-0000-4000-8000-{n:012}"),
        "name": name,
        "address": address,
        "adapter": "generic",
        "protocol": protocol,
        "ports": ports,
        "sequenceChannels": null,
    })
}

pub(crate) fn slot(prop: u32) -> Value {
    json!({ "prop": prop_id(prop), "segment": null, "nullPixels": 0, "reverse": false, "brightness": null,
            "gamma": null, "smartReceiver": null })
}

pub(crate) fn port(number: u32, slots: Value) -> Value {
    json!({ "number": number, "maxPixels": null, "brightness": 100, "gamma": 1, "slots": slots })
}

/// A controller on this computer (nothing leaves it), wired to props `wired` on port 1.
pub(crate) fn bench(n: u32, wired: &[u32]) -> Value {
    let slots: Vec<Value> = wired.iter().map(|p| slot(*p)).collect();
    controller(
        n,
        "Bench",
        "127.0.0.1:9",
        json!({ "type": "ddp" }),
        json!([port(1, json!(slots))]),
    )
}

/// A value as the window receives it: through JSON text.
pub(crate) fn wire<T: serde::Serialize>(value: &T) -> Value {
    serde_json::from_str(&serde_json::to_string(value).unwrap()).unwrap()
}

/// Everything `event` carries while `work` runs.
pub(crate) fn heard<T>(app: &App<MockRuntime>, event: &str, work: impl FnOnce() -> T) -> (T, Vec<Value>) {
    use tauri::Listener;
    let payloads: Arc<Mutex<Vec<Value>>> = Arc::default();
    let seen = Arc::clone(&payloads);
    let listener = app.listen_any(event, move |event| {
        seen.lock()
            .unwrap()
            .push(serde_json::from_str(event.payload()).unwrap());
    });
    let done = work();
    app.unlisten(listener);
    let payloads = payloads.lock().unwrap().clone();
    (done, payloads)
}

#[test]
fn values_are_made_the_same_on_every_run_and_platform() {
    let dir = Path::new(if cfg!(windows) {
        r"C:\Temp\.tmpAb12"
    } else {
        "/tmp/.tmpAb12"
    });
    let inside = dir.join("Show").join("house.png");
    let mut stable = Stable::dir(dir).swap("127.0.0.1:49152", "192.0.2.50");
    let value = stable.value(json!({
        "path": inside.display().to_string(),
        "message": format!("Could not read {}", inside.display()),
        "address": "127.0.0.1:49152",
        "chosen": "11111111-0000-4000-8000-000000000001",
        "made": ["7c9e6679-7425-40de-944b-e07fc1f90ae7", "0b0e8f65-17c2-4f5a-9d0c-2a59c1b3a111", "7C9E6679-7425-40DE-944B-E07FC1F90AE7"],
        "note": "kept as 7c9e6679-7425-40de-944b-e07fc1f90ae7.json",
        "third": 1.0 / 3.0,
        "whole": 2.0,
        "count": 7,
        "savedAtMs": 1_760_000_000_123u64,
        "nested": [{ "openedAt": 41 }],
    }));
    assert_eq!(
        value,
        json!({
            "path": "/fixture/Show/house.png",
            "message": "Could not read /fixture/Show/house.png",
            "address": "192.0.2.50",
            "chosen": "11111111-0000-4000-8000-000000000001",
            "made": ["00000000-0000-4000-a000-000000000001", "00000000-0000-4000-a000-000000000002", "00000000-0000-4000-a000-000000000001"],
            "note": "kept as 00000000-0000-4000-a000-000000000001.json",
            "third": 0.333333,
            "whole": 2.0,
            "count": 7,
            "savedAtMs": PINNED_TIME,
            "nested": [{ "openedAt": PINNED_TIME }],
        })
    );
    let mut status = json!({ "nowMs": 12, "controllers": [{ "packetsSent": 3, "lastError": null }] });
    pin(&mut status, "nowMs", json!(0));
    pin(&mut status, "packetsSent", json!(0));
    pin(&mut status, "lastError", json!("x"));
    assert_eq!(
        status,
        json!({ "nowMs": 0, "controllers": [{ "packetsSent": 0, "lastError": null }] })
    );
}
