import { afterEach, describe, expect, it, vi } from "vitest";
import type { PreviewProp } from "../api/types";
import { type DotImage, FAINT, FINE_FROM, drawPixels, dotShapes, dotSize, packColor, pixelSpacing, pixelsAllRound, stampPixels } from "./pixelBatches";

const COLORS = { unlit: "rgba(200, 200, 200, 0.5)", selected: "#a78bfa", dark: "rgba(80, 80, 80, 0.5)" };
const SIZE = { width: 100, height: 100 };
// Screen = world × 10, with world (0, 0) in the middle of the screen pixel at (50, 50).
const VIEW = { cx: -0.05, cy: 0.05, zoom: 10 };
const NONE: ReadonlySet<string> = new Set();

const prop = (id: string, points: number[], frameOffset = 0): PreviewProp => ({ prop: id, frameOffset, channelsPerPixel: 3, points });
/** Pixels in a row from world (`x`, `y`), `step` apart. */
const row = (count: number, step: number, x = 0, y = 0) => Array.from({ length: count }, (_, i) => [x + i * step, y]).flat();

const blank = (ratio = 1): DotImage => ({ pixels: new Int32Array(100 * ratio * 100 * ratio), width: 100 * ratio, height: 100 * ratio });
/** The image pixel at (x, y) as red, green, blue and how solid it is. */
const at = (image: DotImage, x: number, y: number) => {
  const n = image.pixels[y * image.width + x];
  return [n & 255, (n >> 8) & 255, (n >> 16) & 255, n >>> 24];
};
const solid = (image: DotImage, x: number, y: number) => at(image, x, y)[3];
/** Stamps props into a fresh image, with dots up to `radius` screen pixels. */
function stamped(props: PreviewProp[], frame: Uint8Array | null, { radius = 2.5, ratio = 1, selected = NONE, colors = COLORS as typeof COLORS | Omit<typeof COLORS, "dark"> } = {}) {
  const image = blank(ratio);
  const box = stampPixels(image, props, frame, VIEW, SIZE, selected, colors, radius, ratio);
  return { image, box };
}

describe("how far apart a prop's pixels are", () => {
  it("is the step along a string", () => {
    expect(pixelSpacing(row(40, 0.5))).toBeCloseTo(0.5);
  });

  it("is the nearer way across a matrix, whichever way it's wired", () => {
    // Columns 0.2 apart, wired up each column in turn: the next pixel in the wiring is 1 away.
    const points: number[] = [];
    for (let column = 0; column < 12; column++) for (let up = 0; up < 20; up++) points.push(column * 0.2, up);
    expect(pixelSpacing(points)).toBeCloseTo(0.2);
  });

  it("is the same when the wiring jumps about", () => {
    const order = [7, 2, 9, 0, 5, 3, 8, 1, 6, 4];
    expect(pixelSpacing(order.flatMap((i) => [i * 0.3, 1]))).toBeCloseTo(0.3);
  });

  it("doesn't count pixels on the very same spot as neighbors", () => {
    expect(pixelSpacing([0, 0, 0, 0, 3, 0, 3, 0, 6, 0, 6, 0])).toBeCloseTo(3);
  });

  it("is endless for a prop with no two pixels apart", () => {
    expect(pixelSpacing([])).toBe(Infinity);
    expect(pixelSpacing([4, 2])).toBe(Infinity);
    expect(pixelSpacing([4, 2, 4, 2, 4, 2])).toBe(Infinity);
  });

  it("comes with whether the pixels have neighbors on every side", () => {
    const grid = (columns: number, rows: number, dx: number, dy: number) => Array.from({ length: columns * rows }, (_, i) => [(i % columns) * dx, Math.floor(i / columns) * dy]).flat();
    expect(pixelsAllRound(grid(10, 10, 0.3, 0.3))).toBe(true);
    expect(pixelsAllRound(grid(16, 8, 0.3, 0.4))).toBe(true);
    expect(pixelsAllRound(row(40, 0.5))).toBe(false);
    // Rows far apart beside the step along them: each is a string of its own.
    expect(pixelsAllRound(grid(12, 20, 0.2, 1))).toBe(false);
    expect(pixelsAllRound([4, 2])).toBe(false);
  });

  it("is measured once for a set of points", () => {
    const points = row(10, 0.5);
    expect(pixelSpacing(points)).toBeCloseTo(0.5);
    for (let i = 0; i < points.length; i++) points[i] *= 4;
    expect(pixelSpacing(points)).toBeCloseTo(0.5);
    expect(pixelSpacing([...points])).toBeCloseTo(2);
  });
});

describe("a dot's size", () => {
  it("is most of the way to the next pixel, within the smallest and the largest", () => {
    const sizes = (pitches: number[]) => pitches.map((pitch) => dotSize(pitch, 2, 8));
    expect(sizes([1, 2.9, 3, 4, 5, 7, 10, 12, 40, Infinity])).toEqual([2, 2, 2, 3, 3, 5, 7, 8, 8, 8]);
  });

  it("always leaves a device pixel clear between neighbors", () => {
    for (let pitch = 2; pitch < 60; pitch += 0.25) {
      const size = dotSize(pitch, 1, 99);
      // A dot placed finely spreads its soft edge over one more device pixel.
      expect(size >= FINE_FROM ? size + 1 : size).toBeLessThanOrEqual(pitch - 1);
    }
  });
});

describe("a dot's shape", () => {
  it("is the whole square when it's too small to round", () => {
    expect(dotShapes(1)).toEqual([{ side: 1, cover: null, apart: expect.any(Uint8Array) }]);
    expect(dotShapes(2)[0]).toMatchObject({ side: 2, cover: null });
  });

  it("is round, solid in the middle with a soft edge", () => {
    const [{ side, cover }] = dotShapes(3);
    expect(side).toBe(3);
    expect(Array.from(cover!)).toEqual([149, 255, 149, 255, 255, 255, 149, 255, 149]);
    const big = dotShapes(8)[0].cover!;
    // The corners of its square are outside it; it's the same turned or flipped.
    expect([big[0], big[7], big[56], big[63]]).toEqual([0, 0, 0, 0]);
    expect(big[3 * 8 + 3]).toBe(255);
    for (let y = 0; y < 8; y++) for (let x = 0; x < 8; x++) expect([big[y * 8 + (7 - x)], big[x * 8 + y]]).toEqual([big[y * 8 + x], big[y * 8 + x]]);
  });

  it("says how far each of its pixels is from its middle", () => {
    const { apart } = dotShapes(3)[0];
    expect(apart[4]).toBe(1);
    expect(apart[1]).toBeGreaterThan(apart[4]);
    expect(apart[0]).toBeGreaterThan(apart[1]);
    expect([apart[3], apart[5], apart[7]]).toEqual([apart[1], apart[1], apart[1]]);
  });

  it("has one for every place it can sit across a device pixel, when placed finely", () => {
    const places = dotShapes(6, true);
    expect(places).toHaveLength(16);
    expect(places.every((p) => p.side === 7)).toBe(true);
    // Sitting on whole device pixels it's the plain shape, with room to spare at its right and bottom.
    const whole = dotShapes(6)[0].cover!;
    for (let y = 0; y < 7; y++) for (let x = 0; x < 7; x++) expect(places[0].cover![y * 7 + x]).toBe(x < 6 && y < 6 ? whole[y * 6 + x] : 0);
    // Half a pixel to the right, it's the mirror image of itself about the middle of its square.
    const half = places[2].cover!;
    for (let x = 0; x < 7; x++) expect(half[3 * 7 + x]).toBe(half[3 * 7 + 6 - x]);
    // Each covers the same amount, to within a little rounding.
    const sums = places.map((p) => p.cover!.reduce((a, b) => a + b, 0));
    expect(Math.max(...sums) - Math.min(...sums)).toBeLessThan(sums[0] * 0.03);
  });
});

describe("stamping pixels as dots", () => {
  it("draws a pixel as a round dot at its place, in exactly its color", () => {
    // 3 apart in the layout is 30 screen pixels: room for the largest dot, 5 across.
    const { image, box } = stamped([prop("string", row(2, 3))], new Uint8Array([254, 1, 2, 3, 100, 201]));
    expect(at(image, 50, 50)).toEqual([254, 1, 2, 255]);
    expect(at(image, 80, 50)).toEqual([3, 100, 201, 255]);
    // Solid to its edge, soft at its corners, nothing past it.
    expect([solid(image, 48, 50), solid(image, 52, 50), solid(image, 50, 48), solid(image, 50, 52)]).toEqual([255, 255, 255, 255]);
    expect(solid(image, 48, 48)).toBeGreaterThan(0);
    expect(solid(image, 48, 48)).toBeLessThan(255);
    expect(at(image, 48, 48).slice(0, 3)).toEqual([254, 1, 2]);
    expect([solid(image, 47, 50), solid(image, 53, 50), solid(image, 50, 47), solid(image, 50, 53)]).toEqual([0, 0, 0, 0]);
    expect(box).toEqual({ x0: 48, y0: 48, x1: 84, y1: 54 });
  });

  it("keeps every level of a fade apart", () => {
    const frame = new Uint8Array(Array.from({ length: 30 }, (_, i) => [100 + i, 0, 0]).flat());
    const { image } = stamped([prop("string", row(30, 0.3, -4.5))], frame, { radius: 1 });
    const reds = Array.from({ length: 30 }, (_, i) => at(image, 5 + i * 3, 50)[0]);
    expect(reds).toEqual(Array.from({ length: 30 }, (_, i) => 100 + i));
  });

  it("sizes dots from how far apart a prop's own pixels are, so neighbors never touch", () => {
    // The same view and the same largest dot: pixels 4, 6 and 30 screen pixels apart.
    const props = [prop("dense", row(5, 0.4, 0, 2)), prop("matrix", row(5, 0.6, 0, 1)), prop("string", row(2, 3))];
    const { image } = stamped(props, new Uint8Array(36).fill(255), { radius: 4 });
    const across = (y: number, from: number, to: number) => Array.from({ length: to - from }, (_, i) => (solid(image, from + i, y) > 0 ? "#" : ".")).join("");
    // Three across; four, and eight (each with its soft edge showing on either side): always with clear between.
    expect(across(30, 47, 64)).toBe("..###.###.###.###");
    expect(across(40, 46, 66)).toBe("..#####.#####.#####.");
    expect(across(50, 44, 58)).toBe("..#########...");
  });

  it("draws every dot of a size the same shape, wherever its pixel falls across a device pixel", () => {
    // Three across: small enough to sit on whole device pixels.
    const offsets = [0, 0.013, 0.027, 0.041];
    const { image } = stamped(offsets.map((dx, i) => prop(`p${i}`, row(2, 0.5, dx - 4, i - 2))), new Uint8Array(24).fill(255), { radius: 4 });
    const shape = (x: number, y: number) => [-2, -1, 0, 1, 2].flatMap((dy) => [-2, -1, 0, 1, 2].map((dx) => solid(image, x + dx, y + dy)));
    const first = shape(10, 70);
    expect(first.filter((v) => v > 0)).toHaveLength(9);
    offsets.forEach((_, i) => expect(shape(10, 70 - i * 10)).toEqual(first));
  });

  it("places larger dots to a quarter of a device pixel, so a grid of them comes out even", () => {
    const size = dotSize(7.5, 2, 99);
    expect(size).toBeGreaterThanOrEqual(FINE_FROM);
    const { image } = stamped([prop("matrix", row(8, 0.75, -3))], new Uint8Array(24).fill(255), { radius: 4 });
    // Where each dot's weight lies along the row: 7.5 device pixels on from the last, not 7 and 8 by turns.
    const middle = (i: number) => {
      const from = Math.floor(20.5 + i * 7.5 - 3.5);
      let [sum, weight] = [0, 0];
      for (let x = from; x < from + 8; x++) {
        sum += solid(image, x, 50) * (x + 0.5);
        weight += solid(image, x, 50);
      }
      return sum / weight;
    };
    for (let i = 0; i < 8; i++) expect(Math.abs(middle(i) - (20.5 + i * 7.5))).toBeLessThan(0.15);
    for (let i = 1; i < 8; i++) expect(Math.abs(middle(i) - middle(i - 1) - 7.5)).toBeLessThan(0.3);
  });

  it("draws lit pixels over ones that are off, whatever their order or colors", () => {
    // Two props with a pixel 3 screen pixels apart: their dots overlap.
    const lit = prop("lit", row(2, 3), 0);
    const off = prop("off", row(2, 3, 0.3), 6);
    const frame = new Uint8Array([200, 0, 0, 200, 0, 0, 0, 0, 0, 0, 0, 0]);
    const whole = stamped([lit], frame).image;
    for (const props of [
      [lit, off],
      [off, lit],
    ]) {
      const { image } = stamped(props, frame);
      // Wherever the lit dot is solid on its own, it is here too: nothing of it is hidden or bitten into.
      let solidPixels = 0;
      for (let y = 46; y < 55; y++) {
        for (let x = 46; x < 55; x++) {
          if (solid(whole, x, y) < 255) continue;
          expect(at(image, x, y)).toEqual([200, 0, 0, 255]);
          solidPixels++;
        }
      }
      expect(solidPixels).toBe(13);
      // The dot that's off still shows where the lit one doesn't reach.
      expect(at(image, 55, 50)).toEqual([80, 80, 80, 128]);
    }
  });

  it("doesn't let overlapping dots that are off add up", () => {
    const { image } = stamped([prop("a", row(2, 3)), prop("b", row(2, 3, 0.2))], new Uint8Array(12));
    for (let x = 48; x < 55; x++) expect(at(image, x, 50)).toEqual([80, 80, 80, 128]);
  });

  it("draws a selected prop's pixels over the others while nothing plays", () => {
    const props = [prop("picked", row(2, 3)), prop("other", row(2, 3, 0.2), 6)];
    const { image } = stamped(props, null, { selected: new Set(["picked"]) });
    expect(at(image, 50, 50)).toEqual([0xa7, 0x8b, 0xfa, 255]);
    expect(at(image, 52, 50)).toEqual([0xa7, 0x8b, 0xfa, 255]);
    expect(at(image, 54, 50)).toEqual([200, 200, 200, 128]);
    // Playing, selection doesn't matter: lit pixels show their colors, pixels that are off the dark one.
    const playing = stamped(props, new Uint8Array([9, 9, 250, 0, 0, 0, 0, 0, 0, 0, 0, 0]), { selected: new Set(["picked"]) }).image;
    expect(at(playing, 50, 50)).toEqual([9, 9, 250, 255]);
    expect(at(playing, 54, 50)).toEqual([80, 80, 80, 128]);
  });

  it("finds each prop's colors at its own place in the frame, and pixels past the frame's end are off", () => {
    const { image } = stamped([prop("a", row(2, 3)), prop("b", row(2, 3, 0, 2), 6)], new Uint8Array([0, 0, 0, 0, 0, 0, 0, 255, 0]));
    expect(at(image, 50, 50)).toEqual([80, 80, 80, 128]);
    expect(at(image, 50, 30)).toEqual([0, 255, 0, 255]);
    expect(at(image, 80, 30)).toEqual([80, 80, 80, 128]);
  });

  it("leaves out pixels that are off when there's no color for them", () => {
    const colors = { unlit: COLORS.unlit, selected: COLORS.selected };
    const { image, box } = stamped([prop("a", row(2, 3))], new Uint8Array([255, 0, 0, 0, 0, 0]), { colors });
    expect(at(image, 50, 50)).toEqual([255, 0, 0, 255]);
    expect(solid(image, 80, 50)).toBe(0);
    expect(box).toEqual({ x0: 48, y0: 48, x1: 54, y1: 54 });
    // With nothing playing, every pixel is still drawn.
    expect(solid(stamped([prop("a", row(2, 3))], null, { colors }).image, 80, 50)).toBe(128);
  });

  it("lets the dot of a pixel that's off show through a faint light, so it's never darker than one", () => {
    // Off, barely on, half way to `FAINT`, and at it.
    const frame = new Uint8Array([0, 0, 0, 1, 0, 0, FAINT / 2, 0, 0, FAINT, 0, 0]);
    const props = [prop("string", row(4, 2, -3))];
    const { image } = stamped(props, frame);
    // As drawn over black: the light, and what shows of the dot of a pixel that's off (80, at half).
    const shown = (i: number) => at(image, 20 + i * 20, 50).slice(0, 3).map((c) => (c * solid(image, 20 + i * 20, 50)) / 255);
    const [off, barely, half, full] = [0, 1, 2, 3].map(shown);
    expect(off.map(Math.round)).toEqual([40, 40, 40]);
    expect(barely.map(Math.round)).toEqual([41, 40, 40]);
    expect(half.map(Math.round)).toEqual([52, 20, 20]);
    expect(at(image, 80, 50)).toEqual([FAINT, 0, 0, 255]);
    expect(full[0]).toBeGreaterThan(half[0]);
    // With no color for pixels that are off, a faint light is see-through instead: its own color, thinly.
    const bare = stamped(props, frame, { colors: { unlit: COLORS.unlit, selected: COLORS.selected } }).image;
    expect(at(bare, 60, 50)).toEqual([FAINT, 0, 0, 128]);
    expect(at(bare, 80, 50)).toEqual([FAINT, 0, 0, 255]);
  });

  it("clips dots at the image's edges, and skips pixels off it", () => {
    const { image, box } = stamped([prop("string", [-5, 5, 0, 0, 30, 0])], null);
    expect(solid(image, 0, 0)).toBe(128);
    expect(solid(image, 2, 2)).toBeGreaterThan(0);
    expect(box).toEqual({ x0: 0, y0: 0, x1: 54, y1: 54 });
    expect(stamped([prop("string", [30, 0, 33, 0])], null).box).toBeNull();
    expect(stamped([], null).box).toBeNull();
  });

  it("draws dots twice the size on a canvas with two device pixels to a screen pixel", () => {
    const { image, box } = stamped([prop("string", row(2, 3))], new Uint8Array(6).fill(255), { ratio: 2 });
    expect(box).toEqual({ x0: 96, y0: 96, x1: 167, y1: 107 });
    expect(solid(image, 101, 101)).toBe(255);
    expect(Array.from({ length: 13 }, (_, i) => (solid(image, 95 + i, 101) > 0 ? "#" : ".")).join("")).toBe(".##########..");
  });
});

describe("stamping pixels too close together for dots of their own", () => {
  // Two screen pixels apart: closer than the smallest dot and a gap.
  const string = (colors: number[][]) => ({ props: [prop("string", row(colors.length, 0.2, -1))], frame: new Uint8Array(colors.flat()) });
  const RED = [255, 0, 0];
  const BLUE = [0, 0, 255];
  const OFF = [0, 0, 0];

  it("draws the smallest dot around each: a string comes out an even line with no gaps", () => {
    const { props, frame } = string(Array.from({ length: 10 }, () => RED));
    const { image, box } = stamped(props, frame);
    for (let x = 40; x < 60; x++) expect([solid(image, x, 49), at(image, x, 50), at(image, x, 51), solid(image, x, 52)]).toEqual([0, [255, 0, 0, 255], [255, 0, 0, 255], 0]);
    expect(box).toEqual({ x0: 40, y0: 50, x1: 60, y1: 52 });
  });

  it("shows at each place the pixel nearest to it, whatever the wiring order", () => {
    const colors = [RED, BLUE, RED, RED, BLUE, OFF, BLUE, RED, OFF, RED];
    const { props, frame } = string(colors);
    const { image } = stamped(props, frame);
    // Every pixel keeps the two screen pixels around it: none is hidden or bitten into, lit or not.
    const expected = colors.map((c) => (c === OFF ? [80, 80, 80, 128] : [...c, 255]));
    colors.forEach((_, i) => expect([at(image, 40 + 2 * i, 50), at(image, 41 + 2 * i, 50), at(image, 40 + 2 * i, 51)]).toEqual([expected[i], expected[i], expected[i]]));
    // Wired the other way, the picture is the same.
    const backwards = stamped([prop("string", row(10, -0.2, 0.8))], new Uint8Array([...colors].reverse().flat())).image;
    expect(Array.from(backwards.pixels)).toEqual(Array.from(image.pixels));
  });

  it("merges pixels closer together than a screen pixel into one light with their average color", () => {
    // Four to a screen pixel: red, blue, off, off.
    const colors = Array.from({ length: 40 }, (_, i) => [RED, BLUE, OFF, OFF][i % 4]);
    const { image } = stamped([prop("string", row(40, 0.025, -0.045))], new Uint8Array(colors.flat()));
    for (let x = 51; x < 58; x++) {
      const [r, g, b, a] = at(image, x, 50);
      // Half light (red and blue alike), half the dots of pixels that are off (80, at half).
      expect(a).toBe(192);
      expect([(r * a) / 255, (g * a) / 255, (b * a) / 255].map(Math.round)).toEqual([84, 20, 84]);
    }
    // A hundred of one color in one device pixel are that color.
    const pile = stamped([prop("pile", row(100, 0.0005, -0.02))], new Uint8Array(300).fill(7).map((_, i) => [10, 200, 30][i % 3])).image;
    expect(at(pile, 50, 50)).toEqual([10, 200, 30, 255]);
  });

  it("fills a crowded matrix's squares, leaving no corner between four pixels half covered", () => {
    const matrix = (step: number) => Array.from({ length: 144 }, (_, i) => [(i % 12) * step, Math.floor(i / 12) * step]).flat();
    const frame = new Uint8Array(144 * 3).fill(200);
    // At two device pixels to a screen pixel, where the smallest dot is round and 4 across:
    // pixels 2.4 device pixels apart, then 1.5 (closer than a screen pixel: merged).
    for (const step of [0.12, 0.075]) {
      const { image, box } = stamped([prop("matrix", matrix(step))], frame, { ratio: 2 });
      expect(box!.x1 - box!.x0).toBeGreaterThan(11 * step * 20);
      for (let y = box!.y0; y < box!.y1; y++) for (let x = box!.x0; x < box!.x1; x++) expect(at(image, x, y)).toEqual([200, 200, 200, 255]);
    }
    // A string as crowded keeps its round dots: soft at the corners of its ends.
    const { image, box } = stamped([prop("string", row(12, 0.12))], frame, { ratio: 2 });
    expect(solid(image, box!.x0, box!.y0)).toBeLessThan(255);
    expect(solid(image, box!.x0 + 2, box!.y0 + 2)).toBe(255);
  });

  it("is under the dots of props with room for their own", () => {
    const line = prop("line", row(10, 0.2, -1));
    const dot = prop("dot", row(2, 3), 30);
    const frame = new Uint8Array(36);
    frame.set([0, 255, 0], 30);
    for (const props of [
      [line, dot],
      [dot, line],
    ]) {
      const { image } = stamped(props, frame);
      expect(at(image, 50, 50)).toEqual([0, 255, 0, 255]);
      expect(at(image, 50, 51)).toEqual([0, 255, 0, 255]);
      expect(at(image, 44, 50)).toEqual([80, 80, 80, 128]);
    }
  });

  it("draws with the unlit and selected colors while nothing plays", () => {
    const { image } = stamped([prop("a", row(10, 0.2, -1)), prop("b", row(10, 0.2, -1, 1))], null, { selected: new Set(["b"]) });
    expect(at(image, 45, 50)).toEqual([200, 200, 200, 128]);
    expect(at(image, 45, 40)).toEqual([0xa7, 0x8b, 0xfa, 255]);
  });

  it("leaves no trace behind for the next image", () => {
    const { props, frame } = string(Array.from({ length: 10 }, () => RED));
    const first = stamped(props, frame).image;
    stamped([prop("pile", row(100, 0.0005, -0.02))], new Uint8Array(300).fill(9));
    expect(Array.from(stamped(props, frame).image.pixels)).toEqual(Array.from(first.pixels));
  });
});

describe("drawing pixels on a canvas", () => {
  const getContext = HTMLCanvasElement.prototype.getContext;
  afterEach(() => {
    HTMLCanvasElement.prototype.getContext = getContext;
  });

  /** A canvas context that records the images drawn on it, and gives the dots' image a context of its own. */
  function canvases() {
    const drawn: number[][] = [];
    const put = vi.fn<(image: { data: Uint8ClampedArray }, ...at: number[]) => void>();
    HTMLCanvasElement.prototype.getContext = function () {
      // Like a browser's, it can't make an image with no size.
      const createImageData = (w: number, h: number) => {
        if (w === 0 || h === 0) throw new Error("The source width or height is zero.");
        return { data: new Uint8ClampedArray(w * h * 4) };
      };
      return { createImageData, putImageData: put };
    } as unknown as typeof HTMLCanvasElement.prototype.getContext;
    const ctx = { canvas: { width: 100, height: 100 }, save() {}, restore() {}, setTransform() {}, drawImage: (_image: unknown, ...args: number[]) => drawn.push(args) };
    return { ctx: ctx as unknown as CanvasRenderingContext2D, drawn, put };
  }

  it("stamps the dots into one image and draws the part of it they're in", () => {
    const { ctx, drawn, put } = canvases();
    drawPixels(ctx, [prop("string", row(2, 3))], new Uint8Array([255, 0, 0, 0, 0, 0]), VIEW, SIZE, NONE, COLORS, 2.5, 1);
    expect(put).toHaveBeenCalledTimes(1);
    expect(put.mock.calls[0].slice(1)).toEqual([0, 0, 48, 48, 36, 6]);
    expect(drawn).toEqual([[48, 48, 36, 6, 48, 48, 36, 6]]);
    // The image holds the dot's color as the canvas reads it: red, green, blue, alpha.
    const data = put.mock.calls[0][0].data;
    expect(Array.from(data.subarray((50 * 100 + 50) * 4, (50 * 100 + 50) * 4 + 4))).toEqual([255, 0, 0, 255]);
  });

  it("empties what the last frame drew before the next", () => {
    const { ctx, put } = canvases();
    drawPixels(ctx, [prop("string", row(2, 3))], null, VIEW, SIZE, NONE, COLORS, 2.5, 1);
    drawPixels(ctx, [prop("string", row(2, 3, 0, 2))], null, VIEW, SIZE, NONE, COLORS, 2.5, 1);
    const data = put.mock.calls[1][0].data;
    expect(data[(50 * 100 + 50) * 4 + 3]).toBe(0);
    expect(data[(30 * 100 + 50) * 4 + 3]).toBe(128);
  });

  it("draws nothing when there's nothing to draw, or on a canvas with no size yet", () => {
    const { ctx, drawn, put } = canvases();
    drawPixels(ctx, [], null, VIEW, SIZE, NONE, COLORS, 2.5, 1);
    (ctx.canvas as { width: number }).width = 0;
    drawPixels(ctx, [prop("string", row(2, 3))], null, VIEW, { width: 0, height: 100 }, NONE, COLORS, 2.5, 1);
    expect(drawn).toEqual([]);
    expect(put).not.toHaveBeenCalled();
  });
});

describe("packing colors", () => {
  it("packs CSS colors as image pixels", () => {
    const bytes = (n: number) => Array.from(new Uint8Array(new Int32Array([n]).buffer));
    expect(bytes(packColor("#a78bfa"))).toEqual([0xa7, 0x8b, 0xfa, 255]);
    expect(bytes(packColor("rgba(220, 220, 220, 0.7)"))).toEqual([220, 220, 220, 179]);
    expect(bytes(packColor("rgb(1, 2, 3)"))).toEqual([1, 2, 3, 255]);
  });
});
