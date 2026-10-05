import { type RefObject, useEffect } from "react";
import { duplicateEdits, removeEdits } from "../../lib/layoutEdits";
import { nudgeStep } from "../../lib/layoutMath";
import { useLayoutEditor } from "../../state/layoutEditor";
import { addNudge, flushNudge } from "../../state/layoutGestures";
import { useApp } from "../../state/store";
import type { LayoutCanvasHandle } from "./LayoutCanvas";

const ARROWS: Record<string, [number, number]> = {
  ArrowLeft: [-1, 0],
  ArrowRight: [1, 0],
  ArrowUp: [0, 1],
  ArrowDown: [0, -1],
};

/** A held arrow key's move is sent this long after its last repeat, even if its release is missed. */
const NUDGE_IDLE_MS = 500;

/** Inputs that use these keys themselves: a checkbox or button in the props list doesn't. */
const KEPT_INPUT_TYPES = new Set(["checkbox", "radio", "button", "submit", "reset", "image", "color", "file"]);

/** True while focus is somewhere typing (or a slider or menu) needs the keys. */
function typing(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable || target.tagName === "TEXTAREA" || target.tagName === "SELECT") return true;
  return target instanceof HTMLInputElement && !KEPT_INPUT_TYPES.has(target.type);
}

/**
 * Layout editor keys while the Layout screen is open: Escape, Delete/Backspace, arrow keys
 * (Shift: ten times as far), ⌘A select all, ⌘D duplicate. Typing in a field is left alone.
 * Holding an arrow key moves the selection as it repeats and sends one move (one undo step)
 * when the key is let go.
 */
export function useLayoutKeys(canvas: RefObject<LayoutCanvasHandle | null>) {
  useEffect(() => {
    let idle: ReturnType<typeof setTimeout> | undefined;
    const send = () => {
      clearTimeout(idle);
      void flushNudge();
    };

    // Any other key first sends a move being built up, so undo, delete, and the rest come after it.
    const beforeOthers = (e: KeyboardEvent) => {
      if (!ARROWS[e.key]) send();
    };

    const onKey = (e: KeyboardEvent) => {
      const app = useApp.getState();
      if (app.paletteOpen || app.pendingReplace || e.defaultPrevented) return;
      if (typing(e.target)) return;
      const show = app.snapshot?.show;
      if (!show) return;
      const editor = useLayoutEditor.getState();
      const ids = editor.selected.filter((id) => show.props.some((p) => p.id === id));
      const key = e.key.toLowerCase();

      if (e.metaKey || e.ctrlKey) {
        if (key === "a") {
          e.preventDefault();
          editor.select(show.props.map((p) => p.id));
        } else if (key === "d") {
          e.preventDefault();
          if (ids.length === 0) return;
          let copies: string[] = [];
          const duplicate = (latest: typeof show) => {
            const made = duplicateEdits(latest, ids);
            copies = made.ids;
            return made.edits;
          };
          void app.apply(duplicate).then((ok) => ok && useLayoutEditor.getState().select(copies));
        }
        return;
      }
      if (e.altKey) return;

      if (e.key === "Escape") {
        if (canvas.current?.cancel()) return;
        if (editor.editPhoto) editor.setEditPhoto(false);
        else if (editor.tool !== "select") editor.setTool("select");
        else editor.clear();
        return;
      }
      if (ids.length === 0) return;
      if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        void app.apply(removeEdits(ids)).then((ok) => ok && useLayoutEditor.getState().clear());
        return;
      }
      const arrow = ARROWS[e.key];
      if (arrow) {
        e.preventDefault();
        const step = nudgeStep(editor.snap, editor.grid, e.shiftKey);
        addNudge(ids, arrow[0] * step, arrow[1] * step);
        clearTimeout(idle);
        idle = setTimeout(send, NUDGE_IDLE_MS);
      }
    };

    const onKeyUp = (e: KeyboardEvent) => {
      if (ARROWS[e.key]) send();
    };

    window.addEventListener("keydown", beforeOthers, true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", send);
    return () => {
      window.removeEventListener("keydown", beforeOthers, true);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", send);
      send();
    };
  }, [canvas]);
}
