import { describe, expect, it } from "vitest";
import type { PreviewProp, PreviewProp3d } from "../api/types";
import { BOOST_STEPS, VIEW_DEFAULTS, boostFrame, boostTable, hidesUnlit, readViewOptions } from "./viewOptions";

describe("the view options", () => {
  it("start as the previews have always looked: the grid with the ground, no names, dim pixels that are off, medium dots, no boost", () => {
    expect(readViewOptions(undefined)).toEqual(VIEW_DEFAULTS);
    expect(readViewOptions({})).toEqual({ grid: true, names: false, unlit: "dim", dotSize: "medium", boost: 0 });
  });

  it("read back what was chosen, and the usual for anything not understood", () => {
    expect(readViewOptions({ grid: false, names: true, unlit: "auto", dotSize: "large", boost: 3 })).toEqual({ grid: false, names: true, unlit: "auto", dotSize: "large", boost: 3 });
    expect(readViewOptions({ grid: 0, names: "yes", unlit: "off", dotSize: "huge", boost: 9 })).toEqual(VIEW_DEFAULTS);
    expect(readViewOptions({ boost: 1.5 }).boost).toBe(0);
    expect(readViewOptions({ boost: -1 }).boost).toBe(0);
    expect(readViewOptions("dim")).toEqual(VIEW_DEFAULTS);
  });

  it("leave pixels that are off out when hidden, never when dim, and with Auto only while the show plays", () => {
    expect([hidesUnlit("dim", false), hidesUnlit("dim", true)]).toEqual([false, false]);
    expect([hidesUnlit("hidden", false), hidesUnlit("hidden", true)]).toEqual([true, true]);
    expect([hidesUnlit("auto", false), hidesUnlit("auto", true)]).toEqual([false, true]);
  });
});

describe("Boost dim", () => {
  it("lifts dim levels the more the higher its step, keeping off off and full full", () => {
    for (let step = 1; step <= BOOST_STEPS; step++) {
      const table = boostTable(step);
      expect(table[0]).toBe(0);
      expect(table[255]).toBe(255);
      for (let m = 1; m < 255; m++) {
        expect(table[m]).toBeGreaterThanOrEqual(table[m - 1]);
        // Lifted, though near full there's little room left to lift into.
        if (m < 200) expect(table[m]).toBeGreaterThan(m);
        else expect(table[m]).toBeGreaterThanOrEqual(m);
        if (step > 1) expect(table[m]).toBeGreaterThanOrEqual(boostTable(step - 1)[m]);
      }
    }
    // A tenth of full: about a fifth at Low, a third at Medium, near half at High.
    expect([1, 2, 3].map((step) => boostTable(step)[26])).toEqual([54, 81, 113]);
    // The faintest level shows at every step.
    expect([1, 2, 3].map((step) => boostTable(step)[1])).toEqual([8, 16, 40]);
    // Off changes nothing.
    expect(Array.from(boostTable(0))).toEqual(Array.from({ length: 256 }, (_, m) => m));
  });

  const prop = (frameOffset: number, count: number, channelsPerPixel = 3): PreviewProp => ({
    prop: `p${frameOffset}`,
    frameOffset,
    channelsPerPixel,
    points: new Float32Array(count * 2),
  });

  it("lifts each pixel keeping its hue, in a copy: the frame itself is left as it is", () => {
    const frame = new Uint8Array([26, 13, 0, 0, 0, 0, 255, 128, 64, 2, 0, 1]);
    const boosted = boostFrame(frame, [prop(0, 4)], 2);
    expect(boosted).not.toBe(frame);
    expect(Array.from(frame)).toEqual([26, 13, 0, 0, 0, 0, 255, 128, 64, 2, 0, 1]);
    // Orange stays orange (half as much green as red), only brighter; off stays off; full stays as it is.
    expect(Array.from(boosted.subarray(0, 9))).toEqual([81, 41, 0, 0, 0, 0, 255, 128, 64]);
    expect(Array.from(boosted.subarray(9))).toEqual([boostTable(2)[2], 0, Math.floor((boostTable(2)[2] + 1) / 2)]);
  });

  it("is the frame itself when off", () => {
    const frame = new Uint8Array([10, 0, 0]);
    expect(boostFrame(frame, [prop(0, 1)], 0)).toBe(frame);
  });

  it("lifts only the props' colors: a fixture's motor channels between them, and a white channel, are left as they are", () => {
    // A head's cell (3 colors), its pan and tilt, then an RGBW string of two pixels.
    const frame = new Uint8Array([20, 0, 0, 40, 80, 10, 10, 10, 10, 0, 0, 0, 30]);
    const head: PreviewProp = { ...prop(0, 1), motion: { basis: [1, 0, 0, 0, 1, 0, 0, 0, 1], beam: { angleDeg: 20, length: 1 } } };
    const rgbw = prop(5, 2, 4);
    const boosted = boostFrame(frame, [head, rgbw], 1);
    expect(boosted[0]).toBe(boostTable(1)[20]);
    expect(Array.from(boosted.subarray(3, 5))).toEqual([40, 80]);
    const ten = boostTable(1)[10];
    expect(Array.from(boosted.subarray(5))).toEqual([ten, ten, ten, 10, 0, 0, 0, 30]);
  });

  it("lifts a pixel in two props once, and reads a 3D view's props as well", () => {
    const frame = new Uint8Array([50, 50, 50]);
    const flat = prop(0, 1);
    const deep: PreviewProp3d = { prop: "d", frameOffset: 0, channelsPerPixel: 3, xyz: new Float32Array(3) };
    const once = boostTable(3)[50];
    expect(Array.from(boostFrame(frame, [flat, flat], 3))).toEqual([once, once, once]);
    expect(Array.from(boostFrame(frame, [deep], 3))).toEqual([once, once, once]);
  });

  it("stops at the frame's end", () => {
    expect(Array.from(boostFrame(new Uint8Array([9, 9, 9, 9]), [prop(0, 3)], 2))).toEqual([boostTable(2)[9], boostTable(2)[9], boostTable(2)[9], 9]);
  });
});
