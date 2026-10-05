// Editing several selected effects at once: what they have in common, and the edits (sent as one
// batch, so one undo step) that change all of them. Pure: no React, no engine calls.

import type { Effect, EffectKind, Sequence, SequenceEdit } from "../api/sequence";
import { effectBounds } from "./timelineMath";

/** The effects with ids `ids`, in the document's order (ids no longer there are left out). */
export function effectsById(doc: Sequence, ids: readonly string[]): Effect[] {
  const wanted = new Set(ids);
  const out: Effect[] = [];
  for (const row of doc.rows) for (const layer of row.layers) for (const e of layer.effects) if (wanted.has(e.id)) out.push(e);
  return out;
}

/** A value across several effects: the same on all of them, or `mixed` (with the first one's). */
export interface Shared<T> {
  mixed: boolean;
  value: T;
}

/** Whether `values` are all the same (compared as JSON, so lists of colors compare by content). */
export function shared<T>(values: readonly T[]): Shared<T> {
  const first = JSON.stringify(values[0]);
  return { mixed: values.some((v) => JSON.stringify(v) !== first), value: values[0] };
}

/** The kind every effect is, or null when they're of different kinds. */
export function sharedKind(effects: readonly Effect[]): EffectKind | null {
  const kind = effects[0]?.params.kind;
  return kind !== undefined && effects.every((e) => e.params.kind === kind) ? kind : null;
}

/** An update for each effect `change` changes, built from the effect as it is in `doc`. */
export function updateEach(doc: Sequence, ids: readonly string[], change: (effect: Effect, doc: Sequence) => Effect | null): SequenceEdit[] {
  const edits: SequenceEdit[] = [];
  for (const effect of effectsById(doc, ids)) {
    const changed = change(effect, doc);
    if (changed && JSON.stringify(changed) !== JSON.stringify(effect)) edits.push({ type: "updateEffect", effect: changed });
  }
  return edits;
}

/**
 * Makes every effect `ids` `lengthMs` long from where it starts: at least a frame (room allowing),
 * and no further than the next effect on its layer or the song's end. Fades longer than the new
 * length are cut to it.
 */
export function lengthEdits(doc: Sequence, ids: readonly string[], lengthMs: number): SequenceEdit[] {
  return updateEach(doc, ids, (e, latest) => {
    const hi = Math.min(latest.durationMs, effectBounds(latest, e.id)?.hi ?? latest.durationMs);
    // At least a frame, but never into the next effect or past the end (both win over the frame).
    const endMs = Math.min(hi, Math.max(e.startMs + latest.frameMs, e.startMs + Math.round(lengthMs)));
    const length = endMs - e.startMs;
    return { ...e, endMs, fadeInMs: Math.min(e.fadeInMs, length), fadeOutMs: Math.min(e.fadeOutMs, length) };
  });
}
