# Targets and layouts

What a row can light (a prop, a group, a submodel), and how each lays its pixels out for an effect: render styles, group layouts, a prop's own layout, turning and flipping.

## What a row lights

A row's `target` is one of three things. `get_open_sequence` lists the rows there are; `sequence_add_row` adds one.

- A prop: `{"prop": id}`. The effect is drawn on that prop alone, on its own grid.
- A group: `{"group": id}`. The effect is drawn once across every member, as one canvas. This is how a wipe travels across the house or a spiral winds up a pair of trees together.
- A submodel or face of a prop: `{"region": {"prop": id, "region": id}}`. The effect is drawn on those pixels only: the top of a tree, one window pane, a star's outer ring. `list_props` gives each prop's submodels.

A group's row and its members' rows both light the same pixels. Put the group's row above its props' rows in the list, so a prop's own effect covers the group's where it plays and the group's look shows everywhere else (topic `blends`).

## The grid an effect sees

Every effect draws on a grid of cells and each pixel takes its cell's color.

- A matrix is its own grid: columns across, rows up.
- A tree is a grid with a column for each string and a row for each pixel up it, so "across" on a tree goes round it and a diagonal stripe becomes a spiral.
- An arch, a line, or any single run is one row: "across" runs along the wiring. There is no "up" to speak of, so effects that need height (`fire`, `text`, `picture`, falling `snowflakes`) show a one-pixel slice. Use the along-the-string effects on them instead.
- A row of candy canes or icicles is a grid with a column for each cane or drop.
- A window frame is the edge of a grid: effects that sweep across or up read well on it, and anything drawn in the middle of the grid misses it.
- A group, by default, is a grid laid over its members where they sit in the layout, so its picture is the house seen from the street.

## Render styles

An effect's `renderStyle` picks another grid for that one effect. The styles: {{render-styles}}.

On a single prop only four differ:

- `default`: the prop's own grid, as above.
- `perPreview`: each pixel where it really sits in the layout, seen from the front. On a tree this draws on the cone's outline rather than round its strings.
- `singleLine`: every pixel end to end in wiring order, as one long row. It turns any prop into a string, which is how to run a `chase` along a matrix's wiring or up and down a tree's strings.
- `asPixel`: the whole target is one cell. The effect becomes a single color changing over time: `colorWash`, `fade`, `pulse`, a `vuMeter` level.

On a group the rest come into play:

- `perModelDefault`, `perModelPerPreview`, and `perModelSingleLine` draw the effect separately on each member, on its own grid, its real positions, or its wiring. Every arch gets its own complete chase, in step with the others. This is the style to reach for when a group's members should all do the same thing rather than share one picture.
- `horizontalPerModel` makes each member a column, its pixels running up it; `verticalPerModel` makes each a row. A group of four arches becomes a 4-wide grid, so `bars` or a `vuMeter` across it lights whole arches as bars.
- `horizontalStack` and `verticalStack` put the members' own grids side by side or one above another at their own sizes; `horizontalStackScaled` and `verticalStackScaled` stretch each to the same size first. Use them when members of different sizes should each show a full share of one picture.
- `overlayCentered` and `overlayScaled` lay the members' grids on top of each other, centered or each stretched to the largest, so every member shows the same part of the picture: matching windows all showing one `spiral`.
- `singleLineModelAsPixel` makes each member one cell in a row, and `defaultModelAsPixel` makes each one cell where it sits. Anything that moves across the grid (`bars` sliding across, a `wipe`, a `chase` with its `order` set to `across`) then lights whole props one after another.

One thing overrides all of this: a `chase` in its usual `order`, `wiring`, runs along the pixels in the order they are wired, through each member in turn, whatever the render style, and turning or flipping the grid doesn't change it. Its own `order` and `direction` settings are how to steer it.

## A group's own layout

A group has a `layout` of its own that effects with the `default` style use: {{group-layouts}}. `minimalGrid` (the default) and `grid` are the house-as-one-picture grids; the others name the render style the group uses unless an effect picks its own. Change it with `show_update_group` only when a group should always behave that way (a row of arches that should always be `perModelDefault`); otherwise set the effect's `renderStyle`.

## A prop's own layout

A prop's `ownLayout` replaces its shape's grid for every effect on it: `shape` (the default), a `strand` (one row along the wiring, either way round), or `cells` (a grid you give cell by cell). It is the fix for a custom prop whose effects come out scrambled; it is not something to change for one effect.

## Turning and flipping

An effect's `bufferTransform` turns or flips its grid: {{buffer-transforms}}.

- `flipHorizontal` mirrors left and right: the right-hand matrix of a pair showing the mirror image of the left. When an effect has a direction setting of its own, use that instead; it is clearer, and for a `chase` along the wiring it is the only thing that works.
- `flipVertical` turns up into down: `fire` hanging from the top edge, bars that fall instead of rise.
- `rotateCw90` and `rotateCcw90` turn the grid a quarter turn, which makes an up-and-down effect travel sideways. `rotate180` is both flips at once.
- `rotateCw90FlipHorizontal` and `rotateCcw90FlipHorizontal` are a quarter turn and a mirror together, for the rare prop wired on its side and backwards.
- `none` leaves it alone.

## Choosing

- One picture across many props: a row on their group, `default` style.
- The same thing on each of many props: a row on their group with `perModelDefault`, or `place_effects` on each prop's row.
- A run along several props end to end: the group with `singleLine`.
- Props lighting whole, one after another: the group with `horizontalPerModel` and `bars` sliding across, a `chase` with its `order` set to `props`, or `place_effects` with a `spread` of `sweep`.
