import { type RefObject, useEffect } from "react";
import { duplicateEdits, gestureEdits, removeEdits } from "../../lib/layoutEdits";
import { nudgeStep } from "../../lib/layoutMath";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useApp } from "../../state/store";
import type { LayoutCanvasHandle } from "./LayoutCanvas";

const ARROWS: Record<string, [number, number]> = {
  ArrowLeft: [-1, 0],
  ArrowRight: [1, 0],
  ArrowUp: [0, 1],
  ArrowDown: [0, -1],
};

/**
 * Layout editor keys while the Layout screen is open: Escape, Delete/Backspace, arrow keys
 * (Shift: ten times as far), ⌘A select all, ⌘D duplicate. Typing in a field is left alone.
 */
export function useLayoutKeys(canvas: RefObject<LayoutCanvasHandle | null>) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const app = useApp.getState();
      if (app.paletteOpen || app.pendingReplace || e.defaultPrevented) return;
      const target = e.target;
      if (target instanceof HTMLElement && (["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName) || target.isContentEditable)) return;
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
          const copies = duplicateEdits(show, ids);
          void app.apply(copies.edits).then((ok) => ok && useLayoutEditor.getState().select(copies.ids));
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
        void app.apply(gestureEdits(show, ids, { kind: "move", dx: arrow[0] * step, dy: arrow[1] * step }));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [canvas]);
}
