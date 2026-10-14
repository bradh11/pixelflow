# DMX

Sets a DMX fixture's channels directly: what the color drawn on a flood can't say, like its strobe, a mode, or its dimmer held at a level.

## What it draws

No color of its own. On a DMX fixture (a flood, a par, a bar of several cells) it sets the channels it names, counted from the fixture's own first channel, for as long as it plays. The fixture's other channels come from the color the other effects draw on it, as always, so it sits on a layer above an `on` or a `colorWash`.

- On a single flood or par: the strobe opened, a mode or macro picked, the dimmer held or swelled. Setting its dimmer and color channels lights it in that color on its own.
- On a bar of cells it sets the bar's shared channels (its dimmer, its strobe); the cells' colors still come from what is drawn on them.
- On the whole house, or any group, it sets channel n of every fixture in the group, so it suits a group of floods of one kind; the props that aren't fixtures ignore it.
- On a string, an arch, a matrix, or a tree it does nothing: they have no DMX channels.

Which channel does what is the fixture's own: `get_prop` shows a flood's channels by number and role.

## Settings

- `channels`: which channels to set and to what, by number from the fixture's first channel, each 0 to 255, like {"1": 255, "5": 200}. A channel left out comes from the color. A curve keyed "channels." and the number changes one over the effect: a "ramp" from 0 to 255 on the dimmer swells the floods up.

## When to use it

- A strobe burst on the floods for a big hit, with the color from an `on` on the layer below. Keep it short, and mind flash safety: a strobe channel is a real strobe.
- Swelling the floods' dimmer up into a chorus, or down at the end, while their color comes from the effects below.
- Holding a mode or macro channel at the value the fixture's manual gives.
- Don't use it for color: draw color with `on`, `colorWash`, or `fade`, and the fixture's red, green, blue, white, amber, and dimmer channels follow by themselves.

## Looks

```look
{"name": "Floods to full white", "suits": ["house"],
 "note": "Every flood's dimmer and its red, green, and blue channels at full: white light on the house front.",
 "effect": {"kind": "dmx", "settings": {"channels": {"1": 255, "2": 255, "3": 255, "4": 255}}}}
```

```look
{"name": "Dimmer swell", "suits": ["house"],
 "note": "The floods' dimmer rising from dark to full across the effect, in warm orange.",
 "effect": {"kind": "dmx", "settings": {"channels": {"2": 255, "3": 120}}, "curves": {"channels.1": {"shape": "ramp", "from": 0, "to": 255}}}}
```

```look
{"name": "Strobe hit on the floods", "suits": ["house"],
 "note": "The floods at full white with their strobe channel (here the fifth) opened fast, for a short hit.",
 "effect": {"kind": "dmx", "settings": {"channels": {"1": 255, "2": 255, "3": 255, "4": 255, "5": 230}}}}
```
