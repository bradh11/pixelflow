# What every effect shares

Colors, fades, sparkles, blur, and color adjustments: the settings every effect has on top of its kind's own, and when each earns its place.

## The settings

{{effect-wide}}

In `place_effects` and `change_effects` the colors are a plain list, `colors`; in `sequence_add_effect` they are `palette` with its `colors` inside. Everything else has the same name in all three.

## Colors

- An effect takes up to 32 colors, as "#rrggbb". With none it draws white.
- How the list is used depends on the effect. One-color effects (a `fade`, a `pulse`, a `strobe`) take the first. Striped and banded effects (`bars`, `chase`, `spiral`, `pinwheel`) hand the colors out in turn, so two colors give candy stripes and three give red, white, green. Washes (`colorWash`, `on` with a gradient, `fill`) blend from one to the next.
- Black in a list is a gap: red, black, red, black gives red stripes with dark between them, which reads cleaner than a second color on thin props.
- Two or three colors is a look. Five is a rainbow, whatever you meant. See topic `palettes`.

## Fades

- `fadeInMs` and `fadeOutMs` ramp the whole effect up from nothing and down to nothing. A fade lasts a whole number of frames, and `place_effects` trims each to half the effect's length.
- A section's base look wants 200 to 600 ms each way, so sections melt into each other instead of snapping.
- A hit wants no fade in and a long fade out: the punch, then the decay.
- Short effects cut on beats want none: at 120 beats a minute a beat is 500 ms, and a 200 ms fade each way leaves almost nothing at full.
- Two effects one after the other on a layer, each fading, dip to dark between them. To cross from one to the other, put them on two layers and overlap them by the fade.

## Sparkles

- `sparkles` flashes the lit pixels, each at its own random moment, in `sparkleColor` (white unless you say). It is sparse at low numbers: about 3 pixels in a hundred are flashing at 50, 5 at 100, 9 at 150, and more than half at 200.
- 20 to 60 is a glint on a steady look: the cheapest way to make an `on` or a slow wash feel alive.
- 120 and up is a snowstorm of white. Keep it for peaks and endings, a bar or two at a time.
- `musicSparkles` makes the count follow the music's loudness, up to `sparkles`: quiet passages glint, loud ones glitter. Set `sparkles` to the most you want at the loudest moment (120 to 200), because quiet music scales it right down. Without music it stays at the steady count.
- Sparkles go where the effect draws, so a sparse effect sparkles sparsely. For glitter over a whole prop, put them on something that covers the whole prop (an `on`, a `colorWash`).
- A curve on `sparkles` builds glitter through a rise: see topic `curves`.

## Blur

- `blur` softens the effect before it mixes with the layers below: 0 is off, 1 to 3 takes the edge off stripes and text, 6 and up turns shapes into glows.
- Blur spreads light, so it dims as it softens. Stripes a few pixels wide are a faint glow by 8 to 10, on a string as much as on a matrix; on a tree even 3 takes a thin stripe down to a dull line. Raise `brightness` with it, or blur only bold shapes.
- Never blur `text` you want read. A little blur under sharp text (the text on a layer above a blurred wash) reads well.

## Color adjustments

- `hueAdjust` turns every color round the color wheel: 1 is a full turn, 0.5 the opposite color. With a `ramp` curve from 0 to 1 it makes any effect cycle through the rainbow once.
- `saturationAdjust` below 0 washes the colors toward white (pastels at -0.4); above 0 deepens them.
- `valueAdjust` lightens or darkens. `brightness` multiplies: 0.5 is half as bright, 2 pushes dim colors up, 1 leaves it alone.
- `contrast` above 0 makes the darker parts darker and the brighter parts brighter. It is the one adjustment a curve can't drive.
- Use `brightness` to balance props against each other: a dense matrix beside a thin roofline usually wants 0.5 to 0.7 so it doesn't drown the outline.

## Mixing and layout

- `blend` says how the effect mixes with the layers under it on its row: topic `blends`.
- `renderStyle` and `bufferTransform` choose how the target's pixels are laid out for the effect, and turn or flip that layout: topic `layouts`.

## Changing over time

`curves` makes a number setting change while the effect plays, or follow the music or a timing track: topic `curves`.
