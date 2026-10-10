import { describe, expect, it } from "vitest";
import type { PreviewProp } from "../api/types";
import { type PixelContext, batchPixels, fillBatches, packColor, stampDots } from "./pixelBatches";

const COLORS = { unlit: "unlit", selected: "selected", dark: "dark" };
const SIZE = { width: 100, height: 100 };
// Screen = world × 10, centered: world (0, 0) is the canvas center.
const VIEW = { cx: 0, cy: 0, zoom: 10 };

const prop = (id: string, points: number[], frameOffset = 0): PreviewProp => ({ prop: id, frameOffset, channelsPerPixel: 3, points });

/** A context that only counts what it's asked to draw. */
function counter() {
  const calls = { fills: 0, arcs: 0, rects: 0, styles: [] as string[] };
  const ctx = {
    set fillStyle(v: string) {
      calls.styles.push(v);
    },
    beginPath() {},
    moveTo() {},
    arc() {
      calls.arcs++;
    },
    rect() {
      calls.rects++;
    },
    fill() {
      calls.fills++;
    },
  } as unknown as PixelContext;
  return { ctx, calls };
}

describe("drawing pixels by color", () => {
  it("groups unlit and selected props into one batch each, selected drawn last", () => {
    const batches = batchPixels([prop("a", [0, 0, 1, 1]), prop("b", [2, 2]), prop("c", [-1, -1])], null, VIEW, SIZE, new Set(["b"]), COLORS);
    expect(batches.map((b) => b.color)).toEqual(["unlit", "selected"]);
    expect(Array.from(batches[0].xy)).toEqual([50, 50, 60, 40, 40, 60]);
    expect(Array.from(batches[1].xy)).toEqual([70, 30]);
  });

  it("groups lit pixels by their color, rounded, and draws off pixels dark", () => {
    const frame = new Uint8Array([255, 0, 0, 0, 0, 0, 254, 1, 2, 0, 0, 255]);
    const batches = batchPixels([prop("a", [0, 0, 0.1, 0, 0.2, 0, 0.3, 0])], frame, VIEW, SIZE, new Set(), COLORS);
    const byColor = Object.fromEntries(batches.map((b) => [b.color, b.xy.length / 2]));
    expect(byColor).toEqual({ "rgb(255, 0, 0)": 2, dark: 1, "rgb(0, 0, 255)": 1 });
  });

  it("finds each prop's colors at its own place in the frame, and pixels past the frame's end are dark", () => {
    const frame = new Uint8Array([0, 0, 0, 0, 255, 0]);
    const batches = batchPixels([prop("a", [0, 0]), prop("b", [1, 0, 2, 0], 3)], frame, VIEW, SIZE, new Set(), COLORS);
    expect(Object.fromEntries(batches.map((b) => [b.color, b.xy.length / 2]))).toEqual({ dark: 2, "rgb(0, 255, 0)": 1 });
  });

  it("leaves out pixels that are off when there's no color for them", () => {
    const frame = new Uint8Array([255, 0, 0, 0, 0, 0]);
    const batches = batchPixels([prop("a", [0, 0, 1, 0])], frame, VIEW, SIZE, new Set(), { unlit: "unlit", selected: "selected" });
    expect(batches.map((b) => [b.color, b.xy.length / 2])).toEqual([["rgb(255, 0, 0)", 1]]);
    // With nothing playing, every pixel is still drawn.
    expect(batchPixels([prop("a", [0, 0, 1, 0])], null, VIEW, SIZE, new Set(), { unlit: "unlit", selected: "selected" })[0].xy.length / 2).toBe(2);
  });

  it("draws lit pixels dimmer when asked to (a glow adds the rest), and the others as they are", () => {
    const frame = new Uint8Array([255, 128, 0, 0, 0, 0]);
    const colors = (dim?: number) => batchPixels([prop("a", [0, 0, 1, 0])], frame, VIEW, SIZE, new Set(), COLORS, 0, dim).map((b) => b.color);
    expect(colors()).toEqual(["rgb(255, 132, 0)", "dark"]);
    expect(colors(1)).toEqual(colors());
    expect(colors(0.5)).toEqual(["rgb(132, 66, 0)", "dark"]);
    expect(batchPixels([prop("a", [0, 0])], null, VIEW, SIZE, new Set(), COLORS, 0, 0.5)[0].color).toBe("unlit");
  });

  it("skips pixels off the canvas, keeping ones within the margin", () => {
    const batches = batchPixels([prop("a", [0, 0, 5.2, 0, 9, 9])], null, VIEW, SIZE, new Set(), COLORS, 3);
    expect(batches[0].xy.length / 2).toBe(2);
  });

  it("fills once per color: dots when large, squares when tiny", () => {
    const frame = new Uint8Array(3 * 1000);
    for (let i = 0; i < frame.length; i += 3) frame[i + (i / 3) % 3] = 200;
    const points = Array.from({ length: 2000 }, (_, i) => (i % 2 === 0 ? (i / 2) * 0.004 - 2 : 0));
    const batches = batchPixels([prop("a", points)], frame, VIEW, SIZE, new Set(), COLORS);
    const { ctx, calls } = counter();
    fillBatches(ctx, batches, 3);
    expect(calls).toMatchObject({ fills: 3, arcs: 1000, rects: 0 });
    const tiny = counter();
    fillBatches(tiny.ctx, batches, 1);
    expect(tiny.calls).toMatchObject({ fills: 3, arcs: 0, rects: 1000 });
  });

  it("stamps round dots into an image, clipped at its edges", () => {
    const red = packColor("rgb(255, 0, 0)");
    const pixels = new Uint32Array(10 * 10);
    // Screen points at ratio 2: (1, 1) lands on device pixel (2, 2); (4.5, 0) on (9, 0), at the edge.
    stampDots(pixels, 10, 10, [{ color: "red", rgba: red, xy: new Float32Array([1, 1, 4.5, 0]) }], 0.5, 2);
    const lit = (x: number, y: number) => pixels[y * 10 + x] === red;
    expect([lit(2, 2), lit(1, 2), lit(3, 2), lit(2, 1), lit(2, 3)]).toEqual([true, true, true, true, true]);
    expect(lit(1, 1)).toBe(false);
    expect([lit(9, 0), lit(8, 0), lit(9, 1)]).toEqual([true, true, true]);
    expect(pixels.filter((p) => p === red)).toHaveLength(5 + 3);
  });

  it("packs CSS colors as image pixels", () => {
    const bytes = (n: number) => Array.from(new Uint8Array(new Uint32Array([n]).buffer));
    expect(bytes(packColor("#a78bfa"))).toEqual([0xa7, 0x8b, 0xfa, 255]);
    expect(bytes(packColor("rgba(220, 220, 220, 0.7)"))).toEqual([220, 220, 220, 179]);
    expect(bytes(packColor("rgb(1, 2, 3)"))).toEqual([1, 2, 3, 255]);
  });
});
