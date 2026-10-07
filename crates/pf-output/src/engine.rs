//! The output thread: paces frames and sends every controller's packets.

use crate::clock::FrameClock;
use crate::ddp::DdpPackets;
use crate::gather::render_controller;
use crate::health::{ControllerState, Health};
use crate::plan::{ControllerPlan, OutputPlan, Wire};
use crate::sacn::{SacnPackets, SacnSequences, multicast_addr, sync_packet};
use crate::settings::OutputSettings;
use crate::transport::Transport;
use pf_frame::FrameReader;
use pf_mapping::UniverseSpan;
use pf_model::ControllerId;
use std::collections::HashSet;
use std::io;
use std::net::SocketAddr;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Publish stats to readers at least this often.
const STATS_INTERVAL: Duration = Duration::from_millis(250);

/// How many times the final black frame is sent (UDP is lossy)...
const BLACKOUT_REPEATS: usize = 3;
/// ...and the gap between those passes, so one burst of loss can't swallow them all.
const BLACKOUT_GAP: Duration = Duration::from_millis(20);
/// How many Stream_Terminated packets end each sACN stream (E1.31 section 6.7.1 asks for three).
const TERMINATE_REPEATS: usize = 3;

const BUFFER_FULL_MESSAGE: &str = "send buffer full; packets dropped";

/// Whether a send error means "try again later" rather than "the controller is unreachable".
fn is_transient(e: &io::Error) -> bool {
    #[cfg(target_os = "macos")]
    const ENOBUFS: i32 = 55;
    #[cfg(target_os = "linux")]
    const ENOBUFS: i32 = 105;
    #[cfg(windows)]
    const ENOBUFS: i32 = 10055;
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    const ENOBUFS: i32 = -1;
    e.kind() == io::ErrorKind::WouldBlock || e.raw_os_error() == Some(ENOBUFS)
}

/// Live counters for one controller.
#[derive(Debug, Clone, PartialEq)]
pub struct ControllerStats {
    pub controller: ControllerId,
    pub name: String,
    pub state: ControllerState,
    pub packets_sent: u64,
    pub send_errors: u64,
    pub last_error: Option<String>,
}

/// Live counters for the whole output session.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutputStats {
    pub frames: u64,
    /// Frames whose deadline was missed by more than one period.
    pub late_frames: u64,
    pub achieved_fps: f32,
    pub controllers: Vec<ControllerStats>,
    /// Set when the output thread stopped by itself (it crashed): what went wrong. Nothing has
    /// been sent since, and the controllers were not blacked out.
    pub failure: Option<String>,
}

/// A plan waiting for the output thread to switch to it, with the frames to read from then on.
type PendingPlan = Arc<Mutex<Option<(OutputPlan, FrameReader)>>>;

/// Controls a running output thread. Dropping it stops output.
#[derive(Debug)]
pub struct OutputHandle {
    stop: Arc<AtomicBool>,
    stats: Arc<Mutex<OutputStats>>,
    pending: PendingPlan,
    thread: Option<JoinHandle<()>>,
}

impl OutputHandle {
    /// The most recently published stats: published once when output starts, then at least
    /// every 250 ms, and again at stop.
    pub fn stats(&self) -> OutputStats {
        self.stats.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Why the output thread stopped by itself, if it did (see [`OutputStats::failure`]).
    pub fn failure(&self) -> Option<String> {
        self.stats
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .failure
            .clone()
    }

    /// Switches the running output to `plan`, reading frames from `reader`, from the next frame
    /// on: controllers see no black frame in between (publish `reader`'s first frame before
    /// calling this). The sACN source (CID) and each stream's sequence numbers carry on, and a
    /// controller still in the plan at the same address keeps its health and counters. sACN
    /// universes and DDP controllers the new plan no longer sends to are blacked out (and their
    /// sACN streams terminated) over the next few frames. A plan given before the thread got to
    /// the previous one replaces it.
    pub fn replace_plan(&self, plan: OutputPlan, reader: FrameReader) {
        *self.pending.lock().unwrap_or_else(PoisonError::into_inner) = Some((plan, reader));
    }

    /// Sends the black frame three times (`BLACKOUT_REPEATS`, 20 ms apart), ends each sACN
    /// stream with Stream_Terminated packets, stops the thread, and returns the final stats.
    pub fn stop(mut self) -> OutputStats {
        self.shutdown();
        self.stats()
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take()
            && let Err(payload) = thread.join()
        {
            // The thread catches its own panics, so this is a last resort.
            let mut stats = self.stats.lock().unwrap_or_else(PoisonError::into_inner);
            stats
                .failure
                .get_or_insert_with(|| panic_message(payload.as_ref()));
        }
    }
}

impl Drop for OutputHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    let detail = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown error".to_string());
    format!("output stopped unexpectedly: {detail}")
}

enum Packets {
    Sacn(SacnPackets),
    Ddp(DdpPackets),
    None,
}

impl Packets {
    fn update(&mut self, channels: &[u8]) {
        match self {
            Packets::Sacn(p) => p.update(channels),
            Packets::Ddp(p) => p.update(channels),
            Packets::None => {}
        }
    }

    fn len(&self) -> usize {
        match self {
            Packets::Sacn(p) => p.len(),
            Packets::Ddp(p) => p.len(),
            Packets::None => 0,
        }
    }

    fn packet(&self, i: usize) -> (&[u8], SocketAddr) {
        match self {
            Packets::Sacn(p) => p.packet(i),
            Packets::Ddp(p) => p.packet(i),
            Packets::None => unreachable!("no packets"),
        }
    }

    fn set_terminated(&mut self, terminated: bool) {
        if let Packets::Sacn(p) = self {
            p.set_terminated(terminated);
        }
    }
}

/// One stream on the wire: an sACN universe at a destination, or a DDP destination.
type Stream = (SocketAddr, Option<u16>);

/// What one pass of sending a controller's packets came to.
#[derive(Default)]
struct Sent {
    packets: u64,
    errors: u64,
    /// A non-transient error, which stopped the pass.
    failure: Option<String>,
    /// Some sends were dropped because the send buffer was full.
    transient: bool,
}

/// Copies `channels` into the packets and sends them all.
fn transmit(packets: &mut Packets, channels: &[u8], transport: &mut dyn Transport) -> Sent {
    packets.update(channels);
    let mut sent = Sent::default();
    for i in 0..packets.len() {
        let (packet, destination) = packets.packet(i);
        match transport.send_to(packet, destination) {
            Ok(()) => sent.packets += 1,
            Err(e) if is_transient(&e) => {
                sent.errors += 1;
                sent.transient = true;
            }
            Err(e) => {
                sent.errors += 1;
                sent.failure = Some(e.to_string());
                break;
            }
        }
    }
    sent
}

struct Runtime {
    plan: ControllerPlan,
    buffer: Vec<u8>,
    packets: Packets,
    health: Health,
    stats: ControllerStats,
}

impl Runtime {
    fn new(plan: ControllerPlan, settings: &OutputSettings) -> Self {
        let (packets, state, last_error) = match (&plan.wire, &plan.destination) {
            (
                Wire::Sacn {
                    universes,
                    multicast: true,
                },
                destination,
            ) => {
                let unused = destination.clone().unwrap_or_else(|_| multicast_addr(1));
                (
                    Packets::Sacn(SacnPackets::new(universes, true, unused, settings)),
                    ControllerState::Ok,
                    None,
                )
            }
            (
                Wire::Sacn {
                    universes,
                    multicast: false,
                },
                Ok(destination),
            ) => (
                Packets::Sacn(SacnPackets::new(universes, false, *destination, settings)),
                ControllerState::Ok,
                None,
            ),
            (
                Wire::Ddp {
                    data_type,
                    offset_base,
                },
                Ok(destination),
            ) => (
                Packets::Ddp(DdpPackets::new(
                    plan.channel_count,
                    *data_type,
                    *offset_base,
                    *destination,
                )),
                ControllerState::Ok,
                None,
            ),
            (_, Err(reason)) => (Packets::None, ControllerState::Unresolved, Some(reason.clone())),
        };
        let stats = ControllerStats {
            controller: plan.id,
            name: plan.name.clone(),
            state,
            packets_sent: 0,
            send_errors: 0,
            last_error,
        };
        Self {
            buffer: vec![0; plan.channel_count],
            plan,
            packets,
            health: Health::new(state),
            stats,
        }
    }

    /// Where the multicast-or-unicast sACN universe `u` goes.
    fn sacn_destination(&self, multicast: bool, universe: u16) -> SocketAddr {
        if multicast {
            multicast_addr(universe)
        } else {
            self.plan
                .destination
                .clone()
                .unwrap_or_else(|_| multicast_addr(universe))
        }
    }

    /// Every stream this controller sends on.
    fn streams(&self) -> Vec<Stream> {
        match (&self.packets, &self.plan.wire) {
            (Packets::Sacn(_), Wire::Sacn { universes, multicast }) => universes
                .iter()
                .map(|u| (self.sacn_destination(*multicast, u.universe), Some(u.universe)))
                .collect(),
            (Packets::Ddp(p), _) if !p.is_empty() => vec![(p.destination(), None)],
            _ => Vec::new(),
        }
    }

    /// Sends one frame, unless the controller is unresolved or backing off after failures.
    fn send(&mut self, frame: &[u8], luts: &[[u8; 256]], transport: &mut dyn Transport, now: Instant) {
        if !self.health.ready(now) {
            return;
        }
        render_controller(frame, &self.plan, luts, &mut self.buffer);
        self.transmit(transport, now);
    }

    /// Sends black, even while backing off (unresolved controllers are never sent to).
    fn send_black(&mut self, transport: &mut dyn Transport, now: Instant) {
        if self.health.state == ControllerState::Unresolved {
            return;
        }
        self.buffer.fill(0);
        self.transmit(transport, now);
    }

    /// Ends an sACN controller's streams with Stream_Terminated packets.
    fn terminate(&mut self, transport: &mut dyn Transport, now: Instant) {
        if self.health.state == ControllerState::Unresolved || !matches!(self.packets, Packets::Sacn(_)) {
            return;
        }
        self.buffer.fill(0);
        self.packets.set_terminated(true);
        for _ in 0..TERMINATE_REPEATS {
            self.transmit(transport, now);
        }
    }

    fn transmit(&mut self, transport: &mut dyn Transport, now: Instant) {
        let sent = transmit(&mut self.packets, &self.buffer, transport);
        self.stats.packets_sent += sent.packets;
        self.stats.send_errors += sent.errors;
        match sent.failure {
            Some(message) => {
                self.health.on_failure(now);
                self.stats.last_error = Some(message);
            }
            None => {
                if sent.packets > 0 {
                    self.health.on_success();
                }
                if sent.transient {
                    if self.stats.last_error.as_deref() != Some(BUFFER_FULL_MESSAGE) {
                        self.stats.last_error = Some(BUFFER_FULL_MESSAGE.to_string());
                    }
                } else if sent.packets > 0 {
                    // Every packet went: an earlier error no longer describes the controller.
                    self.stats.last_error = None;
                }
            }
        }
        self.stats.state = self.health.state;
    }

    /// Takes on the health and counters of the same controller from the plan being replaced.
    fn carry_on_from(&mut self, old: &Runtime) {
        self.health = old.health.clone();
        self.stats.state = old.stats.state;
        self.stats.packets_sent = old.stats.packets_sent;
        self.stats.send_errors = old.stats.send_errors;
        self.stats.last_error.clone_from(&old.stats.last_error);
    }

    /// What of this controller's output a new plan no longer sends (`kept` are the new plan's
    /// streams), to black out: its dropped sACN universes, or the whole DDP controller.
    fn retire(
        self,
        kept: &HashSet<Stream>,
        settings: &OutputSettings,
        sequences: &SacnSequences,
    ) -> Option<Retiring> {
        let streams = self.streams();
        let buffer = vec![0; self.plan.channel_count];
        match (self.packets, &self.plan.wire) {
            (Packets::Sacn(_), Wire::Sacn { universes, multicast }) => {
                let dropped: Vec<UniverseSpan> = universes
                    .iter()
                    .zip(&streams)
                    .filter(|(_, stream)| !kept.contains(stream))
                    .map(|(u, _)| *u)
                    .collect();
                let first = streams.first()?.0;
                if dropped.is_empty() {
                    return None;
                }
                let mut packets = SacnPackets::new(&dropped, *multicast, first, settings);
                packets.continue_sequences(sequences);
                Some(Retiring {
                    packets: Packets::Sacn(packets),
                    buffer,
                    passes: 0,
                })
            }
            (packets @ Packets::Ddp(_), _) if streams.iter().any(|s| !kept.contains(s)) => Some(Retiring {
                packets,
                buffer,
                passes: 0,
            }),
            _ => None,
        }
    }
}

/// Output a replaced plan no longer sends, being blacked out over the next few frames.
struct Retiring {
    packets: Packets,
    /// All zero.
    buffer: Vec<u8>,
    passes: usize,
}

impl Retiring {
    /// Sends black once; the last time, ends sACN streams. Returns false once it is done.
    fn step(&mut self, transport: &mut dyn Transport) -> bool {
        transmit(&mut self.packets, &self.buffer, transport);
        self.passes += 1;
        if self.passes < BLACKOUT_REPEATS {
            return true;
        }
        self.terminate(transport);
        false
    }

    /// Ends sACN streams with Stream_Terminated packets (DDP has no such thing).
    fn terminate(&mut self, transport: &mut dyn Transport) {
        if matches!(self.packets, Packets::Sacn(_)) {
            self.packets.set_terminated(true);
            for _ in 0..TERMINATE_REPEATS {
                transmit(&mut self.packets, &self.buffer, transport);
            }
        }
    }

    fn streams(&self) -> Vec<Stream> {
        (0..self.packets.len())
            .map(|i| {
                let (packet, to) = self.packets.packet(i);
                match self.packets {
                    Packets::Sacn(_) => (to, Some(u16::from_be_bytes([packet[113], packet[114]]))),
                    _ => (to, None),
                }
            })
            .collect()
    }
}

/// Everything the output thread works with.
struct Output {
    settings: OutputSettings,
    runtimes: Vec<Runtime>,
    luts: Vec<[u8; 256]>,
    retiring: Vec<Retiring>,
    reader: FrameReader,
    clock: FrameClock,
    frame_rate: u16,
    sync_sequence: u8,
}

impl Output {
    fn new(plan: OutputPlan, reader: FrameReader, settings: OutputSettings) -> Self {
        let runtimes = plan
            .controllers
            .into_iter()
            .map(|c| Runtime::new(c, &settings))
            .collect();
        Self {
            runtimes,
            luts: plan.luts,
            retiring: Vec::new(),
            reader,
            clock: FrameClock::new(plan.frame_rate),
            frame_rate: plan.frame_rate,
            sync_sequence: 0,
            settings,
        }
    }

    /// Switches to a new plan between frames (see [`OutputHandle::replace_plan`]).
    fn replace(&mut self, plan: OutputPlan, reader: FrameReader) {
        let mut sequences = SacnSequences::new();
        for old in &self.runtimes {
            if let Packets::Sacn(p) = &old.packets {
                p.save_sequences(&mut sequences);
            }
        }
        for retiring in &self.retiring {
            if let Packets::Sacn(p) = &retiring.packets {
                p.save_sequences(&mut sequences);
            }
        }
        let mut runtimes: Vec<Runtime> = plan
            .controllers
            .into_iter()
            .map(|c| Runtime::new(c, &self.settings))
            .collect();
        for runtime in &mut runtimes {
            if let Packets::Sacn(p) = &mut runtime.packets {
                p.continue_sequences(&sequences);
            }
            if let Some(old) = self
                .runtimes
                .iter()
                .find(|o| o.plan.id == runtime.plan.id && o.plan.destination == runtime.plan.destination)
            {
                runtime.carry_on_from(old);
            }
        }
        let kept: HashSet<Stream> = runtimes.iter().flat_map(Runtime::streams).collect();
        // Output being blacked out that the new plan sends to again is the new plan's now.
        self.retiring
            .retain(|r| r.streams().iter().all(|s| !kept.contains(s)));
        for old in std::mem::take(&mut self.runtimes) {
            if let Some(retiring) = old.retire(&kept, &self.settings, &sequences) {
                self.retiring.push(retiring);
            }
        }
        self.runtimes = runtimes;
        self.luts = plan.luts;
        self.reader = reader;
        if plan.frame_rate != self.frame_rate {
            self.frame_rate = plan.frame_rate;
            self.clock = FrameClock::new(plan.frame_rate);
        }
    }

    /// Sends the latest frame to every controller, and black to retiring output.
    fn frame(&mut self, transport: &mut dyn Transport) {
        let now = Instant::now();
        let frame = self.reader.latest();
        for runtime in &mut self.runtimes {
            runtime.send(frame, &self.luts, transport, now);
        }
        self.retiring.retain_mut(|r| r.step(transport));
        self.sync(transport);
    }

    fn sync(&mut self, transport: &mut dyn Transport) {
        if let Some(universe) = self.settings.sync_universe {
            self.sync_sequence = self.sync_sequence.wrapping_add(1);
            let packet = sync_packet(&self.settings, universe, self.sync_sequence);
            let _ = transport.send_to(&packet, multicast_addr(universe));
        }
    }

    /// The final blackout: black three times, spaced out, then Stream_Terminated on sACN.
    fn blackout(&mut self, transport: &mut dyn Transport) {
        for pass in 0..BLACKOUT_REPEATS {
            if pass > 0 {
                std::thread::sleep(BLACKOUT_GAP);
            }
            let now = Instant::now();
            for runtime in &mut self.runtimes {
                runtime.send_black(transport, now);
            }
            for retiring in &mut self.retiring {
                transmit(&mut retiring.packets, &retiring.buffer, transport);
            }
            self.sync(transport);
        }
        let now = Instant::now();
        for runtime in &mut self.runtimes {
            runtime.terminate(transport, now);
        }
        for mut retiring in std::mem::take(&mut self.retiring) {
            retiring.terminate(transport);
        }
    }
}

/// Starts the output thread. It sends the latest published frame every period until the
/// returned handle is stopped or dropped, then blacks out (see [`OutputHandle::stop`]).
pub fn start_output(
    plan: OutputPlan,
    settings: OutputSettings,
    reader: FrameReader,
    mut transport: Box<dyn Transport>,
) -> OutputHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(Mutex::new(OutputStats::default()));
    let pending: PendingPlan = Arc::new(Mutex::new(None));
    let thread_stop = Arc::clone(&stop);
    let thread_stats = Arc::clone(&stats);
    let thread_pending = Arc::clone(&pending);
    let thread = std::thread::Builder::new()
        .name("pixelflow-output".into())
        .spawn(move || {
            let run = AssertUnwindSafe(|| {
                run(
                    Output::new(plan, reader, settings),
                    transport.as_mut(),
                    &thread_stop,
                    &thread_stats,
                    &thread_pending,
                );
            });
            if let Err(payload) = std::panic::catch_unwind(run) {
                thread_stats
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .failure = Some(panic_message(payload.as_ref()));
            }
        })
        .expect("spawn output thread");
    OutputHandle {
        stop,
        stats,
        pending,
        thread: Some(thread),
    }
}

fn run(
    mut output: Output,
    transport: &mut dyn Transport,
    stop: &AtomicBool,
    stats: &Mutex<OutputStats>,
    pending: &Mutex<Option<(OutputPlan, FrameReader)>>,
) {
    let mut session = OutputStats::default();
    let started = Instant::now();
    publish(stats, &mut session, &output.runtimes, started);
    let mut last_publish = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let late = output.clock.wait();
        // Never wait on the lock here: a plan handed over mid-frame is picked up next frame.
        if let Ok(mut next) = pending.try_lock()
            && let Some((plan, reader)) = next.take()
        {
            drop(next);
            output.replace(plan, reader);
        }
        output.frame(transport);
        session.frames += 1;
        session.late_frames += u64::from(late);
        if last_publish.elapsed() >= STATS_INTERVAL {
            publish(stats, &mut session, &output.runtimes, started);
            last_publish = Instant::now();
        }
    }
    // A plan handed over just before stopping: black out its controllers too.
    if let Some((plan, reader)) = pending.lock().unwrap_or_else(PoisonError::into_inner).take() {
        output.replace(plan, reader);
    }
    output.blackout(transport);
    publish(stats, &mut session, &output.runtimes, started);
}

fn publish(shared: &Mutex<OutputStats>, session: &mut OutputStats, runtimes: &[Runtime], started: Instant) {
    let elapsed = started.elapsed().as_secs_f32();
    session.achieved_fps = if elapsed > 0.0 {
        session.frames as f32 / elapsed
    } else {
        0.0
    };
    session.controllers = runtimes.iter().map(|r| r.stats.clone()).collect();
    *shared.lock().unwrap_or_else(PoisonError::into_inner) = session.clone();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_full_and_would_block_are_transient_but_unreachable_is_not() {
        #[cfg(target_os = "macos")]
        const ENOBUFS: i32 = 55;
        #[cfg(target_os = "linux")]
        const ENOBUFS: i32 = 105;
        #[cfg(windows)]
        const ENOBUFS: i32 = 10055;
        assert!(is_transient(&io::Error::from_raw_os_error(ENOBUFS)));
        assert!(is_transient(&io::Error::from(io::ErrorKind::WouldBlock)));
        assert!(!is_transient(&io::Error::from(io::ErrorKind::HostUnreachable)));
    }

    #[test]
    fn panic_messages_name_what_went_wrong() {
        let payload: Box<dyn std::any::Any + Send> = Box::new("boom");
        assert_eq!(
            panic_message(payload.as_ref()),
            "output stopped unexpectedly: boom"
        );
        let payload: Box<dyn std::any::Any + Send> = Box::new(String::from("bang"));
        assert_eq!(
            panic_message(payload.as_ref()),
            "output stopped unexpectedly: bang"
        );
        let payload: Box<dyn std::any::Any + Send> = Box::new(7);
        assert_eq!(
            panic_message(payload.as_ref()),
            "output stopped unexpectedly: unknown error"
        );
    }
}
