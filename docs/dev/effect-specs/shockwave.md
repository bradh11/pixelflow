# Shockwave: behaviour specification

This document describes what the Shockwave effect shows, so that it can be built afresh from this
description alone. It says what a viewer can see and what a test can measure on a rendered frame.
It does not say how to compute any of it: every method is the implementer's choice.

This is the description `crates/pf-render/src/shockwave.rs` was built from, kept as it was
written. "Today's Shockwave" below means the one it replaced. What building and checking
showed is in the last section. The reference table and the percent-width cases are kept whole
beside the effect's tests (`crates/pf-render/tests/shockwave/`), with fixed samples of the
other three companion files.

**The picture to reproduce is the one drawn today.** Every statement below was measured on
frames drawn by today's Shockwave through the renderer's public interface, and, where a real
sequence allowed it, checked against the frames xLights itself rendered (section 11). Where
today's picture has a habit that can be seen (a lit pixel a cell away, a brightness more than
about 2% different, something appearing a frame sooner or later), the habit is part of the
description. The ones that look like plain defects are gathered in section 8; they are **kept**
too, unless the owner says otherwise. Only what cannot be seen at that scale is free (section 9).

Sequences imported from xLights carry their Shockwave numbers across unchanged, so the meaning of
each number (how many cells, which frame) is the contract that keeps an imported show looking the
same. Sections 2 and 3 state that contract.

Five numbers-only companions sit beside this file, in a form a program can read (appendix C says
what is in each): `acceleration.tsv`, `reference.tsv`, `edges.tsv`, `restarts.tsv` and
`widths.tsv`. Every number in them was measured the same way.

---

## 0. Conventions

**Buffer.** The effect is asked to draw on a buffer described as `C` columns by `R` rows (a count
given as 0 counts as 1). It is then asked, pixel by pixel, for a colour. Each pixel has a position
`(u, v)`: `u` runs from 0 at the left to 1 at the right, `v` from 0 at the bottom to 1 at the top,
as seen from the front of the display. Pixels need not sit on a regular grid, there may be far
fewer of them than `C × R`, and several may share a position.

**Grid and cells.** The picture lives on a grid of `C` by `R` cells. Cell `(x, y)` is column `x`
counting from 0 at the left and row `y` counting from 0 at the bottom, so the last cell is
`(C − 1, R − 1)`. The unit of length is one cell: one column step or one row step, which count as
the same length. (Section 5.6: a buffer of more than 1,048,576 cells gets a coarser grid.)

**A pixel's cell.** Every pixel shows the colour of one cell, the nearest: column `u × (C − 1)`
and row `v × (R − 1)`, each rounded to the nearest whole number, a half rounding up. Nothing is
worked out between cells. All the measurements below are therefore about cells.

**Distance.** The distance between two cells is the straight-line distance between their whole
numbered places: cells 3 across and 4 up from each other are 5 apart. Distances are therefore
always the square root of a whole number.

**Words used below.**

- *Centre cell*: the cell the ring is drawn about. It is always a whole cell (section 2).
- *Ring*: the lit band. Its *radius* is the distance from the centre cell to the middle of the
  band; its *width* is the distance across the band; its *inner edge* is at the radius less half
  the width and its *outer edge* at the radius plus half the width. A ring whose inner edge
  would be at or inside the centre is a *disc*.
- *Progress*: how far a ring is through its travel, from 0 (start radius, start width, first
  palette colour) to 1 (end radius, end width, last palette colour).
- *Frame*: a stretch of time one frame time long, counted from the effect's start. Frame 0 is the
  first.
- *Alpha*: how much of a pixel the effect covers, 0 to 1. 0 is unlit: layers beneath show
  through untouched. Colour and alpha are separate; the colour is not darkened by alpha.
- *Lit*: alpha above 0. *At half strength*: alpha 0.5 or more.
- *Hard* and *soft*: with `blendEdges` off and on.
- *In cells* and *in percent*: with `scale` off and on.

**Numbers.** Colour and alpha come back as single-precision numbers. Alpha is always a whole
number of 255ths. A palette colour's channels are its 8-bit values divided by 255.

---

## 1. What a viewer sees

### At default settings

A soft ring bursts out from the middle of the prop. On the first frame it is a small soft dot at
the centre. It grows steadily and reaches the ends of the prop's longer side exactly on the
effect's last frame, thickening as it goes from a twentieth to a tenth of the longer side. It is
brightest along the middle of its band and fades to nothing at both edges. Its colour runs
through the palette from the first colour on the first frame to the last colour on the last (a
new effect's palette is plain white). Everything that is not on the ring is unlit, inside it as
well as outside, so whatever is on the layers beneath shows through.

The ring is a true circle on the grid of cells whatever the prop's proportions; on a wide, low
prop most of it is cut off above and below.

### As the settings move

- **Start and end radius**: where the ring begins and where it ends. A start larger than the end
  makes a ring that closes in. Equal values hold it still.
- **Start and end width**: how thick the band is at the start and at the end. A width as large
  as twice the radius fills the middle: a disc.
- **Rings**: that many rings over the effect, one after another, each starting again from the
  start sizes and the first colour.
- **Acceleration**: above 0 the ring starts slowly and finishes fast; below 0 it starts fast and
  slows to a stop. The colour and the width go at the same uneven pace.
- **Soft edges off**: the band is the same full strength all the way across, with a crisp edge,
  and the cells exactly on its edges are lit.
- **Sizes in percent of the prop off**: radii and widths count cells of the grid instead of
  percent of the prop, so the same numbers make a ring of the same size in cells on every prop.
  Imports from older xLights files come this way.
- **Centre across / up**: the cell the ring is drawn about, from the left (bottom) edge at 0 to
  just past the right (top) edge at 100.

---

## 2. Settings

Values reach the effect already pulled into their ranges (a value that is not a number has been
replaced by the default; `cycles` has been rounded to a whole number). Keys are the names in the
saved file.

| Key | Type | Range | Default | Label |
|---|---|---|---|---|
| `centerX` | number | 0 to 100, percent | 50 | Center, across |
| `centerY` | number | 0 to 100, percent | 50 | Center, up |
| `startRadius` | number | 0 to 750 | 1 | Start radius |
| `endRadius` | number | 0 to 750 | 100 | End radius |
| `startWidth` | number | 0 to 255 | 5 | Start width |
| `endWidth` | number | 0 to 255 | 10 | End width |
| `cycles` | whole number | 1 to 100 | 1 | Rings |
| `acceleration` | number | −10 to 10 | 0 | Acceleration |
| `blendEdges` | on / off | | on | Soft edges |
| `scale` | on / off | | on | Sizes in percent of the prop |

An xLights import fills each one with the number from the xLights file, unchanged. A setting the
file leaves out takes the value xLights' own panel starts with, which differs from the default
above in one place: end radius 10. A file saved by an xLights older than 2025.04 is imported with
`scale` off, whatever the file says.

**Every number setting but `cycles` is read as a whole number: its fraction is dropped** (towards
zero, so −2.9 is −2 and 3.9 is 3). 50.9 is the same picture as 50, bit for bit. This applies to
the six sizes and positions and to acceleration, in cells and in percent alike, and it is the
setting's own number whose fraction goes: a width of 2.97 percent is a width of 2 percent.

### `centerX`, `centerY`: the centre cell

The centre is always one whole cell. Its column is `centerX` percent (fraction dropped) of the
**number of columns**, rounded down; its row is `centerY` percent of the number of rows, rounded
down, counting from the bottom. Measured, as the centre cell's column on rows of several lengths:

| Columns | 0 | 1 | 3 | 10 | 25 | 33 | 49 | 50 | 51 | 75 | 90 | 99 | 100 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 |
| 2 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 1 | 1 | 1 | 1 | 2 |
| 7 | 0 | 0 | 0 | 0 | 1 | 2 | 3 | 3 | 3 | 5 | 6 | 6 | 7 |
| 40 | 0 | 0 | 1 | 4 | 10 | 13 | 19 | 20 | 20 | 30 | 36 | 39 | 40 |
| 41 | 0 | 0 | 1 | 4 | 10 | 13 | 20 | 20 | 20 | 30 | 36 | 40 | 41 |
| 100 | 0 | 1 | 3 | 10 | 25 | 33 | 49 | 50 | 51 | 75 | 90 | 99 | 100 |
| 101 | 0 | 1 | 3 | 10 | 25 | 33 | 49 | 50 | 51 | 75 | 90 | 99 | 101 |
| 200 | 0 | 2 | 6 | 20 | 50 | 66 | 98 | 100 | 102 | 150 | 180 | 198 | 200 |

What follows from it, all of it visible:

- **At 50 on an odd number of cells the centre is the middle cell** (cell 50 of 0 to 100). **On
  an even number it is the cell just right of (above) the middle** (cell 50 of 0 to 99, whose
  true middle is 49.5), so the ring sits half a cell right and up.
- **At 0 the centre is the first cell. At 100 it is one cell beyond the last** (column 101 of a
  buffer whose last column is 100), off the buffer. The centre is on the last cell at 99 only on
  buffers of 100 cells or fewer.
- The centre moves in jumps: of 2 cells for each percent on 200 cells, of 4 on 400; on fewer
  than 100 cells several settings share a cell. A curve on a centre setting moves the ring in
  those jumps.
- On a buffer one column wide the centre's column is 0 for every `centerX` below 100, and 1 (one
  cell beside the column) at exactly 100. Likewise one row high and `centerY`.

### `startRadius`, `endRadius`, `startWidth`, `endWidth`: sizes

The ring's radius goes from the start radius at progress 0 to the end radius at progress 1 in a
straight line with progress, and its width from the start width to the end width the same way.
What the numbers measure depends on `scale`.

**In cells (`scale` off).** A radius of `N` is `N` cells. A width of `N` is `N` cells from inner
edge to outer edge. Anchor points, measured with hard edges on a 101 by 101 buffer about its
centre cell:

| Radius | Width | Lit cells are exactly those whose distance from the centre cell is | How many |
|---|---|---|---|
| 30 | 4 | 28 to 32, both included | 760 |
| 10 | 4 | 8 to 12 | 248 |
| 30 | 10 | 25 to 35 | 1,912 |
| 30 | 5 | 27.5 to 32.5 | 936 |
| 30 | 3 | 28.5 to 31.5 | 564 |
| 30 | 2 | 29 to 31 | 384 |
| 30 | 1 | 29.5 to 30.5 | 200 |
| 30 | 0 | 29.75 to 30.25 | 92 |
| 2 | 4 | 0 to 4 (a disc) | 49 |
| 0 | 4 | 0 to 2 (a disc) | 13 |

**The band is never thinner than half a cell**: a quarter of a cell either side of the radius.
A width of 0 is drawn half a cell wide; a width of 1 is one cell wide. (Section 8, item 1, says
what such thin rings look like.)

At the top of the ranges, a radius of 750 with a width of 255 lights the cells 623 to 877 from
the centre.

**In percent (`scale` on).** Sizes are taken from the grid's longer side, counted in cells (the
larger of `C` and `R`; call it `L`).

- A radius of `N` is `N` percent of **half** the longer side, as a number of cells with its
  fraction kept. 100 is `L ÷ 2` cells: 50.5 cells on a longer side of 101, 50 on 100, 20.5 on
  41. With the centre in the middle, 100 reaches the ends of the longer side.
- A width of `N` is `N` percent of the **whole** longer side, **rounded down to a whole number of
  cells**. On a longer side of 101: 1 gives 1 cell, 2 gives 2, 5 gives 5, 10 gives 10, 33 gives
  33. On 41: 1 and 2 give 0 cells (drawn half a cell wide), 5 gives 2, 10 gives 4, 33 gives 13.
  On 301: 1 gives 3, 2 gives 6, 5 gives 15, 33 gives 99.
- Only the start width and the end width are whole numbers of cells. Between them the width
  moves in a straight line and takes every value on the way: from 0 to 3 percent on a longer
  side of 101 (0 cells to 3) the width is 1.5 cells half way.

Measured with hard edges:

| Buffer | Radius | Width | Lit cells are exactly those whose distance from the centre cell is | How many |
|---|---|---|---|---|
| 101 by 101 | 50 | 10 | 20.25 to 30.25 | 1,572 |
| 101 by 101 | 100 | 5 | 48 to 53 | 1,396 |
| 100 by 100 | 50 | 10 | 20 to 30 | 1,576 |
| 41 by 41 | 50 | 10 | 8.25 to 12.25 | 260 |
| 41 by 41 | 50 | 2 | 10 to 10.5 | 44 |
| 201 by 81 | 40 | 10 | 30.2 to 50.2 | 4,256 |
| 81 by 201 | 40 | 10 | 30.2 to 50.2 | 4,256 |
| 301 by 301 | 25 | 5 | 30.125 to 45.125 | 3,532 |

Which side is the longer makes no difference to the shape: 201 by 81 and 81 by 201 give the same
ring, a circle 30.2 to 50.2 cells from the centre, cut off by the short side.

At the top of the ranges the ring is far outside any buffer: a radius of 750 percent is 3.75
times the longer side.

### `cycles`: rings

How many rings there are over the effect, one after another (section 3).

### `acceleration`

Bends progress (section 3 and appendix A). Read as a whole number, so everything from −0.99 to
0.99 is no acceleration at all, 1.5 is 1 and −9.99 is −9.

### `blendEdges`: soft edges

Off: every cell of the band has alpha 1. On: alpha is 1 only where a cell's distance is exactly
the radius, and falls in a straight line to 0 at each edge (section 4).

### `scale`

Whether the four sizes are in percent of the prop or in cells, as above. It does not touch the
centre settings, which are always percent.

---

## 3. Motion and time

**What the effect is told.** The time since it started in whole milliseconds, its own length in
milliseconds (at least 1), and the sequence's frame time in milliseconds. It is never asked for a
time at or after its end.

**Time moves in whole frames.** The frame number is the time since the start divided by the frame
time, rounded down. The picture is the same, bit for bit, for every millisecond of a frame: with
50 ms frames, the pictures at 500, 501, 525 and 549 ms are identical and the one at 550 ms is the
next.

**The frame count.** The effect has as many frames as there are frame starts within its length:
its length divided by the frame time, rounded up. 1,000 ms at 50 ms is 20 frames; so are 975 ms
and 951 ms; 950 ms is 19; 1,001 ms and 1,050 ms are 21. An effect no longer than one frame time
has 1 frame.

### One ring

**Progress goes in equal steps from 0 on the first frame to exactly 1 on the last frame.** With
`n` frames, frame `k` (0 to `n − 1`) is `k` of `n − 1` equal steps along. A 20-frame effect moves
a nineteenth of the way each frame; a 21-frame effect a twentieth. An effect of one frame shows
progress 0 throughout; an effect of two frames shows 0 and then 1.

Measured as the progress shown at 500 ms into the effect:

| Length, ms | Frame time, ms | Frames | Frame number at 500 ms | Progress shown |
|---|---|---|---|---|
| 1,000 | 50 | 20 | 10 | 0.5263 |
| 975 | 50 | 20 | 10 | 0.5263 |
| 951 | 50 | 20 | 10 | 0.5263 |
| 950 | 50 | 19 | 10 | 0.5556 |
| 1,001 | 50 | 21 | 10 | 0.5000 |
| 1,050 | 50 | 21 | 10 | 0.5000 |
| 1,000 | 25 | 40 | 20 | 0.5128 |
| 1,000 | 20 | 50 | 25 | 0.5102 |
| 1,000 | 33 | 31 | 15 | 0.5000 |
| 1,000 | 40 | 25 | 12 | 0.5000 |
| 1,000 | 100 | 10 | 5 | 0.5556 |

So the ring's speed depends on the effect's length and on the frame time: the same effect made
twice as long moves half as fast, and a ring always arrives exactly on its last frame.

### Several rings

With `cycles` at `c` and `n` frames, each ring has `n ÷ c` frames of the effect to itself, one
ring's share following the last without a gap. That share need not be a whole number of frames.

- A ring is at progress 0 at the moment its share begins and moves one step a frame, where a
  step is one part in (its share less one frame). So it reaches progress 1 one frame before its
  share is up, and **stays at 1 until the next ring's share begins**.
- Where a share begins between two frames, the ring is first seen a part of a step along: the
  part of a frame that has passed since its share began.
- **With as many rings as frames, or more, a share is one frame or less and nothing moves:
  every frame shows progress 0.**
- With nearly as many rings as frames, a step is longer than the whole travel: each ring is
  seen at most once short of its end, and otherwise at progress 1.

Measured progress, frame by frame, on a 20-frame effect:

| Rings | Frames 0 to 19 |
|---|---|
| 1 | 0, 0.0526, 0.1053, 0.1579, … 0.9474, 1 |
| 2 | 0, 0.1111, 0.2222, … 0.8889, 1, then the same ten again |
| 4 | 0, 0.25, 0.5, 0.75, 1, four times |
| 5 | 0, 0.3333, 0.6667, 1, five times |
| 10 | 0, 1, ten times |
| 3 | 0, 0.1765, 0.3529, 0.5294, 0.7059, 0.8824, 1, 0.0588, 0.2353, 0.4118, 0.5882, 0.7647, 0.9412, 1, 0.1176, 0.2941, 0.4706, 0.6471, 0.8235, 1 |
| 7 | 0, 0.5385, 1, 0.0769, 0.6154, 1, 0.1538, 0.6923, 1, 0.2308, 0.7692, 1, 0.3077, 0.8462, 1, 0.3846, 0.9231, 1, 0.4615, 1 |
| 6 | 0, 0.4286, 0.8571, 1, 0.2857, 0.7143, 1, 0.1429, 0.5714, 1, **1**, 0.4286, 0.8571, 1, 0.2857, 0.7143, 1, 0.1429, 0.5714, 1 |
| 15 | 0, 1, 1, 1, five times |
| 18 or 19 | 0 on the first frame, then 1 on all the rest |
| 20 to 100 | 0 on every frame |

and on a 21-frame effect:

| Rings | Frames 0 to 20 |
|---|---|
| 3 | 0, 0.1667, 0.3333, 0.5, 0.6667, 0.8333, 1, three times |
| 7 | 0, 0.5, 1, seven times |
| 2 | 0, 0.1053, 0.2105, … 0.9474, 1, 0.0526, 0.1579, … 0.8947, 1 |
| 4 | 0, 0.2353, 0.4706, 0.7059, 0.9412, 1, 0.1765, 0.4118, 0.6471, 0.8824, 1, 0.1176, 0.3529, 0.5882, 0.8235, 1, 0.0588, 0.2941, 0.5294, 0.7647, 1 |
| 20 | 0 on the first frame, then 1 on all the rest |
| 21, 22 | 0 on every frame |

The 1 in bold on the 6-ring row is a ring's share beginning exactly on a frame and that frame
showing the ring before at its end instead; section 8, item 3, says when that happens.

### Acceleration

Acceleration bends each ring's progress, and the radius, the width and the colour all follow the
bent progress alike. The bend leaves 0 at 0 and 1 at 1, and never runs backwards. At 0 nothing
is bent. With several rings each ring is bent on its own.

- **Negative**: the ring starts fast and slows to a stop. At −10, a quarter of the way through
  its time it has covered 76% of its travel, and half way through, 97%.
- **Positive**: the ring starts slowly and finishes fast. At 10, half way through its time it
  has covered 6%, three quarters of the way 22%, nine tenths 53%.

A few of the measured values (appendix A has all 21 accelerations at 41 times):

| Share of the ring's time | −10 | −7 | −5 | −1 | 0 | 1 | 5 | 10 |
|---|---|---|---|---|---|---|---|---|
| 0.10 | 0.4095 | 0.3323 | 0.2753 | 0.1462 | 0.1 | 0.0316 | 0.0032 | 0.0100 |
| 0.25 | 0.7627 | 0.6681 | 0.5848 | 0.3505 | 0.25 | 0.1250 | 0.0236 | 0.0221 |
| 0.50 | 0.9688 | 0.9298 | 0.8797 | 0.6464 | 0.5 | 0.3536 | 0.1213 | 0.0625 |
| 0.75 | 0.9990 | 0.9951 | 0.9855 | 0.8750 | 0.75 | 0.6495 | 0.3779 | 0.2208 |
| 0.90 | 1.0000 | 0.9999 | 0.9991 | 0.9684 | 0.9 | 0.8538 | 0.6853 | 0.5314 |

One habit to keep: on the positive side a larger number is not a slower start at every moment.
In the first tenth of its time a ring at 10 is ahead of one at 5, 6, 7, 8 or 9 (0.0100 against
0.0028 at 6), and falls behind them only later.

The acceleration table is what has to be reproduced. A smooth curve drawn through its points
stays within 0.00015 of today's response at every thousandth of the time; straight lines between
its points stay within 0.0033. Being within 0.0005 everywhere is close enough that no ring is a
tenth of a cell out on a travel of 200 cells.

### Settings that change over the effect (value curves)

A curve on any number setting is resolved before the effect is asked for a frame: the effect
receives, for each frame, the values the curves give at that moment, and draws the frame those
values describe. It keeps no memory of earlier values. A curve runs once over the whole effect,
not once for each ring. What that looks like:

- a curve on the end radius changes where the ring is heading: at each frame the ring is at the
  frame's progress between the start radius and that frame's end radius;
- because fractions are dropped, a size under a curve moves in whole numbers: whole cells in
  cells, whole percents of the longer side in percent (2 cells of radius at a time on a longer
  side of 400);
- a curve on a centre setting moves the centre a cell (or several) at a time;
- a curve on `cycles` changes the count in whole steps and so can make the ring jump;
- a curve on acceleration steps from one whole acceleration to the next.

### A frame depends on nothing else

The same settings, palette, buffer size, pixel position, effect length, frame time and time since
the start always give the same colour, bit for bit: wherever the effect sits in the sequence,
whatever random seed the renderer supplies, in whatever order pixels and frames are asked for,
and from several threads at once. Nothing is random.

---

## 4. Colour and transparency

### The ring's colour

On any one frame every lit cell has the same colour. It is the palette read at the ring's
progress (after acceleration): the palette's colours are spread evenly along progress, the first
at 0 and the last at 1, and between two neighbours the colour is a straight-line mix of the two,
channel by channel. With several rings the colour starts again from the first with each ring.

| Palette | Progress 0 | 0.25 | 0.5 | 0.75 | 1 |
|---|---|---|---|---|---|
| (empty) | white | white | white | white | white |
| red | red | red | red | red | red |
| red, blue | (1, 0, 0) | (0.75, 0, 0.25) | (0.5, 0, 0.5) | (0.25, 0, 0.75) | (0, 0, 1) |
| red, green, blue | (1, 0, 0) | (0.5, 0.5, 0) | (0, 1, 0) | (0, 0.5, 0.5) | (0, 0, 1) |
| red, green, blue, white | (1, 0, 0) | (0.25, 0.75, 0) | (0, 0.5, 0.5) | (0.25, 0.25, 1) | (1, 1, 1) |
| red, green, blue, white, black | (1, 0, 0) | (0, 1, 0) | (0, 0, 1) | (1, 1, 1) | (0, 0, 0) |

A palette holds up to 32 colours. A black palette colour gives a black ring that covers what is
beneath it (colour black, alpha as below).

### Alpha across the band

Take a cell's distance from the centre cell and compare it with the ring's radius and width.

- **Hard edges.** Alpha is 1 on every cell whose distance is no further from the radius than
  half the width, **the cells exactly on either edge included**. It is 0 on every other cell.
- **Soft edges.** Alpha is 1 where the distance equals the radius and falls in a straight line
  to 0 at half the width away, on both sides. It is given in whole 255ths, **rounded down**. So a
  cell exactly on an edge has alpha 0, and so has the thin sliver just inside each edge where the
  straight line is below one 255th.

Measured on a ring of radius 30 and width 20, in cells, along the row through the centre:

| Distance | 20 | 21 | 22 | 23 | 24 | 25 | 26 | 27 | 28 | 29 | 30 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Alpha, 255ths | 0 | 25 | 50 | 76 | 102 | 127 | 153 | 178 | 204 | 229 | 255 |

and the same going out from 30 to 40. Over all 10,201 cells of that frame every alpha is a whole
number of 255ths, none is above the straight line and none is more than one 255th below it.

What a soft ring amounts to: **half as much light as the same ring with hard edges, and half the
width at half strength or more.** The ring above lights 3,780 cells hard and 3,740 soft, of which
1,872 are at half strength or more; the alphas add up to 1,878. Full strength is reached only on
the few cells whose distance is exactly the radius.

A soft disc is brightest at the distance of its radius, not at its centre: with radius 3 and
width 10 the alphas along the row from the centre are 0.4, 0.6, 0.8, 1, 0.8, 0.6, 0.4, 0.2 and
then nothing.

Whatever the alpha, a lit cell carries the ring's colour at full value.

### What is left unlit

- **Outside the band, alpha is 0 and the colour is black: red, green and blue are 0 too.** That
  matters: when an effect is blurred the renderer blurs the colours it was given, lit or not, so
  a colour handed back with alpha 0 outside the band would brighten a blurred ring's edges.
- Inside a soft band, a cell whose alpha has rounded down to 0 carries the ring's colour with
  alpha 0. Nothing can be seen of it.
- Inside the ring (nearer the centre than its inner edge) is unlit like the outside.

Fades, sparkles, blur, colour adjustments and the blending with other layers are applied
afterwards by the renderer and are not the effect's business. One consequence to keep in mind:
the renderer may flash a sparkle, at full strength, on any pixel the effect has lit even faintly
(from about one 500th of full brightness), and the fidelity measure counts a pixel as lit from
about a tenth. So the faint outer parts of a soft band are not harmless, and light where today's
picture has none is not free.

---

## 5. Geometry

### 5.1 Round in cells

Distances count a column step and a row step as the same, so the ring is a true circle on the
grid whatever the buffer's proportions. On 101 columns by 41 rows a ring of radius 15 and width
2, in cells, is 33 cells wide and 33 tall (columns 34 to 66, rows 4 to 36). It is not stretched
to fit. (If the display's cells are not square the ring looks stretched to the same degree on the
display; that is the buffer's doing.)

### 5.2 Whole cells only

The centre is a whole cell and every pixel takes its nearest cell's value, so the picture is
made of whole cells: a pixel a third of the way between two cells shows exactly what the nearer
one shows, and pixels sharing a cell always match. A prop whose pixels do not sit on the grid
gets the grid's picture sampled at its nearest cells.

Measured on 9 by 9 cells with one lit cell, (4, 4): a pixel is lit for `u × 8` from 3.5,
included, up to 4.5, not included, and the same for `v`.

### 5.3 Odd and even sizes

At the default centre the ring is exactly centred on a buffer with odd counts of cells and half a
cell to the right and up on even counts (section 2). On 100 by 100 a centred ring reaches column
0 on the left one cell later than it reaches column 99 on the right.

### 5.4 A single row or column of pixels

A buffer one row high is a line of cells at whole distances from the centre cell (which is on
the line unless `centerY` is exactly 100). The ring shows as two runs of lit cells, one each side
of the centre, moving apart. Each run is as many cells long as there are whole numbers between
the ring's inner and outer edges, both included: a ring 4 cells wide shows 4 or 5 cells a side.
A buffer one column wide behaves the same, up and down.

At the default settings on a row of 41 cells (hard edges, a 20-frame effect) the lit cells are:
frame 0, cells 19 to 21; frame 5, cells 14, 15, 25 and 26; frame 10, cells 8 to 10 and 30 to 32;
frame 15, cells 2 to 5 and 35 to 38; frame 19, cells 0, 1, 39 and 40.

A band less than a cell wide lights a cell of the line only while its radius is within half its
width of a whole number, so on a line a thin ring blinks (section 8, item 1).

### 5.5 Very small buffers

There is no special rule: the same picture, on very few cells. With sizes in percent, a small
buffer makes small numbers of cells, and widths round down to 0 cells, so on small props the
default ring is the thinnest ring there is.

- One pixel (1 by 1), default settings, a 20-frame effect: alpha 0.976 on frame 0, 0.459 on
  frame 5, 0 from frame 10 on.
- 2 by 2: the centre cell is the top right one. On the last frame the top left and bottom right
  cells are at alpha 1 and the other two are unlit.
- 3 by 3, frame 15 of 20: the four side cells at 0.247, the four corners at 0.090, the middle
  unlit.

### 5.6 Buffers of more than 1,048,576 cells

A buffer whose columns times rows come to more than 1,048,576 gets a coarser grid: both counts
are shrunk by the same factor, just enough for their product to fit, and rounded down. Everything
in this document then holds on that coarser grid: the centre cell, sizes in cells, the longer
side, and each pixel taking its nearest cell.

| Buffer | Grid the picture is on |
|---|---|
| 1000 by 1000 | 1000 by 1000 |
| 1024 by 1024 | 1024 by 1024 |
| 1025 by 1024 | 1024 by 1023 |
| 1100 by 1000 | 1073 by 976 |
| 2000 by 600 | 1869 by 560 |
| 4096 by 300 | 3783 by 277 |
| 2048 by 1024 | 1448 by 724 |
| 2048 by 2048 | 1024 by 1024 |
| 3000 by 3000 | 1024 by 1024 |

So on 2048 by 2048 a radius of 100 cells is 100 cells of the coarse grid, which is 200 of the
buffer's own columns, and the picture is made of blocks two columns by two rows. Sizes in percent
keep their proportion to the buffer.

---

## 6. Edge cases

**Radius**
- Start and end equal: the ring holds still (its width and colour may still change).
- Start larger than end: the ring closes in.
- 0: the band is centred on the centre cell: a disc as far out as half the width.
- Larger than the buffer reaches: nothing of the band's middle is on the buffer; its inner part
  may be. Larger still: nothing is lit at all.

**Width**
- 0 and, in percent, any width that rounds down to 0 cells: half a cell wide.
- Twice the radius or more: a disc.
- Start and end widths different: the width moves in a straight line between them, and the least
  width of half a cell applies to the result at each frame.

**Rings**
- 1: one ring over the whole effect.
- More rings than there are frames to show them: section 3 and section 8, item 2.

**Acceleration**
- ±10: the strongest bends (appendix A). With several rings each ring is bent separately.

**Centre**
- 100: off the buffer by one cell. A ring whose band does not reach back onto the buffer lights
  nothing.
- A fraction: dropped.

**Time**
- An effect of one frame: the start sizes and the first colour, for that frame.
- An effect of two frames: start, then end.
- Very long effects (hours): the same rules; progress still reaches exactly 1 on the last frame.
- An effect that starts between two of the sequence's frames is told a time since its start that
  is not a whole number of frames; the frame number rounds it down like any other.

**Palette**
- Empty: white. One colour: that colour throughout. Black: an opaque black ring.

**Pixels**
- Positions are normally within 0 to 1. One below 0 counts as 0 and one above 1 as 1. One that
  is not a finite number (not a number, or either infinity) counts as 0 on that axis. Never fail,
  and never return a colour or alpha that is not a finite number between 0 and 1.

**Settings beyond their ranges** never arrive: 150 for a centre has become 100, a radius of 900
has become 750, 25 for acceleration has become 10, and `cycles` 0 has become 1.

---

## 7. Acceptance properties

**The test rig.** Unless a property says otherwise:

- Buffer 101 columns by 101 rows, so the centre cell at the default centre is (50, 50), the exact
  middle. Palette: red (255, 0, 0) alone. Effect 1,050 ms long with a frame time of 50 ms: 21
  frames, and frame `k` (any time from `k × 50` to `k × 50 + 49` ms) is `k` twentieths of the way.
  Settings are at their defaults except those named.
- *In cells* means `scale` off; *hard* means `blendEdges` off.
- A *sample at cell (x, y)* is the colour returned for a pixel at `u = x ÷ (C − 1)`,
  `v = y ÷ (R − 1)` (0.5 where there is one column or row). *Distance* is from the centre cell.
- *The row* is the cells to the right of the centre cell on its own row, named by their distance:
  cell 7 of the row is (57, 50).
- *Exactly* means the returned numbers are equal, not close. Where a tolerance is not given,
  exactly is meant: every number below was measured. Properties 10, 11 and 20 include cells that
  lie exactly on an edge of a moving ring; on every frame they name, those cells are lit when
  the edges are hard, as section 4 says (section 8, item 4, is about frames where they are not).
- *Progress read off the colour*: with the palette black then white, a lit sample's red, green
  and blue all equal the ring's progress.

Every property was measured on today's Shockwave, which meets all of them.

1. **Unlit is black and clear; lit is the palette colour.** In cells, hard, radius 30 and width 4
   throughout, any frame: the 760 cells at distance 28 to 32, both included, return exactly
   (1, 0, 0, 1). Every other cell returns exactly (0, 0, 0, 0).

2. **The centre is one whole cell.** In cells, hard, radii 0 and widths 0, default centre: exactly
   one cell is lit. It is (50, 50) on 101 by 101, (50, 50) on 100 by 100, (20, 10) on 41 by 21,
   (20, 10) on 40 by 20, (25, 10) on 50 by 20, (1, 1) on 3 by 3, (1, 1) on 2 by 2, and (0, 0) on
   1 by 1.

3. **Centre settings.** Same disc. The lit cell's column for each `centerX` on rows of 1, 2, 7,
   40, 41, 100, 101 and 200 cells is as tabled in section 2, and nothing is lit where the table
   gives a column beyond the last. `centerY` gives the same numbers counted from the bottom.
   With (`centerX`, `centerY`) = (25, 75) the lit cell is (25, 75) on 101 by 101, (10, 15) on
   41 by 21 and (12, 15) on 50 by 20.

4. **A centre of 100 is one cell past the last.** 41 by 41, in cells, hard, radius 0, width 6,
   `centerX` 100, `centerY` 50: exactly 11 cells are lit: (38, 20), and rows 18 to 22 of columns
   39 and 40. With `centerX` 50 and `centerY` 100, the same with columns and rows exchanged. With
   width 0 instead, nothing is lit.

5. **A radius counts cells.** In cells, hard, width 4, radius `r` throughout: the lit cells are
   exactly those at distance `r − 2` to `r + 2`: 248 cells for 10, 760 for 30, 1,136 for 45.

6. **A width counts cells, and the band is never under half a cell.** In cells, hard, radius 30:
   for widths 0, 1, 2, 3, 4, 5, 10 and 20 the lit cells are exactly those within 0.25, 0.5, 1,
   1.5, 2, 2.5, 5 and 10 of distance 30: 92, 200, 384, 564, 760, 936, 1,912 and 3,780 cells.

7. **In percent, sizes follow the longer side.** Hard, radius and width throughout. On 101 by
   101, radius 50 and width 10: the lit cells are exactly the 1,572 at distance 20.25 to 30.25;
   radius 100 and width 5: the 1,396 at distance 48 to 53. On 100 by 100, radius 50 and width
   10: the 1,576 at distance 20 to 30 from (50, 50). On 201 by 81 and on 81 by 201, radius 40
   and width 10: the 4,256 cells at distance 30.2 to 50.2 from the centre cell, in both.

8. **In percent, widths are whole cells and radii are not.** A row of 101 cells (101 by 1) with
   `centerX` 0, so that a cell's column is its distance; hard; in percent. Radius 50 (25.25
   cells): widths 0, 1 and 1.99 light cell 25 alone; 2 and 2.97 light 25 and 26; 3 lights 24 to
   26; 5 lights 23 to 27; 10 lights 21 to 30. Width 4 (4 cells): radii 40 and 41 light cells 19
   to 22; 42 and 43 light 20 to 23; 44 lights 21 to 24; 100 lights 49 to 52.

9. **Fractions are dropped.** Each of these pairs gives identical frames, compared in cells and
   in percent, hard and soft, at frames 0, 7, 14 and 20: `centerX` 50.9 and 50; `centerY` 49.99
   and 49; `startRadius` 3.9 and 3; `endRadius` 60.99 and 60; `startWidth` 5.9 and 5; `endWidth`
   10.5 and 10; `acceleration` 2.9 and 2; −2.9 and −2; 0.9 and 0; −0.9 and 0.

10. **Equal steps, the start on the first frame and the end on the last.** In cells, hard, radius
    0 to 20, width 4: on frame `k`, for every `k` from 0 to 20, the lit cells of the row are
    exactly `k − 2` to `k + 2` (those not below 0): cells 0 to 2 on frame 0, 8 to 12 on frame 10,
    18 to 22 on frame 20.

11. **The width steps the same way.** In cells, hard, radius 30, width 0 to 20: the row's lit
    cells are 30 alone on frames 0 and 1; 29 to 31 on frame 2; 28 to 32 on frame 5; 25 to 35 on
    frame 10; 23 to 37 on frame 15; 21 to 39 on frame 19; 20 to 40 on frame 20.

12. **How many frames an effect has.** Progress read off the colour at 500 ms, for the lengths
    and frame times tabled in section 3, is as tabled there, within 0.0001. An effect 50 ms long
    with 50 ms frames shows 0. An effect 100 ms long shows 0 at 0 and 49 ms and 1 at 50 and
    99 ms.

13. **A frame holds.** At the default settings the frames at 500, 501, 525 and 549 ms are
    identical, and the frame at 550 ms is different.

14. **A frame depends on nothing else.** For fixed settings, palette, buffer size, effect length,
    frame time and time since the start, every sample is identical: with the effect starting at
    0, at 60,000 or at 123,457 ms of the sequence; with any random seed; in whatever order
    samples and frames are asked for; and asked for from several threads at once.

15. **Rings.** Progress read off the colour, frame by frame, is as tabled in section 3 for 1, 2,
    3, 4, 5, 7, 10 and 15 rings on a 20-frame effect and for 2, 3, 4 and 7 rings on a 21-frame
    effect, within 0.0001. In cells, hard, radius 0 to 20, width 4, 2 rings on a 20-frame effect
    of 1,000 ms: frames 10 to 19 are identical to frames 0 to 9.

16. **More rings than the effect can show.** In cells, hard, radius 0 to 20, width 4, on a
    20-frame effect: with 20, 21, 40 or 100 rings every frame is identical to frame 0 of the same
    effect with 1 ring. With 19 rings, frame 0 is that same frame and every other frame is
    identical to the last frame of the effect with 1 ring. (With 18 rings likewise, except that
    frames 9 and 19 lack the four cells at distance 22 straight across and up and down from the
    centre: section 8, item 4.)

17. **Acceleration.** Progress read off the colour on an effect of 1,001 frames (frame `k` is `k`
    thousandths of the time) is as appendix A gives it for every whole acceleration from −10 to
    10 at every tabled time, within 0.0005, and never falls from one frame to the next. In
    cells, hard, radius 0 to 40, width 2, on the rig: the row's lit cells on frames 5, 10 and 15
    are

    | Acceleration | Frame 5 | Frame 10 | Frame 15 |
    |---|---|---|---|
    | −10 | 30, 31 | 38, 39 | 39, 40 |
    | −5 | 23, 24 | 35, 36 | 39, 40 |
    | −1 | 14, 15 | 25, 26 | 34 to 36 |
    | 0 | 9 to 11 | 19 to 21 | 29 to 31 |
    | 1 | 4 to 6 | 14, 15 | 25, 26 |
    | 5 | 0, 1 | 4, 5 | 15, 16 |
    | 10 | 0, 1 | 2, 3 | 8, 9 |

18. **Acceleration bends the colour and the width with the radius.** Palette red then blue,
    acceleration −10, frame 10 of 21: the lit colour is (0.03125, 0, 0.96875), within 0.0005.
    Acceleration 10, same frame: (0.9375, 0, 0.0625). In cells, hard, radius 500 throughout,
    width 0 to 200, on a row of 1,001 cells from the centre, half way through a 1,001-frame
    effect: cells 404 to 596 are lit at acceleration −10, 450 to 550 at 0, and 494 to 506 at 10.

19. **Soft edges.** In cells, soft, radius 30, width 20: along the row, alpha in 255ths is 0 at
    cell 20; 25, 50, 76, 102, 127, 153, 178, 204 and 229 at cells 21 to 29; 255 at cell 30; and
    the same in reverse out to 0 at cell 40. Over all 10,201 cells: every alpha is a whole number
    of 255ths; none is above the straight line from 1 at distance 30 to 0 at distances 20 and
    40; none is more than one 255th below it; 3,740 cells are lit, 1,872 of them at half strength
    or more; and every lit cell's colour is exactly (1, 0, 0).

20. **Cells on an edge.** In cells, radius 0 to 20, width 4, frame 10: with hard edges cells 8
    and 12 of the row are lit; with soft edges both have alpha 0, cell 10 has alpha 1 and cells
    9 and 11 have alpha 127 ÷ 255.

21. **Colour through the palette.** A disc that covers the centre cell (in cells, hard, radii 0,
    widths 10). Palette red, green, blue: the centre cell's colour on frames 0, 5, 10, 15 and 20
    is (1, 0, 0), (0.5, 0.5, 0), (0, 1, 0), (0, 0.5, 0.5) and (0, 0, 1). The palettes tabled in
    section 4 give the colours tabled there, within 0.0001. With 2 rings on a 20-frame effect
    the colour is (1, 0, 0) on frames 0 and 10 and (0, 0, 1) on frames 9 and 19. With an empty
    palette it is (1, 1, 1). With the palette black alone the disc's cells return (0, 0, 0, 1).

22. **Round, not stretched.** Buffer 101 by 41. In cells, hard, radius 15, width 2: 188 cells
    are lit, in columns 34 to 66 and rows 4 to 36. In percent, hard, radius 50, width 10: the
    lit cells are exactly those of the buffer at distance 20.25 to 30.25 from (50, 20): 972
    cells, reaching columns 20 to 80 and every row.

23. **A single row and a single column.** Default settings but hard, a 1,000 ms effect, buffer
    41 by 1: the lit cells are 19 to 21 on frame 0; 14, 15, 25 and 26 on frame 5; 8 to 10 and 30
    to 32 on frame 10; 2 to 5 and 35 to 38 on frame 15; 0, 1, 39 and 40 on frame 19. Buffer 1
    by 41: the same numbers, as rows.

24. **Thin rings on a line (section 8, item 1).** Buffer 41 by 1, in cells, hard, radius 0 to
    20, width 0, a 1,000 ms effect: nothing is lit on frames 5 to 14; two cells are lit on every
    other frame but the first, which has one. Over 1,050 ms (21 frames) instead, frame `k`
    lights exactly cells `20 − k` and `20 + k`.

25. **Tiny buffers.** Default settings, a 1,000 ms effect. 1 by 1: alpha is 249 ÷ 255 on frame
    0, 117 ÷ 255 on frame 5, and 0 on frames 10, 15 and 19. 2 by 2, frame 19: cells (1, 0) and
    (0, 1) have alpha 1; (0, 0) and (1, 1) have 0. 3 by 3, frame 15: the four side cells have
    alpha 63 ÷ 255, the four corners 23 ÷ 255, the middle 0.

26. **Each pixel takes its nearest cell.** 9 by 9, in cells, hard, radii 0, widths 0 (one lit
    cell, (4, 4)): a pixel at `v` = 0.5 is lit for `u` = 0.4375, 0.46875, 0.5 and 0.546875 and
    unlit for 0.421875 and 0.5625; the same with `u` and `v` exchanged. On 5 by 5 with one lit
    cell at a time: a pixel at `u` = −0.5 takes column 0, at 1.5 column 4; a pixel whose `u` is
    not a number, or is either infinity, takes column 0; likewise `v` and rows.

27. **Buffers of more than 1,048,576 cells.** With one lit cell at the centre (in cells, hard,
    radii 0, widths 0), the stretch of `u` that is lit along the centre's row is one cell of the
    grids tabled in section 5.6: 1 ÷ 1023 wide on 1024 by 1024 and on 2048 by 2048, 1 ÷ 1447
    wide on 2048 by 1024, 1 ÷ 1072 wide on 1100 by 1000 (each within 1%). On 2048 by 2048, in
    cells, hard, radius 100, width 2: the ring's lit stretch along the centre's row runs from
    197 to 203 of the buffer's own columns from the centre (± 0.5).

28. **A ring that closes in and thins.** In cells, hard, radius 40 to 0, width 10 to 0: the
    row's lit cells are 35 to 45 on frame 0, 27 to 33 on frame 5, 18 to 22 on frame 10, 9 to 11
    on frame 15, cell 2 alone on frame 19 and cell 0 alone on frame 20.

29. **The default effect on the rig.** Soft, in percent, radius 1 to 100, width 5 to 10. The lit
    cells lie between these distances, and alpha along the row is:

    | Frame | Lit cells | Nearest lit, furthest lit | Alpha along the row, by distance (255ths) |
    |---|---|---|---|
    | 0 | 25 | 0, 2.83 | 0: 203, 1: 204, 2: 102 |
    | 5 | 504 | 9.90, 16.03 | 10: 9, 11: 91, 12: 173, 13: 254, 14: 173, 15: 92, 16: 10 |
    | 10 | 1,220 | 21.84, 29.21 | 22: 16, 23: 84, 24: 152, 25: 220, 26: 221, 27: 153, 28: 85, 29: 17 |
    | 15 | 2,060 | 33.73, 42.30 | 34: 21, 35: 80, 36: 138, 37: 196, 38: 254, 39: 196, 40: 138, 41: 80, 42: 21 |
    | 20 | 2,560 | 45.54, 55.47 | 46: 25, 47: 76, 48: 127, 49: 178, 50: 229 |

    (On frame 10 the ring's radius is 25.5 cells and its width 7.5.) Alphas within one 255th.

30. **Nothing breaks at the extremes.** On buffers 1 by 1, 2 by 1, 1 by 2, 2 by 2, 3 by 3, 7 by 5
    and 101 by 3, with every combination of: `centerX` and `centerY` 0 and 100; radii 0 to 750,
    750 to 0, 750 throughout and 0 throughout; widths 0 to 255 and 255 to 0; 1 and 100 rings;
    acceleration −10 and 10; hard and soft; in cells and in percent; at 0, 500 and 1,000 ms of
    1,050 ms, in an effect 1 ms long, and in the last frame of an hour-long effect: no failure,
    and every colour and alpha returned is a finite number from 0 to 1. (17,920 frames.)

---

## 8. Defects in today's picture, kept unless the owner says otherwise

Each of these can be seen and looks like a fault rather than a choice. **The new Shockwave
reproduces every one of them.** Each entry says what is seen and what would change on screen if
it were put right, so that the owner can decide.

1. **A ring under a cell wide breaks into dots, and on a line of pixels it blinks.**
   The band is as thin as half a cell (a width of 0) or one cell (a width of 1), and a cell is
   lit only if its distance falls inside the band. On a grid, cells do not come at every
   distance.
   - Which rings are that thin: in cells, widths 0 and 1. In percent, any width that comes to
     under 2 cells: on a longer side under 20 cells the default start width (5 percent) is 0
     cells, and on a longer side under 10 so is the default end width.
   - On a 101 by 101 buffer at radius 30 (a circle 188 cells round): hard and width 0 lights 92
     cells, with none at all in some ten-degree stretches of the circle; width 1 lights 200 and
     is unbroken. Soft and width 1 lights the same 200 but only 92 at half strength or more;
     soft and width 0 lights 92, 60 at half strength or more.
   - On a row of 41 cells, in cells, radius 0 to 20 over a 20-frame effect: with width 0 nothing
     at all is lit on frames 5 to 14; with width 1 and soft edges those ten frames are lit at
     under half strength. With widths of 2 or more no frame is dark.
   - The default effect on a single row: dark on 10 of its 20 frames on 5 or 8 cells, on 8
     frames on 10 cells, on 7 on 12 or 16 cells, on 10 on 19 cells; never dark on 20 cells or
     more.

   **Kept unless the owner says otherwise.** Put right (a band never under one cell wide,
   lighting every cell it passes over), thin rings would be unbroken circles, about twice as
   many pixels would be lit by a width-0 ring, and small props would show the default ring on
   every frame. No Shockwave in the owner's sequences is this thin (section 10), and xLights'
   own frames have none to compare with.

2. **With as many rings as frames, nothing moves.** A 1-second effect at 20 frames a second set
   to 20 rings or more shows its first picture for the whole second. Set to 18 or 19 it shows
   the first picture for one frame and the last picture for the rest. In general a ring needs a
   share of at least two frames to be seen at both its start and its end.

   **Kept unless the owner says otherwise.** Put right, the rings would be spread over the
   frames there are, and such an effect would flicker between sizes where it now stands still.

3. **A ring's first frame is sometimes the end of the ring before.** When a ring's share of the
   effect is not a whole number of frames, yet some later ring's share begins exactly on a frame
   (the frame count and the ring count share a factor), that frame shows either the new ring at
   progress 0 or the old one still at progress 1. Measured on all 23,973 such frames of effects
   of 2 to 240 frames with 2 to 100 rings: 15,657 show the new ring's start and 8,316 the old
   ring's end. For any one frame count and ring count they all go the same way. Which way
   follows one regularity, with no exception among the 23,973: the frame shows the old ring's
   end exactly when the double-precision number nearest to (frames ÷ rings) is greater than the
   true quotient. `restarts.tsv` lists all 23,973 with the progress each shows.

   Frame and ring counts up to 40 frames whose exact restarts show the old ring's end (every
   other pair with an exact restart shows the new ring's start):

   | Frames | Rings |
   |---|---|
   | 10 | 6 |
   | 14 | 6, 12 |
   | 15 | 9 |
   | 16 | 10 |
   | 18 | 10, 14 |
   | 20 | 6, 12, 14, 18 |
   | 21 | 9, 18 |
   | 22 | 10, 18, 20 |
   | 24 | 15 |
   | 25 | 15 |
   | 26 | 10, 14, 20, 22 |
   | 27 | 15, 21 |
   | 28 | 6, 12, 18, 24 |
   | 30 | 9, 18, 21, 27 |
   | 32 | 10, 20, 22, 26 |
   | 33 | 15, 27, 30 |
   | 34 | 6, 12, 24, 26 |
   | 35 | 15, 21, 30 |
   | 36 | 10, 14, 20, 22, 28, 34 |
   | 38 | 14, 18, 22, 28, 34, 36 |
   | 39 | 15, 21, 30, 33 |
   | 40 | 6, 12, 14, 18, 24, 25, 26, 28, 34, 36 |

   **Kept unless the owner says otherwise.** Put right, every such frame would show the new
   ring's start: one frame of each affected ring changes.

4. **With hard edges, a cell exactly on an edge is now and then left unlit.** The rule is that
   such a cell is lit (section 4), and it is in 23,630 of 24,595 measured cases in which the
   settings and the frame put an edge exactly on a whole number of cells. In the other 965 the
   cell is unlit. Where the frame's progress is an exact binary fraction (0, 1, a half, a
   quarter, …) the cell is always lit: 9,455 of 9,455. Where it is not (a third of the way, a
   fifth, a seventh, …) it is lit in 14,175 of 15,140. Rows 48 to 50 of appendix B are three of
   the unlit ones and row 51 is one that is lit. `edges.tsv` lists all 24,595 with what each
   shows.

   The same shows with several rings, on a frame where a ring should be exactly at its end and
   is a hair short of it: with 18 rings on a 20-frame effect, in cells, hard, radius 0 to 20 and
   width 4, frames 9 and 19 lack the four cells 22 out from the centre along its row and column
   that the effect's true last frame has.

   **Kept unless the owner says otherwise**, as far as it can be: see section 13, item 1. Put
   right, those frames would light the four cells straight across, up and down from the centre
   at that distance, and any other cell exactly that far away.

5. **In percent, a width that should be an exact whole number of cells is occasionally one cell
   less.** Of 489 measured pairs of percentage and longer side whose product is a whole number
   of cells, 484 give that number and 5 give one fewer: 50 percent of 58 cells is drawn 28 cells
   wide, not 29; 90 percent of 70 is 62, not 63; 50 percent of 114 is 56, not 57; 25 percent of
   116 is 28, not 29; 50 percent of 116 is 57, not 58. (Rows 23 and 24 of appendix B;
   `widths.tsv` lists all 489.)

   **Kept unless the owner says otherwise.** Put right, those rings would be one cell wider.

6. **On a buffer of more than 1,048,576 cells the picture is coarse, and sizes in cells grow.**
   Section 5.6. On 2048 by 2048 everything is drawn in blocks of two by two cells and a radius
   of 100 cells spans 200 columns.

   **Kept unless the owner says otherwise.** Put right, such buffers would get the fine picture
   and, for sizes in cells, rings half the size they show today.

7. **A centred ring is half a cell off centre on even-sized buffers, and the centre cannot be
   put on a long buffer's last cell.** Section 2. **Keep it: xLights' own frames have it**
   (section 11; every buffer the owner's Shockwaves play on is an even number of cells wide).
   Put right, a centred ring would move half a cell left and down on even sizes, and imports
   would stop agreeing with xLights there.

8. **A stronger positive acceleration is not always a slower start.** In the first tenth of its
   time a ring at acceleration 10 is ahead of one at 5 to 9 (section 3).

   **Kept unless the owner says otherwise.** Put right, the positive accelerations would be in
   order at every moment, and the first tenth of a ring at 10 would change.

---

## 9. What does not have to match

Only what cannot be seen:

- The colour carried by a cell **inside** a soft band whose alpha is 0. (Outside the band the
  colour must be black: section 4.)
- One 255th of alpha either way, on a cell whose straight-line value is within rounding error of
  a whole 255th. But a cell that is unlit today must not become lit because a new design rounds
  alpha up or keeps fractions of a 255th: today's sliver of zero alpha just inside each soft
  edge is one 255th of half the band's width, and sparkles treat any alpha above about a 500th
  as lit.
- A colour channel by less than 0.0005.
- With acceleration other than 0, a cell within 0.01 of a cell of an edge: the response is pinned
  only as closely as appendix A pins it.
- What is returned for a time at or after the effect's end: it is never asked for.
- How the frame is worked out, in what order, on which thread, and what it costs (but see
  `integration.md` for the cost today, which the new one should not exceed).

Everything else is part of the look, including each item of section 8.

---

## 10. How it is used in practice

From the seven real sequences the fidelity corpus is built on (counts and values only).

- One of the seven has Shockwaves: **59 effects**, all on five layout-wide or part-layout groups.
  The other six have none.
- They are six designs, each repeated on different groups:

| | A | B | C | D | E | F |
|---|---|---|---|---|---|---|
| Effects | 24 | 16 | 9 | 5 | 3 | 2 |
| Start radius | 1 | 1 | 1 | 1 | 1 | 1 |
| End radius | 170 | 154 | 196 | 250 | 185 | 154 |
| Start width | 5 | 5 | 5 | 5 | 5 | 5 |
| End width | 118 | 10 | 200 | 85 | 203 | 59 |
| Acceleration | 0 | 0 | −7 | −5 | 0 | 0 |
| Palette colours | 1 | 1 | 4 (2 different) | 1 | 1 | 1 |
| Length, ms | 900 (9), 950 (15) | 900 | 3,600 (2), 5,400 (2), 7,200 (5) | 2,050 | 1,800 | 3,600 |
| Fade out, ms | none | none | 1,000 | 1,000 | none | 2,000 |
| Sparkles (of 200) | 54 | none | 54 | none | 54 | none |

- **Every one**: centre 50 and 50; 1 ring; soft edges; **sizes in cells** (the file is from an
  xLights older than 2025.04); no curves; no blur; normal blending; no colour adjustments; 50 ms
  frames; a length that is a whole number of frames (18 to 144 frames).
- **Never used**: sizes in percent, hard edges, more than one ring, a centre other than the
  middle, positive acceleration, a curve on any setting, a width under 5 cells.
- The buffers are large and sparse: 400 by 177 cells with 2,148 pixels, 400 by 178 with 4,959,
  206 by 175 with 1,200, 400 by 221 with 478, and 400 by 118 with 1,168. All are an even number
  of cells wide. The furthest pixel of the largest is about 218 cells from the centre: the wide
  designs (A, C, D and E) have swept every pixel of it by their last frame, and B and F stop
  short of its far ends.
- 36 of the 59 carry sparkles, so the faint outer part of their soft band matters (section 4).
- All share their pixels with other effects: other layers on the same group (Color Wash,
  Snowflakes, Fill, Fan, Plasma, or another Shockwave, three deep in design C) and other rows.

What that says about priorities: the soft band's exact profile, sizes in cells, the centre cell
on even-sized buffers, timing to the frame, and the negative accelerations are what the owner's
show exercises. They are also the parts xLights' frames confirm (section 11).

---

## 11. Against xLights' own frames

The 59 effects above were drawn by today's Shockwave and held, frame by frame and pixel by
pixel, against the frames xLights itself rendered for the sequence. Because every Shockwave there
shares its pixels with other effects, the comparison was made on the pixel-frames where
PixelFlow's whole picture is the Shockwave and nothing else, and then on the pixels among those
where xLights' frame shows nothing else either.

**Where the ring can be isolated, the two agree to within 1 or 2 of 255.**

| Design | What it exercises | Pixels | Pixel-frames | Agreeing within 2 of 255 |
|---|---|---|---|---|
| A (on 206 by 175) | no acceleration, one colour, 18 frames, the last frame included | 445 | 3,000 | 100% |
| E (on 400 by 178) | no acceleration, the widest band | 351 | 3,422 | 99.9% |
| E (on 206 by 175) | the same, with the last frame | 371 | 5,071 | 99.3% |
| C (on 400 by 178), 72 frames | acceleration −7, four palette colours | 424 | 5,812 | 98.4% |
| C (on 400 by 178), 144 frames | the same | 497 | 12,565 | 97.2% |
| F (on 400 by 178) | a fade over most of the effect (allowed for) | 611 | 7,549 | 99.7% |

The pixel-frames that do not agree are xLights' sparkles: on them xLights' frame is white at 53%,
75% or 100%, for one to three frames on a pixel, where the ring is some other colour and alpha.
Each renderer flashes its own pixels, and designs C and E carry sparkles.

How sharp that agreement is: with either side's frame moved by a single frame the share within 2
of 255 falls from 98 to 100% to about 1% (a mean difference of 23 to 44 of 255). The agreeing
pixels cover the whole profile of the band, alpha 1 to 254 of 255. So xLights' frames confirm, to
the frame and to a fraction of a cell:

- the centre cell at 50 and 50 on buffers 400 and 206 cells wide and 175, 177 and 178 high;
- a radius and a width in cells meaning cells, with the width from edge to edge;
- progress in equal steps from the first frame to exactly 1 on the last;
- the straight-line soft edge, in whole 255ths rounded down;
- the colour running through the palette with progress;
- acceleration −7.

**No visible difference between today's Shockwave and xLights' frames was found that belongs to
the effect, and none that would belong to the importer's translation: every xLights number means
in xLights' frames what it says** (cells for cells, the frame for the frame).

**Where the frames do differ, it is not the ring.** Two of the 42 judged instances score "close"
rather than "matches" (section 12).

- The one judged alone (design E on the largest group, look 62) differs on about 660 of its
  pixels, gathered on a few props. On those pixels xLights' frame is the same ring, at the same
  alpha, over a second light that brightens steadily from frame to frame and that PixelFlow does
  not draw there: worked back from each frame, that second light comes out the same whatever the
  ring's alpha is on the pixel. On the other pixels the two frames are equal to the unit. So the
  difference is another effect's, or the reference's (the corpus notes say this reference holds
  stale frames on a few props), not the Shockwave's and not its import.
- The other (design D on a small group, look 75, layered) lies in the part of the sequence where
  xLights' file was saved before it had finished rendering: xLights' frame is dark on nearly all
  of the pixels the ring crosses.

**What xLights' frames could not be asked**, because no corpus sequence has it: sizes in percent,
hard edges, more than one ring, a centre away from the middle, positive acceleration, acceleration
−5 (its five instances all lie where the reference is dark), thin rings, and frame times other
than 50 ms. For those, today's picture is the only reference there is.

---

## 12. Fidelity baseline

How today's Shockwave scores against the reference render of the one corpus sequence that has
any, over five fresh runs of the same code. "Look" is 0 to 100; 85 or more is a match, 55 or more
is close. The new Shockwave should score the same, within the spread shown.

- **Shockwave as a kind: look 94.0 in four runs and 94.1 in the fifth.** Verdicts: 40 match and 2
  close in four runs; 39 match and 3 close in one.
- Parts, the same in all five to two places: coverage 0.97, brightness 0.94, colour 1.00, pattern
  0.95, motion 0.89 (0.90 once), rhythm 0.99.
- Of the 59 effects, 42 could be judged, and only 1 of those had pixels to itself; the other 41
  were judged together with whatever was layered on the same pixels. 17 could not be judged.
- The sequence as a whole moved more than its Shockwaves did: look 92.9 to 95.6 and "as seen"
  95.5 to 96.4 over the five runs, because other effects in it are random.

Each judged instance, by its number in the scorecard. Where one number is given the look was the
same in all five runs to one decimal place.

| Scorecard number | Design | Buffer, cells | Length, ms | Look over five runs | Weakest part at its lowest |
|---|---|---|---|---|---|
| 49 | E | 400 by 177 | 1,800 | 99.8 to 100.0 | motion 0.98 |
| 50, 51 | A | 400 by 177 | 900 | 100.0 | – |
| 52 | A | 400 by 177 | 950 | 97.5 to 100.0 | motion 0.94 |
| 53 | A | 400 by 177 | 950 | 99.9 to 100.0 | motion 0.99 |
| 54 to 57 | A | 400 by 177 | 950, 900 | 100.0 | – |
| 90 | E | 206 by 175 | 1,800 | 100.0 | – |
| 91 | A | 206 by 175 | 900 | 93.6 to 100.0 | motion 0.84 |
| 92 | A | 206 by 175 | 900 | 99.8 to 100.0 | motion 0.99 |
| 93 to 95 | A | 206 by 175 | 950 | 100.0 | – |
| 96 to 98 | A | 206 by 175 | 950, 900 | 99.9 to 100.0 | motion 0.99 |
| 170 | C | 400 by 178 | 3,600 | 99.7 to 99.8 | rhythm 0.98 |
| 171 | B | 400 by 178 | 900 | 99.8 to 100.0 | motion 0.98 |
| 172 | B | 400 by 178 | 900 | **77.1 to 100.0** | motion 0.44 |
| 173 | B | 400 by 178 | 900 | **89.4 to 100.0** | motion 0.74 |
| 174 | B | 400 by 178 | 900 | 97.1 to 100.0 | motion 0.93 |
| 175 | B | 400 by 178 | 900 | 95.8 to 99.9 | motion 0.90 |
| 176 | B | 400 by 178 | 900 | **87.0 to 99.9** | motion 0.68 |
| 177 | B | 400 by 178 | 900 | 94.4 to 100.0 | motion 0.86 |
| 178 | B | 400 by 178 | 900 | 96.9 to 100.0 | motion 0.92 |
| 179 | F | 400 by 178 | 3,600 | 100.0 | – |
| 185 | C | 400 by 178 | 5,400 | 99.8 to 99.9 | motion 0.98 |
| 201 | C | 400 by 178 | 7,200 | 97.1 to 98.0 | motion 0.93 |
| 204 | D | 400 by 178 | 2,050 | 100.0 | – |
| 216 | E | 400 by 178 | 1,800 | **62.0 to 62.2** (judged alone; 99.9 layered) | motion 0.35 |
| 217 to 224 | A | 400 by 178 | 900, 950 | 99.9 to 100.0 | motion 0.99 |
| 263 | C | 400 by 118 | 7,200 | 99.7 to 99.9 | motion 0.97 |
| 266 | D | 400 by 118 | 2,050 | **75.0** | motion 0.38 |

Reading it:

- **The short ones move.** The eight 900 ms instances of design B on the largest group, which
  share their pixels with two layers of snow, ranged over as much as 23 points from run to run
  with the code unchanged (one of them from 77 to 100), and five instances in all moved by more
  than 5. Nine moved by more than 1. A single run cannot tell a change in the effect from this;
  compare several runs before and after, instance by instance.
- **Two never match, and neither is the effect's doing** (section 11): 216, steady at 62, and
  266, steady at 75.
- Everything else is at or within a point of 100 in every run, apart from 201 (97 to 98).
- Pixel for pixel on the one instance judged alone (a diagnostic, not a target): mean error 26.7
  of 255 with 63% of pixels within tolerance, PixelFlow lighting 32% of the pixels where the
  reference lights 51%. Those are the pixels of section 11's "second light".
- The importer expects every one of the 59 to match.

---

## 13. What could not be pinned down

1. **Which exact-edge cells are unlit (section 8, item 4).** No rule could be found that a viewer
   could state. What is known: the cell is always lit when the frame's progress is an exact
   binary fraction, and is unlit in about one case in sixteen when it is not; the pattern is that
   of double-precision arithmetic (worked in single precision the same 24,595 cases come out
   differently in about 1,450); and there is more than one natural way to place a ring a share
   of the way between its start and its end in double precision, of which at least one
   reproduces every case and others differ in 600 to 950 of them. `edges.tsv` has every case, so
   a design can be held against all of them. A mismatch here is the four cells at the compass
   points of one frame (and any other cell at exactly that distance), with hard edges only.
2. **Sizes in percent against xLights.** Every corpus Shockwave is in cells. Whether xLights
   draws a percent radius and width as today's picture does (half the longer side for a radius,
   the whole longer side rounded down to cells for a width) rests on today's picture alone.
3. **Hard edges, several rings, an off-centre ring, positive acceleration, acceleration −5, thin
   rings, and frame times other than 50 ms against xLights.** None is in a corpus sequence that
   can be measured (section 11).
4. **Curves in xLights.** The importer notes that xLights restarts a setting's curve with each
   ring while PixelFlow runs it once over the effect. That is the importer's and the renderer's
   business, not the effect's, and no corpus Shockwave has a curve.
5. **Blur.** That a colour returned with alpha 0 outside the band would show under blur
   (section 4) follows from how the renderer blurs; it was not measured on frames, because no
   corpus Shockwave is blurred.
6. **The 17 instances that could not be judged**, and the five of design D, say nothing either
   way.
7. **Single rows and columns, tiny buffers, and buffers over a million cells** have no xLights
   frames to compare with. Sections 5.4 to 5.6 and properties 23 to 27 describe today's picture.

---

## Appendix A. The acceleration response

How far along its travel a ring is shown (progress after acceleration), against the share of the
ring's own time that has passed (progress before acceleration), for every whole acceleration.
Measured on an effect of 1,001 frames, read off the colour, to five places. At acceleration 0 the
two are equal. Every column starts at 0, ends at 1 and never falls.

`acceleration.tsv`, beside this file, has the same measurement at every thousandth of the time,
to the full single precision it was returned in: 1,001 rows, one column for each acceleration
from −10 to 10.

**Negative accelerations**

| Time | -10 | -9 | -8 | -7 | -6 | -5 | -4 | -3 | -2 | -1 |
|---|---|---|---|---|---|---|---|---|---|---|
| 0.000 | 0.00000 | 0.00000 | 0.00000 | 0.00000 | 0.00000 | 0.00000 | 0.00000 | 0.00000 | 0.00000 | 0.00000 |
| 0.025 | 0.11890 | 0.11019 | 0.10138 | 0.09249 | 0.08351 | 0.07444 | 0.06529 | 0.05604 | 0.04670 | 0.03726 |
| 0.050 | 0.22622 | 0.21063 | 0.19473 | 0.17850 | 0.16195 | 0.14506 | 0.12784 | 0.11027 | 0.09234 | 0.07405 |
| 0.075 | 0.32281 | 0.30197 | 0.28048 | 0.25833 | 0.23550 | 0.21197 | 0.18771 | 0.16271 | 0.13693 | 0.11036 |
| 0.100 | 0.40951 | 0.38481 | 0.35908 | 0.33228 | 0.30435 | 0.27525 | 0.24494 | 0.21336 | 0.18046 | 0.14619 |
| 0.125 | 0.48709 | 0.45975 | 0.43096 | 0.40063 | 0.36868 | 0.33503 | 0.29959 | 0.26225 | 0.22293 | 0.18151 |
| 0.150 | 0.55629 | 0.52735 | 0.49651 | 0.46366 | 0.42867 | 0.39139 | 0.35169 | 0.30939 | 0.26433 | 0.21634 |
| 0.175 | 0.61782 | 0.58813 | 0.55614 | 0.52166 | 0.48450 | 0.44445 | 0.40130 | 0.35479 | 0.30467 | 0.25066 |
| 0.200 | 0.67232 | 0.64261 | 0.61022 | 0.57488 | 0.53634 | 0.49431 | 0.44846 | 0.39847 | 0.34393 | 0.28446 |
| 0.225 | 0.72042 | 0.69128 | 0.65912 | 0.62359 | 0.58437 | 0.54106 | 0.49324 | 0.44043 | 0.38212 | 0.31774 |
| 0.250 | 0.76270 | 0.73460 | 0.70319 | 0.66805 | 0.62876 | 0.58481 | 0.53567 | 0.48070 | 0.41923 | 0.35048 |
| 0.275 | 0.79970 | 0.77301 | 0.74277 | 0.70851 | 0.66967 | 0.62567 | 0.57580 | 0.51929 | 0.45525 | 0.38268 |
| 0.300 | 0.83193 | 0.80692 | 0.77820 | 0.74519 | 0.70728 | 0.66373 | 0.61370 | 0.55622 | 0.49019 | 0.41434 |
| 0.325 | 0.85987 | 0.83673 | 0.80977 | 0.77835 | 0.74175 | 0.69910 | 0.64940 | 0.59150 | 0.52404 | 0.44543 |
| 0.350 | 0.88397 | 0.86281 | 0.83779 | 0.80821 | 0.77323 | 0.73187 | 0.68297 | 0.62515 | 0.55679 | 0.47595 |
| 0.375 | 0.90463 | 0.88551 | 0.86255 | 0.83498 | 0.80188 | 0.76215 | 0.71445 | 0.65718 | 0.58843 | 0.50589 |
| 0.400 | 0.92224 | 0.90515 | 0.88431 | 0.85888 | 0.82787 | 0.79004 | 0.74390 | 0.68762 | 0.61898 | 0.53524 |
| 0.425 | 0.93715 | 0.92205 | 0.90334 | 0.88013 | 0.85134 | 0.81565 | 0.77138 | 0.71648 | 0.64841 | 0.56398 |
| 0.450 | 0.94967 | 0.93650 | 0.91988 | 0.89891 | 0.87245 | 0.83906 | 0.79694 | 0.74379 | 0.67672 | 0.59211 |
| 0.475 | 0.96012 | 0.94876 | 0.93417 | 0.91542 | 0.89133 | 0.86039 | 0.82063 | 0.76955 | 0.70392 | 0.61960 |
| 0.500 | 0.96875 | 0.95908 | 0.94642 | 0.92985 | 0.90814 | 0.87972 | 0.84251 | 0.79378 | 0.72999 | 0.64645 |
| 0.525 | 0.97582 | 0.96770 | 0.95686 | 0.94237 | 0.92302 | 0.89717 | 0.86264 | 0.81652 | 0.75492 | 0.67263 |
| 0.550 | 0.98155 | 0.97483 | 0.96566 | 0.95316 | 0.93610 | 0.91283 | 0.88109 | 0.83778 | 0.77871 | 0.69813 |
| 0.575 | 0.98613 | 0.98066 | 0.97302 | 0.96237 | 0.94752 | 0.92680 | 0.89790 | 0.85759 | 0.80136 | 0.72293 |
| 0.600 | 0.98976 | 0.98538 | 0.97912 | 0.97018 | 0.95741 | 0.93918 | 0.91314 | 0.87595 | 0.82285 | 0.74702 |
| 0.625 | 0.99258 | 0.98914 | 0.98410 | 0.97671 | 0.96590 | 0.95006 | 0.92687 | 0.89291 | 0.84318 | 0.77036 |
| 0.650 | 0.99475 | 0.99210 | 0.98812 | 0.98212 | 0.97311 | 0.95955 | 0.93916 | 0.90849 | 0.86234 | 0.79294 |
| 0.675 | 0.99637 | 0.99439 | 0.99131 | 0.98654 | 0.97917 | 0.96775 | 0.95007 | 0.92270 | 0.88033 | 0.81472 |
| 0.700 | 0.99757 | 0.99612 | 0.99380 | 0.99010 | 0.98419 | 0.97475 | 0.95967 | 0.93558 | 0.89712 | 0.83568 |
| 0.725 | 0.99843 | 0.99740 | 0.99571 | 0.99291 | 0.98828 | 0.98064 | 0.96802 | 0.94716 | 0.91271 | 0.85579 |
| 0.750 | 0.99902 | 0.99833 | 0.99713 | 0.99508 | 0.99156 | 0.98553 | 0.97520 | 0.95748 | 0.92709 | 0.87500 |
| 0.775 | 0.99942 | 0.99897 | 0.99816 | 0.99671 | 0.99413 | 0.98952 | 0.98127 | 0.96655 | 0.94025 | 0.89327 |
| 0.800 | 0.99968 | 0.99940 | 0.99888 | 0.99791 | 0.99609 | 0.99268 | 0.98632 | 0.97442 | 0.95217 | 0.91056 |
| 0.825 | 0.99984 | 0.99968 | 0.99936 | 0.99875 | 0.99753 | 0.99514 | 0.99042 | 0.98113 | 0.96283 | 0.92679 |
| 0.850 | 0.99992 | 0.99984 | 0.99967 | 0.99931 | 0.99855 | 0.99696 | 0.99365 | 0.98672 | 0.97222 | 0.94191 |
| 0.875 | 0.99997 | 0.99993 | 0.99985 | 0.99965 | 0.99922 | 0.99826 | 0.99609 | 0.99123 | 0.98031 | 0.95581 |
| 0.900 | 0.99999 | 0.99998 | 0.99994 | 0.99985 | 0.99964 | 0.99912 | 0.99785 | 0.99472 | 0.98708 | 0.96838 |
| 0.925 | 1.00000 | 0.99999 | 0.99998 | 0.99995 | 0.99987 | 0.99963 | 0.99900 | 0.99726 | 0.99250 | 0.97946 |
| 0.950 | 1.00000 | 1.00000 | 1.00000 | 0.99999 | 0.99997 | 0.99989 | 0.99966 | 0.99891 | 0.99651 | 0.98882 |
| 0.975 | 1.00000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 | 0.99999 | 0.99995 | 0.99978 | 0.99906 | 0.99605 |
| 1.000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 |

**Positive accelerations**

| Time | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---|---|---|---|---|---|---|---|---|---|
| 0.000 | 0.00000 | 0.00000 | 0.00000 | 0.00000 | 0.00000 | 0.00000 | 0.00000 | 0.00000 | 0.00000 | 0.00000 |
| 0.025 | 0.00395 | 0.00108 | 0.00041 | 0.00021 | 0.00014 | 0.00014 | 0.00018 | 0.00031 | 0.00076 | 0.00249 |
| 0.050 | 0.01118 | 0.00385 | 0.00169 | 0.00095 | 0.00069 | 0.00063 | 0.00074 | 0.00112 | 0.00215 | 0.00529 |
| 0.075 | 0.02054 | 0.00805 | 0.00388 | 0.00230 | 0.00168 | 0.00151 | 0.00167 | 0.00226 | 0.00378 | 0.00778 |
| 0.100 | 0.03162 | 0.01359 | 0.00699 | 0.00430 | 0.00316 | 0.00278 | 0.00293 | 0.00369 | 0.00555 | 0.01000 |
| 0.125 | 0.04419 | 0.02039 | 0.01101 | 0.00696 | 0.00515 | 0.00446 | 0.00451 | 0.00535 | 0.00742 | 0.01205 |
| 0.150 | 0.05809 | 0.02840 | 0.01596 | 0.01031 | 0.00766 | 0.00654 | 0.00642 | 0.00724 | 0.00939 | 0.01400 |
| 0.175 | 0.07321 | 0.03759 | 0.02186 | 0.01439 | 0.01073 | 0.00906 | 0.00866 | 0.00937 | 0.01148 | 0.01593 |
| 0.200 | 0.08944 | 0.04793 | 0.02870 | 0.01921 | 0.01438 | 0.01202 | 0.01124 | 0.01174 | 0.01371 | 0.01789 |
| 0.225 | 0.10673 | 0.05939 | 0.03653 | 0.02482 | 0.01864 | 0.01547 | 0.01419 | 0.01438 | 0.01610 | 0.01993 |
| 0.250 | 0.12500 | 0.07198 | 0.04534 | 0.03125 | 0.02356 | 0.01944 | 0.01754 | 0.01731 | 0.01870 | 0.02210 |
| 0.275 | 0.14421 | 0.08567 | 0.05518 | 0.03854 | 0.02918 | 0.02396 | 0.02133 | 0.02058 | 0.02154 | 0.02444 |
| 0.300 | 0.16432 | 0.10046 | 0.06607 | 0.04673 | 0.03555 | 0.02908 | 0.02559 | 0.02422 | 0.02466 | 0.02700 |
| 0.325 | 0.18528 | 0.11636 | 0.07803 | 0.05587 | 0.04271 | 0.03487 | 0.03039 | 0.02829 | 0.02811 | 0.02983 |
| 0.350 | 0.20706 | 0.13335 | 0.09110 | 0.06602 | 0.05074 | 0.04137 | 0.03578 | 0.03283 | 0.03195 | 0.03298 |
| 0.375 | 0.22964 | 0.15145 | 0.10532 | 0.07722 | 0.05970 | 0.04867 | 0.04183 | 0.03791 | 0.03623 | 0.03651 |
| 0.400 | 0.25298 | 0.17066 | 0.12072 | 0.08956 | 0.06967 | 0.05683 | 0.04862 | 0.04361 | 0.04103 | 0.04048 |
| 0.425 | 0.27707 | 0.19098 | 0.13735 | 0.10308 | 0.08072 | 0.06595 | 0.05623 | 0.05002 | 0.04643 | 0.04497 |
| 0.450 | 0.30187 | 0.21242 | 0.15525 | 0.11786 | 0.09294 | 0.07612 | 0.06476 | 0.05722 | 0.05252 | 0.05007 |
| 0.475 | 0.32737 | 0.23499 | 0.17447 | 0.13399 | 0.10644 | 0.08745 | 0.07432 | 0.06533 | 0.05941 | 0.05587 |
| 0.500 | 0.35355 | 0.25871 | 0.19506 | 0.15154 | 0.12131 | 0.10007 | 0.08505 | 0.07448 | 0.06722 | 0.06250 |
| 0.525 | 0.38040 | 0.28358 | 0.21706 | 0.17061 | 0.13768 | 0.11409 | 0.09708 | 0.08482 | 0.07609 | 0.07009 |
| 0.550 | 0.40789 | 0.30962 | 0.24055 | 0.19128 | 0.15568 | 0.12969 | 0.11058 | 0.09650 | 0.08620 | 0.07880 |
| 0.575 | 0.43602 | 0.33684 | 0.26556 | 0.21367 | 0.17544 | 0.14702 | 0.12573 | 0.10972 | 0.09773 | 0.08883 |
| 0.600 | 0.46476 | 0.36526 | 0.29218 | 0.23788 | 0.19712 | 0.16626 | 0.14273 | 0.12471 | 0.11091 | 0.10039 |
| 0.625 | 0.49411 | 0.39490 | 0.32045 | 0.26403 | 0.22089 | 0.18763 | 0.16182 | 0.14171 | 0.12600 | 0.11375 |
| 0.650 | 0.52405 | 0.42577 | 0.35046 | 0.29226 | 0.24691 | 0.21134 | 0.18327 | 0.16101 | 0.14331 | 0.12922 |
| 0.675 | 0.55457 | 0.45790 | 0.38227 | 0.32268 | 0.27540 | 0.23766 | 0.20737 | 0.18294 | 0.16319 | 0.14718 |
| 0.700 | 0.58566 | 0.49130 | 0.41596 | 0.35545 | 0.30657 | 0.26686 | 0.23446 | 0.20790 | 0.18607 | 0.16807 |
| 0.725 | 0.61732 | 0.52599 | 0.45161 | 0.39073 | 0.34065 | 0.29926 | 0.26492 | 0.23632 | 0.21243 | 0.19241 |
| 0.750 | 0.64952 | 0.56200 | 0.48931 | 0.42867 | 0.37789 | 0.33520 | 0.29919 | 0.26872 | 0.24285 | 0.22084 |
| 0.775 | 0.68226 | 0.59935 | 0.52913 | 0.46946 | 0.41858 | 0.37508 | 0.33776 | 0.30567 | 0.27800 | 0.25410 |
| 0.800 | 0.71554 | 0.63806 | 0.57117 | 0.51327 | 0.46302 | 0.41930 | 0.38118 | 0.34787 | 0.31869 | 0.29309 |
| 0.825 | 0.74934 | 0.67816 | 0.61553 | 0.56032 | 0.51154 | 0.46837 | 0.43009 | 0.39609 | 0.36584 | 0.33889 |
| 0.850 | 0.78366 | 0.71968 | 0.66231 | 0.61081 | 0.56449 | 0.52279 | 0.48519 | 0.45125 | 0.42056 | 0.39279 |
| 0.875 | 0.81849 | 0.76263 | 0.71162 | 0.66497 | 0.62228 | 0.58317 | 0.54731 | 0.51439 | 0.48415 | 0.45635 |
| 0.900 | 0.85381 | 0.80706 | 0.76355 | 0.72305 | 0.68532 | 0.65015 | 0.61735 | 0.58674 | 0.55815 | 0.53144 |
| 0.925 | 0.88964 | 0.85297 | 0.81823 | 0.78531 | 0.75409 | 0.72447 | 0.69637 | 0.66970 | 0.64438 | 0.62032 |
| 0.950 | 0.92595 | 0.90042 | 0.87579 | 0.85202 | 0.82908 | 0.80693 | 0.78556 | 0.76491 | 0.74498 | 0.72573 |
| 0.975 | 0.96274 | 0.94941 | 0.93633 | 0.92348 | 0.91085 | 0.89844 | 0.88625 | 0.87428 | 0.86251 | 0.85095 |
| 1.000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 | 1.00000 |

---

## Appendix B. Reference table

What today's Shockwave returns at a handful of cells, for 59 settings spread over the ranges.
Numbers only, exactly as returned: single-precision values printed with the fewest digits that
read back to the same number. `reference.tsv`, beside this file, has the same 352 samples one to a
line.

How to read a row:

- *Buffer*: columns by rows. *Effect ms / frame ms*: the effect's length and the sequence's frame
  time. *At ms*: the time since the effect's start.
- *Palette*: its colours in order, each as red, green, blue out of 255.
- *Settings*: only those that differ from the defaults of section 2.
- *Samples*: `(x, y)` is a cell; the pixel asked for is at `u = x ÷ (C − 1)`, `v = y ÷ (R − 1)`,
  each worked out in single precision (0.5 where there is one column or row). The four numbers
  after it are red, green, blue and alpha.

A sample of `0 0 0 0` is outside the band. A sample with a colour and alpha `0` is a cell inside a
soft band whose alpha has rounded down to nothing; section 9 leaves its colour free.

Rows 1 to 7 are the default effect; 8 to 17 sizes in cells; 18 to 25 sizes in percent (23 is one
of the widths of section 8, item 5, and 24 an ordinary one beside it); 26 to 30 the centre; 31 to
40 time and rings (32 is a time inside a frame; 37 is one of the frames of section 8, item 3; 39
and 40 are section 8, item 2); 41 to 47 acceleration; 48 to 51 cells exactly on an edge (section
8, item 4: unlit in 48, 49 and 50 at cells 10, 16 and 8; lit in 51 at cells 8 and 12); 52 to 59
tiny buffers and lines.

| # | Buffer | Effect ms / frame ms | At ms | Palette | Settings other than the defaults | Samples: cell (x, y) gives red, green, blue, alpha |
|---|---|---|---|---|---|---|
| 1 | 101 x 101 | 1050 / 50 | 0 | 255,0,0 | (defaults) | (50, 50) 1 0 0 0.79607844; (51, 50) 1 0 0 0.8; (48, 48) 1 0 0 0.07058824; (50, 48) 1 0 0 0.4; (49, 51) 1 0 0 0.63529414; (50, 47) 1 0 0 0 |
| 2 | 101 x 101 | 1050 / 50 | 250 | 255,0,0 | (defaults) | (50, 50) 0 0 0 0; (63, 50) 1 0 0 0.99607843; (43, 43) 1 0 0 0.003921569; (46, 36) 1 0 0 0.49803922; (38, 48) 1 0 0 0.7294118; (49, 34) 1 0 0 0.02745098; (48, 34) 1 0 0 0 |
| 3 | 101 x 101 | 1050 / 50 | 500 | 255,0,0 | (defaults) | (50, 50) 0 0 0 0; (75, 45) 1 0 0 0.99607843; (32, 27) 1 0 0 0.011764706; (31, 36) 1 0 0 0.49019608; (61, 28) 1 0 0 0.75686276; (46, 21) 0 0 0 0; (38, 32) 0 0 0 0 |
| 4 | 101 x 101 | 1050 / 50 | 1000 | 255,0,0 | (defaults) | (50, 50) 0 0 0 0; (100, 43) 1 0 0 0.99607843; (26, 0) 1 0 0 0.003921569; (24, 96) 1 0 0 0.5294118; (31, 98) 1 0 0 0.77254903; (24, 1) 1 0 0 0.003921569; (22, 2) 0 0 0 0 |
| 5 | 100 x 100 | 1000 / 50 | 500 | 255,0,0 | (defaults) | (50, 50) 0 0 0 0; (75, 41) 1 0 0 0.99215686; (44, 28) 1 0 0 0.015686275; (37, 29) 1 0 0 0.5137255; (59, 76) 1 0 0 0.74509805; (46, 20) 1 0 0 0.023529412; (41, 21) 1 0 0 0 |
| 6 | 41 x 21 | 1000 / 50 | 450 | 255,0,0 0,255,0 0,0,255 | (defaults) | (20, 10) 0 0 0 0; (29, 6) 0.052631557 0.94736844 0 0.9764706; (15, 0) 0.052631557 0.94736844 0 0.07450981; (30, 7) 0.052631557 0.94736844 0 0.5764706; (30, 12) 0.052631557 0.94736844 0 0.7411765; (12, 2) 0 0 0 0; (18, 2) 0 0 0 0 |
| 7 | 400 x 177 | 900 / 50 | 400 | 255,0,0 | (defaults) | (200, 88) 0 0 0 0; (295, 83) 1 0 0 0.99607843; (133, 1) 1 0 0 0.003921569; (259, 153) 1 0 0 0.49411765; (231, 2) 1 0 0 0.7411765; (108, 28) 1 0 0 0; (180, 10) 1 0 0 0 |
| 8 | 101 x 101 | 1050 / 50 | 0 | 255,0,0 | scale off, blendEdges off, startRadius 30, endRadius 30, startWidth 4, endWidth 4 | (50, 50) 0 0 0 0; (78, 50) 1 0 0 1; (77, 50) 0 0 0 0; (82, 50) 1 0 0 1; (83, 50) 0 0 0 0; (50, 18) 1 0 0 1; (68, 74) 1 0 0 1; (69, 75) 1 0 0 1 |
| 9 | 101 x 101 | 1050 / 50 | 0 | 255,0,0 | scale off, blendEdges on, startRadius 30, endRadius 30, startWidth 20, endWidth 20 | (80, 50) 1 0 0 1; (75, 50) 1 0 0 0.49803922; (71, 50) 1 0 0 0.09803922; (70, 50) 1 0 0 0; (89, 50) 1 0 0 0.09803922; (90, 50) 1 0 0 0; (68, 74) 1 0 0 1; (50, 23) 1 0 0 0.69803923 |
| 10 | 101 x 101 | 1050 / 50 | 0 | 255,0,0 | scale off, blendEdges on, startRadius 30, endRadius 30, startWidth 0, endWidth 0 | (80, 50) 1 0 0 1; (68, 74) 1 0 0 1; (79, 57) 1 0 0 0.32941177; (80, 51) 1 0 0 0.93333334; (80, 52) 1 0 0 0.73333335 |
| 11 | 101 x 101 | 1050 / 50 | 0 | 255,0,0 | scale off, blendEdges off, startRadius 30, endRadius 30, startWidth 1, endWidth 1 | (80, 50) 1 0 0 1; (79, 57) 1 0 0 1; (80, 51) 1 0 0 1; (80, 53) 1 0 0 1; (80, 54) 1 0 0 1 |
| 12 | 101 x 101 | 1050 / 50 | 500 | 255,0,0 0,255,0 0,0,255 | scale off, blendEdges off, startRadius 0, endRadius 20, startWidth 4, endWidth 4 | (50, 50) 0 0 0 0; (58, 50) 0 1 0 1; (57, 50) 0 0 0 0; (62, 50) 0 1 0 1; (63, 50) 0 0 0 0; (56, 58) 0 1 0 1 |
| 13 | 101 x 101 | 1050 / 50 | 350 | 255,0,0 0,255,0 0,0,255 | scale off, blendEdges on, startRadius 1, endRadius 100, startWidth 5, endWidth 59 | (50, 50) 0 0 0 0; (85, 43) 0.3 0.7 0 0.99607843; (43, 3) 0.3 0.7 0 0.003921569; (25, 34) 0.3 0.7 0 0.49803922; (88, 43) 0.3 0.7 0 0.7490196; (38, 4) 0.3 0.7 0 0.003921569; (20, 13) 0 0 0 0 |
| 14 | 101 x 101 | 1050 / 50 | 700 | 255,128,0 0,128,255 | scale off, blendEdges on, startRadius 60, endRadius 5, startWidth 30, endWidth 2 | (50, 50) 0 0 0 0; (70, 42) 0.3 0.5019608 0.7 0.99215686; (35, 28) 0.3 0.5019608 0.7 0.011764706; (33, 33) 0.3 0.5019608 0.7 0.50980395; (68, 36) 0.3 0.5019608 0.7 0.7490196; (44, 24) 0.3 0.5019608 0.7 0; (47, 34) 0 0 0 0 |
| 15 | 101 x 101 | 1050 / 50 | 1000 | 255,128,0 0,128,255 | scale off, blendEdges off, startRadius 40, endRadius 0, startWidth 10, endWidth 0 | (50, 50) 0 0.5019608 1 1; (51, 50) 0 0 0 0; (50, 51) 0 0 0 0; (49, 49) 0 0 0 0 |
| 16 | 201 x 201 | 3600 / 50 | 1800 | 255,0,0 | scale off, blendEdges on, startRadius 1, endRadius 196, startWidth 5, endWidth 200, acceleration -7 | (100, 100) 0 0 0 0; (200, 0) 1 0 0 0.5529412; (94, 10) 1 0 0 0.003921569; (59, 194) 1 0 0 0.13725491; (24, 16) 1 0 0 0.2509804; (0, 0) 1 0 0 0.5529412; (49, 26) 1 0 0 0 |
| 17 | 1501 x 1 | 1050 / 50 | 1000 | 255,0,0 | scale off, blendEdges off, centerX 0, startRadius 0, endRadius 750, startWidth 255, endWidth 255 | (622, 0) 0 0 0 0; (623, 0) 1 0 0 1; (750, 0) 1 0 0 1; (877, 0) 1 0 0 1; (878, 0) 0 0 0 0 |
| 18 | 101 x 101 | 1050 / 50 | 0 | 255,0,0 | blendEdges off, startRadius 50, endRadius 50, startWidth 10, endWidth 10 | (70, 50) 0 0 0 0; (71, 50) 1 0 0 1; (80, 50) 1 0 0 1; (81, 50) 0 0 0 0; (75, 50) 1 0 0 1; (50, 50) 0 0 0 0 |
| 19 | 201 x 81 | 1050 / 50 | 0 | 255,0,0 | blendEdges on, startRadius 40, endRadius 40, startWidth 10, endWidth 10 | (100, 40) 0 0 0 0; (140, 36) 1 0 0 0.99607843; (96, 10) 1 0 0 0.003921569; (70, 21) 1 0 0 0.5294118; (130, 70) 1 0 0 0.7764706; (54, 20) 1 0 0 0.003921569; (65, 4) 0 0 0 0 |
| 20 | 81 x 201 | 1050 / 50 | 0 | 255,0,0 | blendEdges on, startRadius 40, endRadius 40, startWidth 10, endWidth 10 | (40, 100) 0 0 0 0; (80, 96) 1 0 0 0.99607843; (36, 50) 1 0 0 0.003921569; (10, 81) 1 0 0 0.5294118; (34, 142) 1 0 0 0.7764706; (5, 64) 0 0 0 0; (23, 75) 1 0 0 0 |
| 21 | 101 x 1 | 1050 / 50 | 0 | 255,0,0 | blendEdges off, centerX 0, startRadius 50, endRadius 50, startWidth 2.97, endWidth 2.97 | (24, 0) 0 0 0 0; (25, 0) 1 0 0 1; (26, 0) 1 0 0 1; (27, 0) 0 0 0 0 |
| 22 | 101 x 1 | 1050 / 50 | 0 | 255,0,0 | blendEdges off, centerX 0, startRadius 50, endRadius 50, startWidth 3, endWidth 3 | (23, 0) 0 0 0 0; (24, 0) 1 0 0 1; (26, 0) 1 0 0 1; (27, 0) 0 0 0 0 |
| 23 | 58 x 58 | 1050 / 50 | 0 | 255,0,0 | blendEdges off, startRadius 0, endRadius 0, startWidth 50, endWidth 50 | (43, 29) 1 0 0 1; (44, 29) 0 0 0 0; (29, 43) 1 0 0 1; (39, 39) 0 0 0 0; (40, 39) 0 0 0 0 |
| 24 | 60 x 60 | 1050 / 50 | 0 | 255,0,0 | blendEdges off, startRadius 0, endRadius 0, startWidth 50, endWidth 50 | (44, 30) 1 0 0 1; (45, 30) 1 0 0 1; (46, 30) 0 0 0 0; (30, 15) 1 0 0 1 |
| 25 | 41 x 41 | 1050 / 50 | 500 | 255,0,0 0,255,0 0,0,255 | blendEdges on, startRadius 10, endRadius 150, startWidth 20, endWidth 5 | (20, 20) 0 0 0 0; (33, 10) 0 1 0 0.99607843; (15, 7) 0 1 0 0.007843138; (8, 33) 0 1 0 0.48235294; (33, 31) 0 1 0 0.74509805; (10, 4) 0 1 0 0.011764706; (14, 2) 0 0 0 0 |
| 26 | 101 x 101 | 1050 / 50 | 0 | 255,0,0 | scale off, blendEdges off, centerX 25, centerY 75, startRadius 10, endRadius 10, startWidth 2, endWidth 2 | (25, 75) 0 0 0 0; (35, 75) 1 0 0 1; (36, 75) 1 0 0 1; (34, 75) 1 0 0 1; (25, 64) 1 0 0 1; (19, 83) 1 0 0 1 |
| 27 | 100 x 100 | 1050 / 50 | 0 | 255,0,0 | scale off, blendEdges off, centerX 0, centerY 0, startRadius 10, endRadius 10, startWidth 2, endWidth 2 | (0, 0) 0 0 0 0; (9, 0) 1 0 0 1; (10, 0) 1 0 0 1; (11, 0) 1 0 0 1; (12, 0) 0 0 0 0; (6, 8) 1 0 0 1 |
| 28 | 40 x 20 | 1050 / 50 | 0 | 255,0,0 | scale off, blendEdges off, centerX 100, centerY 100, startRadius 3, endRadius 3, startWidth 2, endWidth 2 | (39, 19) 0 0 0 0; (38, 19) 1 0 0 1; (37, 17) 0 0 0 0; (36, 19) 0 0 0 0; (39, 16) 0 0 0 0; (38, 17) 1 0 0 1 |
| 29 | 41 x 21 | 1050 / 50 | 0 | 255,0,0 | scale off, blendEdges on, centerX 33.7, centerY 66.2, startRadius 5, endRadius 5, startWidth 6, endWidth 6 | (20, 10) 1 0 0 0.1254902; (17, 10) 1 0 0 1; (8, 7) 1 0 0 0.0627451; (8, 17) 1 0 0 0.5294118; (12, 17) 1 0 0 0.7058824; (7, 18) 1 0 0 0.0627451; (5, 7) 0 0 0 0 |
| 30 | 7 x 5 | 1050 / 50 | 0 | 255,0,0 | scale off, blendEdges off, centerX 99, centerY 99, startRadius 0, endRadius 0, startWidth 0, endWidth 0 | (6, 4) 1 0 0 1; (5, 4) 0 0 0 0; (6, 3) 0 0 0 0 |
| 31 | 101 x 101 | 1000 / 50 | 500 | 255,0,0 0,255,0 0,0,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8 | (50, 50) 0 0 0 0; (71, 49) 0 0.9473684 0.052631617 0.99215686; (49, 25) 0 0.9473684 0.052631617 0.007843138; (27, 49) 0 0.9473684 0.052631617 0.5058824; (67, 36) 0 0.9473684 0.052631617 0.75686276; (38, 28) 0 0 0 0; (49, 33) 0 0 0 0 |
| 32 | 101 x 101 | 1000 / 50 | 524 | 255,0,0 0,255,0 0,0,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8 | (50, 50) 0 0 0 0; (71, 49) 0 0.9473684 0.052631617 0.99215686; (49, 25) 0 0.9473684 0.052631617 0.007843138; (27, 49) 0 0.9473684 0.052631617 0.5058824; (67, 36) 0 0.9473684 0.052631617 0.75686276; (38, 28) 0 0 0 0; (49, 33) 0 0 0 0 |
| 33 | 101 x 101 | 975 / 50 | 950 | 255,0,0 0,255,0 0,0,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8 | (50, 50) 0 0 0 0; (90, 50) 0 0 1 1; (37, 8) 0 0 1 0.007843138; (88, 50) 0 0 1 0.49803922; (27, 84) 0 0 1 0.7372549; (50, 6) 0 0 1 0; (49, 14) 0 0 1 0 |
| 34 | 101 x 101 | 1000 / 25 | 500 | 255,0,0 0,255,0 0,0,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8 | (50, 50) 0 0 0 0; (65, 36) 0 0.9743589 0.025641084 0.99607843; (43, 35) 0 0.9743589 0.025641084 0.007843138; (42, 29) 0 0.9743589 0.025641084 0.50980395; (40, 31) 0 0.9743589 0.025641084 0.75686276; (36, 30) 0 0.9743589 0.025641084 0.023529412; (45, 26) 0 0 0 0 |
| 35 | 101 x 101 | 1000 / 50 | 300 | 255,0,0 0,255,0 0,0,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8, cycles 3 | (50, 50) 0 0 0 0; (90, 50) 0 0 1 1; (37, 8) 0 0 1 0.007843138; (88, 50) 0 0 1 0.49803922; (27, 84) 0 0 1 0.7372549; (50, 6) 0 0 1 0; (49, 14) 0 0 1 0 |
| 36 | 101 x 101 | 1000 / 50 | 350 | 255,0,0 0,255,0 0,0,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8, cycles 3 | (50, 50) 0.88235295 0.11764706 0 0.4117647; (52, 49) 0.88235295 0.11764706 0 0.96862745; (48, 44) 0.88235295 0.11764706 0 0.003921569; (54, 48) 0.88235295 0.11764706 0 0.46666667; (52, 53) 0.88235295 0.11764706 0 0.6862745; (46, 45) 0 0 0 0 |
| 37 | 101 x 101 | 1000 / 50 | 500 | 255,0,0 0,255,0 0,0,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8, cycles 6 | (50, 50) 0 0 0 0; (90, 50) 0 0 1 1; (37, 8) 0 0 1 0.007843138; (88, 50) 0 0 1 0.49803922; (27, 84) 0 0 1 0.7372549; (50, 6) 0 0 1 0; (49, 14) 0 0 1 0 |
| 38 | 101 x 101 | 1000 / 50 | 500 | 255,0,0 0,255,0 0,0,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8, cycles 15 | (50, 50) 0 0 0 0; (90, 50) 0 0 1 1; (37, 8) 0 0 1 0.007843138; (88, 50) 0 0 1 0.49803922; (27, 84) 0 0 1 0.7372549; (50, 6) 0 0 1 0; (49, 14) 0 0 1 0 |
| 39 | 101 x 101 | 1000 / 50 | 500 | 255,0,0 0,255,0 0,0,255 | scale off, blendEdges on, startRadius 10, endRadius 40, startWidth 8, endWidth 8, cycles 20 | (50, 50) 0 0 0 0; (60, 50) 1 0 0 1; (45, 37) 1 0 0 0.015686275; (41, 58) 1 0 0 0.4862745; (61, 49) 1 0 0 0.7372549; (50, 36) 1 0 0 0; (50, 44) 1 0 0 0 |
| 40 | 101 x 101 | 1000 / 50 | 500 | 255,0,0 0,255,0 0,0,255 | scale off, blendEdges on, startRadius 10, endRadius 40, startWidth 8, endWidth 8, cycles 19 | (50, 50) 0 0 0 0; (90, 50) 0 0 1 1; (37, 8) 0 0 1 0.007843138; (88, 50) 0 0 1 0.49803922; (27, 84) 0 0 1 0.7372549; (50, 6) 0 0 1 0; (49, 14) 0 0 1 0 |
| 41 | 101 x 101 | 1050 / 50 | 500 | 255,128,0 0,128,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8, acceleration -10 | (50, 50) 0 0 0 0; (88, 42) 0.03125 0.5019608 0.96875 0.9764706; (38, 9) 0.03125 0.5019608 0.96875 0.003921569; (18, 68) 0.03125 0.5019608 0.96875 0.49019608; (14, 67) 0.03125 0.5019608 0.96875 0.73333335; (42, 8) 0 0 0 0; (43, 16) 0 0 0 0 |
| 42 | 101 x 101 | 1050 / 50 | 500 | 255,128,0 0,128,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8, acceleration -5 | (50, 50) 0 0 0 0; (84, 41) 0.120278 0.5019608 0.879722 0.99215686; (30, 26) 0.120278 0.5019608 0.879722 0.011764706; (41, 18) 0.120278 0.5019608 0.879722 0.50980395; (82, 38) 0.120278 0.5019608 0.879722 0.74509805; (47, 11) 0.120278 0.5019608 0.879722 0.015686275; (46, 11) 0 0 0 0 |
| 43 | 101 x 101 | 1050 / 50 | 500 | 255,128,0 0,128,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8, acceleration -1 | (50, 50) 0 0 0 0; (71, 35) 0.3535534 0.5019608 0.6464466 0.9843137; (43, 21) 0.3535534 0.5019608 0.6464466 0.003921569; (30, 37) 0.3535534 0.5019608 0.6464466 0.49803922; (32, 30) 0.3535534 0.5019608 0.6464466 0.7372549; (37, 23) 0 0 0 0; (44, 29) 0 0 0 0 |
| 44 | 101 x 101 | 1050 / 50 | 500 | 255,128,0 0,128,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8, acceleration 1 | (50, 50) 0 0 0 0; (64, 48) 0.6464466 0.5019608 0.35355338 1; (48, 32) 0.6464466 0.5019608 0.35355338 0.003921569; (44, 35) 0.6464466 0.5019608 0.35355338 0.49411765; (44, 36) 0.6464466 0.5019608 0.35355338 0.7254902; (47, 32) 0 0 0 0; (49, 40) 0 0 0 0 |
| 45 | 101 x 101 | 1050 / 50 | 500 | 255,128,0 0,128,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8, acceleration 5 | (50, 50) 0 0 0 0; (55, 50) 0.87868834 0.5019608 0.12131166 0.9607843; (50, 49) 0.87868834 0.5019608 0.12131166 0.03529412; (47, 44) 0.87868834 0.5019608 0.12131166 0.53333336; (45, 53) 0.87868834 0.5019608 0.12131166 0.7529412; (45, 43) 0.87868834 0.5019608 0.12131166 0.05882353; (46, 42) 0 0 0 0 |
| 46 | 101 x 101 | 1050 / 50 | 800 | 255,128,0 0,128,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8, acceleration 10 | (50, 50) 0 0 0 0; (61, 46) 0.70691407 0.5019608 0.2930859 0.99215686; (43, 36) 0.70691407 0.5019608 0.2930859 0.015686275; (46, 41) 0.70691407 0.5019608 0.2930859 0.5294118; (46, 40) 0.70691407 0.5019608 0.2930859 0.7607843; (45, 35) 0 0 0 0; (47, 43) 0 0 0 0 |
| 47 | 101 x 101 | 1050 / 50 | 800 | 255,128,0 0,128,255 | scale off, blendEdges on, startRadius 0, endRadius 40, startWidth 8, endWidth 8, acceleration 3.9 | (50, 50) 0 0 0 0; (71, 41) 0.4288268 0.5019608 0.5711732 0.99607843; (40, 34) 0.4288268 0.5019608 0.5711732 0.003921569; (44, 70) 0.4288268 0.5019608 0.5711732 0.5058824; (66, 35) 0.4288268 0.5019608 0.5711732 0.76862746; (44, 24) 0.4288268 0.5019608 0.5711732 0.039215688; (38, 26) 0.4288268 0.5019608 0.5711732 0 |
| 48 | 141 x 1 | 200 / 50 | 50 | 255,0,0 | scale off, blendEdges off, centerX 0, startRadius 0, endRadius 20, startWidth 5, endWidth 10 | (3, 0) 0 0 0 0; (4, 0) 1 0 0 1; (9, 0) 1 0 0 1; (10, 0) 0 0 0 0; (11, 0) 0 0 0 0 |
| 49 | 141 x 1 | 200 / 50 | 100 | 255,0,0 | scale off, blendEdges off, centerX 0, startRadius 0, endRadius 20, startWidth 0, endWidth 8 | (10, 0) 0 0 0 0; (11, 0) 1 0 0 1; (15, 0) 1 0 0 1; (16, 0) 0 0 0 0; (17, 0) 0 0 0 0 |
| 50 | 141 x 1 | 350 / 50 | 100 | 255,0,0 | scale off, blendEdges off, centerX 0, startRadius 0, endRadius 20, startWidth 0, endWidth 8 | (5, 0) 0 0 0 0; (6, 0) 1 0 0 1; (7, 0) 1 0 0 1; (8, 0) 0 0 0 0; (9, 0) 0 0 0 0 |
| 51 | 141 x 1 | 1050 / 50 | 500 | 255,0,0 | scale off, blendEdges off, centerX 0, startRadius 0, endRadius 20, startWidth 4, endWidth 4 | (7, 0) 0 0 0 0; (8, 0) 1 0 0 1; (12, 0) 1 0 0 1; (13, 0) 0 0 0 0 |
| 52 | 1 x 1 | 1000 / 50 | 0 | 255,0,0 | (defaults) | (0, 0) 1 0 0 0.9764706 |
| 53 | 1 x 1 | 1000 / 50 | 250 | 255,0,0 | (defaults) | (0, 0) 1 0 0 0.45882353 |
| 54 | 2 x 2 | 1000 / 50 | 950 | 255,0,0 | (defaults) | (0, 0) 0 0 0 0; (1, 0) 1 0 0 1; (0, 1) 1 0 0 1; (1, 1) 0 0 0 0 |
| 55 | 3 x 3 | 1000 / 50 | 750 | 255,0,0 | (defaults) | (0, 0) 1 0 0 0.09019608; (1, 0) 1 0 0 0.24705882; (1, 1) 0 0 0 0; (2, 1) 1 0 0 0.24705882; (2, 2) 1 0 0 0.09019608 |
| 56 | 41 x 1 | 1000 / 50 | 350 | 255,0,0 | blendEdges off | (11, 0) 1 0 0 1; (12, 0) 1 0 0 1; (13, 0) 1 0 0 1; (14, 0) 0 0 0 0; (20, 0) 0 0 0 0; (27, 0) 1 0 0 1; (29, 0) 1 0 0 1; (30, 0) 0 0 0 0 |
| 57 | 1 x 41 | 1000 / 50 | 350 | 255,0,0 | blendEdges off | (0, 11) 1 0 0 1; (0, 13) 1 0 0 1; (0, 20) 0 0 0 0; (0, 29) 1 0 0 1; (0, 30) 0 0 0 0 |
| 58 | 41 x 1 | 1000 / 50 | 350 | 255,0,0 | scale off, blendEdges off, startRadius 0, endRadius 20, startWidth 0, endWidth 0 | (12, 0) 0 0 0 0; (13, 0) 0 0 0 0; (20, 0) 0 0 0 0; (27, 0) 0 0 0 0; (28, 0) 0 0 0 0 |
| 59 | 41 x 1 | 1000 / 50 | 150 | 255,0,0 | scale off, blendEdges off, startRadius 0, endRadius 20, startWidth 0, endWidth 0 | (16, 0) 0 0 0 0; (17, 0) 1 0 0 1; (23, 0) 1 0 0 1; (24, 0) 0 0 0 0 |

---

## Appendix C. The companion files

Tab-separated, one header line, numbers only. Each was measured on today's Shockwave through the
renderer's public interface.

- **`acceleration.tsv`**: appendix A in full. `time` is the share of the ring's time that has
  passed, in thousandths; the 21 columns after it are the progress shown at accelerations −10
  to 10.
- **`reference.tsv`**: appendix B, one sample to a line: the row number, the buffer's columns and
  rows, the effect's length and the frame time in ms, the time in ms, the palette (colours
  separated by semicolons), the settings other than the defaults, the cell `x` and `y`, and the
  red, green, blue and alpha returned.
- **`edges.tsv`**: section 8, item 4. 24,595 cases in which the settings and the frame put an
  edge of a hard ring exactly on a whole number of cells. Each was measured on a row of 141 cells
  with `centerX` 0 (so a cell's column is its distance), in cells, hard, with 50 ms frames, in
  an effect `frames` frames long, on frame `frame`. `edge` says which edge falls on the cell,
  `cell` is the cell, and `lit` is 1 if it is lit and 0 if not.
- **`restarts.tsv`**: section 8, item 3. 23,973 frames on which a ring's share begins exactly,
  in effects of 2 to 240 frames with 2 to 100 rings whose share is not a whole number of frames.
  `progress_shown` is the progress read off the colour on frame `frame`: 1 where the old ring's
  end is shown, 0 (or a number under a million-millionth) where the new ring's start is.
- **`widths.tsv`**: section 8, item 5. 489 pairs of a longer side (2 to 130 cells) and a width in
  percent (1 to 130) whose product is a whole number of cells: that number, and the number of
  cells the band is drawn.

---

## After building it

The new Shockwave was written from this description alone and reproduces every number in it
exactly: all 352 reference samples, all 24,595 exact-edge cases, all 23,973 restarts, all 489
percent widths and the whole acceleration table. All 30 properties are tested as written.

Old and new were then drawn side by side: 303,780,289 frames, 113,932,196,731 pixel-frames, over
every centre setting on every buffer size from 1 to 400, every size in percent on odd and even
buffers, every setting through and just outside its range, a million random combinations, every
frame of effects from 1 to 401 frames long, and buffers from one cell to above 1,048,576. They are
the same to the bit everywhere a show can reach. They differ only on a single row or column of at
least 439,804,673 cells, where the old effect's centre overflowed and the new one's does not.

What the description left open, and what turned out:

- **The centre cell** (section 2). The table could not tell exact rounding down from other ways of
  working the percent out. Exact rounding down is what the old effect drew, on every setting and
  size.
- **Cells exactly on an edge** (section 13, item 1). No rule was given. The plainest sums give
  every one of the measured cases: radius and width each as start plus travel times progress, and
  a cell lit when its distance from the ring's middle is no more than half the width.
- **Section 12's ranges are too narrow.** Five runs understate how far a short Shockwave's look
  moves between runs with nothing changed; over 195 earlier scorecards several instances span 20
  points or more. Over 28 runs the new effect sits inside the old one's spread.

It costs 3.65 ns a pixel where the old one cost 5.4 (about 0.3 ns over a plain fill, where it was
1.5), and requests no memory. A pixel off the ring is answered from two whole numbers worked out
once a frame, with no square root.

