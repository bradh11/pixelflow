//! Where the benchmark's packets go: nowhere at all, or to listeners on this machine's loopback
//! interface that count them. Never anywhere else: [`Guarded`] refuses every destination that
//! isn't a loopback address, whatever the show says, and its socket is bound to loopback too.

use pf_output::{Transport, UdpTransport};
use serde::{Deserialize, Serialize};
use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

/// What the pipeline sends into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "camelCase")]
pub enum SinkKind {
    /// Packets are built and dropped: no sockets, to measure the engine's own work.
    Null,
    /// Packets are sent over UDP to listeners on 127.0.0.1 that count them.
    Loopback,
}

impl SinkKind {
    pub const ALL: [SinkKind; 2] = [SinkKind::Null, SinkKind::Loopback];

    pub fn label(self) -> &'static str {
        match self {
            SinkKind::Null => "null",
            SinkKind::Loopback => "loopback",
        }
    }
}

/// Whether a send error means "the send buffer is full; this packet was dropped" rather than
/// "this destination can't be reached" (the same test `pf-output`'s output thread makes).
pub(crate) fn is_transient(e: &io::Error) -> bool {
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

/// What went through a [`Guarded`] transport, shared with whoever reports it.
#[derive(Debug, Default)]
pub struct Sent {
    pub packets: AtomicU64,
    pub bytes: AtomicU64,
    /// Packets dropped because the send buffer was full.
    pub dropped: AtomicU64,
    /// Sends that failed for any other reason.
    pub failed: AtomicU64,
    /// Sends refused because the destination wasn't a loopback address.
    pub refused: AtomicU64,
}

/// The counters at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SentNow {
    pub packets: u64,
    pub bytes: u64,
    pub dropped: u64,
    pub failed: u64,
    pub refused: u64,
}

impl Sent {
    pub fn now(&self) -> SentNow {
        SentNow {
            packets: self.packets.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            refused: self.refused.load(Ordering::Relaxed),
        }
    }
}

/// A transport that only ever sends to this machine: the engine's own UDP transport behind a
/// check of every destination, or nothing at all (the null sink).
pub struct Guarded {
    udp: Option<UdpTransport>,
    sent: Arc<Sent>,
}

impl Guarded {
    /// A transport into the `kind` sink that counts into `sent`: nothing at all for the null
    /// sink, the engine's UDP transport on a socket of the loopback interface for the other.
    pub fn open(kind: SinkKind, sent: Arc<Sent>) -> io::Result<Self> {
        let udp = match kind {
            SinkKind::Null => None,
            SinkKind::Loopback => Some(UdpTransport::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))?),
        };
        Ok(Self { udp, sent })
    }
}

impl Transport for Guarded {
    fn send_to(&mut self, packet: &[u8], destination: SocketAddr) -> io::Result<()> {
        if !destination.ip().is_loopback() {
            self.sent.refused.fetch_add(1, Ordering::Relaxed);
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("the benchmark only sends to this machine, not to {destination}"),
            ));
        }
        let result = match &mut self.udp {
            Some(udp) => udp.send_to(packet, destination),
            None => Ok(()),
        };
        match &result {
            Ok(()) => {
                self.sent.packets.fetch_add(1, Ordering::Relaxed);
                self.sent.bytes.fetch_add(packet.len() as u64, Ordering::Relaxed);
            }
            Err(e) if is_transient(e) => {
                self.sent.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => {
                self.sent.failed.fetch_add(1, Ordering::Relaxed);
            }
        }
        result
    }
}

/// The receive buffer asked for on each listener (the system may give less).
const RECEIVE_BUFFER: usize = 4 << 20;
/// How long the receiver sleeps when nothing is waiting.
const IDLE: Duration = Duration::from_micros(500);

/// UDP listeners on the loopback interface, one per controller, read by one thread that counts
/// what arrives.
pub struct Listeners {
    addresses: Vec<SocketAddr>,
    packets: Arc<AtomicU64>,
    bytes: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Listeners {
    /// Binds `count` listeners, each on a port of its own that the system picks.
    pub fn start(count: usize) -> io::Result<Self> {
        let mut sockets = Vec::with_capacity(count);
        for _ in 0..count {
            let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
            socket.set_nonblocking(true)?;
            // A burst of a whole frame should fit; a smaller buffer only means more loss counted.
            let _ = socket2::SockRef::from(&socket).set_recv_buffer_size(RECEIVE_BUFFER);
            sockets.push(socket);
        }
        let addresses = sockets
            .iter()
            .map(UdpSocket::local_addr)
            .collect::<io::Result<Vec<_>>>()?;
        let packets = Arc::new(AtomicU64::new(0));
        let bytes = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (count_packets, count_bytes, stopping) =
            (Arc::clone(&packets), Arc::clone(&bytes), Arc::clone(&stop));
        let thread = std::thread::Builder::new()
            .name("pf-bench-listeners".into())
            .spawn(move || {
                let mut buffer = [0u8; 2048];
                loop {
                    // Read first, so a stop still drains what was already sent.
                    let stopped = stopping.load(Ordering::Relaxed);
                    let (mut got, mut size) = (0u64, 0u64);
                    for socket in &sockets {
                        while let Ok(n) = socket.recv(&mut buffer) {
                            got += 1;
                            size += n as u64;
                        }
                    }
                    count_packets.fetch_add(got, Ordering::Relaxed);
                    count_bytes.fetch_add(size, Ordering::Relaxed);
                    if got == 0 {
                        if stopped {
                            break;
                        }
                        std::thread::sleep(IDLE);
                    }
                }
            })?;
        Ok(Self {
            addresses,
            packets,
            bytes,
            stop,
            thread: Some(thread),
        })
    }

    /// Where each listener listens, in the order they were made.
    pub fn addresses(&self) -> &[SocketAddr] {
        &self.addresses
    }

    /// Packets and bytes received so far.
    pub fn received(&self) -> (u64, u64) {
        (
            self.packets.load(Ordering::Relaxed),
            self.bytes.load(Ordering::Relaxed),
        )
    }

    /// Reads what is still on its way, stops listening, and says what arrived in all.
    pub fn finish(mut self) -> (u64, u64) {
        self.shutdown();
        self.received()
    }

    fn shutdown(&mut self) {
        // Loopback delivery is quick but not instant: give the last packets a moment to land.
        std::thread::sleep(Duration::from_millis(20));
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Listeners {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn open(kind: SinkKind) -> (Guarded, Arc<Sent>) {
        let sent = Arc::new(Sent::default());
        (Guarded::open(kind, Arc::clone(&sent)).unwrap(), sent)
    }

    #[test]
    fn nothing_leaves_the_machine_whichever_sink_is_used() {
        let outside: SocketAddr = "192.0.2.10:4048".parse().unwrap();
        let multicast: SocketAddr = "239.255.0.1:5568".parse().unwrap();
        for kind in SinkKind::ALL {
            let (mut transport, sent) = open(kind);
            for destination in [outside, multicast] {
                let error = transport.send_to(&[1, 2, 3], destination).unwrap_err();
                assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
                assert!(!is_transient(&error));
            }
            assert_eq!((sent.now().refused, sent.now().packets), (2, 0), "{kind:?}");
        }
    }

    #[test]
    fn the_null_sink_counts_and_the_listeners_hear_the_loopback_sink() {
        let (mut null, counted) = open(SinkKind::Null);
        null.send_to(&[0; 100], "127.0.0.1:4048".parse().unwrap())
            .unwrap();
        assert_eq!((counted.now().packets, counted.now().bytes), (1, 100));

        let listeners = Listeners::start(2).unwrap();
        assert!(listeners.addresses().iter().all(|a| a.ip().is_loopback()));
        let (mut transport, sent) = open(SinkKind::Loopback);
        for &address in listeners.addresses() {
            transport.send_to(&[7; 64], address).unwrap();
        }
        assert_eq!((sent.now().packets, sent.now().bytes), (2, 128));
        // However long this machine takes to hand two packets across.
        let deadline = Instant::now() + Duration::from_secs(10);
        while listeners.received().0 < 2 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(listeners.finish(), (2, 128));
    }

    #[test]
    fn a_full_send_buffer_is_a_dropped_packet_not_a_dead_controller() {
        assert!(is_transient(&io::Error::from(io::ErrorKind::WouldBlock)));
        assert!(!is_transient(&io::Error::from(io::ErrorKind::HostUnreachable)));
    }
}
