//! Exporting a sequence as an FPP sequence file (`.fseq`), ready to play on FPP.
//!
//! Every frame is rendered, then turned into controller channels exactly as live output would
//! send them (wiring order, color order, reverse, brightness, gamma), and laid out in the file's
//! channel space:
//! - a controller whose sequence channels are known (from its FPP's output list) gets exactly
//!   that block, so the file plays on FPP with the same outputs;
//! - controllers without them follow, back to back in show order.

use crate::{AudioSource, Pictures, Renderer};
use pf_fseq::{FseqError, FseqWriter, WriteOptions};
use pf_mapping::ChannelMap;
use pf_model::{ControllerId, Show};
use pf_output::render_controller;
use pf_sequence::Sequence;
use serde::Serialize;
use std::fs;
use std::io::{self, BufWriter, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Numbers each export's temporary file.
static EXPORT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Where one controller's channels sit in the exported file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportBlock {
    pub controller: ControllerId,
    pub name: String,
    /// First channel, counting from 1 (as FPP shows it).
    pub start: u32,
    pub count: u32,
    /// True when the block came from the controller's sequence channels; false when PixelFlow
    /// placed it.
    pub from_sequence_channels: bool,
}

/// The exported file's channel space.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportLayout {
    pub channels: u32,
    pub blocks: Vec<ExportBlock>,
    /// Plain-language notes about how the channels were laid out.
    pub notes: Vec<String>,
}

/// What an export wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub frames: u32,
    pub frame_ms: u32,
    pub duration_ms: u64,
    pub channels: u32,
    /// The music file name recorded in the file, if any.
    pub media: Option<String>,
    pub blocks: Vec<ExportBlock>,
    pub notes: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error(
        "There's nothing to export: no controller has any channels. Wire your props to a controller first."
    )]
    NoChannels,
    #[error("The sequence has no length. Give it a duration first.")]
    Empty,
    #[error("{0}")]
    Fseq(#[from] FseqError),
    #[error("Could not save {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
    #[error("The export was cancelled.")]
    Cancelled,
}

/// One block with the controller (index into the show's controllers) it comes from.
struct Placed {
    controller: usize,
    start: usize,
    count: usize,
}

fn place(show: &Show, map: &ChannelMap) -> (ExportLayout, Vec<Placed>) {
    let mut blocks = Vec::new();
    let mut placed = Vec::new();
    let mut notes = Vec::new();
    let mut end: u64 = 0;
    let outputs: Vec<_> = show
        .controllers
        .iter()
        .zip(&map.controllers)
        .enumerate()
        .collect();
    for &(i, (controller, output)) in &outputs {
        let Some(range) = controller
            .sequence_channels
            .filter(|r| r.start >= 1 && r.count >= 1)
        else {
            continue;
        };
        if output.channel_count > range.count as usize {
            notes.push(format!(
                "{} has {} channels wired, but its sequence channels ({}–{}) only hold {}; the rest are left out.",
                controller.name,
                output.channel_count,
                range.start,
                u64::from(range.start) + u64::from(range.count) - 1,
                range.count
            ));
        }
        blocks.push(ExportBlock {
            controller: controller.id,
            name: controller.name.clone(),
            start: range.start,
            count: range.count,
            from_sequence_channels: true,
        });
        placed.push(Placed {
            controller: i,
            start: range.start as usize - 1,
            count: range.count as usize,
        });
        end = end.max(u64::from(range.start) - 1 + u64::from(range.count));
    }
    let mut sorted: Vec<&ExportBlock> = blocks.iter().collect();
    sorted.sort_by_key(|b| b.start);
    for pair in sorted.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if u64::from(a.start) + u64::from(a.count) > u64::from(b.start) {
            notes.push(format!(
                "{} and {} share sequence channels starting at {}; only one of them gets them.",
                a.name, b.name, b.start
            ));
        }
    }
    let had_known = !blocks.is_empty();
    let mut appended = Vec::new();
    let first_appended = end + 1;
    for &(i, (controller, output)) in &outputs {
        if blocks.iter().any(|b| b.controller == controller.id) || output.channel_count == 0 {
            continue;
        }
        let count = u32::try_from(output.channel_count).unwrap_or(u32::MAX);
        blocks.push(ExportBlock {
            controller: controller.id,
            name: controller.name.clone(),
            start: u32::try_from(end + 1).unwrap_or(u32::MAX),
            count,
            from_sequence_channels: false,
        });
        placed.push(Placed {
            controller: i,
            start: end as usize,
            count: output.channel_count,
        });
        end += u64::from(count);
        appended.push(controller.name.clone());
    }
    if had_known && !appended.is_empty() {
        notes.push(format!(
            "{} {} no sequence channels set, so {} placed after the others from channel {first_appended}. \
             Add them from your FPP's output list on the Controllers screen so FPP sends them the right data.",
            appended.join(", "),
            if appended.len() == 1 { "has" } else { "have" },
            if appended.len() == 1 {
                "it was"
            } else {
                "they were"
            },
        ));
    }
    let layout = ExportLayout {
        channels: u32::try_from(end).unwrap_or(u32::MAX),
        blocks,
        notes,
    };
    (layout, placed)
}

/// The channel space an export of this show would use.
pub fn export_layout(show: &Show, map: &ChannelMap) -> ExportLayout {
    place(show, map).0
}

/// Renders every frame of `seq` (effects that follow the music reading `audio`, Picture effects
/// drawing from `pictures`, waiting for each) and writes an `.fseq` file to `out`. `progress` is called after each frame with (frames done, total frames);
/// returning `false` cancels the export ([`ExportError::Cancelled`]) before the next frame.
#[allow(clippy::too_many_arguments)]
pub fn export_fseq<W: Write + Seek>(
    show: &Show,
    map: &ChannelMap,
    seq: &Sequence,
    audio: &AudioSource,
    pictures: &Pictures,
    out: W,
    unique_id: u64,
    mut progress: impl FnMut(u32, u32) -> bool,
) -> Result<(W, ExportSummary), ExportError> {
    let (layout, placed) = place(show, map);
    if layout.channels == 0 {
        return Err(ExportError::NoChannels);
    }
    let frames = u32::try_from(seq.frame_count()).unwrap_or(u32::MAX);
    if frames == 0 {
        return Err(ExportError::Empty);
    }
    let media = seq
        .audio
        .as_deref()
        .and_then(|a| Path::new(a).file_name())
        .map(|n| n.to_string_lossy().into_owned());
    let mut options = WriteOptions::new(layout.channels, frames, seq.frame_ms);
    options.media = media.clone();
    options.producer = Some(format!("PixelFlow {}", env!("CARGO_PKG_VERSION")));
    options.unique_id = unique_id;
    let mut writer = FseqWriter::new(out, options)?;

    let plan = pf_output::build_offline_plan(show, map);
    let mut renderer = Renderer::new(show, map);
    renderer.set_audio(audio.clone());
    renderer.set_pictures(pictures.waiting());
    let mut show_frame = vec![0u8; renderer.frame_len()];
    let mut seq_frame = vec![0u8; layout.channels as usize];
    let mut controller_frame = Vec::new();
    for index in 0..frames {
        renderer.render_frame(seq, u64::from(index), &mut show_frame);
        seq_frame.fill(0);
        for block in &placed {
            let controller = &plan.controllers[block.controller];
            controller_frame.clear();
            controller_frame.resize(controller.channel_count, 0);
            render_controller(&show_frame, controller, &plan.luts, &mut controller_frame);
            let n = block.count.min(controller_frame.len());
            if let Some(target) = seq_frame.get_mut(block.start..block.start + n) {
                target.copy_from_slice(&controller_frame[..n]);
            }
        }
        writer.write_frame(&seq_frame)?;
        if !progress(index + 1, frames) {
            return Err(ExportError::Cancelled);
        }
    }
    let out = writer.finish()?;
    Ok((
        out,
        ExportSummary {
            frames,
            frame_ms: seq.frame_ms,
            duration_ms: seq.duration_ms,
            channels: layout.channels,
            media,
            blocks: layout.blocks,
            notes: layout.notes,
        },
    ))
}

/// [`export_fseq`] to a file, written atomically: a crash, error, or cancel never leaves a
/// half-written file at `path`.
pub fn export_fseq_file(
    show: &Show,
    map: &ChannelMap,
    seq: &Sequence,
    audio: &AudioSource,
    pictures: &Pictures,
    path: &Path,
    progress: impl FnMut(u32, u32) -> bool,
) -> Result<ExportSummary, ExportError> {
    let write_err = |source| ExportError::Write {
        path: path.to_path_buf(),
        source,
    };
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    // Unique per export, so two exports to the same file at once don't share a temporary file.
    let n = EXPORT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".{name}.{}.{n}.tmp", std::process::id()));
    let result = (|| {
        let file = fs::File::create(&tmp).map_err(write_err)?;
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);
        let (out, summary) = export_fseq(
            show,
            map,
            seq,
            audio,
            pictures,
            BufWriter::new(file),
            unique_id,
            progress,
        )?;
        let file = out.into_inner().map_err(|e| write_err(e.into_error()))?;
        file.sync_all().map_err(write_err)?;
        fs::rename(&tmp, path).map_err(write_err)?;
        Ok(summary)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_model::{Controller, Generator, Port, PortSlot, Prop, Protocol, SequenceChannels, ShapeSource};

    fn controller(name: &str, prop: &Prop, sequence: Option<(u32, u32)>) -> Controller {
        let mut c = Controller::new(name, "192.0.2.1", Protocol::Ddp);
        let mut port = Port::new(1);
        port.slots.push(PortSlot::new(prop.id));
        c.ports.push(port);
        c.sequence_channels = sequence.map(|(start, count)| SequenceChannels {
            start,
            count,
            raw_ddp_offsets: false,
        });
        c
    }

    fn strip(nodes: u32) -> Prop {
        Prop::new(
            "Strip",
            ShapeSource::Generator(Generator::Line { nodes, length: 1.0 }),
        )
    }

    #[test]
    fn known_sequence_channels_are_kept_and_the_rest_follow() {
        let (a, b, c) = (strip(4), strip(2), strip(3));
        let mut show = Show::new("t");
        show.props = vec![a.clone(), b.clone(), c.clone()];
        show.controllers = vec![
            controller("Porch", &a, None),
            controller("Falcon", &b, Some((101, 30))),
            controller("Garage", &c, Some((1, 6))),
        ];
        let (map, _) = pf_mapping::map_show(&show);
        let layout = export_layout(&show, &map);
        let blocks: Vec<(&str, u32, u32, bool)> = layout
            .blocks
            .iter()
            .map(|b| (b.name.as_str(), b.start, b.count, b.from_sequence_channels))
            .collect();
        assert_eq!(
            blocks,
            vec![
                ("Falcon", 101, 30, true),
                ("Garage", 1, 6, true),
                ("Porch", 131, 12, false)
            ]
        );
        assert_eq!(layout.channels, 142);
        assert_eq!(
            layout.notes,
            vec![
                "Garage has 9 channels wired, but its sequence channels (1–6) only hold 6; the rest are left out.",
                "Porch has no sequence channels set, so it was placed after the others from channel 131. Add them from your FPP's output list on the Controllers screen so FPP sends them the right data.",
            ]
        );
    }

    #[test]
    fn without_sequence_channels_controllers_go_back_to_back() {
        let (a, b) = (strip(4), strip(2));
        let mut show = Show::new("t");
        show.props = vec![a.clone(), b.clone()];
        show.controllers = vec![controller("One", &a, None), controller("Two", &b, None)];
        let (map, _) = pf_mapping::map_show(&show);
        let layout = export_layout(&show, &map);
        let starts: Vec<(u32, u32)> = layout.blocks.iter().map(|b| (b.start, b.count)).collect();
        assert_eq!(starts, vec![(1, 12), (13, 6)]);
        assert!(layout.notes.is_empty());
    }

    #[test]
    fn overlapping_blocks_are_noted() {
        let (a, b) = (strip(4), strip(4));
        let mut show = Show::new("t");
        show.props = vec![a.clone(), b.clone()];
        show.controllers = vec![
            controller("One", &a, Some((1, 12))),
            controller("Two", &b, Some((10, 12))),
        ];
        let (map, _) = pf_mapping::map_show(&show);
        let notes = export_layout(&show, &map).notes;
        assert_eq!(
            notes,
            vec!["One and Two share sequence channels starting at 10; only one of them gets them."]
        );
    }
}
