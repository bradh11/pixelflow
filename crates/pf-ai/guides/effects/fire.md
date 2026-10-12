# Fire

Flames in their own yellows, oranges, and reds burning up from an edge of a matrix, a tree, or the whole house; the palette does nothing to them.

## What it draws

Flames along one edge of the target: yellow at the edge, cooling through orange to red, and see-through above the flame tops, so a layer underneath shows over them. Fire brings its own colors. Red, green, and blue palettes all drew the same orange fire, so don't set `colors`; change the color with `hueShift` or `hueAdjust` (see Settings).

There are two kinds of flame. `rolling` is smooth and soft, and it lights itself: each effect starts cold and takes about a second to grow to full height. `flicker` is grainy, ragged, and redder, redrawn from scratch every frame, and at full height from the first frame.

- On a matrix: a bed of yellow along the bottom with orange and red tongues licking above it. This is where it looks best.
- On a tree: the foot of every string yellow, reddening up the cone, the top left dark. It reads as a bonfire.
- On a string or an arch there is no height to burn up, so it shows one thin slice. `rolling` gives a dim red-orange line that wavers, and nothing at all when `height` is under about 0.5. `flicker` gives a steady row of alternate yellow and orange-red pixels with a slight shimmer. Set `location` to `left` or `right` and the fire burns along the prop from that end instead: yellow at the end, through red, to dark, reaching about as far along as `height` says. That is the way to use Fire on a one-row prop.
- On the whole house (a group) the fire climbs the layout as one picture: the arches and the foot of the tree yellow, the matrix and windows orange and red, and the roofline a dull red only when `height` is 1. A group of arches burns from each arch's feet toward its top.
- Fire has no speed. `rolling` always moves at the same calm pace; `flicker` is always restless. The only pace you set is `growthCycles`.

## Settings

- `style`: `rolling` for a natural fire (a hearth, a bonfire, a burning house). `flicker` for a harsher, busier fire, and for anything that needs `hueShift`, `growthCycles`, or `growWithMusic`: those three do nothing to `rolling` flames.
- `height`: how far the flames reach from their edge, as a share of the prop. 0.3 to 0.5 is a low hearth, 0.8 leaves the top dark, 1 reaches the far edge. With `flicker` at 1 the picture is mostly red.
- `sparks`: how much of the edge is alight, for `rolling` only. 0.15 is a few separate tongues with dark between them, 0.6 a full fire, 1 a solid wall. At 0 nothing is drawn.
- `location`: the edge the fire burns from. `bottom` rises. `top` hangs down, which lights a roofline and the top of a tree when used on the whole house. `left` and `right` blow sideways across a matrix and run along a string or an arch. Sideways on a tree goes round the trunk and reads as red noise. On a group as wide as a whole house, `flicker` from `left` or `right` never cools and paints everything flat orange: use `rolling` there.
- `hueShift`: turns `flicker` flames round the color wheel. 0.2 is lime, 0.33 green with a teal edge, 0.6 blue with a violet edge, 0.8 magenta. For `rolling` flames the effect-wide `hueAdjust` does the same job: 0.33 gave green flames, 0.6 blue.
- `growthCycles`: `flicker` flames grow taller by up to half the prop and sink back, this many times over the effect. Start from a low `height` (0.3) so there is room to grow, and count cycles against the effect's length: one a beat or one a bar.
- `growWithMusic`: `flicker` flames as tall as the music is loud, from a tenth of the prop in silence to all of it at the loudest. `height` and `growthCycles` are then ignored. Without a song the flames stay at a tenth.

## When to use it

- Halloween: the whole house burning from the ground up, a green or purple witch-fire on the matrix or tree, flames under `text` or a `picture` on a layer above.
- Christmas: a low hearth fire on a matrix, `flicker` on the arches as a steady line of ember colors, a tree glowing from its foot in a quiet verse.
- Because the flames are see-through above their tops, a dim `on` or `colorWash` on the layer below (deep red, purple, or blue) gives the fire a glow to sit in.
- One long `rolling` effect is better than several short ones: every new effect starts cold and takes a second to catch.
- A curve on `height` makes a fire that climbs or dies down over a section (`ramp`), or pulses on the "Beats" track (`timingFade`): topic `curves`.
- Avoid `rolling` flames rising from the `bottom` on one-row props, where they are a dim line or nothing: burn along the prop from an end, or use a `twinkle` in fire colors. Avoid `sparks` near 0, which is an empty prop.
- Flash safety: keep `growthCycles` under about two a second on a large prop. The `flicker` grain itself changes every frame but stays the same overall brightness.

## Looks

```look
{"name": "The whole house burning from the ground up", "suits": ["house", "matrix", "tree"],
 "note": "Soft flames that catch in the first second and then roll, yellow at the ground and red at the roofline.",
 "effect": {"kind": "fire", "settings": {"height": 1, "sparks": 0.7}}}
```

```look
{"name": "A low hearth fire", "suits": ["matrix", "tree"],
 "note": "A calm bed of flame across the bottom third, for a fireside verse in a Christmas show.",
 "effect": {"kind": "fire", "settings": {"height": 0.45, "sparks": 0.8}, "fadeOutMs": 800}}
```

```look
{"name": "Green witch-fire", "suits": ["matrix", "tree", "house"],
 "note": "Ragged green flames with a teal base; put a dim purple wash on the layer below.",
 "effect": {"kind": "fire", "settings": {"style": "flicker", "hueShift": 0.33, "height": 0.6}}}
```

```look
{"name": "Flames that leap with the music", "suits": ["matrix", "tree", "house"],
 "note": "Flickering flames whose height follows the loudness of the song.",
 "effect": {"kind": "fire", "settings": {"style": "flicker", "growWithMusic": true}}}
```

```look
{"name": "Burning along an outline", "suits": ["string", "arch"],
 "note": "Fire running in from the left end: yellow at the end, red by the middle, dark for the last third.",
 "effect": {"kind": "fire", "settings": {"style": "flicker", "location": "left", "height": 0.6}}}
```
