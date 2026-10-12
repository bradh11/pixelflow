# Vocals

Making the display sing: faces, any prop following the voice, words on a matrix, and pops on the words that matter.

## What you need

`analyze_song` lists the sequence's lyrics when it has them: the lines and when each is sung, the sung stretches, and the names of the tracks: "Lyrics" (lines), "Lyrics (words)", "Lyrics (syllables)", "Lyrics (phonemes)". `add_song_timing` makes the syllables and phonemes tracks from the words if they are missing.

No lyrics in the sequence? Say so, and offer Find lyrics (beside Detect beats in the sequence editor). The user starts it; you can't. Never make up a song's words. If the user types the words for you, topic `timing` says how to turn them into tracks.

## Four ways to show a voice

1. **A singing face.** A prop with a face (`list_props` says which have one) and a `faces` effect on the phonemes track: the mouth makes the shapes of the words and the eyes blink. Nothing else looks as alive. Put it on for every sung stretch, and give the face a resting look between them rather than leaving it dark. The effect's guide has the settings.
2. **Any prop singing.** The `sing` effect makes a prop with no face follow the voice: its brightness opening with each syllable, a bar widening like a mouth, a flash on each word, a fill across each word. An arch or a tree becomes the lead singer. Use it when there is no face, or to have the house answer the face.
3. **Words on a matrix.** A `text` effect for the hook: the two or three words the audience would sing back. Not every line: a matrix can show a short word still, or scroll a phrase, and reading takes longer than singing. The `text` guide says how many letters fit and how fast they can go. Words need a matrix; on a string or an arch they can't be read.
4. **Word pops.** A flash or burst exactly on a chosen word, across a few props or the whole house: the `word_pop` cue of `stage_cue` with `match` set to the word, or `place_effects` on the words track with `match`. For the title of the song every time it is sung, and for shouts.

`stage_cue` with the `sing` cue does the first two for you: faces on props that have them, `sing` on the rest of the props you name, over each sung stretch.

## Who sings

- One lead: the face if there is one, else the most central prop that isn't doing something else (a tree, a matrix, a pair of arches).
- The rest of the house keeps its section look, a little quieter under the voice, and comes forward in the gaps between lines.
- Backing vocals and echoes: a second prop group answering on a different color.
- Call and response in the lyrics is call and response in the lights: two groups, taking turns line by line (`place_effects` on the "Lyrics" track with a `spread` of `alternate`).

## Timing to the voice

- Lines ("Lyrics") are for looks that last a phrase: a color per line, a prop per line.
- Words ("Lyrics (words)") are for pops and for text.
- Syllables ("Lyrics (syllables)") are for motion that bounces with the melody: a chase that steps a syllable at a time, a `sing` in its brightness mode.
- Phonemes ("Lyrics (phonemes)") are for mouths. Nothing else needs them.
- Sung words are rarely on the beat. Effects placed on a lyrics track stay on the words; don't drag them to the grid.

## Instrumental stretches

When the voice stops, the look should change: the lead prop rests or does something else, and the rhythm props take over. A verse, a chorus, and a guitar break that all look alike is a common miss. The sung stretches `analyze_song` lists tell you where the gaps are.

## Mistakes to avoid

- A face that sings while its prop is also covered by a busy group effect: give the face's row its own layer and let nothing else play on that prop while it sings.
- Scrolling every lyric across the matrix. It turns the show into subtitles.
- Text too long for the time it has, or too small for the matrix: `review_draft` checks whether words can be read.
- Word pops on common words. Pop the words that carry the song.
