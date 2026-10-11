//! Running the pipeline: flat out on one thread, or in real time on playback's threads.
//!
//! [`run_paced`] is live playback of an authored sequence with a stopwatch on it: the player
//! thread works out which frame is due, draws it, copies it for the preview, hands it over, and
//! sleeps to the next frame boundary (`pf-engine`'s `run_player`); the output thread sends the
//! latest frame at twice the sequence's rate on [`FrameClock`] (`pf-output`'s `run`); and a third
//! thread reads the preview frame as often as the app does. `--mode engine` plays the same
//! sequence in the engine itself, to check the two agree.

use crate::alloc::counted;
use crate::measure::{
    Frames, Mode, Options, Pacing, RunResult, Sinks, Stages, Verdict, Warmup, Window, lit_percent,
};
use crate::pipeline::{Controllers, Pass, Workload, engine_send_rate};
use crate::show::tier_label;
use crate::sink::SinkKind;
use crate::stats::{Spread, deviation, ms};
use crate::system::load_average;
use anyhow::{Context, Result};
use pf_frame::{FrameWriter, frame_buffers};
use pf_output::{FrameClock, OutputSettings};
use pf_render::Renderer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// The longest the player sleeps before looking again (`pf-engine`'s `POLL`).
const POLL: Duration = Duration::from_millis(10);
/// How often the app reads the preview frame while something plays (the Play screen's timer).
const PREVIEW_POLL: Duration = Duration::from_millis(50);
/// Bytes after the show frame that carry the frame's number from the player to the output
/// thread, so each pass knows exactly which frame it sent. Nothing reads them but the benchmark.
const TRAILER: usize = 8;

/// How long the player sleeps once it has dealt with the frame due at `light` milliseconds: to
/// the next frame boundary, less the time `spent` since it read the clock, or [`POLL`] if that
/// is sooner (`pf-engine`'s `until_next_frame`).
fn player_wait(light: u64, frame_ms: u64, spent: Duration) -> Duration {
    Duration::from_millis(frame_ms - light % frame_ms)
        .saturating_sub(spent)
        .min(POLL)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the app's read of the preview frame costs: the lock, and a copy of the whole frame.
fn read_preview(preview: &Mutex<Vec<u8>>) -> Duration {
    let started = Instant::now();
    let copy = lock(preview).clone();
    let took = started.elapsed();
    std::hint::black_box(copy);
    took
}

/// Runs the pipeline one frame after another on this thread, as fast as it goes.
pub fn run_asap(workload: &Workload, sink: SinkKind, options: Options) -> Result<RunResult> {
    let load = load_average();
    let seq = &workload.sequence.sequence;
    let frame_ms = workload.frame_ms();
    let per_second = u64::from(1000 / frame_ms);
    let warm = u64::from(options.warmup_seconds) * per_second;
    let total = warm + u64::from(options.seconds).max(1) * per_second;
    let sequence_frames = seq.frame_count().max(1);

    let sinks = Sinks::open(sink, workload.show.show.controllers.len())?;
    let mut transport = sinks.transport()?;
    let mut controllers = Controllers::new(workload.plan(sinks.addresses()), &OutputSettings::default())?;
    let mut renderer = workload.renderer();
    let mut frame = vec![0u8; workload.map.frame_len];
    let preview = Mutex::new(vec![0u8; frame.len()]);
    let raw = Mutex::new(vec![0u8; frame.len()]);

    let mut timings: [Vec<f64>; 6] = std::array::from_fn(|_| Vec::with_capacity(total as usize));
    let (mut warmup, mut lit) = (Vec::with_capacity(warm as usize), Vec::new());
    // Allocations made drawing frames, and made sending them.
    let (mut drawing, mut sending) = (0u64, 0u64);
    let mut window = None;
    for index in 0..total {
        if index == warm {
            window = Some(Window::open(&sinks));
        }
        let before = counted();
        let started = Instant::now();
        renderer.render_frame(seq, index % sequence_frames, &mut frame);
        let rendered = Instant::now();
        lock(&preview).copy_from_slice(&frame);
        lock(&raw).copy_from_slice(&frame);
        let shown = Instant::now();
        let between = counted();
        let pass = controllers.send(&frame, &mut transport);
        let whole = started.elapsed();
        if index < warm {
            warmup.push(ms(whole));
            continue;
        }
        if let (Some(before), Some(between), Some(after)) = (before, between, counted()) {
            drawing += between.allocations - before.allocations;
            sending += after.allocations - between.allocations;
        }
        let stages = [
            rendered - started,
            shown - rendered,
            pass.map,
            pass.packetize,
            pass.send,
            whole,
        ];
        for (timing, stage) in timings.iter_mut().zip(stages) {
            timing.push(ms(stage));
        }
        if index.is_multiple_of(per_second) {
            lit.push(lit_percent(&frame));
        }
    }
    let measured = total - warm;
    let window = window.context("nothing was measured: the run is all warm-up")?;
    let (mut resources, network) = window.close(measured, 1, &sinks);
    if counted().is_some() {
        resources.render_allocations_per_frame = Some(drawing as f64 / measured as f64);
        resources.output_allocations_per_pass = Some(sending as f64 / measured as f64);
    }
    let [render, shown, map, packetize, send, whole] = timings.each_ref().map(|t| Spread::of(t));
    let sum = |stages: &[usize]| -> Vec<f64> {
        (0..measured as usize)
            .map(|i| stages.iter().map(|&s| timings[s][i]).sum())
            .collect()
    };
    let fps = measured as f64 / resources.wall_seconds;
    let budget = f64::from(frame_ms);
    let within_budget = whole.p99 <= budget;
    // Flat out, a loopback socket's buffer fills: that says nothing about the pipeline.
    let clean = network.send_failures == 0 && network.refused == 0;
    Ok(RunResult {
        tier: tier_label(workload.show.pixels()),
        pixels: workload.show.pixels(),
        variant: workload.sequence.summary.variant,
        mode: Mode::Asap,
        sink,
        frame_ms,
        send_hz: None,
        wiring: controllers.wiring(),
        frames: Frames {
            due: measured,
            sent: measured,
            dropped: 0,
            late: timings[5].iter().filter(|&&t| t > budget).count() as u64,
        },
        fps,
        stages: Stages {
            render,
            preview: shown,
            player: Spread::of(&sum(&[0, 1])),
            map,
            packetize,
            send,
            output_pass: Spread::of(&sum(&[2, 3, 4])),
            frame: whole,
        },
        pacing: None,
        preview_poll: Spread::of_durations((0..200).map(|_| read_preview(&preview))),
        lit_percent: lit.iter().sum::<f64>() / lit.len().max(1) as f64,
        warmup: Warmup::of(&warmup),
        resources,
        network,
        verdict: Verdict {
            pass: within_budget && clean,
            sustained: fps >= 1000.0 / budget,
            within_budget,
            on_time: None,
            clean,
            summary: format!(
                "{fps:.0} frames a second flat out; a frame takes {:.1} ms at the 99th percentile, {} the {budget:.0} ms period",
                whole.p99,
                if within_budget { "inside" } else { "over" },
            ),
        },
        load_average: load,
        setup: workload.setup,
        notes: Vec::new(),
    })
}

/// One frame the player drew, with times counted from the sequence's start.
struct Drawn {
    index: u64,
    /// When the player started on it, and when it had handed it over.
    started: Duration,
    shown: Duration,
    render: Duration,
    preview: Duration,
}

/// One pass of the output thread.
struct Sending {
    /// The frame it sent.
    frame: u64,
    cost: Pass,
    /// When it finished, counted from the sequence's start.
    finished: Duration,
    /// The pass began more than a period late.
    late: bool,
}

/// Plays the sequence in real time on playback's two threads, with the app's read of the
/// preview frame on a third.
pub fn run_paced(workload: &Workload, sink: SinkKind, options: Options) -> Result<RunResult> {
    let load = load_average();
    let seq = &workload.sequence.sequence;
    let frame_ms = workload.frame_ms();
    let step = Duration::from_millis(u64::from(frame_ms));
    let per_second = u64::from(1000 / frame_ms);
    // The first frame is drawn before the clock starts, so the warm-up is never shorter than it.
    let warm = (u64::from(options.warmup_seconds) * per_second).max(1);
    let total = warm + u64::from(options.seconds).max(1) * per_second;
    let sequence_frames = seq.frame_count().max(1);
    let send_hz = options.send_hz.unwrap_or_else(|| engine_send_rate(frame_ms));

    let sinks = Sinks::open(sink, workload.show.show.controllers.len())?;
    let mut transport = sinks.transport()?;
    let mut controllers = Controllers::new(workload.plan(sinks.addresses()), &OutputSettings::default())?;
    let wiring = controllers.wiring();
    let frame_len = workload.map.frame_len;
    let mut renderer = workload.renderer();
    let (mut writer, mut reader) = frame_buffers(frame_len + TRAILER);
    let preview = Mutex::new(vec![0u8; frame_len]);
    let raw = Mutex::new(vec![0u8; frame_len]);
    let stop = AtomicBool::new(false);

    // Draws a frame and hands it over, as the player's `Frames::show` does.
    let show = |index: u64, renderer: &mut Renderer, writer: &mut FrameWriter| {
        let started = Instant::now();
        let frame = writer.frame_mut();
        renderer.render_frame(seq, index % sequence_frames, &mut frame[..frame_len]);
        frame[frame_len..].copy_from_slice(&index.to_le_bytes());
        let rendered = Instant::now();
        lock(&preview).copy_from_slice(&frame[..frame_len]);
        lock(&raw).copy_from_slice(&frame[..frame_len]);
        writer.publish();
        (rendered - started, rendered.elapsed())
    };
    // Playback draws and publishes the first frame before the output starts or the clock runs.
    let first = show(0, &mut renderer, &mut writer);
    let zero = Instant::now();
    let (drawn, lit, closed, passes, polls) = std::thread::scope(|scope| -> Result<_> {
        let player = std::thread::Builder::new()
            .name("pf-bench-playback".into())
            .spawn_scoped(scope, || {
                let mut drawn: Vec<Drawn> = Vec::with_capacity(total as usize);
                let mut lit = Vec::new();
                let mut shown = Some(0);
                loop {
                    // The player reads its clock in whole milliseconds.
                    let read = Instant::now();
                    let light = (read - zero).as_millis() as u64;
                    let due = light / u64::from(frame_ms);
                    if due >= total {
                        break;
                    }
                    if shown != Some(due) {
                        let started = zero.elapsed();
                        let (render, preview) = show(due, &mut renderer, &mut writer);
                        drawn.push(Drawn {
                            index: due,
                            started,
                            shown: zero.elapsed(),
                            render,
                            preview,
                        });
                        shown = Some(due);
                        if due.is_multiple_of(per_second) {
                            lit.push(lit_percent(&lock(&raw)));
                        }
                    }
                    std::thread::sleep(player_wait(light, u64::from(frame_ms), read.elapsed()));
                }
                (drawn, lit)
            })
            .context("couldn't start the playback thread")?;
        let sender = std::thread::Builder::new()
            .name("pf-bench-output".into())
            .spawn_scoped(scope, || {
                let mut passes: Vec<Sending> = Vec::with_capacity((total * 4) as usize);
                let mut clock = FrameClock::new(send_hz);
                while !stop.load(Ordering::Relaxed) {
                    let late = clock.wait();
                    let (frame, trailer) = reader.latest().split_at(frame_len);
                    let number = u64::from_le_bytes(trailer.try_into().expect("an eight-byte trailer"));
                    let cost = controllers.send(frame, &mut transport);
                    passes.push(Sending {
                        frame: number,
                        cost,
                        finished: zero.elapsed(),
                        late,
                    });
                }
                passes
            })
            .context("couldn't start the output thread")?;
        let poller = std::thread::Builder::new()
            .name("pf-bench-preview".into())
            .spawn_scoped(scope, || {
                let mut polls = Vec::new();
                while !stop.load(Ordering::Relaxed) {
                    polls.push(read_preview(&preview));
                    std::thread::sleep(PREVIEW_POLL);
                }
                polls
            })
            .context("couldn't start the preview thread")?;

        std::thread::sleep((step * warm as u32).saturating_sub(zero.elapsed()));
        let window = Window::open(&sinks);
        let drawn = player.join();
        let closed = window.close(total - warm, 3, &sinks);
        // The last frame gets its period to reach the wire, like every other.
        std::thread::sleep(step);
        stop.store(true, Ordering::Relaxed);
        let (passes, polls) = (sender.join(), poller.join());
        let panicked = || anyhow::anyhow!("a pipeline thread panicked");
        let (drawn, lit) = drawn.map_err(|_| panicked())?;
        Ok((
            drawn,
            lit,
            closed,
            passes.map_err(|_| panicked())?,
            polls.map_err(|_| panicked())?,
        ))
    })?;
    let (resources, network) = closed;
    drop(sinks);

    // When each frame's first send finished, and what that pass cost.
    let mut first_sent: Vec<Option<(Duration, Pass)>> = vec![None; total as usize];
    for pass in &passes {
        if let Some(slot) = first_sent.get_mut(pass.frame as usize)
            && slot.is_none()
        {
            *slot = Some((pass.finished, pass.cost));
        }
    }
    let due_at = |index: u64| step * index as u32;
    let since_due = |at: Duration, index: u64| ms(at.saturating_sub(due_at(index)));
    let (mut latency, mut work, mut finished) = (Vec::new(), Vec::new(), Vec::new());
    let (mut render, mut copies, mut start_delay, mut shown) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut player = Vec::new();
    for frame in drawn.iter().filter(|d| d.index >= warm) {
        render.push(ms(frame.render));
        copies.push(ms(frame.preview));
        player.push(ms(frame.render + frame.preview));
        start_delay.push(since_due(frame.started, frame.index));
        shown.push(since_due(frame.shown, frame.index));
        if let Some((done, cost)) = first_sent[frame.index as usize] {
            latency.push(since_due(done, frame.index));
            work.push(ms(frame.render + frame.preview + cost.total()));
            finished.push(ms(done));
        }
    }
    let intervals: Vec<f64> = finished.windows(2).map(|pair| pair[1] - pair[0]).collect();
    let in_window: Vec<&Sending> = passes
        .iter()
        .filter(|p| p.finished >= due_at(warm) && p.finished < due_at(total))
        .collect();
    let stage = |of: fn(&Pass) -> Duration| Spread::of_durations(in_window.iter().map(|p| of(&p.cost)));
    let stages = Stages {
        render: Spread::of(&render),
        preview: Spread::of(&copies),
        player: Spread::of(&player),
        map: stage(|p| p.map),
        packetize: stage(|p| p.packetize),
        send: stage(|p| p.send),
        output_pass: stage(Pass::total),
        frame: Spread::of(&work),
    };
    let due = total - warm;
    let budget = f64::from(frame_ms);
    let frames = Frames {
        due,
        sent: latency.len() as u64,
        dropped: due - latency.len() as u64,
        late: shown.iter().filter(|&&after_due| after_due > budget).count() as u64,
    };
    let pacing = Pacing {
        start_delay: Spread::of(&start_delay),
        shown: Spread::of(&shown),
        latency: Spread::of(&latency),
        interval: Spread::of(&intervals),
        jitter: deviation(&intervals),
        passes_per_second: in_window.len() as f64 / resources.wall_seconds,
        late_passes: in_window.iter().filter(|p| p.late).count() as u64,
    };
    let fps = frames.sent as f64 / resources.wall_seconds;
    let period = 1000.0 / f64::from(send_hz);
    let sustained = frames.dropped == 0;
    // Each thread has the frame period for its part: a pass that overruns the output thread's
    // own, shorter period only delays the next pass, and every frame still goes out.
    let within_budget = stages.player.p99 <= budget && stages.output_pass.p99 <= budget;
    let on_time = sustained && pacing.shown.p99 <= budget;
    let clean = network.clean();
    let mut summary = format!(
        "{fps:.1} frames a second, {} of {} dropped; at the 99th percentile the player takes {:.1} ms of {budget:.0} to draw a frame and the output thread {:.1} ms for a pass (it makes one every {period:.1}); frames are handed over {:.1} ms after due ({} late) and on the wire {:.1} ms after due",
        frames.dropped,
        frames.due,
        stages.player.p99,
        stages.output_pass.p99,
        pacing.shown.p99,
        frames.late,
        pacing.latency.p99,
    );
    if !clean {
        summary.push_str(&format!(
            "; {} packets dropped at a full send buffer, {} sends failed",
            network.send_buffer_full,
            network.send_failures + network.refused
        ));
    }
    let warmup: Vec<f64> = std::iter::once(ms(first.0 + first.1))
        .chain(
            drawn
                .iter()
                .filter(|d| d.index < warm)
                .map(|d| ms(d.render + d.preview)),
        )
        .collect();
    Ok(RunResult {
        tier: tier_label(workload.show.pixels()),
        pixels: workload.show.pixels(),
        variant: workload.sequence.summary.variant,
        mode: Mode::Paced,
        sink,
        frame_ms,
        send_hz: Some(send_hz),
        wiring,
        frames,
        fps,
        stages,
        pacing: Some(pacing),
        preview_poll: Spread::of_durations(polls),
        lit_percent: lit.iter().sum::<f64>() / lit.len().max(1) as f64,
        warmup: Warmup::of(&warmup),
        resources,
        network,
        verdict: Verdict {
            pass: sustained && within_budget && clean,
            sustained,
            within_budget,
            on_time: Some(on_time),
            clean,
            summary,
        },
        load_average: load,
        setup: workload.setup,
        notes: Vec::new(),
    })
}

/// Runs the pipeline as `mode` says.
pub fn run(workload: &Workload, mode: Mode, sink: SinkKind, options: Options) -> Result<RunResult> {
    match mode {
        Mode::Asap => run_asap(workload, sink, options),
        Mode::Paced => run_paced(workload, sink, options),
        Mode::Engine => crate::engine::run_engine(workload, sink, options),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_player_sleeps_to_the_next_frame_or_ten_milliseconds() {
        let ms = Duration::from_millis;
        assert_eq!(player_wait(0, 25, ms(0)), ms(10));
        assert_eq!(player_wait(20, 25, ms(0)), ms(5));
        assert_eq!(player_wait(24, 25, ms(0)), ms(1));
        assert_eq!(player_wait(25, 25, ms(0)), ms(10));
        assert_eq!(player_wait(49, 25, ms(0)), ms(1));
        // The time a frame took to draw comes off the wait for the next.
        assert_eq!(player_wait(0, 25, ms(20)), ms(5));
        assert_eq!(player_wait(0, 25, ms(30)), ms(0));
    }
}
