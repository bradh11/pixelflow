# Following the music

The three ways lights can follow sound (the marks, the level, the analysis) and every option that reacts to the audio by itself.

## Three ways to follow a song

1. **On the marks.** Effects cut on timing tracks, or stepping on them: beats, bars, sections, drum hits, sung words. Exact and repeatable; this is how most of a sequence is timed (topic `timing`).
2. **With the level.** Effects and curves that read the music's loudness (and its bass, its notes, its onsets) frame by frame as it plays. Nothing to place by hand: the lights move because the sound does. Good for texture and for passages too busy to mark.
3. **By the analysis.** `analyze_song` tells you where the builds, drops, and breakdowns are, and you design for them. This is the plan; the other two are how it is carried out.

A strong sequence uses all three: planned by the analysis, cut on the marks, with one or two things riding the level for life.

## Options that follow the level

All of these need the sequence to have a song. Without one, they sit still or at a middle value.

- **`vuMeter`**: the dedicated effect. Spectrograms, volume bars, level-driven color and pulses, and meter types that jump on a timing track's marks. Its guide covers the many types. A spectrogram on a matrix, or a group of arches laid out as bars, is the classic "the house is an equalizer" look.
- **`pulse`** with its `source` set to the music's level, its bass, or its onsets: a prop breathing with the song. On the bass it follows the kick drum without any marks.
- **`musicSparkles`** on any effect: the number of sparkles follows the loudness, up to `sparkles` (topic `effect-wide`).
- **Music curves** on any number setting of any effect: `music`, `invertedMusic`, and `musicTrigger` (topic `curves`). A curve on `brightness` makes anything pump; a curve on a size or speed makes it swell.
- **`fire`** with `growWithMusic`: the flames rise with the level.
- **`shape`** and **`fireworks`** with `fireOnMusic`: a shape or a burst whenever the level passes `triggerLevel`.
- **`tendril`**: movements that follow the music.
- **`dancer`** with `bassBounce`: the character bounces on the bass as well as stepping on the beat.

Each effect's own guide says what its music settings look like on real props.

## Which channel is the level

The sequence says which channel of a stereo song is how loud it is (`musicLevelSource` in `get_open_sequence`), set with `sequence_set_music_level_source`:

- `louderChannel`: whichever side is louder at each moment, so a sound on either side is heard. A sequence made in PixelFlow starts with it; keep it.
- `leftChannel`: the left side alone, as xLights reads it. A sequence imported from xLights has it, so its sparkles, curves, and fires move at the moments they did there. Leave it unless the user asks: changing it changes how every level-driven effect in the sequence plays.

It changes the level only: the bass, notes, onsets, beats, and `pulse` read the whole mix either way. A song with one channel reads the same either way.

## Level or marks?

- The level follows loudness, not rhythm. In a dense, loud chorus the level is high all the time and level-driven effects sit near full: they stop reading as "in time". Marks are better there.
- In a sparse passage (a solo voice, a piano, a single drum) the level follows every note, and level-driven effects look magical. Use them there.
- A kick-drum pulse: `pulse` on the bass if the mix is clean; on the "Beats" track if the bass is muddy or constant.
- Hits and accents: always marks (the "Moments", "Accents", and "Drums" tracks, or `stage_cue`). The level is too late and too soft for a hit.
- When both are available, put the structure on marks and one decorative layer on the level.

## Getting level-driven effects to read

- Quiet songs under-drive them. Curves have a `gain`, `vuMeter` has `gain` and `sensitivity`, and a trigger level can come down: raise the drive until the effect uses its whole range in the loud parts.
- Give them contrast: a level effect over a dark or dim base shows its movement; over a bright base it disappears.
- One level-driven thing per section is usually enough. Several, all reading the same loudness, all move together and add nothing.
- A flash driven by the level can go off many times a second in a busy passage: topic `safety`. Prefer a pulse with a slow release, or keep it to a small prop.

## Vocals

The sung words are followed on their own tracks, not by the level: topic `vocals`.
