# Life

Single-pixel cells that are born and die in steps by the Game of Life's rules: a crawling, clumping texture for a matrix, and a stepping random twinkle anywhere else.

## What it draws

It starts as a random scatter of single lit pixels, each a random blend of the palette's colors, on a see-through background. Then, a set number of times a second, every cell lives, dies, or is born according to how many of its eight neighbors are lit. The picture changes in steps, not smoothly, and what it becomes depends on the `rules` and on how long it has run.

- On a matrix: the real thing. Clumps that writhe, split, and die back, with dark opening up between them as it runs. With the usual rules a 40 by 20 matrix was plainly emptier after 25 steps than at the start.
- On a tree: the same cells wrapped round the cone. The shapes don't read; it looks like dots winking on and off in steps. With the usual rules a 16-string tree was down to a handful of lit pixels after 80 steps.
- On a string or an arch there is only one row, so the rules play out differently. `classic` and `b35S236` are a thin, restless scatter of dots that can dwindle to a few. `amoeba` and `coagulations` light most of the prop with a few flickering gaps. `b25678S5678` leaves it almost dark.
- On the whole house (a group) the cells are fine grain spread over the layout: a speckle on every prop that reshuffles at each step, like a twinkle with a pulse. Only the matrix shows anything you would call Life.
- Slow (`speed` 2 to 4) you can follow each step and it can sit on the beat. From about 10 up it is a seethe, and with the denser rules it turns into television static.

## Settings

- `rules`: which pattern of life.
  - `classic` churns in clumps and steadily thins out. It is the recognizable one, and the one that runs down.
  - `b35S236` stays busy: a dense, seething texture that was as full after 80 steps as at the start.
  - `amoeba` also stays full, in rounder blobs that bulge and shrink. With a little `blur` it looks organic.
  - `coagulations` grows: cells spread into solid patches with ragged edges until the prop is mostly covered. From a lowish `density` you can watch separate patches creep outward, like frost or mold.
  - `b25678S5678` explodes into dense static from almost any start, a different scatter at every step.
- `density`: how many cells start alive; 50 lights roughly a quarter of the prop. `classic` needs 30 or more: at 16 every cell died in the first step and the prop stayed dark. `coagulations` shows its growth best from 25 to 30: at 14 only a speck or two survived, and at 50 the prop is busy from the start. It is only read at the first frame, so a curve on it does nothing.
- `speed`: steps a second. On the stage 1 gave one step a second, 2 two, and 3 three, so at 120 beats a minute a `speed` of 2 steps on every beat (counted from the effect's start). 4 to 6 is lively and still readable. Above 10 the steps blur into flicker.

## When to use it

- A matrix feature for a quirky or electronic passage: `classic` in one or two colors at `speed` 2 to 4.
- Halloween: `amoeba` in greens with `blur` is bubbling slime; `coagulations` in purple is something spreading.
- Christmas: `coagulations` from a low `density` in white and ice blue is frost creeping over a window or matrix.
- A twinkle that moves in time: `b35S236` on a tree or the whole house, one step a beat.
- It restarts from a fresh random scatter with every effect. For a long section with `classic`, lay several shorter effects end to end instead of one long one that has run down.
- Because the background is see-through, it sits well over a dim `on` or `colorWash`.
- Avoid a low `density` with `classic`, and don't count on shapes showing on anything but a matrix. On one-row props `twinkle` is easier to control.
- Flash safety: with `b25678S5678`, or any rule on a full prop, most pixels change at every step. Keep `speed` to 3 or under on a large prop and save the fast settings for a small one.

## Looks

```look
{"name": "Classic Life on a matrix", "suits": ["matrix"],
 "note": "Green and white clumps that churn and thin out, four steps a second.",
 "effect": {"kind": "life", "settings": {"speed": 4}, "colors": ["#00c040", "#ffffff"]}}
```

```look
{"name": "Bubbling slime", "suits": ["matrix", "tree"],
 "note": "Soft green blobs that bulge and shrink without ever settling.",
 "effect": {"kind": "life", "settings": {"rules": "amoeba", "speed": 5}, "colors": ["#00ff40", "#a0ff00"], "blur": 1, "brightness": 1.5}}
```

```look
{"name": "Frost creeping in", "suits": ["matrix", "tree"],
 "note": "Scattered white and ice-blue specks that spread into ragged patches; about half the prop is covered after three seconds.",
 "effect": {"kind": "life", "settings": {"rules": "coagulations", "density": 30, "speed": 8}, "colors": ["#ffffff", "#80c0ff"]}}
```

```look
{"name": "Twinkle that steps on the beat", "suits": ["house", "tree", "matrix"],
 "note": "A warm white speckle over everything that reshuffles twice a second, which is every beat at 120 beats a minute.",
 "effect": {"kind": "life", "settings": {"rules": "b35S236", "speed": 2}, "colors": ["#ffc880", "#ffffff"]}}
```

```look
{"name": "Static", "suits": ["matrix", "tree", "house"],
 "note": "Dense purple and orange noise that flips at every step, three steps a second.",
 "effect": {"kind": "life", "settings": {"rules": "b25678S5678", "speed": 3}, "colors": ["#8000ff", "#ff6000"]}}
```
