//! The words as sung, heard by OpenAI's speech recognition (`POST /v1/audio/transcriptions`,
//! `whisper-1`, `response_format=verbose_json`, word and segment timestamps), with the user's
//! OpenAI key, and only after they agreed to send the song's audio.
//!
//! The recognizer is always told the language (`language`, ISO 639-1): left to guess, it can
//! hear a sung English song as another language and write it in that one. A short `prompt` (the
//! song's name and its first published line, when known) steers its spelling; whisper-1 reads
//! only the last 224 tokens of one.
//!
//! Uploads are at most 25 MB. A song file under that in a format OpenAI takes goes as it is;
//! anything else is decoded to mono 16 kHz WAV (about 1.9 MB a minute), and a song still too
//! big for one upload (over ~13 minutes) is cut in parts, at section starts where it can be.
//! Each part's times are moved back to where the part starts in the song.

use crate::error::AiError;
use crate::http::{HeaderValue, HttpRequest, Method, RetryPolicy, Transport, send_with_retries};
use crate::provider::{Cancel, ProviderId};
use crate::secret::ApiKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

pub const MODEL: &str = "whisper-1";
/// OpenAI's upload limit.
pub const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;
/// What a file may be to go as it is, leaving room for the rest of the form.
const MAX_FILE_BYTES: usize = MAX_UPLOAD_BYTES - 64 * 1024;
/// The sample rate audio is brought down to.
pub const RATE: u32 = 16_000;
/// The largest reply read.
const MAX_REPLY: u64 = 16 * 1024 * 1024;
/// A part never ends sooner than this after it starts, to find a section start to cut at.
const MIN_PART_MS: u64 = 60_000;

/// One word as heard, with when it was sung.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeardWord {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// What the recognizer heard: the words, and the stretches it heard as one line (segments).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Heard {
    pub words: Vec<HeardWord>,
    pub lines: Vec<(u64, u64)>,
    /// The language it was told to hear.
    #[serde(default)]
    pub language: Option<String>,
}

impl Heard {
    /// The words heard, one after another.
    pub fn text(&self) -> String {
        self.words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// What the recognizer is told about the song.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    /// ISO 639-1 ("en").
    pub language: String,
    pub prompt: Option<String>,
}

/// The most of a prompt sent (whisper-1 reads only its last 224 tokens).
const MAX_PROMPT_CHARS: usize = 400;

/// A prompt for the recognizer: the song's title and artist, then its first published line
/// ("Lantern Song, by Lantern Band. Paper lanterns glowing"). `None` when nothing is known.
pub fn prompt(title: Option<&str>, artist: Option<&str>, first_line: Option<&str>) -> Option<String> {
    let mut text = match (title, artist) {
        (Some(title), Some(artist)) => format!("{title}, by {artist}."),
        (Some(title), None) => format!("{title}."),
        (None, Some(artist)) => format!("By {artist}."),
        (None, None) => String::new(),
    };
    if let Some(line) = first_line.map(str::trim).filter(|l| !l.is_empty()) {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(line);
    }
    let text: String = text
        .replace(['\r', '\n'], " ")
        .chars()
        .take(MAX_PROMPT_CHARS)
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn ms(seconds: &Value) -> Option<u64> {
    let s = seconds.as_f64()?;
    (s.is_finite() && s >= 0.0).then(|| (s * 1000.0).round() as u64)
}

/// The words and segments of a `verbose_json` reply, moved later by `offset_ms`.
pub fn parse_verbose_json(body: &str, offset_ms: u64) -> Option<Heard> {
    let value: Value = serde_json::from_str(body).ok()?;
    let words = value["words"]
        .as_array()?
        .iter()
        .filter_map(|w| {
            let text = w["word"].as_str()?.trim().to_string();
            let start = ms(&w["start"])?;
            let end = ms(&w["end"])?.max(start);
            (!text.is_empty()).then(|| HeardWord {
                text,
                start_ms: start + offset_ms,
                end_ms: end + offset_ms,
            })
        })
        .collect();
    let lines = value["segments"]
        .as_array()
        .map(|segments| {
            segments
                .iter()
                .filter_map(|s| Some((ms(&s["start"])? + offset_ms, ms(&s["end"])? + offset_ms)))
                .filter(|(s, e)| e > s)
                .collect()
        })
        .unwrap_or_default();
    Some(Heard {
        words,
        lines,
        language: None,
    })
}

/// Audio to send: a file's bytes, and where it starts in the song.
#[derive(Clone, PartialEq, Eq)]
pub struct Upload {
    pub file_name: String,
    pub mime: &'static str,
    pub bytes: Vec<u8>,
    pub offset_ms: u64,
}

impl std::fmt::Debug for Upload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Upload")
            .field("file_name", &self.file_name)
            .field("bytes", &self.bytes.len())
            .field("offset_ms", &self.offset_ms)
            .finish()
    }
}

/// The type OpenAI is told a file has, for the formats it takes as they are.
fn mime_for(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "mp3" | "mpga" | "mpeg" => "audio/mpeg",
        "m4a" | "mp4" => "audio/mp4",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        "webm" => "audio/webm",
        _ => return None,
    })
}

/// Where to cut `total` samples into parts of at most `most` samples: at section starts
/// (`cuts_ms`, in order) when one falls late enough in a part, else at the most a part holds.
pub fn part_bounds(total: usize, most: usize, cuts_ms: &[u64]) -> Vec<(usize, usize)> {
    let at = |ms: u64| (ms as u128 * u128::from(RATE) / 1000) as usize;
    let least = at(MIN_PART_MS).min(most);
    let mut bounds = Vec::new();
    let mut start = 0;
    while start < total {
        let limit = (start + most.max(1)).min(total);
        let end = if limit == total {
            total
        } else {
            cuts_ms
                .iter()
                .map(|&c| at(c))
                .rfind(|&c| c >= start + least && c <= limit)
                .unwrap_or(limit)
        };
        bounds.push((start, end));
        start = end;
    }
    bounds
}

/// The audio to send for a song: the file itself when it can go as it is, else mono 16 kHz WAV
/// in as few parts as fit, cut at `sections_ms` where it can be.
pub fn uploads_for(path: &Path, sections_ms: &[u64], stop: &dyn Fn() -> bool) -> Result<Vec<Upload>, String> {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(u64::MAX);
    if let Some(mime) = mime_for(path)
        && size <= MAX_FILE_BYTES as u64
    {
        let bytes = std::fs::read(path).map_err(|_| "PixelFlow couldn't read the song file.".to_string())?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "song".into());
        return Ok(vec![Upload {
            file_name: name,
            mime,
            bytes,
            offset_ms: 0,
        }]);
    }
    let samples = pf_audio::mono_at_rate(path, RATE, stop).map_err(|e| e.to_string())?;
    let most = (MAX_FILE_BYTES - 44) / 2;
    Ok(part_bounds(samples.len(), most, sections_ms)
        .into_iter()
        .enumerate()
        .map(|(i, (start, end))| Upload {
            file_name: format!("song-part-{}.wav", i + 1),
            mime: "audio/wav",
            bytes: pf_audio::wav_bytes(&samples[start..end], RATE),
            offset_ms: (start as u64 * 1000) / u64::from(RATE),
        })
        .collect())
}

/// A `multipart/form-data` body: the file, then the text fields.
pub fn multipart(boundary: &str, upload: &Upload, fields: &[(&str, &str)]) -> Vec<u8> {
    let mut body = Vec::with_capacity(upload.bytes.len() + 1024);
    let name = upload.file_name.replace(['"', '\r', '\n'], "_");
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: {}\r\n\r\n",
            upload.mime
        )
        .as_bytes(),
    );
    body.extend_from_slice(&upload.bytes);
    body.extend_from_slice(b"\r\n");
    for (field, value) in fields {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"\r\n\r\n{value}\r\n")
                .as_bytes(),
        );
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

/// Sends songs to OpenAI's speech recognition.
pub struct Transcriber {
    transport: Arc<dyn Transport>,
    base_url: String,
    retry: RetryPolicy,
}

impl Transcriber {
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self {
            transport,
            base_url: crate::openai::BASE_URL.to_string(),
            retry: RetryPolicy::default(),
        }
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// What OpenAI hears in each upload, in song time, told the language and given the prompt
    /// in `hint`.
    pub fn transcribe(
        &self,
        key: &ApiKey,
        uploads: &[Upload],
        hint: &Hint,
        cancel: &Cancel,
    ) -> Result<Heard, AiError> {
        let mut heard = Heard {
            language: Some(hint.language.clone()),
            ..Heard::default()
        };
        let mut fields = vec![
            ("model", MODEL),
            ("response_format", "verbose_json"),
            ("timestamp_granularities[]", "word"),
            ("timestamp_granularities[]", "segment"),
            ("language", hint.language.as_str()),
        ];
        if let Some(prompt) = &hint.prompt {
            fields.push(("prompt", prompt.as_str()));
        }
        for upload in uploads {
            cancel.check()?;
            let boundary = format!("pixelflow-{}", uuid::Uuid::new_v4().simple());
            let body = multipart(&boundary, upload, &fields);
            let request = HttpRequest {
                method: Method::Post,
                url: format!("{}/v1/audio/transcriptions", self.base_url),
                headers: vec![
                    (
                        "authorization",
                        HeaderValue::SecretWithPrefix("Bearer ", key.clone()),
                    ),
                    (
                        "content-type",
                        HeaderValue::Plain(format!("multipart/form-data; boundary={boundary}")),
                    ),
                ],
                body: Some(body.into()),
            };
            let response = send_with_retries(
                self.transport.as_ref(),
                &request,
                self.retry,
                cancel,
                &|reply| !reply.body.contains("insufficient_quota"),
                &mut |_, _| {},
            )
            .map_err(|e| match crate::openai::send_error(e, key, MODEL) {
                error if *error.root() == AiError::TooLong => AiError::Provider {
                    provider: ProviderId::Openai,
                    message: "the song's audio was too large to send".into(),
                },
                error => error,
            })?;
            let mut text = String::new();
            response
                .body
                .take(MAX_REPLY)
                .read_to_string(&mut text)
                .map_err(|_| AiError::Interrupted(ProviderId::Openai))?;
            let part = parse_verbose_json(&text, upload.offset_ms)
                .ok_or(AiError::BadResponse(ProviderId::Openai))?;
            heard.words.extend(part.words);
            heard.lines.extend(part.lines);
        }
        Ok(heard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FAKE_KEY, FakeTransport, Reply, fake_key};

    /// A reply as OpenAI sends it, with made-up words.
    const REPLY: &str = r#"{
        "task": "transcribe", "language": "english", "duration": 9.5,
        "text": "Paper lanterns glowing, snowy rooftops shine.",
        "segments": [
            {"id": 0, "seek": 0, "start": 1.0, "end": 3.2, "text": " Paper lanterns glowing,"},
            {"id": 1, "seek": 0, "start": 4.0, "end": 6.1, "text": " snowy rooftops shine."}
        ],
        "words": [
            {"word": "Paper", "start": 1.0, "end": 1.4},
            {"word": "lanterns", "start": 1.4, "end": 2.2},
            {"word": "glowing", "start": 2.2, "end": 3.2},
            {"word": " ", "start": 3.2, "end": 3.3},
            {"word": "snowy", "start": 4.0, "end": 4.5},
            {"word": "rooftops", "start": 4.5, "end": 5.3},
            {"word": "shine", "start": 5.3, "end": 6.1}
        ]
    }"#;

    #[test]
    fn verbose_json_words_and_segments_in_song_time() {
        let heard = parse_verbose_json(REPLY, 60_000).unwrap();
        let words: Vec<(&str, u64, u64)> = heard
            .words
            .iter()
            .map(|w| (w.text.as_str(), w.start_ms, w.end_ms))
            .collect();
        assert_eq!(
            words,
            [
                ("Paper", 61_000, 61_400),
                ("lanterns", 61_400, 62_200),
                ("glowing", 62_200, 63_200),
                ("snowy", 64_000, 64_500),
                ("rooftops", 64_500, 65_300),
                ("shine", 65_300, 66_100),
            ]
        );
        assert_eq!(heard.lines, [(61_000, 63_200), (64_000, 66_100)]);
        assert_eq!(parse_verbose_json("{}", 0), None);
        assert_eq!(parse_verbose_json("nope", 0), None);
    }

    #[test]
    fn prompts_name_the_song_and_its_first_line() {
        assert_eq!(
            prompt(Some("Lantern Song"), None, Some(" Paper lanterns\nglowing ")).as_deref(),
            Some("Lantern Song. Paper lanterns glowing")
        );
        assert_eq!(prompt(None, None, Some("  ")), None);
        let long = "la ".repeat(500);
        assert!(prompt(None, None, Some(&long)).unwrap().chars().count() <= MAX_PROMPT_CHARS);
    }

    #[test]
    fn parts_are_cut_at_section_starts_when_they_fit() {
        let second = RATE as usize;
        // 30 minutes, at most 13 minutes a part, sections every 2 minutes.
        let sections: Vec<u64> = (0..15).map(|i| i * 120_000).collect();
        let bounds = part_bounds(1800 * second, 780 * second, &sections);
        assert_eq!(
            bounds,
            [
                (0, 720 * second),
                (720 * second, 1440 * second),
                (1440 * second, 1800 * second)
            ]
        );
        // No sections: cut where a part is full.
        assert_eq!(
            part_bounds(1000 * second, 780 * second, &[]),
            [(0, 780 * second), (780 * second, 1000 * second)]
        );
        assert_eq!(part_bounds(10, 100, &[]), [(0, 10)]);
    }

    #[test]
    fn a_small_song_goes_as_it_is_and_a_wav_part_starts_with_its_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tune.wav");
        std::fs::write(&path, pf_audio::wav_bytes(&vec![0.1; 8_000], 8_000)).unwrap();
        let uploads = uploads_for(&path, &[], &|| false).unwrap();
        assert_eq!(uploads.len(), 1);
        assert_eq!(uploads[0].mime, "audio/wav");
        assert_eq!(uploads[0].bytes, std::fs::read(&path).unwrap());
        // A format OpenAI doesn't take is decoded to 16 kHz WAV.
        let odd = dir.path().join("tune.aiff-not");
        std::fs::copy(&path, &odd).unwrap();
        let uploads = uploads_for(&odd, &[], &|| false).unwrap();
        assert_eq!(uploads.len(), 1);
        assert!(uploads[0].bytes.starts_with(b"RIFF"));
        // One second at 16 kHz, 16-bit.
        assert_eq!(uploads[0].bytes.len(), 44 + 2 * RATE as usize);
    }

    #[test]
    fn the_request_is_a_multipart_form_with_the_key_only_in_its_header() {
        let fake = Arc::new(FakeTransport::new(vec![Reply::ok(REPLY), Reply::ok(REPLY)]));
        let transcriber = Transcriber::new(fake.clone()).with_retry(RetryPolicy::immediate());
        let uploads = [
            Upload {
                file_name: "song-part-1.wav".into(),
                mime: "audio/wav",
                bytes: b"RIFFfake".to_vec(),
                offset_ms: 0,
            },
            Upload {
                file_name: "song-part-2.wav".into(),
                mime: "audio/wav",
                bytes: b"RIFFfake".to_vec(),
                offset_ms: 10_000,
            },
        ];
        let hint = Hint {
            language: "en".into(),
            prompt: prompt(
                Some("Lantern Song"),
                Some("Lantern Band"),
                Some("Paper lanterns glowing"),
            ),
        };
        let heard = transcriber
            .transcribe(&fake_key(), &uploads, &hint, &Cancel::new())
            .unwrap();
        assert_eq!(heard.language.as_deref(), Some("en"));
        assert_eq!(heard.words.len(), 12);
        assert_eq!(heard.words[6].start_ms, 11_000);
        let requests = fake.requests();
        assert_eq!(requests[0].url, "https://api.openai.com/v1/audio/transcriptions");
        assert_eq!(
            requests[0].header("authorization").unwrap(),
            format!("Bearer {FAKE_KEY}")
        );
        let body = String::from_utf8_lossy(requests[0].body.as_ref().unwrap().as_bytes()).into_owned();
        assert!(!body.contains(FAKE_KEY));
        for part in [
            "name=\"file\"; filename=\"song-part-1.wav\"",
            "name=\"model\"\r\n\r\nwhisper-1\r\n",
            "name=\"response_format\"\r\n\r\nverbose_json\r\n",
            "name=\"timestamp_granularities[]\"\r\n\r\nword\r\n",
            "name=\"language\"\r\n\r\nen\r\n",
            "name=\"prompt\"\r\n\r\nLantern Song, by Lantern Band. Paper lanterns glowing\r\n",
        ] {
            assert!(body.contains(part), "{part}");
        }
        assert!(!format!("{:?}", requests[0]).contains("RIFFfake"));
    }

    #[test]
    fn openai_errors_are_plain() {
        let fake = Arc::new(FakeTransport::new(vec![Reply::status(
            401,
            r#"{"error":{"message":"Incorrect API key provided","type":"invalid_request_error","code":"invalid_api_key"}}"#,
        )]));
        let upload = Upload {
            file_name: "a.mp3".into(),
            mime: "audio/mpeg",
            bytes: vec![1, 2, 3],
            offset_ms: 0,
        };
        let hint = Hint {
            language: "en".into(),
            prompt: None,
        };
        let error = Transcriber::new(fake)
            .transcribe(&fake_key(), std::slice::from_ref(&upload), &hint, &Cancel::new())
            .unwrap_err();
        assert_eq!(*error.root(), AiError::InvalidKey(ProviderId::Openai));
        let fake = Arc::new(FakeTransport::new(vec![Reply::status(413, "{}")]));
        let error = Transcriber::new(fake)
            .transcribe(&fake_key(), &[upload], &hint, &Cancel::new())
            .unwrap_err();
        assert!(error.to_string().contains("too large"), "{error}");
    }
}
