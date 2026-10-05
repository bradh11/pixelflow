// Timing, not correctness: skipped unless PIXELFLOW_BENCH=1 (see pixelBench.ts).

import { describe, expect, it } from "vitest";
import { type PixelContext, batchPixels, fillBatches } from "./pixelBatches";
import { BENCH_COLORS, median, syntheticShow } from "./pixelBench";

const env = (globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env ?? {};

describe.skipIf(!env.PIXELFLOW_BENCH)("drawing a 50,000-pixel frame", () => {
  it("groups the pixels by color quickly, for a handful of fills", () => {
    const { props, frame, view, size } = syntheticShow();
    let fills = 0;
    const ctx = {
      set fillStyle(_: string) {},
      beginPath() {},
      moveTo() {},
      arc() {},
      rect() {},
      fill() {
        fills++;
      },
    } as unknown as PixelContext;
    const groupMs = median(() => batchPixels(props, frame, view, size, new Set(), BENCH_COLORS, 3), 30);
    const drawMs = median(() => fillBatches(ctx, batchPixels(props, frame, view, size, new Set(), BENCH_COLORS, 3), 3), 30);
    fills = 0;
    fillBatches(ctx, batchPixels(props, frame, view, size, new Set(), BENCH_COLORS, 3), 3);
    console.log(`50,000 pixels: grouping ${groupMs.toFixed(2)} ms, grouping + path calls ${drawMs.toFixed(2)} ms, ${fills} fills (one per pixel before)`);
    expect(fills).toBeLessThan(1000);
  });
});
