# Source provenance: what follows xLights (and other) source

This is an inventory of the PixelFlow files that were written from, or with reference to,
another project's source code. It records what the code looks like next to the source it cites.
It is **not legal advice**, and it does not say whether any file is a "derivative work": that is
a legal question, and the author of this inventory is not a lawyer. See
[distribution-checklist.md](distribution-checklist.md) for what follows from it.

## What this inventory is for

PixelFlow is open source under the GNU General Public License, version 3
([`LICENSE`](../../LICENSE)), the licence xLights is under. Offering the whole project under that
licence covers every file below, so the inventory is not a list of problems to clear. It is:

- **the record behind the credit.** [`NOTICE`](../../NOTICE) says that parts of PixelFlow are
  derived from xLights' source and points here for which parts. PixelFlow is an independent
  project inspired by xLights; the rows below marked "direct port" are the parts that are more
  than inspired, and they are credited as derived;
- **for anyone who wants to know** where a file came from, for example before reusing it
  somewhere else;
- **a guide to what is worth rewriting.** A part is rewritten as PixelFlow's own where there is a
  real gain (speed, simplicity, or a closer match to what xLights draws), not to get out from
  under the licence. A rewritten part moves to "behaviour-matching" and the list of direct ports
  gets shorter. Pinwheel, Shockwave and Plasma are done, and the rest stays, credited.

## How this was made

- Every PixelFlow file that names xLights, FPP, or WLED source was read, and compared with the
  source it cites.
- xLights was read at commit `fa00fd3c80713d7d9a9a88f7474c717bf748e45f` (`master`, 2026-10-10) of
  <https://github.com/xLightsSequencer/xLights>: `src-core/effects`, `src-core/render`,
  `src-core/models`, `src-core/lyrics`, `src-core/controllers/{Falcon,FPP}.cpp`, and three files
  under `resources/`. Each xLights source file's header points to `License.txt`, which is the
  GNU General Public License, version 3.
- FPP and WLED source were **not** read. Rows that name them report only what PixelFlow's own
  comments cite.
- Some xLights code that PixelFlow's comments cite was outside the files read
  (`XmlSerializer/*`, `utils/VectorMath`, `SettingsMap`, `AudioManager.cpp`, `LogarithmicScale`,
  the import-mapping dialog, and the layout background drawing). Rows that depend on those rest
  on PixelFlow's comments and say so.
- Line counts are as they stood at that reading, apart from Pinwheel's, which were brought up
  to date when it was rebuilt (see "Rebuilt from a description of its behaviour"). The renderer's
  multi-threaded drawing, the Fireworks effect, and the Shader effect with its engine, added
  since, are PixelFlow's own and are not in the totals.
- Line counts are of the file as it stands. "Impl" leaves out the file's own `#[cfg(test)]`
  module. The share of a mixed file that follows the source is an estimate from reading, not a
  measured diff.
- Where a judgement was uncertain, the more derived category was chosen and marked "likely".

No PixelFlow file carries a copyright or licence notice of its own, and none reproduces
xLights' file header. The files below name the xLights file and function they follow in their
opening comment. The project's licence and its credit to xLights are stated once, in `LICENSE`
and `NOTICE` at the repo root.

## Categories

| Category | Meaning |
|---|---|
| **Direct port** | Follows the cited source line by line or structure by structure: the same sequence of steps, intermediate values, formulae, constants, and branches, in Rust (or TypeScript) idiom. |
| **Behaviour-matching** | Organised independently, but reproduces specific constants, tables, formulae, or thresholds that came from reading the source. |
| **Format reading** | Reads or writes a file format or wire protocol for interoperability: attribute names, setting keys, packet layouts. No algorithm carried over. |
| **Independent** | PixelFlow's own design. At most it shares public names or visible behaviour. |

The "Clean-room" column is what rewriting the piece as PixelFlow's own would take: **S** under a
day, **M** days, **L** a week or more. "Clean room" here means one person writes a description of
the behaviour (from watching xLights, its manual, or files it saves) and a different person, who
has not read the xLights source, writes the code from that description. The estimates were made
when relicensing was still an open question; they serve now to choose which rewrites are worth
their cost.

## Summary

| Category | Files | Where |
|---|---|---|
| Direct port (the whole file, or most of it) | 32 | 13 effects and the two timing rules they share (`carried.rs`) in `pf-render`; 7 files in `pf-xlights` (model geometry and value curves); 10 shape generators in `pf-geometry`; `app/src/lib/geometry.ts` |
| Mixed: a direct-port function group inside an otherwise independent file | 12 | 10 in `pf-render` (text, raster, blur, color, geometry, styles, sparkles, effects, adjust, picture/resample); `pf-xlights/src/geometry.rs`; `pf-sequence/src/curve.rs` |
| Behaviour-matching | 31 | effect default tables, the model-to-shape converter, the mouth-shape table, controller tables, Pinwheel, Shockwave and Plasma (rebuilt from descriptions of their behaviour), and smaller pieces |
| Format reading only | 27 | the rest of the xLights importer, the FPP REST client, the `.fseq` reader |
| Independent | the rest | including the engine, output, audio analysis, the assistant, camera mapping, video export, the desktop shell, and nearly all of the UI |

Approximate implementation lines by category, for the crates with the most:

| Crate | Impl lines | Direct port | Behaviour-matching | Format reading | Independent |
|---|---|---|---|---|---|
| `pf-render` (46 files) | about 16,000 | about 4,800 | about 1,200 | 0 | about 10,000 |
| `pf-xlights`, `pf-sequence`, `pf-model` (53 files) | about 23,100 | about 3,400 | about 5,800 | about 5,600 | about 8,300 |
| `pf-geometry` (17 files) | about 1,620 | about 900 | about 330 | 0 | about 400 |
| `app/src/lib/geometry.ts` | 691 | 691 | | | |

For scale: the Rust code is about 168,000 lines including tests. The direct-port lines are
roughly 9,000 to 10,000 of those, and are concentrated in three places: the effects that carry an
xLights effect's name, the model geometry (where each pixel of an imported prop sits), and value
curves.

**The renderer.** About a third of `pf-render`'s implementation lines follow xLights source. Thirteen
effects (Morph, Tendril, Snowflakes, Garlands, Circles, Fan, Butterfly, Life,
Lines, Shape, VU Meter, Fill, and Fire's flickering flames) are ports of the xLights
effect of the same name: 80 to 95% of each
file, except Circles and Life, where about half is ported and the rest is PixelFlow's own
arrangement of xLights' constants or the ordinary algorithm. So are the pieces every layer passes through: line and circle drawing, blur, the xLights layer
blend modes and HSV conversion, sparkles, a layer's color adjustments, submodel and group buffer
layout, and Text's movement.
Pinwheel, Shockwave and Plasma were among them until each was rebuilt from a description of
its behaviour; they are now listed under behaviour-matching.
The rest is PixelFlow's own: the render pipeline, the picture loader, the closed-form
effects in `effects.rs` (On, Color Wash, Fade, Chase, Bars, Wave, Twinkle, Shimmer, Strobe,
Spiral, Meteors, Ripple, and Fire's rolling flames), and Wipe, Impact, Pulse, Sing, Color Shift,
Lightning, Dancer, Picture, and Fireworks.
Fireworks (`fireworks.rs`) was written without xLights' `FireworksEffect.cpp` being read: from
the frames xLights rendered for a sequence (see [../fidelity.md](../fidelity.md)), its public
manual, and the setting names a sequence file saves. The importer's translation of it reads
those setting names, which are format, and the numbers it uses were measured from the frames.

The Shader effect and its engine are PixelFlow's own. `crates/pf-shader` (the GLSL lexer,
preprocessor, parser, type check, the machine that runs a batch of pixels at a time, and the
math functions it works out the same way on every system) was written for PixelFlow, without
xLights' `ShaderEffect.cpp` or any shader compiler's source being read; the renderer's Shader
effect (`crates/pf-render/src/glsl.rs`) and the importer's translation of it
(`crates/pf-xlights/src/sequence/shaders.rs`, `shader` in `effects.rs`) likewise. What they follow
is format: the Interactive Shader Format's header keys (`INPUTS`, `NAME`, `TYPE`, `MIN`, `MAX`,
`DEFAULT`, `LABELS`, `VALUES`, `PASSES`, `IMPORTED`, and the rest) and built-in names (`TIME`,
`RENDERSIZE`, `isf_FragNormCoord`, `TIMEDELTA`, `FRAMEINDEX`, `DATE`) from the format's public
description; GLSL's own language and built-in functions; and the setting names an xLights sequence
saves for a Shader effect, with how their values map, worked out from sequences and xLights'
rendered frames. The eight shaders that come with PixelFlow (`crates/pf-shader/shaders/*.fs`)
were written for it from scratch and are credited to the project's author.

The Moving Head and Servo effects are PixelFlow's own design (`crates/pf-render/src/heads.rs`,
the head and servo parts of `crates/pf-render/src/dmx.rs`, and `app/src/lib/headEffects.ts`):
their settings, patterns, fan and timing were not taken from xLights' effects of those names,
whose source wasn't read. The importer's translation of xLights' two effects (`moving_head` and
`servo` in `crates/pf-xlights/src/sequence/effects.rs`) follows format only: the setting names and
value layout seen in public sample sequences (the test sequences at
`github.com/derwin12/xlights-tests`, `Effect_MovingHead.xsq` and `Effect_Servo.xsq`, read as data
and not copied into the repository) and the xLights manual's description of the controls. That
repository says its sequences are written by a script, so they show what xLights accepts rather
than what its editor saves; the script itself wasn't read. The corpus's own cases of both effects
are written by `madeup.rs`.

**Third-party shader files are never bundled.** Shader files from shared collections are their
authors' work under their own licences. None is in the repository, its tests, fixtures, or
documents; the tests use shaders written in them. A show's own files are copied into that show's
folder, on the user's computer, by the user's import or choice.

**Tests.** No unit test compares against frames rendered by xLights, and there are no golden
files from xLights in the repository. (A separate tool, `pf-fidelity`, measures an imported
sequence against the `.fseq` xLights itself rendered for it, from files the user supplies: see
[../fidelity.md](../fidelity.md). It compares output with output and reads no xLights source,
which is also what describing behaviour for a clean-room rewrite would need.) The effect tests
assert properties (cells lit, counts, positions worked out from
the formula in the test's comment). A rewrite that kept the same visible behaviour and settings
would be expected to pass most of them. Tests of the shared render code assert particular xLights
rules by number (the blur box table, sub-buffer edge rounding, group grid sizes, the sparkle
ratio, the `> width` wrap). The geometry tests are the strictest: their expected node positions
were worked by hand from the xLights formulas, and `pf-xlights/src/shapes.rs` requires
PixelFlow's shapes to land within 0.002 of the ported geometry.

## Direct ports

### Effects (`crates/pf-render/src/`)

Except where a row says so, these files have no test module, so impl equals total lines.

| PixelFlow file | Lines | xLights source followed | Share following it | What matches | Clean-room |
|---|---|---|---|---|---|
| `vumeter.rs` | 1,550 (impl 1,511) | `effects/VUMeterEffect.cpp` (the `Render*Frame` functions, `Draw*` shapes, `GetTimingEvent`, `ApplyGain`); `render/Effect.cpp` `FilteredIn`; `render/RenderBuffer.cpp` `GetMultiColorBlend` | about 90% | About 45 meter types and 20 shapes, step for step: the 0.05 fall per frame, peak-hold counter, `255 / keep` alpha steps. Shape outlines and star constants identical. | **L** |
| `shape.rs` | 496 | `effects/ShapeEffect.cpp` (`Render`, `Draw*` for 12 shapes, `AddShape`, `DecodeShape`) | about 70% direct, 17% constants | Each drawer mirrors its C++ counterpart (step constants 0.75/0.6/0.05, star offsets, `2.618034`, heart curve, candy-cane fractions, per-shape rotations). The shape lifecycle is reorganised. | **L** |
| `tendril.rs` | 489 | `effects/TendrilEffect.cpp` (`ATendril`, `Tendril`, `Render`); `RenderBuffer.cpp` `DrawAALine`, `DrawAACircle` | about 90% | Spring update order and clamps, the ten movement cases with xLights' `_mv1.._mv4` state, slider mappings (`/20*0.2+0.4`, `/39*0.039+0.96`), anti-aliased line and circle with parallel local names. | **L** |
| `snowflakes.rs` | 448 | `effects/SnowflakesEffect.cpp` (`MoveFlakes`, `possible_downward_moves`, `DrawSnowflakes`, `AdvanceState`) | about 90% | The falling and piling rule, the `1/2/4` move bits, the 20-try refill, speed gate `(f*(speed+1))/30`. | **M** |
| `morph.rs` | 245 | `effects/MorphEffect.cpp` (`Render`, `StoreLine`, `calcPosition`); `RenderBuffer.cpp` `calcAccel`, `DrawThickLine` | 85 to 90% | `StoreLine` statement for statement; repeat, stagger, head and tail maths in the same order with the same sentinels. | **M** to **L** |
| `butterfly.rs` | 211 | `effects/ButterflyEffect.cpp` `Render`; `ispc/ButterflyFunctions.ispc` | about 95% | All ten per-pixel formulae, `h2rgb`, the special case for two pixels, the truncated pi kept on purpose. | **M** |
| `fan.rs` | 149 | `effects/FanEffect.cpp` `Render`; `ispc/FanFunctions.ispc` | about 95% | Setup in the same order, the grow and shrink branches, the per-pixel angle test and edge blend. | **M** |
| `circles.rs` | 215 | `effects/CirclesEffect.cpp` (`RenderRadial`, `RenderMetaBalls`, `UpdateCacheState`), `CirclesEffect.h` | about 45% direct, 35% constants (likely) | Radial and plasma looks line by line (thresholds 0.30, 0.90). Ball motion is PixelFlow's closed form with xLights' constants. | **M** |
| `lines.rs` | 204 | `effects/LinesEffect.cpp`; `RenderBuffer.cpp` `DrawThickLine`, `DrawLine` | about 90% | Bounce tests in the same order, trail trimming, alpha `255*i/size`. | **S** |
| `life.rs` | 132 | `effects/LifeEffect.cpp`; `ispc/LifeFunctions.ispc` | about 50% (likely) | Seeding count `W*H*density/200 + 1`, pacing, the five rule sets in order. The generation step is ordinary Game of Life. | **S** |
| `garlands.rs` | 89 | `effects/GarlandsEffect.cpp` `Render`; `ispc/GarlandsFunctions.ispc` | about 90% | Spacing, direction folding, ring position step for step; four swag patterns. | **S** |
| `fill.rs` | 315 (impl 116, with a test module) | `effects/FillEffect.cpp` (`Render`, `UpdateFillColor`, `GetColorFromPosition`), read at tag `2024.19` | about 90% | The four directions' loops with their targets in xLights' arithmetic (truncated), the offset as cells or `(size − 1) × offset / 100`, the palette position `line / (size + offset − 1)`, and the band and gap counter. One loop serves each pair of directions. | **S** |
| `fire.rs` | 382 (impl 158, with a test module) | `effects/FireEffect.cpp` (`Render`, `FirePaletteClass`), read at tag `2024.19` | about 90% | The edge row seeded `190 + rand % 10` and `100 + rand % 50` in turn, the mean of the row before with the cell straight behind counted twice, the one-in-five step of `255 * 100 / height / percent`, the 200-shade palette (hue step `0.00166666`), the hue shift stopped at 1, `10 + 90 × peak` for the music, and the four locations. Random numbers come from the effect's seed and the frame rather than `rand()`. | **S** |

### Shared render code with a ported function group (`crates/pf-render/src/`)

| PixelFlow file | Lines (impl) | xLights source followed | Lines following it | What matches | Clean-room |
|---|---|---|---|---|---|
| `styles.rs` | 759 (481) | `models/ModelGroup.cpp` (`GetBufferSize`, `InitRenderBufferNodes`), `models/Model.cpp` (Per Preview) | about 215 | `grid_cells` and `preview_cells` step for step (2048 cap, `count*5 > area`, 400 cap). Group stacking arms are likely ports, re-expressed per pixel. | **M** |
| `text.rs` | 419 (419) | `effects/TextEffect.cpp` (`RenderTextLine`, the `zigzag` macro, `FormatCountdown`) | about 190 | Movement offsets case by case (`xlimit = totwidth*8 + 1`), countdown formatting including "Invalid Format". Layout, colouring, and the font are not from xLights, nor is drawing in a computer's fonts (`pf-fonts`, under "Independent"). | **M** |
| `geometry.rs` | 1,353 (786) | `models/SubModel.cpp` (`initDefaultBuffer`, `initSubbufferRange`), `models/Model.cpp` | about 190 | `grid_buffer` step for step; the sub-buffer edge rounding (start edge rounded, end truncated). | **M** |
| `color.rs` | 617 (377) | `effects/ispc/LayerBlendingFunctions.ispc` | about 165 | `to_hsv` and `from_hsv` (both `1e-20` epsilons), and 16 blend arms branch for branch (`/ 5.0` shadow, `/ 2.0` shadow below). Normal, Add, Max, Multiply, and coverage are PixelFlow's. | **S** |
| `raster.rs` | 340 (269) | `render/RenderBuffer.cpp` (`SetPixel`, `DrawLine`, `DrawThickLine`, `DrawCircle`, `DrawFadingCircle`) | about 150 | The gap-filling thick line case for case, the eight-point circle, and the `> width` wrap quirk (asserted by a test). The underlying algorithms are textbook. | **S** |
| `blur.rs` | 577 (420) | `render/PixelBuffer.cpp` (`boxesForGauss`, `gaussBlur_4`, `Blur`) | about 130 | The box-size table and the thresholds that pick a blur. The box passes are organised differently. | **S** |
| `carried.rs` | 36 (36) | `RenderBuffer.cpp` `calcAccel`, `GetEffectTimeIntervalPosition` | about 25 | The two timing rules nine effects share (where an effect is in its cycle, and acceleration), kept apart from the effects that call them until they are replaced. | **S** |
| `effects.rs` | 1,169 (1,169) | `ShapeEffect.cpp` music trigger | about 35 | One small helper. The effects themselves are closed-form and PixelFlow's (see the note on what was compared, below). | **S** |
| `picture/resample.rs` | 458 (279) | wxWidgets `src/common/image.cpp` (`wxImage::Scale`, `ResampleNearest`, `ResampleBilinear`, `ShrinkBy`), read at tags `xlights_2024.06` to `xlights_2026.02` of xLights' wxWidgets (wxWindows Library Licence 3.1); xLights `effects/PicturesEffect.cpp` | about 45 | The two oldest ways xLights resized a picture: the `(old << 16) / new` stride from the corner, and the `(old − 1) / (new − 1)` placing with the two nearest pixels mixed, colour and alpha each on its own; which applies when (shrinking, a whole-number shrink). PixelFlow's own averaging and the rest of the file are independent. | **S** |
| `adjust.rs` | 247 (98) | `render/PixelBuffer.cpp` (`GetMixedColor`, `LayerInfo::calculateNodeOutputParams`), read at tag `2024.19` | about 60 | The order (hue, saturation, and value before the sparkles; brightness and contrast after), the hue wrap and the clamps, contrast's split at a value of 0.5 after brightness, and an unlit pixel counting as black on a layer that isn't blended Normal. | **S** |
| `sparkles.rs` | 119 (60) | `LayerBlendingFunctions.ispc` `ApplySparkles`; `PixelBuffer.cpp` | about 25 | `% (208 - count)`, the brightness steps 0.53 / 0.75 / 1.0, phase `% 10000`. | **S** |

Not every xLights counterpart of the `effects.rs` effects was opened. On, Color Wash, Shimmer,
and Fire were compared and share no structure or constants with xLights. Bars, Wave, Twinkle,
Strobe, Spiral, and Meteors were judged from the form of PixelFlow's code alone.

### Model geometry in the importer (`crates/pf-xlights/src/`)

These place every pixel of an imported xLights prop where xLights places it, which is the
importer's stated purpose.

| PixelFlow file | Lines | xLights source followed | Share | Tables carried over | Clean-room |
|---|---|---|---|---|---|
| `geometry/poly.rs` | 708 | `models/PolyLineModel.cpp`, `MultiPointModel.cpp`, `Shapes.cpp` (`BezierCurveCubic3D`), `PolyPointScreenLocation.cpp` | 85 to 90% | 25-step curves, corner offsets; each node's cell in the model's own render buffer (`bufX`, `bufY`, `SetBufferSize`) | **M** |
| `geometry/grid.rs` | 655 | `MatrixModel.cpp`, `TreeModel.cpp`, `SphereModel.cpp`, `CubeModel.cpp` | about 95% | Cube orientation table (48 rows of 4), identical row for row; tree and sphere constants | **M** |
| `geometry/radial.rs` | 589 | `CircleModel.cpp`, `WreathModel.cpp`, `StarModel.cpp`, `SpinnerModel.cpp`, `WindowFrameModel.cpp` | about 95% | Window-frame start and index tables; star start rules | **M** |
| `geometry/lines.rs` | 498 | `SingleLineModel.cpp`, `ChannelBlockModel.cpp`, `ArchesModel.cpp`, `CandyCaneModel.cpp`, `IciclesModel.cpp` | about 95% | Cane proportions, drop pattern default; a single line's and an icicle run's render-buffer cells | **M** |
| `geometry/xform.rs` | 273 | `BoxedScreenLocation.cpp`, `TwoPointScreenLocation.cpp`, `ThreePointScreenLocation.cpp`, `PolyPointScreenLocation.cpp` | about 170 lines | Matrix order, the `3.14159` literal, the 0.001 nudge | **S** |
| `geometry/custom.rs` | 206 | `CustomModel.cpp` (`InitCustomMatrix`, `SetStringStartChannels`, `ParseCustomModel`, `ParseCompressed`), read at tag `2024.19` | about 110 lines (likely); the rest is format reading | The render-buffer cell `layer * width + col`, `height - row - 1`, on the grid the cell data spans | **S** |
| `geometry.rs` | 732 | `Model.cpp` (`GetNodeChannelCount`, `CalcChannelsPerString`, `SetStringStartChannels`, `ComputeStringStartNode`), `Model.h` | about 190 lines; the rest is PixelFlow's harness | String-type to channel-count rules | **S** |

As a set these are **L**: reproducing node order and position exactly is the goal, so a
clean-room version needs positions exported from xLights for every model type and option, not
the source.

### Value curves

| PixelFlow file | Lines (impl) | xLights source followed | Share | Clean-room |
|---|---|---|---|---|
| `crates/pf-xlights/src/sequence/curves.rs` | 798 (627) | `render/ValueCurve.cpp` (`RenderType`, `GetRangeParm1..4`, `Normalise`, `Deserialise`, the Random generator) | about 400 lines direct, 110 behaviour-matching | **M** |
| `crates/pf-sequence/src/curve.rs` | 699 (495) | `ValueCurve.cpp` `GetValueAt` (music-trigger and timing-track branches), `ApplyGain` | about 130 lines; the ramp, sine, square, saw, and custom curves are PixelFlow's | **S** |

`curves.rs` carries each curve type's formula and constants (steps 0.05 and 0.025, the shortened
`6.283185307`), the table of which parameters use the setting's own range, and the random
generator's two seed constants.

### Shape generators (`crates/pf-geometry/src/`)

The shapes PixelFlow's own props are built from. Several place pixels the way the xLights model
of the same name does.

| PixelFlow file | Lines (impl) | xLights source followed | Notes | Clean-room |
|---|---|---|---|---|
| `cube.rs` | 334 (214) | `models/CubeModel.cpp` (`transformations`, `RotateX/Y/Z90Degrees`, `BuildCube`) | The 48 by 4 table again; the build loop statement for statement | **M** |
| `tree.rs` | 306 (161) | `TreeModel.cpp` `SetTreeCoord` | The ten-stretch spiral (about 47 lines) is direct; positions follow the formulae | **M** |
| `arch.rs` | 256 (124) | `ArchesModel.cpp` (layered branch, `SetLayerdArchCoord`) | Layered arches (about 54 lines) direct, with xLights' variable names; plain arches are PixelFlow's | **S** to **M** |
| `window_frame.rs` | 242 (114) | `WindowFrameModel.cpp` `InitFrame` | Both tables identical; the function is named `xlights_layout` | **M** |
| `candy_canes.rs` | 217 (84) | `CandyCaneModel.cpp` `SetCaneCoord` | Likely: restructured into one loop, same proportions and angle formula | **S** to **M** |
| `spinner.rs` | 202 (65) | `SpinnerModel.cpp` `SetSpinnerCoord` | | **S** |
| `sphere.rs` | 175 (87) | `SphereModel.cpp` `SetSphereCoord` | The formula block, with xLights' names (`remove`, `fudge`) | **S** |
| `icicles.rs` | 172 (82) | `IciclesModel.cpp` `InitModel` | | **S** |
| `circle.rs` | 119 (54) | `CircleModel.cpp` `SetCircleCoord` | Likely: a small, generic function | **S** |
| `wreath.rs` | 95 (42) | `WreathModel.cpp` `InitWreath` | | **S** |

### In the UI

| PixelFlow file | Lines | Follows | Notes | Clean-room |
|---|---|---|---|---|
| `app/src/lib/geometry.ts` | 691 | The same xLights model files, by way of `pf-geometry` | A TypeScript mirror of the shape generators for the browser preview: the cube table, `cubeCells`, `spiralOffsets`, `windowFrame`, `wreath` | **M** to **L** (the sum of the rows above) |

## Behaviour-matching

| PixelFlow file | Lines (impl) | Source it draws on | What was taken | Clean-room |
|---|---|---|---|---|
| `crates/pf-render/src/plasma.rs` | 530 | A written description of what the effect shows ([../dev/effect-specs/plasma.md](../dev/effect-specs/plasma.md)), measured from frames the earlier port of `effects/PlasmaEffect.cpp` drew and checked against frames xLights itself rendered | The picture: the seven waves the field is made of, as measured (their directions, wavelengths and speeds), the five colourings, the pace counted in frames, each pixel taking its cell's colour. No code or structure was carried over: wave angles are whole-number steps of a turn and sines come from a parabola across one of 512 arcs. See "Rebuilt from a description of its behaviour". | done |
| `crates/pf-render/src/shockwave.rs` | 326 (222) | A written description of what the effect shows ([../dev/effect-specs/shockwave.md](../dev/effect-specs/shockwave.md)), measured from frames the earlier port of `effects/ShockwaveEffect.cpp` drew and checked against frames xLights itself rendered | What a setting's number means in cells and frames, to the cell and the frame: whole-cell sizes and centre, percent of the longer side, the half-cell least width, the soft edge in 255ths, the palette by progress. It calls the two timing rules in `carried.rs`, which are still carried-over code. No code or structure was carried over: a pixel off the ring is answered from two whole numbers worked out once a frame. See "Rebuilt from a description of its behaviour". | done |
| `crates/pf-render/src/pinwheel.rs` | 989 (764) | A written description of what the effect shows ([../dev/effect-specs/pinwheel.md](../dev/effect-specs/pinwheel.md)), drawn up from the earlier port of `effects/PinwheelEffect.cpp` and `ispc/PinwheelFunctions.ispc`; and frames xLights itself rendered | About 200 lines carry what an import relies on: where the first arm points and which way arms are numbered in each style, 20 degrees a second for each unit of speed, which palette colour each arm takes, what an arm length of 100 reaches, and, for the older "spokes" style, where xLights' light falls as measured in its frames (whole-degree rays, marks half a cell apart, shading by ray). No code, formula, or structure was carried over: directions are whole-number steps of a turn, each pixel is answered from its own place, and nothing is drawn into a buffer. See "Rebuilt from a description of its behaviour". | done |
| `crates/pf-xlights/src/sequence/effects.rs` | 3,564 (2,158) | The static defaults, ranges, and divisors in `effects/*Effect.cpp`; each effect's `adjustSettings` | A translator PixelFlow organised itself. The setting key names are format. The per-key defaults, ranges, divisors, and slider-to-behaviour conversions are more than format: 136 of 138 numeric defaults and 92 of 93 ranges compared are identical to the `.cpp` statics. About 137 numeric keys, 41 choice keys with their value lists, 61 checkbox keys, 18 divisors, for 31 effects. | **M** |
| `crates/pf-sequence/src/effect.rs` | 2,884 (2,754) | The same statics | About 1,070 lines: the settings (defaults and ranges) of 14 effects mirror xLights', as do the choice lists (44 VU Meter types, 18 VU shapes, 15 Shape objects, 10 tendril movements) and the names of 16 blend modes. The other effects' settings, and the file's structure, are PixelFlow's. | **M** |
| `crates/pf-xlights/src/shapes.rs` | 2,690 (1,262) | The model files listed above | Converts an xLights model's attributes to PixelFlow shape settings, keeping a result only if it lands on the ported geometry. No coordinate tables. About 100 lines are near-verbatim fragments (the arch layer loop, star inflation); it also carries about 71 attribute names with their defaults. | **M** to **L** |
| `crates/pf-model/src/shape.rs` | 1,365 (830) | Model option sets | Type definitions. The option lists match xLights' (cube start 8, cube style 6, in the same order), with three defaults. Light. | **S** |
| `crates/pf-xlights/src/channels.rs` | 287 (145) | `Model.cpp` (`GetNumberFromChannelString`) | The meaning of the five start-channel reference forms. Likely; mostly format. | **S** |
| `crates/pf-geometry/src/star.rs` | 182 (90) | `StarModel.cpp` `InitModel` | Start-angle rules. Structure is PixelFlow's. | **S** |
| `crates/pf-geometry/src/polyline.rs` | 192 (103) | `PolyLineModel.cpp`, `Shapes.cpp` | 25 curve steps and half-gap spacing. Likely. | **S** |
| `crates/pf-lexicon/src/phones.rs` | 270 (215) | `resources/dictionaries/phoneme_mapping`; `lyrics/PhonemeDictionary.cpp` | The mouth-shape table: see "Borrowed data". | **S** |
| `crates/pf-ai/src/lyrics/syllables.rs` | 367 (183) | `lyrics/LyricBreakdown.cpp` `BreakdownWord` | `shape_marks` (37 lines) follows the timing arithmetic: 50 ms for short shapes, the same short set, the same division. Applied per syllable, not per word. | **S** |
| `crates/pf-render/src/faces.rs` | 344 (228) | `effects/FacesEffect.cpp` | About 55 lines: the blink loop's shape (with PixelFlow's own timings) and palette roles. | **S** |
| `crates/pf-render/src/picture/mod.rs` | 398 (320) | `effects/PicturesEffect.cpp` | About 25 lines: centring, scroll positions, black as clear. | **S** |
| `crates/pf-render/src/render.rs` | 543 (543) | `LayerBlendingFunctions.ispc` | One threshold (`rows / 2`). Otherwise independent. | **S** |
| `crates/pf-analysis/src/track.rs` | 869 (729) | `AudioManager::CalculateSpectrumAnalysis` (not read) | About 40 lines: the note spectrogram (127 notes, 2048-sample chunks). Per PixelFlow's comments. | **S** |
| `crates/pf-mapping/src/wiring.rs` | 233 | xLights (rule only) | Ports counted in channels of three; smart receivers share a port's limit. | **S** |
| `crates/pf-devices/src/falcon.rs` | 452 (410) | `controllers/Falcon.cpp`, `resources/controllers/falcon.xcontroller` | Product code to model (10), board mode to ports (12), controller modes, per-port limits. The query protocol is format. Structure is PixelFlow's. | **S** to **M** |
| `crates/pf-devices/src/adapter.rs` | 1,831 (1,818) | xLights `FPP.cpp`; FPP `PixelString.cpp`, `Sequence.h` (not read); WLED `const.h` (not read) | Small parts: the 11 keys of a new FPP string, limits (1,600 pixels, 8,192 by 1,024 channels), the checks fppd makes when loading a string. | **S** |
| `crates/pf-devices/src/fpp_ping.rs` | 183 (81) | FPP `docs/ControlProtocol.txt`, `MultiSync.cpp` (not read) | Packet layout and port. Wire constants throughout. Likely. | **S** |
| `crates/pf-devices/src/wled.rs` | 158 (146) | WLED `wled00/const.h` (not read) | Bus type and colour-order codes. Likely. | **S** |
| `crates/pf-devices/src/fingerprint.rs` | 64 (37) | xLights' Falcon detection (not found in the files read) | Four page markers. | **S** |
| `crates/pf-devices/src/fake_fpp.rs`, `fake_wled.rs` | 1,001 and 171 | FPP's PHP and `PixelString.cpp`, WLED's `cfg.cpp` (not read) | Test doubles (the `test-fixtures` feature; not in the app) modelled on upstream behaviour. | **S** to **M** |
| `crates/pf-devices/src/fpp_upload.rs` | 975 (878) | FPP's `MoveFile()` (not read) | The list of 9 music file types FPP files as music. The rest is the FPP upload protocol (format reading) and PixelFlow's own staging; the 4 MB chunk size is PixelFlow's. | **S** |
| `crates/pf-fseq/src/writer.rs` | 306 (273) | FPP's fseq writer (not read) | First block of 10 frames at level -10; 4-byte header padding. Likely. | **S** |
| `app/src/api/effectCatalog.json` | 3,803 | Generated from `pf-sequence` | The same settings tables, as data for the UI. | with `effect.rs` |
| `app/src/lib/controllerKinds.ts` | 57 | `falcon.xcontroller`, `Falcon.cpp` | Port limits 1,024 and 704. | **S** |
| `app/src/lib/submodels.ts` | 447 | `SubModel.cpp` | One formula (`cellSpan`). | **S** |
| `app/src/lib/renderStyles.ts`, `wiringMath.ts` | 69 and 610 | xLights | Style and layout names; the 704-pixel rule. | **S** |

## Format reading only

These read or write another program's files or talk a device's protocol. They carry names, keys,
and layouts, not algorithms.

| Area | Files | What they read or write |
|---|---|---|
| xLights show and sequence files | `crates/pf-xlights/src/`: `import.rs`, `layout.rs`, `model.rs`, `networks.rs`, `submodels.rs`, `background.rs`, `timing.rs`, `lib.rs`, `sequence/mod.rs`, `sequence/settings.rs`, `sequence/xsq.rs`, `vendor/mod.rs`, `vendor/xmap.rs` | `xlights_rgbeffects.xml`, `xlights_networks.xml`, `.xsq`, `.xtiming`, `.xmap`, vendor packages. `submodels.rs` also matches about 110 lines of `SubModel.cpp`'s parse rules, and `background.rs` about 60 lines of placement. |
| Vocabulary | `crates/pf-model/src/render_style.rs`, `region.rs` | 17 render style names, 14 group layouts, 8 buffer transforms (with their 8 one-line formulas), submodel line syntax, 10 phoneme names. |
| Lyric tracks | `crates/pf-ai/src/lyrics/tracks.rs` | The phrase, word, and phoneme track layout, so tracks export to `.xtiming`. |
| FPP and controller web pages | `crates/pf-devices/src/`: `fpp.rs`, `fpp_info.rs`, `fpp_download.rs`, `fpp_software.rs`, `fpp_player.rs`, `discover.rs`, `identify.rs` | FPP's REST endpoints and JSON fields; which pages identify a controller. `fpp.rs` also carries one rule from FPP's `DDP.cpp` (which DDP type starts at channel 0). |
| `.fseq` | `crates/pf-fseq/src/header.rs`, `sequence.rs` | The FSEQ v2 layout, from FPP's published format description. |
| UI | `app/src/lib/fppHealth.ts`, `app/src/components/sequencer/effectControls.tsx` | A pattern for FPP's warning text; help text naming xLights layer methods. |

### `pf-devices`, file by file

No file in `pf-devices` is a port. Its 25 files divide as follows.

| Category | Files | Which |
|---|---|---|
| Direct port | 0 | |
| Behaviour-matching, with tables or constants from source | 8 | `falcon.rs`, `adapter.rs`, `fpp_ping.rs`, `fingerprint.rs`, `wled.rs`, `fpp_upload.rs`, and the test doubles `fake_fpp.rs` and `fake_wled.rs` |
| Format and protocol reading only | 7 | `fpp.rs`, `fpp_info.rs`, `fpp_download.rs`, `fpp_software.rs`, `fpp_player.rs`, `discover.rs`, `identify.rs` |
| Independent | 10 | `config.rs`, `device.rs`, `error.rs`, `http.rs`, `import.rs`, `lib.rs`, `reach.rs`, `setup.rs`, `testing.rs`, `xml.rs` |

The independent files were checked for citations of other projects' source and found to have
none; they were not all read line by line.

## Independent

Everything not listed above. In particular: `pf-shader` (see the summary above for what its
format follows), `pf-engine`, `pf-output` (sACN and DDP from their
public specifications), `pf-fidelity` (which compares rendered output and cites no source),
`pf-audio`, `pf-analysis` apart from the note spectrogram, `pf-align`,
`pf-camera-map`, `pf-video`, `pf-patterns`, `pf-frame`, `pf-cli`, `pf-ai` apart from
`shape_marks`, `pf-lexicon` apart from the mouth table (its letter-to-sound rules and
syllabifier have no xLights counterpart), `pf-xlights`'s XML reader, vendor package reader, and
auto-mapping, `pf-sequence`'s documents, timing, and validation, `pf-model` apart from the files
named, the desktop shell (`app/src-tauri`), and the UI apart from the files named.

Written for PixelFlow where a library could have stood instead:

| Part | Where | What it is | Notes |
|---|---|---|---|
| Finding the computer's fonts, and drawing their letters | `crates/pf-fonts` (`find.rs`, `sfnt.rs`, `raster.rs`, `sized.rs`), used by the Text effect (`pf-render/src/text.rs`), the importer, and the Assistant's review | PixelFlow's own (October 2026; `sfnt.rs` November 2026). It looks through the system's, everyone's, and the user's font folders on macOS, Windows, and Linux, reading only each font file's table directory and its `name`, `OS/2`, and `head` tables; it reads the tables a letter's glyph, place, and size come from (character maps, advances, line metrics, kerning, composite letters); it cuts a letter's outline into short lines and fills it by the exact area each pixel has inside it, so edges are partial brightness; it makes up a bold or an italic a family lacks; and it keeps each letter drawn once a size. | The letters' outlines (TrueType points and curves, CFF charstrings) are read with `skrifa` (Google Fonts' fontations; MIT OR Apache-2.0), a dependency, not copied. Until November 2026 the font files were read with `ttf-parser` (MIT OR Apache-2.0), no longer kept up; `sfnt.rs` reads the other tables as it did, quirks included, so that moving changed nothing a show draws: `ttf-parser`'s source was read to match its behavior, and none of it was copied. Filling an outline by signed area is a well-known technique; no rasterizer's source (FreeType, font-rs, fontdue, ab_glyph) and no font finder's (fontdb, fontconfig) was read. No font file ships: the Text effect uses the fonts installed on the user's computer. How xLights describes a font in a sequence ("bold 'arial narrow' 12 utf-8", wxWidgets' words for a font) is read as format. |

## Borrowed data

| Data | Where in PixelFlow | Origin | Notes |
|---|---|---|---|
| Mouth-shape (Preston Blair) mapping | `crates/pf-lexicon/src/phones.rs`, `Arpa::mouth`: 39 phones to 9 shapes | xLights `resources/dictionaries/phoneme_mapping` | Identical for all 39 phones. That file's own header reads "Papagayo ... Copyright (C) 2005 Mike Clifton" and places it under the GNU GPL, version 2 or any later version. PixelFlow's comment names the origin ("xLights' `phoneme_mapping` (from Papagayo)"). Any mapping that makes faces sing as xLights does will have the same content. |
| CMU Pronouncing Dictionary | `crates/pf-lexicon/data/cmudict.zst` (670 KB, about 125,000 words), built into the binary | `cmudict.dict` from <https://github.com/cmusphinx/cmudict> at commit `74790861f652` (CMU's own distribution, not xLights' copy), by `crates/pf-lexicon/examples/build_cmudict.rs` | BSD-style two-clause licence, in `crates/pf-lexicon/data/cmudict-LICENSE.txt`. Before this change the built app carried the data without the notice; the notices file now includes it. |
| Shape outlines | `shape.rs` and `vumeter.rs`: tree (14 segments), cross (12), present (9); the order of 13 random shapes | `ShapeEffect.cpp`, `VUMeterEffect.cpp` | Identical in content and order. |
| Snowflake shapes, swag patterns | `snowflakes.rs` (8 flake types), `garlands.rs` (4 patterns) | `SnowflakesEffect.cpp`, `GarlandsFunctions.ispc` | |
| Cube orientation table | `pf-geometry/src/cube.rs`, `pf-xlights/src/geometry/grid.rs`, `app/src/lib/geometry.ts` | `CubeModel.cpp` | 48 rows of 4, identical. |
| Window-frame tables | `window_frame.rs`, `geometry/radial.rs`, `geometry.ts` | `WindowFrameModel.cpp` | |
| Effect defaults and ranges | `pf-xlights/src/sequence/effects.rs`, `pf-sequence/src/effect.rs`, `app/src/api/effectCatalog.json` | `effects/*Effect.cpp` | See "Behaviour-matching". |
| Value-curve formulae and seeds | `pf-xlights/src/sequence/curves.rs` | `ValueCurve.cpp` | |
| Gaussian box sizes | `blur.rs` | `PixelBuffer.cpp` `boxesForGauss` | A published approximation; the exact table and thresholds are xLights'. |
| Logarithmic scale | `vumeter.rs`, `LOG_X` (127 values) | Attributed in the file to xLights' `LogarithmicScale` (not read) | The values spot-checked equal `127*ln((i+2)/(i+1))/ln(127)`, so the table can be regenerated from that formula. |
| Falcon tables | `pf-devices/src/falcon.rs`, `app/src/lib/controllerKinds.ts` | `Falcon.cpp`, `falcon.xcontroller` | Facts about the hardware, as xLights records them. |
| FPP limits and checks | `pf-devices/src/adapter.rs`, `fake_fpp.rs` | FPP source, per PixelFlow's comments | FPP's `LICENSE` file puts its core under LGPL 2.1 or later and its channel outputs under GPL 2 or later. |
| WLED codes | `pf-devices/src/wled.rs` | WLED `const.h`, per PixelFlow's comments | WLED's repository is under EUPL 1.2. |
| 5 by 7 pixel font | `pf-render/src/text.rs`, `FONT` (95 glyphs) | Not stated; the file calls it "the classic 5 x 7 one" | Not from xLights. The table resembles one that circulates widely in display libraries; its origin and terms were not established. |
| Fire constants | `pf-render/src/effects.rs`, `Fire` | Not stated | The cooling and spark constants resemble the FastLED "Fire2012" demo (MIT). Not verified; the code's structure is PixelFlow's. |
| Device fixtures | `crates/pf-devices/fixtures/` | Per its README: `fpp/` and `falcon/` recorded from the owner's own devices with addresses, MAC, and serial removed; `fpp-hat/` and `wled/` synthetic; `falcon/synthetic/` built from the sample replies quoted in comments in `Falcon.cpp` (about 1.2 KB); `fpp/github_releases.json` a trimmed public release list | Tests only. They hold no credentials. The details of the owner's own show that the recordings carried (prop names, a sequence name, and a controller name whose suffix may have derived from a MAC address) have been replaced with made-up ones, here and in the tests that use them; the earlier values remain in the repository's history. |
| FPP oracle | `scripts/fseq-fpp-oracle.sh` | Fetches FPP's `src/fseq` when run, into a scratch directory | Nothing from FPP is copied into the repository. The script holds a 45-line harness written against FPP's `FSEQFile` API. |

## What a clean-room rewrite would involve

Nothing here has to be rewritten: under the GPL the ported code may stay, credited. This table is
what each rewrite would take, for choosing the ones that pay for themselves in speed, simplicity,
or fidelity.

| Piece | Effort | What it takes |
|---|---|---|
| Model geometry: `pf-xlights/src/geometry*`, the ten `pf-geometry` generators, `geometry.ts`, then `shapes.rs` on top | **L** | Export node positions from xLights for every model type, start corner, and option, and write generators to match. The cube and window-frame tables fall out of that. |
| VU Meter | **L** | About 45 meter types and 20 shapes to specify from behaviour. |
| Shape, Tendril | **L** each | Twelve shape drawers (tree, cross, and present outlines redrawn, so no longer pixel-identical); ten tendril movements. |
| Morph | **M** to **L** | Head, tail, stagger, and palette rules are not documented publicly. |
| Snowflakes, Butterfly, Fan, Circles | **M** each | Pinwheel, of the same size, took under a day this way, most of it measuring: see below. |
| Plasma, Garlands, Life, Lines | **S** each | Public techniques or small effects. |
| Value curves (`curves.rs`, and 130 lines of `curve.rs`) | **M** | Each curve type's maths as a written rule. The file keys are format and stay. |
| Effect settings: `sequence/effects.rs` and the 14 mirrored tables in `pf-sequence` | **M** | Re-collect about 240 defaults and ranges from the xLights effect panels or saved sequences rather than the source; saved PixelFlow sequences may need migrating. |
| Group and submodel buffers: `styles.rs`, `pf-render/src/geometry.rs`; Text movement | **M** each | Rounding and scaling rules are asserted by tests and need characterising. |
| Drawing, blur, blend, HSV, sparkles | **S** each | Textbook algorithms plus a few xLights-specific rules. |
| Mouth-shape table, `shape_marks`, Faces, Picture placement, Falcon tables, the rest | **S** each | |

### Rebuilt from a description of its behaviour

Pinwheel was the first effect taken through this, in October 2026, as a trial of the method.

1. **Described.** The old `pinwheel.rs` (about 80% a direct port, 269 lines) was read and what it
   shows was written down: each setting's visible meaning, motion, colour, geometry, edge cases,
   25 properties a test can measure, and a list of blemishes that came from how it was computed
   and were not to be copied. No code, formula, constant other than the settings' ranges and
   defaults, variable name, or order of computation went into the description. It is kept at
   [../dev/effect-specs/pinwheel.md](../dev/effect-specs/pinwheel.md).
2. **Cleared.** The old file, its three tests, and its row in this inventory (which quoted its
   arithmetic) were removed on the branch before any new code was written.
3. **Built.** The new file was written from the description and from frames xLights rendered for
   real sequences, without the old file, its history, or xLights' source being consulted. Its
   tests are the description's properties.
4. **Judged.** By how an import looks against xLights' frames ([../fidelity.md](../fidelity.md))
   and by its cost ([../benchmarks.md](../benchmarks.md)); then drawn side by side with the old
   file on about 4,600 settings to catch conventions the description might have got wrong.

What it showed about the method: the description, written from reading, was wrong in three places
that only xLights' frames or the side-by-side drawing revealed (where the centre settings put the
centre, how shading falls on the rays of the older style, and that a twisted arm there must light
every cell it passes through), and xLights' own arm turned out a degree wider than its setting
says, which now lives in the import rather than the effect. Reading is not enough; the frames
have to be measured.

What this inventory does and doesn't say about the result: the new file is organised
independently and shares no code with the old one or with xLights, and it is listed as
behaviour-matching, not independent, because the conventions it keeps reached it by way of a
description drawn up from the port. Whether that makes it free of xLights' licence is the same
legal question as for every other row, and this inventory doesn't answer it. With the whole
project under the GPL it doesn't need answering: the rewrite was done for what it gained (see
[../benchmarks.md](../benchmarks.md) and [../fidelity.md](../fidelity.md)), and the credit to
xLights stands either way.

Shockwave followed, with the method tightened so that the output stays the same. Its description
was measured, not read: the old effect was drawn through the renderer's public interface across
its settings, sizes and times, every rounding was pinned (with the measured cases kept as numbers
where no rule could be stated), and the result was checked against xLights' frames. The new file,
written from that description alone, reproduced every measured number at the first attempt. Drawn
side by side with the old file on 303,780,289 frames it is the same to the bit on every buffer a
show can have; the only difference is on a single row of 439,804,673 cells or more, where the old
centre overflowed. It is listed as behaviour-matching for the same reason as Pinwheel, and it
still calls the two shared timing rules in `carried.rs`, which are carried-over code until they
are replaced in turn.

Plasma followed the same way. Its picture is a smooth field, so its description says what the
field is made of as measured from the output (seven waves, each with a direction, a wavelength
and a speed, fitted to within 0.000004) rather than as the old code wrote it; a build from the
description alone matched the reference table at the first attempt. Drawn side by side with the
old file on 9,188,584,667 pixel-frames, the two differ only by the old file's single-precision
speckle, which grows as an effect runs and which the new file does not copy; on the real
sequences' designs 99.979% of pixel-frames are identical and the rest one step of 255.

Attribute names, setting keys, choice strings, value-curve keys, phoneme names, and the values
xLights assumes when a key is missing from a file are needed to read its files at all, however
the code is written.
