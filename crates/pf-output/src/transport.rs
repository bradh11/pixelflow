//! Where packets go: real UDP sockets, or an in-memory recorder for tests.

use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Sends one packet to one destination.
pub trait Transport: Send {
    fn send_to(&mut self, packet: &[u8], destination: SocketAddr) -> io::Result<()>;
}

/// A non-blocking UDP socket, so a slow or unreachable controller can never stall output.
#[derive(Debug)]
pub struct UdpTransport {
    socket: UdpSocket,
}

impl UdpTransport {
    /// Binds to `local` (use `0.0.0.0:0` to let the OS choose the interface and port).
    pub fn bind(local: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(local)?;
        socket.set_nonblocking(true)?;
        socket.set_multicast_ttl_v4(4)?;
        Ok(Self { socket })
    }
}

impl Transport for UdpTransport {
    fn send_to(&mut self, packet: &[u8], destination: SocketAddr) -> io::Result<()> {
        self.socket.send_to(packet, destination).map(|_| ())
    }
}

/// Packets captured by a [`RecordingTransport`], shared with the test that reads them.
pub type Recorded = Arc<Mutex<Vec<(Vec<u8>, SocketAddr)>>>;

/// Destinations that currently fail, and with which error kind.
pub type Failures = Arc<Mutex<HashMap<SocketAddr, io::ErrorKind>>>;

/// Records every packet instead of sending it. Destinations in `failing` return the chosen error kind.
#[derive(Debug, Default)]
pub struct RecordingTransport {
    recorded: Recorded,
    failing: Failures,
    failed_sends: Arc<AtomicU64>,
}

impl RecordingTransport {
    /// A recorder plus a handle for reading what it captured.
    pub fn new() -> (Self, Recorded) {
        let transport = Self::default();
        let recorded = Arc::clone(&transport.recorded);
        (transport, recorded)
    }

    /// Makes sends to `destination` fail with "host unreachable".
    pub fn fail(self, destination: SocketAddr) -> Self {
        self.failing
            .lock()
            .expect("failing lock")
            .insert(destination, io::ErrorKind::HostUnreachable);
        self
    }

    /// Makes sends to `destination` fail with `WouldBlock` (a full send buffer).
    pub fn would_block(self, destination: SocketAddr) -> Self {
        self.failing
            .lock()
            .expect("failing lock")
            .insert(destination, io::ErrorKind::WouldBlock);
        self
    }

    /// The failing destinations, shared so a test can heal one while output runs.
    pub fn failures(&self) -> Failures {
        Arc::clone(&self.failing)
    }

    /// How many sends have failed so far, shared so a test can wait on it.
    pub fn failed_sends(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.failed_sends)
    }
}

impl Transport for RecordingTransport {
    fn send_to(&mut self, packet: &[u8], destination: SocketAddr) -> io::Result<()> {
        let kind = self
            .failing
            .lock()
            .expect("failing lock")
            .get(&destination)
            .copied();
        if let Some(kind) = kind {
            self.failed_sends.fetch_add(1, Ordering::Relaxed);
            return Err(if kind == io::ErrorKind::HostUnreachable {
                io::Error::new(kind, "host unreachable")
            } else {
                io::Error::from(kind)
            });
        }
        self.recorded
            .lock()
            .expect("recorder lock")
            .push((packet.to_vec(), destination));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorder_captures_and_fails_on_request() {
        let bad: SocketAddr = "10.0.0.9:4048".parse().unwrap();
        let good: SocketAddr = "10.0.0.1:4048".parse().unwrap();
        let (transport, recorded) = RecordingTransport::new();
        let mut transport = transport.fail(bad);
        transport.send_to(&[1, 2], good).unwrap();
        assert!(transport.send_to(&[3], bad).is_err());
        assert_eq!(*recorded.lock().unwrap(), vec![(vec![1, 2], good)]);
    }

    #[test]
    fn udp_transport_delivers_over_loopback() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        transport
            .send_to(b"hello", receiver.local_addr().unwrap())
            .unwrap();
        let mut buf = [0u8; 16];
        let (n, _) = receiver.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"hello");
    }
}
