# Pinwheel

Arms of color turning steadily round a center point, straight like a windmill or curled into a swirl: a wheel for a matrix or the whole house that turns at the same rate however long it runs.

## What it draws

Arms reaching out from a center, one palette color to each arm in turn, all turning together.

- At its defaults the arms are lines one pixel wide: three thin spokes on a matrix and a few stray pixels anywhere else. Raise `thickness` first; almost every use wants 40 to 100.
- On a matrix: wedges of color turning like a windmill, with dark between them below 100 and a full pie at 100. With `twist` the wedges curl into a swirl, the peppermint candy.
- On the whole house (a group): the wheel sits on the middle of the layout and its wedges sweep round the yard like searchlight beams. Each prop takes the color of the wedge passing over it: one half of the roofline and then the other, each arch in turn, the windows, the tree.
- On a tree: the wheel is drawn on the tree's unrolled grid, centered half way up one string, so from the street it is patches of color circling over the tree, not a wheel. Set `renderStyle` to `perPreview` and it is a true wheel on the tree's face.
- On a string or an arch: the prop is a single line through the hub. With straight arms each half of it is one color at a time, or dark, switching as the arms pass, so the prop only blinks through the palette. With `twist` it becomes useful: bands of the colors travelling between the middle and the ends.
- Speed 1 to 4 is a slow, stately turn (at 1 a full turn takes 18 seconds). 8 to 12 is lively. Above 20 with wide arms it is a flicker.

## Settings

- `arms`: how many. Colors go round arm by arm, so use a multiple of the number of colors, or two neighbours match. 2 to 4 for bold wedges, 6 to 8 for a swirl or for rays.
- `thickness`: how much of its slice of the circle each arm fills. 0 is a thin line, 50 is arms and gaps of equal width, 100 is color against color.
- `armSize`: how far the arms reach. 100 reaches the prop's corners from its middle; 50 goes half way. Go above 100 only when the center is moved: 200 covers the prop from an edge or a corner.
- `twist`: how far the arms bend between hub and tip, in degrees. 0 is straight, 90 to 180 a curved blade, 360 wraps each arm once round. Turning counterclockwise, a positive twist makes the swirl draw in toward the hub and a negative one makes it spin outward; turning clockwise it is the other way about.
- `speed`: the turning rate. Each unit is 20 degrees a second: 18 is one turn a second, and the default 10 a turn in just under two. It doesn't change with the effect's length. 0 holds the wheel still.
- `counterclockwise`: on, it turns counterclockwise; off, clockwise.
- `shading`: `flat` is solid arms. `raised` is brightest down the middle of each arm and fades to its edges: soft, glowing blades. `sunken` is bright at the edges and dark down the middle: arms in outline. `sweep` fades from one edge of an arm to the other, so a single arm at `thickness` 100 is a radar sweep. Shading shows only on arms with some width.
- `offset`: where the first arm points at the start, in degrees. Use it to set a still wheel, or to start two wheels out of step.
- `centerX` and `centerY`: where the hub is. 0 is the middle; -100 is the left edge (or the bottom) and 100 the right edge (or the top). A hub on the bottom edge turns the arms into rays fanning upward. On a one-row prop `centerY` does nothing.
- `style`: `smooth` lights every pixel of each arm, and is the one to use. `spokes` draws each arm as a bundle of rays a degree apart: solid near the hub, breaking into separate dotted lines far from it on a big prop or a group, and reaching only the middle of the longer side at an `armSize` of 100. It is there for imported sequences.

## When to use it

- A turning look that runs: a chorus or an instrumental on a matrix or across the whole house. `fan` also spins blades from a center, but it is a set piece tied to its effect's length (it opens, makes a set number of turns, and closes, with every color on each blade). Use `fan` for a flourish over one phrase and Pinwheel for a look that keeps going.
- Rays behind a singing face or `text`: hub on the bottom edge, many arms, slow.
- Thin or `raised` arms with blend `max` over a `colorWash` or an `on` give moving beams on a colored base.
- A `ramp` curve on `speed` spins it up. Keep `music` and timing shapes off `speed` (the wheel is placed by speed times the time played, so it jumps); put them on `thickness`, where the arms swell with the song, or on `brightness`.
- Flash safety: at each pixel, `arms` times `speed` divided by 18 arms pass every second. Keep that under 3 for wide arms: 4 arms no faster than 12, 8 arms no faster than 6.
- Avoid straight arms on a single string or arch, and `thickness` 0 anywhere but a matrix.

## Looks

```look
{"name": "Peppermint swirl", "suits": ["matrix", "house"],
 "note": "Six curled wedges, red against white, turning slowly and drawing in toward the middle.",
 "effect": {"kind": "pinwheel", "settings": {"arms": 6, "thickness": 100, "twist": 360, "speed": 6}, "colors": ["#ff0000", "#ffffff"]}}
```

```look
{"name": "Searchlight wedges round the house", "suits": ["matrix", "house"],
 "note": "Four straight wedges, red and green with dark between, sweeping round the middle of the display.",
 "effect": {"kind": "pinwheel", "settings": {"arms": 4, "thickness": 50, "speed": 5}, "colors": ["#ff0000", "#00c040"]}}
```

```look
{"name": "Radar sweep", "suits": ["matrix", "house"],
 "note": "One green arm filling the whole circle, bright at its leading edge and fading behind it.",
 "effect": {"kind": "pinwheel", "settings": {"arms": 1, "thickness": 100, "shading": "sweep", "speed": 10}, "colors": ["#00ff60"]}}
```

```look
{"name": "Rays fanning up from the ground", "suits": ["matrix", "tree", "house"],
 "note": "Eight soft gold rays spreading from the middle of the bottom edge and turning slowly.",
 "effect": {"kind": "pinwheel", "settings": {"arms": 8, "thickness": 50, "armSize": 200, "centerY": -100, "speed": 3, "shading": "raised"}, "colors": ["#ffc040"]}}
```

```look
{"name": "Bands running out along an outline", "suits": ["string", "arch"],
 "note": "Red and white bands leaving the middle of the prop and travelling to both ends.",
 "effect": {"kind": "pinwheel", "settings": {"arms": 2, "thickness": 100, "twist": -360, "speed": 10}, "colors": ["#ff0000", "#ffffff"]}}
```
