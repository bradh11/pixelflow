# Fireworks

Bursts of sparks that fly out from a point, slow, sink, and fade, going off at random, on a timing track's marks, or with the music: the natural finale.

## What it draws

A burst goes off at a point. Its sparks fly out all round, slowing as they go, sinking a little, and dimming to nothing over the glow time. Each burst is one palette color unless you ask otherwise. Bursts that come faster than they fade pile up; the newest twelve are drawn. Between the sparks it is dark, so a layer below shows through.

- On a matrix it is the real thing: round bursts opening and fading. At the defaults a burst opens to about half the matrix's width, in one-pixel sparks, one a second.
- On a tree the burst is drawn on the tree's grid, so it opens round the trunk as well as up and down: a splash of color that spreads and fades, less round than on a matrix.
- On a string or an arch there is no room to open, so the sparks fly both ways along the prop from the burst's point: a run of dots spreading apart and dimming, a pop travelling outward along the line.
- On the whole house (a group) the bursts go off anywhere in the picture of the layout, and most of that is empty space between props. At the defaults a burst catches a corner of one prop or nothing: weak. Either make the bursts huge, so whole props flare in a burst's color as its sparks cross them, or set `renderStyle` to `perModelDefault`, so every prop has bursts of its own.
- Slow (a low `rate`, a long `life`) it is lazy shells hanging in the air. Fast (a `rate` of 3 to 5) it fills the prop with overlapping bursts.

## Settings

When bursts go off. A timing track wins over the music, and the music over random moments:

- `rate`: bursts a second at random moments. The first always goes off as the effect starts, so even a one-beat effect shows a burst. 0.5 to 1 for the odd shell, 2 to 4 for a finale.
- `timingTrack`: a burst exactly at each mark that starts while the effect plays, and none in between; `rate` is ignored. Marks closer together than `life` overlap their bursts (beats do); marks further apart leave dark between, so on a Bars track raise `life` to 2 or more. A drums track or marks of your own land a burst on a hit.
- `fireOnMusic`: a burst when the music rises past `triggerLevel`, and one more every half second while it stays above. Needs a song. Start `triggerLevel` at 50 to 70; lower it if a quiet passage sets nothing off, raise it if the bursts never stop.
- `randomLocation`: on, each burst is somewhere new. Off, every burst goes off at `centerX`, `centerY` (percent from the left and from the bottom): one spot that keeps firing, or one big burst placed where you want it.

What a burst looks like:

- `sparks`: how many. 20 to 40 is a thin ring of dots, 80 to 120 a full ball, 200 solid.
- `size`: how far the sparks reach, as a percent of half the prop's longer side. 60 to 90 on a matrix or tree; 30 to 60 on a string for a local pop; 150 to 250 on a whole house.
- `life`: seconds a burst glows. 1 to 2 is snappy; 3 to 5 hangs and overlaps the next ones.
- `trail`: the tail behind each spark. 0 is dots; 0.3 to 0.5 short streaks; 1 draws every spark's whole path back to the center, a spiky star.
- `fall`: how far the burst sinks as it fades. It drops as one piece; the sparks don't arc. 0.2 to 0.5 looks natural; above 1 it slides off the bottom.
- `slowing`: 1 lets the sparks rush out and hang; near 0 they fly at one speed until they fade. The difference is slight.
- `sparkSize`: each spark's width as a percent of the prop's longer side, never under one pixel. Leave it at 2 on a single prop. 4 to 6 on a whole house turns sparks into patches big enough to light whole props; that much on a matrix makes a burst one solid disc.
- `colors` (the setting, not the palette list): `burst` gives each burst one palette color; `spark` gives every spark its own, a confetti ball; `fade` runs every spark from the first palette color to the last as it dims (white, gold, deep red burns out like an ember).
- `soft`: on, sparks glide smoothly between pixels. Off, each lights whole pixels and hops: crisper, a little jittery.
- `scale`: leave it on. Off, `size` and `sparkSize` count grid cells instead of percent, which is how imported sequences give them; 60 is then far bigger than most props.

## When to use it

- The last chorus and the final hit. Keep it out of the rest of the show so it means something.
- One burst on an accent: a short effect, `randomLocation` off, a big `size`.
- On a beat or a drum fill with `timingTrack`; on a song with no timing yet, `fireOnMusic`.
- Over a dim base: an `on` in deep blue or a slow `twinkle` under it reads as a night sky.
- It pairs with a white `on` flash on the first burst and a `shockwave` from the same center.
- Avoid it at the defaults on a group, tiny bursts on a big matrix (raise `size`), and a `rate` above 3 with large sparks across a whole house: big areas then flash several times a second, more than is comfortable or safe.

## Looks

```look
{"name": "A firework finale on a matrix", "suits": ["matrix", "tree"],
 "note": "Four bursts a second of red, gold, white, and green confetti sparks with short tails, overlapping until the prop is full.",
 "effect": {"kind": "fireworks", "settings": {"rate": 4, "sparks": 100, "size": 80, "life": 1.8, "trail": 0.5, "fall": 0.4, "colors": "spark"}, "colors": ["#ff2020", "#ffc040", "#ffffff", "#20c040"]}}
```

```look
{"name": "Every prop of the house bursting on the beat", "suits": ["house"],
 "note": "On each beat every prop sets off a burst of its own, all in the same palette color for that beat.",
 "effect": {"kind": "fireworks", "settings": {"timingTrack": "Beats", "sparks": 60, "size": 90, "life": 1, "trail": 0.4}, "renderStyle": "perModelDefault", "colors": ["#ff2020", "#ffc040", "#ffffff", "#20c040"]}}
```

```look
{"name": "One huge burst from the middle of the house", "suits": ["house", "matrix"],
 "note": "A single gold burst from the center whose rays sweep out over everything and fade over three seconds: the final hit.",
 "effect": {"kind": "fireworks", "settings": {"rate": 0.25, "randomLocation": false, "sparks": 200, "size": 200, "sparkSize": 5, "life": 3, "trail": 1, "fall": 0.2}, "colors": ["#ffc040"]}}
```

```look
{"name": "Pops along an outline", "suits": ["string", "arch"],
 "note": "Two pops a second in red, white, or green, each a run of dots spreading both ways along the prop.",
 "effect": {"kind": "fireworks", "settings": {"rate": 2, "sparks": 24, "size": 50, "life": 1.2}, "colors": ["#ff2020", "#ffffff", "#20c040"]}}
```

```look
{"name": "Bursts set off by the music", "suits": ["matrix", "tree"],
 "note": "A full burst each time the music jumps past the trigger level.",
 "effect": {"kind": "fireworks", "settings": {"fireOnMusic": true, "triggerLevel": 60, "sparks": 80, "size": 80, "life": 1.2}, "colors": ["#ff2020", "#ffc040", "#ffffff"]}}
```
