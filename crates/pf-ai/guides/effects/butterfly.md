# Butterfly

Ten ready-made full-color patterns (rainbow wings, pulsing rings, a grid of dots, plasmas) that fill a matrix or a tree with moving color, in a rainbow unless you ask for the palette.

## What it draws

Every pixel lit in a pattern picked by number with `pattern`, the colors running through it as it plays. Out of the box it is a full rainbow and takes no notice of the palette: set its `colors` setting to `palette` to use yours.

- On a matrix: pattern 1, the default, is a rainbow in four curved lobes round a red X, like a butterfly's wings, the hues streaming through them. Each pattern is described under Settings; all of them are made for a matrix.
- On a tree: the wings wrap into slanted rainbow bands winding round the cone. Colorful and busy; it reads as a swirl, not as wings.
- On a string or an arch: the wing patterns (1, 4, 5) become a rainbow lying along the prop, about two full sweeps of the hues on a 60-pixel string, sliding steadily toward one end. That is a good rainbow chase for an outline. Patterns 2 and 3 are almost solid red there, and with `chunks` above 1 pattern 2 goes dark.
- On the whole house (a group): pattern 1 is one big rainbow laid across the layout and drifting, each prop a different slice of it: a classic all-colors look. Pattern 5 is close to one color over the whole house, slowly changing. Pattern 2 puts its rings on whatever sits in the middle of the layout and paints everything else red.
- Slow (`speed` 5 to 15) the colors glide. By 40 they race, and at 100 every pixel runs through the whole rainbow about three times a second, which is a flash hazard on anything large.

## Settings

- `pattern`: a whole number, and each is a different picture.
  - 1: smooth wings. 4: the same wings with hard edges where the colors wrap, sharper and busier. 5: a single broad sweep of color with a faint diagonal ray, the calmest: nearly a slow color change.
  - 2: rings round the center of the prop that swell and shrink, with everything outside them one flat color (red in a rainbow, the last color with the palette).
  - 3: a grid of dots that grow and shrink together on a flat field, like wrapping paper.
  - 6 to 9: plasmas in fixed colors that ignore both `colors` and the palette: 6 red, green, and yellow with black; 7 blue, green, and cyan; 8 a pastel rainbow; 9 white, gray, and black.
  - 10: a plasma in the palette, in hard-edged patches of the first and last colors. It needs two colors or more; with one it draws black.
  - Don't curve `pattern`: it jumps from one picture to another.
- `colors`: `rainbow` (the default) ignores the palette. `palette` blends through your palette instead; give it two or three colors, since one color makes patterns 1 to 5 a flat fill. The palette itself goes in the effect-wide `colors` list, outside `settings`.
- `speed`: for patterns 1, 4, and 5, how fast the colors stream: 10 takes about three seconds to run through them once. For 2 and 3, how fast they pulse: 10 is one swell and shrink every two seconds, 40 two a second. For the plasmas only the top of the range moves at all: use 85 to 95.
- `direction`: for patterns 1, 4, and 5. With `forward` the colors travel toward the first pixel of a string (left and down on a matrix); `reverse` sends them the other way. The other patterns ignore it.
- `chunks`: at 1 the pattern is solid. Above 1, patterns 1 to 5 are cut into that many color bands and some are left dark (see `skip`), so the layer below shows through: 2 leaves bold blobs, 3 to 6 curved stripes, 10 a fine mesh. On the plasmas it instead multiplies the bands of color, which soon turns to noise: leave it at 1 there.
- `skip`: with `chunks` above 1, every band this many apart is dark. 2 darkens every other band; a higher number darkens fewer and lets more color through. The first band is always a dark one, so the start of the palette (or the rainbow's reds and oranges) is what goes missing. It does nothing when `chunks` is 1, and nothing to the plasmas.

## When to use it

- A burst of every color for a happy, busy chorus: pattern 1 on the matrix and tree, or across the whole house.
- A rainbow running along rooflines and arches.
- In two or three palette colors it stops being a rainbow and becomes a rich moving wash: blue and white for ice, orange and purple for Halloween.
- With `chunks` at 2 or 3 over a dim `on` or `colorWash`, moving shapes of color on a calm base.
- Pattern 2 with `chunks` as a pulsing target in the middle of a matrix. Its pulse does not follow the beat, so pick `speed` to suit the tempo.
- It pairs with plain outlines: when Butterfly is on the matrix, keep the rest of the house to one or two steady colors.
- Avoid the rainbow under `text` or `faces`, in a section meant to feel calm, and in a show with a strict palette. Avoid `speed` above about 40 on large props. For a plasma in your own colors, `plasma` has more control than patterns 6 to 10.

## Looks

```look
{"name": "Rainbow wings", "suits": ["matrix", "tree"],
 "note": "The full rainbow streaming through two curved wings; round a tree it winds into slanted bands.",
 "effect": {"kind": "butterfly", "settings": {"pattern": 1, "speed": 12}}}
```

```look
{"name": "A rainbow across the whole house", "suits": ["house", "string", "arch"],
 "note": "One rainbow laid over everything and drifting, so each prop is a different color; along an outline it is a rainbow sliding to one end.",
 "effect": {"kind": "butterfly", "settings": {"speed": 8}}}
```

```look
{"name": "Frost wings", "suits": ["matrix", "tree", "house", "string", "arch"],
 "note": "The wing pattern in deep blue and white only: soft, icy, and slow.",
 "effect": {"kind": "butterfly", "settings": {"colors": "palette", "speed": 8}, "colors": ["#0040ff", "#ffffff"]}}
```

```look
{"name": "Pulsing rings", "suits": ["matrix"],
 "note": "Bright rings round the middle that swell and shrink about once a second, dark between them.",
 "effect": {"kind": "butterfly", "settings": {"pattern": 2, "chunks": 4, "skip": 2, "speed": 20}}}
```

```look
{"name": "Orange and purple shapes over a base", "suits": ["matrix", "tree", "string", "arch"],
 "note": "Bold moving lobes of orange and purple with dark between them, made to sit over a dim wash. The black is the half of the palette that the dark band hides.",
 "effect": {"kind": "butterfly", "settings": {"colors": "palette", "chunks": 2, "speed": 15}, "colors": ["#000000", "#ff6000", "#8000ff"]}}
```
