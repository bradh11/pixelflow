//! A fake WLED that answers real HTTP on `127.0.0.1` (feature `test-fixtures`), for testing
//! "Send setup" end to end without a device. It answers `GET /json/info`, `GET /json/cfg`, and
//! `POST /json/cfg`, which, like WLED's `deserializeConfig()` (`wled00/cfg.cpp`), takes what the
//! body has and keeps the rest. Like WLED it adds a body's color order overrides (`hw.com`) to
//! the ones it has rather than replacing them (`cfg.cpp` calls `add()` without `reset()`), and
//! drops an LED output that runs past its LED limit (`start + len > MAX_LEDS`). It can fail,
//! refuse, or ignore config writes.

use crate::fake_fpp::{read_body, read_head, respond};
use serde_json::{Value, json};
use std::io::BufReader;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

/// Everything the fake WLED knows; tests read and change it through [`FakeWled::state`].
#[derive(Debug)]
pub struct FakeWledState {
    /// What `/json/info` answers.
    pub info: Value,
    /// The saved configuration (`/json/cfg`).
    pub cfg: Value,
    /// Every request, as `METHOD /path`.
    pub requests: Vec<String>,
    /// Every body POSTed to `/json/cfg`, in order.
    pub config_writes: Vec<Value>,
    /// Config writes from this one on (counting from 1) answer HTTP 500 and change nothing.
    pub fail_writes_from: Option<usize>,
    /// Config writes answer `{"success":true}` but change nothing.
    pub ignore_writes: bool,
    /// Config writes answer this HTTP status and change nothing (401: settings locked by PIN).
    pub refuse_writes: Option<u16>,
    /// The most LEDs it drives (WLED's `MAX_LEDS`): an output ending past it is dropped.
    pub max_leds: i64,
}

/// A fake WLED listening on `127.0.0.1` until dropped.
pub struct FakeWled {
    address: String,
    state: Arc<Mutex<FakeWledState>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// Merges `from` into `into` the way WLED reads a config body: objects key by key, anything else
/// (including arrays such as the LED outputs) replaced whole, except color order overrides
/// (`hw.com`), which are added to the ones there.
fn merge(into: &mut Value, from: &Value, path: &str) {
    match (into, from) {
        (Value::Object(into), Value::Object(from)) => {
            for (key, value) in from {
                let here = format!("{path}/{key}");
                merge(into.entry(key.clone()).or_insert(Value::Null), value, &here);
            }
        }
        (Value::Array(into), Value::Array(from)) if path == "/hw/com" => into.extend(from.iter().cloned()),
        (into, from) => *into = from.clone(),
    }
}

/// Drops the LED outputs that end past `max_leds`, as WLED does when it sets its outputs up.
fn drop_outputs(cfg: &mut Value, max_leds: i64) {
    if let Some(ins) = cfg["hw"]["led"]["ins"].as_array_mut() {
        ins.retain(|bus| bus["start"].as_i64().unwrap_or(0) + bus["len"].as_i64().unwrap_or(1) <= max_leds);
    }
}

impl FakeWled {
    /// A WLED with the fixture configuration (an RGB and an RGBW output).
    pub fn start() -> Self {
        let state = FakeWledState {
            info: serde_json::from_str(include_str!("../fixtures/wled/info.json")).expect("fixture parses"),
            cfg: serde_json::from_str(include_str!("../fixtures/wled/cfg.json")).expect("fixture parses"),
            requests: Vec::new(),
            config_writes: Vec::new(),
            fail_writes_from: None,
            ignore_writes: false,
            refuse_writes: None,
            max_leds: 1500,
        };
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
        let address = listener.local_addr().expect("local address").to_string();
        let state = Arc::new(Mutex::new(state));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let state = Arc::clone(&state);
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    let Ok(stream) = stream else { continue };
                    let state = Arc::clone(&state);
                    std::thread::spawn(move || {
                        let _ = serve(stream, &state);
                    });
                }
            })
        };
        Self {
            address,
            state,
            stop,
            thread: Some(thread),
        }
    }

    /// `127.0.0.1:<port>`: use it where a WLED's address goes.
    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn state(&self) -> MutexGuard<'_, FakeWledState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for FakeWled {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(&self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(stream: TcpStream, state: &Mutex<FakeWledState>) -> std::io::Result<()> {
    let lock = || state.lock().unwrap_or_else(PoisonError::into_inner);
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut stream = stream;
    let Some(request) = read_head(&mut reader)? else {
        return Ok(());
    };
    let body = if request.method == "POST" {
        read_body(&mut reader, &request)?
    } else {
        Vec::new()
    };
    let (status, reply) = {
        let mut s = lock();
        s.requests.push(format!("{} {}", request.method, request.path));
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/json/info") => (200, s.info.to_string()),
            ("GET", "/json/cfg") => (200, s.cfg.to_string()),
            ("POST", "/json/cfg") => match serde_json::from_slice::<Value>(&body) {
                Ok(doc @ Value::Object(_)) => {
                    s.config_writes.push(doc.clone());
                    let count = s.config_writes.len();
                    if let Some(status) = s.refuse_writes {
                        (status, json!({"error": 1}).to_string())
                    } else if s.fail_writes_from.is_some_and(|from| count >= from) {
                        (500, "Internal Server Error".to_string())
                    } else {
                        if !s.ignore_writes {
                            merge(&mut s.cfg, &doc, "");
                            let max = s.max_leds;
                            drop_outputs(&mut s.cfg, max);
                        }
                        (200, json!({"success": true}).to_string())
                    }
                }
                _ => (400, json!({"error": 9}).to_string()),
            },
            _ => (404, "Not Found".to_string()),
        }
    };
    respond(&mut stream, status, &reply)
}
