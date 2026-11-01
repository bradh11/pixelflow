// Drawing every pixel of the layout as its own crisp dot, fast enough for big shows.
//
// A dot only has to shrink where it has neighbors to run into, so it's sized from how far apart
// its prop's pixels are on screen (measured once for each set of points): a matrix gets dots
// with gaps between them, a sparse string the view's whole dot. Every dot is drawn in exactly its
// color in the frame. Pixels that are off (or not selected) are drawn first and lit (or selected)
// ones over them, so what's on top never depends on a pixel's color or its place in the wiring.
//
// Along a string the only neighbors are the next pixels on the wire: there's nothing to its
// side. So where a prop's pixels are too close along the wire for dots of their own, they run
// together instead of shrinking: every pixel gets a dot as wide as the room to its side allows
// (most of the view's dot, for a string), and where those dots overlap each place shows the
// pixel nearest to it. A dense outline comes out as an even line of the weight a sparse string's
// dots have, the rows of a matrix as bars with the gaps between them kept, a dense matrix as a
// picture with no gaps, each cut cleanly between one pixel's color and the next. Where two
// props' dots meet, the nearer pixel shows too, but a lit one always over one that isn't.
// Pixels closer together than a screen pixel are merged first, into one light with their average
// color: a fair picture of their brightness that stays steady as the view changes size.
//
// The dots are stamped straight into an image, which is drawn in one go: thousands of fills, or
// one fill per color, are far slower, and an image costs the same however many colors it holds.
//
// The video export stamps the same picture with code of its own (crates/pf-video's dots.rs, step
// for step what `stampPixels` does here). The scenes in api/contracts/pixels/ hold the two to
// each other: a change to what's drawn here shows there, and the export's tests then fail until
// it draws the same.

import type { PreviewProp } from "../api/types";
import type { Size, View } from "./layoutMath";
import { DOT_RADIUS, type HaloPlan, type PixelBox, coreDim, drawHalos, planHalos } from "./pixelGlow";
import { FLOOD_STRENGTH, UNLIT_WASH, washAt, washRadius } from "./floodWash";
import { ARM_COLOR, BEAM_REACH, aimAt, aimOutline, armAt, armBox, armShape, beamAt, beamColumns, beamLight, beamRows, beamShape, beamWidth, flareAt, flareBox, flareShape, headDirection, servoDirection } from "./beams";

export { DOT_RADIUS };

/** The colors used when a pixel isn't showing a lit color of its own. */
export interface PixelColors {
  /** Not playing: an unselected prop's pixels. */
  unlit: string;
  /** Not playing: a selected prop's pixels. */
  selected: string;
  /** Playing, but this pixel is off (black). Left out, pixels that are off aren't drawn. */
  dark?: string;
  /** Not playing: the outline of where a selected moving head points. Left out, none is drawn. */
  aim?: string;
}

/** How much of the way to its neighbor a dot may reach: the rest is the gap between them. */
export const FILL = 0.7;
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
/**
 * How far to a pixel's side is looked at for other pixels, in layout units (five of a pixel's
 * own dots): farther off than that, nothing is in the way of a dot at any zoom worth drawing.
 * The squares pixels are sorted into are no smaller than a `REACH_SQUARES`th of it.
 */
export const REACH = 10 * DOT_RADIUS;
const REACH_SQUARES = 5;
/**
 * A pixel is to another's side, not along its line, when the way to it is more than this off
 * the line to that pixel's nearest neighbor (as a cosine: about 37°).
 */
const SIDEWAYS = 0.8;

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

/**
 * How a prop's pixels are spread out: how far apart they are, how far they are from the pixels
 * to their side, and whether they have neighbors on every side.
 */
export interface Spread {
  spacing: number;
  across: number;
  allRound: boolean;
}

const ALONE: Spread = { spacing: Infinity, across: Infinity, allRound: false };
const spreads = new WeakMap<object, Spread>();

/** How a prop's pixels are spread out, measured once for each set of points (x, y pairs). */
function spreadOf(points: ArrayLike<number>): Spread {
  let found = spreads.get(points);
  if (!found) {
    found = measureSpread(points, 2);
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
 * How much room a prop's pixels have to their side, in layout units: the distance from a pixel
 * to the nearest one that isn't along the line to its nearest neighbor (more than `SIDEWAYS`
 * off it, either way), that a quarter of them are within. Along a string there's nothing to the
 * side, and it's Infinity; so it is when the nearest is more than `REACH` away. Across a matrix
 * it's the way between its rows.
 */
export const pixelRoom = (points: ArrayLike<number>): number => spreadOf(points).across;

/**
 * Whether a prop's pixels have neighbors on every side, as a matrix's do and a string's don't:
 * whether most of them have four or more within one and a half times their spacing.
 */
export const pixelsAllRound = (points: ArrayLike<number>): boolean => spreadOf(points).allRound;

/**
 * How a prop's pixels are spread out where they stand in the layout (x, y, z triples), for the 3D
 * view: the same measure as the flat drawings take, with every distance taken in depth too. For
 * a prop that's flat and faces the front, it's what they measure.
 *
 * A prop with depth (a tree, whose strings at the back show between the ones in front) has no
 * more room to its pixels' side than the flat drawings find looking at it from the front: its
 * dots are then no bolder in 3D than they are flat.
 *
 * Measured once for each set of points. `same`: a set measured before (the prop's points before
 * the layout last changed, say): when `xyz` holds the very same numbers, it isn't measured again.
 */
export function pixelSpread3d(xyz: ArrayLike<number>, same?: ArrayLike<number>): Spread {
  let found = spreads.get(xyz);
  if (!found && same && same.length === xyz.length) {
    let alike = true;
    for (let i = 0; i < xyz.length && alike; i++) alike = xyz[i] === same[i];
    if (alike) found = spreads.get(same);
  }
  if (!found) {
    found = measureSpread(xyz, 3);
    const count = Math.floor(xyz.length / 3);
    let flat = true;
    for (let i = 1; i < count && flat; i++) flat = xyz[3 * i + 2] === xyz[2];
    if (!flat) {
      const front = new Float64Array(count * 2);
      for (let i = 0; i < count; i++) {
        front[2 * i] = xyz[3 * i];
        front[2 * i + 1] = xyz[3 * i + 1];
      }
      const { across } = measureSpread(front, 2);
      if (across < found.across) found = { ...found, across };
    }
  }
  spreads.set(xyz, found);
  return found;
}

function measureSpread(points: ArrayLike<number>, stride: 2 | 3): Spread {
  const n = Math.floor(points.length / stride);
  if (n < 2) return ALONE;
  const deep = stride === 3;
  const picks = Math.min(n, SAMPLES);
  const pick = (k: number) => Math.floor(((k + 0.5) * n) / picks);
  const between = deep
    ? (i: number, j: number) => {
        const [dx, dy, dz] = [points[3 * i] - points[3 * j], points[3 * i + 1] - points[3 * j + 1], points[3 * i + 2] - points[3 * j + 2]];
        // Two pixels at one depth are as far apart as the flat drawings find them, to the last digit.
        return dz === 0 ? Math.hypot(dx, dy) : Math.sqrt(dx * dx + dy * dy + dz * dz);
      }
    : (i: number, j: number) => Math.hypot(points[2 * i] - points[2 * j], points[2 * i + 1] - points[2 * j + 1]);
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

  let [minX, minY, minZ, maxX, maxY, maxZ] = [Infinity, Infinity, Infinity, -Infinity, -Infinity, -Infinity];
  for (let i = 0; i < n; i++) {
    const [x, y] = [points[stride * i], points[stride * i + 1]];
    if (x < minX) minX = x;
    if (x > maxX) maxX = x;
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
    if (!deep) continue;
    const z = points[3 * i + 2];
    if (z < minZ) minZ = z;
    if (z > maxZ) maxZ = z;
  }
  // The two widest ways across the prop's box: the face of a flat prop, whichever way it's turned.
  const [w, h] = deep ? [maxX - minX, maxY - minY, maxZ - minZ].sort((a, b) => b - a) : [maxX - minX, maxY - minY];
  // Every pixel goes in a grid of squares (cubes, with depth), so a pixel's neighbors are found in
  // the squares around it. A square is as wide as the step along the wiring, unless the wiring
  // jumps about (then a few pixels wide, going by how many pixels share the prop's box), and wide
  // enough that a few squares each way reach as far as a pixel's side is looked at.
  const cell = Math.max(REACH / REACH_SQUARES, Math.min(step, 4 * Math.max(Math.sqrt((w * h) / n), Math.max(w, h) / n)));
  let buckets = 1;
  while (buckets < 2 * n) buckets <<= 1;
  const bucketOf = (x: number, y: number, z: number) => (Math.imul(x, 73856093) ^ Math.imul(y, 19349663) ^ Math.imul(z, 83492791)) & (buckets - 1);
  const column = (i: number) => Math.floor((points[stride * i] - minX) / cell);
  const row = (i: number) => Math.floor((points[stride * i + 1] - minY) / cell);
  const layer = deep ? (i: number) => Math.floor((points[3 * i + 2] - minZ) / cell) : () => 0;
  // The last column, row and layer of squares with pixels in them: none are looked for past them.
  const lastColumn = Math.floor((maxX - minX) / cell);
  const lastRow = Math.floor((maxY - minY) / cell);
  const lastLayer = deep ? Math.floor((maxZ - minZ) / cell) : 0;
  const starts = new Int32Array(buckets + 1);
  for (let i = 0; i < n; i++) starts[bucketOf(column(i), row(i), layer(i)) + 1]++;
  for (let b = 0; b < buckets; b++) starts[b + 1] += starts[b];
  const filled = starts.slice(0, buckets);
  const inBucket = new Int32Array(n);
  for (let i = 0; i < n; i++) inBucket[filled[bucketOf(column(i), row(i), layer(i))]++] = i;

  const same = step * 1e-3;
  /**
   * Calls `each` with every other pixel in the squares `ring` squares out from pixel `i`'s own
   * (its own for 0), and how far from it each is.
   */
  const around = (i: number, ring: number, each: (d: number, j: number) => void) => {
    const [cx, cy, cz] = [column(i), row(i), layer(i)];
    const [x0, x1] = [Math.max(-ring, -cx), Math.min(ring, lastColumn - cx)];
    for (let dz = Math.max(-ring, -cz); dz <= Math.min(ring, lastLayer - cz); dz++) {
      const face = dz === -ring || dz === ring;
      for (let dy = Math.max(-ring, -cy); dy <= Math.min(ring, lastRow - cy); dy++) {
        // Inside the ring, only its two ends on this row are part of it.
        const ends = !face && dy !== -ring && dy !== ring && ring > 0;
        for (let dx = ends ? -ring : x0; dx <= x1; dx += ends ? 2 * ring : 1) {
          if (dx < x0) continue;
          const b = bucketOf(cx + dx, cy + dy, cz + dz);
          for (let at = starts[b]; at < starts[b + 1]; at++) {
            const j = inBucket[at];
            // Other squares can share the bucket.
            if (column(j) !== cx + dx || row(j) !== cy + dy || layer(j) !== cz + dz) continue;
            const d = between(i, j);
            if (d > same) each(d, j);
          }
        }
      }
    }
  };

  const nearest: number[] = [];
  const sideways: number[] = [];
  for (let k = 0; k < picks; k++) {
    const i = pick(k);
    let [best, toward] = [Infinity, -1];
    for (let ring = 0; ring <= 1; ring++) {
      around(i, ring, (d, j) => {
        if (d < best) [best, toward] = [d, j];
      });
    }
    if (toward < 0) {
      if (toNext(i) > 0) nearest.push(toNext(i));
      continue;
    }
    nearest.push(best);
    // The nearest pixel that isn't along the line to that one, looking a ring of squares
    // farther out until nothing nearer can be left to find.
    const [ux, uy] = [(points[stride * toward] - points[stride * i]) / best, (points[stride * toward + 1] - points[stride * i + 1]) / best];
    const uz = deep ? (points[3 * toward + 2] - points[3 * i + 2]) / best : 0;
    let side = Infinity;
    for (let ring = 0; ring <= REACH_SQUARES && side > (ring - 1) * cell; ring++) {
      around(i, ring, (d, j) => {
        if (d >= side) return;
        let along = (points[stride * j] - points[stride * i]) * ux + (points[stride * j + 1] - points[stride * i + 1]) * uy;
        if (deep) along += (points[3 * j + 2] - points[3 * i + 2]) * uz;
        const lined = Math.abs(along) / d;
        if (lined < SIDEWAYS) side = d;
      });
    }
    if (side <= REACH) sideways.push(side);
  }
  if (nearest.length === 0) return ALONE;
  nearest.sort((a, b) => a - b);
  const spacing = nearest[nearest.length >> 2];
  sideways.sort((a, b) => a - b);
  // A quarter of the pixels looked at, not of those with something to their side.
  const across = sideways.length > picks >> 2 ? sideways[picks >> 2] : Infinity;

  // Far enough to find every pixel within one and a half times the spacing (a few squares at most).
  const reach = Math.min(3, Math.ceil((spacing * 1.5) / cell));
  let surrounded = 0;
  for (let k = 0; k < picks; k++) {
    let near = 0;
    for (let ring = 0; ring <= reach; ring++) {
      around(pick(k), ring, (d) => {
        if (d <= spacing * 1.5) near++;
      });
    }
    if (near >= 4) surrounded++;
  }
  return { spacing, across, allRound: surrounded * 2 > picks };
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
 * The shapes of a round dot `size` device pixels wide: solid in the middle with a soft edge.
 * There's one shape for a dot sitting on whole device pixels (under 3 wide it's the whole square:
 * too small to round), or with `fine` one for each place it can sit across a device pixel,
 * `PLACES` each way (row by row), its soft edge following.
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
 * Adds light (red, green and blue as one number, `amount` of it out of 255, more where it's that
 * bright) to the image pixel at `at`, as light adds where beams cross in haze: the pixel's color
 * and the light's, each times how much of it there is, summed. Too bright to show, it's drawn as
 * bright as its hue goes (its brightest part at 255), not washed toward white.
 */
export function shine(pixels: Int32Array, at: number, rgb: number, amount: number) {
  const under = pixels[at];
  const underAlpha = under >>> 24;
  const r = (under & 255) * underAlpha + (rgb & 255) * amount;
  const g = ((under >> 8) & 255) * underAlpha + ((rgb >> 8) & 255) * amount;
  const b = ((under >> 16) & 255) * underAlpha + ((rgb >> 16) & 255) * amount;
  const most = Math.max(r, g, b);
  if (most > 65025) {
    const half = Math.floor(most / 2);
    const full = (c: number) => Math.floor((c * 255 + half) / most);
    pixels[at] = (255 << 24) | (full(b) << 16) | (full(g) << 8) | full(r);
    return;
  }
  const alpha = Math.floor((most + 127) / 255);
  if (alpha === 0) return;
  const half = alpha >> 1;
  const part = (c: number) => Math.min(255, Math.floor((c + half) / alpha));
  pixels[at] = (alpha << 24) | (part(b) << 16) | (part(g) << 8) | part(r);
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
 * Stamps one dot of a crowded prop: only where no dot stamped so far is nearer. `nearest` says,
 * for each image pixel, whose dot is shown there and how near it is (0 for none): the dot's
 * `tag` times 256, plus how far the pixel is from its middle (see `DotShape`). A tag is a prop's
 * number (up to 127) times two, plus one for a lit (or selected) pixel's dot. Between two props,
 * a lit pixel's dot stays over one that isn't, whichever is nearer.
 */
function claimDot(pixels: Int32Array, nearest: Uint16Array, width: number, height: number, x0: number, y0: number, shape: DotShape, color: number, tag: number) {
  const { side, cover, apart } = shape;
  const left = Math.max(0, x0);
  const right = Math.min(width, x0 + side);
  const top = Math.max(0, y0);
  const bottom = Math.min(height, y0 + side);
  const rgb = color & 0xffffff;
  const alpha = color >>> 24;
  const mine = tag << 8;
  for (let y = top; y < bottom; y++) {
    let k = (y - y0) * side + left - x0;
    for (let at = y * width + left, end = y * width + right; at < end; at++, k++) {
      const covered = cover ? cover[k] : 255;
      if (covered === 0) continue;
      const shown = nearest[at];
      if (shown !== 0) {
        const other = (shown >>> 8) ^ tag;
        if (other > 1 && (other & 1) !== 0) {
          if ((tag & 1) === 0) continue;
          // Over another prop's unlit dot, a soft edge lets it show through.
          if (covered < 255) {
            nearest[at] = mine | apart[k];
            blend(pixels, at, rgb, ((covered * alpha + 127) / 255) | 0);
            continue;
          }
        } else if ((shown & 255) <= apart[k]) continue;
      }
      nearest[at] = mine | apart[k];
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

// Kept between frames. `sizes` and `ways` say how each prop is drawn this frame: how wide its
// dots are (device pixels), and whether they stand apart, run together (crowded), or are merged
// first. For the props whose dots run together, `nearest` says for every image pixel whose dot
// is shown there and how near it is (see `claimDot`). `mergedAt` says, for every screen pixel,
// which merged light is there (0 for none), and `merged` holds the merged lights: for each, its
// screen pixel, the sums of its lights' red, green and blue, of how solid they are and of where
// they are (x, y), how many they are, and how it's stamped: the widest dot any of its lights is
// drawn with times 4, plus 2 when any is of a prop whose pixels have neighbors on every side,
// plus 1 when any is lit (or selected).
let sizes = new Uint16Array(0);
let ways = new Uint8Array(0);
const APART = 0;
const CROWDED = 1;
const MERGED_FIRST = 2;
let nearest = new Uint16Array(0);
let mergedAt = new Int32Array(0);
let merged = new Float64Array(0);
const MERGED = 9;

/**
 * Stamps the props' pixels into `image`, a picture of the canvas `ratio` device pixels to a
 * screen pixel: their colors in `frame` while something plays, otherwise the unlit or selected
 * color. `radius` (screen pixels) is the largest a dot is drawn. A prop's dots are no bigger than
 * leaves them clear of its pixels to their side (see `dotSize`). Where its pixels are too close
 * along the wire for that, they run together: every pixel gets a dot, as wide as the room to its
 * side allows, and each place shows the pixel nearest to it. With `halos`, lit dots are drawn
 * dimmer by what their own glow adds back (see pixelGlow.ts). Returns the box of the image it
 * drew in, or null when nothing was drawn.
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
  // A line of pixels run together is this wide at most: one as wide as the view's whole dot
  // would weigh more than a row of those dots standing apart.
  const widestLine = Math.max(smallest, Math.round(largest * FILL));
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
  const aim = colors.aim === undefined ? 0 : packColor(colors.aim);
  const dimOf = (dot: number) => (halos ? coreDim(halos.step, (halos.radius * 2 * ratio) / dot) : 1);

  // How each prop is drawn: its dots as big as its pixels' room along the wire allows, but no
  // thinner than the line they make where they run together.
  if (sizes.length < props.length) {
    sizes = new Uint16Array(props.length);
    ways = new Uint8Array(props.length);
  }
  let anyCrowded = false;
  let anyMerged = false;
  for (let n = 0; n < props.length; n++) {
    const spread = spreadOf(props[n].points);
    // A hair more than it is, as in `dotSize`.
    const along = spread.spacing * zoom * ratio + 1e-6;
    // Nothing found to a pixel's side means nothing within `REACH`: that much room is sure.
    const line = Math.min(dotSize(Math.min(spread.across, REACH) * zoom * ratio, smallest, largest), widestLine);
    const dot = Math.max(dotSize(along, smallest, largest), line);
    sizes[n] = dot;
    ways[n] = along < screenPixel ? MERGED_FIRST : along < footprint(dot) + 1 ? CROWDED : APART;
    if (ways[n] !== APART) anyCrowded = true;
    if (ways[n] === MERGED_FIRST) anyMerged = true;
  }

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
  const stampProp = (p: PreviewProp, dot: number, over: boolean) => {
    const pts = p.points;
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
   * Stamps a crowded prop, the `number`th: a dot around every pixel, lit or not, each place
   * showing the pixel nearest to it. Where a prop's pixels have neighbors on every side and its
   * dots overlap, each is its whole square: round ones would leave the corners between four of
   * them half covered.
   */
  const stampCrowded = (p: PreviewProp, dot: number, number: number) => {
    const pts = p.points;
    const square = pixelsAllRound(pts) && pixelSpacing(pts) * zoom * ratio <= dot;
    const fine = !square && dot >= FINE_FROM;
    const places = square ? squareShape(dot) : dotShapes(dot, fine);
    const side = places[0].side;
    const half = dot / 2;
    const dim = dimOf(dot);
    const fixed = frame ? dark : selected.has(p.prop) ? picked : unlit;
    // 0 is the merged lights' number.
    const fixedTag = ((1 + (number % 127)) << 1) | (!frame && selected.has(p.prop) ? 1 : 0);
    for (let i = 0, o = p.frameOffset; i + 1 < pts.length; i += 2, o += p.channelsPerPixel) {
      let color = fixed;
      let tag = fixedTag;
      if (frame && o + 2 < frame.length && (frame[o] | frame[o + 1] | frame[o + 2]) !== 0) {
        color = lit(frame[o], frame[o + 1], frame[o + 2], dim);
        tag |= 1;
      }
      placeDot((halfW + (pts[i] - cx) * zoom) * ratio - half, (halfH - (pts[i + 1] - cy) * zoom) * ratio - half, fine);
      if (placedX >= width || placedY >= height || placedX + side <= 0 || placedY + side <= 0) continue;
      claimDot(pixels, nearest, width, height, placedX, placedY, places[placedShape], color, tag);
      grow(placedX, placedY, side);
    }
  };

  /**
   * Stamps a flood's wash round each of its cells, as solid as `washAt` says, in the cell's color
   * (lit) or the unlit color faintly (not playing), over what's there (see floodWash.ts).
   */
  const stampWashes = (p: PreviewProp) => {
    const pts = p.points;
    const reach = washRadius(zoom) * ratio;
    const reach2 = reach * reach;
    const strength = frame ? FLOOD_STRENGTH : UNLIT_WASH;
    for (let i = 0, o = p.frameOffset; i + 1 < pts.length; i += 2, o += p.channelsPerPixel) {
      let rgb = unlit & 0xffffff;
      if (frame) {
        if (!(o + 2 < frame.length && (frame[o] | frame[o + 1] | frame[o + 2]) !== 0)) continue;
        rgb = (frame[o + 2] << 16) | (frame[o + 1] << 8) | frame[o];
      }
      const x = (halfW + (pts[i] - cx) * zoom) * ratio;
      const y = (halfH - (pts[i + 1] - cy) * zoom) * ratio;
      const x0 = Math.max(0, Math.floor(x - reach));
      const x1 = Math.min(width, Math.ceil(x + reach));
      const y0 = Math.max(0, Math.floor(y - reach));
      const y1 = Math.min(height, Math.ceil(y + reach));
      if (x0 >= x1 || y0 >= y1) continue;
      for (let py = y0; py < y1; py++) {
        const dy = py + 0.5 - y;
        for (let px = x0; px < x1; px++) {
          const dx = px + 0.5 - x;
          const alpha = Math.floor(strength * washAt((dx * dx + dy * dy) / reach2) * 255 + 0.5);
          if (alpha > 0) blend(pixels, py * width + px, rgb, alpha);
        }
      }
      if (x0 < box.x0) box.x0 = x0;
      if (y0 < box.y0) box.y0 = y0;
      if (x1 > box.x1) box.x1 = x1;
      if (y1 > box.y1) box.y1 = y1;
    }
  };

  /**
   * Stamps a moving head's beam and its lens flare from its first cell while something plays, in
   * its color and as bright as it is (nothing while it's dark), their light added to what's
   * there; or, not playing, the outline of where a selected head points when `colors.aim` says
   * how; and a servo's arm at each of its cells, over what's there (see beams.ts).
   */
  const stampMotion = (p: PreviewProp) => {
    const motion = p.motion!;
    const pts = p.points;
    const placeX = (i: number) => (halfW + (pts[2 * i] - cx) * zoom) * ratio;
    const placeY = (i: number) => (halfH - (pts[2 * i + 1] - cy) * zoom) * ratio;
    const cells = pts.length >> 1;
    const stretch = (x0: number, x1: number, y0: number, y1: number) => {
      if (x0 >= x1 || y0 >= y1) return;
      if (x0 < box.x0) box.x0 = x0;
      if (y0 < box.y0) box.y0 = y0;
      if (x1 > box.x1) box.x1 = x1;
      if (y1 > box.y1) box.y1 = y1;
    };
    const o = p.frameOffset;
    const light = frame && o + 2 < frame.length ? beamLight(frame[o], frame[o + 1], frame[o + 2]) : null;
    const aiming = !frame && aim !== 0 && selected.has(p.prop);
    if (motion.beam && cells > 0 && (light || aiming)) {
      const direction = headDirection(motion, frame);
      const angle = beamWidth(motion, frame);
      if (light) {
        const shape = beamShape(placeX(0), placeY(0), pts[1], direction, BEAM_REACH * motion.beam.length, angle, zoom, ratio);
        const [y0, y1] = beamRows(shape, height);
        for (let py = y0; py < y1; py++) {
          const [x0, x1] = beamColumns(shape, py, width);
          for (let px = x0; px < x1; px++) {
            const amount = Math.floor(light.level * beamAt(shape, px, py) * 255 + 0.5);
            if (amount > 0) shine(pixels, py * width + px, light.rgb, amount);
          }
          stretch(x0, x1, py, py + 1);
        }
        const flare = flareShape(placeX(0), placeY(0), light, zoom, ratio);
        const [x0, x1, fy0, fy1] = flareBox(flare, width, height);
        for (let py = fy0; py < fy1; py++) {
          for (let px = x0; px < x1; px++) {
            const amount = Math.floor(flareAt(flare, px, py) * 255 + 0.5);
            if (amount > 0) shine(pixels, py * width + px, flare.rgb, amount);
          }
        }
        stretch(x0, x1, fy0, fy1);
      } else {
        const shape = beamShape(placeX(0), placeY(0), pts[1], direction, motion.beam.length, angle, zoom, ratio);
        const path = aimOutline(shape);
        const solid = aim >>> 24;
        let [x0, x1, y0, y1] = [width, 0, height, 0];
        for (const [ax, ay, bx, by] of path) {
          x0 = Math.min(x0, Math.max(0, Math.floor(Math.min(ax, bx) - ratio - 1)));
          x1 = Math.max(x1, Math.min(width, Math.ceil(Math.max(ax, bx) + ratio + 1)));
          y0 = Math.min(y0, Math.max(0, Math.floor(Math.min(ay, by) - ratio - 1)));
          y1 = Math.max(y1, Math.min(height, Math.ceil(Math.max(ay, by) + ratio + 1)));
        }
        for (let py = y0; py < y1; py++) {
          for (let px = x0; px < x1; px++) {
            const alpha = Math.floor(aimAt(path, px, py, ratio) * solid + 0.5);
            if (alpha > 0) blend(pixels, py * width + px, aim & 0xffffff, alpha);
          }
        }
        stretch(x0, x1, y0, y1);
      }
    }
    const servos = motion.servos ?? [];
    const armRgb = frame ? (ARM_COLOR[2] << 16) | (ARM_COLOR[1] << 8) | ARM_COLOR[0] : (selected.has(p.prop) ? picked : unlit) & 0xffffff;
    for (let k = 0; k < servos.length && cells > 0; k++) {
      const at = Math.min(k, cells - 1);
      const arm = armShape(placeX(at), placeY(at), servoDirection(motion, k, frame), zoom, ratio);
      const [x0, x1, y0, y1] = armBox(arm, width, height);
      for (let py = y0; py < y1; py++) {
        for (let px = x0; px < x1; px++) {
          const alpha = Math.floor(armAt(arm, px, py) * 255 + 0.5);
          if (alpha > 0) blend(pixels, py * width + px, armRgb, alpha);
        }
      }
      stretch(x0, x1, y0, y1);
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
  const mergeProp = (p: PreviewProp, dot: number) => {
    const pts = p.points;
    const room = (lights + (pts.length >> 1) + 1) * MERGED;
    if (merged.length < room) {
      const longer = new Float64Array(Math.max(room, merged.length * 2));
      longer.set(merged.subarray(0, (lights + 1) * MERGED));
      merged = longer;
    }
    const dim = dimOf(dot);
    const fixed = frame ? 0 : selected.has(p.prop) ? picked : unlit;
    const fixedSolid = (fixed >>> 24) / 255;
    const stamping = dot * 4 + (pixelsAllRound(pts) ? 2 : 0) + (!frame && selected.has(p.prop) ? 1 : 0);
    for (let i = 0, o = p.frameOffset; i + 1 < pts.length; i += 2, o += p.channelsPerPixel) {
      const x = (halfW + (pts[i] - cx) * zoom) * ratio;
      const y = (halfH - (pts[i + 1] - cy) * zoom) * ratio;
      if (x <= -dot || y <= -dot || x >= width + dot || y >= height + dot) continue;
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
      // The widest of its lights' dots, and whichever of them is all round or selected.
      const was = merged[at + 8];
      if (was !== stamping) merged[at + 8] = (Math.max(was >> 2, dot) << 2) | ((was | stamping) & 3);
      if (!frame) {
        merged[at + 1] += (fixed & 255) * fixedSolid;
        merged[at + 2] += ((fixed >> 8) & 255) * fixedSolid;
        merged[at + 3] += ((fixed >> 16) & 255) * fixedSolid;
        merged[at + 4] += fixedSolid;
      } else if (o + 2 < frame.length && (frame[o] | frame[o + 1] | frame[o + 2]) !== 0) {
        merged[at + 1] += frame[o] * dim;
        merged[at + 2] += frame[o + 1] * dim;
        merged[at + 3] += frame[o + 2] * dim;
        merged[at + 4] += Math.min(1, Math.max(frame[o], frame[o + 1], frame[o + 2]) / FAINT);
        merged[at + 8] |= 1;
      }
    }
  };

  /** Stamps the merged lights as crowded pixels are, each on whole device pixels: together they count as one prop. */
  const stampMerged = () => {
    for (let at = MERGED; at <= lights * MERGED; at += MERGED) if (merged[at] >= 0) mergedAt[merged[at]] = 0;
    // The shape last stamped, and which it was: most merged lights are stamped alike.
    let shape = dotShapes(smallest)[0];
    let shaped = -1;
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
      const stamping = merged[at + 8];
      const dot = stamping >> 2;
      const x0 = Math.round(merged[at + 5] / count - dot / 2);
      const y0 = Math.round(merged[at + 6] / count - dot / 2);
      if (x0 >= width || y0 >= height || x0 + dot <= 0 || y0 + dot <= 0) continue;
      if (stamping >> 1 !== shaped) {
        shaped = stamping >> 1;
        shape = (stamping & 2 ? squareShape(dot) : dotShapes(dot))[0];
      }
      claimDot(pixels, nearest, width, height, x0, y0, shape, color, stamping & 1);
      grow(x0, y0, dot);
    }
  };

  // Floods' washes go under every dot.
  for (const p of props) if (p.flood) stampWashes(p);
  // Then heads' beams and servos' arms, also under every dot.
  for (const p of props) if (p.motion) stampMotion(p);
  // Crowded props go under everything else: what's stamped over them covers them by the usual rules.
  if (anyCrowded) {
    if (nearest.length < width * height) nearest = new Uint16Array(width * height);
    if (anyMerged) {
      if (mergedAt.length < across * down) mergedAt = new Int32Array(across * down);
      for (let n = 0; n < props.length; n++) if (ways[n] === MERGED_FIRST) mergeProp(props[n], sizes[n]);
      stampMerged();
    }
    for (let n = 0; n < props.length; n++) if (ways[n] === CROWDED) stampCrowded(props[n], sizes[n], n + 1);
    for (let y = Math.max(0, box.y0); y < Math.min(height, box.y1); y++) nearest.fill(0, y * width + Math.max(0, box.x0), y * width + Math.min(width, box.x1));
  }
  for (let n = 0; n < props.length; n++) if (ways[n] === APART) stampProp(props[n], sizes[n], false);
  for (let n = 0; n < props.length; n++) if (ways[n] === APART) stampProp(props[n], sizes[n], true);
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
