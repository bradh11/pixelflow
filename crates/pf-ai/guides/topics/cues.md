# Cues

`stage_cue`: saying what a moment should do (a hit, a blackout, a build) and letting PixelFlow turn it into finished effects on the right props.

## What a cue is

A cue is a director's instruction at a time: "a hit here", "dark through the stop", "build into the drop". `stage_cue` expands each one, the same way every time, into ordinary effects on a new top layer of the rows that light its props, chosen by what each prop is (outline, tree, matrix, arch, window, round prop, talking prop) and where it sits. The section looks you laid stay underneath. The user can edit everything a cue made.

Use cues for the moments. Use `place_effects` and the effects' own guides for the looks that run through a section.

## Every cue

{{cues}}

## Saying when and where

- `at` is the time: milliseconds, a moment by its number in `analyze_song`'s list ("m3"), or a mark ("Sections:4" is the fourth mark of the Sections track). A moment or mark that lasts gives the cue its end as well.
- `until` ends a cue that lasts (`blackout`, `ramp`, `chase`, `minimal`, `full`, `sustain`, `breathe`, `sing`, `dance`, `call_response`). It takes the same forms as `at`.
- `targets` names the props: groups or props by name, or the words `all`, `left`, `right`, `top`, `bottom`, `outlines`, `trees`, `matrices`, `arches`, `windows`, `rounds`, `faces`. Leave it out for the whole show.
- `with` is the other side of a `call_response`.
- `intensity` is how big, 0 to 1. Left out, a moment's own importance is used. From 0.85 a cue gets the biggest treatment: a `hit` adds strobe on the outlines and lightning on the matrices, a `word_pop` pops the whole house and sets the faces singing. Keep that for the two or three moments that earn it.
- `colors` overrides the cue's own colors. Give the section's palette so a hit belongs to its section.
- `direction` is which way a `sweep` or `chase` travels: `leftToRight`, `rightToLeft`, `up`, `down`, `centerOut`, `edgesIn`.
- `track` is the timing track a `chase`, `call_response`, or `breathe` steps on.
- `match` is the sung word a `word_pop` lands on, or the character for a `dance`.
- `hit` (true or false) says whether a `ramp` ends, or a `blackout` returns, on a hit. Left out, the music decides.

`stageMoments` stages every moment the song has from `minImportance` up (0.5 unless you say), each with the treatment the analysis suggests. It is a good first pass: stage them all, look at the review, then add or replace cues by hand where the song deserves better.

## Which cue for which moment

- Impact, drop, crash, restart: `hit`. Give the biggest one of the song an intensity of 1 and the rest 0.6 to 0.8.
- Stop (the band cuts out): `blackout` to the restart. Dark is the effect; the hit that follows is what people remember.
- Build: `ramp` over its length, ending on the hit at the top.
- Drum fill: `chase` across the props, on the Drums track when there is one.
- Breakdown: `minimal`. A few props breathing, the rest dark.
- Peak, final chorus: `full`, opened with a hit.
- Held last chord: `sustain`. Hold, then fade.
- Key change, or a section that needs to look new without new effects: `color_shift`.
- A hook word: `word_pop` with `match`.
- Vocals with talking props: `sing` over the sung stretches.
- A quiet stretch with nothing lit: `breathe`.
- A change of section with nothing else to mark it: `sweep`.
- Two voices or instruments answering each other: `call_response`.
- A playful instrumental break and a matrix to fill: `dance`.

## Habits

- Lay the section looks first, then cue the moments. A cue staged on an empty sequence has nothing to contrast with.
- A hit lands harder out of dark. The `hit` cue dips for half a beat by itself when that time is free; a `blackout` for a beat or a bar before the biggest hit makes it bigger still.
- Don't stack cues on one instant. Two hits within half a beat are skipped for you; a `full` over a `minimal` is a contradiction.
- If a cue says it staged nothing, its props have no rows in the sequence or the time is taken: check `get_open_sequence`.
