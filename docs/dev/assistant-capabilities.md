# What the Assistant can do in a sequence

An audit of everything the sequence document and the Sequence editor can express, against the
Assistant's tools (`crates/pf-ai`). The rule it is held to: **anything a person can do to a
sequence by hand, the Assistant can draft**, as a proposal the person applies in one undoable step.
What it may never do is listed at the end; those are decisions, not gaps.

The first table is the audit as it stood before this work. [Gaps closed](#gaps-closed) says what
changed, and [Still out of reach](#still-out-of-reach) what did not and why.

## The audit

"Tool" names are the Assistant's tools. The `sequence_` and `show_` tools are generated one per
engine edit, so a new edit becomes a tool by itself.

| What a person can do | In the document | The Assistant, before |
|---|---|---|
| **Effects** | | |
| Add an effect of any kind, with any of its settings | `Effect.params` (40 kinds, 303 settings, 287 choice values) | Yes: `sequence_add_effect` (one), `place_effects` (many). But it could not *see* half of them: `list_effect_kinds` answered all 40 kinds as 70 KB, and answers are cut at 40 KB, so the last kinds' settings never arrived unless asked for one kind at a time. |
| Know what a setting does, or what an effect looks like on a given prop | (the editor shows it) | Only each setting's one-line description. Nothing on what an effect draws on a string, an arch, a matrix, a tree, or a group; nothing on which settings matter or what to pair. |
| Colors (up to 32) | `palette` | Yes, both tools. |
| Blend with the layers below (21 blends) | `blend` | Yes, both tools. The 21 one-line descriptions rode along in every request (2.3 KB of the 8 KB `sequence_add_effect` definition). |
| Fades | `fadeInMs`, `fadeOutMs` | Yes, both tools. |
| Sparkles, sparkle color, sparkles that follow the music | `sparkles`, `sparkleColor`, `musicSparkles` | `sequence_add_effect` only. **Not** `place_effects`, the tool the instructions tell it to build looks with. |
| Blur | `blur` | `sequence_add_effect` only. Not `place_effects`. |
| Color adjustments (hue, saturation, lightness, brightness, contrast) | `hueAdjust` … `contrast` | `sequence_add_effect` only. Not `place_effects`. |
| Render style (17) and turn or flip (8) | `renderStyle`, `bufferTransform` | `sequence_add_effect` only, as bare names with no word on what any of them does. Not `place_effects`. |
| Value curves (11 shapes) on any number setting, sparkles, blur, and four adjustments | `curves` | `sequence_add_effect` only. Not `place_effects`. A curve on a timing track needed the track's id. |
| Settings that follow a timing track | a `timingTrack` setting (an id) | Yes, by id only: a second lookup for every use. |
| Audio-reactive options (VU Meter, Pulse, fire/shape/fireworks on the music, music curves, music sparkles, Dancer's bass bounce) | settings, curves | The settings: yes. Music curves and music sparkles: `sequence_add_effect` only. |
| Faces and mouth shapes | Faces effect; a prop's face | The effect: yes. Which props *have* a face: only by reading each prop in full (`get_prop`); `list_props` listed submodels without saying which were faces. |
| Pictures and animated GIFs | Picture effect, `file` | Yes, for pictures already in the show (`get_open_sequence` lists them). |
| Dancer characters | Dancer effect | Yes (`stage_cue` dance, or the effect). |
| Change one effect's settings, colors, blend, timing | `updateEffect`, `setEffectParams`, `setEffectTiming` | Yes, one effect per call, the whole effect sent back each time. |
| Change many effects at once (the editor's multi-edit: settings, colors, mixing, fades, move all by, length of each) | several edits | **No.** One call per effect. Recoloring a chorus of 60 effects was 60 calls. |
| Move an effect to another row or layer | `moveEffect` | Yes, one per call. |
| Copy and paste, duplicate | new effects | In time only (`repeat_effects`, same rows). **Not** to other rows. |
| Delete effects | `removeEffect` | Yes, one per call. **No** way to clear a stretch of time or a set of rows. |
| Nudge effects in time | `setEffectTiming` | One per call. |
| **Rows and layers** | | |
| Add, remove, reorder rows; target a prop, a group, or a submodel | `rows`, `Target` | Yes (`sequence_add_row` and friends). |
| Add and remove layers | `layers` | Yes. |
| How rows meet (cover, blend, replace) | `rowBlending` | Yes (`sequence_set_row_blending`). |
| **Timing tracks** | | |
| Create, rename, reorder, delete tracks of any kind | `timingTracks` | Yes. |
| Add, move, resize, relabel, split, merge, delete marks; generate marks; copy every nth mark | marks | Yes. |
| Beats, bars, sections, accents, moments, drums from the song | detect beats | Yes (`add_song_timing`). |
| Lyrics: paste lines, spread them, break into words | lyric edits | Yes (`sequence_spread_lyrics`, `sequence_break_into_words`). |
| Syllables and mouth shapes from the words | derived tracks | Yes (`add_song_timing`). |
| Nudge lyrics (shift a lyrics track with its words, syllables, and mouth shapes) | mark edits | Only by sending each whole track back (`sequence_update_timing_track`). |
| Find lyrics, re-time to vocals, tap timing, import or export a timing file | (editor commands) | No: see [Still out of reach](#still-out-of-reach). |
| **The sequence** | | |
| Name, length, frame time, music | `updateInfo` | Yes. |
| **The layout it draws on** | | |
| A group's layout, a prop's own layout, submodels, faces | the show file | Yes (`show_update_group`, `show_update_prop`), whole item at a time. |
| Know each prop's size and what it is | (the layout shows it) | Name, shape type, pixel count, and position only: not a matrix's columns and rows, nor the role `stage_cue` gives each prop. |
| **Design** | | |
| Plan a show against a song | (the person's craft) | A seven-step outline in its instructions. No guidance on palettes, roles, layering, motion, vocals, openings, or endings. |
| Check a draft as a designer would | (the person's eye) | `review_draft`: moments, contrast, energy, consistency, variety, flash safety, hook words, dead air, restraint. It did not look at which props were used, how many kinds of effect were piled on, whether the colors held, whether anything changed at each section, whether effects suited their props, or whether text could be read. |
| Read a long sequence back | (the timeline) | `list_sequence_effects` answers every effect in full, cut at 40 KB: a full sequence could not be read back at all. |

## Gaps closed

Filled in as each lands; the commit that closes a gap adds its line here.

- **Seeing every effect.** `list_effect_kinds` without a kind now answers every kind with its
  settings' keys (it fits); with a kind, that kind in full, as before.
- **Knowing what things do.** `get_guide` answers a guide for any effect kind or topic: see
  [assistant-guides.md](assistant-guides.md). Every effect's reference (each setting's range,
  default, choices, and what the code's own comments say of it) is generated from the catalog.
- **Everything an effect has, in `place_effects`.** Its `effect` now takes sparkles, sparkle
  color, music sparkles, blur, the five color adjustments, render style, turn or flip, and curves,
  by the same names the document uses (`crates/pf-ai/src/effect_spec.rs`).
- **Timing tracks by name.** In `place_effects`, a `timingTrack` setting or a curve's
  `timingTrack` may be a track's name ("Beats"); it is drafted as the id.
- **Refusals that teach.** A setting the kind doesn't have is answered with the settings it does
  have; a number out of range with its range and default; a wrong choice with the choices; a curve
  on something curves can't drive with what they can; a track that isn't there with the tracks
  that are.
- **A false refusal fixed.** `sequence_add_effect` (and every other generated tool) refused an
  input that spelled out a setting at its neutral value (`"brightness": 1`, `"renderStyle":
  "default"`, `"curves": {}`), calling it a setting PixelFlow doesn't know. Those names are now
  checked against the edit's schema.
- **Budgets.** `sequence_add_effect` went from 7,960 to about 4,950 of its 8,000 bytes: what each
  blend and curve shape does moved to the guides, generated from the same comments.
- **What each effect draws, and how to use it.** A written guide for each of the 40 effect
  kinds, from frames drawn by the engine on a string, an arch, a matrix, a tree, and the whole
  house as one group: what it draws on each, what every setting and choice is for, when to use
  it, and 186 named looks ready to place. The build stops (the `match` in
  `crates/pf-ai/src/knowledge/library.rs`) when an effect kind has no guide, and the tests fail,
  saying what to write, when a guide misses a setting or a choice, names one that isn't there,
  or gives a look that isn't a real effect or lights nothing where it says it suits.
- **Many effects at once.** `change_effects` chooses effects (by id, row, the time they start in,
  layer, and kind) and does what the editor's multi-edit and clipboard do: changes their
  settings, colors, mixing, fades, sparkles, and curves, keeping what isn't named; sets each
  one's length; moves them all earlier or later; copies them to other rows; moves them to
  another row or layer; removes them. One call, one batch, all or nothing; a result that would
  overlap another effect is refused unless told to replace it.
- **Knowing the display.** `list_props` gives each prop's role (the one `stage_cue` casts it in),
  its grid (a matrix's columns and rows, a tree's strings and pixels up each), and its faces.
- **Reading a long sequence back.** `list_sequence_effects` gives every effect in full when that
  fits; otherwise (or when asked to be brief) each effect as a line, row by row; and when even
  that is too long, each row's layers with how many effects of which kinds, when. It is never
  cut off in the middle.
- **Planning a show.** Thirteen show-design topics (the method, reading a song, roles for props,
  layering and restraint, palettes with Christmas and Halloween sets, motion, intensity and
  change, vocals, beginnings and endings, common mistakes, flash safety, following the music,
  changing what is there), fetched like any guide. The standing instructions carry the method in
  brief and say which topic to read before each decision; they stay inside their 5,300 bytes.

## Still out of reach

By design (these stay behind the person's own clicks, and no test should ever let them in):

- Saving, exporting, sending to controllers, starting output or playback.
- Reading the disk: finding or relinking the show's files, importing or exporting timing files,
  adding pictures to the show.
- **Find lyrics** (it contacts a lyrics service and, with consent, a speech service) and
  **Re-time to vocals** (it listens to the song). The Assistant offers them; the person starts them.
- **Tap timing**: it is a person tapping along.
- Undo and redo of the person's own history. The Assistant's undo is its draft: `reset_draft`, or
  Discard.
