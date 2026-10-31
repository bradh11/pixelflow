# The fidelity corpus

A made-up xLights show and four short sequences, kept here so that CI can hold PixelFlow's
xLights import to how it looks ([docs/fidelity.md](../../../docs/fidelity.md), "The corpus in
CI"). Everything in it is PixelFlow's own, under the project's licence: the props, the
sequences, the picture (drawn by `src/madeup.rs`), and the shader (PixelFlow's own Prism). No
music: every sequence is an animation.

| File | What it is |
|---|---|
| `show/xlights_rgbeffects.xml`, `show/xlights_networks.xml` | The layout: a matrix, a tree, arches with a submodel, a line, candy canes, a star, and a custom model (1,091 pixels), two groups, one controller at a documentation address |
| `show/washes.xsq` | Washes and shapes: effects that keep time, layers and blends, fades, value curves, a group drawn per prop, a submodel (24 s, 50 ms frames) |
| `show/chance.xsq` | Chance: snow, fireworks, circles, shapes, fire, twinkles, sparkles (20 s, 25 ms frames) |
| `show/marks.xsq` | Marks, words, and pictures: a timing track and shapes it sets off, text, a picture, a shader (20 s, 50 ms frames) |
| `show/words.xsq` | Words in the computer's fonts: Text in Arial, Times New Roman, Verdana, and Courier New, bold and italic, still and moving (16 s, 50 ms frames) |
| `show/*.fseq` | **xLights' renders of the four** (the references). Not here until they are rendered: see below |
| `manifest.json` | The four, for `pf-fidelity` |
| `fingerprint.json` | What PixelFlow draws for each, for the check that needs no xLights |
| `baseline.json` | How each effect looked next to the references, for the check that does |

`src/madeup.rs` writes everything but the references, the fingerprint, and the baseline.
Change the corpus there and run `cargo run -p pf-fidelity -- corpus-files`; a test fails when the
files here and that module disagree.

## Rendering the references

Only xLights can make them, so they are rendered on the maintainer's computer, once, and again
whenever the corpus changes. It takes a few minutes and doesn't touch your own show.

1. Copy the corpus's show to a folder of its own:

   ```sh
   scripts/fidelity-corpus.sh prepare
   ```

   It goes to `~/PixelFlow fidelity corpus` (name another folder after `prepare` if you like).

2. In xLights, make that folder the show folder, the way you would switch to another show. If
   xLights offers to convert or upgrade anything, accept for this folder.

3. **Tools → Batch Render**, tick `washes.xsq`, `chance.xsq`, `marks.xsq`, and `words.xsq`, and
   render. (Or open each sequence, **Render All**, and **File → Save**.) Each `.fseq` is saved
   beside its sequence, or in the FSEQ folder your preferences name.

4. Look at the matrix and the tree in `marks.xsq` once: the picture should be a rainbow with a
   white frame, and the shader a flowing rainbow. Yellow or nothing there means xLights didn't
   find a file; tell whoever asked for the render. In `words.xsq` the words should be in Arial,
   Times New Roman, Verdana, and Courier New, as the effects name them.

5. Don't save any change to the layout or the controllers. Then switch xLights back to your own
   show folder.

6. Bring the renders back, make the baseline, check it, and commit:

   ```sh
   scripts/fidelity-corpus.sh collect
   cargo run --release -p pf-fidelity -- regress --accept
   cargo run --release -p pf-fidelity -- regress
   git add crates/pf-fidelity/corpus/show/*.fseq crates/pf-fidelity/corpus/baseline.json
   ```

   `collect` refuses if a render is missing or older than the copy, or if the layout in the
   folder was changed and saved (a render of another layout isn't a reference for this one).

The four references come to well under a megabyte together: xLights compresses them.

**Without the window.** xLights' release notes describe a headless render from 2026.13 on
("`--headless -s <seq.xsq>`: renders sequences to .fseq with no window, then exits") and, from
long before, an `-r` switch that opens a sequence, renders it, saves the `.fseq`, and exits. The
manual doesn't document either, so the steps above are the ones to rely on; if you try one, run
it on the folder `prepare` made and finish with step 6 as above.

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
