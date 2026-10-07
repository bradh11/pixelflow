//! FPP MultiSync "ping" packets on UDP 32320, used for discovery.
//!
//! Layout per FPP's `docs/ControlProtocol.txt` and `src/MultiSync.cpp`: a 7-byte header
//! (`FPPD`, packet type, little-endian extra-data length) followed by the ping payload.

use crate::device::DeviceKind;
use std::net::Ipv4Addr;

/// MultiSync control port.
pub const MULTISYNC_PORT: u16 = 32320;
/// MultiSync multicast group ("239.F.P.P").
pub const MULTISYNC_GROUP: Ipv4Addr = Ipv4Addr::new(239, 70, 80, 80);

const PACKET_TYPE_PING: u8 = 0x04;
/// App type byte for "other" (non-FPP) senders, as xLights uses.
const TYPE_OTHER: u8 = 0xC0;

/// A v2 discover request (207 bytes): every FPP-compatible device answers with a ping. The
/// sender's IP is 0.0.0.0 so peers don't record PixelFlow as an FPP instance.
pub fn discover_packet() -> [u8; 207] {
    let mut packet = [0u8; 207];
    packet[0..4].copy_from_slice(b"FPPD");
    packet[4] = PACKET_TYPE_PING;
    packet[5..7].copy_from_slice(&200u16.to_le_bytes());
    packet[7] = 2; // ping version
    packet[8] = 1; // subtype: discover
    packet[9] = TYPE_OTHER;
    packet[84..93].copy_from_slice(b"PixelFlow");
    packet
}

/// A ping from a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ping {
    pub address: Ipv4Addr,
    pub type_id: u8,
    /// Operating-mode flags: 0x01 bridge, 0x02 player, 0x08 remote.
    pub mode: u8,
    pub hostname: String,
    pub version: String,
    pub hardware: String,
    /// True for a discover request (another discoverer), false for a device's answer.
    pub discover: bool,
}

/// Parses a MultiSync ping, or `None` if the datagram isn't one.
pub fn parse_ping(data: &[u8]) -> Option<Ping> {
    if data.len() < 7 || &data[0..4] != b"FPPD" || data[4] != PACKET_TYPE_PING {
        return None;
    }
    let extra = u16::from_le_bytes([data[5], data[6]]) as usize;
    // Version 2 is the oldest layout with the fields below (version 1 was shorter).
    if data.len() < 7 + extra || extra < 118 || data[7] < 2 {
        return None;
    }
    let text = |start: usize, end: usize| -> String {
        let field = data.get(start..end.min(7 + extra)).unwrap_or(&[]);
        let field = field.split(|&b| b == 0).next().unwrap_or(&[]);
        String::from_utf8_lossy(field).trim().to_string()
    };
    Some(Ping {
        address: Ipv4Addr::new(data[15], data[16], data[17], data[18]),
        type_id: data[9],
        mode: data[14],
        hostname: text(19, 84),
        version: text(84, 125),
        hardware: text(125, 166),
        discover: data[8] == 1,
    })
}

/// The device kind for a ping's app type byte.
pub fn kind_for_type(type_id: u8) -> Option<DeviceKind> {
    match type_id {
        0x01..=0x7F => Some(DeviceKind::Fpp),
        0x80..=0x9F => Some(DeviceKind::Falcon),
        0xFB => Some(DeviceKind::Wled),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A v4 ping as FPP sends it (7 + 359 bytes).
    fn fpp_ping() -> Vec<u8> {
        let mut p = vec![0u8; 7 + 359];
        p[0..4].copy_from_slice(b"FPPD");
        p[4] = 4;
        p[5..7].copy_from_slice(&359u16.to_le_bytes());
        p[7] = 4;
        p[8] = 0;
        p[9] = 0x0D; // a Raspberry Pi
        p[10..12].copy_from_slice(&9u16.to_be_bytes());
        p[12..14].copy_from_slice(&3u16.to_be_bytes());
        p[14] = 0x02;
        p[15..19].copy_from_slice(&[192, 0, 2, 10]);
        p[19..22].copy_from_slice(b"FPP");
        p[84..87].copy_from_slice(b"9.3");
        p[125..152].copy_from_slice(b"Raspberry Pi 3 Model B Plus");
        p
    }

    #[test]
    fn discover_packet_layout() {
        let p = discover_packet();
        assert_eq!(&p[0..4], b"FPPD");
        assert_eq!((p[4], p[5], p[6], p[7], p[8], p[9]), (4, 200, 0, 2, 1, 0xC0));
        assert_eq!(&p[15..19], &[0, 0, 0, 0]);
        let parsed = parse_ping(&p).unwrap();
        assert!(parsed.discover);
        assert_eq!(parsed.version, "PixelFlow");
    }

    #[test]
    fn parses_an_fpp_ping() {
        let ping = parse_ping(&fpp_ping()).unwrap();
        assert_eq!(ping.address, Ipv4Addr::new(192, 0, 2, 10));
        assert_eq!(ping.hostname, "FPP");
        assert_eq!(ping.version, "9.3");
        assert_eq!(ping.hardware, "Raspberry Pi 3 Model B Plus");
        assert_eq!(ping.mode, 0x02);
        assert!(!ping.discover);
        assert_eq!(kind_for_type(ping.type_id), Some(DeviceKind::Fpp));
    }

    #[test]
    fn rejects_other_packets_and_truncated_pings() {
        assert!(parse_ping(b"hello").is_none());
        let mut sync = fpp_ping();
        sync[4] = 1;
        assert!(parse_ping(&sync).is_none());
        assert!(parse_ping(&fpp_ping()[..100]).is_none());
    }

    /// A v2 ping with exactly `extra` bytes after the header.
    fn ping_with_extra(extra: u16) -> Vec<u8> {
        let mut p = vec![0u8; 7 + usize::from(extra)];
        p[0..4].copy_from_slice(b"FPPD");
        p[4] = 4;
        p[5..7].copy_from_slice(&extra.to_le_bytes());
        p[7] = 2;
        p[9] = 0x0D;
        p[19..22].copy_from_slice(b"FPP");
        p
    }

    #[test]
    fn the_shortest_ping_is_118_bytes_after_the_header() {
        assert!(parse_ping(&ping_with_extra(117)).is_none());
        let ping = parse_ping(&ping_with_extra(118)).unwrap();
        assert_eq!(ping.hostname, "FPP");
        assert_eq!(
            ping.hardware, "",
            "the hardware field lies past the end of a v2 ping"
        );
        // The length field claims more than arrived.
        let mut short = ping_with_extra(118);
        short[5..7].copy_from_slice(&119u16.to_le_bytes());
        assert!(parse_ping(&short).is_none());
    }

    #[test]
    fn pings_before_version_2_are_rejected() {
        for version in [0, 1] {
            let mut old = fpp_ping();
            old[7] = version;
            assert!(parse_ping(&old).is_none(), "version {version}");
        }
        let mut v2 = fpp_ping();
        v2[7] = 2;
        assert!(parse_ping(&v2).is_some());
    }

    #[test]
    fn maps_type_bytes_to_kinds() {
        assert_eq!(kind_for_type(0x8A), Some(DeviceKind::Falcon));
        assert_eq!(kind_for_type(0xFB), Some(DeviceKind::Wled));
        assert_eq!(kind_for_type(0xC0), None);
        assert_eq!(kind_for_type(0x00), None);
    }
}
