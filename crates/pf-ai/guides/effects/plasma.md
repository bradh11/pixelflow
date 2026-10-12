# Plasma

Soft blobs and ribbons of the palette's colors slowly folding into each other over the whole prop: a living background texture, with no direction and no beat.

## What it draws

Every pixel is lit, in a color blended from the palette, and the colors lie in cloudy blobs and curved ribbons that drift and merge like oil on water. It covers everything on the layers under it unless you give it a blend.

It needs at least two colors. With one color, or none, every pixel is that same flat color and nothing moves: a plain fill. This is the mistake to avoid.

- On a matrix: clouds and ribbons a quarter to a half of the matrix wide. This is where it looks richest.
- On a tree: the same clouds wrapped round the cone, reading as patches of color that swell and slide across the strings.
- On a string or an arch: a row of soft-edged stretches of each color, five or six along a 60-pixel string, slowly sliding and changing length. A gentle shifting two-tone, and a perfectly good use.
- On the whole house (a group): the clouds are stretched across the whole layout, so each prop takes a slow, smooth slice: the tree mostly one color fading into the other, the matrix a wide gradient, each arch its own shade, all changing together. It is one of the better whole-house backgrounds. With `renderStyle` set to `perModelDefault` every prop gets its own small plasma instead, busier and less of one picture.
- At the default `speed` it hardly moves: over two seconds the picture barely changed. It only flows visibly from about 80 up, and at 100 it churns.

## Settings

- `colors`: this setting (inside `settings`) picks the color scheme; the effect's own palette is the separate, effect-wide `colors` list. `palette` (the default) blends through your palette from the first color to the last, so two colors give a two-tone marble and three put the middle color in ribbons between the other two. A black in the palette becomes dark ribbons that let nothing through but give the look depth. The others ignore the palette: `redGreen` is red, green, and the yellow between them; `blueGreen` is blue, green, and cyan; `rainbow` is every hue; `white` is white fading to black, made to be a mask or tint over another layer.
- `speed`: how fast the picture flows, in whole numbers, and it is steep at the top. Up to 50 it is close to a still image. 80 to 90 is a slow, visible drift (a full change of the picture in five to ten seconds). 95 is lively. 100 churns twice a second and is too much for anything large. It moves a step each frame, so a sequence with longer frames flows slower.
- `twist`: tightens the ring-shaped wave, adding finer ripples running across the blobs. 1 is smooth; 3 to 5 adds texture on a matrix; 10 is stripy.
- `density`: how many times the colors repeat across each wave. 1 is broad clouds. 2 is marbling with clear contour bands, good on a matrix. From 3 up it is a busy pattern, and by 10 it is confetti noise on a matrix and random dots on a tree or a string. Keep it at 1 on one-row props and whole-house groups.

## When to use it

- The bed under a quiet section: two close colors (deep blue and ice white, red and deep red), `speed` 80 to 90, `brightness` 0.4 to 0.6, with outlines and accents on rows that cover it.
- A whole-house wash that is alive without having a direction, when `colorWash` is too plain and `bars` too busy.
- Halloween: purple, black, and orange gives orange ribbons edged in black on purple, like lava or a cauldron.
- A texture over another effect: the `white` scheme with blend `multiply` on the layer above darkens the look below in drifting patches, so plain `bars` or an `on` gets a moving, cloudy shade.
- It has no beat. To tie it to the music put a `timingFade` or `music` curve on `brightness` (topic `curves`) rather than raising `speed`.
- Avoid it under `text` or a `picture` without dimming it first: it is full-brightness everywhere. Avoid one-color palettes, high `density` on anything but a matrix, and `speed` near 100 across a large area.

## Looks

```look
{"name": "Icy clouds", "suits": ["matrix", "tree", "house", "string", "arch"],
 "note": "Deep blue, ice blue, and white drifting slowly into each other; a winter background.",
 "effect": {"kind": "plasma", "settings": {"speed": 85}, "colors": ["#0030c0", "#40b0ff", "#ffffff"], "brightness": 0.7}}
```

```look
{"name": "Cauldron lava", "suits": ["matrix", "tree", "house"],
 "note": "Orange ribbons edged in black winding through purple.",
 "effect": {"kind": "plasma", "settings": {"speed": 88}, "colors": ["#8000ff", "#000000", "#ff6000"]}}
```

```look
{"name": "Candy marble", "suits": ["matrix", "tree"],
 "note": "Red, white, and green swirled in tight contour bands.",
 "effect": {"kind": "plasma", "settings": {"density": 2, "speed": 90}, "colors": ["#ff0000", "#ffffff", "#00a030"]}}
```

```look
{"name": "Slow rainbow over everything", "suits": ["house", "matrix", "tree", "string", "arch"],
 "note": "Every hue in broad soft patches, drifting; each prop takes its own slice of the rainbow.",
 "effect": {"kind": "plasma", "settings": {"colors": "rainbow", "speed": 88}}}
```

```look
{"name": "Drifting shade over the layer below", "suits": ["matrix", "tree", "house"],
 "note": "White-to-black clouds that darken whatever is on the layer under them in moving patches.",
 "effect": {"kind": "plasma", "settings": {"colors": "white", "speed": 90}, "blend": "multiply"}}
```
