# Lightning

Jagged bolts that strike at random moments and flutter like the real thing on a matrix or a tree, and a fluttering flash on everything else: storms, and the crackle on top of a big hit.

## What it draws

Strikes come at random moments, about `density` of them a second. Each strike is a bright stroke, then one to three quick re-strikes a little dimmer, 40 to 130 ms apart, then a short fading tail. So a strike is a flutter lasting between a tenth and about half a second, never one clean flash. Everything is drawn in the first palette color; the others are ignored.

- On a matrix: each strike draws a bolt, a thin jagged line from a random place on the top edge down to the bottom, with dimmer forks peeling off down and sideways. The rest of the panel flashes faintly behind it (`glow`). A bolt keeps its shape while it flutters; the next strike draws a new one. When strikes overlap, up to four bolts show at once.
- On a tree: the same bolt from the top of the tree to its foot, hopping from string to string on the way down. A bolt that lands round the back isn't seen from the street; the glow is.
- On a string or an arch: no bolt. A prop one pixel tall has no room for one, so the whole run flashes with each strike, as if `flashOnly` were on.
- On the whole house (a group): one bolt is drawn across the whole layout, a pixel wide, so most of it falls in the gaps between props. In the frames it showed as a short streak where it crossed the matrix and a dot or two on the roofline, with the faint glow everywhere else. For bolts on a group set `renderStyle` to `perModelDefault`: every matrix and tree then draws its own bolts and every string and arch flashes whole.
- Window frames and rows of canes catch only scraps of a bolt. Treat them like strings.
- Between strikes it is see-through, so the look below shows.
- Sparse (a `density` under 0.5) there are seconds of dark between strikes: a storm in the distance. At 1 to 2 there is nearly always a bolt fading somewhere. From 4 up the strikes overlap into a continuous crackle.

## Settings

- `density`: strikes a second, on average. Each strike lands at a random moment inside its own share of time (a second divided by `density`), so it can't be aimed at a beat, and an effect shorter than that share may hold no strike at all. 0.2 to 0.5 for a storm behind a section; 1 to 2 for a busy storm on a matrix; 4 to 6 for a beat or two of crackle, where the first strike comes within a fifth of a second.
- `branches`: how much each bolt forks. 0 is one clean line, 0.3 to 0.5 looks natural, 1 is bushy and fills a small matrix.
- `segments`: the jags from top to bottom. 2 is an almost straight stroke with one kink; 8 is a natural zigzag. Past about half the prop's height in pixels the jags are smaller than a pixel and the bolt crumbles into a scribble (24 did on a matrix 20 tall).
- `thickness`: the bolt's width in pixels. 1 is thin and sharp; 2 to 3 carries from the street on a matrix. On a tree of 16 strings keep it to 1 or 2.
- `glow`: how brightly the rest of the prop flashes with each bolt, as a share of the bolt. 0 is the bolt alone on black, 0.1 to 0.25 a faint sky, and at 1 the whole prop flashes as bright as the bolt and the bolt is lost in it.
- `flashOnly`: no bolts; the whole target flashes with each strike. For outlines, windows, and a storm that should light everything.

## When to use it

- Flash safety: every strike is two to four flashes inside half a second. So wherever Lightning flashes a whole prop (strings, arches, `flashOnly`, a high `glow`) it flickers faster than three times a second while a strike lasts, whatever the `density`. Keep those uses small and short: one outline or a pair of arches for a bar or two, not the whole house. Where it must cover more, make it dim and sparse: a `brightness` near 0.3 over a base that stays lit, and a `density` of 0.5 or under so strikes are seconds apart. Bolts with a `glow` of 0.25 or less change only a thin line and a faint wash, which is fine on a matrix or a tree for a whole section. For one bright flash of the whole house use `impact`: it flashes once.
- A storm: a dim dark blue `on` on the group's row so nothing is black; Lightning with bolts on the matrix and the tree; a dim `flashOnly` Lightning on the roofline. Pale blue or white for weather; purple or green for Halloween.
- The crackle on a hit: `stage_cue` with `hit` adds a beat of Lightning (6 a second, forked, in white) on the matrices above its `impact` when `intensity` is 0.85 or more. Place Lightning yourself for a storm through a section, bolts on a tree, a color, or crackle on a prop the cue leaves out.
- The flash that must land on the beat is an `impact`. Lightning goes on the layer above it and rumbles around it.
- Avoid thick, bushy bolts at a high `density` on a small matrix: it fills up and reads as static, not lightning.

## Looks

```look
{"name": "Storm on a matrix", "suits": ["matrix", "tree"],
 "note": "Pale blue forked bolts striking about twice a second with a faint flash of sky behind each. Lower the density toward 0.5 for a storm further off.",
 "effect": {"kind": "lightning", "settings": {"density": 2, "branches": 0.5, "glow": 0.15}, "colors": ["#dfe8ff"]}}
```

```look
{"name": "Bold bolts down a tree", "suits": ["tree", "matrix"],
 "note": "Thick white bolts with many forks and no glow, so each one stands alone on black.",
 "effect": {"kind": "lightning", "settings": {"density": 2, "branches": 0.8, "thickness": 2, "segments": 10, "glow": 0}, "colors": ["#ffffff"]}}
```

```look
{"name": "Crackle on top of a hit", "suits": ["matrix", "tree"],
 "note": "Overlapping white bolts and a bright sky: one beat long, on the layer above an Impact, at the biggest moment of the song.",
 "effect": {"kind": "lightning", "settings": {"density": 6, "branches": 0.5, "glow": 0.4}, "colors": ["#ffffff"]}}
```

```look
{"name": "Purple flicker along an outline", "suits": ["string", "arch"],
 "note": "The whole run fluttering in dim purple with each strike. One outline or a pair of arches, a bar or two at a time.",
 "effect": {"kind": "lightning", "settings": {"density": 2, "flashOnly": true}, "colors": ["#b070ff"], "brightness": 0.6}}
```
