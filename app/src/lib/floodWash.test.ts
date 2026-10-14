import { describe, expect, it, vi } from "vitest";
import type { PreviewProp } from "../api/types";
import { FLOOD_RADIUS, FLOOD_STRENGTH, drawFloods, washAt, washRadius } from "./floodWash";

function fakeContext() {
  const stops: [number, string][] = [];
  const ctx = {
    save: vi.fn(),
    restore: vi.fn(),
    beginPath: vi.fn(),
    arc: vi.fn(),
    fill: vi.fn(),
    globalCompositeOperation: "source-over",
    fillStyle: "",
    createRadialGradient: vi.fn(() => ({ addColorStop: (t: number, c: string) => stops.push([t, c]) })),
  };
  return { ctx: ctx as unknown as CanvasRenderingContext2D, raw: ctx, stops };
}

const view = { cx: 0, cy: 0, zoom: 40 };
const size = { width: 400, height: 200 };
const flood = (flood: boolean): PreviewProp => ({ prop: "f", frameOffset: 0, channelsPerPixel: 3, points: [0, 0, 1, 0], flood });

describe("flood washes", () => {
  it("fall from full at the middle to nothing at the edge, as the video export's do", () => {
    expect(washAt(0)).toBe(1);
    expect(washAt(0.5)).toBeCloseTo(0.5625);
    expect(washAt(1)).toBe(0);
    expect(washRadius(view)).toBe(FLOOD_RADIUS * 40);
    expect(washRadius({ ...view, zoom: 1 })).toBe(12);
  });

  it("wash each lit cell of a flood in its color, and nothing for other props or dark cells", () => {
    const { ctx, raw, stops } = fakeContext();
    drawFloods(ctx, [flood(true), { ...flood(false), frameOffset: 6 }], new Uint8Array([255, 128, 0, 0, 0, 0, 9, 9, 9]), view, size, [90, 90, 90]);
    expect(raw.createRadialGradient).toHaveBeenCalledTimes(1);
    expect(raw.createRadialGradient).toHaveBeenCalledWith(200, 100, 0, 200, 100, 30);
    expect(stops[0]).toEqual([0, `rgba(255, 128, 0, ${FLOOD_STRENGTH.toFixed(3)})`]);
    expect(stops.at(-1)).toEqual([1, "rgba(255, 128, 0, 0.000)"]);
    expect(raw.globalCompositeOperation).toBe("lighter");
  });

  it("show a flood's reach faintly when nothing plays", () => {
    const { ctx, raw, stops } = fakeContext();
    drawFloods(ctx, [flood(true)], null, view, size, [90, 90, 90]);
    expect(raw.createRadialGradient).toHaveBeenCalledTimes(2);
    expect(stops[0][1]).toBe("rgba(90, 90, 90, 0.250)");
  });
});
