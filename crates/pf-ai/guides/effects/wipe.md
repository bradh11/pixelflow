# Wipe

Color sweeping across a prop or the whole layout behind a soft edge, then holding or clearing, or a bar of light crossing it: the one-off transition and the sweep.

## What it draws

An edge crosses the target once. Behind it the prop is lit; ahead of it nothing is drawn, so the layers below show until the edge arrives. The crossing takes `duration` percent of the effect, and then the color holds or clears, as `mode` says. It moves by itself: no curve is needed.

With more than one color the palette is laid along the sweep, the first color where it starts and the last where it ends, so the wipe leaves a blend behind it. Give it one color for a flat wipe.

- On a string or an arch it runs along the wiring: `leftToRight` from the left end, `centerOut` from the middle to both ends, `edgesIn` from both ends to the middle. `up` is the same as `leftToRight` there, and `down` the same as `rightToLeft`.
- On a matrix all eight directions are distinct: a straight edge crossing, rising, or falling, two edges parting or meeting, a slanted edge from the bottom-left corner, an oval growing from the middle.
- On a tree the sideways directions go round the trunk, which from the street looks like strings lighting from one side. `up` from the foot and `down` from the tip are the clear ones. With `renderStyle` set to `perPreview` a sideways wipe crosses the cone as the audience sees it instead.
- On the whole house (a group) the sweep crosses the layout as one picture, and each prop lights as the edge reaches it. `leftToRight` takes the tree at the left first and the far end last; `up` starts with the arches and canes on the ground and ends on the roofline; `radial` spreads from the middle of the house. On a group of arches `leftToRight` goes arch by arch, `up` fills each from its feet to its crown, and `centerOut` opens from the gap between the middle two. With `renderStyle` set to `perModelDefault` every member runs its own wipe at once, and with `defaultModelAsPixel` whole props switch on one after another.
- The crossing time is the effect's length times `duration`. Over half a second it is a whoosh. Over two bars it is a slow curtain.

## Settings

- `direction`: `leftToRight` and `rightToLeft` cross the prop; `up` rises from the bottom and `down` falls from the top; `centerOut` runs from the middle to both sides and `edgesIn` from both sides to the middle; `diagonal` goes from the bottom-left corner to the top right; `radial` is a circle growing from the center. Keep `diagonal` and `radial` for matrices and groups: on a string or an arch `diagonal` starts late and ends early, and `radial` is a `centerOut` that finishes early.
- `mode`: `on` wipes on and holds to the end. `onOff` wipes on, holds, and wipes off in the same direction, clearing first what it lit first; each wipe gets at most half the effect, so with `duration` at 50 or more there is no hold. `off` starts fully lit and clears.
- `duration`: the share of the effect each wipe takes. 100 makes the crossing last the whole effect; 10 to 25 is a quick sweep with a long hold after it.
- `softness`: how wide the fading edge is, as a share of the prop. 0 to 0.03 is a hard line. 0.1 to 0.2 is a feathered edge. 0.3 to 0.4 is a long glow running ahead of the color. Near 1 the whole prop brightens at once with only a lean to one side, which reads as a fade, not a wipe.
- `band`: at 0 the prop fills behind the edge. Above 0 only a bar that wide (a share of the prop) is lit: it enters, crosses, and leaves, and nothing stays lit behind it. 0.1 to 0.25 is a bar, and it only reaches full brightness when it is wider than `softness`. With `onOff` the bar crosses and comes back; with `off` it crosses backward, from the end of the sweep to its start. A `ramp` curve on it widens the bar as it travels.

## When to use it

- A change of section: the new color wiped over the old look, on a layer above it.
- A reveal: Wipe above another effect, with blend `reveal`, and that effect appears behind the edge.
- Taking a look away: `off`, or the second half of `onOff`, uncovers whatever is under it.
- A shine: a thin white `band` with blend `add` crossing a colored base.
- A scanner: a `band` with `onOff`, there and back.
- `stage_cue` with `sweep` places this effect for you: `onOff`, `duration` 50, `softness` 0.3, white unless you give colors, on a group's row with `perPreview`, or timed prop by prop so it still crosses the yard. Use the cue to mark a section change. Place Wipe yourself for anything more particular.
- Choosing: `fill` only moves with a curve, but can stop anywhere and follow the music; Wipe is the one-off move. `colorShift` with `colorShift.stagger` changes a prop that is already lit from one color to another; Wipe brings color onto a dark prop or takes it off. `chase` and `bars` keep going round, while a Wipe happens once an effect: for a sweep every bar, put one on each mark with `place_effects` on the Bars track.
- Flash safety: a very short Wipe with a soft edge over the whole house is in effect a flash. One is fine. A run of them faster than about three a second across a large part of the display is not.

## Looks

```look
{"name": "New color sweeping in", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Red crossing from the left behind a feathered edge, then holding.",
 "effect": {"kind": "wipe", "settings": {"direction": "leftToRight", "mode": "on", "duration": 40, "softness": 0.15}, "colors": ["#ff0000"]}}
```

```look
{"name": "Soft white sweep that clears behind itself", "suits": ["string", "arch", "matrix", "house"],
 "note": "White washing across with a long glowing edge, then wiping off the same way: a section change over the look below.",
 "effect": {"kind": "wipe", "settings": {"mode": "onOff", "duration": 50, "softness": 0.3}, "colors": ["#ffffff"]}}
```

```look
{"name": "Curtain rising, green to white", "suits": ["matrix", "tree", "house"],
 "note": "Color climbing from the ground, green at the bottom paling to white at the top, then holding.",
 "effect": {"kind": "wipe", "settings": {"direction": "up", "duration": 60}, "colors": ["#00a030", "#ffffff"]}}
```

```look
{"name": "Shine crossing a colored prop", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "A narrow bar of white travelling across over the whole effect, added to the color under it.",
 "effect": {"kind": "wipe", "settings": {"duration": 100, "softness": 0.1, "band": 0.15}, "colors": ["#ffffff"], "blend": "add"}}
```

```look
{"name": "Gold bursting from the middle", "suits": ["matrix", "house"],
 "note": "A circle of gold growing from the center until everything is lit, white at the far corners.",
 "effect": {"kind": "wipe", "settings": {"direction": "radial", "duration": 50, "softness": 0.2}, "colors": ["#ffc040", "#ffffff"]}}
```
