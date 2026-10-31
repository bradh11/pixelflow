# Servo

Turns servos: holds one at a position, or sweeps, waves or steps it back and forth, fanned out across several.

## What it draws

No pixels and no light. On a DMX servo fixture (one with one or more servos) it sets where each servo turns, for as long as it plays; the previews draw each servo as an arm that turns. Positions are degrees from Home, where the arm points straight up; each servo's own limits keep it where its owner set it up to turn.

- On one servo fixture: every servo in it turns (or only the one `servo` names), fanned out in order.
- On a group: every servo of every fixture in the group, in member order.
- On anything else it does nothing: only servos turn.

## Settings

- `position`: where it turns to, in degrees from Home: + turns it clockwise, seen from the street. A curve on it makes any movement of your own: a "ramp" from -60 to 60 turns it once across.
- `pattern`: how it moves about the position. `still` holds there. `sweep` goes back and forth at a steady pace, like a wiper. `wave` goes back and forth easing at each end, like a wave of a hand. `steps` jumps between `steps` even places from one end to the other, then back to the first: a ticking, marching movement.
- `size`: how far the movement goes, end to end, in degrees.
- `speed`: times back and forth each second. Servos are slow: 0.5 to 2 is about right.
- `steps`: how many places `steps` jumps between.
- `fan`: degrees between neighboring servos, spread evenly about the middle one.
- `delay`: seconds each servo runs behind the one before: 0 moves them together; 0.1 to 0.3 makes a ripple.
- `servo`: only this servo of each fixture (1 is its first); 0 turns every one.

## When to use it

- Props that move: a waving arm, a nodding head, a wagging tail, a flag, a gate. `wave` suits anything alive; `sweep` anything mechanical.
- Something turning to face a moment: one effect `still` at a new position on the beat.
- A row of servos rippling: `delay` 0.15 with `wave`.

## Looks

```look
{"name": "Friendly wave", "suits": ["house"],
 "note": "Each servo waving back and forth 60 degrees, about once a second.",
 "effect": {"kind": "servo", "settings": {"pattern": "wave", "size": 60, "speed": 1}}}
```

```look
{"name": "Ticking march", "suits": ["house"],
 "note": "Servos stepping across in four jumps, one a beat behind the next.",
 "effect": {"kind": "servo", "settings": {"pattern": "steps", "size": 90, "steps": 4, "speed": 0.5, "delay": 0.25}}}
```
