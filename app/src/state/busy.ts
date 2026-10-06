// One check, shared by the keyboard shortcuts, the menu bar, and closing the window: is the app
// in the middle of something (a file dialog, an open or import, a save) or waiting on an answer
// (the unsaved-work questions, "Name your show", any other modal dialog)? Then another New,
// Open, Close, or Save would ask a second question over the first, or queue a second dialog.

import { fileDialogShowing } from "../api/fileDialogs";
import { unsavedWork, useCloseGuard } from "./closeGuard";
import { useSequencer } from "./sequencer";
import { useApp } from "./store";

/** True while something is being done or asked, so show actions must wait. */
export function isBusyOrAsking(): boolean {
  const app = useApp.getState();
  return (
    app.pendingReplace !== null ||
    app.naming !== null ||
    app.opening !== null ||
    useSequencer.getState().replacing !== null ||
    useCloseGuard.getState().asking ||
    fileDialogShowing() ||
    modalDialogShowing()
  );
}

function modalDialogShowing(): boolean {
  return typeof document !== "undefined" && document.querySelector('[aria-modal="true"]') !== null;
}

/**
 * The window was asked to close (its close button, Close Window, Quit): true when it may. With
 * unsaved work it asks first (once: asking again shows the same question), but not over another
 * question or while a dialog is up: then it stays open and nothing more is asked. With nothing
 * unsaved it always closes.
 */
export function requestWindowClose(): boolean {
  const guard = useCloseGuard.getState();
  const work = unsavedWork();
  if ((work.show || work.sequence) && !guard.asking && isBusyOrAsking()) return false;
  return guard.request();
}
