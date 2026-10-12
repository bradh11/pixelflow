# Garlands

Lines that slide in one after another and stack until the prop is full of a palette gradient: a fill that arrives as garlands, or with no motion a tree hung with them.

## What it draws

One thin line, a garland, for every row of the prop, each a step of a blend through the palette. They start spread out with dark between them, travel toward one edge, and stack there solid, one on the last. The stack grows steadily: half the prop is full half way through the effect, and at the end the whole prop is a solid block of color, the last palette color at the edge where the stack began and the first color at the far edge. The dark between garlands is see-through.

- On a matrix: level lines drifting down and a solid block rising from the bottom to meet them. With a swag `shape` the lines in flight are zigzags.
- On a tree: rings round the tree dropping and packing from the foot, so the tree fills from the bottom up in a gradient. Swags read as draped tinsel while they are in flight.
- On a string or an arch, mind the `direction`. The prop is one row, so with `up` or `down` there is only one garland and the whole prop is simply lit in the first color from start to finish. With `left` or `right` each pixel is its own garland: single dots slide along the prop and pack against one end, filling it end to end with the gradient. That is the version to use.
- On the whole house (a group): lines cross the layout and stack from the ground, so the arches and the foot of the tree fill first, then the windows and the matrix, and the roofline last. Sideways, the house fills from one side of the yard to the other. Swags are too fine to see at that size.
- Its pace is set by the effect's length, not by a speed: over two beats it is a quick wipe, over eight bars a slow gathering.

## Settings

- `shape`: what a garland looks like in flight. `straight` is a clean line. `smallSwags`, `swags`, and `deepSwags` are zigzags that droop 2, 4, and 6 rows, and `doubleDips` is a tight dotted zigzag. The swags repeat every five or six columns whatever the prop's width, so they are small: about three to a 16-string tree. Once a garland lands it lies flat and solid, so the shape only shows above the stack. At the default `spacing` swags mesh into a lattice of dots; raise `spacing` to 25 to 40 to see separate garlands.
- `spacing`: how far apart the garlands are in flight, as a share of the prop. 10 is every other row on a 20-row matrix, a dense stripe. 30 to 40 leaves a few distinct garlands in the air, moving faster because they have further to come in the same time. At 100 they shoot in one at a time.
- `cycles`: how many times the prop fills over the effect. At 1 it is full exactly as the effect ends. At 0.5 it ends half full. At 0 nothing moves: the garlands hang where they started for the whole effect, which is how to get a still, decorated tree. Above 1 the prop fills, snaps back to its thin starting lines, and fills again, and each snap is a flash.
- `direction`: which way the stack grows. `up` builds from the bottom edge (the garlands themselves travel down to it); `down` hangs from the top and grows downward; `right` starts at the left edge and `left` at the right edge. `upThenDown`, `downThenUp`, `leftThenRight`, and `rightThenLeft` stack and then unstack within each cycle, so the prop is full in the middle of the effect and empty again at its end.
- Colors: the first palette color belongs to the last garland to land, so with `up` it ends at the top. White then green gives a green tree with a white top. One color is a plain fill.

## When to use it

- A build: on the tree, the matrix, or the whole house over the bar before a chorus, so everything is full on the downbeat. Hold the result afterward with an `on` whose `on.gradient` runs bottom to top, with the same colors in the opposite order.
- A decorated tree: gold or white swags with `cycles` at 0 over a green `on` on the layer below.
- An outline filling end to end with `right` or `left`, or filling and emptying with `leftThenRight`.
- A rise and fall over a phrase: `upThenDown` on the whole house.
- It pairs with `fill` and `wipe`, which do the same job without the garlands, and with `twinkle` over the finished stack.
- Avoid `up` and `down` on one-row props (an `on` does that), and `straight` with `cycles` at 0 there, which draws nothing. Don't expect swags to show on a whole-house group.
- Flash safety: every extra cycle ends with a full prop snapping to nearly empty. Keep that under about two a second, and slower on the whole house.

## Looks

```look
{"name": "A tree filling from the foot", "suits": ["tree", "matrix", "house"],
 "note": "Lines dropping and stacking from the bottom until everything is lit, green at the foot shading to white at the top.",
 "effect": {"kind": "garlands", "settings": {"spacing": 15}, "colors": ["#ffffff", "#00a030"]}}
```

```look
{"name": "Gold garlands hung on a tree", "suits": ["tree", "matrix"],
 "note": "Still rows of gold zigzag swags with dark between them; put a dim green wash on the layer below.",
 "effect": {"kind": "garlands", "settings": {"shape": "swags", "spacing": 40, "cycles": 0}, "colors": ["#ffc040"]}}
```

```look
{"name": "An outline filling end to end", "suits": ["string", "arch"],
 "note": "Dots sliding in and packing from the left end until the prop is full, green at the left shading to red at the right.",
 "effect": {"kind": "garlands", "settings": {"direction": "right", "spacing": 10}, "colors": ["#ff0000", "#00c040"]}}
```

```look
{"name": "Rising and draining", "suits": ["tree", "matrix", "house"],
 "note": "Purple at the ground building up to orange, full half way through, then draining back down.",
 "effect": {"kind": "garlands", "settings": {"direction": "upThenDown", "spacing": 20}, "colors": ["#ff6000", "#8000ff"]}}
```

```look
{"name": "A net of warm lights", "suits": ["matrix", "tree"],
 "note": "A still lattice of warm white dots over the whole prop, like net lights.",
 "effect": {"kind": "garlands", "settings": {"shape": "doubleDips", "spacing": 20, "cycles": 0}, "colors": ["#ffc880"]}}
```
