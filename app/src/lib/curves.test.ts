import { describe, expect, it } from "vitest";
import type { Curve, Effect } from "../api/sequence";
import { curveValue, effectAt, shapeChoice, startCurve, withCurve, withShape } from "./curves";

describe("curves", () => {
  it("read the way the engine reads them", () => {
    const ramp: Curve = { shape: "ramp", from: 10, to: 20 };
    expect([0, 0.25, 1].map((t) => curveValue(ramp, t))).toEqual([10, 12.5, 20]);
    const sine: Curve = { shape: "sine", from: 0, to: 1, cycles: 2 };
    expect(curveValue(sine, 0)).toBeCloseTo(0);
    expect(curveValue(sine, 0.25)).toBeCloseTo(1);
    expect(curveValue(sine, 0.125)).toBeCloseTo(0.5);
    const square: Curve = { shape: "square", from: 2, to: 8, cycles: 2 };
    expect([0, 0.2, 0.25, 0.49, 0.5, 0.8].map((t) => curveValue(square, t))).toEqual([2, 2, 8, 8, 2, 8]);
    const saw: Curve = { shape: "saw", from: 0, to: 10, cycles: 4 };
    expect(curveValue(saw, 0.125)).toBeCloseTo(5);
    const custom: Curve = { shape: "custom", from: 0, to: 100, points: [[0.2, 0], [0.5, 1], [0.5, 0.25], [1, 0.25]] };
    expect(curveValue(custom, 0)).toBe(0);
    expect(curveValue(custom, 0.35)).toBeCloseTo(50);
    expect(curveValue(custom, 0.5)).toBe(25);
  });

  it("put each curve's value in its setting as the effect plays", () => {
    const effect: Effect = {
      id: "e",
      startMs: 1000,
      endMs: 3000,
      params: { kind: "chase", speed: 1, bands: 2 },
      palette: { colors: [] },
      blend: "normal",
      fadeInMs: 0,
      fadeOutMs: 0,
      curves: { speed: { shape: "ramp", from: 0, to: 10 }, blur: { shape: "ramp", from: 0, to: 14 } },
    };
    const now = effectAt(effect, 1500);
    expect(now.params).toMatchObject({ speed: 2.5, bands: 2 });
    expect(now.blur).toBe(4);
    const plain = { ...effect, curves: undefined };
    expect(effectAt(plain, 1500), "no curves, no copy").toBe(plain);
  });

  it("change shape keeping their values, ramps up or down by which end is higher", () => {
    const up: Curve = { shape: "ramp", from: 2, to: 8 };
    expect(shapeChoice(up)).toBe("rampUp");
    expect(withShape(up, "rampDown")).toEqual({ shape: "ramp", from: 8, to: 2 });
    expect(shapeChoice(withShape(up, "rampDown"))).toBe("rampDown");
    expect(withShape(up, "sine")).toEqual({ shape: "sine", from: 2, to: 8 });
    expect(withShape(up, "custom")).toEqual({ shape: "custom", from: 2, to: 8, points: [[0, 0], [1, 1]] });
    // A shape turned custom starts out the same.
    const square = withShape({ shape: "square", from: 0, to: 1, cycles: 1 }, "custom");
    expect(square.points?.[4]).toEqual([0.25, 0]);
    expect(square.points?.[12]).toEqual([0.75, 1]);
  });

  it("start toward the far end of the range, and drop out when turned off", () => {
    expect(startCurve(0.8, 0, 1)).toEqual({ shape: "ramp", from: 0.8, to: 0 });
    expect(startCurve(2, 0, 50)).toEqual({ shape: "ramp", from: 2, to: 50 });
    const curves = withCurve(undefined, "speed", { shape: "ramp", from: 0, to: 1 });
    expect(Object.keys(curves!)).toEqual(["speed"]);
    expect(withCurve(curves, "speed", null)).toBeUndefined();
  });
});
