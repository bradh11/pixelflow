# Spiral

Stripes of the palette winding up a tree or slanting across a matrix, all turning round it together: the candy cane and the barber pole, and the first thing to reach for on a mega tree.

## What it draws

Stripes, each in the next palette color, that lean sideways as they climb, with the whole pattern turning round the prop at a steady speed.

- On a tree it is at its best. A tree's "across" goes round the trunk, so the leaning stripes wind up it like the stripe on a candy cane and the pattern turns like a barber pole. Turning `forward` with a positive `twist` the stripes seem to climb; `reverse` makes them sink.
- On a matrix: slanted stripes sliding sideways, leaving one edge and coming back in at the other. A matrix has far more columns than a tree has strings, so thin stripes with a lot of twist break up into short dashes there.
- On a string or an arch there is no height to wind up, so it is dashes of each color sliding along the prop, end to end once for every turn. `twist` and `lean` do nothing there, and `gradient` paints every dash the first color. It is still worth using: it reads like `bars` laid along the prop, and it keeps exact step with a spiral on the tree.
- On the whole house (a group) the stripes become a few broad slanted bands sweeping across the yard. Each prop shows the slice passing over it: slanted bands on the tree and the matrix, long dashes sliding along the roofline, each arch and window changing color as a band crosses it.
- Slow (0.1 to 0.3) it is a calm turning pole that can run for a whole section. 0.5 to 1 drives a chorus. Above 2 the stripes blur into flicker. At 0 they stand still.

## Settings

- `count`: how many stripes go round the prop. Colors are dealt out stripe by stripe, so make it a multiple of the number of colors (2 or 4 for two colors, 3 or 6 for three), or two neighbours come out the same color. On a tree, choose a count that divides its strings evenly (2, 4, or 8 on 16 strings); otherwise the stripes sit closer together and leave a wider gap after the last. More stripes than the tree has strings collapse into one thin line.
- `thickness`: how much of each stripe's space is lit, plus one string (or column). 1 puts color against color with no dark. On a matrix 0.5 is about half and half. On a tree the extra string counts: with 4 stripes on 16 strings, 0.5 lights 3 strings in every 4 and 0.25 lights 2. At 1 each stripe laps one string over the next, so on a tree they come out a string unequal; 0.9 keeps them equal. 0 to 0.15 is a thin line: a ribbon or a garland.
- `twist`: how many times the stripes wind round between the foot and the top. 1 is one full wind; 2 or 3 is tighter, with more stripes crossing the face of the tree; 0 is straight up-and-down stripes turning round it; negative winds the other way. 1 to 2 on a tree, 0.5 to 1 on a matrix.
- `lean`: more lean, counted in strings or columns and added to the twist: 16 on a 16-string tree is one more wind, and the same number winds less on a wider prop. Leave it at 0 and use `twist`; it is there for imported sequences.
- `speed`: turns per second.
- `direction`: `forward` turns the pattern toward the right (round a tree, toward its higher-numbered strings), which makes stripes with a positive twist climb; `reverse` turns it the other way.
- `shaded`: each stripe fades across its width from full to nothing, which rounds it like a ribbon catching the light. It wants wide stripes (`thickness` 0.7 to 1) and dims the prop overall.
- `gradient`: every stripe takes the palette blended down the prop, first color at the top and last at the foot, in place of a color for each stripe. Keep `thickness` well under 1, or nothing is left to tell the stripes apart.
- `grow` and `shrink`: the stripes change width through each turn. `grow` widens them from their `thickness` until they fill the gaps, then snaps back. `shrink` starts full and narrows. Both together widen for half a turn and narrow for the other half, which reads as breathing. Start from a low `thickness` (0.1 to 0.3). The `speed` sets the pulse, and at 0 nothing changes.

## When to use it

- The mega tree's base look for a chorus or a whole upbeat song: two or three colors, `twist` 1 to 2, `speed` 0.2 to 0.5.
- With the outline: `bars` running along the roofline in the same colors at a matching speed makes the house move with the tree, or put Spiral itself on the strings and arches.
- Layered: a thin white spiral with blend `max` over an `on` or a `colorWash` in green is a garland of light on a green tree. `twinkle` on a layer above adds life.
- A build: a `ramp` curve on `speed`. Keep to `ramp`: the stripes are placed by the speed times the time played, so a speed that jumps about (`music`, a timing shape) throws them back and forth. Put those on `brightness`.
- On a group, one spiral is one picture across all of it. To give each tree or arch a spiral of its own, see topic `layouts`.
- Flash safety: each pixel changes `count` times `speed` times a second. Keep that under 3 for wide stripes on a tree, a matrix, or the house.
- Avoid stripes on every prop at once: a striped tree reads best against something calmer.

## Looks

```look
{"name": "Candy cane tree", "suits": ["tree", "matrix"],
 "note": "Red and white stripes, color against color, winding twice round and climbing slowly.",
 "effect": {"kind": "spiral", "settings": {"count": 4, "thickness": 0.9, "twist": 2, "speed": 0.25}, "colors": ["#ff0000", "#ffffff"]}}
```

```look
{"name": "Slow two-color barber pole", "suits": ["tree", "matrix", "house"],
 "note": "Two fat stripes, red and green, one lazy wind, turning once every ten seconds.",
 "effect": {"kind": "spiral", "settings": {"count": 2, "thickness": 0.9, "twist": 1, "speed": 0.1}, "colors": ["#ff0000", "#00a030"]}}
```

```look
{"name": "Garland of white light", "suits": ["tree"],
 "note": "Two thin white lines winding up the tree; made to sit over a green or red base.",
 "effect": {"kind": "spiral", "settings": {"count": 2, "thickness": 0.1, "twist": 2, "speed": 0.3}, "colors": ["#ffffff"], "blend": "max"}}
```

```look
{"name": "Frosted twist", "suits": ["tree", "matrix"],
 "note": "Three stripes with dark between them, white at the top melting to deep blue at the foot.",
 "effect": {"kind": "spiral", "settings": {"count": 3, "thickness": 0.4, "twist": 1.5, "speed": 0.3, "gradient": true}, "colors": ["#ffffff", "#0040ff"]}}
```

```look
{"name": "Three-color dashes along an outline", "suits": ["string", "arch"],
 "note": "Red, white, and green dashes sliding along the prop with dark between, in step with a spiral on the tree.",
 "effect": {"kind": "spiral", "settings": {"count": 3, "thickness": 0.5, "speed": 0.3}, "colors": ["#ff0000", "#ffffff", "#00a030"]}}
```
