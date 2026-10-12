# Color Shift

A prop held in one color that changes to the next palette color at a set moment, everywhere at once or as a change travelling across it: for key changes, new sections, and drops.

## What it draws

Every pixel lit in one flat color, hiding the layers under it. The effect starts on the first palette color and begins changing to the second on its first frame; once the change is done it holds the new color to the end. With more colors there is a change for each step, the first at the start and the rest spaced evenly through the effect: three colors change again at the half-way point, and five colors on a four-bar effect change on every bar line. With one color nothing changes.

So start the effect on the beat where the change should happen, and make its first color the one the prop was already showing.

- With `stagger` at 0 the whole prop changes together, and it looks the same on a string, an arch, a matrix, and a tree.
- With `stagger` raised, the new color starts at one side and travels along `direction`, the old color still showing ahead of it. On a string or an arch it runs along the wiring (`up` and `down` run along it too, from the left end and from the right). On a matrix every direction is distinct: an edge crossing, rising, or falling, a slant from the bottom-left corner, an oval growing from the middle. On a tree the sideways directions go round the trunk, and `up` climbs from the foot, which is the one that reads best from the street.
- On the whole house (a group) a staggered change crosses the layout as one picture. With `leftToRight` the tree at the left turns first and the far end last; with `up` the arches and the foot of the tree turn first and the roofline last; `radial` spreads from the middle of the house. With `renderStyle` set to `perModelDefault` every prop changes across itself at the same moment instead.
- Quick (a beat or less) it is a cut that lands on the music. Slow (one change taking the whole effect) it is a long melt from one color into the other.

## Settings

- `duration`: how long each change takes, as a share of the effect, so work it out from the effect's length: on an 8-second effect at 120 beats a minute, 6 is one beat and 25 is a bar. It can't be longer than the gap between changes. At 100 with two colors the change takes the whole effect.
- `ease`: `smooth` starts and ends each change gently and suits almost everything. `linear` changes at an even rate, for a long slow blend. `instant` snaps: with no `stagger` the new color is simply there from the second frame, and with `stagger` it gives a hard edge sweeping across.
- `stagger`: how much of the change time is spent travelling. 0 changes everywhere at once. 20 to 40 is a soft lean, one side a little ahead. 70 to 90 is a clear front with a short blend behind it. 100 is a hard edge that takes the whole change time to cross.
- `direction`: which way a staggered change travels; it does nothing while `stagger` is 0. `leftToRight` and `rightToLeft` cross the prop. `up` rises from the bottom and `down` falls from the top. `centerOut` spreads from the middle to both sides, and `edgesIn` closes from both sides on the middle. `diagonal` runs from the bottom-left corner to the top right, and `radial` grows as a circle from the center. Keep those two for matrices and groups: on a string or an arch they cover only part of the travel, so the change starts late or ends early.

## When to use it

- A key change or a new section: one effect from the first beat of the section to its end, its first color the old section's and its second the new. It is then the section's base color, with the moving effects on layers above it.
- A drop: `stagger` at 80 with `radial` or `centerOut` on the house group and a `duration` worth a beat, so the new color bursts outward from the middle.
- A color for every bar: one more color than there are bars, with a short `duration`. The first color is only where it starts from; each of the others is held for a bar.
- A long melt through a verse: two colors, `duration` at 100, `linear`.
- `stage_cue` with `color_shift` places this effect for you (`smooth`, with a `duration` of 30) on each prop's row, each starting a little after its neighbour so the change crosses the display over a beat. Use the cue to mark a moment over looks that are already there. Place the effect yourself when it should be the section's base, or when one group should carry a staggered sweep.
- Choosing: `colorWash` never rests; it drifts the whole time. Color Shift holds, changes, and holds. `wipe` brings color onto a dark prop and can take it off again; Color Shift is always fully lit. `on` doesn't change at all, which is also what two colors with `instant` and no `stagger` amounts to.
- It hides the layers under it, so it belongs at the bottom of a row.
- Flash safety: an `instant` change between colors that contrast, on the whole house, is a flash. One is fine. Many colors on a short effect is a run of them: keep it under about three changes a second across a large part of the display.

## Looks

```look
{"name": "Key change, everywhere at once", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Red melting to green over the first part of the effect, then holding green. Start it on the first beat of the new section.",
 "effect": {"kind": "colorShift", "settings": {"duration": 15}, "colors": ["#ff0000", "#00c040"]}}
```

```look
{"name": "New color crossing the display", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Blue giving way to white from the left, a clear front with a short blend behind it.",
 "effect": {"kind": "colorShift", "settings": {"duration": 40, "stagger": 80, "direction": "leftToRight"}, "colors": ["#2a6bff", "#ffffff"]}}
```

```look
{"name": "Color rising from the ground", "suits": ["matrix", "tree", "house"],
 "note": "Green turning gold from the bottom up, the top the last to change.",
 "effect": {"kind": "colorShift", "settings": {"duration": 50, "stagger": 70, "direction": "up"}, "colors": ["#00a030", "#ffc040"]}}
```

```look
{"name": "Opening out from the middle", "suits": ["string", "arch", "matrix", "house"],
 "note": "Purple splitting at the center to show orange, the two edges running out to both ends: a Halloween drop.",
 "effect": {"kind": "colorShift", "settings": {"duration": 30, "stagger": 85, "direction": "centerOut"}, "colors": ["#7a00d0", "#ff6a00"]}}
```

```look
{"name": "A new color on every bar", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Leaving red at once, then green, white, blue, and gold, each held for a quarter of the effect and melting quickly to the next. Give it four bars and it changes on every bar line.",
 "effect": {"kind": "colorShift", "settings": {"duration": 4}, "colors": ["#ff0000", "#00c040", "#ffffff", "#2a6bff", "#ffc040"]}}
```
