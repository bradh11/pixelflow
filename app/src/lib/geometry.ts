// Pixel positions for prop shapes: a TypeScript mirror of crates/pf-geometry, used by the
// in-browser backend and the layout editor. Every generator returns `nodeCount()` points in
// prop-local coordinates, in wiring order.

import type { Generator, MatrixWiring, Prop, ShapeSource, Transform, Vec3 } from "../api/types";

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
