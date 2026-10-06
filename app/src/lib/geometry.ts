// Pixel positions for prop shapes: a TypeScript mirror of crates/pf-geometry, used by the
// in-browser backend and the layout editor. Every generator returns `nodeCount()` points in
// prop-local coordinates, in wiring order.

import type { CubeStart, CubeStyle, Generator, MatrixWiring, PolySegment, Prop, ShapeSource, StrandStyle, Transform, Vec3 } from "../api/types";

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
export const CURVE_STEPS = 25;

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
  for (let nth = 0; nth < g.canes && out.length < MAX_POINTS; nth++) {
    const i = g.startRight ? g.canes - 1 - nth : nth;
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

type WindowFrame = Extract<Generator, { type: "windowFrame" }>;
type Spinner = Extract<Generator, { type: "spinner" }>;

/** A window frame as xLights lays it out (in its single-precision steps), scaled to `width` by `height`. */
function windowFrame(g: WindowFrame): Vec3[] {
  const f = Math.fround;
  const [top, side, bottom] = [g.top, g.sides, g.bottom];
  const total = Math.min(top + 2 * side + bottom, MAX_POINTS);
  if (total <= 0) return [];
  const ccw = g.counterClockwise;
  const ltor = g.start === "bottomLeft" || g.start === "topLeft";
  const btot = g.start === "bottomLeft" || g.start === "bottomRight";
  const w = Math.max(top, bottom) + 2;
  const dir = ccw ? -1 : 1;
  // The edge the string starts along takes the corners.
  const odd = ccw ? btot === ltor : btot !== ltor;
  const [wadj, hadj] = odd ? [2, -2] : [0, 0];
  const topSi = top + wadj - 1 !== 0 ? f(w / (top + 1)) : 1;
  const botSi = bottom + wadj - 1 !== 0 ? f(-w / (bottom + 1)) : 1;
  const lengths = [side + hadj, top + wadj, side + hadj, bottom + wadj];
  const xsi = [0, topSi, 0, botSi];
  const ysi = [1, 0, -1, 0];
  const hh = (side - 1) / 2;
  const half = w / 2;
  const [xs, ys] = ccw
    ? odd
      ? [[-half, half, half, -half], [hh - 1, hh, -hh + 1, -hh]]
      : [[-half, f(half - topSi), half, f(-half - botSi)], [hh, hh, -hh, -hh]]
    : odd
      ? [[-half, -half, half, half], [-hh + 1, hh, hh - 1, -hh]]
      : [[-half, f(-half + topSi), half, f(half + botSi)], [-hh, hh, hh, -hh]];
  const orders: Record<string, number[]> = {
    "true,true,false": [0, 1, 2, 3],
    "true,true,true": [3, 2, 1, 0],
    "true,false,false": [1, 2, 3, 0],
    "true,false,true": [0, 3, 2, 1],
    "false,true,false": [3, 0, 1, 2],
    "false,true,true": [2, 1, 0, 3],
    "false,false,false": [2, 3, 0, 1],
    "false,false,true": [1, 0, 3, 2],
  };
  const idx = orders[`${ltor},${btot},${ccw}`];
  const nextEdge = (s: number) => {
    for (let i = 0; i < 4 && lengths[idx[s]] === 0; i++) s = (s + 1) % 4;
    return s;
  };
  let s = nextEdge(0);
  let [x, y, left] = [xs[idx[s]], ys[idx[s]], lengths[idx[s]]];
  const [kx, ky] = [g.width / w, g.height / Math.max(side - 1, 1)];
  const out: Vec3[] = [];
  for (let i = 0; i < total; i++) {
    out.push(v(x * kx, y * ky));
    x = f(x + xsi[idx[s]] * dir);
    y = f(y + ysi[idx[s]] * dir);
    if (--left <= 0) {
      s = nextEdge((s + 1) % 4);
      [x, y, left] = [xs[idx[s]], ys[idx[s]], lengths[idx[s]]];
    }
  }
  return out;
}

/** A wreath as xLights places its lights: round a ring, each rounded to a square grid `nodes / 2` steps across the radius. */
function wreath(nodes: number, radius: number, startAtBottom: boolean, counterClockwise: boolean): Vec3[] {
  const offset = Math.floor(nodes / 2);
  const unit = radius / Math.max(offset, 1);
  let pct = startAtBottom ? 0.5 : 0;
  const step = 1 / nodes;
  const incr = counterClockwise ? -step : step;
  return range(nodes).map(() => {
    const a = pct * 2 * Math.PI;
    const x = Math.trunc(offset * Math.sin(a) + offset + 0.5) - offset;
    const y = Math.trunc(offset * Math.cos(a) + offset + 0.5) - offset;
    pct += incr;
    if (pct >= 1) pct -= 1;
    if (pct < 0) pct += 1;
    return v(x * unit, y * unit);
  });
}

/** A spinner as xLights lays it out, its arms' angles stepping in single precision as xLights' do. */
function spinner(g: Spinner): Vec3[] {
  const f = Math.fround;
  const [arms, npa] = [g.arms, g.nodesPerArm];
  if (arms <= 0 || npa <= 0) return [];
  const pi2 = f(Math.PI) * 2;
  let angle = f(f(pi2 * f(270 + f(g.startAngle))) / 360);
  const sweep = f(pi2 * f(g.arc));
  const incr = g.arc < 360 && arms > 1 ? f(sweep / f((arms - 1) * 360)) : f(sweep / f(arms * 360));
  const hollow = (g.hollow * 2 * npa) / 100;
  const unit = g.radius / (npa - 0.5 + hollow);
  const out: Vec3[] = [];
  for (let a = 0; a < arms && out.length < MAX_POINTS; a++) {
    const [sin, cos] = [Math.sin(angle), Math.cos(angle)];
    const outward = g.fromCenter !== (g.zigZag && a % 2 === 1);
    for (let n = 0; n < npa; n++) {
      const step = g.alternate ? (n < Math.ceil(npa / 2) ? 2 * n : (npa - (n + 1)) * 2 + 1) : outward ? n : npa - n - 1;
      const r = (0.5 + step + hollow) * unit;
      out.push(v(r * cos, r * sin));
    }
    angle = f(g.clockwise ? angle - incr : angle + incr);
  }
  return out;
}

type Arch = Extract<Generator, { type: "arch" }>;

/** Arches as xLights lays them out: parts of an ellipse `arc` degrees round, feet `width` apart on y = 0, in a row or nested in layers (pf-geometry's arch.rs). */
function arch(g: Arch): Vec3[] {
  const arc = Number.isFinite(g.arc ?? 180) ? Math.min(Math.max(g.arc ?? 180, 1), 180) : 180;
  const theta = rad(arc);
  const half = theta / 2;
  const ea = g.width / 2 / Math.sin(half);
  const eb = g.height / (1 - Math.cos(half));
  const drop = eb * Math.cos(half);
  const skew = rad(g.skewDeg ?? 0);
  const place = (x: number, adj: number, angle: number) => {
    const px = x + ea * adj * Math.sin(angle);
    const py = eb * adj * Math.cos(angle) - drop;
    return v(px - py * Math.sin(skew), py * Math.cos(skew));
  };
  const layers = g.layers ?? [];
  if (layers.length === 0) {
    const n = g.arches ?? 1;
    const gap = g.gap ?? 0;
    const total = n * g.width + Math.max(n - 1, 0) * gap;
    const out: Vec3[] = [];
    for (let k = 0; k < n && out.length < MAX_POINTS; k++) {
      const x = -total / 2 + g.width / 2 + k * (g.width + gap);
      for (const i of range(g.nodes)) out.push(place(x, 1, -half + theta * spread(i, g.nodes)));
    }
    return g.startRight ? out.reverse() : out;
  }
  // Layered: each pixel's spot along the outermost layer and its layer, as xLights numbers them.
  const lc = layers.length;
  const maxLen = Math.max(...layers);
  const nodes = Math.min(g.nodes, MAX_POINTS);
  const spots: [number, number][] = Array.from({ length: nodes }, () => [0, 0]);
  let idx = 0;
  let forward = !g.startRight;
  for (let layer = 0; layer < lc && idx < nodes; layer++) {
    const yy = g.startInside ? layer : lc - layer - 1;
    const it = layers[yy];
    if (it === 1) {
      spots[idx++] = [Math.floor(maxLen / 2), yy];
    } else {
      const step = Math.fround(Math.fround(maxLen - 1) / Math.fround(it - 1));
      for (let x = 0; x < it; x++, idx++) {
        if (idx >= nodes) continue;
        let xx = Math.round(Math.fround(x * step));
        if (!forward) xx = maxLen - 1 - xx;
        spots[idx] = [xx, yy];
      }
    }
    if (g.zigZag) forward = !forward;
  }
  const midpt = (maxLen - 1) / 2;
  const layerGap = lc > 1 ? (1 - (g.hollow ?? 70) / 100) / (lc - 1) : 0;
  return spots.map(([x, y]) => place(0, 1 - layerGap * (lc - 1 - y), midpt === 0 ? 0 : -half + (theta * x) / midpt / 2));
}

function generate(g: Generator): Vec3[] {
  switch (g.type) {
    case "line":
      return range(g.nodes).map((i) => v(-g.length / 2 + spread(i, g.nodes) * g.length, 0));
    case "arch":
      return arch(g);
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
      const n = g.strings;
      const [style, degrees, startAngle] = [g.style ?? "round", g.degrees ?? 360, g.startAngle ?? 0];
      const step = degrees < 350 && n > 1 ? degrees / (n - 1) : degrees / Math.max(n, 1);
      for (let s = 0; s < n && out.length < MAX_POINTS; s++) {
        const angle = rad(startAngle + s * step);
        const across = (s + 0.5 - n / 2) / (n / 2);
        const [xb, xt] = [across * g.baseRadius, across * g.topRadius];
        const slant = Math.hypot(g.height, xt - xb);
        for (let j = 0; j < g.nodesPerString; j++) {
          let t = spread(j, g.nodesPerString);
          if (g.serpentine && s % 2 === 1) t = 1 - t;
          if (style === "round") {
            const r = g.baseRadius + (g.topRadius - g.baseRadius) * t;
            out.push(v(r * Math.sin(angle), t * g.height, r * Math.cos(angle)));
          } else if (style === "flat") out.push(v(xb + (xt - xb) * t, t * g.height));
          else out.push(v(xb + (xt - xb) * t, slant > 0 ? (t * g.height * g.height) / slant : 0));
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
    case "windowFrame":
      return windowFrame(g);
    case "wreath":
      return wreath(g.nodes, g.radius, g.startAtBottom, g.counterClockwise);
    case "spinner":
      return spinner(g);
    case "sphere":
      return sphere(g);
    case "cube":
      return cube(g);
  }
}

/** How far along its strand the `y`th of `n` pixels sits (pf-geometry's `along_strand`). */
function alongStrand(y: number, n: number, strand: number, style: StrandStyle): number {
  if (style === "zigZag") return strand % 2 === 1 ? n - 1 - y : y;
  if (style === "noZigZag") return y;
  return y < Math.ceil(n / 2) ? 2 * y : (n - (y + 1)) * 2 + 1;
}

/** A sphere as xLights lays it out: strands round a globe, the first at the back running up from the south. */
function sphere(g: Extract<Generator, { type: "sphere" }>): Vec3[] {
  const { columns, rows, radius } = g;
  const [lat0, lat1, degrees] = [g.startLatitude ?? -86, g.endLatitude ?? 86, g.degrees ?? 360];
  const start = g.start ?? "bottomLeft";
  const style = g.strandStyle ?? "zigZag";
  const fromLeft = start === "bottomLeft" || start === "topLeft";
  const fromBottom = start === "bottomLeft" || start === "bottomRight";
  const remove = rad(360 - degrees);
  const fudge = rad((360 - degrees) / columns);
  const vIncr = rows > 1 ? rad(lat1 - lat0) / (rows - 1) : 0;
  const out: Vec3[] = [];
  for (let x = 0; x < columns && out.length < MAX_POINTS; x++) {
    const column = fromLeft ? x : columns - 1 - x;
    const h = Math.PI / 2 + 0.003 - remove / 2 + (column * (-2 * Math.PI + remove - fudge)) / columns;
    for (let y = 0; y < rows; y++) {
      const along = alongStrand(y, rows, x, style);
      const row = fromBottom ? along : rows - 1 - along;
      const vv = rad(lat0 - 90) + row * vIncr;
      const sv = Math.sin(vv);
      out.push(v(Math.cos(h) * sv * radius, Math.cos(vv) * radius, Math.sin(h) * sv * radius));
    }
  }
  return out;
}

/** `{ turns about X, Y, Z, mirror }` per start corner and style, in xLights' order (pf-geometry's cube table). */
const CUBE_TRANSFORMS = [
  [1, 0, -1, 0], [0, 0, -1, 1], [0, -1, 0, 1], [0, 0, 0, 0], [-1, 2, 0, 1], [-1, -1, 0, 0],
  [1, 0, -1, 1], [0, 0, -1, 0], [0, -1, 0, 0], [0, 0, 0, 1], [-1, 2, 0, 0], [-1, -1, 0, 1],
  [1, 0, 1, 1], [0, 0, 1, 0], [0, -1, 2, 0], [0, 0, 2, 1], [-1, 2, 2, 0], [-1, -1, 2, 1],
  [1, 0, 1, 0], [0, 0, 1, 1], [0, -1, 2, 1], [0, 0, 2, 0], [-1, 2, 2, 1], [-1, -1, 2, 0],
  [-1, 0, -1, 1], [0, 2, 1, 0], [0, 1, 0, 0], [0, 2, 0, 1], [-1, 0, 0, 0], [-1, 1, 0, 1],
  [-1, 0, -1, 0], [0, 2, 1, 1], [0, 1, 0, 1], [0, 2, 0, 0], [-1, 0, 0, 1], [-1, 1, 0, 0],
  [-1, 0, 1, 0], [0, 2, -1, 1], [0, -1, 2, 0], [2, 0, 0, 0], [-1, 2, 2, 0], [1, -1, 0, 0],
  [-1, 0, 1, 1], [0, 2, -1, 0], [0, -1, 2, 1], [2, 0, 0, 1], [-1, 2, 2, 1], [1, -1, 0, 1],
];
const CUBE_STARTS: CubeStart[] = ["frontBottomLeft", "frontBottomRight", "frontTopLeft", "frontTopRight", "backBottomLeft", "backBottomRight", "backTopLeft", "backTopRight"];
const CUBE_STYLES: CubeStyle[] = ["verticalFrontBack", "verticalLeftRight", "horizontalFrontBack", "horizontalLeftRight", "stackedFrontBack", "stackedLeftRight"];

/** Each pixel's cell `[x, y, z]` (x from the left, y from the bottom, z from the front) in wiring order. */
function cubeCells(w0: number, h0: number, d0: number, start: CubeStart, style: CubeStyle, strandStyle: StrandStyle, perLayer: boolean): [number, number, number][] {
  const strand = strandStyle === "zigZag" ? 0 : strandStyle === "noZigZag" ? 1 : 2;
  const [xr, yr, zr, mirror] = CUBE_TRANSFORMS[CUBE_STARTS.indexOf(start) * 6 + CUBE_STYLES.indexOf(style)];
  let [width, height, depth] = [w0, h0, d0];
  if (Math.abs(zr) === 1) [width, height] = [height, width];
  if (Math.abs(yr) === 1) [width, depth] = [depth, width];
  if (Math.abs(xr) === 1) [height, depth] = [depth, height];
  const total = Math.min(width * height * depth, MAX_POINTS);
  const out: [number, number, number][] = [];
  for (let i = 0; i < total; i++) {
    const z = Math.floor(i / (width * height));
    const base = i % (width * height);
    let y = Math.floor(base / width);
    let x: number;
    if ((strand === 1 || y % 2 === 0) && strand !== 2) x = base % width;
    else if (strand === 2) {
      const pos = (base % width) + 1;
      x = pos <= Math.floor((width + 1) / 2) ? 2 * (pos - 1) : (width - pos) * 2 + 1;
    } else x = width - (base % width) - 1;
    if (!perLayer && z % 2 !== 0) {
      y = height - y - 1;
      if (height % 2 !== 0 && strand === 0) x = width - x - 1;
    }
    let [px, py, pz] = [x, y, z];
    let [w, h, d] = [width, height, depth];
    for (let k = 0; k < Math.abs(xr); k++) {
      if (xr > 0) [py, pz] = [d - pz - 1, py];
      else [pz, py] = [h - py - 1, pz];
      [h, d] = [d, h];
    }
    for (let k = 0; k < Math.abs(yr); k++) {
      if (yr > 0) [pz, px] = [w - px - 1, pz];
      else [px, pz] = [d - pz - 1, px];
      [w, d] = [d, w];
    }
    for (let k = 0; k < Math.abs(zr); k++) {
      if (zr > 0) [px, py] = [py, w - px - 1];
      else [px, py] = [h - py - 1, px];
      [w, h] = [h, w];
    }
    if (mirror > 0) px = w - px - 1;
    out.push([px, py, pz]);
  }
  return out;
}

/** A cube's pixels `spacing` apart, centered, the front layer toward +z. */
function cube(g: Extract<Generator, { type: "cube" }>): Vec3[] {
  const mid = (n: number) => (n - 1) / 2;
  const cells = cubeCells(g.width, g.height, g.depth, g.start ?? "frontBottomLeft", g.style ?? "verticalFrontBack", g.strandStyle ?? "zigZag", g.strandPerLayer ?? false);
  return cells.map(([x, y, z]) => v((x - mid(g.width)) * g.spacing, (y - mid(g.height)) * g.spacing, (mid(g.depth) - z) * g.spacing));
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
