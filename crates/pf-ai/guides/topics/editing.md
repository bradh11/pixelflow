# Changing what is there

Working on a sequence that already has effects: finding them, changing many at once, moving, copying, retiming, and deleting; rows and layers; and the sequence's own settings.

## Look before you change

The user's sequence is theirs. Read what is there before touching it.

- `get_open_sequence`: the rows (what each lights, how many effects on each layer), the timing tracks, the pictures.
- `list_sequence_effects`: the effects themselves, for one row or a stretch of time. Ask with `brief` for a long stretch: each effect as its id, layer, kind, and times; then ask in full for the row and time you mean to change.
- `get_selection`: what the user has selected, when they say "these" or "this one".
- Change only what was asked for. "Make the chorus brighter" is not a licence to redo the verses.

## Many effects at once

`change_effects` chooses effects and does one thing to them.

Choosing (everything you give must hold): `ids`, `rowIds`, `fromMs` and `toMs` (effects that start in that stretch), `layer`, `kinds`.

Then one of:

- `change`: new settings, colors, blend, fades, sparkles, curves: anything an effect has, by name, exactly as in `place_effects`. What you don't name stays as it was. Settings and curves are changed key by key; a curve given as null is taken off. This is how to recolor a section, slow every chase in the verses, or add sparkles to the last chorus.
- `lengthMs` and `shiftMs`: make each effect that long, and move them all later or (with a negative number) earlier. These two and `change` can go together in one call.
- `copyToRowIds`: the same effects, at the same times and on the same layers, on other rows. What works on one arch, on all four.
- `moveTo`: to another row, another layer, or both.
- `remove`: gone. To clear a stretch before laying something new: `fromMs`, `toMs`, and `remove`.

A result that would overlap another effect on its layer is refused unless you add `replace`, which removes what is in the way.

## One effect

- `sequence_update_effect` replaces a whole effect: get it first with `list_sequence_effects`, send it back changed.
- `sequence_set_effect_timing` moves or resizes one; `sequence_move_effect` takes it to another row or layer; `sequence_remove_effect` deletes it.
- `sequence_add_effect` adds one, in the document's own form (its colors under `palette`, an id you make up, a timing track by its id). For anything more than a single effect, `place_effects` is easier: it takes a timing track by name and many rows at once.

## Copying in time

`repeat_effects` copies every effect that starts in a stretch to other start times, keeping rows, layers, and lengths: the first chorus's look onto the second and third. Then `change_effects` over the copy for the variation.

## Rows and layers

- A row lights one prop, group, or submodel. `sequence_add_row` adds one (topic `layouts` has the forms a target takes); `sequence_move_row` reorders; `sequence_remove_row` deletes it and everything on it.
- Order matters: rows are drawn top to bottom, and a later row covers an earlier one where both light the same pixels. Group rows first, prop rows after.
- Layers stack on a row, layer 0 at the bottom. Adding an effect on a layer one past the top makes that layer; `sequence_add_layer` and `sequence_remove_layer` do it directly. Effects on one layer can't overlap in time.

## The sequence's own settings

- `sequence_update_info`: its name, its length, the time between frames (25 ms is 40 frames a second and the usual choice), and its music. Send all four; get the current ones from `get_open_sequence`. Don't change the music or the length unless asked: effects past a shortened end are lost to view.
- `sequence_set_row_blending`: how rows meet (topic `blends`). Leave it alone unless the user asks or the sequence was imported and looks wrong.
- `sequence_set_music_level_source`: which channel of the song is how loud it is to everything that follows the level (topic `music-reactive`). Leave it alone unless the user asks.

## Timing tracks

Adding, renaming, and editing marks: topic `timing`.

## Starting over

`reset_draft` throws away everything you have drafted and returns to the user's sequence as it is. Use it when a draft has gone wrong enough that fixing it would take longer than redoing it; say that you did.

## Leave it tidy

- No effects shorter than a frame or two, no effects hanging past the end, no empty layers left on top of rows.
- The same look in the same place on similar props: if three arches chase and the fourth doesn't, it should be on purpose.
- `review_draft` before `propose_changes`, even for a small change: a recolor can break the palette of the section next door.
