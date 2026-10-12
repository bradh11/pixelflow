# How to build a sequence

The order a seasoned show designer works in, with the tool for each step and the guide to read before it.

## Before anything: how big is the job?

- A small request ("make the arches chase", "add a skeleton to the chorus", "make the tree red in the verses") is one job, done small: read that effect's guide, look at what is there (`get_open_sequence`, `list_sequence_effects` for the stretch), place or change it, `review_draft`, `propose_changes`. Leave the rest of the sequence alone.
- "Make a sequence for this song" is the whole method below.
- "Make it better" starts at step 9: review, then fix what the review finds.
- A request about how the parts feel ("calmer verses, bigger choruses", "it all looks the same") is a job about levels: every section's level follows its own music, the parts nobody named too (topic `intensity`).

## 1. Understand the song

`analyze_song`, then `add_song_timing`. Read the sections, each bar's energy, and the moments before you think about a single effect. Topic `song` says how to read them. Write yourself a one-line plan per section: what it is, how loud, what happens in it.

## 2. Understand the display

`get_open_sequence` for the rows and `list_props` for what each prop is (its role, its grid, where it sits, whether it has a face). `list_groups` for what can move together. Know which props are the big canvases (matrices, trees), which are structure (rooflines, outlines, windows), and which are rhythm (arches, canes, stakes). Topic `roles`.

## 3. Plan by section

Decide, for each group of sections (every verse, every chorus):

- its palette, two or three colors (topic `palettes`);
- which props play and which rest (topic `layering`);
- what each playing prop does, by its role (topic `roles`);
- which way things move (topic `motion`);
- how it differs from the section before (topic `intensity`).

Plan the loudest section first, with everything you have, then work down: each quieter section is that look with something taken away. It is far easier than building up from the intro and running out of ideas at the last chorus.

## 4. Lay the base

One look per section on the bottom layer, with `place_effects` on the group rows and prop rows, cut on the "Sections" track so each look starts where its section does. Take the looks from the effects' guides (`get_guide` with `effects`): each has ready-made ones written for each kind of prop. A section that comes back gets the same look again with `repeat_effects`, then a variation with `change_effects` (new colors, a faster speed).

## 5. Add movement

On a layer above the base, the thing that moves with the music: chases and bars cut on "Bars" or "Beats", a spiral on the tree, a pulse on the beat. One moving idea per prop group is enough. Blends decide how it sits on the base (topic `blends`).

## 6. Stage the moments

`stage_cue` with `stageMoments` for a first pass over every moment the song has, then cues by hand for the ones that matter most: a `blackout` before the biggest hit, a `ramp` through the build, `minimal` through the breakdown. Topic `cues`.

## 7. Handle the vocals

If the song has lyrics: faces sing, a matrix shows the hook, key words pop. If it has none in the sequence, say so and offer Find lyrics. Topic `vocals`.

## 8. Open and close

The first bars and the last chord are what people remember. Topic `openings-endings`.

## 9. Review

`review_draft` scores the draft against the song and as a design, and lists fixes, most with the tool calls that make them. Make them, review once more, then `propose_changes`. Two reviews at most: a third pass is rarely worth the user's wait.

## Things to hold on to the whole way

- Every prop gets a part, and every prop gets a rest.
- Something changes at every new section.
- The whole house at full is for the few moments that earn it.
- Nothing flashes faster than three times a second over a large part of the display (topic `safety`).
- When in doubt, fewer effects, held longer. Topic `mistakes` has the usual ways a sequence goes wrong.
- Changing what is already there: topic `editing`.

## Where the rest is

- What every effect has besides its own settings (colors, fades, sparkles, blur): topic `effect-wide`.
- Settings that change while an effect plays, or follow the music or a track: topic `curves`.
- What a row can light, and how a group or a tree lays its pixels out: topic `layouts`.
- Timing tracks, and cutting effects on them: topic `timing`.
- Lights that follow the sound by themselves: topic `music-reactive`.
- An effect's own guide (`get_guide` with `effects`) before using it, every time: what it draws on a string is rarely what it draws on a matrix.
