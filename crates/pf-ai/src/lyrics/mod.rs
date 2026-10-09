//! Finding a song's lyrics and when each word is sung, for the sequencer's Find lyrics.
//!
//! Only when the user asks, and only once the assistant is set up ([`gate`]):
//!
//! 1. The song's artist, title, album, length, and language come from its tags (or its file
//!    name).
//! 2. Published lyrics are looked up in LRCLIB ([`lrclib`]); only the song's name and length
//!    are sent. The entries that could be the song are kept as candidates.
//! 3. With an OpenAI key, and only after the user agreed to send the song's audio, OpenAI's
//!    speech recognition hears the words and when they're sung ([`transcribe`]). It's told the
//!    language ([`language`]): the best candidate's, else the song's tags', else the user's
//!    Lyrics language. Words heard in another alphabet or language are asked for once more
//!    (without the prompt); heard so again, they aren't used, and the user is told.
//! 4. The candidate that is the song is chosen ([`choose`]): by the words heard in common, its
//!    language, name, and length. The user can pick another, or paste lyrics ([`Choice`]); the
//!    words are then lined up again from what was gathered ([`Gathered`]), asking no one.
//! 5. The two are put together ([`combine`]), the word times locked onto the song's lead vocal
//!    ([`refine`]), sung stretches found ([`vocals`]), the words
//!    split into syllables and mouth shapes ([`syllables`]), and all of it written as Lyrics,
//!    Lyrics (words), Lyrics (syllables), Lyrics (phonemes), and Vocals timing tracks
//!    ([`tracks`]).
//!
//! **On-device alignment** ([`forced`]), when the user turned it on and its model is
//! downloaded: once the words are known, each line is lined up with the song letter by letter
//! on this computer, and words, syllables, and mouth shapes are timed from it (what the aligner
//! is unsure of keeps the timing above). It needs only the text, so with published English
//! lyrics the song's audio isn't sent to OpenAI at all; OpenAI then only hears the words of
//! songs with none published.
//!
//! What LRCLIB and OpenAI answered, and the user's choice, are kept by the song file's hash
//! ([`cache`]), so the same song is never sent twice, until the user asks to Find again.

pub mod cache;
pub mod choose;
pub mod combine;
pub mod forced;
pub mod gate;
pub mod language;
pub mod lrc;
pub mod lrclib;
pub mod refine;
pub mod syllables;
pub mod tracks;
pub mod transcribe;
pub mod vocals;

pub use cache::LyricsCache;
pub use choose::Candidate;
pub use combine::{Phrase, Word, WordSource};
pub use gate::{LyricsGate, gate};

use crate::error::AiError;
use crate::provider::Cancel;
use crate::secret::ApiKey;
use lrclib::{LookupError, Lrclib, Published, SongQuery};
use pf_analysis::{VOCAL_TRACK_FORMAT, VocalTrack};
use pf_audio::SongTags;
use pf_sequence::TimingTrack;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::Path;
use transcribe::{Heard, Hint, Transcriber};

/// What's being done, for the progress line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Step {
    ReadingSong,
    LookingUp,
    Listening,
    LiningUp,
    /// Bringing the voice forward for on-device alignment.
    Separating,
    /// Hearing the song letter by letter for on-device alignment.
    Aligning,
}

impl Step {
    pub fn label(self) -> &'static str {
        match self {
            Step::ReadingSong => "Reading the song",
            Step::LookingUp => "Looking up published lyrics",
            Step::Listening => "Sending the audio to OpenAI to hear the words",
            Step::LiningUp => "Lining up the words",
            Step::Separating => "Separating the vocals",
            Step::Aligning => "Aligning the words",
        }
    }
}

/// Reads a song's lead vocal (see [`pf_analysis::vocal_track`]), telling the last argument how
/// far it has got (0–1); tests use a stand-in.
pub type VoiceReader = dyn Fn(&Path, &Cancel, &dyn Fn(f32)) -> Result<VocalTrack, String> + Send + Sync;

/// Reads a song's tags (see [`pf_audio::read_tags`]).
pub type TagReader = dyn Fn(&Path) -> Option<SongTags> + Send + Sync;

/// Writes a line to the app's log: what went wrong, in more detail than the user is told.
pub type Logger = dyn Fn(&str) + Send + Sync;

/// Who's asked, and how.
pub struct Services {
    pub lrclib: Lrclib,
    pub transcriber: Transcriber,
    pub voice: Box<VoiceReader>,
    pub tags: Box<TagReader>,
    pub log: Box<Logger>,
}

impl Services {
    /// The real thing: LRCLIB and OpenAI over HTTPS, the song's own tags and voice. Nothing is
    /// logged until `log` is set.
    pub fn live() -> Self {
        Self {
            lrclib: Lrclib::new(std::sync::Arc::new(crate::http::UreqTransport::quick())),
            transcriber: Transcriber::new(std::sync::Arc::new(crate::http::UreqTransport::new())),
            voice: Box::new(|path, cancel, progress| {
                pf_analysis::vocal_track_file(path, &|| cancel.is_cancelled(), progress)
                    .map_err(|e| e.to_string())
            }),
            tags: Box::new(|path| pf_audio::read_tags(path).ok()),
            log: Box::new(|_| {}),
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
    /// The user's Lyrics language (ISO 639-1), for when neither the published lyrics nor the
    /// song's tags say.
    pub language: &'a str,
    /// Find again: what's kept for the song (and the user's choice for it) isn't used.
    pub fresh: bool,
    /// Told how far a step that reads the whole song has got (0–1), for a progress bar.
    pub progress: &'a dyn Fn(f32),
    /// The on-device aligner, when the user turned it on and its model is here ([`forced`]).
    pub aligner: Option<&'a dyn forced::SongAligner>,
}

/// Lyrics the user chose instead of the ones picked for them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Choice {
    /// One of the candidates, by its LRCLIB id.
    Candidate(i64),
    /// Lyrics the user pasted: plain lines, or LRC.
    Pasted(String),
}

/// The most pasted lyrics taken.
pub const MAX_PASTED_BYTES: usize = 64 * 1024;

/// Where the text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TextFrom {
    Lrclib,
    Openai,
    Pasted,
}

/// Where the word times came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TimingFrom {
    /// OpenAI's speech recognition.
    Openai,
    /// The lyrics' own word stamps (LRCLIB's, or pasted).
    LrclibWords,
    /// The lyrics' line times, words shared out by syllables.
    LrclibLines,
    /// Lined up on this computer ([`forced`]).
    OnDevice,
}

/// A candidate as the user sees it, to pick another.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateView {
    pub id: i64,
    pub artist: String,
    pub title: String,
    pub duration_s: f64,
    /// Its language's name ("English"), when it can be told.
    pub language: Option<String>,
    pub synced: bool,
}

impl From<&Candidate> for CandidateView {
    fn from(c: &Candidate) -> Self {
        Self {
            id: c.entry.id,
            artist: c.entry.artist.clone(),
            title: c.entry.title.clone(),
            duration_s: c.entry.duration_s,
            language: c.language.as_deref().map(language::name),
            synced: c.entry.synced.is_some(),
        }
    }
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
    /// "Lyrics: Lantern Band — Lantern Song (LRCLIB) · word timing: OpenAI"
    pub source: String,
    /// Anything worth knowing that didn't stop it ("OpenAI couldn't be used: …").
    pub notes: Vec<String>,
    /// Words whose timing is a guess (confidence under 0.5).
    pub unsure_words: usize,
    /// The published lyrics that could be the song, best first.
    pub candidates: Vec<CandidateView>,
    /// The candidate whose lyrics were used.
    pub chosen: Option<i64>,
    /// Whether the user's pasted lyrics were used.
    pub pasted: bool,
    /// How the word times were locked onto the voice ("Word timing locked to the vocals
    /// (average shift 120 ms)."), when they were.
    pub timing_note: Option<String>,
    /// What locking did, in numbers.
    #[serde(skip)]
    pub locked: Option<refine::Report>,
    /// What on-device alignment did, in numbers, when it ran.
    #[serde(skip)]
    pub aligned: Option<forced::Report>,
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

/// What was gathered for a song: LRCLIB's candidates and what the recognizer heard, to be put
/// together ([`assemble`]), and again with another [`Choice`] without asking anyone.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Gathered {
    /// The song file's hash, when it could be read.
    pub hash: Option<String>,
    /// Best first, at most [`choose::KEPT`].
    pub candidates: Vec<Candidate>,
    pub heard: Option<Heard>,
    /// The user's choice for this song.
    pub choice: Option<Choice>,
    pub notes: Vec<String>,
}

impl Gathered {
    /// Uses `choice` from now on, and keeps it for the song.
    pub fn choose(&mut self, choice: Choice, cache: Option<&LyricsCache>) -> Result<(), String> {
        match &choice {
            Choice::Pasted(text) if text.trim().is_empty() => {
                return Err("Paste the song's lyrics first.".into());
            }
            Choice::Pasted(text) if text.len() > MAX_PASTED_BYTES => {
                return Err("That's too long for one song's lyrics.".into());
            }
            Choice::Candidate(id) if !self.candidates.iter().any(|c| c.entry.id == *id) => {
                return Err("Those lyrics aren't among the ones found. Find lyrics again.".into());
            }
            _ => {}
        }
        if let (Some(hash), Some(cache)) = (&self.hash, cache) {
            cache.store(hash, CHOICE, &choice);
        }
        self.choice = Some(choice);
        Ok(())
    }
}

const LRCLIB: &str = "lrclib";
const OPENAI: &str = "openai";
const CHOICE: &str = "choice";

const NO_LYRICS: &str = "No lyrics found for this song.";
const NOT_ALIGNED: &str =
    "These lyrics have no times, and they couldn't be lined up with the song on this computer.";
/// Said when LRCLIB couldn't be asked but the recognizer heard the words.
pub const LRCLIB_UNAVAILABLE: &str =
    "LRCLIB (published lyrics) was unavailable, so the words come from speech recognition only.";

/// What's kept for the song of `kind`, unless finding it afresh.
fn kept<T: DeserializeOwned>(request: &Request<'_>, hash: Option<&str>, kind: &str) -> Option<T> {
    if request.fresh {
        return None;
    }
    hash.zip(request.cache).and_then(|(h, c)| c.load(h, kind))
}

/// An [`AiError`]'s words for the log: the provider's own when there are any.
fn log_detail(error: &AiError) -> String {
    error.details().map_or_else(|| error.to_string(), str::to_string)
}

/// Why nothing heard is used.
enum Unheard {
    /// The recognizer couldn't be used (a sentence for the user).
    Failed(String),
    /// It heard the words in another language (its name), twice.
    Elsewhere(String),
}

/// What the recognizer heard, from the cache (when told the same language) or OpenAI, checked
/// to be in the language it was told.
fn hear(
    services: &Services,
    request: &Request<'_>,
    key: &ApiKey,
    hash: Option<&str>,
    hint: &Hint,
) -> Result<Heard, Unheard> {
    if let Some(heard) = kept::<Heard>(request, hash, OPENAI)
        && heard.language.as_deref() == Some(hint.language.as_str())
    {
        return Ok(heard);
    }
    let cancel = request.cancel;
    let uploads = transcribe::uploads_for(
        request.path,
        request.sections_ms,
        &|| cancel.is_cancelled(),
        request.progress,
    )
    .map_err(Unheard::Failed)?;
    let ask = |hint: &Hint| {
        services
            .transcriber
            .transcribe(key, &uploads, hint, cancel)
            .map_err(|error| {
                (services.log)(&format!("OpenAI speech recognition: {}", log_detail(&error)));
                Unheard::Failed(error.to_string())
            })
    };
    let mut heard = ask(hint)?;
    if !language::fits(&hint.language, &heard.text()) {
        (services.log)(&format!(
            "OpenAI speech recognition heard {} when told {}; asking again without the prompt",
            language::detect(&heard.text()).unwrap_or("another language"),
            hint.language
        ));
        let plain = Hint {
            language: hint.language.clone(),
            prompt: None,
        };
        heard = ask(&plain)?;
        if !language::fits(&hint.language, &heard.text()) {
            let found = language::detect(&heard.text()).unwrap_or("another language");
            (services.log)(&format!(
                "OpenAI speech recognition heard {found} again when told {}; not used",
                hint.language
            ));
            return Err(Unheard::Elsewhere(language::name(found)));
        }
    }
    if let (Some(hash), Some(cache)) = (hash, request.cache) {
        cache.store(hash, OPENAI, &heard);
    }
    Ok(heard)
}

/// Looks the song's lyrics up and, with the recognizer, hears its words, calling `on_step` as
/// it goes. The `Err` is a plain sentence for the user.
pub fn gather(
    services: &Services,
    request: &Request<'_>,
    on_step: &mut dyn FnMut(Step),
) -> Result<Gathered, String> {
    let cancel = request.cancel;
    let stopped = || "Stopped.".to_string();
    on_step(Step::ReadingSong);
    let tags = (services.tags)(request.path).unwrap_or_default();
    let hash = cache::file_hash(request.path, &|| cancel.is_cancelled());
    cancel.check().map_err(|_| stopped())?;
    let hash_ref = hash.as_deref();
    if request.fresh
        && let (Some(hash), Some(cache)) = (hash_ref, request.cache)
    {
        cache.remove(hash, CHOICE);
    }
    let choice: Option<Choice> = kept(request, hash_ref, CHOICE);
    let query = song_query(&tags, request.duration_ms);

    on_step(Step::LookingUp);
    let mut notes = Vec::new();
    let mut lookup_failed = None;
    let entries: Vec<Published> = match (&query, kept::<Vec<Published>>(request, hash_ref, LRCLIB)) {
        (None, _) => Vec::new(),
        (Some(_), Some(entries)) => entries,
        (Some(query), None) => match services.lrclib.candidates(query, cancel) {
            Ok(entries) => {
                if let (Some(hash), Some(cache), false) = (hash_ref, request.cache, entries.is_empty()) {
                    cache.store(hash, LRCLIB, &entries);
                }
                entries
            }
            Err(LookupError::Cancelled) => return Err(stopped()),
            Err(error) => {
                (services.log)(&error.detail());
                lookup_failed = Some(error);
                Vec::new()
            }
        },
    };
    if let Some(error) = &lookup_failed
        && request.recognizer.is_none()
    {
        // Without the recognizer there's nothing else to go by.
        return Err(error.to_string());
    }
    let query = query.unwrap_or_default();

    // The language to hear: the chosen or best published lyrics', else the tags', else the
    // user's setting.
    let fallback = tags
        .language
        .as_deref()
        .and_then(language::code)
        .unwrap_or_else(|| request.language.to_string());
    let first_pick = choose::rank(&query, entries.clone(), &fallback, None);
    let picked = match &choice {
        Some(Choice::Candidate(id)) => first_pick.iter().find(|c| c.entry.id == *id),
        _ => first_pick.first(),
    };
    let expected = match &choice {
        Some(Choice::Pasted(text)) => language::detect(text).map(str::to_string),
        _ => picked.and_then(|c| c.language.clone()),
    }
    .unwrap_or(fallback);
    let has_lines = entries.iter().any(|e| e.synced.is_some());
    // With the on-device aligner, published (or pasted) English words need nothing heard: the
    // song's audio stays here.
    let has_text = match &choice {
        Some(Choice::Pasted(text)) => !text.trim().is_empty(),
        _ => picked.is_some_and(|c| {
            (c.entry.synced.is_some() || c.entry.plain.is_some())
                && c.language.as_deref().is_none_or(|l| l == expected)
        }),
    };
    let aligned_here = request.aligner.is_some() && has_text && expected == forced::LANGUAGE;

    let heard = match &request.recognizer {
        // What was heard before is still used (it's here already); nothing is sent.
        Some(_) if aligned_here => kept::<Heard>(request, hash_ref, OPENAI)
            .filter(|h| h.language.as_deref() == Some(expected.as_str()) && !h.words.is_empty()),
        Some(key) => {
            on_step(Step::Listening);
            let first_line = picked
                .filter(|c| c.language.as_deref().is_none_or(|l| l == expected))
                .map(|c| choose::lyrics_text(&c.entry))
                .and_then(|text| {
                    text.lines()
                        .map(str::trim)
                        .find(|l| !l.is_empty())
                        .map(str::to_string)
                });
            let hint = Hint {
                prompt: transcribe::prompt(
                    tags.title.as_deref(),
                    tags.artist.as_deref(),
                    first_line.as_deref(),
                ),
                language: expected.clone(),
            };
            match hear(services, request, key, hash_ref, &hint) {
                Ok(heard) if !heard.words.is_empty() => Some(heard),
                Ok(_) => None,
                Err(_) if cancel.is_cancelled() => return Err(stopped()),
                Err(Unheard::Elsewhere(found)) => {
                    let wanted = language::name(&expected);
                    let told = format!(
                        "OpenAI's speech recognition heard the words in {found}, not {wanted}. If the song is sung in {found}, choose it as the Lyrics language in Settings → AI, then use Find again."
                    );
                    if !has_lines {
                        return Err(told);
                    }
                    notes.push(format!("{told} Its word timing wasn't used."));
                    None
                }
                // Published lines still work without it.
                Err(Unheard::Failed(error)) if has_lines => {
                    notes.push(format!("OpenAI couldn't be used: {error}"));
                    None
                }
                Err(Unheard::Failed(error)) => return Err(error),
            }
        }
        None => None,
    };
    cancel.check().map_err(|_| stopped())?;
    if let Some(error) = lookup_failed {
        if heard.is_none() {
            return Err(error.to_string());
        }
        notes.push(LRCLIB_UNAVAILABLE.into());
    }
    Ok(Gathered {
        hash,
        candidates: choose::rank(&query, entries, &expected, heard.as_ref()),
        heard,
        choice,
        notes,
    })
}

/// What the song's lead vocal is kept under in the cache.
fn voice_kind() -> String {
    format!("voice{VOCAL_TRACK_FORMAT}")
}

/// The song's lead vocal ([`VocalTrack`]): kept by the song file's `hash` once worked out.
/// `None` when the song can't be read (or it's stopped).
pub fn read_voice(
    services: &Services,
    path: &Path,
    hash: Option<&str>,
    cache: Option<&LyricsCache>,
    cancel: &Cancel,
    progress: &dyn Fn(f32),
) -> Option<VocalTrack> {
    let kind = voice_kind();
    if let (Some(hash), Some(cache)) = (hash, cache)
        && let Some(track) = cache
            .load_bytes(hash, &kind)
            .and_then(|b| VocalTrack::from_bytes(&b))
    {
        return Some(track);
    }
    let track = (services.voice)(path, cancel, progress).ok()?;
    if let (Some(hash), Some(cache), false) = (hash, cache, track.is_empty()) {
        cache.store_bytes(hash, &kind, &track.to_bytes());
    }
    Some(track)
}

/// The source line: what the words and their timing came from.
fn source_line(text_from: TextFrom, timing_from: TimingFrom, chosen: Option<&Candidate>) -> String {
    let text = match (text_from, chosen) {
        (TextFrom::Lrclib, Some(c)) if c.entry.artist.is_empty() => {
            format!("Lyrics: {} (LRCLIB)", c.entry.title)
        }
        (TextFrom::Lrclib, Some(c)) => format!("Lyrics: {} — {} (LRCLIB)", c.entry.artist, c.entry.title),
        (TextFrom::Pasted, _) => "Lyrics: pasted".to_string(),
        _ => "Lyrics: OpenAI speech recognition".to_string(),
    };
    let own = if text_from == TextFrom::Pasted {
        "pasted"
    } else {
        "LRCLIB"
    };
    let timing = match timing_from {
        TimingFrom::Openai => "word timing: OpenAI".to_string(),
        TimingFrom::LrclibWords => format!("word timing: {own}"),
        TimingFrom::LrclibLines => format!("line timing: {own}"),
        TimingFrom::OnDevice => "word timing: on this computer".to_string(),
    };
    format!("{text} · {timing}")
}

/// Lines of plain lyrics with no times yet (all at 0), their words to be placed by the aligner.
fn untimed(lines: &[String]) -> Vec<Phrase> {
    lines
        .iter()
        .map(|line| Phrase {
            text: line.clone(),
            start_ms: 0,
            end_ms: 0,
            words: combine::split_words(line)
                .into_iter()
                .map(|text| Word {
                    text,
                    start_ms: 0,
                    end_ms: 0,
                    source: WordSource::Spread,
                    confidence: 0.0,
                    sung: None,
                })
                .collect(),
        })
        .filter(|p| !p.words.is_empty())
        .collect()
}

/// The song heard letter by letter by `aligner` ([`forced`]): kept by the song file's hash and
/// the model, worked out (bringing the voice forward, then hearing it) when not kept yet.
fn hear_letters(
    request: &Request<'_>,
    aligner: &dyn forced::SongAligner,
    hash: Option<&str>,
    on_step: &mut dyn FnMut(Step),
) -> Result<pf_align::Emission, String> {
    let kind = forced::cache_kind(aligner.model());
    if let (Some(hash), Some(cache)) = (hash, request.cache)
        && let Some(kept) = cache
            .load_bytes(hash, &kind)
            .and_then(|b| pf_align::Emission::from_bytes(&b))
    {
        return Ok(kept);
    }
    on_step(Step::Separating);
    let voice = aligner.voice(request.path, request.cancel, request.progress)?;
    request.cancel.check().map_err(|_| "Stopped.".to_string())?;
    on_step(Step::Aligning);
    let emission = aligner.hear(&voice, request.cancel, request.progress)?;
    if let (Some(hash), Some(cache), false) = (hash, request.cache, emission.frames() == 0) {
        cache.store_bytes(hash, &kind, &emission.to_bytes());
    }
    Ok(emission)
}

/// Puts what was `gathered` together as timing tracks: the user's choice's lyrics (or the best
/// candidate's), lined up with what was heard. Reads only the song file (for its voice).
pub fn assemble(
    services: &Services,
    request: &Request<'_>,
    gathered: &Gathered,
    on_step: &mut dyn FnMut(Step),
) -> Result<Found, String> {
    let cancel = request.cancel;
    let stopped = || "Stopped.".to_string();
    let pasted = match &gathered.choice {
        Some(Choice::Pasted(text)) => Some(text.as_str()),
        _ => None,
    };
    let chosen = match &gathered.choice {
        Some(Choice::Pasted(_)) => None,
        Some(Choice::Candidate(id)) => gathered
            .candidates
            .iter()
            .find(|c| c.entry.id == *id)
            .or(gathered.candidates.first()),
        None => gathered.candidates.first(),
    };
    let (synced, plain) = match pasted {
        Some(text) if lrc::parse_lrc(text).iter().any(|l| !l.text.is_empty()) => (Some(text), None),
        Some(text) => (None, Some(text)),
        None => (
            chosen.and_then(|c| c.entry.synced.as_deref()),
            chosen.and_then(|c| c.entry.plain.as_deref()),
        ),
    };
    let own = if pasted.is_some() {
        TextFrom::Pasted
    } else {
        TextFrom::Lrclib
    };
    let synced = synced
        .map(lrc::parse_lrc)
        .map(|lines| combine::timed_lines(&lines, request.duration_ms))
        .filter(|lines| !lines.is_empty());
    let plain = plain.map(lrc::plain_lines).filter(|lines| !lines.is_empty());
    let heard = gathered.heard.as_ref();

    on_step(Step::LiningUp);
    let voice = read_voice(
        services,
        request.path,
        gathered.hash.as_deref(),
        request.cache,
        cancel,
        request.progress,
    );
    cancel.check().map_err(|_| stopped())?;
    let onsets = voice.as_ref().map_or(&[][..], |v| &v.activity.onsets[..]);
    let end = request.duration_ms;
    let (phrases, text_from, timing_from) = match (&synced, &plain, heard) {
        (Some(lines), _, Some(heard)) => (
            combine::from_lines_and_heard(lines, heard, end),
            own,
            TimingFrom::Openai,
        ),
        (Some(lines), _, None) => {
            let stamped = lines.iter().all(|l| !l.stamped.is_empty());
            let timing = if stamped {
                TimingFrom::LrclibWords
            } else {
                TimingFrom::LrclibLines
            };
            (combine::from_lines(lines, onsets), own, timing)
        }
        (None, Some(lines), Some(heard)) => {
            let phrases = combine::from_plain_and_heard(lines, heard, end);
            if phrases.is_empty() {
                (combine::from_heard(heard), TextFrom::Openai, TimingFrom::Openai)
            } else {
                (phrases, own, TimingFrom::Openai)
            }
        }
        (None, None, Some(heard)) => (combine::from_heard(heard), TextFrom::Openai, TimingFrom::Openai),
        // No times at all: the aligner places the words across the whole song.
        (None, Some(lines), None) if request.aligner.is_some() => (untimed(lines), own, TimingFrom::OnDevice),
        (None, Some(_), None) if pasted.is_some() => {
            return Err(
                "Pasted lyrics without times need OpenAI's speech recognition to hear when they're sung. Paste LRC lyrics (with [mm:ss] times), or set up an OpenAI key for the assistant.".into(),
            );
        }
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
    // No times to go by: only the aligner can place the words.
    let whole_song = timing_from == TimingFrom::OnDevice;
    // Word times locked onto the voice: line times only spread onto it, the rest moved to it.
    let locked = voice
        .as_ref()
        .filter(|v| !v.is_empty() && !whole_song)
        .map(|v| match timing_from {
            TimingFrom::LrclibLines => refine::spread_onto_voice(&phrases, v, end),
            _ => refine::lock_to_voice(&phrases, v, end),
        });
    let (mut phrases, locked) = match locked {
        Some(l) => (l.phrases, Some(l.report)),
        None => (phrases, None),
    };
    // Lined up on this computer, letter by letter, when the aligner is here and hears the
    // lyrics' language.
    let mut notes = gathered.notes.clone();
    let mut timing_from = timing_from;
    let mut sounds = Vec::new();
    let mut aligned = None;
    let text: String = phrases
        .iter()
        .map(|p| p.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let english = language::detect(&text).is_none_or(|l| l == forced::LANGUAGE);
    match request.aligner {
        Some(aligner) if english => match hear_letters(request, aligner, gathered.hash.as_deref(), on_step) {
            Ok(emission) => {
                let voice = voice.as_ref().filter(|v| !v.is_empty());
                let done = forced::align(&phrases, &emission, voice, end, whole_song);
                if whole_song && done.report.aligned == 0 {
                    return Err(NOT_ALIGNED.into());
                }
                if whole_song || done.report.mostly_aligned() {
                    timing_from = TimingFrom::OnDevice;
                }
                phrases = done.phrases;
                sounds = done.sounds;
                aligned = Some(done.report);
            }
            Err(_) if cancel.is_cancelled() => return Err(stopped()),
            Err(error) => {
                (services.log)(&format!("on-device alignment: {error}"));
                if whole_song {
                    return Err(NOT_ALIGNED.into());
                }
                notes.push(
                    "On-device alignment couldn't be used this time, so the word timing is as found.".into(),
                );
            }
        },
        Some(_) if whole_song => return Err(NOT_ALIGNED.into()),
        Some(_) => {
            notes.push("On-device alignment hears English only, so these words' timing is as found.".into())
        }
        None => {}
    }
    cancel.check().map_err(|_| stopped())?;
    let words: Vec<(u64, u64)> = phrases
        .iter()
        .flat_map(|p| &p.words)
        .map(|w| (w.start_ms, w.end_ms))
        .collect();
    let mut vocals = vocals::regions_from_words(&words);
    if let Some(voice) = &voice {
        vocals = vocals::refine(&vocals, &voice.activity, end);
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
        (TextFrom::Pasted, TimingFrom::Openai) => "Your pasted lyrics, word timing from OpenAI.",
        (TextFrom::Pasted, TimingFrom::LrclibWords) => "Your pasted lyrics and their word timing.",
        (TextFrom::Pasted, TimingFrom::LrclibLines) => {
            "Your pasted lyrics and their line timing; words are spread over each line."
        }
        (TextFrom::Lrclib, TimingFrom::OnDevice) => "Lyrics from LRCLIB, word timing found on this computer.",
        (TextFrom::Pasted, TimingFrom::OnDevice) => "Your pasted lyrics, word timing found on this computer.",
        (TextFrom::Openai, TimingFrom::OnDevice) => "Lyrics from OpenAI, word timing found on this computer.",
        (TextFrom::Openai, _) if gathered.candidates.is_empty() => {
            "Lyrics and word timing from OpenAI (LRCLIB had none for this song)."
        }
        (TextFrom::Openai, _) => "Lyrics and word timing from OpenAI.",
    }
    .to_string();
    let chosen = chosen.filter(|_| text_from == TextFrom::Lrclib);
    let tracks = tracks::lyric_tracks_timed(&phrases, &vocals, onsets, end, &sounds);
    let timing_note = aligned
        .as_ref()
        .and_then(forced::Report::sentence)
        .or_else(|| locked.as_ref().and_then(refine::Report::sentence));
    Ok(Found {
        phrases,
        vocals,
        text_from,
        timing_from,
        summary,
        source: source_line(text_from, timing_from, chosen),
        notes,
        unsure_words,
        candidates: gathered.candidates.iter().map(CandidateView::from).collect(),
        chosen: chosen.map(|c| c.entry.id),
        pasted: text_from == TextFrom::Pasted,
        timing_note,
        locked,
        aligned,
        tracks,
    })
}

/// Finds the song's lyrics and word times ([`gather`], then [`assemble`]), calling `on_step` as
/// it goes. The `Err` is a plain sentence for the user.
pub fn find_lyrics(
    services: &Services,
    request: &Request<'_>,
    on_step: &mut dyn FnMut(Step),
) -> Result<Found, String> {
    let gathered = gather(services, request, on_step)?;
    assemble(services, request, &gathered, on_step)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::RetryPolicy;
    use crate::testing::{FakeTransport, Reply, fake_key};
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Made-up lyrics.
    const SYNCED: &str = "[00:01.00]Paper lanterns glowing\n[00:04.00]Snowy rooftops shine\n[00:07.00]";
    /// Made-up words, in Russian.
    const RUSSIAN: &str = "[00:01.00]Привет молоко привет\n[00:04.00]Молоко и снег\n[00:07.00]";

    fn record(id: i64, artist: &str, synced: Option<&str>, plain: Option<&str>) -> serde_json::Value {
        json!({
            "id": id, "trackName": "Lantern Song", "artistName": artist, "albumName": "Made Up",
            "duration": 10.0, "instrumental": false, "plainLyrics": plain, "syncedLyrics": synced,
        })
    }

    fn entry(synced: Option<&str>, plain: Option<&str>) -> String {
        json!([record(1, "Lantern Band", synced, plain)]).to_string()
    }

    const HEARD: &str = r#"{"text": "paper lanterns glowing snowy rooftops shine", "words": [
        {"word": "paper", "start": 1.2, "end": 1.6}, {"word": "lanterns", "start": 1.6, "end": 2.3},
        {"word": "glowing", "start": 2.3, "end": 3.1}, {"word": "snowy", "start": 4.1, "end": 4.6},
        {"word": "rooftops", "start": 4.6, "end": 5.3}, {"word": "shine", "start": 5.3, "end": 6.4}]}"#;

    /// What the recognizer might hear when left to guess the language: made-up Russian words.
    const HEARD_RUSSIAN: &str = r#"{"text": "привет молоко привет", "words": [
        {"word": "Привет", "start": 1.2, "end": 1.6}, {"word": "молоко", "start": 1.6, "end": 2.3},
        {"word": "привет", "start": 2.3, "end": 3.1}]}"#;

    struct Fakes {
        services: Services,
        lrclib: Arc<FakeTransport>,
        openai: Arc<FakeTransport>,
        log: Arc<Mutex<Vec<String>>>,
    }

    fn fakes(lrclib: Vec<Reply>, openai: Vec<Reply>) -> Fakes {
        let lrclib_fake = Arc::new(FakeTransport::new(lrclib));
        let openai_fake = Arc::new(FakeTransport::new(openai));
        let log = Arc::new(Mutex::new(Vec::new()));
        let logged = log.clone();
        let services = Services {
            lrclib: Lrclib::new(lrclib_fake.clone()).with_retry_delay(Duration::ZERO),
            transcriber: Transcriber::new(openai_fake.clone()).with_retry(RetryPolicy::immediate()),
            voice: Box::new(|_, _, _| Ok(VocalTrack::default())),
            tags: Box::new(|_| {
                Some(SongTags {
                    title: Some("Lantern Song".into()),
                    duration_ms: Some(10_000),
                    title_from_file_name: true,
                    ..SongTags::default()
                })
            }),
            log: Box::new(move |line| logged.lock().unwrap().push(line.to_string())),
        };
        Fakes {
            services,
            lrclib: lrclib_fake,
            openai: openai_fake,
            log,
        }
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
            language: language::DEFAULT,
            fresh: false,
            progress: &|_| {},
            aligner: None,
        }
    }

    /// The text fields of a multipart request.
    fn form(fake: &FakeTransport, index: usize) -> String {
        String::from_utf8_lossy(fake.requests()[index].body.as_ref().unwrap().as_bytes()).into_owned()
    }

    #[test]
    fn published_lines_and_heard_words_together() {
        let (_dir, path) = song();
        let f = fakes(vec![Reply::ok(entry(Some(SYNCED), None))], vec![Reply::ok(HEARD)]);
        let mut steps = Vec::new();
        let cancel = Cancel::new();
        let found = find_lyrics(
            &f.services,
            &request(&path, Some(fake_key()), None, &cancel),
            &mut |s| steps.push(s),
        )
        .unwrap();
        assert_eq!(found.summary, "Lyrics from LRCLIB, word timing from OpenAI.");
        assert_eq!(
            found.source,
            "Lyrics: Lantern Band — Lantern Song (LRCLIB) · word timing: OpenAI"
        );
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
        assert_eq!(found.tracks.len(), 5);
        assert_eq!(found.tracks[1].marks.len(), 6);
        assert_eq!(found.chosen, Some(1));
        assert_eq!(found.candidates[0].language.as_deref(), None);
        // No artist in the tags: a search by title; the audio went to OpenAI once.
        assert!(
            f.lrclib.requests()[0]
                .url
                .contains("/api/search?q=Lantern%20Song")
        );
        assert_eq!(f.openai.requests().len(), 1);
    }

    #[test]
    fn the_recognizer_is_told_the_language_and_given_a_prompt() {
        let (_dir, path) = song();
        let cancel = Cancel::new();
        // Nothing published: the user's Lyrics language, and the song's title for a prompt.
        let f = fakes(vec![Reply::ok("[]")], vec![Reply::ok(HEARD)]);
        find_lyrics(
            &f.services,
            &request(&path, Some(fake_key()), None, &cancel),
            &mut |_| {},
        )
        .unwrap();
        let body = form(&f.openai, 0);
        assert!(body.contains("name=\"language\"\r\n\r\nen\r\n"), "{body}");
        assert!(
            body.contains("name=\"prompt\"\r\n\r\nLantern Song.\r\n"),
            "{body}"
        );

        // Published Russian lyrics: Russian, and their first line in the prompt.
        let f = fakes(
            vec![Reply::ok(
                json!([record(2, "Cover Band", Some(RUSSIAN), None)]).to_string(),
            )],
            vec![Reply::ok(HEARD_RUSSIAN)],
        );
        let found = find_lyrics(
            &f.services,
            &request(&path, Some(fake_key()), None, &cancel),
            &mut |_| {},
        )
        .unwrap();
        let body = form(&f.openai, 0);
        assert!(body.contains("name=\"language\"\r\n\r\nru\r\n"), "{body}");
        assert!(
            body.contains("name=\"prompt\"\r\n\r\nLantern Song. Привет молоко привет\r\n"),
            "{body}"
        );
        assert_eq!(found.candidates[0].language.as_deref(), Some("Russian"));

        // Nothing published, the song tagged Spanish: Spanish.
        let mut f = fakes(vec![Reply::ok("[]")], vec![Reply::ok(HEARD)]);
        f.services.tags = Box::new(|_| {
            Some(SongTags {
                title: Some("Lantern Song".into()),
                language: Some("spa".into()),
                ..SongTags::default()
            })
        });
        let _ = find_lyrics(
            &f.services,
            &request(&path, Some(fake_key()), None, &cancel),
            &mut |_| {},
        );
        assert!(form(&f.openai, 0).contains("name=\"language\"\r\n\r\nes\r\n"));
    }

    #[test]
    fn words_heard_in_another_alphabet_are_asked_for_again_then_refused() {
        let (_dir, path) = song();
        let cancel = Cancel::new();
        // Cyrillic when English was expected: asked again, without the prompt, and heard right.
        let f = fakes(
            vec![Reply::ok("[]")],
            vec![Reply::ok(HEARD_RUSSIAN), Reply::ok(HEARD)],
        );
        let found = find_lyrics(
            &f.services,
            &request(&path, Some(fake_key()), None, &cancel),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(found.phrases[0].words[0].text, "paper");
        assert_eq!(f.openai.requests().len(), 2);
        let again = form(&f.openai, 1);
        assert!(again.contains("name=\"language\"\r\n\r\nen\r\n"));
        assert!(!again.contains("name=\"prompt\""));
        assert!(f.log.lock().unwrap()[0].contains("heard ru when told en"));

        // Cyrillic twice: nothing published to fall back on, so the user is told and no tracks
        // are made.
        let f = fakes(
            vec![Reply::ok("[]")],
            vec![Reply::ok(HEARD_RUSSIAN), Reply::ok(HEARD_RUSSIAN)],
        );
        let error = find_lyrics(
            &f.services,
            &request(&path, Some(fake_key()), None, &cancel),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(
            error.starts_with("OpenAI's speech recognition heard the words in Russian, not English."),
            "{error}"
        );
        assert!(error.contains("Lyrics language in Settings → AI"), "{error}");

        // With published lines, those are used, and the note says why the timing isn't heard.
        let f = fakes(
            vec![Reply::ok(entry(Some(SYNCED), None))],
            vec![Reply::ok(HEARD_RUSSIAN), Reply::ok(HEARD_RUSSIAN)],
        );
        let found = find_lyrics(
            &f.services,
            &request(&path, Some(fake_key()), None, &cancel),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(found.timing_from, TimingFrom::LrclibLines);
        assert!(
            found.notes[0].contains("heard the words in Russian"),
            "{:?}",
            found.notes
        );
    }

    #[test]
    fn lrclib_is_tried_again_and_when_it_stays_down_the_words_are_heard() {
        let (_dir, path) = song();
        let cancel = Cancel::new();
        let f = fakes(
            vec![Reply::status(500, ""), Reply::ok(entry(Some(SYNCED), None))],
            vec![Reply::ok(HEARD)],
        );
        let found = find_lyrics(
            &f.services,
            &request(&path, Some(fake_key()), None, &cancel),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(found.text_from, TextFrom::Lrclib);
        assert!(found.notes.is_empty(), "{:?}", found.notes);
        assert_eq!(f.lrclib.requests().len(), 2);

        let f = fakes(vec![Reply::status(500, ""); 3], vec![Reply::ok(HEARD)]);
        let found = find_lyrics(
            &f.services,
            &request(&path, Some(fake_key()), None, &cancel),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(f.lrclib.requests().len(), 3);
        assert_eq!(found.text_from, TextFrom::Openai);
        assert_eq!(found.notes, [LRCLIB_UNAVAILABLE]);
        assert_eq!(
            found.source,
            "Lyrics: OpenAI speech recognition · word timing: OpenAI"
        );
        // The status is in the log, not shown.
        assert!(found.notes.iter().all(|n| !n.contains("500")));
        assert_eq!(f.log.lock().unwrap().as_slice(), ["LRCLIB: HTTP 500"]);

        // Without the recognizer: a plain sentence, no status code.
        let f = fakes(vec![Reply::status(503, ""); 3], vec![]);
        let error = find_lyrics(&f.services, &request(&path, None, None, &cancel), &mut |_| {}).unwrap_err();
        assert_eq!(
            error,
            "LRCLIB, the published lyrics library, isn't working right now. Try again later."
        );
    }

    #[test]
    fn the_original_is_chosen_over_a_cover_in_another_language() {
        let (_dir, path) = song();
        let cancel = Cancel::new();
        let both = json!([
            record(2, "Cover Band", Some(RUSSIAN), None),
            record(1, "Lantern Band", Some(SYNCED), None),
        ])
        .to_string();
        // Without the recognizer: English expected.
        let f = fakes(vec![Reply::ok(both.clone())], vec![]);
        let found = find_lyrics(&f.services, &request(&path, None, None, &cancel), &mut |_| {}).unwrap();
        assert_eq!(found.chosen, Some(1));
        // Heard in English: the words in common too.
        let f = fakes(vec![Reply::ok(both)], vec![Reply::ok(HEARD)]);
        let found = find_lyrics(
            &f.services,
            &request(&path, Some(fake_key()), None, &cancel),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(found.chosen, Some(1));
        assert_eq!(found.candidates.len(), 2);
        assert_eq!(found.candidates[1].language.as_deref(), Some("Russian"));
    }

    #[test]
    fn another_candidate_or_pasted_lyrics_line_up_again_asking_no_one() {
        let (dir, path) = song();
        let cache = LyricsCache::new(dir.path());
        let cancel = Cancel::new();
        let both = json!([
            record(1, "Lantern Band", Some(SYNCED), None),
            record(
                3,
                "Other Band",
                Some("[00:02.00]Rooftop waltz is turning\n[00:05.00]Round and round"),
                None
            ),
        ])
        .to_string();
        let f = fakes(vec![Reply::ok(both)], vec![Reply::ok(HEARD)]);
        let req = request(&path, Some(fake_key()), Some(&cache), &cancel);
        let mut gathered = gather(&f.services, &req, &mut |_| {}).unwrap();
        let asked = (f.lrclib.requests().len(), f.openai.requests().len());
        assert_eq!(
            assemble(&f.services, &req, &gathered, &mut |_| {})
                .unwrap()
                .chosen,
            Some(1)
        );

        gathered.choose(Choice::Candidate(3), Some(&cache)).unwrap();
        let found = assemble(&f.services, &req, &gathered, &mut |_| {}).unwrap();
        assert_eq!(found.chosen, Some(3));
        assert_eq!(found.phrases[0].text, "Rooftop waltz is turning");
        assert!(
            found
                .source
                .starts_with("Lyrics: Other Band — Lantern Song (LRCLIB)")
        );

        // Pasted plain lyrics: lined up with what was heard.
        gathered
            .choose(
                Choice::Pasted("Paper lanterns glowing\nSnowy rooftops shine".into()),
                Some(&cache),
            )
            .unwrap();
        let found = assemble(&f.services, &req, &gathered, &mut |_| {}).unwrap();
        assert!(found.pasted);
        assert_eq!(found.chosen, None);
        assert_eq!(found.source, "Lyrics: pasted · word timing: OpenAI");
        assert_eq!(found.phrases[1].text, "Snowy rooftops shine");
        assert_eq!(found.phrases[1].start_ms, 4_100);
        assert_eq!(
            (f.lrclib.requests().len(), f.openai.requests().len()),
            asked,
            "no one was asked again"
        );
        assert!(gathered.choose(Choice::Pasted(" ".into()), None).is_err());
        assert!(gathered.choose(Choice::Candidate(99), None).is_err());

        // The choice is kept for the song: found again, the pasted lyrics are used, from the cache.
        let f = fakes(vec![], vec![]);
        let found = find_lyrics(&f.services, &req, &mut |_| {}).unwrap();
        assert!(found.pasted);
        assert!(f.lrclib.requests().is_empty() && f.openai.requests().is_empty());
    }

    #[test]
    fn without_the_recognizer_words_share_their_lines() {
        let (_dir, path) = song();
        let f = fakes(vec![Reply::ok(entry(Some(SYNCED), None))], vec![]);
        let cancel = Cancel::new();
        let found = find_lyrics(&f.services, &request(&path, None, None, &cancel), &mut |_| {}).unwrap();
        assert_eq!(found.timing_from, TimingFrom::LrclibLines);
        assert!(found.summary.contains("line timing from LRCLIB"));
        assert_eq!(
            found.source,
            "Lyrics: Lantern Band — Lantern Song (LRCLIB) · line timing: LRCLIB"
        );
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
        assert!(f.openai.requests().is_empty());
    }

    #[test]
    fn no_published_lyrics_falls_back_to_the_recognizer_or_says_so() {
        let (_dir, path) = song();
        let cancel = Cancel::new();
        let f = fakes(vec![Reply::ok("[]")], vec![Reply::ok(HEARD)]);
        let found = find_lyrics(
            &f.services,
            &request(&path, Some(fake_key()), None, &cancel),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(found.text_from, TextFrom::Openai);
        assert_eq!(found.phrases[0].words[0].text, "paper");

        let f = fakes(vec![Reply::ok("[]")], vec![]);
        let error = find_lyrics(&f.services, &request(&path, None, None, &cancel), &mut |_| {}).unwrap_err();
        assert_eq!(error, "No lyrics found for this song.");

        // Plain lyrics alone can't be timed.
        let f = fakes(vec![Reply::ok(entry(None, Some("Paper lanterns")))], vec![]);
        let error = find_lyrics(&f.services, &request(&path, None, None, &cancel), &mut |_| {}).unwrap_err();
        assert!(error.contains("not when they're sung"), "{error}");
    }

    #[test]
    fn answers_are_cached_by_the_song_file_until_found_again() {
        let (dir, path) = song();
        let cache = LyricsCache::new(dir.path());
        let cancel = Cancel::new();
        let f = fakes(vec![Reply::ok(entry(Some(SYNCED), None))], vec![Reply::ok(HEARD)]);
        find_lyrics(
            &f.services,
            &request(&path, Some(fake_key()), Some(&cache), &cancel),
            &mut |_| {},
        )
        .unwrap();
        // Again: nobody is asked.
        let f = fakes(vec![], vec![]);
        let found = find_lyrics(
            &f.services,
            &request(&path, Some(fake_key()), Some(&cache), &cancel),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(found.timing_from, TimingFrom::Openai);
        assert!(f.lrclib.requests().is_empty() && f.openai.requests().is_empty());
        // Find again: both are asked afresh.
        let f = fakes(vec![Reply::ok(entry(Some(SYNCED), None))], vec![Reply::ok(HEARD)]);
        let mut fresh = request(&path, Some(fake_key()), Some(&cache), &cancel);
        fresh.fresh = true;
        find_lyrics(&f.services, &fresh, &mut |_| {}).unwrap();
        assert_eq!((f.lrclib.requests().len(), f.openai.requests().len()), (1, 1));
    }

    #[test]
    fn what_an_earlier_version_kept_is_ignored() {
        let (dir, path) = song();
        let cache = LyricsCache::new(dir.path());
        let cancel = Cancel::new();
        // As an earlier version kept them: Russian words heard, under the old names.
        let hash = cache::file_hash(&path, &|| false).unwrap();
        std::fs::create_dir_all(dir.path().join("lyrics")).unwrap();
        let old = transcribe::parse_verbose_json(HEARD_RUSSIAN, 0).unwrap();
        std::fs::write(
            dir.path().join(format!("lyrics/{hash}-openai.json")),
            serde_json::to_string(&old).unwrap(),
        )
        .unwrap();
        let f = fakes(vec![Reply::ok("[]")], vec![Reply::ok(HEARD)]);
        let found = find_lyrics(
            &f.services,
            &request(&path, Some(fake_key()), Some(&cache), &cancel),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(found.phrases[0].words[0].text, "paper");
        assert_eq!(f.openai.requests().len(), 1);
        // A kept answer heard in another language than now expected is asked for again.
        let f = fakes(vec![Reply::ok("[]")], vec![Reply::ok(HEARD_RUSSIAN)]);
        let mut russian = request(&path, Some(fake_key()), Some(&cache), &cancel);
        russian.language = "ru";
        russian.fresh = false;
        let found = find_lyrics(&f.services, &russian, &mut |_| {}).unwrap();
        assert_eq!(found.phrases[0].words[0].text, "Привет");
        assert_eq!(f.openai.requests().len(), 1);
    }

    /// An aligner that hears the made-up song's words where SYNCED puts them, a little late,
    /// counting how often it listens.
    struct FakeAligner {
        listened: Mutex<usize>,
        fails: bool,
    }

    impl FakeAligner {
        fn new() -> Self {
            Self {
                listened: Mutex::new(0),
                fails: false,
            }
        }
    }

    impl forced::SongAligner for FakeAligner {
        fn model(&self) -> &str {
            "fake-1"
        }

        fn voice(&self, _: &Path, _: &Cancel, progress: &dyn Fn(f32)) -> Result<Vec<f32>, String> {
            *self.listened.lock().unwrap() += 1;
            progress(1.0);
            if self.fails {
                return Err("the model file is damaged".into());
            }
            Ok(vec![0.0; 16])
        }

        fn hear(&self, _: &[f32], _: &Cancel, _: &dyn Fn(f32)) -> Result<pf_align::Emission, String> {
            // 10 s; each line heard 300 ms after its published time.
            let mut path = vec![pf_align::vocab::BLANK; 500];
            forced::tests::heard(&mut path, 65, "paper lanterns glowing");
            forced::tests::heard(&mut path, 215, "snowy rooftops shine");
            Ok(forced::tests::emission_for(&path))
        }
    }

    #[test]
    fn with_the_aligner_published_lyrics_are_timed_here_and_nothing_is_sent() {
        let (dir, path) = song();
        let cache = LyricsCache::new(dir.path());
        let cancel = Cancel::new();
        let aligner = FakeAligner::new();
        // An OpenAI key and the user's say-so, but published lines: the audio isn't sent.
        let f = fakes(vec![Reply::ok(entry(Some(SYNCED), None))], vec![]);
        let mut req = request(&path, Some(fake_key()), Some(&cache), &cancel);
        req.aligner = Some(&aligner);
        let mut steps = Vec::new();
        let found = find_lyrics(&f.services, &req, &mut |s| steps.push(s)).unwrap();
        assert!(f.openai.requests().is_empty());
        assert_eq!(found.timing_from, TimingFrom::OnDevice);
        assert_eq!(
            found.summary,
            "Lyrics from LRCLIB, word timing found on this computer."
        );
        assert_eq!(
            found.source,
            "Lyrics: Lantern Band — Lantern Song (LRCLIB) · word timing: on this computer"
        );
        assert_eq!(
            steps,
            [
                Step::ReadingSong,
                Step::LookingUp,
                Step::LiningUp,
                Step::Separating,
                Step::Aligning
            ]
        );
        // Each word where its letters were heard: "Paper" at step 65.
        let words: Vec<&Word> = found.phrases.iter().flat_map(|p| &p.words).collect();
        assert_eq!(words[0].start_ms, 1_300);
        assert_eq!(found.phrases[1].start_ms, 4_300);
        assert!(words.iter().all(|w| w.source == WordSource::Aligned));
        assert_eq!(found.unsure_words, 0);
        assert_eq!(found.aligned.as_ref().unwrap().aligned, 6);
        assert!(
            found
                .timing_note
                .unwrap()
                .starts_with("Word timing found on this computer for 6 of 6 words")
        );
        // Syllables and mouth shapes from the letters: "Paper" is two syllables, the second
        // from its second p (step 69).
        let syllables = &found.tracks[2].marks;
        assert_eq!((syllables[0].start_ms, syllables[1].start_ms), (1_300, 1_380));
        assert!(!found.tracks[3].marks.is_empty());

        // Again (another candidate chosen, say): what was heard is kept, nothing listened to.
        let f = fakes(vec![], vec![]);
        find_lyrics(&f.services, &req, &mut |_| {}).unwrap();
        assert_eq!(*aligner.listened.lock().unwrap(), 1);
    }

    #[test]
    fn with_the_aligner_plain_lyrics_need_no_times() {
        let (_dir, path) = song();
        let cancel = Cancel::new();
        let aligner = FakeAligner::new();
        let plain = "Paper lanterns glowing\nSnowy rooftops shine";
        let f = fakes(vec![Reply::ok(entry(None, Some(plain)))], vec![]);
        let mut req = request(&path, None, None, &cancel);
        req.aligner = Some(&aligner);
        let found = find_lyrics(&f.services, &req, &mut |_| {}).unwrap();
        assert_eq!(found.timing_from, TimingFrom::OnDevice);
        assert_eq!(found.phrases[0].start_ms, 1_300);
        assert_eq!(found.phrases[1].start_ms, 4_300);
        // Without the aligner, as before: they can't be timed.
        let f = fakes(vec![Reply::ok(entry(None, Some(plain)))], vec![]);
        let error = find_lyrics(&f.services, &request(&path, None, None, &cancel), &mut |_| {}).unwrap_err();
        assert!(error.contains("not when they're sung"), "{error}");
    }

    #[test]
    fn with_no_published_lyrics_openai_hears_them_and_the_aligner_times_them() {
        let (_dir, path) = song();
        let cancel = Cancel::new();
        let aligner = FakeAligner::new();
        let f = fakes(vec![Reply::ok("[]")], vec![Reply::ok(HEARD)]);
        let mut req = request(&path, Some(fake_key()), None, &cancel);
        req.aligner = Some(&aligner);
        let found = find_lyrics(&f.services, &req, &mut |_| {}).unwrap();
        assert_eq!(f.openai.requests().len(), 1);
        assert_eq!(found.text_from, TextFrom::Openai);
        assert_eq!(found.timing_from, TimingFrom::OnDevice);
        assert_eq!(found.phrases[0].words[0].start_ms, 1_300);
    }

    #[test]
    fn an_aligner_that_fails_leaves_the_timing_as_found() {
        let (_dir, path) = song();
        let cancel = Cancel::new();
        let aligner = FakeAligner {
            fails: true,
            ..FakeAligner::new()
        };
        let f = fakes(vec![Reply::ok(entry(Some(SYNCED), None))], vec![]);
        let mut req = request(&path, None, None, &cancel);
        req.aligner = Some(&aligner);
        let found = find_lyrics(&f.services, &req, &mut |_| {}).unwrap();
        assert_eq!(found.timing_from, TimingFrom::LrclibLines);
        assert_eq!(found.phrases[0].start_ms, 1_000);
        assert!(
            found.notes[0].starts_with("On-device alignment couldn't be used"),
            "{:?}",
            found.notes
        );
        assert!(f.log.lock().unwrap().iter().any(|l| l.contains("damaged")));
    }

    #[test]
    fn a_failed_recognizer_still_leaves_published_lines_and_stop_stops() {
        let (_dir, path) = song();
        let cancel = Cancel::new();
        let f = fakes(
            vec![Reply::ok(entry(Some(SYNCED), None))],
            vec![Reply::status(401, "{}")],
        );
        let found = find_lyrics(
            &f.services,
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
        assert!(!f.log.lock().unwrap().is_empty());

        // LRCLIB unreachable and no recognizer: the reason.
        let f = fakes(vec![Reply::Unreachable], vec![]);
        let error = find_lyrics(&f.services, &request(&path, None, None, &cancel), &mut |_| {}).unwrap_err();
        assert!(error.contains("Couldn't reach LRCLIB"), "{error}");

        let stopped = Cancel::new();
        stopped.cancel();
        let f = fakes(vec![], vec![]);
        let error = find_lyrics(&f.services, &request(&path, None, None, &stopped), &mut |_| {}).unwrap_err();
        assert_eq!(error, "Stopped.");
        assert!(f.lrclib.requests().is_empty());
    }
}
