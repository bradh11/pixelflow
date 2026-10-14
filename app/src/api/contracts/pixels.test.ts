// What the flat previews draw, for the video export to draw too. Each file in pixels/ is a scene:
// a small made-up layout and a frame of colors, the canvas of a preview showing it (fitted as
// lib/flatPreview.ts fits a display), and the picture of dots the preview draws there
// (`stampPixels` in lib/pixelBatches.ts), with no glow. This test writes them, and fails when one
// is stale. The export's tests (crates/pf-video/src/raster.rs) draw the same scenes with the
// export's own code and fail unless every pixel comes out the same.

import { describe, expect, it } from "vitest";
import { COLORS, fitDisplay } from "../../lib/flatPreview";
import { type Size, boxOfPoints, unionBox } from "../../lib/layoutMath";
import { type DotImage, packColor, stampPixels } from "../../lib/pixelBatches";
import type { PreviewProp } from "../types";
import { listJson, staleFile } from "./support";

const REGENERATE = "`PIXELFLOW_UPDATE_CONTRACTS=1 npx vitest run src/api/contracts` in app, then `cargo test --locked -p pf-video` in the repository's root (the export must draw the same)";

type Color = [number, number, number];
const OFF: Color = [0, 0, 0];
const RED: Color = [255, 0, 0];
const GREEN: Color = [0, 255, 0];
const BLUE: Color = [0, 0, 255];
const WHITE: Color = [255, 255, 255];
const ORANGE: Color = [255, 128, 0];
const CYAN: Color = [0, 200, 255];
const PURPLE: Color = [160, 0, 255];

/** `count` of a color in a row. */
const run = (count: number, color: Color): Color[] => Array.from({ length: count }, () => color);

/** A layout being made: its props (each one's pixels as x, y pairs, and where its colors are in the frame), and the frame. */
class Layout {
  props: { frameOffset: number; channelsPerPixel: number; points: number[]; flood?: boolean }[] = [];
  frame: number[] = [];

  /** Adds a prop with a color for each of its pixels (and a white channel too, with `channels` 4). */
  add(points: number[], colors: Color[], channels: 3 | 4 = 3): this {
    if (colors.length * 2 !== points.length) throw new Error(`${points.length / 2} pixels were given ${colors.length} colors`);
    this.props.push({ frameOffset: this.frame.length, channelsPerPixel: channels, points });
    for (const color of colors) this.frame.push(...color, ...(channels === 4 ? [255] : []));
    return this;
  }

  /** Adds a DMX flood: a cell at each point, with the color it shows. */
  flood(points: number[], colors: Color[]): this {
    this.add(points, colors);
    this.props[this.props.length - 1].flood = true;
    return this;
  }
}

/** Pixels `step` apart in a row from (`x`, `y`), each `rise` higher than the last. */
const row = (count: number, step: number, x = 0, y = 0, rise = 0) => Array.from({ length: count }, (_, i) => [x + i * step, y + i * rise]).flat();
/** Pixels in rows of `columns` from (`x`, `y`), `dx` apart along a row and `dy` between rows, wired row by row. */
const grid = (columns: number, rows: number, dx: number, dy: number, x = 0, y = 0) =>
  Array.from({ length: columns * rows }, (_, i) => [x + (i % columns) * dx, y + Math.floor(i / columns) * dy]).flat();

/** A scene: what it shows, a preview's canvas (its size in screen pixels, and device pixels to each), and the layout. */
interface Scene {
  about: string;
  size: Size;
  ratio: number;
  layout: Layout;
}

// Every coordinate is a whole number of 128ths, so it's the same number whether it's kept as the
// window keeps it or as the engine does (in half the bits).
const SCENES: Record<string, Scene> = {
  sparseString: {
    about: "A string with room between its pixels: each is a whole round dot in exactly its color, placed to a quarter of a device pixel. The last is past the frame's end, so it's off.",
    size: { width: 72, height: 40 },
    ratio: 2,
    layout: (() => {
      const layout = new Layout().add(row(6, 1 / 4, 0, 0, 1 / 32), [RED, GREEN, OFF, [3, 100, 201], WHITE, BLUE]);
      layout.frame.length -= 2;
      return layout;
    })(),
  },
  denseString: {
    about: "A roof line with pixels under two device pixels apart, at the 1080p export's 1.28 device pixels to a screen pixel: they run together into one even line, each place showing the nearest pixel, with clean cuts between colors.",
    size: { width: 96, height: 56 },
    ratio: 1.28,
    layout: new Layout().add(
      [...row(24, 1 / 32, 0, 0, 1 / 64), ...row(24, 1 / 32, 24 / 32, 23 / 64, -1 / 64)],
      [...run(6, RED), ...run(6, GREEN), ...run(4, OFF), ...run(8, BLUE), WHITE, OFF, WHITE, ...run(5, ORANGE), ...run(4, [40, 40, 0]), ...run(12, PURPLE)],
    ),
  },
  crossingStrings: {
    about: "A lit string crossed by one that's off: the lit one is whole through the crossing whichever is drawn first, and the other shows on either side of it.",
    size: { width: 64, height: 64 },
    ratio: 2,
    layout: new Layout()
      .add(
        Array.from({ length: 16 }, (_, i) => [15 / 32, i / 16]).flat(),
        run(16, OFF),
      )
      .add(row(16, 1 / 16, 0, 15 / 32), [...run(8, CYAN), ...run(8, ORANGE)]),
  },
  smallMatrix: {
    about: "A matrix with room between its pixels, wired with a white channel: its dots are sized from how far apart they are, with clear between them.",
    size: { width: 80, height: 56 },
    ratio: 2,
    layout: new Layout().add(
      grid(8, 5, 1 / 8, 1 / 8),
      Array.from({ length: 40 }, (_, i): Color => (i % 7 === 3 ? OFF : [(i % 8) * 36, Math.floor(i / 8) * 60, 255 - (i % 8) * 30])),
      4,
    ),
  },
  denseMatrix: {
    about: "A matrix dense both ways: every pixel is its whole square, so the picture has no gaps and no corner between four pixels is half covered.",
    size: { width: 64, height: 52 },
    ratio: 1.28,
    layout: new Layout().add(
      grid(24, 16, 1 / 32, 1 / 32),
      Array.from({ length: 384 }, (_, i): Color => {
        const [x, y] = [i % 24, Math.floor(i / 24)];
        if ((x + y) % 8 < 2) return OFF;
        return (x + y) % 8 < 5 ? [255, 40 + y * 10, 0] : [0, 80, 100 + x * 6];
      }),
    ),
  },
  matrixRows: {
    about: "A matrix dense along its rows with room between them: each row is a bar as thick as that room allows, and the gaps between the rows stay.",
    size: { width: 56, height: 64 },
    ratio: 2,
    layout: new Layout().add(
      grid(12, 5, 1 / 32, 1 / 8),
      Array.from({ length: 60 }, (_, i): Color => (Math.floor(i / 12) === 2 ? OFF : i % 12 < 6 ? GREEN : [40 * Math.floor(i / 12), 0, 200])),
    ),
  },
  litBesideUnlit: {
    about: "Three strings whose dots overlap, the lit one between two that are off in the frame's order: a lit dot is whole over the ones that are off, and those don't add up where they overlap.",
    size: { width: 68, height: 34 },
    ratio: 1,
    layout: (() => {
      const layout = new Layout()
        .add(row(5, 1 / 4, 1 / 16, 1 / 32), run(5, OFF))
        .add(row(5, 1 / 4), [RED, GREEN, BLUE, WHITE, [200, 0, 0]])
        .add(row(5, 1 / 4, 1 / 8), run(5, OFF));
      // Drawn in another order than their colors are in the frame.
      layout.props.reverse();
      return layout;
    })(),
  },
  dimFade: {
    about: "Lights fading out, standing apart and run together: below a quarter brightness the dot of a pixel that's off shows through, so a light ends as that dot and is never darker than one.",
    size: { width: 104, height: 48 },
    ratio: 1.28,
    layout: new Layout()
      .add(
        row(10, 1 / 4),
        [255, 128, 64, 63, 48, 32, 16, 4, 1, 0].map((v): Color => [v, 0, 0]),
      )
      .add(
        row(40, 1 / 32, 0, -1 / 4),
        Array.from({ length: 40 }, (_, i): Color => [0, 80 - 2 * i, Math.max(0, 20 - i)]),
      ),
  },
  floodWashes: {
    about: "DMX floods: each lit cell washes its color round it under every dot, fading to nothing at its reach; two washes overlap, the later over the earlier; a string's dots stand whole over a wash; a dark cell washes nothing.",
    size: { width: 96, height: 64 },
    ratio: 1.28,
    layout: new Layout()
      .flood(row(1, 0, 0, 0), [ORANGE])
      .flood(row(3, 1 / 4, 3 / 4, 1 / 8), [BLUE, OFF, [40, 200, 90]])
      .add(row(8, 1 / 8, -1 / 4, 3 / 8), [...run(4, RED), ...run(4, WHITE)]),
  },
  closerThanAPixel: {
    about: "A string and a matrix with several pixels to a screen pixel: those in one screen pixel are merged into one light with their average color, the dots of ones that are off showing through.",
    size: { width: 72, height: 56 },
    ratio: 2,
    layout: new Layout()
      .add(row(96, 1 / 128, 0, 1 / 4), [
        ...run(12, RED),
        ...run(12, BLUE),
        ...Array.from({ length: 24 }, (_, i) => (i % 2 ? RED : BLUE)),
        ...run(12, OFF),
        ...Array.from({ length: 24 }, (_, i) => (i % 2 ? OFF : GREEN)),
        ...Array.from({ length: 12 }, (_, i): Color => [10 * i, 10 * i, 0]),
      ])
      .add(
        grid(32, 12, 1 / 128, 1 / 128, 1 / 4, 0),
        Array.from({ length: 384 }, (_, i): Color => {
          const [x, y] = [i % 32, Math.floor(i / 32)];
          return y < 4 ? [255, 255, 255] : x < 16 ? [0, 16 * (y - 4), 255] : (x + y) % 2 ? OFF : ORANGE;
        }),
      ),
  },
};

const hex = (bytes: number[]) => bytes.map((b) => b.toString(16).padStart(2, "0")).join("");
/** An image pixel's red, green, blue and how solid it is. */
const bytes = (pixel: number) => [pixel & 255, (pixel >> 8) & 255, (pixel >> 16) & 255, pixel >>> 24];

/**
 * An image as a scene's file holds it: its size, where the part with anything in it is (`left`,
 * `top`; the rest is empty), the colors in that part (each `rrggbbaa`), and for each of its rows
 * which of them each pixel is.
 */
function picture({ pixels, width, height }: DotImage) {
  let [left, top, right, bottom] = [width, height, 0, 0];
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (pixels[y * width + x] === 0) continue;
      [left, top, right, bottom] = [Math.min(left, x), Math.min(top, y), Math.max(right, x + 1), Math.max(bottom, y + 1)];
    }
  }
  const colors: string[] = [];
  const found = new Map<number, number>();
  const rows: string[] = [];
  for (let y = top; y < bottom; y++) {
    const row: number[] = [];
    for (let x = left; x < right; x++) {
      const pixel = pixels[y * width + x];
      if (!found.has(pixel)) {
        found.set(pixel, colors.length);
        colors.push(hex(bytes(pixel)));
      }
      row.push(found.get(pixel)!);
    }
    rows.push(row.join(" "));
  }
  return { width, height, left, top, colors, rows };
}

/** A scene's file: the scene as the preview shows it, and the picture the preview draws. */
function drawn({ about, size, ratio, layout }: Scene) {
  const props: PreviewProp[] = layout.props.map((p, i) => ({ prop: `prop ${i}`, ...p }));
  const { view, radius } = fitDisplay(unionBox(props.map((p) => boxOfPoints(p.points))), size);
  // A canvas's size in device pixels, as the previews set it.
  const [width, height] = [Math.round(size.width * ratio), Math.round(size.height * ratio)];
  const image: DotImage = { pixels: new Int32Array(width * height), width, height };
  stampPixels(image, props, new Uint8Array(layout.frame), view, size, new Set(), COLORS, radius, ratio);
  return {
    about,
    size,
    ratio,
    view,
    radius,
    dark: bytes(packColor(COLORS.dark)),
    props: layout.props.map((p) => ({ ...p, points: Array.from({ length: p.points.length / 2 }, (_, i) => [p.points[2 * i], p.points[2 * i + 1]]) })),
    frame: hex(layout.frame),
    picture: picture(image),
  };
}

describe("the scenes the previews and the video export draw alike", () => {
  for (const [name, scene] of Object.entries(SCENES)) {
    it(`${name}: the preview's picture is the one on file`, () => {
      const file = drawn(scene);
      expect(file.picture.rows.length).toBeGreaterThan(0);
      expect(staleFile(`pixels/${name}.json`, file, REGENERATE)).toBeNull();
    });
  }

  it("has no file left from a scene that's gone", () => {
    expect(listJson("pixels")).toEqual(
      Object.keys(SCENES)
        .map((name) => `${name}.json`)
        .sort(),
    );
  });

  it("is in numbers the window and the engine keep alike", () => {
    for (const { layout } of Object.values(SCENES)) for (const { points } of layout.props) expect(points.every((v) => Math.fround(v) === v)).toBe(true);
  });
});
