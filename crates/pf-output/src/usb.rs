//! USB DMX interfaces that speak the ENTTEC DMX USB Pro protocol (the "Pro" serial protocol
//! many interfaces copy): each frame is one framed "send DMX" message written to the
//! interface's serial port.
//!
//! The message: start byte 0x7E, label 6 ("Output Only Send DMX Packet Request"), the data length
//! (513: the start code and 512 channels) low byte first, the DMX start code 0, the 512
//! channels, and the end byte 0xE7.
//!
//! The interface itself is opened by a [`SerialPorts`]: [`crate::SystemSerialPorts`] for this
//! computer's real ports (raw 8-bit mode, so a newline byte isn't turned into two and the
//! channels don't shift), or [`FakeSerialPorts`] in tests.
//!
//! The Pro sends DMX on its own, over and over, from the last message it was given, so one
//! message per show frame is all it needs: [`UsbDmxPacket::due`] keeps that to no more often than
//! a whole universe can go out on the DMX line ([`MIN_GAP`]).

use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Bytes in one "send DMX" message: 4 header bytes, the start code, 512 channels, the end byte.
pub const PRO_MESSAGE_LEN: usize = 4 + 1 + 512 + 1;
const START: u8 = 0x7E;
const END: u8 = 0xE7;
/// "Output Only Send DMX Packet Request".
const LABEL_SEND_DMX: u8 = 6;
const DMX_START_CODE: u8 = 0;

/// The shortest time between two messages to one interface: how long a whole universe takes on
/// the DMX line (a break, the mark after it, and 513 slots of 44 µs), about 44 a second. Messages
/// sent faster than that would only replace each other in the interface before it sends them.
pub const MIN_GAP: Duration = Duration::from_micros(22_700);

/// A "send DMX" message for `channels` (the first 512; any missing are zero), into `out`.
pub fn write_pro_send_dmx(channels: &[u8], out: &mut [u8; PRO_MESSAGE_LEN]) {
    let len = (1 + 512u16).to_le_bytes();
    out[0] = START;
    out[1] = LABEL_SEND_DMX;
    out[2] = len[0];
    out[3] = len[1];
    out[4] = DMX_START_CODE;
    let data = &mut out[5..5 + 512];
    let n = channels.len().min(512);
    data[..n].copy_from_slice(&channels[..n]);
    data[n..].fill(0);
    out[PRO_MESSAGE_LEN - 1] = END;
}

/// One open serial port.
pub trait SerialPort: Send {
    /// Writes all of `bytes`, or fails.
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()>;
}

/// A serial port a USB DMX interface could be on, as the port picker offers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortChoice {
    /// What opens it, and what a controller's address holds: `/dev/cu.usbserial-EN1234`, `COM3`.
    pub name: String,
    /// What a person reads: "ENTTEC DMX USB Pro (cu.usbserial-EN1234)".
    pub label: String,
}

impl PortChoice {
    /// A port known only by its name.
    pub fn named(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            label: short_name(&name).to_string(),
            name,
        }
    }
}

/// A port's name without its directory: `cu.usbserial-EN1234` for `/dev/cu.usbserial-EN1234`.
pub fn short_name(name: &str) -> &str {
    name.rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or(name)
}

/// Opens serial ports by name, and lists the ones there are.
pub trait SerialPorts: Send {
    fn open(&mut self, name: &str) -> io::Result<Box<dyn SerialPort>>;
    /// The serial ports a USB DMX interface could be on.
    fn list(&self) -> io::Result<Vec<PortChoice>>;
}

/// The USB serial devices in `dev` (macOS names them `cu.usbserial-…` and `cu.usbmodem…`, Linux
/// `ttyUSB…` and `ttyACM…`), sorted: the ports offered when the system can't describe them.
/// Windows has no `/dev`: there, the list is empty.
pub fn list_dev_ports(dev: &std::path::Path) -> io::Result<Vec<String>> {
    if !dev.is_dir() {
        return Ok(Vec::new());
    }
    let mut found: Vec<String> = std::fs::read_dir(dev)?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| {
            ["cu.usbserial", "cu.usbmodem", "ttyUSB", "ttyACM"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
        })
        .map(|name| dev.join(name).to_string_lossy().into_owned())
        .collect();
    found.sort();
    Ok(found)
}

/// What was written to each serial port, in order: the port, then the bytes.
pub type Written = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// A pretend serial port for tests: records what is written, or fails.
#[derive(Debug, Default, Clone)]
pub struct FakeSerialPorts {
    /// What was written to each port, in order.
    pub written: Written,
    /// Ports that fail to open.
    pub missing: Vec<String>,
    /// Ports there are.
    pub ports: Vec<PortChoice>,
    /// Ports unplugged right now: they don't open, and a port already open fails to write, as a
    /// real interface pulled out mid-show does. Shared, so a test can unplug one (and plug it back
    /// in) while output runs.
    pub unplugged: Arc<Mutex<Vec<String>>>,
}

fn is_unplugged(unplugged: &Mutex<Vec<String>>, name: &str) -> bool {
    unplugged
        .lock()
        .expect("fake serial lock")
        .iter()
        .any(|u| u == name)
}

struct FakePort {
    name: String,
    written: Written,
    unplugged: Arc<Mutex<Vec<String>>>,
}

impl SerialPort for FakePort {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        if is_unplugged(&self.unplugged, &self.name) {
            // What macOS says when a USB serial device goes away under an open port.
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "Device not configured"));
        }
        self.written
            .lock()
            .expect("fake serial lock")
            .push((self.name.clone(), bytes.to_vec()));
        Ok(())
    }
}

impl SerialPorts for FakeSerialPorts {
    fn open(&mut self, name: &str) -> io::Result<Box<dyn SerialPort>> {
        if self.missing.iter().any(|m| m == name) || is_unplugged(&self.unplugged, name) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "No such file or directory",
            ));
        }
        Ok(Box::new(FakePort {
            name: name.to_string(),
            written: Arc::clone(&self.written),
            unplugged: Arc::clone(&self.unplugged),
        }))
    }

    fn list(&self) -> io::Result<Vec<PortChoice>> {
        Ok(self.ports.clone())
    }
}

/// A USB DMX interface's message, filled each frame.
#[derive(Debug)]
pub struct UsbDmxPacket {
    pub port: String,
    message: Box<[u8; PRO_MESSAGE_LEN]>,
    /// When the next message may go (see [`UsbDmxPacket::due`]).
    next: Option<Instant>,
}

impl UsbDmxPacket {
    pub fn new(port: impl Into<String>) -> Self {
        let mut message = Box::new([0u8; PRO_MESSAGE_LEN]);
        write_pro_send_dmx(&[], &mut message);
        Self {
            port: port.into(),
            message,
            next: None,
        }
    }

    /// Whether a frame drawn at `now` should go to the interface, keeping messages [`MIN_GAP`]
    /// apart on average: every frame of a show at 40 frames a second or slower goes, and a
    /// faster show sends about 44 a second (the newest frame each time). The next one is counted
    /// from when this one was due rather than when it went, so a frame clock running a little
    /// late doesn't halve the rate.
    pub fn due(&mut self, now: Instant) -> bool {
        match self.next {
            Some(at) if now < at => false,
            Some(at) if now.duration_since(at) <= MIN_GAP => {
                self.next = Some(at + MIN_GAP);
                true
            }
            _ => {
                self.next = Some(now + MIN_GAP);
                true
            }
        }
    }

    /// Copies the interface's channels (its one universe) into the message.
    pub fn update(&mut self, channels: &[u8]) {
        write_pro_send_dmx(channels, &mut self.message);
    }

    pub fn message(&self) -> &[u8] {
        &self.message[..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_send_dmx_message_is_framed_as_the_pro_protocol_says() {
        let mut out = [9u8; PRO_MESSAGE_LEN];
        write_pro_send_dmx(&[1, 2, 3], &mut out);
        assert_eq!(
            &out[..5],
            &[0x7E, 6, 0x01, 0x02, 0],
            "start, label 6, 513 low byte first, start code"
        );
        assert_eq!(&out[5..8], &[1, 2, 3]);
        assert!(
            out[8..PRO_MESSAGE_LEN - 1].iter().all(|&b| b == 0),
            "the rest of the universe is zero"
        );
        assert_eq!(out[PRO_MESSAGE_LEN - 1], 0xE7);
        assert_eq!(PRO_MESSAGE_LEN, 518);
    }

    #[test]
    fn more_than_a_universe_is_cut_to_512_channels() {
        let mut out = [0u8; PRO_MESSAGE_LEN];
        write_pro_send_dmx(&[7; 600], &mut out);
        assert!(out[5..517].iter().all(|&b| b == 7));
        assert_eq!(out[517], 0xE7);
    }

    #[test]
    fn messages_are_paced_to_what_the_dmx_line_can_carry() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        // 40 frames a second: every frame goes, even one a little late or early.
        let mut packet = UsbDmxPacket::new("COM3");
        let sent = [0, 25, 51, 74, 100, 125, 150]
            .iter()
            .filter(|&&t| packet.due(ms(t)))
            .count();
        assert_eq!(sent, 7);
        // 100 frames a second for a second: about 44 go, never two closer than a frame apart.
        let mut packet = UsbDmxPacket::new("COM3");
        let times: Vec<u64> = (0..100).map(|i| i * 10).filter(|&t| packet.due(ms(t))).collect();
        assert!((40..=46).contains(&times.len()), "{}", times.len());
        assert!(times.windows(2).all(|w| w[1] - w[0] >= 20));
        // After a pause (a stall, a backoff) it starts again at once, without a burst.
        assert!(packet.due(ms(5000)));
        assert!(!packet.due(ms(5010)));
    }

    #[test]
    fn a_fake_port_fails_once_unplugged_and_opens_again_when_plugged_back_in() {
        let ports = FakeSerialPorts::default();
        let mut opener = ports.clone();
        let mut port = opener.open("COM3").unwrap();
        port.write_all(&[1]).unwrap();
        ports.unplugged.lock().unwrap().push("COM3".into());
        assert_eq!(
            port.write_all(&[2]).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        assert_eq!(opener.open("COM3").err().unwrap().kind(), io::ErrorKind::NotFound);
        ports.unplugged.lock().unwrap().clear();
        opener.open("COM3").unwrap().write_all(&[3]).unwrap();
        let written: Vec<u8> = ports
            .written
            .lock()
            .unwrap()
            .iter()
            .flat_map(|(_, b)| b.clone())
            .collect();
        assert_eq!(written, vec![1, 3]);
    }

    #[test]
    fn a_port_is_offered_by_its_short_name_when_nothing_better_is_known() {
        assert_eq!(
            PortChoice::named("/dev/cu.usbserial-EN1").label,
            "cu.usbserial-EN1"
        );
        assert_eq!(PortChoice::named("COM3").label, "COM3");
        assert_eq!(short_name("/dev/"), "/dev/");
    }

    #[test]
    fn usb_serial_devices_are_found_by_name() {
        let dev = std::env::temp_dir().join(format!("pf-usb-dmx-{}", std::process::id()));
        std::fs::create_dir_all(&dev).unwrap();
        for name in [
            "cu.usbserial-EN1234",
            "ttyUSB0",
            "cu.Bluetooth",
            "tty.usbserial-EN1234",
            "null",
        ] {
            std::fs::write(dev.join(name), b"").unwrap();
        }
        let found = list_dev_ports(&dev).unwrap();
        let names: Vec<String> = found
            .iter()
            .map(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, vec!["cu.usbserial-EN1234", "ttyUSB0"]);
        assert!(list_dev_ports(&dev.join("none")).unwrap().is_empty());
        std::fs::remove_dir_all(&dev).unwrap();
    }
}
