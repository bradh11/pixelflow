//! sACN (ANSI E1.31-2018) data and synchronization packets.

use crate::settings::OutputSettings;
use pf_mapping::UniverseSpan;
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};

use crate::plan::SACN_PORT;

/// Bytes before the DMX data in an E1.31 data packet (root + framing + DMP layers + start code).
pub const DATA_HEADER_LEN: usize = 126;
/// Length of an E1.31 synchronization packet.
pub const SYNC_PACKET_LEN: usize = 49;

const ACN_PACKET_ID: [u8; 12] = *b"ASC-E1.17\0\0\0";
const VECTOR_ROOT_E131_DATA: u32 = 0x0000_0004;
const VECTOR_ROOT_E131_EXTENDED: u32 = 0x0000_0008;
const VECTOR_E131_DATA_PACKET: u32 = 0x0000_0002;
const VECTOR_E131_EXTENDED_SYNCHRONIZATION: u32 = 0x0000_0001;
const VECTOR_DMP_SET_PROPERTY: u8 = 0x02;
/// Options bit telling receivers this source has stopped (E1.31 section 6.2.6).
const OPTION_STREAM_TERMINATED: u8 = 0x40;
/// Longest source name sent, in bytes (the field is 64 bytes, null-terminated).
const MAX_SOURCE_NAME: usize = 63;

/// The last sequence number sent on each sACN stream, by destination and universe.
pub type SacnSequences = HashMap<(SocketAddr, u16), u8>;

/// The multicast group for a universe: 239.255.<high byte>.<low byte>, port 5568.
pub fn multicast_addr(universe: u16) -> SocketAddr {
    let [hi, lo] = universe.to_be_bytes();
    SocketAddr::from((Ipv4Addr::new(239, 255, hi, lo), SACN_PORT))
}

/// Flags (0x7) and a 12-bit PDU length, big-endian.
fn flags_and_length(len: usize) -> [u8; 2] {
    (0x7000 | (len as u16 & 0x0fff)).to_be_bytes()
}

/// Writes a complete E1.31 data packet header for `channels` DMX channels; sequence 0.
pub fn write_data_header(packet: &mut [u8], universe: u16, channels: usize, settings: &OutputSettings) {
    let len = DATA_HEADER_LEN + channels;
    packet[0..2].copy_from_slice(&0x0010u16.to_be_bytes());
    packet[2..4].copy_from_slice(&0u16.to_be_bytes());
    packet[4..16].copy_from_slice(&ACN_PACKET_ID);
    packet[16..18].copy_from_slice(&flags_and_length(len - 16));
    packet[18..22].copy_from_slice(&VECTOR_ROOT_E131_DATA.to_be_bytes());
    packet[22..38].copy_from_slice(&settings.cid);
    packet[38..40].copy_from_slice(&flags_and_length(len - 38));
    packet[40..44].copy_from_slice(&VECTOR_E131_DATA_PACKET.to_be_bytes());
    packet[44..108].fill(0);
    let name = source_name(&settings.source_name).as_bytes();
    packet[44..44 + name.len()].copy_from_slice(name);
    packet[108] = settings.priority.min(200);
    packet[109..111].copy_from_slice(&settings.sync_universe.unwrap_or(0).to_be_bytes());
    packet[111] = 0;
    packet[112] = 0;
    packet[113..115].copy_from_slice(&universe.to_be_bytes());
    packet[115..117].copy_from_slice(&flags_and_length(len - 115));
    packet[117] = VECTOR_DMP_SET_PROPERTY;
    packet[118] = 0xa1;
    packet[119..121].copy_from_slice(&0u16.to_be_bytes());
    packet[121..123].copy_from_slice(&1u16.to_be_bytes());
    packet[123..125].copy_from_slice(&((channels + 1) as u16).to_be_bytes());
    packet[125] = 0;
}

/// `name` cut to at most [`MAX_SOURCE_NAME`] bytes on a character boundary: receivers read
/// the field as UTF-8, so half a character would show as garbage.
fn source_name(name: &str) -> &str {
    let mut end = name.len().min(MAX_SOURCE_NAME);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    &name[..end]
}

/// An E1.31 synchronization packet for `sync_universe`.
pub fn sync_packet(settings: &OutputSettings, sync_universe: u16, sequence: u8) -> [u8; SYNC_PACKET_LEN] {
    let mut packet = [0u8; SYNC_PACKET_LEN];
    packet[0..2].copy_from_slice(&0x0010u16.to_be_bytes());
    packet[4..16].copy_from_slice(&ACN_PACKET_ID);
    packet[16..18].copy_from_slice(&flags_and_length(SYNC_PACKET_LEN - 16));
    packet[18..22].copy_from_slice(&VECTOR_ROOT_E131_EXTENDED.to_be_bytes());
    packet[22..38].copy_from_slice(&settings.cid);
    packet[38..40].copy_from_slice(&flags_and_length(SYNC_PACKET_LEN - 38));
    packet[40..44].copy_from_slice(&VECTOR_E131_EXTENDED_SYNCHRONIZATION.to_be_bytes());
    packet[44] = sequence;
    packet[45..47].copy_from_slice(&sync_universe.to_be_bytes());
    packet
}

/// Pre-built data packets for one controller, one per universe.
#[derive(Debug)]
pub struct SacnPackets {
    packets: Vec<Vec<u8>>,
    universes: Vec<UniverseSpan>,
    destinations: Vec<SocketAddr>,
}

impl SacnPackets {
    /// Allocates and pre-fills every packet header. With `multicast`, each universe goes to
    /// its multicast group; otherwise all go to `unicast`.
    pub fn new(
        universes: &[UniverseSpan],
        multicast: bool,
        unicast: SocketAddr,
        settings: &OutputSettings,
    ) -> Self {
        let packets = universes
            .iter()
            .map(|u| {
                let mut packet = vec![0u8; DATA_HEADER_LEN + u.len as usize];
                write_data_header(&mut packet, u.universe, u.len as usize, settings);
                packet
            })
            .collect();
        let destinations = universes
            .iter()
            .map(|u| {
                if multicast {
                    multicast_addr(u.universe)
                } else {
                    unicast
                }
            })
            .collect();
        Self {
            packets,
            universes: universes.to_vec(),
            destinations,
        }
    }

    /// Copies the controller's channels into each packet and advances each universe's
    /// sequence number.
    pub fn update(&mut self, channels: &[u8]) {
        for (packet, u) in self.packets.iter_mut().zip(&self.universes) {
            let start = u.controller_channel;
            let len = u.len as usize;
            if let Some(source) = channels.get(start..start + len) {
                packet[DATA_HEADER_LEN..].copy_from_slice(source);
            }
            packet[111] = packet[111].wrapping_add(1);
        }
    }

    /// Marks every packet Stream_Terminated, or clears the mark. Receivers ignore the data in a
    /// terminated packet and stop expecting this source, rather than waiting for it to time out.
    pub fn set_terminated(&mut self, terminated: bool) {
        for packet in &mut self.packets {
            if terminated {
                packet[112] |= OPTION_STREAM_TERMINATED;
            } else {
                packet[112] &= !OPTION_STREAM_TERMINATED;
            }
        }
    }

    /// Records the last sequence number sent on each of these streams into `into`.
    pub fn save_sequences(&self, into: &mut SacnSequences) {
        for ((packet, u), to) in self.packets.iter().zip(&self.universes).zip(&self.destinations) {
            into.insert((*to, u.universe), packet[111]);
        }
    }

    /// Carries on from the sequence numbers another set of packets reached on the same streams,
    /// so receivers don't see a stream jump back (and drop its packets as out of order) when
    /// output switches to a new plan.
    pub fn continue_sequences(&mut self, last: &SacnSequences) {
        for ((packet, u), to) in self
            .packets
            .iter_mut()
            .zip(&self.universes)
            .zip(&self.destinations)
        {
            if let Some(&sequence) = last.get(&(*to, u.universe)) {
                packet[111] = sequence;
            }
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
        (&self.packets[i], self.destinations[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> OutputSettings {
        OutputSettings {
            source_name: "PixelFlow".into(),
            priority: 100,
            sync_universe: None,
            cid: [0xAB; 16],
        }
    }

    #[test]
    fn data_packet_layout_matches_e131() {
        let mut packet = vec![0u8; DATA_HEADER_LEN + 3];
        write_data_header(&mut packet, 0x0102, 3, &settings());
        assert_eq!(&packet[0..4], &[0x00, 0x10, 0x00, 0x00]);
        assert_eq!(&packet[4..16], b"ASC-E1.17\0\0\0");
        assert_eq!(&packet[16..18], &[0x70, 0x71]); // 129 - 16 = 113
        assert_eq!(&packet[18..22], &[0, 0, 0, 4]);
        assert_eq!(&packet[22..38], &[0xAB; 16]);
        assert_eq!(&packet[38..40], &[0x70, 0x5B]); // 129 - 38 = 91
        assert_eq!(&packet[40..44], &[0, 0, 0, 2]);
        assert_eq!(&packet[44..53], b"PixelFlow");
        assert!(packet[53..108].iter().all(|&b| b == 0));
        assert_eq!(packet[108], 100);
        assert_eq!(&packet[109..111], &[0, 0]);
        assert_eq!(&packet[113..115], &[0x01, 0x02]);
        assert_eq!(&packet[115..117], &[0x70, 0x0E]); // 129 - 115 = 14
        assert_eq!(
            &packet[117..126],
            &[0x02, 0xA1, 0x00, 0x00, 0x00, 0x01, 0x00, 0x04, 0x00]
        );
    }

    #[test]
    fn sync_universe_and_long_names_are_encoded() {
        let mut s = settings();
        s.sync_universe = Some(999);
        s.source_name = "x".repeat(80);
        s.priority = 250;
        let mut packet = vec![0u8; DATA_HEADER_LEN];
        write_data_header(&mut packet, 1, 0, &s);
        assert_eq!(&packet[109..111], &999u16.to_be_bytes());
        assert_eq!(packet[44 + 62], b'x');
        assert_eq!(packet[44 + 63], 0);
        assert_eq!(packet[108], 200);
    }

    #[test]
    fn long_names_are_cut_on_a_character_boundary() {
        let mut s = settings();
        // 62 ASCII bytes, then a 3-byte character that byte 63 would split.
        s.source_name = format!("{}€x", "a".repeat(62));
        let mut packet = vec![0u8; DATA_HEADER_LEN];
        write_data_header(&mut packet, 1, 0, &s);
        assert_eq!(&packet[44..106], "a".repeat(62).as_bytes());
        assert!(packet[106..108].iter().all(|&b| b == 0), "no half character");
        assert_eq!(source_name("Hallo Welt ✨"), "Hallo Welt ✨");
    }

    #[test]
    fn terminated_packets_carry_the_option_bit_and_sequences_carry_over() {
        let universes = [UniverseSpan {
            universe: 7,
            controller_channel: 0,
            len: 3,
        }];
        let dest: SocketAddr = "10.0.0.1:5568".parse().unwrap();
        let mut packets = SacnPackets::new(&universes, false, dest, &settings());
        assert_eq!(packets.packet(0).0[112], 0);
        packets.set_terminated(true);
        assert_eq!(packets.packet(0).0[112], 0x40);
        packets.set_terminated(false);
        assert_eq!(packets.packet(0).0[112], 0);

        for _ in 0..300 {
            packets.update(&[0, 0, 0]);
        }
        assert_eq!(packets.packet(0).0[111], 44, "300 wraps past 255");
        let mut last = SacnSequences::new();
        packets.save_sequences(&mut last);
        assert_eq!(last.get(&(dest, 7)), Some(&44));
        let mut next = SacnPackets::new(&universes, false, dest, &settings());
        next.continue_sequences(&last);
        next.update(&[0, 0, 0]);
        assert_eq!(next.packet(0).0[111], 45);
        // Universe 7 at another destination is a different stream.
        let other: SocketAddr = "10.0.0.2:5568".parse().unwrap();
        let mut elsewhere = SacnPackets::new(&universes, false, other, &settings());
        elsewhere.continue_sequences(&last);
        elsewhere.update(&[0, 0, 0]);
        assert_eq!(elsewhere.packet(0).0[111], 1);
    }

    #[test]
    fn sync_packet_layout_matches_e131() {
        let packet = sync_packet(&settings(), 7, 42);
        assert_eq!(&packet[16..18], &[0x70, 0x21]); // 49 - 16 = 33
        assert_eq!(&packet[18..22], &[0, 0, 0, 8]);
        assert_eq!(&packet[38..40], &[0x70, 0x0B]); // 49 - 38 = 11
        assert_eq!(&packet[40..44], &[0, 0, 0, 1]);
        assert_eq!(packet[44], 42);
        assert_eq!(&packet[45..47], &[0, 7]);
    }

    #[test]
    fn multicast_groups_follow_universe_number() {
        assert_eq!(multicast_addr(1), "239.255.0.1:5568".parse().unwrap());
        assert_eq!(multicast_addr(0x0203), "239.255.2.3:5568".parse().unwrap());
    }

    #[test]
    fn update_copies_each_universe_slice_and_counts_sequences() {
        let universes = [
            UniverseSpan {
                universe: 5,
                controller_channel: 0,
                len: 3,
            },
            UniverseSpan {
                universe: 6,
                controller_channel: 3,
                len: 2,
            },
        ];
        let dest: SocketAddr = "10.0.0.1:5568".parse().unwrap();
        let mut packets = SacnPackets::new(&universes, false, dest, &settings());
        packets.update(&[1, 2, 3, 4, 5]);
        packets.update(&[1, 2, 3, 4, 5]);
        let (first, to) = packets.packet(0);
        assert_eq!(to, dest);
        assert_eq!(&first[DATA_HEADER_LEN..], &[1, 2, 3]);
        assert_eq!(first[111], 2);
        let (second, _) = packets.packet(1);
        assert_eq!(&second[DATA_HEADER_LEN..], &[4, 5]);
        assert_eq!(&second[113..115], &[0, 6]);

        let multicast = SacnPackets::new(&universes, true, dest, &settings());
        assert_eq!(multicast.packet(1).1, multicast_addr(6));
    }
}
