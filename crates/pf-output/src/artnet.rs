//! Art-Net 4 ArtDMX packets: one universe of DMX channels each, sent unicast to the controller.
//!
//! Every packet carries a whole universe (512 channels; any the controller doesn't use are zero):
//! DMX receivers expect complete frames. PixelFlow doesn't poll for nodes (ArtPoll): a controller
//! is sent to at the address it was given.

use pf_mapping::UniverseSpan;
use std::net::SocketAddr;

/// Art-Net's UDP port.
pub const ART_NET_PORT: u16 = 6454;
/// Bytes before the channels in an ArtDMX packet.
pub const HEADER_LEN: usize = 18;
/// Channels every ArtDMX packet carries.
pub const CHANNELS: usize = 512;

const ID: [u8; 8] = *b"Art-Net\0";
/// OpDmx, sent low byte first.
const OP_DMX: u16 = 0x5000;
/// The protocol version this follows (Art-Net 4 is version 14).
const PROTOCOL_VERSION: u16 = 14;

/// Writes an ArtDMX header for `universe` (the 15-bit port-address: net, sub-net, universe).
pub fn write_header(packet: &mut [u8], universe: u16) {
    packet[0..8].copy_from_slice(&ID);
    packet[8..10].copy_from_slice(&OP_DMX.to_le_bytes());
    packet[10..12].copy_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    packet[12] = 0; // sequence: set as each packet is sent
    packet[13] = 0; // physical input port: informational
    let [high, low] = (universe & 0x7fff).to_be_bytes();
    packet[14] = low; // SubUni: sub-net and universe
    packet[15] = high; // Net
    packet[16..18].copy_from_slice(&(CHANNELS as u16).to_be_bytes());
}

/// Pre-built ArtDMX packets for one controller, one per universe.
#[derive(Debug)]
pub struct ArtNetPackets {
    packets: Vec<[u8; HEADER_LEN + CHANNELS]>,
    universes: Vec<UniverseSpan>,
    destination: SocketAddr,
    sequence: u8,
}

impl ArtNetPackets {
    pub fn new(universes: &[UniverseSpan], destination: SocketAddr) -> Self {
        let packets = universes
            .iter()
            .map(|u| {
                let mut packet = [0u8; HEADER_LEN + CHANNELS];
                write_header(&mut packet, u.universe);
                packet
            })
            .collect();
        Self {
            packets,
            universes: universes.to_vec(),
            destination,
            sequence: 0,
        }
    }

    /// Copies the controller's channels into each packet and advances the sequence number
    /// (1–255: 0 would tell the node not to reorder).
    pub fn update(&mut self, channels: &[u8]) {
        self.sequence = self.sequence % 255 + 1;
        for (packet, u) in self.packets.iter_mut().zip(&self.universes) {
            let start = u.controller_channel;
            let len = usize::from(u.len).min(CHANNELS);
            if let Some(source) = channels.get(start..start + len) {
                packet[HEADER_LEN..HEADER_LEN + len].copy_from_slice(source);
            }
            packet[12] = self.sequence;
        }
    }

    pub fn len(&self) -> usize {
        self.packets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }

    /// Packet `i` and where to send it.
    pub fn packet(&self, i: usize) -> (&[u8], SocketAddr) {
        (&self.packets[i], self.destination)
    }

    pub fn destination(&self) -> SocketAddr {
        self.destination
    }

    /// The universes, in packet order.
    pub fn universes(&self) -> &[UniverseSpan] {
        &self.universes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(universe: u16, controller_channel: usize, len: u16) -> UniverseSpan {
        UniverseSpan {
            universe,
            controller_channel,
            len,
        }
    }

    #[test]
    fn an_art_dmx_packet_is_laid_out_as_art_net_4_says() {
        let to: SocketAddr = "192.0.2.50:6454".parse().unwrap();
        let mut packets = ArtNetPackets::new(&[span(0x0123, 0, 3)], to);
        packets.update(&[10, 20, 30]);
        let (packet, destination) = packets.packet(0);
        assert_eq!(destination, to);
        assert_eq!(packet.len(), 530, "a whole universe");
        assert_eq!(&packet[0..8], b"Art-Net\0");
        assert_eq!(&packet[8..10], &[0x00, 0x50], "OpDmx, low byte first");
        assert_eq!(&packet[10..12], &[0, 14], "protocol version 14");
        assert_eq!(packet[12], 1, "the first sequence number");
        assert_eq!(packet[13], 0);
        assert_eq!((packet[14], packet[15]), (0x23, 0x01), "SubUni, then Net");
        assert_eq!(&packet[16..18], &[0x02, 0x00], "512 channels, high byte first");
        assert_eq!(&packet[18..21], &[10, 20, 30]);
        assert!(packet[21..].iter().all(|&b| b == 0), "unused channels are zero");
    }

    #[test]
    fn each_universe_gets_its_own_channels_and_sequence_numbers_skip_zero() {
        let to: SocketAddr = "192.0.2.50:6454".parse().unwrap();
        let mut packets = ArtNetPackets::new(&[span(0, 0, 512), span(1, 512, 512)], to);
        let channels: Vec<u8> = (0..1024).map(|i| (i / 512 + 1) as u8).collect();
        for _ in 0..255 {
            packets.update(&channels);
        }
        assert_eq!(packets.packet(0).0[12], 255);
        packets.update(&channels);
        assert_eq!(packets.packet(0).0[12], 1, "after 255 comes 1, never 0");
        assert!(packets.packet(0).0[18..].iter().all(|&b| b == 1));
        assert!(packets.packet(1).0[18..].iter().all(|&b| b == 2));
        assert_eq!(packets.packet(1).0[14], 1);
    }
}
