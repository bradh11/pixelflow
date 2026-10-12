# Snowflakes

A set number of small flakes sliding or falling down a matrix or a tree, or settling into a drift that fills it: the snow scene, and it needs a grid to fall through.

## What it draws

A few flakes on a see-through background, so whatever is on the layer below shows between them. The first palette color is the flake; the second, if a flake has arms, is its arms.

- On a matrix: this is its home. At the defaults ten single dots slide diagonally down, half drifting left and half right, crossing a 20-row matrix in about a second and coming round again. That is brisk and sparse: for a calm snowfall raise `count` and lower `speed`.
- On a tree: dots travelling down the strings and drifting round the trunk. Keep to `dot` or the small flakes: a flake five cells wide wraps a third of the way round a 16-string tree and the tree turns into a blur of color.
- On a string or an arch there is nowhere to fall. `blowing` shows dots sliding along the prop, half each way, which is pleasant. `falling` blinks a new random set of dots many times a second, a fast glitter. `pilingUp` leaves the dots sitting still.
- On the whole house (a group as one picture) it draws almost nothing: the flakes land in the empty space between props. A hundred large flakes lit a handful of pixels, and a drift never formed. Set `renderStyle` to `perModelDefault` and every prop gets its own snowfall. A window frame catches only the odd flake either way, because most fall through its empty middle.
- Slow, it is weather you can leave on for a whole verse. Fast and dense it is a blizzard, and with big flakes the prop is simply full.

## Settings

- `count`: flakes on the target at once, whatever its size (it is not scaled to the prop). `blowing` shows every flake twice. 10 to 30 dots suits a matrix or a tree; 3 to 6 for the large flakes; 4 to 10 on a string. 100 is a white-out on a matrix and a solid line on a string. A curve on `count` re-scatters `blowing` snow at every step, which jumps.
- `flake`: `dot` is one pixel and the only one that reads on a tree or a one-row prop. `square` (2 by 2), `plus` and `x` (3 by 3), and `diamond` (5 by 5, solid) are all in the first color. `cross` (3 by 3), `bar` (three in a row, tumbling as it falls), `bigCross`, and `star` (5 by 5) have a center in the first color and arms in the second; with one color in the palette the arms are white. `random` gives each flake a different shape, which looks the most like real snow on a matrix.
- `speed`: whole numbers. For `blowing`, 10 is about 20 rows down and 10 columns sideways each second; 2 to 4 is a gentle drift and 0 is frozen in place. For `falling` and `pilingUp`, 4 is about 7 rows a second, 10 about 15, and from 29 up a row every frame, which is as fast as it goes.
- `motion`: `blowing` slides the same flakes diagonally and wraps them round, so it looks the same from its first frame to its last. `falling` drops each flake straight down with a little sideways wander and starts new ones at the top. `pilingUp` does the same but flakes stop where they land, and the drift grows until the prop is one solid block of the first color.
- How long a drift takes: on a 40 by 20 matrix, 30 flakes at `speed` 10 had laid two rows after five seconds; 30 flakes at 30 half filled it in five seconds; 100 flakes at 50 filled it in under three. More flakes and more speed both build it faster.
- `warmup`: frames of falling done before the effect starts, at most 100 (a couple of seconds). Flakes are spread over the prop from the first frame anyway, so its real use is to open `pilingUp` with a drift already on the ground. It does nothing for `blowing`.

## When to use it

- Christmas: white dots over a deep blue `on` or `colorWash` on the matrix and the tree while the outlines hold a steady color. Large two-color flakes on a matrix in a quiet passage.
- An ending: `pilingUp` on the matrix or tree for the last bars, timed so the drift tops out as the song stops.
- Halloween: the same motion in orange is embers raining over `fire`; in green or purple it is falling dust.
- The whole display snowing: one effect on the house's group with `perModelDefault`, or the same effect on each prop's row.
- Avoid it on a group as one picture, on window frames, and with `pilingUp` on a long effect unless a prop full of solid color is what you want. For glints on a one-row prop `twinkle` does the job better than `falling`.

## Looks

```look
{"name": "Gentle snowfall", "suits": ["matrix", "tree"],
 "note": "White dots falling slowly; put a deep blue wash on the layer below.",
 "effect": {"kind": "snowflakes", "settings": {"count": 14, "speed": 4, "motion": "falling"}, "colors": ["#ffffff"]}}
```

```look
{"name": "Big two-tone flakes drifting", "suits": ["matrix"],
 "note": "A few large flakes of mixed shapes, white hearts with ice-blue arms, sliding down slowly.",
 "effect": {"kind": "snowflakes", "settings": {"count": 4, "flake": "random", "speed": 2}, "colors": ["#ffffff", "#80c0ff"]}}
```

```look
{"name": "Snow settling into a drift", "suits": ["matrix", "tree"],
 "note": "Snow that lands and stays; it half fills a 20-row matrix with white in five seconds.",
 "effect": {"kind": "snowflakes", "settings": {"count": 30, "speed": 30, "motion": "pilingUp"}, "colors": ["#ffffff"]}}
```

```look
{"name": "Snow on every prop at once", "suits": ["house", "string", "arch"],
 "note": "Each prop gets its own few flakes: sliding along the outlines, falling on the matrix and tree.",
 "effect": {"kind": "snowflakes", "settings": {"count": 8, "speed": 3}, "colors": ["#ffffff"], "renderStyle": "perModelDefault"}}
```

```look
{"name": "Embers raining down", "suits": ["matrix", "tree"],
 "note": "Fast orange specks falling, for a Halloween fire scene.",
 "effect": {"kind": "snowflakes", "settings": {"count": 20, "speed": 25, "motion": "falling"}, "colors": ["#ff7a00"]}}
```
