//! This computer's serial ports, through the `serialport` library: opening a USB DMX interface
//! in raw 8-bit mode, writing a whole message within a short time, and listing the ports with
//! the names their USB devices give.
//!
//! **Settings.** The ENTTEC DMX USB Pro talks to the computer through an FTDI USB FIFO chip, as
//! a virtual COM port, and its protocol (the "DMX USB Pro Widget API") is framed messages on
//! that link; the widget makes the DMX timing (250 kbaud, the break) itself. ENTTEC's API
//! document says the baud rate used to open the port is a dummy value that doesn't set the USB
//! speed. What matters is that every byte goes through as it is: the port is opened raw (no
//! newline, carriage-return, or flow-control byte rewritten or swallowed; a 0x0A channel value
//! stays one byte), 8 data bits, no parity, 2 stop bits, no flow control, at 57600 baud, a
//! standard rate every driver accepts.
//!
//! **Only serial devices.** A show file names the port, and a show can come from someone else,
//! so a name is opened only if it is one of this computer's serial ports: a port the system
//! lists (or a USB serial device in `/dev`, or a link to one such as `/dev/serial/by-id/…`), or
//! `COMn` on Windows. Never an arbitrary file. Nothing is opened until output starts.
//!
//! **Never stuck.** A message is written within [`WRITE_TIMEOUT`], or the write fails (an
//! interface that stopped taking data). An interface unplugged mid-show makes the write fail at
//! once; the output thread reports it, closes the port, and opens it again on its next try.

use crate::usb::{PortChoice, SerialPort, SerialPorts, list_dev_ports, short_name};
use serialport::{ClearBuffer, DataBits, FlowControl, Parity, SerialPortType, StopBits, UsbPortInfo};
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

/// The line rate asked of the port. The Pro doesn't use it (see the module notes).
pub const BAUD_RATE: u32 = 57_600;

/// The longest a message may take to write before the interface counts as not taking data.
/// Writing one goes into the driver's buffer at once; only an interface that has stopped
/// draining it makes a write wait.
pub const WRITE_TIMEOUT: Duration = Duration::from_millis(20);

/// This computer's serial ports.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemSerialPorts;

impl SerialPorts for SystemSerialPorts {
    fn open(&mut self, name: &str) -> io::Result<Box<dyn SerialPort>> {
        if !is_serial_port(name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("\"{name}\" isn't a serial port on this computer"),
            ));
        }
        Ok(Box::new(open_device(name, BAUD_RATE)?))
    }

    fn list(&self) -> io::Result<Vec<PortChoice>> {
        Ok(list_ports())
    }
}

/// An open serial port.
pub(crate) struct SystemPort {
    port: Box<dyn serialport::SerialPort>,
}

impl SerialPort for SystemPort {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        let written = write_within(self.port.as_mut(), bytes, WRITE_TIMEOUT);
        if written.is_err() {
            // Whatever is left waiting would make closing the port wait for it to drain.
            let _ = self.port.clear(ClearBuffer::Output);
        }
        written
    }
}

/// Opens `name` with the Pro's settings (see the module notes). `baud` is [`BAUD_RATE`]; a test
/// on a macOS pseudo-terminal, which has no line rate to set, gives 0.
pub(crate) fn open_device(name: &str, baud: u32) -> io::Result<SystemPort> {
    let port = serialport::new(name, baud)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::Two)
        .flow_control(FlowControl::None)
        .timeout(WRITE_TIMEOUT)
        .open()?;
    Ok(SystemPort { port })
}

/// Writes all of `bytes` within `limit`, or fails with `TimedOut`.
fn write_within(port: &mut dyn serialport::SerialPort, bytes: &[u8], limit: Duration) -> io::Result<()> {
    let deadline = Instant::now() + limit;
    let mut rest = bytes;
    let mut first = true;
    while !rest.is_empty() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(timed_out());
        }
        if !first {
            port.set_timeout(left)?;
        }
        first = false;
        match port.write(rest) {
            // Windows' write gives back what went before its timeout: none at all is a timeout.
            Ok(0) => return Err(timed_out()),
            Ok(n) => rest = &rest[n..],
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    if port.timeout() != limit {
        port.set_timeout(limit)?;
    }
    Ok(())
}

fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "the write timed out")
}

/// Whether `name` is one of this computer's serial ports (see the module notes).
fn is_serial_port(name: &str) -> bool {
    if cfg!(windows) {
        return is_com_port(name);
    }
    let path = Path::new(name);
    if !path.is_absolute() {
        return false;
    }
    let wanted = path.canonicalize().ok();
    let mut known: Vec<String> = system_ports().into_iter().map(|p| p.port_name).collect();
    known.extend(list_dev_ports(Path::new("/dev")).unwrap_or_default());
    known
        .iter()
        .any(|k| k == name || wanted.is_some() && Path::new(k).canonicalize().ok() == wanted)
}

/// `COM3`, or `\\.\COM12`: a Windows serial port's name.
fn is_com_port(name: &str) -> bool {
    let bare = name.strip_prefix(r"\\.\").unwrap_or(name);
    bare.len() > 3
        && bare[..3].eq_ignore_ascii_case("COM")
        && bare[3..].bytes().all(|b| b.is_ascii_digit())
        && !bare[3..].starts_with('0')
}

/// The system's own list of serial ports (empty where it can't be read).
fn system_ports() -> Vec<serialport::SerialPortInfo> {
    // Without libudev, Linux lists ports from /sys, and the library assumes it's there.
    if cfg!(target_os = "linux") && !Path::new("/sys/class/tty").is_dir() {
        return Vec::new();
    }
    serialport::available_ports().unwrap_or_default()
}

/// The ports to offer: USB serial ports, named by their devices, sorted by name. On macOS only
/// the "call-out" device (`/dev/cu.…`), the one to open; Windows offers every COM port.
fn list_ports() -> Vec<PortChoice> {
    let mut choices: Vec<PortChoice> = system_ports()
        .into_iter()
        .filter(|p| !cfg!(target_os = "macos") || p.port_name.starts_with("/dev/cu."))
        .filter_map(|p| match &p.port_type {
            SerialPortType::UsbPort(usb) => Some(PortChoice {
                label: format!("{} ({})", describe(usb), short_name(&p.port_name)),
                name: p.port_name,
            }),
            _ if cfg!(windows) => Some(PortChoice::named(p.port_name)),
            _ => None,
        })
        .collect();
    for name in list_dev_ports(Path::new("/dev")).unwrap_or_default() {
        if !choices.iter().any(|c| c.name == name) {
            choices.push(PortChoice::named(name));
        }
    }
    choices.sort_by(|a, b| a.name.cmp(&b.name));
    choices.dedup_by(|a, b| a.name == b.name);
    choices
}

/// What a USB serial device is, in a person's words, from what it says about itself.
fn describe(usb: &UsbPortInfo) -> String {
    describe_device(
        usb.vid,
        usb.pid,
        usb.manufacturer.as_deref(),
        usb.product.as_deref(),
    )
}

fn describe_device(vid: u16, pid: u16, manufacturer: Option<&str>, product: Option<&str>) -> String {
    let clean = |s: Option<&str>| s.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let (maker, product) = (clean(manufacturer), clean(product));
    // ENTTEC's own interfaces call themselves "DMX USB PRO" (and "… Mk2").
    if let Some(p) = &product
        && p.to_ascii_uppercase().starts_with("DMX USB PRO")
    {
        let rest = p["DMX USB PRO".len()..].trim();
        return if rest.is_empty() {
            "ENTTEC DMX USB Pro".to_string()
        } else {
            format!("ENTTEC DMX USB Pro {rest}")
        };
    }
    match (maker, product) {
        (Some(m), Some(p)) if p.to_lowercase().contains(&m.to_lowercase()) => p,
        (Some(m), Some(p)) => format!("{m} {p}"),
        (None, Some(p)) => p,
        (Some(m), None) => format!("{m} USB serial"),
        (None, None) => format!("USB serial {vid:04x}:{pid:04x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_are_named_as_they_name_themselves() {
        assert_eq!(
            describe_device(0x0403, 0x6001, Some("ENTTEC"), Some("DMX USB PRO")),
            "ENTTEC DMX USB Pro"
        );
        assert_eq!(
            describe_device(0x0403, 0x6001, Some("ENTTEC"), Some("DMX USB PRO Mk2")),
            "ENTTEC DMX USB Pro Mk2"
        );
        assert_eq!(
            describe_device(0x0403, 0x6001, Some("DMXking.com"), Some("ultraDMX Micro")),
            "DMXking.com ultraDMX Micro"
        );
        assert_eq!(
            describe_device(0x0403, 0x6001, Some("FTDI"), Some("FTDI FT232R USB UART")),
            "FTDI FT232R USB UART"
        );
        assert_eq!(describe_device(0x0403, 0x6001, None, Some(" FT232R ")), "FT232R");
        assert_eq!(
            describe_device(0x0403, 0x6001, Some("FTDI"), None),
            "FTDI USB serial"
        );
        assert_eq!(
            describe_device(0x16c0, 0x05dc, None, Some("")),
            "USB serial 16c0:05dc"
        );
    }

    #[test]
    fn windows_ports_are_com_and_a_number() {
        for good in ["COM3", "com12", r"\\.\COM10"] {
            assert!(is_com_port(good), "{good}");
        }
        for bad in [
            "COM",
            "COM0",
            "COM3x",
            "LPT1",
            r"C:\Windows\win.ini",
            "/dev/ttyUSB0",
            "NUL",
        ] {
            assert!(!is_com_port(bad), "{bad}");
        }
    }

    #[test]
    fn a_name_that_isnt_a_serial_port_is_never_opened() {
        // A file a show could name: refused before anything is opened.
        let file = std::env::temp_dir().join(format!("pf-not-a-port-{}", std::process::id()));
        std::fs::write(&file, b"").unwrap();
        for name in [file.to_string_lossy().as_ref(), "relative/ttyUSB0", "", "COM0"] {
            let error = SystemSerialPorts.open(name).err().expect("refused");
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{name}");
            assert!(error.to_string().contains("isn't a serial port"), "{error}");
        }
        assert_eq!(std::fs::read(&file).unwrap(), b"", "nothing was written to it");
        std::fs::remove_file(&file).unwrap();
    }

    #[test]
    fn listing_ports_opens_nothing_and_offers_names_that_would_open() {
        // Listing only reads the system's list; whatever is offered passes the open check.
        for choice in SystemSerialPorts.list().unwrap() {
            assert!(!choice.label.is_empty());
            assert!(is_serial_port(&choice.name), "{}", choice.name);
        }
    }

    /// A pseudo-terminal pair stands in for an interface: the real port code opens its one end
    /// by name, and the test reads the other.
    #[cfg(unix)]
    mod pseudo_terminal {
        use super::*;
        use crate::usb::{PRO_MESSAGE_LEN, write_pro_send_dmx};
        use serialport::SerialPort as _;
        use serialport::TTYPort;
        use std::io::Read;

        /// macOS can't set a line rate on a pseudo-terminal (it isn't a serial line); Linux can.
        const BAUD: u32 = if cfg!(target_os = "macos") { 0 } else { BAUD_RATE };

        /// A frame whose channels hold every byte a terminal would rewrite or act on if the port
        /// weren't raw: newline, carriage return, XON/XOFF, interrupt, end of file, delete.
        fn tricky_frame() -> [u8; PRO_MESSAGE_LEN] {
            let mut channels: Vec<u8> = vec![0x0A, 0x0D, 0x11, 0x13, 0x03, 0x04, 0x1A, 0x7F, 0xFF, 0x00];
            channels.extend((0..=255u8).cycle().take(512 - channels.len()));
            let mut frame = [0u8; PRO_MESSAGE_LEN];
            write_pro_send_dmx(&channels, &mut frame);
            frame
        }

        fn read_exactly(other: &mut TTYPort, len: usize) -> Vec<u8> {
            other.set_timeout(Duration::from_millis(100)).unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut got = Vec::new();
            let mut buf = [0u8; 1024];
            while got.len() < len {
                assert!(
                    Instant::now() < deadline,
                    "only {} of {len} bytes arrived",
                    got.len()
                );
                match other.read(&mut buf) {
                    Ok(n) => got.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == io::ErrorKind::TimedOut => {}
                    Err(e) => panic!("{e}"),
                }
            }
            got
        }

        /// The port's end of a fresh pair, opened by name, and the other end.
        fn opened() -> (SystemPort, TTYPort, String) {
            let (other, end) = TTYPort::pair().expect("a pseudo-terminal pair");
            let name = end.name().expect("its name");
            drop(end);
            let port = open_device(&name, BAUD).expect("opens");
            (port, other, name)
        }

        #[test]
        fn a_frame_goes_through_byte_for_byte_with_the_pros_settings() {
            let (mut port, mut other, _) = opened();
            assert_eq!(port.port.data_bits().unwrap(), DataBits::Eight);
            assert_eq!(port.port.parity().unwrap(), Parity::None);
            assert_eq!(port.port.stop_bits().unwrap(), StopBits::Two);
            assert_eq!(port.port.flow_control().unwrap(), FlowControl::None);
            let frame = tricky_frame();
            // (A frame at a time: macOS holds a pseudo-terminal's next write until the other end
            // has read, as a real interface's driver does until the device has taken the last.)
            for which in ["first", "second"] {
                port.write_all(&frame).unwrap();
                let got = read_exactly(&mut other, PRO_MESSAGE_LEN);
                assert_eq!(&got[..], &frame[..], "the {which} frame, byte for byte");
            }
        }

        #[test]
        fn the_port_is_had_by_one_program_at_a_time() {
            let (_port, _other, name) = opened();
            assert!(open_device(&name, BAUD).is_err(), "a second open is refused");
        }

        #[test]
        fn unplugging_fails_the_write_at_once() {
            let (mut port, mut other, _) = opened();
            let frame = tricky_frame();
            port.write_all(&frame).unwrap();
            read_exactly(&mut other, PRO_MESSAGE_LEN);
            // The other end goes away, as a USB interface does when it's pulled out.
            drop(other);
            let started = Instant::now();
            let error = (0..20)
                .find_map(|_| port.write_all(&frame).err())
                .expect("writing fails once the other end is gone");
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "{:?}",
                started.elapsed()
            );
            assert_ne!(error.kind(), io::ErrorKind::TimedOut, "{error}");
        }

        /// Linux lets a pseudo-terminal take tens of kilobytes before it stops, and then blocks
        /// in the middle of a write; a real USB serial driver there only says it's ready once a
        /// whole message fits. macOS's terminal layer behaves the same for both, so this runs
        /// there.
        #[cfg(target_os = "macos")]
        #[test]
        fn an_interface_that_stops_taking_data_times_out_instead_of_hanging() {
            let (mut port, _other, _) = opened();
            let frame = tricky_frame();
            let started = Instant::now();
            let mut slowest = Duration::ZERO;
            let error = loop {
                assert!(started.elapsed() < Duration::from_secs(5), "never timed out");
                let one = Instant::now();
                let result = port.write_all(&frame);
                slowest = slowest.max(one.elapsed());
                if let Err(e) = result {
                    break e;
                }
            };
            assert_eq!(error.kind(), io::ErrorKind::TimedOut, "{error}");
            assert!(slowest < Duration::from_millis(500), "a write took {slowest:?}");
        }
    }
}
