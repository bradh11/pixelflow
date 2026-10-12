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
| **Different** | It reads as a different look. | "looks different: shown as falling meteors" |
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
| Fireworks drawn as steady twinkles (before it had an effect of its own), where xLights is dark between bursts | error 26 | 22 | Different |
| Snow on a tree that PixelFlow also washes blue and the reference leaves dark (the file turned out to be unfinished there) | error 94 | 5 | Different |
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
  split between them. And an effect under others can differ on every prop it has and still be
  judged a match, when the picture the others make is most of what is there: read the props'
  looks (`props[].look`) beside the effects'.
- **Short effects are judged on little.** Under half a second there are few frames to read a
  speed or a rhythm from; the score allows for it, and says less.
- **Random effects move the numbers.** Each import seeds them anew: a share of a sequence that
  sits near a line can fall on either side from run to run.
- **Bursts at random moments read as out of step.** Color is compared second by second and
  rhythm step by step, so light that comes in short bursts has to come at the same moments to
  count as the same light. Fireworks goes off at random moments in both programs. PixelFlow's
  bursts can be as many, as big, as bright, and as short-lived as xLights' and still score
  next to nothing for color and rhythm, as would any render whose bursts fall at other
  moments. With those two at 0 the look cannot pass 45.5, whatever the rest says. For such an
  effect read coverage, brightness, pattern, and motion, and look at the frames.
- **Only what has channels in both files is measured.** A reference with fewer channels, or a
  sparse one, leaves props out; `coverage` names them.
- **A reference can be unrendered, or stale.** xLights saves a file with whatever it has rendered
  so far. A prop the reference leaves dark for 20 seconds or more while PixelFlow lights it is
  left out (`reference.leftOut`; `--dark-run-ms 0` scores everything). Props it leaves dark that
  way to the end of the file were still being rendered when it was saved, and before they go
  dark the file can hold some of their rows without the rest: they are all left out from the
  first moment any of them is missing light, and the scorecard's notes say the reference was cut
  short, and from when. What can't be told from a wrong render, and scores as different: a
  shorter gap, and a prop that stays lit with frames from an older version of the sequence or
  the layout. A file isn't rendered afresh each time it's saved: one in this corpus holds what
  look like frames from before a prop joined a group, and light from effects the sequence no
  longer has. When an effect matches up to some moment and not after it, with its rows set the
  same on both sides, suspect the file: render the sequence again in xLights (Render All, then
  save) before changing the import.
- **Effects the importer skips aren't seen.** A pixel xLights lights from an effect that wasn't
  imported counts against whatever was imported there.
- **Red, green, and blue only.** An RGBW prop's white channel isn't compared.

## What it found

Seven sequences of one real show (about 6,200 pixels, 341 imported effects), October 2026. One
has no effects. Of the rest, 242 effects could be judged (75% of the effect-time):

| Sequence | Matches | Close | Different | No equivalent | Look | As seen |
|---|---|---|---|---|---|---|
| A | 94% | 2% | 5% | – | 94 | 96 |
| B | 72% | 28% | 1% | – | 88 | 94 |
| C | 100% | – | – | – | 98 | 98 |
| D | 100% | – | – | – | 100 | 100 |
| E | 100% | – | – | – | 94 | 93 |
| G | 53% | – | 17% | 30% | 69 | 72 |
| All | 91% | 2% | 5% | 2% | 93 | 93 |

That is one run of five. The shares move with the random effects, which each import seeds anew.
B is mostly fire and circles under short flashes, and three stretches of it sit near the line,
two of ten seconds under circles that wander at random and eighteen seconds of a tendril that
does: its share that matches was 72% in two runs and 43%, 57%, and 99% in the others. A has one
cellular automaton nine minutes long that matched in all five of these runs and was close in two
of ten others, which alone takes A from 94% to 76%. Hold a change against several runs before
and after, effect by effect.

The first scorecards of this corpus (A at 87%, 4%, and 8%; D as seen at 91) are what the first
two findings below changed. G's look was 65, and 71 as seen, before Fireworks had an effect of
its own (see "Fireworks, measured and given an effect of its own").

**A reference that wasn't finished.** Most of what looked different in sequence A was the file,
not the import. xLights saved it part-way through rendering: seventeen of its props go dark in
the middle of an effect and stay dark for the five minutes to the end, five at 4:02.8, ten at
4:05.2, and two at 5:05.9. For the minute before those two go dark they show their own row's
effects with nothing of the layout-wide group's over them: a mean error of 4 to 24 a channel
against their row drawn alone, 14 to 91 against the whole sequence. Up to 4:02 the same two
props show the group's effects over their own: 9 to 12 against the whole sequence, 32 to 67
against the row alone. The rows are set the same on both sides of 4:02, so no way of laying
rows over one another draws both, and the four minutes that are whole are the ones PixelFlow
matches. This was the stretch the first scorecards ranked first, a group's wash over its props'
own snow and circles (eight effects at looks of 5 to 52). Those props are now left out from
4:02.8 (see Limits), and the wash matches on the props that have it.

Three props that stay lit to the end hold frames from other versions of the show. One, an arch,
follows a smaller group's row alone for the first three minutes, dark whenever that row has no
effect while the layout-wide group it belongs to plays on, and takes that group's effects only
from about 3:20, as a prop would that was rendered before it joined the group. That is the prop
lit in another color under the smaller group's wash. All three carry light the sequence has no
effect for: a white line for twelve seconds near the start, a blue wash for fourteen seconds
from 4:50, red where the sequence has blue near the end, and for eighteen seconds from 6:47 a
wheel in two blues, its arms shaded from dark to full and turning. The wheel says most plainly
that these frames aren't this sequence's. It is wiped in from the middle of the props over six
frames and wiped out the same way, and is at full brightness from the start. The sequence has a
blue wash under stars there, fading in over a second and a half from 6:47.25 and gone by
7:02.55, and a wheel in one blue on a smaller group. Nothing in the sequence ends when the two
blues do, two and a half seconds after the wash; none of its effects there has those two blues;
and no effect in the whole sequence has a transition. PixelFlow shows the wash and the stars,
then the fill that follows them. A wash laid over stars the same way matches where the file is
whole (a look of 99).

What in A never scores as a match is on these three props or on the props cut short: the 5% of
its time that scores as different, and the five effects that score as close in every run (a
Pinwheel at about 70, two Shockwaves, a Shape, and a Color Wash). Held prop by prop, it is
these three that differ in what is lit and in what color; one of the five, a Shockwave on a
small group, has only one of them to be measured on. None of it is known to be the import's
doing. A fresh render of the sequence in xLights would say.

**A difference no effect's verdict showed.** A Twinkle set to strobe was drawn as PixelFlow's
soft twinkle. In xLights' frames it is a few pixels flashing: at count 20 and steps 98, 0.35%
of the pixels are lit in any one frame, each at full brightness and for that frame alone, at
random. PixelFlow lit some forty times as many, as white specks over every prop of the effect's
group wherever the rows over it were dark or dim. The effect itself was judged a match, at a
look of 100: it has no pixels to itself, and the picture the other rows make matched. The
props said otherwise: ten outline props looked 61 to 84. A strobing twinkle is now imported as
a Strobe with a flash a frame, those props look 89 to 100, and sequence D as seen went from 91
to 99.

**An effect the end of the sequence cuts off.** Four of sequence D's effects run a tenth or a
fifth of a second past its end. xLights spreads what it counts per effect (a spiral's turns,
bars' cycles) over the length the file gives the effect; the importer cut the effect at the end
of the sequence first and spread the count over what was left, so it ran up to 0.7% fast. No
effect's verdict showed this either. Two outline props that have one such spiral to themselves
did: each turn started a little earlier than in xLights' frames, a fifth of a second early by
the end, and they looked 89 and 90. The count is now spread over the whole length, both props
look 100, and D as seen went from 99 to 100. What PixelFlow spreads over an effect's own
progress (a brightness ramp, a setting's curve, a fade-out) still ends where the effect is cut.
On an effect cut by a large share of its length that would show; this corpus has none.

**What follows the music.** Sequence B has sparkles, and an On's transparency, following the
music. Three things were found in its frames: one is fixed, and two are decisions to make.

- *A value curve under the Music checkbox.* The palette's Music checkbox scales the sparkle
  count by the music's peak. The importer carried it over only when the Sparkles slider itself
  was above nothing. One On leaves the slider alone and gives the count by a value curve that
  sits at 200 while the music is quiet, so its sparkles came on thick as the music died away:
  on a third of its pixels, where xLights flashes a few in a hundred. The checkbox now comes
  across with a curve too, and that On went from a look of 90 to 99.7 (its rhythm from 0.73 to
  0.97).
- *Which channel is the music.* The level xLights follows can be read off its frames: an On
  whose transparency follows the music is as bright as the music is quiet. Over 400 frames that
  level is the left channel's highest sample in the frame, as a share of the song's (a mean
  difference of 0.006, a correlation of 0.99). PixelFlow mixes both channels to one before it
  looks for the highest sample. On this song that is 0.029 and 0.90 from xLights' level, about a
  sixth lower on average, and at full level on 17 of the first 1,500 frames where the left
  channel is on 40. Sparkles that follow the music burst when it is at full level, so PixelFlow
  shows fewer bursts. A fire's music sparkles over three short flashes are what makes one of
  them look 79 to 84 ("flashes less often") and the other two 88 to 92. The same fire grows with
  the music, and reaches a roof prop on 16% of the frames that prop is lit where xLights' fire
  reaches it on 29%. Reading the left channel alone was tried: the three flashes look 100, the
  fire reaches the roof on the very frames xLights' does, and no other effect or prop in the
  corpus moves outside what it does from run to run. It was not kept, because it changes what a
  PixelFlow sequence's own music curves, music sparkles, and fire follow, and a song with
  something on the right alone would go unheard.
- *Sparkles on a see-through effect.* xLights flashes an On's sparkles however see-through the
  On is. On the three loudest frames of one half-second On its transparency is 99 to 100% and
  five or six in ten of its pixels flash. PixelFlow flashes sparkles only where the effect
  shows, and is dark on two of those frames. That is the whole of that On's difference: it
  looks 54, and with sparkles flashing wherever the effect's own color is lit, whatever its
  transparency, it looked 99 in five runs of six and 100 once the left channel was read too.
  That was tried and not kept either: it changes how PixelFlow's own sparkles behave.

On a group, xLights' sparkles are also wider. On a matrix twelve pixels wide inside a
layout-wide group, four in ten of its sparkling pixels have the pixel beside them flashing with
them; PixelFlow's flash one at a time (one in a hundred). As many pixels flash on both sides.

**Where the look and the pixels disagree.** Ranked by pixel error, the kinds to fix were Shader,
Fan, Color Wash, Spirals, Plasma, Bars, and Butterfly. By look, Spirals, Plasma, Bars, and
Butterfly have nothing to fix: every instance matches (their error is a pattern a pixel over, or
a plasma's blobs in other places). The cellular automaton, the twinkles, and the fire that pixel
error called the worst of the random effects match too. Going the other way, Fireworks had a
small pixel error, because both renders are mostly dark, and was the plainest difference there
was: PixelFlow twinkled steadily where xLights is dark between bursts.

**Fireworks, measured and given an effect of its own.** The corpus has one Fireworks: 52
seconds in sequence G on a group of outline props, 611 pixels on a grid of 139 by 257 cells,
with xLights' usual settings (16 explosions, 50 particles, velocity 2, particle fade 50, gravity
and hold color on, at random places) and 25 ms frames. Its frames show what a burst is:

- A burst is at one place, somewhere on the grid, and bursts go off at moments with no pattern
  to them. Six could be told apart in the 52 seconds; the rest crossed too few pixels to tell.
- A spark is one cell of the grid, lit whole. On this group one cell in sixty has a pixel, so
  over the 25 seconds the effect has the pixels to itself xLights lights one pixel in ten
  thousand: the props are dark but for a few specks where a burst's sparks cross an outline.
- Every spark of a burst is the same brightness, and loses a twenty-fifth of full brightness a
  frame: `trunc(255 × (1 − k / 25))` on frame `k`, gone after 25 frames.
- Sparks fly straight out, each way as likely as any other, at up to 2 cells a frame (the
  fastest seen crossed 12 cells in 6 frames), with slower ones down to a quarter of that or
  less: a disc that fills as it grows, 50 cells in radius when it fades out.
- With gravity on the sparks were still on straight lines after 20 frames, to within what the
  few crossings can show: a drop of no more than about a tenth of how far they fly.
- A burst is one palette color (white, then magenta, of white, red, and magenta).

PixelFlow's Fireworks draws that from its own settings: bursts a second, sparks, a burst size,
a glow time, a fall, one color a burst, and sparks one cell wide that light whole pixels. An
import sets them in cells and seconds from xLights' numbers (`fireworks` in
`pf_xlights::sequence::effects`). How a burst is drawn is PixelFlow's own: no spark is kept
from frame to frame, and a pixel finds the few sparks that could reach it from its direction
from the burst.

Over four runs before and six after, the look went from 22 to between 38 and 45.5, the most
it can be. Coverage, brightness, and motion went from 0.01, 0.01, and 0.02 to 1.00, 1.00, and
0.99. Pattern went from 0.98 or more to between 0.37 and 0.99, by where a run's few bursts
happen to cross a pixel. Color stays near 0 (0.02 before, 0.00 to 0.03 after), and rhythm fell
from 1.00 to 0.00: both renders burst at random moments, and not the same ones, which is the
limit above. Where other rows cover the same pixels the picture went from 68 to 97; sequence
G's look went from 65.5 to between 68 and 69, and as seen from 71.5 to 72.3. One other effect
in the sequence moved by more than it does from run to run: a Shader's dim stand-in, from 39.8
to 37.6 in every run. The import says "bursts go off at other random moments than xLights'" of
a Fireworks that isn't on a timing track and expects it to look close; the score, for the
reason above, still says different.

Not measured, because no sequence in the corpus has them: a particle fade other than 50 (taken
as half as many frames of glow, and noted), frames other than 25 ms (taken as xLights counting
in frames), colors that change (hold color off), a firework that moves, firing with the music,
and firing from a timing track.

**What to improve, by look:**

- *Mappings to check.* None is known in this corpus. What the last list had here is fixed (the
  Spirals a turn early), is a decision to make (the half-second On: see "What follows the
  music"), or is the reference's doing (the Pinwheel on a small group that scores Close, and the
  wash and the stars under it: the reference has a wheel in two blues there that the sequence
  has no effect for). Not measured, because no sequence here has one: a strobing twinkle's share
  of the pixels at more than one count and number of steps (it is taken to rise with the first
  and fall with the second), and a strobing twinkle in the older way of drawing twinkles, which
  is still shown as soft twinkles.
- *Decisions to make.* Whether the music an effect follows is the left channel, as in xLights,
  or both, and whether sparkles flash on an effect that is see-through. Each is a change to
  PixelFlow's own behavior, tried and measured above. And what an imported Fireworks is called:
  the importer expects close, and the score cannot say better than different of bursts at other
  moments (see Limits).
- *What PixelFlow can't show yet.* Shader (ten effects, the largest item by far) and Text in
  the sequence's own font. A Tendril that wanders at random shows less of itself and pulses at
  other moments. A Wave has no square shape. A Fireworks that moves stays where it goes off.

How rows are laid over one another was not changed: where the references are whole, rows that
blend as the sequence says match them, a group's row over its props' own rows included.

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
