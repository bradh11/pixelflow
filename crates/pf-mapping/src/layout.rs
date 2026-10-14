//! Channel map types and frame-buffer layout.

use pf_model::{ColorOrder, ControllerId, DmxChannel, PropId, Show};
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

/// Each prop's fixture layout (`None` for a pixel prop, or a fixture PixelFlow can't use), its
/// channels laid out back to back from `from`, after every prop's pixels.
pub(crate) fn fixture_layouts(show: &Show, props: &[PropLayout], from: usize) -> Vec<Option<FixtureLayout>> {
    let mut offset = from;
    show.props
        .iter()
        .zip(props)
        .map(|(prop, layout)| {
            let dmx = prop.dmx.as_ref()?;
            if dmx.problem(layout.nodes).is_some() {
                return None;
            }
            let fixture = FixtureLayout {
                prop: prop.id,
                cells_offset: layout.frame_offset,
                cells: layout.nodes,
                dmx_offset: offset,
                address: dmx.address,
                channels: dmx.channels.clone(),
            };
            offset += fixture.footprint();
            Some(fixture)
        })
        .collect()
}

/// Where a DMX fixture's channels live in the show frame: after every prop's pixels, one byte a
/// channel, in the fixture's own order. Its cells (in [`PropLayout`]) hold the colour it shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureLayout {
    pub prop: PropId,
    /// Frame offset of its first cell's colour (three bytes a cell).
    pub cells_offset: usize,
    pub cells: u32,
    /// Frame offset of its first channel.
    pub dmx_offset: usize,
    /// The DMX address (1-512) its first channel goes to.
    pub address: u16,
    pub channels: Vec<DmxChannel>,
}

impl FixtureLayout {
    /// Channels it takes.
    pub fn footprint(&self) -> usize {
        self.channels.len()
    }

    /// Its cells' colour and its channels, as two slices of `frame`; `None` when it isn't in it.
    fn parts<'a>(&self, frame: &'a mut [u8]) -> Option<(&'a mut [u8], &'a mut [u8])> {
        let cells_end = self.cells_offset + self.cells as usize * 3;
        if cells_end > self.dmx_offset || self.dmx_offset + self.footprint() > frame.len() {
            return None;
        }
        let (pixels, dmx) = frame.split_at_mut(self.dmx_offset);
        Some((
            &mut pixels[self.cells_offset..cells_end],
            &mut dmx[..self.footprint()],
        ))
    }

    /// Turns the colour drawn on its cells into its channels (see `pf_model::write_channels`),
    /// sets the channels `overrides` sets (channel `i` to `overrides[i]` where that's `Some`;
    /// empty: none), then writes the colour those channels make back into its cells, for the
    /// previews. Allocation-free; a fixture outside `frame` is left alone.
    pub fn write(&self, frame: &mut [u8], overrides: &[Option<u8>]) {
        let Some((cells, dmx)) = self.parts(frame) else {
            return;
        };
        pf_model::write_channels(&self.channels, cells, dmx);
        for (byte, value) in dmx.iter_mut().zip(overrides) {
            if let Some(value) = value {
                *byte = *value;
            }
        }
        pf_model::emitted_colors(&self.channels, dmx, cells);
    }

    /// Sets its channels to `values` as they are (the others: colour and dimmer channels to
    /// zero, the rest to their held values), and its cells to the colour they make: a test of
    /// the fixture by hand.
    pub fn write_values(&self, frame: &mut [u8], values: &[Option<u8>]) {
        let Some((cells, dmx)) = self.parts(frame) else {
            return;
        };
        for (i, (byte, channel)) in dmx.iter_mut().zip(&self.channels).enumerate() {
            let held = if channel.role.from_color() {
                0
            } else {
                channel.value
            };
            *byte = values.get(i).copied().flatten().unwrap_or(held);
        }
        pf_model::emitted_colors(&self.channels, dmx, cells);
    }
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
    /// Art-Net universes in controller-channel order, numbered from 0.
    ArtNet { universes: Vec<UniverseSpan> },
    /// A USB DMX interface: one universe, controller channel `n` is DMX channel `n + 1`.
    DmxUsb,
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
    /// `channel` is 1-based within the universe.
    ArtNet {
        universe: u16,
        channel: u16,
    },
    /// `channel` is 1-based.
    DmxUsb {
        channel: u16,
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
                let (universe, channel) = within(universes, controller_channel)?;
                Some(ChannelAddress::Sacn { universe, channel })
            }
            Addressing::ArtNet { universes } => {
                let (universe, channel) = within(universes, controller_channel)?;
                Some(ChannelAddress::ArtNet { universe, channel })
            }
            Addressing::DmxUsb => (controller_channel < 512).then(|| ChannelAddress::DmxUsb {
                channel: controller_channel as u16 + 1,
            }),
        }
    }

    /// The universes it numbers on the wire (sACN or Art-Net).
    pub fn universes(&self) -> &[UniverseSpan] {
        match self {
            Addressing::Sacn { universes, .. } | Addressing::ArtNet { universes } => universes,
            Addressing::Ddp | Addressing::DmxUsb => &[],
        }
    }
}

/// The universe holding `controller_channel` among `universes`, and the channel's place in it
/// (1-based).
fn within(universes: &[UniverseSpan], controller_channel: usize) -> Option<(u16, u16)> {
    let i = universes.partition_point(|u| u.controller_channel + u.len as usize <= controller_channel);
    let u = universes.get(i)?;
    let within = controller_channel.checked_sub(u.controller_channel)?;
    Some((u.universe, u16::try_from(within + 1).ok()?))
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
    /// The DMX fixtures' channels, after every prop's pixels, in show order.
    pub fixtures: Vec<FixtureLayout>,
    pub controllers: Vec<ControllerOutput>,
}

impl ChannelMap {
    pub fn prop_layout(&self, prop: PropId) -> Option<&PropLayout> {
        self.props.iter().find(|p| p.prop == prop)
    }

    pub fn fixture(&self, prop: PropId) -> Option<&FixtureLayout> {
        self.fixtures.iter().find(|f| f.prop == prop)
    }

    /// Turns the colour drawn on every fixture into its channels (see [`FixtureLayout::write`]),
    /// with no channel set by hand or by an effect.
    pub fn write_fixtures(&self, frame: &mut [u8]) {
        for fixture in &self.fixtures {
            fixture.write(frame, &[]);
        }
    }

    /// Every wire location of a prop node (usually one; more if it is wired twice). For a DMX
    /// fixture, where its first channel goes, whichever cell.
    pub fn locate(&self, prop: PropId, node: u32) -> Vec<PixelLocation> {
        let Some(layout) = self.prop_layout(prop) else {
            return Vec::new();
        };
        if node >= layout.nodes {
            return Vec::new();
        }
        let frame_byte = match self.fixture(prop) {
            Some(fixture) => fixture.dmx_offset,
            None => layout.frame_offset + node as usize * layout.channels_per_pixel as usize,
        };
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

    /// Total sACN and Art-Net universes across all controllers.
    pub fn universe_count(&self) -> usize {
        self.controllers
            .iter()
            .map(|c| c.addressing.universes().len())
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
