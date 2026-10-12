# Wave

A wavy line rolling across a matrix or winding round a tree, colored by the palette from one side to the other.

## What it draws

A sine-wave line of a set thickness, rolling sideways. The palette is spread across the target from left to right and stays put while the wave moves through it. Everything off the line is see-through.

- On a matrix it is exactly that: a clean wave rolling across, in the first color at the left blending to the last at the right.
- On a tree the wave wraps round: a ribbon that climbs one side and drops down the other, turning as it rolls.
- A string or an arch is one row tall and sits at the wave's mid-height, so only the places where the line crosses the middle light up: two short dashes for each wave, sliding along the prop. When `height` is smaller than `thickness` the line never leaves the middle, and the whole prop sits lit and still in the palette's blend. A `chase` makes better dashes.
- On the whole house (a group) one wave spans the layout and each prop lights as the line passes over it: the arches when a trough comes by, a slanted slab on the matrix, the roofline only at a crest, and only when `height` and `thickness` together reach the top edge. Many props are dark at any moment. It reads as something slow and lumpy moving through the yard, not as a wave.
- At 0.25 to 0.5 it is a gentle roll, at 1 it is lively, and above about 3 it shivers instead of rolling.

## Settings

- `cycles`: how many waves fit across the target. 1 to 2 on a matrix. On a tree use a whole number, so the ribbon joins up round the back. Many waves on a narrow prop turn into a zigzag. At 0 the line is flat and simply bobs up and down.
- `speed`: waves passing each second. 0 freezes the wave where it is.
- `height`: how far the line swings, as a share of the target's height. 1 touches the top and bottom edges; the default leaves a margin; 0 is a flat, still line.
- `thickness`: the line's thickness as a share of the height. 0.1 is a thin trace (faint on a tree), 0.2 to 0.35 is bold, and near 1 it fills most of the prop, leaving wave-shaped holes.
- `direction`: `forward` rolls to the right, `reverse` to the left. The colors stay where they are either way.

Turned a quarter with `bufferTransform` set to `rotateCw90`, the wave runs up and down instead. On a string or an arch that turn makes something else: with `cycles` at 0, a short bar (as long as `thickness`) swinging from end to end like a pendulum, `speed` swings a second, `height` how far it travels.

## When to use it

- A matrix accent: a white wave with blend `max` over a deep blue `on` reads as water.
- A ribbon round the tree while the outlines run a `chase`.
- It pairs with `on` or `colorWash` under it and `twinkle` beside it. A `music` curve on `height` makes the wave swell with the song.
- Don't curve `speed` to follow the music or a timing track: the wave sits where the time so far times the speed at that moment puts it, so it jumps about. A rising `ramp` works (it ends near twice its `to`); a falling one stalls once it has halved, then rolls backward.
- Avoid it on a group that spans the house, and on strings and arches unless dashes or the pendulum are what you want. Avoid `blur` on a thin line: it fades to almost nothing.
- Flash safety: with `cycles` near 0 a one-row prop lights whole and goes dark twice for every wave, so keep `speed` under 1.5 there.

## Looks

```look
{"name": "An icy wave across a matrix", "suits": ["matrix", "tree"],
 "note": "Two waves, blue blending to white, rolling slowly to the right.",
 "effect": {"kind": "wave", "settings": {"cycles": 2, "speed": 0.5, "thickness": 0.25}, "colors": ["#2a6bff", "#ffffff"]}}
```

```look
{"name": "A ribbon round the tree", "suits": ["tree", "matrix"],
 "note": "One broad red-to-green ribbon climbing one side and dropping down the other as it turns.",
 "effect": {"kind": "wave", "settings": {"cycles": 1, "speed": 0.4, "thickness": 0.35, "height": 0.7}, "colors": ["#ff0000", "#00c040"]}}
```

```look
{"name": "Dashes sliding along an outline", "suits": ["string", "arch"],
 "note": "Six short dashes, red at one end of the prop blending to white at the other, sliding along it.",
 "effect": {"kind": "wave", "settings": {"cycles": 3, "speed": 0.5, "height": 1, "thickness": 0.5}, "colors": ["#ff0000", "#ffffff"]}}
```

```look
{"name": "A bar swinging end to end", "suits": ["string", "arch", "matrix", "tree"],
 "note": "A short gold bar swinging from one end to the other and back every two seconds, slowing at each end.",
 "effect": {"kind": "wave", "settings": {"cycles": 0, "speed": 0.5, "height": 1, "thickness": 0.15}, "colors": ["#ffc040"], "bufferTransform": "rotateCw90"}}
```

```look
{"name": "A slow swell through the house", "suits": ["house", "matrix"],
 "note": "One thick blue wave drifting across the whole layout, lighting each prop as it passes.",
 "effect": {"kind": "wave", "settings": {"cycles": 1, "speed": 0.25, "height": 0.5, "thickness": 0.5}, "colors": ["#2a6bff", "#80c0ff"]}}
```
