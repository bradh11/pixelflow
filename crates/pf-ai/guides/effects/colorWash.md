# Color Wash

The whole prop in one color that drifts through the palette as the effect plays, or a gradient that slides across it: the moving base under a section.

## What it draws

Every pixel lit, all the time, in a color that blends from the first palette color toward the last. Nothing travels unless `gradient` is set, and it hides whatever is on the layers under it.

- On a string or an arch: the whole run one color, changing together. With `gradient` set to `horizontal` the palette is spread along the run and slides toward its start, turning back on itself at each end of the palette so there is never a seam.
- On a matrix: a solid block changing color, or a smooth blend sliding to the left (`horizontal`) or downward (`vertical`).
- On a tree: a solid cone changing color. A `vertical` gradient puts soft bands of color up the tree that sink toward the foot; a `horizontal` one runs the colors round the trunk.
- On the whole house (a group): everything the same color at the same moment, which is what makes it a wash. With a gradient there is one blend across the whole layout, from the left of the yard to the right or from the arches up to the roofline, sliding as it goes.
- A `vertical` gradient on a one-row prop (a string, an arch) has no height to spread over: the prop is one flat color, half a palette ahead of what `none` would show. Use `none` or `horizontal` there.
- With one color nothing changes at all. That is a plain steady color, and `on` is the simpler way to get it.
- Slow (a pass every four seconds or longer) it is a mood that shifts without anyone noticing when. A pass a second is a clear swing from color to color. Several passes a second is a flicker.

## Settings

- `cycles`: how many passes through the palette the effect makes, whatever its length. The first pass runs from the first color to the last and the next comes back. So 1 is a one-way change that ends on the last color, and 2 goes out and comes home, which lets effects placed one after another join without a jump. Work it out from the effect's length: a pass every 2 to 8 seconds is a wash, so an 8-second effect wants 1 to 4. At 0 the color never changes, and with a gradient that is a still blend. A part number stops part way: 0.5 with two colors ends on their mix.
- `gradient`: `none` gives the whole prop one color. `horizontal` spreads the palette from left to right and `vertical` from bottom to top, and the wash then slides that blend toward the left or toward the bottom as it runs.

A curve on `cycles` bends the pace (a rising `ramp` makes the color change faster and faster). It is seldom needed.

## When to use it

- The base of a section: two or three neighbouring colors (deep blue to violet, red to gold), slow, on a group's row, with the moving effects on layers above. A `chase`, a `twinkle`, or thin `bars` with blend `max` reads well over it.
- Glitter: a wash lights every pixel, so `sparkles` on it cover the whole prop.
- A slow breath: a color and black, with an even number of `cycles`, sinks and swells without ever cutting. Because a wash hides the layers under it, the black really is dark.
- Color inside a moving shape: a wash on the lower layer and a `wipe` above it with blend `reveal` shows the shifting color only where the wipe has reached.
- Choosing among the one-color effects: `on` holds a color or a still gradient. `fade` takes one color up from nothing or down to nothing. A wash stays at full brightness and changes hue the whole time. `colorShift` holds a color, changes at a moment you choose, and holds again, so it is the one for a key change. `wipe` and `fill` have an edge that travels across the prop.
- On a group the gradient spans the whole layout. For each prop to carry its own blend, set `renderStyle` to `perModelDefault`.
- Avoid five or more colors (it turns into a rainbow, whatever you meant). Avoid fast washes between colors that contrast: red to white, or a color to black, at more than about six passes a second is a strobe, since one flash is two passes. Nothing should flash faster than about three times a second across a large part of the display.

## Looks

```look
{"name": "Slow drift from deep blue to violet", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Everything deep blue easing to violet and back over the effect, with a few white glints.",
 "effect": {"kind": "colorWash", "settings": {"cycles": 2}, "colors": ["#0a2a9c", "#6a1fd0"], "sparkles": 40}}
```

```look
{"name": "Red and green sliding along an outline", "suits": ["string", "arch", "matrix", "house"],
 "note": "A red to green blend spread along the prop, sliding toward its start and turning back on itself.",
 "effect": {"kind": "colorWash", "settings": {"cycles": 2, "gradient": "horizontal"}, "colors": ["#ff0000", "#00c040"]}}
```

```look
{"name": "Warm bands sinking down a tree", "suits": ["tree", "matrix", "house"],
 "note": "Gold, red, and deep red in soft bands up the prop, drifting downward.",
 "effect": {"kind": "colorWash", "settings": {"cycles": 2, "gradient": "vertical"}, "colors": ["#ffc040", "#ff3010", "#8a0020"]}}
```

```look
{"name": "Breathing orange", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Orange sinking to dark and swelling back, two breaths over the effect: a Halloween idle.",
 "effect": {"kind": "colorWash", "settings": {"cycles": 4}, "colors": ["#ff6a00", "#000000"]}}
```
