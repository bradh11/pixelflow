import { describe, expect, it } from "vitest";
import type { PreviewProp } from "../api/types";
import { COLORS } from "./flatPreview";
import { FLOOD_RADIUS, FLOOD_STRENGTH, washAt, washRadius } from "./floodWash";
import { type DotImage, stampPixels } from "./pixelBatches";

const view = { cx: 0, cy: 0, zoom: 40 };
const size = { width: 100, height: 60 };

function stamp(flood: boolean, frame: Uint8Array | null): DotImage {
  const image: DotImage = { pixels: new Int32Array(100 * 60), width: 100, height: 60 };
  const prop: PreviewProp = { prop: "f", frameOffset: 0, channelsPerPixel: 3, points: [0, 0], flood };
  stampPixels(image, [prop], frame, view, size, new Set(), COLORS, 2, 1);
  return image;
}

const at = (image: DotImage, x: number, y: number) => image.pixels[y * image.width + x];

describe("flood washes", () => {
  it("fall from full at the middle to nothing at the edge", () => {
    expect(washAt(0)).toBe(1);
    expect(washAt(0.25)).toBeCloseTo(0.5625);
    expect(washAt(1)).toBe(0);
    expect(washRadius(40)).toBe(FLOOD_RADIUS * 40);
    expect(washRadius(1)).toBe(12);
  });

  it("wash round a lit flood's cell in its color, under its dot, and not round other props", () => {
    const lit = stamp(true, new Uint8Array([255, 128, 0]));
    // Twenty screen pixels out from the dot, inside the 30-pixel wash: orange, partly solid.
    const out = at(lit, 70, 30);
    expect([out & 255, (out >> 8) & 255, (out >> 16) & 255]).toEqual([255, 128, 0]);
    const alpha = out >>> 24;
    expect(alpha).toBeGreaterThan(0);
    expect(alpha).toBeLessThan(Math.round(FLOOD_STRENGTH * 255));
    expect(at(lit, 81, 30)).toBe(0);
    expect(at(stamp(false, new Uint8Array([255, 128, 0])), 70, 30)).toBe(0);
    // A dark flood washes nothing.
    expect(at(stamp(true, new Uint8Array([0, 0, 0])), 70, 30)).toBe(0);
  });

  it("show a flood's reach faintly when nothing plays", () => {
    const unlit = at(stamp(true, null), 70, 30) >>> 24;
    expect(unlit).toBeGreaterThan(0);
    expect(unlit).toBeLessThan(at(stamp(true, new Uint8Array([255, 255, 255])), 70, 30) >>> 24);
  });
});
