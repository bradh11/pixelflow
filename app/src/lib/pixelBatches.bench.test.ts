// Timing, not correctness: skipped unless PIXELFLOW_BENCH=1 (see pixelBench.ts).

import { describe, expect, it } from "vitest";
import { boxOfPoints, fitView, unionBox } from "./layoutMath";
import { DOT_RADIUS, type DotImage, stampPixels } from "./pixelBatches";
import { BENCH_COLORS, median, syntheticShow } from "./pixelBench";

const env = (globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env ?? {};

describe.skipIf(!env.PIXELFLOW_BENCH)("stamping a frame's dots", () => {
  it("stamps shows of 5,000 to 200,000 pixels into an image quickly", () => {
    for (const pixels of [5_000, 50_000, 200_000]) {
      const { props, frame, size } = syntheticShow(pixels);
      const view = fitView(unionBox(props.map((p) => boxOfPoints(p.points))), size, 16);
      const radius = Math.min(4, Math.max(1.2, view.zoom * DOT_RADIUS));
      const times = [1, 2].map((ratio) => {
        const image: DotImage = { pixels: new Int32Array(size.width * ratio * size.height * ratio), width: size.width * ratio, height: size.height * ratio };
        const stamp = () => stampPixels(image, props, frame, view, size, new Set(), BENCH_COLORS, radius, ratio);
        expect(stamp()).not.toBeNull();
        return median(() => {
          image.pixels.fill(0);
          stamp();
        }, 30);
      });
      console.log(`${pixels.toLocaleString("en-US")} pixels: stamped in ${times[0].toFixed(2)} ms at 1 device pixel to a screen pixel, ${times[1].toFixed(2)} ms at 2`);
    }
  });
});
