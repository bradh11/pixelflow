import { type PointerEvent as ReactPointerEvent, useEffect, useRef } from "react";
import type { Edit, Show } from "../../api/types";
import { dropIndex, moveSlotEdits, nodeCounts, unwireEdits, wirePropEdits } from "../../lib/wiringMath";
import { useApp } from "../../state/store";
import { type DragItem, type DropTarget, sameTarget, useWiring } from "../../state/wiring";

/** Drags this far (screen pixels) before a press becomes a drag. */
const DRAG_PX = 4;

const inside = (r: DOMRect, x: number, y: number) => x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;

/**
 * The drop target under (x, y): a port row (and where among its chips), or the props list.
 * Drop zones are marked `data-wiring-drop` ("port", with `data-controller`, `data-port` and
 * `data-port-at`, or "props"); chips inside a port are marked `data-wiring-chip`.
 */
export function dropTargetAt(x: number, y: number): DropTarget | null {
  for (const zone of document.querySelectorAll<HTMLElement>("[data-wiring-drop]")) {
    if (!inside(zone.getBoundingClientRect(), x, y)) continue;
    if (zone.dataset.wiringDrop === "props") return { kind: "props" };
    const chips = [...zone.querySelectorAll<HTMLElement>("[data-wiring-chip]")].map((c) => c.getBoundingClientRect());
    const at = zone.dataset.portAt === undefined ? undefined : Number(zone.dataset.portAt);
    return { kind: "port", controller: zone.dataset.controller!, port: Number(zone.dataset.port), at, index: dropIndex(chips, { x, y }) };
  }
  return null;
}

/** The edits a drop makes, built from the show as it is when their turn comes. */
export function dropEdits(item: DragItem, target: DropTarget): (show: Show) => Edit[] {
  return (show) => {
    const nodes = nodeCounts(useApp.getState().snapshot?.channelMap ?? { frameLen: 0, props: [], controllers: [] });
    if (target.kind === "props") return item.kind === "slot" ? unwireEdits(show, item.from) : [];
    const to = { controller: target.controller, port: target.port, at: target.at, index: target.index };
    return item.kind === "slot" ? moveSlotEdits(show, item.from, to) : wirePropEdits(show, item.prop, to, nodes);
  };
}

/** Lets go of a drag where it is: one batch of edits (one undo step). An open settings panel
 * follows its slot by identity, so it needs nothing here. */
async function drop(item: DragItem, target: DropTarget | null) {
  useWiring.setState({ drag: null });
  if (!target) return;
  await useApp.getState().apply(dropEdits(item, target));
}

/** Escape lets go of whatever is being dragged, changing nothing. One listener for the screen,
 * while a drag is on. */
export function useDragEscape() {
  const dragging = useWiring((s) => s.drag !== null);
  useEffect(() => {
    if (!dragging) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      useWiring.setState({ drag: null });
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [dragging]);
}

/**
 * Pointer handlers that make an element a drag source for `item`. A press that doesn't move is
 * a click (`onClick` still fires); after a drag (dropped or let go with Escape) the click is
 * ignored. The item is the one under the pointer when the press began.
 */
export function useDragSource(item: DragItem) {
  const press = useRef<{ x: number; y: number; moved: boolean; item: DragItem } | null>(null);
  /** Set when a drag ends, so the click the browser sends next is ignored. */
  const dragged = useRef(false);
  const owner = useRef({});

  // Removed mid-drag (an undo took the chip away): let go rather than leave the drag stuck.
  useEffect(() => {
    const me = owner.current;
    return () => {
      if (useWiring.getState().drag?.owner === me) useWiring.setState({ drag: null });
    };
  }, []);

  const letGo = () => {
    if (press.current?.moved) dragged.current = true;
    press.current = null;
    if (useWiring.getState().drag?.owner === owner.current) useWiring.setState({ drag: null });
  };

  return {
    onPointerDown(e: ReactPointerEvent<HTMLElement>) {
      if (e.button !== 0) return;
      press.current = { x: e.clientX, y: e.clientY, moved: false, item };
      dragged.current = false;
      e.currentTarget.setPointerCapture?.(e.pointerId);
    },
    onPointerMove(e: ReactPointerEvent<HTMLElement>) {
      const p = press.current;
      if (!p) return;
      if (!p.moved && Math.hypot(e.clientX - p.x, e.clientY - p.y) < DRAG_PX) return;
      const current = useWiring.getState().drag;
      // Let go with Escape: this press is done.
      if (p.moved && current?.owner !== owner.current) return;
      p.moved = true;
      const target = dropTargetAt(e.clientX, e.clientY);
      const over = current && sameTarget(current.over, target) ? current.over : target;
      useWiring.setState({ drag: { item: p.item, x: e.clientX, y: e.clientY, over, owner: owner.current } });
    },
    onPointerUp(e: ReactPointerEvent<HTMLElement>) {
      const p = press.current;
      press.current = null;
      if (!p?.moved) return;
      dragged.current = true;
      if (useWiring.getState().drag?.owner !== owner.current) return;
      void drop(p.item, dropTargetAt(e.clientX, e.clientY));
    },
    onPointerCancel: letGo,
    /** The browser took the pointer back (the element went away, say) before the press ended. */
    onLostPointerCapture() {
      if (press.current) letGo();
    },
    /** True (once) when the click that follows a drag should be ignored. */
    endedDrag() {
      const was = dragged.current;
      dragged.current = false;
      return was;
    },
  };
}
