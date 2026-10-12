# On

A prop held in one color, or the palette spread across it, steady or changing brightness: the base under almost everything.

## What it draws

Every pixel lit in the first palette color, at one brightness that can ramp from the start of the effect to its end. Nothing moves.

- On a string or an arch: the whole run in one color. With `gradient` set to `horizontal` the palette is spread along it, first color at the left end and last at the right.
- On a matrix: a solid block, or a smooth blend across it (`horizontal`) or up it (`vertical`).
- On a tree: a solid cone. A `vertical` gradient puts the first color at the foot and the last at the top, which is the classic two-tone tree. A `horizontal` one runs the colors round the trunk.
- On the whole house (a group): one color everywhere, or one gradient laid across the whole layout, so with `horizontal` the left of the yard is the first color and the right the last, and with `vertical` the arches and the foot of the tree are the first color and the roofline the last.
- A `vertical` gradient on a one-row prop (a string, an arch) has no height to spread over, so the whole prop takes one blended color from the middle of the palette. Use `horizontal` there.

## Settings

- `gradient`: `none` uses the first color only. `horizontal` spreads the palette left to right, `vertical` bottom to top. With two colors it is a blend from one to the other; with three, through the middle one.
- `startLevel` and `endLevel`: brightness at the start and at the end of the effect, each from 0 to 1, with a straight ramp between. 1 and 1 is steady. 0 to 1 swells in over the whole effect; 1 to 0 dies away. For a level that does anything other than one straight ramp, leave these at 1 and put a curve on `brightness`.
- `solid`: when on, a dimmed On still hides the layers under it. Left off (the default), the dimmer it is the more of what is below shows through, so a fading On uncovers the layer beneath rather than fading to black.
- `transparency`: lets what is below show through whatever the brightness: 0 hides it, 1 shows it all. Around 0.5 it tints the layer under it with its color instead of covering it.
- `shimmer`: every other frame is drawn in the second palette color, or dark when there is only one color. It is a very fast flicker (twenty times a second at the usual frame rate), which reads as a buzz or a blend of the two colors, not as a twinkle. Keep it to one small prop for a moment; never the whole house.

## When to use it

- The floor of a look: a dim On (a `brightness` of 0.2 to 0.4, or a dark color) on a group's row, so nothing is ever fully black while accents play above it.
- A swell or a fade: `startLevel` and `endLevel`, or fades, across a held chord or the last bar.
- Color blocking: a different solid color on each group for a section, changed at the next. Simple, and it reads from across the street.
- A two-tone tree or a house washed warm at the bottom and cool at the top: `vertical` gradient on the group.
- Glitter: On in a dark color with `sparkles` (and `musicSparkles`) turns a plain prop into a field of glints.
- A beat pulse without another effect: a `timingFade` curve on `brightness` (topic `curves`).
- Avoid leaving a full-brightness white On under everything: it flattens the show and there is nowhere to go for the big moments. Avoid `shimmer` across large areas.

## Looks

```look
{"name": "Warm white hold", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Everything steady in warm white, slightly held back, with slow fades in and out.",
 "effect": {"kind": "on", "colors": ["#ffc880"], "brightness": 0.7, "fadeInMs": 600, "fadeOutMs": 600}}
```

```look
{"name": "Two-tone tree", "suits": ["tree", "matrix", "house"],
 "note": "Green at the foot blending to white at the top.",
 "effect": {"kind": "on", "settings": {"gradient": "vertical"}, "colors": ["#00a030", "#ffffff"]}}
```

```look
{"name": "Red to green along an outline", "suits": ["string", "arch", "house"],
 "note": "Red at the left end blending to green at the right.",
 "effect": {"kind": "on", "settings": {"gradient": "horizontal"}, "colors": ["#ff0000", "#00c040"]}}
```

```look
{"name": "Swell", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Blue rising from almost dark to full over the effect: for a held note or the bar before a hit.",
 "effect": {"kind": "on", "settings": {"startLevel": 0.05, "endLevel": 1}, "colors": ["#2a6bff"]}}
```

```look
{"name": "Night sky glitter", "suits": ["matrix", "tree", "house"],
 "note": "Deep blue with white glints that come and go.",
 "effect": {"kind": "on", "colors": ["#001a66"], "sparkles": 60}}
```
