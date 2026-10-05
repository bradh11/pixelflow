//! Timing files: xLights' `.xtiming` (XML: `<timing name=…><EffectLayer><Effect label=…
//! starttime=… endtime=…/>…`, or several in `<timings>`) and Audacity label files (`.txt`), read
//! into timing tracks and written back out.

use crate::XlightsError;
use crate::sequence::unxml_safe;
use crate::xml;
use pf_sequence::{MAX_TEXT_LEN, MAX_TIMING_FILE_BYTES, Mark, TidyReport, TimingKind, TimingTrack};
use std::path::Path;

/// Timing tracks read from a file, and what didn't come across.
#[derive(Debug, Clone, PartialEq)]
pub struct TimingFileImport {
    pub tracks: Vec<TimingTrack>,
    pub notes: Vec<String>,
}

/// What a track's name says it marks ("Beats", "Song bars", "Lyrics"…), else custom.
pub fn kind_for_name(name: &str) -> TimingKind {
    let lower = name.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).collect();
    let has = |options: &[&str]| words.iter().any(|w| options.contains(w));
    if has(&["beat", "beats"]) {
        TimingKind::Beats
    } else if has(&["bar", "bars", "measure", "measures"]) {
        TimingKind::Bars
    } else if has(&["word", "words"]) {
        TimingKind::Words
    } else if has(&["phoneme", "phonemes"]) {
        TimingKind::Phonemes
    } else if has(&["lyric", "lyrics", "phrase", "phrases", "vocals", "singing"]) {
        TimingKind::Lyrics
    } else if has(&["section", "sections", "verse", "chorus"]) {
        TimingKind::Sections
    } else {
        TimingKind::Custom
    }
}

fn bad(file: &str, reason: impl Into<String>) -> XlightsError {
    XlightsError::BadTimingFile(file.to_string(), reason.into())
}

fn bounded(text: &str) -> String {
    text.chars().take(MAX_TEXT_LEN).collect()
}

/// Tidies a track's marks for a sequence `duration_ms` long, adding up what was left out.
fn tidy(marks: Vec<Mark>, duration_ms: u64, total: &mut TidyReport) -> Vec<Mark> {
    let (kept, report) = pf_sequence::tidy_marks(marks, duration_ms);
    total.no_length += report.no_length;
    total.overlapping += report.overlapping;
    total.outside += report.outside;
    total.cut += report.cut;
    kept
}

/// Reads an xLights `.xtiming` file (`file` names it in messages) for a sequence `duration_ms`
/// long. A lyrics timing with words and phonemes (three layers) becomes three tracks.
pub fn parse_xtiming(text: &str, duration_ms: u64, file: &str) -> Result<TimingFileImport, XlightsError> {
    if text.len() > MAX_TIMING_FILE_BYTES {
        return Err(bad(file, too_big(text.len())));
    }
    let doc = xml::parse(text).map_err(|reason| bad(file, reason))?;
    let root = doc.root_element();
    let timings: Vec<_> = match root.tag_name().name() {
        "timing" => vec![root],
        "timings" => root
            .children()
            .filter(|c| c.is_element() && c.tag_name().name() == "timing")
            .collect(),
        _ => return Err(bad(file, "it isn't an xLights timing file (no <timing> in it)")),
    };
    if timings.is_empty() {
        return Err(bad(file, "it has no timing tracks in it"));
    }
    let mut tracks = Vec::new();
    let mut report = TidyReport::default();
    let mut unreadable = 0;
    for timing in timings {
        let name = bounded(unxml_safe(timing.attribute("name").unwrap_or("").trim()).trim());
        let name = if name.is_empty() {
            "Timing".to_string()
        } else {
            name
        };
        let layers: Vec<Vec<Mark>> = timing
            .children()
            .filter(|c| c.is_element() && c.tag_name().name() == "EffectLayer")
            .map(|layer| {
                layer
                    .children()
                    .filter(|c| c.is_element() && c.tag_name().name() == "Effect")
                    .filter_map(|e| {
                        let time = |a| e.attribute(a).and_then(|t: &str| t.trim().parse::<u64>().ok());
                        let (Some(start), Some(end)) = (time("starttime"), time("endtime")) else {
                            unreadable += 1;
                            return None;
                        };
                        let label = bounded(unxml_safe(e.attribute("label").unwrap_or("")).trim());
                        Some(Mark::new(start, end, label))
                    })
                    .collect()
            })
            .collect();
        let lyric = layers.len() >= 2;
        for (i, marks) in layers.into_iter().enumerate() {
            if i > 0 && marks.is_empty() {
                continue;
            }
            let (kind, track_name) = match (lyric, i) {
                (false, _) => (kind_for_name(&name), name.clone()),
                (true, 0) => (TimingKind::Lyrics, name.clone()),
                (true, 1) => (TimingKind::Words, format!("{name} (words)")),
                (true, 2) => (TimingKind::Phonemes, format!("{name} (phonemes)")),
                _ => (TimingKind::Custom, format!("{name} layer {}", i + 1)),
            };
            let marks = tidy(marks, duration_ms, &mut report);
            tracks.push(TimingTrack::new(bounded(&track_name), kind, marks));
        }
    }
    let mut notes = report.notes(file);
    if unreadable > 0 {
        notes.insert(
            0,
            format!(
                "{} in {file} had times PixelFlow couldn't read and {} left out.",
                if unreadable == 1 {
                    "1 mark".to_string()
                } else {
                    format!("{unreadable} marks")
                },
                if unreadable == 1 { "was" } else { "were" }
            ),
        );
    }
    Ok(TimingFileImport { tracks, notes })
}

fn too_big(bytes: usize) -> String {
    format!(
        "it is {} MB; PixelFlow reads timing files up to {} MB",
        bytes / (1024 * 1024),
        MAX_TIMING_FILE_BYTES / (1024 * 1024)
    )
}

/// Reads an Audacity label file as one track named `name`.
pub fn parse_audacity(
    text: &str,
    duration_ms: u64,
    name: &str,
    file: &str,
) -> Result<TimingFileImport, XlightsError> {
    let marks = pf_sequence::parse_audacity_labels(text).map_err(|reason| bad(file, reason))?;
    let mut report = TidyReport::default();
    let marks = tidy(marks, duration_ms, &mut report);
    let name = bounded(name.trim());
    let name = if name.is_empty() {
        "Labels".to_string()
    } else {
        name
    };
    Ok(TimingFileImport {
        tracks: vec![TimingTrack::new(name.clone(), kind_for_name(&name), marks)],
        notes: report.notes(file),
    })
}

/// Reads a timing file by its extension: `.xtiming` (xLights) or `.txt` (Audacity labels, one
/// track named after the file), for a sequence `duration_ms` long.
pub fn read_timing_file(path: &Path, duration_ms: u64) -> Result<TimingFileImport, XlightsError> {
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let err = |e| XlightsError::Read(path.display().to_string(), e);
    let size = std::fs::metadata(path).map_err(err)?.len();
    if size > MAX_TIMING_FILE_BYTES as u64 {
        return Err(bad(&file, too_big(size as usize)));
    }
    let bytes = std::fs::read(path).map_err(err)?;
    let text = String::from_utf8_lossy(&bytes);
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "xtiming" | "xml" => parse_xtiming(&text, duration_ms, &file),
        "txt" | "lab" | "labels" => {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            parse_audacity(&text, duration_ms, &stem, &file)
        }
        _ => Err(bad(
            &file,
            "PixelFlow reads xLights timing files (.xtiming) and Audacity labels (.txt)",
        )),
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' | '\r' | '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// An `.xtiming` file as xLights writes it. Each entry is one `<timing>`: a track, followed by
/// the tracks that are its further layers (a lyrics track's words and phonemes). Several entries
/// go in `<timings>`.
pub fn xtiming(timings: &[Vec<&TimingTrack>]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let several = timings.len() > 1;
    if several {
        out.push_str("<timings>\n");
    }
    for layers in timings {
        let Some(first) = layers.first() else { continue };
        out.push_str(&format!(
            "<timing name=\"{}\" subType=\"Generic\" SourceVersion=\"PixelFlow {}\">\n",
            escape(&first.name),
            env!("CARGO_PKG_VERSION")
        ));
        for layer in layers {
            out.push_str("   <EffectLayer>\n");
            for m in &layer.marks {
                out.push_str(&format!(
                    "      <Effect label=\"{}\" starttime=\"{}\" endtime=\"{}\" />\n",
                    escape(&m.label),
                    m.start_ms,
                    m.end_ms
                ));
            }
            out.push_str("   </EffectLayer>\n");
        }
        out.push_str("</timing>\n");
    }
    if several {
        out.push_str("</timings>\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(track: &TimingTrack) -> Vec<(u64, u64, &str)> {
        track
            .marks
            .iter()
            .map(|m| (m.start_ms, m.end_ms, m.label.as_str()))
            .collect()
    }

    #[test]
    fn reads_an_xtiming_file_from_xlights() {
        let text = r#"<?xml version="1.0" encoding="UTF-8"?>
<timing name="Beat &amp; Bars" subType="Generic" SourceVersion="2024.1">
   <EffectLayer>
      <Effect label="1" starttime="0" endtime="500" />
      <Effect label="2" starttime="500" endtime="1000" />
   </EffectLayer>
</timing>"#;
        let import = parse_xtiming(text, 60_000, "beats.xtiming").unwrap();
        assert_eq!(import.tracks.len(), 1);
        let track = &import.tracks[0];
        assert_eq!(
            (track.name.as_str(), track.kind),
            ("Beat & Bars", TimingKind::Beats)
        );
        assert_eq!(spans(track), vec![(0, 500, "1"), (500, 1000, "2")]);
        assert!(import.notes.is_empty());
    }

    #[test]
    fn lyrics_with_words_and_phonemes_become_three_tracks() {
        let text = r#"<timings>
<timing name="Vocals" subType="Generic">
 <EffectLayer><Effect label="Hi there" starttime="0" endtime="1000"/></EffectLayer>
 <EffectLayer><Effect label="Hi" starttime="0" endtime="400"/><Effect label="there" starttime="400" endtime="1000"/></EffectLayer>
 <EffectLayer><Effect label="AI" starttime="0" endtime="400"/></EffectLayer>
</timing>
<timing name="Count"><EffectLayer><Effect label="one" starttime="0" endtime="10"/></EffectLayer></timing>
</timings>"#;
        let import = parse_xtiming(text, 60_000, "song.xtiming").unwrap();
        let names: Vec<(&str, TimingKind)> =
            import.tracks.iter().map(|t| (t.name.as_str(), t.kind)).collect();
        assert_eq!(
            names,
            vec![
                ("Vocals", TimingKind::Lyrics),
                ("Vocals (words)", TimingKind::Words),
                ("Vocals (phonemes)", TimingKind::Phonemes),
                ("Count", TimingKind::Custom)
            ]
        );
        assert_eq!(
            spans(&import.tracks[1]),
            vec![(0, 400, "Hi"), (400, 1000, "there")]
        );
    }

    #[test]
    fn xtiming_round_trips() {
        let lyrics = TimingTrack::new(
            "Song <1>",
            TimingKind::Lyrics,
            vec![Mark::new(0, 1500, "Rock & \"roll\"")],
        );
        let words = TimingTrack::new(
            "Song <1> (words)",
            TimingKind::Words,
            vec![Mark::new(0, 700, "Rock"), Mark::new(700, 1500, "roll")],
        );
        let beats = TimingTrack::new("Beats", TimingKind::Beats, vec![Mark::new(0, 500, "1")]);
        let text = xtiming(&[vec![&lyrics, &words], vec![&beats]]);
        assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<timings>\n<timing name=\"Song &lt;1&gt;\" subType=\"Generic\""));
        assert!(
            text.contains(
                "<Effect label=\"Rock &amp; &quot;roll&quot;\" starttime=\"0\" endtime=\"1500\" />"
            )
        );
        let back = parse_xtiming(&text, 60_000, "x.xtiming").unwrap();
        assert_eq!(back.tracks.len(), 3);
        for (read, wrote) in back.tracks.iter().zip([&lyrics, &words, &beats]) {
            assert_eq!(
                (&read.name, read.kind, &read.marks),
                (&wrote.name, wrote.kind, &wrote.marks)
            );
        }
        // One timing alone is written without <timings>, as xLights does.
        let single = xtiming(&[vec![&beats]]);
        assert!(!single.contains("<timings>"));
        assert_eq!(
            parse_xtiming(&single, 60_000, "b.xtiming").unwrap().tracks[0].marks,
            beats.marks
        );
    }

    #[test]
    fn malformed_xtiming_is_explained() {
        let msg = |text: &str| parse_xtiming(text, 10_000, "x.xtiming").unwrap_err().to_string();
        assert_eq!(
            msg("<sequence/>"),
            "x.xtiming isn't a timing file PixelFlow can read: it isn't an xLights timing file (no <timing> in it)"
        );
        assert!(
            msg("<timing name='a'><EffectLayer>")
                .starts_with("x.xtiming isn't a timing file PixelFlow can read: ")
        );
        assert_eq!(
            msg("<timings></timings>"),
            "x.xtiming isn't a timing file PixelFlow can read: it has no timing tracks in it"
        );
        let deep = format!("<timing>{}{}</timing>", "<a>".repeat(100), "</a>".repeat(100));
        assert!(msg(&deep).contains("nested more than 64 deep"));
        assert!(msg(&" ".repeat(MAX_TIMING_FILE_BYTES + 1)).contains("up to 16 MB"));
        // Marks that can't be used are left out, and the notes say so.
        let import = parse_xtiming(
            r#"<timing name="T"><EffectLayer>
<Effect label="a" starttime="0" endtime="1000"/>
<Effect label="bad" starttime="x" endtime="1000"/>
<Effect label="overlap" starttime="500" endtime="1500"/>
<Effect label="late" starttime="20000" endtime="21000"/>
</EffectLayer></timing>"#,
            10_000,
            "x.xtiming",
        )
        .unwrap();
        assert_eq!(spans(&import.tracks[0]), vec![(0, 1000, "a")]);
        assert_eq!(
            import.notes,
            vec![
                "1 mark in x.xtiming had times PixelFlow couldn't read and was left out.",
                "1 mark in x.xtiming overlapped the mark before and was left out.",
                "1 mark in x.xtiming started after the end of the sequence and was left out."
            ]
        );
    }

    #[test]
    fn timing_files_are_read_by_their_extension() {
        let dir = std::env::temp_dir().join(format!("pf-timing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let labels = dir.join("Lyrics.txt");
        std::fs::write(&labels, "0.0\t1.5\tSilent night\n1.5\t3.0\tholy night\n").unwrap();
        let import = read_timing_file(&labels, 60_000).unwrap();
        assert_eq!(
            (import.tracks[0].name.as_str(), import.tracks[0].kind),
            ("Lyrics", TimingKind::Lyrics)
        );
        assert_eq!(spans(&import.tracks[0])[1], (1500, 3000, "holy night"));
        let other = dir.join("song.mp3");
        std::fs::write(&other, "x").unwrap();
        assert_eq!(
            read_timing_file(&other, 60_000).unwrap_err().to_string(),
            "song.mp3 isn't a timing file PixelFlow can read: PixelFlow reads xLights timing files (.xtiming) and Audacity labels (.txt)"
        );
        assert!(
            read_timing_file(&dir.join("missing.xtiming"), 60_000)
                .unwrap_err()
                .to_string()
                .starts_with("Could not read")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn kinds_come_from_track_names() {
        assert_eq!(kind_for_name("Beats"), TimingKind::Beats);
        assert_eq!(kind_for_name("Song bars"), TimingKind::Bars);
        assert_eq!(kind_for_name("Barbara"), TimingKind::Custom);
        assert_eq!(kind_for_name("Lead vocals"), TimingKind::Lyrics);
        assert_eq!(kind_for_name("Lyrics (words)"), TimingKind::Words);
        assert_eq!(kind_for_name("Chorus"), TimingKind::Sections);
    }
}
