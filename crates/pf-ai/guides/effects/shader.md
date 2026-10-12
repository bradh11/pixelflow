# Shader

A flowing, ever-changing pattern worked out pixel by pixel from a shader file: auroras, lava lamps, rings on water, nebulas, stained glass, kaleidoscopes, tunnels, and rainbows.

## What it draws

A shader is a small program that paints a picture the size of the prop's grid, every frame. PixelFlow has eight of its own, and a show can have its own files besides; `list_effect_kinds` with kind "shader" lists them all with what each looks like, and with a `file`, its inputs. Each has its own look, and none takes the palette: its colors are its own inputs.

- On a matrix it is at its best: the whole picture, moving, in the shader's own colors. Aurora drew green and violet curtains leaning across a 40 by 20 matrix; Lava Lamp drew orange and yellow blobs on dark; Pond, light rings crossing on blue water; Stained Glass, panes of strong color with dark leading; Kaleidoscope, a turning star of color.
- On a tree the strings stand side by side, so the picture is wrapped round the cone: Prism became rainbow stripes running round the tree, Pond and Nebula soft patches of light climbing it, Lava Lamp blobs sliding up and down. Every shader reads on a tree.
- On a string or an arch there is one row of the picture. Prism is a rainbow flowing along the roofline and is the one to use there. The others are a slice through their picture: Pond and Kaleidoscope give colored dashes, Aurora and Lava Lamp mostly dark with the odd glow, Stained Glass a few blocks of color.
- On the whole house (a group) the picture is the size of the whole layout and each prop shows the part of it that falls on it, like a picture behind the house seen through its lights: Prism laid one rainbow across the yard, Stained Glass gave each prop a pane or two. Props far apart show distant parts, so a big slow shape reads best.
- At `speed` 0.25 the picture drifts; at 4 Kaleidoscope and Tunnel spin and rush. A shader is the same picture at the same moment every time it is played.

## Settings

- `file`: which shader, as `list_effect_kinds` gives its file: "pixelflow:aurora" and the rest for PixelFlow's own, "shaders/" and a name for the show's own files. A file the show doesn't have, or one that can't be drawn, is refused, and the reason is said.
- `inputs`: the shader's own settings, by the names `list_effect_kinds` gives for that file, for example {"curtains": 5, "low": [0, 1, 0.5, 1]}. A number stays within the range the file gives it; a switch is true or false; a choice is the number of the one chosen; a point is [across, up]; a color is [red, green, blue, alpha], each 0 to 1. Leave one out to keep the shader's own value, which is usually a good start. A number input can change over the effect: a curve keyed "inputs." and the input's name (a point's ".x" or ".y" after it).
- `speed`: how fast the shader's time runs, 1 its own pace. 0.5 for a calm verse, 2 for a chorus; 0 holds a still picture, below 0 runs it backwards. Many shaders also have a speed of their own among their inputs.
- `leadIn`: seconds the shader is already into when the effect starts, so two effects of one shader in a row don't start from the same picture. 0 is fine.
- `offsetX`, `offsetY`, `zoom`: kept from an imported xLights sequence and not used. Leave them at 0.

## When to use it

- A slow, rich background on a matrix or a tree through a verse or an instrumental: Aurora, Nebula, Pond, or Lava Lamp, with a fade in and out.
- A big bright look for a chorus on the matrix: Kaleidoscope or Tunnel at `speed` 1 to 2; Stained Glass for a held, colorful moment.
- Prism on outlines, arches, the tree, and the whole house: a rainbow that flows. On a group it ties the house together.
- Layering: Lava Lamp, Aurora, and Nebula are clear where their shapes are thin, so they sit well over a dim `on` or `colorWash` on the layer below, or under `text` on a matrix. The others cover the prop.
- Choose colors with the shader's color inputs to fit the section's palette: the effect's `colors` do nothing.
- Avoid most shaders on a single string or arch (only Prism reads there), and putting several on one prop at once: one shader is already a busy picture. Keep `speed` low on a large area: Tunnel and Kaleidoscope at high speed flash and spin.
- Use the show's own shader files when the user asks for them by name; `list_effect_kinds` says which can't be drawn and why. The Assistant can't write or change shader files.

## Looks

```look
{"name": "Northern lights on the matrix", "suits": ["matrix", "tree"],
 "note": "Green and violet curtains swaying slowly, fading in and out.",
 "effect": {"kind": "shader", "settings": {"file": "pixelflow:aurora", "speed": 0.75}, "fadeInMs": 1000, "fadeOutMs": 1000}}
```

```look
{"name": "Rainbow flowing over the house", "suits": ["string", "arch", "tree", "house"],
 "note": "One rainbow moving across every prop, the colors in step from one side of the yard to the other.",
 "effect": {"kind": "shader", "settings": {"file": "pixelflow:prism", "inputs": {"rainbows": 0.75}}}}
```

```look
{"name": "Lava lamp in the season's colors", "suits": ["matrix", "tree"],
 "note": "Red and green blobs drifting and merging, dark between them.",
 "effect": {"kind": "shader", "settings": {"file": "pixelflow:lava-lamp", "speed": 0.5, "inputs": {"first": [1, 0, 0, 1], "second": [0, 0.8, 0.1, 1]}}}}
```

```look
{"name": "Kaleidoscope for a chorus", "suits": ["matrix"],
 "note": "A star of saturated color turning about the middle of the matrix, its wedges flowing.",
 "effect": {"kind": "shader", "settings": {"file": "pixelflow:kaleidoscope", "speed": 1.5, "inputs": {"wedges": 8}}, "fadeInMs": 300}}
```

```look
{"name": "Rings spreading faster", "suits": ["matrix", "tree"],
 "note": "Rings on water, coming more and more quickly through the effect.",
 "effect": {"kind": "shader", "settings": {"file": "pixelflow:pond"}, "curves": {"inputs.speed": {"shape": "ramp", "from": 0.5, "to": 3}}}}
```
