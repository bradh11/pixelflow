# Chase

Bands of light running along a prop's pixels, across it, or from prop to prop of a group: the first choice for strings, arches, and outlines.

## What it draws

One or more bands of solid color, evenly spaced, travelling at a steady speed and wrapping from the far end back to the start. Nothing is drawn between the bands, so a layer below shows through the gaps.

- On a string or an arch it is the classic runner: a lit stretch (a fifth of the prop by default) travelling from the first pixel to the last and starting again.
- On a matrix the usual `order` follows the wiring, row after row, so the band is a block of whole rows climbing the matrix, ragged where it starts and ends part-way along a row. With `order` set to `across` it is a clean upright bar sweeping left to right.
- On a tree, along the wiring it lights a few whole strings at a time and moves round the tree string by string. Several bands along the wiring break part-way up a string; `across` keeps them to whole strings.
- On a group the usual order runs through every pixel, one member after another in the group's order. On four arches that reads well: one band running through arch after arch as if they were one string. On the whole house it rarely does: a trip is counted in pixels, so the band crawls through a matrix or a tree and crosses a roofline or an arch in a blink. There, `across` is a wall of light sweeping the yard left to right, and `props` lights whole props in turn.
- Slow (under 0.5) it is a calm base motion. At 1 to 2 it drives. Above about 4 a band lands somewhere new every frame and reads as flicker.

## Settings

- `speed`: trips along the whole target each second. 0.2 to 0.5 for a base look, 1 to 2 for a chorus. With many bands go far slower (0.05 to 0.2): the pattern repeats after one band's spacing, so many bands at 1 are a blur. 0 holds the bands still as fixed stripes.
- `bands`: how many bands share the target, evenly spaced. They take the palette colors in turn, so one band shows only the first color, and the count should be a multiple of the number of colors or two neighbours match where the pattern wraps. 1 is a runner, 4 to 6 on a string is a stripe, a quarter of the pixel count is a marquee of dots.
- `width`: how much of each band's share is lit; the rest is gap. 0.5 is even dashes, 1 is color butted against color, 0.05 a spark. With one band it is the fraction of the prop lit.
- `direction`: `forward` runs from the first pixel toward the last (left to right with `across`, first prop to last with `props`). `reverse` runs back the other way.
- `bounce`: there and back instead of wrapping. As a band reaches the far end its front spills round to the start for a moment before it turns, so keep `width` at 0.2 or less for a clean scanner.
- `order`: what the bands run along.
  - `wiring` (the default): the pixels in the order they are wired; on a group, through each member in turn. The render style and any turn or flip of the grid don't change it; only `direction` reverses it. The one exception is the per-model render styles (`perModelDefault` and its kin), which give every member its own whole chase, all in step: how to make four arches each run a band together.
  - `across`: left to right over the target's grid, however it is wired. Clean bars on a matrix, whole strings round a tree, a sweep across a group's layout. This one does follow the render style and turns: `flipHorizontal` runs it right to left, `rotateCcw90` bottom to top (a ring climbing a tree), `rotateCw90` top to bottom. Don't turn it on a string or an arch: one row leaves nothing to cross, and the whole prop blinks.
  - `props`: on a group, whole props light one after another in the group's order. `width` becomes the share of the props lit at once, never fewer than one: 0.05 for one prop at a time, 0.5 for half the group. `bands` puts that many lit props evenly through the order, each in the next color.
  - `propsAcross`: the same, taking the props left to right by where each one's middle sits (a roofline spanning the house counts as the middle).
  - On a single prop, `props` and `propsAcross` behave as `across`.
- `timingTrack`: step on a timing track's marks instead of moving at `speed`, which is then ignored. The bands hold still and jump on each mark after the first: one prop per mark with `props` or `propsAcross`; otherwise one band's length, which is `width` divided by `bands` of the trip. At the defaults that is a fifth of the prop a mark and a trip every five marks. With "Beats" and `props` it is a prop a beat; with `bands` at 4 and `width` at 0.5 it is a marquee whose dashes step into the gaps on every beat; with "Bars" it moves once a bar. `bounce` still turns it round after a whole trip.

## When to use it

- The base motion of outlines and arches: one or two colors, slow, over a dim `on` in another color.
- A marquee round a roofline or windows; a scanner on one arch with `bounce`.
- Props answering each other: `props` on the arches' group with "Beats" walks the light along them in time.
- A build: a rising `ramp` curve on `speed`. A curve on `speed` is not a throttle: the bands sit where the time so far times the speed at that moment puts them. So a rising ramp ends near twice its `to`; a falling one stalls once it has halved and then runs backward; and one that follows the music or a timing track makes the bands jump about. For a pulse with the music, curve `brightness` or `width` instead.
- It pairs with `bars` or `spiral` on the tree and matrix moving the same way, and with `twinkle` or `on` under it.
- Avoid the usual order on a mixed group such as the whole house, and several bands along the wiring of a matrix or a tree.
- Flash safety: wide bands faster than about 3 trips a second on a short prop, or `props` that fast with a large `width`, blink big areas like a strobe.

## Looks

```look
{"name": "A white runner along an outline", "suits": ["string", "arch"],
 "note": "One short white band running end to end, once every two seconds.",
 "effect": {"kind": "chase", "settings": {"speed": 0.5, "width": 0.15}, "colors": ["#ffffff"]}}
```

```look
{"name": "Red and green marquee", "suits": ["string", "arch"],
 "note": "Six dashes, red and green in turn, creeping along the prop.",
 "effect": {"kind": "chase", "settings": {"bands": 6, "width": 0.5, "speed": 0.2}, "colors": ["#ff0000", "#00c040"]}}
```

```look
{"name": "Every prop running its own band", "suits": ["house"],
 "note": "Each member of the group runs a gold band along itself, all in step: made for a group of arches or matching strings.",
 "effect": {"kind": "chase", "settings": {"speed": 1, "width": 0.3}, "colors": ["#ffc040"], "renderStyle": "perModelDefault"}}
```

```look
{"name": "One prop per beat, left to right", "suits": ["house"],
 "note": "Whole props light white one at a time across the yard, stepping on each beat.",
 "effect": {"kind": "chase", "settings": {"order": "propsAcross", "width": 0.05, "timingTrack": "Beats"}, "colors": ["#ffffff"]}}
```

```look
{"name": "A bar sweeping across", "suits": ["matrix", "tree", "house"],
 "note": "An upright icy bar crossing left to right every two seconds; on a tree it travels round.",
 "effect": {"kind": "chase", "settings": {"order": "across", "width": 0.15, "speed": 0.5}, "colors": ["#80c0ff"]}}
```
