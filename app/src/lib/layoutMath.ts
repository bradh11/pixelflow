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

/** Pinches and ⌘/Ctrl-scrolls zoom, as do mouse wheel clicks; two-finger scrolling pans. */
export function wheelIntent(e: WheelLike): "zoom" | "pan" {
  if (e.ctrlKey || e.metaKey) return "zoom";
  if (e.deltaMode !== 0) return "zoom"; // line or page steps: a mouse wheel
  return e.deltaX === 0 && Number.isInteger(e.deltaY) && Math.abs(e.deltaY) >= 50 ? "zoom" : "pan";
}

/** How much one wheel event zooms (above 1 zooms in). Pinches send small, frequent steps. */
export function wheelZoomFactor(e: WheelLike): number {
  const step = e.deltaMode === 1 ? e.deltaY * 33 : e.deltaY;
  return Math.exp(-clamp(step, -120, 120) * (e.ctrlKey ? 0.01 : 0.002));
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

/** The prop with a pixel nearest `p`, within `radius` (world units); the topmost wins a tie. */
export function hitTest(props: PreviewProp[], p: Pt, radius: number): string | null {
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

// ---- Gestures --------------------------------------------------------------------------

/** A change to selected props: moved, turned about a point, or resized from an anchor point. */
export type Gesture =
  | { kind: "move"; dx: number; dy: number }
  | { kind: "rotate"; cx: number; cy: number; deg: number }
  | { kind: "scale"; ax: number; ay: number; fx: number; fy: number };

const rad = (deg: number) => (deg * Math.PI) / 180;

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
    case "scale":
      return { x: g.ax + (p.x - g.ax) * g.fx, y: g.ay + (p.y - g.ay) * g.fy };
  }
}

/** An angle in (-180, 180]. */
export function normalizeDeg(deg: number): number {
  const d = ((deg % 360) + 360) % 360;
  return tidy(d > 180 ? d - 360 : d);
}

/**
 * The prop transform that puts its pixels where `gesturePoint` draws them. Exact for moves,
 * turns, and even resizes; a stretch (different x and y factors) of a prop turned at an odd
 * angle can't be shown exactly by a transform, so it stretches along the prop's own axes.
 */
export function gestureTransform(g: Gesture, t: Transform): Transform {
  const at = { x: t.position.x, y: t.position.y };
  const p = gesturePoint(g, at);
  const position = { ...t.position, x: tidy(p.x), y: tidy(p.y) };
  switch (g.kind) {
    case "move":
      return { ...t, position };
    case "rotate":
      return { ...t, position, rotationDeg: { ...t.rotationDeg, z: normalizeDeg(t.rotationDeg.z + g.deg) } };
    case "scale": {
      if (g.fx === g.fy) {
        const f = g.fx;
        return { ...t, position, scale: { x: tidy(t.scale.x * f), y: tidy(t.scale.y * f), z: tidy(t.scale.z * f) } };
      }
      const quarter = Math.abs(normalizeDeg(t.rotationDeg.z)) % 180;
      const sideways = quarter > 45 && quarter < 135;
      const [fx, fy] = sideways ? [g.fy, g.fx] : [g.fx, g.fy];
      return { ...t, position, scale: { ...t.scale, x: tidy(t.scale.x * fx), y: tidy(t.scale.y * fy) } };
    }
  }
}

export function isNoop(g: Gesture): boolean {
  switch (g.kind) {
    case "move":
      return Math.abs(g.dx) < 1e-9 && Math.abs(g.dy) < 1e-9;
    case "rotate":
      return Math.abs(g.deg) < 1e-9;
    case "scale":
      return Math.abs(g.fx - 1) < 1e-9 && Math.abs(g.fy - 1) < 1e-9;
  }
}

/** The selection box handles: four corners to resize, and one above the box to turn it. */
export type Handle = "nw" | "ne" | "sw" | "se" | "rotate";
/** How far above the selection box the turn handle sits, in screen pixels. */
export const ROTATE_HANDLE_GAP = 28;

export function handlePositions(box: Box, view: View, size: Size): Record<Handle, Pt> {
  const nw = toScreen(view, size, { x: box.minX, y: box.maxY });
  const se = toScreen(view, size, { x: box.maxX, y: box.minY });
  return {
    nw,
    ne: { x: se.x, y: nw.y },
    sw: { x: nw.x, y: se.y },
    se,
    rotate: { x: (nw.x + se.x) / 2, y: nw.y - ROTATE_HANDLE_GAP },
  };
}

/** The handle under screen point `s`, if any. */
export function handleAt(box: Box, view: View, size: Size, s: Pt, tolerance = 8): Handle | null {
  const handles = handlePositions(box, view, size);
  for (const h of ["rotate", "nw", "ne", "sw", "se"] as Handle[]) {
    if (Math.abs(handles[h].x - s.x) <= tolerance && Math.abs(handles[h].y - s.y) <= tolerance) return h;
  }
  return null;
}

/** The corner that stays put while `handle` is dragged. */
export function oppositeCorner(box: Box, handle: Exclude<Handle, "rotate">): Pt {
  return {
    x: handle === "nw" || handle === "sw" ? box.maxX : box.minX,
    y: handle === "nw" || handle === "ne" ? box.minY : box.maxY,
  };
}

const MIN_FACTOR = 0.02;

/**
 * Resizing from a corner handle: the opposite corner stays put. Same proportions unless
 * `free`, when width and height change separately. Never flips the props over.
 */
export function scaleGesture(box: Box, handle: Exclude<Handle, "rotate">, from: Pt, to: Pt, free: boolean): Gesture {
  const a = oppositeCorner(box, handle);
  const ratio = (now: number, start: number, anchor: number) => {
    const span = start - anchor;
    return Math.abs(span) < 1e-9 ? 1 : Math.max(MIN_FACTOR, (now - anchor) / span);
  };
  let fx = ratio(to.x, from.x, a.x);
  let fy = ratio(to.y, from.y, a.y);
  const flatX = Math.abs(box.maxX - box.minX) < 1e-9;
  const flatY = Math.abs(box.maxY - box.minY) < 1e-9;
  if (!free) {
    // Follow the pointer's distance from the anchor, so any drag direction works.
    const start = Math.hypot(from.x - a.x, from.y - a.y);
    const along = start < 1e-9 ? 1 : ((to.x - a.x) * (from.x - a.x) + (to.y - a.y) * (from.y - a.y)) / (start * start);
    fx = fy = Math.max(MIN_FACTOR, along);
  } else {
    if (flatX) fx = 1;
    if (flatY) fy = 1;
  }
  const fine = (f: number) => Math.round(f * 1e4) / 1e4;
  return { kind: "scale", ax: a.x, ay: a.y, fx: fine(fx), fy: fine(fy) };
}

/** Turning about `center`: the angle swept from `from` to `to`, in 15° steps when `stepped`. */
export function rotateGesture(center: Pt, from: Pt, to: Pt, stepped: boolean): Gesture {
  const angle = (p: Pt) => (Math.atan2(p.y - center.y, p.x - center.x) * 180) / Math.PI;
  let deg = normalizeDeg(angle(to) - angle(from));
  if (stepped) deg = Math.round(deg / 15) * 15;
  return { kind: "rotate", cx: center.x, cy: center.y, deg };
}

/** Moving with snap on: the first prop's origin lands on the grid. */
export function moveGesture(from: Pt, to: Pt, origin: Pt | null, grid: number | null): Gesture {
  let dx = to.x - from.x;
  let dy = to.y - from.y;
  if (grid && origin) {
    dx = snapValue(origin.x + dx, grid) - origin.x;
    dy = snapValue(origin.y + dy, grid) - origin.y;
  }
  return { kind: "move", dx: tidy(dx), dy: tidy(dy) };
}

// ---- Drawing new props -----------------------------------------------------------------

/** Kinds drawn by dragging from one end to the other (the rest are drawn as a box). */
export const DRAWN_BY_ENDS: PropKind[] = ["line", "arch"];

/**
 * A new prop shaped and placed to what was drawn: a line or arch from `a` to `b`, or the
 * other kinds filling the box with corners `a` and `b`. Pixel counts stay as they are.
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
  if (kind === "line" || kind === "arch") {
    const length = Math.hypot(b.x - a.x, b.y - a.y);
    place((a.x + b.x) / 2, (a.y + b.y) / 2);
    transform.rotationDeg = { ...transform.rotationDeg, z: normalizeDeg((Math.atan2(b.y - a.y, b.x - a.x) * 180) / Math.PI) };
    if (shape.type === "line") shape.length = r(length);
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
  } else if (shape.type === "circle") {
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
export function resizeBackground(bg: Background, aspect: number, corner: Exclude<Handle, "rotate">, to: Pt): Background {
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
