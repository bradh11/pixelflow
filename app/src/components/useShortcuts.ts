import { useEffect } from "react";
import { useAssistant } from "../state/assistant";
import { isBusyOrAsking } from "../state/busy";
import { saveFocused, undoFocused } from "../state/menuActions";
import { useShortcutSheet } from "../state/shortcutSheet";
import { useApp } from "../state/store";
import { typing } from "./layout/useLayoutKeys";

/**
 * Global keyboard shortcuts (⌘ on macOS, Ctrl elsewhere). Text fields keep their own undo.
 *
 * ⌘N new show · ⌘O open · ⇧⌘O open recent (the show menu, at its recent shows) · ⌘W close the
 * show (with no show open, the window closes as usual) · ⌘S save · ⇧⌘S save as · ⌘Z / ⇧⌘Z undo
 * and redo · ⌘K command palette · ⌘L assistant · ? the shortcut sheet (when not typing).
 * The full list is in lib/shortcuts.ts.
 */
/** The keys the app (and its File menu) acts on; others (⌘C, ⌘V, ⌘Q, ⌘H…) are left alone. */
const APP_KEYS = new Set(["k", "l", "s", "o", "n", "w", "z"]);

export function useShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "?" && !e.metaKey && !e.ctrlKey && !e.altKey) {
        if (e.defaultPrevented || typing(e.target) || useApp.getState().paletteOpen || isBusyOrAsking()) return;
        e.preventDefault();
        useShortcutSheet.getState().setOpen(true);
        return;
      }
      if (!(e.metaKey || e.ctrlKey)) return;
      const state = useApp.getState();
      const key = e.key.toLowerCase();
      const inField = e.target instanceof HTMLElement && ["INPUT", "TEXTAREA", "SELECT"].includes(e.target.tagName);
      if (isBusyOrAsking()) {
        // A dialog or question is up, or a show is opening: the app's keys wait, and are held
        // back from the menu bar too (it would run the same action). A text field keeps its
        // own undo.
        if (APP_KEYS.has(key) && !(inField && key === "z")) e.preventDefault();
        return;
      }
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
      // ⇧⌘W is Close Window (the File menu's), not Close Show.
      if (key === "w" && e.shiftKey) return;
      if (key === "z" && !inField) {
        e.preventDefault();
        void undoFocused(e.shiftKey, { repeat: e.repeat });
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
