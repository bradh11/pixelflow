import { type PointerEvent as ReactPointerEvent, type Ref, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import type { Background, PreviewProp, PreviewSet, Show } from "../../api/types";
import { frontView } from "../../lib/geometry";
import {
  type Box,
  type Frame,
  type Gesture,
  type Pt,
  type ResizeHandle,
  type Size,
  type View,
  CORNERS,
  DRAWN_BY_ENDS,
  backgroundBox,
  boxFrom,
  boxOfPoints,
  composeGestures,
  constrainAngle,
  drawnProp,
  fitView,
  frameAngle,
  frameCenter,
  frameOfPoints,
  handleAt,
  handleCursor,
  handlePositions,
  hitTest,
  inBox,
  inFrame,
  isNoop,
  moveBackground,
  moveGesture,
  panBy,
  pinchFactor,
  placedProp,
  propAngles,
  propsInBox,
  resizeBackground,
  rotateGesture,
  scaleGesture,
  snapPoint,
  toScreen,
  toWorld,
  unionBox,
  visibleHandles,
  wheelIntent,
  wheelZoomFactor,
  zoomAt,
  resizeView,
} from "../../lib/layoutMath";
import { batchPixels, drawBatches } from "../../lib/pixelBatches";
import { type PropKind, newProp, nodeCount } from "../../lib/shows";
import {
  type GuideIndex,
  type Marks,
  guideIndex,
  guideThreshold,
  guidesActive,
  nearbyBoxes,
  snapMove,
  snapPointTo,
  snapResize,
} from "../../lib/smartGuides";
import { highlightPixels } from "../../lib/submodels";
import { useLayoutEditor } from "../../state/layoutEditor";
import { commitGesture, settlePending, unsettled } from "../../state/layoutGestures";
import { useApp } from "../../state/store";
import { drawGuideMarks } from "./guideMarks";
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

type Drag =
  /** `from`/`clear`: pressed on empty space; a click there (no drag) clears the selection. */
  | { kind: "pan"; last: Pt; from: Pt; clear: boolean }
  /**
   * `narrowTo`: pressed on one prop of several selected; a click (no drag) selects just it.
   * `deselect`: Shift-pressed on a selected prop; a click takes it out of the selection.
   */
  | {
      kind: "move";
      ids: string[];
      from: Pt;
      fromScreen: Pt;
      origin: Pt | null;
      gesture: Gesture;
      narrowTo: string | null;
      deselect: string | null;
    }
  | { kind: "scale"; ids: string[]; frame: Frame; handle: ResizeHandle; from: Pt; gesture: Gesture; stretchable: boolean }
  | { kind: "rotate"; ids: string[]; center: Pt; from: Pt; gesture: Gesture }
  | { kind: "marquee"; from: Pt; to: Pt; fromScreen: Pt; toScreen: Pt; additive: string[] }
  | { kind: "draw"; tool: PropKind; from: Pt; to: Pt; fromScreen: Pt; toScreen: Pt }
  | { kind: "photo"; corner: ResizeHandle | null; from: Pt; start: Background };

/** The selection's outline along the props' own axes, and whether it can be stretched. */
interface Selection {
  frame: Frame;
  stretchable: boolean;
}

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

/** WebKit's pinch events (Safari and the macOS app); other browsers send Ctrl-scrolls instead. */
type PinchEvent = Event & { scale: number; clientX: number; clientY: number };

/** Says what's selected, for screen readers. */
export function SelectionAnnouncer({ show }: { show: Show }) {
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
  /** Where the pointer last was on the canvas (screen pixels), so Shift can take effect mid-drag. */
  const lastPointer = useRef<Pt | null>(null);
  /** Smart guides for the drag in progress: the other props' boxes, the dragged box as it started, and what to draw. */
  const guides = useRef<{ index: GuideIndex; start: Box | null; marks: Marks | null } | null>(null);
  /** Modifier keys as last seen, so pressing or letting go of one mid-drag takes effect at once. */
  const held = useRef({ shift: false, alt: false });
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

  /** The selected props' frame: along their own axes when they're all turned alike. */
  const selection = (props: PreviewProp[]): Selection | null => {
    const selected = new Set(useLayoutEditor.getState().selected);
    const { deg, stretchable } = frameAngle(latest.current.show.props.filter((p) => selected.has(p.id)));
    const frame = frameOfPoints(
      props.filter((p) => selected.has(p.prop)).map((p) => p.points),
      deg,
    );
    return frame && { frame, stretchable };
  };

  const hitProp = (props: PreviewProp[], w: Pt, v: View) => hitTest(props, w, HIT_PX / v.zoom, propAngles(latest.current.show.props));

  /** Sets up smart guides for a drag starting now: every prop on screen except `moving` (the nearest, if many) guides `start`. */
  const startGuides = (props: PreviewProp[], moving: string[], start: Box | null) => {
    const [v, s, skip] = [currentView(), size(), new Set(moving)];
    const view = boxFrom(toWorld(v, s, { x: 0, y: 0 }), toWorld(v, s, { x: s.width, y: s.height }));
    const boxes = props.flatMap((p) => (skip.has(p.prop) ? [] : (boxOfPoints(p.points) ?? [])));
    const near = start ? { x: (start.minX + start.maxX) / 2, y: (start.minY + start.maxY) / 2 } : { x: v.cx, y: v.cy };
    guides.current = { index: guideIndex(nearbyBoxes(boxes, view, near)), start, marks: null };
  };

  /** The smart guides to snap to now: none while they're off or Alt is held. */
  const activeGuides = () => {
    const g = guides.current;
    return g && guidesActive(useLayoutEditor.getState().smartGuides, { altKey: held.current.alt }) ? g : null;
  };

  /** A point being drawn, on a smart guide if one is near, or else at `fallback` (the grid's point). */
  const guidedPoint = (w: Pt, fallback: Pt): Pt => {
    const g = activeGuides();
    if (!g) return fallback;
    const r = snapPointTo(g.index, w, { threshold: guideThreshold(currentView().zoom), fallback });
    g.marks = r.marks;
    return r.point;
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
        for (const h of CORNERS) drawHandle(ctx, handles[h]);
      }
    }

    if (editor.snap) drawGrid(ctx, view, s, editor.grid);

    const props = effectivePreview();
    const selected = new Set(editor.selected);
    const radius = Math.min(4.5, Math.max(1.3, view.zoom * 0.05));
    drawBatches(ctx, batchPixels(props, frame.current, view, s, selected, PIXEL_COLORS, radius), radius, ratio);

    // A submodel or face picked in the properties panel, drawn over its prop a little bigger.
    const hl = editor.highlight;
    const hlProp = hl && latest.current.show.props.find((p) => p.id === hl.prop);
    const hlRegion = hl && hlProp?.regions.find((r) => r.id === hl.region);
    const hlPoints = hl && props.find((p) => p.prop === hl.prop)?.points;
    if (hl && hlProp && hlRegion && hlPoints) {
      const { points, rgb } = highlightPixels(hlRegion, nodeCount(hlProp.shape), hlPoints, hl.phoneme);
      const big = radius * 1.4;
      drawBatches(ctx, batchPixels([{ prop: hl.region, frameOffset: 0, channelsPerPixel: 3, points }], rgb, view, s, new Set(), PIXEL_COLORS, big), big, ratio);
    }

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
      const sel = selection(props);
      if (sel) {
        const handles = handlePositions(sel.frame, view, s);
        strokeWithHalo(ctx, 1, [5, 4], () => {
          ctx.beginPath();
          for (const h of ["nw", "ne", "se", "sw"] as const) ctx.lineTo(handles[h].x, handles[h].y);
          ctx.closePath();
          ctx.stroke();
        });
        strokeWithHalo(ctx, 1, [], () => {
          ctx.beginPath();
          ctx.moveTo(handles.n.x, handles.n.y);
          ctx.lineTo(handles.rotate.x, handles.rotate.y);
          ctx.stroke();
        });
        for (const h of visibleHandles(sel.frame, view, sel.stretchable)) if (h !== "rotate") drawHandle(ctx, handles[h]);
        ctx.fillStyle = "#fff";
        ctx.strokeStyle = ACCENT;
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        ctx.arc(handles.rotate.x, handles.rotate.y, 5, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
      }
    }

    const marks = d && guides.current?.marks;
    if (marks) drawGuideMarks(ctx, marks, at, { accent: ACCENT, halo: HALO, ink: BACKDROP });

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
    // When the window (and so the canvas) changes size, scale the drawing with it: the same part
    // of the layout stays in view instead of being cropped or left in a corner.
    let last = size();
    const observer = new ResizeObserver(() => {
      const now = size();
      const st = useLayoutEditor.getState();
      if (st.view) {
        const next = resizeView(st.view, last, now);
        if (next !== st.view) st.setView(next);
      }
      if (now.width > 0 && now.height > 0) last = now;
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
      if (e.key === "Shift") return shiftChanged(true);
      if (e.key === "Alt") return altChanged(true);
      // ⌘-Space and the like belong to the system, which may keep the key's release to itself.
      if (e.key === " " && quietTarget(e.target) && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        if (!spaceHeld.current) {
          spaceHeld.current = true;
          setCursor("grab");
        }
      }
    };
    const releaseSpace = () => {
      if (!spaceHeld.current) return;
      spaceHeld.current = false;
      setCursor("default");
    };
    const up = (e: KeyboardEvent) => {
      if (e.key === "Shift") shiftChanged(false);
      if (e.key === "Alt") altChanged(false);
      if (e.key === " ") releaseSpace();
    };
    // A key let go while the window is in the background never says so: forget Space then.
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", releaseSpace);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", releaseSpace);
    };
  }, []);

  const cancel = () => {
    const d = drag.current;
    if (!d) return false;
    drag.current = null;
    guides.current = null;
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
    if (e.button !== 0 && e.button !== 1 && e.button !== 2) return;
    const canvas = canvasRef.current!;
    // Keyboard shortcuts (Delete, ⌘C, arrows) work on the canvas right after a click.
    canvas.focus({ preventScroll: true });
    try {
      canvas.setPointerCapture?.(e.pointerId);
    } catch {
      // A pointer the browser no longer tracks can't be captured; the drag still works inside the canvas.
    }
    const s = point(e);
    lastPointer.current = s;
    held.current = { shift: e.shiftKey, alt: e.altKey };
    guides.current = null;
    const v = currentView();
    const w = toWorld(v, size(), s);
    const st = useLayoutEditor.getState();
    const { show, photo } = latest.current;

    if (e.button !== 0 || spaceHeld.current) {
      drag.current = { kind: "pan", last: s, from: s, clear: false };
      setCursor("grabbing");
      return;
    }
    if (st.editPhoto) {
      const bg = background();
      const box = bg ? backgroundBox(bg, photo.aspect) : null;
      const handle = box ? handleAt(box, v, size(), s, CORNERS) : null;
      if (bg && handle && handle !== "rotate") drag.current = { kind: "photo", corner: handle, from: w, start: bg };
      else if (bg && box && inBox(box, w)) drag.current = { kind: "photo", corner: null, from: w, start: bg };
      else drag.current = { kind: "pan", last: s, from: s, clear: false };
      return;
    }
    if (st.tool !== "select") {
      startGuides(effectivePreview(), [], null);
      const p = guidedPoint(w, st.snap ? snapPoint(w, st.grid) : w);
      drag.current = { kind: "draw", tool: st.tool, from: p, to: p, fromScreen: s, toScreen: s };
      redraw();
      return;
    }

    const props = effectivePreview();
    const sel = selection(props);
    // Handles work with Shift held from the start (Shift keeps proportions, or turns in 15° steps).
    if (sel) {
      const handle = handleAt(sel.frame, v, size(), s, visibleHandles(sel.frame, v, sel.stretchable));
      const ids = st.selected;
      if (handle === "rotate") {
        const center = frameCenter(sel.frame);
        drag.current = { kind: "rotate", ids, center, from: w, gesture: { kind: "rotate", cx: center.x, cy: center.y, deg: 0 } };
        return;
      }
      if (handle) {
        const gesture: Gesture = { kind: "scale", ax: 0, ay: 0, fx: 1, fy: 1 };
        drag.current = { kind: "scale", ids, frame: sel.frame, handle, from: w, gesture, stretchable: sel.stretchable };
        startGuides(props, ids, sel.frame.box);
        return;
      }
    }
    const startMove = (ids: string[], narrowTo: string | null = null, deselect: string | null = null) => {
      const first = show.props.find((p) => p.id === ids[0]);
      const origin = first ? { x: first.transform.position.x, y: first.transform.position.y } : null;
      const gesture: Gesture = { kind: "move", dx: 0, dy: 0 };
      drag.current = { kind: "move", ids, from: w, fromScreen: s, origin, gesture, narrowTo, deselect };
      const moving = new Set(ids);
      startGuides(props, ids, frameOfPoints(props.filter((p) => moving.has(p.prop)).map((p) => p.points), 0)?.box ?? null);
    };
    const hit = hitProp(props, w, v);
    if (hit) {
      // Shift adds the prop at once, so Shift-dragging it moves it straight with the rest; a
      // Shift-click (no drag) on a selected prop takes it out.
      if (e.shiftKey) {
        if (st.selected.includes(hit)) startMove(st.selected, null, hit);
        else {
          st.toggle(hit);
          startMove(useLayoutEditor.getState().selected);
        }
        return;
      }
      if (!st.selected.includes(hit)) {
        st.select([hit]);
        startMove([hit]);
      } else startMove(st.selected, st.selected.length > 1 ? hit : null);
      return;
    }
    if (sel && !e.shiftKey && inFrame(sel.frame, w)) {
      startMove(st.selected);
      return;
    }
    // Empty space: drag to move the view (a click clears the selection), Shift-drag to box-select.
    if (e.shiftKey) drag.current = { kind: "marquee", from: w, to: w, fromScreen: s, toScreen: s, additive: st.selected };
    else {
      drag.current = { kind: "pan", last: s, from: s, clear: true };
      setCursor("grabbing");
    }
  };

  const updateHover = (s: Pt) => {
    const st = useLayoutEditor.getState();
    const v = currentView();
    const w = toWorld(v, size(), s);
    if (spaceHeld.current) return setCursor("grab");
    if (st.editPhoto) {
      const bg = background();
      const box = bg ? backgroundBox(bg, latest.current.photo.aspect) : null;
      const handle = box ? handleAt(box, v, size(), s, CORNERS) : null;
      return setCursor(handle ? handleCursor(handle, 0) : box && inBox(box, w) ? "move" : "grab");
    }
    if (st.tool !== "select") return setCursor("crosshair");
    const props = effectivePreview();
    const sel = selection(props);
    const handle = sel ? handleAt(sel.frame, v, size(), s, visibleHandles(sel.frame, v, sel.stretchable)) : null;
    const hit = hitProp(props, w, v);
    setHovered(hit);
    setCursor(handle && sel ? handleCursor(handle, sel.frame.deg) : hit || (sel && inFrame(sel.frame, w)) ? "move" : "grab");
  };

  /** Follows the pointer at screen point `s`; `straight` (Shift) keeps lines and moves straight, and resizes in proportion. */
  const follow = (d: Drag, s: Pt, straight: boolean) => {
    const v = currentView();
    const w = toWorld(v, size(), s);
    const st = useLayoutEditor.getState();
    if (guides.current) guides.current.marks = null;
    const g = activeGuides();
    const threshold = guideThreshold(v.zoom);
    switch (d.kind) {
      case "pan":
        st.setView(panBy(v, s.x - d.last.x, s.y - d.last.y));
        d.last = s;
        break;
      case "move":
        // Until the pointer has really moved, it's still a click.
        if (Math.hypot(s.x - d.fromScreen.x, s.y - d.fromScreen.y) < CLICK_PX && isNoop(d.gesture)) break;
        d.gesture = moveGesture(d.from, w, d.origin, st.snap ? st.grid : null, straight);
        const raw = moveGesture(d.from, w, d.origin, null, straight);
        if (g?.start && raw.kind === "move" && d.gesture.kind === "move") {
          // Smart guides win where one is near; the grid applies elsewhere. Shift's held axis stays put.
          const sideways = Math.abs(w.x - d.from.x) >= Math.abs(w.y - d.from.y);
          const lock = straight ? { x: !sideways, y: sideways } : undefined;
          const r = snapMove(g.index, g.start, raw, { threshold, fallback: d.gesture, lock });
          d.gesture = { kind: "move", dx: r.dx, dy: r.dy };
          g.marks = r.marks;
        }
        break;
      case "scale":
        d.gesture = scaleGesture(d.frame, d.handle, d.from, w, straight || !d.stretchable);
        if (g?.start) {
          const r = snapResize(g.index, g.start, d.handle, d.gesture, { threshold, keepAspect: straight || !d.stretchable });
          d.gesture = r.gesture;
          g.marks = r.marks;
        }
        break;
      case "rotate":
        d.gesture = rotateGesture(d.center, d.from, w, straight);
        break;
      case "marquee":
        d.to = w;
        d.toScreen = s;
        break;
      case "draw": {
        const to = st.snap ? snapPoint(w, st.grid) : w;
        d.to = straight && DRAWN_BY_ENDS.includes(d.tool) ? constrainAngle(d.from, to) : guidedPoint(w, to);
        d.toScreen = s;
        break;
      }
      case "photo": {
        const aspect = latest.current.photo.aspect;
        st.setPhotoDraft(d.corner ? resizeBackground(d.start, aspect, d.corner, w) : moveBackground(d.start, w.x - d.from.x, w.y - d.from.y));
        break;
      }
    }
    redraw();
  };

  /** Shift pressed or let go mid-drag takes effect at once, without waiting for the pointer to move. */
  function shiftChanged(on: boolean) {
    held.current.shift = on;
    const d = drag.current;
    if (d && d.kind !== "pan" && lastPointer.current) follow(d, lastPointer.current, on);
  }

  /** Alt (Option) pressed or let go mid-drag turns smart guides off or back on at once. */
  function altChanged(on: boolean) {
    held.current.alt = on;
    const d = drag.current;
    if (d && d.kind !== "pan" && lastPointer.current) follow(d, lastPointer.current, held.current.shift);
  }

  const onPointerMove = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    const s = point(e);
    lastPointer.current = s;
    held.current = { shift: e.shiftKey, alt: e.altKey };
    const d = drag.current;
    if (!d) return updateHover(s);
    follow(d, s, e.shiftKey);
  };

  const onPointerUp = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    canvasRef.current?.releasePointerCapture?.(e.pointerId);
    const d = drag.current;
    drag.current = null;
    guides.current = null;
    if (!d) return;
    const st = useLayoutEditor.getState();
    const { apply } = useApp.getState();
    switch (d.kind) {
      case "pan":
        if (d.clear && Math.hypot(d.last.x - d.from.x, d.last.y - d.from.y) < CLICK_PX) st.clear();
        setCursor("grab");
        break;
      case "move":
        if (isNoop(d.gesture)) {
          if (d.narrowTo) st.select([d.narrowTo]);
          if (d.deselect) st.toggle(d.deselect);
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
        onContextMenu={(e) => e.preventDefault()}
      />
      <p id="layout-canvas-help" className="sr-only">
        Click a prop to select it, or shift-click to select more. Shift-drag across empty space to select everything inside,
        or press Command-A to select every prop. Drag selected props to move them (hold Shift to keep straight across or up and
        down). Drag a corner handle to resize them (hold Shift to keep their proportions), a side handle to stretch them one
        way, or the round handle above them to turn them. Arrow keys move the selection (hold Shift to move it further),
        Command-C copies it, Command-X cuts it, Command-V pastes, Command-D duplicates it, Delete removes it, and Escape
        clears it. To draw a new prop, pick Line, Arch, Matrix, Tree, Circle, or Star in the tool bar and drag here; hold Shift
        to keep a line or arch level, upright, or at 45 degrees. Drag empty space, or scroll with two fingers, to move
        around; pinch, or hold Command and scroll, to zoom. Every prop is also in the props list below.
        With Smart guides on, props snap to line up with, space evenly from, and match the size of others as you move,
        resize, and draw them; hold Option (Alt) to place them freely.
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
