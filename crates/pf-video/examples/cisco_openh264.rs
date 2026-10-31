//! Tries the real path the desktop app takes for video export: downloads Cisco's OpenH264 binary
//! for this computer from Cisco's server into FOLDER (as the app does into its data folder),
//! checks it, loads it, and exports a few seconds of a made-up show with it to FOLDER/cisco.mp4.
//! It reaches the internet, so it's never run by the tests.
//!
//! cargo run -p pf-video --example cisco_openh264 -- FOLDER

use anyhow::{Context, bail};
use pf_engine::{Edit, Engine};
use pf_model::{Generator, Prop, Rgb, ShapeSource};
use pf_sequence::{Effect, EffectKind, Row, Target};
use pf_video::cisco::{self, State, Store};
use pf_video::{Encoder, VideoOptions, export_video};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let Some(folder) = std::env::args().nth(1).map(PathBuf::from) else {
        bail!("usage: cisco_openh264 FOLDER");
    };
    let store = Store::new(&folder);
    let binary = store
        .binary()
        .context("Cisco makes no OpenH264 for this system")?;
    println!(
        "{} for {} {} from {}",
        binary.file, binary.os, binary.arch, binary.url
    );
    if store.state() == State::Missing {
        let library = store
            .download(
                &|received, total| eprint!("\rdownloading: {received} of {total} bytes"),
                &|| false,
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        eprintln!();
        println!(
            "kept, checked against SHA-256 {}: {}",
            binary.sha256,
            library.display()
        );
    }
    let library = store.ready().map_err(|e| anyhow::anyhow!("{e}"))?;
    // What an export does first: check it again and load it.
    cisco::load(&library).map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("loaded {}", library.display());

    let data = tempfile::tempdir()?;
    let mut engine = Engine::new(data.path());
    let mut props = Vec::new();
    let mut rows = Vec::new();
    for (n, color) in [Rgb::new(255, 40, 0), Rgb::new(0, 200, 80), Rgb::new(40, 80, 255)]
        .into_iter()
        .enumerate()
    {
        let mut prop = Prop::new(
            format!("Line {n}"),
            ShapeSource::Generator(Generator::Line {
                nodes: 50,
                length: 4.0,
            }),
        );
        prop.transform.position.y = n as f32 * 1.5;
        let mut row = Row::new(Target::Prop(prop.id));
        row.layers[0].effects.push(
            Effect::new(EffectKind::On, n as u64 * 1000, 3000 + n as u64 * 1000).with_palette(vec![color]),
        );
        props.push(prop);
        rows.push(row);
    }
    engine.apply(props.into_iter().map(|prop| Edit::AddProp { prop }).collect())?;
    engine.new_sequence_doc_with_rows("Cisco", 5000, None, rows)?;
    let job = engine.sequence_export()?;
    let options = VideoOptions {
        width: 1280,
        height: 720,
        fps: 30,
        start_ms: 0,
        end_ms: None,
        photo: false,
        pixel_size: 1.0,
        glow: 0.3,
        encoder: Encoder::OpenH264(library),
    };
    let out = folder.join("cisco.mp4");
    let summary = export_video(&job, &options, &out, |_| true)?;
    println!(
        "wrote {}: {} frames, {} ms, {}x{}, {} bytes, {}",
        out.display(),
        summary.frames,
        summary.duration_ms,
        summary.width,
        summary.height,
        summary.bytes,
        summary.video
    );
    Ok(())
}
