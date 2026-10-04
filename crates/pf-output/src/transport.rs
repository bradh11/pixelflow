//! Where packets go: real UDP sockets, or an in-memory recorder for tests.

use std::collections::HashSet;
use std::io;
use std::net::{SocketAddr, UdpSocket};
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

/// Records every packet instead of sending it. Destinations in `failing` return an error.
#[derive(Debug, Default)]
pub struct RecordingTransport {
    recorded: Recorded,
    failing: Arc<Mutex<HashSet<SocketAddr>>>,
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
        self.failing.lock().expect("failing lock").insert(destination);
        self
    }

    /// The set of failing destinations, shared so a test can heal one while output runs.
    pub fn failures(&self) -> Arc<Mutex<HashSet<SocketAddr>>> {
        Arc::clone(&self.failing)
    }
}

impl Transport for RecordingTransport {
    fn send_to(&mut self, packet: &[u8], destination: SocketAddr) -> io::Result<()> {
        if self.failing.lock().expect("failing lock").contains(&destination) {
            return Err(io::Error::new(io::ErrorKind::HostUnreachable, "host unreachable"));
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
