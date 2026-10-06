import { describe, expect, it } from "vitest";
import { emptyShow } from "../api/memory";
import type { Prop, Vec3 } from "../api/types";
import { frontView, localPositions } from "./geometry";
import {
  type PolyShape,
  addBend,
  addPoint,
  asPoly,
  bendSegment,
  finishDraft,
  insertVertex,
  joinLines,
  lineEnds,
  localAt,
  moveControl,
  moveVertex,
  placePoint,
  polyHandles,
  removeLastPoint,
  removeVertex,
  segmentMiddle,
  setSegmentNodes,
  setSpread,
  splitAt,
  straighten,
  toLocal,
} from "./polylineMath";
import { newProp, nodeCount } from "./shows";
import type { Pt } from "./layoutMath";
import { guideIndex, snapAlong, snapPointTo } from "./smartGuides";

const v = (x: number, y: number, z = 0): Vec3 => ({ x, y, z });
const near = (a: { x: number; y: number }, b: { x: number; y: number }) => {
  expect(a.x).toBeCloseTo(b.x, 3);
  expect(a.y).toBeCloseTo(b.y, 3);
};

function poly(points: [number, number][], nodes = 10): PolyShape {
  return {
    source: "generator",
    type: "polyLine",
    vertices: points.map(([x, y]) => v(x, y)),
    segments: points.slice(1).map(() => ({ nodes })),
  };
}

function prop(shape: Prop["shape"], at: [number, number] = [0, 0], deg = 0, name = "P"): Prop {
  const p = { ...newProp("polyLine", emptyShow("x")), name, shape };
  p.transform.position = { x: at[0], y: at[1], z: 0 };
  p.transform.rotationDeg.z = deg;
  return p;
}

const line = (length: number, nodes: number, at: [number, number], deg = 0) =>
  prop({ source: "generator", type: "line", nodes, length }, at, deg, "Line");

describe("drawing a poly line", () => {
  const base = newProp("polyLine", emptyShow("x"));

  it("adds a point per click, ignores the second click of a double-click, and Backspace takes the last off", () => {
    let d = addPoint({ points: [] }, { x: 1, y: 1 });
    d = addPoint(d, { x: 3, y: 1 });
    d = addPoint(d, { x: 3, y: 1 });
    expect(d.points).toHaveLength(2);
    d = addPoint(d, { x: 3, y: 4 });
    expect(removeLastPoint(d).points).toEqual([
      { x: 1, y: 1 },
      { x: 3, y: 1 },
    ]);
  });

  it("finishes as a prop whose origin is the first point, with pixels for each stretch's length", () => {
    const made = finishDraft({ points: [{ x: 1, y: 1 }, { x: 3, y: 1 }, { x: 3, y: 4 }] }, base)!;
    expect(made.transform.position).toEqual({ x: 1, y: 1, z: 0 });
    expect(made.shape).toEqual({
      source: "generator",
      type: "polyLine",
      vertices: [v(0, 0), v(2, 0), v(2, 3)],
      segments: [{ nodes: 20 }, { nodes: 30 }],
    });
    const pixels = frontView(made);
    expect(pixels.length / 2).toBe(50);
    near({ x: pixels[0], y: pixels[1] }, { x: 1.05, y: 1 });
    expect(finishDraft({ points: [{ x: 1, y: 1 }] }, base)).toBeNull();
    expect(finishDraft({ points: [{ x: 1, y: 1 }, { x: 1, y: 1 }] }, base)).toBeNull();
  });

  it("joins onto a line end nearby, else keeps to 45° with Shift, else snaps to the grid", () => {
    const ends = lineEnds([line(4, 40, [0, 0])]);
    expect(ends.map((e) => [e.end, e.at])).toEqual([
      ["start", { x: -2, y: 0 }],
      ["end", { x: 2, y: 0 }],
    ]);
    const opts = { from: { x: 5, y: 5 }, straight: true, grid: 0.5, ends, radius: 0.2 };
    const joined = placePoint({ x: 2.1, y: 0.1 }, opts);
    expect(joined.at).toEqual({ x: 2, y: 0 });
    expect(joined.join).toMatchObject({ end: "end" });
    near(placePoint({ x: 9, y: 5.4 }, opts).at, { x: 9, y: 5 });
    expect(placePoint({ x: 9.2, y: 5.4 }, { ...opts, straight: false })).toEqual({ at: { x: 9, y: 5.5 }, join: null });
    expect(placePoint({ x: 9.2, y: 5.4 }, { ...opts, straight: false, grid: null }).at).toEqual({ x: 9.2, y: 5.4 });
  });

  it("snaps to smart guides when it joins no line end: along its 45° line with Shift, else on either axis", () => {
    const ends = lineEnds([line(4, 40, [0, 0])]);
    // A guide at x = 3 and y = 2 (another prop's edges), within 0.25.
    const index = guideIndex([{ minX: 3, maxX: 6, minY: 2, maxY: 4 }]);
    const guides = {
      point: (p: Pt, fallback: Pt) => snapPointTo(index, p, { threshold: 0.25, fallback }).point,
      along: (from: Pt, to: Pt) => snapAlong(index, from, to, { threshold: 0.25 }).point,
    };
    const opts = { from: { x: 0, y: 2 }, straight: false, grid: 0.5, ends, radius: 0.2, guides };
    // Near both guides: on both.
    expect(placePoint({ x: 2.9, y: 2.1 }, opts)).toEqual({ at: { x: 3, y: 2 }, join: null });
    // Near the x guide only: the grid takes y.
    expect(placePoint({ x: 3.1, y: 7.2 }, opts)).toEqual({ at: { x: 3, y: 7 }, join: null });
    // With Shift, level from (0, 2), slid along to x = 3.
    near(placePoint({ x: 2.8, y: 2.3 }, { ...opts, straight: true }).at, { x: 3, y: 2 });
    // Diagonal from (0, 0): slides along the 45° line to x = 3 (and so y = 3).
    near(placePoint({ x: 2.85, y: 2.9 }, { ...opts, from: { x: 0, y: 0 }, straight: true }).at, { x: 3, y: 3 });
    // A line end within reach wins over a guide that is nearer.
    const joined = placePoint({ x: 2.15, y: 0.05 }, { ...opts, guides: { point: () => ({ x: 9, y: 9 }), along: () => ({ x: 9, y: 9 }) } });
    expect(joined).toMatchObject({ at: { x: 2, y: 0 }, join: { end: "end" } });
    // No guides: as before.
    expect(placePoint({ x: 2.9, y: 2.1 }, { ...opts, guides: null }).at).toEqual({ x: 3, y: 2 });
    expect(placePoint({ x: 3.2, y: 2.2 }, { ...opts, guides: null }).at).toEqual({ x: 3, y: 2 });
    expect(placePoint({ x: 3.1, y: 2.4 }, { ...opts, guides: null, grid: null }).at).toEqual({ x: 3.1, y: 2.4 });
  });
});

describe("editing a poly line's points", () => {
  it("moves a point and the curve controls beside it", () => {
    const bent = bendSegment(poly([[0, 0], [2, 0], [4, 0]]), 0, v(1, 1));
    const moved = moveVertex(bent, 1, v(2, 1));
    expect(moved.vertices[1]).toEqual(v(2, 1));
    expect(moved.segments[0].curve![1]).toEqual({ ...bent.segments[0].curve![1], y: tidy3(bent.segments[0].curve![1].y + 1) });
    expect(moved.segments[0].curve![0]).toEqual(bent.segments[0].curve![0]);
  });

  it("adds a point in the middle of a stretch, sharing its pixels", () => {
    const s = insertVertex(poly([[0, 0], [4, 0]], 9), 0);
    expect(s.vertices).toEqual([v(0, 0), v(2, 0), v(4, 0)]);
    // The second half gets the odd pixel, as xLights does.
    expect(s.segments).toEqual([{ nodes: 4 }, { nodes: 5 }]);
    expect(nodeCount(s)).toBe(9);
  });

  it("cuts a curve exactly in two, so the line keeps its shape", () => {
    const curved = bendSegment(poly([[0, 0], [4, 0]], 8), 0, v(2, 2));
    near(segmentMiddle(curved, 0), v(2, 2));
    const split = insertVertex(curved, 0);
    near(split.vertices[1], v(2, 2));
    // Same curve: pixels spread evenly along both land in the same places (within how finely
    // curves are measured).
    const after = localPositions({ ...split, spreadNodes: 8 });
    const before = localPositions({ ...curved, spreadNodes: 8 });
    after.forEach((p, i) => expect(Math.hypot(p.x - before[i].x, p.y - before[i].y)).toBeLessThan(5e-3));
  });

  it("removes a point: an end takes its stretch, a middle point joins two stretches straight", () => {
    const s = bendSegment(poly([[0, 0], [2, 0], [4, 0], [6, 0]], 5), 1, v(3, 1));
    expect(removeVertex(s, 0)!.segments).toHaveLength(2);
    expect(removeVertex(s, 3)!.vertices).toEqual([v(0, 0), v(2, 0), v(4, 0)]);
    const middle = removeVertex(s, 2)!;
    expect(middle.vertices).toEqual([v(0, 0), v(2, 0), v(6, 0)]);
    expect(middle.segments).toEqual([{ nodes: 5 }, { nodes: 10 }]);
    expect(removeVertex(poly([[0, 0], [1, 0]]), 0)).toBeNull();
  });

  it("bends a stretch through a point, moves its controls, and straightens it", () => {
    const bent = bendSegment(poly([[0, 0], [3, 0]]), 0, v(1.5, 1.5));
    expect(bent.segments[0].curve).toEqual([v(1, 2), v(2, 2)]);
    const pulled = moveControl(bent, 0, 1, v(3, 3));
    expect(pulled.segments[0].curve).toEqual([v(1, 2), v(3, 3)]);
    expect(straighten(pulled, 0).segments[0]).toEqual({ nodes: 10 });
  });

  it("switches between per-stretch counts and pixels spread evenly, keeping the total", () => {
    const s = setSegmentNodes(poly([[0, 0], [1, 0], [4, 0]], 10), 1, 30);
    const spread = setSpread(s, true);
    expect(spread.spreadNodes).toBe(40);
    const back = setSpread({ ...spread, spreadNodes: 41 }, false);
    expect(back.segments.map((x) => x.nodes)).toEqual([10, 31]);
    expect(back.spreadNodes).toBeUndefined();
  });

  it("maps canvas points to the prop's own points through its turn and scale", () => {
    const t = { position: v(10, 5), rotationDeg: v(0, 0, 90), scale: v(2, 1, 1) };
    near(toLocal(t, v(10, 7)), v(1, 0));
    near(localAt(t, v(0, 0), { x: 9, y: 5 }), v(0, 1));
  });

  it("puts handles on the points, the middles, and the curve controls", () => {
    const p = prop(bendSegment(poly([[0, 0], [2, 0], [2, 2]]), 1, v(3, 1)), [1, 1]);
    const h = polyHandles(p)!;
    expect(h.vertices).toEqual([
      { x: 1, y: 1 },
      { x: 3, y: 1 },
      { x: 3, y: 3 },
    ]);
    near(h.middles[0], { x: 2, y: 1 });
    near(h.middles[1], { x: 4, y: 2 });
    expect(h.controls.map((c) => [c.segment, c.which])).toEqual([
      [1, 0],
      [1, 1],
    ]);
  });
});

describe("lines into poly lines, joined and split", () => {
  it("adds a bend to a line, keeping its place and pixel count", () => {
    const l = line(4, 40, [5, 2], 30);
    const bent = addBend(l);
    expect(bent.transform).toEqual(l.transform);
    expect(bent.shape).toMatchObject({ type: "polyLine", vertices: [v(-2, 0), v(0, 0), v(2, 0)] });
    expect(nodeCount(bent.shape)).toBe(40);
    expect(asPoly({ source: "generator", type: "arch", nodes: 3, width: 1, height: 1 })).toBeNull();
  });

  it("joins a line onto the end of another, in the first line's direction", () => {
    const a = prop(poly([[0, 0], [2, 0]], 4), [0, 0]);
    const b = line(2, 6, [2, 1], 90); // from (2, 0) up to (2, 2)
    const joined = joinLines(a, b, 0.05)!;
    expect(joined).toMatchObject({ kept: a.id, removed: b.id, first: a.id, reversed: null });
    expect(joined.prop.id).toBe(a.id);
    const shape = joined.prop.shape as PolyShape;
    shape.vertices.forEach((p, i) => near(p, [v(0, 0), v(2, 0), v(2, 2)][i]));
    expect(shape.segments.map((s) => s.nodes)).toEqual([4, 6]);
  });

  it("puts the line whose end touches first, and keeps its wiring unless told otherwise", () => {
    const a = prop(poly([[0, 0], [2, 0]], 4), [3, 0]); // from (3, 0) to (5, 0)
    const b = prop(poly([[0, 0], [3, 0]], 6), [0, 0]); // from (0, 0) to (3, 0): it leads into a
    a.regions = [{ id: "ra", name: "Tip", kind: "nodes", lines: [[{ first: 0, last: 1 }]], layout: "horizontal", buffer: "default" }];
    b.regions = [{ id: "rb", name: "Tip", kind: "nodes", lines: [[{ first: 5, last: 4 }]], layout: "horizontal", buffer: "default" }];
    const joined = joinLines(a, b, 0.05)!;
    expect(joined).toMatchObject({ kept: b.id, removed: a.id, first: b.id, reversed: null, dropped: [] });
    expect(joined.prop.id).toBe(b.id);
    // b's pixels stay where they were; a's come after them, and its submodel follows them.
    expect(joined.prop.regions).toEqual([
      b.regions[0],
      { ...a.regions[0], name: "Tip (2)", lines: [[{ first: 6, last: 7 }]] },
    ]);
    const keepA = joinLines(a, b, 0.05, a.id)!;
    expect(keepA).toMatchObject({ kept: a.id, removed: b.id, first: b.id });
    expect(keepA.prop.transform).toEqual(a.transform);
    expect(keepA.prop.regions.map((r) => r.name)).toEqual(["Tip", "Tip (2)"]);
    expect(polyHandles(keepA.prop)!.vertices.map((p) => p.x)).toEqual(polyHandles(joined.prop)!.vertices.map((p) => p.x));
  });

  it("turns the second line round when both start (or both end) at the join, its submodels too", () => {
    const a = prop(poly([[0, 0], [2, 0]], 4), [5, 5], 180); // from (5, 5) to (3, 5)
    const b = prop(poly([[0, 0], [1, 0]], 3), [5, 5], 90); // from (5, 5) up to (5, 6)
    b.regions = [{ id: "rb", name: "Bottom", kind: "nodes", lines: [[{ first: 0, last: 0 }]], layout: "horizontal", buffer: "default" }];
    const joined = joinLines(a, b, 0.05)!;
    expect(joined).toMatchObject({ kept: b.id, first: b.id, reversed: b.id });
    expect(joined.prop.regions[0]).toMatchObject({ lines: [[{ first: 2, last: 2 }]] });
    const points = polyHandles(joined.prop)!.vertices;
    [{ x: 5, y: 6 }, { x: 5, y: 5 }, { x: 3, y: 5 }].forEach((p, i) => near(points[i], p));
    expect((joined.prop.shape as PolyShape).segments.map((s) => s.nodes)).toEqual([3, 4]);
  });

  it("won't join lines whose ends are apart, or shapes that aren't lines", () => {
    const a = prop(poly([[0, 0], [2, 0]]), [0, 0]);
    expect(joinLines(a, prop(poly([[0, 0], [1, 0]]), [3, 0]), 0.05)).toBeNull();
    const arch = { ...a, id: "arch", shape: { source: "generator" as const, type: "arch" as const, nodes: 3, width: 1, height: 1 } };
    expect(joinLines(a, arch, 10)).toBeNull();
  });

  it("splits at a point: the first part keeps the prop, the second is new", () => {
    const p = prop(setSpread(poly([[0, 0], [1, 0], [2, 0], [3, 0]], 10), true), [1, 0]);
    p.regions = [
      { id: "r", name: "Left", kind: "subBuffer", x1: 0, y1: 0, x2: 50, y2: 100 },
      { id: "s", name: "Middle", kind: "nodes", lines: [[{ first: 8, last: 12 }]], layout: "horizontal", buffer: "default" },
      { id: "t", name: "End", kind: "nodes", lines: [[{ first: 29, last: 25 }]], layout: "horizontal", buffer: "default" },
    ];
    const [first, second] = splitAt(p, 1, "new", "P 2")!;
    expect(first.id).toBe(p.id);
    expect((first.shape as PolyShape).vertices).toEqual([v(0, 0), v(1, 0)]);
    // Each part keeps the submodel pixels on it, counted from its own start.
    expect(first.regions.map((r) => r.name)).toEqual(["Left", "Middle"]);
    expect(first.regions[1]).toMatchObject({ lines: [[{ first: 8, last: 9 }]] });
    expect(second).toMatchObject({ id: "new", name: "P 2", transform: p.transform });
    expect(second.regions.map((r) => r.name)).toEqual(["Middle", "End"]);
    expect(second.regions[1]).toMatchObject({ lines: [[{ first: 19, last: 15 }]] });
    expect(second.regions.every((r) => !["r", "s", "t"].includes(r.id))).toBe(true);
    expect((second.shape as PolyShape).vertices).toEqual([v(1, 0), v(2, 0), v(3, 0)]);
    expect(nodeCount(first.shape) + nodeCount(second.shape)).toBe(30);
    expect(splitAt(p, 0, "x", "x")).toBeNull();
    expect(splitAt(p, 3, "x", "x")).toBeNull();
  });
});

const tidy3 = (n: number) => Math.round(n * 1000) / 1000;
