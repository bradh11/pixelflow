# DMX floods, pars, and bars

PixelFlow drives DMX fixtures alongside your pixels: RGB, RGBW, RGBA and RGBWA floods and pars,
usually with a master dimmer and often a strobe, and bars of several separately colored cells.
You color them with the same effects as any prop, and PixelFlow turns that color into the
fixture's channels.

> **Safety first.** A DMX universe can drive things that flash or move: a strobe channel is a real
> strobe, a macro channel can start a light show of its own, and the same universe can carry a
> moving head or a fog machine. Before you send anything, know what is on each address of the
> universe, and read each fixture's manual for what its channels do. Keep strobes short.

## Adding a fixture

1. On **Layout**, choose **Add prop ▾ → DMX flood or bar**. A flood lands in the middle of the
   canvas; drag it to where the light is (a flood on the lawn washing the house front, a par at
   the foot of a tree).
2. In its properties, under **DMX fixture**:
   - **Channels**: pick the layout from the fixture's manual. The library covers the common ones,
     named by their channels: 3 channels (red, green, blue), 4 (dimmer, red, green, blue), 4 (red,
     green, blue, white), 5, 6, 7 (dimmer, RGBW, strobe, mode), 8 (dimmer, RGBWA, UV, strobe), and
     bars with red, green and blue (or white) for each cell.
   - **Start address**: the address set on the fixture (its DIP switches or menu), 1 to 512.
   - **Cells**, for a bar: how many separately colored sections it has.
   - **Each channel**: for a fixture the library doesn't have, set each channel's name and what it
     does: dimmer, red, green, blue, white, amber, UV, strobe, mode (a value held), and the
     moving-head roles (pan, tilt, their fine channels, color wheel, gobo), which hold their value
     for now. A channel that isn't a color holds the value you give it, unless a DMX effect sets
     it: put the mode channel at the value the manual gives for "DMX control".

Several fixtures share one universe: give each its own start address, after the last channel of
the one before. The Wiring screen warns when two overlap.

## How a color becomes channels

- **Dimmer.** On a fixture with a dimmer channel, the dimmer carries the brightness and the color
  channels carry the color at full. Floods have 8-bit color channels: a dim orange sent through
  them alone loses its hue (orange at 5% is red 13, green 5), and a slow fade steps. Through the
  dimmer the hue stays true and the fade is as smooth as the fixture's dimmer. On a bar, the one
  dimmer follows the brightest cell. Without a dimmer channel, the color channels carry the
  brightness.
- **White** is the part of the color that red, green and blue share; it comes out of them and
  goes to the white LEDs, which make a whiter white.
- **Amber** stands for orange (255, 128, 0): after white, as much of the color as is orange goes
  to amber.
- **UV, strobe and mode** channels hold their value.

The previews show the color the channels make, as a soft wash around the fixture.

## The DMX effect

The **DMX** effect sets any of a fixture's channels directly, by number from its start address
(channel 1 is the fixture's first), each 0 to 255, and each can change over the effect with a
curve: a strobe burst on a hit, the dimmer swelling up into a chorus, a mode or macro. Channels it
doesn't set come from the color drawn on the fixture as usual, so put it on a layer above an
`On` or `Color Wash`. On a group, it sets that channel of every fixture in the group. Where two
DMX effects set one channel at once, the later row (or the higher layer) wins. It draws no color
and isn't faded by fades.

## Wiring: DMX nodes and USB interfaces

On **Wiring**, choose **Add controller** and a DMX controller type:

- **DMX node (Art-Net)** or **DMX node (sACN)**: a box on your network with DMX outputs. Each port
  is one DMX universe. Leave the start universe empty to let PixelFlow number them (Art-Net from
  0, sACN from 1), or set the one the node listens on. PixelFlow sends every universe whole, all
  512 channels, every frame, as DMX receivers expect. Art-Net goes to the node's address on UDP
  port 6454 (PixelFlow doesn't poll for nodes: give it the address); sACN to port 5568, or by
  multicast.
- **USB DMX interface**: an interface that speaks the ENTTEC DMX USB Pro protocol (many do). Its
  address is its serial port (`/dev/cu.usbserial-…` on a Mac, `/dev/ttyUSB0` on Linux, `COM3` on
  Windows); the ports PixelFlow can see are offered. One universe. **Not yet working**: this
  version can't open a serial port, so the Test screen says so instead of sending. Everything else
  about it (the wiring, the messages it would send) is in place.

Drag each fixture onto its universe (or use **+ Add…**): it goes to its own start address, shown
as "@ 9". A pixel controller can't drive a fixture, and a DMX controller can't drive pixels; the
Wiring screen says so.

An xLights show's DMX floods (`DmxGeneral`, `DmxFloodlight`, `DmxFloodArea`) import as fixtures,
with their color channels and their shutter, each on a DMX controller for its universe at its
address. Moving heads and servos import as rows of channel dots, not wired: PixelFlow doesn't
drive them yet.

## Testing

On **Test**, pick the fixture as the target and choose **DMX channels by hand**: a slider for each
channel, live on the fixture while you move it. Any other pattern lights the fixture through its
color, like a pixel. The same rules apply as for every test: nothing is sent until you press
Start, and Stop sends black.
