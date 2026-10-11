# Source provenance: what follows xLights (and other) source

This is an inventory of the PixelFlow files that were written from, or with reference to,
another project's source code. It records what the code looks like next to the source it cites.
It is **not legal advice**, and it does not say whether any file is a "derivative work": that is
a legal question, and the author of this inventory is not a lawyer. See
[distribution-checklist.md](distribution-checklist.md) for what follows from it.

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
- Line counts are of the file as it stands. "Impl" leaves out the file's own `#[cfg(test)]`
  module. The share of a mixed file that follows the source is an estimate from reading, not a
  measured diff.
- Where a judgement was uncertain, the more derived category was chosen and marked "likely".

No PixelFlow file carries a copyright or licence notice of its own, and none reproduces
xLights' file header. The files below name the xLights file and function they follow in their
opening comment.

## Categories

| Category | Meaning |
|---|---|
| **Direct port** | Follows the cited source line by line or structure by structure: the same sequence of steps, intermediate values, formulae, constants, and branches, in Rust (or TypeScript) idiom. |
| **Behaviour-matching** | Organised independently, but reproduces specific constants, tables, formulae, or thresholds that came from reading the source. |
| **Format reading** | Reads or writes a file format or wire protocol for interoperability: attribute names, setting keys, packet layouts. No algorithm carried over. |
| **Independent** | PixelFlow's own design. At most it shares public names or visible behaviour. |

Clean-room effort: **S** under a day, **M** days, **L** a week or more. "Clean room" here means
one person writes a description of the behaviour (from watching xLights, its manual, or files it
saves) and a different person, who has not read the xLights source, writes the code from that
description.

## Summary

| Category | Files | Where |
|---|---|---|
| Direct port (the whole file, or most of it) | 31 | 13 effects in `pf-render`; 7 files in `pf-xlights` (model geometry and value curves); 10 shape generators in `pf-geometry`; `app/src/lib/geometry.ts` |
| Mixed: a direct-port function group inside an otherwise independent file | 10 | 8 in `pf-render` (text, raster, blur, color, geometry, styles, sparkles, effects); `pf-xlights/src/geometry.rs`; `pf-sequence/src/curve.rs` |
| Behaviour-matching | 28 | effect default tables, the model-to-shape converter, the mouth-shape table, controller tables, and smaller pieces |
| Format reading only | 27 | the rest of the xLights importer, the FPP REST client, the `.fseq` reader |
| Independent | the rest | including the engine, output, audio analysis, the assistant, camera mapping, video export, the desktop shell, and nearly all of the UI |

Approximate implementation lines by category, for the crates with the most:

| Crate | Impl lines | Direct port | Behaviour-matching | Format reading | Independent |
|---|---|---|---|---|---|
| `pf-render` (41 files) | about 14,200 | about 4,800 | about 400 | 0 | about 9,000 |
| `pf-xlights`, `pf-sequence`, `pf-model` (53 files) | about 23,100 | about 3,400 | about 5,800 | about 5,600 | about 8,300 |
| `pf-geometry` (17 files) | about 1,620 | about 900 | about 330 | 0 | about 400 |
| `app/src/lib/geometry.ts` | 691 | 691 | | | |

For scale: the Rust code is about 168,000 lines including tests. The direct-port lines are
roughly 9,000 to 10,000 of those, and are concentrated in three places: the effects that carry an
xLights effect's name, the model geometry (where each pixel of an imported prop sits), and value
curves.

**The renderer.** About 37% of `pf-render`'s implementation lines follow xLights source. Thirteen
effects (Plasma, Morph, Tendril, Snowflakes, Garlands, Circles, Fan, Butterfly, Pinwheel, Life,
Lines, Shape, VU Meter) are ports of the xLights effect of the same name: 80 to 95% of each
file, except Circles and Life, where about half is ported and the rest is PixelFlow's own
arrangement of xLights' constants or the ordinary algorithm. So are the pieces every layer passes through: line and circle drawing, blur, the xLights layer
blend modes and HSV conversion, sparkles, submodel and group buffer layout, and Text's movement.
The remaining 63% is PixelFlow's own: the render pipeline, the picture loader, the closed-form
effects in `effects.rs` (On, Color Wash, Fade, Chase, Bars, Wave, Twinkle, Shimmer, Strobe,
Spiral, Meteors, Ripple, Fire), and Wipe, Impact, Pulse, Sing, Color Shift, Lightning, Dancer, and
Picture.

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

None of these files has a test module, so impl equals total lines.

| PixelFlow file | Lines | xLights source followed | Share following it | What matches | Clean-room |
|---|---|---|---|---|---|
| `vumeter.rs` | 1,550 (impl 1,511) | `effects/VUMeterEffect.cpp` (the `Render*Frame` functions, `Draw*` shapes, `GetTimingEvent`, `ApplyGain`); `render/Effect.cpp` `FilteredIn`; `render/RenderBuffer.cpp` `GetMultiColorBlend` | about 90% | About 45 meter types and 20 shapes, step for step: the 0.05 fall per frame, peak-hold counter, `255 / keep` alpha steps. Shape outlines and star constants identical. | **L** |
| `shape.rs` | 496 | `effects/ShapeEffect.cpp` (`Render`, `Draw*` for 12 shapes, `AddShape`, `DecodeShape`) | about 70% direct, 17% constants | Each drawer mirrors its C++ counterpart (step constants 0.75/0.6/0.05, star offsets, `2.618034`, heart curve, candy-cane fractions, per-shape rotations). The shape lifecycle is reorganised. | **L** |
| `tendril.rs` | 489 | `effects/TendrilEffect.cpp` (`ATendril`, `Tendril`, `Render`); `RenderBuffer.cpp` `DrawAALine`, `DrawAACircle` | about 90% | Spring update order and clamps, the ten movement cases with xLights' `_mv1.._mv4` state, slider mappings (`/20*0.2+0.4`, `/39*0.039+0.96`), anti-aliased line and circle with parallel local names. | **L** |
| `snowflakes.rs` | 448 | `effects/SnowflakesEffect.cpp` (`MoveFlakes`, `possible_downward_moves`, `DrawSnowflakes`, `AdvanceState`) | about 90% | The falling and piling rule, the `1/2/4` move bits, the 20-try refill, speed gate `(f*(speed+1))/30`. | **M** |
| `morph.rs` | 245 | `effects/MorphEffect.cpp` (`Render`, `StoreLine`, `calcPosition`); `RenderBuffer.cpp` `calcAccel`, `DrawThickLine` | 85 to 90% | `StoreLine` statement for statement; repeat, stagger, head and tail maths in the same order with the same sentinels. | **M** to **L** |
| `pinwheel.rs` | 269 | `effects/PinwheelEffect.cpp` (`RenderNewMethod`, `RenderOldMethod`, `adjustColor`, `Draw_arm`); `ispc/PinwheelFunctions.ispc` | about 80% | Arm angle maths including `3.14159`, `+ 540.0`, and the truncating mod; both render styles. Trig cache and clipping are PixelFlow's. | **M** |
| `butterfly.rs` | 211 | `effects/ButterflyEffect.cpp` `Render`; `ispc/ButterflyFunctions.ispc` | about 95% | All ten per-pixel formulae, `h2rgb`, the special case for two pixels, the truncated pi kept on purpose. | **M** |
| `fan.rs` | 149 | `effects/FanEffect.cpp` `Render`; `ispc/FanFunctions.ispc` | about 95% | Setup in the same order, the grow and shrink branches, the per-pixel angle test and edge blend. | **M** |
| `circles.rs` | 215 | `effects/CirclesEffect.cpp` (`RenderRadial`, `RenderMetaBalls`, `UpdateCacheState`), `CirclesEffect.h` | about 45% direct, 35% constants (likely) | Radial and plasma looks line by line (thresholds 0.30, 0.90). Ball motion is PixelFlow's closed form with xLights' constants. | **M** |
| `lines.rs` | 204 | `effects/LinesEffect.cpp`; `RenderBuffer.cpp` `DrawThickLine`, `DrawLine` | about 90% | Bounce tests in the same order, trail trimming, alpha `255*i/size`. | **S** |
| `life.rs` | 132 | `effects/LifeEffect.cpp`; `ispc/LifeFunctions.ispc` | about 50% (likely) | Seeding count `W*H*density/200 + 1`, pacing, the five rule sets in order. The generation step is ordinary Game of Life. | **S** |
| `plasma.rs` | 123 | `effects/PlasmaEffect.cpp` `Render`; `ispc/PlasmaFunctions.ispc` | about 85% | The seven-term sine sum term for term, with xLights' field names (`sin_time_5`, `cos_time_3`); the five colour schemes. xLights' own comment credits a public plasma tutorial. | **S** |
| `garlands.rs` | 89 | `effects/GarlandsEffect.cpp` `Render`; `ispc/GarlandsFunctions.ispc` | about 90% | Spacing, direction folding, ring position step for step; four swag patterns. | **S** |

### Shared render code with a ported function group (`crates/pf-render/src/`)

| PixelFlow file | Lines (impl) | xLights source followed | Lines following it | What matches | Clean-room |
|---|---|---|---|---|---|
| `styles.rs` | 759 (481) | `models/ModelGroup.cpp` (`GetBufferSize`, `InitRenderBufferNodes`), `models/Model.cpp` (Per Preview) | about 215 | `grid_cells` and `preview_cells` step for step (2048 cap, `count*5 > area`, 400 cap). Group stacking arms are likely ports, re-expressed per pixel. | **M** |
| `text.rs` | 419 (419) | `effects/TextEffect.cpp` (`RenderTextLine`, the `zigzag` macro, `FormatCountdown`) | about 190 | Movement offsets case by case (`xlimit = totwidth*8 + 1`), countdown formatting including "Invalid Format". Layout, colouring, and the font are not from xLights. | **M** |
| `geometry.rs` | 1,353 (786) | `models/SubModel.cpp` (`initDefaultBuffer`, `initSubbufferRange`), `models/Model.cpp` | about 190 | `grid_buffer` step for step; the sub-buffer edge rounding (start edge rounded, end truncated). | **M** |
| `color.rs` | 617 (377) | `effects/ispc/LayerBlendingFunctions.ispc` | about 165 | `to_hsv` and `from_hsv` (both `1e-20` epsilons), and 16 blend arms branch for branch (`/ 5.0` shadow, `/ 2.0` shadow below). Normal, Add, Max, Multiply, and coverage are PixelFlow's. | **S** |
| `raster.rs` | 340 (269) | `render/RenderBuffer.cpp` (`SetPixel`, `DrawLine`, `DrawThickLine`, `DrawCircle`, `DrawFadingCircle`) | about 150 | The gap-filling thick line case for case, the eight-point circle, and the `> width` wrap quirk (asserted by a test). The underlying algorithms are textbook. | **S** |
| `blur.rs` | 577 (420) | `render/PixelBuffer.cpp` (`boxesForGauss`, `gaussBlur_4`, `Blur`) | about 130 | The box-size table and the thresholds that pick a blur. The box passes are organised differently. | **S** |
| `effects.rs` | 1,169 (1,169) | `RenderBuffer.cpp` `calcAccel`, `GetEffectTimeIntervalPosition`; `ShapeEffect.cpp` music trigger | about 63 | Three small helpers. The effects themselves are closed-form and PixelFlow's (see the note on what was compared, below). | **S** |
| `sparkles.rs` | 119 (60) | `LayerBlendingFunctions.ispc` `ApplySparkles`; `PixelBuffer.cpp` | about 25 | `% (208 - count)`, the brightness steps 0.53 / 0.75 / 1.0, phase `% 10000`. | **S** |

Not every xLights counterpart of the `effects.rs` effects was opened. On, Color Wash, Shimmer,
and Fire were compared and share no structure or constants with xLights. Bars, Wave, Twinkle,
Strobe, Spiral, and Meteors were judged from the form of PixelFlow's code alone.

### Model geometry in the importer (`crates/pf-xlights/src/`)

These place every pixel of an imported xLights prop where xLights places it, which is the
importer's stated purpose.

| PixelFlow file | Lines | xLights source followed | Share | Tables carried over | Clean-room |
|---|---|---|---|---|---|
| `geometry/poly.rs` | 703 | `models/PolyLineModel.cpp`, `MultiPointModel.cpp`, `Shapes.cpp` (`BezierCurveCubic3D`), `PolyPointScreenLocation.cpp` | 85 to 90% | 25-step curves, corner offsets | **M** |
| `geometry/grid.rs` | 655 | `MatrixModel.cpp`, `TreeModel.cpp`, `SphereModel.cpp`, `CubeModel.cpp` | about 95% | Cube orientation table (48 rows of 4), identical row for row; tree and sphere constants | **M** |
| `geometry/radial.rs` | 589 | `CircleModel.cpp`, `WreathModel.cpp`, `StarModel.cpp`, `SpinnerModel.cpp`, `WindowFrameModel.cpp` | about 95% | Window-frame start and index tables; star start rules | **M** |
| `geometry/lines.rs` | 486 | `SingleLineModel.cpp`, `ChannelBlockModel.cpp`, `ArchesModel.cpp`, `CandyCaneModel.cpp`, `IciclesModel.cpp` | about 95% | Cane proportions, drop pattern default | **M** |
| `geometry/xform.rs` | 273 | `BoxedScreenLocation.cpp`, `TwoPointScreenLocation.cpp`, `ThreePointScreenLocation.cpp`, `PolyPointScreenLocation.cpp` | about 170 lines | Matrix order, the `3.14159` literal, the 0.001 nudge | **S** |
| `geometry/custom.rs` | 181 | `CustomModel.cpp` (`InitCustomMatrix`, `SetStringStartChannels`) | about 95 lines (likely); the rest is format reading | none | **S** |
| `geometry.rs` | 685 | `Model.cpp` (`GetNodeChannelCount`, `CalcChannelsPerString`, `SetStringStartChannels`, `ComputeStringStartNode`), `Model.h` | about 190 lines; the rest is PixelFlow's harness | String-type to channel-count rules | **S** |

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

Everything not listed above. In particular: `pf-engine`, `pf-output` (sACN and DDP from their
public specifications), `pf-fidelity` (which compares rendered output and cites no source),
`pf-audio`, `pf-analysis` apart from the note spectrogram, `pf-align`,
`pf-camera-map`, `pf-video`, `pf-patterns`, `pf-frame`, `pf-cli`, `pf-ai` apart from
`shape_marks`, `pf-lexicon` apart from the mouth table (its letter-to-sound rules and
syllabifier have no xLights counterpart), `pf-xlights`'s XML reader, vendor package reader, and
auto-mapping, `pf-sequence`'s documents, timing, and validation, `pf-model` apart from the files
named, the desktop shell (`app/src-tauri`), and the UI apart from the files named.

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
| Device fixtures | `crates/pf-devices/fixtures/` | Per its README: `fpp/` and `falcon/` recorded from the owner's own devices with addresses, MAC, and serial removed; `fpp-hat/` and `wled/` synthetic; `falcon/synthetic/` built from the sample replies quoted in comments in `Falcon.cpp` (about 1.2 KB); `fpp/github_releases.json` a trimmed public release list | Tests only. They hold no credentials, but they still carry details of the owner's own show: prop names, a sequence name ("Christmas Medley 2017"), and a controller name whose suffix may derive from a MAC address. Scrubbing those is optional and is the owner's call; the fixtures were not changed. |
| FPP oracle | `scripts/fseq-fpp-oracle.sh` | Fetches FPP's `src/fseq` when run, into a scratch directory | Nothing from FPP is copied into the repository. The script holds a 45-line harness written against FPP's `FSEQFile` API. |

## What a clean-room rewrite would involve

| Piece | Effort | What it takes |
|---|---|---|
| Model geometry: `pf-xlights/src/geometry*`, the ten `pf-geometry` generators, `geometry.ts`, then `shapes.rs` on top | **L** | Export node positions from xLights for every model type, start corner, and option, and write generators to match. The cube and window-frame tables fall out of that. |
| VU Meter | **L** | About 45 meter types and 20 shapes to specify from behaviour. |
| Shape, Tendril | **L** each | Twelve shape drawers (tree, cross, and present outlines redrawn, so no longer pixel-identical); ten tendril movements. |
| Morph | **M** to **L** | Head, tail, stagger, and palette rules are not documented publicly. |
| Snowflakes, Pinwheel, Butterfly, Fan, Circles | **M** each | |
| Plasma, Garlands, Life, Lines | **S** each | Public techniques or small effects. |
| Value curves (`curves.rs`, and 130 lines of `curve.rs`) | **M** | Each curve type's maths as a written rule. The file keys are format and stay. |
| Effect settings: `sequence/effects.rs` and the 14 mirrored tables in `pf-sequence` | **M** | Re-collect about 240 defaults and ranges from the xLights effect panels or saved sequences rather than the source; saved PixelFlow sequences may need migrating. |
| Group and submodel buffers: `styles.rs`, `pf-render/src/geometry.rs`; Text movement | **M** each | Rounding and scaling rules are asserted by tests and need characterising. |
| Drawing, blur, blend, HSV, sparkles | **S** each | Textbook algorithms plus a few xLights-specific rules. |
| Mouth-shape table, `shape_marks`, Faces, Picture placement, Falcon tables, the rest | **S** each | |

Attribute names, setting keys, choice strings, value-curve keys, phoneme names, and the values
xLights assumes when a key is missing from a file are needed to read its files at all, however
the code is written.
