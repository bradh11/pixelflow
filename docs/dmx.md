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

The previews show the color the channels make, as a soft wash around the fixture: the same
wash, from the same numbers, in the 2D previews, the 3D view and an exported video.

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
  Windows). The ports PixelFlow can see are offered by what each device calls itself, such as
  "ENTTEC DMX USB Pro (cu.usbserial-EN123456)"; listing them doesn't open any. One universe. See
  [USB DMX interfaces](#usb-dmx-interfaces) below.

Drag each fixture onto its universe (or use **+ Add…**): it goes to its own start address, shown
as "@ 9". A DMX controller can't drive pixels; the Wiring screen says so.

A pixel controller with a DMX output (a Falcon's or an FPP's DMX port, fed from its own
universes) carries fixtures too: wire the fixture on that controller's port after its pixels,
with null channels before it if there's a gap. It takes its channels where it falls, in one piece
(never split between two universes), and the Wiring screen warns when that isn't the start
address set on the fixture.

An xLights show's DMX floods (`DmxGeneral`, `DmxFloodlight`, `DmxFloodArea`) import as fixtures,
with their color channels and their shutter, wired on their controller at the channels xLights
gives them, beside its pixels. Moving heads and servos import as rows of channel dots, not wired:
PixelFlow doesn't drive them yet.

## Testing

On **Test**, pick the fixture as the target and choose **DMX channels by hand**: a slider for each
channel, live on the fixture while you move it. Any other pattern lights the fixture through its
color, like a pixel. The same rules apply as for every test, over the network or USB: nothing is
sent (and no USB interface is opened) until you press Start, and Stop sends black.

## USB DMX interfaces

PixelFlow drives interfaces that speak the ENTTEC DMX USB Pro protocol: the ENTTEC DMX USB Pro
and Pro Mk2 (its first universe), and the many interfaces that copy the Pro's protocol, such as
DMXking's ultraDMX range. Each frame it sends the interface the universe's 512 channels as one
"send DMX" message; the interface makes the DMX signal itself and repeats the last message it was
given until the next one comes.

- **How often.** One message per show frame, but no more often than a DMX line can carry a whole
  universe (about 44 a second): a show at 40 frames a second sends every frame, a faster one the
  newest frame each time.
- **The port.** PixelFlow opens the port only when you start a test or playback, and holds it
  while it sends: another program can't use the interface at the same time (close other lighting
  software first). It opens only real serial ports: a name in a show file that isn't one of this
  computer's serial ports is refused.
- **Unplugged mid-show.** The controller turns to "degraded" in the Test screen's list, with
  "the USB DMX interface on … stopped answering: was it unplugged?", and everything else carries
  on. PixelFlow tries again after a moment, less often the longer it's gone (up to every five
  seconds), and picks the interface up again by itself once it's plugged back in on the same
  port. An interface that is still there but stops taking data is given 20 ms per message before
  it counts as stuck, so it never holds up the rest of the show.

### Setting up, by system

- **macOS.** The Pro and most copies appear as `/dev/cu.usbserial-<serial number>` (FTDI chips,
  with the driver built into macOS) or `/dev/cu.usbmodem…` (interfaces that are USB serial
  devices themselves). Use the `cu.` port; each also has a `tty.` twin, meant for modems. No driver
  to install. If PixelFlow is ever built as a sandboxed app, it needs the
  `com.apple.security.device.serial` entitlement to open serial ports (today's builds aren't
  sandboxed and need nothing).
- **Linux.** The interface appears as `/dev/ttyUSB0` (FTDI, the `ftdi_sio` driver) or
  `/dev/ttyACM0` (`cdc_acm`). Opening it needs your user in the group that owns it: `dialout` on
  Debian, Ubuntu and Fedora, `uucp` on Arch (`sudo usermod -aG dialout $USER`, then sign out and
  in again). Without it, Test says PixelFlow isn't allowed to open the port. A name that stays the
  same whichever USB socket you use is under `/dev/serial/by-id/`; PixelFlow accepts those too.
  The port list comes from `/sys`, so nothing extra (no libudev) needs to be installed.
- **Windows.** The interface is a COM port (`COM3`, say) through FTDI's VCP ("virtual COM port")
  driver, which Windows Update usually installs (if not, it's on FTDI's website); Device Manager
  under **Ports (COM & LPT)** shows its number. Windows lets only one program open a COM port at a time.

### Interfaces that aren't supported

"Open DMX"-style interfaces (the ENTTEC Open DMX USB and its many copies: a bare FTDI chip with no
processor of its own) don't speak the Pro protocol. With them the computer has to make the DMX
signal itself: switch the line to 250 kbaud, make the break and the mark after it with exact
timing, and send all 513 slots, again and again. A desktop system's scheduling makes that timing
waver, which fixtures show as flicker, and doing it well needs FTDI's own closed-source driver.
PixelFlow doesn't drive them; use a Pro-compatible interface or an Art-Net or sACN node instead.
