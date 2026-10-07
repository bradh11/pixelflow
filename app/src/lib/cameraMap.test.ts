import { describe, expect, it } from "vitest";
import type { CameraMapPlan, Prop, PropPlan } from "../api/types";
import { emptyShow } from "../api/memory";
import { frontView } from "./geometry";
import { canFit, correctedColorOrder, defaultChoice, describeAnomaly, placementEdits, sequenceSeconds, slotCount, symbolAt } from "./cameraMap";
import { newProp } from "./shows";

describe("the camera-mapping sequence", () => {
  it("matches the engine's slots and codes", () => {
    // Same cases as pf-camera-map's code tests: 100 pixels → 4 digits, 16 slots, 8 seconds.
    expect(slotCount(100, "four")).toBe(16);
    expect(sequenceSeconds(100, "four")).toBe(8);
    expect(slotCount(1000, "two")).toBe(6 + 3 + 10 + 2 + 1);
    expect([0, 1, 2, 3, 4, 5].map((s) => symbolAt(100, "four", s, 5))).toEqual(["off", "white", "off", "white", "white", "off"]);
    expect([6, 7, 8].map((s) => symbolAt(100, "four", s, 5))).toEqual(["red", "green", "blue"]);
    // Pixel 5 is number 6: digits 2, 1, 0, 0 (least first).
    expect([9, 10, 11, 12].map((s) => symbolAt(100, "four", s, 5))).toEqual(["green", "red", "off", "off"]);
    expect(symbolAt(100, "four", 15, 5)).toBe("off");
    expect([9, 10, 11].map((s) => symbolAt(100, "two", s, 5))).toEqual(["off", "white", "white"]);
  });

  it("works out the colour order a strip really has", () => {
    expect(correctedColorOrder("RGB", [1, 0, 2])).toBe("GRB");
    expect(correctedColorOrder("GRB", [1, 0, 2])).toBe("RGB");
    expect(correctedColorOrder("RGBW", [2, 1, 0])).toBe("BGRW");
  });

  it("says what each anomaly means", () => {
    const names = ["Arch", "Tree"];
    expect(describeAnomaly({ kind: "missing", prop: 0, ranges: [[3, 3], [9, 11]] }, names)).toMatch(/^Arch: 4 pixels 4, 10–12 never lit up/);
    expect(describeAnomaly({ kind: "colorOrder", prop: 1, configured: "RGB", suggested: "GRB" }, names)).toMatch(/looks like GRB, not RGB/);
    expect(describeAnomaly({ kind: "duplicate", prop: 1, node: 0, x: 1, y: 2 }, names)).toMatch(/pixel 1 was seen twice/);
    expect(describeAnomaly({ kind: "unreadable", count: 1 }, names)).toMatch(/^1 lit spot didn't/);
  });
});

describe("placing props from a capture", () => {
  const line = (): Prop => ({
    ...newProp("line", emptyShow("t")),
    id: "p1",
    transform: { position: { x: 1, y: 2, z: 0.5 }, rotationDeg: { x: 0, y: 0, z: 10 }, scale: { x: 1, y: 1, z: 1 } },
  });
  const result = (prop: Prop, plan: PropPlan): CameraMapPlan => ({
    props: [{ prop: prop.id, name: prop.name, nodes: plan.nodes }],
    plan: { alignment: null, alignmentError: 0, props: [plan], anomalies: [] },
  });

  it("sets a measured shape around the pixels' centre", () => {
    const prop = line();
    const nodes = frontView(prop).length / 2;
    const points: [number, number][] = Array.from({ length: nodes }, (_, i) => [i, i % 2 === 0 ? 0 : 0.5]);
    const plan: PropPlan = { nodes, found: nodes, points, measured: points.map(() => true), fit: { scale: 1, rotationDeg: 0, tx: 0, ty: 0, error: 0.2, fits: false } };
    const show = { ...emptyShow("t"), props: [prop] };
    expect(defaultChoice(prop, plan)).toBe("measured");
    const [edit] = placementEdits(show, result(prop, plan), { p1: "measured" });
    if (edit.type !== "updateProp") throw new Error(edit.type);
    expect(edit.prop.shape.source).toBe("measured");
    const placed = frontView(edit.prop);
    points.forEach(([x, y], i) => {
      expect(placed[2 * i]).toBeCloseTo(x);
      expect(placed[2 * i + 1]).toBeCloseTo(y);
    });
    expect(edit.prop.transform.position.z).toBe(0.5);
    expect(placementEdits(show, result(prop, plan), { p1: "skip" })).toEqual([]);
  });

  it("keeps a fitting shape and moves it onto the pixels", () => {
    const prop = line();
    const before = frontView(prop);
    const nodes = before.length / 2;
    const fit = { scale: 1.5, rotationDeg: 20, tx: -3, ty: 4, error: 0.01, fits: true };
    const plan: PropPlan = { nodes, found: nodes, points: Array.from({ length: nodes }, () => [0, 0]), measured: [], fit };
    expect(canFit(prop, plan)).toBe(true);
    expect(defaultChoice(prop, plan)).toBe("fit");
    const [edit] = placementEdits({ ...emptyShow("t"), props: [prop] }, result(prop, plan), { p1: "fit" });
    if (edit.type !== "updateProp") throw new Error(edit.type);
    expect(edit.prop.shape).toEqual(prop.shape);
    const after = frontView(edit.prop);
    const a = (20 * Math.PI) / 180;
    for (let i = 0; i < nodes; i++) {
      const [x, y] = [before[2 * i], before[2 * i + 1]];
      expect(after[2 * i]).toBeCloseTo(1.5 * (Math.cos(a) * x - Math.sin(a) * y) - 3);
      expect(after[2 * i + 1]).toBeCloseTo(1.5 * (Math.sin(a) * x + Math.cos(a) * y) + 4);
    }
    const tilted = { ...prop, transform: { ...prop.transform, rotationDeg: { x: 15, y: 0, z: 0 } } };
    expect(canFit(tilted, plan)).toBe(false);
  });
});
