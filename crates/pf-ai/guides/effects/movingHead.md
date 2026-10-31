# Moving Head

Points moving heads and moves them: a still position, a sweep, a circle, a figure eight, a wave, or a random wander, fanned out across a group of heads, lit in the palette's colors.

## What it draws

No pixels. On a moving head (a DMX fixture with pan and tilt) it sets where the head points and what light it gives, for as long as it plays; the previews draw the head's beam. Positions are degrees from Home: Home is the head facing the street, pointing straight up. Each head's own limits keep it where its owner set it up to point.

- On one head: it points that head, and moves it about that position.
- On a group of heads: each head takes the same settings, fanned out in member order. `fanPan` and `fanTilt` spread them apart, evenly about the middle head; `delay` makes each head run a moment behind the one before, so a sweep ripples down the line.
- On the whole house, or a group with floods and strings in it: only the heads move; everything else is left alone.
- On a string, an arch, a matrix, or a tree it does nothing: they have no motors.

## Settings

- `pan` and `tilt`: where it points, in degrees from Home. Pan turns it round (+ to the right, seen from the street); tilt leans it (+ toward the street, - toward the house). Tilt 90 points at the street; tilt 45 lights the lawn in front. Curves on them make any path of your own: a "ramp" on `pan` from -90 to 90 sweeps once across.
- `pattern`: how it moves about the pan and tilt. `still` holds there. `sweep` goes back and forth along a line (width across, height up and down; height 0 is a flat sweep). `circle` goes round (an oval when width and height differ). `eight` traces a figure eight. `wave` sweeps across, rising and falling three times as it goes. `wander` drifts on a smooth random path, each head its own; `seed` picks the paths.
- `width` and `height`: how far the movement goes, end to end, in degrees.
- `speed`: times round the movement each second. 0.1 to 0.3 is a slow, stately move; 1 is brisk; above 2 the heads race, and real heads may not keep up. A curve on it speeds the movement up and slows it down smoothly.
- `fanPan` and `fanTilt`: degrees between neighboring heads of a group. 20 to 40 opens the beams into a fan; negative turns them inward, crossing.
- `delay`: seconds each head runs behind the one before. 0 moves them together; 0.1 to 0.3 makes a ripple.
- `fans`: splits a group's heads into this many fans side by side that move alike: with 8 heads and 2, heads 1 and 5 match.
- `mirror`: the right half of each fan points and moves as a mirror image of the left (pan turned the other way), for symmetrical looks: with `pan` 30, the left half points right and the right half left, crossing.
- `color`: where its light's color comes from. `first` is the first palette color; `wash` goes through the palette's colors over the effect; `perHead` gives each head the next palette color; `drawn` leaves the color to the effects on the layers below, so the effect only moves the head.
- `dimmer`: how bright, 0 to 100. A curve swells it up or down.
- `strobe`: flashes per second, 0 for steady. Mind flash safety: keep it short and under 3 flashes a second for long stretches.

## When to use it

- Beams that sweep the sky over the house on a big chorus, fanned wide.
- A slow wander of searchlights through a quiet verse.
- Heads snapping to a new position on each beat: one effect per beat, each `still` at its own pan and tilt.
- Beams crossing and uncrossing: `fanPan` from positive to negative with a curve.
- Color changes ride along: `wash` for a slow change, `perHead` for a rainbow of beams.
- Fades dim the light; they never move the head.

## Looks

```look
{"name": "Sky sweep", "suits": ["house"],
 "note": "Every head sweeping side to side, fanned out 30 degrees apart, rippling down the line.",
 "effect": {"kind": "movingHead", "settings": {"tilt": 20, "pattern": "sweep", "width": 120, "height": 0, "speed": 0.25, "fanPan": 30, "delay": 0.2}, "colors": ["#ffffff"]}}
```

```look
{"name": "Searchlights", "suits": ["house"],
 "note": "Each head wandering slowly on its own path, high over the house, in cold blue.",
 "effect": {"kind": "movingHead", "settings": {"tilt": 25, "pattern": "wander", "width": 140, "height": 40, "speed": 0.1}, "colors": ["#4060ff"]}}
```

```look
{"name": "Rainbow circles", "suits": ["house"],
 "note": "Every head circling, each in the next palette color.",
 "effect": {"kind": "movingHead", "settings": {"tilt": 30, "pattern": "circle", "width": 60, "height": 30, "speed": 0.5, "color": "perHead"}, "colors": ["#ff0000", "#00ff00", "#0000ff", "#ffff00"]}}
```
