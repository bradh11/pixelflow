import { describe, expect, it } from "vitest";
import type { ShapeSource, Transform, Vec3 } from "../api/types";
import { applyTransform, frontView, localPositions } from "./geometry";
import { newProp, nodeCount, PROP_KINDS } from "./shows";
import { emptyShow } from "../api/memory";
import sharedShapes from "../../../crates/pf-geometry/tests/fixtures/shapes.json";

const close = (a: Vec3, b: Partial<Vec3>) => {
  expect(a.x).toBeCloseTo(b.x ?? 0, 4);
  expect(a.y).toBeCloseTo(b.y ?? 0, 4);
  expect(a.z).toBeCloseTo(b.z ?? 0, 4);
};

const identity: Transform = { position: { x: 0, y: 0, z: 0 }, rotationDeg: { x: 0, y: 0, z: 0 }, scale: { x: 1, y: 1, z: 1 } };

describe("localPositions (mirrors pf-geometry)", () => {
  it("gives every default prop exactly its pixel count", () => {
    for (const { kind } of PROP_KINDS) {
      const prop = newProp(kind, emptyShow("x"));
      expect(localPositions(prop.shape)).toHaveLength(nodeCount(prop.shape));
    }
  });

  it("runs a line along X, centered", () => {
    const p = localPositions({ source: "generator", type: "line", nodes: 5, length: 4 });
    close(p[0], { x: -2 });
    close(p[2], {});
    close(p[4], { x: 2 });
  });

  it("runs an arch from the left base over the top to the right base", () => {
    const p = localPositions({ source: "generator", type: "arch", nodes: 3, width: 4, height: 2 });
    close(p[0], { x: -2 });
    close(p[1], { y: 2 });
    close(p[2], { x: 2 });
  });

  it("starts a circle at the top and runs clockwise", () => {
    const p = localPositions({ source: "generator", type: "circle", nodes: 4, radius: 1 });
    close(p[0], { y: 1 });
    close(p[1], { x: 1 });
  });

  it("wires a matrix from its start corner, zig-zagging", () => {
    const shape: ShapeSource = {
      source: "generator",
      type: "matrix",
      columns: 3,
      rows: 2,
      width: 4,
      height: 2,
      wiring: { start: "bottomLeft", orientation: "horizontal", serpentine: true },
    };
    const p = localPositions(shape);
    close(p[0], { x: -2, y: -1 });
    close(p[2], { x: 2, y: -1 });
    close(p[3], { x: 2, y: 1 });
  });

  it("runs tree strings base to top, tapering", () => {
    const p = localPositions({
      source: "generator",
      type: "tree",
      strings: 4,
      nodesPerString: 3,
      height: 6,
      baseRadius: 2,
      topRadius: 0,
    });
    close(p[0], { z: 2 });
    close(p[2], { y: 6 });
    close(p[3], { x: 2 });
  });

  it("starts a star at its top tip", () => {
    const p = localPositions({ source: "generator", type: "star", points: 5, nodes: 100, outerRadius: 2, innerRadius: 1 });
    close(p[0], { y: 2 });
    expect(p.every((q) => Math.hypot(q.x, q.y) <= 2 + 1e-6)).toBe(true);
  });

  it("places custom grid nodes by number, top row highest", () => {
    const p = localPositions({ source: "generator", type: "customGrid", columns: 3, rows: 2, cells: [1, 0, 2, 0, 3, 0] });
    expect(p).toEqual([
      { x: -1, y: 0.5, z: 0 },
      { x: 1, y: 0.5, z: 0 },
      { x: 0, y: -0.5, z: 0 },
    ]);
  });

  it("uses measured points as they are", () => {
    const points = [{ x: 1, y: 2, z: 3 }];
    expect(localPositions({ source: "measured", points, provenance: "import" })).toBe(points);
  });
});

describe("the shapes pf-geometry checks itself against (crates/pf-geometry/tests/fixtures/shapes.json)", () => {
  for (const { name, shape, positions } of sharedShapes as { name: string; shape: ShapeSource; positions: number[][] }[]) {
    it(`puts the pixels of "${name}" where the engine does`, () => {
      const ours = localPositions(shape);
      expect(ours).toHaveLength(positions.length);
      expect(nodeCount(shape)).toBe(positions.length);
      ours.forEach((p, i) => {
        const [x, y, z] = positions[i];
        expect(Math.max(Math.abs(p.x - x), Math.abs(p.y - y), Math.abs(p.z - z)), `pixel ${i}`).toBeLessThan(1e-4);
      });
    });
  }
});

describe("applyTransform", () => {
  it("scales, then rotates, then moves", () => {
    const t: Transform = { position: { x: 10, y: 0, z: 0 }, rotationDeg: { x: 0, y: 0, z: 90 }, scale: { x: 2, y: 1, z: 1 } };
    close(applyTransform({ x: 1, y: 0, z: 0 }, t), { x: 10, y: 2 });
    close(applyTransform({ x: 1, y: 0, z: 0 }, { ...identity, rotationDeg: { x: 0, y: 90, z: 0 } }), { z: -1 });
  });

  it("gives a prop's front view as x, y pairs", () => {
    const prop = { ...newProp("line", emptyShow("x")), transform: { ...identity, position: { x: 1, y: 2, z: 0 } } };
    prop.shape = { source: "generator", type: "line", nodes: 2, length: 2 };
    expect(frontView(prop)).toEqual([0, 2, 2, 2]);
  });
});
