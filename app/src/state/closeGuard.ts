// Closing the window with unsaved work: ask first (Save / Don't save / Cancel), like New and Open
// do for the show. Both the show and the open sequence count.

import { create } from "zustand";
import { useSequencer } from "./sequencer";
import { useApp } from "./store";

/** The names of what has unsaved changes: the show and the open sequence (null when saved). */
export function unsavedWork(): { show: string | null; sequence: string | null } {
  const app = useApp.getState();
  const seq = useSequencer.getState();
  return {
    show: app.started && app.snapshot?.dirty ? app.snapshot.show.name : null,
    sequence: seq.doc && seq.dirty ? seq.doc.name : null,
  };
}

interface CloseGuard {
  /** The window was asked to close while there was unsaved work: the question is showing. */
  asking: boolean;
  /** The window wants to close: true when it may, else the question is shown. */
  request(): boolean;
  /**
   * The answer: Save (everything unsaved; stays open if a save fails or is cancelled), Don't save,
   * or Cancel. True when the window was closed.
   */
  resolve(choice: "save" | "discard" | "cancel"): Promise<boolean>;
}

export const useCloseGuard = create<CloseGuard>((set) => ({
  asking: false,

  request() {
    const work = unsavedWork();
    if (!work.show && !work.sequence) return true;
    set({ asking: true });
    return false;
  },

  async resolve(choice) {
    if (choice === "cancel") {
      set({ asking: false });
      return false;
    }
    const work = unsavedWork();
    const seq = useSequencer.getState();
    if (choice === "save") {
      if (work.sequence && !(await seq.save())) return false;
      if (work.show && !(await useApp.getState().save())) return false;
    } else if (work.sequence) {
      // Not saving it on purpose: don't offer it back next time.
      await seq.api?.closeSequenceDoc().catch(() => undefined);
    }
    set({ asking: false });
    await useApp.getState().backend?.closeWindow();
    return true;
  },
}));
