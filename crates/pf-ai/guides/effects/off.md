# Off

Black that hides whatever is under it: how to make a prop go dark while its group carries on.

## What it draws

Nothing lit: every pixel of the target black, covering the layers below it on its row and, on a prop's own row, the group rows that would otherwise show there.

- On a string, an arch, a matrix, or a tree, the prop goes dark for as long as the effect lasts, whatever its group is doing.
- On the whole house (a group's row) it darkens every member that has nothing of its own playing on a later row.
- With fades it is a dip: the look under it sinks to black over `fadeInMs` and comes back over `fadeOutMs`.

An empty stretch of a row is not the same thing. An empty row is see-through, so the group's look shows; Off is a black cover.

## Settings

Off has no settings of its own. Its fades are what shape it.

## When to use it

- Dark space on purpose: taking the arches out for a verse while the rest of the house plays on the group's row.
- A dip before a hit: half a beat of Off on a top layer, then the hit. `stage_cue` with `blackout` or `hit` does this for you across the right props.
- Blinking one prop out of a group look on the beat: short Offs on the prop's row with `place_effects` on the Beats track and a `spread` of `alternate`.
- Don't use it to end a sequence or to leave a gap: just stop the effects, or fade them out. Don't put it on the bottom layer of a group's row under nothing: an empty row is already dark.

## Looks

```look
{"name": "Dip to black", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "Whatever is under it sinks to black and comes back, a quarter of a second each way.",
 "effect": {"kind": "off", "fadeInMs": 250, "fadeOutMs": 250}}
```
