# Strobe

Random pixels popping on at full brightness, a fresh handful with every flash: camera flashes and crackle, hard-edged where Twinkle is soft.

## What it draws

Time is cut into `rate` slots a second. For the first half of each slot a random share of the pixels (`density`) is lit at full brightness, each in a palette color picked at random. For the second half everything is see-through. The next slot lights a different random set.

Which pixels light is random, but when they light is not: the whole set comes on together and goes dark together. So the prop as a whole blinks `rate` times a second, and `density` only says how much of it takes part.

- On a string or an arch: a few points pop at once along the run (six on a 30-pixel arch at the defaults), then all dark, then another few somewhere else.
- On a matrix: salt thrown across the panel and thrown again with each flash. On a tree: the same, all over the cone.
- On the whole house (a group): the scatter covers every prop, and all of it flashes in the same instant.
- Slow (a `rate` of 1 to 3) each flash is a pop you can see, lasting half its slot: at 2 that is a quarter second lit and a quarter second dark, which reads as cameras going off in a crowd. At the default of 10 it is a hard crackle. A sequence usually draws a frame every 25 ms, so 20 a second is the fastest real strobe, one frame on and one off; above that it turns uneven, and can stop going dark at all and become static.
- At a `rate` of 0 one random set stays lit and never changes: a still field of points at full brightness.
- Over another layer the pops sit on top of the look below, which shows everywhere else and through the dark half.

## Settings

- `rate`: flashes a second, counted from the start of the effect. The song's beats a minute divided by 60 puts one flash on each beat (2 at 120 beats a minute) when the effect starts on a beat.
- `density`: the share of pixels in each flash. 0.03 to 0.1 is a few camera flashes; 0.2 is a crackle; from 0.5 up more than half the prop lights each time and it is close to a whole-prop blink. A `ramp` curve on `density` thickens the pops through a build.
- A curve on `rate` runs faster than its numbers say (the effect works out where it is as the time so far multiplied by the rate now), so a `ramp` ends at nearly twice its `to`. Curve `density` instead.

## When to use it

- Flash safety: all the lit pixels flash in step, so `rate` is the flash rate whatever the `density`. Keep `rate` at 3 or under on the whole house or any large prop. The default of 10 is over that. A low `density` softens a flash but does not slow it.
- Fast strobe is for a burst: half a beat to a beat, on outlines or a few small props, at the one or two biggest hits of a song. `stage_cue` with `hit` adds exactly that (20 a second at a `density` of 0.5, in white, on outlines, windows, and arches) when its `intensity` is 0.85 or more, so let the cue do it.
- Place Strobe yourself for what the cue doesn't do: slow pops on the beat through a whole section, a color other than white, a lighter scatter, or a build.
- Over a colored base it adds bright points without hiding the color.
- When a chorus wants crackle across the whole house for bars at a time, use `twinkle` at a high `twinkle.rate` instead: its pixels are never in step, so nothing flashes as a whole.
- For the whole prop blinking use `shimmer`. For glints added to another effect, `sparkles` (topic `effect-wide`).
- Keep a group on its own layout: with a per-model render style every member draws the same scatter.

## Looks

```look
{"name": "Camera flashes on the beat", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "A few white points popping on each beat at 120 beats a minute, somewhere new every time. Set the rate to the song's beats a second and start on a beat.",
 "effect": {"kind": "strobe", "settings": {"rate": 2, "density": 0.08}, "colors": ["#ffffff"]}}
```

```look
{"name": "Christmas confetti pops", "suits": ["matrix", "tree", "house"],
 "note": "A quarter of the pixels popping in red, green, and gold on every beat at 120 beats a minute.",
 "effect": {"kind": "strobe", "settings": {"rate": 2, "density": 0.25}, "colors": ["#ff2020", "#00c040", "#ffc040"]}}
```

```look
{"name": "Still field of stars", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "A scatter of white and ice blue points that holds perfectly still: a night sky to put other things over.",
 "effect": {"kind": "strobe", "settings": {"rate": 0, "density": 0.2}, "colors": ["#ffffff", "#9fd0ff"]}}
```

```look
{"name": "Pops thickening into a hit", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "A pop on every beat at 120 beats a minute, a few points at first and over half the prop by the end: lay it across the bars before a hit.",
 "effect": {"kind": "strobe", "settings": {"rate": 2}, "colors": ["#ffffff"], "curves": {"density": {"shape": "ramp", "from": 0.03, "to": 0.6}}}}
```
