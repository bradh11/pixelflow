//! What a music file says about itself: artist, title, and album from its tags (ID3 in MP3s,
//! the `ilst` atoms in M4As, Vorbis comments in FLAC and Ogg), and how long it plays. A file
//! without tags is named by its file name instead ("04 - Lantern Song.mp3" is "Lantern Song").

use crate::error::AudioError;
use std::fs::File;
use std::path::Path;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;

/// Longest tag value kept (longer ones are cut).
const MAX_TAG_CHARS: usize = 200;

/// A song's name and where it came from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SongTags {
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    /// The language its words are sung in, as tagged (ID3's TLAN is a three-letter ISO 639-2
    /// code, "eng").
    pub language: Option<String>,
    /// How long the file plays, when its header says.
    pub duration_ms: Option<u64>,
    /// Whether the title came from the file name rather than a tag.
    pub title_from_file_name: bool,
}

fn clean(value: &str) -> Option<String> {
    let text: String = value
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TAG_CHARS)
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn take_tags(revision: &MetadataRevision, tags: &mut SongTags) {
    for tag in revision.tags() {
        let slot = match tag.std_key {
            Some(StandardTagKey::Artist) => &mut tags.artist,
            Some(StandardTagKey::TrackTitle) => &mut tags.title,
            Some(StandardTagKey::Album) => &mut tags.album,
            Some(StandardTagKey::Language) => &mut tags.language,
            _ => continue,
        };
        if slot.is_none() {
            *slot = clean(&tag.value.to_string());
        }
    }
}

/// Reads a music file's tags and length. The file is only read; a file with no tags at all
/// still gets a title from its name.
pub fn read_tags(path: &Path) -> Result<SongTags, AudioError> {
    let shown = path.display().to_string();
    let file = File::open(path).map_err(|source| AudioError::Open {
        path: shown.clone(),
        source,
    })?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }
    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| AudioError::Decode {
            path: shown,
            reason: e.to_string(),
        })?;
    let mut tags = SongTags::default();
    // Tags ahead of the audio (ID3v2) come with the probe; the rest from the container.
    if let Some(revision) = probed.metadata.get().as_ref().and_then(|m| m.current()) {
        take_tags(revision, &mut tags);
    }
    if let Some(revision) = probed.format.metadata().current() {
        take_tags(revision, &mut tags);
    }
    tags.duration_ms = probed.format.default_track().and_then(|track| {
        let params = &track.codec_params;
        let frames = params.n_frames?;
        let rate = u64::from(params.sample_rate?);
        (rate > 0).then(|| frames * 1000 / rate)
    });
    if tags.title.is_none() {
        tags.title = title_from_file_name(path);
        tags.title_from_file_name = tags.title.is_some();
    }
    Ok(tags)
}

/// A title from a file name: the name without its extension, a leading track number
/// ("04 - ", "04. ", "04_"), or an "Artist - " before it when there's one dash.
pub fn title_from_file_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy().replace('_', " ");
    let mut text = stem.trim();
    // A leading track number and what separates it from the name.
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if (1..=3).contains(&digits) {
        let rest = &text[digits..];
        let trimmed = rest.trim_start_matches([' ', '-', '.', ')']);
        if trimmed.len() < rest.len() && !trimmed.is_empty() {
            text = trimmed;
        }
    }
    // "Artist - Title": the part after the dash.
    if let Some((_, title)) = text.split_once(" - ")
        && !title.contains(" - ")
    {
        text = title;
    }
    clean(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> Option<String> {
        title_from_file_name(Path::new(name))
    }

    #[test]
    fn file_names_give_a_title() {
        assert_eq!(named("04 - Lantern Song.mp3").as_deref(), Some("Lantern Song"));
        assert_eq!(named("12. Jingle Bells.m4a").as_deref(), Some("Jingle Bells"));
        assert_eq!(named("lanterns.mp3").as_deref(), Some("lanterns"));
        assert_eq!(named("Band - Song Name.flac").as_deref(), Some("Song Name"));
        assert_eq!(named("01_Silent_Night.wav").as_deref(), Some("Silent Night"));
        // A number that is the name stays.
        assert_eq!(named("1999.mp3").as_deref(), Some("1999"));
        assert_eq!(named("7.mp3").as_deref(), Some("7"));
    }

    #[test]
    fn a_wav_without_tags_is_named_by_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("03 - Made Up Song.wav");
        std::fs::write(&path, crate::wav::wav_bytes(&[0.0; 1600], 16_000)).unwrap();
        let tags = read_tags(&path).unwrap();
        assert_eq!(tags.title.as_deref(), Some("Made Up Song"));
        assert!(tags.title_from_file_name);
        assert_eq!(tags.artist, None);
        assert_eq!(tags.duration_ms, Some(100));
    }

    #[test]
    fn tag_values_are_one_tidy_line() {
        assert_eq!(clean("  Lantern Song\n").as_deref(), Some("Lantern Song"));
        assert_eq!(clean(" \t "), None);
        assert_eq!(clean(&"x".repeat(500)).map(|s| s.len()), Some(MAX_TAG_CHARS));
    }
}
