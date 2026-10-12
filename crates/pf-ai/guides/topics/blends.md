# Layers and blends

How effects stack: layers on a row, the blend that mixes each effect with what is under it, and how one row meets another.

## How a frame is built

- A row is one target (a prop, a group, or a submodel) with layers. Layer 0 is the bottom; higher layers draw on top of it. Effects on one layer take turns and may not overlap in time.
- Each effect's `blend` says how it mixes with the layers below it on its own row. The lowest effect playing on a row always just draws: there is nothing under it to mix with.
- Rows are drawn in order, top of the list first. Where two rows light the same pixel (a group's row and one of its props' rows), the later row wins, as the sequence's row blending says.
- So: a group's row carries the base look, and a prop's own row, lower in the list, carries what that prop does differently. Cues from `stage_cue` go on a new top layer of each row for the same reason.

## Every blend

{{blends}}

## Which to reach for

- `normal` is right most of the time: the upper effect covers where it is lit and lets the lower one show where it is dark. A `chase` over a `colorWash` is a bright band travelling across a colored prop.
- `add` and `max` let both show. `add` sums the light, so overlaps go toward white: good for a flash or glow on top of a look. `max` keeps the brighter of the two without washing out: good for laying `twinkle` over a base.
- `multiply` tints. A moving effect in white and gray over a colored base keeps the base's colors and takes the top's motion; a colored top layer recolors whatever is under it.
- `mask` and `reveal` cut shapes. `mask` punches the top effect's lit shape out of what is below (black bars travelling across a picture). `reveal` shows what is below only inside the top effect's shape (text filled with a moving wash: the wash below, the `text` above with `reveal`). `revealBrightness` does the same but lets the top effect's brightness dim it, for soft edges.
- `clip` and `cutOut` do the opposite job: they keep the top effect's own colors, only where the layer below is lit (`clip`) or dark (`cutOut`). `clipBrightness` takes the brightness of what is below as well.
- `over` and `behind` fill gaps: `behind` shows the top effect only where the lower layers are dark, which puts a background behind something without touching it.
- `subtract`, `min`, and `average` are rarely what a look needs; `subtract` can carve a dark moving shape out of a bright base.
- `highlight` and `highlightAdd` light only where both are lit: sparkle that stays inside a moving shape.
- `shadow` and `shadowBelow` shift hues by the other layer: an odd, shimmering recolor. Try them on a matrix only.
- `bottomHalf` and `leftHalf` split the target in two, one effect on each half.

## Rows meeting rows

The sequence has one setting for this, set with `sequence_set_row_blending`: {{row-blending}}.

- `cover` is PixelFlow's own way and the one to leave on for a sequence made here. Where a row is lit it covers the rows before it; where it is dark they show through. A prop's row with nothing playing shows its group's row.
- `blend` makes the lowest effect on each row mix with the rows before it by its own blend, as its other effects mix with the layers below. Choose it when a prop's row should add to or tint what its group is doing rather than replace it.
- `replace` makes a row with an effect playing overwrite all its pixels, lit or not. It is how an imported xLights sequence with Model Blending off behaves; keep it for those.

## Habits

- Two layers are plenty for most looks: a base and one thing moving over it. A third is for an accent that comes and goes.
- Put the brighter, sparser effect on top. A dense effect on top hides everything under it whatever the blend.
- If a look turns white and flat, an `add` is stacking: switch it to `max`, or lower the top effect's `brightness`.
- If an effect with a cutting blend (`reveal`, `clip`, `mask`) shows everything or nothing, check that another effect is playing on a lower layer of the same row at that moment. Blends only mix within a row, and the lowest effect playing ignores its blend.
