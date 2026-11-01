# Moving Head

Points moving heads and moves them: held still, or sweeping, circling, tracing a figure eight, waving, or wandering, fanned out across a group of heads and lit in the palette's colors.

## What it draws

No pixels. On a moving head (a DMX fixture with pan and tilt) it sets where the head points and the light it gives, for as long as it plays; the previews draw its beam. Positions are degrees from Home, where the head faces the street and points straight up. The limits the show's owner set for each head always hold, so it never points where it shouldn't.

- On one head: it points the head and moves it about that position.
- On a group of heads: each head takes the same settings, in member order. `fanPan` and `fanTilt` spread them out evenly about the middle head; `delay` runs each a moment behind the one before; `fans` and `mirror` split them into matching fans or mirrored halves.
- On the whole house, or any group with floods and pixels in it: only the heads move and light. Nothing else changes.
- On a string, an arch, a matrix, or a tree it does nothing: they have no motors.
- Its light replaces whatever other effects draw on the head, unless `color` is `drawn`. Where it and a `dmx` effect set the same channel, the later row or the higher layer wins.
- When it ends, the head goes back to where it rests (Home, on most) and its light to whatever is drawn on it.
- It draws no pixels, so `brightness`, `sparkles`, `blur`, render styles, and turns do nothing to it. Fades dim its light and never move the head.

## Settings

- `pan` and `tilt`: where it points, in degrees from Home. `tilt` leans the beam from straight up toward the street (+) or back over the house (-): 30 to 60 leans it out over the street, 90 points it level at the street, and past 90 it points down at the ground in front, for a head mounted up high. `pan` turns the way it leans, + to the right seen from the street: 90 leans it right, -90 left, 180 back over the house. At tilt 0 the beam points straight up, and pan changes nothing anyone can see.
- `pattern`: how it moves about the pan and tilt. `still` holds there. `sweep` goes back and forth along a line, `width` of pan and `height` of tilt at once (height 0 for pan alone; the default 45 slants it). `circle` goes round, an oval when width and height differ. `eight` traces a figure eight lying on its side: across once while rising and falling twice. `wave` goes across and back on a wavy line that rises and falls three times each round. `wander` drifts on a smooth random path, each head its own.
- `width` and `height`: how far the movement goes, end to end, in degrees: width 90 swings 45 each way. Width moves the pan and height the tilt, so what the street sees depends on where the head points. Leaning out (tilt 30 to 60), width swings the beam side to side and height dips and lifts it. At pan 90 it is the other way round: height swings the beam from side to side across the house front.
- `speed`: times round the movement each second (for a sweep, there and back). 0.1 to 0.3 is slow and stately; 0.5 to 1 is lively; past 2 the heads race, and real heads may not keep up. A curve on it speeds the movement up or slows it down smoothly.
- `fanPan` and `fanTilt`: degrees between neighboring heads of a group, spread evenly about the middle. With the heads leaning out, `fanPan` 20 to 40 opens them into a fan. At pan 90, `fanTilt` fans them across the house front like a peacock's tail. Negative turns the fan the other way: with the group's heads listed left to right, the beams cross.
- `delay`: seconds each head runs behind the one before it in its fan. 0 moves them together; 0.1 to 0.3 makes a ripple down the line. A head running behind waits where the movement starts.
- `fans`: splits a group's heads, in member order, into this many fans side by side that move alike: with 8 heads and 2 fans, heads 1 and 5 match.
- `mirror`: the right half of each fan turns its pan the other way, so the halves move as mirror images. With a `sweep` they swing in and out together, like wings; held at `pan` 30 and leaning out, the halves lean toward each other and the beams cross.
- `color`: where its light's color comes from. `first` is the first palette color. `wash` goes through the palette's colors over the effect. `perHead` gives each head the next palette color, in member order. `drawn` leaves the light to the effects drawn on the head (an `on`, a `pulse`, a `colorWash` on its row), so this effect only moves it. A head with a color wheel turns the wheel to the nearest color it has.
- `dimmer`: how bright its light is, 0 to 100. This, not `brightness`, sets it; a curve swells it up or down.
- `strobe`: flashes the head this many times a second, 1 to 20, through its strobe channel: the effect sets the channel to the value that flashes at that pace, by what the head's strobe values do. 0 opens the shutter for a steady light. A head with no strobe channel flashes its light instead. Either way the previews show the flashes.
- `seed`: picks the paths `wander` takes. Another seed, another set of paths.

Curves work on every number here; `fans` and `seed` take whole numbers, so a curve on them jumps from one to the next.

## When to use it

- Beams moving over the house give a show scale: a sweep or a wander reads from far down the street, above everything else.
- Where beams cross, their light adds up, as it does in haze: heads of one color crossing make a bright knot of that color in the sky; different colors mix as light does.
- Slow movement looks grand; fast movement looks frantic. Real heads take a second or more to swing right round, so a big jump on a short beat may not arrive in time.
- Moves that land on the music: one `still` effect per beat or bar, each at its own pan and tilt, snaps the heads from place to place. Curves on `pan` and `tilt` glide them instead.
- Light that follows the music: `color` `drawn`, with a `pulse`, a `fade`, or a `colorWash` on the heads' row. This effect moves them; the other lights them.
- Color that changes as they move: `wash` for a slow change, `perHead` for a rainbow of beams.
- A beam fading up where it points: `fadeInMs`.
- Keep beams up, out of drivers' and passers-by's eyes: tilt near 90 points level at the street. A beam flashing is a real strobe, so `strobe` keeps to topic `safety`.

## Looks

```look
{"name": "Sky sweep", "suits": ["house"],
 "note": "Every head leaning out over the street and sweeping side to side, fanned 25 degrees apart, rippling down the line.",
 "effect": {"kind": "movingHead", "settings": {"tilt": 40, "pattern": "sweep", "width": 100, "height": 0, "speed": 0.25, "fanPan": 25, "delay": 0.2}, "colors": ["#ffffff"]}}
```

```look
{"name": "Peacock fan", "suits": ["house"],
 "note": "The beams start together, straight up, and open into a fan across the house front.",
 "effect": {"kind": "movingHead", "settings": {"pan": 90}, "colors": ["#ffd700"],
            "curves": {"fanTilt": {"shape": "ramp", "from": 0, "to": 25}}}}
```

```look
{"name": "Searchlights", "suits": ["house"],
 "note": "Each head drifting slowly on its own path over the street, in cold blue.",
 "effect": {"kind": "movingHead", "settings": {"tilt": 35, "pattern": "wander", "width": 140, "height": 40, "speed": 0.1}, "colors": ["#4060ff"]}}
```

```look
{"name": "Rainbow circles", "suits": ["house"],
 "note": "Every head circling, each in the next palette color.",
 "effect": {"kind": "movingHead", "settings": {"tilt": 30, "pattern": "circle", "width": 60, "height": 30, "speed": 0.5, "color": "perHead"}, "colors": ["#ff0000", "#00ff00", "#0000ff", "#ffff00"]}}
```

```look
{"name": "Crossing wings", "suits": ["house"],
 "note": "The two halves swinging in to cross over the middle and out again, washing from red to blue.",
 "effect": {"kind": "movingHead", "settings": {"tilt": 45, "pattern": "sweep", "width": 80, "height": 0, "speed": 0.3, "mirror": true, "color": "wash"}, "colors": ["#ff2a2a", "#2a6bff"]}}
```

```look
{"name": "Figure eights, lit from below", "suits": ["house"],
 "note": "Heads tracing slow figure eights, fanned out; their light comes from what is drawn on them, a pulse on the beat, say.",
 "effect": {"kind": "movingHead", "settings": {"tilt": 40, "pattern": "eight", "width": 70, "height": 30, "speed": 0.2, "fanPan": 20, "color": "drawn"}}}
```
