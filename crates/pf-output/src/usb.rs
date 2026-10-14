//! USB DMX interfaces that speak the ENTTEC DMX USB Pro protocol (the "Pro" serial protocol
//! many interfaces copy): each frame is one framed "send DMX" message written to the
//! interface's serial port.
//!
//! The message: start byte 0x7E, label 6 ("Output Only Send DMX Packet Request"), the data length
//! (513: the start code and 512 channels) low byte first, the DMX start code 0, the 512
//! channels, and the end byte 0xE7.
//!
//! Opening a serial port takes setting it to raw 8-bit mode (or the system turns a newline into
//! two bytes and the channels shift), which needs the operating system's serial settings
//! (termios, or Windows' DCB). That takes a serial-port library this build doesn't have yet, so
//! [`NoSerialPorts`] lists the ports it can see but can't open them; everything else here works
//! against any [`SerialPorts`].

use std::io;

/// Bytes in one "send DMX" message: 4 header bytes, the start code, 512 channels, the end byte.
pub const PRO_MESSAGE_LEN: usize = 4 + 1 + 512 + 1;
const START: u8 = 0x7E;
const END: u8 = 0xE7;
/// "Output Only Send DMX Packet Request".
const LABEL_SEND_DMX: u8 = 6;
const DMX_START_CODE: u8 = 0;

/// Why USB DMX can't be sent from this build, in plain words.
pub const USB_UNAVAILABLE: &str =
    "this version of PixelFlow can't open USB DMX interfaces yet (it needs a serial-port library)";

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

/// Opens serial ports by name, and lists the ones there are.
pub trait SerialPorts: Send {
    fn open(&mut self, name: &str) -> io::Result<Box<dyn SerialPort>>;
    /// The serial ports a USB DMX interface could be on, by name.
    fn list(&self) -> io::Result<Vec<String>>;
}

/// The serial ports of a build without a serial-port library: listed (on macOS and Linux, from
/// `/dev`), never opened.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoSerialPorts;

impl SerialPorts for NoSerialPorts {
    fn open(&mut self, _name: &str) -> io::Result<Box<dyn SerialPort>> {
        Err(io::Error::new(io::ErrorKind::Unsupported, USB_UNAVAILABLE))
    }

    fn list(&self) -> io::Result<Vec<String>> {
        list_dev_ports(std::path::Path::new("/dev"))
    }
}

/// The USB serial devices in `dev` (macOS names them `cu.usbserial-…` and `cu.usbmodem…`, Linux
/// `ttyUSB…` and `ttyACM…`), sorted. Windows numbers its ports (`COM3`) in the registry, which
/// needs the serial-port library to read: there, the list is empty and a port is typed in.
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
pub type Written = std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<u8>)>>>;

/// A pretend serial port for tests: records what is written, or fails.
#[derive(Debug, Default, Clone)]
pub struct FakeSerialPorts {
    /// What was written to each port, in order.
    pub written: Written,
    /// Ports that fail to open.
    pub missing: Vec<String>,
    /// Ports there are.
    pub ports: Vec<String>,
}

struct FakePort {
    name: String,
    written: Written,
}

impl SerialPort for FakePort {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.written
            .lock()
            .expect("fake serial lock")
            .push((self.name.clone(), bytes.to_vec()));
        Ok(())
    }
}

impl SerialPorts for FakeSerialPorts {
    fn open(&mut self, name: &str) -> io::Result<Box<dyn SerialPort>> {
        if self.missing.iter().any(|m| m == name) {
            return Err(io::Error::new(io::ErrorKind::NotFound, format!("no port {name}")));
        }
        Ok(Box::new(FakePort {
            name: name.to_string(),
            written: std::sync::Arc::clone(&self.written),
        }))
    }

    fn list(&self) -> io::Result<Vec<String>> {
        Ok(self.ports.clone())
    }
}

/// A USB DMX interface's message, filled each frame.
#[derive(Debug)]
pub struct UsbDmxPacket {
    pub port: String,
    message: Box<[u8; PRO_MESSAGE_LEN]>,
}

impl UsbDmxPacket {
    pub fn new(port: impl Into<String>) -> Self {
        let mut message = Box::new([0u8; PRO_MESSAGE_LEN]);
        write_pro_send_dmx(&[], &mut message);
        Self {
            port: port.into(),
            message,
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
    fn without_a_serial_library_ports_are_listed_but_not_opened() {
        let mut ports = NoSerialPorts;
        let error = ports.open("/dev/cu.usbserial-1").err().expect("refused");
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
        assert!(error.to_string().contains("serial-port library"));
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
