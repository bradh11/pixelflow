// What the File menu in the menu bar does, and the save it shares with ⌘S: the same store
// actions as the show menu and the shortcuts, so each asks about unsaved work the same way.

import type { MenuAction } from "../api/types";
import { isBusyOrAsking, requestWindowClose } from "./busy";
import { useSequencer } from "./sequencer";
import { useApp } from "./store";

/** Save (or Save As…) whatever is being worked on: the open sequence on the Sequence screen,
 * else the show. */
export function saveFocused(as: boolean): Promise<boolean> {
  const app = useApp.getState();
  const sequencer = useSequencer.getState();
  if (app.screen === "sequence" && sequencer.doc) return as ? sequencer.saveAs() : sequencer.save();
  return as ? app.saveAs() : app.save();
}

/** Closes the window (asking about unsaved work first, like its close button). */
export async function closeWindow(): Promise<void> {
  if (requestWindowClose()) await useApp.getState().backend?.closeWindow();
}

/** Runs a File menu item chosen in the menu bar. */
export async function runMenuAction(action: MenuAction): Promise<void> {
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
