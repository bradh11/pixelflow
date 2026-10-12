# Text

Words, numbers, or a countdown in a pixel font, held still or scrolled: for a matrix, and little else.

## What it draws

The words in a blocky font, placed in the middle of the prop's grid, one line under another. Only the letters are lit; everything round them is clear, so a layer below shows as the background.

- On a matrix it reads. At `size` 8 a letter is 5 columns wide and 7 rows tall, and takes 6 columns with its gap. Each letter takes three quarters of `size` in columns, so a 40-column matrix holds 6 letters at 8, 4 at 12, 3 at 16, and 2 at 20 to 24. A line takes `size` rows: a 20-row matrix holds two lines at 8 or 10, one line above that. Anything wider or taller than the matrix is cut off at the edges, not shrunk.
- On a tree the grid goes round the trunk, so a word laid across is wrapped round the cone and only the strings facing the street show: about one letter on a 16-string tree, pinched by the taper. Scrolling text travels round the tree once a pass; it does not join up end to end. What does read is letters stacked (`orientation`): each is 5 strings wide and faces the front.
- On a string or an arch there is one row, so all that shows is a slice through the middle of the letters: a few dashes. It is not text.
- On the whole house (a group) the words sit in the middle of the layout, on a grid finer than any prop's pixels. `size` 8 is a broken smudge. On a test house `size` 24 came out as letters 12 pixels tall, and only because a matrix sat in the middle; scrolling, each prop shows the scrap passing over it. Put text on the matrix's own row.
- Scrolling slowly it is read letter by letter; fast it is a streak of color (see `speed`).

## Settings

- `text`: what it says. Always set it: left out, it says Hello, except cut at a lyrics track, where each effect says the words sung at its marks. A backslash and n (\n) starts a new line. Capitals read best: small letters are two rows shorter.
- `size`: the height of a line in rows; capitals fill seven eighths of it (7 rows at 8, 14 at 16). 8 is the font's own size; 16 and 24 are clean doubles and triples; 12 and 20 are fine; 10 is a little uneven. Below 8 the letters lose dots and cannot be read.
- `movement`: `none` holds it still. `left` brings it in from the right edge, first letter first, and across: the one for reading. `right` comes from the left with the last letter leading, so keep it to one short word. `up` rises from below and `down` drops from above. `upLeft`, `downLeft`, `upRight`, and `downRight` cross corner to corner. `wavy` is `left` bobbing up and down. `leftRight` crosses to the left and comes back, and `upDown` rises and comes back down. `vector` slides from the start position to the end position over the whole effect, whatever the `speed`.
- `speed`: columns (or rows) travelled, 2.5 a second for each 1 of speed. One pass takes (prop columns + text columns) divided by (2.5 times `speed`) seconds: five letters at `size` 8 on 40 columns at 10 is 2.8 seconds. For reading keep `speed` near `size` (8 to 12 at 8, 16 to 20 at 16), about three to four letters a second. At twice that a word is a glance; at 40 it crosses a 40-column matrix in under a second and is a streak. `up` and `down` have only the rows to cross, so use 3 to 5. `leftRight` moves four times as fast as `left` and runs well off the prop at each end, and `upDown` moves twice as fast as `up`, so divide. At 0 a moving text is parked at the edge. To cross exactly once, work the speed out from the effect's length.
- `toCenter`: with a straight or corner-to-corner movement, the text comes in and stops in the middle for the rest of the effect: the way to land a word. `noRepeat`: it crosses once and is gone, instead of coming round again.
- `orientation`: `across` is normal. `stackedDown` puts one letter on each line, the first at the top; `stackedUp` puts the first at the bottom. Each letter needs `size` rows: three letters at 8 or 10 on a 30-pixel tree.
- `startX` and `startY`: where still text sits, as a share of the prop's width or height from the middle: `startY` 25 on 20 rows is 5 rows up. A `left` or `right` scroll uses only `startY` (which rows it runs along); `up` and `down` use only `startX`. `endX` and `endY`: where a `vector` move ends; 100 or -100 is off the prop, so -100 to 0 slides it in and stops it in the middle as the effect ends. `pixelOffsets`: all four count pixels instead.
- Colors: one color for everything; with more, a color for each letter in turn. `colorPerWord` gives each word a color instead.
- `countdown`: `none` shows the text. `seconds` takes a number of seconds as the text and counts it down from the effect's start, one a second, stopping at 0: "10" at `size` 16 fills a 40 by 20 matrix. `minutesSeconds` shows it as minutes and seconds ("90" or "1:30" shows as 1 : 30) and needs 41 columns at `size` 8. Leave `speed` above 0 or the count never moves.

## When to use it

- A greeting or a title on the matrix at the start or the end; one word landed on a downbeat in a chorus; a countdown before the show.
- Still words need time to be read: 300 ms and 60 ms more for each letter (600 ms for LIGHT, 780 for LANTERNS), and a second is better. `review_draft` holds still words to that rule. One word at a time, each on its own beat, reads better than a sentence scrolling under a busy chorus.
- Words as they are sung: one `place_effects` for the stretch, a `text` effect with no `text`, `movement` `none`, on `track` "Lyrics (words)". Each effect says the words sung then and stays up as long as they take to read: short words share the grid (on lines of their own when they don't fit across), a word too wide for it is cut over two lines, and a word sung while the one before is still being read waits for it. Placing each sung word by hand (`match`) leaves most of them up for less time than they take to read.
- Give it a background: a dim `on` or `colorWash` on the layer below. For two lines in two colors, use two layers, one with `startY` 25 and one with -25.
- Don't blur it, don't put it on outlines, arches, or a group, and don't ask a 40-column matrix to hold a sentence still: scroll it. `review_draft` says whether placed words can be read.
- Words punched on every beat are flashes: keep a full matrix to two or three changes a second.

## Looks

```look
{"name": "A word held on a matrix", "suits": ["matrix"],
 "note": "Three tall gold letters filling the matrix, fading in and out.",
 "effect": {"kind": "text", "settings": {"text": "JOY", "size": 16}, "colors": ["#ffc040"], "fadeInMs": 300, "fadeOutMs": 300}}
```

```look
{"name": "A greeting scrolling across a matrix", "suits": ["matrix"],
 "note": "Red then green words crossing right to left once in about four seconds, at a pace that can be read.",
 "effect": {"kind": "text", "settings": {"text": "MERRY XMAS", "movement": "left", "speed": 10, "colorPerWord": true}, "colors": ["#ff2020", "#20d040"]}}
```

```look
{"name": "A word that slides in and stops", "suits": ["matrix"],
 "note": "Orange letters arriving from the right in under a second and holding in the middle.",
 "effect": {"kind": "text", "settings": {"text": "BOO", "size": 16, "movement": "left", "speed": 20, "toCenter": true}, "colors": ["#ff7a00"]}}
```

```look
{"name": "Letters stacked down a tree", "suits": ["tree"],
 "note": "One letter above another facing the street, red, white, and green.",
 "effect": {"kind": "text", "settings": {"text": "JOY", "orientation": "stackedDown"}, "colors": ["#ff2020", "#ffffff", "#20d040"]}}
```

```look
{"name": "A ten-second countdown", "suits": ["matrix"],
 "note": "Big white numbers counting down from 10, one a second. Make the effect ten seconds long, ending where the show starts.",
 "effect": {"kind": "text", "settings": {"text": "10", "size": 16, "countdown": "seconds"}, "colors": ["#ffffff"]}}
```
