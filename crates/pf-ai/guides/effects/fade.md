# Fade

One color rising from nothing to full, or dying from full to nothing, across the whole length of the effect: a swell or a decay placed like a note.

## What it draws

Every pixel in the first palette color, at a brightness that runs in a straight line from one end of the effect to the other. There is no shape and nothing moves, so it looks the same on a string, an arch, a matrix, and a tree: the whole prop brightening or dimming together. On the whole house (a group) everything swells or dies as one.

- It never holds. Half way through it is at half, so a long fade in spends its first third looking nearly dark.
- It is see-through by as much as it is dim. Over an empty layer that reads as fading up from black or down to it. Over another effect it is a crossfade: a red fade in over a green prop goes green, then brown, then red, and a fade out uncovers what is under it.
- Only the first color is used. Any others are ignored.
- Short (a quarter to half a second) and falling, it is a flash with a tail: a hit. Long (a bar or more) and rising, it is a swell.

## Settings

- `direction`: `in` starts at nothing and reaches full on the last frame; pick it for a swell into a downbeat, or to bring a new color up over the old. `out` starts full and reaches nothing at the end; pick it for a hit that dies away or a last chord.

That is the only setting: the length of the effect is the length of the fade. Adding `fadeOutMs` to an `out` makes the tail fall away faster, because the two fades multiply.

## When to use it

- A hit: white, `out`, a beat or less long, on a layer above the section's look with blend `add`. The prop flashes toward white and settles back into its own color.
- A pulse on the beat: one `out` on every mark, with `place_effects` on the Beats track. A `spread` of `alternate` sends the pulse back and forth between neighbouring props.
- A swell: `in` across the bar before a drop, ending exactly on it.
- A crossfade: `in` in the new color on the layer above the old look.
- Dimming a look: a black `in` on the layer above sinks whatever is under it to dark by the end, and a black `out` starts dark and gives it back.
- Choosing: `on` with `on.startLevel` at 0 and `on.endLevel` at 1 draws the same rise, and can also stop part way, start part way, or carry a gradient. To soften the ends of some other effect, use that effect's own `fadeInMs` and `fadeOutMs`, not a Fade beside it. To hold a color use `on`; to change hue instead of brightness, `colorWash` or `colorShift`; for color that arrives from one side, `wipe`.
- Flash safety: a Fade on every beat at 120 beats a minute is two flashes a second, which is fine. One on every half beat is four, which is too fast for the whole house: nothing should flash faster than about three times a second across a large part of the display.

## Looks

```look
{"name": "White hit that dies away", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Full white on the first frame and gone by the end, added over whatever is under it. Start it on the hit and make it a beat or less long.",
 "effect": {"kind": "fade", "settings": {"direction": "out"}, "colors": ["#ffffff"], "blend": "add"}}
```

```look
{"name": "Swell into a downbeat", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Gold rising from nothing to full. End it on the beat the next section starts.",
 "effect": {"kind": "fade", "settings": {"direction": "in"}, "colors": ["#ffc040"]}}
```

```look
{"name": "Last chord falling away", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Warm white sinking to nothing, quicker toward the end. Lay it across the final held chord.",
 "effect": {"kind": "fade", "settings": {"direction": "out"}, "colors": ["#ffc880"], "fadeOutMs": 1500}}
```
