# Dancer

A cartoon character (skeleton, ghost, witch, Santa, snowman, or elf) that dances on a matrix in time with the beat.

## What it draws

One character, drawn fresh at the size of the prop, standing at the bottom of the grid in the middle. It strikes a pose on every beat and changes move every bar. Everything round it is clear, so a layer below shows as its background.

- On a matrix it is the real thing. At its usual `size` it stands 18 rows tall on a 20-row matrix and every character is easy to tell. At 12 rows they still read (the skull and ribs, Santa's hat and beard). At 8 rows it is a blob that wiggles. So: 12 rows or more, and 10 or more columns for each dancer. A 16 by 16 matrix carries one well.
- On a tree the figure is wrapped round the cone: pinched at the top and spread wide at the foot. The skeleton became a web of white lines and Santa red and white stripes. The street sees the character's colors bobbing in time, not who it is.
- On a string or an arch there is one row, and two or three pixels light in the middle. Nothing to see.
- On the whole house (a group) it is one giant dancer over the whole layout: the matrix showed its ribs or its head and two arches caught its feet. With `count`, each dancer takes its share of the yard's width and lights whatever stands there. On a group with a gap in the middle (a row of four arches, a pair of windows) a single dancer stood in the gap and lit nothing at all.
- It moves at the song's pace: `speed` halves or doubles that.

## Settings

- `character`: `skeleton` (white bones; the default), `ghost` (a pale sheet with stubby arms that floats, no legs), and `witch` (purple hat and dress, green face) for Halloween; `santa` (red suit, white beard), `snowman` (three white balls, hat, scarf, stick arms), and `elf` (green suit, pointed hat) for Christmas.
- `moves`: `mix` dances a different move each bar and goes through all seven before repeating. Or hold one: `bounce` (knees bend on each beat, both arms thrown up), `armWave` (one arm up, the other, both up, both out), `kick` (a kick to one side with that arm up, feet together, then the other side), `twist` (low, hips one way and shoulders the other), `shuffle` (a step to the side and a tap), `jump` (a star jump on the beat, a crouch between), `headBob` (hands on hips, head rocking: the calm one).
- `timingTrack`: the marks it treats as beats: a pose on each, a new move every four. Leave it out and it follows the song's "Beats" track, or two beats a second when the song has none. Given "Bars" it held each pose for a whole bar, a slow-motion dance; given "Lyrics (words)" it struck a pose on each sung word.
- `speed`: `normal` is a pose on every beat. `half` suits ballads and slow verses; `double` is frantic, for a few bars at a peak.
- `size`: its height as a share of the prop's. 90 leaves headroom for the jumps. It never grows wider than its room: a dancer is at most three to four times as tall as its share of the columns is wide. Above 100 it is cropped (at 200 only the legs showed); under about 60 on a 20-row matrix it is too small to recognise.
- `count`: dancers side by side, each in an equal share of the columns. Three on 40 columns are clear; eight were a crowd of thin figures.
- `stagger`: how many beats later each next dancer moves. 0 is unison; 1 sends each move down the line like a ripple.
- `routine`: which order `mix` deals its moves in. Dancers with the same number do the same move at the same moment, on any prop, wherever their effects start. Give two dancers different numbers to make them differ.
- `mirror`: flips the picture left to right, so dancers on a pair of matrices face each other.
- `usePalette`: colors the character from the palette, part by part, instead of its own colors (one color gives a one-color figure with paler and darker markings). Left off, the palette is ignored.
- `bassBounce`: a deeper dip when the bass hits, up to about a tenth of its height. On an 18-row dancer that is a row or two: subtle. 0.5 to 1 on a tall matrix.
- `x` and `y`: where it stands in its share: `x` across (50 is the middle) and `y` up from the bottom. Raise `y` and lower `size` together, or the head goes off the top.
- `background`: `off` is nothing. `glow` puts a dim oval of color behind each dancer: purple behind the skeleton, a color of its own for each character, or the last palette color with `usePalette`.

## When to use it

- The matrix's main act for a chorus or an instrumental break; bring it in and take it out rather than leaving it on all song.
- Two matrices: the same `routine` on both and `mirror` on one. Different routines read as two dancers trading moves.
- Layers: a slow `colorWash` or dim `on` below it, or `glow`; `text` above it for a word on the downbeat.
- Avoid it on trees, outlines, arches, and groups: use `sing` or beat effects there and keep the character on the matrix.
- It does not flash, so it is safe at any tempo.

## Looks

```look
{"name": "A skeleton dancing on a matrix", "suits": ["matrix"],
 "note": "White bones on a dim purple glow, a new move every bar.",
 "effect": {"kind": "dancer", "settings": {"character": "skeleton", "background": "glow"}}}
```

```look
{"name": "Santa bouncing on the beat", "suits": ["matrix"],
 "note": "Santa bending his knees and throwing both arms up on every beat, dipping a little deeper with the bass.",
 "effect": {"kind": "dancer", "settings": {"character": "santa", "moves": "bounce", "bassBounce": 0.6}}}
```

```look
{"name": "Three elves kicking in a ripple", "suits": ["matrix"],
 "note": "A line of three elves, each kicking one beat after the one beside it.",
 "effect": {"kind": "dancer", "settings": {"character": "elf", "count": 3, "moves": "kick", "stagger": 1}}}
```

```look
{"name": "A ghost in icy blue", "suits": ["matrix"],
 "note": "A pale blue ghost floating and swaying over a deep blue glow.",
 "effect": {"kind": "dancer", "settings": {"character": "ghost", "usePalette": true, "background": "glow"}, "colors": ["#bfe8ff", "#2a6bff"]}}
```

```look
{"name": "A snowman swaying at half time", "suits": ["matrix"],
 "note": "A snowman rocking its head from side to side, one pose every two beats: for a slow verse.",
 "effect": {"kind": "dancer", "settings": {"character": "snowman", "moves": "headBob", "speed": "half"}}}
```
