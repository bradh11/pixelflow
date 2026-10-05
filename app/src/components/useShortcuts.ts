import { useEffect } from "react";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";

/** Global keyboard shortcuts (⌘ on macOS, Ctrl elsewhere). Text fields keep their own undo. */
export function useShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      const state = useApp.getState();
      if (state.pendingReplace) return;
      const key = e.key.toLowerCase();
      const inField = e.target instanceof HTMLElement && ["INPUT", "TEXTAREA", "SELECT"].includes(e.target.tagName);
      // On the Sequence screen, undo, redo, and save act on the open sequence.
      const seq = state.screen === "sequence" && useSequencer.getState().doc ? useSequencer.getState() : null;
      const handlers: Record<string, () => unknown> = {
        k: () => state.setPaletteOpen(!state.paletteOpen),
        s: () => (seq ? (e.shiftKey ? seq.saveAs() : seq.save()) : e.shiftKey ? state.saveAs() : state.save()),
        o: () => state.openShow(),
        n: () => state.newShow(),
      };
      if (!state.started && key !== "o" && key !== "n") return;
      if (key === "z" && !inField) {
        e.preventDefault();
        if (seq) void (e.shiftKey ? seq.redo() : seq.undo());
        else void (e.shiftKey ? state.redo() : state.undo());
        return;
      }
      const handler = handlers[key];
      if (handler) {
        e.preventDefault();
        void handler();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
