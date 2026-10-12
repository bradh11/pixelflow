# Layering and restraint

Building a look from a base, movement, accents, and highlights; and the discipline of leaving things dark.

## Four jobs, in order

A finished look is at most four things stacked, each doing one job.

1. **The base**: the section's color, held. An `on`, a slow `colorWash`, a gentle `twinkle`. It sits on the bottom layer of a group's row and says "this is the verse". It can be dim; often it should be.
2. **Movement**: the one thing that travels with the music. A `chase` on the arches, `bars` along the roofline, a `spiral` on the tree. It goes on a layer above the base or on the props' own rows.
3. **Accents**: short things on beats and words. A `pulse`, a flash on a snare, a pop on a sung word. They come and go; most of the time the accent layer is empty.
4. **Highlights**: the big moments. Hits, blackouts, sweeps. `stage_cue` puts these on a new top layer so they sit over everything else.

Most sections need only the first two. A quiet verse may be base alone. Adding a third and fourth is how a chorus becomes a chorus.

## Where each goes

- Base: the group's row, layer 0, cut on "Sections".
- Movement: the props' own rows (below the group rows in the list, so they cover the base where they play), or layer 1 of the group's row with a blend that lets the base show (topic `blends`).
- Accents and highlights: the top layers. Let `stage_cue` find the layer.

If two effects fight on the same pixels, one of them is in the wrong place. Give the second a different prop, or take it out.

## Dark is a color

- A prop that is off makes the lit ones matter. A whole house at full has no way to get bigger.
- In a quiet section, light a third of the display. In a middling one, half to two thirds. Save all of it for the loud sections, and all of it at full brightness for the two or three moments that earn it.
- Half a beat of dark before a hit is worth more than twice the brightness after it.
- A stop in the music is a stop in the lights. Go black for exactly as long as the band does.
- Between the dark and the full house there is dim. A base at 20 to 40 percent brightness keeps the shape of the house visible without spending anything.

## How much is too much

- More than two things moving on one prop at once is noise.
- More than three or four kinds of effect across the house at once is soup. Pick two or three for a section and let several props share each.
- Whatever you add for the chorus, take something away for the next verse. If the second verse looks as busy as the first chorus, the second chorus has nowhere to go.

## A worked section

A chorus on a house with a roofline, windows, four arches, a tree, and a matrix:

- Base: the whole-house group in the section's main color at 30 percent.
- Movement: the tree in a two-color `spiral`; the arches chasing left to right, one arch a beat.
- Structure: the roofline in slow two-color `bars`, the windows holding the second color.
- Picture: the matrix showing the hook word, or a meter.
- Accents: a white `pulse` on the windows on beats 2 and 4.
- Highlights: a hit on the first beat, staged with `stage_cue`.

The verse after it: the base, the roofline, and the tree slowed down. Everything else dark.
