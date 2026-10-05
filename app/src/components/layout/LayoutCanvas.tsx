import { type PointerEvent as ReactPointerEvent, type Ref, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import type { Background, Edit, PreviewProp, Show } from "../../api/types";
import { frontView } from "../../lib/geometry";
import { gestureEdits } from "../../lib/layoutEdits";
import {
  type Box,
  type Gesture,
  type Handle,
  type Pt,
  type Size,
  type View,
  DRAWN_BY_ENDS,
  backgroundBox,
  boxCenter,
  boxFrom,
  boxOfPoints,
  drawnProp,
  fitView,
  gesturePoint,
  handleAt,
  handlePositions,
  hitTest,
  inBox,
  moveBackground,
  moveGesture,
  panBy,
  placedProp,
  propsInBox,
  resizeBackground,
  rotateGesture,
  scaleGesture,
  snapPoint,
  toScreen,
  toWorld,
  unionBox,
  wheelIntent,
  wheelZoomFactor,
  zoomAt,
} from "../../lib/layoutMath";
import { type PropKind, newProp } from "../../lib/shows";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useApp } from "../../state/store";
import type { PhotoImage } from "./useLayoutData";

/** How close (screen pixels) a click must be to a pixel to pick its prop. */
const HIT_PX = 8;
/** Drags shorter than this (screen pixels) count as clicks. */
const CLICK_PX = 4;
const BACKDROP = "#0a0a0c";
const UNLIT = "rgba(220, 220, 220, 0.7)";
const ACCENT = "#a78bfa";
const DARK_PIXEL = "rgba(90, 90, 90, 0.6)";

type Corner = Exclude<Handle, "rotate">;

type Drag =
  | { kind: "pan"; last: Pt }
  | { kind: "move"; ids: string[]; from: Pt; origin: Pt | null; gesture: Gesture }
  | { kind: "scale"; ids: string[]; box: Box; handle: Corner; from: Pt; gesture: Gesture }
  | { kind: "rotate"; ids: string[]; center: Pt; from: Pt; gesture: Gesture }
  | { kind: "marquee"; from: Pt; to: Pt; fromScreen: Pt; toScreen: Pt; additive: string[] }
  | { kind: "draw"; tool: PropKind; from: Pt; to: Pt; fromScreen: Pt; toScreen: Pt }
  | { kind: "photo"; corner: Corner | null; from: Pt; start: Background };

export interface LayoutCanvasHandle {
  /** Stops a drag in progress, leaving everything as it was. True if there was one. */
  cancel(): boolean;
}

interface LayoutCanvasProps {
  preview: PreviewProp[];
  frame: Uint8Array | null;
  show: Show;
  photo: PhotoImage;
  apply(edits: Edit[]): Promise<boolean>;
  ref?: Ref<LayoutCanvasHandle>;
}

function movedPoints(points: ArrayLike<number>, g: Gesture): number[] {
  const out = new Array<number>(points.length);
  for (let i = 0; i + 1 < points.length; i += 2) {
    const p = gesturePoint(g, { x: points[i], y: points[i + 1] });
    out[i] = p.x;
    out[i + 1] = p.y;
  }
  return out;
}

const CURSORS: Record<Handle, string> = {
  nw: "nwse-resize",
  se: "nwse-resize",
  ne: "nesw-resize",
  sw: "nesw-resize",
  rotate: "grab",
};

/**
 * The layout drawn over the background photo, where props are selected, moved, turned,
 * resized, and drawn. Every finished drag is sent as one batch of edits (one undo step); while
 * dragging, the props are only redrawn here.
 */
export function LayoutCanvas({ preview, frame, show, photo, apply, ref }: LayoutCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const drag = useRef<Drag | null>(null);
  /** A finished gesture still drawn until the engine's new positions arrive. */
  const pending = useRef<{ ids: string[]; gesture: Gesture; preview: PreviewProp[] } | null>(null);
  const spaceHeld = useRef(false);
  const frameRequest = useRef<number | null>(null);
  const [cursor, setCursor] = useState("default");
  const [hovered, setHovered] = useState<string | null>(null);

  const editor = useLayoutEditor();
  // Everything the drawing and pointer handlers need, current as of the last render.
  const latest = useRef({ preview, frame, show, photo, editor, apply });
  latest.current = { preview, frame, show, photo, editor, apply };

  const size = (): Size => {
    const canvas = canvasRef.current;
    return { width: canvas?.clientWidth ?? 0, height: canvas?.clientHeight ?? 0 };
  };

  const background = (): Background | null => {
    const { editor, show } = latest.current;
    return editor.photoDraft ?? show.background ?? null;
  };

  /** Everything worth showing: the props and the photo. */
  const contentBox = (): Box | null => {
    const { preview, photo } = latest.current;
    const bg = background();
    return unionBox([...preview.map((p) => boxOfPoints(p.points)), bg ? backgroundBox(bg, photo.aspect) : null]);
  };

  const currentView = (): View => useLayoutEditor.getState().view ?? fitView(contentBox(), size());

  const liveGesture = (): { ids: string[]; gesture: Gesture } | null => {
    const d = drag.current;
    if (d && (d.kind === "move" || d.kind === "scale" || d.kind === "rotate")) return d;
    return pending.current;
  };

  /** The props as they should look now, with any dragged ones where they're being dragged. */
  const effectivePreview = (): PreviewProp[] => {
    const { preview } = latest.current;
    const live = liveGesture();
    if (!live) return preview;
    return preview.map((p) => (live.ids.includes(p.prop) ? { ...p, points: movedPoints(p.points, live.gesture) } : p));
  };

  const selectionBox = (props: PreviewProp[]): Box | null => {
    const selected = new Set(useLayoutEditor.getState().selected);
    return unionBox(props.filter((p) => selected.has(p.prop)).map((p) => boxOfPoints(p.points)));
  };

  const draw = useCallback(() => {
    frameRequest.current = null;
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const { frame, photo, editor } = latest.current;
    const ratio = window.devicePixelRatio || 1;
    const s = size();
    if (canvas.width !== Math.round(s.width * ratio) || canvas.height !== Math.round(s.height * ratio)) {
      canvas.width = Math.round(s.width * ratio);
      canvas.height = Math.round(s.height * ratio);
    }
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.fillStyle = BACKDROP;
    ctx.fillRect(0, 0, s.width, s.height);
    const view = currentView();
    const at = (p: Pt) => toScreen(view, s, p);

    const bg = background();
    if (bg) {
      const box = backgroundBox(bg, photo.aspect);
      const tl = at({ x: box.minX, y: box.maxY });
      const br = at({ x: box.maxX, y: box.minY });
      if (photo.image) {
        ctx.globalAlpha = bg.opacity;
        ctx.drawImage(photo.image, tl.x, tl.y, br.x - tl.x, br.y - tl.y);
        ctx.globalAlpha = 1;
      }
      if (editor.editPhoto || !photo.image) {
        ctx.setLineDash(editor.editPhoto ? [] : [6, 4]);
        ctx.strokeStyle = editor.editPhoto ? ACCENT : "rgba(255,255,255,0.3)";
        ctx.lineWidth = editor.editPhoto ? 2 : 1;
        ctx.strokeRect(tl.x, tl.y, br.x - tl.x, br.y - tl.y);
        ctx.setLineDash([]);
      }
      if (editor.editPhoto) {
        const handles = handlePositions(box, view, s);
        for (const h of ["nw", "ne", "sw", "se"] as Corner[]) drawHandle(ctx, handles[h]);
      }
    }

    if (editor.snap) drawGrid(ctx, view, s, editor.grid);

    const props = effectivePreview();
    const selected = new Set(editor.selected);
    const radius = Math.min(4.5, Math.max(1.3, view.zoom * 0.05));
    for (const p of props) {
      const pts = p.points;
      if (!frame) {
        ctx.fillStyle = selected.has(p.prop) ? ACCENT : UNLIT;
        ctx.beginPath();
        for (let i = 0; i + 1 < pts.length; i += 2) {
          const q = at({ x: pts[i], y: pts[i + 1] });
          ctx.moveTo(q.x + radius, q.y);
          ctx.arc(q.x, q.y, radius, 0, Math.PI * 2);
        }
        ctx.fill();
        continue;
      }
      for (let i = 0, n = 0; i + 1 < pts.length; i += 2, n++) {
        const q = at({ x: pts[i], y: pts[i + 1] });
        const o = p.frameOffset + n * p.channelsPerPixel;
        const [r, g, b] = o + 2 < frame.length ? [frame[o], frame[o + 1], frame[o + 2]] : [0, 0, 0];
        ctx.fillStyle = r + g + b === 0 ? DARK_PIXEL : `rgb(${r}, ${g}, ${b})`;
        ctx.beginPath();
        ctx.arc(q.x, q.y, radius, 0, Math.PI * 2);
        ctx.fill();
      }
    }

    const d = drag.current;
    if (d?.kind === "draw") {
      const draft = draftProp(d);
      if (draft) {
        const pts = frontView(draft);
        ctx.fillStyle = ACCENT;
        ctx.beginPath();
        for (let i = 0; i + 1 < pts.length; i += 2) {
          const q = at({ x: pts[i], y: pts[i + 1] });
          ctx.moveTo(q.x + radius, q.y);
          ctx.arc(q.x, q.y, radius, 0, Math.PI * 2);
        }
        ctx.fill();
      }
      ctx.setLineDash([5, 4]);
      ctx.strokeStyle = ACCENT;
      ctx.lineWidth = 1;
      const [a, b] = [at(d.from), at(d.to)];
      if (DRAWN_BY_ENDS.includes(d.tool)) {
        ctx.beginPath();
        ctx.moveTo(a.x, a.y);
        ctx.lineTo(b.x, b.y);
        ctx.stroke();
      } else {
        ctx.strokeRect(Math.min(a.x, b.x), Math.min(a.y, b.y), Math.abs(b.x - a.x), Math.abs(b.y - a.y));
      }
      ctx.setLineDash([]);
    }

    if (!editor.editPhoto && selected.size > 0) {
      const box = selectionBox(props);
      if (box) {
        const handles = handlePositions(box, view, s);
        ctx.strokeStyle = ACCENT;
        ctx.lineWidth = 1;
        ctx.setLineDash([5, 4]);
        ctx.strokeRect(handles.nw.x, handles.nw.y, handles.se.x - handles.nw.x, handles.se.y - handles.nw.y);
        ctx.setLineDash([]);
        ctx.beginPath();
        ctx.moveTo(handles.rotate.x, handles.nw.y);
        ctx.lineTo(handles.rotate.x, handles.rotate.y);
        ctx.stroke();
        for (const h of ["nw", "ne", "sw", "se"] as Corner[]) drawHandle(ctx, handles[h]);
        ctx.fillStyle = "#fff";
        ctx.beginPath();
        ctx.arc(handles.rotate.x, handles.rotate.y, 5, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
      }
    }

    if (d?.kind === "marquee") {
      const [a, b] = [d.fromScreen, d.toScreen];
      ctx.fillStyle = "rgba(167, 139, 250, 0.12)";
      ctx.strokeStyle = ACCENT;
      ctx.lineWidth = 1;
      ctx.fillRect(Math.min(a.x, b.x), Math.min(a.y, b.y), Math.abs(b.x - a.x), Math.abs(b.y - a.y));
      ctx.strokeRect(Math.min(a.x, b.x), Math.min(a.y, b.y), Math.abs(b.x - a.x), Math.abs(b.y - a.y));
    }
    // Every helper used here reads `latest`, so the function never needs to change.
  }, []);

  const redraw = useCallback(() => {
    if (frameRequest.current !== null) return;
    if (typeof requestAnimationFrame === "function") frameRequest.current = requestAnimationFrame(draw);
    else draw();
  }, [draw]);

  // Redraw after every render: new positions, colors, selection, photo, or tool.
  useEffect(redraw);

  // A finished gesture is drawn locally until the engine's positions for it arrive.
  useEffect(() => {
    if (pending.current && pending.current.preview !== preview) pending.current = null;
  }, [preview]);

  // Fit everything in once the canvas has a size and the props have arrived.
  const view = editor.view;
  useEffect(() => {
    if (view) return;
    const s = size();
    const ready = show.props.length === 0 || preview.length > 0;
    if (s.width > 0 && ready) useLayoutEditor.getState().setView(fitView(contentBox(), s));
  }, [view, preview, show.props.length, photo.aspect]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const observer = new ResizeObserver(() => {
      if (!useLayoutEditor.getState().view && size().width > 0) {
        useLayoutEditor.getState().setView(fitView(contentBox(), size()));
      }
      redraw();
    });
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [redraw]);

  const point = (e: { clientX: number; clientY: number }): Pt => {
    const rect = canvasRef.current!.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  };

  // Wheel: zoom about the cursor, or pan (a passive React listener can't stop page scrolling).
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const v = currentView();
      const setView = useLayoutEditor.getState().setView;
      if (wheelIntent(e) === "zoom") setView(zoomAt(v, size(), point(e), wheelZoomFactor(e)));
      else setView(panBy(v, -e.deltaX, -e.deltaY));
    };
    canvas.addEventListener("wheel", onWheel, { passive: false });
    return () => canvas.removeEventListener("wheel", onWheel);
  }, []);

  // Space held: drag to move the view.
  useEffect(() => {
    const quietTarget = (t: EventTarget | null) => t === canvasRef.current || t === document.body;
    const down = (e: KeyboardEvent) => {
      if (e.key === " " && quietTarget(e.target)) {
        e.preventDefault();
        if (!spaceHeld.current) {
          spaceHeld.current = true;
          setCursor("grab");
        }
      }
    };
    const up = (e: KeyboardEvent) => {
      if (e.key === " ") {
        spaceHeld.current = false;
        setCursor("default");
      }
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
    };
  }, []);

  const cancel = () => {
    const d = drag.current;
    if (!d) return false;
    drag.current = null;
    if (d.kind === "photo") useLayoutEditor.getState().setPhotoDraft(null);
    redraw();
    return true;
  };
  useImperativeHandle(ref, () => ({ cancel }));

  function draftProp(d: Extract<Drag, { kind: "draw" }>) {
    const base = newProp(d.tool, latest.current.show);
    const click = Math.hypot(d.toScreen.x - d.fromScreen.x, d.toScreen.y - d.fromScreen.y) < CLICK_PX;
    return click ? placedProp(base, d.from) : drawnProp(d.tool, d.from, d.to, base);
  }

  const onPointerDown = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    if (e.button !== 0 && e.button !== 1) return;
    const canvas = canvasRef.current!;
    canvas.focus();
    canvas.setPointerCapture?.(e.pointerId);
    const s = point(e);
    const v = currentView();
    const w = toWorld(v, size(), s);
    const st = useLayoutEditor.getState();
    const { show, photo } = latest.current;

    if (e.button === 1 || spaceHeld.current || st.tool === "pan") {
      drag.current = { kind: "pan", last: s };
      setCursor("grabbing");
      return;
    }
    if (st.editPhoto) {
      const bg = background();
      const box = bg ? backgroundBox(bg, photo.aspect) : null;
      const handle = box ? handleAt(box, v, size(), s) : null;
      if (bg && handle && handle !== "rotate") drag.current = { kind: "photo", corner: handle, from: w, start: bg };
      else if (bg && box && inBox(box, w)) drag.current = { kind: "photo", corner: null, from: w, start: bg };
      else drag.current = { kind: "pan", last: s };
      return;
    }
    if (st.tool !== "select") {
      const p = st.snap ? snapPoint(w, st.grid) : w;
      drag.current = { kind: "draw", tool: st.tool, from: p, to: p, fromScreen: s, toScreen: s };
      redraw();
      return;
    }

    const props = effectivePreview();
    const box = selectionBox(props);
    // Handles work with Shift held from the start (Shift picks free resizing or 15° steps).
    if (box) {
      const handle = handleAt(box, v, size(), s);
      const ids = st.selected;
      if (handle === "rotate") {
        const { cx, cy } = boxCenter(box);
        drag.current = { kind: "rotate", ids, center: { x: cx, y: cy }, from: w, gesture: { kind: "rotate", cx, cy, deg: 0 } };
        return;
      }
      if (handle) {
        drag.current = { kind: "scale", ids, box, handle, from: w, gesture: { kind: "scale", ax: 0, ay: 0, fx: 1, fy: 1 } };
        return;
      }
    }
    const startMove = (ids: string[]) => {
      const first = show.props.find((p) => p.id === ids[0]);
      const origin = first ? { x: first.transform.position.x, y: first.transform.position.y } : null;
      drag.current = { kind: "move", ids, from: w, origin, gesture: { kind: "move", dx: 0, dy: 0 } };
    };
    const hit = hitTest(props, w, HIT_PX / v.zoom);
    if (hit) {
      if (e.shiftKey) {
        st.toggle(hit);
        return;
      }
      if (!st.selected.includes(hit)) st.select([hit]);
      startMove(st.selected.includes(hit) ? st.selected : [hit]);
      return;
    }
    if (box && !e.shiftKey && inBox(box, w)) {
      startMove(st.selected);
      return;
    }
    if (!e.shiftKey) st.clear();
    drag.current = { kind: "marquee", from: w, to: w, fromScreen: s, toScreen: s, additive: e.shiftKey ? st.selected : [] };
  };

  const updateHover = (s: Pt) => {
    const st = useLayoutEditor.getState();
    const v = currentView();
    const w = toWorld(v, size(), s);
    if (spaceHeld.current || st.tool === "pan") return setCursor("grab");
    if (st.editPhoto) {
      const bg = background();
      const box = bg ? backgroundBox(bg, latest.current.photo.aspect) : null;
      const handle = box ? handleAt(box, v, size(), s) : null;
      return setCursor(handle && handle !== "rotate" ? CURSORS[handle] : box && inBox(box, w) ? "move" : "grab");
    }
    if (st.tool !== "select") return setCursor("crosshair");
    const props = effectivePreview();
    const box = selectionBox(props);
    const handle = box ? handleAt(box, v, size(), s) : null;
    const hit = hitTest(props, w, HIT_PX / v.zoom);
    setHovered(hit);
    setCursor(handle ? CURSORS[handle] : hit || (box && inBox(box, w)) ? "move" : "default");
  };

  const onPointerMove = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    const s = point(e);
    const d = drag.current;
    if (!d) return updateHover(s);
    const v = currentView();
    const w = toWorld(v, size(), s);
    const st = useLayoutEditor.getState();
    switch (d.kind) {
      case "pan":
        st.setView(panBy(v, s.x - d.last.x, s.y - d.last.y));
        d.last = s;
        break;
      case "move":
        d.gesture = moveGesture(d.from, w, d.origin, st.snap ? st.grid : null);
        break;
      case "scale":
        d.gesture = scaleGesture(d.box, d.handle, d.from, w, e.shiftKey);
        break;
      case "rotate":
        d.gesture = rotateGesture(d.center, d.from, w, e.shiftKey);
        break;
      case "marquee":
        d.to = w;
        d.toScreen = s;
        break;
      case "draw":
        d.to = st.snap ? snapPoint(w, st.grid) : w;
        d.toScreen = s;
        break;
      case "photo": {
        const aspect = latest.current.photo.aspect;
        st.setPhotoDraft(d.corner ? resizeBackground(d.start, aspect, d.corner, w) : moveBackground(d.start, w.x - d.from.x, w.y - d.from.y));
        break;
      }
    }
    redraw();
  };

  const onPointerUp = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    canvasRef.current?.releasePointerCapture?.(e.pointerId);
    const d = drag.current;
    drag.current = null;
    if (!d) return;
    const st = useLayoutEditor.getState();
    const { show, preview, apply } = latest.current;
    switch (d.kind) {
      case "pan":
        setCursor(spaceHeld.current || st.tool === "pan" ? "grab" : "default");
        break;
      case "move":
      case "scale":
      case "rotate": {
        const edits = gestureEdits(show, d.ids, d.gesture);
        if (edits.length === 0) break;
        pending.current = { ids: d.ids, gesture: d.gesture, preview };
        const revision = useApp.getState().snapshot?.revision;
        void apply(edits).then((ok) => {
          // Refused, or too small to change anything: no new positions are coming.
          if (!ok || useApp.getState().snapshot?.revision === revision) pending.current = null;
          redraw();
        });
        break;
      }
      case "marquee": {
        const dragged = Math.hypot(d.toScreen.x - d.fromScreen.x, d.toScreen.y - d.fromScreen.y) >= CLICK_PX;
        if (dragged) st.select([...d.additive, ...propsInBox(effectivePreview(), boxFrom(d.from, d.to))]);
        break;
      }
      case "draw": {
        const prop = draftProp(d);
        void apply([{ type: "addProp", prop }]).then((ok) => {
          if (!ok) return;
          const now = useLayoutEditor.getState();
          now.setTool("select");
          now.select([prop.id]);
        });
        break;
      }
      case "photo": {
        const draft = st.photoDraft;
        if (!draft || JSON.stringify(draft) === JSON.stringify(d.start)) {
          st.setPhotoDraft(null);
          break;
        }
        void apply([{ type: "setBackground", background: draft }]).then(() => useLayoutEditor.getState().setPhotoDraft(null));
        break;
      }
    }
    redraw();
  };

  const hoveredName = hovered ? show.props.find((p) => p.id === hovered)?.name : null;

  return (
    <div className="relative h-full w-full overflow-hidden rounded-lg border border-neutral-200 dark:border-neutral-800">
      <canvas
        ref={canvasRef}
        role="application"
        aria-label="Layout canvas"
        aria-describedby="layout-canvas-help"
        tabIndex={0}
        data-testid="layout-canvas"
        className="block h-full w-full touch-none select-none"
        style={{ cursor: drag.current?.kind === "pan" ? "grabbing" : cursor }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={() => cancel()}
        onPointerLeave={() => setHovered(null)}
      />
      <p id="layout-canvas-help" className="sr-only">
        Click a prop to select it, or shift-click to select more. Drag across empty space to select everything inside. Drag
        selected props to move them, drag a corner handle to resize them, or drag the round handle above them to turn them.
        Arrow keys move the selection, Delete removes it, and Escape clears it. Hold Space and drag, or scroll with two
        fingers, to move around; pinch or hold Command and scroll to zoom. Every prop is also in the props list below.
      </p>
      {hoveredName && (
        <div className="pointer-events-none absolute bottom-2 left-2 rounded bg-black/70 px-2 py-0.5 text-xs text-white">
          {hoveredName}
        </div>
      )}
    </div>
  );
}

function drawHandle(ctx: CanvasRenderingContext2D, p: Pt) {
  ctx.fillStyle = "#fff";
  ctx.strokeStyle = ACCENT;
  ctx.lineWidth = 1.5;
  ctx.fillRect(p.x - 4, p.y - 4, 8, 8);
  ctx.strokeRect(p.x - 4, p.y - 4, 8, 8);
}

/** Grid lines every `grid` units, thinned out so they're never closer than 8 pixels. */
function drawGrid(ctx: CanvasRenderingContext2D, view: View, size: Size, grid: number) {
  let step = grid;
  while (step * view.zoom < 8) step *= 2;
  const tl = toWorld(view, size, { x: 0, y: 0 });
  const br = toWorld(view, size, { x: size.width, y: size.height });
  ctx.strokeStyle = "rgba(255, 255, 255, 0.07)";
  ctx.lineWidth = 1;
  ctx.beginPath();
  for (let x = Math.ceil(tl.x / step) * step; x <= br.x; x += step) {
    const sx = Math.round(toScreen(view, size, { x, y: 0 }).x) + 0.5;
    ctx.moveTo(sx, 0);
    ctx.lineTo(sx, size.height);
  }
  for (let y = Math.ceil(br.y / step) * step; y <= tl.y; y += step) {
    const sy = Math.round(toScreen(view, size, { x: 0, y }).y) + 0.5;
    ctx.moveTo(0, sy);
    ctx.lineTo(size.width, sy);
  }
  ctx.stroke();
  // The ground line (y = 0) a little stronger.
  const ground = Math.round(toScreen(view, size, { x: 0, y: 0 }).y) + 0.5;
  ctx.strokeStyle = "rgba(255, 255, 255, 0.18)";
  ctx.beginPath();
  ctx.moveTo(0, ground);
  ctx.lineTo(size.width, ground);
  ctx.stroke();
}

