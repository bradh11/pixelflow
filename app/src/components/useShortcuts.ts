import { useEffect } from "react";
import { useAssistant } from "../state/assistant";
import { saveFocused } from "../state/menuActions";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";

/**
 * Global keyboard shortcuts (⌘ on macOS, Ctrl elsewhere). Text fields keep their own undo.
 *
 * ⌘N new show · ⌘O open · ⇧⌘O open recent (the show menu, at its recent shows) · ⌘W close the
 * show (with no show open, the window closes as usual) · ⌘S save · ⇧⌘S save as · ⌘Z / ⇧⌘Z undo
 * and redo · ⌘K command palette · ⌘L assistant.
 */
export function useShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      const state = useApp.getState();
      if (state.pendingReplace || state.naming !== null || useSequencer.getState().replacing) return;
      const key = e.key.toLowerCase();
      const inField = e.target instanceof HTMLElement && ["INPUT", "TEXTAREA", "SELECT"].includes(e.target.tagName);
      // On the Sequence screen, undo, redo, and save act on the open sequence.
      const seq = state.screen === "sequence" && useSequencer.getState().doc ? useSequencer.getState() : null;
      const handlers: Record<string, () => unknown> = {
        k: () => state.setPaletteOpen(!state.paletteOpen),
        l: () => useAssistant.getState().toggle(),
        s: () => saveFocused(e.shiftKey),
        o: () => (e.shiftKey ? state.setShowMenu("recent") : state.openShow()),
        n: () => state.newShow(),
        w: () => state.closeShow(),
      };
      if (!state.started) {
        // The start page: New and Open; ⇧⌘O goes to the recent shows. ⌘W is left to the window.
        if (key === "o" && e.shiftKey) {
          e.preventDefault();
          document.querySelector<HTMLElement>("[data-recent-show]")?.focus();
          return;
        }
        if (key !== "o" && key !== "n") return;
      }
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
