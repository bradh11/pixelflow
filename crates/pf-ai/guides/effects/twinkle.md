# Twinkle

Single pixels swelling and fading, each in its own time, so a prop glitters softly and never blinks as a whole: the gentle one of the flicker family.

## What it draws

Every pixel keeps its own clock. Each turn of that clock (there are `rate` turns a second) the pixel either stays dark or twinkles once: it rises smoothly to full, falls smoothly back to nothing, and takes a palette color picked at random for that twinkle. No two pixels are in step, so while some rise others fall and the amount of light on the prop stays level.

- On a string or an arch: scattered points along the run, some bright, some dim, slowly trading places. At the defaults about a third of the pixels are in a twinkle at any moment and half of those are too dim to notice, so a 30-pixel arch shows five or so clear points.
- On a matrix: an even speckle over the whole panel, like a starry sky. It has no shape and no direction.
- On a tree: the same speckle all over the cone.
- On the whole house (a group): every pixel of every prop twinkles by itself, so the house glitters evenly from the roofline down to the arches. One effect on the group's row is all it needs.
- Slow (a `rate` under 0.5) it is starlight: each point takes two seconds or more to swell and fade. At 1 to 3 it is lively glitter. Above about 8 there are too few frames to show the rise and fall, so the points pop instead of swelling; by 20 it is television static.
- It is see-through wherever it is dark and partly see-through while a point is dim, so over another layer each twinkle melts from the look below into its own color and back.

## Settings

- `density`: the share of pixels twinkling at any moment. A prop looks about half as full as the number, because every twinkle spends half its time dim. 0.05 to 0.15 is a few lonely stars; 0.3 to 0.5 is glitter; 1 has every pixel always swelling or fading, a restless full-prop glow. A `ramp` curve on it from 0 to 1 fills a prop with twinkles through a build.
- `rate`: how many turns a second each pixel gets, so one twinkle lasts a second divided by `rate`. 0.25 to 0.5 is calm, 1 to 3 lively, 8 and up hard and fizzy. At 0 nothing changes: a still scatter of points at mixed brightness.

## When to use it

- Over a base, for a quiet section: a dim `on` or a slow `colorWash` on the bottom layer and Twinkle above it with blend `max`.
- Alone on a dark house for an intro, a bridge, or the last bars: sparse and slow.
- With several colors each twinkle picks one, so red, green, gold, and white gives old-fashioned Christmas lights.
- Excitement without a flash. Twinkle never blinks a prop as a whole at any setting, because its pixels are never in step. That makes it the safe one of the flicker family across a whole house: when a chorus wants strobe-like energy everywhere, a `rate` of 8 to 12 with a `density` of 0.5 or more crackles hard while the total light stays steady.
- It has no beat. For brightness that keeps time use `pulse`; for hard pops that all land together, `strobe`; for the whole prop blinking, `shimmer`.
- `sparkles` (topic `effect-wide`) add hard glints to another effect. Twinkle is a layer of its own, in its own colors, and its points swell and fade instead of popping.
- Keep a group on its own layout. With a per-model render style such as `perModelDefault` every member draws the same pattern, so matching arches twinkle in unison and it looks mechanical.
- Avoid a very low `density` on short props: 0.05 on a 30-pixel arch is often nothing lit at all.

## Looks

```look
{"name": "Starlight", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "A few white and pale blue points, each taking over two seconds to swell and fade.",
 "effect": {"kind": "twinkle", "settings": {"density": 0.15, "rate": 0.4}, "colors": ["#ffffff", "#cfe0ff"]}}
```

```look
{"name": "Warm glitter over a base", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Warm white glitter made to sit on a layer above a dim color: the base shows between the points.",
 "effect": {"kind": "twinkle", "settings": {"density": 0.4, "rate": 1.5}, "colors": ["#ffd9a0"], "blend": "max"}}
```

```look
{"name": "Old-fashioned Christmas lights", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Most of the pixels slowly coming and going in red, green, gold, and white.",
 "effect": {"kind": "twinkle", "settings": {"density": 0.7, "rate": 0.8}, "colors": ["#ff2020", "#00b030", "#ffc040", "#ffffff"]}}
```

```look
{"name": "Filling up with glitter", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Starts nearly dark and ends with every pixel twinkling: lay it across a build.",
 "effect": {"kind": "twinkle", "settings": {"rate": 2}, "colors": ["#ffffff"], "curves": {"density": {"shape": "ramp", "from": 0.02, "to": 1}}}}
```

```look
{"name": "Icy crackle for a chorus", "suits": ["matrix", "tree", "house"],
 "note": "Fast, dense points of white and ice blue: strobe-like energy with no whole-prop flash.",
 "effect": {"kind": "twinkle", "settings": {"density": 0.6, "rate": 10}, "colors": ["#ffffff", "#9fd0ff"]}}
```
