# Servo

Turns servos: holds them at a position, or sweeps, waves, or steps them back and forth, fanned out across several.

## What it draws

No pixels and no light. On a DMX servo fixture (a prop of one servo or several) it sets where each servo turns, for as long as it plays; the previews draw each servo as an arm. Positions are degrees from Home, where the arm points straight up. The limits the show's owner set for each servo always hold; a usual servo turns 90 degrees each way.

- On one servo fixture: every servo in it turns (or only the one `servo` names), fanned out in order.
- On a group: every servo of every fixture in it, in member order, fanned and delayed across them all.
- On the whole house: only the servos turn. Lights and everything else are left alone.
- On a string, an arch, a matrix, or a tree it does nothing: they have no servos.
- Where it and a `dmx` effect set the same channel, the later row or the higher layer wins.
- When it ends, each servo goes back to where it rests (Home, on most).
- It has no light, so fades, `brightness`, and the other settings every effect shares do nothing to it.

## Settings

- `position`: where it turns to, in degrees from Home, + clockwise as seen from the street: 90 points the arm right, -90 left. A curve on it makes any movement of your own: a "ramp" from -60 to 60 turns it once across.
- `pattern`: how it moves about the position. `still` holds there. `sweep` goes back and forth at a steady pace, like a wiper. `wave` goes back and forth easing at each end, like a hand waving. `steps` jumps between even places from one end to the other, then back to the first: a ticking, marching movement.
- `size`: how far the movement goes, end to end, in degrees: 60 swings 30 each way.
- `speed`: times through the movement each second: there and back for `sweep` and `wave`, every place once for `steps` (0.5 with 4 steps holds each for half a second). Servos are slow: 0.5 to 2 is about right. A curve on it speeds the movement up or slows it down smoothly.
- `steps`: how many places `steps` jumps between; 2 flips between two, a tick-tock.
- `fan`: degrees between neighboring servos, spread evenly about the middle one. 20 opens a row of arms like fingers.
- `delay`: seconds each servo runs behind the one before. 0 moves them together; 0.1 to 0.3 makes a ripple down the row.
- `servo`: only this servo of each fixture (1 is its first), for a prop whose servos do different things, an arm and a head; 0 turns every one.

Curves work on every number here; `steps` and `servo` take whole numbers, so a curve on them jumps from one to the next.

## When to use it

- Props that move: a waving arm, a nodding head, a wagging tail, a flag, a gate, a jaw. `wave` looks alive; `sweep` looks mechanical; `steps` looks like a clock or a march.
- Something turning to face a moment: one effect `still` at a new position on the beat, or a curve on `position` for a slow turn.
- A row of servos rippling: `delay` 0.15 with `wave`.
- The servo moves the thing; the effects on that prop's own pixels light it.
- Fast, wide moves are hard on small servos: a smaller `size` reads as well at speed.

## Looks

```look
{"name": "Friendly wave", "suits": ["house"],
 "note": "Each servo waving back and forth 60 degrees, about once a second.",
 "effect": {"kind": "servo", "settings": {"pattern": "wave", "size": 60, "speed": 1}}}
```

```look
{"name": "Ticking march", "suits": ["house"],
 "note": "Servos stepping across in four jumps, each a quarter of a second behind the one before.",
 "effect": {"kind": "servo", "settings": {"pattern": "steps", "size": 90, "steps": 4, "speed": 0.5, "delay": 0.25}}}
```

```look
{"name": "Slow turn to look", "suits": ["house"],
 "note": "Each servo turning slowly from left to right across the effect, like a head following something past.",
 "effect": {"kind": "servo", "curves": {"position": {"shape": "ramp", "from": -45, "to": 45}}}}
```
