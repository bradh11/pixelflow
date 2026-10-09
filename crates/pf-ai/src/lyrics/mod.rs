//! Finding a song's lyrics and when each word is sung, for the sequencer's Find lyrics.
//!
//! Only when the user asks, and only once the assistant is set up ([`gate`]):
//!
//! 1. The song's artist, title, album, and length come from its tags (or its file name).
//! 2. Published lyrics are looked up in LRCLIB ([`lrclib`]); only those four things are sent.
//! 3. With an OpenAI key, and only after the user agreed to send the song's audio, OpenAI's
//!    speech recognition hears the words and when they're sung ([`transcribe`]).
//! 4. The two are put together ([`combine`]), sung stretches found ([`vocals`]), and all of it
//!    written as Lyrics, Lyrics (words), and Vocals timing tracks ([`tracks`]).
//!
//! What LRCLIB and OpenAI answered is kept by the song file's hash ([`cache`]), so the same song
//! is never sent twice.

pub mod cache;
pub mod combine;
pub mod gate;
pub mod lrc;
pub mod lrclib;
pub mod tracks;
pub mod transcribe;
pub mod vocals;

pub use cache::LyricsCache;
pub use combine::{Phrase, Word, WordSource};
pub use gate::{LyricsGate, gate};

use crate::provider::Cancel;
use crate::secret::ApiKey;
use lrclib::{LookupError, Lrclib, Published, SongQuery};
use pf_analysis::VocalActivity;
use pf_audio::SongTags;
use pf_sequence::TimingTrack;
use serde::Serialize;
use std::path::Path;
use transcribe::{Heard, Transcriber};

/// What's being done, for the progress line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Step {
    ReadingSong,
    LookingUp,
    Listening,
    LiningUp,
}

impl Step {
    pub fn label(self) -> &'static str {
        match self {
            Step::ReadingSong => "Reading the song",
            Step::LookingUp => "Looking up published lyrics",
            Step::Listening => "Sending the audio to OpenAI to hear the words",
            Step::LiningUp => "Lining up the words",
        }
    }
}

/// Reads a song's voice (see [`pf_analysis::vocal_activity`]); tests use a stand-in.
pub type VoiceReader = dyn Fn(&Path, &Cancel) -> Result<VocalActivity, String> + Send + Sync;

/// Reads a song's tags (see [`pf_audio::read_tags`]).
pub type TagReader = dyn Fn(&Path) -> Option<SongTags> + Send + Sync;

/// Who's asked, and how.
pub struct Services {
    pub lrclib: Lrclib,
    pub transcriber: Transcriber,
    pub voice: Box<VoiceReader>,
    pub tags: Box<TagReader>,
}

impl Services {
    /// The real thing: LRCLIB and OpenAI over HTTPS, the song's own tags and voice.
    pub fn live() -> Self {
        Self {
            lrclib: Lrclib::new(std::sync::Arc::new(crate::http::UreqTransport::quick())),
            transcriber: Transcriber::new(std::sync::Arc::new(crate::http::UreqTransport::new())),
            voice: Box::new(|path, cancel| {
                pf_analysis::vocal_activity_file(path, &|| cancel.is_cancelled()).map_err(|e| e.to_string())
            }),
            tags: Box::new(|path| pf_audio::read_tags(path).ok()),
        }
    }
}

/// One song to find lyrics for.
pub struct Request<'a> {
    pub path: &'a Path,
    /// The sequence's length: nothing is placed after it.
    pub duration_ms: u64,
    /// Section starts, where audio too long for one upload is cut.
    pub sections_ms: &'a [u64],
    /// The OpenAI key, only when the recognizer may be used: the chosen provider is OpenAI and
    /// the user agreed to send the song's audio.
    pub recognizer: Option<ApiKey>,
    pub cache: Option<&'a LyricsCache>,
    pub cancel: &'a Cancel,
}

/// Where the text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextFrom {
    Lrclib,
    Openai,
}

/// Where the word times came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TimingFrom {
    /// OpenAI's speech recognition.
    Openai,
    /// LRCLIB's own word stamps.
    LrclibWords,
    /// LRCLIB's line times, words shared out by syllables.
    LrclibLines,
}

/// What was found.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub phrases: Vec<Phrase>,
    pub vocals: Vec<(u64, u64)>,
    pub text_from: TextFrom,
    pub timing_from: TimingFrom,
    /// "Lyrics from LRCLIB, word timing from OpenAI."
    pub summary: String,
    /// Anything worth knowing that didn't stop it ("OpenAI couldn't be used: …").
    pub notes: Vec<String>,
    /// Words whose timing is a guess (confidence under 0.5).
    pub unsure_words: usize,
    #[serde(skip)]
    pub tracks: Vec<TimingTrack>,
}

/// What's asked of LRCLIB for a song with `tags`.
pub fn song_query(tags: &SongTags, duration_ms: u64) -> Option<SongQuery> {
    Some(SongQuery {
        artist: tags.artist.clone(),
        title: tags.title.clone()?,
        album: tags.album.clone(),
        duration_s: Some(tags.duration_ms.unwrap_or(duration_ms) as f64 / 1000.0),
    })
}

/// The published lyrics, from the cache or LRCLIB.
fn published(
    services: &Services,
    request: &Request<'_>,
    hash: Option<&str>,
    query: Option<&SongQuery>,
) -> Result<Option<Published>, LookupError> {
    let cached = hash
        .zip(request.cache)
        .and_then(|(h, c)| c.load::<Published>(h, "lrclib"));
    if cached.is_some() {
        return Ok(cached);
    }
    let Some(query) = query else {
        return Ok(None);
    };
    let found = services.lrclib.find(query, request.cancel)?;
    if let (Some(found), Some(hash), Some(cache)) = (&found, hash, request.cache) {
        cache.store(hash, "lrclib", found);
    }
    Ok(found)
}

/// What the recognizer heard, from the cache or OpenAI.
fn heard(
    services: &Services,
    request: &Request<'_>,
    key: &ApiKey,
    hash: Option<&str>,
) -> Result<Heard, String> {
    if let Some(cached) = hash
        .zip(request.cache)
        .and_then(|(h, c)| c.load::<Heard>(h, "openai"))
    {
        return Ok(cached);
    }
    let cancel = request.cancel;
    let uploads = transcribe::uploads_for(request.path, request.sections_ms, &|| cancel.is_cancelled())?;
    let heard = services
        .transcriber
        .transcribe(key, &uploads, cancel)
        .map_err(|e| e.to_string())?;
    if let (Some(hash), Some(cache)) = (hash, request.cache) {
        cache.store(hash, "openai", &heard);
    }
    Ok(heard)
}

const NO_LYRICS: &str = "No lyrics found for this song.";

/// Finds the song's lyrics and word times, calling `on_step` as it goes. The `Err` is a plain
/// sentence for the user.
pub fn find_lyrics(
    services: &Services,
    request: &Request<'_>,
    on_step: &mut dyn FnMut(Step),
) -> Result<Found, String> {
    let cancel = request.cancel;
    let stopped = || "Stopped.".to_string();
    on_step(Step::ReadingSong);
    let tags = (services.tags)(request.path).unwrap_or_default();
    let hash = cache::file_hash(request.path, &|| cancel.is_cancelled());
    cancel.check().map_err(|_| stopped())?;
    let query = song_query(&tags, request.duration_ms);

    on_step(Step::LookingUp);
    let mut notes = Vec::new();
    let published = match published(services, request, hash.as_deref(), query.as_ref()) {
        Ok(found) => found,
        Err(LookupError::Cancelled) => return Err(stopped()),
        // Without the recognizer there's nothing else to go by.
        Err(error) if request.recognizer.is_none() => return Err(error.to_string()),
        Err(error) => {
            notes.push(error.to_string());
            None
        }
    };
    let synced = published
        .as_ref()
        .and_then(|p| p.synced.as_deref())
        .map(lrc::parse_lrc)
        .map(|lines| combine::timed_lines(&lines, request.duration_ms))
        .filter(|lines| !lines.is_empty());
    let plain = published
        .as_ref()
        .and_then(|p| p.plain.as_deref())
        .map(lrc::plain_lines)
        .filter(|lines| !lines.is_empty());

    let heard = match &request.recognizer {
        Some(key) => {
            on_step(Step::Listening);
            match heard(services, request, key, hash.as_deref()) {
                Ok(heard) if !heard.words.is_empty() => Some(heard),
                Ok(_) => None,
                Err(_) if cancel.is_cancelled() => return Err(stopped()),
                // Published lines still work without it.
                Err(error) if synced.is_some() => {
                    notes.push(format!("OpenAI couldn't be used: {error}"));
                    None
                }
                Err(error) => return Err(error),
            }
        }
        _ => None,
    };
    cancel.check().map_err(|_| stopped())?;

    on_step(Step::LiningUp);
    let voice = (services.voice)(request.path, cancel).ok();
    cancel.check().map_err(|_| stopped())?;
    let onsets = voice.as_ref().map_or(&[][..], |v| &v.onsets[..]);
    let end = request.duration_ms;
    let (phrases, text_from, timing_from) = match (&synced, &plain, &heard) {
        (Some(lines), _, Some(heard)) => (
            combine::from_lines_and_heard(lines, heard, end),
            TextFrom::Lrclib,
            TimingFrom::Openai,
        ),
        (Some(lines), _, None) => {
            let stamped = lines.iter().all(|l| !l.stamped.is_empty());
            let timing = if stamped {
                TimingFrom::LrclibWords
            } else {
                TimingFrom::LrclibLines
            };
            (combine::from_lines(lines, onsets), TextFrom::Lrclib, timing)
        }
        (None, Some(lines), Some(heard)) => {
            let phrases = combine::from_plain_and_heard(lines, heard, end);
            if phrases.is_empty() {
                (combine::from_heard(heard), TextFrom::Openai, TimingFrom::Openai)
            } else {
                (phrases, TextFrom::Lrclib, TimingFrom::Openai)
            }
        }
        (None, None, Some(heard)) => (combine::from_heard(heard), TextFrom::Openai, TimingFrom::Openai),
        (None, Some(_), None) => {
            return Err(
                "LRCLIB has this song's lyrics, but not when they're sung. With an OpenAI key set up for the assistant, PixelFlow can hear when.".into(),
            );
        }
        (None, None, None) => return Err(NO_LYRICS.into()),
    };
    let phrases: Vec<Phrase> = phrases.into_iter().filter(|p| p.start_ms < end).collect();
    if phrases.is_empty() {
        return Err(NO_LYRICS.into());
    }
    let words: Vec<(u64, u64)> = phrases
        .iter()
        .flat_map(|p| &p.words)
        .map(|w| (w.start_ms, w.end_ms))
        .collect();
    let mut vocals = vocals::regions_from_words(&words);
    if let Some(voice) = &voice {
        vocals = vocals::refine(&vocals, voice, end);
    }
    let unsure_words = phrases
        .iter()
        .flat_map(|p| &p.words)
        .filter(|w| w.confidence < 0.5)
        .count();
    let summary = match (text_from, timing_from) {
        (TextFrom::Lrclib, TimingFrom::Openai) => "Lyrics from LRCLIB, word timing from OpenAI.",
        (TextFrom::Lrclib, TimingFrom::LrclibWords) => "Lyrics and word timing from LRCLIB.",
        (TextFrom::Lrclib, TimingFrom::LrclibLines) => {
            "Lyrics and line timing from LRCLIB; words are spread over each line."
        }
        (TextFrom::Openai, _) => "Lyrics and word timing from OpenAI (LRCLIB had none for this song).",
    }
    .to_string();
    let tracks = tracks::lyric_tracks(&phrases, &vocals, end);
    Ok(Found {
        phrases,
        vocals,
        text_from,
        timing_from,
        summary,
        notes,
        unsure_words,
        tracks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::RetryPolicy;
    use crate::testing::{FakeTransport, Reply, fake_key};
    use serde_json::json;
    use std::sync::Arc;

    /// Made-up lyrics.
    const SYNCED: &str = "[00:01.00]Paper lanterns glowing\n[00:04.00]Snowy rooftops shine\n[00:07.00]";

    fn entry(synced: Option<&str>, plain: Option<&str>) -> String {
        json!([{
            "id": 1, "trackName": "Lantern Song", "artistName": "Lantern Band", "albumName": "Made Up",
            "duration": 10.0, "instrumental": false, "plainLyrics": plain, "syncedLyrics": synced,
        }])
        .to_string()
    }

    const HEARD: &str = r#"{"text": "paper lanterns glowing snowy rooftops shine", "words": [
        {"word": "paper", "start": 1.2, "end": 1.6}, {"word": "lanterns", "start": 1.6, "end": 2.3},
        {"word": "glowing", "start": 2.3, "end": 3.1}, {"word": "snowy", "start": 4.1, "end": 4.6},
        {"word": "rooftops", "start": 4.6, "end": 5.3}, {"word": "shine", "start": 5.3, "end": 6.4}]}"#;

    fn fakes(lrclib: Vec<Reply>, openai: Vec<Reply>) -> (Services, Arc<FakeTransport>, Arc<FakeTransport>) {
        let lrclib_fake = Arc::new(FakeTransport::new(lrclib));
        let openai_fake = Arc::new(FakeTransport::new(openai));
        let services = Services {
            lrclib: Lrclib::new(lrclib_fake.clone()),
            transcriber: Transcriber::new(openai_fake.clone()).with_retry(RetryPolicy::immediate()),
            voice: Box::new(|_, _| Ok(VocalActivity::default())),
            tags: Box::new(|_| {
                Some(SongTags {
                    title: Some("Lantern Song".into()),
                    duration_ms: Some(10_000),
                    title_from_file_name: true,
                    ..SongTags::default()
                })
            }),
        };
        (services, lrclib_fake, openai_fake)
    }

    fn song() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lantern song.wav");
        std::fs::write(&path, pf_audio::wav_bytes(&[0.0; 1_600], 16_000)).unwrap();
        (dir, path)
    }

    fn request<'a>(
        path: &'a Path,
        key: Option<ApiKey>,
        cache: Option<&'a LyricsCache>,
        cancel: &'a Cancel,
    ) -> Request<'a> {
        Request {
            path,
            duration_ms: 10_000,
            sections_ms: &[],
            recognizer: key,
            cache,
            cancel,
        }
    }

    #[test]
    fn published_lines_and_heard_words_together() {
        let (_dir, path) = song();
        let (services, lrclib, openai) =
            fakes(vec![Reply::ok(entry(Some(SYNCED), None))], vec![Reply::ok(HEARD)]);
        let mut steps = Vec::new();
        let cancel = Cancel::new();
        let found = find_lyrics(
            &services,
            &request(&path, Some(fake_key()), None, &cancel),
            &mut |s| steps.push(s),
        )
        .unwrap();
        assert_eq!(found.summary, "Lyrics from LRCLIB, word timing from OpenAI.");
        assert_eq!(
            steps,
            [
                Step::ReadingSong,
                Step::LookingUp,
                Step::Listening,
                Step::LiningUp
            ]
        );
        assert_eq!(found.phrases[0].text, "Paper lanterns glowing");
        assert_eq!(found.phrases[0].start_ms, 1_200);
        assert_eq!(found.vocals, [(1_200, 6_400)]);
        assert_eq!(found.tracks.len(), 3);
        assert_eq!(found.tracks[1].marks.len(), 6);
        // No artist in the tags: a search by title; the audio went to OpenAI once.
        assert!(lrclib.requests()[0].url.contains("/api/search?q=Lantern%20Song"));
        assert_eq!(openai.requests().len(), 1);
    }

    #[test]
    fn without_the_recognizer_words_share_their_lines() {
        let (_dir, path) = song();
        let (services, _, openai) = fakes(vec![Reply::ok(entry(Some(SYNCED), None))], vec![]);
        let cancel = Cancel::new();
        let found = find_lyrics(&services, &request(&path, None, None, &cancel), &mut |_| {}).unwrap();
        assert_eq!(found.timing_from, TimingFrom::LrclibLines);
        assert!(found.summary.contains("line timing from LRCLIB"));
        assert_eq!(found.phrases[0].start_ms, 1_000);
        assert!(
            found
                .phrases
                .iter()
                .flat_map(|p| &p.words)
                .all(|w| w.source == WordSource::Spread)
        );
        assert_eq!(found.unsure_words, 6);
        // Nothing was sent to OpenAI.
        assert!(openai.requests().is_empty());
    }

    #[test]
    fn no_published_lyrics_falls_back_to_the_recognizer_or_says_so() {
        let (_dir, path) = song();
        let cancel = Cancel::new();
        let (services, _, _) = fakes(vec![Reply::ok("[]")], vec![Reply::ok(HEARD)]);
        let found = find_lyrics(
            &services,
            &request(&path, Some(fake_key()), None, &cancel),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(found.text_from, TextFrom::Openai);
        assert_eq!(found.phrases[0].words[0].text, "paper");

        let (services, _, _) = fakes(vec![Reply::ok("[]")], vec![]);
        let error = find_lyrics(&services, &request(&path, None, None, &cancel), &mut |_| {}).unwrap_err();
        assert_eq!(error, "No lyrics found for this song.");

        // Plain lyrics alone can't be timed.
        let (services, _, _) = fakes(vec![Reply::ok(entry(None, Some("Paper lanterns")))], vec![]);
        let error = find_lyrics(&services, &request(&path, None, None, &cancel), &mut |_| {}).unwrap_err();
        assert!(error.contains("not when they're sung"), "{error}");
    }

    #[test]
    fn answers_are_cached_by_the_song_file() {
        let (dir, path) = song();
        let cache = LyricsCache::new(dir.path());
        let cancel = Cancel::new();
        let (services, _, _) = fakes(vec![Reply::ok(entry(Some(SYNCED), None))], vec![Reply::ok(HEARD)]);
        find_lyrics(
            &services,
            &request(&path, Some(fake_key()), Some(&cache), &cancel),
            &mut |_| {},
        )
        .unwrap();
        // Again: nobody is asked.
        let (services, lrclib, openai) = fakes(vec![], vec![]);
        let found = find_lyrics(
            &services,
            &request(&path, Some(fake_key()), Some(&cache), &cancel),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(found.timing_from, TimingFrom::Openai);
        assert!(lrclib.requests().is_empty() && openai.requests().is_empty());
    }

    #[test]
    fn a_failed_recognizer_still_leaves_published_lines_and_stop_stops() {
        let (_dir, path) = song();
        let cancel = Cancel::new();
        let (services, _, _) = fakes(
            vec![Reply::ok(entry(Some(SYNCED), None))],
            vec![Reply::status(401, "{}")],
        );
        let found = find_lyrics(
            &services,
            &request(&path, Some(fake_key()), None, &cancel),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(found.timing_from, TimingFrom::LrclibLines);
        assert!(
            found.notes[0].starts_with("OpenAI couldn't be used"),
            "{:?}",
            found.notes
        );

        // LRCLIB unreachable and no recognizer: the reason.
        let (services, _, _) = fakes(vec![Reply::Unreachable], vec![]);
        let error = find_lyrics(&services, &request(&path, None, None, &cancel), &mut |_| {}).unwrap_err();
        assert!(error.contains("Couldn't reach LRCLIB"), "{error}");

        let stopped = Cancel::new();
        stopped.cancel();
        let (services, lrclib, _) = fakes(vec![], vec![]);
        let error = find_lyrics(&services, &request(&path, None, None, &stopped), &mut |_| {}).unwrap_err();
        assert_eq!(error, "Stopped.");
        assert!(lrclib.requests().is_empty());
    }
}
