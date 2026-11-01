# The fidelity corpus

A made-up xLights show and five short sequences, kept here so that CI can hold PixelFlow's
xLights import to how it looks ([docs/fidelity.md](../../../docs/fidelity.md), "The corpus in
CI"). Everything in it is PixelFlow's own, under the project's licence: the props, the
sequences, the picture (drawn by `src/madeup.rs`), and the shader (PixelFlow's own Prism). No
music: every sequence is an animation.

| File | What it is |
|---|---|
| `show/xlights_rgbeffects.xml`, `show/xlights_networks.xml` | The layout: a matrix, a tree, arches with a submodel, a line, candy canes, a star, and a custom model (1,091 pixels), two DMX moving heads (one 16-bit with red, green, blue and white, its dimmer and shutter held by presets; one 8-bit with a color wheel, hung upside down) and two 16-bit servos, the second reversed (24 channels), three groups, one controller at a documentation address |
| `show/washes.xsq` | Washes and shapes: effects that keep time, layers and blends, fades, value curves, a group drawn per prop, a submodel (24 s, 50 ms frames) |
| `show/chance.xsq` | Chance: snow, fireworks, circles, shapes, fire, twinkles, sparkles (20 s, 25 ms frames) |
| `show/marks.xsq` | Marks, words, and pictures: a timing track and shapes it sets off, text, a picture, a shader (20 s, 50 ms frames) |
| `show/heads.xsq` | Moving heads and servos: the DMX effect pointing the heads and turning the servos, under colors drawn on the heads by an On and color washes; then xLights' Moving Head effect on both heads (a position, a fan, a circle with a dimmer, a pan swept by a value curve) and its Servo effect on each servo (40 s, 50 ms frames) |
| `show/words.xsq` | Words in the computer's fonts: Text in Arial, Times New Roman, Verdana, and Courier New, bold and italic, still and moving (16 s, 50 ms frames) |
| `show/*.fseq` | **xLights' renders of the five** (the references): xLights 2026.19 for macOS, headless, 1 November 2026 |
| `manifest.json` | The five, for `pf-fidelity` |
| `fingerprint.json` | What PixelFlow draws for each, for the check that needs no xLights |
| `baseline.json` | How each effect looked next to the references, for the check that does |

`src/madeup.rs` writes everything but the references, the fingerprint, and the baseline.
Change the corpus there and run `cargo run -p pf-fidelity -- corpus-files`; a test fails when the
files here and that module disagree. It writes the files as xLights 2026 keeps them: DMX models
are "Single Color White", a head's dimmer is `MhDimmerChannel`, a picture is named in
`E_TEXTCTRL_Pictures_Filename`, and so on (docs/fidelity.md, "The references", says why).

## Rendering the references

Only xLights can make them, so they are rendered on the maintainer's computer, and again
whenever the corpus changes (a change to `madeup.rs` makes the old renders of something else).
It takes a minute and doesn't touch your own show. You need xLights 2026.13 or later.

1. Copy the corpus's show to a folder of its own:

   ```sh
   scripts/fidelity-corpus.sh prepare
   ```

   On macOS it goes to `~/Library/Containers/org.xlights/Data/PixelFlow fidelity corpus`;
   elsewhere to `~/PixelFlow fidelity corpus`. Name another folder after `prepare` if that one
   exists. **On macOS, keep it in `~/Library/Containers/org.xlights/Data/`**: xLights for macOS
   is sandboxed, and run from the command line it can read only folders in that, its container.

2. Render the five sequences with xLights, with no window:

   ```sh
   scripts/fidelity-corpus.sh render /Applications/xLights.app
   ```

   Name the xLights program (on macOS, its `.app`), and the folder after it if you named one in
   step 1. It runs `xLights --headless -q -s <folder> -od <folder>` on the five sequences, which
   renders each to a `.fseq` beside it and exits; it never sends anything to lights. xLights tries
   the corpus's controller, at a documentation address where nothing answers, and gives up after
   a couple of seconds. If it fails, its log says why: on macOS,
   `~/Library/Containers/org.xlights/Data/Library/Logs/xLights_spdlog.log`.

   Or in xLights' window: make that folder the show folder (if xLights offers to convert or
   upgrade anything, accept for this folder), **Tools → Batch Render**, tick the five sequences,
   and render (or open each sequence, **Render All**, and **File → Save**). Don't save any change
   to the layout or the controllers, and switch back to your own show folder afterwards.

3. Bring the renders back, make the baseline, check it, and commit:

   ```sh
   scripts/fidelity-corpus.sh collect
   cargo run --release -p pf-fidelity -- regress --accept
   cargo run --release -p pf-fidelity -- regress
   git add crates/pf-fidelity/corpus/show/*.fseq crates/pf-fidelity/corpus/baseline.json
   ```

   `collect` refuses if a render is missing or older than the copy, or if the layout in the
   folder was changed and saved (a render of another layout isn't a reference for this one).

4. Look at what xLights rendered before you commit it, with
   `cargo run --release -p pf-fidelity -- corpus crates/pf-fidelity/corpus/manifest.json --out <folder>`
   and `pf-fidelity strips`: the matrix in `marks.xsq` should show the picture (a rainbow with a
   white frame and diagonal) from 5 s and from 15 s, and the shader a flowing rainbow on the
   tree. A red fill there means xLights didn't read the picture. In `heads.xsq` every head and
   servo should be compared ("DMX fixtures"), and none should be dark in xLights throughout. In
   `words.xsq` the words should be in Arial, Times New Roman, Verdana, and Courier New.

The five references come to under a megabyte together: xLights compresses them, and leaves out
the channels of models a sequence doesn't touch.

## When a check fails

- **The fingerprint** (PixelFlow's own render moved). If the change is meant, accept it:
  `cargo run --release -p pf-fidelity -- fingerprint --accept`, and commit `fingerprint.json`
  with the change.
- **The baseline** (an effect looks worse next to xLights). Fix the import; or, if the import is
  better on purpose and something else moved with it, make the baseline again:
  `cargo run --release -p pf-fidelity -- regress --accept`, and commit `baseline.json`.
- **Text in a font the computer hasn't got.** The check's own computer may lack a font the
  corpus names (Linux has none of the four `words.xsq` uses). There the Text is drawn in the
  pixel font, so the check lists it and doesn't hold it to the baseline, which is of the font.
  The fingerprint draws every Text in the pixel font, so it holds on every computer.
- A reference is rendered again whenever a sequence or the layout here changes: `madeup.rs`
  changed, so the old renders are of something else.
