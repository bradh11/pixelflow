# Impact

One full-brightness flash at the start of the effect that dies away over its length: the hit for a drop, a crash, or a downbeat.

## What it draws

The whole target lit at full brightness on the first frame, then fading to nothing by the end of the effect. The effect's length is the fade's length. Nothing moves unless `bloom` is on.

- On a string, an arch, a matrix, or a tree: the entire prop in one flat color, dimming.
- On the whole house (a group): everything flashes together and fades together.
- With `bloom` the light starts at a point and spreads as a soft-edged disc. On a matrix it is an oval growing from the middle. On a tree it starts halfway up one side and spreads up, down, and round the trunk. On a string it runs from the middle to both ends, and on an arch from the top down both legs. On the whole house it starts mid-layout (on the matrix, in the frames) and reaches the tree and the far arches last. With `renderStyle` set to `perModelDefault` every prop of a group blooms from its own middle instead.
- It is see-through as it fades. Over another layer the look below comes back up through it; on an empty row it fades to black.
- Short, a beat long, it is a stab. A bar or two long, it is a boom with a tail. With the default decay most of the light goes early: half is gone after about a sixth of the length and nine tenths by halfway, so a four-second Impact reads as a one-second flash and a long faint afterglow.

## Settings

- `decay`: how it fades. `exponential` (the default) drops fast, then lingers: the natural flash. `linear` fades evenly and stays visible to the end: a long, deliberate fall. `punch` drops to about a third a tenth of the way in, bounces back to three quarters by a quarter of the way, then eases out, which reads as the hit and an aftershock.
- `color`: `white` (the default) hits in white whatever the palette. `palette` hits in the first palette color.
- `colorShift`: the color changes as it fades: from white through each palette color in turn, or with `palette` from the first color through the rest. Use `linear` with it. With `exponential` the later colors arrive when it is already nearly dark.
- `hold`: time at full brightness before the fade begins. 0 is a sharp hit; 100 to 300 ms gives it weight. The fade takes whatever is left of the effect.
- `bloom`: time the hit takes to spread from its point; 0 lights everything at once. 100 to 250 ms still lands as a hit. It fades while it spreads, so a long bloom reaches the edges already dim: give it a `hold` at least as long when the spread should arrive at full brightness.
- `centerX` and `centerY`: where the bloom starts, across from the left and up from the bottom. They do nothing without `bloom`. `centerY` at 0 rises from the foot of a tree or the bottom of the yard; at 100 it falls from the top. `centerX` at 0 or 100 sweeps in from one side: on a group of arches it lights them one after another. On a string or an arch only `centerX` is useful; leave `centerY` at 50 there or the hit arrives late.

## When to use it

- `stage_cue` with `hit` builds Impacts for you across the right props: a `punch` one to two beats long, half a beat of dark before it, a short `bloom` on matrices, in the cue's colors. Use the cue for a song's moments.
- Place Impact yourself when you want what the cue doesn't do: a long tail (a bar or more), a `linear` fade or a `hold`, a bloom from a chosen point, no dip before it, a hit on a few props only, or a hit on every bar with `place_effects` on the "Bars" track.
- Put it on a layer above the section's look: it covers the look at the hit and the look returns as it fades.
- A hit lands harder out of dark: a half beat of `off` before it.
- Flash safety: each Impact is one flash, and a `punch` counts as two. Across a large part of the display keep hits at least a third of a second apart: one a beat is fine up to about 180 beats a minute, one every half beat is not. Keep `punch` for single hits a beat or more long, not for a run of them.
- The exact moment is the start of the effect, so start it on the beat with no `fadeInMs`.
- For brightness that keeps pulsing use `pulse`. For crackle on top of the biggest hits, a beat of `lightning` or `strobe` on the layer above.
- Avoid long white Impacts stacked close together: the tails pile up and wash the look out.

## Looks

```look
{"name": "White hit with a long tail", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Everything white at once, falling away fast and lingering faintly. Make it two to four beats long.",
 "effect": {"kind": "impact", "settings": {"decay": "exponential", "hold": 60}}}
```

```look
{"name": "Punch in the section's color", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "A red hit that dips and bounces back before it fades: one to two beats long, for a single accent.",
 "effect": {"kind": "impact", "settings": {"decay": "punch", "color": "palette"}, "colors": ["#ff2020"]}}
```

```look
{"name": "Cooling from white through gold to red", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "A white flash that turns gold, then red, as it fades evenly: a bar long, like a firework dying.",
 "effect": {"kind": "impact", "settings": {"decay": "linear", "colorShift": true}, "colors": ["#ffc040", "#ff2020"]}}
```

```look
{"name": "Burst from the middle", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Ice blue light spreading from the center in a fifth of a second, held until it has arrived, then fading.",
 "effect": {"kind": "impact", "settings": {"color": "palette", "bloom": 200, "hold": 200}, "colors": ["#9fd0ff"]}}
```

```look
{"name": "Rising from the ground", "suits": ["matrix", "tree", "house"],
 "note": "Warm white light climbing from the bottom over half a second, then fading evenly: two bars long, for a swell into a chorus.",
 "effect": {"kind": "impact", "settings": {"decay": "linear", "color": "palette", "bloom": 500, "hold": 500, "centerY": 0}, "colors": ["#ffd9a0"]}}
```
