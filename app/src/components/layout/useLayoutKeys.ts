import { type RefObject, useEffect } from "react";
import type { Edit, Show } from "../../api/types";
import { duplicateEdits, pasteEdits, removeEdits, updateEdits } from "../../lib/layoutEdits";
import { nudgeStep } from "../../lib/layoutMath";
import { isPoly, removeVertex } from "../../lib/polylineMath";
import { useLayoutEditor } from "../../state/layoutEditor";
import { addNudge, flushNudge } from "../../state/layoutGestures";
import { isBusyOrAsking } from "../../state/busy";
import { groupSelected } from "../../state/groups";
import { useApp } from "../../state/store";
import { deleteProps } from "./PropsList";
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
export function typing(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable || target.tagName === "TEXTAREA" || target.tagName === "SELECT") return true;
  return target instanceof HTMLInputElement && !KEPT_INPUT_TYPES.has(target.type);
}

/** True when some text on the page is picked out (outside any field). */
function textSelected(): boolean {
  const selection = typeof window.getSelection === "function" ? window.getSelection() : null;
  return !!selection && !selection.isCollapsed && selection.toString().trim() !== "";
}

/** How far (layout units, right and down) each paste lands from the last. */
const PASTE_OFFSET = 0.5;

/**
 * Layout editor keys while the Layout screen is open: Escape, Delete/Backspace, arrow keys
 * (Shift: ten times as far), ⌘A select all, ⌘G group the selection, ⌘D duplicate, ⌘C copy, ⌘X cut, ⌘V paste (Ctrl
 * works for ⌘ too). Typing in a field is left alone.
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
      // A dialog or question is up (a delete warning, "Name your show"), or a show is opening:
      // Delete, ⌘D, ⌘G, ⌘X/⌘V, and the arrows wait instead of changing the show behind it.
      if (isBusyOrAsking()) return;
      if (typing(e.target)) return;
      // A poly line being drawn takes Enter, Backspace and Delete before anything else does.
      if (!e.metaKey && !e.ctrlKey && !e.altKey && canvas.current?.polyKey(e.key)) {
        e.preventDefault();
        return;
      }
      const show = app.snapshot?.show;
      if (!show) return;
      const editor = useLayoutEditor.getState();
      const ids = editor.selected.filter((id) => show.props.some((p) => p.id === id));
      const key = e.key.toLowerCase();

      if (e.metaKey || e.ctrlKey) {
        if (e.altKey) return;
        /** Adds props built from the latest show, then selects them (one undo step). */
        const addAndSelect = (build: (latest: Show) => { edits: Edit[]; ids: string[] }) => {
          let made: string[] = [];
          const edits = (latest: Show) => {
            const built = build(latest);
            made = built.ids;
            return built.edits;
          };
          void app.apply(edits).then((ok) => ok && made.length > 0 && useLayoutEditor.getState().select(made));
        };
        if (key === "a") {
          e.preventDefault();
          editor.select(show.props.map((p) => p.id));
        } else if (key === "g") {
          e.preventDefault();
          if (ids.length > 0) void groupSelected();
        } else if (key === "d") {
          e.preventDefault();
          if (ids.length > 0) addAndSelect((latest) => duplicateEdits(latest, ids));
        } else if (key === "c" || key === "x") {
          // Text picked out on the page is copied as text, as usual.
          if (ids.length === 0 || textSelected()) return;
          e.preventDefault();
          const props = structuredClone(show.props.filter((p) => ids.includes(p.id)));
          useLayoutEditor.setState({ clipboard: { props, nextOffset: key === "x" ? 0 : PASTE_OFFSET } });
          if (key === "x") void app.apply(removeEdits(ids)).then((ok) => ok && useLayoutEditor.getState().clear());
        } else if (key === "v") {
          const clipboard = editor.clipboard;
          if (!clipboard) return;
          e.preventDefault();
          useLayoutEditor.setState({ clipboard: { ...clipboard, nextOffset: clipboard.nextOffset + PASTE_OFFSET } });
          addAndSelect((latest) => pasteEdits(latest, clipboard.props, clipboard.nextOffset));
        }
        return;
      }
      if (e.altKey) return;

      if (e.key === "Escape") {
        if (canvas.current?.cancel()) return;
        if (editor.editPhoto) editor.setEditPhoto(false);
        else if (editor.tool !== "select") editor.setTool("select");
        else if (editor.polyPoint) editor.setPolyPoint(null);
        else editor.clear();
        return;
      }
      if (ids.length === 0) return;
      if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        // A point picked on the selected poly line goes, not the whole line, unless the line
        // would be left with fewer than two points: then the line goes.
        const point = editor.polyPoint;
        const picked = point && show.props.find((p) => p.id === point.prop);
        const pointed = picked && isPoly(picked.shape) && picked.shape.vertices.length > 2;
        if (point && pointed && ids.length === 1 && ids[0] === point.prop) {
          editor.setPolyPoint(null);
          void app.apply(updateEdits(point.prop, (p) => (isPoly(p.shape) ? { ...p, shape: removeVertex(p.shape, point.index) ?? p.shape } : p)));
          return;
        }
        void deleteProps(
          ids,
          ids.map((id) => show.props.find((p) => p.id === id)?.name ?? ""),
        );
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
