# Pinwheel: behaviour specification

This document describes what the Pinwheel effect shows, so that it can be built afresh from this
description alone. It says what a viewer can see and what a test can measure on a rendered
frame. It does not say how to compute any of it: every method is the implementer's choice.

This is the description `crates/pf-render/src/pinwheel.rs` was built from, kept as it was
written. "Today's picture" below means the Pinwheel it replaced. Where building and measuring
showed the description wrong or incomplete, the correction is in section 13, not edited in
above; sections 10 to 12 record what was known before the new effect existed.

Where the picture drawn today has blemishes that come from the way it happens to be computed,
they are listed in section 8 so that they are **not** copied. Where something could not be pinned
down by description alone, it is in section 12, "To be measured".

Sequences imported from xLights carry their Pinwheel settings across unchanged, number for
number. So the meaning of each number (how many degrees, how many cells, how fast) is the
contract that keeps an imported show looking the same. Section 2 states that contract.

---

## 0. Conventions

**Buffer.** The effect is asked to draw on a buffer described as `C` columns by `R` rows (each at
least 1). It is then asked, pixel by pixel, for a colour. Each pixel has a position `(u, v)`:
`u` runs from 0 at the left to 1 at the right, `v` from 0 at the bottom to 1 at the top, as seen
from the front of the display. Pixels need not sit on a regular grid, there may be far fewer of
them than `C × R`, and several may share a position.

**Cell.** The unit of length is one cell: one column step, or one row step (they count as the
same length). A pixel is `u × (C − 1)` cells from the left and `v × (R − 1)` cells from the
bottom. A buffer one column wide has no horizontal extent (every pixel is at the same place
across); likewise one row high.

**Middle.** The middle of the buffer is `u = v = 0.5`.

**Angles.** In degrees, counterclockwise positive, measured from the direction pointing right:
0° is right (3 o'clock), 90° up, 180° left, 270° down. "Clockwise" and "counterclockwise" are
as a viewer facing the display sees them.

**Words used below.**

- *Centre*: the point the arms turn about.
- *Sector*: 360° divided by the number of arms. Each arm owns one sector.
- *Centre line* of an arm: the line (curve, with twist) down its middle.
- *Arm width*: how many degrees of its sector an arm fills, measured around the centre.
- *Arm length*: the distance from the centre at which arms end.
- *Leading edge / trailing edge*: the edge of an arm that faces the way the wheel turns, and the
  other one.
- *Alpha*: how much of a pixel the effect covers, 0 to 1. 0 is unlit: layers beneath show
  through untouched. Colour and alpha are separate; the colour is not darkened by alpha.
- *Lit*, when measuring: alpha of 0.5 or more.

---

## 1. What a viewer sees

### At default settings

Three thin straight arms reach from the middle of the buffer out to its corners' distance, like
three equally spaced clock hands. At the first moment one arm points straight down. They turn
together, counterclockwise, one full turn every 1.8 seconds, for as long as the effect lasts.
Each arm is a single colour from the palette (a new effect's palette is plain white, so all three
are white). Everything that is not on an arm is unlit, so whatever is on the layers beneath shows
between the arms.

### As the settings move

- **More arms**: more hands, always evenly spaced.
- **Thickness up**: each arm fattens from a line into a wedge, a slice of pie that is narrow at
  the centre and wide at the rim. At 50 the wedges and the gaps between them are equal. At 100
  the wedges touch and the whole disc is lit.
- **Arm length down**: the arms stop short of the corners; the wheel becomes a smaller disc.
  Up, beyond the buffer: nothing changes at first glance, but twist is spread over the longer
  arm, so less of it is on show.
- **Twist**: the arms bend into spirals, like a galaxy's. The further from the centre, the
  further round the arm has turned. The sign sets which way they curl.
- **Speed**: faster or slower turning. Zero holds the wheel still.
- **Counterclockwise off**: the wheel turns the other way.
- **Shading**: instead of a flat colour, each arm is graded across its width: bright down the
  middle, or bright at both edges, or bright on one edge fading to nothing at the other (a
  radar sweep with a tail).
- **Start angle**: the whole wheel starts turned by that many degrees.
- **Centre across / up**: the point the arms turn about slides away from the middle, as far as
  the buffer's edges. The arms stay the same length, so the far side may be left dark.
- **Drawing style**: "smooth" draws solid wedges. "Spokes" draws each arm as a fan of fine
  lines; close to the centre they merge into the same wedge, far from it they separate and the
  arm looks like a spray of thin rays. The two styles also differ in a few conventions (which
  way the first arm points, what "full length" means); each is spelled out below. Most imported
  sequences use spokes (section 10).

---

## 2. Settings

Values reach the effect already pulled into their ranges (a value that is not a number has been
replaced by the default). Keys are the names in the saved file. An xLights import fills each one
with the number from the xLights file, unchanged.

| Key | Type | Range | Default | Label |
|---|---|---|---|---|
| `arms` | whole number | 1 to 20 | 3 | Arms |
| `armSize` | number | 0 to 400, percent | 100 | Arm length |
| `twist` | number | −360 to 360, degrees | 0 | Twist |
| `thickness` | number | 0 to 100, percent | 0 | Thickness |
| `speed` | number | 0 to 50 | 10 | Speed |
| `counterclockwise` | on / off | | on | Counterclockwise |
| `shading` | choice | `flat`, `raised`, `sunken`, `sweep` | `flat` | Shading |
| `offset` | number | 0 to 360, degrees | 0 | Start angle |
| `centerX` | number | −100 to 100 | 0 | Center across |
| `centerY` | number | −100 to 100 | 0 | Center up |
| `style` | choice | `smooth`, `spokes` | `smooth` | Drawing |

### `arms`

The number of arms. Their centre lines are exactly one sector apart all the way round, for every
count from 1 to 20 (see section 8 for counts the current picture gets wrong).

### `armSize`: arm length

A percentage of a reference length that depends on the style.

- **Smooth**: the reference is half the buffer's diagonal, in cells (for `C` by `R`, half of the
  diagonal of a `C × R` rectangle). With the centre in the middle: at 100 the arms reach exactly
  to the corners, so the whole buffer is within reach; at 50 they reach half way to the corners;
  on a square buffer about 71 reaches the middle of each side.
- **Spokes**: the reference is half the buffer's longer side, in cells. With the centre in the
  middle of a square buffer: at 100 the arms reach the middle of each side and the corners stay
  dark; about 142 reaches the corners.

It scales linearly from 0 (nothing is drawn at all) to 400 (four times the reference). Arms
end at that distance: the end of an arm is an arc of a circle about the centre.

### `twist`

How far round the centre the far end of an arm is turned, compared with where it leaves the
centre. The turn grows in direct proportion to distance from the centre and reaches the full
`twist` angle at the arm's full length (wherever that is, on the buffer or beyond it). So at
±360 an arm of length 100 winds once round the centre between the centre and its tip; the same
twist at arm length 400 shows only a quarter of a turn within the first quarter of the arm.

Positive twist curls the outer parts counterclockwise, negative clockwise. This does not depend
on which way the wheel turns, or on the style.

### `thickness`

The share of its sector that each arm fills, as an angle. An arm of thickness `t` in a wheel of
`n` arms is `t` percent of `360° ÷ n` wide, centred on its centre line, at every distance from
the centre. Anchor points for four arms (sector 90°): 10 gives 9°, 25 gives 22.5°, 50 gives 45°,
100 gives 90° (arms touch; the disc is full).

The thin end of the range:

- **Smooth**: anything under 1 looks the same as 1 (an arm one hundredth of its sector wide).
  And wherever an arm would be narrower than about one cell, it is shown one cell wide, at full
  strength, so a thin arm is an unbroken line from the centre to its end and never fades out or
  breaks up. This holds with twist too.
- **Spokes**: at 0 each arm is a single line one cell wide.

### `speed`

How fast the wheel turns: 20 degrees a second for each unit. Anchor points:

| `speed` | Degrees a second | One full turn takes |
|---|---|---|
| 0 | 0 | never (still) |
| 1 | 20 | 18 s |
| 10 | 200 | 1.8 s |
| 18 | 360 | 1 s |
| 50 | 1000 | 0.36 s |

The rate is in real time. It does not depend on how long the effect is, or on the frame rate.

### `counterclockwise`

On: the wheel turns counterclockwise. Off: clockwise. In the smooth style, turning it off also
mirrors the layout (section 4 and 5 give the details); in the spokes style it reverses the
turning and nothing else.

### `shading`

How bright each arm is across its width (section 4). `flat`: the same all over. `raised`
("Bright middle"): brightest along the centre line. `sunken` ("Bright edges"): brightest at the
two edges. `sweep` ("Fading sweep"): brightest at one edge, fading to nothing at the other.

### `offset`: start angle

Turns the whole wheel from its starting position by this many degrees.

- **Smooth**: in the direction the wheel turns. With counterclockwise on, 90 moves the first arm
  from pointing down to pointing right; with it off, to pointing left.
- **Spokes**: always counterclockwise. 90 moves the first arm from pointing right to pointing up.

360 is the same as 0.

### `centerX`, `centerY`

Where the centre is. 0 and 0 is the middle of the buffer. −100 puts it on the left (bottom)
edge, 100 on the right (top) edge, and it moves evenly in between: 50 is half way from the
middle to the right edge. In `(u, v)` terms the centre runs from 0 to 1 as the setting runs from
−100 to 100. The centre never goes outside the buffer.

### `style`

`smooth` or `spokes`: two looks with slightly different conventions. Summary of the differences,
each described in full where it belongs:

| | Smooth | Spokes |
|---|---|---|
| An arm is | a solid wedge | a fan of one-cell-wide lines, one for each whole degree it covers (5.5) |
| First arm starts pointing | down (270°) | right (0°) |
| Arm length 100 means | half the buffer's diagonal | half the buffer's longer side |
| Next arms go round | clockwise (counterclockwise when turning clockwise) | counterclockwise |
| Start angle turns it | the way it rotates | counterclockwise |
| Sweep's bright edge | the leading edge | the counterclockwise edge |
| Thickness 0 | same as 1 | a single line |

An import from xLights takes `spokes` when the file says "Old Render Method" (or was saved before
the choice existed), and `smooth` otherwise.

---

## 3. Motion

**The only thing that moves by itself is the wheel's angle.** At time `t` after the effect
starts, the wheel has turned by its rate (section 2, `speed`) times `t`: counterclockwise or
clockwise as set. The whole wheel turns rigidly about the centre: arms, their colours, their
shading and their spiral shape all go round together. (In the spokes style the fine lines that
make up an arm do not go round with it; see 5.5.)

**Time.** The effect is given the time since it started in whole milliseconds, the effect's
length, and the sequence's frame time. The picture today changes only on whole frames: for the
length of a frame it holds. A new design may turn by the exact elapsed time instead; the two
agree whenever the elapsed time is a whole number of frames, which is when frames are drawn for
output, and the tests in section 7 only use such times.

**Nothing depends on the effect's length**, on where the effect sits in the sequence, on the
frame rate, or on any random number. Stretching an effect does not slow the wheel.

**Settings that change over the effect (value curves).** A curve on any number setting is
resolved before the effect is asked for a frame: the effect simply receives, for each frame, the
values the curves give at that moment, and draws the frame those values describe. It keeps no
memory of earlier values. What that looks like:

- a curve on start angle swings the wheel to and fro (with speed 0 it is the only motion);
- a curve on the centre slides the whole wheel about;
- a curve on twist winds and unwinds the spirals;
- a curve on thickness makes the arms swell and shrink;
- a curve on arm length makes the wheel grow and shrink;
- a curve on arms changes the count in whole steps (the value is rounded to a whole number
  before it arrives), so arms pop in and out;
- a curve on speed does **not** accumulate. The angle at any moment is that moment's rate times
  the time since the start. A speed that ramps up makes the wheel run ahead faster than a
  truly accelerating wheel would, and a speed that falls to zero at the end brings the wheel
  backwards to its starting position rather than letting it coast to a stop. This follows from
  the effect seeing only the current value; keep it.

Fractional values (10.5 degrees, 37.25 percent) are to be honoured as they are, so that
curve-driven motion is smooth. Today they are not (section 8).

**A frame is a pure function of the settings and the time.** The same settings, palette, buffer
size, pixel position and elapsed time always give the same colour, bit for bit, whatever was
drawn before, in whatever order frames or pixels are requested, on whatever thread. Scrubbing to
a moment shows exactly what playing through it shows.

**Fast wheels strobe.** At the usual 20 frames a second the wheel turns `speed` degrees between
frames (1° a frame for each unit of speed). When that approaches half a sector the wheel appears
to jump, stand still or run backwards, as a filmed wheel does. That is how the effect looks at
those settings; do not blur or smooth it away.

---

## 4. Colour

### Which palette colour is on which arm

The palette is an ordered list of 1 to 32 colours (an empty palette counts as one white).

Number the arms 1, 2, 3, …: arm 1 is the one that starts in the style's starting direction
(down for smooth, right for spokes, before start angle and rotation). The next arms follow one
sector apart:

- **smooth, counterclockwise on**: going **clockwise** round the wheel;
- **smooth, counterclockwise off**: going **counterclockwise**;
- **spokes** (either direction): going **counterclockwise**.

**Arm 1 takes the second palette colour**, arm 2 the third, and so on, wrapping round to the
first colour after the last. Examples:

| Palette | Arms | Colours of arms 1, 2, 3, … |
|---|---|---|
| red | any | all red |
| red, green, blue | 3 | green, blue, red |
| red, green, blue | 1 | green |
| red, green, blue | 2 | green, blue (red is not used) |
| red, blue | 5 | blue, red, blue, red, blue |
| (empty) | any | all white |

So with more colours than arms, some colours (the first among them) do not appear. Each colour
stays on its arm as the wheel turns.

Concretely, at the first moment with three arms and the palette red, green, blue, the arms are
centred at these angles:

| | green | blue | red |
|---|---|---|---|
| smooth, counterclockwise on | 270° | 150° | 30° |
| smooth, counterclockwise off | 270° | 30° | 150° |
| spokes | 0° | 120° | 240° |

### Shading: brightness across an arm

Shading changes alpha only. A lit pixel always carries its arm's palette colour at full value;
a "dim" part of an arm is the same colour with lower alpha, so it is partly see-through over
other layers and simply dimmer over black.

Take a position across the arm from 0 at one edge to 1 at the other, measured as an angle about
the centre. Brightness depends only on that position. It is the same all the way along the arm:
with no twist, two points on the same line from the centre have the same alpha.

| Shading | Alpha across the arm |
|---|---|
| `flat` | 1 everywhere |
| `raised` | 1 on the centre line, falling in a straight line to 0 at each edge (a triangle) |
| `sunken` | 0 on the centre line, rising in a straight line to 1 at each edge (a V) |
| `sweep` | 1 at the bright edge, falling in a straight line to 0 at the other edge (a ramp) |

Averaged across an arm, each of the three shaded styles is half as bright as flat.

Which edge of a sweep is bright:

- **smooth**: the leading edge, whichever way the wheel turns. The arm looks like a beam with a
  tail fading behind it.
- **spokes**: the counterclockwise edge, whichever way the wheel turns. So with counterclockwise
  on it looks the same as smooth; with it off the bright edge trails and the arm brightens
  towards its back.

Where an arm is so thin that it is shown as a one-cell line (section 2, `thickness`), the line
is at full alpha in every shading.

At thickness 100 the arms touch. Flat: the disc is fully lit with no dark seam between
neighbouring arms (with one colour it is a plain disc and its turning cannot be seen). Raised:
bright ridges with dark seams where arms meet. Sunken: the reverse. Sweep with one arm: a full
circle graded once round, bright at the leading edge and fading all the way round to nothing just
behind it.

### What is left unlit

- Everything outside the arms, and everything beyond the arm length, has alpha exactly 0:
  layers beneath show through.
- Inside a flat arm alpha is 1: the arm covers what is beneath it. This is so even when the
  palette colour is black (a black arm blacks out what is under it).
- Shaded arms are partly see-through according to the table above.

Fades, sparkles, blur, colour adjustments and the blending with other layers are applied
afterwards by the renderer and are not the effect's business. One consequence to keep in mind:
the renderer may flash a sparkle, at full strength, on any pixel the effect has lit even
faintly, and the fidelity measure counts a pixel as lit from about a tenth of full brightness.
So light that spills beyond an arm, however dim, is not harmless (section 9).

---

## 5. Geometry

### 5.1 The centre

With `centerX` and `centerY` at 0 the centre is the exact middle of the buffer, `u = v = 0.5`.
The settings move it in proportion to the buffer's extent: ±100 is the left/right (bottom/top)
edge. All angles and distances in this document are taken from the centre, wherever it has been
put. With the centre on an edge, half the wheel is off the buffer and arms may not reach the far
side.

### 5.2 Distances are in cells; the wheel is round in cells

Distances from the centre are measured in cells, with a column step and a row step counting the
same. The wheel is therefore a true circle on the cell grid whatever the buffer's proportions: on
a buffer 101 columns by 41 rows, a wheel whose arms are 16 cells long is 33 cells wide and 33
cells tall. It is not stretched to fit. (If the display's cells are not square, the wheel looks
stretched to the same degree on the display; that is the buffer's doing.)

On a buffer much wider than tall, a centred wheel at arm length 100 has arms long enough to
reach the corners, so most of each arm is cut off above and below.

### 5.3 Where the arms start

Before rotation and start angle:

- **smooth**: arm 1's centre line points straight down (270°);
- **spokes**: arm 1's centre line points right (0°).

### 5.4 Turning direction off, in the smooth style

In the smooth style with no twist, the picture with counterclockwise off is the left-to-right
mirror image (about the vertical line through the centre) of the picture with it on, at every
moment: arm positions, the order of colours, the bright edge of a sweep, the direction start
angle turns the wheel, and the direction of rotation are all mirrored. Twist is the one thing
that does not mirror: positive twist curls counterclockwise either way.

In the spokes style, turning it off reverses the rotation only. The first frame is the same
either way.

### 5.5 What the spokes style looks like, exactly

In the spokes style, light appears only along a fixed set of straight directions out from the
centre: one for every whole degree (0°, 1°, 2°, … 359°), each a line about one cell wide. An arm
lights those of the lines that fall within its width at each distance (with twist, the arm's
position shifts with distance, so the lit stretch of each line starts and stops accordingly). An
arm of thickness 0 lights the single line nearest its centre line. The number of lines in an arm
is its width in whole degrees, plus one: six arms at thickness 50 are 30° wide and show 31
lines each.

What this looks like depends on distance from the centre:

- **Within about 57 cells** (where one degree of arc is less than a cell) neighbouring lines
  overlap and the arm is a solid wedge. On buffers up to about 115 cells across, with the centre
  in the middle and arm length 100, the spokes style is solid everywhere.
- **Further out** the lines separate. At a distance of `r` cells an arm lights roughly a
  fraction 57 ÷ `r` of the pixels a solid wedge would: about half at 115 cells, a third at 170.
  The arm reads as a spray of fine straight rays with dark between them.
- The lines do not turn with the wheel and do not bend with twist. As the wheel turns, an arm's
  light passes from line to line; pixels that lie between two lines far from the centre stay
  dark for the whole effect. The fan of lines moves only when the centre moves.

A lit point on a line has the brightness the arm's shading gives for that point's position
across the arm (section 4); with no twist that is one brightness for the whole of each line.
Where several lines cover the same pixel (near the centre), the pixel likewise takes the shading
for its own position across the arm.

Whether this ray structure is kept is the one open question that affects import fidelity; see
sections 8 and 12. Until it is settled, treat it as part of the look.

### 5.6 The middle of the wheel

All arms meet at the centre. A pixel at the centre (within half a cell of it) is lit whenever
the arm length is more than zero. Its colour is one of the arms' colours; which one is not
specified, only that it is always the same for the same settings.

### 5.7 A single row or column of pixels

A buffer one row high is a horizontal line through the centre (a one-row buffer has no height,
so `centerY` does nothing). Its pixels see only two directions, 0° to the right of the centre
and 180° to the left.

- **Smooth**: the half of the line to the right lights, all at once, while an arm lies over 0°,
  and goes dark when it has passed; likewise the left half and 180°. An arm that is `w` degrees
  wide keeps a half lit for the time the wheel takes to turn `w` degrees. With an even number of
  arms both halves light together. With twist the picture changes character: arms cross the line
  at a distance that changes with time, so bands of light travel along the line, outwards or
  inwards.
- **Spokes**: the same, except that as an arm turns away from the line its light draws back
  towards the centre over a few frames instead of going out at once (a line at a slight angle
  to the row still covers the pixels nearest the centre).

A buffer one column wide behaves the same way for 90° (above the centre) and 270° (below).

### 5.8 Very small buffers

On buffers a few cells across the picture is the same picture, sampled at very few points: there
is no special rule. Two consequences:

- A one-pixel buffer is a single pixel at the centre: steadily lit (5.6) whenever arm length is
  above zero.
- Thickness 100 at arm length 100 in the smooth style lights every pixel of a buffer of any
  size.

---

## 6. Edge cases

**Arms**
- 1: a single arm. At thickness 100 it fills the whole circle in one colour.
- 20: sectors are 18° wide. At low thickness the arms are well under a degree wide and show as
  one-cell lines.
- Counts that do not divide 360 evenly (7, 11, 13, 14, 16, 17, 19): evenly spaced like any
  other.

**Arm length**
- 0: nothing is lit anywhere.
- Small enough that the arm is under a cell long: only pixels within that distance of the centre
  can be lit; possibly none but the one at the centre.
- Above what reaches the far corner: no visible difference except through twist.

**Twist**
- 0: straight arms.
- ±360: a full turn between the centre and the arm's end. At high twist and high thickness
  neighbouring turns of the spiral are close together; thin arms must stay unbroken lines.
- Twist with arm length 0: nothing is drawn.

**Thickness**
- 0: section 2 (thinnest arms, never invisible, in every shading and both styles).
- 100: section 4 (arms touch, no gaps).

**Speed**
- 0: every frame is identical unless a curve changes a setting.
- 50 at 20 frames a second: 50° between frames; strobing is expected (section 3).
- Very long effects (hours): the wheel keeps exact time; no jitter or drift creeps in.

**Start angle**
- 0 and 360 are the same picture.

**Centre**
- ±100: the centre is on the buffer's edge; half the wheel is visible.
- On a dimension one cell wide the setting for that dimension does nothing.

**Shading**
- Any shading at thickness 0: the arms are still visible at full strength.
- Raised at thickness 100: alpha falls to 0 where arms meet.

**Palette**
- Empty: white. One colour: every arm that colour. More colours than arms: section 4.
- A black colour gives an opaque black arm.

**Pixels**
- Positions are always within 0 to 1. A pixel whose position is not a finite number: treat it
  as being at 0 on that axis. Never fail, and never return a colour or alpha that is not a
  finite number between 0 and 1.

---

## 7. Acceptance properties

**The test rig.** Unless a property says otherwise:

- Buffer 101 columns by 101 rows. Palette red (255, 0, 0), green (0, 255, 0), blue (0, 0, 255),
  in that order. Effect 10 seconds long, frame time 50 ms. Times are milliseconds since the
  effect's start and are whole multiples of the frame time. Settings are at their defaults
  except those named.
- A *sample* at distance `r` cells and angle `θ` from the wheel's centre is the colour returned
  for a pixel at that exact position (positions between cell centres are allowed; on the
  101-square rig one cell is 0.01 in `u` or `v`).
- The *ring at `r`* is the 720 samples at distance `r` and angles 0°, 0.5°, 1°, …. A *run* is a
  maximal stretch of neighbouring lit samples (the ring wraps). A run's *centre* is the angle
  half way along it and its *width* the angle it spans.
- *Lit* means alpha ≥ 0.5. *Smooth arm length* on this rig at `armSize` 100 is 71.4 cells;
  *spokes arm length* is 50.5 cells.
- "±3°" allows for one cell of position (1.4° at 40 cells) plus measuring error.

Properties marked **(new)** are not met by today's picture; they are where the new design is
expected to be better (section 8). All the others describe the look that imports rely on.

1. **Unlit is clear; lit is the palette colour.** With 4 arms, thickness 50, flat shading,
   among samples within 45 cells of the centre: every sample more than 2 cells outside every arm
   has alpha exactly 0, and every sample more than 2 cells inside an arm has alpha exactly 1 and
   red, green and blue exactly equal to that arm's palette colour. Both styles.

2. **Arm count and even spacing.** For `arms` in 1, 2, 3, 4, 5, 6, 8, 12, 20, with thickness 30,
   speed 0, a one-colour palette, in both styles: the ring at 40 has exactly that many runs, and
   neighbouring run centres are one sector apart, ±3°. **(new)** The same holds for 7, 11, 13,
   14, 16, 17 and 19.

3. **Thickness sets the angular width.** With 4 arms, speed 0, and thickness 10, 25, 50, 75:
   every run on the ring at 40 is that percentage of 90° wide (9°, 22.5°, 45°, 67.5°), ±3° in
   the smooth style and ±4° in the spokes style.

4. **The lit share does not depend on resolution (smooth).** With 4 arms, flat, speed 0, on
   buffers 51, 101 and 201 cells square: the mean alpha over the ring at 60% of the arm length
   is 0.25 ± 0.03 at thickness 25 and 0.50 ± 0.03 at thickness 50. With raised, sunken or sweep
   shading it is half of that, ± 0.04.

5. **Full thickness leaves no gaps.** Smooth, thickness 100, 3 arms, flat: every sample closer
   to the centre than the arm length less one cell has alpha ≥ 0.99, including on the boundaries
   between arms; the ring at 40 shows three stretches of colour, each 120° ± 3°. Spokes,
   thickness 100: every sample within 49 cells of the centre is lit.

6. **Thin arms never vanish. (new)** For thickness 0 and 1, `arms` 1, 3, 12 and 20, twist 0 and
   360, each of the four shadings, both styles: at every whole distance from 3 cells out to two
   cells short of the arm's end (or the buffer's edge), each arm has a sample of alpha ≥ 0.5
   within 1.5 cells of its centre line. In the smooth style the frames for thickness 0 and
   thickness 1 are identical.

7. **Thin arms stay thin.** Defaults (3 arms, thickness 0), speed 0, smooth: on the ring at 20
   each run is between 0.7 and 2.5 cells of arc wide (2° to 7.2°).

8. **Starting direction.** 1 arm, thickness 10, speed 0: the single run on the ring at 40 is
   centred at 270° ± 3° in the smooth style and at 0° ± 3° in the spokes style.

9. **Rate and direction of rotation.** 1 arm, thickness 10, and speed 5, 10 and 25: at 400 ms
   the run on the ring at 40 is centred 40°, 80° and 200° counterclockwise of where it was at
   0 ms (± 3°) with counterclockwise on, and the same amounts clockwise with it off. Both
   styles. At speed 10 the frame at 1800 ms matches the frame at 0 ms (no pixel's alpha differs
   by more than 0.02).

10. **Speed 0 is still.** With speed 0 the frames at 0, 1000 and 7000 ms are identical bit for
    bit, for any other settings.

11. **A frame depends on nothing else.** For fixed settings, palette, buffer size and elapsed
    time, every pixel's colour is identical bit for bit: whether the effect is 1 second or 60
    seconds long; wherever it starts in the sequence; whatever random seed the renderer supplies;
    in whatever order pixels and frames are requested; and when pixels are requested from
    several threads at once.

12. **Start angle.** 1 arm, thickness 10, speed 0, `offset` 90. Smooth: the run on the ring at
    40 is centred at 0° with counterclockwise on and at 180° with it off. Spokes: at 90° either
    way. All ± 3°. `offset` 360 gives the same frame as `offset` 0.

13. **Centre.** Smooth, 1 arm, thickness 100, `armSize` 20 (a disc 14.3 cells in radius), one
    colour. For (`centerX`, `centerY`) = (0, 0), (50, 0), (0, −50), (−50, 50): the centroid of
    the lit pixels is at `u = 0.5 + centerX/200`, `v = 0.5 + centerY/200`, within one cell. For
    `centerX` = 100: no pixel more than 16 cells from the right edge is lit; the right-hand
    column is lit over a stretch 28.6 ± 2 cells long, centred on `v = 0.5` within one cell.
    The same for −100 on the left, and for `centerY` at the top and bottom.

14. **Arm length.** One colour, thickness 100, centre in the middle. Smooth: at `armSize` 10,
    25 and 50 the lit pixels are exactly those within 7.1, 17.9 and 35.7 cells of the centre,
    ± 1.5 cells; at 100 every pixel of the buffer is lit; at 0 none is. Spokes: at 50 the lit
    pixels are those within 25.3 cells, ± 1.5; at 100 the middle pixel of each side of the
    buffer is lit and the four corner pixels are not; at 150 the corners are lit too.

15. **Round, not stretched.** Buffer 101 by 41, smooth, thickness 100, `armSize` 30 (arm length
    16.4 cells): the lit region is 33 ± 3 cells wide and 33 ± 3 cells tall.

16. **Twist.** 1 arm, thickness 10, speed 0. Smooth: with twist 90 the run on the ring at 20 is
    centred 25° counterclockwise of where it is with twist 0, and on the ring at 40, 50°
    counterclockwise; with twist −90, the same amounts clockwise. Spokes: 36° and 71°. All ± 4°.
    None of these changes when counterclockwise is turned off.

17. **Which colour on which arm.** 3 arms, thickness 30, speed 0. The runs on the ring at 40
    are centred, ± 3°, at: smooth with counterclockwise on, green 270°, blue 150°, red 30°;
    smooth with it off, green 270°, blue 30°, red 150°; spokes either way, green 0°, blue 120°,
    red 240°. With the palette red alone every run is red; with an empty palette every run is
    white; with red, green, blue and 1 arm the run is green; with 2 arms the runs are green and
    blue and no sample anywhere is red.

18. **Colours ride with their arms.** 3 arms, thickness 30, speed 10, smooth, counterclockwise
    on, at 600 ms (a third of a turn): green is centred at 30°, blue at 270°, red at 150°, ± 3°.

19. **Shading profiles.** 2 arms, thickness 40 (arms 72° wide), speed 0; read the alpha on the
    ring at 40 across the arm centred at 270° (smooth) or 0° (spokes):
    - flat: 1 throughout;
    - raised: ≥ 0.9 at the centre; 0.5 ± 0.1 a quarter and three quarters of the way across;
      ≤ 0.2 at 3° inside each edge;
    - sunken: ≤ 0.1 at the centre; 0.5 ± 0.1 at the quarters; ≥ 0.8 at 3° inside each edge;
    - sweep: ≥ 0.85 at 3° inside the bright edge; 0.5 ± 0.1 at the centre; ≤ 0.15 at 3° inside
      the other edge; never rising by more than 0.05 from one sample to the next going from
      bright to dark.

    The sweep's bright edge is the counterclockwise edge in the smooth style with
    counterclockwise on, the clockwise edge in the smooth style with it off, and the
    counterclockwise edge in the spokes style either way. In every case red, green and blue of a
    lit sample equal the palette colour exactly. With twist 0, the alpha at the same angle on
    the rings at 15, 30 and 45 (smooth) agrees within 0.1.

20. **Mirror image (smooth).** 3 arms, thickness 30, twist 0, centre in the middle, at 0, 150
    and 400 ms: flipping the counterclockwise-on frame left to right gives the
    counterclockwise-off frame; at most 4% of pixels differ in lit state or colour.

21. **Spokes: rays separate with distance.** Buffer 401 by 401, spokes, 4 arms, thickness 100,
    `armSize` 100, speed 0, one colour. The share of lit samples on a ring is ≥ 0.98 at 40
    cells, between 0.40 and 0.70 at 115 cells, and between 0.22 and 0.40 at 190 cells. The ring
    at 150 cells, sampled at 7200 points, shows 360 ± 20 separate runs. *(Provisional: stands or
    falls with the open question in section 12.)*

22. **Spokes: the rays stay put.** Same buffer and settings but speed 1 and frame time 25 ms:
    among the pixels between 100 and 190 cells from the centre, at least 97% have the same lit
    state at 0 ms and at 125 ms (a turn of two and a half degrees). *(Provisional, as 21.)*

23. **A single row and a single column.** 2 arms, thickness 20, speed 10, sampled every 50 ms
    from 0 to 850 ms.
    Buffer 41 by 1 (pixels at `u` = 0, 1/40, … 1, all at `v` = 0.5), smooth: all 41 pixels are
    lit in 3 to 5 consecutive samples centred on 450 ms (± 50 ms), and in every sample 150 ms or
    more outside that stretch at most 5 pixels are lit. Buffer 1 by 41, smooth: the same, with
    the fully lit stretch centred on 0 ms (and returning at 900 ms).
    Spokes, 41 by 1: at least 39 pixels lit at 0 ms and at most 4 at 450 ms; 1 by 41: the
    reverse.

24. **Tiny buffers and hostile input.** On buffers 1 by 1, 2 by 1, 2 by 2 and 3 by 3, with arms
    20, `armSize` 400, twist −360, thickness 100, speed 50, at 0 ms, 50 ms and one hour: no
    failure, and every colour and alpha returned is a finite number from 0 to 1. Smooth,
    thickness 100, `armSize` 100: every pixel of each of those buffers is lit. A pixel whose
    `u` or `v` is not a finite number does not cause a failure.

25. **Fractional settings and soft edges. (new)** Smooth, 1 arm, thickness 50, speed 0. As
    `offset` goes from 0 to 5 in steps of 0.1, the alpha of the sample at 40 cells and 2.5°
    counterclockwise of the arm's starting leading edge rises from 0 to 1 without ever falling
    by more than 0.01, and takes at least five different values on the way. The band of samples
    across any arm edge whose alpha is strictly between 0 and 1 is never more than 1.5 cells
    wide.

---

## 8. Symptoms not to reproduce

Each of these is something that can be seen in today's picture and that comes from how it is
worked out, not from what the effect is meant to show. The new design is free to do better, and
should. They are described only by what is seen.

**Both styles**

1. *Settings snap to whole numbers.* A twist of 10.9° is drawn as 10°, a centre at 12.7 as 12,
   a speed of 3.8 as 3. Fractions are dropped towards zero. With a curve on a setting the
   wheel therefore moves in visible steps instead of gliding. (Curves on centre, twist and start
   angle are in real use; section 10.)
2. *Uneven arms.* With 7, 11, 13, 14, 16, 17 or 19 arms the spacing is a whole number of degrees
   and the remainder (3° to 18°) is left over in one place. In the spokes style one gap is
   visibly wider than the rest. In the smooth style there is a seam on the line straight down
   from the centre where the pattern breaks, the whole wheel sits a few degrees off its proper
   starting position, thin stray lines appear beside the arms, and 19 arms shows as 20.
3. *The centre is off by half a cell.* In the smooth style the wheel sits half a cell up and to
   the right of the true middle. In the spokes style it sits half a cell down and to the left on
   buffers with an odd number of cells (and in the right place on even ones). So the picture is
   not symmetric, which shows on small buffers. Centre settings and arm lengths also snap to
   whole cells.
4. *Each pixel takes the colour of the nearest grid point*, not of its own position. Props whose
   pixels do not sit on the grid get a blocky sampling, and pixels sharing a grid point always
   match.
5. *On buffers of more than about a million cells the picture is drawn coarser* than the buffer.
6. *The picture changes only on whole frames.* Not visible at the sequence's own frame rate.
7. *Angles are off by a few ten-thousandths of a degree.* Not visible; no need to match.

**Smooth style**

8. *Arm widths come in whole degrees*, and each arm is up to a degree wider than its thickness
   asks.
9. *Shading is banded.* Raised, sunken and sweep change brightness in one-degree steps, which
   show as stripes several cells wide far from the centre.
10. *Thin arms are ragged.* The one-cell line that keeps a thin arm visible sits at a
    whole-degree angle, so it jumps a degree at a time as the wheel turns and is stair-stepped.
    With twist it breaks into dots and dashes towards the tips (and the arm proper, being slanted
    by the twist, is thinner than a cell there and also breaks up). The line is half a cell out
    of place against the rest of the arm on odd-sized buffers, and it is drawn at full
    brightness whatever the shading, including down the middle of a sunken arm.
11. *The arm's end is a hard, stair-stepped circle* at a whole number of cells.

**Spokes style**

12. *The wheel turns in whole-degree steps.* At slow speeds and short frame times it holds for
    two or three frames and then jumps. Within the solid part of the wheel nothing calls for
    this. (Beyond it, light can only move from one fixed ray to the next: see item 16.)
13. *Near the centre, shading takes the value of one edge.* For the first several cells out
    from the centre a raised arm is dark and a sweep is uniformly bright, instead of following
    their profiles. The pixel at the very centre takes the colour of the highest-numbered arm,
    and where arms touch at thickness 100 the line they share shows the higher-numbered arm's
    colour.
14. *Thickness 0 with raised or sweep shading draws nothing at all.* The single line gets the
    brightness of the dark edge.
15. *A one-pixel buffer stays dark*, because the arm length rounds down to a whole number of
    cells (on small buffers that is a large part of it).
16. *The gaps between rays far from the centre* (5.5), and what follows from them (an arm's
    light there moving a whole degree at a time, and twisted arms there being made of short
    straight dashes instead of curves), arise the same way as the rest of this list: nobody
    chose them. But unlike the rest they are large, and they are in the reference render that
    imports are scored against. Making the spokes style solid everywhere would light up to twice
    as many pixels (or more) on the outer parts of buffers wider than about 115 cells, and make
    those parts correspondingly brighter. **That is large enough to move the look score**
    (coverage and brightness) for imported spokes-style Pinwheels on large props and groups.
    Section 5.5 therefore describes the rays as part of the look, and section 12 asks for the
    measurement that decides it. What is free in any case: which exact pixels a ray lights, and
    anything about it that is finer than a cell.

---

## 9. Goals for the new design

Stated as results. How to reach them is open.

1. **Edges that move smoothly.** As an arm's edge crosses a pixel, the pixel's alpha should pass
   through intermediate values in proportion to how much of the pixel's cell the arm covers, so
   that slow wheels glide instead of popping from pixel to pixel. This applies to the sides of
   an arm and to its end.
2. **No spilled light.** Soft edges must not widen the arms. A pixel should get any alpha at all
   only if the arm actually overlaps its cell; the partly lit band across an edge should be no
   more than about a cell and a half wide; and, apart from the one-cell minimum for thin arms,
   the total light of an arm should be what its geometry says. A one-cell line should stay a
   one-cell line (lighting at most two pixels across where it falls between them, never
   three). See the note on sparkles and on the
   fidelity measure's idea of "lit" at the end of section 4.
3. **Each pixel judged at its own position.** No snapping to grid points, so that the picture is
   the same shape at any resolution and on props whose pixels are irregular.
4. **Fractions honoured** in every number setting, so that curves give smooth motion.
5. **Even spacing for every arm count.**
6. **True centre, true symmetry.** A centred wheel of `n` arms on a square buffer has exact
   `n`-fold symmetry about the buffer's middle, for odd and even buffer sizes alike.
7. **Cost that does not grow with the number of arms, the thickness, the twist or the arm
   length**: a fixed, small amount of work for each pixel asked for, and a small fixed amount of
   work once a frame. Today's measured cost on the benchmark show is about 10 nanoseconds a
   pixel, plus on average about 19 KB of memory requested and released for each effect on each
   frame. The target is to be faster for each pixel and to request no memory at all while
   drawing.
8. **The same output regardless of thread, pixel order or frame order**, bit for bit
   (property 11). No shared state that changes while drawing.
9. **Stable over long effects.** After hours of turning the wheel is as sharp and as steady as in
   its first second.
10. **Time used as given.** The wheel may turn by exact elapsed time, so that a preview drawn
    more often than the sequence's frame rate is smooth too; frames drawn on whole frame times
    must come out the same as they do counting whole frames.
11. **Nothing invisible, nothing surprising at the extremes.** Thin arms visible in every
    shading and style; no setting combination that makes the effect disappear other than arm
    length 0.

Not goals: motion blur; changing which palette colour lands on which arm; changing the starting
directions, the meaning of arm length in either style, or the direction conventions in section 2.
Saved sequences and imports depend on those.

---

## 10. How it is used in practice

From the seven real sequences the fidelity corpus is built on (counts and values only).

- Two of the seven contain Pinwheels: **11 effects** in all (10 in one sequence, 1 in another).
- They are three designs, each repeated on different targets:

| | Design A | Design B | Design C |
|---|---|---|---|
| Effects | 5 | 5 | 1 |
| Style | spokes | spokes | smooth |
| Arms | 12 | 6 | 1 |
| Arm length | 100 | 400 | 181 |
| Thickness | 1 | 50 | 5 |
| Twist | curve, within −360 to 360 | −360 | 0 |
| Speed | 4 | 6 | 0 |
| Counterclockwise | on | on | on |
| Shading | flat | sweep | flat |
| Start angle | 0 | 0 | curve, 0 to 360, following a timing track |
| Centre across | curve, within −100 to 100 | 25 | 0 |
| Centre up | curve, within −100 to 100 | 0 | 100 |
| Palette | one colour (white) | one colour | one colour |
| Length | 15.65 s | 28.8 s | 16.3 s |
| Frame time | 50 ms | 50 ms | 20 ms |

What that says about priorities:

- **Spokes is the style that matters most for imports: 10 of 11.**
- **Every one has a single palette colour.** Which colour lands on which arm is never exercised.
- **Every one turns counterclockwise** (or stands still). The clockwise conventions are never
  exercised.
- **Thin arms are common**: 5 of 11 at thickness 1 with 12 arms (single lines), 1 at 5.
- **Settings in use vary over**: arms 1, 6, 12; arm length 100, 181, 400; thickness 1, 5, 50;
  twist −360, 0, and a curve over the full range; speed 0, 4, 6; shading flat (6) and sweep (5);
  centre across 0, 25 and a curve; centre up 0, 100 and a curve; start angle 0 and a curve.
  Never used: raised, sunken, clockwise, more than one colour, speed above 6, a curve on speed,
  thickness, arms or arm length.
- **Curves are common** (6 of 11 have at least one): on centre across and up and on twist
  together (5), on start angle (1).
- The targets are large: between about 480 and 5,000 pixels each. None is known to be a single
  row or a handful of pixels.
- Around the effect: all 11 carry sparkles (at 6, 42 and 88 of 200); 10 fade in and out over 1
  to 2 seconds; 5 are blended so that they show only where the layers beneath are dark.

---

## 11. Fidelity baseline

How today's Pinwheel scores against the reference renders of those sequences. "Look" is 0 to
100; 85 or more is a match, 55 or more is close. The new Pinwheel must not fall below these.

- **Pinwheel as a kind: look 93** (94 on an immediate re-run of the same code; other effects in
  the same sequences are random, which moves it by about a point). Verdicts: **4 match, 1
  close, 0 different**.
- Parts: coverage 0.97, brightness 0.96, colour 0.99, pattern 0.99, **motion 0.86** (the
  weakest), rhythm 0.98.
- Of the 11 effects, 5 could be judged, and only 1 of those had pixels to itself; the other 4
  were judged together with whatever was layered on the same pixels. The other 6 could not be
  judged.

| Design (section 10) | Judged | Look | Weakest part | Notes |
|---|---|---|---|---|
| A (12 thin arms, spokes) | layered | 100.0 | – | |
| A | layered | 99.8 | – | |
| B (6 swept arms, spokes) | layered | 95.9 | motion 0.91 | |
| B | layered | 68.0 (close) | motion 0.32 | "changes less from frame to frame"; 1.3 times as bright as the reference; brightness 0.88, pattern 0.82 |
| C (1 arm, smooth) | alone | 92.9 | motion 0.85 | lights 11.8% of the target where the reference lights 9.0% |

- Pixel for pixel (a diagnostic, not a target): where it had pixels to itself, mean error 3.5 of
  255, 97% of pixels within tolerance, and the lit pixels of the two renders overlap by 0.77.
- The importer expects every one of the 11 to match.

---

## 12. To be measured

These could not be settled by description. Each needs frames to be rendered and looked at or
scored.

1. **Do the rays of the spokes style matter on the real targets?** How many cells across are
   the buffers that designs A and B play on, and so how much of each lies beyond about 57 cells
   from the wheel's centre? Score design B drawn (a) with separated, fixed rays as in 5.5 and
   (b) solid, against the reference. This decides whether section 5.5 and properties 21 and 22
   stand.
2. **Why one instance of design B scores 68 while the same settings on a larger target score
   96.** On the lower one today's render is brighter and steadier than the reference. Whether
   that is the rays (the two renderers' buffers for that target having different numbers of
   cells), or the layers it shares pixels with, needs frames side by side.
3. **How wide a ray is in the reference, and how bright the crowded region near the centre is**
   under sweep shading. Section 5.5 says "about one cell" and "the shading at the pixel's own
   position"; the first is right to within a cell, the second is a choice (see symptom 13).
4. **Soft edges against coverage and sparkles.** Design A is five instances of one-cell lines,
   scoring 100 today, all with sparkles. Measure the count of lit pixels (above a tenth of full
   brightness) and the look score for those instances with the new edges.
5. **The one instance judged alone (design C) is a still wheel moved only by a curve on start
   angle that follows a timing track.** Its weakest part is motion and it lights about 30% more
   of the target than the reference does. That probably comes from how the curve is carried
   over, not from the effect. Confirm by checking that the new Pinwheel scores the same there
   as the old.
6. **Single rows, single columns and buffers a few cells across.** Section 5.7 and 5.8 give the
   proposed behaviour, and property 23 its measurable outline. No corpus sequence is known to
   use Pinwheel that way, so there is no reference to compare with; judge by eye.
7. **Clockwise, and more than one colour.** The corpus has neither, so the conventions in
   sections 2, 4 and 5.4 for those cases have no reference render behind them here. If a
   reference export with a clockwise spokes sweep and a three-colour palette can be made, check
   properties 17 and 19 against it.
8. **Re-run the baseline immediately before and after the change**, on the same machine, since
   the layered scores move by a point or so from run to run.

---

## 13. Corrections and answers, after building it

The new Pinwheel was scored against xLights' own rendered frames of the sequences in section 10,
and then drawn side by side with the old one on about 4,600 settings. What that changed:

### Where the description was wrong or incomplete

1. **The centre (sections 2 and 5.1).** The centre settings move the centre over the buffer's
   whole extent of `n` cells, not over the `n − 1` steps between the first and last pixel. At 0 it
   is the middle either way; at ±100 it is on the buffer's outer edge, half a cell beyond the
   last pixel's own place. xLights' frames fit this reading (99.5% of pixels against 94.8%, on a
   wheel a quarter of the way across a 400-cell buffer). Across a dimension one cell wide the
   setting still does nothing (section 5.7).
2. **Shading in the spokes style (section 4).** An arm that lights `m` rays shades them by ray:
   the `k`-th ray, counting from 0 at the clockwise edge, takes the shading for position `k/m`
   across the arm. So in a sweep the ray on the dark edge is unlit and the brightest ray is at
   `(m − 1)/m`, not full. An arm of a single ray is at full strength in every shading (xLights
   leaves it dark: symptom 14).
3. **Rays (section 5.5).** A ray is a string of marks half a cell apart, each lighting the cell
   it falls in. Where an arm's edge lies exactly on a ray, the ray on its counterclockwise edge is
   lit and the one on its clockwise edge is not. Near the centre, where rays share cells, the arm
   is solid and half a cell wider each side than its angle.
4. **Twisted arms in the spokes style (sections 5.5 and 6).** A twisted arm crosses from ray to
   ray as it goes out, and it lights every cell its path passes through, so it is unbroken at
   every distance (and lights more pixels the more it is twisted: its path is longer). Property 6
   tested this only where the rays share cells; it now also covers a 400-cell buffer at twists of
   0, ±90, ±180 and ±360.
5. **How wide xLights draws a smooth arm (section 2, `thickness`).** In xLights' own frames a
   smooth arm is wider than its thickness says: the setting's angle rounded down to a whole
   degree, plus one degree (a thickness under 1 counting as 1). PixelFlow's arm is exactly as wide
   as its setting says; the import carries the wider figure over
   (`crates/pf-xlights/src/sequence/effects.rs`).
6. **Property 19** asks that a sweep never rise going from bright to dark. Across a soft bright
   edge it must, so the test starts 3° inside it.
7. **Sections 5.6 and 6** disagree about an arm under a cell long. Section 5.6 governs: the pixel
   at the centre is lit.

### Choices made where two goals met

- **Soft edges against spilled light (goals 1 and 2).** A soft band a full cell wide lit 40 to
  80% more pixels faintly on thin wheels, which matters to sparkles and to layers that show only
  where this one is dark. The band is at most a cell and never more than a quarter of the arm's
  own width.
- **A still thin arm exactly between two columns** (an even-sized buffer, the arm along an axis)
  shows as two columns at half strength. Total light is the same.

### Section 12, answered

1. **The rays matter.** Drawn solid, design B on its large group scored 73 for look against 96 to
   97; with rays, 97 to 98. Properties 21 and 22 stand. Hard one-cell rays on whole degrees agree
   with which pixels xLights lights in 93% of pixel-frames 84 to 150 cells from the centre; solid
   arms in 76%.
2. **The instance at 68** is not the wheel: another effect on the same pixels is drawn
   differently, and the score moves with how much of that layer the wheel hides.
3. **Near the crowded centre** under a sweep could not be measured: no rendered prop comes within
   41 cells of it.
4. **Soft edges and sparkles**: see the choice above. Design A's scores did not move.
5. **Design C** scores about 3 lower than before, because the new wheel glides where xLights'
   start angle moves a whole degree at a time. That was kept on purpose: the wheel honours
   fractions. An import makes up the difference instead, by handing the wheel the whole numbers
   xLights shows for a start angle or a centre on a curve (`pf_xlights::sequence::whole`).
6. and 7. **Single rows and columns, tiny buffers, clockwise, several colours, raised and sunken**
   have no xLights frames in the corpus. They agree with the old Pinwheel in every convention
   (192 of 192 combinations of style, direction, shading, twist, start angle and time).

### Look score, old and new

| Design (section 10) | Old (two runs) | New (six runs) |
|---|---|---|
| A, large group | 100.0 / 100.0 | 100.0 in all six |
| A, small group | 99.8 / 99.8 | 99.6 to 99.9 |
| B, large group | 95.9 / 97.0 | 97.2 to 97.9 |
| B, small group | 68.0 / 68.2 | 68.3 to 74.6 |
| C, alone | 92.9 / 93.4 | 88.8 to 90.3 |

Verdicts are unchanged: four match, one close.

With the importer handing design C's start angle over in whole degrees (item 5 above), C scores
92.5 to 93.1 over six runs, and the other four are where they were.
