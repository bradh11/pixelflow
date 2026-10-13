//! What a run leaves behind to look at: each scenario's final draft saved as a sequence file
//! beside the made-up stage it plays on (a show file, its picture, and the song's sound), so
//! both open in PixelFlow; and, drawn from those files as the app would open them, a video of the
//! draft and a contact sheet of a frame from every section, each labelled with the section, when
//! it is, and the effects playing then.
//!
//! Nothing here is sent anywhere: it is all written to the run's own folder (`--out`).

use super::{Outcome, effects};
use crate::knowledge::reference;
use crate::stage::{self, PICTURE, Stage};
use pf_engine::Engine;
use pf_sequence::Sequence;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Where in the run's folder the show and the drafts go, the videos, and the contact sheets.
pub const SHOW_FOLDER: &str = "show";
pub const VIDEO_FOLDER: &str = "videos";
pub const SHEET_FOLDER: &str = "sheets";
/// The stage's show file, and the song's sound beside it.
pub const SHOW_FILE: &str = "Paper Lanterns stage.pixelflow.json";
pub const SONG_SOUND: &str = "paper-lanterns.wav";

/// How the videos and contact sheets are drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendering {
    /// The video's size and frame rate, and where it stops (the song's end when `None`).
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub end_ms: Option<u64>,
    /// The size of each frame on the contact sheet.
    pub frame_width: u32,
    pub frame_height: u32,
}

impl Default for Rendering {
    /// The stage is about three times as wide as it is tall: pictures that shape, with little
    /// black above and below.
    fn default() -> Self {
        Self {
            width: 1280,
            height: 448,
            fps: 25,
            end_ms: None,
            frame_width: 720,
            frame_height: 252,
        }
    }
}

/// The dots' size against the preview's: the stage is small, so its pixels are drawn larger.
const PIXEL_SIZE: f32 = 1.6;

fn wrote(path: &Path) -> impl Fn(std::io::Error) -> String + '_ {
    move |e| format!("Couldn't write {}: {e}", path.display())
}

/// The stage written as a show file in `folder`, with its picture and the song's sound beside
/// it. Returns the show file's path.
pub fn save_stage(stage: &Stage, folder: &Path) -> Result<PathBuf, String> {
    let picture = folder.join(PICTURE);
    if let Some(images) = picture.parent() {
        std::fs::create_dir_all(images).map_err(wrote(images))?;
    }
    std::fs::write(&picture, stage::snowman()).map_err(wrote(&picture))?;
    let sound = folder.join(SONG_SOUND);
    std::fs::write(&sound, stage::song_wav()).map_err(wrote(&sound))?;
    let show = folder.join(SHOW_FILE);
    pf_engine::save_show_atomic(&show, &stage.show).map_err(|e| e.to_string())?;
    Ok(show)
}

/// `doc` saved as `<name>.pfseq.json` in `folder` (the stage's), playing the song's sound.
pub fn save_draft(folder: &Path, name: &str, doc: &Sequence) -> Result<PathBuf, String> {
    let path = folder.join(format!("{name}.pfseq.json"));
    let mut doc = doc.clone();
    doc.audio = Some(pf_model::path_to_text(&folder.join(SONG_SOUND)));
    pf_engine::save_sequence_atomic(&path, &doc).map_err(|e| e.to_string())?;
    Ok(path)
}

/// A scenario's name as a file's ("whole-song #2" → "whole-song-2").
pub fn file_name(scenario: &str) -> String {
    let mut name = String::new();
    for c in scenario.chars() {
        if c.is_ascii_alphanumeric() {
            name.push(c.to_ascii_lowercase());
        } else if !name.ends_with('-') {
            name.push('-');
        }
    }
    name.trim_matches('-').to_string()
}

/// "1:05" for 65,000 ms.
fn clock(ms: u64) -> String {
    format!("{}:{:02}", ms / 60_000, ms / 1000 % 60)
}

/// The kinds of effect playing at `at_ms`, each once, a showpiece marked with a star.
fn playing(doc: &Sequence, at_ms: u64) -> Vec<String> {
    let kinds: BTreeSet<String> = effects(doc)
        .filter(|(_, _, e)| e.start_ms <= at_ms && at_ms < e.end_ms)
        .map(|(_, _, e)| {
            let star = if crate::knowledge::is_showpiece(e.kind()) {
                "*"
            } else {
                ""
            };
            format!("{}{star}", reference::key_of(&e.kind()))
        })
        .collect();
    kinds.into_iter().collect()
}

/// `words` joined with ", " in lines of at most `width` characters.
fn lines_of(words: &[String], width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in words {
        match lines.last_mut() {
            Some(line) if line.len() + 2 + word.len() <= width => {
                line.push_str(", ");
                line.push_str(word);
            }
            _ => lines.push(word.clone()),
        }
    }
    lines
}

/// A frame from every section of the song: a beat into its middle bar (beat-led effects are lit
/// just after the beat), with its name, when it is, and the effects playing.
pub fn stills(doc: &Sequence, frame_width: u32) -> Vec<pf_video::Still> {
    let song = stage::song();
    let width = (frame_width / 12) as usize;
    let mut counted: Vec<(String, usize)> = Vec::new();
    song.sections
        .iter()
        .map(|section| {
            let repeats = song.sections.iter().filter(|s| s.label == section.label).count() > 1;
            let n = match counted.iter_mut().find(|(label, _)| *label == section.label) {
                Some((_, n)) => {
                    *n += 1;
                    *n
                }
                None => {
                    counted.push((section.label.clone(), 1));
                    1
                }
            };
            let name = if repeats {
                format!("{} {n}", section.label)
            } else {
                section.label.clone()
            };
            let middle = section.start_ms + (section.end_ms - section.start_ms) / 2;
            let at = middle / stage::BEAT_MS * stage::BEAT_MS + 120;
            let mut caption = vec![format!(
                "{name} at {} ({}-{})",
                clock(at),
                clock(section.start_ms),
                clock(section.end_ms)
            )];
            let kinds = playing(doc, at);
            if kinds.is_empty() {
                caption.push("nothing playing".into());
            } else {
                caption.extend(lines_of(&kinds, width).into_iter().take(2));
            }
            pf_video::Still { time_ms: at, caption }
        })
        .collect()
}

/// The draft at `sequence` (on the show at `show`, both opened as the app opens them) as a video
/// at `video` and a contact sheet at `sheet`, its first lines `title`.
pub fn render(
    engine: &mut Engine,
    sequence: &Path,
    video: &Path,
    sheet: &Path,
    title: Vec<String>,
    rendering: &Rendering,
) -> Result<(), String> {
    engine.open_sequence_doc(sequence).map_err(|e| e.to_string())?;
    let job = engine.sequence_export().map_err(|e| e.to_string())?;
    let options = pf_video::VideoOptions {
        width: rendering.width,
        height: rendering.height,
        fps: rendering.fps,
        start_ms: 0,
        end_ms: rendering.end_ms,
        photo: false,
        pixel_size: PIXEL_SIZE,
        glow: 0.0,
        ffmpeg: None,
    };
    for folder in [video.parent(), sheet.parent()].into_iter().flatten() {
        std::fs::create_dir_all(folder).map_err(wrote(folder))?;
    }
    pf_video::export_video(&job, &options, video, |_| true).map_err(|e| e.to_string())?;
    let sheet_options = pf_video::SheetOptions {
        frame_width: rendering.frame_width,
        frame_height: rendering.frame_height,
        columns: 2,
        pixel_size: PIXEL_SIZE,
        title,
    };
    let stills = stills(job.sequence(), rendering.frame_width);
    pf_video::contact_sheet(&job, &stills, &sheet_options, sheet).map_err(|e| e.to_string())?;
    Ok(())
}

/// The lines across the top of a scenario's contact sheet.
fn title(outcome: &Outcome, model: &str) -> Vec<String> {
    let m = &outcome.measures;
    vec![
        format!("{}: {}", outcome.scenario, outcome.title),
        format!(
            "{model} | {} of {} checks | review {} | {} kinds of effect, {} showpiece{} (*)",
            outcome.passed(),
            outcome.checks.len(),
            m.review.as_ref().map_or("-".to_string(), |r| r.score.to_string()),
            m.effect_kinds.len(),
            m.showpieces.len(),
            if m.showpieces.len() == 1 { "" } else { "s" },
        ),
    ]
}

/// Every outcome's draft saved under `out` beside `stage` (the stage they were answered on: a
/// draft's rows name its props), and with `rendering`, drawn as a video and a contact sheet.
/// Returns what was written, a line each.
pub fn keep(
    out: &Path,
    stage: &Stage,
    outcomes: &[Outcome],
    model: &str,
    rendering: Option<&Rendering>,
) -> Result<Vec<String>, String> {
    let folder = out.join(SHOW_FOLDER);
    std::fs::create_dir_all(&folder).map_err(wrote(&folder))?;
    let show = save_stage(stage, &folder)?;
    let mut said = vec![format!(
        "{SHOW_FOLDER}/{SHOW_FILE}: the made-up stage, to open in PixelFlow (its picture and the song's sound, a thump on every beat and a steady tone, beside it)"
    )];
    let mut saved = Vec::new();
    for outcome in outcomes {
        let name = file_name(&outcome.scenario);
        let path = save_draft(&folder, &name, &outcome.draft)?;
        saved.push((outcome, name, path));
    }
    let Some(rendering) = rendering else {
        said.extend(
            saved
                .iter()
                .map(|(o, name, _)| format!("{SHOW_FOLDER}/{name}.pfseq.json: the {} draft", o.scenario)),
        );
        return Ok(said);
    };
    // The engine keeps the song's worked-out sound in its data folder: one of its own, removed
    // when done.
    let data = out.join(".render-data");
    let mut engine = Engine::new(&data);
    let drawn = (|| -> Result<(), String> {
        engine.open(&show).map_err(|e| e.to_string())?;
        for (outcome, name, path) in &saved {
            let video = out.join(VIDEO_FOLDER).join(format!("{name}.mp4"));
            let sheet = out.join(SHEET_FOLDER).join(format!("{name}.png"));
            render(
                &mut engine,
                path,
                &video,
                &sheet,
                title(outcome, model),
                rendering,
            )?;
            said.push(format!(
                "{SHOW_FOLDER}/{name}.pfseq.json, {VIDEO_FOLDER}/{name}.mp4, {SHEET_FOLDER}/{name}.png: the {} draft, its video, and its contact sheet",
                outcome.scenario
            ));
        }
        Ok(())
    })();
    drop(engine);
    let _ = std::fs::remove_dir_all(&data);
    drawn.map(|()| said)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_plain() {
        assert_eq!(file_name("whole-song #2"), "whole-song-2");
        assert_eq!(file_name("review-and-fix"), "review-and-fix");
    }

    #[test]
    fn a_still_for_every_section_says_what_plays() {
        let stage = Stage::new();
        let doc = crate::eval::scenarios::christmas(&stage);
        let stills = stills(&doc, 640);
        assert_eq!(stills.len(), stage::song().sections.len());
        assert_eq!(stills[1].caption[0], "Verse 1 at 0:16 (0:08-0:24)");
        assert_eq!(stills[1].caption[1], "chase, on, twinkle");
        assert_eq!(stills[2].caption[0], "Chorus 1 at 0:32 (0:24-0:40)");
        assert_eq!(stills[5].caption[0], "Bridge at 1:16 (1:12-1:20)");
    }
}
