//! DDP (Distributed Display Protocol) packets.

use std::net::SocketAddr;

/// DDP header length.
pub const HEADER_LEN: usize = 10;
/// Largest data payload per packet (keeps packets inside a standard Ethernet MTU).
pub const MAX_DATA: usize = 1440;

const FLAG_VERSION_1: u8 = 0x40;
const FLAG_PUSH: u8 = 0x01;
const DESTINATION_DISPLAY: u8 = 0x01;

/// Pre-built DDP packets covering one controller's channels.
#[derive(Debug)]
pub struct DdpPackets {
    packets: Vec<Vec<u8>>,
    destination: SocketAddr,
    sequence: u8,
}

impl DdpPackets {
    /// Splits `channel_count` bytes into packets of at most [`MAX_DATA`] bytes. The last
    /// packet carries the push flag so the controller displays the frame. `offset_base` is the
    /// DDP offset of the first byte: 0 normally, or the absolute channel number (zero-based)
    /// for a controller that expects raw channel numbers.
    pub fn new(channel_count: usize, data_type: u8, offset_base: u32, destination: SocketAddr) -> Self {
        let chunks = channel_count.div_ceil(MAX_DATA);
        let packets = (0..chunks)
            .map(|i| {
                let offset = i * MAX_DATA;
                let len = (channel_count - offset).min(MAX_DATA);
                let mut packet = vec![0u8; HEADER_LEN + len];
                packet[0] = FLAG_VERSION_1 | if i + 1 == chunks { FLAG_PUSH } else { 0 };
                packet[2] = data_type;
                packet[3] = DESTINATION_DISPLAY;
                packet[4..8].copy_from_slice(&offset_base.wrapping_add(offset as u32).to_be_bytes());
                packet[8..10].copy_from_slice(&(len as u16).to_be_bytes());
                packet
            })
            .collect();
        Self {
            packets,
            destination,
            sequence: 0,
        }
    }

    /// Copies the channels into the packets and advances the sequence number (1–15).
    pub fn update(&mut self, channels: &[u8]) {
        self.sequence = self.sequence % 15 + 1;
        for (i, packet) in self.packets.iter_mut().enumerate() {
            let offset = i * MAX_DATA;
            let len = packet.len() - HEADER_LEN;
            if let Some(source) = channels.get(offset..offset + len) {
                packet[HEADER_LEN..].copy_from_slice(source);
            }
            packet[1] = self.sequence;
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

    /// Where every packet goes.
    pub fn destination(&self) -> SocketAddr {
        self.destination
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dest() -> SocketAddr {
        "10.0.0.2:4048".parse().unwrap()
    }

    #[test]
    fn splits_into_mtu_sized_packets_with_push_on_last() {
        let packets = DdpPackets::new(3000, 0x0B, 0, dest());
        assert_eq!(packets.len(), 3);
        let lens: Vec<usize> = (0..3).map(|i| packets.packet(i).0.len()).collect();
        assert_eq!(lens, vec![1450, 1450, 130]);
        let (last, to) = packets.packet(2);
        assert_eq!(to, dest());
        assert_eq!(
            &last[..HEADER_LEN],
            &[0x41, 0, 0x0B, 0x01, 0, 0, 0x0B, 0x40, 0, 120]
        );
        assert_eq!(packets.packet(0).0[0], 0x40);
    }

    #[test]
    fn raw_channel_numbers_start_the_offset_at_the_controllers_first_channel() {
        let packets = DdpPackets::new(3000, 0x0B, 6147, dest());
        let offsets: Vec<u32> = (0..3)
            .map(|i| u32::from_be_bytes(packets.packet(i).0[4..8].try_into().unwrap()))
            .collect();
        assert_eq!(offsets, vec![6147, 6147 + 1440, 6147 + 2880]);
    }

    #[test]
    fn update_copies_data_and_cycles_sequence_1_to_15() {
        let mut packets = DdpPackets::new(4, 0x0B, 0, dest());
        packets.update(&[9, 8, 7, 6]);
        assert_eq!(&packets.packet(0).0[HEADER_LEN..], &[9, 8, 7, 6]);
        assert_eq!(packets.packet(0).0[1], 1);
        for _ in 0..14 {
            packets.update(&[0; 4]);
        }
        assert_eq!(packets.packet(0).0[1], 15);
        packets.update(&[0; 4]);
        assert_eq!(packets.packet(0).0[1], 1);
    }

    #[test]
    fn no_channels_means_no_packets() {
        assert!(DdpPackets::new(0, 0x0B, 0, dest()).is_empty());
    }
}
