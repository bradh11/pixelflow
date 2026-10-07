// What the File menu in the menu bar does, and the save it shares with ⌘S: the same store
// actions as the show menu and the shortcuts, so each asks about unsaved work the same way.

import type { MenuAction } from "../api/types";
import { isBusyOrAsking, requestWindowClose } from "./busy";
import { saveSequenceAndShow } from "./saveAll";
import { useSequencer } from "./sequencer";
import { useApp } from "./store";

/** Save (or Save As…) whatever is being worked on. On the Sequence screen, Save saves the show
 * when it has changes and then the open sequence (one toast says what was saved), and Save As
 * saves the sequence under a new name; elsewhere, the show. */
export function saveFocused(as: boolean): Promise<boolean> {
  const app = useApp.getState();
  const sequencer = useSequencer.getState();
  if (app.screen === "sequence" && sequencer.doc) return as ? sequencer.saveAs() : saveSequenceAndShow();
  return as ? app.saveAs() : app.save();
}

/** Undo (or redo) whatever is being worked on: the open sequence on the Sequence screen, else
 * the show. */
export function undoFocused(redo: boolean): Promise<boolean> {
  const app = useApp.getState();
  const sequencer = useSequencer.getState();
  if (app.screen === "sequence" && sequencer.doc) return redo ? sequencer.redo() : sequencer.undo();
  return redo ? app.redo() : app.undo();
}

/** True when a text field (or other editable text) has the keyboard. */
export function editingText(): boolean {
  const active = typeof document === "undefined" ? null : document.activeElement;
  return active instanceof HTMLElement && (["INPUT", "TEXTAREA", "SELECT"].includes(active.tagName) || active.isContentEditable);
}

/** Closes the window (asking about unsaved work first, like its close button). */
export async function closeWindow(): Promise<void> {
  if (requestWindowClose()) await useApp.getState().backend?.closeWindow();
}

/** Runs a File menu item chosen in the menu bar. */
export async function runMenuAction(action: MenuAction): Promise<void> {
  if (action.action === "undo" || action.action === "redo") {
    // Edit → Undo / Redo (and ⌘Z / ⇧⌘Z the window left alone): a text field's own undo while
    // one is being typed in, else the show's or the sequence's.
    if (editingText()) document.execCommand(action.action);
    else if (useApp.getState().started && !isBusyOrAsking()) await undoFocused(action.action === "redo");
    return;
  }
  // A dialog or question is up, or a show is opening: that comes first.
  if (isBusyOrAsking()) return;
  const app = useApp.getState();
  switch (action.action) {
    case "newShow":
      await app.newShow();
      break;
    case "openShow":
      await app.openShow();
      break;
    case "openRecent":
      await app.openRecent(action.path);
      break;
    case "clearRecent":
      await app.clearRecent();
      break;
    case "closeShow":
      // ⌘W closes the show; with no show open, it closes the window.
      if (app.started) await app.closeShow();
      else await closeWindow();
      break;
    case "save":
    case "saveAs":
      if (app.started) await saveFocused(action.action === "saveAs");
      break;
  }
}
