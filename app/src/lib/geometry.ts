// Pixel positions for prop shapes: a TypeScript mirror of crates/pf-geometry, used by the
// in-browser backend and the layout editor. Every generator returns `nodeCount()` points in
// prop-local coordinates, in wiring order.

import type { Generator, MatrixWiring, PolySegment, Prop, ShapeSource, Transform, Vec3 } from "../api/types";

const v = (x: number, y: number, z = 0): Vec3 => ({ x, y, z });

/** Fraction i / (n - 1) in [0, 1]; a single node sits at 0.5. */
function spread(i: number, n: number): number {
  return n <= 1 ? 0.5 : i / (n - 1);
}

/** Generators beyond this many points are cut short here (the engine draws them in full). */
const MAX_POINTS = 200_000;

function range(n: number): number[] {
  return Array.from({ length: Math.max(0, Math.min(Math.floor(n), MAX_POINTS)) }, (_, i) => i);
}

function matrixCell(k: number, columns: number, rows: number, wiring: MatrixWiring): [number, number] {
  const stringLen = wiring.orientation === "horizontal" ? columns : rows;
  const string = Math.floor(k / stringLen);
  let along = k % stringLen;
  if (wiring.serpentine && string % 2 === 1) along = stringLen - 1 - along;
  let [col, row] = wiring.orientation === "horizontal" ? [along, string] : [string, along];
  if (wiring.start === "bottomRight" || wiring.start === "topRight") col = columns - 1 - col;
  if (wiring.start === "topLeft" || wiring.start === "topRight") row = rows - 1 - row;
  return [col, row];
}

function starPoints(points: number, nodes: number, outer: number, inner: number): Vec3[] {
  if (points <= 0 || nodes <= 0) return range(nodes).map(() => v(0, 0));
  const vertices = range(points * 2).map((i) => {
    const r = i % 2 === 0 ? outer : inner;
    const angle = Math.PI / 2 - (Math.PI * i) / points;
    return v(r * Math.cos(angle), r * Math.sin(angle));
  });
  const edges = vertices.map((a, i) => [a, vertices[(i + 1) % vertices.length]] as const);
  const length = (a: Vec3, b: Vec3) => Math.hypot(b.x - a.x, b.y - a.y);
  const perimeter = edges.reduce((sum, [a, b]) => sum + length(a, b), 0);
  return range(nodes).map((i) => {
    let distance = (perimeter * i) / nodes;
    for (const [a, b] of edges) {
      const edge = length(a, b);
      if (distance <= edge && edge > 0) {
        const t = distance / edge;
        return v(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
      }
      distance -= edge;
    }
    return vertices[0];
  });
}

function customGrid(columns: number, rows: number, cells: number[]): Vec3[] {
  const count = Math.min(cells.reduce((m, c) => Math.max(m, c), 0), MAX_POINTS);
  const sums = Array.from({ length: count }, () => ({ x: 0, y: 0, n: 0 }));
  cells.forEach((node, index) => {
    if (node === 0 || columns === 0 || node > count) return;
    const col = index % columns;
    const row = Math.floor(index / columns);
    const s = sums[node - 1];
    s.x += col - (columns - 1) / 2;
    s.y += (rows - 1) / 2 - row;
    s.n++;
  });
  return sums.map((s) => (s.n === 0 ? v(0, 0) : v(s.x / s.n, s.y / s.n)));
}

/** Straight pieces a curved stretch is measured along (the same as pf-geometry's CURVE_STEPS). */
export const CURVE_STEPS = 32;

/** The point at `t` (0–1) along a cubic Bézier from `a` to `b` with control points `c`. */
export function bezier(a: Vec3, c: readonly [Vec3, Vec3], b: Vec3, t: number): Vec3 {
  const u = 1 - t;
  const [k0, k1, k2, k3] = [u * u * u, 3 * u * u * t, 3 * u * t * t, t * t * t];
  return v(
    a.x * k0 + c[0].x * k1 + c[1].x * k2 + b.x * k3,
    a.y * k0 + c[0].y * k1 + c[1].y * k2 + b.y * k3,
    a.z * k0 + c[0].z * k1 + c[1].z * k2 + b.z * k3,
  );
}

const dist = (a: Vec3, b: Vec3) => Math.hypot(b.x - a.x, b.y - a.y, b.z - a.z);

export interface StretchPath {
  joints: Vec3[];
  /** Distance along the stretch at each joint. */
  at: number[];
}

/** One stretch as joints of straight pieces (a curve is cut into CURVE_STEPS of them). */
export function stretchPath(a: Vec3, b: Vec3, curve?: readonly [Vec3, Vec3] | null): StretchPath {
  const joints = curve ? Array.from({ length: CURVE_STEPS + 1 }, (_, i) => bezier(a, curve, b, i / CURVE_STEPS)) : [a, b];
  const at = [0];
  for (let i = 1; i < joints.length; i++) at.push(at[i - 1] + dist(joints[i - 1], joints[i]));
  return { joints, at };
}

export const pathLength = (path: StretchPath) => path.at[path.at.length - 1];

/** The point `d` along a stretch (clamped to its ends). */
export function pointAlong(path: StretchPath, d: number): Vec3 {
  const { joints, at } = path;
  if (d <= 0 || pathLength(path) <= 0) return joints[0];
  let k = at.findIndex((a) => a >= d);
  if (k < 0) k = joints.length - 1;
  k = Math.min(Math.max(k, 1), joints.length - 1);
  const span = at[k] - at[k - 1];
  const t = span > 0 ? Math.min(1, (d - at[k - 1]) / span) : 0;
  const [p, q] = [joints[k - 1], joints[k]];
  return v(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t, p.z + (q.z - p.z) * t);
}

/** Pixels along a poly line: each stretch's own, with half gaps at its ends, or `spread` every length/spread from the first point. */
function polyLine(vertices: Vec3[], segments: PolySegment[], spread: number | null | undefined): Vec3[] {
  const count = Math.min(spread ?? segments.reduce((n, s) => n + s.nodes, 0), MAX_POINTS);
  if (vertices.length < 2) return range(count).map(() => vertices[0] ?? v(0, 0));
  const paths = vertices.slice(1).map((b, k) => stretchPath(vertices[k], b, segments[k]?.curve));
  const lengths = paths.map(pathLength);
  const out: Vec3[] = [];
  if (spread !== null && spread !== undefined) {
    const step = lengths.reduce((a, b) => a + b, 0) / Math.max(1, spread);
    let [k, base] = [0, 0];
    for (const i of range(spread)) {
      const d = i * step;
      while (k + 1 < paths.length && d > base + lengths[k]) {
        base += lengths[k];
        k++;
      }
      out.push(pointAlong(paths[k], d - base));
    }
    return out;
  }
  paths.forEach((path, k) => {
    const n = segments[k]?.nodes ?? 0;
    for (let i = 0; i < n && out.length < MAX_POINTS; i++) out.push(pointAlong(path, ((i + 0.5) / n) * lengths[k]));
  });
  while (out.length < count) out.push(vertices[vertices.length - 1]);
  return out;
}

type CandyCanes = Extract<Generator, { type: "candyCanes" }>;
type Icicles = Extract<Generator, { type: "icicles" }>;

/** Where the `x`th pixel of `n` sits along its cane or drop: alternating goes out every other spot and comes back. */
function spot(x: number, n: number, alternate: boolean): number {
  if (!alternate) return x;
  return x < Math.ceil(n / 2) ? 2 * x : (n - (x + 1)) * 2 + 1;
}

/** Candy canes as xLights lays them out (one light per node), scaled to `width`. */
function candyCanes(g: CandyCanes): Vec3[] {
  const n = g.nodesPerCane;
  if (g.canes <= 0 || n <= 0) return [];
  const gap = 2;
  const caneWidth = (n * 3) / 9;
  const upright = Math.floor((n * 6) / 9);
  const arc = n - upright;
  const total = g.canes * caneWidth + (g.canes - 1) * gap;
  const k = g.width / total;
  const radius = (caneWidth / 2) * g.height;
  const [sin, cos] = [Math.sin(rad(g.skewDeg)), Math.cos(rad(g.skewDeg))];
  const out: Vec3[] = [];
  for (let i = 0; i < g.canes && out.length < MAX_POINTS; i++) {
    const left = i * (caneWidth + gap);
    for (let x = 0; x < n; x++) {
      const p = spot(x, n, g.alternateNodes);
      let foot: number, px: number, py: number;
      if (g.sticks) {
        foot = px = left + caneWidth / 2;
        py = g.caneHeight * p * g.height;
      } else {
        foot = g.reverse ? left + caneWidth : left;
        if (p < upright) {
          px = foot;
          py = g.caneHeight * p * g.height;
        } else {
          const a = Math.PI - (Math.PI * (p - upright + 1)) / arc;
          const along = radius + Math.cos(a) * radius;
          px = g.reverse ? foot - along : foot + along;
          py = g.caneHeight * ((upright - 1) * g.height + Math.sin(a) * radius);
        }
      }
      const dx = px - foot;
      out.push(v((dx * cos - py * sin + foot - total / 2) * k, (dx * sin + py * cos) * k));
    }
  }
  return out;
}

/** Icicles as xLights lays them out: drops filled in turn, a column apart, spread over `width`. */
function icicles(g: Icicles): Vec3[] {
  if (g.strings <= 0 || g.lightsPerString <= 0) return [];
  const drops = g.drops.some((d) => d > 0) ? g.drops : [5];
  const longest = Math.max(...drops);
  const spacing = g.dropHeight / Math.max(longest - 1, 1);
  const spots: [number, number][] = [];
  let column = -1;
  for (let s = 0; s < g.strings && spots.length < MAX_POINTS; s++) {
    column++;
    let [y, d] = [0, 0];
    for (let i = 0; i < g.lightsPerString; i++) {
      while (y >= drops[d]) {
        column++;
        y = 0;
        d = (d + 1) % drops.length;
      }
      spots.push([column, spot(y, drops[d], g.alternateNodes)]);
      y++;
    }
  }
  return spots.map(([col, s]) => v(column === 0 ? 0 : (col / column - 0.5) * g.width, -s * spacing));
}

function generate(g: Generator): Vec3[] {
  switch (g.type) {
    case "line":
      return range(g.nodes).map((i) => v(-g.length / 2 + spread(i, g.nodes) * g.length, 0));
    case "arch":
      return range(g.nodes).map((i) => {
        const angle = Math.PI * (1 - spread(i, g.nodes));
        return v((g.width / 2) * Math.cos(angle), g.height * Math.sin(angle));
      });
    case "circle":
      return range(g.nodes).map((i) => {
        const angle = Math.PI / 2 - (2 * Math.PI * i) / g.nodes;
        return v(g.radius * Math.cos(angle), g.radius * Math.sin(angle));
      });
    case "matrix": {
      const wiring = g.wiring ?? { start: "bottomLeft", orientation: "horizontal", serpentine: true };
      return range(g.columns * g.rows).map((k) => {
        const [col, row] = matrixCell(k, g.columns, g.rows, wiring);
        return v(-g.width / 2 + spread(col, g.columns) * g.width, -g.height / 2 + spread(row, g.rows) * g.height);
      });
    }
    case "tree": {
      const out: Vec3[] = [];
      for (let s = 0; s < g.strings && out.length < MAX_POINTS; s++) {
        const angle = (2 * Math.PI * s) / g.strings;
        for (let j = 0; j < g.nodesPerString; j++) {
          let t = spread(j, g.nodesPerString);
          if (g.serpentine && s % 2 === 1) t = 1 - t;
          const r = g.baseRadius + (g.topRadius - g.baseRadius) * t;
          out.push(v(r * Math.sin(angle), t * g.height, r * Math.cos(angle)));
        }
      }
      return out;
    }
    case "star":
      return starPoints(g.points, g.nodes, g.outerRadius, g.innerRadius);
    case "customGrid":
      return customGrid(g.columns, g.rows, g.cells);
    case "polyLine":
      return polyLine(g.vertices, g.segments, g.spreadNodes);
    case "candyCanes":
      return candyCanes(g);
    case "icicles":
      return icicles(g);
  }
}

/** Pixel positions in prop-local coordinates, in wiring order. */
export function localPositions(shape: ShapeSource): Vec3[] {
  if (shape.source === "measured") return shape.points;
  const { source: _source, ...generator } = shape;
  return generate(generator as Generator);
}

const rad = (deg: number) => (deg * Math.PI) / 180;

/** Scale, then rotation about X, Y, Z (degrees), then translation — as the engine does. */
export function applyTransform(p: Vec3, t: Transform): Vec3 {
  let { x, y, z } = { x: p.x * t.scale.x, y: p.y * t.scale.y, z: p.z * t.scale.z };
  if (t.rotationDeg.x) {
    const [s, c] = [Math.sin(rad(t.rotationDeg.x)), Math.cos(rad(t.rotationDeg.x))];
    [y, z] = [y * c - z * s, y * s + z * c];
  }
  if (t.rotationDeg.y) {
    const [s, c] = [Math.sin(rad(t.rotationDeg.y)), Math.cos(rad(t.rotationDeg.y))];
    [x, z] = [x * c + z * s, -x * s + z * c];
  }
  if (t.rotationDeg.z) {
    const [s, c] = [Math.sin(rad(t.rotationDeg.z)), Math.cos(rad(t.rotationDeg.z))];
    [x, y] = [x * c - y * s, x * s + y * c];
  }
  return v(x + t.position.x, y + t.position.y, z + t.position.z);
}

/** A prop's pixels in the front view (x, y pairs in layout units), in wiring order. */
export function frontView(prop: Prop): number[] {
  const out: number[] = [];
  for (const p of localPositions(prop.shape)) {
    const w = applyTransform(p, prop.transform);
    out.push(w.x, w.y);
  }
  return out;
}

/** A prop's pixels in 3D (x, y, z triples in layout units), in wiring order. */
export function deepView(prop: Prop): Float32Array {
  const local = localPositions(prop.shape);
  const out = new Float32Array(local.length * 3);
  local.forEach((p, i) => {
    const w = applyTransform(p, prop.transform);
    out.set([w.x, w.y, w.z], i * 3);
  });
  return out;
}
