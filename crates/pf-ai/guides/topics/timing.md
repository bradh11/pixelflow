# Timing tracks

Marks in time to line effects up with: the kinds of track, making and editing them, and the three ways an effect uses one.

## What a track is

A timing track is a named list of marks, each with a start, an end, and a label. Marks on a track never overlap. `get_open_sequence` lists the tracks; `get_timing_marks` reads one.

A track's `kind` says what it marks: {{timing-kinds}}.

- `beats`: one mark a beat, labeled 1 to 4 within its bar.
- `bars`: one mark a bar, numbered.
- `sections`: verse, chorus, bridge: the song's parts, labeled.
- `lyrics`: one mark for each sung line, labeled with its words.
- `words`: one mark for each sung word.
- `phonemes`: one mark for each mouth shape, made from the words. A singing face reads these. They are made whole from the words and can't be edited mark by mark.
- `custom`: anything else: accents, moments, drum hits, syllables, marks of your own.

## The song's tracks

`add_song_timing` makes tracks from PixelFlow's own analysis of the song and from the sung words: {{song-tracks}}.

- `beats`, `bars`, `sections`: as above. If the user has made their own "Sections" track, theirs is the one to follow.
- `onsets`: every note or hit that starts. Dense; good for glitter that follows a busy passage.
- `accents`: the hits, drops, breaks, and builds that stand out.
- `moments`: the song's dramatic points, each labeled with what it is ("Drop", "Shout: word").
- `drums`: kicks, snares, and crashes that stand out, labeled by drum.
- `syllables` and `phonemes`: made from the sung words when the sequence has lyrics. Syllables are for motion that follows the singing more closely than words; phonemes are for faces.

Lyrics themselves come from Find lyrics in the sequence editor, which the user starts. If there are none, say so and offer it; don't invent them. If the user gives you the words, you can make the tracks yourself: `sequence_add_timing_track` (kind `lyrics`), `sequence_spread_lyrics` to lay the lines over the time they are sung, then `sequence_break_into_words`, then `add_song_timing` for syllables and phonemes.

## Making and editing tracks

- A new track: `sequence_add_timing_track`, with marks or empty.
- Marks every so often: `sequence_generate_marks` (every half beat, say, across a stretch).
- Every second or fourth mark of another track: `sequence_copy_marks` with `every` (half-time beats from Beats, phrases from Bars).
- One mark at a time: `sequence_add_marks`, `sequence_set_mark` (move, resize, relabel), `sequence_split_mark`, `sequence_merge_marks`, `sequence_remove_marks`, `sequence_label_marks`.
- The whole track: `sequence_update_timing_track`, `sequence_rename_timing_track`, `sequence_move_timing_track`, `sequence_remove_timing_track`.

Make a track of your own when the song has a pattern the analysis doesn't name: a riff that lands on the same three beats of each bar, the hand claps in a chorus. Mark it once, then place effects on it everywhere it happens.

## Three ways to use a track

1. Cut effects at its marks. `place_effects` with `track` puts one effect on each mark between `fromMs` and `toMs`. `marksEach` joins marks (4 on Beats is an effect a bar; 2 on Bars is a two-bar phrase). `match` keeps only the marks whose label has a word or phrase, which is how an effect lands on a sung word. `spread` shares the marks out among the rows:
   - `together`: every row, every mark.
   - `alternate`: neighbouring rows take turns, mark by mark.
   - `sweep`: one row per mark, in the order you gave the rows: a chase across props.
   - `build`: the first row from the first mark, the second joining at the second, and so on until all are in.
2. Hand it to an effect that steps on marks. Effects with a `timingTrack` setting move on its marks instead of at their own speed: a `chase` that steps a prop per beat, a `pulse` on bars, `fireworks` that burst on drum hits, a `vuMeter` bar that jumps on each mark, `faces` and `sing` following words or phonemes.
3. Drive a setting with it. A curve of shape `timingFade`, `timingFadeSpan`, or `timingToggle` makes any number setting jump or switch on the marks (topic `curves`).

## Which track for what

- Changes of look: Sections. Nothing says "new part of the song" like every prop changing on the first beat of a chorus.
- The pulse of a base look: Bars for slow songs and quiet sections, Beats for loud ones. Half beats only for a bar or two at a time.
- Hits and stops: Moments and Accents, with `stage_cue` (topic `cues`).
- Singing: the words track for word pops and text, the phonemes track for faces, syllables for a prop that bounces with the melody.
- Fills and runs: Drums.

## Good habits

- Before the user sees a proposal, PixelFlow moves new effect edges that are within a beat of a section start, moment, accent, sung word, bar, or beat onto it. Aim close and let it finish the job; don't fight it with effects a few milliseconds off the grid on purpose.
- An effect shorter than about 100 ms is a flicker, not an effect. On a fast track, join marks with `marksEach`.
- Don't cut everything on Beats. A show where every prop changes every beat for three minutes has no shape. Let the base look run in bars and phrases and keep beat-by-beat cutting for one layer or one group.
