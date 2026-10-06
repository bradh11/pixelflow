// Smart guides for the 2D layout editor, like PowerPoint, Google Slides, and Figma: while props
// are moved, resized, or drawn, their edges and centers snap to other props' edges and centers,
// gaps snap to equal the gaps between other props in the same row or column, and sizes snap to
// other props' widths and heights. Each function also says what to draw: the guide lines, the
// equal gaps, and the matching sizes.
//
// Every box is axis-aligned, in world (layout) units, y up. The other props' boxes go into a
// `GuideIndex` once per gesture: sorted edge, center, and size lists, searched by bisection on
// every pointer move, so a move costs a few binary searches plus one pass over the row.

import type { Box, Gesture, Pt, ResizeHandle } from "./layoutMath";

export type Axis = "x" | "y";

/** How close (screen pixels) an edge has to come to a guide to snap to it, at any zoom. */
export const GUIDE_PX = 6;
/** At most this many props, the nearest, are guides at once. */
export const GUIDE_LIMIT = 400;
/** Lengths closer than this (layout units) count as equal: snapped positions are rounded to 3 decimals. */
const EPS = 1e-3;

/** A line props line up on: for axis "x", the vertical line x = `at` from y `from` to `to`; for "y", horizontal. */
export interface Guide {
  axis: Axis;
  at: number;
  from: number;
  to: number;
}

/** One of several equal gaps: for axis "x", from x `from` to `to`, drawn at height `at`; for "y", upright. */
export interface Gap {
  axis: Axis;
  from: number;
  to: number;
  at: number;
}

/** A box the same width (or height) as the one being resized; `moving` is that one. */
export interface SizeMark {
  dim: "width" | "height";
  box: Box;
  moving: boolean;
}

/** Everything to draw for a snap. */
export interface Marks {
  guides: Guide[];
  gaps: Gap[];
  sizes: SizeMark[];
}

const NO_MARKS: Marks = { guides: [], gaps: [], sizes: [] };

interface Entry {
  v: number;
  i: number;
}

/** The other props' boxes, with their edges, centers, and sizes sorted for searching. */
export interface GuideIndex {
  boxes: Box[];
  lines: Record<Axis, Entry[]>;
  sizes: Record<Axis, Entry[]>;
}

const lo = (b: Box, a: Axis) => (a === "x" ? b.minX : b.minY);
const hi = (b: Box, a: Axis) => (a === "x" ? b.maxX : b.maxY);
const mid = (b: Box, a: Axis) => (lo(b, a) + hi(b, a)) / 2;
const span = (b: Box, a: Axis) => hi(b, a) - lo(b, a);
const across = (a: Axis): Axis => (a === "x" ? "y" : "x");
const shift = (b: Box, dx: number, dy: number): Box => ({ minX: b.minX + dx, minY: b.minY + dy, maxX: b.maxX + dx, maxY: b.maxY + dy });
const AXES: Axis[] = ["x", "y"];

/** The snapping distance in layout units at `zoom` (screen pixels per unit). */
export const guideThreshold = (zoom: number, px = GUIDE_PX) => px / zoom;

/** Guides are on unless turned off, or Alt (Option) is held to place something freely. */
export const guidesActive = (enabled: boolean, mods: { altKey: boolean }) => enabled && !mods.altKey;

/** A gap's length as the properties panel shows numbers: plain, to two decimals. */
export const formatGap = (v: number) => String(Math.round(v * 100) / 100);

export function guideIndex(boxes: Box[]): GuideIndex {
  const lines: Record<Axis, Entry[]> = { x: [], y: [] };
  const sizes: Record<Axis, Entry[]> = { x: [], y: [] };
  boxes.forEach((b, i) => {
    for (const a of AXES) {
      lines[a].push({ v: lo(b, a), i }, { v: mid(b, a), i }, { v: hi(b, a), i });
      sizes[a].push({ v: span(b, a), i });
    }
  });
  for (const a of AXES) {
    lines[a].sort((p, q) => p.v - q.v);
    sizes[a].sort((p, q) => p.v - q.v);
  }
  return { boxes, lines, sizes };
}

/**
 * The boxes that can be guides: those in `view`, and of those only the `limit` nearest `near`,
 * so a show with thousands of props stays quick.
 */
export function nearbyBoxes(boxes: Box[], view: Box, near: Pt, limit = GUIDE_LIMIT): Box[] {
  const seen = boxes.filter((b) => b.maxX >= view.minX && b.minX <= view.maxX && b.maxY >= view.minY && b.minY <= view.maxY);
  if (seen.length <= limit) return seen;
  const d = (b: Box) => Math.hypot(mid(b, "x") - near.x, mid(b, "y") - near.y);
  return seen
    .map((b) => ({ b, d: d(b) }))
    .sort((p, q) => p.d - q.d)
    .slice(0, limit)
    .map((e) => e.b);
}

/** The first entry at or above `v`. */
function lowerBound(list: Entry[], v: number): number {
  let [a, b] = [0, list.length];
  while (a < b) {
    const m = (a + b) >> 1;
    if (list[m].v < v) a = m + 1;
    else b = m;
  }
  return a;
}

/** The entries within `t` of `v`. */
function within(list: Entry[], v: number, t: number): Entry[] {
  const out: Entry[] = [];
  for (let k = lowerBound(list, v - t); k < list.length && list[k].v <= v + t; k++) out.push(list[k]);
  return out;
}

/** The entry value nearest `v`, if one is within `t`. */
function nearest(list: Entry[], v: number, t: number): number | null {
  const k = lowerBound(list, v);
  let best: number | null = null;
  for (const e of [list[k - 1], list[k]]) {
    if (e && Math.abs(e.v - v) <= t && (best === null || Math.abs(e.v - v) < Math.abs(best - v))) best = e.v;
  }
  return best;
}

/** The smaller of two offsets (by size), if any. */
const better = (p: number | null, q: number | null) => (p === null ? q : q === null ? p : Math.abs(q) < Math.abs(p) ? q : p);

/** How far to move `b` along `a` to put an edge or center on another's, if one is within `t`. */
function alignOffset(index: GuideIndex, b: Box, a: Axis, t: number): number | null {
  let best: number | null = null;
  for (const s of [lo(b, a), mid(b, a), hi(b, a)]) {
    const target = nearest(index.lines[a], s, t);
    if (target !== null) best = better(best, target - s);
  }
  return best;
}

/**
 * The other boxes in `b`'s row (for "x": overlapping it up and down) or column, counting those
 * up to `tol` (the snapping distance) short of overlapping, so hand-placed rows don't flicker.
 */
function rowOf(index: GuideIndex, b: Box, a: Axis, tol: number): Box[] {
  const c = across(a);
  return index.boxes.filter((o) => lo(o, c) <= hi(b, c) + tol && hi(o, c) >= lo(b, c) - tol);
}

interface RowGap {
  before: Box;
  after: Box;
  gap: number;
}

/** The gaps between neighbours along a row, left to right (or bottom to top). */
function rowGaps(row: Box[], a: Axis): RowGap[] {
  const sorted = [...row].sort((p, q) => lo(p, a) - lo(q, a));
  const gaps: RowGap[] = [];
  let reach: Box | null = null;
  for (const b of sorted) {
    if (reach && lo(b, a) - hi(reach, a) > EPS) gaps.push({ before: reach, after: b, gap: lo(b, a) - hi(reach, a) });
    if (!reach || hi(b, a) > hi(reach, a)) reach = b;
  }
  return gaps;
}

/** `b`'s nearest neighbours before and after it in its row, allowing `slack` of overlap. */
function neighbours(row: Box[], b: Box, a: Axis, slack: number): { before: Box | null; after: Box | null } {
  let before: Box | null = null;
  let after: Box | null = null;
  for (const o of row) {
    if (mid(o, a) < mid(b, a) && hi(o, a) <= lo(b, a) + slack && (!before || hi(o, a) > hi(before, a))) before = o;
    if (mid(o, a) > mid(b, a) && lo(o, a) >= hi(b, a) - slack && (!after || lo(o, a) < lo(after, a))) after = o;
  }
  return { before, after };
}

/** How far to move `b` along `a` to make a gap equal another in its row, or to sit midway, if within `t`. */
function spacingOffset(index: GuideIndex, b: Box, a: Axis, t: number): number | null {
  const row = rowOf(index, b, a, t);
  if (row.length === 0) return null;
  const { before, after } = neighbours(row, b, a, t);
  let best: number | null = null;
  const consider = (off: number) => {
    if (Math.abs(off) <= t) best = better(best, off);
  };
  if (before && after) {
    const start = (hi(before, a) + lo(after, a) - span(b, a)) / 2;
    if (start >= hi(before, a)) consider(start - lo(b, a));
  }
  for (const { gap } of rowGaps(row, a)) {
    if (before) consider(hi(before, a) + gap - lo(b, a));
    if (after) consider(lo(after, a) - gap - hi(b, a));
  }
  return best;
}

/** Guides for the lines `sources` of `b` along `a` that other props share. */
function alignGuides(index: GuideIndex, b: Box, a: Axis, sources: number[]): Guide[] {
  const c = across(a);
  const guides: Guide[] = [];
  for (const s of sources) {
    const hits = within(index.lines[a], s, EPS);
    if (hits.length === 0) continue;
    const at = hits[0].v;
    if (guides.some((g) => Math.abs(g.at - at) < EPS)) continue;
    let [from, to] = [lo(b, c), hi(b, c)];
    for (const { i } of hits) {
      from = Math.min(from, lo(index.boxes[i], c));
      to = Math.max(to, hi(index.boxes[i], c));
    }
    guides.push({ axis: a, at, from, to });
  }
  return guides;
}

/** Where to draw the gap between two boxes in a row: halfway across where they overlap. */
function gapMark(p: Box, q: Box, a: Axis): Gap {
  const c = across(a);
  const at = (Math.max(lo(p, c), lo(q, c)) + Math.min(hi(p, c), hi(q, c))) / 2;
  return { axis: a, from: hi(p, a), to: lo(q, a), at };
}

/** The gaps either side of `b` along `a` that equal each other or a gap between other props, and those gaps. */
function equalGaps(index: GuideIndex, b: Box, a: Axis, tol: number): Gap[] {
  const row = rowOf(index, b, a, tol);
  if (row.length === 0) return [];
  const { before, after } = neighbours(row, b, a, EPS);
  const gaps = rowGaps(row, a);
  const left = before ? lo(b, a) - hi(before, a) : null;
  const right = after ? lo(after, a) - hi(b, a) : null;
  const marks: Gap[] = [];
  const matched = new Set<RowGap>();
  const check = (own: number | null, other: number | null, mark: () => Gap) => {
    if (own === null || own <= EPS) return;
    const same = gaps.filter((g) => Math.abs(g.gap - own) < EPS);
    if (same.length === 0 && (other === null || Math.abs(other - own) >= EPS)) return;
    marks.push(mark());
    for (const g of same) matched.add(g);
  };
  check(left, right, () => gapMark(before!, b, a));
  check(right, left, () => gapMark(b, after!, a));
  for (const g of matched) marks.push(gapMark(g.before, g.after, a));
  return marks;
}

/** What to draw for a box `b` at rest among the others: shared edges and centers, and equal gaps. */
function moveMarks(index: GuideIndex, b: Box, tol: number): Marks {
  const guides = AXES.flatMap((a) => alignGuides(index, b, a, [lo(b, a), mid(b, a), hi(b, a)]));
  const gaps = AXES.flatMap((a) => equalGaps(index, b, a, tol));
  return guides.length || gaps.length ? { guides, gaps, sizes: [] } : NO_MARKS;
}

export interface MoveSnap {
  dx: number;
  dy: number;
  marks: Marks;
}

/**
 * Moving the box `start` by `raw`, snapped to the guides within `threshold` (layout units) on
 * each axis. An axis with no guide that near takes `fallback` (the grid's move) instead, and a
 * `lock`ed axis (held straight with Shift) is never snapped. Up and down is snapped first, so the
 * row whose gaps the left-right snap matches is the row the box ends up in.
 */
export function snapMove(
  index: GuideIndex,
  start: Box,
  raw: { dx: number; dy: number },
  opts: { threshold: number; fallback?: { dx: number; dy: number }; lock?: { x?: boolean; y?: boolean } },
): MoveSnap {
  const fallback = opts.fallback ?? raw;
  const along = (a: Axis, moved: Box) => {
    const [r, f] = a === "x" ? [raw.dx, fallback.dx] : [raw.dy, fallback.dy];
    if (opts.lock?.[a] || index.boxes.length === 0) return f;
    const off = better(alignOffset(index, moved, a, opts.threshold), spacingOffset(index, moved, a, opts.threshold));
    return off === null ? f : r + off;
  };
  const dy = along("y", shift(start, raw.dx, raw.dy));
  const dx = along("x", shift(start, raw.dx, dy));
  if (index.boxes.length === 0) return { dx, dy, marks: NO_MARKS };
  return { dx, dy, marks: moveMarks(index, shift(start, dx, dy), opts.threshold) };
}

/** The box `start` after a resize gesture with no turn. */
function scaled(start: Box, g: { ax: number; ay: number; fx: number; fy: number }): Box {
  const x = [g.ax + (start.minX - g.ax) * g.fx, g.ax + (start.maxX - g.ax) * g.fx];
  const y = [g.ay + (start.minY - g.ay) * g.fy, g.ay + (start.maxY - g.ay) * g.fy];
  return { minX: Math.min(x[0], x[1]), maxX: Math.max(x[0], x[1]), minY: Math.min(y[0], y[1]), maxY: Math.max(y[0], y[1]) };
}

/** Same-size marks along `a` for the box `b` being resized, and every box that size. */
function sizeMarks(index: GuideIndex, b: Box, a: Axis): SizeMark[] {
  const hits = within(index.sizes[a], span(b, a), EPS);
  if (hits.length === 0 || span(b, a) <= EPS) return [];
  const dim = a === "x" ? "width" : "height";
  return [{ dim, box: b, moving: true }, ...hits.map(({ i }) => ({ dim, box: index.boxes[i], moving: false }) as const)];
}

/**
 * A resize from `handle` of the box `start`, its dragged edges snapped to other props' edges
 * and centers, and its width or height to other props' widths and heights, within `threshold`.
 * With `keepAspect` (a corner, in proportion) whichever side snaps nearer sets both. Frames
 * turned with their props aren't snapped: guides are upright.
 */
export function snapResize(
  index: GuideIndex,
  start: Box,
  handle: ResizeHandle,
  g: Gesture,
  opts: { threshold: number; keepAspect: boolean },
): { gesture: Gesture; marks: Marks } {
  if (g.kind !== "scale" || (g.deg ?? 0) !== 0 || index.boxes.length === 0) return { gesture: g, marks: NO_MARKS };
  const moving: Record<Axis, boolean> = { x: /[ew]/.test(handle), y: /[ns]/.test(handle) };
  const anchor = { x: g.ax, y: g.ay };
  const factor = { x: g.fx, y: g.fy };
  // The dragged edge, before the gesture: east and north are the high sides (y is up).
  const high = (a: Axis) => handle.includes(a === "x" ? "e" : "n");
  const startEdge = (a: Axis) => (high(a) ? hi(start, a) : lo(start, a));
  const snaps: Partial<Record<Axis, { f: number; off: number }>> = {};
  for (const a of AXES) {
    if (!moving[a]) continue;
    const reach = startEdge(a) - anchor[a];
    if (Math.abs(reach) < 1e-9) continue;
    const edge = anchor[a] + reach * factor[a];
    const dir = Math.sign(reach);
    let best: number | null = null;
    const line = nearest(index.lines[a], edge, opts.threshold);
    if (line !== null) best = better(best, line - edge);
    const size = nearest(index.sizes[a], Math.abs(edge - anchor[a]), opts.threshold);
    if (size !== null) best = better(best, anchor[a] + dir * size - edge);
    if (best === null) continue;
    const f = (edge + best - anchor[a]) / reach;
    if (f > 0.02) snaps[a] = { f, off: best };
  }
  const { x, y } = snaps;
  if (!x && !y) return { gesture: g, marks: NO_MARKS };
  let [fx, fy] = [x?.f ?? g.fx, y?.f ?? g.fy];
  if (opts.keepAspect && moving.x && moving.y) {
    const pick = !y || (x && Math.abs(x.off) <= Math.abs(y.off)) ? x! : y;
    fx = fy = pick.f;
  }
  const gesture: Gesture = { ...g, fx, fy };
  const box = scaled(start, gesture);
  const edges = (a: Axis) => [high(a) ? hi(box, a) : lo(box, a)];
  const marks: Marks = {
    guides: AXES.filter((a) => moving[a]).flatMap((a) => alignGuides(index, box, a, edges(a))),
    gaps: [],
    sizes: AXES.filter((a) => moving[a]).flatMap((a) => sizeMarks(index, box, a)),
  };
  return { gesture, marks };
}

/**
 * A point being drawn (a new prop's corner or end), snapped to other props' edges and centers
 * within `threshold`; each axis with none that near takes `fallback` (the grid's point) instead.
 */
export function snapPointTo(index: GuideIndex, p: Pt, opts: { threshold: number; fallback?: Pt }): { point: Pt; marks: Marks } {
  const fallback = opts.fallback ?? p;
  const sx = nearest(index.lines.x, p.x, opts.threshold);
  const sy = nearest(index.lines.y, p.y, opts.threshold);
  const point = { x: sx ?? fallback.x, y: sy ?? fallback.y };
  if (sx === null && sy === null) return { point, marks: NO_MARKS };
  const at: Box = { minX: point.x, maxX: point.x, minY: point.y, maxY: point.y };
  const guides = [...(sx === null ? [] : alignGuides(index, at, "x", [point.x])), ...(sy === null ? [] : alignGuides(index, at, "y", [point.y]))];
  return { point, marks: { guides, gaps: [], sizes: [] } };
}

/**
 * The end `to` of a line held straight from `from` (level, upright, or at 45°, with Shift), slid
 * along that line to the nearer guide on whichever coordinate it changes, if one is within
 * `threshold`. The line keeps its angle.
 */
export function snapAlong(index: GuideIndex, from: Pt, to: Pt, opts: { threshold: number }): { point: Pt; marks: Marks } {
  const [dx, dy] = [to.x - from.x, to.y - from.y];
  let best: { a: Axis; off: number } | null = null;
  for (const a of AXES) {
    const [d, v] = a === "x" ? [dx, to.x] : [dy, to.y];
    if (Math.abs(d) < 1e-9) continue;
    const target = nearest(index.lines[a], v, opts.threshold);
    if (target !== null && (!best || Math.abs(target - v) < Math.abs(best.off))) best = { a, off: target - v };
  }
  if (!best) return { point: to, marks: NO_MARKS };
  // Moving one coordinate by `off` moves the other in proportion, along the line.
  const t = best.off / (best.a === "x" ? dx : dy);
  const point = best.a === "x" ? { x: to.x + best.off, y: to.y + dy * t } : { x: to.x + dx * t, y: to.y + best.off };
  const at: Box = { minX: point.x, maxX: point.x, minY: point.y, maxY: point.y };
  return { point, marks: { guides: alignGuides(index, at, best.a, [best.a === "x" ? point.x : point.y]), gaps: [], sizes: [] } };
}
