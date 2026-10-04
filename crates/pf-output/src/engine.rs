//! The output thread: paces frames and sends every controller's packets.

use crate::clock::FrameClock;
use crate::ddp::DdpPackets;
use crate::gather::render_controller;
use crate::health::{ControllerState, Health};
use crate::plan::{ControllerPlan, OutputPlan, Wire};
use crate::sacn::{SacnPackets, multicast_addr, sync_packet};
use crate::settings::OutputSettings;
use crate::transport::Transport;
use pf_frame::FrameReader;
use pf_model::ControllerId;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Publish stats to readers at least this often.
const STATS_INTERVAL: Duration = Duration::from_millis(250);

/// How many times the final blackout frame is sent (UDP is lossy).
const BLACKOUT_REPEATS: usize = 3;

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
}

/// Controls a running output thread. Dropping it stops output.
#[derive(Debug)]
pub struct OutputHandle {
    stop: Arc<AtomicBool>,
    stats: Arc<Mutex<OutputStats>>,
    thread: Option<JoinHandle<()>>,
}

impl OutputHandle {
    /// The most recently published stats: published once when output starts, then at least
    /// every 250 ms, and again at stop.
    pub fn stats(&self) -> OutputStats {
        self.stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Sends one all-black frame, stops the thread, and returns the final stats.
    pub fn stop(mut self) -> OutputStats {
        self.shutdown();
        self.stats()
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for OutputHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
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
            (Wire::Ddp { data_type }, Ok(destination)) => (
                Packets::Ddp(DdpPackets::new(plan.channel_count, *data_type, *destination)),
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

    /// Sends one frame. `force` skips the backoff wait for degraded controllers (used for the
    /// final blackout); unresolved controllers are never sent to.
    fn send(
        &mut self,
        frame: &[u8],
        luts: &[[u8; 256]],
        transport: &mut dyn Transport,
        now: Instant,
        force: bool,
    ) {
        let due = if force {
            self.health.state != ControllerState::Unresolved
        } else {
            self.health.ready(now)
        };
        if !due {
            return;
        }
        render_controller(frame, &self.plan, luts, &mut self.buffer);
        self.packets.update(&self.buffer);
        let mut failure = None;
        let mut sent = 0u64;
        let mut transient = false;
        for i in 0..self.packets.len() {
            let (packet, destination) = self.packets.packet(i);
            match transport.send_to(packet, destination) {
                Ok(()) => {
                    self.stats.packets_sent += 1;
                    sent += 1;
                }
                Err(e) if is_transient(&e) => {
                    self.stats.send_errors += 1;
                    transient = true;
                }
                Err(e) => {
                    self.stats.send_errors += 1;
                    failure = Some(e.to_string());
                    break;
                }
            }
        }
        match failure {
            Some(message) => {
                self.health.on_failure(now);
                self.stats.last_error = Some(message);
            }
            None => {
                if sent > 0 {
                    self.health.on_success();
                }
                if transient && self.stats.last_error.as_deref() != Some(BUFFER_FULL_MESSAGE) {
                    self.stats.last_error = Some(BUFFER_FULL_MESSAGE.to_string());
                }
            }
        }
        self.stats.state = self.health.state;
    }
}

/// Starts the output thread. It sends the latest published frame every period until the
/// returned handle is stopped or dropped, then sends one all-black frame.
pub fn start_output(
    plan: OutputPlan,
    settings: OutputSettings,
    mut reader: FrameReader,
    mut transport: Box<dyn Transport>,
) -> OutputHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stats = Arc::new(Mutex::new(OutputStats::default()));
    let thread_stop = Arc::clone(&stop);
    let thread_stats = Arc::clone(&stats);
    let thread = std::thread::Builder::new()
        .name("pixelflow-output".into())
        .spawn(move || {
            let OutputPlan {
                frame_len,
                frame_rate,
                controllers,
                luts,
            } = plan;
            let mut runtimes: Vec<Runtime> = controllers
                .into_iter()
                .map(|c| Runtime::new(c, &settings))
                .collect();
            let mut clock = FrameClock::new(frame_rate);
            let mut sync_sequence: u8 = 0;
            let mut session = OutputStats::default();
            let started = Instant::now();

            let mut send_frame =
                |frame: &[u8], runtimes: &mut Vec<Runtime>, transport: &mut dyn Transport, force: bool| {
                    let now = Instant::now();
                    for runtime in runtimes.iter_mut() {
                        runtime.send(frame, &luts, transport, now, force);
                    }
                    if let Some(universe) = settings.sync_universe {
                        sync_sequence = sync_sequence.wrapping_add(1);
                        let packet = sync_packet(&settings, universe, sync_sequence);
                        let _ = transport.send_to(&packet, multicast_addr(universe));
                    }
                };

            publish(&thread_stats, &mut session, &runtimes, started);
            let mut last_publish = Instant::now();
            while !thread_stop.load(Ordering::Relaxed) {
                let late = clock.wait();
                send_frame(reader.latest(), &mut runtimes, transport.as_mut(), false);
                session.frames += 1;
                session.late_frames += u64::from(late);
                if last_publish.elapsed() >= STATS_INTERVAL {
                    publish(&thread_stats, &mut session, &runtimes, started);
                    last_publish = Instant::now();
                }
            }
            let black = vec![0; frame_len];
            for _ in 0..BLACKOUT_REPEATS {
                send_frame(&black, &mut runtimes, transport.as_mut(), true);
            }
            publish(&thread_stats, &mut session, &runtimes, started);
        })
        .expect("spawn output thread");
    OutputHandle {
        stop,
        stats,
        thread: Some(thread),
    }
}

fn publish(shared: &Mutex<OutputStats>, session: &mut OutputStats, runtimes: &[Runtime], started: Instant) {
    let elapsed = started.elapsed().as_secs_f32();
    session.achieved_fps = if elapsed > 0.0 {
        session.frames as f32 / elapsed
    } else {
        0.0
    };
    session.controllers = runtimes.iter().map(|r| r.stats.clone()).collect();
    *shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = session.clone();
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
}
