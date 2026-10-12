# Palettes

Choosing few colors, holding them for a section, changing them on purpose; with palettes for Christmas and Halloween.

## The rules that matter

- **Two or three colors a section.** One main color, one partner, and sometimes white or a single accent. A fourth color is where a look starts to read as "all the colors".
- **Hold the palette for the whole section**, on every prop. The roofline, the arches, and the tree all drawing from the same three colors is what makes a display look designed.
- **Change it at a section boundary, not in the middle.** A new palette is one of the strongest ways to say "new part of the song".
- **The same music gets the same palette.** Every verse in one, every chorus in another. The last chorus may add white or gold to the chorus palette; it shouldn't become a third thing.
- **A song wants two or three palettes in all**, related to each other: they share a color, or sit next to each other on the wheel, with one contrasting palette for the bridge.

## Picking colors that work on pixels

- Saturated colors read best from the street. Pure red, green, blue, and the colors between two of them (yellow, cyan, magenta, orange, purple) are strong; pastels wash out against streetlights.
- Full white is the brightest thing a pixel can do: three times the light of pure red. Use it as an accent or at reduced `brightness`, or it will swallow the color beside it.
- Warm white (#ffc880) is kinder than pure white for held looks; pure white (#ffffff) is for sparkle and hits.
- Blue reads dimmer than it measures; give blue sections more props or pair blue with white.
- Dark colors (a deep blue, a dim red) make good bases: the shape of the house without the glare.
- Black in an effect's color list is a gap, not a color. Red, black gives red with dark between.

## Contrast between sections

- Warm against cool is the simplest strong change: red and gold verses, blue and white choruses, or the other way round.
- Brightness is a change too: the same colors with white added for the chorus.
- For a key change or a last chorus, shift the same palette a step round the wheel (`hueAdjust`, or new colors with `change_effects`) rather than starting over.

## Christmas

- Classic: red #ff0000, green #00b030, white #ffffff. Red and white for candy stripes; red and green for everything else; white for sparkle.
- Warm and traditional: warm white #ffc880, gold #ffb000, deep red #c00000.
- Winter: ice blue #60c8ff, deep blue #1030c0, white #ffffff. Snow, frost, quiet carols.
- Candy: red #ff0000, white #ffffff, pink #ff5090.
- Rich: purple #8000c0, gold #ffb000, teal #00a0a0.
- Don't put red and green at full on the same thin prop in fine stripes: from a distance they blur to a muddy yellow. Use wide stripes, or put white between them.

## Halloween

- Classic: orange #ff6000, purple #8000ff, green #40ff00.
- Spooky: purple #6000c0, sickly green #80ff00, with black for long dark gaps.
- Fire and blood: red #ff0000, orange #ff5000, yellow #ffc000.
- Ghostly: cold white #c0d0ff, pale blue #4060ff, a little purple.
- Halloween wants more dark than Christmas does. Let things lurk: long dim stretches, sudden flashes.

## Putting it in the sequence

- Give the palette in each effect's `colors`. Order matters: the first color is the one single-color effects use, and striped effects hand them out in order.
- To recolor a whole section in one call: `change_effects` with the section's time range and `change` set to the new `colors`.
- A cue's `colors` makes a hit or a sweep belong to its section instead of using the cue's own.
- `review_draft` counts the colors each section uses and says when one has too many.
