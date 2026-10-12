# Fan

Curved blades that open out from a point, make a set number of turns over the effect, and fly away as it ends: a spinning flourish for a matrix or the whole house, timed to its own length.

## What it draws

Blades turning round a center, filling the ring between an inner and an outer radius. Every blade carries all the palette colors side by side as stripes (it is not one color to a blade). The blades grow out from the center as the effect starts and leave outward as it ends, and the turns are counted over the whole effect, so the same settings spin slower on a longer effect.

- On a matrix: at the defaults a small three-bladed propeller in the middle, as tall as the matrix and half as wide. Raise `endRadius` to fill it.
- On the whole house (a group): one fan across the layout, centered on its middle. At the defaults it covers only the middle of the house; with `endRadius` at 100 or more the blades sweep over every prop.
- On a tree: the fan is drawn on the tree's unrolled grid, centered half way up one string, and shows as curved streaks scattered over the strings. It doesn't read as a fan. Set `renderStyle` to `perPreview` for a true fan on the tree's face, or use `spiral`.
- On a string or an arch: the prop is one line through the center, so the blades show as short dashes of the colors where they cross it, only as far out as the outer radius reaches. With curved blades the dashes travel: in toward the middle turning `forward`, out toward the ends turning `reverse`.
- A turn every three or four seconds is a sweep. Two turns a second or more is a blur.

## Settings

- `centerX` and `centerY`: where the center is, in percent of the prop. 50 is the middle, 0 the left edge (or the bottom), 100 the right edge (or the top). `centerY` at 0 stands half a fan on the bottom edge.
- `startRadius` and `endRadius`: the inner and outer edge of the blades. 100 reaches from the center to the end of the prop's longer side. So on a matrix twice as wide as it is tall, 50 already touches the top and bottom, 100 reaches the two ends, and about 115 covers the corners; from an edge, double those. A large `startRadius` leaves a hole in the middle. With the inner larger than the outer, the blades grow inward from the rim.
- `scale`: on, the radii are percent as above, so a look fits any prop. Off, they count pixels of the prop's grid, which is how imported sequences may have them.
- `blades`: how many. 2 to 4 reads as a fan, 6 to 8 as a flower or a star; more turns to speckle on a small matrix.
- `bladeWidth`: how much of its slice of the circle each blade fills. 100 leaves no dark between blades; 10 to 20 gives thin streaks.
- `bladeAngle`: how far each blade curves between its inner and outer end, in degrees. 0 is straight wedges, 90 a propeller, 360 a full spiral arm. Negative curves the other way, and swaps which `direction` draws the swirl inward.
- `revolutions`: turns over the whole effect, not per second. 2 on a four-second effect is a turn every two seconds; 2 on a one-second effect is a whirl. Work it out from the effect's length. 0 holds the blades still while they open and leave.
- `direction`: `forward` or `reverse`: which way the blades turn.
- `duration`: how much of the effect the blades spend at full length; the rest is shared between growing at the start and leaving at the end. 100 is full length throughout, for a look that should simply run. 80 opens and closes briskly. 0 grows for the first half and flies outward for the second: a burst.
- `acceleration`: bends time through the whole effect, growing and leaving included. Below 0 the fan opens and spins at once, then slows and takes a long time to leave: -3 to -5 suits a hit. Above 0 it stays small and nearly still for most of the effect and rushes at the end.
- `startAngle`: where the first blade points at the start, in degrees.
- `elements`: how many stripes each color is split into across a blade. 1 is one band of each color; 2 to 4 give finer pinstripes.
- `elementWidth`: how much of its space each stripe fills. At 100 the stripes touch; lower leaves dark lines between them.
- `blendEdges`: on, each stripe is brightest along its middle and fades to its edges: soft, and dimmer. Off, the stripes are flat and hard-edged: brighter, and better for candy colors and small props.

## When to use it

- A flourish with a beginning and an end: a fan that opens on the first beat of a phrase, turns through it, and is gone by the next. Make the effect as long as the phrase and choose `revolutions` to suit.
- A burst on a hit: `duration` 0 and half a turn, on a short effect.
- For a wheel that just keeps turning, use `pinwheel`: its speed is in degrees a second whatever the effect's length, and each arm is one color. Fan can run steadily too (`duration` 100), and is the one with striped blades, a hole in the middle, and a spin that speeds up or slows down.
- It sits well on a top layer with blend `max` or `add` over a calm base.
- Flash safety: on a wide fan, keep `blades` times the turns each second under 3.
- Avoid it on a tree's own layout and on single strings unless the travelling dashes are what you want.

## Looks

```look
{"name": "Candy propeller", "suits": ["matrix", "house"],
 "note": "Three curved blades, each half red and half white with hard edges, reaching the corners and turning twice over the effect.",
 "effect": {"kind": "fan", "settings": {"endRadius": 115, "duration": 100, "blendEdges": false}, "colors": ["#ff0000", "#ffffff"]}}
```

```look
{"name": "Two-color swirl", "suits": ["matrix", "house"],
 "note": "Soft red and green spiral arms filling the prop and spinning outward: one turn over the effect.",
 "effect": {"kind": "fan", "settings": {"blades": 2, "bladeWidth": 100, "bladeAngle": 360, "endRadius": 115, "duration": 100, "revolutions": 1, "direction": "reverse"}, "colors": ["#ff0000", "#00c040"]}}
```

```look
{"name": "Burst that opens and flies apart", "suits": ["matrix", "house"],
 "note": "Six gold and white blades shoot out from the middle, then leave through the edges: for a hit, on an effect a second or two long.",
 "effect": {"kind": "fan", "settings": {"blades": 6, "endRadius": 120, "duration": 0, "revolutions": 0.5, "acceleration": -3}, "colors": ["#ffc040", "#ffffff"]}}
```

```look
{"name": "Half fan rising from the bottom edge", "suits": ["matrix", "house"],
 "note": "Five straight gold blades standing on the middle of the bottom edge and sweeping slowly round.",
 "effect": {"kind": "fan", "settings": {"centerY": 0, "endRadius": 200, "blades": 5, "bladeAngle": 0, "revolutions": 0.5, "duration": 100}, "colors": ["#ffc040"]}}
```
