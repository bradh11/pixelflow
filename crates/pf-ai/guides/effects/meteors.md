# Meteors

Streaks with bright heads and fading tails falling down a matrix or a tree, or one comet running along a string: rain, shooting stars, rising sparks.

## What it draws

A number of streaks, each one pixel wide, brightest at the head and fading to nothing along the tail, all travelling the same way at the same speed. Each one starts again in a new random lane after every trip. The tails fade by turning see-through, so a layer below shows through them.

- On a matrix: thin streaks falling down random columns. The default five are sparse on a matrix forty wide; 15 to 30 reads as rain.
- On a tree: streaks sliding down the strings, one of its best looks. With `up` they are sparks climbing.
- A string or an arch has only one lane, so every meteor runs along the same line: `down` and `left` toward its left end, `up` and `right` toward its right. With the default five they overlap and nearly the whole prop is lit, rippling. With `count` at 1 or 2 it is a clean comet with a tail.
- On the whole house (a group) the lanes are one pixel wide across the entire layout and most fall between props, so with the default count a pixel or two lights somewhere and that is all. A group of arches or of window frames shows nothing most of the time. Give the effect the `perModelDefault` render style and every prop gets its own meteors.
- Under 0.5 they drift like snow. At 1 it is rain. At 2 and up they streak.

## Settings

- `count`: how many meteors are in flight at once. 1 or 2 on a string or an arch; 15 to 30 on a matrix or a tree; 60 or more is a downpour. With a per-model render style every prop gets this many, so keep it to 3 or 4.
- `speed`: trips down the target (or across it) each second. A meteor takes a little longer than one trip, because its tail has to leave too.
- `length`: the tail's length as a share of the trip. 0.1 is a dart, 0.25 to 0.5 a comet, and at 1 the streaks are as long as the prop and read as fading stripes.
- `direction`: `down` falls and `up` rises. `left` and `right` send the streaks sideways along the rows: across a matrix, and round a tree, where they are only short scattered dashes. On a one-row prop there is no down, so `down` behaves as `left` and `up` as `right`.

Each meteor takes the next palette color in turn, so two colors give a mix of both. To send comets along the wiring of a matrix or a tree, set the effect's render style to `singleLine`: whole rows or strings then light and fade as each comet passes.

## When to use it

- Rain or snowfall on the matrix and tree through a quiet verse; a single comet along the roofline on a held note; sparks up the tree into a chorus (a `ramp` curve on `count` builds them).
- It pairs with a dark `on` under it (deep blue for a night sky) and `twinkle` on the outlines.
- Avoid `blur`: one-pixel streaks blur away to almost nothing. Avoid high counts on strings and arches, which fill solid. Avoid it on a group in the usual layout.
- Don't put a curve on `speed` or `length` that follows the music or a timing track: each meteor sits where the time so far times the speed at that moment puts it, so they jump about and change lanes. A rising `ramp` on `speed` works; a falling one stalls once it has halved, then runs backward.
- It doesn't flash; very high `speed` only flickers.

## Looks

```look
{"name": "Icy rain", "suits": ["matrix", "tree"],
 "note": "Twenty white and pale blue streaks falling steadily.",
 "effect": {"kind": "meteors", "settings": {"count": 20, "length": 0.3, "speed": 0.6}, "colors": ["#ffffff", "#80c0ff"]}}
```

```look
{"name": "A comet along an outline", "suits": ["string", "arch"],
 "note": "One white comet with a long fading tail crossing the prop every three seconds.",
 "effect": {"kind": "meteors", "settings": {"count": 1, "length": 0.4, "speed": 0.5}, "colors": ["#ffffff"]}}
```

```look
{"name": "Sparks rising", "suits": ["tree", "matrix"],
 "note": "Gold and orange sparks climbing with long tails.",
 "effect": {"kind": "meteors", "settings": {"count": 12, "length": 0.5, "speed": 0.5, "direction": "up"}, "colors": ["#ffc040", "#ff6000"]}}
```

```look
{"name": "Meteors on every prop", "suits": ["house"],
 "note": "Each prop of the group gets three streaks of its own: falling on the matrix and tree, running along the strings and arches.",
 "effect": {"kind": "meteors", "settings": {"count": 3, "length": 0.4, "speed": 0.7}, "colors": ["#ffffff", "#80c0ff"], "renderStyle": "perModelDefault"}}
```

```look
{"name": "Downpour", "suits": ["matrix", "tree"],
 "note": "Sixty short white darts falling fast: a storm for a loud passage.",
 "effect": {"kind": "meteors", "settings": {"count": 60, "length": 0.2, "speed": 1.5}, "colors": ["#ffffff"]}}
```
