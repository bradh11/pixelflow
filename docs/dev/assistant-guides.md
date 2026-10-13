# The Assistant's guides

What the Assistant knows about making light shows, where it lives, and how to add to it.

## How its knowledge is organised

The Assistant's instructions and tool definitions ride along with every request, so they are
kept small (`INSTRUCTIONS_BUDGET_BYTES` in `crates/pf-ai/src/agent.rs`, the tool budgets in
`crates/pf-ai/src/tools.rs`; tests hold both). Only the core tools ride along (`CORE` in
`tools.rs`: reading the show and sequence, the guides, the song, placing and changing many
effects at once, cues, and the draft's tools); the rest come in groups (`GROUPS`: layout,
show, rows, effects one at a time, timing) that a chat adds with `use_tools` when a job needs
them, and keeps. Both Anthropic's and OpenAI's prompt caches cover the tool list as part of the
prefix, so changing it means the cache is rebuilt once: the list only ever grows, a group at a
time, and the sequencing jobs need none of the groups. `FIXED_REQUEST_BUDGET_BYTES` holds the
instructions and the core together. What every job looks up first (the props, groups, rows,
timing tracks, and the song's analysis) comes in the first message's context block instead
(`crates/pf-ai/src/context.rs`). Everything else it knows it **fetches when it needs it**, with
the `get_guide` tool:

| | What | Where it comes from |
|---|---|---|
| **Reference** | Every effect kind, setting, choice, blend, curve shape, render style, cue, and timing track, with ranges and defaults | Generated from the tables the app itself runs on: `pf_sequence::effect_catalog()`, the edit types' JSON Schemas, the cue and track lists (`crates/pf-ai/src/knowledge/reference.rs`). Never typed out. |
| **Craft** | For each effect kind: what PixelFlow draws with it on a string, an arch, a matrix, a tree, and the whole house; which settings matter; what it pairs with; what to avoid; named ready-made looks | Written, one Markdown file per kind: `crates/pf-ai/guides/effects/<kind>.md`. |
| **Show design** | Reading a song, roles for props, layering, palettes, motion, vocals, beginnings and endings, mistakes, safety; and what every effect shares (blends, curves, layouts, timing tracks, cues) | Written, one Markdown file per topic: `crates/pf-ai/guides/topics/<topic>.md`. |

The guides are compiled into the app (`crates/pf-ai/src/knowledge/library.rs`). `get_guide` with
no arguments answers the index; with `effects` and `topics` it answers those guides, an effect's
written guide followed by its generated reference, up to 12 in one answer of at most 48 KB (the
reading a design job does before it starts, in one round trip). With `parts` it answers only
those parts of each effect's guide (`draws`, `settings`, `use`, `looks`, `reference`: its
sections), after its title and summary; topics always come whole. A guide, or a part of one, is
sent once in a chat: asked for again, the answer points back at it. Topic `choosing` sets every
effect side by side in a line each (what it is for, and the kinds of prop its looks were written
for), generated from the effects' own guides, so the Assistant can choose before reading the
guides of only the effects it will place.

## The rules a guide keeps

Tests (`crates/pf-ai/tests/knowledge.rs`, through `crates/pf-ai/src/knowledge/check.rs`) fail,
saying what to write, when a guide breaks one:

- **Names in backticks are PixelFlow's own names**: a setting's key, a choice's value, an effect
  kind, a blend, a tool, a tool's input field, exactly as spelled. Anything else is plain words.
  A timing track's name goes in quotes ("Beats"), a color as "#rrggbb". Another effect's setting
  is written `kind.setting`.
- **An effect's guide names every setting and every choice its kind has**, and none it doesn't.
  The reference adds each one's range and default by itself, so the guide says what it is *for*.
- **An effect's guide has these sections, in this order**: `What it draws`, `Settings`,
  `When to use it`, `Looks`. "What it draws" says it for a string, an arch, a matrix, a tree, and
  the (whole) house.
- **Looks are real**: each is an effect exactly as `place_effects` takes one, of the guide's own
  kind, with settings in range; and it lights something on every kind of target it says it suits.
- **A guide fits in one answer** with room for others: 12,000 bytes as the Assistant reads it.

## Writing an effect's guide

Write it from what PixelFlow draws, not from what an effect of that name does elsewhere.

A new effect kind stops the build at `effect_source` in `crates/pf-ai/src/knowledge/library.rs`
until it has a guide. Create `crates/pf-ai/guides/effects/<kind>.md` with a first line of
`# <the effect's label>` and add its line to that `match`: the crate builds again, and
`cargo test -p pf-ai --test knowledge` fails with everything the guide still has to say, by name.

1. Draw it. This renders the effect through the engine on a made-up display (`pf_ai::stage`: a
   roofline, an arch, a 40×20 matrix, a 16-string mega tree, and the whole house as a group), at
   its defaults and with each setting pushed low and high and each choice tried:

   ```
   cargo run -p pf-ai --example guide_sheets -- /tmp/sheets --kinds bars --auto
   ```

   It writes `bars-1.png`, `bars-2.png`, … (eight rows a sheet) and `bars.txt` saying what each
   row is. Each row shows three moments (0.4 s, 0.55 s, and 2.6 s into a four-second effect) on
   each kind of target. Look at them.
2. Write `crates/pf-ai/guides/effects/<kind>.md`:

   ````markdown
   # Bars

   One sentence: what it is for. (It becomes the effect's line in the index.)

   ## What it draws

   What you saw on a string, an arch, a matrix, a tree, and the whole house; fast and slow.

   ## Settings

   - `count`: what it is for, and a good starting value.
   - `axis`: `horizontal` does this; `vertical` does that.

   ## When to use it

   What it is good for, what it pairs with, what to avoid.

   ## Looks

   ```look
   {"name": "Candy stripe along an outline", "suits": ["string", "arch"],
    "note": "Red and white stripes sliding along the prop.",
    "effect": {"kind": "bars", "settings": {"axis": "horizontal", "count": 6, "speed": 0.4},
               "colors": ["#ff0000", "#ffffff"]}}
   ```
   ````

   A look's `suits` are the kinds of target it was written for: `string`, `arch`, `matrix`,
   `tree`, `house`. Its `effect` takes `kind`, `settings`, `colors`, and anything else an effect
   has (`blend`, `fadeInMs`, `sparkles`, `curves`, …); a timing track may be given by name. The
   stage's sequence has "Beats", "Bars", "Lyrics", "Lyrics (words)", "Lyrics (syllables)", and
   "Lyrics (phonemes)", and one picture, `images/snowman.png`.
3. Draw the looks and check the guide, without rebuilding:

   ```
   cargo run -p pf-ai --example guide_sheets -- /tmp/sheets --kinds bars --guides crates/pf-ai/guides
   cargo run -p pf-ai --example guide_sheets -- --check --guides crates/pf-ai/guides
   ```

   The first draws each look as a row after the defaults. The second says everything the tests
   would. For one-off experiments (two layers and a blend, a render style on a group), give
   `--spec rows.json`: see the top of `crates/pf-ai/examples/guide_sheets.rs`.

## Writing a topic

A topic is the same Markdown: a title, a sentence, sections of your choosing. A list that should
never go stale is asked for by name, `{{blends}}` for instance, and filled in when the guide is
read: `GENERATED` in `crates/pf-ai/src/knowledge/mod.rs` has the names. List a new topic in
`TOPICS` in `library.rs`.

## Range and showpieces

Left to themselves, models build sound shows from the same dozen effects every time. Topic
`showpieces` (read with the method, which points to it at the planning step, and named in the
instructions' list of topics) teaches the rest:

- **A concept from the song**: its title, words, and mood choose the showpieces and the colors,
  and the first palette that comes to mind is one choice of several, each section still held to
  two or three colors.
- **Signature moments**: at least one showpiece on each canvas (a matrix, a mega tree): clouds
  in a breakdown, fireworks or a shockwave on the biggest hit, a dancer or a picture in a chorus,
  snow or meteors in a quiet verse. Each suggestion names a ready-made look from that effect's
  guide, as `` `kind` "Look name" ``, with the props it goes on.
- **Range across a show**: different parts of the song get different effects, not one kit in
  new colors; repeats of the same music keep their look.
- **What suits what**, plainly: motion that travels along the line for outlines and arches
  (each again a named look written for a string and an arch), and the effects never to put
  there.

Two of its lists are generated, so they never go stale: `{{showpieces}}`, every showpiece with
its looks for a matrix or a tree, and `{{grid-only}}`, the effects whose guides have no look for
a single run of pixels (the ones the review's fit criterion flags on an outline or an arch). The
showpieces themselves are `SHOWPIECES` in `crates/pf-ai/src/knowledge/mod.rs`. A test
(`the_showpieces_and_motion_the_guides_suggest_are_real_looks_on_the_props_they_say` in
`crates/pf-ai/tests/knowledge.rs`) holds every look the topic names to its effect's guide, and
every prop a table puts it on to the props the look is written for; the effect guides' own test
draws each look on each of those props. Rename or retarget a look and the topic has to follow.

The review scores the same thing (`range`, weighed lightly beside the rest): kinds of effect for
the song's length and the display, a showpiece on every canvas, and no section playing exactly
the effects of a different part just before it. Its fixes name the canvas and give the
`place_effects` call for a look from the guides (clouds through the quietest section on a
matrix, ending before a stop; a firework finale over a tree's look through the loudest, in that
section's colors), and they take only the room the other fixes leave. An effect on a prop it
doesn't suit adds nothing to the range: the fit criterion still names it. It needs the song's
analysis, and leaves a sequence under 30 seconds (a jingle) alone.

## What is sent

A guide the Assistant fetches becomes part of that conversation, like anything else a tool
answers. The guides are PixelFlow's own text and say nothing about the user's show: see
[privacy and data flow](../privacy-and-data-flow.md#the-assistant-ai).

## Does it work?

The tests say the guides are complete and true to the catalog. Whether a model that reads them
makes good shows is measured separately, against a real model with your own key: see
[assistant-eval.md](assistant-eval.md).
