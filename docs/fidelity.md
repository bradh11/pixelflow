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
