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
how they look. It works from the renders and knows nothing of how either was made, but for one
thing it is told of each effect: whether chance decides when its light comes (see "Effects that
draw on chance").

```sh
# One sequence: prints the scorecard, or writes <label>.json and <label>.md with --out.
cargo run --release -p pf-fidelity -- score \
  --show path/to/show --xsq path/to/song.xsq --fseq path/to/song.fseq --out reports/

# Every sequence a manifest lists, scored together.
cargo run --release -p pf-fidelity -- corpus path/to/corpus.json --out reports/
PF_FIDELITY_CORPUS=path/to/corpus.json cargo run --release -p pf-fidelity -- corpus --out reports/

# The same with every effect held moment by moment against xLights' render alone: the numbers
# from before effects that draw on chance were held against PixelFlow's other seeds.
cargo run --release -p pf-fidelity -- corpus path/to/corpus.json --by-moment --out reports/

# The importer's table of expected looks, held against a scorecard.
cargo run --release -p pf-fidelity -- labels reports/corpus.json

# Frames side by side (xLights' above, PixelFlow's below), to see a difference for yourself.
cargo run --release -p pf-fidelity -- strips --show … --xsq … --fseq … \
  --effect 12 --target Matrix --frames 10 --levels --out strips/

# The score held against PixelFlow's own renders, where the answer is known (no xLights needed).
cargo run --release -p pf-fidelity -- calibrate --out reports/
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

Four things keep the score honest.

**People notice ratios.** Each part is how much of the two renders is the same, passed through
one curve: a fifth off passes for the same (0.97), half off is half alike (0.5), three quarters
off has next to nothing in common (0.11).

**A render isn't asked to be more like the other than it is like itself.** The pixels are split
in two at random. What the two halves of one render disagree on (which pixels twinkle) is
chance, and is taken off before the renders are compared. A layout that doesn't hold still
within one render (blobs coming and going) isn't asked to be where the other render has it. A
speed is only as sure as its readings agree.

**Nor more like xLights than it is like itself from another seed.** Splitting the pixels allows
for chance that is each pixel's own. A burst, a ball, or a flake moves many pixels together:
both halves of a render see it, and no other render has it at that moment in that place. So
where an effect draws on chance, PixelFlow renders the sequence again from other seeds, and
what its own renders disagree on isn't held against the import. Where an effect's moments are
chance, the parts of its look that chance moves are read over the whole effect, not moment by
moment. "Effects that draw on chance" has the rule, the effects it applies to, and what it
costs.

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

## Effects that draw on chance

Fireworks go off at moments and in places no two renders share, in PixelFlow or in xLights. Held
second against second, two good renders of the same fireworks have no color in common and never
pulse together, and that used to cap such an effect at a look of about 45, "different", however
alike the two were. The same fault moved other scores from run to run: a wandering ball is
somewhere else in every render, a game of life settles into other shapes, and a sparkle falls
on another pixel.

Three things are done about it. They are kept apart, because only the second could hide a
fault, and the third is there to stop the first from hiding one.

**What PixelFlow's own renders disagree on is chance.** An effect draws its chance from its
seed. Where any effect of a sequence draws on chance, PixelFlow renders the sequence twice more,
every effect from another seed (`--seeds` changes how many). Each part of a look is then how
much PixelFlow's renders have in common with xLights' on average, next to how much the two of
PixelFlow's own renders that are least alike have in common with each other. Where its renders
agree from seed to seed, nothing is allowed. Where they differ in which pixels sparkle, where a
ball started, which color a burst took, or how many bursts crossed a prop, that much is allowed
and no more. This is done for both looks of an effect that draws on chance, and for the layered
look of an effect that shares its pixels with one: the picture there changes from seed to seed
whoever drew it. An effect that draws on nothing, on pixels nothing else draws on by chance, is
judged exactly as before, to the last digit.

**Where the moments are chance, the parts chance moves are read over the whole effect.** Held
moment against moment, two renders of bursts at chance moments have next to nothing in common,
PixelFlow's own included, and next to nothing over next to nothing says nothing of whether the
colors are right. So for an effect whose moments are chance, a part of the look that
PixelFlow's own renders have less than half of in common moment by moment is read with the
order of the moments set aside:

| Part | Over the whole effect |
|---|---|
| Coverage | The same curve of the share of pixels lit, sorted from its highest moment to its lowest: how much of the time how much is lit, and how much of the time it is dark. |
| Brightness | The same for mean brightness. |
| Color | All the light of the effect sorted into the same hue bins once, not second by second. |
| Pattern | Where the light fell over the whole effect, on the same grid of at most 8 by 8, in place of where it is at each look. Light a single burst left where it happened to go off counts for no layout of the render's own, and a render too dim on the whole to have a pattern (a few sparks crossing a prop) has no layout. Times the texture, as before. |
| Motion | Which way the picture moves over all its one-second windows together, how fast, and the share of its light that changes from frame to frame: three amounts, each held against PixelFlow's seeds (below). Whether the two move the same way in each window is not asked. |
| Rhythm | How often dark pixels light up: an amount, held the same way. Whether the two pulse in step is not asked. |

A part PixelFlow's renders do share moment by moment is still read that way. Snow over a
steady wash has its moments left to chance, and the wash under it is held to its own.

Which effects those are is declared from the effect's kind and settings
(`pf_fidelity::chance`), and never read off xLights' frames: an effect that ought to keep time
and doesn't would look just like one chance moves. An effect set off by its own clock, a timing
track, or the music keeps its moments, whatever else it draws by chance, and is held moment by
moment, so that coming in late shows.

| Effect | Chance decides | Why |
|---|---|---|
| Fireworks, bursting at random moments | its moments | Each burst goes off at a moment, in a place, and in a color drawn from the seed. |
| Shape, with a staggered start and no timing track or music | its moments | Each shape keeps time from a chance start of its own, in a chance place. |
| Circles (balls, bubbles, plasma blobs) | its moments | The balls start in chance places and head off in chance directions at chance speeds. Nothing brings one back at a set moment. |
| Meteors | its moments | Each meteor falls in a chance lane, starting at a chance moment. |
| Snowflakes | its moments | The flakes are scattered by chance, and new ones start at chance moments. |
| Lines | its moments | The lines start in chance places and head off in chance directions. |
| Lightning | its moments | It strikes at chance moments. |
| Tendril, moving at random | its moments | The point it follows wanders by chance. |
| Fireworks or Shape on a timing track or with the music; Shape with every shape starting together | what it draws | Each appears when the track, the music, or the clock says, in a chance place and color. |
| Life | what it draws | A new generation comes by the clock. The cells it starts from are chance. |
| Fire | what it draws | It burns where and as high as it is set to (or as the music is loud). Its flames flare by chance. |
| Twinkle, Strobe | what it draws | Each pixel twinkles from a chance moment of its own, and over a prop they add up to the same light at every moment. A strobe flashes by the clock on chance pixels. |
| VU Meter's random bars | what it draws | The bar lights with the music or on a mark. Which bar is chance. |
| Faces with blinking eyes | what it draws | It sings from its timing track. Its blinks fall at chance moments. |
| Any effect with sparkles | what it draws | Each lit pixel flashes at a moment of its own. |
| Everything else, and Circles' rings, a Tendril on a set path | nothing | Worked out from the clock, the settings, a timing track, the music, or a file. Every render is the same. |

A kind added to PixelFlow has to be given a line in that table before `pf-fidelity` builds.

**And there has to be as much of it.** What chance leaves in common between two renders can be
little, and half of little passes for little: a ball half the size, somewhere else at every
moment, has nearly as much in common with xLights' ball as PixelFlow's own renders have with
each other. So next to other seeds, coverage and brightness are also held to how much is lit
and how brightly over the whole effect. PixelFlow's seeds give an average and a spread.
xLights' amount within what the spread allows is chance, and counts for nothing; from twice
that far it counts in full, the smaller amount over the larger; in between, a share of it.
What the spread allows is one and a half times a spread that is sure. A spread read from
three renders is itself a matter of chance and often too small, so it is allowed 2.45 times
(Student's t, at the same odds), and a little more for the average of three being only so
sure. Over the whole effect, movement and rhythm are amounts too, and are held the same way.
That reads chance that is drawn once for a whole effect, such as which way three balls set
off, better than how alike any two renders happen to be.

**How much it takes to say anything.** The allowance is measured, not assumed, so it is as wide
as chance is. Sixty bursts differ little from one seed to the next, and a render with half as
many is caught. Four bursts differ as much from seed to seed as from a render with eight, and
nothing can tell those apart, a viewer least of all. Each look next to other seeds says how
alike PixelFlow's own renders were in the part chance moves most (`itself` in the scorecard).
Under 0.5, chance alone moves that part by a factor of two: the look is marked "says little",
it leans toward matching, and it should be read as "nothing was found", not as "these match".
A short effect is judged the same way, and for the same reason says less.

**What it can no longer tell**, for a part read over the whole effect:

- *The order of things over the effect.* Sorted curves and a single palette are the same for a
  fireworks that builds up as for one that dies down, for a fade in as for a fade out, and for
  colors that come in another order. Only how much of each there is over the whole effect is
  compared.
- *Whether it pulses in step with anything.* That is the point for bursts at chance moments. It
  is also why a burst fired by a timing track or the music is not declared so.
- *Where each thing is at each moment.* Where the light fell over the whole effect is still
  compared: every burst in the middle of the prop against bursts all over it is caught.

And for any look held against other seeds:

- *What PixelFlow leaves to chance and xLights doesn't.* The allowance is what PixelFlow's own
  renders disagree on. If PixelFlow scattered by chance something xLights always puts in one
  place, the scatter would be allowed. Only PixelFlow can be rendered again, so xLights' own
  chance is never seen: it is taken to be as large.
- *A difference smaller than chance.* Where the look "says little" that is a large difference.
  The calibration below says how large.
- *A steady effect's fault under a busy one.* An effect that keeps time is held to it as far as
  the picture it is judged in keeps still from seed to seed. Under falling snow that is nearly
  all the way. Under something that changes most of the picture, most of the fault is allowed.
- *What is too sparse or too dim to count.* This is not new. A share of pixels lit under one in
  a hundred, a brightness under 0.4%, and fewer than one flash per pixel every four seconds are
  each taken as nothing, for every effect. Fireworks as xLights' usual settings come across
  light a few pixels in a thousand of a group of outline props, and twice as many bursts there
  is twice next to nothing.

`--by-moment` judges every effect moment by moment against xLights' render alone, as all were
before. A scorecard made without it shows both: each effect held against other seeds has the
look it scores by moment beside the one it is judged on, and "How it looks" gives the whole
sequence both ways.

### Calibration

`pf-fidelity calibrate` holds the score against PixelFlow's own renders, where the answer is
known. One render of an effect stands in for xLights' frames, and the effect is rendered again
from other seeds, or with one setting changed, and scored against it as an import is against
its reference. The tables below are its output: effects 60 seconds long in 25 ms frames, each
line scored eight times from fresh seeds, on a 32 by 16 matrix and on a group of eight outline
props (1,200 pixels on a grid of 400 by 179 cells, so that most of what an effect draws falls
between the props). "By moment" is the look held moment by moment against the one render
alone, as every effect was judged before. "Next to other seeds" is what an effect that draws
on chance is judged on now. A look is the middle score of the eight with the least and the
most; the counts are matches / close / different; "itself" is as above, averaged; the parts
are coverage, brightness, color, pattern, motion, and rhythm, averaged.

#### The same effect from other seeds

Every line should match.

| Effect | Shown | On | Chance decides | By moment: look (least–most) | Matches / close / different | Next to other seeds: look (least–most) | Matches / close / different | Itself | Parts judged on |
|---|---|---|---|---|---|---|---|---|---|
| Fireworks | the same | matrix | its moments | 51 (44–57) | 0 / 2 / 6 | 100 (93–100) | 8 / 0 / 0 | 0.51 | 1.00 1.00 1.00 0.96 1.00 1.00 |
| Fireworks, big bursts | the same | matrix | its moments | 49 (42–57) | 0 / 1 / 7 | 100 (97–100) | 8 / 0 / 0 | 0.52 | 1.00 1.00 1.00 0.99 1.00 1.00 |
| Fireworks, as imported | the same | matrix | its moments | 42 (37–44) | 0 / 0 / 8 | 100 (100–100) | 8 / 0 / 0 | 0.59 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Fireworks, dark between bursts | the same | matrix | its moments | 24 (21–25) | 0 / 0 / 8 | 100 (96–100) | 8 / 0 / 0 | 0.62 | 1.00 1.00 0.99 1.00 1.00 1.00 |
| Circles | the same | matrix | its moments | 58 (54–78) | 0 / 7 / 1 | 100 (78–100) | 7 / 1 / 0 | 0.05 | 1.00 1.00 1.00 1.00 1.00 0.93 |
| Meteors | the same | matrix | its moments | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.90 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Snowflakes, blowing | the same | matrix | its moments | 100 (83–100) | 7 / 1 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.73 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Snowflakes, falling | the same | matrix | its moments | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.88 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Lines | the same | matrix | its moments | 68 (67–72) | 0 / 8 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.77 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Lightning | the same | matrix | its moments | 50 (48–51) | 0 / 0 / 8 | 100 (100–100) | 8 / 0 / 0 | 0.89 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Tendril, wandering | the same | matrix | its moments | 94 (93–96) | 8 / 0 / 0 | 100 (98–100) | 8 / 0 / 0 | 0.67 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Shape | the same | matrix | its moments | 60 (56–65) | 0 / 8 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.55 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Shape, all at once | the same | matrix | what it draws | 62 (61–67) | 0 / 8 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.28 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Life | the same | matrix | what it draws | 57 (48–69) | 0 / 5 / 3 | 100 (59–100) | 6 / 2 / 0 | 0.24 | 0.85 0.84 1.00 0.96 1.00 1.00 |
| Twinkle | the same | matrix | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.92 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Strobe | the same | matrix | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.92 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Fire, rolling | the same | matrix | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.98 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Fire, flickering | the same | matrix | what it draws | 88 (88–89) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.58 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| On with sparkles | the same | matrix | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.90 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Fireworks | the same | outlines | its moments | 34 (30–35) | 0 / 0 / 8 | 100 (91–100) | 8 / 0 / 0 | 0.41 | 1.00 1.00 1.00 0.97 1.00 1.00 |
| Fireworks, big bursts | the same | outlines | its moments | 39 (34–44) | 0 / 0 / 8 | 100 (100–100) | 8 / 0 / 0 | 0.52 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Fireworks, as imported | the same | outlines | its moments | 34 (32–45) | 0 / 0 / 8 | 100 (100–100) | 8 / 0 / 0 | 0.45 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Fireworks, dark between bursts | the same | outlines | its moments | 28 (25–29) | 0 / 0 / 8 | 100 (96–100) | 8 / 0 / 0 | 0.56 | 1.00 1.00 0.98 1.00 1.00 1.00 |
| Circles | the same | outlines | its moments | 39 (36–47) | 0 / 0 / 8 | 100 (100–100) | 8 / 0 / 0 | 0.64 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Meteors | the same | outlines | its moments | 63 (61–64) | 0 / 8 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.76 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Snowflakes, blowing | the same | outlines | its moments | 63 (63–65) | 0 / 8 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.94 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Snowflakes, falling | the same | outlines | its moments | 63 (63–67) | 0 / 8 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.91 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Lines | the same | outlines | its moments | 46 (41–50) | 0 / 0 / 8 | 100 (95–100) | 8 / 0 / 0 | 0.46 | 1.00 1.00 1.00 0.97 1.00 1.00 |
| Lightning | the same | outlines | its moments | 49 (48–51) | 0 / 0 / 8 | 100 (100–100) | 8 / 0 / 0 | 0.93 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Tendril, wandering | the same | outlines | its moments | 59 (59–60) | 0 / 8 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.63 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Shape | the same | outlines | its moments | 36 (35–42) | 0 / 0 / 8 | 100 (100–100) | 8 / 0 / 0 | 0.76 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Shape, all at once | the same | outlines | what it draws | 37 (35–46) | 0 / 0 / 8 | 100 (87–100) | 8 / 0 / 0 | 0.08 | 1.00 1.00 1.00 0.99 1.00 0.96 |
| Life | the same | outlines | what it draws | 65 (60–76) | 0 / 8 / 0 | 100 (97–100) | 8 / 0 / 0 | 0.31 | 1.00 1.00 1.00 0.99 1.00 1.00 |
| Twinkle | the same | outlines | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.89 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Strobe | the same | outlines | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.84 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Fire, rolling | the same | outlines | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.97 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Fire, flickering | the same | outlines | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.99 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| On with sparkles | the same | outlines | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.98 | 1.00 1.00 1.00 1.00 1.00 1.00 |

#### One thing wrong

No line should match.

| Effect | Shown | On | Chance decides | By moment: look (least–most) | Matches / close / different | Next to other seeds: look (least–most) | Matches / close / different | Itself | Parts judged on |
|---|---|---|---|---|---|---|---|---|---|
| Fireworks, big bursts | twice as many bursts | matrix | its moments | 50 (47–56) | 0 / 1 / 7 | 83 (81–87) | 2 / 6 / 0 | 0.53 | 0.73 0.67 1.00 0.99 1.00 1.00 |
| Fireworks, big bursts | half as many bursts | matrix | its moments | 30 (26–41) | 0 / 0 / 8 | 78 (75–81) | 0 / 8 / 0 | 0.60 | 0.58 0.56 1.00 1.00 1.00 1.00 |
| Fireworks, big bursts | bursts twice the size | matrix | its moments | 37 (34–44) | 0 / 0 / 8 | 69 (64–71) | 0 / 8 / 0 | 0.62 | 0.76 0.75 1.00 0.84 0.41 0.95 |
| Fireworks, big bursts | bursts half the size | matrix | its moments | 34 (31–38) | 0 / 0 / 8 | 74 (72–75) | 0 / 8 / 0 | 0.49 | 0.51 0.51 1.00 1.00 0.89 1.00 |
| Fireworks, big bursts | all in a wrong color (green) | matrix | its moments | 27 (24–29) | 0 / 0 / 8 | 41 (40–45) | 0 / 0 / 8 | 0.52 | 1.00 1.00 0.00 0.20 1.00 1.00 |
| Fireworks, big bursts | one of its three colors wrong (green for red) | matrix | its moments | 37 (33–42) | 0 / 0 / 8 | 91 (88–99) | 8 / 0 / 0 | 0.49 | 1.00 1.00 0.85 1.00 1.00 1.00 |
| Fireworks, big bursts | half as bright | matrix | its moments | 45 (37–48) | 0 / 0 / 8 | 80 (76–82) | 0 / 8 / 0 | 0.53 | 0.93 0.52 1.00 0.97 1.00 1.00 |
| Fireworks, big bursts | sparks glow twice as long | matrix | its moments | 43 (38–51) | 0 / 0 / 8 | 82 (80–85) | 0 / 8 / 0 | 0.62 | 0.72 0.65 1.00 0.99 0.91 0.99 |
| Fireworks, big bursts | sparks glow half as long | matrix | its moments | 31 (27–38) | 0 / 0 / 8 | 78 (75–81) | 0 / 8 / 0 | 0.62 | 0.59 0.57 1.00 0.98 0.99 0.99 |
| Fireworks, big bursts | every burst in the middle | matrix | its moments | 47 (44–56) | 0 / 1 / 7 | 74 (69–81) | 0 / 8 / 0 | 0.57 | 0.96 0.96 1.00 0.46 0.86 1.00 |
| Fireworks, big bursts | steady light: as many sparks, spread evenly over the time | matrix | its moments | 49 (47–53) | 0 / 0 / 8 | 62 (57–68) | 0 / 8 / 0 | 0.82 | 0.86 0.60 0.67 0.44 0.84 0.95 |
| Fireworks, big bursts | a steady twinkle | matrix | what it draws | 42 (40–45) | 0 / 0 / 8 | 42 (41–45) | 0 / 0 / 8 | 0.93 | 0.85 0.95 0.59 0.07 0.39 0.92 |
| Fireworks, big bursts | nothing at all | matrix | its moments | 7 (7–7) | 0 / 0 / 8 | 7 (7–7) | 0 / 0 / 8 | 1.00 | 0.01 0.01 0.00 0.01 0.04 0.85 |
| Fireworks | twice as many bursts | matrix | its moments | 49 (44–62) | 0 / 2 / 6 | 79 (77–81) | 0 / 8 / 0 | 0.55 | 0.60 0.61 0.99 0.98 1.00 1.00 |
| Fireworks | half as bright | matrix | its moments | 47 (43–50) | 0 / 0 / 8 | 81 (80–84) | 0 / 8 / 0 | 0.44 | 0.77 0.61 1.00 1.00 1.00 1.00 |
| Fireworks, dark between bursts | twice as many bursts | matrix | its moments | 20 (19–26) | 0 / 0 / 8 | 90 (58–95) | 5 / 3 / 0 | 0.63 | 0.83 0.86 1.00 0.78 1.00 1.00 |
| Fireworks, dark between bursts | steady light: as many sparks, spread evenly over the time | matrix | its moments | 20 (18–23) | 0 / 0 / 8 | 25 (24–27) | 0 / 0 / 8 | 0.61 | 0.34 0.44 0.16 0.23 1.00 0.01 |
| Fireworks, as imported | twice as many bursts | matrix | its moments | 37 (36–38) | 0 / 0 / 8 | 94 (93–95) | 8 / 0 / 0 | 0.55 | 0.94 0.88 1.00 1.00 1.00 0.96 |
| Fireworks, as imported | four times as many bursts | matrix | its moments | 32 (31–35) | 0 / 0 / 8 | 66 (64–68) | 0 / 8 / 0 | 0.54 | 0.53 0.36 1.00 1.00 1.00 0.58 |
| Circles | twice as many balls | matrix | its moments | 53 (46–79) | 0 / 3 / 5 | 78 (57–80) | 0 / 8 / 0 | 0.08 | 0.61 0.59 1.00 1.00 0.79 0.84 |
| Circles | balls twice the size | matrix | its moments | 41 (36–54) | 0 / 0 / 8 | 59 (55–63) | 0 / 8 / 0 | 0.16 | 0.29 0.27 1.00 0.94 0.68 0.98 |
| Snowflakes, blowing | twice as many flakes | matrix | its moments | 75 (65–83) | 0 / 8 / 0 | 78 (67–79) | 0 / 8 / 0 | 0.77 | 0.59 0.55 1.00 1.00 1.00 0.81 |
| Meteors | twice as many meteors | matrix | its moments | 94 (93–95) | 8 / 0 / 0 | 82 (81–83) | 0 / 8 / 0 | 0.94 | 0.67 0.63 1.00 1.00 1.00 0.95 |
| Meteors | falling half as fast | matrix | its moments | 79 (76–80) | 0 / 8 / 0 | 85 (83–86) | 2 / 6 / 0 | 0.38 | 1.00 1.00 1.00 1.00 0.62 1.00 |
| Lightning | twice as many strikes | matrix | its moments | 51 (49–51) | 0 / 0 / 8 | 75 (74–78) | 0 / 8 / 0 | 0.63 | 0.55 0.60 1.00 1.00 1.00 0.80 |
| Life | a new generation twice as often | matrix | what it draws | 52 (45–84) | 0 / 2 / 6 | 98 (68–100) | 6 / 2 / 0 | 0.17 | 0.92 0.92 1.00 1.00 0.91 1.00 |
| Fireworks, big bursts | twice as many bursts | outlines | its moments | 48 (43–53) | 0 / 0 / 8 | 81 (79–85) | 1 / 7 / 0 | 0.57 | 0.68 0.62 0.96 1.00 1.00 0.99 |
| Fireworks, big bursts | half as many bursts | outlines | its moments | 27 (23–35) | 0 / 0 / 8 | 78 (73–80) | 0 / 8 / 0 | 0.51 | 0.58 0.56 1.00 0.93 1.00 0.97 |
| Fireworks, big bursts | bursts twice the size | outlines | its moments | 37 (34–41) | 0 / 0 / 8 | 78 (72–82) | 0 / 8 / 0 | 0.62 | 0.67 0.66 1.00 0.73 1.00 0.87 |
| Fireworks, big bursts | bursts half the size | outlines | its moments | 31 (27–34) | 0 / 0 / 8 | 69 (67–73) | 0 / 8 / 0 | 0.25 | 0.40 0.42 1.00 1.00 1.00 0.90 |
| Fireworks, big bursts | all in a wrong color (green) | outlines | its moments | 28 (27–30) | 0 / 0 / 8 | 44 (40–50) | 0 / 0 / 8 | 0.54 | 1.00 1.00 0.00 0.41 1.00 1.00 |
| Fireworks, big bursts | one of its three colors wrong (green for red) | outlines | its moments | 36 (34–38) | 0 / 0 / 8 | 95 (89–97) | 8 / 0 / 0 | 0.49 | 1.00 1.00 0.88 0.99 1.00 1.00 |
| Fireworks, big bursts | half as bright | outlines | its moments | 38 (33–44) | 0 / 0 / 8 | 80 (78–81) | 0 / 8 / 0 | 0.53 | 0.95 0.52 1.00 1.00 1.00 0.96 |
| Fireworks, big bursts | sparks glow twice as long | outlines | its moments | 36 (35–39) | 0 / 0 / 8 | 79 (76–81) | 0 / 8 / 0 | 0.58 | 0.71 0.65 1.00 0.99 0.66 0.95 |
| Fireworks, big bursts | sparks glow half as long | outlines | its moments | 25 (22–30) | 0 / 0 / 8 | 74 (73–76) | 0 / 8 / 0 | 0.60 | 0.60 0.58 1.00 0.91 0.64 1.00 |
| Fireworks, big bursts | every burst in the middle | outlines | its moments | 35 (32–38) | 0 / 0 / 8 | 61 (59–63) | 0 / 8 / 0 | 0.60 | 0.99 0.99 1.00 0.12 1.00 1.00 |
| Fireworks, big bursts | steady light: as many sparks, spread evenly over the time | outlines | its moments | 41 (39–44) | 0 / 0 / 8 | 59 (54–62) | 0 / 7 / 1 | 0.81 | 0.95 0.55 0.74 0.42 0.82 0.43 |
| Fireworks, big bursts | a steady twinkle | outlines | what it draws | 43 (42–46) | 0 / 0 / 8 | 44 (43–47) | 0 / 0 / 8 | 0.90 | 0.87 0.91 0.53 0.06 0.98 0.42 |
| Fireworks, big bursts | nothing at all | outlines | its moments | 2 (2–3) | 0 / 0 / 8 | 2 (2–3) | 0 / 0 / 8 | 1.00 | 0.01 0.01 0.00 0.01 0.04 0.21 |
| Fireworks | twice as many bursts | outlines | its moments | 34 (33–36) | 0 / 0 / 8 | 80 (78–82) | 0 / 8 / 0 | 0.52 | 0.62 0.62 0.98 1.00 1.00 0.98 |
| Fireworks | half as bright | outlines | its moments | 29 (27–34) | 0 / 0 / 8 | 86 (83–88) | 5 / 3 / 0 | 0.49 | 0.93 0.67 1.00 0.97 1.00 0.99 |
| Fireworks, dark between bursts | twice as many bursts | outlines | its moments | 26 (24–28) | 0 / 0 / 8 | 97 (95–97) | 8 / 0 / 0 | 0.83 | 0.94 0.94 0.99 1.00 1.00 1.00 |
| Fireworks, dark between bursts | steady light: as many sparks, spread evenly over the time | outlines | its moments | 28 (26–30) | 0 / 0 / 8 | 78 (52–98) | 3 / 4 / 1 | 0.62 | 0.94 0.95 0.83 0.64 0.99 1.00 |
| Fireworks, as imported | twice as many bursts | outlines | its moments | 36 (32–41) | 0 / 0 / 8 | 100 (95–100) | 8 / 0 / 0 | 0.68 | 1.00 1.00 0.99 0.99 1.00 1.00 |
| Fireworks, as imported | four times as many bursts | outlines | its moments | 47 (34–48) | 0 / 0 / 8 | 99 (79–100) | 7 / 1 / 0 | 0.72 | 1.00 1.00 0.89 0.94 1.00 1.00 |
| Circles | twice as many balls | outlines | its moments | 45 (39–53) | 0 / 0 / 8 | 100 (82–100) | 7 / 1 / 0 | 0.57 | 1.00 0.99 0.95 1.00 0.95 1.00 |
| Circles | balls twice the size | outlines | its moments | 36 (32–50) | 0 / 0 / 8 | 97 (91–100) | 8 / 0 / 0 | 0.58 | 0.99 0.96 1.00 1.00 0.97 1.00 |
| Snowflakes, blowing | twice as many flakes | outlines | its moments | 64 (63–66) | 0 / 8 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.92 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Meteors | twice as many meteors | outlines | its moments | 63 (60–65) | 0 / 8 / 0 | 99 (99–100) | 8 / 0 / 0 | 0.76 | 0.99 0.97 1.00 1.00 1.00 1.00 |
| Meteors | falling half as fast | outlines | its moments | 54 (48–58) | 0 / 3 / 5 | 88 (88–91) | 8 / 0 / 0 | 0.66 | 1.00 1.00 1.00 1.00 0.71 1.00 |
| Lightning | twice as many strikes | outlines | its moments | 51 (49–51) | 0 / 0 / 8 | 77 (74–80) | 0 / 8 / 0 | 0.61 | 0.56 0.62 1.00 1.00 1.00 0.81 |
| Life | a new generation twice as often | outlines | what it draws | 62 (56–66) | 0 / 8 / 0 | 92 (87–96) | 8 / 0 / 0 | 0.33 | 0.99 0.99 1.00 1.00 0.79 1.00 |

#### Short effects and few events

Fireworks with a big burst a second, from one second and one burst to two minutes.

| Effect | Shown | On | Seconds | Chance decides | By moment: look (least–most) | Matches / close / different | Next to other seeds: look (least–most) | Matches / close / different | Itself | Parts judged on |
|---|---|---|---|---|---|---|---|---|---|---|
| Fireworks, big bursts | the same | matrix | 1 | its moments | 35 (28–100) | 2 / 0 / 6 | 76 (48–100) | 4 / 0 / 4 | 0.00 | 1.00 1.00 0.62 0.88 0.90 1.00 |
| Fireworks, big bursts | twice as many bursts | matrix | 1 | its moments | 35 (18–54) | 0 / 0 / 8 | 82 (45–100) | 4 / 2 / 2 | 0.05 | 0.90 0.90 0.75 0.81 1.00 1.00 |
| Fireworks, big bursts | the same | matrix | 2 | its moments | 46 (34–59) | 0 / 2 / 6 | 100 (75–100) | 6 / 2 / 0 | 0.01 | 0.96 0.95 1.00 0.93 1.00 1.00 |
| Fireworks, big bursts | twice as many bursts | matrix | 2 | its moments | 50 (24–76) | 0 / 3 / 5 | 93 (85–96) | 8 / 0 / 0 | 0.26 | 0.95 0.87 0.99 1.00 0.95 1.00 |
| Fireworks, big bursts | the same | matrix | 4 | its moments | 42 (26–73) | 0 / 2 / 6 | 94 (55–100) | 5 / 2 / 1 | 0.31 | 0.99 1.00 0.87 0.81 0.95 1.00 |
| Fireworks, big bursts | twice as many bursts | matrix | 4 | its moments | 44 (36–54) | 0 / 0 / 8 | 82 (58–92) | 2 / 6 / 0 | 0.32 | 0.88 0.78 0.81 0.92 0.96 1.00 |
| Fireworks, big bursts | the same | matrix | 8 | its moments | 55 (28–67) | 0 / 4 / 4 | 100 (50–100) | 7 / 0 / 1 | 0.40 | 1.00 1.00 1.00 0.91 0.88 1.00 |
| Fireworks, big bursts | twice as many bursts | matrix | 8 | its moments | 48 (37–78) | 0 / 1 / 7 | 82 (72–96) | 3 / 5 / 0 | 0.44 | 0.76 0.71 0.95 0.94 0.93 1.00 |
| Fireworks, big bursts | the same | matrix | 15 | its moments | 48 (36–68) | 0 / 3 / 5 | 100 (87–100) | 8 / 0 / 0 | 0.47 | 1.00 1.00 1.00 0.95 0.98 1.00 |
| Fireworks, big bursts | twice as many bursts | matrix | 15 | its moments | 54 (45–58) | 0 / 3 / 5 | 83 (80–92) | 3 / 5 / 0 | 0.51 | 0.74 0.68 0.99 0.97 1.00 1.00 |
| Fireworks, big bursts | the same | matrix | 30 | its moments | 47 (40–55) | 0 / 1 / 7 | 100 (72–100) | 7 / 1 / 0 | 0.52 | 1.00 1.00 1.00 0.92 1.00 1.00 |
| Fireworks, big bursts | twice as many bursts | matrix | 30 | its moments | 52 (45–64) | 0 / 2 / 6 | 82 (80–85) | 1 / 7 / 0 | 0.54 | 0.71 0.65 0.98 0.99 1.00 1.00 |
| Fireworks, big bursts | the same | matrix | 60 | its moments | 49 (37–55) | 0 / 0 / 8 | 100 (100–100) | 8 / 0 / 0 | 0.52 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Fireworks, big bursts | twice as many bursts | matrix | 60 | its moments | 50 (47–55) | 0 / 1 / 7 | 83 (73–86) | 3 / 5 / 0 | 0.53 | 0.71 0.65 1.00 0.95 1.00 1.00 |
| Fireworks, big bursts | the same | matrix | 120 | its moments | 50 (45–59) | 0 / 1 / 7 | 100 (100–100) | 8 / 0 / 0 | 0.51 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Fireworks, big bursts | twice as many bursts | matrix | 120 | its moments | 52 (48–58) | 0 / 2 / 6 | 84 (83–86) | 1 / 7 / 0 | 0.52 | 0.74 0.67 0.99 1.00 1.00 1.00 |

#### Effects that keep time

Effects that keep time, with their timing wrong: alone (judged as ever, moment by moment), with
sparkles on them, and under falling snow (both held against other seeds). What a fault costs
alone, it should cost the other two. Where it costs nothing alone, the look score never saw it.

| Effect | Shown | On | Chance decides | By moment: look (least–most) | Matches / close / different | Next to other seeds: look (least–most) | Matches / close / different | Itself | Parts judged on |
|---|---|---|---|---|---|---|---|---|---|
| Spirals | a quarter of a second early, alone | matrix | nothing | 100 (100–100) | 8 / 0 / 0 | – | – | – | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Spirals | a quarter of a second early, with sparkles | matrix | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 1.00 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Spirals | a quarter of a second early, under falling snow | matrix | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.98 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Chase | a hundredth faster, alone | matrix | nothing | 100 (100–100) | 8 / 0 / 0 | – | – | – | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Chase | a hundredth faster, with sparkles | matrix | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.99 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Chase | a hundredth faster, under falling snow | matrix | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.98 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Chase | half as fast, alone | matrix | nothing | 76 (76–76) | 0 / 8 / 0 | – | – | – | 1.00 1.00 1.00 1.00 0.41 1.00 |
| Chase | half as fast, with sparkles | matrix | what it draws | 76 (76–76) | 0 / 8 / 0 | 77 (76–77) | 0 / 8 / 0 | 0.99 | 1.00 1.00 1.00 1.00 0.42 1.00 |
| Chase | half as fast, under falling snow | matrix | what it draws | 72 (71–72) | 0 / 8 / 0 | 73 (73–73) | 0 / 8 / 0 | 0.96 | 1.00 1.00 1.00 1.00 0.36 0.80 |
| Chase | the other way, alone | matrix | nothing | 60 (60–60) | 0 / 8 / 0 | – | – | – | 1.00 1.00 1.00 1.00 0.01 1.00 |
| Chase | the other way, with sparkles | matrix | what it draws | 60 (60–60) | 0 / 8 / 0 | 60 (60–60) | 0 / 8 / 0 | 0.99 | 1.00 1.00 1.00 1.00 0.01 1.00 |
| Chase | the other way, under falling snow | matrix | what it draws | 60 (60–60) | 0 / 8 / 0 | 60 (60–60) | 0 / 8 / 0 | 0.98 | 1.00 1.00 1.00 1.00 0.02 1.00 |
| Shimmer | a quarter of a second late, alone | matrix | nothing | 63 (63–63) | 0 / 8 / 0 | – | – | – | 1.00 1.00 1.00 1.00 1.00 0.00 |
| Shimmer | a quarter of a second late, with sparkles | matrix | what it draws | 63 (63–63) | 0 / 8 / 0 | 63 (63–63) | 0 / 8 / 0 | 0.94 | 1.00 1.00 1.00 1.00 1.00 0.00 |
| Shimmer | a quarter of a second late, under falling snow | matrix | what it draws | 61 (61–62) | 0 / 8 / 0 | 61 (61–62) | 0 / 8 / 0 | 0.95 | 1.00 1.00 1.00 0.89 1.00 0.00 |
| Spirals | a quarter of a second early, alone | outlines | nothing | 89 (89–89) | 8 / 0 / 0 | – | – | – | 1.00 1.00 1.00 1.00 1.00 0.71 |
| Spirals | a quarter of a second early, with sparkles | outlines | what it draws | 89 (89–89) | 8 / 0 / 0 | 89 (89–89) | 8 / 0 / 0 | 0.99 | 1.00 1.00 1.00 1.00 1.00 0.71 |
| Spirals | a quarter of a second early, under falling snow | outlines | what it draws | 89 (89–89) | 8 / 0 / 0 | 89 (89–89) | 8 / 0 / 0 | 1.00 | 1.00 1.00 1.00 1.00 1.00 0.71 |
| Chase | a hundredth faster, alone | outlines | nothing | 100 (100–100) | 8 / 0 / 0 | – | – | – | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Chase | a hundredth faster, with sparkles | outlines | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.99 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Chase | a hundredth faster, under falling snow | outlines | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 1.00 | 1.00 1.00 1.00 1.00 1.00 1.00 |
| Chase | half as fast, alone | outlines | nothing | 81 (81–81) | 0 / 8 / 0 | – | – | – | 1.00 1.00 1.00 1.00 0.58 0.72 |
| Chase | half as fast, with sparkles | outlines | what it draws | 83 (83–83) | 0 / 8 / 0 | 83 (83–83) | 0 / 8 / 0 | 1.00 | 1.00 1.00 1.00 1.00 0.63 0.73 |
| Chase | half as fast, under falling snow | outlines | what it draws | 81 (81–81) | 0 / 8 / 0 | 81 (81–81) | 0 / 8 / 0 | 1.00 | 1.00 1.00 1.00 1.00 0.59 0.73 |
| Chase | the other way, alone | outlines | nothing | 100 (100–100) | 8 / 0 / 0 | – | – | – | 1.00 1.00 1.00 1.00 0.99 1.00 |
| Chase | the other way, with sparkles | outlines | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 0.99 | 1.00 1.00 1.00 1.00 0.99 1.00 |
| Chase | the other way, under falling snow | outlines | what it draws | 100 (100–100) | 8 / 0 / 0 | 100 (100–100) | 8 / 0 / 0 | 1.00 | 1.00 1.00 1.00 1.00 0.99 1.00 |
| Shimmer | a quarter of a second late, alone | outlines | nothing | 63 (63–63) | 0 / 8 / 0 | – | – | – | 1.00 1.00 1.00 1.00 1.00 0.00 |
| Shimmer | a quarter of a second late, with sparkles | outlines | what it draws | 63 (63–63) | 0 / 8 / 0 | 63 (63–63) | 0 / 8 / 0 | 1.00 | 1.00 1.00 1.00 1.00 1.00 0.00 |
| Shimmer | a quarter of a second late, under falling snow | outlines | what it draws | 63 (63–63) | 0 / 8 / 0 | 63 (63–63) | 0 / 8 / 0 | 0.97 | 1.00 1.00 1.00 1.00 1.00 0.00 |

#### What the tables say

**The same effect from other seeds.** Of 304 scores, 111 matched by moment, 80 were close, and
113 were different. Next to other seeds 301 match and 3 are close: Circles on the matrix once
(78) and Life on the matrix twice (59 at the least). Both draw their chance once for a whole
effect (which way three balls set off, which cells start alive), three renders say least about
that, and both looks are marked "says little" (itself 0.05 and 0.24).

**One thing wrong.** Of 416 scores, 8 matched by moment, 52 were close, and 356 were different,
which was no credit to it: it called the same effect different as readily. Next to other seeds
58 are different, 240 close, and 118 match. Told on both stages in every one of eight scorings,
with bursts big enough to cross the props: half as many bursts (78), bursts twice or half the
size (69 to 78), every burst in a wrong color (41 and 44, different), half as bright (80),
sparks that glow twice or half as long (74 to 82), every burst in the middle (74 and 61), a
steady twinkle in their place (42 and 44, different), and nothing at all (7 and 2). The same
sparks spread evenly over the time are close (62 and 59), and different once in 16. Twice as
many meteors on the matrix matched by moment (94) and is close now (82): there is more light
than chance gives.

Not told, or not every time:

- *Twice as many bursts* is told where there are many and they are small, and sits on the line
  or over it otherwise. The usual Fireworks: close in all 16 scorings (79 and 80). Big bursts:
  83 on the matrix and 81 on the outlines, a match in 3 of 16, because where bursts overlap
  twice the bursts is less than twice the light, and the line for a match is 85. Few bursts
  with dark between them: 90 and 97, a match in 13 of 16, because so few differ nearly as much
  from one seed to the next.
- *One color of three wrong* matches in all 16 (91 and 95). A third of the light in another hue
  takes 0.12 to 0.15 from the color part and nothing from the rest, and that is a match by the
  yardstick every effect is held to.
- *Light too sparse to count.* Fireworks as an import usually sets them, with twice as many
  bursts: 94 on the matrix and 100 on the outlines. With four times as many: 66 and 99. And on
  the outlines, where each of these lights under one pixel in a hundred of the props: twice as
  many balls (100), balls twice the size (97), twice as many snowflakes (100) or meteors (99),
  and meteors at half the speed (88) all match, and the usual Fireworks at half the brightness
  matches in 5 scorings of 8. On the matrix those six are close (59 to 85), but for meteors at
  half the speed in 2 scorings of 8.
- *Steady light for bursts with dark between them, on the outlines*: 78, a match in 3 of 8 and
  different in 1. On the matrix it is different in all 8, at 25. On the outlines those bursts
  cross too few pixels for the dark between them to count.
- *A Life whose generations come twice as often*: 98 and 92. By moment it was close or
  different, and so was the same Life from another seed.

**Short effects and few events.** With a big burst a second, the same Fireworks from another
seed matches from 15 seconds on, in all but one scoring of 32, and one with twice as many
bursts is close more often than it matches from 4 seconds on. Under that, little is told. At
one second the same effect is different in 4 scorings of 8 and one with twice the bursts in 2.
At two seconds twice the bursts matches in all 8. At 4 and at 8 seconds the same effect is
still different once in 8. Those looks are marked "says little" (itself 0.00 to 0.44 on
average, up to 8 seconds), and "nothing was found" is all that should be read from them. From
15 seconds on a Fireworks' itself stays near 0.5 however long it runs (0.47 to 0.54), so the
mark comes and goes on a Fireworks whose score no longer moves.

**Effects that keep time.** Each fault scores the same with sparkles on the effect or falling
snow over it, held against other seeds, as it does by moment: to within a point on all 20
lines. Alone, it is judged by moment as ever. Two of the faults the look score never saw as
less than a match, and still doesn't: a Chase a hundredth faster (100 on every line), and
Spirals a quarter of a second early (100 on the matrix; 89 on the outlines, for a rhythm out
of step). A Chase half as fast is close (73 to 83), a Chase the other way is close on the
matrix (60) and unseen on the outlines (100), and a Shimmer a quarter of a second late is
close (61 to 63).

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
   they would be. When any effect draws on chance, the sequence is rendered twice more, every
   effect from another seed.
4. **Line the frames up by time**: PixelFlow's frame at `t` against the reference's frame at `t`
   (`--frame-offset` moves that).
5. **Look at every effect twice**: on the pixel-frames where it was the only effect (`lookSolo`),
   and on all of them, whatever else was on (`lookLayered`). It is judged alone when at least a
   tenth of what it was lit on had nothing else on it; otherwise layered. A layered look is the
   look of the picture there, which several effects share: trust "alone", and treat "layered" as
   a hint. An effect that draws on chance has both looks held against PixelFlow's other seeds,
   and one that shares its pixels with such an effect its layered look; what each scores moment
   by moment against xLights' render alone is kept beside it (`lookSoloByMoment`,
   `lookLayeredByMoment`).
6. **Look at every prop** over the frames either side lit it, whatever effects were on it
   (`props[].look`). The mean over the props, by pixel-seconds, is the sequence's `asSeen`:
   the look of the whole picture, with no question of which effect to blame.

## Reading a scorecard

**How it looks** comes first: the share of effect-time (each judged effect's length, added up)
that matches, is close, is different, or has no equivalent, the same by pixel-seconds, the look
score, and its six parts. "Not judged" is effect-time that couldn't be measured. Where effects
were held against PixelFlow's other seeds, the same summary with every effect held moment by
moment against xLights' render alone follows (`lookByMoment`): what the scorecard said before.

**Effects held against other seeds** counts the effects judged next to PixelFlow's other seeds,
and lists those whose look that changed by half a point or more: what chance decides in each
and why, the look it is judged on, what it scores by moment, and how alike PixelFlow's own
renders of it were (`itself`). One marked "says little" has told little.

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
- **Random effects still move the numbers, by less.** Each import seeds them anew, and each
  scorecard draws its other seeds anew: what is allowed for chance is measured from three
  renders, and is itself a little different each time. A look that sits on a line can still
  fall on either side of it. "What it found" has how far the corpus moves.
- **An effect whose moments are chance is not held to the order of things**, and a look with
  little that chance doesn't move says little: see "Effects that draw on chance" for what can
  no longer be told.
- **It takes three renders where it took one.** A corpus that scored in half a minute takes two.
  `--by-moment` is as quick as before.
- **The picture as a whole is still read moment by moment.** A prop's look (`props[].look`, and
  the sequence's `asSeen`) runs over the whole show, through effects that keep time and effects
  that don't, and is not held against other seeds. A prop that shows nothing but bursts at
  chance moments looks worse there than its effect is judged to.
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

| Sequence | Matches | Close | Different | No equivalent | Look | Look by moment | As seen |
|---|---|---|---|---|---|---|---|
| A | 94% | 5% | 2% | – | 97 | 94 | 97 |
| B | 100% | – | – | – | 100 | 87 | 94 |
| C | 100% | – | – | – | 99 | 96 | 99 |
| D | 100% | – | – | – | 100 | 100 | 100 |
| E | 100% | – | – | – | 95 | 94 | 93 |
| G | 70% | – | – | 30% | 79 | 68 | 72 |
| All | 93% | 4% | 1% | 2% | 96 | 93 | 94 |

That is one run of eight, with effects that draw on chance held against PixelFlow's other
seeds. "Look by moment" is the same run with every effect held moment by moment against
xLights' render alone, as all were before: 91% matching, 2% close, 5% different. Of the 242
effects judged, 219 are held against other seeds: 139 that draw on chance with their moments
set, most of them for their sparkles; 40 whose moments are chance; and 40 that draw on none and
share their pixels with one that does. The other 23 are judged exactly as before. Over the
eight runs, 83 in 100 of the looks held against other seeds are within half a point of what
the same renders score by moment (88 in 100 for effects that draw on none, which are never
more than 12 points from it and never get another verdict by it; 87 for those with their
moments set; 67 for those whose moments are chance). Nine of 1,752 are lower, by 0.7 at most.

**How far it moves from run to run.** Each import seeds the random effects anew. Over eight
runs before and eight after:

| | Before: every effect by moment | After |
|---|---|---|
| Look, all sequences | 91.6 to 93.3 | 95.6 to 95.8 |
| Matches, all sequences | 90.8% to 91.5%, and 76.7% in one of eight more | 92.4% to 94.0% |
| Look, B | 85.6 to 97.0 | 99.9 in seven runs, 94.0 in one |
| Matches, B | 59% to 100% | 100% in seven runs, 73% in one |
| Look, A | 92.9 to 95.0 | 96.7 to 96.9 |
| Look, G | 67.8 to 70.1 | 78.8 in all eight |
| As seen, all sequences (not held against other seeds) | 92.9 to 93.9 | 92.9 to 93.8 |

The eight more are the runs after, judged by moment as well: in one of them A's cellular
automaton, nine minutes long, was close, which alone takes A from 94% to 76%. It matched by
moment in the other fifteen, and was close in three of twenty-one earlier runs. Held against
other seeds it scores 100 in all eight: its cells are in other places in PixelFlow's own
renders too.

The effects that moved most from run to run, before and after:

| Effect | Before | After |
|---|---|---|
| A tendril with sparkles, 18 seconds | 64 to 100: close in five runs of eight | Matching in seven runs, close (78) in one. Says little: its sparkles pulse at other moments in every one of PixelFlow's own renders. |
| A Shockwave of 0.9 seconds over falling snow | 76 to 100: close in four | 100 |
| Circles over an On, 10 seconds, and the On | 74 to 94: close in two; 80 to 94: close in one | 100; 100 |
| The Fireworks, 52 seconds | 36 to 50: different | 100. Says little (below). |
| The cellular automaton | 85 to 96, and close in one run of eight more | 100 |
| Snow, a wash, and circles, 15 seconds each | 88 to 100 | 99 to 100 |
| Circles over a wash, in two pictures of 15 seconds (four effects) | 33 to 46: different | 74 to 86: close, but for one picture in one run (86) |

What still moves is that last pair of pictures, the tendril, and a Pinwheel that scores close
either way (70 to 72 before, 75 to 82 after: it shares its pixels with effects that draw on
chance, and what is allowed for them is measured anew with each run's seeds).

**What changed verdict, and why.** Eight effects, 163 seconds of the 4,007 judged.

- *The Fireworks*: different (36 to 50) to matching (100, all eight runs). By moment its color
  and its rhythm scored 0, for bursts at other moments. Held against other seeds, neither is
  told apart from chance. The look says little, though, and says so in every run: on this
  group xLights lights one pixel in ten thousand and PixelFlow about as few, both renders are
  dark but for a few specks, and of the part PixelFlow's own renders share least they have
  between 2 and 46 in 100 in common. "Nothing was found" is all that can be said of it there.
- *The tendril*: close in five runs of eight (64 at the least, for "pulses out of step") to
  matching in seven of eight, saying little. Its sparkles give the props a pulse that comes
  at other moments in each of PixelFlow's own renders: 6 to 34 in 100 in common. By moment its
  rhythm scored 0.02 to 0.09 and every other part 0.9 or more.
- *Circles over a wash, twice* (four effects, two pictures of 15 seconds on one group):
  different (33 to 46) to close (74 to 86). By moment nearly every part scored low, because the
  balls are somewhere else at every moment. Held against other seeds, color, pattern, motion,
  and rhythm are within chance, and what is left is that PixelFlow lights about a sixth of the
  group where xLights lights nearly a third, three fifths as brightly: "less of the target
  lit". This stretch is in the part of the file that wasn't finished (see "A reference that
  wasn't finished"), and close may be as much as its frames can show.
- *A Shape under a wash* (two effects, one picture of 15 seconds): different (48 to 51) to
  close in seven runs of eight (55 to 57; 53 in the other). Where its shapes are no longer
  counts. That a ninth as much of its light changes from frame to frame as in xLights' (2%
  against 18%), and that it is half again as bright, still do. This is the stretch where three
  props hold a turning wheel the sequence doesn't have, and it sits on the line between close
  and different.

Hold a change against several runs before and after, effect by effect: three or four runs now
show what took ten.

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
its time that scored as different by moment (2% different and 3% close now that the circles in
it are held against other seeds), and the five effects that score as close in every run (a
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
music. Three things were found in its frames, and all three are now drawn as the frames show
them.

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
  difference of 0.006, a correlation of 0.99). PixelFlow mixed both channels to one before it
  looked for the highest sample. On this song that was 0.029 and 0.90 from xLights' level, about
  a sixth lower on average, and at full level on 15 of the first 1,500 frames where the left
  channel is on 39. Sparkles that follow the music burst when it is at full level, so PixelFlow
  showed fewer bursts. A fire's music sparkles over three short flashes made one of them look
  80 to 84 ("flashes less often") and the other two 88 to 92. The same fire grows with the
  music, and its own light reached a roof prop on 90 of the 858 frames it plays where xLights'
  reaches it on 165.

  Reading the left channel alone was tried, and matched: the three flashes looked 100 and the
  fire reached the roof on the very frames xLights' does. It was not kept, because a song with
  something on the right alone would go unheard. The level is now the louder channel's: the
  frame's highest sample on either side, as a share of the song's highest on either
  (`pf_analysis::AudioTrack::peak`). A song with as much on one side as the other reads as it
  does in xLights, and a sound on one side alone is heard whichever side it is on. Beats,
  sections, moments, voices, and how loud a frame sounds are still worked out from the two
  mixed, and a song with one channel reads exactly as it did.

  This song's right channel is the louder on six frames in ten, so the level now reads above
  xLights' where it read below: 0.027 and 0.92 from it, about a sixth higher on average, and at
  full level on 67 of those 1,500 frames. Over five runs before and twenty-two after, the three
  flashes go from 80 to 84, 88 to 89, and 90 to 92 up to 100, 99.8 to 99.9, and 99.8 to 100, as
  they did on the left channel alone. Two other flashes over the same fire go the other way,
  from 99.6 to 99.8 down to between 94.2 and 97.1: the fire's sparkles now burst on frames
  where xLights' don't. And the fire reaches the roof on 211 frames: every one of xLights' 165,
  and 46 more. Nothing else in the corpus moves outside what it does from run to run.
- *Sparkles on a see-through effect.* xLights flashes an On's sparkles however see-through the
  On is. On the three loudest frames of one half-second On its transparency is 99 to 100% and
  five or six in ten of its pixels flash, each at full strength over a dark prop. PixelFlow
  flashed sparkles only where the effect showed, and was dark on two of those frames. That was
  the whole of that On's difference: it looked 54.

  An effect's own Transparency now hides the effect and not its sparkles (`pf_render`'s
  `sparkles`), so an On at full Transparency with sparkles is a layer of sparkles alone. Only On
  has such a setting, and a PixelFlow sequence changes only where an On has both sparkles and
  some Transparency. That On now looks 88 to 97 (seventeen runs). What keeps it from 100 is the
  level above: on one of its 23 frames the right channel is at full level and the left at two
  thirds, and PixelFlow flashes six pixels in ten where xLights flashes one in fifteen.

  A wider rule was tried first, sparkles wherever the effect's own color is lit whatever covers
  it, and xLights' frames don't ask for it. Nothing else was found in them that sparkles look
  past:

  - *A fade.* Through a plasma's two-second fade-in its sparkles rise with it, in both renders,
    and show at their three full levels (53%, 75%, and all of the sparkle color) only once the
    fade is over.
  - *Faint light.* The faint parts of an effect flash as often as PixelFlow's already do. Of the
    pixels a fan had lit under a tenth of full a few frames before, 2.98% are flashing in
    xLights' frames and 3.12% in PixelFlow's; of those it had lit brightly, 3.25% and 3.33%. Of
    those in a morph's fading tail, 2.6% and 3.0%; of those in its head, 4.6% and 4.5%.
  - *Nothing at all.* Pixels an effect doesn't reach never flash. A thin wheel's sparkles flash
    on 1,296 pixels in the course of its sixteen seconds in xLights' frames, and every one is a
    pixel the wheel itself crosses.

On a group, xLights' sparkles are also wider. On a matrix twelve pixels wide inside a
layout-wide group, four in ten of its sparkling pixels have the pixel beside them flashing with
them; PixelFlow's flash one at a time (one in a hundred). As many pixels flash on both sides.

**A wheel on a curve moves a whole unit at a time.** PixelFlow's Pinwheel honours fractions in
every setting, so a setting on a curve glides. In xLights' frames it steps. Sequence C has a
still, one-armed wheel whose start angle jumps to 31 degrees on each mark of a timing track and
falls back over ten frames, 3.1 degrees a frame. It looked 92.9 to 93.4 before the Pinwheel was
redrawn to honour fractions, and 88.8 to 90.2 after. Drawn with the start angle's fraction
dropped (31, 27, 24, 21 … 3, 0) it is nearest xLights' frames: a mean error of 3.63 a channel,
against 3.71 with the fraction kept, 3.73 with the angle rounded, and 3.80 with it raised to the
next whole number. Sequence A has wheels whose center slides from one side of the middle to the
other and whose twist winds and unwinds, each on a curve. On the larger group, dropping the
center's fraction (toward zero) brings the mean error from 11.3 (11.29 to 11.39 in four runs)
to 10.85; rounded it stays at 11.3, and rounded down it is 11.7. Dropping, rounding, or raising
the twist's makes no difference that can be told (11.34 to 11.45).

The wheel still glides: the difference is made up in the import
(`pf_xlights::sequence::whole`). A curve on a Pinwheel's start angle or center comes across as
a curve through the whole numbers xLights shows, one for each frame the effect plays on. C's
wheel looks 92.5 to 93.1 (six runs), and the four other Pinwheels are where they were: 100,
97.3 to 97.9, 99.7 to 99.9, and 66 to 76, against 100, 97.3 to 97.7, 99.5 to 99.9, and 68 to
74.

What that leaves. A curve that follows a timing track becomes plain points, read at the marks
the sequence came with: it no longer moves when the marks do. One that follows the music can't
be read until the music is, and one that turns a corner on more frames than a curve has points
(512) doesn't fit: both glide as before. And twist, thickness, speed, and arm length are left
to glide: the frames don't say of twist, and no sequence here has a curve on the others. A
curve that could give whole numbers itself would cover all of these, and is a change to the
saved file.

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
a Fireworks that isn't on a timing track and expects it to look close; the score, held moment
by moment, still said different.

Held against PixelFlow's other seeds and read over the whole effect (see "Effects that draw on
chance"), it scores 100 in eight runs of eight, and sequence G's look goes from about 69 to 79.
On this group that says little, as above: the same Fireworks on a matrix, and on a group of
outlines with a burst big enough to cross them, matches itself from seed to seed and is told
from one with half the bursts, half the size, half the brightness, or another color (see
"Calibration").

Not measured, because no sequence in the corpus has them: a particle fade other than 50 (taken
as half as many frames of glow, and noted), frames other than 25 ms (taken as xLights counting
in frames), colors that change (hold color off), a firework that moves, firing with the music,
and firing from a timing track.

**What to improve, by look:**

- *Mappings to check.* None is known in this corpus. What the last list had here is fixed (the
  Spirals a turn early, and the half-second On: see "What follows the music"), or is the
  reference's doing (the Pinwheel on a small group that scores Close, and the
  wash and the stars under it: the reference has a wheel in two blues there that the sequence
  has no effect for). Not measured, because no sequence here has one: a strobing twinkle's share
  of the pixels at more than one count and number of steps (it is taken to rise with the first
  and fall with the second), and a strobing twinkle in the older way of drawing twinkles, which
  is still shown as soft twinkles.
- *Decisions to make.* What an imported Fireworks is called: the importer expects close, for
  "bursts go off at other random moments than xLights'", and the score now says that alone is
  no difference (it matches, saying little, in this corpus's one). And
  whether a curve should be able to give whole numbers itself, so that an imported Pinwheel
  setting can step and still follow its timing track or the music (see "A wheel on a curve
  moves a whole unit at a time"): that changes the saved file. The two that were here are made,
  and measured above: the music an effect follows is the louder of the two channels, and
  sparkles flash on an effect its own Transparency hides.
- *What PixelFlow can't show yet.* Shader (ten effects, the largest item by far) and Text in
  the sequence's own font. A Tendril that wanders at random shows less of itself and pulses at
  other moments (the pulsing no longer counts against it: PixelFlow's own renders of it pulse
  at other moments from each other, see "What changed verdict"). A Wave has no square shape. A
  Fireworks that moves stays where it goes off.

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
