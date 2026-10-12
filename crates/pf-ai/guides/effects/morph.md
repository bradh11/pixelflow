# Morph

A line that sweeps once from one place to another over the effect's length, a bright head with a fading tail behind it: wipes, fills, and comets with a set start and finish.

## What it draws

One sweep per effect; it never loops. A line travels from where you say it starts to where you say it ends, lighting what it passes. The head arrives part-way through the effect, and the tail then fades out behind it, from the starting side first, until the effect ends. How fast it moves comes from the effect's length: on a half-second effect it is a swipe, on four seconds a slow fill.

- On a matrix, as it comes: a line as wide as the matrix rises from the bottom edge to the top in the first fifth of the effect, filling in below itself; then the fill fades away from the bottom up.
- On a tree the same thing is a ring climbing the tree with the cone lit beneath it, then draining upward.
- A string or an arch is one row tall, so the usual upward sweep has nowhere to go: the whole prop lights at once and then fades. Give it a sideways sweep (below) and it becomes a comet along the prop.
- On the whole house (a group) the line rises across the whole layout: the arches and the foot of the tree first, then the windows and the matrix, the roofline last, and then it all fades from the ground up. A sideways sweep is an upright line crossing the yard with its tail behind it.

## Settings

Where the line goes. X is a percentage from the left, Y from the bottom. On a tree X goes round it; on a string or an arch only X counts.

- `startX1`, `startY1` and `startX2`, `startY2`: the two ends of the line where it starts.
- `endX1`, `endY1` and `endX2`, `endY2`: the two ends where it finishes. End 1 travels from its start place to its end place, end 2 likewise, and everything between is swept.
- Up, as it comes: starts on the bottom edge (0,0 to 100,0), ends on the top (0,100 to 100,100).
- Down: `startY1` and `startY2` 100, `endY1` and `endY2` 0.
- Left to right: start on the left edge (`startX1` 0, `startY1` 0, `startX2` 0, `startY2` 100) and end on the right (`endX1` 100, `endY1` 0, `endX2` 100, `endY2` 100). Swap the X values for right to left.
- From the middle outward on a string: `startX1` and `startX2` 50, `endX1` 0, `endX2` 100.
- A line that starts as a point fans out; a slanted line sweeps as a slanted band (a twist round a tree); one end kept still makes a wiper.

How it moves.

- `headDuration`: the share of the effect the head takes to arrive. It also sets the tail: at 20 the tail fills everything behind the head and takes the rest of the effect to fade; at 50 the tail is as long as the sweep; at 80 it is a short comet tail; at 100 there is none, only a line crossing for the whole effect. At 0 there is no head: the tail color fills in over the first half and drains over the second.
- `startLength` and `endLength`: the head's thickness in pixels as it sets off and as it arrives. 1 is a fine line; 3 to 8 a bold bar; different values make it grow or shrink on the way.
- `headAtStart`: the whole head shows on the start line from the first frame instead of growing out of it. It matters only with a thick head.
- `acceleration`: above 0 it lingers at the start and then rushes (a wind-up into a hit). Below 0 it rushes first and eases off, and finishes early, leaving the end of the effect nearly dark.

Copies. They only help when the line is short, such as a bar a few pixels wide rising; a full-width line just lands on top of itself.

- `repeats`: extra copies beside the first, set out across the direction of travel.
- `repeatSpacing`: pixels from one copy to the next; make it the bar's width plus the gap you want.
- `autoRepeat`: as many copies as fit across the prop at that spacing, in place of `repeats`.
- `stagger`: fires the copies one after another instead of together, each sweep shortened to fit; below 0 the last copy goes first. A row of bars shooting up in sequence across a matrix, or strings firing round a tree.
- A sideways sweep stacks its copies upward, so on a string or an arch copies do nothing.

Colors: one color paints head and tail. With two, the head is the first and the tail the second. With three the tail runs from the second to the third as it fades. With four or more the head also changes, from the first to the second as it travels.

## When to use it

- A hit with a direction: one Morph a bar or a beat (`place_effects` on a timing track), up the tree, across the roofline, over the whole house.
- A fill and release: the house filling with light from the ground on a rise, then letting go.
- The tail fades by turning see-through, so over another layer it uncovers that layer instead of going black.
- It pairs with `on` under it, with `shockwave` or `fireworks` landing as the head arrives, and with `chase` when you want motion that loops.
- Avoid one long Morph for a whole section (most of it is a slow fade), and don't expect it to repeat by itself.
- Flash safety: on a string or an arch the usual sweep is a whole-prop flash, so placing one on every beat of a fast song across many props is a strobe; keep that to bars, or use a sideways sweep.

## Looks

```look
{"name": "A rising fill that fades", "suits": ["matrix", "tree", "house"],
 "note": "A white line climbs from the bottom with blue filling in below it, then the blue fades away from the ground up.",
 "effect": {"kind": "morph", "settings": {"headDuration": 30, "startLength": 2, "endLength": 2}, "colors": ["#ffffff", "#2a6bff"]}}
```

```look
{"name": "A comet from left to right", "suits": ["string", "arch", "matrix", "tree", "house"],
 "note": "A white head with a short red tail crossing from the left end to the right over the effect.",
 "effect": {"kind": "morph", "settings": {"startX1": 0, "startY1": 0, "startX2": 0, "startY2": 100, "endX1": 100, "endY1": 0, "endX2": 100, "endY2": 100, "headDuration": 80}, "colors": ["#ffffff", "#ff0000"]}}
```

```look
{"name": "Opening from the middle of an outline", "suits": ["string", "arch"],
 "note": "White spreads from the center to both ends, then green fades outward behind it.",
 "effect": {"kind": "morph", "settings": {"startX1": 50, "startX2": 50, "endX1": 0, "endX2": 100, "headDuration": 40}, "colors": ["#ffffff", "#00c040"]}}
```

```look
{"name": "Bars firing up one after another", "suits": ["matrix", "tree"],
 "note": "Narrow gold bars with white tips shoot up in turn from left to right; on a tree, every fourth string in turn.",
 "effect": {"kind": "morph", "settings": {"startX2": 5, "endX2": 5, "autoRepeat": true, "repeatSpacing": 4, "stagger": 40, "headDuration": 40}, "colors": ["#ffffff", "#ffc040"]}}
```

```look
{"name": "A curtain falling", "suits": ["matrix", "tree", "house"],
 "note": "A white line drops from the top with pale blue filling in above it, then fades from the top down.",
 "effect": {"kind": "morph", "settings": {"startY1": 100, "startY2": 100, "endY1": 0, "endY2": 0, "headDuration": 40}, "colors": ["#ffffff", "#80c0ff"]}}
```
