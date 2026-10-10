import { afterEach, describe, expect, it, vi } from "vitest";
import type { PreviewProp } from "../api/types";
import { drawPixels } from "./pixelBatches";
import { DOT_RADIUS, GLOW_STEPS, type Halo, coreDim, glowStep, haloSprite, newSums, planHalos, stampHalos, sumsToImage } from "./pixelGlow";

const SIZE = { width: 100, height: 100 };
// Screen = world × 10, centered: world (0, 0) is the canvas center.
const VIEW = { cx: 0, cy: 0, zoom: 10 };
const COLORS = { unlit: "unlit", selected: "selected", dark: "dark" };
const HALF = GLOW_STEPS / 2;

const prop = (id: string, points: number[], frameOffset = 0): PreviewProp => ({ prop: id, frameOffset, channelsPerPixel: 3, points });

describe("a glow level", () => {
  it("is drawn at the nearest of twenty steps, none to full", () => {
    expect([0, 0.05, 0.5, 0.52, 1].map(glowStep)).toEqual([0, 1, HALF, HALF, GLOW_STEPS]);
    // Too little to make a step is none; out of range is kept in range; not a number is none.
    expect([0.01, -1, 7, NaN, Infinity].map(glowStep)).toEqual([0, 0, GLOW_STEPS, 0, 0]);
  });
});

describe("a halo", () => {
  const at = (halo: Halo, dx: number, dy: number) => halo.weights[(dy + halo.reach) * (2 * halo.reach + 1) + dx + halo.reach];

  it("is a soft bell around its dot: wider and stronger the more glow", () => {
    const [low, half, full] = [haloSprite(4, 1), haloSprite(4, HALF), haloSprite(4, GLOW_STEPS)];
    // Beside the dot, the video export's strengths: 0.35 of the color at half glow, 0.7 at full.
    expect(at(half, 0, 0)).toBe(Math.round(0.35 * 256));
    expect(at(full, 0, 0)).toBe(Math.round(0.7 * 256));
    expect(at(low, 0, 0)).toBeLessThan(at(half, 0, 0));
    expect(full.reach).toBeGreaterThan(half.reach);
    expect(half.reach).toBeGreaterThan(low.reach);
    // It fades with distance, the same in every direction, to next to nothing at its edge.
    expect(at(full, 6, 0)).toBeLessThan(at(full, 3, 0));
    expect(at(full, 12, 0)).toBeLessThan(at(full, 6, 0));
    expect(at(full, 6, 0)).toBe(at(full, -6, 0));
    expect(at(full, 6, 0)).toBe(at(full, 0, 6));
    expect(at(full, full.reach, 0)).toBeLessThanOrEqual(2);
    expect(at(full, 8, 0)).toBeGreaterThan(at(half, 8, 0));
  });

  it("says where each of its rows has anything in it", () => {
    const halo = haloSprite(2, HALF);
    const side = 2 * halo.reach + 1;
    for (let row = 0; row < side; row++) {
      const weights = Array.from(halo.weights.subarray(row * side, (row + 1) * side));
      const lit = weights.map((w, col) => (w > 0 ? col : -1)).filter((col) => col >= 0);
      if (lit.length === 0) expect(halo.from[row]).toBeGreaterThanOrEqual(halo.to[row]);
      else expect([halo.from[row], halo.to[row]]).toEqual([lit[0], lit[lit.length - 1] + 1]);
    }
    // The corners of the square are too far out to have any: the middle row is the widest.
    expect(halo.to[0] - halo.from[0]).toBeLessThan(halo.to[halo.reach] - halo.from[halo.reach]);
  });

  it("stays centered on a dot that isn't in the middle of its pixel", () => {
    const left = haloSprite(4, HALF, 0.125, 0.5);
    expect(at(left, -3, 0)).toBeGreaterThan(at(left, 3, 0));
    const low = haloSprite(4, HALF, 0.5, 0.875);
    expect(at(low, 0, 3)).toBeGreaterThan(at(low, 0, -3));
  });

  it("with the dot drawn dimmer under it, leaves the dot about its own color", () => {
    expect(coreDim(0)).toBe(1);
    for (const step of [1, 5, HALF, 15, GLOW_STEPS]) {
      const dim = coreDim(step);
      const halo = haloSprite(4, step);
      // The dimmed dot plus its own halo: a little over at its center, a little under at its edge.
      const [center, edge] = [dim + at(halo, 0, 0) / 256, dim + at(halo, 4, 0) / 256];
      expect(center).toBeGreaterThan(1);
      expect(center).toBeLessThan(1.08);
      expect(edge).toBeLessThan(1);
      expect(edge).toBeGreaterThan(0.9);
    }
    expect(coreDim(GLOW_STEPS)).toBeLessThan(coreDim(HALF));
    // A halo made for a smaller dot than the one drawn adds less back over it: the dot is dimmed less.
    expect(coreDim(HALF, 0.5)).toBeGreaterThan(coreDim(HALF));
    expect(coreDim(HALF, 0.5)).toBeLessThan(1);
  });
});

describe("stamping halos", () => {
  it("adds a halo around each lit pixel, and nothing around one that's off", () => {
    // A lit pixel in the middle and an unlit one at the right.
    const props = [prop("pillar", [0, 0, 3, 0])];
    const frame = new Uint8Array([200, 100, 0, 0, 0, 0]);
    const sums = newSums(100, 100);
    const box = stampHalos(sums, props, frame, VIEW, SIZE, HALF, 2, 1)!;
    const red = (x: number, y: number) => sums.r[y * 100 + x] >> 8;
    const green = (x: number, y: number) => sums.g[y * 100 + x] >> 8;
    // The lit pixel is at screen (50, 50): 0.35 of its color beside it, fading away.
    expect(red(50, 50)).toBeGreaterThan(60);
    expect(red(50, 50)).toBeLessThanOrEqual(70);
    expect(green(50, 50)).toBe(Math.floor(red(50, 50) / 2));
    expect(red(54, 50)).toBeLessThan(red(52, 50));
    expect(red(52, 50)).toBeLessThan(red(50, 50));
    expect(sums.b.every((v) => v === 0)).toBe(true);
    // Nothing around the unlit one (screen 80, 50).
    expect(red(80, 50)).toBe(0);
    // The box holds everything that was added.
    expect(box.x0).toBeLessThan(50 - 4);
    expect(box.x1).toBeGreaterThan(50 + 4);
    for (let y = 0; y < 100; y++) for (let x = 0; x < 100; x++) if (sums.r[y * 100 + x]) expect(x >= box.x0 && x < box.x1 && y >= box.y0 && y < box.y1).toBe(true);
  });

  it("finds each prop's colors at its own place in the frame", () => {
    const sums = newSums(100, 100);
    stampHalos(sums, [prop("roof", [-3, 0]), prop("pillar", [3, 0], 3)], new Uint8Array([0, 0, 0, 0, 0, 255]), VIEW, SIZE, HALF, 2, 1);
    expect(sums.b[50 * 100 + 20]).toBe(0);
    expect(sums.b[50 * 100 + 80]).toBeGreaterThan(0);
  });

  it("adds up where halos overlap, and is clipped at the picture's edges", () => {
    const one = newSums(100, 100);
    stampHalos(one, [prop("pillar", [0, 0])], new Uint8Array([100, 0, 0]), VIEW, SIZE, HALF, 2, 1);
    const two = newSums(100, 100);
    stampHalos(two, [prop("pillar", [-0.2, 0, 0.2, 0])], new Uint8Array([100, 0, 0, 100, 0, 0]), VIEW, SIZE, HALF, 2, 1);
    expect(two.r[50 * 100 + 50]).toBeGreaterThan(one.r[50 * 100 + 50] * 1.3);

    const edge = newSums(100, 100);
    const box = stampHalos(edge, [prop("pillar", [-5, 5])], new Uint8Array([100, 0, 0]), VIEW, SIZE, HALF, 2, 1)!;
    expect([box.x0, box.y0]).toEqual([0, 0]);
    expect(edge.r[0]).toBeGreaterThan(0);
    // Too far off the picture to reach it, with nothing lit, or with no glow: nothing to draw.
    expect(stampHalos(edge, [prop("pillar", [-9, 0])], new Uint8Array([100, 0, 0]), VIEW, SIZE, HALF, 2, 1)).toBeNull();
    expect(stampHalos(edge, [prop("pillar", [0, 0])], new Uint8Array([0, 0, 0]), VIEW, SIZE, HALF, 2, 1)).toBeNull();
    expect(stampHalos(newSums(100, 100), [prop("pillar", [0, 0])], new Uint8Array([100, 0, 0]), VIEW, SIZE, 0, 2, 1)).toBeNull();
  });

  it("draws smaller halos into a coarser picture", () => {
    // Half the pixels each way: the pixel at screen (50, 50) lands at (25, 25), its dot half the size.
    const sums = newSums(50, 50);
    const box = stampHalos(sums, [prop("pillar", [0, 0])], new Uint8Array([100, 0, 0]), VIEW, SIZE, HALF, 1, 0.5)!;
    expect(sums.r[25 * 50 + 25]).toBeGreaterThan(0);
    expect(box.x1 - box.x0).toBeLessThan(haloSprite(2, HALF).reach * 2 + 1);
  });

  it("becomes light to add: each color as bright as it added up to, clear where there's none", () => {
    const sums = newSums(4, 2);
    sums.r[1] = 100 * 256;
    sums.g[1] = 50 * 256;
    sums.r[2] = 900 * 256;
    sums.b[2] = 300 * 256;
    sums.g[5] = 7 * 256;
    const rgba = new Uint8ClampedArray(4 * 2 * 4).fill(9);
    sumsToImage(sums, { x0: 0, y0: 0, x1: 3, y1: 1 }, rgba);
    const pixel = (i: number) => Array.from(rgba.subarray(i * 4, i * 4 + 4));
    expect(pixel(0)).toEqual([0, 0, 0, 0]);
    // As solid as its brightest color, with the others in proportion.
    expect(pixel(1)).toEqual([255, 128, 0, 100]);
    // Past full brightness is full brightness.
    expect(pixel(2)).toEqual([255, 0, 255, 255]);
    // Outside the box nothing is touched; inside, the sums are emptied for the next frame.
    expect(pixel(3)).toEqual([9, 9, 9, 9]);
    expect(pixel(5)).toEqual([9, 9, 9, 9]);
    expect([sums.r[1], sums.g[1], sums.r[2], sums.b[2]]).toEqual([0, 0, 0, 0]);
    expect(sums.g[5]).toBe(7 * 256);
  });
});

describe("drawing pixels with their glow", () => {
  const getContext = HTMLCanvasElement.prototype.getContext;
  afterEach(() => {
    HTMLCanvasElement.prototype.getContext = getContext;
  });

  /** A canvas context that records what's drawn on it, and gives the coarse picture one of its own. */
  function canvases() {
    const drawn: { op: string; smooth: boolean; args: number[] }[] = [];
    const styles: string[] = [];
    const main = {
      canvas: { width: 200, height: 200 },
      globalCompositeOperation: "source-over",
      imageSmoothingEnabled: false,
      set fillStyle(v: string) {
        styles.push(v);
      },
      beginPath() {},
      moveTo() {},
      arc() {},
      rect() {},
      fill() {},
      save() {},
      restore() {
        main.globalCompositeOperation = "source-over";
      },
      setTransform() {},
      drawImage(_image: unknown, ...args: number[]) {
        drawn.push({ op: main.globalCompositeOperation, smooth: main.imageSmoothingEnabled, args });
      },
    };
    const put = vi.fn();
    HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement) {
      return { createImageData: (w: number, h: number) => ({ data: new Uint8ClampedArray(w * h * 4) }), putImageData: put, clearRect() {} };
    } as unknown as typeof HTMLCanvasElement.prototype.getContext;
    return { ctx: main as unknown as CanvasRenderingContext2D, drawn, styles, put };
  }

  const props = [prop("pillar", [0, 0])];
  const frame = new Uint8Array([255, 0, 0]);
  // Zoomed so a pixel's dot in the layout is the 3 screen pixels it's drawn with.
  const CLOSE = { ...VIEW, zoom: 3 / DOT_RADIUS };
  const draw = (ctx: CanvasRenderingContext2D, f: Uint8Array | null, glow: number, view = CLOSE) => drawPixels(ctx, props, f, view, SIZE, new Set(), COLORS, 3, 2, glow);

  it("with no glow, draws crisp dots in their own colors and nothing else", () => {
    const { ctx, drawn, styles, put } = canvases();
    draw(ctx, frame, 0);
    draw(ctx, frame, 0.01);
    draw(ctx, frame, NaN);
    expect(styles).toEqual(["rgb(255, 0, 0)", "rgb(255, 0, 0)", "rgb(255, 0, 0)"]);
    expect(drawn).toEqual([]);
    expect(put).not.toHaveBeenCalled();
  });

  it("with glow, adds the halos over the dots in one enlarged, smoothed picture", () => {
    const { ctx, drawn, styles, put } = canvases();
    draw(ctx, frame, 1);
    expect(drawn).toHaveLength(1);
    expect(drawn[0].op).toBe("lighter");
    expect(drawn[0].smooth).toBe(true);
    // The part of the coarse picture with halos in it, drawn several times its size, over the
    // lit pixel (device pixel 100, 100 of the 200 × 200 canvas).
    const [sx, sy, sw, sh, dx, dy, dw, dh] = drawn[0].args;
    const cell = dw / sw;
    expect(cell).toBeGreaterThan(1);
    expect(dx).toBeCloseTo(sx * cell);
    expect(dy).toBeCloseTo(sy * cell);
    expect(dh).toBeCloseTo(sh * cell);
    expect(dx).toBeLessThan(100);
    expect(dx + dw).toBeGreaterThan(100);
    expect(dy).toBeLessThan(100);
    expect(dy + dh).toBeGreaterThan(100);
    expect(put).toHaveBeenCalledTimes(1);
    // The dot is drawn dimmer by what its own halo adds back.
    expect(styles).toEqual(["rgb(90, 0, 0)"]);
  });

  it("draws no halos while nothing plays, or when every pixel is off", () => {
    const { ctx, drawn, styles, put } = canvases();
    draw(ctx, null, 1);
    expect(styles).toEqual(["unlit"]);
    draw(ctx, new Uint8Array(3), 1);
    expect(styles).toEqual(["unlit", "dark"]);
    expect(drawn).toEqual([]);
    expect(put).not.toHaveBeenCalled();
  });

  it("sizes halos from the dot a pixel has in the layout, not from one drawn bigger to be seen", () => {
    const { ctx, drawn, styles } = canvases();
    // Zoomed out: the layout's dot would be 1 screen pixel, but it's drawn with 3.
    const far = { ...VIEW, zoom: 1 / DOT_RADIUS };
    expect(planHalos(ctx, 1, far, 3, 2)?.radius).toBeCloseTo(1);
    expect(planHalos(ctx, 1, CLOSE, 3, 2)?.radius).toBeCloseTo(3);
    // Zoomed in past the biggest dot drawn, halos keep to the dot that's drawn.
    expect(planHalos(ctx, 1, { ...VIEW, zoom: 9 / DOT_RADIUS }, 3, 2)?.radius).toBeCloseTo(3);
    draw(ctx, frame, 1, far);
    draw(ctx, frame, 1);
    // The smaller halo is drawn over less of the canvas, and its dot is dimmed less.
    expect(drawn).toHaveLength(2);
    expect(drawn[0].args[6]).toBeLessThan(drawn[1].args[6]);
    const red = (style: string) => Number(/\d+/.exec(style)![0]);
    expect(red(styles[0])).toBeGreaterThan(red(styles[1]));
    expect(red(styles[0])).toBeLessThan(255);
  });

  it("draws plain dots when a halo would be too small to see", () => {
    const { ctx, drawn, styles } = canvases();
    const tiny = { ...VIEW, zoom: 0.1 / DOT_RADIUS };
    expect(planHalos(ctx, 1, tiny, 3, 2)).toBeNull();
    draw(ctx, frame, 1, tiny);
    expect(drawn).toEqual([]);
    expect(styles).toEqual(["rgb(255, 0, 0)"]);
  });

  it("draws the dots as they are where a halo can't be drawn", () => {
    const { ctx, drawn, styles } = canvases();
    HTMLCanvasElement.prototype.getContext = (() => null) as typeof HTMLCanvasElement.prototype.getContext;
    // Another size of canvas, so the coarse picture has to be made again (and can't be).
    (ctx.canvas as { width: number }).width = 260;
    draw(ctx, frame, 1);
    expect(drawn).toEqual([]);
    expect(styles).toEqual(["rgb(255, 0, 0)"]);
  });
});
