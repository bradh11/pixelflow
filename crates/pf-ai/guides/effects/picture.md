# Picture

One of the show's own pictures or animated GIFs on a matrix: held, scrolled, zoomed, or panned.

## What it draws

The picture named in `file`, sized to the prop's grid and placed in its middle. Clear parts of a picture stay clear. A picture with no clear parts covers its whole rectangle, black included, and hides the layer below there.

It can only show a picture the user has already put in the show. The names are in what `get_open_sequence` answers; there is no tool that adds one. With no `file`, or a name the show doesn't have, it draws nothing.

- On a matrix it is the picture. A square picture on a 40 by 20 matrix came out 20 by 20 in the middle with dark either side. A picture needs about 12 rows to be recognised: shrunk to 5 rows a snowman was a white dot.
- On a tree the picture is wrapped round the trunk, so the street sees the middle half of it pulled into a cone: a snowman was white strings with a red band where its scarf was. Colors, not a picture.
- On a string or an arch there is one row. Fitted, the picture shrinks to a single lit pixel. Filling, the row shows one slice through its middle as dashes of color. Don't.
- On the whole house (a group) it is sized to the whole layout, and each prop shows the piece of a giant picture that falls on it: the matrix showed the snowman's chest and scarf, two arches its base, everything else stayed dark. Scrolled, it is a big shape crossing the yard, lighting props as it passes.
- Scrolling at the usual `moveSpeed` it takes four seconds to cross, which is easy to follow; at 1 it crosses in a second and is only a flick of color.

## Settings

- `file`: the picture, by its name in the show.
- `fit` (how it is sized): `fit` shows all of it with room left on two sides. `fill` covers the prop and cuts off what hangs over. `stretch` squashes it to the prop's shape. `actual` is one picture pixel for each prop pixel, cropped at the edges: right for art drawn at the matrix's own size.
- `scale`: its size on top of that, 100 unchanged. 50 to 80 leaves room beside it; above 100 it is cropped by the prop.
- `xOffset` and `yOffset`: move it right or up by a share of the prop (50 is half the prop's width); past about 100 it is off the prop. `pixelOffsets`: count the offsets in pixels instead.
- `movement`: `none` holds it. `left`, `right`, `up`, and `down` scroll it from just off one side to just off the other, over and over. `zoomIn` grows it from nothing to full size over the effect and `zoomOut` shrinks it away. `pan` is for a picture larger than the prop (`fill`, `actual`, or a `scale` above 100): it slides from one end of the overhang to the other over the effect. On a picture that already fits, `pan` does nothing.
- `moveSpeed`: crossings a second when scrolling. 0.25 is four seconds. For one crossing in an effect, use 1 divided by its length in seconds.
- `wrap`: what leaves one side comes straight back on the other, with no gap when the picture is at least as wide as the prop: a banner that runs for ever. A still picture pushed off one side by `xOffset` comes back on the other too.
- `timing` (animated GIFs only): `loop` plays at the GIF's own speed again and again. `once` plays it through and holds the last frame. `stretch` makes one pass last the whole effect, so the animation ends as the effect does.
- `playSpeed`: times the GIF's own speed; with `timing` set to `stretch`, how many passes fit in the effect. `startFrame`: the frame it begins on. Neither does anything to a still picture.
- `blackTransparent`: black in the picture becomes clear, so a wash on the layer below shows round the subject. Use it for art drawn on black. `blackLevel` is how dark still counts as black, and matters only with `blackTransparent` on: 0 is pure black, 10 to 20 clears dark fringes. Go gently: by 35 a pure red or blue counts as black too, and at 60 the snowman's scarf and nose had gone.
- `tint`: colors the picture with the first palette color: white parts take it fully, so a white snowman turns gold.
- `turn`: `none` is upright; `right` and `left` are quarter turns; `half` is upside down.
- `crisp`: hard-edged pixels when it is resized. Turn it on for pixel art made bigger; leave it off for photos and anything made smaller.
- `resize`: leave it at `own`. `xlightsNearest` and `xlightsMixed` copy how older xLights versions resized, and are only for sequences imported from them.

## When to use it

- A character, a logo, or a seasonal picture on the matrix for an intro, an ending, or a quiet verse; an animated GIF as the matrix's own act.
- First look at the pictures the show has. If none suits, say so and offer `text` or `dancer` instead; never invent a file name.
- Layers: a `colorWash` or a dim `on` below with `blackTransparent` on; `text` on a layer above.
- A zoom or a scroll lands well on a section change: `zoomIn` through the bar before a chorus, then hold with a second effect.
- Avoid it on outlines, arches, trees, and groups, and avoid shrinking a detailed picture onto a small matrix. The one use on a group: a wide strip of color blocks with `fit` set to `stretch`, scrolled with `wrap`, washes moving colors over every prop.
- A GIF that flashes is a strobe on a matrix: keep `playSpeed` down so nothing blinks faster than about three times a second.

## Looks

```look
{"name": "A picture held on a matrix", "suits": ["matrix"],
 "note": "The whole picture in the middle of the matrix, fading in and out.",
 "effect": {"kind": "picture", "settings": {"file": "images/snowman.png"}, "fadeInMs": 400, "fadeOutMs": 400}}
```

```look
{"name": "A picture crossing a matrix", "suits": ["matrix"],
 "note": "It comes in from the right edge and leaves at the left, four seconds a crossing.",
 "effect": {"kind": "picture", "settings": {"file": "images/snowman.png", "movement": "left", "moveSpeed": 0.25}}}
```

```look
{"name": "A picture growing from nothing", "suits": ["matrix"],
 "note": "A dot in the middle that swells to the full picture by the end of the effect.",
 "effect": {"kind": "picture", "settings": {"file": "images/snowman.png", "movement": "zoomIn"}}}
```

```look
{"name": "A picture over a colored background", "suits": ["matrix"],
 "note": "The subject with its black surround cleared away. Put a wash on the layer below and it shows all round.",
 "effect": {"kind": "picture", "settings": {"file": "images/snowman.png", "blackTransparent": true}}}
```

```look
{"name": "A close-up panning down the picture", "suits": ["matrix"],
 "note": "The picture filling the matrix edge to edge, sliding slowly from its top to its bottom over the effect.",
 "effect": {"kind": "picture", "settings": {"file": "images/snowman.png", "fit": "fill", "movement": "pan"}}}
```
