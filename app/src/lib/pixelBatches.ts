// Drawing every pixel of the layout as its own crisp dot, fast enough for big shows.
//
// A dot is sized from how far apart its prop's pixels are on screen, so it never reaches its
// neighbors: a matrix gets dots with gaps between them, a sparse string larger ones, up to the
// size the view asks for. Every dot is drawn in exactly its color in the frame. Pixels that are
// off (or not selected) are drawn first and lit (or selected) ones over them, so what's on top
// never depends on a pixel's color or its place in the wiring.
//
// Dots can't shrink forever: under a couple of screen pixels a light is too thin to see. A prop
// whose pixels are closer together than the smallest dot is drawn with that dot around every
// pixel, and where those dots overlap each place shows the pixel nearest to it: a dense string
// comes out as a thin even line, a dense matrix as a picture with no gaps, each cut cleanly
// between one pixel's color and the next. Pixels closer together than a screen pixel are merged
// first, into one light with their average color: a fair picture of their brightness that stays
// steady as the view changes size.
//
// The dots are stamped straight into an image, which is drawn in one go: thousands of fills, or
// one fill per color, are far slower, and an image costs the same however many colors it holds.

import type { PreviewProp } from "../api/types";
import type { Size, View } from "./layoutMath";
import { type HaloPlan, type PixelBox, coreDim, drawHalos, planHalos } from "./pixelGlow";

export { DOT_RADIUS } from "./pixelGlow";

/** The colors used when a pixel isn't showing a lit color of its own. */
export interface PixelColors {
  /** Not playing: an unselected prop's pixels. */
  unlit: string;
  /** Not playing: a selected prop's pixels. */
  selected: string;
  /** Playing, but this pixel is off (black). Left out, pixels that are off aren't drawn. */
  dark?: string;
}

/** How much of the way to its neighbor a dot may reach: the rest is the gap between them. */
const FILL = 0.7;
/** The smallest dot, in screen pixels: smaller, a light is too thin to see what color it is. */
export const SMALLEST = 2;
/**
 * Dots at least this wide (device pixels) are placed to the nearest of `PLACES` places across a
 * device pixel, so a grid of them comes out even. Smaller ones sit on whole device pixels: a
 * soft edge that moved about would blur them.
 */
export const FINE_FROM = 4;
const PLACES = 4;
/**
 * A light dimmer than this (its brightest channel, of 255) lets the dot of a pixel that's off
 * show through, the more the dimmer it is. So a light fading out ends as the dot it would have
 * when off, and a lit pixel is never darker than an unlit one.
 */
export const FAINT = 64;
/** How many of a prop's pixels are measured to find how far apart they are. */
const SAMPLES = 256;

const packed = new Map<string, number>();

/**
 * A CSS color (#rrggbb, rgb(), or rgba()) as an image pixel: its red, green, blue and alpha bytes
 * as one 32-bit number, red lowest (so a solid color is a negative number).
 */
export function packColor(css: string): number {
  let found = packed.get(css);
  if (found === undefined) {
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
    found = (a << 24) | (b << 16) | (g << 8) | r;
    packed.set(css, found);
  }
  return found;
}

/** How a prop's pixels are spread out: how far apart they are, and whether they have neighbors on every side. */
interface Spread {
  spacing: number;
  allRound: boolean;
}

const ALONE: Spread = { spacing: Infinity, allRound: false };
const spreads = new WeakMap<object, Spread>();

/** How a prop's pixels are spread out, measured once for each set of points. */
function spreadOf(points: ArrayLike<number>): Spread {
  let found = spreads.get(points);
  if (!found) {
    found = measureSpread(points);
    spreads.set(points, found);
  }
  return found;
}

/**
 * How far apart a prop's pixels are, in layout units: the distance from a pixel to its nearest
 * neighbor that a quarter of them are within (pixels on the very same spot aren't neighbors).
 * Infinity for a prop with no two pixels apart.
 */
export const pixelSpacing = (points: ArrayLike<number>): number => spreadOf(points).spacing;

/**
 * Whether a prop's pixels have neighbors on every side, as a matrix's do and a string's don't:
 * whether most of them have four or more within one and a half times their spacing.
 */
export const pixelsAllRound = (points: ArrayLike<number>): boolean => spreadOf(points).allRound;

function measureSpread(points: ArrayLike<number>): Spread {
  const n = points.length >> 1;
  if (n < 2) return ALONE;
  const picks = Math.min(n, SAMPLES);
  const pick = (k: number) => Math.floor(((k + 0.5) * n) / picks);
  const between = (i: number, j: number) => Math.hypot(points[2 * i] - points[2 * j], points[2 * i + 1] - points[2 * j + 1]);
  /** From pixel `i` to the nearer of the pixels wired before and after it that aren't on the same spot. */
  const toNext = (i: number) => {
    const [before, after] = [i > 0 ? between(i, i - 1) : 0, i + 1 < n ? between(i, i + 1) : 0];
    return before > 0 && after > 0 ? Math.min(before, after) : before || after;
  };

  // Along the wiring, a pixel's nearest neighbor is no farther than the next pixel.
  const along: number[] = [];
  for (let k = 0; k < picks; k++) {
    const d = toNext(pick(k));
    if (d > 0) along.push(d);
  }
  if (along.length === 0) return ALONE;
  along.sort((a, b) => a - b);
  const step = along[along.length >> 1];

  let [minX, minY, maxX, maxY] = [Infinity, Infinity, -Infinity, -Infinity];
  for (let i = 0; i < n; i++) {
    const [x, y] = [points[2 * i], points[2 * i + 1]];
    if (x < minX) minX = x;
    if (x > maxX) maxX = x;
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
  }
  const [w, h] = [maxX - minX, maxY - minY];
  // Every pixel goes in a grid of squares, so a pixel's neighbors are found in the squares
  // around it. A square is as wide as the step along the wiring, unless the wiring jumps about:
  // then a few pixels wide, going by how many pixels share the prop's box.
  const cell = Math.min(step, 4 * Math.max(Math.sqrt((w * h) / n), Math.max(w, h) / n));
  let buckets = 1;
  while (buckets < 2 * n) buckets <<= 1;
  const bucketOf = (x: number, y: number) => (Math.imul(x, 73856093) ^ Math.imul(y, 19349663)) & (buckets - 1);
  const column = (i: number) => Math.floor((points[2 * i] - minX) / cell);
  const row = (i: number) => Math.floor((points[2 * i + 1] - minY) / cell);
  const starts = new Int32Array(buckets + 1);
  for (let i = 0; i < n; i++) starts[bucketOf(column(i), row(i)) + 1]++;
  for (let b = 0; b < buckets; b++) starts[b + 1] += starts[b];
  const filled = starts.slice(0, buckets);
  const inBucket = new Int32Array(n);
  for (let i = 0; i < n; i++) inBucket[filled[bucketOf(column(i), row(i))]++] = i;

  const same = step * 1e-3;
  /** Calls `each` with how far pixel `i` is from every other pixel in its own square and those within `reach` squares of it. */
  const around = (i: number, reach: number, each: (d: number) => void) => {
    const [cx, cy] = [column(i), row(i)];
    for (let dy = -reach; dy <= reach; dy++) {
      for (let dx = -reach; dx <= reach; dx++) {
        const b = bucketOf(cx + dx, cy + dy);
        for (let at = starts[b]; at < starts[b + 1]; at++) {
          const j = inBucket[at];
          // Other squares can share the bucket.
          if (column(j) !== cx + dx || row(j) !== cy + dy) continue;
          const d = between(i, j);
          if (d > same) each(d);
        }
      }
    }
  };
  const nearest: number[] = [];
  for (let k = 0; k < picks; k++) {
    let best = Infinity;
    around(pick(k), 1, (d) => {
      if (d < best) best = d;
    });
    if (best === Infinity) best = toNext(pick(k));
    if (best > 0) nearest.push(best);
  }
  if (nearest.length === 0) return ALONE;
  nearest.sort((a, b) => a - b);
  const spacing = nearest[nearest.length >> 2];

  // Far enough to find every pixel within one and a half times the spacing (a few squares at most).
  const reach = Math.min(3, Math.ceil((spacing * 1.5) / cell));
  let surrounded = 0;
  for (let k = 0; k < picks; k++) {
    let near = 0;
    around(pick(k), reach, (d) => {
      if (d <= spacing * 1.5) near++;
    });
    if (near >= 4) surrounded++;
  }
  return { spacing, allRound: surrounded * 2 > picks };
}

/** How much room a dot `size` device pixels wide takes up: a dot placed finely spreads its soft edge over one more. */
const footprint = (size: number) => (size >= FINE_FROM ? size + 1 : size);

/**
 * How wide a dot is, in device pixels, for pixels `pitch` device pixels apart: most of the way
 * to its neighbor, always leaving a pixel clear between them, but no smaller than `smallest`
 * and no larger than `largest`.
 */
export function dotSize(pitch: number, smallest: number, largest: number): number {
  // A hair more room than there is, so pixels a whole number of device pixels apart don't lose
  // one to rounding.
  const room = pitch - 1 + 1e-6;
  let size = Math.min(largest, Math.floor(room), Math.round(pitch * FILL));
  if (footprint(size) > room) size--;
  return Math.max(smallest, size);
}

/**
 * A dot ready to stamp: the square of device pixels it's stamped in (`side` each way) and, for
 * each of them row by row, how much of it the dot covers (0–255; null when it covers them all)
 * and how far it is from the dot's middle (1 for the nearest, up to 255 the farther).
 */
export interface DotShape {
  side: number;
  cover: Uint8Array | null;
  apart: Uint8Array;
}

const shapes: DotShape[][] = [];

/**
 * The shapes of a round dot `size` device pixels wide: solid in the middle with a soft edge, as
 * the video export's dots are (crates/pf-video's raster.rs). There's one shape for a dot sitting
 * on whole device pixels (under 3 wide it's the whole square: too small to round), or with
 * `fine` one for each place it can sit across a device pixel, `PLACES` each way (row by row),
 * its soft edge following.
 */
export function dotShapes(size: number, fine = false): DotShape[] {
  let found = shapes[size * 2 + (fine ? 1 : 0)];
  if (!found) {
    const side = fine ? size + 1 : size;
    found = [];
    for (let place = 0; place < (fine ? PLACES * PLACES : 1); place++) {
      const cover = size < 3 ? null : new Uint8Array(side * side);
      const apart = new Uint8Array(side * side);
      const cx = (place % PLACES) / PLACES + size / 2;
      const cy = Math.floor(place / PLACES) / PLACES + size / 2;
      for (let y = 0, k = 0; y < side; y++) {
        for (let x = 0; x < side; x++, k++) {
          const away = Math.hypot(x + 0.5 - cx, y + 0.5 - cy);
          apart[k] = Math.min(255, 1 + Math.round(away * away * 16));
          if (cover) cover[k] = Math.round(255 * Math.min(1, Math.max(0, size / 2 + 0.5 - away)));
        }
      }
      found.push({ side, cover, apart });
    }
    shapes[size * 2 + (fine ? 1 : 0)] = found;
  }
  return found;
}

const squares: DotShape[][] = [];

/** The shape of a dot `size` device pixels wide that isn't rounded: its whole square, sitting on whole device pixels. */
function squareShape(size: number): DotShape[] {
  return (squares[size] ??= [{ ...dotShapes(size)[0], cover: null }]);
}

// Where `placeDot` last put a dot: the top left of its square, and which of its shapes to stamp.
let placedX = 0;
let placedY = 0;
let placedShape = 0;

/** Places a dot whose own top left would be at `left`, `top` (device pixels): on whole device pixels, or with `fine` to the nearest place across one. */
function placeDot(left: number, top: number, fine: boolean) {
  if (!fine) {
    placedX = Math.round(left);
    placedY = Math.round(top);
    placedShape = 0;
    return;
  }
  placedX = Math.floor(left);
  placedY = Math.floor(top);
  let across = Math.round((left - placedX) * PLACES);
  let down = Math.round((top - placedY) * PLACES);
  if (across === PLACES) {
    across = 0;
    placedX++;
  }
  if (down === PLACES) {
    down = 0;
    placedY++;
  }
  placedShape = down * PLACES + across;
}

/** Puts a color (red, green and blue as one number, and how solid it is of 255) over the image pixel at `at`. */
function blend(pixels: Int32Array, at: number, rgb: number, alpha: number) {
  const under = pixels[at];
  const underAlpha = under >>> 24;
  if (underAlpha === 0 || alpha === 255) {
    pixels[at] = (alpha << 24) | rgb;
    return;
  }
  // Both colors weighted by how much of each shows, out of 255 × 255.
  const top = alpha * 255;
  const rest = underAlpha * (255 - alpha);
  const all = top + rest;
  const half = all >> 1;
  const r = (((rgb & 255) * top + (under & 255) * rest + half) / all) | 0;
  const g = ((((rgb >> 8) & 255) * top + ((under >> 8) & 255) * rest + half) / all) | 0;
  const b = ((((rgb >> 16) & 255) * top + ((under >> 16) & 255) * rest + half) / all) | 0;
  pixels[at] = (Math.round(all / 255) << 24) | (b << 16) | (g << 8) | r;
}

/**
 * Stamps one dot with the top left of its square at `x0`, `y0`, clipped at the image's edges. A
 * dot stamped `over` covers what's there; otherwise it's one of the dots under the lit ones,
 * where two overlapping never add up to more than either alone.
 */
function stampDot(pixels: Int32Array, width: number, height: number, x0: number, y0: number, shape: DotShape, color: number, over: boolean) {
  const { side, cover } = shape;
  const left = Math.max(0, x0);
  const right = Math.min(width, x0 + side);
  const top = Math.max(0, y0);
  const bottom = Math.min(height, y0 + side);
  if (!cover) {
    for (let y = top; y < bottom; y++) for (let at = y * width + left, end = y * width + right; at < end; at++) pixels[at] = color;
    return;
  }
  const rgb = color & 0xffffff;
  const alpha = color >>> 24;
  for (let y = top; y < bottom; y++) {
    let k = (y - y0) * side + left - x0;
    for (let at = y * width + left, end = y * width + right; at < end; at++, k++) {
      const covered = cover[k];
      if (covered === 255) pixels[at] = color;
      else if (covered > 0) {
        const solid = ((covered * alpha + 127) / 255) | 0;
        if (over) blend(pixels, at, rgb, solid);
        else if (solid > pixels[at] >>> 24) pixels[at] = (solid << 24) | rgb;
      }
    }
  }
}

/**
 * Stamps one dot of a crowded prop: only where no dot stamped so far is nearer. `nearest` says
 * how near the dot shown at each image pixel is (0 for none).
 */
function claimDot(pixels: Int32Array, nearest: Uint8Array, width: number, height: number, x0: number, y0: number, shape: DotShape, color: number) {
  const { side, cover, apart } = shape;
  const left = Math.max(0, x0);
  const right = Math.min(width, x0 + side);
  const top = Math.max(0, y0);
  const bottom = Math.min(height, y0 + side);
  const rgb = color & 0xffffff;
  const alpha = color >>> 24;
  for (let y = top; y < bottom; y++) {
    let k = (y - y0) * side + left - x0;
    for (let at = y * width + left, end = y * width + right; at < end; at++, k++) {
      const covered = cover ? cover[k] : 255;
      if (covered === 0) continue;
      const shown = nearest[at];
      if (shown !== 0 && shown <= apart[k]) continue;
      nearest[at] = apart[k];
      pixels[at] = covered === 255 ? color : (((covered * alpha + 127) / 255) << 24) | rgb;
    }
  }
}

/** An image the dots are stamped into: its pixels (see `packColor`), row by row. */
export interface DotImage {
  pixels: Int32Array;
  width: number;
  height: number;
}

// Kept between frames, for the props whose pixels are too close together for dots of their own.
// `nearest` says, for every image pixel, how near the dot shown there is (0 for none). `mergedAt`
// says, for every screen pixel, which merged light is there (0 for none), and `merged` holds the
// merged lights: for each, its screen pixel, the sums of its lights' red, green and blue, of how
// solid they are and of where they are (x, y), how many they are, and whether any is of a prop
// whose pixels have neighbors on every side (1 or 0).
let nearest = new Uint8Array(0);
let mergedAt = new Int32Array(0);
let merged = new Float64Array(0);
const MERGED = 9;

/**
 * Stamps the props' pixels into `image`, a picture of the canvas `ratio` device pixels to a
 * screen pixel: their colors in `frame` while something plays, otherwise the unlit or selected
 * color. `radius` (screen pixels) is the largest a dot is drawn; a prop whose pixels are closer
 * together gets smaller ones (see `dotSize`), and one whose pixels are too close for the
 * smallest is drawn with that dot around every pixel, each place showing the pixel nearest to
 * it. With `halos`, lit dots are drawn dimmer by what their own glow adds back (see
 * pixelGlow.ts). Returns the box of the image it drew in, or null when nothing was drawn.
 */
export function stampPixels(
  image: DotImage,
  props: PreviewProp[],
  frame: Uint8Array | null,
  view: View,
  size: Size,
  selected: ReadonlySet<string>,
  colors: PixelColors,
  radius: number,
  ratio: number,
  halos: HaloPlan | null = null,
): PixelBox | null {
  const { pixels, width, height } = image;
  const screenPixel = Math.max(1, Math.round(ratio));
  const largest = Math.max(1, Math.round(2 * radius * ratio));
  const smallest = Math.min(largest, Math.max(1, Math.round(SMALLEST * ratio)));
  const [halfW, halfH, zoom, cx, cy] = [size.width / 2, size.height / 2, view.zoom, view.cx, view.cy];
  const box = { x0: width, y0: height, x1: 0, y1: 0 };
  const grow = (x0: number, y0: number, side: number) => {
    if (x0 < box.x0) box.x0 = x0;
    if (y0 < box.y0) box.y0 = y0;
    if (x0 + side > box.x1) box.x1 = x0 + side;
    if (y0 + side > box.y1) box.y1 = y0 + side;
  };
  // The dot of a pixel that's off, while something plays: its color as it is, and as light
  // (each channel times how solid the color is).
  const dark = frame && colors.dark !== undefined ? packColor(colors.dark) : 0;
  const darkAlpha = dark >>> 24;
  const darkR = ((dark & 255) * darkAlpha) / 255;
  const darkG = (((dark >> 8) & 255) * darkAlpha) / 255;
  const darkB = (((dark >> 16) & 255) * darkAlpha) / 255;
  const unlit = packColor(colors.unlit);
  const picked = packColor(colors.selected);
  const dimOf = (dot: number) => (halos ? coreDim(halos.step, (halos.radius * 2 * ratio) / dot) : 1);
  /** How far apart a prop's pixels are, in device pixels. */
  const pitchOf = (p: PreviewProp) => pixelSpacing(p.points) * zoom * ratio;
  /** Whether a prop's pixels are too close together for the smallest dots to stay apart. */
  const crowded = (p: PreviewProp) => pitchOf(p) < footprint(smallest) + 1;

  /** A lit pixel's color in the image: its own, or over the dot of a pixel that's off when it's faint. */
  const lit = (r: number, g: number, b: number, dim: number): number => {
    const most = Math.max(r, g, b);
    if (dim < 1) {
      r = Math.round(r * dim);
      g = Math.round(g * dim);
      b = Math.round(b * dim);
    }
    if (most >= FAINT) return (255 << 24) | (b << 16) | (g << 8) | r;
    const through = 1 - most / FAINT;
    const solid = most / FAINT + (through * darkAlpha) / 255;
    const red = Math.min(255, Math.round((r + through * darkR) / solid));
    const green = Math.min(255, Math.round((g + through * darkG) / solid));
    const blue = Math.min(255, Math.round((b + through * darkB) / solid));
    return (Math.round(solid * 255) << 24) | (blue << 16) | (green << 8) | red;
  };

  /** Stamps a prop's dots, each apart from the next: the lit (or selected) ones when `over`, otherwise the rest. */
  const stampProp = (p: PreviewProp, over: boolean) => {
    const pts = p.points;
    const dot = dotSize(pitchOf(p), smallest, largest);
    const fine = dot >= FINE_FROM;
    const places = dotShapes(dot, fine);
    const side = places[0].side;
    const half = dot / 2;
    const dim = dimOf(dot);
    let fixed = 0;
    if (!frame) {
      if (selected.has(p.prop) !== over) return;
      fixed = over ? picked : unlit;
    } else if (!over) {
      if (dark === 0) return;
      fixed = dark;
    }
    for (let i = 0, o = p.frameOffset; i + 1 < pts.length; i += 2, o += p.channelsPerPixel) {
      let color = fixed;
      if (frame) {
        const on = o + 2 < frame.length && (frame[o] | frame[o + 1] | frame[o + 2]) !== 0;
        if (on !== over) continue;
        if (on) color = lit(frame[o], frame[o + 1], frame[o + 2], dim);
      }
      placeDot((halfW + (pts[i] - cx) * zoom) * ratio - half, (halfH - (pts[i + 1] - cy) * zoom) * ratio - half, fine);
      if (placedX >= width || placedY >= height || placedX + side <= 0 || placedY + side <= 0) continue;
      stampDot(pixels, width, height, placedX, placedY, places[placedShape], color, over);
      grow(placedX, placedY, side);
    }
  };

  /**
   * Stamps a crowded prop: the smallest dot around every pixel, lit or not, each place showing
   * the pixel nearest to it. Where a prop's pixels have neighbors on every side and its dots
   * overlap, each is its whole square: round ones would leave the corners between four of them
   * half covered.
   */
  const stampCrowded = (p: PreviewProp) => {
    const pts = p.points;
    const square = pixelsAllRound(pts) && pitchOf(p) <= smallest;
    const fine = !square && smallest >= FINE_FROM;
    const places = square ? squareShape(smallest) : dotShapes(smallest, fine);
    const side = places[0].side;
    const half = smallest / 2;
    const dim = dimOf(smallest);
    const fixed = frame ? dark : selected.has(p.prop) ? picked : unlit;
    for (let i = 0, o = p.frameOffset; i + 1 < pts.length; i += 2, o += p.channelsPerPixel) {
      let color = fixed;
      if (frame && o + 2 < frame.length && (frame[o] | frame[o + 1] | frame[o + 2]) !== 0) color = lit(frame[o], frame[o + 1], frame[o + 2], dim);
      placeDot((halfW + (pts[i] - cx) * zoom) * ratio - half, (halfH - (pts[i + 1] - cy) * zoom) * ratio - half, fine);
      if (placedX >= width || placedY >= height || placedX + side <= 0 || placedY + side <= 0) continue;
      claimDot(pixels, nearest, width, height, placedX, placedY, places[placedShape], color);
      grow(placedX, placedY, side);
    }
  };

  let lights = 0;
  // Screen pixels across the image, and down it.
  const across = Math.ceil(width / screenPixel);
  const down = Math.ceil(height / screenPixel);

  /**
   * Adds the pixels of a prop that are closer together than a screen pixel to the merged lights:
   * those in the same screen pixel become one, in the middle of them, their colors averaged.
   */
  const mergeProp = (p: PreviewProp) => {
    const pts = p.points;
    const room = (lights + (pts.length >> 1) + 1) * MERGED;
    if (merged.length < room) {
      const longer = new Float64Array(Math.max(room, merged.length * 2));
      longer.set(merged.subarray(0, (lights + 1) * MERGED));
      merged = longer;
    }
    const dim = dimOf(smallest);
    const fixed = frame ? 0 : selected.has(p.prop) ? picked : unlit;
    const fixedSolid = (fixed >>> 24) / 255;
    const allRound = pixelsAllRound(pts);
    for (let i = 0, o = p.frameOffset; i + 1 < pts.length; i += 2, o += p.channelsPerPixel) {
      const x = (halfW + (pts[i] - cx) * zoom) * ratio;
      const y = (halfH - (pts[i + 1] - cy) * zoom) * ratio;
      if (x <= -smallest || y <= -smallest || x >= width + smallest || y >= height + smallest) continue;
      // A pixel just off the image still reaches into it: it's a light of its own.
      const inside = x >= 0 && y >= 0 && x < width && y < height;
      const pixel = inside ? Math.floor(y / screenPixel) * across + Math.floor(x / screenPixel) : -1;
      let at = (inside ? mergedAt[pixel] : 0) * MERGED;
      if (at === 0) {
        at = ++lights * MERGED;
        if (inside) mergedAt[pixel] = lights;
        merged[at] = pixel;
        merged[at + 1] = merged[at + 2] = merged[at + 3] = merged[at + 4] = merged[at + 5] = merged[at + 6] = merged[at + 7] = merged[at + 8] = 0;
      }
      merged[at + 5] += x;
      merged[at + 6] += y;
      merged[at + 7]++;
      if (allRound) merged[at + 8] = 1;
      if (!frame) {
        merged[at + 1] += (fixed & 255) * fixedSolid;
        merged[at + 2] += ((fixed >> 8) & 255) * fixedSolid;
        merged[at + 3] += ((fixed >> 16) & 255) * fixedSolid;
        merged[at + 4] += fixedSolid;
      } else if (o + 2 < frame.length) {
        merged[at + 1] += frame[o] * dim;
        merged[at + 2] += frame[o + 1] * dim;
        merged[at + 3] += frame[o + 2] * dim;
        merged[at + 4] += Math.min(1, Math.max(frame[o], frame[o + 1], frame[o + 2]) / FAINT);
      }
    }
  };

  /** Stamps the merged lights as crowded pixels are, each on whole device pixels. */
  const stampMerged = () => {
    for (let at = MERGED; at <= lights * MERGED; at += MERGED) if (merged[at] >= 0) mergedAt[merged[at]] = 0;
    const round = dotShapes(smallest)[0];
    const square = squareShape(smallest)[0];
    const half = smallest / 2;
    for (let at = MERGED; at <= lights * MERGED; at += MERGED) {
      const count = merged[at + 7];
      // What isn't light is the dot of a pixel that's off, showing through.
      const through = count - merged[at + 4];
      const solid = (merged[at + 4] + (through * darkAlpha) / 255) / count;
      let color = 0;
      if (solid > 0) {
        const share = 1 / (count * solid);
        const red = Math.min(255, Math.round((merged[at + 1] + through * darkR) * share));
        const green = Math.min(255, Math.round((merged[at + 2] + through * darkG) * share));
        const blue = Math.min(255, Math.round((merged[at + 3] + through * darkB) * share));
        color = (Math.round(solid * 255) << 24) | (blue << 16) | (green << 8) | red;
      }
      const x0 = Math.round(merged[at + 5] / count - half);
      const y0 = Math.round(merged[at + 6] / count - half);
      if (x0 >= width || y0 >= height || x0 + smallest <= 0 || y0 + smallest <= 0) continue;
      claimDot(pixels, nearest, width, height, x0, y0, merged[at + 8] ? square : round, color);
      grow(x0, y0, smallest);
    }
  };

  // Crowded props go under everything: what's stamped over them covers them by the usual rules.
  if (props.some(crowded)) {
    if (nearest.length < width * height) nearest = new Uint8Array(width * height);
    if (props.some((p) => pitchOf(p) < screenPixel)) {
      if (mergedAt.length < across * down) mergedAt = new Int32Array(across * down);
      for (const p of props) if (pitchOf(p) < screenPixel) mergeProp(p);
      stampMerged();
    }
    for (const p of props) if (crowded(p) && pitchOf(p) >= screenPixel) stampCrowded(p);
    for (let y = Math.max(0, box.y0); y < Math.min(height, box.y1); y++) nearest.fill(0, y * width + Math.max(0, box.x0), y * width + Math.min(width, box.x1));
  }
  for (const p of props) if (!crowded(p)) stampProp(p, false);
  for (const p of props) if (!crowded(p)) stampProp(p, true);
  if (box.x0 >= box.x1) return null;
  return { x0: Math.max(0, box.x0), y0: Math.max(0, box.y0), x1: Math.min(width, box.x1), y1: Math.min(height, box.y1) };
}

/** The image a canvas's dots are stamped into, kept between frames, and the part of it last drawn in. */
interface DotLayer extends DotImage {
  canvas: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D;
  data: ImageData;
  drawn: PixelBox | null;
}

const layers = new WeakMap<HTMLCanvasElement, DotLayer>();

function layerFor(target: HTMLCanvasElement): DotLayer | null {
  const { width, height } = target;
  // A canvas that hasn't been laid out yet has no size, and nothing can be drawn on it.
  if (width === 0 || height === 0) return null;
  let layer = layers.get(target);
  if (!layer || layer.width !== width || layer.height !== height) {
    const canvas = document.createElement("canvas");
    [canvas.width, canvas.height] = [width, height];
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    const data = ctx.createImageData(width, height);
    layer = { canvas, ctx, data, pixels: new Int32Array(data.data.buffer), width, height, drawn: null };
    layers.set(target, layer);
  }
  return layer;
}

/**
 * Draws the props' pixels (see `stampPixels`) on a canvas whose transform is `ratio` device
 * pixels per screen pixel, over what's there. Then, while something plays, the glow around the
 * lit ones: as much as `glow` (0–1) says, none at 0 (see pixelGlow.ts).
 */
export function drawPixels(
  ctx: CanvasRenderingContext2D,
  props: PreviewProp[],
  frame: Uint8Array | null,
  view: View,
  size: Size,
  selected: ReadonlySet<string>,
  colors: PixelColors,
  radius: number,
  ratio: number,
  glow = 0,
) {
  const halos = frame && glow > 0 ? planHalos(ctx, glow, view, radius, ratio) : null;
  const layer = layerFor(ctx.canvas);
  if (layer) {
    // Only where the last frame's dots were needs emptying.
    const old = layer.drawn;
    if (old) for (let y = old.y0; y < old.y1; y++) layer.pixels.fill(0, y * layer.width + old.x0, y * layer.width + old.x1);
    const box = stampPixels(layer, props, frame, view, size, selected, colors, radius, ratio, halos);
    layer.drawn = box;
    if (box) {
      const [w, h] = [box.x1 - box.x0, box.y1 - box.y0];
      layer.ctx.putImageData(layer.data, 0, 0, box.x0, box.y0, w, h);
      ctx.save();
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.drawImage(layer.canvas, box.x0, box.y0, w, h, box.x0, box.y0, w, h);
      ctx.restore();
    }
  }
  if (halos) drawHalos(ctx, halos, props, frame!, view, size, ratio);
}
