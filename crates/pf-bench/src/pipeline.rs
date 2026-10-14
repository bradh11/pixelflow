//! The pipeline under test: render, map to channels, packetize, send. It is put together from
//! the engine's own parts, the way the engine puts them together:
//!
//! - **render**: [`pf_render::Renderer`], as live playback of an authored sequence and FSEQ
//!   export use it (`pf-engine`'s `playback.rs`, `pf-render`'s `export.rs`);
//! - **preview**: the two copies playback makes of every frame for the app to read;
//! - **map**: [`pf_output::render_controller`] (wiring order, color order, brightness, gamma);
//! - **packetize**: [`SacnPackets`] and [`DdpPackets`];
//! - **send**: a [`Transport`], one packet at a time.
//!
//! [`Controllers::send`] is the output thread's pass over the controllers (`pf-output`'s
//! `engine.rs`) with a stopwatch on each stage. The tests check that a pass puts the same bytes
//! on the wire as the engine's own output thread does.

use crate::sequence::{Media, SyntheticSequence, Variant, synthetic_media, synthetic_sequence};
use crate::show::{SyntheticShow, synthetic_show};
use crate::sink::is_transient;
use crate::stats::ms;
use anyhow::{Result, bail};
use pf_mapping::ChannelMap;
use pf_output::ddp::DdpPackets;
use pf_output::sacn::SacnPackets;
use pf_output::{ControllerPlan, OutputPlan, OutputSettings, Transport, Wire, render_controller};
use pf_render::{RenderPool, Renderer};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The fastest the output thread sends (`pf-engine`'s `MAX_SEND_RATE`).
const MAX_SEND_RATE: u32 = 120;

/// How often live playback's output thread sends a sequence with `frame_ms` frames: twice the
/// sequence's rate, so every frame goes out at least once (`pf-engine`'s `send_rate`).
pub fn engine_send_rate(frame_ms: u32) -> u16 {
    u16::try_from((2000 / frame_ms.max(1)).clamp(1, MAX_SEND_RATE)).unwrap_or(1)
}

/// How long each part of building a workload took, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Setup {
    pub show_ms: f64,
    pub map_ms: f64,
    pub sequence_ms: f64,
    /// Working out the song's audio track and making the picture.
    pub media_ms: f64,
}

/// The threads a run's frames are drawn on.
#[derive(Debug, Clone, Default)]
pub struct RenderThreads(Option<Option<Arc<RenderPool>>>);

impl RenderThreads {
    /// As the engine chooses: the pool its renderers share, for frames worth sharing out.
    pub fn engines() -> Self {
        Self(None)
    }

    /// Exactly `threads` (one: the thread that asks for the frame), however small the frame: for
    /// measuring how drawing scales.
    pub fn exactly(threads: usize) -> Result<Self> {
        if threads <= 1 {
            return Ok(Self(Some(None)));
        }
        match RenderPool::new(threads) {
            Some(pool) => Ok(Self(Some(Some(pool)))),
            None => bail!("couldn't start {threads} threads to draw on"),
        }
    }

    /// Has `renderer` draw on these threads.
    pub fn apply(&self, renderer: &mut Renderer) {
        if let Some(pool) = &self.0 {
            renderer.set_pool(pool.clone());
        }
    }
}

/// A synthetic show with its channel map, a synthetic sequence, and the sequence's music and
/// picture.
pub struct Workload {
    pub show: SyntheticShow,
    pub map: ChannelMap,
    pub sequence: SyntheticSequence,
    pub media: Media,
    pub setup: Setup,
    /// The threads its frames are drawn on.
    pub threads: RenderThreads,
}

impl Workload {
    /// Builds the show with `pixels` pixels and the `variant` sequence for it, `sequence_seconds`
    /// long.
    pub fn new(pixels: u32, variant: Variant, sequence_seconds: u32) -> Result<Self> {
        let started = Instant::now();
        let show = synthetic_show(pixels);
        let first_error = |report: &pf_model::ValidationReport| {
            let error = report
                .issues
                .iter()
                .find(|i| i.severity == pf_model::Severity::Error);
            error.map(|issue| issue.message.clone())
        };
        if let Some(problem) = first_error(&pf_model::validate_show(&show.show)) {
            bail!("the synthetic show has a problem: {problem}");
        }
        let show_ms = ms(started.elapsed());
        let started = Instant::now();
        let (map, report) = pf_mapping::map_show(&show.show);
        if let Some(problem) = first_error(&report) {
            bail!("the synthetic show can't be mapped: {problem}");
        }
        let map_ms = ms(started.elapsed());
        let started = Instant::now();
        let sequence = synthetic_sequence(&show, variant, sequence_seconds);
        if let Some(problem) = pf_sequence::limit_problems(&sequence.sequence).into_iter().next() {
            bail!("the synthetic sequence has a problem: {problem}");
        }
        let sequence_ms = ms(started.elapsed());
        let started = Instant::now();
        let media = synthetic_media(sequence_seconds);
        Ok(Self {
            show,
            map,
            sequence,
            media,
            setup: Setup {
                show_ms,
                map_ms,
                sequence_ms,
                media_ms: ms(started.elapsed()),
            },
            threads: RenderThreads::engines(),
        })
    }

    /// The same workload without its music and picture: what plays when the sequence is opened
    /// in the engine itself, which can't be handed either from memory.
    pub fn without_media(mut self) -> Self {
        self.media = Media::none();
        self
    }

    /// The same workload with its frames drawn on `threads`.
    pub fn drawn_on(mut self, threads: RenderThreads) -> Self {
        self.threads = threads;
        self
    }

    /// The show with each controller at its own listener (`addresses`), when given.
    pub fn show_at(&self, addresses: Option<&[SocketAddr]>) -> pf_model::Show {
        let mut show = self.show.show.clone();
        if let Some(addresses) = addresses {
            for (controller, address) in show.controllers.iter_mut().zip(addresses) {
                controller.address = address.to_string();
            }
        }
        show
    }

    /// The show's output plan, as live playback builds it.
    pub fn plan(&self, addresses: Option<&[SocketAddr]>) -> OutputPlan {
        pf_output::build_plan(&self.show_at(addresses), &self.map)
    }

    /// A renderer for the show with the sequence's music and picture, as playback makes one.
    pub fn renderer(&self) -> Renderer {
        let mut renderer = Renderer::new(&self.show.show, &self.map);
        renderer.set_audio(self.media.audio.clone());
        renderer.set_pictures(self.media.pictures.clone());
        self.threads.apply(&mut renderer);
        renderer
    }

    pub fn frame_ms(&self) -> u32 {
        self.sequence.sequence.frame_ms.max(1)
    }
}

enum Packets {
    Sacn(SacnPackets),
    Ddp(DdpPackets),
}

impl Packets {
    fn update(&mut self, channels: &[u8]) {
        match self {
            Packets::Sacn(p) => p.update(channels),
            Packets::Ddp(p) => p.update(channels),
        }
    }

    fn len(&self) -> usize {
        match self {
            Packets::Sacn(p) => p.len(),
            Packets::Ddp(p) => p.len(),
        }
    }

    fn packet(&self, i: usize) -> (&[u8], SocketAddr) {
        match self {
            Packets::Sacn(p) => p.packet(i),
            Packets::Ddp(p) => p.packet(i),
        }
    }
}

/// One controller as the output thread holds it: its plan, channel buffer, and packets.
struct Runtime {
    plan: ControllerPlan,
    buffer: Vec<u8>,
    packets: Packets,
}

/// What one pass over the controllers cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pass {
    pub map: Duration,
    pub packetize: Duration,
    pub send: Duration,
}

impl Pass {
    pub fn total(&self) -> Duration {
        self.map + self.packetize + self.send
    }
}

/// What a show of this size puts on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wiring {
    pub channels: usize,
    pub controllers: usize,
    pub universes: usize,
    pub packets_per_pass: usize,
    /// Bytes handed to the transport each pass, sACN and DDP headers included (not UDP's or IP's).
    pub bytes_per_pass: usize,
}

/// Every controller of an output plan, ready to send frames: what `pf-output`'s output thread
/// works with.
pub struct Controllers {
    runtimes: Vec<Runtime>,
    luts: Vec<[u8; 256]>,
}

impl Controllers {
    /// Refuses a plan that would send anywhere but this machine.
    pub fn new(plan: OutputPlan, settings: &OutputSettings) -> Result<Self> {
        let mut runtimes = Vec::with_capacity(plan.controllers.len());
        for controller in plan.controllers {
            let destination = match &controller.destination {
                Ok(destination) if destination.ip().is_loopback() => *destination,
                Ok(destination) => bail!(
                    "{} is at {destination}, which isn't this machine",
                    controller.name
                ),
                Err(reason) => bail!("{} has no address to send to: {reason}", controller.name),
            };
            let packets = match &controller.wire {
                Wire::Sacn { multicast: true, .. } => {
                    bail!(
                        "{} sends by multicast, which would leave this machine",
                        controller.name
                    )
                }
                Wire::Sacn { universes, .. } => {
                    Packets::Sacn(SacnPackets::new(universes, false, destination, settings))
                }
                Wire::Ddp {
                    data_type,
                    offset_base,
                } => Packets::Ddp(DdpPackets::new(
                    controller.channel_count,
                    *data_type,
                    *offset_base,
                    destination,
                )),
                Wire::ArtNet { .. } | Wire::DmxUsb { .. } => {
                    bail!(
                        "{} is a DMX controller, which the benchmark doesn't measure",
                        controller.name
                    )
                }
            };
            runtimes.push(Runtime {
                buffer: vec![0; controller.channel_count],
                plan: controller,
                packets,
            });
        }
        Ok(Self {
            runtimes,
            luts: plan.luts,
        })
    }

    /// Sends one frame to every controller, as the output thread does each period: gather the
    /// controller's channels, copy them into its packets, send each packet.
    pub fn send(&mut self, frame: &[u8], transport: &mut dyn Transport) -> Pass {
        let mut pass = Pass::default();
        for runtime in &mut self.runtimes {
            let started = Instant::now();
            render_controller(frame, &runtime.plan, &self.luts, &mut runtime.buffer);
            let mapped = Instant::now();
            runtime.packets.update(&runtime.buffer);
            let packetized = Instant::now();
            for i in 0..runtime.packets.len() {
                let (packet, destination) = runtime.packets.packet(i);
                // A full send buffer drops the packet and carries on; any other failure ends the
                // controller's pass (the transport counts both).
                if let Err(e) = transport.send_to(packet, destination)
                    && !is_transient(&e)
                {
                    break;
                }
            }
            pass.map += mapped - started;
            pass.packetize += packetized - mapped;
            pass.send += packetized.elapsed();
        }
        pass
    }

    pub fn wiring(&self) -> Wiring {
        let packets = |r: &Runtime| r.packets.len();
        Wiring {
            channels: self.runtimes.iter().map(|r| r.plan.channel_count).sum(),
            controllers: self.runtimes.len(),
            universes: self
                .runtimes
                .iter()
                .filter(|r| matches!(r.packets, Packets::Sacn(_)))
                .map(packets)
                .sum(),
            packets_per_pass: self.runtimes.iter().map(packets).sum(),
            bytes_per_pass: self
                .runtimes
                .iter()
                .flat_map(|r| (0..r.packets.len()).map(|i| r.packets.packet(i).0.len()))
                .sum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_frame::frame_buffers;
    use pf_output::{RecordingTransport, start_output};

    #[test]
    fn the_output_thread_sends_at_twice_the_sequences_rate() {
        assert_eq!(engine_send_rate(25), 80);
        assert_eq!(engine_send_rate(50), 40);
        assert_eq!(engine_send_rate(10), 120);
        assert_eq!(engine_send_rate(0), 120);
    }

    #[test]
    fn a_pass_sends_what_the_engines_output_thread_sends() {
        // Large enough for sACN and DDP controllers both.
        let workload = Workload::new(6_000, Variant::Typical, 4).unwrap();
        let plan = workload.plan(None);
        let settings = OutputSettings::default();
        let frame: Vec<u8> = (0..plan.frame_len).map(|i| (i * 7 % 251) as u8).collect();

        // The engine's own output thread, sending one published frame over and over.
        let (transport, engine_sent) = RecordingTransport::new();
        let (mut writer, reader) = frame_buffers(plan.frame_len);
        writer.frame_mut().copy_from_slice(&frame);
        writer.publish();
        let mut controllers = Controllers::new(plan.clone(), &settings).unwrap();
        let wiring = controllers.wiring();
        assert!(wiring.universes > 0 && wiring.packets_per_pass > wiring.universes);
        let handle = start_output(plan, settings, reader, Box::new(transport));
        let deadline = Instant::now() + Duration::from_secs(10);
        while engine_sent.lock().unwrap().len() < wiring.packets_per_pass {
            assert!(Instant::now() < deadline, "the output thread never sent a pass");
            std::thread::sleep(Duration::from_millis(2));
        }
        drop(handle);

        let (mut transport, bench_sent) = RecordingTransport::new();
        controllers.send(&frame, &mut transport);
        let bench_sent = bench_sent.lock().unwrap();
        let engine_sent = engine_sent.lock().unwrap();
        assert_eq!(bench_sent.len(), wiring.packets_per_pass);
        assert_eq!(
            bench_sent.iter().map(|(p, _)| p.len()).sum::<usize>(),
            wiring.bytes_per_pass
        );
        // The same packets to the same places, in the same order.
        assert_eq!(*bench_sent, engine_sent[..wiring.packets_per_pass]);
    }

    #[test]
    fn controllers_anywhere_but_this_machine_are_refused() {
        let workload = Workload::new(1_500, Variant::Typical, 4).unwrap();
        let mut plan = workload.plan(None);
        plan.controllers[0].destination = Ok("192.0.2.7:4048".parse().unwrap());
        let error = Controllers::new(plan, &OutputSettings::default()).err().unwrap();
        assert!(error.to_string().contains("isn't this machine"), "{error}");
    }
}
