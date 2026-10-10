// Dev-only timing of the layout canvas's pixel drawing on a synthetic 50,000-pixel show (not
// part of the app). Two ways to run it:
//
// - Grouping only, in Node: `PIXELFLOW_BENCH=1 pnpm vitest run src/lib/pixelBatches.bench.test.ts`
// - Real drawing, in a browser: start `pnpm dev`, open the app, and in the console run
//   `(await import("/src/lib/pixelBench.ts")).runCanvasBench()`, or `.runGlowBench()` for a
//   15,000-pixel show with every pixel lit, at no glow, half and full.

import type { PreviewProp } from "../api/types";
import type { Size, View } from "./layoutMath";
import { batchPixels, drawBatches, drawPixels } from "./pixelBatches";

export const BENCH_COLORS = { unlit: "rgba(220, 220, 220, 0.7)", selected: "#a78bfa", dark: "rgba(90, 90, 90, 0.6)" };

/** 50 props of 1,000 pixels in rows across a 1200×700 canvas, and a rainbow frame like the demo's. */
export function syntheticShow(pixels = 50_000, perProp = 1_000) {
  const size: Size = { width: 1200, height: 700 };
  const view: View = { cx: 25, cy: 10, zoom: 22 };
  const props: PreviewProp[] = [];
  for (let p = 0; p * perProp < pixels; p++) {
    const points = new Float32Array(perProp * 2);
    for (let i = 0; i < perProp; i++) {
      points[i * 2] = (p % 10) * 5 + (i % 50) * 0.09;
      points[i * 2 + 1] = Math.floor(p / 10) * 4 + Math.floor(i / 50) * 0.18;
    }
    props.push({ prop: `prop-${p}`, frameOffset: p * perProp * 3, channelsPerPixel: 3, points });
  }
  const frame = new Uint8Array(pixels * 3);
  for (let i = 0; i < pixels; i++) {
    const hue = (i * 4) % 360;
    [0, 120, 240].forEach((o, c) => (frame[i * 3 + c] = Math.round(127 + 127 * Math.cos(((hue - o) * Math.PI) / 180))));
  }
  return { props, frame, view, size };
}

/** One fill per pixel with a new color string each: how the canvas used to draw lit pixels. */
export function drawPerPixel(ctx: CanvasRenderingContext2D, props: PreviewProp[], frame: Uint8Array, view: View, size: Size, radius: number) {
  for (const p of props) {
    const pts = p.points;
    for (let i = 0, n = 0; i + 1 < pts.length; i += 2, n++) {
      const x = size.width / 2 + (pts[i] - view.cx) * view.zoom;
      const y = size.height / 2 - (pts[i + 1] - view.cy) * view.zoom;
      const o = p.frameOffset + n * p.channelsPerPixel;
      const [r, g, b] = [frame[o], frame[o + 1], frame[o + 2]];
      ctx.fillStyle = r + g + b === 0 ? BENCH_COLORS.dark : `rgb(${r}, ${g}, ${b})`;
      ctx.beginPath();
      ctx.arc(x, y, radius, 0, Math.PI * 2);
      ctx.fill();
    }
  }
}

export function drawBatched(ctx: CanvasRenderingContext2D, props: PreviewProp[], frame: Uint8Array, view: View, size: Size, radius: number, ratio = 1) {
  drawBatches(ctx, batchPixels(props, frame, view, size, new Set(), BENCH_COLORS, radius), radius, ratio);
}

/** The median time of `runs` calls, in milliseconds. */
export function median(fn: () => void, runs: number): number {
  const times: number[] = [];
  for (let i = 0; i < runs; i++) {
    const start = performance.now();
    fn();
    times.push(performance.now() - start);
  }
  times.sort((a, b) => a - b);
  return times[Math.floor(times.length / 2)];
}

/**
 * Times both ways of drawing a whole frame on a real canvas, at 1 and 2 device pixels per
 * screen pixel, with tiny (square) and round pixels. Each draw is finished before it counts.
 */
export function runCanvasBench(runs = 15) {
  const { props, frame, view, size } = syntheticShow();
  const result: Record<string, number> = { pixels: frame.length / 3, colors: batchPixels(props, frame, view, size, new Set(), BENCH_COLORS).length };
  for (const ratio of [1, 2]) {
    const canvas = document.createElement("canvas");
    canvas.width = size.width * ratio;
    canvas.height = size.height * ratio;
    document.body.append(canvas);
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    const time = (draw: () => void) =>
      median(() => {
        ctx.clearRect(0, 0, size.width, size.height);
        draw();
        ctx.getImageData(0, 0, 1, 1); // waits for the drawing to complete
      }, runs);
    for (const radius of [1.3, 3]) {
      result[`perPixelMs_x${ratio}_r${radius}`] = time(() => drawPerPixel(ctx, props, frame, view, size, radius));
      result[`batchedMs_x${ratio}_r${radius}`] = time(() => drawBatched(ctx, props, frame, view, size, radius, ratio));
    }
    canvas.remove();
  }
  console.table(result);
  return result;
}

/**
 * Times a whole frame of a show with every pixel lit on a real canvas, as the previews draw it:
 * `plain` without any of the glow code, then at no glow, a little, half and full. `zoom` is the
 * view's (22 fits the show on the canvas; zoomed in, halos are bigger and fewer pixels show).
 */
export function runGlowBench(runs = 15, pixels = 15_000, zoom = 22) {
  const { props, frame, size, ...fit } = syntheticShow(pixels);
  const view = { ...fit.view, zoom };
  const result: Record<string, number> = { pixels: frame.length / 3 };
  for (const ratio of [1, 2]) {
    const canvas = document.createElement("canvas");
    canvas.width = size.width * ratio;
    canvas.height = size.height * ratio;
    document.body.append(canvas);
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    const time = (draw: () => void) =>
      median(() => {
        ctx.fillStyle = "#0a0a0c";
        ctx.fillRect(0, 0, size.width, size.height);
        draw();
        ctx.getImageData(0, 0, 1, 1); // waits for the drawing to complete
      }, runs);
    for (const radius of [1.3, 3]) {
      result[`plainMs_x${ratio}_r${radius}`] = time(() => drawBatched(ctx, props, frame, view, size, radius, ratio));
      for (const glow of [0, 0.1, 0.5, 1]) result[`glow${glow * 100}Ms_x${ratio}_r${radius}`] = time(() => drawPixels(ctx, props, frame, view, size, new Set(), BENCH_COLORS, radius, ratio, glow));
    }
    canvas.remove();
  }
  console.table(result);
  return result;
}
