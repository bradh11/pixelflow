# Bars

Stripes in the palette's colors sliding across or up a prop: the workhorse for outlines, arches, trees, and whole-house movement.

## What it draws

Bands of color, one for each palette color in turn, with a dark gap after each, all sliding the same way at a steady speed.

- On a matrix or a tree it is unmistakable: horizontal bands climbing (the default), or vertical bands sliding sideways. On a tree, sideways bands travel round the trunk.
- On a string or an arch, mind the `axis`. These props are one row tall, so the default (bands stacked up and down) has only one band to show: the whole prop is one flat color that switches to the next as each band passes, with a dark beat for each gap. It reads as the prop blinking through the palette. Set `axis` to `horizontal` and the bands lie along the prop and slide down its length, which is what "bars on an arch" usually means.
- On the whole house (a group), the bands cross the whole layout as one picture: rising bands light the arches and the foot of the tree first, then the windows and the matrix, then the roofline. Sideways bands sweep from one end of the yard to the other. Each prop shows only the slice of the picture that passes over it, so a roofline is fully lit or fully dark as a rising band crosses its height.
- Slow (under 0.3) it is a calm scroll you can leave running for a whole verse. At 1 to 2 it drives. Much above 3 the stripes blur into flicker, and on a one-row prop with the default axis that flicker is the whole prop flashing.

## Settings

- `count`: how many bands show at once, before the colors repeat. 2 to 4 on a string or arch, 4 to 8 on a matrix or tree, 2 to 3 across a whole house. Each band is the prop's length divided by `count` times the number of colors, so many colors and a high count on a short prop leaves bands only a pixel or two long.
- `speed`: prop lengths travelled per second. 0.2 to 0.5 for a base look, 1 to 2 for a chorus. A `ramp` curve on `speed` makes a good build; it ends at nearly twice its `to`, and other curve shapes on a speed make the stripes lurch (topic `curves`).
- `axis`: `vertical` (the default) stacks the bands up and down, so they rise or fall. `horizontal` lays them side by side, so they slide left or right, or along a string.
- `direction`: `forward` is up or to the right; `reverse` is down or to the left. With `motion` set to `split`, `forward` moves apart from the center and `reverse` closes in on it.
- `motion`: `slide` moves everything one way. `split` mirrors the bands about the center so they spread from it or close on it: lovely on an arch, a roofline, or a matrix when the music opens out. `step` jumps a whole band at a time instead of gliding, which lands well on a beat when `speed` and `count` are chosen to match the tempo.
- `fill`: how much of each band is lit; the rest is its dark gap. 0.5 is even stripes. 1 is solid color butted against color. 0.1 to 0.2 gives thin lines with wide dark between them, which looks elegant on a matrix or tree and leaves room for a layer below to show.
- `center`: where the bands of a `split` meet, from one edge to the other; 0 is the middle. Move it to put the split on a peak of the roof or a door.
- `shaded`: each band fades out along its length, like a comet's tail, instead of ending square. Softer, and good with `fill` at 1.
- `highlight`: the leading edge of each band is white. A crisp accent on a matrix; on a one-row prop with the default axis it turns the whole prop white, so pair it with `horizontal` there.
- `gradient`: each band blends into the next color instead of meeting it at an edge. With `fill` at 1 it becomes a rolling rainbow of your palette.

## When to use it

- The base look of an up-tempo section: two colors, slow, on the outline and arches.
- Motion that agrees across props: the same direction and speed on every row makes the house move as one.
- A build: one long effect with a `ramp` curve on `speed`.
- Layered: thin white bars (`fill` 0.15) with blend `max` over a `colorWash` gives moving highlights on a colored base.
- Avoid high `count` with many colors on small props, fast `speed` on one-row props with the default axis (it strobes), and Bars on every prop in every section: it is easy to overuse.
- It pairs with `twinkle` or `on` under it, with `chase` on the arches while Bars runs on the roofline, and with `spiral` on the tree (a spiral is Bars with a twist).

## Looks

```look
{"name": "Candy stripe along an outline", "suits": ["string", "arch"],
 "note": "Red and white stripes sliding slowly along the prop.",
 "effect": {"kind": "bars", "settings": {"axis": "horizontal", "count": 3, "speed": 0.3, "fill": 1}, "colors": ["#ff0000", "#ffffff"]}}
```

```look
{"name": "Opening out from the middle", "suits": ["string", "arch", "matrix", "house"],
 "note": "Green and red bands spreading from the center to both ends.",
 "effect": {"kind": "bars", "settings": {"axis": "horizontal", "count": 2, "speed": 0.6, "motion": "split"}, "colors": ["#00c040", "#ff2020"]}}
```

```look
{"name": "Rising bands on a tree", "suits": ["tree", "matrix", "house"],
 "note": "Soft-edged bands of blue and white climbing, each fading as it goes.",
 "effect": {"kind": "bars", "settings": {"count": 3, "speed": 0.5, "fill": 1, "shaded": true}, "colors": ["#2a6bff", "#ffffff"]}}
```

```look
{"name": "Thin gold lines", "suits": ["matrix", "tree"],
 "note": "Fine gold lines drifting upward with wide dark between them; made to sit over another layer.",
 "effect": {"kind": "bars", "settings": {"count": 4, "speed": 0.25, "fill": 0.15}, "colors": ["#ffc040"], "blend": "max"}}
```

```look
{"name": "Round the tree", "suits": ["tree"],
 "note": "Red and green bands travelling round the trunk.",
 "effect": {"kind": "bars", "settings": {"axis": "horizontal", "count": 2, "speed": 0.4, "fill": 0.7}, "colors": ["#ff0000", "#00b030"]}}
```
