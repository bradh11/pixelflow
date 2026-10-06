import { type PointerEvent as ReactPointerEvent, type Ref, useCallback, useEffect, useImperativeHandle, useRef, useState } from "react";
import type { Background, PreviewProp, PreviewSet, Prop, Show } from "../../api/types";
import { applyTransform, frontView } from "../../lib/geometry";
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
import { updateEdits } from "../../lib/layoutEdits";
import {
  type LineEnd,
  type PolyDraft,
  type PolyShape,
  addPoint,
  bendSegment,
  editablePoly,
  finishDraft,
  insertVertex,
  isPoly,
  lineEnds,
  localAt,
  moveControl,
  moveVertex,
  placePoint,
  polyHandles,
  removeLastPoint,
  removeVertex,
  straighten,
} from "../../lib/polylineMath";
import { type PropKind, newProp, nodeCount } from "../../lib/shows";
import { highlightPixels } from "../../lib/submodels";
import { useLayoutEditor } from "../../state/layoutEditor";
import { commitGesture, settlePending, unsettled } from "../../state/layoutGestures";
import { useApp } from "../../state/store";
import { type PhotoImage, useLiveFrame } from "./useLayoutData";

/** How close (screen pixels) a click must be to a pixel to pick its prop. */
const HIT_PX = 8;
/** Drags shorter than this (screen pixels) count as clicks. */
const CLICK_PX = 4;
/** How close (screen pixels) a poly line point must come to a line's end to join it. */
const JOIN_PX = 10;
/** How close (screen pixels) a click must be to a poly line handle to grab it. */
const POLY_HANDLE_PX = 7;

// The canvas stays dark in both themes on purpose: lights are judged against a night sky.
// Selection marks get a dark outline underneath, so they read over a bright photo as well.
const BACKDROP = "#0a0a0c";
const ACCENT = "#a78bfa";
const HALO = "rgba(0, 0, 0, 0.65)";
const GRID = "rgba(160, 160, 160, 0.22)";
const GROUND = "rgba(200, 200, 200, 0.4)";
/** The ring showing a point will join another line's end. */
const JOIN_COLOR = "#4ade80";
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
  | { kind: "photo"; corner: ResizeHandle | null; from: Pt; start: Background }
  /**
   * A selected poly line's handle: a point, the middle of a stretch (a click adds a point there,
   * a drag bends the stretch), or a curve control. `draft` is the shape as dragged so far.
   */
  | {
      kind: "poly";
      prop: string;
      handle: PolyHit;
      fromScreen: Pt;
      moved: boolean;
      alt: boolean;
      shape: PolyShape;
      draft: PolyShape;
      join: LineEnd | null;
    };

type PolyHit = { kind: "vertex"; index: number } | { kind: "middle"; segment: number } | { kind: "control"; segment: number; which: 0 | 1 };

/** Where the Poly Line tool's next point would go, and the line end it would join. */
interface PolyNext {
  at: Pt;
  join: LineEnd | null;
}

/** The selection's outline along the props' own axes, and whether it can be stretched. */
interface Selection {
  frame: Frame;
  stretchable: boolean;
}

export interface LayoutCanvasHandle {
  /** Stops a drag in progress, leaving everything as it was. True if there was one. */
  cancel(): boolean;
  /**
   * A key while a poly line is being drawn: Enter finishes it, Backspace or Delete takes the
   * last point off. True when the key was used (so nothing else should act on it).
   */
  polyKey(key: string): boolean;
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
  const frameRequest = useRef<number | null>(null);
  /** The points of the poly line being drawn with the Poly Line tool, between clicks. */
  const polyDrawing = useRef<PolyDraft | null>(null);
  /** Where the Poly Line tool's next point goes, following the pointer. */
  const polyNext = useRef<PolyNext | null>(null);
  /** Shift held, so the Poly Line tool's next point keeps to 45° as the pointer moves. */
  const shiftHeld = useRef(false);
  /**
   * A poly line's new shape on its way to the engine, drawn until the engine's positions (from
   * `revision`, once known) include it, so the line never jumps back.
   */
  const pendingShape = useRef<{ id: string; points: number[]; revision: number | null } | null>(null);
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
    const moved = composeGestures(preview.props, layers);
    // A poly line being reshaped, or reshaped and on its way, is drawn as it now is.
    const reshaped =
      d?.kind === "poly"
        ? { id: d.prop, points: shapePoints(d.prop, d.draft) }
        : pendingShape.current && (pendingShape.current.revision === null || pendingShape.current.revision > preview.revision)
          ? pendingShape.current
          : null;
    if (!reshaped?.points) return moved;
    return moved.map((p) => (p.prop === reshaped.id ? { ...p, points: reshaped.points! } : p));
  };

  const propById = (id: string) => latest.current.show.props.find((p) => p.id === id);

  /** The prop's pixels (front view) with `shape` instead of its own. */
  const shapePoints = (id: string, shape: PolyShape): number[] | null => {
    const prop = propById(id);
    return prop ? frontView({ ...prop, shape }) : null;
  };

  /** The one selected prop, when it's a poly line whose points can be dragged (Select tool, 2D). */
  const editingPoly = () => {
    const st = useLayoutEditor.getState();
    if (st.tool !== "select" || st.editPhoto || st.selected.length !== 1) return null;
    const prop = propById(st.selected[0]);
    return prop && editablePoly(prop) ? prop : null;
  };

  /** The editing poly line's handle under screen point `s`: points first, then curve controls, then middles. */
  const polyHitAt = (s: Pt): { prop: Prop; hit: PolyHit } | null => {
    const prop = editingPoly();
    const handles = prop && polyHandles(prop);
    if (!prop || !handles) return null;
    const v = currentView();
    const sz = size();
    const near = (w: Pt) => {
      const q = toScreen(v, sz, w);
      return Math.hypot(q.x - s.x, q.y - s.y) <= POLY_HANDLE_PX;
    };
    const vertex = handles.vertices.findIndex(near);
    if (vertex >= 0) return { prop, hit: { kind: "vertex", index: vertex } };
    const control = handles.controls.find((c) => near(c.at));
    if (control) return { prop, hit: { kind: "control", segment: control.segment, which: control.which } };
    const middle = handles.middles.findIndex(near);
    if (middle >= 0) return { prop, hit: { kind: "middle", segment: middle } };
    return null;
  };

  /** Sends a poly line's new shape (one undo step), drawing it until the engine has it. */
  const commitShape = (id: string, shape: PolyShape) => {
    const entry = { id, points: shapePoints(id, shape) ?? [], revision: null as number | null };
    pendingShape.current = entry;
    void useApp
      .getState()
      .edit(updateEdits(id, (p) => ({ ...p, shape })))
      .then((revision) => {
        if (pendingShape.current !== entry) return;
        if (revision === null) pendingShape.current = null;
        else entry.revision = revision;
        redraw();
      });
  };

  /** Where the Poly Line tool puts its next point for the pointer at screen point `s`. */
  const polyPlace = (s: Pt, straight: boolean) => {
    const v = currentView();
    const st = useLayoutEditor.getState();
    const points = polyDrawing.current?.points ?? [];
    return placePoint(toWorld(v, size(), s), {
      from: points[points.length - 1] ?? null,
      straight,
      grid: st.snap ? st.grid : null,
      ends: lineEnds(latest.current.show.props),
      radius: JOIN_PX / v.zoom,
    });
  };

  /** Adds the poly line drawn so far (if it has two points or more), selects it, and goes back to Select. */
  const finishPoly = () => {
    const d = polyDrawing.current;
    if (!d) return;
    const prop = finishDraft({ points: d.points }, newProp("polyLine", latest.current.show), CLICK_PX / currentView().zoom);
    if (!prop) {
      polyDrawing.current = polyNext.current = null;
      return redraw();
    }
    void useApp
      .getState()
      .apply([{ type: "addProp", prop }])
      .then((ok) => {
        // A line the show refuses (over a limit, say) stays drawn, to fix or cancel with Escape.
        if (!ok) return;
        if (polyDrawing.current === d) polyDrawing.current = polyNext.current = null;
        redraw();
        const now = useLayoutEditor.getState();
        now.setTool("select");
        now.select([prop.id]);
      });
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

    /** The selection's outline, with handles to resize and turn it. */
    function drawSelection(props: PreviewProp[]) {
      const sel = selection(props);
      if (!sel || !ctx) return;
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

    const drawing = editor.tool === "polyLine" ? polyDrawing.current : null;
    const next = editor.tool === "polyLine" ? polyNext.current : null;
    if (drawing) {
      const points = next ? [...drawing.points, next.at] : drawing.points;
      const draft = finishDraft({ points }, newProp("polyLine", latest.current.show));
      if (draft) {
        const outline = batchPixels([{ prop: draft.id, frameOffset: 0, channelsPerPixel: 3, points: frontView(draft) }], null, view, s, new Set([draft.id]), PIXEL_COLORS, radius);
        drawBatches(ctx, outline, radius, ratio);
      }
      strokeWithHalo(ctx, 1, [5, 4], () => {
        ctx.beginPath();
        for (const p of points.map(at)) ctx.lineTo(p.x, p.y);
        ctx.stroke();
      });
      for (const p of drawing.points) drawPoint(ctx, at(p), false);
    }
    // The line end a new or dragged point would join.
    const join = d?.kind === "poly" ? d.join : next?.join;
    if (join) drawJoin(ctx, at(join.at));

    if (!editor.editPhoto && selected.size > 0) drawSelection(props);

    const poly = !editor.editPhoto ? editingPoly() : null;
    if (poly) {
      const shown = d?.kind === "poly" && d.prop === poly.id ? { ...poly, shape: d.draft } : poly;
      const handles = polyHandles(shown);
      if (handles) {
        strokeWithHalo(ctx, 1, [3, 3], () => {
          for (const c of handles.controls) {
            const [a, b] = [at(c.from), at(c.at)];
            ctx.beginPath();
            ctx.moveTo(a.x, a.y);
            ctx.lineTo(b.x, b.y);
            ctx.stroke();
          }
        });
        for (const m of handles.middles) drawMiddle(ctx, at(m));
        for (const c of handles.controls) drawControl(ctx, at(c.at));
        const picked = editor.polyPoint?.prop === poly.id ? editor.polyPoint.index : -1;
        handles.vertices.forEach((p, i) => drawPoint(ctx, at(p), i === picked));
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
      useLayoutEditor.subscribe((st) => {
        // Picking another tool drops a poly line half drawn.
        if (st.tool !== "polyLine") polyDrawing.current = polyNext.current = null;
        fitIfNeeded();
        redraw();
      }),
    [redraw],
  );

  // Gestures the engine's new positions include no longer need drawing on top.
  useEffect(() => {
    settlePending(preview.revision);
    const shape = pendingShape.current;
    if (shape?.revision != null && shape.revision <= preview.revision) pendingShape.current = null;
  }, [preview]);

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
    if (!d && polyDrawing.current) {
      polyDrawing.current = null;
      redraw();
      return true;
    }
    if (!d) return false;
    drag.current = null;
    if (d.kind === "photo") useLayoutEditor.getState().setPhotoDraft(null);
    redraw();
    return true;
  };
  const polyKey = (key: string) => {
    const drawing = polyDrawing.current;
    if (!drawing || useLayoutEditor.getState().tool !== "polyLine") return false;
    if (key === "Enter") {
      finishPoly();
      return true;
    }
    if (key === "Backspace" || key === "Delete") {
      const left = removeLastPoint(drawing);
      polyDrawing.current = left.points.length > 0 ? left : null;
      redraw();
      return true;
    }
    return false;
  };
  useImperativeHandle(ref, () => ({ cancel, polyKey }));

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
    if (st.tool === "polyLine") {
      // Each click places a point; double-click or Enter finishes the line. The second click of a
      // double-click, or a click a few pixels from the last point, isn't a new point.
      if (e.detail >= 2) return;
      const { at } = polyPlace(s, e.shiftKey);
      polyDrawing.current = addPoint(polyDrawing.current ?? { points: [] }, at, CLICK_PX / v.zoom);
      polyNext.current = null;
      redraw();
      return;
    }
    if (st.tool !== "select") {
      const p = st.snap ? snapPoint(w, st.grid) : w;
      drag.current = { kind: "draw", tool: st.tool, from: p, to: p, fromScreen: s, toScreen: s };
      redraw();
      return;
    }

    // A selected poly line's own handles come first: its points, curve controls, and middles.
    const polyHit = polyHitAt(s);
    if (polyHit && isPoly(polyHit.prop.shape)) {
      const shape = polyHit.prop.shape;
      drag.current = {
        kind: "poly",
        prop: polyHit.prop.id,
        handle: polyHit.hit,
        fromScreen: s,
        moved: false,
        alt: e.altKey,
        shape,
        draft: shape,
        join: null,
      };
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
        return;
      }
    }
    const startMove = (ids: string[], narrowTo: string | null = null, deselect: string | null = null) => {
      const first = show.props.find((p) => p.id === ids[0]);
      const origin = first ? { x: first.transform.position.x, y: first.transform.position.y } : null;
      const gesture: Gesture = { kind: "move", dx: 0, dy: 0 };
      drag.current = { kind: "move", ids, from: w, fromScreen: s, origin, gesture, narrowTo, deselect };
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
    if (st.tool === "polyLine") {
      // Show where the next point goes (and any line end it would join).
      polyNext.current = polyPlace(s, shiftHeld.current);
      redraw();
      return setCursor("crosshair");
    }
    if (st.tool !== "select") return setCursor("crosshair");
    const polyHover = polyHitAt(s);
    if (polyHover) return setCursor(polyHover.hit.kind === "middle" ? "copy" : "move");
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
    switch (d.kind) {
      case "pan":
        st.setView(panBy(v, s.x - d.last.x, s.y - d.last.y));
        d.last = s;
        break;
      case "move":
        // Until the pointer has really moved, it's still a click.
        if (Math.hypot(s.x - d.fromScreen.x, s.y - d.fromScreen.y) < CLICK_PX && isNoop(d.gesture)) break;
        d.gesture = moveGesture(d.from, w, d.origin, st.snap ? st.grid : null, straight);
        break;
      case "scale":
        d.gesture = scaleGesture(d.frame, d.handle, d.from, w, straight || !d.stretchable);
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
        d.to = straight && DRAWN_BY_ENDS.includes(d.tool) ? constrainAngle(d.from, to) : to;
        d.toScreen = s;
        break;
      }
      case "photo": {
        const aspect = latest.current.photo.aspect;
        st.setPhotoDraft(d.corner ? resizeBackground(d.start, aspect, d.corner, w) : moveBackground(d.start, w.x - d.from.x, w.y - d.from.y));
        break;
      }
      case "poly":
        // Until the pointer has really moved, it's still a click.
        if (!d.moved && Math.hypot(s.x - d.fromScreen.x, s.y - d.fromScreen.y) < CLICK_PX) break;
        d.moved = true;
        dragPoly(d, w, straight);
        break;
    }
    redraw();
  };

  /** Reshapes the poly line as its handle is dragged to world point `w`. */
  const dragPoly = (d: Extract<Drag, { kind: "poly" }>, w: Pt, straight: boolean) => {
    const prop = propById(d.prop);
    if (!prop) return;
    const st = useLayoutEditor.getState();
    const v = currentView();
    const world = (p: { x: number; y: number; z: number }) => {
      const q = applyTransform(p, prop.transform);
      return { x: q.x, y: q.y };
    };
    const { shape, handle } = d;
    if (handle.kind === "vertex") {
      // Shift keeps the stretch to its neighbor at 45° steps; ends of other lines pull it on.
      const neighbor = shape.vertices[handle.index > 0 ? handle.index - 1 : 1];
      const placed = placePoint(w, {
        from: neighbor ? world(neighbor) : null,
        straight,
        grid: st.snap ? st.grid : null,
        ends: lineEnds(latest.current.show.props, new Set([d.prop])),
        radius: JOIN_PX / v.zoom,
      });
      d.join = placed.join;
      d.draft = moveVertex(shape, handle.index, localAt(prop.transform, shape.vertices[handle.index], placed.at));
    } else if (handle.kind === "middle") {
      const mid = shape.vertices[handle.segment];
      d.draft = bendSegment(shape, handle.segment, localAt(prop.transform, mid, w));
    } else {
      const curve = shape.segments[handle.segment]?.curve;
      if (curve) d.draft = moveControl(shape, handle.segment, handle.which, localAt(prop.transform, curve[handle.which], w));
    }
  };

  /** Shift pressed or let go mid-drag takes effect at once, without waiting for the pointer to move. */
  function shiftChanged(held: boolean) {
    shiftHeld.current = held;
    const d = drag.current;
    if (d && d.kind !== "pan" && lastPointer.current) follow(d, lastPointer.current, held);
    else if (!d && lastPointer.current && useLayoutEditor.getState().tool === "polyLine") updateHover(lastPointer.current);
  }

  const onPointerMove = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    const s = point(e);
    lastPointer.current = s;
    const d = drag.current;
    if (!d) return updateHover(s);
    follow(d, s, e.shiftKey);
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
      case "poly": {
        const { handle, shape } = d;
        if (d.moved) {
          commitShape(d.prop, d.draft);
          if (handle.kind === "vertex") st.setPolyPoint({ prop: d.prop, index: handle.index });
          break;
        }
        // A click: Alt/Option takes a point out (or straightens a curve), a middle gets a new
        // point, and a point is picked (Delete then removes it).
        if (handle.kind === "middle") {
          commitShape(d.prop, insertVertex(shape, handle.segment));
          st.setPolyPoint({ prop: d.prop, index: handle.segment + 1 });
        } else if (handle.kind === "vertex" && d.alt) {
          const fewer = removeVertex(shape, handle.index);
          if (fewer) commitShape(d.prop, fewer);
          st.setPolyPoint(null);
        } else if (handle.kind === "vertex") {
          st.setPolyPoint({ prop: d.prop, index: handle.index });
        } else if (d.alt) {
          commitShape(d.prop, straighten(shape, handle.segment));
        }
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
  const drawingPoly = useLayoutEditor((s) => s.tool === "polyLine" && !s.editPhoto);

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
        onDoubleClick={() => {
          if (polyDrawing.current) finishPoly();
        }}
        onPointerLeave={() => setHovered(null)}
        onContextMenu={(e) => e.preventDefault()}
      />
      <p id="layout-canvas-help" className="sr-only">
        Click a prop to select it, or shift-click to select more. Shift-drag across empty space to select everything inside,
        or press Command-A to select every prop. Drag selected props to move them (hold Shift to keep straight across or up and
        down). Drag a corner handle to resize them (hold Shift to keep their proportions), a side handle to stretch them one
        way, or the round handle above them to turn them. Arrow keys move the selection (hold Shift to move it further),
        Command-C copies it, Command-X cuts it, Command-V pastes, Command-D duplicates it, Delete removes it, and Escape
        clears it. To draw a new prop, pick Line, Arch, Matrix, Tree, or a shape under More shapes in the tool bar and drag
        here; hold Shift to keep a line or arch level, upright, or at 45 degrees. For a line that bends, pick Poly Line and
        click each point, then double-click or press Enter to finish (Backspace takes the last point off, Escape stops); a
        point placed on another line's end joins it. A selected poly line shows its points: drag one to move it, Option-click
        or press Delete to remove the one picked, click the small plus in the middle of a stretch to add a point there, or
        drag the plus to curve the stretch. Drag empty space, or scroll with two fingers, to move
        around; pinch, or hold Command and scroll, to zoom. Every prop is also in the props list below.
      </p>
      <SelectionAnnouncer show={show} />
      {drawingPoly && (
        <div className="pointer-events-none absolute top-2 left-2 rounded bg-black/70 px-2 py-1 text-xs text-white">
          Click each point · double-click or Enter to finish · Backspace takes the last point off · Shift keeps 45° · a green ring
          means it joins that line
        </div>
      )}
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

/** A poly line point: white, or filled with the accent when it's the picked one. */
function drawPoint(ctx: CanvasRenderingContext2D, p: Pt, picked: boolean) {
  ctx.beginPath();
  ctx.arc(p.x, p.y, 5, 0, Math.PI * 2);
  ctx.fillStyle = picked ? ACCENT : "#fff";
  ctx.strokeStyle = HALO;
  ctx.lineWidth = 3;
  ctx.stroke();
  ctx.fill();
  ctx.strokeStyle = picked ? "#fff" : ACCENT;
  ctx.lineWidth = 1.5;
  ctx.stroke();
}

/** The middle of a stretch: a small hollow circle with a plus (click adds a point, drag bends it). */
function drawMiddle(ctx: CanvasRenderingContext2D, p: Pt) {
  ctx.beginPath();
  ctx.arc(p.x, p.y, 4, 0, Math.PI * 2);
  ctx.fillStyle = HALO;
  ctx.fill();
  ctx.strokeStyle = ACCENT;
  ctx.lineWidth = 1.25;
  ctx.stroke();
  ctx.beginPath();
  ctx.moveTo(p.x - 2.5, p.y);
  ctx.lineTo(p.x + 2.5, p.y);
  ctx.moveTo(p.x, p.y - 2.5);
  ctx.lineTo(p.x, p.y + 2.5);
  ctx.strokeStyle = "#fff";
  ctx.stroke();
}

/** A curve control: a small diamond. */
function drawControl(ctx: CanvasRenderingContext2D, p: Pt) {
  ctx.beginPath();
  ctx.moveTo(p.x, p.y - 5);
  ctx.lineTo(p.x + 5, p.y);
  ctx.lineTo(p.x, p.y + 5);
  ctx.lineTo(p.x - 5, p.y);
  ctx.closePath();
  ctx.fillStyle = "#fff";
  ctx.strokeStyle = HALO;
  ctx.lineWidth = 3;
  ctx.stroke();
  ctx.fill();
  ctx.strokeStyle = ACCENT;
  ctx.lineWidth = 1.5;
  ctx.stroke();
}

/** Where a point will join another line's end: a bright ring. */
function drawJoin(ctx: CanvasRenderingContext2D, p: Pt) {
  ctx.beginPath();
  ctx.arc(p.x, p.y, 9, 0, Math.PI * 2);
  ctx.strokeStyle = HALO;
  ctx.lineWidth = 4;
  ctx.stroke();
  ctx.strokeStyle = JOIN_COLOR;
  ctx.lineWidth = 2;
  ctx.stroke();
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
