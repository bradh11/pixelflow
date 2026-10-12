# Curves: settings that change

Making a number setting change while its effect plays: ramps and waves, following the music, or stepping on a timing track's marks.

## What a curve is

An effect's `curves` is a list by setting key. Each curve takes the place of that setting's value for as long as the effect plays, going between two values, `from` and `to`, in a shape.

A curve can drive any number setting of the effect's kind (each effect's guide lists them under "Curves can drive"), and on any effect `sparkles`, `blur`, `brightness`, `hueAdjust`, `saturationAdjust`, and `valueAdjust`. `from` and `to` must be inside the setting's own range.

```
"curves": { "speed": { "shape": "ramp", "from": 0.5, "to": 4 } }
```

## A curve's fields

{{curve-fields}}

## Every shape

{{curve-shapes}}

## Shapes over the effect's own time

- `ramp` is the build: `speed` from slow to fast into a chorus, `brightness` from 0.2 to 1 over a rise, `sparkles` from 0 to 150 into a peak. Swap `from` and `to` to wind a brightness or a size down; a speed needs more care (below).
- `sine` breathes. `cycles` is how many breaths the effect holds, so the same curve is slower on a longer effect: for one breath a bar, give an 8-bar effect 8 cycles.
- `square` switches between two values: an effect that is big for half of each cycle and small for the other half.
- `saw` climbs and snaps back, `cycles` times: a setting that keeps winding up and starting over.
- `custom` goes through `points`, each a time and a level from 0 to 1 (level 0 is `from`, 1 is `to`). Two points at the same time make a step. Use it when the change has a shape of its own: hold, then jump at the three-quarter mark.

## Shapes that follow the music

They need the sequence to have a song. Without one they sit halfway between `from` and `to`.

- `music` reads the loudness of each frame: `from` in silence, `to` at the loudest. `gain` boosts a quiet song's level. On `brightness` it is the simplest way to make any effect pump with the track; on a size setting it makes the effect swell.
- `invertedMusic` is the other way up: big when the music is quiet.
- `musicTrigger` snaps to `to` whenever the level passes `trigger` and falls back to `from` over `fade` frames: a kick-drum flash. Start with a trigger of 60 to 75 and a fade of 6 to 10 frames (a frame is 25 ms unless the sequence says otherwise).
- These follow loudness, not beats. For something that lands exactly on the beat, use a timing shape on the Beats track.

## Shapes that follow a timing track

Set `timingTrack` to the track (its id; in `place_effects` and `change_effects` its name will do).

- `timingFade` jumps to `to` at the start of every mark and fades to `from` over `fade` frames: a pulse on every beat, bar, or sung word, as tight or as loose as the fade.
- `timingFadeSpan` does the same with `fade` as a share of the gap to the next mark, so the pulse stretches with a slow passage and tightens with a fast one. 50 to 80 reads as a throb; 100 never quite rests.
- `timingToggle` flips between `from` and `to` at each mark's start and end: on during each word and off between them, or, on marks that sit back to back like beats, on for one and off for the next.

## Good uses

- A build: one long effect with `ramp` on its speed and its `brightness`, instead of four effects each a bit faster.
- A pulse to the beat on any look: `timingFade` on `brightness` from 0.3 to 1 on the Beats track, fade 8.
- Glitter that swells with the song: `music` on `sparkles` from 0 to 160.
- A slow rainbow: `ramp` on `hueAdjust` from 0 to 1 across a section.
- Wind-down ending: `ramp` on `speed` from its value down to half of it, with a long fade out: the motion comes to rest exactly as the effect ends.

## A curve on a speed is not a throttle

An effect works out where its pattern is from the time played times the speed right now. It does not add up the speeds it has had. So a curve on a `speed` or a `rate` behaves in ways worth knowing:

- A rising `ramp` builds smoothly, but ends moving at about twice its `to` minus its `from`: from 0.5 to 2 it finishes at about 3.5. Set `to` to a little over half the speed you want at the top.
- A falling `ramp` to half the starting speed comes smoothly to a stop at the end of the effect. Below half, the pattern stalls partway and runs backward: a ramp to nothing ends where it began.
- `sine`, `square`, `saw`, the music shapes, and the timing shapes on a speed make the whole pattern lurch back and forth rather than speed up and slow down. Put those on `brightness` or a size instead, and change speed between effects.

## Things to avoid

- A curve on a count setting (stripes, arms, blades) redraws the pattern at each whole number, which jumps. Curve the speed, size, or brightness instead.
- Curves on several settings of one effect at once are hard to read and harder to fix. One curve usually carries the idea.
- Fast `square` on `brightness` is a strobe: topic `safety`.
