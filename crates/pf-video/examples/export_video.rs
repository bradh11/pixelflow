//! Exports a test video of a show: a made-up sequence on its props (a slow color wash and other
//! effects, with a white flash on every bar line of the music), for checking how videos look,
//! how fast they export, and that the flashes land on the beat.
//!
//! cargo run --release -p pf-video --example export_video -- SHOW.json MUSIC OUT.mp4 \
//!     [--start SECONDS] [--seconds SECONDS | --full] [--fps 30|60] [--720] [--no-photo] [--ffmpeg]

use anyhow::{Context, bail};
use pf_engine::Engine;
use pf_model::Rgb;
use pf_sequence::{Effect, EffectKind, Layer, Row, Target};
use pf_video::{VideoOptions, export_video};
use std::path::PathBuf;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [show, music, out] = [0, 1, 2].map(|i| args.get(i).map(PathBuf::from));
    let (Some(show), Some(music), Some(out)) = (show, music, out) else {
        bail!(
            "usage: export_video SHOW.json MUSIC OUT.mp4 [--start S] [--seconds S | --full] [--fps N] [--720] [--no-photo] [--ffmpeg]"
        );
    };
    let flag = |name: &str| args.iter().any(|a| a == name);
    let value = |name: &str| -> Option<f64> {
        let at = args.iter().position(|a| a == name)?;
        args.get(at + 1)?.parse().ok()
    };

    let data = std::env::temp_dir().join("pf-video-example");
    let mut engine = Engine::new(&data);
    engine.open(&show).context("opening the show")?;
    let analysis = pf_analysis::analyze_file(&music).context("reading the music")?;
    let frame_ms = u64::from(pf_render::DEFAULT_FRAME_MS);
    let song_ms = analysis.duration_ms;
    // Flashes on bar lines, snapped to the sequence's frames as the timeline snaps effects.
    let flashes: Vec<u64> = analysis
        .bars
        .iter()
        .map(|&b| b.div_ceil(frame_ms) * frame_ms)
        .collect();
    let washes = [
        (
            EffectKind::ColorWash,
            [Rgb::new(30, 0, 120), Rgb::new(120, 0, 60)],
        ),
        (EffectKind::Twinkle, [Rgb::new(0, 90, 160), Rgb::new(160, 120, 0)]),
        (EffectKind::Chase, [Rgb::new(0, 140, 40), Rgb::new(140, 0, 0)]),
        (EffectKind::Shimmer, [Rgb::new(120, 60, 0), Rgb::new(0, 60, 120)]),
    ];
    let rows: Vec<Row> = engine
        .show()
        .props
        .iter()
        .map(|prop| {
            let mut row = Row::new(Target::Prop(prop.id));
            // Below: a different effect each bar. Above: the flash at its start.
            let mut starts = vec![0];
            starts.extend(&flashes);
            starts.push(song_ms);
            let below = starts
                .windows(2)
                .enumerate()
                .filter(|(_, w)| w[1] > w[0])
                .map(|(i, w)| {
                    let (kind, colors) = &washes[i % washes.len()];
                    Effect::new(*kind, w[0], w[1]).with_palette(colors.to_vec())
                })
                .collect();
            let above = flashes
                .iter()
                .map(|&at| {
                    Effect::new(EffectKind::On, at, at + 100).with_palette(vec![Rgb::new(255, 255, 255)])
                })
                .collect();
            row.layers = vec![Layer { effects: below }, Layer { effects: above }];
            row
        })
        .collect();
    let music_text = pf_model::path_to_text(&music);
    engine.new_sequence_doc_with_rows("Video test", song_ms, Some(&music_text), rows)?;
    let job = engine.sequence_export()?;

    let start_ms = (value("--start").unwrap_or(0.0) * 1000.0) as u64;
    let end_ms = if flag("--full") {
        None
    } else {
        Some(start_ms + (value("--seconds").unwrap_or(20.0) * 1000.0) as u64)
    };
    let fps = value("--fps").map_or(30, |f| f as u32);
    let ffmpeg = if flag("--ffmpeg") {
        Some(pf_video::ffmpeg::find().context("no ffmpeg with H.264 found")?)
    } else {
        None
    };
    let (width, height) = if flag("--720") { (1280, 720) } else { (1920, 1080) };
    let options = VideoOptions {
        width,
        height,
        fps,
        start_ms,
        end_ms,
        photo: !flag("--no-photo"),
        pixel_size: 1.0,
        glow: 0.0,
        ffmpeg,
    };
    println!(
        "pixels: {}",
        job.show()
            .props
            .iter()
            .map(|p| pf_geometry::world_positions(p).len())
            .sum::<usize>()
    );
    let in_range: Vec<u64> = flashes
        .iter()
        .copied()
        .filter(|&f| f >= start_ms && end_ms.is_none_or(|e| f < e))
        .collect();
    println!("flashes (ms, sequence frames of {frame_ms} ms): {in_range:?}");
    let first_frames: Vec<u64> = in_range
        .iter()
        .map(|&f| ((f - start_ms) * u64::from(fps)).div_ceil(1000))
        .collect();
    println!("first lit video frame expected for each: {first_frames:?}");

    let started = Instant::now();
    let mut stage = None;
    let summary = export_video(&job, &options, &out, |p| {
        if stage != Some(p.stage) {
            println!("{:>7.2} s  {}", started.elapsed().as_secs_f64(), p.stage.label());
            stage = Some(p.stage);
        }
        true
    })?;
    println!("{:>7.2} s  done", started.elapsed().as_secs_f64());
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
