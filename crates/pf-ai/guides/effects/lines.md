# Lines

Straight lines and wire shapes drifting about a matrix and bouncing off its edges, with optional trails: an abstract, geometric texture.

## What it draws

A few lines, each joining two or more points. Every point moves in its own straight path and bounces off the edges of the grid, so a two-point line is a stick that swings and stretches, and three points make a triangle that tumbles and changes shape. Each line takes the next palette color. Where they start is random and differs from one effect to the next. Everything off the lines is see-through.

- On a matrix it is what it says: thin wire shapes gliding and bouncing. This is where it belongs.
- On a tree the grid is strings by pixels, so the shapes wrap round it and show as slanted lines crossing the strings. It reads as moving diagonals, steppy on a tree with few strings.
- A string or an arch has no height, so each line is just the lit stretch between its outermost points. With the usual two triangles most of the prop is lit, in blocks of color whose ends slide about. With one two-point line it is a single stretch that drifts, grows, and shrinks.
- On the whole house (a group) the lines cross the whole layout and each prop shows only the sliver passing over it: pieces on the matrix, a few pixels on an arch or a window, often nothing on the tree. It looks broken. The `perModelDefault` render style gives each prop its own lines, but strings and arches then sit mostly lit.
- Under 0.5 the shapes drift. At 1 they are busy. Above 2 they jump about, and on a one-row prop fast points overshoot the grid and the line blinks out.

## Settings

- `count`: how many lines. 1 to 3 keeps a matrix readable; 10 or more is a tangle of sticks.
- `points`: points in each line. 2 is a stick, 3 a triangle, 4 to 6 a shape with that many corners that folds over itself as it moves.
- `thickness`: line thickness in pixels. 1 is a wire; 2 to 3 reads better from the street on a coarse matrix; at 10 a line is a blob.
- `speed`: how far each point moves every frame, in pixels, so the same number is faster on a small prop. 0.2 to 0.5 for a drift, 1 for energy. 0 freezes the shapes.
- `trails`: copies of each line where it was in the frames before, drawn behind it. 3 to 5 smear each line like a comet; 10 sweeps broad ribbons that cover much of a small matrix. Trails stretch with `speed`.
- `fadeTrails`: on (the default) the trail dims toward its oldest copy. Off, every copy is full strength and the trails become solid slabs.

## When to use it

- A matrix texture for an instrumental or electronic passage: something that moves without a direction.
- Laser sticks: several two-point lines with short trails over a dark `on`.
- A curve on `speed` (a `music` shape, or a `ramp` into a chorus) makes the shapes agitate with the song.
- It pairs with steady or sweeping looks on the rest of the display; it is too thin to carry a section alone.
- Avoid it on strings, arches, and groups that span the house. Avoid `blur`, which dims thin lines to almost nothing. Avoid a curve on `count`: lines pop in and out at random places.
- It doesn't flash at sensible speeds; near the top of `speed` it flickers.

## Looks

```look
{"name": "Tumbling triangles", "suits": ["matrix", "tree"],
 "note": "A red and a green wire triangle drifting and bouncing, each with a short fading smear.",
 "effect": {"kind": "lines", "settings": {"count": 2, "points": 3, "speed": 0.4, "trails": 4}, "colors": ["#ff0000", "#00c040"]}}
```

```look
{"name": "Laser sticks", "suits": ["matrix", "tree"],
 "note": "Four white and icy blue sticks swinging across each other, with short trails.",
 "effect": {"kind": "lines", "settings": {"count": 4, "points": 2, "speed": 0.6, "trails": 3}, "colors": ["#ffffff", "#80c0ff"]}}
```

```look
{"name": "Folding ribbons", "suits": ["matrix"],
 "note": "Two four-cornered shapes in orange and purple sweeping broad fading ribbons as they fold.",
 "effect": {"kind": "lines", "settings": {"count": 2, "points": 4, "speed": 0.5, "trails": 8}, "colors": ["#ff8000", "#a020f0"]}}
```

```look
{"name": "A stretch that drifts along an outline", "suits": ["string", "arch"],
 "note": "One gold stretch sliding slowly along the prop, growing and shrinking as its two ends wander.",
 "effect": {"kind": "lines", "settings": {"count": 1, "points": 2, "speed": 0.2, "thickness": 1}, "colors": ["#ffc040"]}}
```
