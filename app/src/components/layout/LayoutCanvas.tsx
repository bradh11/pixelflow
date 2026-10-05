import { type PointerEvent as ReactPointerEvent, type Ref, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import type { Background, PreviewProp, PreviewSet, Show } from "../../api/types";
import { frontView } from "../../lib/geometry";
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
  canStretchFreely,
  composeGestures,
  drawnProp,
  fitView,
  handleAt,
  handlePositions,
  hitTest,
  inBox,
  isNoop,
  moveBackground,
  moveGesture,
  panBy,
  pinchFactor,
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
import { batchPixels, drawBatches } from "../../lib/pixelBatches";
import { type PropKind, newProp } from "../../lib/shows";
import { useLayoutEditor } from "../../state/layoutEditor";
import { commitGesture, settlePending, unsettled } from "../../state/layoutGestures";
import { useApp } from "../../state/store";
import { type PhotoImage, useLiveFrame } from "./useLayoutData";

/** How close (screen pixels) a click must be to a pixel to pick its prop. */
const HIT_PX = 8;
/** Drags shorter than this (screen pixels) count as clicks. */
const CLICK_PX = 4;

// The canvas stays dark in both themes on purpose: lights are judged against a night sky.
// Selection marks get a dark outline underneath, so they read over a bright photo as well.
const BACKDROP = "#0a0a0c";
const ACCENT = "#a78bfa";
const HALO = "rgba(0, 0, 0, 0.65)";
const GRID = "rgba(160, 160, 160, 0.22)";
const GROUND = "rgba(200, 200, 200, 0.4)";
const PIXEL_COLORS = { unlit: "rgba(220, 220, 220, 0.7)", selected: ACCENT, dark: "rgba(90, 90, 90, 0.6)" };

type Corner = Exclude<Handle, "rotate">;

type Drag =
  | { kind: "pan"; last: Pt }
  /** `narrowTo`: pressed on one prop of several selected; a click (no drag) selects just it. */
  | { kind: "move"; ids: string[]; from: Pt; fromScreen: Pt; origin: Pt | null; gesture: Gesture; narrowTo: string | null }
  | { kind: "scale"; ids: string[]; box: Box; handle: Corner; from: Pt; gesture: Gesture; stretchable: boolean }
  | { kind: "rotate"; ids: string[]; center: Pt; from: Pt; gesture: Gesture }
  | { kind: "marquee"; from: Pt; to: Pt; fromScreen: Pt; toScreen: Pt; additive: string[] }
  | { kind: "draw"; tool: PropKind; from: Pt; to: Pt; fromScreen: Pt; toScreen: Pt }
  | { kind: "photo"; corner: Corner | null; from: Pt; start: Background };

export interface LayoutCanvasHandle {
  /** Stops a drag in progress, leaving everything as it was. True if there was one. */
  cancel(): boolean;
}

interface LayoutCanvasProps {
  preview: PreviewSet;
  show: Show;
  photo: PhotoImage;
  ref?: Ref<LayoutCanvasHandle>;
}

const CURSORS: Record<Handle, string> = {
  nw: "nwse-resize",
  se: "nwse-resize",
  ne: "nesw-resize",
  sw: "nesw-resize",
  rotate: "grab",
};

/** WebKit's pinch events (Safari and the macOS app); other browsers send Ctrl-scrolls instead. */
type PinchEvent = Event & { scale: number; clientX: number; clientY: number };

/** Says what's selected, for screen readers. */
function SelectionAnnouncer({ show }: { show: Show }) {
  const selected = useLayoutEditor((s) => s.selected);
  const names = selected.map((id) => show.props.find((p) => p.id === id)?.name).filter(Boolean);
  const message = names.length === 0 ? "Nothing selected" : names.length === 1 ? `${names[0]} selected` : `${names.length} props selected`;
  return (
    <p className="sr-only" aria-live="polite" data-testid="selection-announcer">
      {message}
    </p>
  );
}

/**
 * The layout drawn over the background photo, where props are selected, moved, turned,
 * resized, and drawn. Every finished drag is sent as one batch of edits (one undo step); while
 * dragging, the props are only redrawn here. Panning, zooming, selecting, and live colors only
 * redraw the canvas; they never re-render React.
 */
export function LayoutCanvas({ preview, show, photo, ref }: LayoutCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const drag = useRef<Drag | null>(null);
  const frame = useRef<Uint8Array | null>(null);
  const spaceHeld = useRef(false);
  const frameRequest = useRef<number | null>(null);
  const [cursor, setCursor] = useState("default");
  const [hovered, setHovered] = useState<string | null>(null);

  // Everything the drawing and pointer handlers need, current as of the last render.
  const latest = useRef({ preview, show, photo });
  latest.current = { preview, show, photo };

  const size = (): Size => {
    const canvas = canvasRef.current;
    return { width: canvas?.clientWidth ?? 0, height: canvas?.clientHeight ?? 0 };
  };

  const background = (): Background | null => useLayoutEditor.getState().photoDraft ?? latest.current.show.background ?? null;

  /** Everything worth showing: the props and the photo. */
  const contentBox = (): Box | null => {
    const { preview, photo } = latest.current;
    const bg = background();
    return unionBox([...preview.props.map((p) => boxOfPoints(p.points)), bg ? backgroundBox(bg, photo.aspect) : null]);
  };

  const currentView = (): View => useLayoutEditor.getState().view ?? fitView(contentBox(), size());

  /** Fits everything in when nothing has set the view yet, once there's a size and something to show. */
  const fitIfNeeded = () => {
    if (useLayoutEditor.getState().view) return;
    const s = size();
    const { show, preview } = latest.current;
    const ready = show.props.length === 0 || preview.props.length > 0;
    if (s.width > 0 && ready) useLayoutEditor.getState().setView(fitView(contentBox(), s));
  };

  /** The props as they should look now: with gestures still on their way, held arrow keys, and any drag. */
  const effectivePreview = (): PreviewProp[] => {
    const { preview } = latest.current;
    const st = useLayoutEditor.getState();
    const layers: { ids: string[]; gesture: Gesture }[] = unsettled(st.pending, preview.revision);
    if (st.nudge) layers.push({ ids: st.nudge.ids, gesture: { kind: "move", dx: st.nudge.dx, dy: st.nudge.dy } });
    const d = drag.current;
    if (d && (d.kind === "move" || d.kind === "scale" || d.kind === "rotate")) layers.push(d);
    return composeGestures(preview.props, layers);
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
    const { photo } = latest.current;
    const editor = useLayoutEditor.getState();
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
        const rect = () => ctx.strokeRect(tl.x, tl.y, br.x - tl.x, br.y - tl.y);
        if (editor.editPhoto) strokeWithHalo(ctx, 2, [], rect);
        else {
          ctx.setLineDash([6, 4]);
          ctx.strokeStyle = "rgba(255,255,255,0.3)";
          ctx.lineWidth = 1;
          rect();
          ctx.setLineDash([]);
        }
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
    drawBatches(ctx, batchPixels(props, frame.current, view, s, selected, PIXEL_COLORS, radius), radius, ratio);

    const d = drag.current;
    if (d?.kind === "draw") {
      const draft = draftProp(d);
      if (draft) {
        const pts = frontView(draft);
        const outline = batchPixels([{ prop: draft.id, frameOffset: 0, channelsPerPixel: 3, points: pts }], null, view, s, new Set([draft.id]), PIXEL_COLORS, radius);
        drawBatches(ctx, outline, radius, ratio);
      }
      const [a, b] = [at(d.from), at(d.to)];
      strokeWithHalo(ctx, 1, [5, 4], () => {
        if (DRAWN_BY_ENDS.includes(d.tool)) {
          ctx.beginPath();
          ctx.moveTo(a.x, a.y);
          ctx.lineTo(b.x, b.y);
          ctx.stroke();
        } else {
          ctx.strokeRect(Math.min(a.x, b.x), Math.min(a.y, b.y), Math.abs(b.x - a.x), Math.abs(b.y - a.y));
        }
      });
    }

    if (!editor.editPhoto && selected.size > 0) {
      const box = selectionBox(props);
      if (box) {
        const handles = handlePositions(box, view, s);
        strokeWithHalo(ctx, 1, [5, 4], () =>
          ctx.strokeRect(handles.nw.x, handles.nw.y, handles.se.x - handles.nw.x, handles.se.y - handles.nw.y),
        );
        strokeWithHalo(ctx, 1, [], () => {
          ctx.beginPath();
          ctx.moveTo(handles.rotate.x, handles.nw.y);
          ctx.lineTo(handles.rotate.x, handles.rotate.y);
          ctx.stroke();
        });
        for (const h of ["nw", "ne", "sw", "se"] as Corner[]) drawHandle(ctx, handles[h]);
        ctx.fillStyle = "#fff";
        ctx.strokeStyle = ACCENT;
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        ctx.arc(handles.rotate.x, handles.rotate.y, 5, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
      }
    }

    if (d?.kind === "marquee") {
      const [a, b] = [d.fromScreen, d.toScreen];
      const [x, y, w, h] = [Math.min(a.x, b.x), Math.min(a.y, b.y), Math.abs(b.x - a.x), Math.abs(b.y - a.y)];
      ctx.fillStyle = "rgba(167, 139, 250, 0.12)";
      ctx.fillRect(x, y, w, h);
      strokeWithHalo(ctx, 1, [], () => ctx.strokeRect(x, y, w, h));
    }
    // Every helper used here reads `latest` or the stores, so the function never needs to change.
  }, []);

  /** Draws on the next animation frame; any number of calls before then draw once. */
  const redraw = useCallback(() => {
    if (frameRequest.current !== null) return;
    if (typeof requestAnimationFrame === "function") frameRequest.current = requestAnimationFrame(draw);
    else draw();
  }, [draw]);

  useEffect(
    () => () => {
      if (frameRequest.current !== null && typeof cancelAnimationFrame === "function") cancelAnimationFrame(frameRequest.current);
      frameRequest.current = null;
    },
    [],
  );

  // Redraw after every render: new positions or photo.
  useEffect(redraw);

  // Live colors go straight to the canvas.
  useLiveFrame(
    useCallback(
      (f: Uint8Array | null) => {
        if (f === null && frame.current === null) return;
        frame.current = f;
        redraw();
      },
      [redraw],
    ),
  );

  // The editor's state (view, selection, tool, gestures on their way) only needs a redraw.
  useEffect(
    () =>
      useLayoutEditor.subscribe(() => {
        fitIfNeeded();
        redraw();
      }),
    [redraw],
  );

  // Gestures the engine's new positions include no longer need drawing on top.
  useEffect(() => settlePending(preview.revision), [preview]);

  // Fit everything in once the canvas has a size and the props have arrived.
  useEffect(fitIfNeeded, [preview, show.props.length, photo.aspect]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const observer = new ResizeObserver(() => {
      fitIfNeeded();
      redraw();
    });
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [redraw]);

  const point = (e: { clientX: number; clientY: number }): Pt => {
    const rect = canvasRef.current!.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  };

  // Wheel and pinch: zoom about the cursor, or pan (a passive React listener can't stop page
  // scrolling). See `wheelIntent` for which scrolls zoom.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let pinching = false;
    let lastScale = 1;
    const setView = (v: View) => useLayoutEditor.getState().setView(v);
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      // While a WebKit pinch is under way its gesture events do the zooming.
      if (pinching && e.ctrlKey) return;
      const v = currentView();
      if (wheelIntent(e) === "zoom") setView(zoomAt(v, size(), point(e), wheelZoomFactor(e)));
      else setView(panBy(v, -e.deltaX, -e.deltaY));
    };
    const onPinchStart = (e: Event) => {
      e.preventDefault();
      pinching = true;
      lastScale = 1;
    };
    const onPinch = (e: Event) => {
      e.preventDefault();
      const { scale, clientX, clientY } = e as PinchEvent;
      const factor = pinchFactor(lastScale, scale);
      if (scale > 0) lastScale = scale;
      const s = size();
      const at = Number.isFinite(clientX) && Number.isFinite(clientY) ? point({ clientX, clientY }) : { x: s.width / 2, y: s.height / 2 };
      setView(zoomAt(currentView(), s, at, factor));
    };
    const onPinchEnd = (e: Event) => {
      e.preventDefault();
      pinching = false;
    };
    canvas.addEventListener("wheel", onWheel, { passive: false });
    canvas.addEventListener("gesturestart", onPinchStart);
    canvas.addEventListener("gesturechange", onPinch);
    canvas.addEventListener("gestureend", onPinchEnd);
    return () => {
      canvas.removeEventListener("wheel", onWheel);
      canvas.removeEventListener("gesturestart", onPinchStart);
      canvas.removeEventListener("gesturechange", onPinch);
      canvas.removeEventListener("gestureend", onPinchEnd);
    };
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
        const stretchable = canStretchFreely(show.props.filter((p) => ids.includes(p.id)));
        drag.current = { kind: "scale", ids, box, handle, from: w, gesture: { kind: "scale", ax: 0, ay: 0, fx: 1, fy: 1 }, stretchable };
        return;
      }
    }
    const startMove = (ids: string[], narrowTo: string | null = null) => {
      const first = show.props.find((p) => p.id === ids[0]);
      const origin = first ? { x: first.transform.position.x, y: first.transform.position.y } : null;
      drag.current = { kind: "move", ids, from: w, fromScreen: s, origin, gesture: { kind: "move", dx: 0, dy: 0 }, narrowTo };
    };
    const hit = hitTest(props, w, HIT_PX / v.zoom);
    if (hit) {
      if (e.shiftKey) {
        st.toggle(hit);
        return;
      }
      if (!st.selected.includes(hit)) {
        st.select([hit]);
        startMove([hit]);
      } else startMove(st.selected, st.selected.length > 1 ? hit : null);
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
        // Until the pointer has really moved, it's still a click.
        if (Math.hypot(s.x - d.fromScreen.x, s.y - d.fromScreen.y) < CLICK_PX && isNoop(d.gesture)) break;
        d.gesture = moveGesture(d.from, w, d.origin, st.snap ? st.grid : null);
        break;
      case "scale":
        d.gesture = scaleGesture(d.box, d.handle, d.from, w, e.shiftKey && d.stretchable);
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
    const { apply } = useApp.getState();
    switch (d.kind) {
      case "pan":
        setCursor(spaceHeld.current || st.tool === "pan" ? "grab" : "default");
        break;
      case "move":
        if (isNoop(d.gesture)) {
          if (d.narrowTo) st.select([d.narrowTo]);
          break;
        }
        void commitGesture(d.ids, d.gesture);
        break;
      case "scale":
      case "rotate":
        void commitGesture(d.ids, d.gesture);
        break;
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
        // Inside the edge, so the rounded frame around the canvas doesn't cut the focus ring off.
        style={{ cursor: drag.current?.kind === "pan" ? "grabbing" : cursor, outlineOffset: -3 }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={() => cancel()}
        onPointerLeave={() => setHovered(null)}
      />
      <p id="layout-canvas-help" className="sr-only">
        Click a prop to select it, or shift-click to select more. Drag across empty space to select everything inside, or
        press Command-A to select every prop. Drag selected props to move them, drag a corner handle to resize them, or drag
        the round handle above them to turn them. Arrow keys move the selection (hold Shift to move it further), Command-D
        duplicates it, Delete removes it, and Escape clears it. To draw a new prop, pick Line, Arch, Matrix, Tree, Circle, or
        Star in the tool bar and drag here. Hold Space and drag, or scroll with two fingers, to move around; pinch, or hold
        Command and scroll, to zoom. Every prop is also in the props list below.
      </p>
      <SelectionAnnouncer show={show} />
      {hoveredName && (
        <div className="pointer-events-none absolute bottom-2 left-2 rounded bg-black/70 px-2 py-0.5 text-xs text-white">
          {hoveredName}
        </div>
      )}
    </div>
  );
}

/** Strokes whatever `path` draws in the accent color over a dark outline, readable on any background. */
function strokeWithHalo(ctx: CanvasRenderingContext2D, width: number, dash: number[], path: () => void) {
  ctx.setLineDash(dash);
  ctx.strokeStyle = HALO;
  ctx.lineWidth = width + 2;
  path();
  ctx.strokeStyle = ACCENT;
  ctx.lineWidth = width;
  path();
  ctx.setLineDash([]);
}

function drawHandle(ctx: CanvasRenderingContext2D, p: Pt) {
  ctx.fillStyle = "#fff";
  ctx.strokeStyle = HALO;
  ctx.lineWidth = 3;
  ctx.strokeRect(p.x - 4, p.y - 4, 8, 8);
  ctx.fillRect(p.x - 4, p.y - 4, 8, 8);
  ctx.strokeStyle = ACCENT;
  ctx.lineWidth = 1.5;
  ctx.strokeRect(p.x - 4, p.y - 4, 8, 8);
}

/** Grid lines every `grid` units, thinned out so they're never closer than 8 pixels. */
function drawGrid(ctx: CanvasRenderingContext2D, view: View, size: Size, grid: number) {
  let step = grid;
  while (step * view.zoom < 8) step *= 2;
  const tl = toWorld(view, size, { x: 0, y: 0 });
  const br = toWorld(view, size, { x: size.width, y: size.height });
  ctx.strokeStyle = GRID;
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
  ctx.strokeStyle = GROUND;
  ctx.beginPath();
  ctx.moveTo(0, ground);
  ctx.lineTo(size.width, ground);
  ctx.stroke();
}
