//! Where packets go: real UDP sockets (and USB DMX interfaces' serial ports), or an in-memory
//! recorder for tests.

use crate::serial::SystemSerialPorts;
use crate::usb::{SerialPort, SerialPorts};
use std::collections::HashMap;
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Sends one packet to one destination.
pub trait Transport: Send {
    fn send_to(&mut self, packet: &[u8], destination: SocketAddr) -> io::Result<()>;

    /// Writes one message to the serial port `port` (a USB DMX interface).
    fn send_serial(&mut self, _port: &str, _message: &[u8]) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this output can't reach USB DMX interfaces",
        ))
    }
}

/// Why a USB DMX interface's port didn't open, in plain words.
fn open_failed(port: &str, e: &io::Error) -> io::Error {
    let message = match e.kind() {
        // The name isn't a serial port at all: that says so already.
        io::ErrorKind::InvalidInput => e.to_string(),
        io::ErrorKind::PermissionDenied => format!(
            "not allowed to open the USB DMX interface on {port} (on Linux, add yourself to the \
             dialout or uucp group and sign in again)"
        ),
        // Missing, or held by another program (the systems don't tell the two apart).
        io::ErrorKind::NotFound => format!(
            "no USB DMX interface on {port}: is it plugged in, and not in use by another program? ({e})"
        ),
        _ => format!("couldn't open the USB DMX interface on {port} ({e})"),
    };
    io::Error::new(e.kind(), message)
}

/// Why a write to an open USB DMX interface failed, in plain words.
fn write_failed(port: &str, e: &io::Error) -> io::Error {
    let message = if e.kind() == io::ErrorKind::TimedOut {
        format!("the USB DMX interface on {port} stopped taking data; trying it again")
    } else {
        format!("the USB DMX interface on {port} stopped answering: was it unplugged? ({e})")
    };
    io::Error::new(e.kind(), message)
}

/// A non-blocking UDP socket, so a slow or unreachable controller can never stall output, and
/// the serial ports of any USB DMX interfaces (this computer's, unless [`Self::with_serial`]
/// says otherwise), opened the first time each is sent to.
pub struct UdpTransport {
    socket: UdpSocket,
    serial: Box<dyn SerialPorts>,
    open: HashMap<String, Box<dyn SerialPort>>,
}

impl std::fmt::Debug for UdpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UdpTransport")
            .field("socket", &self.socket)
            .field("open_ports", &self.open.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl UdpTransport {
    /// Binds to `local` (use `0.0.0.0:0` to let the OS choose the interface and port).
    pub fn bind(local: SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(local)?;
        socket.set_nonblocking(true)?;
        socket.set_multicast_ttl_v4(4)?;
        Ok(Self {
            socket,
            serial: Box::new(SystemSerialPorts),
            open: HashMap::new(),
        })
    }

    /// Opens USB DMX interfaces' serial ports through `serial`.
    pub fn with_serial(mut self, serial: Box<dyn SerialPorts>) -> Self {
        self.serial = serial;
        self
    }
}

impl Transport for UdpTransport {
    fn send_to(&mut self, packet: &[u8], destination: SocketAddr) -> io::Result<()> {
        self.socket.send_to(packet, destination).map(|_| ())
    }

    /// A port that fails is closed, so the next send opens it again (it may have been
    /// unplugged and plugged back in).
    fn send_serial(&mut self, port: &str, message: &[u8]) -> io::Result<()> {
        if !self.open.contains_key(port) {
            let opened = self.serial.open(port).map_err(|e| open_failed(port, &e))?;
            self.open.insert(port.to_string(), opened);
        }
        let result = self
            .open
            .get_mut(port)
            .map_or(Ok(()), |open| open.write_all(message));
        if let Err(e) = result {
            self.open.remove(port);
            return Err(write_failed(port, &e));
        }
        Ok(())
    }
}

/// Packets captured by a [`RecordingTransport`], shared with the test that reads them.
pub type Recorded = Arc<Mutex<Vec<(Vec<u8>, SocketAddr)>>>;

/// Destinations that currently fail, and with which error kind.
pub type Failures = Arc<Mutex<HashMap<SocketAddr, io::ErrorKind>>>;

/// Records every packet instead of sending it. Destinations in `failing` return the chosen error kind.
/// Clones share the recording.
#[derive(Debug, Default, Clone)]
pub struct RecordingTransport {
    recorded: Recorded,
    failing: Failures,
    failed_sends: Arc<AtomicU64>,
    serial: RecordedSerial,
}

/// Messages a [`RecordingTransport`] was given for serial ports: the port, then the message.
pub type RecordedSerial = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

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

    /// What was written to serial ports (USB DMX interfaces).
    pub fn serial(&self) -> RecordedSerial {
        Arc::clone(&self.serial)
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

    fn send_serial(&mut self, port: &str, message: &[u8]) -> io::Result<()> {
        self.serial
            .lock()
            .expect("recorder lock")
            .push((port.to_string(), message.to_vec()));
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
    fn serial_messages_go_to_the_port_named_and_a_failed_port_opens_again() {
        let ports = crate::usb::FakeSerialPorts {
            missing: vec!["COM9".into()],
            ..Default::default()
        };
        let written = Arc::clone(&ports.written);
        let mut transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap())
            .unwrap()
            .with_serial(Box::new(ports));
        transport.send_serial("COM3", &[1, 2]).unwrap();
        transport.send_serial("COM3", &[3]).unwrap();
        let error = transport.send_serial("COM9", &[4]).unwrap_err();
        assert_eq!(
            error.to_string(),
            "no USB DMX interface on COM9: is it plugged in, and not in use by another program? \
             (No such file or directory)"
        );
        assert_eq!(
            *written.lock().unwrap(),
            vec![("COM3".to_string(), vec![1, 2]), ("COM3".to_string(), vec![3])]
        );
    }

    #[test]
    fn an_unplugged_interface_is_reported_plainly_and_opened_again_once_back() {
        let ports = crate::usb::FakeSerialPorts::default();
        let unplugged = Arc::clone(&ports.unplugged);
        let written = Arc::clone(&ports.written);
        let mut transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap())
            .unwrap()
            .with_serial(Box::new(ports));
        transport.send_serial("COM3", &[1]).unwrap();
        unplugged.lock().unwrap().push("COM3".into());
        let error = transport.send_serial("COM3", &[2]).unwrap_err();
        assert_eq!(
            error.to_string(),
            "the USB DMX interface on COM3 stopped answering: was it unplugged? (Device not configured)"
        );
        assert!(transport.send_serial("COM3", &[3]).is_err(), "still gone");
        unplugged.lock().unwrap().clear();
        transport.send_serial("COM3", &[4]).unwrap();
        let bytes: Vec<u8> = written
            .lock()
            .unwrap()
            .iter()
            .flat_map(|(_, b)| b.clone())
            .collect();
        assert_eq!(bytes, vec![1, 4]);
    }

    #[test]
    fn errors_name_the_port_and_what_to_do() {
        let denied = open_failed("/dev/ttyUSB0", &io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(denied.to_string().contains("dialout"), "{denied}");
        let slow = write_failed("COM3", &io::Error::from(io::ErrorKind::TimedOut));
        assert_eq!(slow.kind(), io::ErrorKind::TimedOut);
        assert!(slow.to_string().contains("stopped taking data"), "{slow}");
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
