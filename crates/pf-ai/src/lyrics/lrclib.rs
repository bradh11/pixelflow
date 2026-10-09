//! Published lyrics from LRCLIB (<https://lrclib.net>), a free lyrics library with no key.
//!
//! Only the song's artist, title, album, and length are sent. First `GET /api/get` (an exact
//! match on artist, title, album, and length, ±2 s on LRCLIB's side) when the artist is known,
//! then `GET /api/search?q=`. The entries that could be the song are kept as candidates: length
//! within ±3 s of the song's, title (and artist, when known) alike, with lyrics, and not
//! instrumental ([`score`]). Which one is used is chosen by [`super::choose`].
//!
//! A server error, a timeout, or a dropped connection is tried again twice, after a short wait.

use crate::http::{
    HeaderValue, HttpRequest, HttpResponse, Method, Transport, TransportError, USER_AGENT,
    sleep_unless_cancelled,
};
use crate::provider::Cancel;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

pub const BASE_URL: &str = "https://lrclib.net";

/// The largest reply read (a search answers up to 20 entries, each with its lyrics twice).
const MAX_BODY: u64 = 8 * 1024 * 1024;
/// How far the published length may be from the song's.
pub const DURATION_SLACK_S: f64 = 3.0;
/// Tries of one request in all (two retries).
const ATTEMPTS: u32 = 3;
/// The wait before the first retry; the second waits twice as long.
const RETRY_DELAY: Duration = Duration::from_millis(500);

/// What is asked of LRCLIB: nothing but the song's name and length.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SongQuery {
    pub artist: Option<String>,
    pub title: String,
    pub album: Option<String>,
    pub duration_s: Option<f64>,
}

/// One entry in LRCLIB.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Published {
    pub id: i64,
    pub artist: String,
    pub title: String,
    pub duration_s: f64,
    pub instrumental: bool,
    /// LRC text, one stamped line per sung line.
    pub synced: Option<String>,
    pub plain: Option<String>,
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// One entry from LRCLIB's JSON (`None` when it isn't one).
pub fn parse_record(value: &Value) -> Option<Published> {
    Some(Published {
        id: value["id"].as_i64()?,
        artist: text(&value["artistName"]).unwrap_or_default(),
        title: text(&value["trackName"]).unwrap_or_default(),
        duration_s: value["duration"].as_f64().unwrap_or(0.0),
        instrumental: value["instrumental"].as_bool().unwrap_or(false),
        synced: text(&value["syncedLyrics"]),
        plain: text(&value["plainLyrics"]),
    })
}

/// The entries of a search reply.
pub fn parse_search(value: &Value) -> Vec<Published> {
    value
        .as_array()
        .map(|items| items.iter().filter_map(parse_record).collect())
        .unwrap_or_default()
}

/// Lowercase words of letters and digits, with what's in brackets and after " - " left out
/// ("Lantern Song - From \"A Film\" (Remastered)" is "lantern song"), and "jr", "feat"
/// and "the" dropped.
pub fn name_words(name: &str) -> Vec<String> {
    let mut kept = String::new();
    let mut depth = 0usize;
    for c in name.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => kept.push(c),
            _ => {}
        }
    }
    let main = kept.split(" - ").next().unwrap_or("");
    main.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && !matches!(*w, "jr" | "feat" | "ft" | "the"))
        .map(str::to_string)
        .collect()
}

/// How alike two names are, 0–1: the same words is 1, one inside the other 0.8, else the share
/// of words they have in common.
pub fn similarity(a: &str, b: &str) -> f64 {
    let (a, b) = (name_words(a), name_words(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let inside = |x: &[String], y: &[String]| x.windows(y.len().max(1)).any(|w| w == y);
    if (a.len() > b.len() && inside(&a, &b)) || (b.len() > a.len() && inside(&b, &a)) {
        return 0.8;
    }
    let common = a.iter().filter(|w| b.contains(w)).count();
    common as f64 / a.len().max(b.len()) as f64
}

/// How well an entry fits the song (higher is better), or `None` when it can't be the song:
/// no lyrics, instrumental, a title too unlike, or a length off by more than 3 s.
pub fn score(query: &SongQuery, entry: &Published) -> Option<f64> {
    if entry.instrumental || (entry.synced.is_none() && entry.plain.is_none()) {
        return None;
    }
    // A title is sometimes filed under the artist's name, and the other way round.
    let title = similarity(&query.title, &entry.title).max(0.9 * similarity(&query.title, &entry.artist));
    if title < 0.5 {
        return None;
    }
    let off = match query.duration_s {
        Some(duration) if entry.duration_s > 0.0 => {
            let off = (entry.duration_s - duration).abs();
            if off > DURATION_SLACK_S {
                return None;
            }
            off
        }
        _ => DURATION_SLACK_S,
    };
    let artist = query.artist.as_deref().map_or(0.5, |a| {
        similarity(a, &entry.artist).max(similarity(a, &entry.title))
    });
    let synced = if entry.synced.is_some() { 1.0 } else { 0.0 };
    Some(3.0 * title + 2.0 * artist + synced - off / DURATION_SLACK_S)
}

/// The entry that fits the song best by name and length alone, if any fits.
pub fn best(query: &SongQuery, entries: Vec<Published>) -> Option<Published> {
    entries
        .into_iter()
        .filter_map(|e| score(query, &e).map(|s| (s, e)))
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, e)| e)
}

/// The entries that could be the song (see [`score`]), each once, in the order given.
pub fn fitting(query: &SongQuery, entries: Vec<Published>) -> Vec<Published> {
    let mut kept: Vec<Published> = Vec::new();
    for entry in entries {
        if score(query, &entry).is_some() && !kept.iter().any(|k| k.id == entry.id) {
            kept.push(entry);
        }
    }
    kept
}

/// Text for a URL query.
fn escape(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The exact-match URL, when the artist is known.
pub fn get_url(base: &str, query: &SongQuery) -> Option<String> {
    let artist = query.artist.as_deref()?;
    let mut url = format!(
        "{base}/api/get?artist_name={}&track_name={}",
        escape(artist),
        escape(&query.title)
    );
    if let Some(album) = &query.album {
        url.push_str(&format!("&album_name={}", escape(album)));
    }
    if let Some(duration) = query.duration_s {
        url.push_str(&format!("&duration={}", duration.round() as u64));
    }
    Some(url)
}

/// The search URL: artist and title as one query.
pub fn search_url(base: &str, query: &SongQuery) -> String {
    let words = match &query.artist {
        Some(artist) => format!("{artist} {}", query.title),
        None => query.title.clone(),
    };
    format!("{base}/api/search?q={}", escape(&words))
}

/// Why LRCLIB couldn't be asked, in words for the user ([`LookupError::detail`] has the
/// particulars, for the log).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LookupError {
    #[error(
        "Couldn't reach LRCLIB, the published lyrics library. Check your internet connection, then try again."
    )]
    Network,
    #[error("LRCLIB, the published lyrics library, took too long to answer. Try again later.")]
    Timeout,
    #[error("The connection to LRCLIB, the published lyrics library, dropped. Try again later.")]
    Dropped,
    #[error("LRCLIB, the published lyrics library, isn't working right now. Try again later.")]
    Status(u16),
    #[error("LRCLIB, the published lyrics library, sent a reply PixelFlow couldn't read.")]
    BadReply,
    #[error("Stopped.")]
    Cancelled,
}

impl LookupError {
    /// What happened, for the log.
    pub fn detail(&self) -> String {
        match self {
            LookupError::Network => "LRCLIB: couldn't connect".into(),
            LookupError::Timeout => format!("LRCLIB: timed out ({ATTEMPTS} tries)"),
            LookupError::Dropped => format!("LRCLIB: the connection dropped ({ATTEMPTS} tries)"),
            LookupError::Status(status) => format!("LRCLIB: HTTP {status}"),
            LookupError::BadReply => "LRCLIB: the reply wasn't the JSON expected".into(),
            LookupError::Cancelled => "LRCLIB: stopped".into(),
        }
    }

    /// Whether trying again soon might work: a server error, a timeout, or a dropped connection.
    fn passing(&self) -> bool {
        match self {
            LookupError::Status(status) => *status >= 500,
            LookupError::Timeout | LookupError::Dropped => true,
            _ => false,
        }
    }
}

/// Asks LRCLIB.
pub struct Lrclib {
    transport: Arc<dyn Transport>,
    base_url: String,
    retry_delay: Duration,
}

impl Lrclib {
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self {
            transport,
            base_url: BASE_URL.to_string(),
            retry_delay: RETRY_DELAY,
        }
    }

    /// Waits `delay` before the first retry (tests: none).
    pub fn with_retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = delay;
        self
    }

    /// The JSON reply to a GET, or `None` for "not found", tried again after a passing problem.
    fn get(&self, url: &str, cancel: &Cancel) -> Result<Option<Value>, LookupError> {
        let mut attempt = 1;
        loop {
            match self.get_once(url, cancel) {
                Err(error) if error.passing() && attempt < ATTEMPTS => {
                    if !sleep_unless_cancelled(self.retry_delay * attempt, cancel) {
                        return Err(LookupError::Cancelled);
                    }
                    attempt += 1;
                }
                result => return result,
            }
        }
    }

    fn get_once(&self, url: &str, cancel: &Cancel) -> Result<Option<Value>, LookupError> {
        if cancel.is_cancelled() {
            return Err(LookupError::Cancelled);
        }
        let request = HttpRequest {
            method: Method::Get,
            url: url.to_string(),
            headers: vec![
                ("user-agent", HeaderValue::Plain(USER_AGENT.into())),
                ("accept", HeaderValue::Plain("application/json".into())),
            ],
            body: None,
        };
        let response: HttpResponse = self.transport.send(&request).map_err(|e| match e {
            TransportError::Timeout | TransportError::ConnectTimeout => LookupError::Timeout,
            TransportError::Failed => LookupError::Dropped,
            TransportError::Unreachable => LookupError::Network,
        })?;
        if response.status == 404 {
            return Ok(None);
        }
        if !(200..300).contains(&response.status) {
            return Err(LookupError::Status(response.status));
        }
        let mut body = String::new();
        response
            .body
            .take(MAX_BODY)
            .read_to_string(&mut body)
            .map_err(|_| LookupError::BadReply)?;
        if cancel.is_cancelled() {
            return Err(LookupError::Cancelled);
        }
        serde_json::from_str(&body)
            .map(Some)
            .map_err(|_| LookupError::BadReply)
    }

    /// The entries that could be the song: the exact match (when the artist is known) first,
    /// then what a search finds. A failed search still leaves an exact match.
    pub fn candidates(&self, query: &SongQuery, cancel: &Cancel) -> Result<Vec<Published>, LookupError> {
        let mut entries = Vec::new();
        if let Some(url) = get_url(&self.base_url, query)
            && let Some(value) = self.get(&url, cancel)?
        {
            entries.extend(parse_record(&value));
        }
        match self.get(&search_url(&self.base_url, query), cancel) {
            Ok(found) => entries.extend(found.map(|v| parse_search(&v)).unwrap_or_default()),
            Err(LookupError::Cancelled) => return Err(LookupError::Cancelled),
            Err(_) if !fitting(query, entries.clone()).is_empty() => {}
            Err(error) => return Err(error),
        }
        Ok(fitting(query, entries))
    }

    /// The published lyrics that best fit the song by name and length, if LRCLIB has any.
    pub fn find(&self, query: &SongQuery, cancel: &Cancel) -> Result<Option<Published>, LookupError> {
        Ok(best(query, self.candidates(query, cancel)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeTransport, Reply};
    use serde_json::json;

    fn entry(id: i64, artist: &str, title: &str, duration: f64, synced: bool) -> Value {
        json!({
            "id": id,
            "trackName": title,
            "artistName": artist,
            "albumName": "Made Up Album",
            "duration": duration,
            "instrumental": false,
            "plainLyrics": "Paper lanterns glowing\nSnowy rooftops shine",
            "syncedLyrics": if synced { json!("[00:10.00]Paper lanterns glowing\n[00:14.00]Snowy rooftops shine") } else { Value::Null },
        })
    }

    fn query(artist: Option<&str>, title: &str, duration: f64) -> SongQuery {
        SongQuery {
            artist: artist.map(str::to_string),
            title: title.into(),
            album: None,
            duration_s: Some(duration),
        }
    }

    #[test]
    fn names_compare_by_their_words() {
        assert_eq!(
            name_words("Lantern Song - From \"A Film\" (Remastered 2009)"),
            ["lantern", "song"]
        );
        assert_eq!(similarity("The Lantern Band, Jr.", "Lantern Band Jr"), 1.0);
        assert_eq!(similarity("Lantern Song", "Lantern Song Extended Mix"), 0.8);
        assert!(similarity("Lantern Song", "Rooftop Waltz") < 0.5);
    }

    #[test]
    fn the_best_match_is_close_in_length_alike_in_name_and_synced() {
        let entries = parse_search(&json!([
            entry(1, "The Lantern Band", "Lantern Song", 251.0, true),
            entry(2, "Lantern Band", "Lantern Song (Live)", 238.0, false),
            entry(3, "Lantern Band", "Lantern Song", 239.5, true),
            entry(4, "Someone Else", "Rooftop Waltz", 238.0, true),
            { "id": 5, "trackName": "Lantern Song", "artistName": "Lantern Band", "duration": 238.0, "instrumental": true },
        ]));
        assert_eq!(entries.len(), 5);
        let q = query(Some("Lantern Band"), "Lantern Song", 237.0);
        // 1 is 14 s too long; 4 is another song; 5 is instrumental: 3 (synced) beats 2 (plain).
        assert_eq!(score(&q, &entries[0]), None);
        assert_eq!(score(&q, &entries[3]), None);
        assert_eq!(score(&q, &entries[4]), None);
        assert_eq!(best(&q, entries.clone()).map(|e| e.id), Some(3));
        // Without an artist the title and length decide.
        let q = query(None, "lantern song", 238.2);
        assert_eq!(best(&q, entries).map(|e| e.id), Some(3));
        // Nothing near the right length: nothing.
        let q = query(None, "Lantern Song", 300.0);
        assert_eq!(
            best(
                &q,
                parse_search(&json!([entry(1, "A", "Lantern Song", 251.0, true)]))
            ),
            None
        );
    }

    #[test]
    fn urls_carry_only_the_song_name_and_length() {
        let q = SongQuery {
            artist: Some("Lantern Band".into()),
            title: "Lantern Song".into(),
            album: Some("Rooftops & Snow".into()),
            duration_s: Some(237.4),
        };
        assert_eq!(
            get_url(BASE_URL, &q).unwrap(),
            "https://lrclib.net/api/get?artist_name=Lantern%20Band&track_name=Lantern%20Song&album_name=Rooftops%20%26%20Snow&duration=237"
        );
        assert_eq!(
            search_url(BASE_URL, &q),
            "https://lrclib.net/api/search?q=Lantern%20Band%20Lantern%20Song"
        );
        assert_eq!(get_url(BASE_URL, &query(None, "Lantern Song", 1.0)), None);
    }

    #[test]
    fn an_exact_match_first_then_a_search() {
        let fake = Arc::new(FakeTransport::new(vec![
            Reply::status(
                404,
                r#"{"code":404,"name":"TrackNotFound","message":"Failed to find specified track"}"#,
            ),
            Reply::ok(json!([entry(7, "Lantern Band", "Lantern Song", 238.0, true)]).to_string()),
        ]));
        let lrclib = Lrclib::new(fake.clone()).with_retry_delay(Duration::ZERO);
        let found = lrclib
            .find(
                &query(Some("Lantern Band"), "Lantern Song", 237.0),
                &Cancel::new(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(found.id, 7);
        assert!(found.synced.unwrap().starts_with("[00:10.00]"));
        let requests = fake.requests();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].url.starts_with("https://lrclib.net/api/get?"));
        assert!(requests[1].url.starts_with("https://lrclib.net/api/search?q="));
        assert!(
            requests
                .iter()
                .all(|r| r.header("user-agent").unwrap().starts_with("PixelFlow/"))
        );

        // An exact match, then a search that finds it again and another: both kept, once each;
        // a search that fails still leaves the exact match.
        let fake = Arc::new(FakeTransport::new(vec![
            Reply::ok(entry(9, "Lantern Band", "Lantern Song", 237.0, false).to_string()),
            Reply::ok(
                json!([
                    entry(9, "Lantern Band", "Lantern Song", 237.0, false),
                    entry(10, "Lantern Band", "Lantern Song", 238.0, true),
                ])
                .to_string(),
            ),
        ]));
        let q = query(Some("Lantern Band"), "Lantern Song", 237.0);
        let found = Lrclib::new(fake.clone()).candidates(&q, &Cancel::new()).unwrap();
        assert_eq!(found.iter().map(|e| e.id).collect::<Vec<_>>(), [9, 10]);
        assert_eq!(fake.requests().len(), 2);
        let fake = Arc::new(FakeTransport::new(vec![
            Reply::ok(entry(9, "Lantern Band", "Lantern Song", 237.0, false).to_string()),
            Reply::Unreachable,
        ]));
        let found = Lrclib::new(fake).find(&q, &Cancel::new()).unwrap();
        assert_eq!(found.map(|e| e.id), Some(9));
    }

    #[test]
    fn a_server_error_or_timeout_is_tried_again_twice() {
        let ok = || Reply::ok(json!([entry(7, "Lantern Band", "Lantern Song", 238.0, true)]).to_string());
        let q = query(None, "Lantern Song", 237.0);
        let fake = Arc::new(FakeTransport::new(vec![Reply::status(500, ""), ok()]));
        let lrclib = Lrclib::new(fake.clone()).with_retry_delay(Duration::ZERO);
        assert_eq!(lrclib.find(&q, &Cancel::new()).unwrap().map(|e| e.id), Some(7));
        assert_eq!(fake.requests().len(), 2);

        let fake = Arc::new(FakeTransport::new(vec![
            Reply::Timeout,
            Reply::status(502, ""),
            ok(),
        ]));
        let lrclib = Lrclib::new(fake.clone()).with_retry_delay(Duration::ZERO);
        assert!(lrclib.find(&q, &Cancel::new()).unwrap().is_some());
        assert_eq!(fake.requests().len(), 3);

        let fake = Arc::new(FakeTransport::new(vec![
            Reply::status(500, ""),
            Reply::status(500, ""),
            Reply::status(500, ""),
            ok(),
        ]));
        let lrclib = Lrclib::new(fake.clone()).with_retry_delay(Duration::ZERO);
        let error = lrclib.find(&q, &Cancel::new()).unwrap_err();
        assert_eq!(error, LookupError::Status(500));
        assert_eq!(fake.requests().len(), 3);
        // The user sees no status code; the log does.
        assert!(!error.to_string().contains("500"), "{error}");
        assert_eq!(error.detail(), "LRCLIB: HTTP 500");

        // A client error isn't tried again.
        let fake = Arc::new(FakeTransport::new(vec![Reply::status(429, ""), ok()]));
        let lrclib = Lrclib::new(fake.clone()).with_retry_delay(Duration::ZERO);
        assert_eq!(lrclib.find(&q, &Cancel::new()), Err(LookupError::Status(429)));
        assert_eq!(fake.requests().len(), 1);
    }

    #[test]
    fn failures_are_plain() {
        let fake = Arc::new(FakeTransport::new(vec![Reply::Unreachable]));
        let error = Lrclib::new(fake)
            .find(&query(None, "Lantern Song", 237.0), &Cancel::new())
            .unwrap_err();
        assert_eq!(error, LookupError::Network);
        let fake = Arc::new(FakeTransport::new(vec![Reply::status(503, ""); 3]));
        let error = Lrclib::new(fake)
            .with_retry_delay(Duration::ZERO)
            .find(&query(None, "Lantern Song", 237.0), &Cancel::new())
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "LRCLIB, the published lyrics library, isn't working right now. Try again later."
        );
        let fake = Arc::new(FakeTransport::new(vec![Reply::ok("not json")]));
        assert_eq!(
            Lrclib::new(fake).find(&query(None, "x", 1.0), &Cancel::new()),
            Err(LookupError::BadReply)
        );
        let cancel = Cancel::new();
        cancel.cancel();
        let fake = Arc::new(FakeTransport::new(vec![]));
        assert_eq!(
            Lrclib::new(fake.clone()).find(&query(None, "x", 1.0), &cancel),
            Err(LookupError::Cancelled)
        );
        assert!(fake.requests().is_empty());
    }
}
