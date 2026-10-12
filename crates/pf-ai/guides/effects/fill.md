# Fill

A prop filled from one side up to a point you set, the palette spread along the way: a level bar, a curtain, or a slow rise once a curve moves the point.

## What it draws

Lines of pixels lit from one side of the prop as far as `position`, and nothing beyond it, so the layers below show there. It does not move by itself: at its defaults the prop is simply full, the palette blended from bottom to top, and every frame is the same. All its motion comes from a curve on `position`.

- The palette is laid over the whole prop, not over the lit part. The first color sits at the side the fill starts from and the last at the far side, and a growing fill uncovers them in turn. With one color it is a plain bar.
- On a matrix: with `up`, a block rising from the bottom edge a row at a time; with `right`, one growing from the left edge a column at a time.
- On a tree: `up` climbs from the foot to the tip. `right` and `left` go round the trunk a string at a time, which from the street looks like strings switching on, not like anything filling.
- On a string or an arch, use `right` or `left`: the fill grows along the run. These props are one row tall, so `up` and `down` have a single line to work with. With `up` the prop stays dark until `position` reaches 100 and then lights whole; with `down` it is fully lit for any `position` above 0.
- On the whole house (a group) it is one fill across the layout. `up` lights the arches, the canes, and the foot of the tree first, then the windows and the matrix, and the roofline last. `right` crosses the yard from the left. A group of arches has height, so there `up` fills every arch from both feet toward the crown, and `right` fills the row arch by arch. With `renderStyle` set to `perModelDefault` each member fills alone.
- The edge is hard and moves in whole lines, so a prop with few lines fills in visible steps. Over half a second it is a swipe. Over several bars it is a slow rise that people watch climb.

## Settings

- `position`: how far the fill reaches. Held at a number it is a part-filled prop (60 on a house lights everything but the top and leaves the roofline dark). To animate it, give it a curve: `ramp` from 0 to 100 fills over the effect, 100 to 0 empties it, `sine` makes it rise and fall, and `music` turns it into a level meter.
- `direction`: the side the fill grows toward. `up` starts at the bottom, `down` at the top, `right` at the left edge, `left` at the right edge.
- `colorByTime`: instead of spreading the palette along the fill, the whole fill is one color that walks from the first palette color at the start of the effect to the last at its end.
- `bandSize` and `skipSize`: only with `colorByTime` on; without it they do nothing. The fill becomes stripes: `bandSize` lines of each palette color in turn, with `skipSize` black lines after each. The stripes stand still while a growing fill uncovers them, and the black lines hide the layers below. They count lines, so 2 to 4 suits a matrix or a tree.
- `offset`: starts the fill that far in from its side, for a band across the middle of a prop. It stretches the palette too, so the fill no longer starts on the first color.
- `offsetInCells`: when on, `offset` counts lines, and a count past the prop's size goes round again (20 on a prop 20 lines tall is no offset at all). When off, it is a percent of the prop, which behaves the same on props of different sizes.
- `wrap`: when on, a fill that started part way in carries on round from the opposite side, so 100 still lights everything. When off, it stops at the far edge and the part before the offset stays dark.

## When to use it

- A level meter: a `music` curve on `position`, in green, yellow, and red, with `up` on a matrix or a tree, or `right` along a roofline. Across a whole house a small change in level switches whole props on and off, so keep it to one prop or narrow the curve's range.
- A build: `up` on the house group with a `ramp` across the bars of a rise, so the display fills from the ground and the roofline lights last.
- A curtain: `down` with a `ramp` from 0 to 100 drops color from the top, and from 100 to 0 lifts it away again.
- A part-lit prop held still: the lower half of a tree in one color with another effect showing above it.
- Fill or `wipe`: a wipe moves by itself, has a soft edge and more directions, and can clear behind itself, so reach for it for a one-off transition. Fill has a hard, stepped edge but stops wherever a curve puts it, so use it when the level has to follow something: the music, a rise, a bounce.
- At its defaults it draws what `on` draws with a gradient. Use `on` for a still blend.
- Avoid `up` and `down` on strings and arches. Avoid a `square` curve, or any fast jumping curve, on `position` over a large prop: a big area snapping on and off is a flash, and nothing should flash faster than about three times a second across a large part of the display.

## Looks

```look
{"name": "Filling from the ground up", "suits": ["matrix", "tree", "house"],
 "note": "Green climbing from the bottom and paling to white at the top, reaching the top as the effect ends.",
 "effect": {"kind": "fill", "settings": {"direction": "up"}, "colors": ["#00a030", "#ffffff"],
            "curves": {"position": {"shape": "ramp", "from": 0, "to": 100}}}}
```

```look
{"name": "Growing along an outline", "suits": ["string", "arch", "matrix", "house"],
 "note": "Red spreading from the left end and warming to gold at the far end, over the whole effect.",
 "effect": {"kind": "fill", "settings": {"direction": "right"}, "colors": ["#ff0000", "#ffc040"],
            "curves": {"position": {"shape": "ramp", "from": 0, "to": 100}}}}
```

```look
{"name": "Level meter on the music", "suits": ["matrix", "tree"],
 "note": "A bar that jumps with the loudness: green at the bottom, yellow in the middle, red only at the loudest moments.",
 "effect": {"kind": "fill", "settings": {"direction": "up"}, "colors": ["#00c040", "#ffe000", "#ff0000"],
            "curves": {"position": {"shape": "music", "from": 5, "to": 100}}}}
```

```look
{"name": "Level bar along a roofline", "suits": ["string", "arch"],
 "note": "The same meter laid along a one-row prop, growing from the left end with the music.",
 "effect": {"kind": "fill", "settings": {"direction": "right"}, "colors": ["#00c040", "#ffe000", "#ff0000"],
            "curves": {"position": {"shape": "music", "from": 5, "to": 100}}}}
```

```look
{"name": "Candy stripes rising", "suits": ["matrix", "tree"],
 "note": "Red and white stripes with a dark line between, uncovered from the bottom as the fill climbs.",
 "effect": {"kind": "fill", "settings": {"direction": "up", "colorByTime": true, "bandSize": 3, "skipSize": 1},
            "colors": ["#ff0000", "#ffffff"], "curves": {"position": {"shape": "ramp", "from": 0, "to": 100}}}}
```
