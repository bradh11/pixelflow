// Drawing every pixel of the layout fast enough for big shows. Instead of one fill per pixel
// (50,000 fills for a 50,000-pixel show, every frame), pixels are grouped by color: tiny pixels
// are drawn as one path of squares per color, and round ones, when there are many, are stamped
// straight into an image that's drawn in one go (filling thousands of little circles is slow
// even in one path). Colors are rounded to 32 levels per channel, which can't be told apart at
// preview size but keeps the number of colors small. Pixels off the edge of the canvas are skipped.

import type { PreviewProp } from "../api/types";
import type { Size, View } from "./layoutMath";

/** The colors used when a pixel isn't showing a lit color of its own. */
export interface PixelColors {
  /** Not playing: an unselected prop's pixels. */
  unlit: string;
  /** Not playing: a selected prop's pixels. */
  selected: string;
  /** Playing, but this pixel is off (black). */
  dark: string;
}

/** Every pixel of one color, as screen x, y pairs. */
export interface PixelBatch {
  color: string;
  /** The color as an image pixel (RGBA bytes in memory order). */
  rgba: number;
  xy: Float32Array;
}

const UNLIT = -1;
const SELECTED = -2;
const DARK = -3;

/** Below this radius (screen pixels) pixels are drawn as squares: much faster, and they look the same. */
export const ROUND_FROM = 2;
/** Round pixels are stamped into an image instead of filled as paths when there are more than this many. */
export const STAMP_FROM = 8000;

/** A CSS color (#rrggbb, rgb(), or rgba()) as an image pixel: RGBA bytes in memory order. */
export function packColor(css: string): number {
  let [r, g, b, a] = [0, 0, 0, 255];
  const hex = /^#([0-9a-f]{6})$/i.exec(css);
  if (hex) {
    const n = parseInt(hex[1], 16);
    [r, g, b] = [n >> 16, (n >> 8) & 255, n & 255];
  } else {
    const parts = css.match(/[\d.]+/g)?.map(Number) ?? [];
    [r, g, b] = [parts[0] ?? 0, parts[1] ?? 0, parts[2] ?? 0];
    if (parts.length > 3) a = Math.round(parts[3] * 255);
  }
  return new Uint32Array(new Uint8Array([r, g, b, a]).buffer)[0];
}

const level = (v: number) => Math.min(31, (v + 4) >> 3);
const shades = new Map<number, { color: string; rgba: number }>();
function shade(key: number): { color: string; rgba: number } {
  let found = shades.get(key);
  if (!found) {
    const channel = (shift: number) => Math.round((((key >> shift) & 31) * 255) / 31);
    const color = `rgb(${channel(10)}, ${channel(5)}, ${channel(0)})`;
    found = { color, rgba: packColor(color) };
    shades.set(key, found);
  }
  return found;
}

// Reused between calls so drawing doesn't make garbage every frame.
let scratchX = new Float32Array(0);
let scratchY = new Float32Array(0);
let scratchKey = new Int32Array(0);

/**
 * The props' pixels on screen, grouped by the color to draw them: their color in `frame` while
 * something plays (rounded), otherwise the unlit or selected color. `margin` (screen pixels) is
 * how far off the canvas a pixel may be and still be drawn (its radius).
 */
export function batchPixels(
  props: PreviewProp[],
  frame: Uint8Array | null,
  view: View,
  size: Size,
  selected: ReadonlySet<string>,
  colors: PixelColors,
  margin = 0,
): PixelBatch[] {
  let total = 0;
  for (const p of props) total += p.points.length >> 1;
  if (scratchKey.length < total) {
    scratchX = new Float32Array(total);
    scratchY = new Float32Array(total);
    scratchKey = new Int32Array(total);
  }
  const [xs, ys, keys] = [scratchX, scratchY, scratchKey];
  const counts = new Map<number, number>();
  const [halfW, halfH, zoom, cx, cy] = [size.width / 2, size.height / 2, view.zoom, view.cx, view.cy];
  const [minX, maxX, minY, maxY] = [-margin, size.width + margin, -margin, size.height + margin];
  let n = 0;
  for (const p of props) {
    const pts = p.points;
    const fixed = frame ? null : selected.has(p.prop) ? SELECTED : UNLIT;
    for (let i = 0, pixel = 0; i + 1 < pts.length; i += 2, pixel++) {
      const sx = halfW + (pts[i] - cx) * zoom;
      const sy = halfH - (pts[i + 1] - cy) * zoom;
      if (sx < minX || sx > maxX || sy < minY || sy > maxY) continue;
      let key: number;
      if (fixed !== null) key = fixed;
      else {
        const o = p.frameOffset + pixel * p.channelsPerPixel;
        if (o + 2 >= frame!.length) key = DARK;
        else {
          const [r, g, b] = [frame![o], frame![o + 1], frame![o + 2]];
          key = r + g + b === 0 ? DARK : (level(r) << 10) | (level(g) << 5) | level(b);
        }
      }
      xs[n] = sx;
      ys[n] = sy;
      keys[n] = key;
      counts.set(key, (counts.get(key) ?? 0) + 1);
      n++;
    }
  }
  const batches = new Map<number, { xy: Float32Array; at: number }>();
  for (const [key, count] of counts) batches.set(key, { xy: new Float32Array(count * 2), at: 0 });
  for (let i = 0; i < n; i++) {
    const b = batches.get(keys[i])!;
    b.xy[b.at++] = xs[i];
    b.xy[b.at++] = ys[i];
  }
  const fixed = (color: string) => ({ color, rgba: packColor(color) });
  const colorOf = (key: number) => (key === UNLIT ? fixed(colors.unlit) : key === SELECTED ? fixed(colors.selected) : key === DARK ? fixed(colors.dark) : shade(key));
  // Selected pixels last, so they're drawn over others.
  return [...batches]
    .sort(([a], [b]) => (a === SELECTED ? 1 : 0) - (b === SELECTED ? 1 : 0))
    .map(([key, { xy }]) => ({ ...colorOf(key), xy }));
}

/** The parts of a 2D canvas context that drawing pixels uses. */
export type PixelContext = Pick<CanvasRenderingContext2D, "fillStyle" | "beginPath" | "moveTo" | "arc" | "rect" | "fill">;

/** Fills one path per color: dots, or squares when they're tiny. */
export function fillBatches(ctx: PixelContext, batches: PixelBatch[], radius: number) {
  const round = radius >= ROUND_FROM;
  const side = radius * 2;
  for (const { color, xy } of batches) {
    ctx.fillStyle = color;
    ctx.beginPath();
    if (round) {
      for (let i = 0; i + 1 < xy.length; i += 2) {
        ctx.moveTo(xy[i] + radius, xy[i + 1]);
        ctx.arc(xy[i], xy[i + 1], radius, 0, Math.PI * 2);
      }
    } else {
      for (let i = 0; i + 1 < xy.length; i += 2) ctx.rect(xy[i] - radius, xy[i + 1] - radius, side, side);
    }
    ctx.fill();
  }
}

/**
 * Stamps round dots into `pixels`, an image `width` device pixels wide (`ratio` device pixels
 * per screen pixel). Later dots cover earlier ones.
 */
export function stampDots(pixels: Uint32Array, width: number, height: number, batches: PixelBatch[], radius: number, ratio: number) {
  const r = radius * ratio;
  const reach = Math.ceil(r);
  const offsets: number[] = [];
  for (let dy = -reach; dy <= reach; dy++) {
    for (let dx = -reach; dx <= reach; dx++) if (dx * dx + dy * dy <= r * r) offsets.push(dx, dy);
  }
  for (const { rgba, xy } of batches) {
    for (let i = 0; i + 1 < xy.length; i += 2) {
      const cx = Math.round(xy[i] * ratio);
      const cy = Math.round(xy[i + 1] * ratio);
      const inside = cx - reach >= 0 && cy - reach >= 0 && cx + reach < width && cy + reach < height;
      for (let k = 0; k < offsets.length; k += 2) {
        const x = cx + offsets[k];
        const y = cy + offsets[k + 1];
        if (inside || (x >= 0 && y >= 0 && x < width && y < height)) pixels[y * width + x] = rgba;
      }
    }
  }
}

/** The image round dots are stamped into, kept between frames. */
let stamp: { canvas: HTMLCanvasElement; ctx: CanvasRenderingContext2D; image: ImageData; pixels: Uint32Array } | null = null;

function stampLayer(width: number, height: number) {
  if (!stamp || stamp.canvas.width !== width || stamp.canvas.height !== height) {
    const canvas = document.createElement("canvas");
    [canvas.width, canvas.height] = [width, height];
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    const image = ctx.createImageData(width, height);
    stamp = { canvas, ctx, image, pixels: new Uint32Array(image.data.buffer) };
  }
  return stamp;
}

/**
 * Draws the batches on a canvas whose transform is `ratio` device pixels per screen pixel:
 * squares or dots as paths, one fill per color, or many dots stamped into one image.
 */
export function drawBatches(ctx: CanvasRenderingContext2D, batches: PixelBatch[], radius: number, ratio = 1) {
  const count = batches.reduce((n, b) => n + b.xy.length / 2, 0);
  const layer = radius >= ROUND_FROM && count > STAMP_FROM ? stampLayer(ctx.canvas.width, ctx.canvas.height) : null;
  if (!layer) return fillBatches(ctx, batches, radius);
  layer.pixels.fill(0);
  stampDots(layer.pixels, layer.canvas.width, layer.canvas.height, batches, radius, ratio);
  layer.ctx.putImageData(layer.image, 0, 0);
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.drawImage(layer.canvas, 0, 0);
  ctx.restore();
}
