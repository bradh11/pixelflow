# Reading a song

Turning what `analyze_song` says (sections, energy, builds, drops, breakdowns, stops, vocals) into a plan for the show.

## What the analysis gives you

- **Sections**: each with a label (Intro, Verse, Pre-Chorus, Chorus, Bridge, Break, Interlude, Outro, or Part), a group letter, start and end, and its energy. The same group letter is the same music. That is the most useful fact in the whole analysis: sections that share a letter should share a look.
- **Energy**: a number from 0 to 1 for each section, and a digit for each bar (0 quiet to 9 full) in three strings: overall energy, bass, and drums. Read the digit strings like a waveform: runs of 2s and 3s are verses, 8s and 9s are choruses, a climb is a build, a sudden fall is a breakdown.
- **Moments**: the song's dramatic points, most important first, each with a kind, an importance from 0 to 1, a suggested treatment, and where it ends if it lasts.
- **Lyrics**, when the sequence has them: the lines, when they are sung, and the names of the words, syllables, and mouth-shape tracks.
- **Confidence**: how sure the analysis is of the tempo, the downbeat, and the sections. Under about 0.5 for sections, trust the energy digits over the labels. If the user has made their own "Sections" track, the analysis says so, and theirs is the truth.

## The kinds of moment, and what each wants

- Impact, crash, drop: everything lands at once. A hit.
- Restart: the band comes back in after a stop or a breakdown. The biggest hit of all when it follows silence.
- Stop: the band cuts out. The lights go dark for exactly as long as the music does.
- Breakdown: the drums drop out while the music goes on. Few props, slow and dim.
- Build: one to eight bars of rising tension. A ramp: brighter, faster, hotter, into the hit at the end.
- Fill: a run of drums into a downbeat. A chase across the props.
- Peak: the loudest, busiest stretch. The full house.
- Hold: a note or chord held with little else. One steady look, then a slow fade.
- Key change: a new palette from there.
- Shout: a word or vocal burst that lands with the band. A pop on that word.
- Section change: something visibly changes.

`stage_cue` knows these treatments (topic `cues`). Your job is to decide which moments get the big version.

## Making the plan

1. Find the loudest section group. That is the full-house look. Usually the chorus; in some songs a final section louder than all of them.
2. Find the quietest. That is the fewest props, the slowest motion, often one color.
3. Rank the rest between them, and give each rank a level: how many props, how fast, how bright. Three or four levels is plenty.
4. Mark the two or three most important moments. Those get the biggest treatment (a blackout before, an intensity of 1). Everything else gets less, on purpose.
5. Look at what comes right before each big moment. A build wants a ramp; a quiet bar wants to go darker still, so the hit has somewhere to come from.
6. Note where the vocals are and where they are not. An instrumental break after two verses and choruses wants to look different from both.

## The shape of most songs

- Intro: establish the palette with very little. One or two props.
- Verse: a calm base, room left over.
- Pre-chorus or build: add a prop group or a layer every two or four bars.
- Chorus: the full look. The second chorus is the first plus one new thing; the last is everything.
- Bridge or breakdown: the odd one out. Change the palette, drop most of the house, slow down.
- Outro: unwind in the reverse order you built, and end on purpose.

A song that doesn't fit this (through-composed, a medley, a slow carol that never gets loud) still has a loudest part, a quietest part, and a few moments. Plan from those.

## When there is no song, or the analysis is thin

Without a song, a sequence is just time. Use bars of the length the user says (or two seconds), keep one calm look, and say that it will come alive once a song is set. With a song but no sections found, cut the looks every 8 or 16 bars where the energy digits change.
