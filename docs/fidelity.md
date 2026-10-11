# xLights import fidelity

**Fidelity is how an imported sequence looks.** Someone who made a show in xLights and imports it
into PixelFlow wants the same show: the right props lit, in the right colors, moving the same way
at the same speed, to the same rhythm. That is all fidelity means here.

It does not mean the same pixels. Two renders of falling snow never light the same pixels, in
xLights or anywhere else, and nobody watching can tell. And it does not mean building an effect
the way xLights builds it: PixelFlow is free to draw an effect its own way, and a faster or
simpler way is better, as long as the result looks the same.

`pf-fidelity` measures this. Given an xLights show folder, a sequence, and the `.fseq` xLights
saved for that sequence, it renders the sequence in PixelFlow and compares the two renders by
how they look. It works from the two renders alone and knows nothing of how either was made.

```sh
# One sequence: prints the scorecard, or writes <label>.json and <label>.md with --out.
cargo run --release -p pf-fidelity -- score \
  --show path/to/show --xsq path/to/song.xsq --fseq path/to/song.fseq --out reports/

# Every sequence a manifest lists, scored together.
cargo run --release -p pf-fidelity -- corpus path/to/corpus.json --out reports/
PF_FIDELITY_CORPUS=path/to/corpus.json cargo run --release -p pf-fidelity -- corpus --out reports/

# The importer's table of expected looks, held against a scorecard.
cargo run --release -p pf-fidelity -- labels reports/corpus.json

# Frames side by side (xLights' above, PixelFlow's below), to see a difference for yourself.
cargo run --release -p pf-fidelity -- strips --show … --xsq … --fseq … \
  --effect 12 --target Matrix --frames 10 --levels --out strips/
```

## The verdicts

Every effect gets one of four, and so does every line of an import's report:

| Verdict | Means | In the import report |
|---|---|---|
| **Matches** | A viewer wouldn't tell the two apart at show distance. | "comes across the same" |
| **Close** | The same idea, with differences a viewer could notice. | "looks close: slower than in xLights" |
| **Different** | It reads as a different look. | "looks different: shown as twinkles" |
| **No equivalent** | PixelFlow has no effect for it yet; a dim stand-in is shown. | "no PixelFlow effect yet" |

## The look score

An effect's look has six parts. Each is scored from 0 (nothing alike) to 1 (alike), on the
target's own grid: a matrix's columns and rows, a tree's strings and the pixels up them, a line's
pixels in a row, a group's layout.

| Part | Asks | How |
|---|---|---|
| **Coverage** | Is as much of the prop lit, all the way through? | The share of pixels lit, as a curve over time (smoothed over half a second): how much of the two curves is the same. |
| **Brightness** | Is it as bright, and does it rise and fall the same? | The same for mean brightness. Half as bright halves it; so does a fade the other render doesn't have. |
| **Color** | Are they the same colors, in the same shares? | Second by second, each render's light sorted into hue bins 15° wide and one for white, weighted by brightness. Light in the same bin matches; a hue up to 45° off matches in part; white matches only white. |
| **Pattern** | Is the light in the same places, with the same fineness? | The layout: the picture on a grid of at most 8 by 8, relative to its own brightness, the better of as it is and moved one cell. Times the texture: how alike cells are that are 1, 2, … 16 cells apart, which is the spacing of stripes and the size of dots. |
| **Motion** | Does it move the same way, as fast, and change as much? | The move that best lays each row and column of the picture on itself a moment later gives a direction and a speed (opposite scores 0, half speed a half). And the share of the light that changes from frame to frame. The lesser of the two. |
| **Rhythm** | Does it pulse in step, and flash as often? | The brightness with its slow curve taken off, the two held against each other a frame either way (a strobe at another rate, or out of step, scores low). And how often dark pixels light up. The lesser of the two. |

Three things keep the score honest.

**People notice ratios.** Each part is how much of the two renders is the same, passed through
one curve: a fifth off passes for the same (0.97), half off is half alike (0.5), three quarters
off has next to nothing in common (0.11).

**A render isn't asked to be more like the other than it is like itself.** The pixels are split
in two at random. What the two halves of one render disagree on (which pixels twinkle, where the
snow is) is chance, and is taken off before the renders are compared. A layout that doesn't hold
still within one render (blobs coming and going) isn't asked to be where the other render has it.
A speed is only as sure as its readings agree. This is why random effects need no special case:
the same density, colors, and rate from another seed matches.

**One thing badly wrong reads as a different look.** The composite, 0–100, is 70% the weighted
mean of the parts and 30% the weakest part. The weights: color 0.25 and pattern 0.20, because
they are what a viewer names first ("the red spiral"); coverage, brightness, and motion 0.15
each; rhythm 0.10, because a rhythm that is off nearly always shows in brightness or motion too.

A score of 85 or more **matches**, 55 or more is **close**, and under 55 is **different**. A
placeholder has **no equivalent** whatever it scores. The lines were drawn by looking: frames
side by side (`strips`) for a spread of effects, and the verdicts moved until they agreed with
what the pictures showed. Some of what settled them:

| What was on the lights | Pixel for pixel | Look | Verdict |
|---|---|---|---|
| A cellular automaton alone on a row of small props for nine minutes: the same sparse dots in other places | error 84 of 255, lit agreement 0.69 | 94 | Matches |
| Plasma on two thin outline props: the same mottling, the blobs elsewhere | error 48 | 100 | Matches |
| Twinkles covering a tree with text over them | error 63, lit agreement 0.48 | 99 | Matches |
| Fire alone on a group that spans the layout: flames on the same props in the same colors | error 7 | 99 | Matches |
| A fill rising up a matrix with sparkles | error 10 | 100 | Matches |
| A tendril wandering two outline props: the same colors, less of it lit, pulsing at other moments | error 52 | 63 | Close |
| A wash on a group of outline props, one of them in another color | error 40 | 57 | Close |
| Fire over a group of outline props where xLights lights one of them blue and little else | error 98 | 47 | Different |
| Fireworks drawn as steady twinkles, where xLights is dark between bursts | error 26 | 22 | Different |
| Snow on a tree that PixelFlow also washes blue and xLights leaves dark | error 94 | 5 | Different |
| A shader shown as a dim fill | error 57–108 | 13–40 | No equivalent |

## The method

1. **Import the layout** as PixelFlow imports it (`pf-xlights`), then change two things so both
   sides are in the same channel space (`pf_fidelity::layout`):
   - *Controller-side settings are set aside.* A model's brightness, gamma, and "reverse" on its
     controller connection are carried out by the controller; xLights' file holds what it rendered
     before them. PixelFlow's export applies them, so the comparison puts every port at full
     brightness, gamma 1, unreversed. Each prop's own color order stays.
   - *Props with no controller get a stand-in.* xLights renders every model, including those on no
     controller PixelFlow imports. Each gets a stand-in controller over exactly its xLights
     channels, so it is exported, and measured, where xLights put it.
2. **Import the sequence** (`pf_xlights::import_sequence_file`). The import lists every effect
   with xLights' name for it and the verdict the importer expects (`SequenceImport::effects`).
3. **Render with the real renderer through the real export**: `pf_render::export::export_fseq`,
   into memory. The music is read when it can be found, so effects that follow it are drawn as
   they would be.
4. **Line the frames up by time**: PixelFlow's frame at `t` against the reference's frame at `t`
   (`--frame-offset` moves that).
5. **Look at every effect twice**: on the pixel-frames where it was the only effect (`lookSolo`),
   and on all of them, whatever else was on (`lookLayered`). It is judged alone when at least a
   tenth of what it was lit on had nothing else on it; otherwise layered. A layered look is the
   look of the picture there, which several effects share: trust "alone", and treat "layered" as
   a hint.
6. **Look at every prop** over the frames either side lit it, whatever effects were on it
   (`props[].look`). The mean over the props, by pixel-seconds, is the sequence's `asSeen`:
   the look of the whole picture, with no question of which effect to blame.

## Reading a scorecard

**How it looks** comes first: the share of effect-time (each judged effect's length, added up)
that matches, is close, is different, or has no equivalent, the same by pixel-seconds, the look
score, and its six parts. "Not judged" is effect-time that couldn't be measured.

**What to improve first** ranks the kinds of effect by what there is to gain: how far each
instance falls short of matching, times the pixel-seconds it was lit on. A look that matches
adds nothing, however long it runs. "Off" names the part that agrees least and what a viewer
would notice ("motion: moves the opposite way", "brightness: dimmer (0.6× as bright)"). That is
the pointer to the fix:

- **A mapping to correct**: the PixelFlow effect can show this look, and the importer set it up
  wrongly. A direction reversed, a speed off by a factor, colors in another order, a count.
  These are cheap, and worth doing first.
- **Something PixelFlow's effect can't do yet**: a shape, a movement, or a setting it has no
  equivalent for. Add the setting to PixelFlow's effect, drawn PixelFlow's way.

"Judged (alone)" says how many instances the ranking rests on and how many of those had pixels
to themselves. Few alone means the difference may belong to what the kind is layered with.

**The importer's labels** holds each effect's expected verdict against the one it earned:
effects expected to look better than they do (judged alone, then judged layered), effects
expected to look worse, and a table by kind and by what the importer noted.

**Diagnostics: pixel for pixel** is the rest: mean error per channel, the share of pixels within
±16, lit agreement, brightness correlation, hue difference, and the window-by-window
distributions, per sequence, per kind, per effect, and per prop. They don't say how an import
looks: a random effect scores badly however alike it looks, and a pattern that is right but a
pixel over scores as wrong. They find structural bugs, which is what they are kept for:

- `timing`: the whole-show error with xLights' frames moved by −2…+2 frames should be lowest at
  0, and effects starting on dark pixels should first light them on the same frame on both sides.
- `props[].reversedMae` far below `props[].scores.mae`: a prop wired the other way round.
- An effect that matches alone and not layered: the way rows or layers are mixed.

## The importer's labels

An import can't measure anything: there is no xLights render to hold it against. It labels each
effect from what it carried over and a table (`pf_xlights::sequence::look::TABLE`):

- An effect whose every setting has a PixelFlow setting is expected to **match**.
- One with a difference noted is expected to look **close**, and the note says what a viewer
  would notice ("fading tail not shown"), not which setting wasn't applied.
- The table names the exceptions: an xLights effect, a condition (always, or a note containing
  some text), and the verdict. The worst row that applies wins.
- A placeholder has **no equivalent**.

The rule and the rows marked measured come from scorecards. `pf-fidelity labels` holds the table
against one: for each kind of effect and each set of notes, what most of its effects earned,
those judged alone when there are any. A row that disagrees on two or more effects judged alone
fails the command. Then either the table gets a row, or the effect gets fixed. Rows for looks no
measured sequence has are judged from what the note says a viewer would see, by the same
yardstick: the wrong colors, nothing moving where xLights moves, or light on another part of the
prop is a different look; a shape, a direction, or a speed that is off is close.

## Adding sequences

A manifest lists what to compare; paths are relative to the manifest's folder:

```json
{ "sequences": [
  { "label": "sequence A", "show": "show", "xsq": "show/a.xsq", "fseq": "show/a.fseq" }
] }
```

`show` must be the layout xLights rendered the `.fseq` on. If the layout has changed since, use
the copy xLights kept in the show folder's `Backup/<date>` from that time. Real shows and their
reports stay out of the repository; keep the manifest and its output elsewhere. CI runs the same
code on a made-up show with references the tests write by hand (`crates/pf-fidelity/tests`), and
the look score on made-up pairs of renders (`crates/pf-fidelity/src/look.rs`).

## Limits

- **It can't read.** Text in other lettering that fills the same space the same way looks the
  same to it. The importer's label for Text says "close" regardless.
- **It doesn't hear.** An effect a beat late against the music shows only as a rhythm out of
  step with xLights' render, never against the song.
- **Layered effects share a look.** Where several effects are on a pixel the difference can't be
  split between them.
- **Short effects are judged on little.** Under half a second there are few frames to read a
  speed or a rhythm from; the score allows for it, and says less.
- **Random effects move the numbers.** Each import seeds them anew: a share of a sequence that
  sits near a line can fall on either side from run to run.
- **Only what has channels in both files is measured.** A reference with fewer channels, or a
  sparse one, leaves props out; `coverage` names them.
- **A reference can be unrendered, or stale.** xLights saves a file with whatever it has rendered
  so far. A prop the reference leaves dark for 20 seconds or more while PixelFlow lights it is
  left out (`reference.leftOut`; `--dark-run-ms 0` scores everything); a shorter gap, or content
  from an older version of the sequence or the layout, can't be told from a wrong render and
  scores as different.
- **Effects the importer skips aren't seen.** A pixel xLights lights from an effect that wasn't
  imported counts against whatever was imported there.
- **Red, green, and blue only.** An RGBW prop's white channel isn't compared.

## What it found

Seven sequences of one real show (about 6,200 pixels, 341 imported effects), October 2026. One
has no effects. Of the rest, 247 effects could be judged (77% of the effect-time):

| Sequence | Matches | Close | Different | No equivalent | Look | As seen |
|---|---|---|---|---|---|---|
| A | 87% | 4% | 8% | – | 91 | 96 |
| B | 43% | 57% | 1% | – | 83 | 93 |
| C | 100% | – | – | – | 99 | 99 |
| D | 100% | – | – | – | 100 | 91 |
| E | 100% | – | – | – | 95 | 94 |
| G | 53% | – | 17% | 30% | 65 | 72 |
| All | 86% | 4% | 8% | 2% | 90 | 93 |

B is mostly fire and circles under short flashes, and two ten-second stretches of it sit near
the line: its share that matches was 43%, 57%, and 72% in three runs.

**Where the look and the pixels disagree.** Ranked by pixel error, the kinds to fix were Shader,
Fan, Color Wash, Spirals, Plasma, Bars, and Butterfly. By look, Spirals, Plasma, Bars, and
Butterfly have nothing to fix: every instance matches (their error is a pattern a pixel over, or
a plasma's blobs in other places). The cellular automaton, the twinkles, and the fire that pixel
error called the worst of the random effects match too. Going the other way, Fireworks has a
small pixel error, because both renders are mostly dark, and is the plainest difference there
is: PixelFlow twinkles steadily where xLights is dark between bursts.

**What to improve, by look:**

- *Mappings to check.* One 15-second stretch of sequence A is most of what looks different: a
  wash on the group that spans the layout shows on props where xLights shows only those props'
  own snow and circles (how that group's row meets the rows under it). A smaller group of
  outline props lights one of them in another color, or lights it where xLights leaves it dark,
  under three kinds of effect (how that group's effects are laid over its props). Under other
  effects, a Pinwheel changed less from frame to frame, and a two-second Shockwave stood still
  where xLights' moved.
- *What PixelFlow can't show yet.* Shader (ten effects, the largest item by far), Fireworks,
  and Text in the sequence's own font. A Tendril that wanders at random shows less of itself
  and pulses at other moments.

No mapping was changed on the strength of these runs: none of the differences was a sign or a
scale that the score could confirm on an effect with pixels to itself.

## Before the look score

Two earlier rounds worked from pixel agreement. They made rows blend the way the sequence says,
counted fades in frames, gave trees, arches, lines, and custom models grids of their own, added a
flickering style to Fire, gave Shockwave and Fill effects of their own, added color adjustments
to every effect, and resized pictures the way the sequence expects. Mean error per channel fell
from 57.8, 70.0, 26.9, 77.8, and 56.9 (sequences A, B, C, D, and G) to 22.6, 29.7, 20.1, 14.4,
and 26.6. Those numbers are still in every scorecard's diagnostics. What they can't say is
whether a change was worth making: several of those fixes moved pixels a viewer never saw
differ. From here on, a change to how an import is drawn is judged by the look score, and a way
of drawing an effect is chosen because it looks right and runs well, not because xLights draws
it so.
