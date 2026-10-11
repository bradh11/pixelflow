//! The same show and sequence played by the engine itself, watched from outside: a check that
//! the paced runs, which rebuild playback from its parts to time them, behave as playback does.
//!
//! The engine is given the show and the sequence as the app would give them, a transport into
//! the benchmark's sink, and played. All that can be seen from outside is which frame is showing
//! and where the clock is, so this measures two things: frames the player skipped, and how long
//! after a frame was due the player had drawn it and handed it over (the paced runs' `shown`).
//!
//! The engine reads a sequence's music and pictures from files, which the benchmark doesn't
//! write, so here the effects that follow the music draw as in silence and the Picture effects
//! draw nothing. Compare with a paced run made the same way (`--without-media`).

use crate::measure::{Frames, Mode, Options, Pacing, RunResult, Sinks, Stages, Verdict, Warmup, Window};
use crate::pipeline::{Controllers, Workload, engine_send_rate};
use crate::show::tier_label;
use crate::sink::{Guarded, SinkKind};
use crate::stats::Spread;
use crate::system::load_average;
use anyhow::{Context, Result, anyhow, bail};
use pf_engine::{CheckedShow, Engine};
use pf_output::{OutputSettings, Transport};
use std::sync::Arc;
use std::time::Duration;

/// How often the watcher asks the engine which frame is showing.
const WATCH: Duration = Duration::from_micros(100);

/// Plays the workload's sequence in the engine for `options.seconds` and reports what could be
/// seen of it.
pub fn run_engine(workload: &Workload, sink: SinkKind, options: Options) -> Result<RunResult> {
    let load = load_average();
    let frame_ms = u64::from(workload.frame_ms());
    let total_ms = u64::from(options.seconds) * 1000;
    let warm_ms =
        (u64::from(options.warmup_seconds) * 1000).clamp(frame_ms, total_ms.saturating_sub(frame_ms));
    if total_ms > workload.sequence.sequence.duration_ms {
        bail!("the engine plays a sequence once: ask for at most its length");
    }

    let sinks = Sinks::open(sink, workload.show.show.controllers.len())?;
    let show = workload.show_at(sinks.addresses());
    let wiring = Controllers::new(workload.plan(sinks.addresses()), &OutputSettings::default())?.wiring();
    let sent = Arc::clone(&sinks.sent);
    let data = std::env::temp_dir().join(format!("pf-bench-{}", std::process::id()));
    let mut engine = Engine::new(data)
        .with_transport(move || Ok(Box::new(Guarded::open(sink, Arc::clone(&sent))?) as Box<dyn Transport>));
    engine.adopt_show(CheckedShow::new(show).map_err(|e| anyhow!("{e}"))?);
    engine
        .adopt_sequence_doc(workload.sequence.sequence.clone())
        .map_err(|e| anyhow!("{e}"))?;
    let status = engine.play_sequence_doc(0).map_err(|e| anyhow!("{e}"))?;
    let mut notes = status.notes.clone();
    notes.push(
        "Played by the engine itself, without the music or the picture (it reads those from files)."
            .to_string(),
    );

    // Each frame the player showed: its number, and how long after it was due.
    let mut shown: Vec<(u64, f64)> = Vec::new();
    let mut warm_frames = 0;
    let mut showing = status.position_ms;
    let mut window = None;
    loop {
        let status = engine.playback_status().context("the engine stopped playing")?;
        if let Some(error) = status.error {
            bail!("the engine stopped playing: {error}");
        }
        if status.now_ms >= warm_ms && window.is_none() {
            window = Some(Window::open(&sinks));
        }
        if status.position_ms != showing {
            showing = status.position_ms;
            let after_due = status.now_ms.saturating_sub(showing) as f64;
            if showing >= warm_ms && showing < total_ms {
                shown.push((showing / frame_ms, after_due));
            } else if showing < warm_ms {
                warm_frames += 1;
            }
        }
        if status.now_ms >= total_ms || status.state != "playing" {
            break;
        }
        std::thread::sleep(WATCH);
    }
    let due = (total_ms - warm_ms) / frame_ms;
    let (mut resources, network) = window
        .context("the engine never got past the warm-up")?
        // The player, the output thread, and this thread watching.
        .close(due, 3, &sinks);
    engine.stop_playback();
    drop(sinks);
    // Asking the engine what is showing allocates, thousands of times a frame: the count would
    // be the watcher's, not playback's.
    resources.allocations_per_frame = None;
    resources.allocated_kb_per_frame = None;

    let budget = frame_ms as f64;
    let after_due: Vec<f64> = shown.iter().map(|&(_, late)| late).collect();
    let frames = Frames {
        due,
        sent: shown.len() as u64,
        dropped: due.saturating_sub(shown.len() as u64),
        late: after_due.iter().filter(|&&late| late > budget).count() as u64,
    };
    let send_hz = engine_send_rate(workload.frame_ms());
    let pacing = Pacing {
        shown: Spread::of(&after_due),
        passes_per_second: network.packets_per_second / wiring.packets_per_pass.max(1) as f64,
        ..Pacing::default()
    };
    let fps = frames.sent as f64 / resources.wall_seconds;
    let sustained = frames.dropped == 0;
    let within_budget = pacing.shown.p99 <= budget;
    let clean = network.clean();
    Ok(RunResult {
        tier: tier_label(workload.show.pixels()),
        pixels: workload.show.pixels(),
        variant: workload.sequence.summary.variant,
        mode: Mode::Engine,
        sink,
        frame_ms: workload.frame_ms(),
        send_hz: Some(send_hz),
        wiring,
        frames,
        fps,
        stages: Stages::default(),
        pacing: Some(pacing),
        preview_poll: Spread::default(),
        lit_percent: 0.0,
        warmup: Warmup {
            frames: warm_frames,
            ..Warmup::default()
        },
        resources,
        network,
        verdict: Verdict {
            pass: sustained && within_budget && clean,
            sustained,
            within_budget,
            on_time: None,
            clean,
            summary: format!(
                "{fps:.1} frames a second, {} of {} dropped; drawn and handed over {:.0} ms after due at the 99th percentile; {:.0} output passes a second",
                frames.dropped, frames.due, pacing.shown.p99, pacing.passes_per_second,
            ),
        },
        load_average: load,
        setup: workload.setup,
        notes,
    })
}
