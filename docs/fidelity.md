# Measuring xLights import fidelity

The `.xsq` importer labels every effect *exact*, *approximate*, or *placeholder* by its own
judgement. `pf-fidelity` checks that judgement against the only thing that can: the frames xLights
itself rendered. Given an xLights show folder, a sequence, and the `.fseq` xLights saved for that
sequence, it renders the sequence in PixelFlow and compares the two channel by channel.

```sh
# One sequence: prints the scorecard, or writes <label>.json and <label>.md with --out.
cargo run --release -p pf-fidelity -- score \
  --show path/to/show --xsq path/to/song.xsq --fseq path/to/song.fseq --out reports/

# Every sequence a manifest lists, scored together.
cargo run --release -p pf-fidelity -- corpus path/to/corpus.json --out reports/
PF_FIDELITY_CORPUS=path/to/corpus.json cargo run --release -p pf-fidelity -- corpus --out reports/

# Frames side by side (xLights' above, PixelFlow's below) for looking into a difference.
cargo run --release -p pf-fidelity -- strips --show … --xsq … --fseq … \
  --effect 12 --target Matrix --frames 10 --levels --out strips/
```

## The method

1. **Import the layout** as PixelFlow imports it (`pf-xlights`), then change two things so both
   sides are in the same channel space (`pf_fidelity::layout`):
   - *Controller-side settings are set aside.* A model's brightness, gamma, and "reverse" on its
     controller connection are carried out by the controller; xLights' file holds what it rendered
     before them. PixelFlow's export applies them, so the comparison puts every port at full
     brightness, gamma 1, unreversed. Each prop's own color order stays: xLights writes its
     channels in that order too. The scorecard says how many props this touched.
   - *Props with no controller get a stand-in.* xLights renders every model, including those on no
     controller PixelFlow imports. Each of those gets a stand-in controller over exactly its
     xLights channels, so it is exported, and measured, where xLights put it.
2. **Import the sequence** (`pf_xlights::import_sequence_file`). The import now lists every effect
   with xLights' name for it and the importer's label (`SequenceImport::effects`).
3. **Render with the real renderer through the real export**: `pf_render::export::export_fseq`,
   into memory. What is compared is the file PixelFlow would write. The music is read when it can
   be found, so effects that follow it are drawn as they would be.
4. **Line the frames up by time**: PixelFlow's frame at `t` against the reference's frame at `t`
   (`--frame-offset` moves that, for trying out a timing difference).
5. **Compare** every pixel both files hold, reading each prop's channels back in its color order.

## The scores

For any set of pixel-frames (a prop's, an effect's, a kind's):

| Score | Meaning |
|---|---|
| `mae`, `rmse` | Mean absolute and root-mean-square error per channel, 0–255. |
| `withinTolerance` | Share of pixels within ±16 on every channel. |
| `litIou` | "Is the right thing lit": pixels both light over pixels either lights. |
| `brightnessCorrelation` | How the two brightnesses rise and fall together over time (−1 to 1). |
| `hueDifference` | Mean degrees between the hues where both are lit and colored. |
| `distribution` | Per one-second window: difference in mean brightness, in share of pixels lit, and between the color histograms (each 0–1), and their mean. |
| `pixelSeconds` | How much was measured. |

**Attribution.** Each pixel-frame is added to every effect lit on it at that moment. An effect has
two sets of scores: `solo`, from the pixel-frames where it was the only effect (the clean signal),
and `layered`, from all of them. Its `share` counts a pixel under three effects as a third, so
stacked effects don't each claim the whole pixel. Pixels lit where no imported effect is go to
`unattributed`.

**Stochastic effects** (Twinkle, Snowflakes, Fire, Meteors, Circles, Shape, Lines, Life,
Lightning, Strobe, … and anything that is mostly sparkles) place things at random, so no two
renders light the same pixels. They are marked `stochastic`, judged by their `distribution`
distance, and ranked apart. A deterministic effect with a few sparkles stays deterministic; the
audit allows it the share of pixels its sparkles can change.

**What to fix first.** Kinds are ranked by impact: error × pixel-seconds, summed over instances,
each instance judged alone where it has pixels of its own. "Measured alone" says how much of a
kind's footprint that is; a low share means the error was mostly measured with other effects mixed
in and may be theirs.

**The label audit** lists, effect by effect:
- *labelled exact, measurably different* when judged alone: the label is wrong or the effect has a
  bug;
- *labelled exact, different, but only measured with other effects*: suspects, not convictions;
- *labelled exact, random by nature, showing something different* (by distribution);
- *labelled approximate, yet matching*.

The limits are in the scorecard (`method`) and are the defaults of `Limits` and `AuditLimits`.

## Reading a scorecard

Start with `coverage` and `notes`: how many props, pixels, effects, and frames were measured, and
anything odd about the reference. Then `timing`: the whole-show error with xLights' frames moved
by −2…+2 frames should be lowest at 0, and effects starting on dark pixels should first light
them on the same frame on both sides. Only then the ranking and the audit. `props[].reversedMae`
far below `props[].scores.mae` means a prop is wired the other way round.

## Adding sequences

A manifest lists what to compare; paths are relative to the manifest's folder:

```json
{ "sequences": [
  { "label": "sequence A", "show": "show", "xsq": "show/a.xsq", "fseq": "show/a.fseq" }
] }
```

`show` must be the layout xLights rendered the `.fseq` on. If the layout has changed since, use
the copy xLights kept in the show folder's `Backup/<date>` from that time: a channel count that
doesn't match the layout is noted in the scorecard, and moved props will not match at all. Real
shows and their reports stay out of the repository; keep the manifest and its output elsewhere.
CI runs the same code on a made-up show with references the tests write by hand
(`crates/pf-fidelity/tests`).

## Limits

- **Only what has channels in both files is measured.** A reference with fewer channels, or a
  sparse one, leaves props out; `coverage` names them.
- **A reference can be unrendered.** xLights saves a file with whatever it has rendered so far.
  A prop the reference leaves dark for 20 seconds or more while PixelFlow lights it is taken as
  unrendered there and left out (`reference.leftOut`; `--dark-run-ms 0` scores everything). A
  reference rendered from an *older version* of the sequence can't be told apart from a wrong
  render.
- **Layered effects share blame.** Where several effects are on a pixel the difference can't be
  split between them; trust `solo`, and treat `layered` as a hint.
- **Effects the importer skips aren't seen.** A pixel that xLights lights from an effect that
  wasn't imported counts against whatever was imported there (or is `unattributed`).
- **Stochastic effects are compared loosely**, and each import gives effects new ids, which seed
  their randomness: their numbers move a little from run to run.
- **Red, green, and blue only.** An RGBW prop's white channel isn't compared.

## What the first run found

Seven sequences of one real show (about 6,200 pixels; 341 imported effects, 215 of them labelled
exact) were scored in October 2026. Every prop could be measured, two thirds of the pixels through
stand-ins. One reference was unrendered for 39% of its pixel-time and one had no effects.

- No timing offset: whole-show error is lowest at 0 frames in every sequence that has
  deterministic content.
- Effects measured alone are often close: Morph, Butterfly, Pinwheel, Pictures, On, and Plasma all
  score under 12 per channel with nothing else on their pixels.
- The largest difference looked into is not in an effect but in how rows are put together. With
  xLights' Model Blending on (it was, in every sequence), a row's bottom layer mixes with what
  earlier rows drew, by its own layer method; PixelFlow lets a row's bottom layer cover whatever
  its method. Where this was checked frame by frame, a group's "2 reveals 1" wash that xLights
  keeps behind a prop's own effect shows in front of it in PixelFlow. How much of the layered
  error this explains hasn't been measured; effects that match alone and miss when layered
  (Plasma, Pinwheel, On) point the same way.
- Effects on a single non-matrix prop draw on its front view, where xLights uses the model's own
  buffer (a 360° tree's strands by nodes), so a spiral that winds round the tree in xLights is a
  diagonal across it in PixelFlow.
- Of the 215 "exact" labels, 154 could be judged; 8 differ when judged alone (the tree spiral, a
  wash's fade, a plasma, a fan, a morph, and three pictures), 73 only with other effects mixed in,
  and 30 stochastic ones by their distribution.

## What the first round of fixes changed

The same seven sequences, scored again after the fixes below (error per channel, share of pixels
within tolerance, lit agreement):

| Sequence | Before | After |
|---|---|---|
| A | 57.8, 43%, 0.78 | 26.2, 68%, 0.92 |
| B | 70.0, 25%, 0.86 | 55.0, 43%, 0.87 |
| C | 26.9, 63%, 0.89 | 29.0, 63%, 0.88 |
| D | 77.8, 28%, 0.81 | 22.0, 77%, 0.91 |
| E | 62.2, 17%, 0.48 | 62.1, 17%, 0.48 |
| G | 56.9, 33%, 0.75 | 28.1, 56%, 0.91 |

What xLights does, as read from its source (2024.19) and checked against the references:

- **Rows.** Rows render top to bottom of the sequence. With Model Blending on, what the rows so
  far have put on a model's pixels becomes one more layer under the row, and the row's lowest
  effect mixes with it by its own layer method. With it off, a row writes all its pixels, lit or
  not, whenever it has an effect playing. A sequence now says which (`rowBlending`); an import
  sets it, and a sequence made in PixelFlow keeps PixelFlow's rule (a later row covers where it's
  lit).
- **Fades** are counted in frames (the first frame of a fade-in is already one step up, the last
  frame of a fade-out is dark), are transparency only under the Normal layer method, and are
  brightness under every other.
- **A model's own buffer.** Trees, arches, candy canes, circles, window frames, spinners,
  spheres, and custom models draw on the grid xLights gives them (a tree's strings by the pixels
  up them), and so do their groups under the per-model render styles.
- **A group left of or below the layout's origin** is measured out to the origin by xLights'
  "minimal grid", which changes its scale.
- **Spirals** are drawn cell by cell: stripes a whole number of cells apart, rotation counted in
  cells across the height rather than turns, with 3D, Blend, Grow, and Shrink.
- **Bars** are drawn cell by cell: bars of whole cells, 'expand' and 'compress' mirrored about
  the center, 'Alternate' a bar at a time, with 3D, highlight, and gradient.
- **On** dims its color and stays solid; only its transparency lets what's below through. With
  shimmer it draws every other frame.
- **An exported file** holds the sequence's whole frames, not one more for a part frame.

The label audit, over all seven: 230 effects are now labelled exact (215 before), 101 approximate
(116), 10 placeholders. Of 168 exact ones that could be judged, 6 differ when judged alone, 63
only with other effects mixed in, and 20 stochastic ones by their distribution (8, 73, and 30 of
154 before). Fire, resized pictures, canvas mode above a row's bottom layer, and effects on a
line that runs up and down, a poly line, or a measured model are now imported as approximations,
each with its reason.

What the remaining differences are:

- **The references themselves.** The six "exact" effects that still differ alone all belong to
  one group that spans the layout, and are alone only on three props whose pixel counts differ
  between this layout and an older one: the reference holds other content there (a plain wash
  is another color), while the same effects match on every other prop. One long reference is
  also only partly rendered in its second half. A second pass should start from references
  rendered on the layout as it is now.
- **Effects that share pixels with a random or approximated one** can't be told apart from it.
  Most of the 63 are of this kind: a wash under random shapes, a flash under fire.
- **Fire** is the largest effect still drawn differently: PixelFlow's is a simulation of its
  own, where xLights builds each frame's flames afresh on the target's grid.
- **Shader** effects are placeholders; **Shockwave** and **Fill** are shown as the nearest
  PixelFlow effect; a layer's hue, saturation, and brightness shifts aren't applied.
- **Lines, poly lines, and measured models** still draw on their front view when an effect is on
  the prop alone. A line needs to keep the direction it was drawn in, and an imported model the
  buffer cell xLights gives each pixel, for those to match.
- **Pictures** are resized by blending; xLights takes the nearest pixel.

## What the second round of fixes changed

The same sequences once more (error per channel, share of pixels within tolerance, lit
agreement):

| Sequence | After round one | After round two |
|---|---|---|
| A | 26.2, 68%, 0.92 | 22.6, 74%, 0.92 |
| B | 55.0, 43%, 0.87 | 29.7, 64%, 0.94 |
| C | 29.0, 63%, 0.88 | 20.1, 79%, 0.89 |
| D | 22.0, 77%, 0.91 | 14.4, 84%, 0.95 |
| E | 62.1, 17%, 0.48 | 62.9, 17%, 0.48 |
| G | 28.1, 56%, 0.91 | 26.6, 61%, 0.91 |

B, C, and E are mostly random effects (fire, twinkles, circles, snowflakes), which each import
seeds anew: their numbers move from run to run with nothing changed, B's by about one either
way.

What xLights does, as read from its source (2024.19, and the wxWidgets it was built on):

- **Fire** keeps a grid of heat between frames but writes all of it again every frame, so a
  frame depends only on its own random numbers. The row at the edge the fire grows from is
  seeded bright; each row after it is the mean of the cells behind it, flaring up a step one
  time in five and cooling a step the rest. PixelFlow's Fire has this as a second style,
  flickering flames from any edge, which an import uses; its own rolling flames stay the
  default. Alone on its pixels, Fire's error fell from 63.2 to 7.0 per channel (39% to 85%
  within tolerance), and the distance between the two brightness distributions from 0.47 to
  0.02.
- **Shockwave** and **Fill** are effects of their own, drawn cell by cell as xLights draws them.
  Fill alone went from 106.7 to 0.0; Shockwave, which is nearly always layered here, from 50.7
  to 13.8 where it is.
- **A layer's color adjustments**: hue, saturation, and value shifts, brightness, and contrast,
  each of which can follow a value curve. xLights applies the three shifts before the sparkles
  and brightness and contrast after. A pixel the effect left unlit counts as black, and can be
  lit by a value shift, only on a layer that isn't blended Normal. Every PixelFlow effect has
  them now; left alone they change nothing.
- **Pictures.** "xLights takes the nearest pixel", above, held only before 2024.16. xLights has
  resized a picture five ways: the nearest pixel counting from the corner (before 2024.16); the
  two nearest pixels mixed when shrinking, the nearest when enlarging (2024.16 to 2025.03, from
  the middle of the cell in 2025.04 and 2025.05); an average of the pixels covered (2025.06 to
  2026.03); and a filter chosen by the effect (2026.04 on). An import resizes a sequence's
  pictures the way the version that saved it did, for the first three; a later one keeps
  PixelFlow's averaging and is labelled approximate. Pictures alone went from 6.9 to 0.7 (81%
  to 100% within tolerance).
- **A line's own buffer** is one row along its nodes in the order they were made, whichever way
  the line runs in the layout and whichever end is wired first. A poly line's and an icicle
  run's is a column for each drop, hanging from the top row. A custom model's is the whole grid
  it was drawn on, at the size the model says, with its layers side by side. An imported prop
  keeps that layout (`ownLayout` in the show file). Nothing in these sequences draws on such a
  prop alone, so their scores don't show it; the unit tests hold it to xLights' arithmetic.
- **Text** in a system font is drawn by the computer's own text engine, at a pixel height equal
  to the font's point size, or the size of the computer's own text when the font names none (13
  on a Mac). Sequence E's text names no size, is in a font PixelFlow can't ship, and has color
  emoji in it; stacked top to bottom, xLights spaces every character by the height of the last
  one it measured, an emoji's. None of that can be reproduced with a font of PixelFlow's own,
  bundled or not, so none was bundled. E's low score isn't the text in any case: a lit
  agreement of 0.48 is what two unrelated random fields lit two thirds of the time give, and
  the text lies over a twinkle that covers every pixel.

A sequence's settings now show how its rows meet the rows above them (cover, blend, or replace),
which an import sets and which could not be seen or changed before.

The label audit, over all the sequences: 321 effects are labelled exact (230 after round
one), 10 approximate (101), 10 placeholders. Of 228 exact ones that could be judged,
10 differ when judged alone, 83 only with other effects mixed in, and 13
stochastic ones by their distribution (6, 63, and 20 of 168). The ones that differ alone are
all effects on the one group that spans the layout, in sequence A, where they are alone only
on the props whose reference holds other content (see round one); more of that group's effects
are labelled exact now, so more are counted. Looking for labels that claimed too much found
two: an effect on a cube more than one layer deep (xLights lays the layers side by side;
PixelFlow draws the cube from the front), and Text's label, which now also says when the size
is a guess and when characters are drawn as boxes.

What still differs:

- **The references.** Sequence A's is unrendered for much of its second half, which is where
  its effects with color-adjustment curves are, and three props hold content from an older
  layout. Fresh references, rendered on the layout as it is now, would settle more than any
  further fix.
- **Text**, for the reasons above.
- **Shader** effects are placeholders. Fireworks, Snowstorm, Galaxy, and Curtain are still
  shown as the nearest PixelFlow effect (one Fireworks in these sequences).
- **Canvas mode** above a row's bottom layer, a square **Wave**, and strobing **Twinkle** are
  approximations, labelled as such.
- **Pictures** in a sequence saved by xLights 2025.06 or later.
- **Cubes, stars, and wreaths** draw where their pixels sit, not on xLights' buffer for them,
  and so does a single-color custom model. A poly line's "Line Segments" render style isn't
  imported. A prop imported before this round has no layout of its own until its show is
  imported again.

Speed: the 100,000-pixel benchmark (`pf-bench --tier 100k --seconds 20 --mode asap --sink null`)
drew a frame in 3.59 ms before the round (the median; 4.43 at the 99th percentile) and 2.49 ms
after it (2.82). The machine was busier for the first run, which got 0.85 of a core where the
second got a whole one, so read that as no slower, not as faster. A frame's allocations are
unchanged at 120. The six sequences score in 24 seconds (36 before, on a first run).
