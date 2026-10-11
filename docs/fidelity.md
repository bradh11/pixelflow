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
