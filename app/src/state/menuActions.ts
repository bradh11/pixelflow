// What the File menu in the menu bar does, and the save it shares with ⌘S: the same store
// actions as the show menu and the shortcuts, so each asks about unsaved work the same way.

import type { MenuAction } from "../api/types";
import { isBusyOrAsking, requestWindowClose } from "./busy";
import { saveSequenceAndShow } from "./saveAll";
import { useSequencer } from "./sequencer";
import { useApp } from "./store";
import { toast } from "./toast";
import { type UndoDoc, nextLabels, useUndoLabels } from "./undoLabels";

/** Save (or Save As…) whatever is being worked on. On the Sequence screen, Save saves the show
 * when it has changes and then the open sequence (one toast says what was saved), and Save As
 * saves the sequence under a new name; elsewhere, the show. */
export function saveFocused(as: boolean): Promise<boolean> {
  const app = useApp.getState();
  const sequencer = useSequencer.getState();
  if (app.screen === "sequence" && sequencer.doc) return as ? sequencer.saveAs() : saveSequenceAndShow();
  return as ? app.saveAs() : app.save();
}

/** True on the Sequence screen with a sequence open: Undo and Redo then work on the sequence. */
function onSequence(): boolean {
  return useApp.getState().screen === "sequence" && useSequencer.getState().doc !== null;
}

/** Whether the document can redo now. */
function canRedo(doc: UndoDoc): boolean {
  return doc === "sequence" ? useSequencer.getState().canRedo : (useApp.getState().snapshot?.canRedo ?? false);
}

/**
 * What Redo acts on: off the Sequence screen, the show. On it, whatever was undone there last and
 * can still be redone (the layout change taken back from the hint, or a sequence step), else the
 * sequence.
 */
export function redoTarget(): UndoDoc {
  if (!onSequence()) return "show";
  const undone = useUndoLabels.getState().undone;
  for (let i = undone.length - 1; i >= 0; i--) if (canRedo(undone[i])) return undone[i];
  return "sequence";
}

/** Notes what an undo on the Sequence screen took back, for Redo; a redo takes it off again. */
function noteUndone(doc: UndoDoc) {
  useUndoLabels.setState((s) => ({ undone: [...s.undone, doc].slice(-200) }));
}

function noteRedone(doc: UndoDoc) {
  useUndoLabels.setState((s) => {
    const i = s.undone.lastIndexOf(doc);
    return { undone: i < 0 ? s.undone : [...s.undone.slice(0, i), ...s.undone.slice(i + 1)] };
  });
}

/** Takes back one layout (show) step from the Sequence screen, remembering it for Redo. */
async function undoLayoutStep(): Promise<boolean> {
  const ok = await useApp.getState().undo();
  if (ok && onSequence()) noteUndone("show");
  return ok;
}

/**
 * Undo (or redo) whatever is being worked on. Off the Sequence screen, the show. On it, only the
 * sequence: with nothing to undo there, a hint offers the latest layout change instead (once per
 * press: a held key's repeats do nothing). Redo follows what was last undone there.
 */
export async function undoFocused(redo: boolean, { repeat = false }: { repeat?: boolean } = {}): Promise<boolean> {
  const app = useApp.getState();
  if (!onSequence()) return redo ? app.redo() : app.undo();
  const sequencer = useSequencer.getState();
  if (redo) {
    const doc = redoTarget();
    const ok = doc === "show" ? await app.redo() : await sequencer.redo();
    if (ok) noteRedone(doc);
    return ok;
  }
  if (sequencer.canUndo) {
    const ok = await sequencer.undo();
    if (ok) noteUndone("sequence");
    return ok;
  }
  const show = app.snapshot;
  if (repeat || !show?.canUndo) return false;
  const what = nextLabels(useUndoLabels.getState().show, show.revision).undo;
  toast(
    what ? `Nothing to undo in the sequence. Undo the layout change “${what}”?` : "Nothing to undo in the sequence. Undo the last layout change?",
    { label: "Undo layout change", run: undoLayoutStep },
    "info",
  );
  return false;
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
