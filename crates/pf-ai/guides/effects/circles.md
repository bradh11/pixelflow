# Circles

Balls of color drifting or bouncing about a matrix (flat, shaded like ornaments, outlined as bubbles, or glowing blobs), or a bullseye of solid rings flowing out from a point.

## What it draws

Two different things, chosen by `look`.

Balls (`solid`, `fading`, `bubbles`, `plasma`): a few discs, one palette color each, that start at random places and travel in straight lines, each at its own pace.

- On a matrix: just that. At the default speed they shoot across it several times a second; turn `speed` down to 1 to 3 for a drift.
- On a tree: the balls are drawn on the tree's unrolled grid, so each is a patch of color sliding over the tree and round it. At the default size one ball covers most of a 16-string tree; go smaller.
- On a string or an arch: each ball is a dash as long as the ball is wide, sliding along the prop at its own speed, some one way and some the other, passing through each other. With `bounce` they run back and forth between the ends. A pleasant, slightly unruly runner.
- On the whole house (a group): next to nothing. The balls are a few pixels of the group's big grid and fall mostly in the gaps between props. Set `renderStyle` to `perModelDefault` and every prop gets balls of its own.

Rings (`radial`, `rainbowRadial`): solid bands of color, touching, in circles round a point and flowing outward. They open out from the point as the effect starts, then keep flowing.

- On a matrix: a bullseye. On a tree: bands moving from the middle up to the top and down to the foot. On the whole house: a bullseye across the layout, each prop one color at a time.
- The rings reach only as far from their center as the prop is tall. A matrix twice as wide as tall is just covered from its middle; a wide house leaves its far ends dark; and a string or an arch, one row tall, gets three pixels in the middle and nothing more. Don't use the rings on one-row props.
- At the default `speed` the bands race: on a matrix every pixel changes color more than ten times a second, which is a hard flicker. Use 1 to 3.
- The opening is slow on a big prop at those speeds: across a whole house the bullseye takes a few seconds to open out.

## Settings

- `look`: `solid` is flat discs. `fading` is bright in the middle and dark at the rim, like glass ornaments. `bubbles` is outlines only, all drifting the same way, toward the left; add a `bufferTransform` of `rotateCw90` to make them rise. `plasma` is soft glows, larger than the ball size, that bulge toward each other, merge, and brighten toward white where they meet: a lava lamp. `radial` is rings in the palette colors. `rainbowRadial` is rings in a rainbow, whatever the palette.
- `count`: how many balls. For `rainbowRadial`, how many times the rainbow repeats between the center and the rim: 1 to 3, as more turns to noise. `radial` ignores it.
- `size`: a ball's radius in pixels, so a ball is twice this across. 2 to 4 on a matrix or a tree, 1 to 3 on a string. For rings it works backwards: the bands are the prop's height divided by one more than `size`, so 1 to 3 gives broad bands and 10 and up gives hairlines.
- `speed`: for balls, 1 moves them 5 to 15 pixels a second (the quickest ball goes three times as fast as the slowest) and the default 10 moves them 50 to 150. For rings, the bands flow outward 5 pixels a second for each unit.
- `bounce`: on, balls bounce off the edges and stay whole inside the prop. Off, they slide out through one edge and come back in at the opposite one. It does nothing for rings.
- `centerX` and `centerY`: for rings only: the point they spread from. 0 is the middle; -50 is the left edge (or the bottom), 50 the right edge (or the top). Because the rings only reach the prop's height from their center, moving it sideways on a wide prop leaves the far side dark; moving it to the bottom edge of a wide matrix gives a rising-sun half bullseye that still covers most of it.

## When to use it

- Balls are a matrix effect: ornaments, bubbles, a lava lamp, for a verse or a playful passage. On a tree they are moving patches of color, good in two or three colors over a dim `on`.
- Rings are a full-coverage look for a matrix, a tree, or the middle of the house: slow rainbow rings for a dreamy passage, two-color rings for a bullseye.
- Three effects draw spreading rings. Circles' rings are solid bands with no dark between, flowing without end. `ripple` is thin separate rings with dark between, also endless, and it works on strings and arches. `shockwave` is a single ring that starts and ends with its effect: the one for a burst on a hit.
- Balls with blend `max` over a `colorWash` or a dim `on` keep the prop from going dark between them.
- Flash safety: the rings recolor the whole prop as they flow. With broad bands (`size` 1 or 2) keep `speed` at 3 or under on a matrix; thinner bands need it slower still. Fast balls only flicker where they pass.
- Avoid balls on a group's default layout, rings on one-row props, and the default `speed` for anything meant to be calm.

## Looks

```look
{"name": "Drifting ornaments", "suits": ["matrix", "tree"],
 "note": "Six shaded balls in red, gold, and green wandering slowly and bouncing off the edges.",
 "effect": {"kind": "circles", "settings": {"look": "fading", "count": 6, "size": 3, "speed": 1, "bounce": true}, "colors": ["#ff0000", "#ffc040", "#00c040"]}}
```

```look
{"name": "Slow bullseye", "suits": ["matrix", "tree", "house"],
 "note": "Broad red and white rings flowing out from the middle: a new band about twice a second on a matrix, slower on anything bigger.",
 "effect": {"kind": "circles", "settings": {"look": "radial", "size": 2, "speed": 2}, "colors": ["#ff0000", "#ffffff"]}}
```

```look
{"name": "Rising bubbles", "suits": ["matrix"],
 "note": "Eight white and pale blue outlines floating upward, each drifting a little to one side.",
 "effect": {"kind": "circles", "settings": {"look": "bubbles", "count": 8, "size": 3, "speed": 2}, "colors": ["#ffffff", "#40c0ff"], "bufferTransform": "rotateCw90"}}
```

```look
{"name": "Lava lamp", "suits": ["matrix", "tree"],
 "note": "Four soft blobs in orange, purple, and green that swell into each other and glow pale where they meet.",
 "effect": {"kind": "circles", "settings": {"look": "plasma", "count": 4, "size": 3, "speed": 1}, "colors": ["#ff4000", "#8000ff", "#00ff60"]}}
```

```look
{"name": "Runners on an outline", "suits": ["string", "arch"],
 "note": "Four short dashes in red, green, and white running back and forth along the prop at different speeds.",
 "effect": {"kind": "circles", "settings": {"count": 4, "size": 2, "speed": 2, "bounce": true}, "colors": ["#ff0000", "#00c040", "#ffffff"]}}
```
