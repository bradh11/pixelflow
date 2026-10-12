# Shimmer

The whole prop switching on and off as one at a steady rate, stepping through the palette as it goes: a blink on the beat when slow, a harsh flicker when fast.

## What it draws

Every pixel of the target does the same thing: lit in one flat color for part of each cycle, see-through for the rest. Each new cycle takes the next palette color. Nothing moves and there is no pattern.

- On a string, an arch, a matrix, or a tree: the whole prop blinks as a block. A matrix is a solid rectangle going on and off.
- On the whole house (a group): every prop on and off in the same instant. That is the biggest flash a display can make, which is why its `rate` needs care.
- With two or more colors it goes red, dark, green, dark. With `duty` at 1 there is no dark: the prop snaps straight from one color to the next.
- Over another layer the dark part shows the layer below, so it flips between the look underneath and its own color.
- Slow (a `rate` of 1 to 3) it is a blink you can count. Fast (the default is 10) it is a hard buzz. A sequence usually draws a frame every 25 ms, so 20 a second is the fastest it can truly flicker, one frame on and one off. Above that the frames can't follow, and it stutters unevenly or stops flickering altogether.

## Settings

- `rate`: flashes a second, counted from the start of the effect. The song's beats a minute divided by 60 gives one flash a beat (2 at 120 beats a minute) when the effect starts on a beat; half that lights every other beat.
- `duty`: the share of each cycle spent lit. 0.5 is even. 0.1 to 0.2 is a short blip with a long dark gap. 0.8 is mostly on with a quick blink off. 1 never goes dark: a steady light with one color, hard color changes with several.
- A curve on `rate` runs faster than its numbers say (the effect works out where it is as the time so far multiplied by the rate now), so a `ramp` ends at nearly twice its `to`. Put the curve on `duty` or `brightness` instead.

## When to use it

- Flash safety comes first with this effect: `rate` is exactly how many times a second the whole target flashes. Keep it at 3 or under on anything large: the whole house, a big group, a mega tree, a matrix. The default of 10 is far over that. Higher rates belong on one small prop (a star, a single arch), for a beat or two, with `brightness` held down.
- A blink on the beat: `rate` at the song's beats a second, two colors taking turns, started on a beat.
- A color change on every beat: `duty` at 1 with two or three colors. The prop snaps from color to color with no dark between, which is kinder than on and off and reads from across the street.
- A blink over a look: on a layer above a colored base it swaps the base for its own color and back, so nothing goes black.
- Shimmer keeps its own time, not the song's. Over a long section, or when the tempo wanders, use `pulse` on the "Beats" track with `pulse.shape` set to `square`: the same blink, locked to the marks.
- For flicker without a flash use `twinkle`. For random pixels popping, `strobe`. For one flash that dies away, `impact`.
- `on.shimmer` is a different thing: it buzzes a steady prop every other frame.

## Looks

```look
{"name": "Blink on the beat, red then green", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "The whole prop lit for the first half of each beat at 120 beats a minute, red and green taking turns. Set the rate to the song's beats a second and start on a beat.",
 "effect": {"kind": "shimmer", "settings": {"rate": 2, "duty": 0.5}, "colors": ["#ff0000", "#00c040"]}}
```

```look
{"name": "Color snap on every beat", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Never dark: red, white, green, each held for one beat at 120 beats a minute.",
 "effect": {"kind": "shimmer", "settings": {"rate": 2, "duty": 1}, "colors": ["#ff0000", "#ffffff", "#00c040"]}}
```

```look
{"name": "Short white blips", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "A quick white blink at the start of each beat with a long gap after it: for a top layer, where the look below shows in the gaps.",
 "effect": {"kind": "shimmer", "settings": {"rate": 2, "duty": 0.15}, "colors": ["#ffffff"]}}
```

```look
{"name": "Gold and pale gold taking turns", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Two close warm colors swapping three times a second with no dark: a gentle flicker that is safe on a large prop.",
 "effect": {"kind": "shimmer", "settings": {"rate": 3, "duty": 1}, "colors": ["#ffb030", "#ffe2a8"]}}
```
