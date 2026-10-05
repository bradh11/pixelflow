import { type PointerEvent as ReactPointerEvent, useEffect, useRef } from "react";
import type { Edit, Show } from "../../api/types";
import { dropIndex, moveSlotEdits, nodeCounts, unwireEdits, wirePropEdits } from "../../lib/wiringMath";
import { useApp } from "../../state/store";
import { type DragItem, type DropTarget, useWiring } from "../../state/wiring";

/** Drags this far (screen pixels) before a press becomes a drag. */
const DRAG_PX = 4;

const inside = (r: DOMRect, x: number, y: number) => x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;

/**
 * The drop target under (x, y): a port row (and where among its chips), or the props list.
 * Drop zones are marked `data-wiring-drop` ("port", with `data-controller` and `data-port`, or
 * "props"); chips inside a port are marked `data-wiring-chip`.
 */
export function dropTargetAt(x: number, y: number): DropTarget | null {
  for (const zone of document.querySelectorAll<HTMLElement>("[data-wiring-drop]")) {
    if (!inside(zone.getBoundingClientRect(), x, y)) continue;
    if (zone.dataset.wiringDrop === "props") return { kind: "props" };
    const chips = [...zone.querySelectorAll<HTMLElement>("[data-wiring-chip]")].map((c) => c.getBoundingClientRect());
    return { kind: "port", controller: zone.dataset.controller!, port: Number(zone.dataset.port), index: dropIndex(chips, { x, y }) };
  }
  return null;
}

/** The edits a drop makes, built from the show as it is when their turn comes. */
export function dropEdits(item: DragItem, target: DropTarget): (show: Show) => Edit[] {
  return (show) => {
    const nodes = nodeCounts(useApp.getState().snapshot?.channelMap ?? { frameLen: 0, props: [], controllers: [] });
    if (target.kind === "props") return item.kind === "slot" ? unwireEdits(show, item.from) : [];
    const to = { controller: target.controller, port: target.port, index: target.index };
    return item.kind === "slot" ? moveSlotEdits(show, item.from, to) : wirePropEdits(show, item.prop, to, nodes);
  };
}

/** Lets go of a drag where it is: one batch of edits (one undo step). */
async function drop(item: DragItem, target: DropTarget | null) {
  useWiring.setState({ drag: null });
  if (!target) return;
  const selected = useWiring.getState().selected;
  const ok = await useApp.getState().apply(dropEdits(item, target));
  // The chip moved: its settings no longer point at it.
  if (ok && item.kind === "slot" && selected) useWiring.setState({ selected: null });
}

/**
 * Pointer handlers that make an element a drag source for `item`. A press that doesn't move is
 * a click (`onClick` still fires); Escape lets go of a drag without changing anything.
 */
export function useDragSource(item: DragItem) {
  const press = useRef<{ x: number; y: number; moved: boolean } | null>(null);
  /** Set when a drag ends, so the click the browser sends next is ignored. */
  const dragged = useRef(false);
  const dragging = useWiring((s) => s.drag !== null);

  useEffect(() => {
    if (!dragging) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      press.current = null;
      useWiring.setState({ drag: null });
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [dragging]);

  return {
    onPointerDown(e: ReactPointerEvent<HTMLElement>) {
      if (e.button !== 0) return;
      press.current = { x: e.clientX, y: e.clientY, moved: false };
      dragged.current = false;
      e.currentTarget.setPointerCapture?.(e.pointerId);
    },
    onPointerMove(e: ReactPointerEvent<HTMLElement>) {
      const p = press.current;
      if (!p) return;
      if (!p.moved && Math.hypot(e.clientX - p.x, e.clientY - p.y) < DRAG_PX) return;
      p.moved = true;
      useWiring.setState({ drag: { item, x: e.clientX, y: e.clientY, over: dropTargetAt(e.clientX, e.clientY) } });
    },
    onPointerUp(e: ReactPointerEvent<HTMLElement>) {
      const p = press.current;
      press.current = null;
      if (!p?.moved || !useWiring.getState().drag) return;
      dragged.current = true;
      void drop(item, dropTargetAt(e.clientX, e.clientY));
    },
    onPointerCancel() {
      press.current = null;
      useWiring.setState({ drag: null });
    },
    /** True (once) when the click that follows a drag should be ignored. */
    endedDrag() {
      const was = dragged.current;
      dragged.current = false;
      return was;
    },
  };
}
