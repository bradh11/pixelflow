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

Topic `range` is the whole palette at a glance, so the Assistant knows every effect well enough
to reach past the few it uses by habit: for every effect, the moments and moods it suits, the
kinds of prop its looks were written for, and one of its looks to start from. It assigns nothing
and requires nothing; the method points to it as a place to choose from to express the show's
idea, a familiar formula included. The only "never" in it is the honest one: the effects whose
guides have no look for a single run of pixels are listed (generated, `{{grid-only}}`), because
they show only a slice of themselves on an outline or an arch and the review's fit criterion says
so. A test (`the_range_says_what_every_effect_can_do_from_its_guide` in
`crates/pf-ai/tests/knowledge.rs`) holds it to the guides: a line for every effect, each prop it
names one its looks are written for, and each look it names one of its guide's.

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

## What is sent

A guide the Assistant fetches becomes part of that conversation, like anything else a tool
answers. The guides are PixelFlow's own text and say nothing about the user's show: see
[privacy and data flow](../privacy-and-data-flow.md#the-assistant-ai).

## Does it work?

The tests say the guides are complete and true to the catalog. Whether a model that reads them
makes good shows is measured separately, against a real model with your own key: see
[assistant-eval.md](assistant-eval.md).
