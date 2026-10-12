# Sing

Makes any prop sing along with a lyrics track, no face needed: the whole prop brightens as the mouth opens, flashes on each word, widens like a mouth, or fills across each word.

## What it draws

One color over the whole prop, driven by the marks of a timing track. It needs that track: with no `timingTrack` it draws nothing, whatever else is set. Where it is dim it is see-through to the same degree, so a base on the layer below shows through a half-open mouth and fully between words.

How open the mouth is comes from the sound being sung: wide for AH and OH, about three quarters for EE, half for OO, L and W, less for most consonants, and shut for M, B, P and between marks.

- On a string or an arch: `mouth` and `wordPop` light the whole run as one. `barMouth` lights a stretch in the middle of the run that grows toward both ends as the mouth opens, so an arch opens from its crown. `karaoke` fills from the first pixel to the last.
- On a matrix: `barMouth` is a band across the middle rows, 9 rows of 20 on a half-open sound and the whole panel on a wide vowel: a mouth without a face. `karaoke` fills left to right. The other two light the whole panel at once, which is a lot of light: hold `brightness` back.
- On a tree: `barMouth` is a ring round the middle that spreads up and down. `karaoke` fills round the trunk, so it starts out of sight at the back and crosses the front partway through the word. The other two light the whole tree.
- On the whole house (a group) it is one picture over the layout. `mouth` turned every prop full on in each word and off between them: the whole house blinking. `barMouth` lit the props at mid height (the matrix, the windows, the middle of the tree) and left the roofline and the arches dark until the mouth was wide. `karaoke` fills from the left end of the yard to the right, prop after prop.
- It has no speed of its own: quick lyrics flicker, a held note holds.

## Settings

- `mode`: how the singing shows.
  - `mouth`: the whole prop as bright as the mouth is open. The plainest, and the best on outlines and arches.
  - `wordPop`: full brightness at the start of each mark, fading fast (a fifth by half way through it), with the next palette color on each mark. Between marks it goes back to the first color at `min`.
  - `barMouth`: a band in the middle as wide as the mouth is open. Outside the band the prop sits at `min`.
  - `karaoke`: over each mark the first palette color fills across the prop; the part not yet reached shows the second color at `min` (the same color, dimmer, when there is only one). Between marks it is dark, whatever `min` is.
- `timingTrack`: the marks it sings. Match it to the mode.
  - "Lyrics (phonemes)": each mouth shape for as long as it is really sung, so vowels are held: `mouth` was at full brightness at a moment when the words track had already dropped to under half. The truest for `mouth` and `barMouth`; in `barMouth` expect the whole prop lit on wide vowels. Too quick for `wordPop` or `karaoke`.
  - "Lyrics (syllables)": one open-and-close for each syllable, its sounds sharing the time evenly, so it spends as long on dim consonants as on the vowel. A softer bounce for `mouth`; `barMouth` shows more of the half-open band.
  - "Lyrics (words)": the same for each whole word (a one-syllable word looks just like its syllable). The track for `wordPop`, and for a `karaoke` that sweeps once a word.
  - "Lyrics" (whole lines): a `karaoke` fill that crosses the prop once over each sung line, the classic use. In `mouth` the prop stays lit all through the line with no gaps between words; `wordPop` gives one flash a line.
  - "Beats" or "Bars": `wordPop` becomes a flash and a new color on every beat, and `karaoke` a fill across every beat or bar. `mouth` just sits at four fifths brightness: don't.
- `min`: the brightness with the mouth shut. At 0 the prop goes clear between sounds: punchy, and a base layer shows. 0.15 to 0.3 keeps a glow so the prop never vanishes. At 0.8 there is almost no movement left. In `karaoke` it is how bright the part still to sing is: 0.2 to 0.4 shows what is coming.
- Colors: one for `mouth` and `barMouth`; two for `karaoke` (sung, then still to sing); two to four for `wordPop` to step through.

## When to use it

- A vocal line on props that have no face: the lead on a pair of arches or the tree, the outline joining in on the chorus.
- Sing or `faces`? `faces` works only on a prop that has a face and shows real mouth shapes on it. Sing works on anything and shows the rhythm of the words as light. A display with a singing face uses both: `faces` on the face, Sing on a few props round it. `stage_cue` with the `sing` cue places both.
- On a group of arches, `renderStyle` `perModelDefault` makes each arch open from its own crown or fill along its own length; without it the group is one picture and a fill crosses the arches one after another.
- Lay it over a dim base (an `on` at low `brightness`) with `min` at 0, or use `min` on its own; not both.
- Take it off in instrumental stretches: with no marks it is dark or sitting at `min`.
- Flash safety: `mouth` or `wordPop` on a whole house is the house blinking on every word, and quick lyrics pass three words a second. Keep those two to a few props, or raise `min` to 0.4 or more. `karaoke` is a fill, not a flash, and is safe on the whole house.

## Looks

```look
{"name": "An outline that sings", "suits": ["string", "arch", "tree"],
 "note": "Warm white, full on every open vowel and down to a faint glow on shut lips and between words.",
 "effect": {"kind": "sing", "settings": {"mode": "mouth", "timingTrack": "Lyrics (phonemes)", "min": 0.15}, "colors": ["#ffd9a0"]}}
```

```look
{"name": "A new color on every word", "suits": ["string", "arch", "matrix", "tree"],
 "note": "A flash at the start of each sung word that fades through it: red, then white, then green, round again.",
 "effect": {"kind": "sing", "settings": {"mode": "wordPop", "timingTrack": "Lyrics (words)", "min": 0.1}, "colors": ["#ff2020", "#ffffff", "#20c040"]}}
```

```look
{"name": "A mouth of light", "suits": ["arch", "string", "matrix", "tree"],
 "note": "A red band in the middle that widens as the mouth opens and shuts between syllables, like a mouth with no face.",
 "effect": {"kind": "sing", "settings": {"mode": "barMouth", "timingTrack": "Lyrics (syllables)"}, "colors": ["#ff3030"]}}
```

```look
{"name": "Karaoke fill across each line", "suits": ["string", "arch", "matrix", "house"],
 "note": "Gold filling from left to right over each sung line, on a dim blue that shows how far there is to go.",
 "effect": {"kind": "sing", "settings": {"mode": "karaoke", "timingTrack": "Lyrics", "min": 0.3}, "colors": ["#ffc040", "#2a6bff"]}}
```

```look
{"name": "A color change on every beat", "suits": ["string", "arch", "tree", "matrix"],
 "note": "A soft flash on each beat that fades before the next, stepping through icy blue, white, and deep blue; it never goes fully dark.",
 "effect": {"kind": "sing", "settings": {"mode": "wordPop", "timingTrack": "Beats", "min": 0.25}, "colors": ["#7fd0ff", "#ffffff", "#2a6bff"]}}
```
