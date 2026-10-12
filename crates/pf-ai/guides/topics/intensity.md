# Intensity and change

Matching how much the lights do to how much the music does, and making sure something changes at every new section.

## The levers

Intensity is not just brightness. From gentlest to strongest, the things that raise it:

1. How many props are lit.
2. How bright they are.
3. How fast things move.
4. How many colors, and how much white.
5. How many layers are playing.
6. How often things change: every four bars, every bar, every beat.
7. Flashes and hits.

Use the cheap ones first. A verse to pre-chorus step is "the arches join". A pre-chorus to chorus step is "everything lights, the tree speeds up, white comes in". If you reach for flashes in verse one, there is nothing left for the end.

## A ladder for a song

Give each section a level from the music's energy (`analyze_song` gives each section's, and a digit for every bar), and keep to it:

- **Level 1, quiet (energy under about 0.3; bar digits 0 to 2):** a third of the props or fewer, dim, slow or still, one color and maybe a second. Changes every four or eight bars.
- **Level 2, moderate (0.3 to 0.6; digits 3 to 5):** half to two thirds of the props, a base and one moving thing, two colors. Changes every two or four bars.
- **Level 3, loud (0.6 to 0.85; digits 6 to 7):** every prop group has a part, two or three colors, movement on the beat. Changes every bar or two.
- **Level 4, the top (over 0.85; digits 8 to 9):** the full house, white in the palette, the fastest movement, accents on beats. Changes every bar, hits on the moments.

Most songs use three of the four. A slow carol may live between 1 and 2 and get its "chorus" from a second color and the tree joining; that is right for it.

Inside a section, follow the bar digits: if bars 5 to 8 of a verse are louder than 1 to 4, add one prop group at bar 5.

## Something changes at every section

When the music moves to a new section, the lights must visibly change on its first beat. Any one of these counts; a big boundary deserves two or three:

- a new palette;
- different props playing (some join, some rest);
- a different effect on the leading prop;
- a change of direction or speed;
- a wipe or a hit on the boundary itself.

The same music coming back (verse 2 after chorus 1) returns to its own look, so the change is built in. Then vary it slightly: `repeat_effects` to bring the look back, `change_effects` for a new speed or one new color, or a different prop leading.

`review_draft` checks both things: that sections with the same music look alike, and that each new section looks different from the one before it.

## Builds and releases

- A build is a climb between levels over one to eight bars. One long effect with a `ramp` curve on speed or brightness, props joining one by one, or the `ramp` cue of `stage_cue`.
- The bar before a drop is often better emptied than filled: thin out, go dark for the last half beat, then land.
- After a peak, release: don't hold the top level for more than eight bars or so without a dip, or it stops feeling like the top.
- A breakdown is level 1 arriving suddenly. The drop in intensity is the effect; don't soften it with a slow fade.

## Checking yourself

Read your plan as a list of levels, section by section: 1, 2, 3, 2, 3, 1, 4, 1 is a show. 3, 3, 4, 4, 4, 3, 4, 4 is a wall. If two neighbouring sections have the same level, they need a different palette or different leading props to tell them apart.
