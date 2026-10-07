// What the Sequence screen does to effects, shared by its keys (useSequenceKeys) and the
// timeline's right-click menu, so both make the same edits and undo the same way.

import type { Sequence, SequenceEdit } from "../api/sequence";
import { buildIndex, pasteEffects } from "../lib/timelineMath";
import type { MenuItem } from "./contextMenu";
import { type Copied, useSequencer } from "./sequencer";

/** Pastes copies built from the document as it is when the paste's turn comes, then selects them. */
function pasteAt(copies: (doc: Sequence) => { copies: Copied[]; atMs: number }) {
  const s = useSequencer.getState();
  let made: string[] = [];
  void s
    .edit((doc) => {
      const { copies: chosen, atMs } = copies(doc);
      const edits = pasteEffects(doc, buildIndex(doc), chosen, atMs);
      made = edits.flatMap((x) => (x.type === "addEffect" ? [x.effect.id] : []));
      return edits;
    })
    .then((ok) => {
      if (!ok || made.length === 0) return;
      useSequencer.getState().select(made);
      useSequencer.getState().reveal();
    });
}

/** Copies the selected effects. */
export function copyEffects(): void {
  const s = useSequencer.getState();
  if (s.selection.length > 0) s.copy();
}

/** Pastes what was copied at the playhead. */
export function pasteEffectsAtPlayhead(): void {
  const { clipboard, playheadMs } = useSequencer.getState();
  if (clipboard.length > 0) pasteAt(() => ({ copies: clipboard, atMs: playheadMs }));
}

/** Copies of the effects, straight after the last of them. */
export function duplicateEffects(ids: string[]): void {
  if (ids.length === 0) return;
  pasteAt((latest) => {
    const index = buildIndex(latest);
    const copies = ids.flatMap((id) => {
      const p = index.byId.get(id);
      return p ? [{ rowId: p.rowId, effect: p.effect }] : [];
    });
    return { copies, atMs: Math.max(0, ...copies.map((c) => c.effect.endMs)) };
  });
}

export function deleteEffects(ids: string[]): void {
  if (ids.length === 0) return;
  void useSequencer.getState().edit((latest) => {
    const index = buildIndex(latest);
    return ids.filter((id) => index.byId.has(id)).map((id): SequenceEdit => ({ type: "removeEffect", id }));
  });
}

/** Selects the effects and puts the keyboard in their settings. */
export function editEffectSettings(ids: string[]): void {
  useSequencer.getState().select(ids);
  setTimeout(() => {
    document.querySelector<HTMLElement>('aside[aria-label="Effect settings"]')?.querySelector<HTMLElement>("input, select, button:not([aria-label='Delete effect'])")?.focus();
  });
}

/** The right-click menu for effects: `ids` is what it acts on (the selection, once the effect clicked is in it). */
export function effectMenuItems(ids: string[]): MenuItem[] {
  const canPaste = useSequencer.getState().clipboard.length > 0;
  if (ids.length === 0) return [{ label: "Paste", shortcut: "seq-paste", run: pasteEffectsAtPlayhead, disabled: !canPaste }];
  return [
    { label: "Copy", shortcut: "seq-copy", run: copyEffects },
    { label: "Paste", shortcut: "seq-paste", run: pasteEffectsAtPlayhead, disabled: !canPaste },
    { label: "Duplicate", shortcut: "seq-duplicate", run: () => duplicateEffects(ids) },
    { label: "Delete", shortcut: "seq-delete", run: () => deleteEffects(ids), danger: true },
    { label: "Edit settings", run: () => editEffectSettings(ids), separated: true },
  ];
}
