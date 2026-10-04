//! Channel map types and frame-buffer layout.

use pf_model::{ColorOrder, ControllerId, PropId, Show};
use serde::Serialize;

/// Where a prop's pixels live in the show-wide frame buffer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropLayout {
    pub prop: PropId,
    /// Byte offset of the prop's first pixel in the frame buffer.
    pub frame_offset: usize,
    pub nodes: u32,
    pub channels_per_pixel: u8,
}

impl PropLayout {
    /// Bytes the prop occupies in the frame buffer.
    pub fn byte_len(&self) -> usize {
        self.nodes as usize * self.channels_per_pixel as usize
    }
}

/// Lays props out back to back in show order.
pub(crate) fn prop_layouts(show: &Show) -> Vec<PropLayout> {
    let mut offset = 0;
    show.props
        .iter()
        .map(|prop| {
            let layout = PropLayout {
                prop: prop.id,
                frame_offset: offset,
                nodes: prop.node_count(),
                channels_per_pixel: prop.channels_per_pixel(),
            };
            offset += layout.byte_len();
            layout
        })
        .collect()
}

/// A run of consecutive prop pixels copied from the frame buffer to consecutive
/// controller channels.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputSpan {
    pub prop: PropId,
    pub port: u16,
    /// First controller channel (0-based) the span writes.
    pub controller_channel: usize,
    /// Frame-buffer byte offset of the span's first node (lowest node index).
    pub frame_offset: usize,
    pub pixels: u32,
    pub channels_per_pixel: u8,
    /// When true, the last node goes out first.
    pub reverse: bool,
    pub color_order: ColorOrder,
    /// Percent, 0–100.
    pub brightness: u8,
    pub gamma: f32,
}

impl OutputSpan {
    pub fn byte_len(&self) -> usize {
        self.pixels as usize * self.channels_per_pixel as usize
    }

    /// Controller channel of the pixel whose data starts at `frame_byte`, if it is in this span.
    pub fn controller_channel_for(&self, frame_byte: usize) -> Option<usize> {
        let end = self.frame_offset + self.byte_len();
        if frame_byte < self.frame_offset || frame_byte >= end {
            return None;
        }
        let cpp = self.channels_per_pixel as usize;
        let index = (frame_byte - self.frame_offset) / cpp;
        let wire_index = if self.reverse {
            self.pixels as usize - 1 - index
        } else {
            index
        };
        Some(self.controller_channel + wire_index * cpp)
    }
}

/// A block of controller channels carried by one sACN universe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UniverseSpan {
    pub universe: u16,
    /// First controller channel (0-based) in this universe.
    pub controller_channel: usize,
    /// Channels used in this universe.
    pub len: u16,
}

/// How controller channels are addressed on the wire.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Addressing {
    /// Universes in controller-channel order; they cover the channels without gaps.
    Sacn {
        universes: Vec<UniverseSpan>,
        multicast: bool,
    },
    /// DDP addresses controller channels directly as byte offsets.
    Ddp,
}

/// Protocol address of one controller channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ChannelAddress {
    /// `channel` is 1-based within the universe.
    Sacn {
        universe: u16,
        channel: u16,
    },
    Ddp {
        offset: usize,
    },
}

impl Addressing {
    /// The wire address of a controller channel, or `None` if it is beyond the mapped channels.
    pub fn address_of(&self, controller_channel: usize) -> Option<ChannelAddress> {
        match self {
            Addressing::Ddp => Some(ChannelAddress::Ddp {
                offset: controller_channel,
            }),
            Addressing::Sacn { universes, .. } => {
                let i = universes
                    .partition_point(|u| u.controller_channel + u.len as usize <= controller_channel);
                let u = universes.get(i)?;
                let within = controller_channel.checked_sub(u.controller_channel)?;
                Some(ChannelAddress::Sacn {
                    universe: u.universe,
                    channel: u16::try_from(within + 1).ok()?,
                })
            }
        }
    }
}

/// Output plan for one controller.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerOutput {
    pub controller: ControllerId,
    /// Total controller channels used, including null pixels.
    pub channel_count: usize,
    pub addressing: Addressing,
    /// Spans in controller-channel order.
    pub spans: Vec<OutputSpan>,
}

/// Where one prop pixel goes on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PixelLocation {
    pub controller: ControllerId,
    pub port: u16,
    pub controller_channel: usize,
    pub address: ChannelAddress,
}

/// The complete mapping from frame buffer to the wire.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMap {
    /// Size of the show-wide frame buffer in bytes.
    pub frame_len: usize,
    pub props: Vec<PropLayout>,
    pub controllers: Vec<ControllerOutput>,
}

impl ChannelMap {
    pub fn prop_layout(&self, prop: PropId) -> Option<&PropLayout> {
        self.props.iter().find(|p| p.prop == prop)
    }

    /// Every wire location of a prop node (usually one; more if it is wired twice).
    pub fn locate(&self, prop: PropId, node: u32) -> Vec<PixelLocation> {
        let Some(layout) = self.prop_layout(prop) else {
            return Vec::new();
        };
        if node >= layout.nodes {
            return Vec::new();
        }
        let frame_byte = layout.frame_offset + node as usize * layout.channels_per_pixel as usize;
        let mut found = Vec::new();
        for output in &self.controllers {
            for span in &output.spans {
                let Some(channel) = span.controller_channel_for(frame_byte) else {
                    continue;
                };
                if let Some(address) = output.addressing.address_of(channel) {
                    found.push(PixelLocation {
                        controller: output.controller,
                        port: span.port,
                        controller_channel: channel,
                        address,
                    });
                }
            }
        }
        found
    }

    /// Total sACN universes across all controllers.
    pub fn universe_count(&self) -> usize {
        self.controllers
            .iter()
            .map(|c| match &c.addressing {
                Addressing::Sacn { universes, .. } => universes.len(),
                Addressing::Ddp => 0,
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(reverse: bool) -> OutputSpan {
        OutputSpan {
            prop: PropId::new(),
            port: 1,
            controller_channel: 30,
            frame_offset: 300,
            pixels: 10,
            channels_per_pixel: 3,
            reverse,
            color_order: ColorOrder::Rgb,
            brightness: 100,
            gamma: 1.0,
        }
    }

    #[test]
    fn span_maps_frame_bytes_forward_and_reversed() {
        assert_eq!(span(false).controller_channel_for(300), Some(30));
        assert_eq!(span(false).controller_channel_for(303), Some(33));
        assert_eq!(span(true).controller_channel_for(300), Some(57));
        assert_eq!(span(true).controller_channel_for(327), Some(30));
        assert_eq!(span(false).controller_channel_for(330), None);
        assert_eq!(span(false).controller_channel_for(299), None);
    }

    #[test]
    fn sacn_address_lookup_uses_one_based_channels() {
        let addressing = Addressing::Sacn {
            universes: vec![
                UniverseSpan {
                    universe: 5,
                    controller_channel: 0,
                    len: 510,
                },
                UniverseSpan {
                    universe: 6,
                    controller_channel: 510,
                    len: 90,
                },
            ],
            multicast: false,
        };
        assert_eq!(
            addressing.address_of(0),
            Some(ChannelAddress::Sacn {
                universe: 5,
                channel: 1
            })
        );
        assert_eq!(
            addressing.address_of(512),
            Some(ChannelAddress::Sacn {
                universe: 6,
                channel: 3
            })
        );
        assert_eq!(addressing.address_of(600), None);
        assert_eq!(
            Addressing::Ddp.address_of(42),
            Some(ChannelAddress::Ddp { offset: 42 })
        );
    }
}
