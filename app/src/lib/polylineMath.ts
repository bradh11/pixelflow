// Pure math for poly lines in the layout editor: drawing one point by point (with angle, grid,
// and line-end snapping), editing its points and curves, turning a line into one, and joining
// and splitting lines. Nothing here touches the DOM, so it is all unit-tested.
//
// A poly line's points are prop-local; the prop's transform places them. Editing works on flat
// props (turned only in the front view), whose local points map one-to-one onto the canvas.

import type { PolySegment, Prop, Region, ShapeSource, Transform, Vec3 } from "../api/types";
import { type PixelMove, freeRegionName, moveRegion } from "./regionRemap";
import { applyTransform, bezier, pathLength, pointAlong, stretchPath } from "./geometry";
import { type Pt, constrainAngle, flatAngle, snapPoint, tidy } from "./layoutMath";

export type PolyShape = Extract<ShapeSource, { type: "polyLine" }>;
type LineShape = Extract<ShapeSource, { type: "line" }>;

/** Pixels per layout unit on stretches drawn with the Poly Line tool (a new line has 50 over 5). */
export const DRAWN_DENSITY = 10;

const v3 = (x: number, y: number, z = 0): Vec3 => ({ x, y, z });
const tidyV = (p: Vec3): Vec3 => v3(tidy(p.x), tidy(p.y), tidy(p.z));
const add = (a: Vec3, b: Vec3) => v3(a.x + b.x, a.y + b.y, a.z + b.z);
const sub = (a: Vec3, b: Vec3) => v3(a.x - b.x, a.y - b.y, a.z - b.z);
const scaleV = (a: Vec3, k: number) => v3(a.x * k, a.y * k, a.z * k);
const lerp = (a: Vec3, b: Vec3, t: number) => add(a, scaleV(sub(b, a), t));
const dist2 = (a: Pt, b: Pt) => Math.hypot(b.x - a.x, b.y - a.y);

export function isPoly(shape: ShapeSource): shape is PolyShape {
  return shape.source === "generator" && shape.type === "polyLine";
}

function isLine(shape: ShapeSource): shape is LineShape {
  return shape.source === "generator" && shape.type === "line";
}

/** A prop whose points can be dragged on the canvas: a poly line turned only in the front view. */
export function editablePoly(prop: Prop): boolean {
  return isPoly(prop.shape) && flatAngle(prop.transform) !== null;
}

/** A Line or Poly Line turned only in the front view: one that can be joined, split, or bent. */
export function joinable(prop: Prop): boolean {
  return (isPoly(prop.shape) || isLine(prop.shape)) && flatAngle(prop.transform) !== null;
}

// ---- Local and world ---------------------------------------------------------------------

const rad = (deg: number) => (deg * Math.PI) / 180;

function rotate(p: Vec3, axis: "x" | "y" | "z", deg: number): Vec3 {
  if (!deg) return p;
  const [s, c] = [Math.sin(rad(deg)), Math.cos(rad(deg))];
  if (axis === "x") return v3(p.x, p.y * c - p.z * s, p.y * s + p.z * c);
  if (axis === "y") return v3(p.x * c + p.z * s, p.y, -p.x * s + p.z * c);
  return v3(p.x * c - p.y * s, p.x * s + p.y * c, p.z);
}

/** The prop-local point that `transform` puts at world point `w` (undoes `applyTransform`). */
export function toLocal(t: Transform, w: Vec3): Vec3 {
  let p = sub(w, t.position);
  p = rotate(p, "z", -t.rotationDeg.z);
  p = rotate(p, "y", -t.rotationDeg.y);
  p = rotate(p, "x", -t.rotationDeg.x);
  const div = (a: number, s: number) => (s === 0 ? a : a / s);
  return v3(div(p.x, t.scale.x), div(p.y, t.scale.y), div(p.z, t.scale.z));
}

/** A local point moved to world point `w` in the front view, keeping its depth. */
export function localAt(t: Transform, old: Vec3, w: Pt): Vec3 {
  const z = applyTransform(old, t).z;
  return toLocal(t, v3(w.x, w.y, z));
}

/**
 * A Line or Poly Line as a poly line: a line becomes its two ends with all its pixels on the
 * one stretch between them. Anything else is null.
 */
export function asPoly(shape: ShapeSource): PolyShape | null {
  if (isPoly(shape)) return shape;
  if (isLine(shape)) {
    const h = shape.length / 2;
    return { source: "generator", type: "polyLine", vertices: [v3(-h, 0), v3(h, 0)], segments: [{ nodes: shape.nodes }] };
  }
  return null;
}

/** A poly line in world coordinates (points and curve controls), for joining. */
function toWorldPoly(prop: Prop): PolyShape | null {
  const poly = asPoly(prop.shape);
  if (!poly) return null;
  const w = (p: Vec3) => applyTransform(p, prop.transform);
  return {
    ...poly,
    vertices: poly.vertices.map(w),
    segments: poly.segments.map((s) => (s.curve ? { ...s, curve: [w(s.curve[0]), w(s.curve[1])] } : { ...s })),
  };
}

// ---- Line ends (for joining) -------------------------------------------------------------

export interface LineEnd {
  prop: string;
  end: "start" | "end";
  at: Pt;
}

/** The two ends of every Line and Poly Line, except those of the props in `except`. */
export function lineEnds(props: Prop[], except: ReadonlySet<string> = new Set()): LineEnd[] {
  const out: LineEnd[] = [];
  for (const prop of props) {
    if (except.has(prop.id)) continue;
    const poly = asPoly(prop.shape);
    if (!poly || poly.vertices.length < 2) continue;
    const [a, b] = [poly.vertices[0], poly.vertices[poly.vertices.length - 1]];
    for (const [end, p] of [["start", a], ["end", b]] as const) {
      const w = applyTransform(p, prop.transform);
      out.push({ prop: prop.id, end, at: { x: w.x, y: w.y } });
    }
  }
  return out;
}

/** The line end nearest `p` within `radius` (world units), if any. */
export function nearestEnd(ends: LineEnd[], p: Pt, radius: number): LineEnd | null {
  let best: LineEnd | null = null;
  let bestD = radius;
  for (const e of ends) {
    const d = dist2(e.at, p);
    if (d <= bestD) {
      bestD = d;
      best = e;
    }
  }
  return best;
}

/**
 * Where a point goes while drawing or dragging: onto a line end within `radius` (so lines
 * join), else at a multiple of 45° from `from` when `straight` (Shift), else onto the grid.
 */
export function placePoint(
  raw: Pt,
  opts: { from: Pt | null; straight: boolean; grid: number | null; ends: LineEnd[]; radius: number },
): { at: Pt; join: LineEnd | null } {
  const join = nearestEnd(opts.ends, raw, opts.radius);
  if (join) return { at: join.at, join };
  if (opts.straight && opts.from) return { at: constrainAngle(opts.from, raw), join: null };
  return { at: opts.grid ? snapPoint(raw, opts.grid) : raw, join: null };
}

// ---- Drawing -----------------------------------------------------------------------------

/** A poly line being drawn: the points placed so far (world). */
export interface PolyDraft {
  points: Pt[];
}

const SAME = 1e-6;

/**
 * The draft with another point, unless it's within `same` (world units) of the last one: a
 * click there, like the second click of a double-click, is the same point.
 */
export function addPoint(draft: PolyDraft, p: Pt, same = SAME): PolyDraft {
  const last = draft.points[draft.points.length - 1];
  if (last && dist2(last, p) < same) return draft;
  return { points: [...draft.points, p] };
}

/** The draft without its last point (Backspace). */
export function removeLastPoint(draft: PolyDraft): PolyDraft {
  return { points: draft.points.slice(0, -1) };
}

/** Pixels for a stretch `length` long when drawn. */
export const drawnNodes = (length: number) => Math.max(1, Math.round(length * DRAWN_DENSITY));

/** The drawn poly line as a prop (its origin on the first point), or null with fewer than two points. */
export function finishDraft(draft: PolyDraft, base: Prop, same = SAME): Prop | null {
  const pts = draft.points.filter((p, i) => i === 0 || dist2(draft.points[i - 1], p) >= same);
  if (pts.length < 2) return null;
  const o = pts[0];
  const vertices = pts.map((p) => tidyV(v3(p.x - o.x, p.y - o.y)));
  const segments = pts.slice(1).map((p, i) => ({ nodes: drawnNodes(dist2(pts[i], p)) }));
  return {
    ...base,
    shape: { source: "generator", type: "polyLine", vertices, segments },
    transform: { position: { x: tidy(o.x), y: tidy(o.y), z: 0 }, rotationDeg: v3(0, 0, 0), scale: v3(1, 1, 1) },
  };
}

// ---- Editing points ----------------------------------------------------------------------

function withSegments(shape: PolyShape, vertices: Vec3[], segments: PolySegment[]): PolyShape {
  return { ...shape, vertices, segments };
}

/** Point `i` moved to `to` (local); the curve controls next to it move along with it. */
export function moveVertex(shape: PolyShape, i: number, to: Vec3): PolyShape {
  const d = sub(to, shape.vertices[i]);
  const vertices = shape.vertices.map((p, k) => (k === i ? tidyV(to) : p));
  const segments = shape.segments.map((s, k) => {
    if (!s.curve || (k !== i && k !== i - 1)) return s;
    const [c0, c1] = s.curve;
    return { ...s, curve: [k === i ? tidyV(add(c0, d)) : c0, k === i - 1 ? tidyV(add(c1, d)) : c1] as [Vec3, Vec3] };
  });
  return withSegments(shape, vertices, segments);
}

/** The middle of stretch `k` (on its curve, if it has one). */
export function segmentMiddle(shape: PolyShape, k: number): Vec3 {
  const [a, b] = [shape.vertices[k], shape.vertices[k + 1]];
  const curve = shape.segments[k]?.curve;
  return curve ? bezier(a, curve, b, 0.5) : lerp(a, b, 0.5);
}

/**
 * A new point in the middle of stretch `k`, splitting it in two: its pixels are shared out
 * (the second half gets the odd one, as xLights does) and a curve is cut exactly in two.
 */
export function insertVertex(shape: PolyShape, k: number): PolyShape {
  const [a, b] = [shape.vertices[k], shape.vertices[k + 1]];
  const seg = shape.segments[k] ?? { nodes: 0 };
  const first = Math.floor(seg.nodes / 2);
  let left: PolySegment = { nodes: first };
  let right: PolySegment = { nodes: seg.nodes - first };
  let mid = lerp(a, b, 0.5);
  if (seg.curve) {
    // de Casteljau at t = 0.5.
    const [c0, c1] = seg.curve;
    const [ab, bc, cd] = [lerp(a, c0, 0.5), lerp(c0, c1, 0.5), lerp(c1, b, 0.5)];
    const [abc, bcd] = [lerp(ab, bc, 0.5), lerp(bc, cd, 0.5)];
    mid = lerp(abc, bcd, 0.5);
    left = { ...left, curve: [tidyV(ab), tidyV(abc)] };
    right = { ...right, curve: [tidyV(bcd), tidyV(cd)] };
  }
  const vertices = [...shape.vertices.slice(0, k + 1), tidyV(mid), ...shape.vertices.slice(k + 1)];
  const segments = [...shape.segments.slice(0, k), left, right, ...shape.segments.slice(k + 1)];
  return withSegments(shape, vertices, segments);
}

/**
 * Point `i` taken out: an end point takes its stretch (and that stretch's pixels) with it; a
 * point between two stretches joins them into one straight stretch with the pixels of both.
 * Null when that would leave fewer than two points.
 */
export function removeVertex(shape: PolyShape, i: number): PolyShape | null {
  const n = shape.vertices.length;
  if (n <= 2 || i < 0 || i >= n) return null;
  const vertices = shape.vertices.filter((_, k) => k !== i);
  let segments: PolySegment[];
  if (i === 0) segments = shape.segments.slice(1);
  else if (i === n - 1) segments = shape.segments.slice(0, -1);
  else {
    const nodes = (shape.segments[i - 1]?.nodes ?? 0) + (shape.segments[i]?.nodes ?? 0);
    segments = [...shape.segments.slice(0, i - 1), { nodes }, ...shape.segments.slice(i + 1)];
  }
  return withSegments(shape, vertices, segments);
}

/** Stretch `k` curved so its middle passes through `through` (local), its controls a third of the way along. */
export function bendSegment(shape: PolyShape, k: number, through: Vec3): PolyShape {
  const [a, b] = [shape.vertices[k], shape.vertices[k + 1]];
  // A cubic's middle is (a + 3·c0 + 3·c1 + b) / 8, so moving both controls by o moves it by ¾·o.
  const o = scaleV(sub(through, lerp(a, b, 0.5)), 4 / 3);
  const curve: [Vec3, Vec3] = [tidyV(add(lerp(a, b, 1 / 3), o)), tidyV(add(lerp(a, b, 2 / 3), o))];
  return withSegments(
    shape,
    shape.vertices,
    shape.segments.map((s, i) => (i === k ? { ...s, curve } : s)),
  );
}

/** Control point `which` of stretch `k`'s curve moved to `to` (local). */
export function moveControl(shape: PolyShape, k: number, which: 0 | 1, to: Vec3): PolyShape {
  return withSegments(
    shape,
    shape.vertices,
    shape.segments.map((s, i) => {
      if (i !== k || !s.curve) return s;
      const curve: [Vec3, Vec3] = which === 0 ? [tidyV(to), s.curve[1]] : [s.curve[0], tidyV(to)];
      return { ...s, curve };
    }),
  );
}

/** Stretch `k` made straight again. */
export function straighten(shape: PolyShape, k: number): PolyShape {
  return withSegments(
    shape,
    shape.vertices,
    shape.segments.map((s, i) => (i === k ? { nodes: s.nodes } : s)),
  );
}

/** Stretch `k` with `nodes` pixels. */
export function setSegmentNodes(shape: PolyShape, k: number, nodes: number): PolyShape {
  return withSegments(
    shape,
    shape.vertices,
    shape.segments.map((s, i) => (i === k ? { ...s, nodes } : s)),
  );
}

/** Each stretch's length (along its curve). */
export function segmentLengths(shape: PolyShape): number[] {
  return shape.segments.map((s, k) => {
    const [a, b] = [shape.vertices[k], shape.vertices[k + 1]];
    return a && b ? pathLength(stretchPath(a, b, s.curve)) : 0;
  });
}

/**
 * Pixels spread evenly over the whole line (`on`), keeping the total; or each stretch given its
 * share of the total by length (largest remainders first, so the total stays the same).
 */
export function setSpread(shape: PolyShape, on: boolean): PolyShape {
  if (on) {
    if (shape.spreadNodes != null) return shape;
    return { ...shape, spreadNodes: shape.segments.reduce((n, s) => n + s.nodes, 0) };
  }
  if (shape.spreadNodes == null) return shape;
  const total = shape.spreadNodes;
  const lengths = segmentLengths(shape);
  const sum = lengths.reduce((a, b) => a + b, 0);
  const exact = lengths.map((l) => (sum > 0 ? (total * l) / sum : total / Math.max(1, lengths.length)));
  const counts = exact.map(Math.floor);
  let left = total - counts.reduce((a, b) => a + b, 0);
  const order = exact.map((e, i) => [e - Math.floor(e), i] as const).sort((a, b) => b[0] - a[0]);
  for (const [, i] of order) {
    if (left <= 0) break;
    counts[i]++;
    left--;
  }
  const { spreadNodes: _spread, ...rest } = shape;
  return { ...rest, segments: shape.segments.map((s, i) => ({ ...s, nodes: counts[i] ?? 0 })) };
}

/** A Line turned into a poly line with a bend point in its middle (same place, same pixel count). */
export function addBend(prop: Prop): Prop {
  const poly = asPoly(prop.shape);
  if (!poly) return prop;
  return { ...prop, shape: insertVertex(poly, Math.floor((poly.vertices.length - 1) / 2)) };
}

// ---- Joining and splitting ---------------------------------------------------------------

function reversed(shape: PolyShape): PolyShape {
  return {
    ...shape,
    vertices: [...shape.vertices].reverse(),
    segments: [...shape.segments].reverse().map((s) => (s.curve ? { ...s, curve: [s.curve[1], s.curve[0]] } : s)),
  };
}

/**
 * Two lines joined into one. `prop` is the joined line: it keeps the id, name, wiring and
 * placement of the line `kept`, and `removed` is the other line's id. `first` is the line whose
 * start is the joined line's start (where the data comes in); `reversed`, if any, is the line
 * that now runs the other way. Both lines' submodels and faces are carried over to their new
 * pixels; `dropped` names any that couldn't be (a rectangle of a line whose pixels moved).
 */
export interface Join {
  prop: Prop;
  kept: string;
  removed: string;
  first: string;
  reversed: string | null;
  dropped: string[];
}

/**
 * `a` and `b` (Lines or Poly Lines) as one poly line, when an end of one is within `tolerance`
 * (world units) of an end of the other: the line touching at its end comes first and the other
 * carries on from there (when both touch at their starts, or both at their ends, `b` is turned
 * round). By default the line that comes first keeps its wiring; `keep` (a's or b's id) picks the
 * other. Spread-out pixels become per-stretch counts first.
 */
export function joinLines(a: Prop, b: Prop, tolerance: number, keep?: string): Join | null {
  if (!joinable(a) || !joinable(b) || a.id === b.id) return null;
  const wa = toWorldPoly(a);
  const wb = toWorldPoly(b);
  if (!wa || !wb) return null;
  const pa = setSpread(wa, false);
  const pb = setSpread(wb, false);
  const [na, nb] = [nodeTotal(pa), nodeTotal(pb)];
  const ends = (s: PolyShape) => [s.vertices[0], s.vertices[s.vertices.length - 1]];
  const [aStart, aEnd] = ends(pa);
  const [bStart, bEnd] = ends(pb);
  // [distance, joined line, the line first, where a's and b's pixels go, b turned round]
  type Option = [number, () => PolyShape, Prop, number, number, boolean];
  const options: Option[] = [
    [dist2(aEnd, bStart), () => chain(pa, pb), a, 0, na, false],
    [dist2(aEnd, bEnd), () => chain(pa, reversed(pb)), a, 0, na, true],
    [dist2(aStart, bEnd), () => chain(pb, pa), b, nb, 0, false],
    [dist2(aStart, bStart), () => chain(reversed(pb), pa), b, nb, 0, true],
  ];
  const [d, build, first, aAt, bAt, bReversed] = options.reduce((best, o) => (o[0] < best[0] ? o : best));
  if (d > tolerance) return null;
  const kept = keep === a.id || keep === b.id ? (keep === a.id ? a : b) : first;
  const other = kept.id === a.id ? b : a;
  const world = build();
  const local = (p: Vec3) => tidyV(toLocal(kept.transform, p));
  const shape: PolyShape = {
    source: "generator",
    type: "polyLine",
    vertices: world.vertices.map(local),
    segments: world.segments.map((s) => (s.curve ? { ...s, curve: [local(s.curve[0]), local(s.curve[1])] } : s)),
  };
  const moves = new Map<string, PixelMove>([
    [a.id, { from: 0, to: na, offset: aAt, reverse: false }],
    [b.id, { from: 0, to: nb, offset: bAt, reverse: bReversed }],
  ]);
  const dropped: string[] = [];
  const regions: Region[] = [];
  const taken = new Set<string>();
  for (const line of [kept, other]) {
    for (const r of line.regions) {
      const moved = moveRegion(r, moves.get(line.id)!);
      if (!moved) {
        dropped.push(`${r.name} (${line.name})`);
        continue;
      }
      const name = line === kept ? moved.name : freeRegionName(moved.name, taken);
      taken.add(name.trim().toLowerCase());
      regions.push({ ...moved, name });
    }
  }
  return {
    prop: { ...kept, shape, regions },
    kept: kept.id,
    removed: other.id,
    first: first.id,
    reversed: bReversed ? b.id : null,
    dropped,
  };
}

const nodeTotal = (s: PolyShape) => s.spreadNodes ?? s.segments.reduce((n, x) => n + x.nodes, 0);

/** `first` then `second`, `second`'s first point dropped (it's where `first` ends). */
function chain(first: PolyShape, second: PolyShape): PolyShape {
  return {
    ...first,
    vertices: [...first.vertices, ...second.vertices.slice(1)],
    segments: [...first.segments, ...second.segments],
  };
}

/**
 * The poly line cut in two at point `i` (not an end): the first part keeps the prop (its id,
 * name and wiring), the second becomes a new prop with `id` and `name`, unwired.
 */
export function splitAt(prop: Prop, i: number, id: string, name: string): [Prop, Prop] | null {
  if (!isPoly(prop.shape)) return null;
  const shape = setSpread(prop.shape, false);
  if (i <= 0 || i >= shape.vertices.length - 1) return null;
  const { spreadNodes: _spread, ...base } = shape;
  const first: PolyShape = { ...base, vertices: shape.vertices.slice(0, i + 1), segments: shape.segments.slice(0, i) };
  const second: PolyShape = { ...base, vertices: shape.vertices.slice(i), segments: shape.segments.slice(i) };
  // Each part keeps the submodels' and faces' pixels that are on it.
  const [k, n] = [nodeTotal(first), nodeTotal(shape)];
  const part = (m: PixelMove) => prop.regions.flatMap((r) => moveRegion(r, m) ?? []);
  return [
    { ...prop, shape: first, regions: part({ from: 0, to: k, offset: 0, reverse: false }) },
    {
      ...structuredClone(prop),
      id,
      name,
      shape: second,
      regions: part({ from: k, to: n, offset: -k, reverse: false }).map((r) => ({ ...r, id: crypto.randomUUID() })),
    },
  ];
}

// ---- Handles -----------------------------------------------------------------------------

/** Where a poly line's handles are, in world (front view) coordinates. */
export interface PolyHandles {
  vertices: Pt[];
  /** The middle of each stretch: click to add a point there, drag to bend the stretch. */
  middles: Pt[];
  /** Each curved stretch's two control points, with its index. */
  controls: { segment: number; which: 0 | 1; at: Pt; from: Pt }[];
}

export function polyHandles(prop: Prop): PolyHandles | null {
  if (!isPoly(prop.shape)) return null;
  const shape = prop.shape;
  const w = (p: Vec3) => {
    const q = applyTransform(p, prop.transform);
    return { x: q.x, y: q.y };
  };
  const controls: PolyHandles["controls"] = [];
  shape.segments.forEach((s, k) => {
    if (!s.curve || !shape.vertices[k + 1]) return;
    controls.push({ segment: k, which: 0, at: w(s.curve[0]), from: w(shape.vertices[k]) });
    controls.push({ segment: k, which: 1, at: w(s.curve[1]), from: w(shape.vertices[k + 1]) });
  });
  return {
    vertices: shape.vertices.map(w),
    middles: shape.segments.map((_, k) => (shape.vertices[k + 1] ? w(segmentMiddle(shape, k)) : w(shape.vertices[k]))),
    controls,
  };
}

/** Points along the whole line (world), to outline it while it's edited. */
export function polyOutline(prop: Prop): Pt[] {
  if (!isPoly(prop.shape)) return [];
  const shape = prop.shape;
  const out: Pt[] = [];
  shape.segments.forEach((s, k) => {
    const [a, b] = [shape.vertices[k], shape.vertices[k + 1]];
    if (!a || !b) return;
    const path = stretchPath(a, b, s.curve);
    const steps = s.curve ? 24 : 1;
    for (let i = k === 0 ? 0 : 1; i <= steps; i++) {
      const q = applyTransform(pointAlong(path, (i / steps) * pathLength(path)), prop.transform);
      out.push({ x: q.x, y: q.y });
    }
  });
  return out;
}
