// The glow around lit pixels in the 2D previews, as much as the viewer's Glow setting says (see
// state/view3d.ts): the look the video export draws at the same level (crates/pf-video's
// raster.rs), from the same numbers. Around each lit dot is a soft halo in its color, added onto
// everything it reaches; the dot itself keeps its own color.
//
// A halo is many times its dot's size, so a filled circle or an image per pixel would be far too
// slow for a big show. Instead every halo is stamped into one small picture, coarse enough that
// a halo's bell is only a few of its pixels wide (a halo has no sharp edges to lose), and that
// picture is drawn over the dots enlarged and smoothed, adding to what's there. Pixels that are
// off cost nothing, and with no glow none of this runs.
//
// A halo here is the plain bell, without the hole the export leaves over the dot itself (too
// sharp for the coarse picture). So that a dot still ends up its own color, lit dots are drawn
// dimmer by what their own halo adds back (`coreDim`).
//
// A halo is sized from the dot a pixel has in the layout (`DOT_RADIUS` of a layout unit), as the
// export's is, not from a dot drawn bigger so it can be seen in a small preview: the glow then
// covers as much of the display as it does in a video, however small the preview.

import type { PreviewProp } from "../api/types";
import type { Size, View } from "./layoutMath";

/** A glow's width (its bell's width, in dot radii) and strength beside the dot, at none and at full glow: the export's. */
const GLOW_WIDTH = [1.2, 2.4] as const;
const GLOW_STRENGTH = [0, 0.7] as const;
/** Glow is drawn at the nearest of this many steps from none to full (the slider's steps of 5%). */
export const GLOW_STEPS = 20;
/** A pixel's dot in the layout: its radius, in layout units (dots are drawn no smaller than they can be seen, though). */
export const DOT_RADIUS = 0.05;
/** Halos whose bell is narrower than this (device pixels) are lost under their dots: they aren't drawn. */
const NARROWEST_BELL = 1;
/** A halo's bell is at least this many pixels of the coarse picture wide, */
const BELL_CELLS = 2;
/** unless that would give the coarse picture more pixels than this (a huge canvas with tiny dots). */
const MOST_CELLS = 500_000;
/** A dot sits at one of this many places across a pixel of the coarse picture (each way), so its halo stays centered on it. */
const PLACES = 4;

/** The step (0 to `GLOW_STEPS`) a glow level (0–1) is drawn at; a level that isn't a number is none. */
export function glowStep(level: number): number {
  return Number.isFinite(level) ? Math.round(Math.min(1, Math.max(0, level)) * GLOW_STEPS) : 0;
}

const level = (step: number) => Math.min(GLOW_STEPS, step) / GLOW_STEPS;
/** A halo's bell width at a glow step, in dot radii. */
const bellWidth = (step: number) => GLOW_WIDTH[0] + (GLOW_WIDTH[1] - GLOW_WIDTH[0]) * level(step);
const strength = (step: number) => GLOW_STRENGTH[0] + (GLOW_STRENGTH[1] - GLOW_STRENGTH[0]) * level(step);

/**
 * How bright to draw lit dots (0–1 of their color) so that with their own halo added they come
 * out their own color: 1 less the halo's strength averaged over the dot. `spread` is the size
 * the halo is made for, as a share of the dot's drawn size (less than 1 for a dot drawn bigger).
 */
export function coreDim(step: number, spread = 1): number {
  if (step <= 0) return 1;
  const w = bellWidth(step) * spread;
  return 1 - strength(step) * w * w * (1 - Math.exp(-1 / (w * w)));
}

/**
 * A halo's weights (out of 256) for the square of pixels within `reach` of the one its dot is in,
 * row by row, and where each row's weights start and end (`from` up to `to`): the rest are zero.
 */
export interface Halo {
  reach: number;
  weights: Uint16Array;
  from: Uint8Array;
  to: Uint8Array;
}

/**
 * The halo of a dot of `radius` (pixels of the picture it's stamped into) at a glow step, for a
 * dot `fx`, `fy` (0–1) across its pixel.
 */
export function haloSprite(radius: number, step: number, fx = 0.5, fy = 0.5): Halo {
  const bell = radius * bellWidth(step);
  const reach = Math.ceil(radius + 2.2 * bell);
  const side = 2 * reach + 1;
  const weights = new Uint16Array(side * side);
  const [from, to] = [new Uint8Array(side).fill(side), new Uint8Array(side)];
  const peak = strength(step) * 256;
  for (let row = 0, k = 0; row < side; row++) {
    for (let col = 0; col < side; col++, k++) {
      const [x, y] = [col - reach + 0.5 - fx, row - reach + 0.5 - fy];
      const w = Math.round(peak * Math.exp(-(x * x + y * y) / (bell * bell)));
      if (w === 0) continue;
      weights[k] = w;
      if (from[row] > col) from[row] = col;
      to[row] = col + 1;
    }
  }
  return { reach, weights, from, to };
}

/** The red, green and blue of a picture being added up, each color times 256 (so halos can add past full brightness). */
export interface Sums {
  width: number;
  height: number;
  r: Int32Array;
  g: Int32Array;
  b: Int32Array;
}

export function newSums(width: number, height: number): Sums {
  const n = width * height;
  return { width, height, r: new Int32Array(n), g: new Int32Array(n), b: new Int32Array(n) };
}

/** A box of pixels: x0, y0 inside it, x1, y1 just past it. */
export interface PixelBox {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

// Halos made for the last radius and step asked for, by place across a pixel.
let sprites: { radius: number; step: number; made: (Halo | undefined)[] } = { radius: -1, step: -1, made: [] };

function spriteFor(radius: number, step: number, px: number, py: number): Halo {
  if (sprites.radius !== radius || sprites.step !== step) sprites = { radius, step, made: [] };
  return (sprites.made[py * PLACES + px] ??= haloSprite(radius, step, (px + 0.5) / PLACES, (py + 0.5) / PLACES));
}

/**
 * Adds the halo of every lit pixel to `sums`, a picture `scale` of its pixels to a screen pixel,
 * with dots of `radius` (its pixels) glowing at `step`. Returns the box it added to, or null when
 * nothing is lit.
 */
export function stampHalos(sums: Sums, props: PreviewProp[], frame: Uint8Array, view: View, size: Size, step: number, radius: number, scale: number): PixelBox | null {
  if (step <= 0) return null;
  const { width, height, r: reds, g: greens, b: blues } = sums;
  const [halfW, halfH, zoom, cx, cy] = [size.width / 2, size.height / 2, view.zoom, view.cx, view.cy];
  const reach = spriteFor(radius, step, 0, 0).reach;
  const side = 2 * reach + 1;
  const box = { x0: width, y0: height, x1: 0, y1: 0 };
  for (const p of props) {
    const pts = p.points;
    for (let i = 0, o = p.frameOffset; i + 1 < pts.length && o + 2 < frame.length; i += 2, o += p.channelsPerPixel) {
      const [r, g, b] = [frame[o], frame[o + 1], frame[o + 2]];
      if (r + g + b === 0) continue;
      const x = (halfW + (pts[i] - cx) * zoom) * scale;
      const y = (halfH - (pts[i + 1] - cy) * zoom) * scale;
      const [ix, iy] = [Math.floor(x), Math.floor(y)];
      const [x0, x1] = [Math.max(0, ix - reach), Math.min(width, ix + reach + 1)];
      const [y0, y1] = [Math.max(0, iy - reach), Math.min(height, iy + reach + 1)];
      if (x0 >= x1 || y0 >= y1) continue;
      const { weights, from, to } = spriteFor(radius, step, Math.floor((x - ix) * PLACES), Math.floor((y - iy) * PLACES));
      const left = ix - reach;
      for (let py = y0; py < y1; py++) {
        const row = py - iy + reach;
        const [a, b2] = [Math.max(x0, left + from[row]), Math.min(x1, left + to[row])];
        let k = row * side + (a - left);
        for (let at = py * width + a, end = py * width + b2; at < end; at++, k++) {
          const w = weights[k];
          reds[at] += r * w;
          greens[at] += g * w;
          blues[at] += b * w;
        }
      }
      if (x0 < box.x0) box.x0 = x0;
      if (y0 < box.y0) box.y0 = y0;
      if (x1 > box.x1) box.x1 = x1;
      if (y1 > box.y1) box.y1 = y1;
    }
  }
  return box.x0 < box.x1 ? box : null;
}

/**
 * Writes the part of `sums` in `box` into `rgba` (image bytes for the whole picture) as light to
 * add: each color as bright as it adds up to, see-through where there's none. Then empties that
 * part of `sums` for the next frame.
 */
export function sumsToImage(sums: Sums, box: PixelBox, rgba: Uint8ClampedArray) {
  const { width, r: reds, g: greens, b: blues } = sums;
  for (let y = box.y0; y < box.y1; y++) {
    for (let at = y * width + box.x0, end = y * width + box.x1; at < end; at++) {
      const [r, g, b] = [Math.min(255, reds[at] >> 8), Math.min(255, greens[at] >> 8), Math.min(255, blues[at] >> 8)];
      // The image holds colors before they're multiplied by how solid they are: as solid as the
      // brightest color, the others in proportion.
      const most = Math.max(r, g, b);
      const o = at * 4;
      rgba[o] = most ? (r * 255) / most : 0;
      rgba[o + 1] = most ? (g * 255) / most : 0;
      rgba[o + 2] = most ? (b * 255) / most : 0;
      rgba[o + 3] = most;
      reds[at] = greens[at] = blues[at] = 0;
    }
  }
}

/** The coarse picture halos are stamped into, kept between frames, and the part of it last drawn. */
let layer: { canvas: HTMLCanvasElement; ctx: CanvasRenderingContext2D; image: ImageData; sums: Sums; drawn: PixelBox | null } | null = null;

function haloLayer(width: number, height: number) {
  if (!layer || layer.sums.width !== width || layer.sums.height !== height) {
    const canvas = document.createElement("canvas");
    [canvas.width, canvas.height] = [width, height];
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    layer = { canvas, ctx, image: ctx.createImageData(width, height), sums: newSums(width, height), drawn: null };
  }
  return layer;
}

/**
 * How halos will be drawn on a canvas: at which glow step, around dots of what radius (screen
 * pixels), into which coarse picture, and how many of the canvas's device pixels across each of
 * its pixels is.
 */
export interface HaloPlan {
  step: number;
  radius: number;
  sums: Sums;
  cell: number;
}

/**
 * Gets ready to draw halos on `ctx`'s canvas (`ratio` device pixels per screen pixel) for a glow
 * level (0–1), in a view where dots are drawn with `radius` (screen pixels). Null when there's
 * no glow, the halos would be too small to see, or they can't be drawn here: then dots are drawn
 * as they are.
 */
export function planHalos(ctx: CanvasRenderingContext2D, glow: number, view: View, radius: number, ratio: number): HaloPlan | null {
  const step = glowStep(glow);
  if (step <= 0) return null;
  const around = Math.min(radius, view.zoom * DOT_RADIUS);
  const bell = around * ratio * bellWidth(step);
  if (!(bell >= NARROWEST_BELL)) return null;
  const { width, height } = ctx.canvas;
  // In quarters, so the coarse picture isn't made again for every little change of zoom.
  const cell = Math.max(1, Math.floor((4 * bell) / BELL_CELLS) / 4, Math.sqrt((width * height) / MOST_CELLS));
  const made = haloLayer(Math.ceil(width / cell), Math.ceil(height / cell));
  return made && { step, radius: around, sums: made.sums, cell };
}

/** Draws the halos of the lit pixels over what's on the canvas, adding to it. */
export function drawHalos(ctx: CanvasRenderingContext2D, plan: HaloPlan, props: PreviewProp[], frame: Uint8Array, view: View, size: Size, ratio: number) {
  if (!layer || layer.sums !== plan.sums) return;
  const { cell } = plan;
  const scale = ratio / cell;
  const box = stampHalos(plan.sums, props, frame, view, size, plan.step, plan.radius * scale, scale);
  if (!box) return;
  sumsToImage(plan.sums, box, layer.image.data);
  const [w, h] = [box.x1 - box.x0, box.y1 - box.y0];
  // What the last frame left outside this one's box must not show at its edges.
  const old = layer.drawn;
  if (old) layer.ctx.clearRect(old.x0, old.y0, old.x1 - old.x0, old.y1 - old.y0);
  layer.drawn = box;
  layer.ctx.putImageData(layer.image, 0, 0, box.x0, box.y0, w, h);
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalCompositeOperation = "lighter";
  ctx.imageSmoothingEnabled = true;
  ctx.drawImage(layer.canvas, box.x0, box.y0, w, h, box.x0 * cell, box.y0 * cell, w * cell, h * cell);
  ctx.restore();
}
