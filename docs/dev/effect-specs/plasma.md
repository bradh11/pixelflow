# Plasma: behaviour specification

This is the description `crates/pf-render/src/plasma.rs` was built from, kept as it was written.
"Today's Plasma" below means the one it replaced. A fixed sample of the reference table is kept
beside the effect's tests (`crates/pf-render/tests/data/plasma_reference.tsv`). What building and
checking showed is in the last section.


This document describes what the Plasma effect shows, so that it can be built afresh from this
description alone. It says what a viewer can see and what a test can measure on a rendered frame.
How any of it is computed is the implementer's choice, with one exception: in two places
(sections 6.1 and 6.2) the rounding of a short piece of arithmetic decides which cell a pixel
belongs to, and there the arithmetic is spelled out.

**The rule for this effect: the output stays the same.** "Today's picture" below is the Plasma in
PixelFlow at `develop` `b14844e`. The new Plasma has to return the same colour as today's for
every setting, palette, buffer, position and moment, to within 1% of full brightness, with the
exceptions listed in section 10 and nowhere else. That includes today's habits, even the ones
that look like defects: those are collected in section 9 so the owner can decide about each, and
until the owner says otherwise each one is **kept**.

Everything here was measured on frames drawn through the renderer's public interface, not taken
from reading. The picture is a smooth field with no edge or arm whose position could be put in a
sentence, so section 3 says what the field is, the way a measured signal is characterised: which
waves it is made of, where each points, how long it is, how fast it travels, how strong it is,
and how they add up. Each number in section 3 came out of a fit to rendered frames. Section 15
says how, and how closely the result reproduces the frames: over 480,000 random samples, to
0.03% of full brightness while the clock of section 4 is below 100 and to 0.12% below 1,000,
with palettes of up to eight colours (section 10 gives the worst case). And a build made from
this document alone, by someone who had seen nothing else, matched every row of the reference
table at the first attempt.

Sequences imported from xLights carry their Plasma settings over, number for number, and
xLights' own rendered frames agree with today's picture wherever they could be compared (section
12). So the meaning of each number below is also what keeps an imported show looking the same.

A table of exact values to check a new Plasma against is in `reference.tsv` beside this file
(section 14).

---

## 0. Conventions

**Buffer.** The effect is asked to draw on a buffer described as `C` columns by `R` rows. It is
then asked, pixel by pixel, for a colour. Each pixel has a position `(u, v)`: `u` runs from 0 at
the left to 1 at the right, `v` from 0 at the bottom to 1 at the top, as seen from the front of
the display. Pixels need not sit on a regular grid, there may be far fewer of them than `C × R`,
and several may share a position. `u` and `v` arrive as single-precision numbers; `C` and `R` as
whole numbers that are never negative.

**Cell.** The buffer is a grid of cells, `C` wide and `R` high, counted from 0 at the left and at
the bottom. Every pixel belongs to one cell (section 6 says which) and shows that cell's colour.

**Place on the picture, `(x, y)`.** The picture is laid over the buffer so that the left column
is `x = 0`, the right column `x = 1`, the bottom row `y = 0` and the top row `y = 1`, whatever
the buffer's proportions. Cell `i` of `C` columns is at `x = i ÷ (C − 1)`; cell `j` of `R` rows
is at `y = j ÷ (R − 1)`. With one column `x = 0`; with one row `y = 0`.

**Span.** The unit of length on the picture: its full width, from the first column's place to
the last's (`C − 1` steps from cell to cell), or its full height (`R − 1` steps). Both count as
1, so a length "0.5 span" is half the width when it lies along `x` and half the height when it
lies along `y`.

**Frames elapsed, `n`.** The whole number of frames since the effect started: the elapsed
milliseconds divided by the sequence's frame time, fractions dropped. Both arrive as whole
numbers of milliseconds that are never negative. The effect's first frame has `n = 0`.

**The clock, `τ`.** The one number that moves the picture (section 4). It is an angle in
radians, and grows by a fixed step, a *tick*, every frame.

**Phase.** Every wave is the sine of an angle called its phase, given in radians. A wave swings
between −1 and +1.

**Colour angle, `θ`.** What the summed waves become before they are turned into colour, given in
degrees (section 5). Only `θ` modulo 360° matters.

**Raised sine.** `rs(a) = (1 + sin a) ÷ 2`: a sine lifted to run from 0 to 1. `rs` is ½ at 0°,
1 at 90°, ½ at 180° and 0 at 270°.

**Colour and alpha.** Red, green and blue are numbers from 0 to 1. Alpha is how much of a pixel
the effect covers, 0 to 1; colour is not darkened by alpha. Plasma's alpha is always 1.

**Colouring.** One of the five choices of the `colors` setting (section 5).

**Brightness**, when measuring: the red channel of the white colouring (all three channels
are equal there).

---

## 1. What a viewer sees

### At default settings, with a palette of a few colours

The whole buffer is lit, every pixel, all the time. It is filled with soft patches and bands of
the palette's colours that melt into one another with no edges anywhere, like dye in slowly
stirred water. About two bright patches fit side by side on the buffer and one or two up it.
They swell, merge, split and slide, mostly drifting to the left, slowly: a given pixel passes
through the palette and back once every 20 to 30 seconds (at 20 frames a second). The picture is
complete from the very first frame and never settles into a loop a viewer would notice.

The picture is stretched over the buffer whatever its shape or size: a wide, low buffer shows the
same picture squashed, and a buffer with ten times the pixels shows the same picture sharper.

### As the settings move

- **Line density up**: the same patches, cut into more and thinner bands of colour, like the
  contour lines of a map. At 1 about two bright bands cross the buffer; at 10 about seventeen
  do, and a pixel runs through its colours ten times as often.
- **Twist up**: more of the picture is made of rings and arcs, and they sit closer together.
- **Speed**: from a slow drift to a boil. Up to about 50 the picture flows calmly. In the
  nineties it churns. At 99 and 100 each frame is very different from the last: the picture
  flickers rather than flows, and with a high line density it is plain noise.
  Speed 0 is the slowest drift, not a standstill.
- **Colors**: the bands take their colours from the palette, blended from the first colour
  through to the last and back; or from one of four fixed sets: red with green, blue with green,
  the full colour wheel, or greys from black to white.

### One colour

With a single palette colour there is nothing to blend between: the whole buffer is that one
colour, steady and unmoving. (A new effect's palette is one plain white, so a new Plasma at its
defaults is a steady white fill until a second colour is added. Section 9, item 1.)

---

## 2. Settings

Through the renderer, values reach the effect already pulled into their ranges: a number outside
its range has become the nearest end of it, and a speed that is not a number has become the
default. That pulling-in is the renderer's and is not part of this rewrite; what the effect
itself does with a value outside its range, should it ever be handed one, is in section 7.
`twist` and `density` are whole-number settings: a value that is not whole cannot arrive.

Keys are the names in the saved file. An xLights import fills each one with the number from the
xLights file, unchanged (the table's last column).

| Key | Type | Range | Default | Label | From xLights |
|---|---|---|---|---|---|
| `colors` | choice | `palette`, `redGreen`, `blueGreen`, `rainbow`, `white` | `palette` | Colors | `Plasma_Color`: Normal, Preset Colors 1, 2, 3, 4, in that order |
| `twist` | whole number | 1 to 10 | 1 | Twist | `Plasma_Style` |
| `density` | whole number | 1 to 10 | 1 | Line density | `Plasma_Line_Density` |
| `speed` | number | 0 to 100 | 10 | Speed | `Plasma_Speed` |

### `colors`

Which colours the picture is painted in (section 5). The labels are "The palette", "Red and
green", "Blue and green", "Rainbow", "Shades of white". Changing it recolours the picture and
changes nothing else: the same places are at the same point of their colour cycle at the same
moment in every choice.

### `twist`

Tightens one of the seven waves, the *twist rings* (section 3). Their spacing, far from their
centre, is `0.889 ÷ √twist` spans:

| `twist` | 1 | 2 | 3 | 4 | 5 | 10 |
|---|---|---|---|---|---|---|
| Ring spacing, spans | 0.889 | 0.628 | 0.513 | 0.444 | 0.397 | 0.281 |

Measured as the number of times brightness crosses one half along a line through the middle of
the buffer (white colouring, density 1, speed 10, averaged over frames 0, 200, … 5800):

| `twist` | 1 | 2 | 4 | 10 |
|---|---|---|---|---|
| Along the middle row | 3.9 | 3.8 | 4.8 | 5.6 |
| Along the middle column | 2.3 | 3.0 | 3.3 | 4.9 |
| Along the rising diagonal | 5.6 | 5.9 | 6.6 | 9.5 |

The other six waves do not change with twist. Twist has no effect on a picture that has run for a
very long time at a high speed (section 4, "The twist rings age").

### `density`

Multiplies the colour angle: the colours go round their cycle `density` times for every once at
density 1, everywhere and always. Nothing about the waves changes; only how many colour bands
they are cut into. The same measurement as above, at twist 1:

| `density` | 1 | 2 | 3 | 5 | 10 |
|---|---|---|---|---|---|
| Along the middle row | 3.9 | 7.0 | 10.4 | 17.5 | 34.8 |
| Along the middle column | 2.3 | 4.9 | 7.1 | 11.8 | 23.8 |
| Along the rising diagonal | 5.6 | 10.9 | 16.7 | 26.8 | 53.3 |

Two crossings make one bright band. Because the colours move as well as multiply, a pixel also
cycles `density` times as often. Counting how often brightness rises through one half at the 25
positions with `u` and `v` each 0, ¼, ½, ¾ or 1, over frames 0 to 4,000 at speed 10: 2.1 times
in 1,000 frames for each pixel at density 1, 4.2 at density 2 and 21 at density 10. (The count
depends on the stretch of the effect it is taken over: between 1.5 and 2.1 at density 1.)

### `speed`

How far the clock moves each frame. Only the whole part of the number counts: 10, 10.5 and 10.999
are the same speed, and 11 is the next. At speed `s` the clock takes `303 − 3 × s` frames to
advance by one radian.

| `speed` | Frames to a radian | Seconds for the *width ripple* to move one of its own wavelengths, at 20 frames a second | Mean change in brightness from one frame to the next (density 1) |
|---|---|---|---|
| 0 | 303 | 95.2 | 0.0031 |
| 10 | 273 | 85.8 | 0.0034 |
| 25 | 228 | 71.6 | 0.0047 |
| 44 | 171 | 53.7 | 0.0064 |
| 50 | 153 | 48.1 | 0.0068 |
| 75 | 78 | 24.5 | 0.013 |
| 90 | 33 | 10.4 | 0.032 |
| 95 | 18 | 5.7 | 0.057 |
| 99 | 6 | 1.9 | 0.16 |
| 100 | 3 | 0.94 | 0.28 |

The scale is far from even. Speed 50 is only twice as fast as speed 0; speed 90 is nine times;
and each of the last three steps is a large jump (97, 98, 99, 100 take 12, 9, 6 and 3 frames to a
radian). Speed 0 still moves, at a hundred-and-first of the pace of speed 100.

The pace is counted in frames, not in seconds: the same effect in a sequence with 25 ms frames
runs twice as fast as in one with 50 ms frames (section 4).

---

## 3. The picture, as measured: seven waves

At every cell and every frame the picture is one number, the **sum** `Σ` of seven waves. Each
wave is the sine of its phase, each has the same strength (it swings between −1 and +1), and they
are simply added, so `Σ` lies between −7 and +7. The sum is then turned into colour (section 5).
Nothing else is in the picture: no noise, no randomness, no memory of earlier frames.

The waves are of three kinds. `x`, `y` are the cell's place on the picture (section 0), `τ` is
the clock (section 4), and all phases are in radians.

### Four swells: long, slow waves, far longer than the buffer

Less than a quarter of a wavelength of any of them fits on the buffer, so each one shows as a
gentle slope of brightness over the whole picture that rises and falls with time. Together
they make the whole picture breathe.

| Wave | Phase | Wavelength | How it moves |
|---|---|---|---|
| **Width swell** | `x + τ` | 6.28 spans (2π), measured along the width | Its fronts are upright lines. They travel to the left, one span for each radian of clock. |
| **Height swell** | `y ÷ 2 + τ ÷ 2` | 12.57 spans (4π), along the height | Its fronts are horizontal lines. They travel downward, one span for each radian of clock. It rises and falls half as often as the width swell. |
| **Diagonal swell** | `x ÷ 2 + y ÷ 2 + τ ÷ 2` | 12.57 spans along the width, and the same along the height | Its fronts run from top left to bottom right. They travel toward the bottom-left corner, crossing one span of width (and one of height) for each radian of clock. |
| **Corner swell** | `√(x² + y²) + τ` | 6.28 spans (2π), measured outward from the bottom-left corner | Its fronts are quarter circles about the bottom-left corner. They close in on the corner, one span for each radian of clock. |

In the corner swell, distance from the corner is taken on the picture, where the width and the
height each count as 1: on a buffer that is not square the "circles" are ellipses in cells.

### Two ripples: about one and a half wavelengths to a span

These give the picture its bands.

| Wave | Phase | Wavelength | How it moves |
|---|---|---|---|
| **Width ripple** | `10 × x + τ` | 0.628 span (2π ÷ 10): 1.59 waves in the width of the buffer | Upright fronts travelling to the left, a tenth of a span for each radian of clock. |
| **Swinging ripple** | `10 × (g × x + h × y) + τ`, where `g = sin(τ ÷ 2)` and `h = cos(τ ÷ 3)` | 0.628 span ÷ √(g² + h²): between 0.444 and 1.47 spans | Straight, parallel fronts whose direction and spacing keep changing. See below. |

The swinging ripple is a plane wave whose *tilt* `(g, h)` is not fixed. The fronts lie square to
the direction `(g, h)`, the wave has `10 × g ÷ 2π` wavelengths along the width and
`10 × h ÷ 2π` up the height, and its fronts travel against the direction `(g, h)`. `g` swings
between −1 and 1 once every 4π (12.57) of clock, and `h` once every 6π (18.85), so the tilt
traces a closed figure every 12π (37.7) of clock. At the start of an effect `g` is nearly 0 and
`h` nearly 1: the fronts are horizontal, 1.59 waves up the buffer. The ripple is at its finest
twice in each 12π: when `g` and `h` are both −1 (at `τ` = 3π, 15π, …), with 3.18 waves from the
bottom-left corner to the top right, and when `g` is 1 and `h` is −1 (at `τ` = 9π, 21π, …), with
3.18 waves from the top-left corner to the bottom right. It is at its longest when `√(g² + h²)`
is at its least, 0.43: about two thirds of a wave over the picture. It never flattens out
altogether. As the tilt turns, the fronts swing round with it, and the way they travel turns
with them.

### One set of rings: the wave that `twist` tightens

| Wave | Phase | Spacing | How it moves |
|---|---|---|---|
| **Twist rings** | `√(d² ÷ ℓ² + τ)` | Far from the centre, 2π × ℓ: 0.889 ÷ √twist spans | Rings about a centre that wanders. See below. |

- `ℓ` is the rings' length scale, in spans: `ℓ² = 0.02 ÷ twist`. (`ℓ` is 0.1414 at twist 1,
  0.0816 at twist 3, 0.0447 at twist 10.)
- `d` is the distance, in spans, from the cell to the rings' centre `(x₀, y₀)`:
  `d² = (x − x₀)² + (y − y₀)²`. Width and height each count as 1, so here too the rings are
  ellipses in cells on a buffer that is not square.
- The centre wanders: `x₀ = −½ × sin(τ ÷ 5)` and `y₀ = −½ × cos(τ ÷ 3)`. It stays within half a
  span, each way, of the buffer's bottom-left corner: `x₀` swings between half a span left of the left edge
  and the middle of the buffer once every 10π (31.4) of clock, `y₀` between half a span below
  the bottom edge and the middle once every 6π (18.85). At the start of an effect it is on the
  line of the left edge, half a span below the bottom. It is on the buffer itself only when
  both are positive, about a quarter of the time. So what the buffer shows of the rings is
  usually a set of arcs sweeping in from below or from the left.
- The clock is inside the square root. Two things follow. The rings shrink toward their centre
  as the clock runs, each vanishing there in turn (the ring whose phase is `φ` is at distance
  `ℓ × √(φ² − τ)` from the centre and is gone when `τ` reaches `φ²`). And the rings *age*: see
  section 4.

### Adding them up

`Σ` = width swell + height swell + diagonal swell + corner swell + width ripple + swinging
ripple + twist rings, each the sine of its phase.

### Worked examples

On a buffer 101 by 101. The lines down to the colour angle are the description's own, worked in
double precision. The last six lines are what today's effect returns, to five places (the
description gives the same to within one in the last place).

| | Example 1 | Example 2 |
|---|---|---|
| Settings | defaults (`twist` 1, `density` 1, `speed` 10) | `twist` 3, `density` 2, `speed` 44 |
| Frame `n`, position `(u, v)` | 0, (0.5, 0.5) | 300, (0.25, 0.75) |
| Clock `τ` | 1 ÷ 273 = 0.003663004 | 301 ÷ 171 = 1.760233918 |
| Tilt `(g, h)` | (0.001832, 0.999999) | (0.770813, 0.832747) |
| Ring centre `(x₀, y₀)`; `ℓ` | (−0.000366, −0.500000); 0.141421 | (−0.172410, −0.416374); 0.081650 |
| Width swell | +0.482637 | +0.904991 |
| Height swell | +0.249178 | +0.950586 |
| Diagonal swell | +0.481032 | +0.981876 |
| Corner swell | +0.652417 | +0.557017 |
| Width ripple | −0.957879 | −0.899508 |
| Swinging ripple | −0.955210 | −0.486514 |
| Twist rings | +0.998591 | +0.441346 |
| Sum `Σ` | +0.950766 | +2.449793 |
| Colour angle `θ` | 85.5690° | 440.9628°, which is 80.9628° |
| `white` | 0.99851, 0.99851, 0.99851 | 0.99379, 0.99379, 0.99379 |
| `redGreen` | 0.99851, 0.53863, 0 | 0.99379, 0.57854, 0 |
| `blueGreen` | 0.00392, 0.53863, 0.99851 | 0.00392, 0.57854, 0.99379 |
| `rainbow` | 0.99851, 0.28420, 0.21729 | 0.99379, 0.32112, 0.18509 |
| `palette`: red, green, blue | 0.43160, 0.56840, 0 | 0.35776, 0.64224, 0 |
| `palette`: black, white | 0.28420, 0.28420, 0.28420 | 0.32112, 0.32112, 0.32112 |

---

## 4. Motion and time

### The clock

**The only thing that moves by itself is the clock.** With `n` the frames elapsed and `s` the
whole part of `speed`:

> `τ = (n + 1) ÷ (303 − 3 × s)`

So the clock advances by one tick, `1 ÷ (303 − 3 × s)` radians, every frame, and the effect's
first frame is already one tick in (`τ` is never 0).

- **It counts frames, not milliseconds.** `n` is the elapsed time divided by the sequence's
  frame time with the fraction dropped. For the length of a frame the picture holds, exactly.
  The frame at 1,000 ms of a sequence with 50 ms frames is the frame at 500 ms of one with 25 ms
  frames and at 400 ms of one with 20 ms frames: all are `n = 20`. A frame time of 0 counts as
  1 ms.
- **Different speeds meet.** Two settings show the same picture whenever
  `(n + 1) ÷ (303 − 3 × s)` is the same for both: speed 100 at frame 0, speed 99 at frame 1,
  speed 67 at frame 33, speed 50 at frame 50, speed 10 at frame 90 and speed 0 at frame 100 are
  one and the same frame (`τ` = ⅓).
- **Nothing else moves it.** Not the effect's length, not where the effect sits in the sequence,
  not a random number. Stretching an effect does not slow it; it only shows more of the same
  flow.

### What the clock does to the picture

Each wave's phase grows with the clock (section 3), so each wave travels:

- the width swell and the width ripple travel left;
- the height swell travels down, the diagonal swell toward the bottom left, the corner swell
  inward to the bottom-left corner;
- the swinging ripple travels square to its fronts and turns as it goes;
- the twist rings shrink toward their wandering centre.

The drift a viewer notices is the width ripple's, to the left: laying the bottom row of a frame
on the same row 20 frames later (speed 10, a buffer 1,001 cells wide) fits best with a shift of
about 8 cells to the left, at 18 of 20 moments tried.

Six of the seven waves repeat: taken together they come back to where they were every 12π
(37.7) of clock, which is 10,292 frames at speed 10 (8.6 minutes at 20 frames a second) and 113
frames at speed 100. The twist rings never repeat: their centre comes back every 30π, but the
clock under their square root does not.

### The twist rings age

Because the clock is inside the twist rings' square root, the rings spread out as an effect runs.
While `τ` is small beside `d² ÷ ℓ²` they are evenly spaced, as section 2 gives. Once `τ` is large
beside it the phase hardly changes from place to place any more: the rings have opened out into
one slow pulse of the whole buffer, and `twist` no longer shows. Over the buffer `d² ÷ ℓ²` is of
the order of 50 at twist 1 and 500 at twist 10 (up to 225 and 2,250 in the corner furthest from
the centre), so this happens when the clock passes those values: never in a song at ordinary
speeds (at 20 frames a second the clock reaches 7.8 after a minute at speed 50), but within
minutes at the top speeds.
Measured at speed 100 (crossings of one half along the middle column, density 1):

| | Frames 0 to 58 (`τ` about 10) | From frame 300 (110) | From 1,500 (510) | From 6,000 (2,010) |
|---|---|---|---|---|
| `twist` 10 | 4.6 | 4.5 | 3.7 | 2.3 |
| `twist` 1 | 2.1 | 2.3 | 2.2 | 2.3 |

### Settings that change over the effect (value curves)

A curve on a number setting is resolved before the effect is asked for a frame: the effect
receives, for each frame, the values the curves give at that moment, and draws the frame those
values describe. It keeps no memory of earlier values.

- `twist` and `density` arrive as whole numbers (a curve's value is rounded to the nearest
  before it arrives), so under a curve they change in steps. A step of `density` recolours the
  whole picture at once.
- `speed` arrives as a number and its whole part is used, so under a curve it too changes in
  steps.
- **A curve on `speed` does not accumulate.** The clock at any moment is `n + 1` times that
  moment's tick. When the speed's whole part changes, the clock jumps to where it
  would have been had the effect run at the new speed from its start: the picture lurches
  forward when speed rises (the further into the effect, and the higher the speed, the bigger
  the lurch) and backward when it falls.

### A frame is a pure function

The same settings, palette, buffer size, pixel position and frames elapsed always give the same
colour, bit for bit, whatever was drawn before, in whatever order frames or pixels are asked for,
on whatever thread. Scrubbing to a moment shows exactly what playing through it shows.

---

## 5. Colour

### From the sum to the colour angle

> `θ = 90° × density × Σ`

Each unit of the sum is a quarter of a turn of colour at density 1. Since `density` is a whole
number, `Σ` could be raised or lowered by 4 without changing any colour.

### The five colourings

Every colouring returns alpha 1 for every pixel. In all of them the colour depends on `θ` alone.

| `colors` | Red | Green | Blue |
|---|---|---|---|
| `white` | `rs(θ)` | `rs(θ)` | `rs(θ)` |
| `redGreen` | `rs(θ)` | `rs(θ + 90°)` | 0 |
| `blueGreen` | 1 ÷ 255 (0.0039) | `rs(θ + 90°)` | `rs(θ)` |
| `rainbow` | `rs(θ)` | `rs(θ + 120°)` | `rs(θ + 240°)` |
| `palette` | the palette's colour at position `rs(θ + 120°)` | | |

What that looks like, at four points of the cycle:

| `θ` | `white` | `redGreen` | `blueGreen` (green, blue) | `rainbow` |
|---|---|---|---|---|
| 0° | mid grey 0.5 | (0.5, 1, 0) yellow-green | (1, 0.5) sea green | (0.5, 0.933, 0.067) |
| 90° | white 1 | (1, 0.5, 0) orange | (0.5, 1) sky blue | (1, 0.25, 0.25) red |
| 180° | mid grey 0.5 | (0.5, 0, 0) dark red | (0, 0.5) dark blue | (0.5, 0.067, 0.933) |
| 270° | black 0 | (0, 0.5, 0) dark green | (0.5, 0) dark green | (0, 0.75, 0.75) cyan |

- **`white`** runs smoothly from black to white and back. Black is solid black: it covers
  what is beneath.
- **`redGreen`** goes round yellow-green, orange, dark red, dark green. Red and green are a
  quarter of a cycle apart: when one is at its brightest or its darkest the other is at one
  half, and the point (2 × red − 1, 2 × green − 1) lies on a circle of radius 1. Blue is
  exactly 0.
- **`blueGreen`** is the same with blue in place of red, and red held at 1 of 255, a steady
  value too small to see.
- **`rainbow`** goes round the colour wheel: red at 90°, then magenta, blue at 210°, cyan,
  green at 330°, yellow, and red again. The three channels are 120° apart, so
  they always add up to 1.5: it is never black, never white, and the same total light
  everywhere.
- These four ignore the palette.

### `palette`

The palette is an ordered list of 1 to 32 colours (an empty palette counts as one white; the
palette the effect is handed never holds more than 32). Each colour is three bytes, and each
channel is its byte divided by 255.

The colours are placed at equal steps along a line from position 0 (the first colour) to
position 1 (the last): with `k` colours, colour number `m` (counting from 1) sits at
`(m − 1) ÷ (k − 1)`. Between two neighbours the colour is their straight-line blend, channel by
channel, of those 0-to-1 values. A pixel shows the colour at position `p = rs(θ + 120°)`.

- `p` is the same number as the `rainbow` colouring's green channel, and the same as every channel
  of `palette` with the two colours black and white.
- The palette is run through **there and back**, never round: as `θ` advances, `p` goes from the
  first colour up to the last and down again. The picture never passes straight from the last
  colour to the first.
- Because a sine lingers at its ends, the first and last colours get broad areas and the ones
  between get narrower bands. With two colours each has half the picture. With three, each has
  33% (the middle one as the band between the other two). With more, the first and the last
  still have the broadest areas (each is the nearest colour over 27% of the picture with four
  colours, 17% with eight) and the ones between share the rest.
- **Two colours** A, B: `A + (B − A) × p`.
- **Three colours** A, B, C: for `p` below ½, `A + (B − A) × 2p`; above, `B + (C − B) × (2p − 1)`.
- **One colour**: that colour, on every pixel, at every moment. Nothing moves.
- **No colours**: white, the same way.

Measured: over the buffer and over time every channel that varies, in every colouring, averages
0.50 (between 0.47 and 0.53 in the cases measured) with a spread (standard deviation) of 0.354,
and reaches both 0 and 1.

### What the renderer does afterwards

Fades, sparkles, blur, colour adjustments and the blending with other layers are applied by the
renderer after the effect and are not the effect's business.

---

## 6. Geometry

### 6.1 The grid of cells

The grid is the buffer's `C` columns by `R` rows (a count of 0 is taken as 1), unless that is
more than 1,048,576 cells. Then the picture is drawn on a coarser grid: both counts are
multiplied by `√(1,048,576 ÷ (C × R))`, fractions dropped, and never less than 1.

The rounding of that arithmetic decides the answer for many buffers (for every square one the
exact product is 1,024 to the dot), so it has to be done as today's is, in double precision and
in this order: the quotient `1,048,576 ÷ (C × R)`; its square root; the root times `C`, and the
root times `R`; then the fractions dropped. Measured, by counting the distinct columns and rows
in a frame (for the three thin buffers at the end: by comparing with a buffer of one row, and by
counting the cells in the first thousandth of the width):

| Buffer | Grid the picture is drawn on | |
|---|---|---|
| 1,024 × 1,024 | 1,024 × 1,024 | exactly at the limit: unchanged |
| 1,025 × 1,024 | 1,024 × 1,023 | fractions are dropped: to the nearest would give 1,025 × 1,024 |
| 2,048 × 2,048, 3,072 × 3,072, 5,000 × 5,000 | 1,024 × 1,024 | |
| 1,027 × 1,027, 1,034 × 1,034, 1,041 × 1,041 | 1,024 × 1,024 | |
| 1,039 × 1,039, 1,040 × 1,040, 1,070 × 1,070 | 1,023 × 1,023 | the product comes out a hair under 1,024 |
| 1,500 × 1,000 | 1,254 × 836 | |
| 4,000 × 300 | 3,739 × 280 | |
| 2,049 × 512 | 2,048 × 511 | |
| 1,303 × 5,212 | 512 × 2,048 | single precision would give 511 × 2,047 |
| 600,000 × 2 | 560,867 × 1 | the two rows become one |
| 1,100,000 × 1 | 1,073,980 × 1 | still above the limit: it is not reduced a second time |
| 3,000,000 × 1 | 1,773,620 × 1 | |

Everything below, and the `(x, y)` of section 0, uses the grid's counts, called `C` and `R` from
here on.

### 6.2 Which cell a pixel belongs to

For the column: multiply `u`, held between 0 and 1, by `C − 1`, and take the nearest whole
number. Exactly half way between two columns goes to the higher one. For the row: the same with
`v` and `R − 1`.

- A `u` below 0 counts as 0 and one above 1 as 1.
- A `u` that is not a finite number (not a number at all, or infinite either way) counts as 0.
  Likewise `v`. Nothing fails.
- With one column every pixel is in column 0, whatever its `u`. Likewise one row.

**The product is worked out in single precision, and that has to be kept.** `u` arrives as a
single-precision number; it is multiplied by `C − 1` as a single-precision number; the product
is rounded to single precision; and that is rounded to the nearest whole number, an exact half
going up. This matters wherever a pixel is meant to sit half way between two cells, which
happens all the time when pixels laid out on one grid are drawn on a buffer of another size: a
position like 0.05 on a grid of 11 is not exactly half way in binary, and the last bit of the
product decides the cell. Measured on 1,068 such positions (`(k + ½) ÷ (C − 1)` as a
single-precision number, for 29 grid sizes from 2 to 1,024): today's effect takes the cell the
single-precision product gives in all 1,068 (the higher cell in 1,033, the lower in 35); a
double-precision product would give a different cell for 476 of them. And for 2,047 positions
that are exactly half way in binary (grids of 2, 3, 5, 9, … 1,025 cells) it takes the higher
cell every time.

So pixels that share a cell show the same colour, and a pixel shows the colour of its cell's
place `(x, y)`, not of its own exact position. Where a prop's pixels sit on the grid, as they do
on a matrix, that is the same thing.

### 6.3 The picture is stretched to the buffer

`(x, y)` runs from 0 to 1 over the grid whatever its proportions and its size. So:

- **It does not depend on resolution.** A cell a given fraction of the way along and up shows
  the same colour on a buffer of any size: the nine cells of a 3 by 3 buffer are the four
  corners, the four edge middles and the centre of a 101 by 101 buffer, of a 201 by 51, and of a
  5 by 9, exactly. More cells only sample the same picture more finely.
- **It is not round in cells.** On a buffer 400 by 178 a ring of the twist rings that is 0.889
  span from the next is 355 cells from it along the width and 157 along the height.
- **It covers the whole buffer** whether or not there are pixels there. A group of props spread
  thinly over a large buffer shows the picture through its pixels as through holes in a mask.

### 6.4 A single row, a single column, a single pixel

- **One row** (`R` = 1) is `y = 0` for every pixel: it shows what the **bottom row** of a taller
  buffer of the same width shows. The height swell is then the same for every pixel (a pulse of
  the whole row), the swinging ripple is a ripple along the row whose wavelength is 0.628 span
  when `g` is ±1 and grows without limit as `g` passes through 0 (the ripple is then one colour
  along the whole row), and which reverses when `g` changes sign, and the rest are as they are
  along any bottom row.
- **One column** (`C` = 1) is `x = 0`: the **left column** of a wider buffer of the same height.
  The width swell and the width ripple are then the same for every pixel (both are `sin τ`), and
  the height swell and the diagonal swell are equal.
- **One pixel** (1 by 1) is the bottom-left corner cell of any buffer: a single colour that
  wanders through the colour cycle. There the width swell, the corner swell, the width ripple
  and the swinging ripple are all `sin τ`, the height and diagonal swells are both
  `sin(τ ÷ 2)`, and the twist rings are seen from `d² = x₀² + y₀²`.

### 6.5 Very small buffers

There is no special rule: the same picture, sampled at very few places. A buffer two cells wide
has `x` = 0 and 1 only.

---

## 7. Edge cases

**Speed**
- 0: still moving, 303 frames to a radian.
- 100 at density 10: consecutive frames have nothing visibly in common (mean change in
  brightness 0.41 a frame, what unrelated frames would give).
- Fractions: dropped (99.99 is 99).
- If the effect is ever handed a speed that has not been pulled into range: below 0 counts as
  0, above 100 as 100, and not a number as 0. (Through the renderer this never happens: a speed
  that is not a number has already become 10.)

**Twist and density**
- If handed a value outside 1 to 10: the nearest of 1 and 10.

**Palette**
- Empty: white. One colour: a flat, still fill of it. Black as a palette colour is solid black.
- The four fixed colourings are the same with any palette.

**Time**
- The first frame is `τ` = one tick, a full picture: there is no fade-in and no blank frame.
- Within a frame's time the picture does not change at all.
- An hour in, at any speed: every number returned is finite, from 0 to 1, with alpha 1.
- Very large clock values: section 9, items 9 and 11.

**Buffer**
- 0 columns or rows: taken as 1.
- More than 1,048,576 cells: section 6.1.
- A buffer of one column, one row, or one cell: section 6.4.

**Pixels**
- Outside 0 to 1, or not a finite number: section 6.2. Never a failure, and never a colour or
  alpha that is not a finite number from 0 to 1.

---

## 8. Acceptance properties

**The test rig.** Unless a property says otherwise:

- Buffer 101 columns by 101 rows. Effect 10 seconds long, starting at 0, frame time 50 ms.
  "Frame `n`" is the moment `n × 50` ms after the effect's start. Settings at their defaults
  (`colors` `palette`, `twist` 1, `density` 1, `speed` 10) except those named. Palette red
  (255, 0, 0), green (0, 255, 0), blue (0, 0, 255).
- A *sample* at `(u, v)` is the colour and alpha returned for a pixel at that position.
- *G21* is the 441 positions with `u` and `v` each one of 0, 0.05, 0.10, … 1.
- *Brightness* is the red channel in the `white` colouring.
- *Crossings* along a line are the number of times brightness passes one half from one sample
  to the next. The *middle row* of a buffer 1,001 by 1,001 is the 1,001 samples at
  `u = i ÷ 1000`, `v = 0.5`; the *middle column* likewise with `u` and `v` exchanged.
- "The same" means every channel and alpha equal. Where it compares two calls with the very same
  inputs it means bit for bit. Where it compares different settings or buffers that should show
  the same thing, a difference of up to 0.001 passes (today's picture gives exactly 0 in every
  such case below).
- After each property: what today's picture measures.

1. **Opaque, and in range.** Every sample, in every colouring, with any palette (including one
   black colour and none), any settings and any frame: alpha exactly 1, and red, green and blue
   finite numbers from 0 to 1. *(26,460 samples: all.)*

2. **A frame depends on nothing else.** For fixed settings, palette, buffer size, position and
   frames elapsed, every sample is the same bit for bit: whether the effect is 1 second or 60
   seconds long; wherever it starts in the sequence; whatever random seed the renderer supplies;
   asked twice; asked in the opposite order; asked from several threads at once.
   *(Difference 0 for length, start, seed, repetition and order. Asked from 8 and from 12
   threads at once, twenty times over each, every answer has the same bits as from one.)*

3. **The picture holds for a frame, and counts frames.** G21 at 1,000 ms is the same at 1,001,
   1,025 and 1,049 ms, and different at 1,050 ms (some sample differs by more than 0.005). G21
   at 1,000 ms with 50 ms frames is the same as at 500 ms with 25 ms frames, 400 ms with 20 ms,
   660 ms with 33 ms, 2,000 ms with 100 ms, and 20 ms with 1 ms.
   *(0 within the frame; the next frame differs by 0.045; 0 between frame times.)*

4. **Speed counts by its whole part.** At frame 100, G21 at speed 10.25, 10.5 and 10.999 is the
   same as at speed 10; at speed 11 and at 9.999 some sample differs by more than 0.005.
   *(0; 0.038; 0.036.)*

5. **The clock law.** With `twist` 3, `density` 2, `rainbow`: G21 is the same at (speed, frame)
   = (100, 0), (99, 1), (67, 33), (50, 50), (10, 90), (1, 99) and (0, 100); and the same at
   (100, 2), (98, 8), (50, 152) and (0, 302). Speed 50 at frame 51 differs from speed 100 at
   frame 0 by more than 0.005 somewhere. *(0 and 0; 0.065.)*

6. **Speed 0 still moves.** At speed 0, between frame 0 and frame 2,000 some G21 sample's
   brightness changes by more than 0.5 and the mean change is more than 0.1.
   *(0.99; 0.43.)*

7. **How fast each speed is.** The mean absolute change in brightness between frame `f` and
   frame `f + 1`, over G21 and over `f` = 0, 60, 120, … 2,940, is 0.0031 at speed 0, 0.0034 at
   10, 0.0068 at 50, 0.0317 at 90 and 0.2823 at 100, each within 15%. At speed 100 and
   `density` 10 it is between 0.35 and 0.46. *(As stated; 0.406.)*

8. **One row, one column, one pixel.** With `rainbow`, `twist` 4, `density` 3, at frame 40, on
   buffers 41 by 17, 101 by 101 and 7 by 300: the samples along `v = 0` are the same as those at
   the same `u` on a buffer of the same width and one row (at any `v`); the samples along
   `u = 0` are the same as those at the same `v` on a buffer one column wide (at any `u`); and
   the sample at (0, 0) is the same as any sample of a 1 by 1 buffer. *(Difference 0.)*

9. **Laid over the buffer whatever its size.** With `rainbow`, `twist` 2, `density` 5, at frame
   80: the nine samples with `u` and `v` each 0, ½ or 1 are the same on buffers 3 by 3, 101 by
   101, 201 by 51, 5 by 9, 11 by 1,001 and 1,001 by 1,001. The sixteen with `u` and `v` each 0,
   ⅓, ⅔ or 1 are the same on 4 by 4, 7 by 7, 100 by 100 and 31 by 61. *(Difference 0.)*

10. **Nearest cell; exact halves go up.** With `rainbow`, `density` 4, at frame 20. On a buffer
    11 by 11: a sample at `u = i ÷ 10 + e` for `e` = −0.049, −0.02, 0.02, 0.049 (kept within
    0 to 1) is the same as the sample at `u = i ÷ 10`; likewise in `v`. On a buffer 5 by 5: the
    samples at `u` = 0.125, 0.375, 0.625 and 0.875 (with `v` = 0) are the same as those at
    `u` = 0.25, 0.5, 0.75 and 1; likewise in `v`. *(Difference 0.)* Positions that are half way
    only as nearly as single precision can say (section 6.2) are tested by cases 47 to 50 of the
    reference table.

11. **Outside, and unusable, positions.** Same settings, buffer 5 by 5. A `u` of −0.3 or −10⁹
    gives the sample at `u = 0`; 1.2 or 10⁹ gives the sample at `u = 1`; not-a-number, +infinity
    and −infinity each give the sample at `u = 0`. Likewise `v`. Nothing fails.
    *(Difference 0.)*

12. **A coarser grid above 1,048,576 cells.** With `rainbow`, `density` 10, `twist` 7, at frame
    20, sampling 40,001 evenly spaced positions along `v = 0.37` (and along `u = 0.37`): the
    number of distinct runs of equal colour is 1,024 (1,024) on a buffer 1,024 by 1,024, 1,024
    (1,023) on 1,025 by 1,024, 1,024 (1,024) on 2,048 by 2,048, 1,023 (1,023) on 1,039 by 1,039,
    1,254 (836) on 1,500 by 1,000, 3,739 (280) on 4,000 by 300, and 512 (2,048) on 1,303 by
    5,212. And at `density` 3 the samples at `(i ÷ 1023, j ÷ 1023)` are the same on 2,048 by
    2,048 as on 1,024 by 1,024. *(As stated; difference 0.)*

13. **The colourings are one picture in different colours.** For the same settings, frame and
    position, with `W` the brightness, `(r, g)` the red and green of `redGreen`, and
    `s = 2r − 1`, `c = 2g − 1`:
    - `white`: three equal channels;
    - `redGreen`: `r = W`, blue 0, and `s² + c² = 1`;
    - `blueGreen`: red 1 ÷ 255, green `g`, blue `W`;
    - `rainbow`: red `W`, green `½ − s ÷ 4 + (√3 ÷ 4) × c`, blue `½ − s ÷ 4 − (√3 ÷ 4) × c`;
    - `palette` with the colours black, white: all three channels equal to `rainbow`'s green.

    Each within 0.0001, over G21, for (`twist`, `density`, `speed`, frame) = (1, 1, 10, 0),
    (1, 1, 10, 777), (3, 2, 44, 300), (10, 10, 90, 55), (2, 6, 16, 400), (7, 3, 100, 12).
    *(Largest departure 0.000002.)*

14. **Density multiplies the colour's angle.** With `redGreen`, for each of the six settings of
    property 13 taken at `density` 1: let `a` be the angle whose sine is `2r − 1` and whose
    cosine is `2g − 1`. Then the same settings at `density` `k` (2, 3, 5, 10) give red
    `rs(k × a)` and green `rs(k × a + 90°)`, within 0.0001. *(Largest difference 0.000004.)*

15. **The palette is a straight-line blend through evenly spaced colours.** With `twist` 2,
    `density` 3, `speed` 44, at frame 300, over G21: let `p` be the sample with the palette
    black, white. For palettes of 2, 3, 4, 8 and 32 colours, the sample is the blend of the two
    colours either side of position `p × (k − 1)`, within 0.0001.
    *(Largest difference 0.0000014.)*

16. **One colour is a flat fill; none is white.** With the single colour (51, 102, 204) and
    `density` 7, every G21 sample at frames 0, 100 and 5,000 is exactly (0.2, 0.4, 0.8), alpha 1.
    With an empty palette every sample is exactly (1, 1, 1), alpha 1. *(As stated.)*

17. **The fixed colourings ignore the palette.** `white`, `redGreen`, `blueGreen` and `rainbow`
    give the same samples with the palette red, green, blue as with none. *(Difference 0.)*

18. **As much light as dark.** Over the 676 positions with `u` and `v` each one of 0, 0.04, …
    1, and frames 0, 80, 160, … 7,920, at `density` 1 and at 10: every channel that varies
    (`white`: one; `redGreen`: red and green; `blueGreen`: green and blue; `rainbow`: all
    three) has a mean within 0.04 of 0.5, a standard deviation within 0.01 of 0.354, a least
    value under 0.01 and a greatest over 0.99. And at frame 0 alone the standard deviation of
    brightness over G21 is above 0.3: the first frame is a full picture.
    *(Means 0.473 to 0.525; deviations 0.350 to 0.356; first frame 0.371.)*

19. **Density sets how many bands there are.** On a buffer 1,001 by 1,001, averaged over frames
    0, 200, … 5,800, the crossings along the middle row are 3.9, 7.0, 17.5 and 34.8 at
    `density` 1, 2, 5 and 10, and along the middle column 2.3, 4.9, 11.8 and 23.8, each within
    15% plus 0.3. *(As stated.)*

20. **Twist adds rings.** The same measurement at `density` 1: along the middle row 3.9, 4.8
    and 5.6 at `twist` 1, 4 and 10; along the middle column 2.3, 3.3 and 4.9, each within 15%
    plus 0.3; and the column count at `twist` 10 is more than 1.7 times that at `twist` 1.
    *(As stated.)*

21. **The twist rings spread out as the clock runs on.** At speed 100, on a buffer 1,001 by
    1,001, the crossings along the middle column averaged over frames 0, 2, … 58 and over frames
    6,000, 6,002, … 6,058: at `twist` 10, 4.6 (within 0.8) and then 2.3 (within 0.6); at
    `twist` 1, 2.1 and then 2.3 (each within 0.6). *(As stated.)*

22. **Smooth: no hard edges.** On a buffer 1,001 by 1,001, over frames 0, 200, … 5,800, the
    largest step in brightness between neighbouring samples of the middle row and of the middle
    column is between 0.012 and 0.025 at the defaults, and no more than 0.36 at `twist` 10,
    `density` 10. *(0.0188; 0.3005.)*

23. **The bands drift to the left.** On a buffer 1,001 by 1,001, for `f` = 0, 300, … 5,700:
    take brightness along the bottom row (`v = 0`, `u = i ÷ 1000`) at frame `f` and at frame
    `f + 20`, and find the whole number of cells `k` from −60 to 60 for which the later row,
    cell for cell, is most like the earlier row moved `k` cells to the left (least mean squared
    difference over the cells both cover). The mean of `k` is between 5 and 12, and `k` is
    positive at 14 or more of the 20 moments. *(Mean 8.4; positive at 18.)*

24. **The reference table.** Every row of `reference.tsv` (3,900 samples: section 14) within
    0.01 in each of red, green, blue and alpha. *(Exactly, by construction.)*

25. **Tiny buffers and extreme settings.** On buffers 1 by 1, 2 by 1, 1 by 2, 2 by 2, 3 by 3 and
    0 by 0, in every colouring, with `twist` 10, `density` 10, `speed` 100, at 0 ms, 50 ms and one
    hour, at positions (0, 0), (1, 1), (0.5, 0.5), (not-a-number, infinity) and (−1, 2): no
    failure, every colour finite and from 0 to 1, alpha 1. A buffer 0 by 0 gives the sample of a
    buffer 1 by 1. A frame time of 0 gives the samples of a frame time of 1 ms.
    *(450 samples: all; difference 0.)*

26. **Settings outside their ranges** (as the renderer hands them on, already pulled in). At
    frame 40, `rainbow`, G21: speed −5 is the same as 0; 250 and infinity the same as 100;
    not-a-number the same as 10; `twist` 0 the same as 1 and 50 the same as 10; `density` 0 the
    same as 1 and 50 the same as 10. *(Difference 0.)*

---

## 9. Habits that look like defects

Each of these can be seen in today's picture and looks more like an accident than a choice.
Each is described by what is seen, and **each is kept unless the owner says otherwise**: the new
Plasma is to show it too. With each is what a viewer would see change if it were fixed, so the
owner can decide.

Imports matter here. Where xLights' frames could be compared (section 12) they show today's
clock, at speeds 10, 16 and 44 with 50 ms frames, today's way of giving a pixel its cell's
colour, and today's picture stretched over buffers that are far from square. So items 3, 4 and 7,
and the stretching of item 10, are known to be how xLights draws it too, and a fix to one of
those in the effect would have to be undone for imports by the importer's translation. Nothing
in the corpus says how xLights treats what items 1, 2, 5, 6 and 9 are about, or the single rows
and columns of item 10, and items 8 and 11 are PixelFlow's own.

1. **One palette colour gives a flat, still fill.** The picture does not move at all, at any
   speed, and a brand-new Plasma (one white) is a white rectangle. *Fixed* (say, by running
   between the colour and dark): a one-colour Plasma would move; every saved one-colour Plasma
   would change from a steady fill to a moving one. **Kept unless the owner says otherwise.**

2. **Speed 0 does not stop the picture.** It drifts at a hundred-and-first of the top pace.
   *Fixed*: speed 0 would hold still; any sequence using speed 0 as "slowest" would freeze.
   **Kept unless the owner says otherwise.**

3. **The pace follows the frame rate.** The same settings run twice as fast in a sequence with
   25 ms frames as in one with 50 ms frames, and a preview drawn more often than the sequence's
   frames shows each frame held, not a smoother flow. *Fixed* (pace by the clock on the wall):
   sequences with frames other than the one the fix is tuned to would run at a different pace
   than today; imports would need their speed translated by frame time.
   **Kept unless the owner says otherwise.**

4. **The speed scale is lopsided.** Half the slider (0 to 50) only doubles the pace; the last
   ten steps multiply it by eleven, and the last step alone doubles it. *Fixed* (an even scale):
   every saved speed would mean a different pace. **Kept unless the owner says otherwise.**

5. **Fractions of speed are dropped.** Under a curve the pace changes in steps.
   *Fixed*: a curve on speed would change the pace smoothly; no fixed speed in a saved sequence
   would change, since the slider moves in whole numbers.
   **Kept unless the owner says otherwise.**

6. **A curve on speed makes the picture lurch.** Each time the speed's whole part changes, the
   picture jumps to where it would have been at the new speed from the start, forward or back
   (section 4). Late in an effect, or near the top of the scale, the jump is a different picture
   altogether. *Fixed* (the clock adding up what each frame's speed gives): a speed curve would
   speed the flow up and slow it down with no jumps; every Plasma with a curve on speed would
   look different from today. The effect cannot do this by itself: it is handed only the
   current value. **Kept unless the owner says otherwise.**

7. **A pixel takes its cell's colour, not its own position's.** Props whose pixels do not sit on
   the grid get a blocky sampling of the picture, pixels that share a cell always match, and a
   pixel exactly half way between cells takes the higher one. *Fixed* (each pixel at its own
   place): such props would show a smoother picture; pixels would change colour by as much as
   the picture changes over half a cell, which is a little at the defaults on a large buffer and
   a lot on a small buffer or at a high density. **Kept unless the owner says otherwise.**

8. **Above 1,048,576 cells the picture is drawn coarser than the buffer.** Neighbouring cells
   share a colour in blocks. *Fixed*: such buffers would show the picture at full sharpness.
   **Kept unless the owner says otherwise.**

9. **The twist rings wear away.** Over minutes at the top speeds the rings spread out until
   `twist` does nothing (section 4). *Fixed* (rings of the same spacing throughout): long, fast
   Plasmas with a high twist would keep their rings; nothing would change at ordinary speeds and
   lengths. **Kept unless the owner says otherwise.**

10. **The picture is stretched to the buffer and hangs from its bottom-left corner.** Rings are
    not round on a buffer that is not square; a single row shows the picture's bottom edge and a
    single column its left edge, not a cut through its middle. *Fixed* (round in cells, or lines
    cut through the middle): every Plasma on a buffer that is not square, and every Plasma on a
    line, would show a different picture. **Kept unless the owner says otherwise.**

11. **Speckle that grows with the clock.** Today's picture is not quite the smooth field of
    section 3: it carries the grain of single-precision arithmetic. Each cell's colour is off by
    a small amount that differs from cell to cell and frame to frame, and the amount grows with
    the clock, with `density`, and with the number of palette colours. Measured, the largest
    departure from the smooth field is never more than

    > `density × k × (0.000004 + 0.0000002 × τ)`

    where `k` is the number of palette colours less one, and 1 for the four fixed colourings and
    for a palette of one colour or two. (The largest seen was 0.95 of that, over the 480,000
    samples of section 15; typically it is half.) Some values, measured:

    | Clock | Reached at speed 100, 20 frames a second, after | `white`, density 1 | `white`, density 10 | 3 colours, density 10 | 8 colours, density 10 | 32 colours, black and white by turns, density 10 |
    |---|---|---|---|---|---|---|
    | 10 | 1.5 seconds | 0.000003 | 0.00002 | 0.00005 | 0.0002 | 0.0008 |
    | 100 | 15 seconds | 0.00001 | 0.0002 | 0.0003 | 0.001 | 0.003 |
    | 1,000 | 2.5 minutes | 0.0001 | 0.0008 | 0.002 | 0.007 | 0.012 |
    | 2,000 | 5 minutes | 0.0002 | 0.002 | 0.006 | 0.02 | 0.04 |
    | 10,000 | 25 minutes | 0.001 | 0.013 | 0.021 | 0.07 | 0.24 |
    | 100,000 | 4 hours | 0.01 | 0.10 | 0.22 | 0.77 | |

    So it is far below anything visible in ordinary use: under 0.001 in every effect of the real
    sequences, and under 0.01 whenever `density × k × τ` is below about 40,000. At speed 90 the
    clock reaches 1,000 after 27 minutes; at speed 50, after more than two hours. And the speeds
    that reach large clock values sooner are those at which, with a high density, one frame
    already has nothing to do with the next. **This is the one habit this description does not
    ask to have copied**, because it cannot be described without saying how today's arithmetic
    is done: the new Plasma should show the smooth field at every clock value. *Seen if so*:
    nothing in ordinary use; over many minutes at the top speeds with a high density and many
    colours, a faint speckle goes. A side-by-side comparison of old and new must allow the
    amount above. **The owner may say otherwise.**

Not defects, but worth knowing they are deliberate parts of today's look and are kept: the
effect's first frame is one tick in, not zero; `twist` and `density` take whole numbers only;
the palette runs there and back and never wraps; black is solid, not see-through.

---

## 10. What does not have to match

Only what cannot be seen.

- **The speckle of section 9, item 11.** The target is the smooth field of sections 3 to 6. A
  new Plasma that shows it differs from today's picture by today's speckle, which is at most
  `density × k × (0.000004 + 0.0000002 × τ)`: under 0.001 in ordinary use, and that difference
  is allowed whatever its size.
- **The last decimal places.** Beyond that, any difference of less than 0.0005 in a colour. So
  which precision the waves are worked out in, in what order they are added, and whose sine is
  used are all free. Work the clock itself out in double precision. (This document's own
  description, worked in double precision, is within 0.00015 of every row of the reference
  table.)
- **Bit-for-bit agreement with today's picture**, and bit-for-bit agreement between machines
  (today's sines come from the system's maths library and may differ in the last bit from one
  machine to another).
- **`blueGreen`'s red.** It is 1 ÷ 255 today, a value that shows as 1 of 255 on a pixel. Keeping
  it is simplest; 0 could not be told from it.
- **A palette position of exactly 1.** It shows the last colour to within a millionth.
- **How fast it draws, and what memory it uses.** Faster is better: see `integration.md`.

Put as one rule for a test or a side-by-side comparison: a new Plasma agrees with today's when
every channel is within `0.0005 + density × k × (0.000004 + 0.0000002 × τ)` of it, with `k` the
number of palette colours less one, and 1 for the four fixed colourings and for a palette of one
colour or two. The plain 1% of the rule at the top of this document is what that comes to, with
room to spare, wherever `density × k × τ` is below 40,000.

Everything else in sections 2 to 7 has to match, section 9 included. In particular the cell a
pixel takes (section 6.2) and the grid of a very large buffer (section 6.1) have to come out the
same, rounding and all: a different cell is a different colour.

---

## 11. How it is used in practice

From the seven real sequences the fidelity corpus is built on (counts and values only).

- Two of the seven contain Plasma: **23 effects** in all (22 in one sequence, 1 in another).
- **Every one uses the `palette` colouring.** The four fixed colourings are never used.
- **`twist`**: 1 in 22, 2 in 1. **`density`**: 1 in 22, 6 in 1. **`speed`**: 44 in 19, 10 in
  3, 16 in 1. So the clock runs 171, 273 or 255 frames to a radian, and reaches at most 2.4 by
  an effect's end: the twist rings never age, and section 9, item 11 never arises.
- **Palettes**: three colours in 20, four in 3; never one or two. The first colour is white in
  20 of the 23.
- **No curves** on any setting, in any of them.
- **Frame time**: 50 ms throughout. **Lengths**: 9.7 to 29.9 seconds.
- **Targets**: every one is on a group of props, drawn on a large, thinly filled buffer: 400 by
  177 or 178 cells (12 effects, on 2,148 or 4,959 pixels), 206 by 175 (6, on 1,200 pixels), 400
  by 221 (2, on 478 pixels), 400 by 118 (2, on 1,168 pixels) and 139 by 259 (1, on 560 pixels).
  Between 0.5% and 7% of the buffer's cells hold a pixel. None is a single row, a single column,
  or a buffer a few cells wide; none is anywhere near 1,048,576 cells.
- **Around the effect**: 22 of 23 carry sparkles (at 54 of 200 in 19, at 6 in 3); 22 fade in
  over 0.5 to 2 seconds and out over 2 seconds; 14 are blended so that they show only where the
  layers beneath are dark, 9 are laid over them normally; none is blurred.
- They are a handful of designs repeated on different groups: five palettes at `speed` 44 or 10
  with `twist` 1 and `density` 1, and one effect with a sixth palette at `twist` 2, `density` 6,
  `speed` 16.

What that says about priorities: the `palette` colouring with three or four colours, `twist` 1,
`density` 1, speeds 10 to 44, on wide sparse buffers, is nearly all of it. What matters most for
those is that the colour a cell shows is right to a few parts in a thousand and that a pixel
takes the same cell as it does today.

---

## 12. Against xLights' own frames, and the fidelity baseline

### What xLights' frames show

Today's picture was held, pixel by pixel and frame by frame, against the frames xLights rendered
for those sequences.

- **Where a pixel sits in the same cell of the buffer in both programs, the two agree.** On
  five effects at `twist` 1, `density` 1, `speed` 44 (palettes of three colours; 478, 1,168 and
  4,959 pixels; 115 to 230 frames each), 89 to 94% of all pixel-frames are within 1 of 255 in
  every channel. The rest are sparkles, which fall on different pixels in each program, other
  layers, and one prop of 168 pixels whose frames in xLights' file hold something other than
  this effect (nothing the description gives for any cell within a fifth of the buffer).
- **The palette is used the same way.** Sorting the pixel-frames of those effects by where the
  description puts them along the palette, xLights' mean colour and today's agree to within
  0.1 of 255 all the way along for two three-colour palettes, and to within 0.6 of 255 for the
  four-colour palette.
- **The clock is the same.** On the effect at `twist` 2, `density` 6, `speed` 16 (560 pixels, 598
  frames, no sparkles or fades), fitting each pixel's colours over time fits xLights' frames 3
  to 4 times worse with the clock moved one frame either way, and worse still with two.
- **On that effect the pictures differ pixel for pixel, and it is not the effect.** The mean
  difference is 48 of 255. But for every one of 250 pixels tried, the pixel's colours in
  xLights' frames over the whole effect are exactly what the description gives (to 0.2 of 255,
  which is the rounding of a byte) for a cell a whole number of cells away from the one
  PixelFlow puts that pixel in: on a buffer 139 by 259, the same cell for 9% of the pixels, 1 to
  8 rows lower for 75%, and 1 to 6 columns to one side for 16%. With `density` 6 one row is
  enough to change the colour. **That belongs to how the group's buffer is laid out, which is
  neither the effect nor the importer's translation of the effect's settings.** A new Plasma
  that matches today's will differ from xLights there exactly as today's does.
- **No xLights number was found to mean anything but its face value.** `Plasma_Style` 1 and 2,
  `Plasma_Line_Density` 1 and 6, and `Plasma_Speed` 10, 16 and 44 all show in xLights' frames
  what the same numbers show in PixelFlow. Nothing here needs translating in the importer.

Not covered by any xLights frame in the corpus: the four fixed colourings, `twist` above 2,
`density` other than 1 and 6, palettes of one or two colours, buffers of one row or column,
speeds above 44, and frames other than 50 ms.

### The baseline

"Look" is 0 to 100; 85 or more is a match, 55 or more is close. Four fresh runs of the whole
corpus on `develop` `b14844e`. The new Plasma must not fall below these.

- **Plasma as a kind: look 99.84 in all four runs** (99.844 to 99.845). Verdicts: **19 match**,
  0 close, 0 different; 4 could not be judged.
- Parts: coverage 1.00, brightness 1.00, colour 1.00, pattern 0.996, motion 1.00, rhythm 1.00.
- Of the 23, 4 were judged with pixels to themselves and 15 together with whatever was layered
  on the same pixels.
- Pixel for pixel, where it had pixels to itself (a diagnostic, not a target): mean error 11.4
  of 255, 88% of pixels within tolerance, lit pixels overlapping by 0.96.
- The importer expects all 23 to match.

Each instance, by its number in the corpus scorecards (sequence letter and effect index):

| Instance | Seconds | Pixels measured | Judged | Look, four runs | Weakest part |
|---|---|---|---|---|---|
| A 20 | 18.45 to 34.65 | 2,148 | layered | 100.0 in all | – |
| A 22 | 49.05 to 63.45 | 2,148 | layered | 100.0 in all | – |
| A 43 | 33.30 to 49.95 | 2,148 | layered | 100.0 in all | – |
| A 44 | 62.10 to 78.30 | 2,148 | layered | 100.0 in all | – |
| A 71 | 390.35 to 400.05 | 2,148 | not judged | | |
| A 75 | 451.35 to 465.30 | 2,148 | not judged | | |
| A 84 | 33.30 to 49.95 | 1,200 | layered | 100.0 in all | – |
| A 85 | 62.10 to 78.30 | 1,200 | layered | 100.0 in all | – |
| A 112 | 390.35 to 400.05 | 1,200 | not judged | | |
| A 116 | 451.35 to 465.30 | 1,200 | not judged | | |
| A 123 | 4.50 to 20.70 | 1,200 | layered | 100.0 in all | – |
| A 125 | 35.10 to 49.50 | 1,200 | layered | 100.0 in all | – |
| A 145 | 3.15 to 19.35 | 478 | layered | 100.0 in all | – |
| A 147 | 33.75 to 48.15 | 478 | layered | 100.0 in all | – |
| A 187 | 18.45 to 34.65 | 4,959 | alone | 100.0 in all | – |
| A 189 | 49.05 to 63.45 | 4,959 | alone | 99.9 in all | – |
| A 210 | 33.30 to 49.95 | 4,959 | layered | 100.0 in all | – |
| A 211 | 62.10 to 78.30 | 4,959 | layered | 100.0 in all | – |
| A 238 | 390.35 to 400.05 | 4,959 | alone | 100.0 in all | – |
| A 242 | 451.35 to 465.30 | 4,959 | alone | 99.9 to 100.0 | motion 0.99 |
| A 249 | 2.25 to 18.45 | 1,168 | layered | 100.0 in all | – |
| A 251 | 32.85 to 47.25 | 1,168 | layered | 94.4 to 94.5 | pattern 0.87 |
| D 5 | 0.10 to 30.00 | 560 | layered | 100.0 in all | – |

The scores do not move from run to run by more than 0.1: Plasma has no randomness, and what
little they move comes from the sparkles and the random effects layered with it.

Two notes on reading them:

- A 251's 94.5 is not the effect's: 168 of its 1,168 pixels are the prop mentioned above, whose
  frames in xLights' file hold something else; its other pixels agree as closely as everywhere
  else.
- D 5 scores 100 although it differs pixel for pixel (see above): the look score does not ask a
  blob to be in the same place. So **the look score cannot tell a right Plasma from a slightly
  wrong one**. The comparison that guards this rewrite is the side-by-side with today's picture
  and the reference table, not this baseline.

---

## 13. What could not be pinned down

1. **The speckle that grows with the clock** (section 9, item 11) is measured in size but not
   described in a way that could be reproduced, on purpose.
2. **Whether xLights shows the same** for the four fixed colourings, for `twist` above 2, for
   `density` other than 1 and 6, for speeds above 44, for one- and two-colour palettes and for
   single rows and columns. No frame in the corpus has them. For all of these this document
   describes today's picture, which is what has to be matched.
3. **Buffers of more than about four million cells** were measured for their grid size (up to
   25 million cells, and lines of up to three million) but only spot-checked for colour.
4. **Why the group buffer of one corpus effect puts pixels a few rows from where xLights has
   them** (section 12). It is outside this effect.
5. **How often a pixel cycles** (sections 1 and 2) depends on which stretch of an effect it is
   counted over; the figures given are for the stretch named with them.

---

## 14. The reference table

`reference.tsv`, beside this file: what today's Plasma returns, exact to the last digit it
computes, for 50 settings spread over the ranges, the colourings, palettes and buffers. Tab
separated, one header line, then one line a sample:

`case`, `colors`, `twist`, `density`, `speed`, `columns`, `rows`,
`frame_ms`, `elapsed_ms`, `palette` (colours as `rrggbb` joined by commas, or `-` for none),
`u`, `v`, `r`, `g`, `b`, `a`.

- Each case has 25 positions at frames 0, 20 and 400 (`elapsed_ms = frame × frame_ms`); cases
  1, 2, 5, 12 and 16 also at frame 6,000, and case 15 at frame 1,500. 156 frames, 3,900 samples.
- In cases 1 to 46 the positions are those with `u` and `v` each one of 0, 0.25, 0.5, 0.75, 1.
  In cases 47 to 50 they are positions half way between two cells as nearly as single precision
  can say: `u` is `(k + ½) ÷ (C − 1)` as a single-precision number for five values of `k`, and
  `v` likewise. Every `u` and `v` in the file is printed so that reading it as a
  single-precision number gives back exactly the value that was used.
- Cases 1 to 5: the defaults in each colouring. 6 to 8: the settings and palettes of the real
  sequences, on buffers like theirs. 9 to 17, 35, 38 to 40, 42: the ends and middles of `twist`,
  `density` and `speed`. 18 to 21, 37, 41: palettes of two, one, none, eight, five and three
  colours. 22 to 28: one row, one column, one cell, and small buffers. 29, 30: buffers on which
  the quarter positions fall between cells. 31, 32: buffers above 1,048,576 cells. 33, 34:
  frame times of 25 and 20 ms. 36: speed 10.9, with the middle moment taken 37 ms into its
  frame. 43 to 46: buffers above 1,048,576 cells whose grid hangs on the rounding of section
  6.1 (1,039 by 1,039; 1,025 by 1,024; 1,303 by 5,212; 600,000 by 2). 47 to 50: the half-way
  positions of section 6.2, on buffers 11 by 11, 23 by 13, 101 by 51 and 400 by 178.
- What the table catches, tried by working the description wrongly on purpose and counting the
  rows that then fall outside 0.01: the product of section 6.2 in double precision, 170 rows
  (cases 47 to 50); halves to the even cell, 352 rows (cases 7, 8, 26, 32, 43, 44, 47 to 50);
  halves down, 570 rows; the grid of section 6.1 to the nearest, 129 rows (cases 43, 44, 46);
  in single precision, 106 rows (cases 43, 45); no coarser grid at all, 139 rows (cases 32, 44,
  45, 46).
- The largest clock value in the table is 500 (case 15 at frame 1,500), at `density` 2, so the
  speckle of section 9 is far below 0.001 in every row.
- The description of sections 3 to 6, worked in double precision, gives every row to within
  0.00015.

A new Plasma passes when every row is within 0.01 in every channel (property 24); it should in
fact be within 0.0005.

---

## 15. How this was measured

- **Frames** were drawn through the renderer's public interface (the effect built from its
  settings, a time, a palette and a buffer size, then asked for the colour at chosen positions),
  never by calling anything inside the effect.
- **The colour angle** was read from the `redGreen` colouring at `density` 1, whose red and green
  are the angle's raised sine and cosine, so each sample gives the angle outright, modulo a
  turn.
- **The waves were fitted, not read off.** 8,820 samples (49 positions of a 25 by 25 buffer; 180
  frames at speeds 0, 7, 50 and 100 and twists 1, 3, 4 and 10; clock values up to 13.3) were
  fitted with seven sines whose every number was free: 38 numbers in all (each wave's strength,
  its rates along the width, up the height and in time, a free phase offset for each, the
  swinging ripple's two turning rates, the twist rings' scale, its power of `twist`, the size
  and rate of its centre's wander each way, and the clock's three numbers), with the width
  ripple's rate in time fixed at 1 to set the clock's unit. Started from values up to 2% off at random, three starts of
  four converged to the same answer; the fourth stalled between the height swell and the
  diagonal swell, which are nearly alike over one buffer. Started 5% off it does not converge.
  At the answer the fit leaves 0.0000008 radian of colour angle on average and 0.000009 at
  worst.
- **What the fit returned**, against the value this document states:

  | Quantity | Fitted | Stated |
  |---|---|---|
  | Clock: frames counted from | 1.00000000 | 1 |
  | Clock: frames to a radian at speed 0 | 302.999998 | 303 |
  | Clock: fewer frames for each unit of speed | 2.99999998 | 3 |
  | Each wave's strength, radians of colour angle at density 1 (seven values) | 1.5707948 to 1.5707979 | a quarter turn, 1.5707963 |
  | Each wave's phase offset (seven values) | within 0.0000006 of 0 | 0 |
  | Width swell: radians a span; a radian of clock | 1.00000064; 0.99999997 | 1; 1 |
  | Height swell: the same | 0.50000031; 0.49999999 | ½; ½ |
  | Diagonal swell: a span of width; of height; a radian of clock | 0.49999950; 0.49999966; 0.49999999 | ½; ½; ½ |
  | Corner swell: a span from the corner; a radian of clock | 0.99999936; 1.00000004 | 1; 1 |
  | Width ripple: radians a span | 9.99999987 | 10 |
  | Swinging ripple: radians a span at full tilt; a radian of clock | 10.00000000; 0.99999999 | 10; 1 |
  | Swinging ripple: rate of `g`; rate of `h` | 0.50000000; 0.33333333 | ½; ⅓ |
  | Twist rings: 1 ÷ (ℓ² × twist) | 49.9999967 | 50 |
  | Twist rings: power of `twist` | 1.00000002 | 1 |
  | Twist rings: clock under the root, a radian of clock | 0.99999968 | 1 |
  | Twist rings: size of the centre's wander, each way | 0.50000001; 0.50000000 | ½; ½ |
  | Twist rings: rate of the wander, each way | 0.20000000; 0.33333333 | ⅕; ⅓ |

- **The description was then held against the effect** as a whole: worked in double precision
  and compared with rendered samples for 8,000 random combinations of colouring, twist, density,
  speed, palette (none, 1, 2, 3, 4, 5, 8 and 32 colours), 23 buffer sizes from 1 by 1 to 5,000
  by 5,000, frame times of 1 to 100 ms, moments up to frame 12,000, and positions on and off the
  grid, out of range and unusable: 480,000 samples. Largest difference in any channel:

  | Clock | Fixed colourings and palettes of up to 8 colours | A palette of 32 colours alternating black and white |
  |---|---|---|
  | below 10 | 0.00014 | 0.0007 |
  | 10 to 100 | 0.00027 | 0.003 |
  | 100 to 1,000 | 0.0012 | 0.006 |
  | 1,000 to 4,000 | 0.007 | 0.042 |

  Alpha was 1 in every sample. The 32-colour palette is the harshest test there is: it turns a
  small error in the angle into thirty-one times as much colour. These differences are today's
  speckle (section 9, item 11), not a shortfall of the description: no sample was further from
  the description than 0.95 of the amount given there. A separate search of the worst corners
  (density 10 with 3, 8 and 32 colours, at chosen clock values) gave the table in that item.
- **Everything in sections 4 to 7** (holding for a frame, the whole part of speed, the clock
  law, rows and columns, scaling with the buffer, the nearest cell, positions out of range, the
  coarse grid, how the colourings relate, what density does, how the palette is used) was measured
  directly on rendered frames as stated in the properties of section 8, each of which was run
  against today's effect and holds. The two places where rounding decides a cell were measured
  where it decides: 1,068 positions half way between cells as nearly as single precision can
  say, 2,047 exactly half way, and seventeen buffers above 1,048,576 cells (sections 6.1 and
  6.2).
- **The document was then tried.** It was handed, with the reference table and nothing else, to
  someone who had not seen the effect, its code, or any of the measuring. Their first build,
  finished before looking at any value in the table, matched all of the table's rows (3,300 at
  the time) to within 0.00015, and both worked examples of section 3. What they found unclear
  or wrong in the wording (how the grid of a very large buffer rounds, the product of section
  6.2, how the tolerance was stated, the period after which six of the waves repeat, and
  smaller points) has been put right above, measured where it needed measuring, and the table
  has cases 43 to 50 because of it.
- **xLights' frames** were read from the files xLights saved for the corpus sequences and
  compared with PixelFlow's export of the same sequences, pixel by pixel (section 12). No
  xLights source was read.

---

## After building it

The new Plasma was written from this description alone. Against the reference table, 3,895 of
3,900 samples have the identical 8-bit colour and five are one step off; those five are the old
picture's own speckle (the description worked out in double precision gives the same 3,895). All
26 properties hold.

Old and new were then drawn side by side: 414,196 cases, 13,048,188 frames, 9,188,584,667
pixel-frames, over every colouring, palettes of 0 to 64 colours, every setting through and beyond
its range, 400,000 random combinations, every frame of five- and ten-minute effects, and buffers
from one cell to billions. They differ by one mechanism only: the old picture's single-precision
speckle, which this description asked not to be copied. The new one follows the field described
here on every pixel. Below an internal clock of 100 (about 40 seconds at the highest speed with 20
ms frames, far longer at ordinary speeds) the largest difference is one step of 255 on any channel;
on the real sequences' designs 99.979% of pixel-frames are identical and the rest one step. Long
effects at the highest speeds with many alternating palette colours and high density differ more,
always toward this description.

What else turned out:

- **The allowance** for the old speckle (the exceptions section) is about half what it should be just above a
  power of two of the clock, where single-precision spacing doubles. The old picture exceeds it
  there by up to 1.7 times. A bound with four times the single-precision spacing at the clock
  always holds.
- **Rendered sequences** that blur a Plasma with the "Default Model As Pixel" style can differ by
  whole pixels downstream, because a later blur in the same frame reads the renderer's shared
  working space. That is the renderer, not Plasma: the old build does the same to a one-level
  change of its own palette.

It costs 11.5 ns a pixel where the old one cost 19.8, and requests no memory (the old one made
336 requests a frame). Every wave's angle is a whole number of steps of a turn, its sine comes
from a parabola across one of 512 arcs, and the clock is reduced to a turn once a frame, so the
picture no longer speckles as an effect runs on.

