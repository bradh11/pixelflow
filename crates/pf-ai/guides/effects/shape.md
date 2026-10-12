# Shape

Outlines of circles, stars, hearts, snowflakes, and other small pictures that pop up, grow or shrink, and fade, a few at a time, at random, on a timing track's marks, or with the music.

## What it draws

Each shape is an outline drawn round a point. It appears, changes size over its lifetime, fades, and is replaced by a new one, in the next palette color. Everything but the outlines is dark, so a layer below shows through.

- On a matrix it is at home. At the defaults: five thin circles at a time, popping up in random places, each swelling from a dot to about ten pixels in radius while it dims.
- On a tree the shapes are drawn on the tree's grid and wrap round it. With few strings across, nothing larger than a few pixels keeps its form: rings and blots of color coming and going on the cone. Use it there for color, not for pictures.
- On a string or an arch only the pixels where an outline crosses the single row light: a pair of dots for each shape, moving apart as it grows. At the defaults that is a thin scatter of brief dots. With `randomLocation` off it becomes two dashes spreading from the middle to the ends, a ripple along the outline, and that is its one good use there.
- On the whole house (a group) sizes are counted in cells of the house's own grid, far finer than the gaps between props, so the default shapes are specks that land on nothing: almost no light. Set `renderStyle` to `perModelDefault` so each prop draws its own shapes, or use thick rings tens of pixels across spreading from the center (a `growth` of 100, the most there is, carries them only part of the way across a wide display).
- Fast or slow is set by `lifetime`, and that is a share of the effect's length: 5 on a four-second effect is a fifth of a second (a flicker of dots), on a thirty-second effect a second and a half. Work it out from how long a shape should last.

## Settings

Which shape:

- `shape`: `circle`; `ellipse` (flattened by `points`); `triangle`, `square`, `pentagon`, `hexagon`, `octagon`; `star`; `heart`; `tree` (a tiered fir with a trunk); `snowflake` (six thin arms, never thicker); `candyCane` (a hook, hard to read when small); `crucifix` (a cross); `present` (a box with a bow); `random` (a different one each time, never an ellipse). The pictures need a radius of 5 or more to be recognized.
- `points`: a star's points, 4 to 6 for a clean star, 8 or 9 for a spiky burst. For an ellipse, its height in tenths of its width.
- `rotation`: turns the shape, in degrees. A `ramp` curve from 0 to 360 on it spins a star once over the effect. Circles and candy canes don't turn.

How many, how long, how big:

- `count`: shapes alive at once. 3 to 8 on a matrix; 1 with `randomLocation` off for a single shape in the middle.
- `lifetime`: see above. 15 to 50 for shapes you can watch; 100 for one shape that lasts the whole effect.
- `randomStart`: on, the shapes are staggered, so there is always something appearing. Off, they all appear, grow, and vanish together, every `lifetime`.
- `startSize` and `growth`: the radius a shape starts at and how much it gains by the end, in pixels. 1 and 10 is a ring opening from a dot. A `startSize` of 6 with a `growth` of 2 holds a picture at a readable size. A negative `growth` with a large `startSize` closes rings in on a point.
- `thickness`: the outline's width, growing inward. 1 is a hairline, 2 or 3 reads from the street, and a value as large as the radius fills the shape solid.
- `fade`: on, each shape dims as it ages. Off, it stays full until it vanishes, which is crisper and blinks more.

Where, and drifting:

- `randomLocation`: on, each new shape lands somewhere new. Off, all appear at `centerX`, `centerY` (percent from the left and from the bottom), one inside another like ripples.
- `speed` and `direction`: a drift in pixels a second; 0 degrees is right, 90 up, 270 down. 5 to 15 suits a matrix: snowflakes sinking, hearts rising. Hundreds throw the shapes off the prop before they are seen.
- `randomMovement`: every shape drifts its own way at its own speed, often fast. A scatter; rarely what a small prop wants.

When they appear, instead of keeping `count` alive:

- `timingTrack`: one shape at each mark that starts while the effect plays, colors going round the palette mark by mark; `count` is ignored. Give `lifetime` the share of the effect that one mark takes (12 for beats on a four-second effect at 120 beats a minute), so each shape is gone as the next arrives. At the default it is a brief ring on each beat and dark between.
- `fireOnMusic`: one shape when the music rises past `triggerLevel`, and again about every half second while it stays above. Needs a song. Start `triggerLevel` at 50 to 70.

## When to use it

- Pictures for the season on a matrix: snowflakes, hearts, stars, presents, trees, in two or three colors over a dim `on` or a slow `colorWash`.
- A ripple from the middle of an arch or a roofline, on every beat or through a held note.
- One shape on a sung word or a hit: a short effect, `count` 1, `randomLocation` off, `randomStart` off.
- Pops in time: a timing track, or `fireOnMusic` when the song has no timing yet.
- For one clean ring bursting from a point, `shockwave`; for balls of color drifting about, `circles`; for snow that falls and piles up, `snowflakes`. Reach for Shape when the outline itself is the point.
- Avoid the defaults on a string, an arch, or a group. Avoid thick, filled shapes with `randomStart` off and a short `lifetime`: the prop blinks, and nothing large should blink faster than about three times a second.

## Looks

```look
{"name": "A ripple from the middle on every beat", "suits": ["string", "arch", "matrix"],
 "note": "On each beat a pair of dashes, white then icy blue, leaves the center and runs to both ends; a ring opening on a matrix. The lifetime is one beat of a four-second effect: scale it to the effect's length.",
 "effect": {"kind": "shape", "settings": {"shape": "circle", "timingTrack": "Beats", "randomLocation": false, "startSize": 0, "growth": 30, "lifetime": 12, "thickness": 3}, "colors": ["#ffffff", "#40a0ff"]}}
```

```look
{"name": "Snowflakes sinking down a matrix", "suits": ["matrix", "tree"],
 "note": "Eight small white and pale blue flakes appearing, drifting slowly down, and fading.",
 "effect": {"kind": "shape", "settings": {"shape": "snowflake", "count": 8, "startSize": 2, "growth": 2, "lifetime": 50, "speed": 6, "direction": 270}, "colors": ["#ffffff", "#a0d0ff"]}}
```

```look
{"name": "Gold stars popping with the music", "suits": ["matrix", "tree"],
 "note": "A gold or white star bursts open each time the music jumps, then fades.",
 "effect": {"kind": "shape", "settings": {"shape": "star", "fireOnMusic": true, "triggerLevel": 60, "lifetime": 12, "startSize": 2, "growth": 12, "thickness": 2}, "colors": ["#ffc040", "#ffffff"]}}
```

```look
{"name": "One star opening and turning", "suits": ["matrix"],
 "note": "A single gold star in the middle growing through the whole effect while it turns once.",
 "effect": {"kind": "shape", "settings": {"shape": "star", "count": 1, "randomLocation": false, "randomStart": false, "lifetime": 100, "startSize": 2, "growth": 8, "thickness": 2, "fade": false}, "colors": ["#ffc040"], "curves": {"rotation": {"shape": "ramp", "from": 0, "to": 360}}}}
```

```look
{"name": "Christmas shapes on every prop", "suits": ["house", "matrix"],
 "note": "Each prop draws its own small red, white, and green shapes, a different one each time.",
 "effect": {"kind": "shape", "settings": {"shape": "random", "count": 6, "lifetime": 20, "startSize": 2, "growth": 5, "thickness": 2}, "renderStyle": "perModelDefault", "colors": ["#ff2020", "#ffffff", "#20c040"]}}
```
