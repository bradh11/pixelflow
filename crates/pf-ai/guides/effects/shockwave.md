# Shockwave

One ring bursting out from a point, or closing in on it, widening and changing color as it goes and timed to the effect's own length: the burst to put on a hit.

## What it draws

A single round ring. It starts at one radius as the effect starts and arrives at another exactly as the effect ends, getting wider (or narrower) on the way. Its color runs through the palette over the trip: the first color at the start, the last at the end, blended between. There is only ever one ring.

- On a matrix: a small ring in the middle that grows until it leaves through the edges. It is a true circle, so on a wide matrix it has passed the top and bottom long before it reaches the two ends.
- On the whole house (a group): the ring starts on the middle of the layout and rolls outward over everything: the matrix, then the windows and arches, then the tree and the far ends of the roofline. This is the blast that reads from across the street.
- On a string or an arch: the ring crosses the row in two places, so it is two short dashes flying apart from the center to the ends.
- On a tree: the ring is drawn on the tree's unrolled grid from half way up one string. It opens as a small arc, then splits into two bands, one climbing to the top and one sinking to the foot. With the center at the foot it is a single band of light rising up the tree. Set `renderStyle` to `perPreview` for a true ring on the tree's face.
- Its speed is its length. The same settings are a snap on a half-second effect and a slow swell over four seconds. When the effect ends the ring is simply gone, at its end radius, unless a fade takes it out first.

## Settings

- `centerX` and `centerY`: where the ring is centered, in percent of the prop. 50 is the middle, 0 the left edge (or the bottom), 100 the right edge (or the top).
- `startRadius` and `endRadius`: the ring's size at the start and at the end. 100 reaches from the center to the end of the prop's longer side, so from the middle of a matrix twice as wide as tall, 50 touches the top and bottom, 100 reaches the ends, and about 115 clears the corners. From an edge or a corner, use 200 to 250. Make the start larger than the end and the ring closes in. Only whole numbers count.
- `startWidth` and `endWidth`: how thick the ring is at the start and at the end, in percent of the prop's longer side: 5 is 2 pixels on a matrix 40 wide and 3 on a string of 60. Thin growing to wide reads as a wave spreading and softening. A very large end width (100 and up) turns the ring into a disc that floods the prop from the center.
- `cycles`: how many rings the effect holds, one after another: each starts when the last arrives, and each runs through the palette again. For a ring on every beat, make one short effect per beat, or one long effect with as many cycles as it has beats.
- `acceleration`: below 0 the ring leaps out and slows as it spreads, like a real blast: -3 to -6. Above 0 it hangs small near the center for most of the effect and rushes out at the end: a wind-up. Only whole numbers count.
- `blendEdges`: on, the ring fades to nothing at both edges: soft, and a thin ring looks faint. Off, it is solid and hard-edged: brighter, and better for thin rings on strings and arches.
- `scale`: on, the sizes are percent as above, so a look fits any prop. Off, they count pixels of the prop's grid, as imported sequences may have them.

## When to use it

- A single burst on a hit: start the effect on the hit, half a second to a second and a half long, on the whole house's row or the matrix, on a layer above the running look with blend `max` or `add`. A short `fadeOutMs` lets it die away as it spreads.
- A wind-up into a hit: a ring closing in (start radius large, end 0) over the bar before, landing on the downbeat.
- A sweep from one side: center on an edge or a corner and the ring crosses the prop as an arc.
- Three effects draw spreading rings. Shockwave is one ring with a start and an end, from any point. `ripple` is an endless train of thin rings from the middle of the prop only, for a continuous look. `circles` with its ring looks fills the prop with solid bands flowing outward. For rings that keep coming, use `ripple`; Shockwave with many `cycles` sends them strictly one at a time.
- Flash safety: thick or flooding rings across the house are flashes. Keep them under 3 a second: `cycles` divided by the effect's length in seconds.
- Avoid long effects with the defaults (a thin ring creeping outward for many seconds reads as nothing happening), and remember the ring is as thick as a share of the prop: on the whole house the default is a broad band.

## Looks

```look
{"name": "Blast on a hit", "suits": ["string", "arch", "matrix", "house"],
 "note": "A white ring leaps out from the middle, slows and thickens as it spreads, and fades as it reaches the edges. Use it on an effect about a second long.",
 "effect": {"kind": "shockwave", "settings": {"endRadius": 115, "startWidth": 4, "endWidth": 14, "acceleration": -4}, "colors": ["#ffffff"], "fadeOutMs": 300}}
```

```look
{"name": "White-hot ring cooling to blue", "suits": ["matrix", "tree", "house"],
 "note": "A soft ring that starts white and turns pale blue, then deep blue, as it grows.",
 "effect": {"kind": "shockwave", "settings": {"endRadius": 115, "startWidth": 6, "endWidth": 16}, "colors": ["#ffffff", "#60b0ff", "#0030ff"]}}
```

```look
{"name": "Closing in before a hit", "suits": ["string", "arch", "matrix", "house"],
 "note": "An orange ring shrinks from the edges to a point, slowly at first and then fast: end it on the downbeat.",
 "effect": {"kind": "shockwave", "settings": {"startRadius": 115, "endRadius": 0, "startWidth": 14, "endWidth": 4, "acceleration": 3}, "colors": ["#ff6000"]}}
```

```look
{"name": "Band of light rising up a tree", "suits": ["tree"],
 "note": "A gold band climbs from the foot of the tree to the top, hard-edged.",
 "effect": {"kind": "shockwave", "settings": {"centerY": 0, "endRadius": 200, "startWidth": 12, "endWidth": 12, "blendEdges": false}, "colors": ["#ffc040"]}}
```

```look
{"name": "Four rings, one after another", "suits": ["string", "arch", "matrix", "house"],
 "note": "Four rings in a row, each starting red and ending green: on a two-bar effect, one ring every two beats.",
 "effect": {"kind": "shockwave", "settings": {"cycles": 4, "endRadius": 115, "blendEdges": false}, "colors": ["#ff0000", "#00c040"]}}
```
