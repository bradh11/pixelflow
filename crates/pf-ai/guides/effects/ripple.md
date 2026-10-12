# Ripple

Thin rings spreading from the middle of the prop one after another for as long as the effect plays: water rings on a matrix or the whole house, pairs of dashes running outward on a string or an arch.

## What it draws

A ring leaves the center of the prop as the effect starts and travels outward at a steady speed. Another follows a fixed distance behind, then another, each in the next palette color, with dark between them. Each ring is brightest along its middle and half as bright at its edges.

- The center is always the middle of the prop, and the rings are stretched to the prop's shape; neither can be changed. On a matrix twice as wide as it is tall the rings are ovals twice as wide as tall, and they arrive at all four edges together.
- On the whole house (a group): wide, flat ovals spreading from the middle of the layout. They cross the matrix and the windows first, reach the roofline and the ground together, and get to the tree and the far ends last.
- On a string or an arch: each ring crosses the row in two places, so it is pairs of short dashes leaving the middle and running to both ends, pair after pair. On an arch they pour from the top down both legs. This is one of the better things to put on an arch.
- On a tree: the rings are drawn on the tree's unrolled grid from half way up one string. From the street they open as arcs in the middle of the tree and turn into bands, one climbing to the top and one sinking to the foot. For true rings on the tree's face, set `renderStyle` to `perPreview`.
- The outer parts stay dark until the first ring gets there: two seconds to the corners at the default speed. On a short effect raise `speed`, or the rings never arrive.
- Slow (0.2 to 0.4) is a pond. 1 to 2 is urgent, a pulse racing outward.

## Settings

All three are measured against the distance from the prop's center to its corner, which counts as 1. The ends of a string are nearer than that, about 0.7.

- `speed`: how fast the rings travel, in those distances per second. At 0.5 a ring takes two seconds to reach the corners.
- `spacing`: the distance from one ring to the next. 0.3 to 0.5 keeps two or three rings in view. Above about 1 there is one ring at a time with a dark pause after it. A new ring leaves every `spacing` divided by `speed` seconds, so the two together set the rhythm: at 120 beats a minute, `speed` 1 with `spacing` 0.5 is a ring on every beat.
- `thickness`: how thick each ring is. 0.03 is a hairline that breaks into dots on a coarse prop; 0.12 is a clean line; 0.3 and up is a fat band. With `thickness` equal to `spacing` the rings touch and the prop fills with bands of the palette flowing outward.

## When to use it

- A continuous look: water, a held note, a quiet verse on the matrix or the arches, or the whole house breathing outward from its middle. It has no end of its own; it runs until the effect stops.
- Three effects draw spreading rings. Ripple is the endless train of thin rings, from the middle only. `shockwave` is one ring, from any point, that starts and finishes with its effect, widening and changing color on the way: the one for a single burst on a hit. `circles` with its ring looks fills the prop with solid touching bands, but draws nothing useful on a one-row prop.
- Layered: white or pale blue rings with blend `max` over a dark blue `on` reads as water. Thin rings over a `colorWash` add movement without hiding it.
- A `ramp` curve on `speed` works. Keep `music` and timing shapes off `speed` and `spacing`: the rings are placed by speed times the time played, so they jump. Put those on `brightness` or `thickness`.
- Flash safety: at each pixel, `speed` divided by `spacing` rings pass every second. Keep that under 3 when the rings are thick.
- Avoid it where the center matters (a ring from a door or a star): it can't be moved. Use `shockwave` there.

## Looks

```look
{"name": "Water rings", "suits": ["string", "arch", "matrix", "house"],
 "note": "Pale blue and white rings leaving the middle one after another, unhurried.",
 "effect": {"kind": "ripple", "settings": {"speed": 0.4, "spacing": 0.4, "thickness": 0.12}, "colors": ["#40a0ff", "#ffffff"]}}
```

```look
{"name": "A ring on every beat", "suits": ["string", "arch", "matrix", "house"],
 "note": "White rings racing outward, a new one every half second: on the beat at 120 beats a minute.",
 "effect": {"kind": "ripple", "settings": {"speed": 1, "spacing": 0.5, "thickness": 0.15}, "colors": ["#ffffff"]}}
```

```look
{"name": "Bands flowing outward", "suits": ["matrix", "tree", "house"],
 "note": "Red and white bands with no dark between them, filling out slowly from the middle.",
 "effect": {"kind": "ripple", "settings": {"speed": 0.25, "spacing": 0.25, "thickness": 0.25}, "colors": ["#ff0000", "#ffffff"]}}
```

```look
{"name": "Rings on the face of a tree", "suits": ["tree"],
 "note": "Blue and white rings spreading across the tree as seen from the street.",
 "effect": {"kind": "ripple", "settings": {"speed": 0.4, "spacing": 0.35, "thickness": 0.15}, "colors": ["#40a0ff", "#ffffff"], "renderStyle": "perPreview"}}
```
