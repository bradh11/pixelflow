// Pure math for the layout editor: the view, hit testing, snapping, selection boxes, gestures
// (move / turn / resize) and how they change a prop's transform, drawing new props, aligning,
// and the background photo. Nothing here touches the DOM, so it is all unit-tested.
//
// World units are PixelFlow layout units: x to the right, y up. Screen units are CSS pixels
// from the canvas's top-left corner, y down.

import type { Background, PreviewProp, Prop, Transform } from "../api/types";
import type { PropKind } from "./shows";

export interface Pt {
  x: number;
  y: number;
}

export interface Box {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

export interface Size {
  width: number;
  height: number;
}

/** What the canvas shows: the world point at its center, and screen pixels per layout unit. */
export interface View {
  cx: number;
  cy: number;
  zoom: number;
}

export const MIN_ZOOM = 0.5;
export const MAX_ZOOM = 2000;
export const DEFAULT_VIEW: View = { cx: 0, cy: 2, zoom: 40 };

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
/** Rounds to 3 decimals so typed-in numbers stay tidy. */
export const tidy = (v: number) => Math.round(v * 1000) / 1000;

// ---- View ------------------------------------------------------------------------------

export function toScreen(view: View, size: Size, p: Pt): Pt {
  return { x: size.width / 2 + (p.x - view.cx) * view.zoom, y: size.height / 2 - (p.y - view.cy) * view.zoom };
}

export function toWorld(view: View, size: Size, s: Pt): Pt {
  return { x: view.cx + (s.x - size.width / 2) / view.zoom, y: view.cy - (s.y - size.height / 2) / view.zoom };
}

/** The view after the canvas changes size: the same center, zoomed so everything that was
 * visible still is (the tighter direction decides). A zero size leaves the view alone. */
export function resizeView(view: View, from: Size, to: Size): View {
  if (from.width <= 0 || from.height <= 0 || to.width <= 0 || to.height <= 0) return view;
  const factor = Math.min(to.width / from.width, to.height / from.height);
  if (factor === 1) return view;
  return { ...view, zoom: clamp(view.zoom * factor, MIN_ZOOM, MAX_ZOOM) };
}

/** Zooms by `factor`, keeping the world point under screen point `s` where it is. */
export function zoomAt(view: View, size: Size, s: Pt, factor: number): View {
  const zoom = clamp(view.zoom * factor, MIN_ZOOM, MAX_ZOOM);
  const anchor = toWorld(view, size, s);
  return {
    zoom,
    cx: anchor.x - (s.x - size.width / 2) / zoom,
    cy: anchor.y + (s.y - size.height / 2) / zoom,
  };
}

/** Moves the view so the picture follows a drag of (dx, dy) screen pixels. */
export function panBy(view: View, dx: number, dy: number): View {
  return { ...view, cx: view.cx - dx / view.zoom, cy: view.cy + dy / view.zoom };
}

/** A view showing all of `box` with `pad` pixels to spare. */
export function fitView(box: Box | null, size: Size, pad = 40): View {
  if (!box || size.width <= 0 || size.height <= 0) return box ? { ...DEFAULT_VIEW, ...boxCenter(box) } : DEFAULT_VIEW;
  const w = Math.max(box.maxX - box.minX, 1e-3);
  const h = Math.max(box.maxY - box.minY, 1e-3);
  const zoom = clamp(Math.min((size.width - 2 * pad) / w, (size.height - 2 * pad) / h), MIN_ZOOM, MAX_ZOOM);
  return { ...boxCenter(box), zoom: w < 1e-2 && h < 1e-2 ? DEFAULT_VIEW.zoom : zoom };
}

export type WheelLike = { deltaX: number; deltaY: number; deltaMode: number; ctrlKey: boolean; metaKey: boolean };

/**
 * ⌘/Ctrl-scrolling zooms, and so do trackpad pinches in Chromium and WebView2 (they arrive as
 * Ctrl-scrolls), as do mouse wheels that scroll by lines or pages. Every other scroll pans.
 * Pinches in Safari and the macOS app arrive as gesture events instead (see `pinchFactor`).
 *
 * Mouse wheels that scroll by pixels pan too: there's no telling them from a fast two-finger
 * flick on a trackpad (WebKit sends both as whole-pixel vertical steps), and zooming by surprise
 * mid-flick is worse than panning. Mouse users zoom with ⌘/Ctrl-scroll or the zoom buttons.
 */
export function wheelIntent(e: WheelLike): "zoom" | "pan" {
  if (e.ctrlKey || e.metaKey) return "zoom";
  return e.deltaMode !== 0 ? "zoom" : "pan";
}

/** How much one wheel event zooms (above 1 zooms in). Pinches send small, frequent steps. */
export function wheelZoomFactor(e: WheelLike): number {
  const step = e.deltaMode === 1 ? e.deltaY * 33 : e.deltaY;
  return Math.exp(-clamp(step, -120, 120) * (e.ctrlKey ? 0.01 : 0.002));
}

/**
 * How much to zoom for one step of a WebKit pinch, whose `scale` is measured from the start of
 * the pinch (1 at the start). Odd values are ignored, and one step never more than doubles.
 */
export function pinchFactor(previousScale: number, scale: number): number {
  if (!(previousScale > 0) || !(scale > 0) || !Number.isFinite(scale)) return 1;
  return clamp(scale / previousScale, 0.5, 2);
}

// ---- Boxes -----------------------------------------------------------------------------

export function boxOfPoints(points: ArrayLike<number>): Box | null {
  if (points.length < 2) return null;
  let [minX, minY, maxX, maxY] = [Infinity, Infinity, -Infinity, -Infinity];
  for (let i = 0; i + 1 < points.length; i += 2) {
    const [x, y] = [points[i], points[i + 1]];
    if (x < minX) minX = x;
    if (x > maxX) maxX = x;
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
  }
  return { minX, minY, maxX, maxY };
}

export function unionBox(boxes: (Box | null | undefined)[]): Box | null {
  let out: Box | null = null;
  for (const b of boxes) {
    if (!b) continue;
    out = out
      ? { minX: Math.min(out.minX, b.minX), minY: Math.min(out.minY, b.minY), maxX: Math.max(out.maxX, b.maxX), maxY: Math.max(out.maxY, b.maxY) }
      : { ...b };
  }
  return out;
}

export function boxCenter(b: Box): { cx: number; cy: number } {
  return { cx: (b.minX + b.maxX) / 2, cy: (b.minY + b.maxY) / 2 };
}

/** The box with corners `a` and `b`, whichever way round they are. */
export function boxFrom(a: Pt, b: Pt): Box {
  return { minX: Math.min(a.x, b.x), minY: Math.min(a.y, b.y), maxX: Math.max(a.x, b.x), maxY: Math.max(a.y, b.y) };
}

export function inBox(b: Box, p: Pt, margin = 0): boolean {
  return p.x >= b.minX - margin && p.x <= b.maxX + margin && p.y >= b.minY - margin && p.y <= b.maxY + margin;
}

// ---- Picking ---------------------------------------------------------------------------

/**
 * The prop under `p`: the one with a pixel nearest it within `radius` (world units; the topmost
 * wins a tie), or else the smallest prop whose outline encloses it, so a click inside an arch,
 * between a matrix's pixels, or within a tree picks it too. The outline is the box around the
 * prop's pixels along its own axes: `angles` holds each turned prop's angle (degrees).
 */
export function hitTest(props: PreviewProp[], p: Pt, radius: number, angles?: ReadonlyMap<string, number>): string | null {
  let best: string | null = null;
  let bestD = radius * radius;
  for (const prop of props) {
    const pts = prop.points;
    for (let i = 0; i + 1 < pts.length; i += 2) {
      const dx = pts[i] - p.x;
      const dy = pts[i + 1] - p.y;
      const d = dx * dx + dy * dy;
      if (d <= bestD) {
        bestD = d;
        best = prop.prop;
      }
    }
  }
  if (best) return best;
  let bestArea = Infinity;
  for (const prop of props) {
    const frame = frameOfPoints([prop.points], angles?.get(prop.prop) ?? 0);
    if (!frame || !inFrame(frame, p, radius)) continue;
    const { box } = frame;
    const area = (box.maxX - box.minX + 2 * radius) * (box.maxY - box.minY + 2 * radius);
    if (area <= bestArea) {
      bestArea = area;
      best = prop.prop;
    }
  }
  return best;
}

/** Props with at least one pixel inside `box`. */
export function propsInBox(props: PreviewProp[], box: Box): string[] {
  return props
    .filter((prop) => {
      for (let i = 0; i + 1 < prop.points.length; i += 2) {
        if (inBox(box, { x: prop.points[i], y: prop.points[i + 1] })) return true;
      }
      return false;
    })
    .map((p) => p.prop);
}

export const snapValue = (v: number, grid: number) => (grid > 0 ? tidy(Math.round(v / grid) * grid) : v);
export const snapPoint = (p: Pt, grid: number): Pt => ({ x: snapValue(p.x, grid), y: snapValue(p.y, grid) });

const EIGHTHS: [number, number][] = [
  [1, 0],
  [Math.SQRT1_2, Math.SQRT1_2],
  [0, 1],
  [-Math.SQRT1_2, Math.SQRT1_2],
  [-1, 0],
  [-Math.SQRT1_2, -Math.SQRT1_2],
  [0, -1],
  [Math.SQRT1_2, -Math.SQRT1_2],
];

/** `to` moved onto the nearest line through `from` at a multiple of 45° (Shift while drawing a line). */
export function constrainAngle(from: Pt, to: Pt): Pt {
  const [dx, dy] = [to.x - from.x, to.y - from.y];
  if (dx === 0 && dy === 0) return to;
  const eighth = ((Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) % 8) + 8) % 8;
  const [ux, uy] = EIGHTHS[eighth];
  const along = dx * ux + dy * uy;
  return { x: from.x + along * ux, y: from.y + along * uy };
}

// ---- Turned frames ---------------------------------------------------------------------

const rad = (deg: number) => (deg * Math.PI) / 180;

/** `p` turned `deg` degrees counterclockwise about the origin. */
export function turn(p: Pt, deg: number): Pt {
  if (deg === 0) return p;
  const [s, c] = [Math.sin(rad(deg)), Math.cos(rad(deg))];
  return { x: p.x * c - p.y * s, y: p.x * s + p.y * c };
}

/**
 * A box along turned axes: `box` holds coordinates turned back by `deg` about the origin, so a
 * prop turned 30° has a snug frame with `deg` 30. With `deg` 0 it's an ordinary box.
 */
export interface Frame {
  box: Box;
  deg: number;
}

const asFrame = (f: Frame | Box): Frame => ("deg" in f ? f : { box: f, deg: 0 });

/** The frame along axes turned `deg` around every point in `points` (each x, y pairs). */
export function frameOfPoints(points: ArrayLike<number>[], deg: number): Frame | null {
  const [s, c] = [Math.sin(rad(-deg)), Math.cos(rad(-deg))];
  let [minX, minY, maxX, maxY] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const pts of points) {
    for (let i = 0; i + 1 < pts.length; i += 2) {
      const x = deg === 0 ? pts[i] : pts[i] * c - pts[i + 1] * s;
      const y = deg === 0 ? pts[i + 1] : pts[i] * s + pts[i + 1] * c;
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    }
  }
  return minX === Infinity ? null : { box: { minX, minY, maxX, maxY }, deg };
}

export function inFrame(frame: Frame, p: Pt, margin = 0): boolean {
  return inBox(frame.box, turn(p, -frame.deg), margin);
}

/** The frame's center, in world units. */
export function frameCenter(frame: Frame): Pt {
  const { cx, cy } = boxCenter(frame.box);
  return turn({ x: cx, y: cy }, frame.deg);
}

/** True when `deg` is a whole number of `step`s (within rounding). */
const multipleOf = (deg: number, step: number) => {
  const r = ((deg % step) + step) % step;
  return r < 1e-3 || step - r < 1e-3;
};

/** A prop's angle in the front view, or null when it's tipped forward or sideways (turned about x or y). */
export function flatAngle(t: Transform): number | null {
  const r = t.rotationDeg;
  return multipleOf(r.x, 180) && multipleOf(r.y, 180) ? r.z : null;
}

/** Each prop's angle in the front view, for `hitTest`: only turned props are listed. */
export function propAngles(props: Prop[]): Map<string, number> {
  const angles = new Map<string, number>();
  for (const p of props) {
    const a = flatAngle(p.transform);
    if (a) angles.set(p.id, a);
  }
  return angles;
}

/**
 * The axes to resize props along, and whether they can be stretched (width and height changed
 * separately) exactly: props all turned the same way, give or take quarter turns, are framed
 * and stretched along their own axes. Props turned different ways, or tipped about x or y, can
 * only be resized in proportion, along the screen's axes: a transform can't skew.
 */
export function frameAngle(props: Prop[]): { deg: number; stretchable: boolean } {
  const angles = props.map((p) => flatAngle(p.transform));
  if (angles.length === 0) return { deg: 0, stretchable: true };
  if (angles.some((a) => a === null)) return { deg: 0, stretchable: false };
  // Within ±45°, so the top handle stays on top.
  const quarter = (a: number) => {
    const q = ((a % 90) + 90) % 90;
    return tidy(q > 45 ? q - 90 : q) || 0;
  };
  const first = quarter(angles[0]!);
  const same = angles.every((a) => multipleOf(quarter(a!) - first, 90));
  return same ? { deg: first, stretchable: true } : { deg: 0, stretchable: false };
}

// ---- Gestures --------------------------------------------------------------------------

/** A change to selected props: moved, turned about a point, or resized from an anchor point. */
export type Gesture =
  /** `dz` (toward the street) is only set by moves in the 3D view. */
  | { kind: "move"; dx: number; dy: number; dz?: number }
  | { kind: "rotate"; cx: number; cy: number; deg: number }
  /** Resized from the anchor (ax, ay) by fx and fy along axes turned `deg` (0 when left out). */
  | { kind: "scale"; ax: number; ay: number; fx: number; fy: number; deg?: number };

/** Where a world point ends up after the gesture (used to draw props while they're dragged). */
export function gesturePoint(g: Gesture, p: Pt): Pt {
  switch (g.kind) {
    case "move":
      return { x: p.x + g.dx, y: p.y + g.dy };
    case "rotate": {
      const [s, c] = [Math.sin(rad(g.deg)), Math.cos(rad(g.deg))];
      const [x, y] = [p.x - g.cx, p.y - g.cy];
      return { x: g.cx + x * c - y * s, y: g.cy + x * s + y * c };
    }
    case "scale": {
      const deg = g.deg ?? 0;
      const q = turn({ x: p.x - g.ax, y: p.y - g.ay }, -deg);
      const r = turn({ x: q.x * g.fx, y: q.y * g.fy }, deg);
      return { x: g.ax + r.x, y: g.ay + r.y };
    }
  }
}

/** An angle in (-180, 180]. */
export function normalizeDeg(deg: number): number {
  const d = ((deg % 360) + 360) % 360;
  return tidy(d > 180 ? d - 360 : d);
}

/**
 * The prop transform that puts its pixels where `gesturePoint` draws them. Exact for moves,
 * turns, resizes in proportion, and stretches along axes the prop is turned to (give or take
 * quarter turns), which are the ones `frameAngle` picks. Any other stretch would need a skew,
 * so it stretches along the prop's own axes instead.
 */
export function gestureTransform(g: Gesture, t: Transform): Transform {
  const at = { x: t.position.x, y: t.position.y };
  const p = gesturePoint(g, at);
  const position = { ...t.position, x: tidy(p.x), y: tidy(p.y) };
  switch (g.kind) {
    case "move":
      return g.dz ? { ...t, position: { ...position, z: tidy(t.position.z + g.dz) } } : { ...t, position };
    case "rotate":
      return { ...t, position, rotationDeg: { ...t.rotationDeg, z: normalizeDeg(t.rotationDeg.z + g.deg) } };
    case "scale": {
      if (g.fx === g.fy) {
        const f = g.fx;
        return { ...t, position, scale: { x: tidy(t.scale.x * f), y: tidy(t.scale.y * f), z: tidy(t.scale.z * f) } };
      }
      const quarter = Math.abs(normalizeDeg(t.rotationDeg.z - (g.deg ?? 0))) % 180;
      const sideways = quarter > 45 && quarter < 135;
      const [fx, fy] = sideways ? [g.fy, g.fx] : [g.fx, g.fy];
      return { ...t, position, scale: { ...t.scale, x: tidy(t.scale.x * fx), y: tidy(t.scale.y * fy) } };
    }
  }
}

/** Each prop's pixels moved by every gesture in `layers` that lists it, in order. */
export function composeGestures(props: PreviewProp[], layers: { ids: string[]; gesture: Gesture }[]): PreviewProp[] {
  if (layers.length === 0) return props;
  const sets = layers.map((l) => new Set(l.ids));
  return props.map((p) => {
    const mine = layers.filter((_, i) => sets[i].has(p.prop));
    if (mine.length === 0) return p;
    const pts = p.points;
    const out = new Float64Array(pts.length);
    for (let i = 0; i + 1 < pts.length; i += 2) {
      let q = { x: pts[i], y: pts[i + 1] };
      for (const { gesture } of mine) q = gesturePoint(gesture, q);
      out[i] = q.x;
      out[i + 1] = q.y;
    }
    return { ...p, points: out };
  });
}

export function isNoop(g: Gesture): boolean {
  switch (g.kind) {
    case "move":
      return Math.abs(g.dx) < 1e-9 && Math.abs(g.dy) < 1e-9 && Math.abs(g.dz ?? 0) < 1e-9;
    case "rotate":
      return Math.abs(g.deg) < 1e-9;
    case "scale":
      return Math.abs(g.fx - 1) < 1e-9 && Math.abs(g.fy - 1) < 1e-9;
  }
}

/** The selection's handles: corners and sides to resize, and one above the box to turn it. */
export type Handle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "rotate";
export type ResizeHandle = Exclude<Handle, "rotate">;
export const CORNERS: ResizeHandle[] = ["nw", "ne", "sw", "se"];
export const SIDES: ResizeHandle[] = ["n", "e", "s", "w"];
/** How far above the selection box the turn handle sits, in screen pixels. */
export const ROTATE_HANDLE_GAP = 28;
/** Side handles show only on edges at least this long on screen, so they don't crowd the corners. */
const SIDE_HANDLE_MIN_PX = 24;

/** Where each handle sits on screen. The turn handle is above the top edge, along the frame's axes. */
export function handlePositions(frame: Frame | Box, view: View, size: Size): Record<Handle, Pt> {
  const { box, deg } = asFrame(frame);
  const at = (x: number, y: number) => toScreen(view, size, turn({ x, y }, deg));
  const [mx, my] = [(box.minX + box.maxX) / 2, (box.minY + box.maxY) / 2];
  const n = at(mx, box.maxY);
  const [s, c] = [Math.sin(rad(deg)), Math.cos(rad(deg))];
  return {
    nw: at(box.minX, box.maxY),
    n,
    ne: at(box.maxX, box.maxY),
    e: at(box.maxX, my),
    se: at(box.maxX, box.minY),
    s: at(mx, box.minY),
    sw: at(box.minX, box.minY),
    w: at(box.minX, my),
    // Screen y points down, so the frame's "up" is (-sin, -cos) there.
    rotate: { x: n.x - ROTATE_HANDLE_GAP * s, y: n.y - ROTATE_HANDLE_GAP * c },
  };
}

/**
 * The handles to offer: the turn handle and the corners always; side handles when the props can
 * be stretched, on edges long enough to hold them, and never across a flat box (a line has no
 * height to stretch).
 */
export function visibleHandles(frame: Frame | Box, view: View, stretchable: boolean): Handle[] {
  const { box } = asFrame(frame);
  const handles: Handle[] = ["rotate", ...CORNERS];
  if (!stretchable) return handles;
  const [w, h] = [(box.maxX - box.minX) * view.zoom, (box.maxY - box.minY) * view.zoom];
  if (w >= SIDE_HANDLE_MIN_PX && h > 1e-6) handles.push("n", "s");
  if (h >= SIDE_HANDLE_MIN_PX && w > 1e-6) handles.push("e", "w");
  return handles;
}

/** The handle under screen point `s`, if any, of those listed. */
export function handleAt(
  frame: Frame | Box,
  view: View,
  size: Size,
  s: Pt,
  handles: readonly Handle[] = ["rotate", ...CORNERS],
  tolerance = 8,
): Handle | null {
  const at = handlePositions(frame, view, size);
  for (const h of handles) {
    if (Math.abs(at[h].x - s.x) <= tolerance && Math.abs(at[h].y - s.y) <= tolerance) return h;
  }
  return null;
}

const RESIZE_CURSORS = ["ew-resize", "nesw-resize", "ns-resize", "nwse-resize"];
const HANDLE_DEG: Record<ResizeHandle, number> = { e: 0, ne: 45, n: 90, nw: 135, w: 180, sw: 225, s: 270, se: 315 };

/** The resize cursor pointing the way the handle drags (the frame may be turned). */
export function handleCursor(handle: Handle, deg: number): string {
  if (handle === "rotate") return "grab";
  const a = (((HANDLE_DEG[handle] + deg) % 180) + 180) % 180;
  return RESIZE_CURSORS[Math.round(a / 45) % 4];
}

/** The corner that stays put while `handle` is dragged (for a side handle, a corner of the opposite side). */
export function oppositeCorner(box: Box, handle: ResizeHandle): Pt {
  return {
    x: handle.includes("w") ? box.maxX : box.minX,
    y: handle.includes("n") ? box.minY : box.maxY,
  };
}

const MIN_FACTOR = 0.02;

/**
 * Resizing from a handle, along the frame's axes, with the opposite corner (or side) staying
 * put. A corner stretches width and height separately, or in proportion when `keepAspect`; a
 * side handle stretches one way only. Never flips the props over.
 */
export function scaleGesture(frame: Frame | Box, handle: ResizeHandle, from: Pt, to: Pt, keepAspect: boolean): Gesture {
  const { box, deg } = asFrame(frame);
  const [f, t] = [turn(from, -deg), turn(to, -deg)];
  const horizontal = handle.includes("e") || handle.includes("w");
  const vertical = handle.includes("n") || handle.includes("s");
  const corner = oppositeCorner(box, handle);
  const a = { x: horizontal ? corner.x : (box.minX + box.maxX) / 2, y: vertical ? corner.y : (box.minY + box.maxY) / 2 };
  const ratio = (now: number, start: number, anchor: number) => {
    const span = start - anchor;
    return Math.abs(span) < 1e-9 ? 1 : Math.max(MIN_FACTOR, (now - anchor) / span);
  };
  let fx = horizontal ? ratio(t.x, f.x, a.x) : 1;
  let fy = vertical ? ratio(t.y, f.y, a.y) : 1;
  if (keepAspect && horizontal && vertical) {
    // Follow the pointer's distance from the anchor, so any drag direction works.
    const start = Math.hypot(f.x - a.x, f.y - a.y);
    const along = start < 1e-9 ? 1 : ((t.x - a.x) * (f.x - a.x) + (t.y - a.y) * (f.y - a.y)) / (start * start);
    fx = fy = Math.max(MIN_FACTOR, along);
  } else {
    if (Math.abs(box.maxX - box.minX) < 1e-9) fx = 1;
    if (Math.abs(box.maxY - box.minY) < 1e-9) fy = 1;
  }
  const fine = (v: number) => Math.round(v * 1e4) / 1e4;
  const anchor = turn(a, deg);
  const g: Gesture = { kind: "scale", ax: anchor.x, ay: anchor.y, fx: fine(fx), fy: fine(fy) };
  return deg === 0 ? g : { ...g, deg };
}

/** Turning about `center`: the angle swept from `from` to `to`, in 15° steps when `stepped`. */
export function rotateGesture(center: Pt, from: Pt, to: Pt, stepped: boolean): Gesture {
  const angle = (p: Pt) => (Math.atan2(p.y - center.y, p.x - center.x) * 180) / Math.PI;
  let deg = normalizeDeg(angle(to) - angle(from));
  if (stepped) deg = Math.round(deg / 15) * 15;
  return { kind: "rotate", cx: center.x, cy: center.y, deg };
}

/**
 * Moving by the drag from `from` to `to`. `straight` (Shift held) keeps to left-right or
 * up-down, whichever the drag went further. With snap on, the first prop's origin lands on the grid.
 */
export function moveGesture(from: Pt, to: Pt, origin: Pt | null, grid: number | null, straight = false): Gesture {
  let dx = to.x - from.x;
  let dy = to.y - from.y;
  const sideways = Math.abs(dx) >= Math.abs(dy);
  if (straight && sideways) dy = 0;
  if (straight && !sideways) dx = 0;
  if (grid && origin) {
    if (!straight || sideways) dx = snapValue(origin.x + dx, grid) - origin.x;
    if (!straight || !sideways) dy = snapValue(origin.y + dy, grid) - origin.y;
  }
  return { kind: "move", dx: tidy(dx), dy: tidy(dy) };
}

// ---- Drawing new props -----------------------------------------------------------------

/** Kinds drawn by dragging from one end to the other (the rest are drawn as a box). */
export const DRAWN_BY_ENDS: PropKind[] = ["line", "arch", "candyCanes", "icicles"];

/**
 * A new prop shaped and placed to what was drawn: a line, arch, candy canes or icicles from `a`
 * to `b`, or the other kinds filling the box with corners `a` and `b`. Pixel counts stay as they
 * are.
 */
export function drawnProp(kind: PropKind, a: Pt, b: Pt, prop: Prop): Prop {
  const shape = structuredClone(prop.shape);
  const transform: Transform = structuredClone(prop.transform);
  if (shape.source !== "generator") return prop;
  const r = (v: number) => Math.max(0.01, Math.round(v * 100) / 100);
  const box = boxFrom(a, b);
  const [w, h] = [box.maxX - box.minX, box.maxY - box.minY];
  const { cx, cy } = boxCenter(box);
  const place = (x: number, y: number) => {
    transform.position = { ...transform.position, x: tidy(x), y: tidy(y) };
  };
  if (DRAWN_BY_ENDS.includes(kind)) {
    const length = Math.hypot(b.x - a.x, b.y - a.y);
    place((a.x + b.x) / 2, (a.y + b.y) / 2);
    transform.rotationDeg = { ...transform.rotationDeg, z: normalizeDeg((Math.atan2(b.y - a.y, b.x - a.x) * 180) / Math.PI) };
    if (shape.type === "line") shape.length = r(length);
    if (shape.type === "candyCanes" || shape.type === "icicles") shape.width = r(length);
    if (shape.type === "arch") {
      const ratio = shape.width > 0 ? shape.height / shape.width : 0.5;
      shape.width = r(length);
      shape.height = r(length * ratio);
    }
  } else if (shape.type === "matrix") {
    place(cx, cy);
    shape.width = r(w);
    shape.height = r(h);
  } else if (shape.type === "tree") {
    place(cx, box.minY);
    const taper = shape.baseRadius > 0 ? shape.topRadius / shape.baseRadius : 0.1;
    shape.height = r(h);
    shape.baseRadius = r(w / 2);
    shape.topRadius = r((w / 2) * taper);
  } else if (shape.type === "windowFrame") {
    place(cx, cy);
    shape.width = r(w);
    shape.height = r(h);
  } else if (shape.type === "circle" || shape.type === "wreath" || shape.type === "spinner") {
    place(cx, cy);
    shape.radius = r(Math.min(w, h) / 2);
  } else if (shape.type === "star") {
    place(cx, cy);
    const ratio = shape.outerRadius > 0 ? shape.innerRadius / shape.outerRadius : 0.4;
    shape.outerRadius = r(Math.min(w, h) / 2);
    shape.innerRadius = r((Math.min(w, h) / 2) * ratio);
  }
  return { ...prop, shape, transform };
}

/** A prop placed with its own size, its origin at `p` (a click instead of a drag). */
export function placedProp(prop: Prop, p: Pt): Prop {
  return { ...prop, transform: { ...prop.transform, position: { ...prop.transform.position, x: tidy(p.x), y: tidy(p.y) } } };
}

/** Where to put a prop whose own pixels span `shape` so it sits just right of `existing`. */
export function besideBox(existing: Box | null, shape: Box | null, gap = 1): Pt {
  if (!existing || !shape) return { x: 0, y: 0 };
  return { x: tidy(existing.maxX + gap - shape.minX), y: tidy(existing.minY - shape.minY) };
}

// ---- Arranging -------------------------------------------------------------------------

export type Align = "left" | "center" | "right" | "top" | "middle" | "bottom";

/** How far each prop moves to line its box up with the others. */
export function alignMoves(boxes: { id: string; box: Box }[], how: Align): Map<string, Pt> {
  const all = unionBox(boxes.map((b) => b.box));
  const moves = new Map<string, Pt>();
  if (!all) return moves;
  const { cx, cy } = boxCenter(all);
  for (const { id, box } of boxes) {
    const c = boxCenter(box);
    const dx = how === "left" ? all.minX - box.minX : how === "center" ? cx - c.cx : how === "right" ? all.maxX - box.maxX : 0;
    const dy = how === "top" ? all.maxY - box.maxY : how === "middle" ? cy - c.cy : how === "bottom" ? all.minY - box.minY : 0;
    moves.set(id, { x: tidy(dx), y: tidy(dy) });
  }
  return moves;
}

/** How far each prop moves to leave equal gaps between them, left to right or bottom to top. */
export function distributeMoves(boxes: { id: string; box: Box }[], axis: "horizontal" | "vertical"): Map<string, Pt> {
  const moves = new Map<string, Pt>();
  if (boxes.length < 3) return moves;
  const lo = (b: Box) => (axis === "horizontal" ? b.minX : b.minY);
  const hi = (b: Box) => (axis === "horizontal" ? b.maxX : b.maxY);
  const sorted = [...boxes].sort((a, b) => lo(a.box) - lo(b.box) || hi(a.box) - hi(b.box));
  const start = lo(sorted[0].box);
  const end = Math.max(...sorted.map((b) => hi(b.box)));
  const sizes = sorted.reduce((sum, b) => sum + hi(b.box) - lo(b.box), 0);
  const gap = (end - start - sizes) / (sorted.length - 1);
  let at = start;
  for (const { id, box } of sorted) {
    const d = tidy(at - lo(box));
    moves.set(id, axis === "horizontal" ? { x: d, y: 0 } : { x: 0, y: d });
    at += hi(box) - lo(box) + gap;
  }
  return moves;
}

/** How far an arrow key moves the selection. */
export function nudgeStep(snap: boolean, grid: number, big: boolean): number {
  const step = snap ? grid : 0.1;
  return tidy(big ? step * 10 : step);
}

/** "Arch 1 copy", or "Arch 1 copy 2" when that's taken. */
export function copyName(name: string, taken: string[]): string {
  const used = new Set(taken);
  const base = `${name} copy`;
  if (!used.has(base)) return base;
  for (let n = 2; ; n++) if (!used.has(`${base} ${n}`)) return `${base} ${n}`;
}

// ---- Background photo ------------------------------------------------------------------

/** The photo's rectangle; `aspect` is its height divided by its width. */
export function backgroundBox(bg: Background, aspect: number): Box {
  return { minX: bg.x, maxX: bg.x + bg.width, maxY: bg.y, minY: bg.y - bg.width * aspect };
}

/** The photo resized from a corner: the opposite corner stays put and its shape is kept. */
export function resizeBackground(bg: Background, aspect: number, corner: ResizeHandle, to: Pt): Background {
  const box = backgroundBox(bg, aspect);
  const a = oppositeCorner(box, corner);
  const width = Math.max(0.5, Math.abs(to.x - a.x), aspect > 0 ? Math.abs(to.y - a.y) / aspect : 0);
  const height = width * aspect;
  const left = corner === "nw" || corner === "sw" ? a.x - width : a.x;
  const top = corner === "nw" || corner === "ne" ? a.y + height : a.y;
  return { ...bg, x: tidy(left), y: tidy(top), width: tidy(width) };
}

export function moveBackground(bg: Background, dx: number, dy: number): Background {
  return { ...bg, x: tidy(bg.x + dx), y: tidy(bg.y + dy) };
}

/** A new photo placed behind the props (a little larger than them), or around the origin. */
export function defaultBackground(path: string, props: Box | null, aspect: number): Background {
  const a = aspect > 0 ? aspect : 0.75;
  if (!props) return { path, x: -10, y: tidy(20 * a * 0.8), width: 20, opacity: 0.7 };
  const { cx, cy } = boxCenter(props);
  const width = Math.max(10, (props.maxX - props.minX) * 1.4, ((props.maxY - props.minY) * 1.4) / a);
  return { path, x: tidy(cx - width / 2), y: tidy(cy + (width * a) / 2), width: tidy(width), opacity: 0.7 };
}
