# Tendril

A springy ribbon that chases a moving point round the prop, curling and whipping behind it: an organic streamer for a matrix, or a run along an outline.

## What it draws

A point moves on a path you choose. A chain of joints follows it, each lagging the one before, drawn as one smooth ribbon. The ribbon's color goes from the first palette color at the start of the effect to the last at its end; with one color it stays that color. Everything else is dark, so a layer below shows through.

- On a matrix it is at its best. At the defaults the point circles the middle and the ribbon winds into a fat spiral curl that spins more than three times a second and never leaves the center.
- On a tree the same curl is a band swirling round the middle third of the cone. Wandering and zig zag paths cover more of it.
- On a string or an arch there is no height, so a path only counts for where it goes along the prop. The default circle shrinks to a dot in the middle: useless. Paths that travel (`verticalZigZag`, `musicLine`, `square`, `random`, or `manual` with a curve) give a lit run that slides along the prop with a long tail.
- On the whole house (a group) the default curl sits in the middle of the layout and lights only what is there; the rest stays dark. `square` runs a streamer round the outside of the display: along the ground props, up one side, back across the roofline. `verticalZigZag` snakes slowly across the yard from the left. Or set `renderStyle` to `perModelDefault` and every prop gets its own curl, in step.
- At full `speed` it is restless. Slower, it is a ribbon easing about, and the tail takes a second or two to pay out.

## Settings

The path:

- `movement`:
  - `circle`: round the middle, on a circle half the prop's shorter side in radius. `movementSize` is how fast: one turn takes about 3 seconds divided by it, so 10 is a blur and 2 to 4 is a ribbon you can follow.
  - `random`: each step the point jumps somewhere new near the ribbon's end. `movementSize` is how far it may jump: 3 wanders in one area, 10 thrashes across the prop, 20 streaks from edge to edge.
  - `square`: round the edge of the prop: bottom, right side, top, left side. Here `movementSize` works backwards: a step is a side divided by it, so 20 is the calmest (a side in half a second at full `speed`) and 3 leaps a third of a side at a time.
  - `horizontalZigZag`: climbs a row a step, swinging side to side, and comes back down as a mirror image. `horizontalZigZagReturn`: the same climb, then straight down the middle.
  - `verticalZigZag`: crosses a column a step, swinging up and down, and comes back mirrored. `verticalZigZagReturn`: the same crossing, then straight back along the middle. On a string these are a steady run to the far end and back.
  - For all four zig zags `movementSize` is the length of a swing: one full swing takes three times it in rows or columns, so 3 gives tight zig zags and 10 one lazy bend on a small matrix.
  - `musicLine`: back and forth across, `movementSize` cells a step, at a height that follows how loud the music is: a line that jumps on the hits. On a string, a streamer running end to end.
  - `musicCircle`: the circle again, its radius following the loudness, so the curl swells on loud moments and collapses in quiet ones. Both music paths need a song; without one they stay small and low.
  - `manual`: the point is held at `manualX`, `manualY` (percent from the left and from the bottom), which alone is a dot. Its use is with curves: a `ramp` on `manualX` from 0 to 100 draws a streamer once along a roofline; a `sine` sweeps it back and forth; curves on both draw any path.
- `offsetX`, `offsetY`: shift the whole path left or right, down or up, as a percent of the prop (a circle moves half as far).
- `speed`: 9 and 10 move the point every frame. Each step lower waits a frame more (5 moves every fifth frame, 1 every ninth), which slows the tail's swing as well.

The ribbon:

- `thickness`: its width in cells. 1 is a hairline, 2 or 3 suits a matrix, 8 to 12 a whole house.
- `tendrils`: how many ribbons follow the point. They stay close together, so more makes one fuller, slightly frayed ribbon rather than separate arms. 1 to 5.
- `length`: joints in the chain. 20 is a short dart, 60 a long tail, 100 a tail that can cover the whole path.

The spring. These change how the tail swings, not where the point goes:

- `friction`: the strongest of the three, and it works the opposite way to its name. Low, the ribbon barely leaves the path's center and is a small squiggle. High, it overshoots and swings wide: at 20 the loops fly past the edges and fill a matrix.
- `dampening`: how much of each joint's swing is handed down the chain. Near 0 the curl is a little tighter; at 20 the tail whips out into wide arcs that reach the edges.
- `tension`: subtle. At 39 the tail keeps to the point's own path, so a circle is a clean ring; lower values differ little from the default. Leave it alone unless you want that ring.

## When to use it

- A living, hand-drawn line on a matrix or tree in a verse or a spooky passage: two or three colors, slowed down.
- Ghostly wisps for Halloween: `random`, thin, purple to green, over a dim `colorWash`.
- A streamer round the whole display with `square`, as a section opens.
- A single stroke along a roofline timed to a rise: `manual` with a `ramp`.
- It pairs with `twinkle` or `on` under it; a `blur` of 1 or 2 softens the ribbon's edges.
- Avoid the default on a string, an arch, or a group. Avoid it for anything that must land on a beat: the tail always lags, so it flows around the music rather than hitting it. For a crisp run along an outline use `chase`; for rings from a point, `shockwave`.

## Looks

```look
{"name": "A gold ribbon circling a matrix", "suits": ["matrix", "tree"],
 "note": "A three-strand ribbon winding round the middle about once a second, turning from gold to red over the effect.",
 "effect": {"kind": "tendril", "settings": {"movement": "circle", "movementSize": 3, "thickness": 2, "tendrils": 3}, "colors": ["#ffc040", "#ff2020"]}}
```

```look
{"name": "A wisp wandering at random", "suits": ["matrix", "tree", "string", "arch"],
 "note": "A thin ribbon darting about and doubling back, purple turning to green; on an outline, a lit run that slides to and fro.",
 "effect": {"kind": "tendril", "settings": {"movement": "random", "movementSize": 6, "thickness": 2, "tendrils": 2, "speed": 7}, "colors": ["#a040ff", "#40ff40"]}}
```

```look
{"name": "A streamer drawn once along an outline", "suits": ["string", "arch", "matrix"],
 "note": "A white line starts at the left end and is pulled to the right end over the effect, turning icy blue.",
 "effect": {"kind": "tendril", "settings": {"movement": "manual", "manualY": 50, "thickness": 2}, "colors": ["#ffffff", "#40a0ff"], "curves": {"manualX": {"shape": "ramp", "from": 0, "to": 100}}}}
```

```look
{"name": "A streamer round the edge of the house", "suits": ["house"],
 "note": "A thick red-to-gold ribbon running round the outside of the display: the ground props, up one side, and back along the roofline.",
 "effect": {"kind": "tendril", "settings": {"movement": "square", "movementSize": 20, "thickness": 10, "length": 100}, "colors": ["#ff2020", "#ffc040"]}}
```

```look
{"name": "A line riding the music", "suits": ["matrix", "string", "arch"],
 "note": "A ribbon sweeping across and back whose height jumps with the loudness; on an outline, a streamer running end to end.",
 "effect": {"kind": "tendril", "settings": {"movement": "musicLine", "movementSize": 2, "thickness": 2, "length": 40}, "colors": ["#ff2020", "#ffc040"]}}
```
