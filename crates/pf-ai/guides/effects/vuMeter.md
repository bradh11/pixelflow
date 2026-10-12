# VU Meter

Bars, levels, flashes, and shapes driven by the song's notes and loudness or by a timing track's marks: lights that visibly follow the sound.

## What it draws

Forty-four meters in one, chosen with `meter`. Most read the song, the timing ones a timing track; without it they draw nothing.

- On a matrix every type works. The default is a spectrum analyser: `bars` columns, low notes at the left, each as tall as its notes are loud, the palette running from bottom to top.
- On a tree the same picture wraps round the cone: bars and rising levels read well, shapes don't.
- A string or an arch is one row, so height is lost: the spectrograms, `volumeBars`, `waveform`, and filled shapes show nothing or one flat color. The whole-prop types, the bar types (a segment hopping along), `intensityWave`, and the timed sweeps and chases all work.
- On the whole house (a group) the picture lies over the layout: spectrum bars are columns across the yard, so ground props light most and the roofline only at peaks; `levelJump100` fills the house from the ground up on each hit. With `renderStyle` `horizontalPerModel` each prop is one bar filling along its length; with `singleLineModelAsPixel` a bar type lights one whole prop at a time.

## Settings

`meter`, by family.

Spectrograms show which notes sound, from `startNote` to `endNote`:

- `spectrogram`: the bars. `spectrogramPeak`: plus a marker in the last palette color resting on each bar's recent peak. `spectrogramLine`: a line through the bar tops, earlier lines fading behind. `spectrogramCircleLine`: that line bent into a wobbling loop round the middle.

Loudness, drawn all the time:

- `on`: the whole prop in the first color, as bright as the music is loud. `colorOn`: sliding from the first palette color (quiet) to the last (loud).
- `volumeBars`: a scrolling history, newest at the right: the last `bars` frames, each a column as tall as it was loud. `waveform`: the same, mirrored about the middle. `intensityWave`: the history as full-height stripes, the first color when loud and the last when quiet.
- `levelShape`: the `shape` in the middle, swelling with the loudness.
- `dominantFrequencyColor`: the whole prop in the palette color picked by the loudest note in range, low notes first. `dominantFrequencyColorGradient` blends between them.

Loudness, set off when it passes `sensitivity`:

- `levelPulse`: the whole prop flashes the first color and fades over `bars` frames. `levelPulseColor`: the next palette color each flash. `levelColor`: holds a color, stepping to the next on each hit.
- `levelJump`: a block leaps from the bottom to the level's height and drains over `bars` frames. `levelJump100`: always to the top.
- `levelBar`: one of `bars` segments across is lit, moving to the next, in the next color. `levelRandomBar`: to a random one. They move on every frame above it, so use 80 or more for one step a hit.

Note types are level types that hear only `startNote` to `endNote`: `noteOn`, `noteLevelPulse`, `noteLevelJump`, `noteLevelJump100`, `noteLevelBar`, `noteLevelRandomBar`. Narrow the range to an instrument: about 28 to 45 for kick and bass, 60 to 84 for a melody (60 is middle C).

Timing types follow the marks of `timingTrack` and need no song:

- `pulse`, `timingEventPulse`: the whole prop flashes the first color on each mark, fading over `bars` frames. `timingEventPulseColor`: the next color each time. `timingEventColor`: holds a color, changing on each mark, and dims in any gap between marks.
- `timingEventJump100`: a block jumps to the top on each mark and drains. `timingEventJump`: to the music's level (needs the song too).
- `timingEventBar`: the next of `bars` segments on each mark. `timingEventBarBounce`: along and back. `timingEventRandomBar`: a random one. `timingEventBars`: every segment lit in the palette's colors, shifting one along on each mark.
- `timingEventSpike`: a stripe enters at the right on each mark and slides off the left over `bars` frames. `timingEventSweep` colors it with the palette bottom to top, `timingEventSweep2` across.
- `timingEventTimedSweep`: a band `bars` columns wide crosses the prop left to right in exactly the length of each mark, entering and leaving. `timingEventTimedSweep2` stays inside the prop. `timingEventAlternateTimedSweep` and `timingEventAlternateTimedSweep2` send every other one backwards. `timingEventChaseFromMiddle` runs two bands from the middle out to the ends; `timingEventChaseToMiddle` from the ends in, crossing and running on out. Best on strings and arches.

The rest:

- `bars`: as above. As frames (25 ms unless the sequence says otherwise), 6 is a blink, 16 to 20 most of a beat. As segments or stripes it should divide the prop's width, or the right-hand end stays dark.
- `sensitivity`: the level, in percent, that sets off the level and note types: 50 to 70, higher if they fire constantly. For `spectrogramPeak`, the frames a peak holds. For `levelShape`, its size: use 15 to 25, as the default overflows the prop.
- `gain`: boosts or cuts the level read; raise it for a quiet song. The bar spectrograms ignore it.
- `filter`: a word a mark's label must have. On a Beats track, 1 keeps the downbeats. `pulse` ignores it.
- `shape`: glowing outlines: `circle`, `square`, `diamond`, `star`, `tree`, `crucifix` (a cross), `present`, `candyCane`, `snowflake`, `heart`. Solid, shaded from the first color at the center to the last at the edge: `filledCircle`, `filledSquare`, `filledDiamond`, `filledStar`, `filledTree`, `filledCrucifix`, `filledPresent`, `filledHeart`. A star has 4 points; `bars` of 25, 50, or 75 gives 5, 6, or 7.
- `slowFalls`: bars and shapes sink back gradually instead of dropping. Leave it on.
- `logX`: the lowest bar takes in about eighteen notes and each higher bar fewer, so the high notes get most of the width.
- `xOffset`, `yOffset`: move a `levelShape` or the circle spectrogram. Elsewhere leave them at 0: `xOffset` squeezes the bar spectrograms.

## When to use it

- A matrix or tree as the chorus's meter: `spectrogramPeak`, or a `levelShape` heart.
- Arches as a meter (`horizontalPerModel`), or each swept once a beat (`perModelDefault`, `timingEventTimedSweep`).
- The whole house on the big hits: `levelJump100`, which needs no timing, or `timingEventJump100` on a drums track.
- Flash safety: no whole-house `pulse` or `levelPulse` on marks or hits less than a third of a second apart.

## Looks

```look
{"name": "A spectrum analyser on a matrix", "suits": ["matrix", "tree"],
 "note": "Sixteen green-to-red bars, a white marker resting on each one's peak.",
 "effect": {"kind": "vuMeter", "settings": {"meter": "spectrogramPeak", "bars": 16, "sensitivity": 20}, "colors": ["#20c040", "#ffc040", "#ff2020", "#ffffff"]}}
```

```look
{"name": "A group of props as a spectrum meter", "suits": ["house"],
 "note": "On a group of arches each arch is one bar filling along its length. Set bars to the number of props.",
 "effect": {"kind": "vuMeter", "settings": {"meter": "spectrogram", "bars": 4}, "renderStyle": "horizontalPerModel", "colors": ["#20c040", "#ffc040", "#ff2020"]}}
```

```look
{"name": "The house filling up on each hit", "suits": ["house", "matrix", "tree"],
 "note": "Each loud hit fills the display, green at the ground to red at the roof; then it drains.",
 "effect": {"kind": "vuMeter", "settings": {"meter": "levelJump100", "bars": 16, "sensitivity": 60}, "colors": ["#20c040", "#ffc040", "#ff2020"]}}
```

```look
{"name": "A band crossing an outline once a beat", "suits": ["string", "arch"],
 "note": "A short red-to-white band runs the prop's length once every beat.",
 "effect": {"kind": "vuMeter", "settings": {"meter": "timingEventTimedSweep", "timingTrack": "Beats", "bars": 6}, "colors": ["#ff2020", "#ffffff"]}}
```

```look
{"name": "Loudness rolling along an outline", "suits": ["string", "arch"],
 "note": "A deep blue outline; each hit sends a white pulse sliding from one end to the other.",
 "effect": {"kind": "vuMeter", "settings": {"meter": "intensityWave", "bars": 30}, "colors": ["#ffffff", "#0020a0"]}}
```
