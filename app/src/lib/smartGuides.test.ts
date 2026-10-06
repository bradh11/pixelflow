import { describe, expect, it } from "vitest";
import type { Box, Gesture } from "./layoutMath";
import {
  GUIDE_PX,
  formatGap,
  guideIndex,
  guideThreshold,
  guidesActive,
  nearbyBoxes,
  snapMove,
  snapPointTo,
  snapResize,
} from "./smartGuides";

const box = (minX: number, minY: number, maxX: number, maxY: number): Box => ({ minX, minY, maxX, maxY });
/** 6 px at 40 px per unit. */
const T = 6 / 40;
/** Marks with their numbers rounded, to compare without floating-point noise. */
const rounded = <M extends object>(marks: M[]): M[] =>
  marks.map((m) => Object.fromEntries(Object.entries(m).map(([k, v]) => [k, typeof v === "number" ? Math.round(v * 1e6) / 1e6 : v])) as M);

describe("guideThreshold", () => {
  it("is a fixed number of screen pixels, so fewer layout units the further in you zoom", () => {
    expect(GUIDE_PX).toBe(6);
    expect(guideThreshold(40)).toBeCloseTo(0.15);
    expect(guideThreshold(10)).toBeCloseTo(0.6);
    expect(guideThreshold(400)).toBeCloseTo(0.015);
  });

  it("snaps from further away (in layout units) when zoomed out", () => {
    const index = guideIndex([box(0, 0, 2, 1)]);
    const start = box(10, 5, 12, 6);
    // The left edge 0.3 units right of the other's left edge: 12 px at zoom 40, 3 px at zoom 10.
    const raw = { dx: -9.7, dy: 0 };
    expect(snapMove(index, start, raw, { threshold: guideThreshold(40) }).dx).toBeCloseTo(-9.7);
    expect(snapMove(index, start, raw, { threshold: guideThreshold(10) }).dx).toBeCloseTo(-10);
  });
});

describe("guidesActive", () => {
  it("is on when turned on, and off while Alt (Option) is held", () => {
    expect(guidesActive(true, { altKey: false })).toBe(true);
    expect(guidesActive(true, { altKey: true })).toBe(false);
    expect(guidesActive(false, { altKey: false })).toBe(false);
  });
});

describe("snapMove: alignment", () => {
  // The other prop: x 0…2 (center 1), y 0…1 (middle 0.5). The moving one is 2 wide and 1 tall.
  const index = guideIndex([box(0, 0, 2, 1)]);
  const start = box(10, 10, 12, 11);

  // [moving edge, its offset in the box, the other's edge it should land on]
  const xCases: [string, number, number][] = [
    ["left to left", 0, 0],
    ["left to center", 0, 1],
    ["left to right", 0, 2],
    ["center to left", 1, 0],
    ["center to center", 1, 1],
    ["center to right", 1, 2],
    ["right to left", 2, 0],
    ["right to center", 2, 1],
    ["right to right", 2, 2],
  ];
  it.each(xCases)("snaps %s, with a vertical guide across both", (_, edge, target) => {
    // Pointer leaves the moving edge 0.1 units right of the target.
    const raw = { dx: target + 0.1 - (10 + edge), dy: 0 };
    const r = snapMove(index, start, raw, { threshold: T });
    expect(r.dx).toBeCloseTo(target - (10 + edge));
    expect(r.dy).toBe(0);
    const guide = r.marks.guides.find((g) => g.axis === "x" && Math.abs(g.at - target) < 1e-9);
    expect(guide).toBeDefined();
    expect(guide!.from).toBeCloseTo(0);
    expect(guide!.to).toBeCloseTo(11);
  });

  const yCases: [string, number, number][] = [
    ["bottom to bottom", 0, 0],
    ["bottom to middle", 0, 0.5],
    ["bottom to top", 0, 1],
    ["middle to bottom", 0.5, 0],
    ["middle to middle", 0.5, 0.5],
    ["middle to top", 0.5, 1],
    ["top to bottom", 1, 0],
    ["top to middle", 1, 0.5],
    ["top to top", 1, 1],
  ];
  it.each(yCases)("snaps %s, with a horizontal guide across both", (_, edge, target) => {
    const raw = { dx: 0, dy: target - 0.08 - (10 + edge) };
    const r = snapMove(index, start, raw, { threshold: T });
    expect(r.dy).toBeCloseTo(target - (10 + edge));
    expect(r.dx).toBe(0);
    const guide = r.marks.guides.find((g) => g.axis === "y" && Math.abs(g.at - target) < 1e-9);
    expect(guide).toBeDefined();
    expect(guide!.from).toBeCloseTo(0);
    expect(guide!.to).toBeCloseTo(12);
  });

  it("snaps both ways at once", () => {
    const r = snapMove(index, start, { dx: -9.9, dy: -9.95 }, { threshold: T });
    expect(r.dx).toBeCloseTo(-10);
    expect(r.dy).toBeCloseTo(-10);
    expect(r.marks.guides.filter((g) => g.axis === "x").map((g) => g.at)).toEqual([0, 1, 2]);
    expect(r.marks.guides.filter((g) => g.axis === "y").map((g) => g.at)).toEqual([0, 0.5, 1]);
  });

  it("leaves the move alone, with no guides, when nothing is within the threshold", () => {
    const r = snapMove(index, start, { dx: -5, dy: -3 }, { threshold: T });
    expect(r).toEqual({ dx: -5, dy: -3, marks: { guides: [], gaps: [], sizes: [] } });
  });

  it("picks the nearest line when several are close", () => {
    const close = guideIndex([box(0, 0, 2, 1), box(0.05, 3, 2, 4)]);
    // Left edge at 0.04: 0.04 from 0, 0.01 from 0.05.
    const r = snapMove(close, start, { dx: -9.96, dy: 0 }, { threshold: T });
    expect(r.dx).toBeCloseTo(-9.95);
  });

  it("never snaps an axis that's held straight (Shift)", () => {
    const r = snapMove(index, start, { dx: -9.9, dy: 0 }, { threshold: T, lock: { y: true } });
    expect(r.dx).toBeCloseTo(-10);
    expect(r.dy).toBe(0);
    const r2 = snapMove(index, start, { dx: 0, dy: -9.95 }, { threshold: T, lock: { x: true } });
    expect(r2.dy).toBeCloseTo(-10);
    expect(r2.dx).toBe(0);
  });

  it("merges guides from several props on the same line into one", () => {
    const stacked = guideIndex([box(0, 0, 2, 1), box(0, 4, 3, 5)]);
    const r = snapMove(stacked, start, { dx: -9.9, dy: 0 }, { threshold: T });
    const left = rounded(r.marks.guides).filter((g) => g.axis === "x" && g.at === 0);
    expect(left).toEqual([{ axis: "x", at: 0, from: 0, to: 11 }]);
  });
});

describe("snapMove: equal spacing", () => {
  it("snaps to the gap between two props in the same row, on either side", () => {
    // A 0…2 and B 4…6: a gap of 2. The moving prop is 2 wide, in the same row (y 0…1).
    const index = guideIndex([box(0, 0, 2, 1), box(4, 0, 6, 1)]);
    const start = box(20, 0.2, 22, 0.8);
    const right = snapMove(index, start, { dx: -11.9, dy: 0 }, { threshold: T });
    expect(right.dx).toBeCloseTo(-12); // lands at 8…10: 2 after B
    expect(rounded(right.marks.gaps)).toEqual(
      expect.arrayContaining([
        { axis: "x", from: 6, to: 8, at: 0.5 },
        { axis: "x", from: 2, to: 4, at: 0.5 },
      ]),
    );
    expect(right.marks.gaps).toHaveLength(2);

    const left = snapMove(index, start, { dx: -23.9, dy: 0 }, { threshold: T });
    expect(left.dx).toBeCloseTo(-24); // lands at -4…-2: 2 before A
    expect(rounded(left.marks.gaps)).toEqual(expect.arrayContaining([{ axis: "x", from: -2, to: 0, at: 0.5 }]));
  });

  it("snaps to the gap between two props in the same column", () => {
    // A y 0…1 and B y 3…4: a gap of 2. The moving prop is 1 tall, in the same column (x 0…2).
    const index = guideIndex([box(0, 0, 2, 1), box(0, 3, 2, 4)]);
    const start = box(0.5, 20, 1.5, 21);
    const r = snapMove(index, start, { dx: 0, dy: -13.92 }, { threshold: T });
    expect(r.dy).toBeCloseTo(-14); // lands at 6…7: 2 above B
    expect(rounded(r.marks.gaps)).toEqual(
      expect.arrayContaining([
        { axis: "y", from: 4, to: 6, at: 1 },
        { axis: "y", from: 1, to: 3, at: 1 },
      ]),
    );
  });

  it("snaps midway between two props, marking both gaps", () => {
    const index = guideIndex([box(0, 0, 2, 1), box(10, 0, 12, 1)]);
    const start = box(20, 0.25, 22, 0.75);
    // Midway puts the 2-wide prop at 5…7, 3 from each.
    const r = snapMove(index, start, { dx: -14.9, dy: 0 }, { threshold: T });
    expect(r.dx).toBeCloseTo(-15);
    expect(rounded(r.marks.gaps)).toEqual([
      { axis: "x", from: 2, to: 5, at: 0.5 },
      { axis: "x", from: 7, to: 10, at: 0.5 },
    ]);
  });

  it("ignores props outside the row", () => {
    const index = guideIndex([box(0, 5, 2, 6), box(4, 5, 6, 6)]);
    const start = box(20, 0, 22, 1);
    const r = snapMove(index, start, { dx: -11.9, dy: 0 }, { threshold: T });
    expect(r.dx).toBeCloseTo(-11.9);
    expect(r.marks.gaps).toEqual([]);
  });

  it("prefers whichever of alignment and spacing is nearer", () => {
    // Equal spacing wants the left edge at 8; C's left edge is at 8.1.
    const index = guideIndex([box(0, 0, 2, 1), box(4, 0, 6, 1), box(8.1, 5, 9, 6)]);
    const start = box(20, 0, 22, 1);
    expect(snapMove(index, start, { dx: -11.97, dy: 0 }, { threshold: T }).dx).toBeCloseTo(-12);
    expect(snapMove(index, start, { dx: -11.88, dy: 0 }, { threshold: T }).dx).toBeCloseTo(-11.9);
  });
});

describe("snapMove: with the grid", () => {
  const index = guideIndex([box(0, 0, 2, 1)]);
  const start = box(10.3, 10, 12.3, 11);

  it("uses the grid's move when no guide is near", () => {
    const r = snapMove(index, start, { dx: -3.2, dy: -2.1 }, { threshold: T, fallback: { dx: -3.3, dy: -2 } });
    expect(r.dx).toBe(-3.3);
    expect(r.dy).toBe(-2);
    expect(r.marks.guides).toEqual([]);
  });

  it("lets a guide within the threshold win over the grid, axis by axis", () => {
    // x: left edge at 0.05 snaps to 0 (the grid would put it at 0.2); y: the grid applies.
    const r = snapMove(index, start, { dx: -10.25, dy: -2.1 }, { threshold: T, fallback: { dx: -10.1, dy: -2 } });
    expect(r.dx).toBeCloseTo(-10.3);
    expect(r.dy).toBe(-2);
  });
});

describe("snapResize", () => {
  const scale = (ax: number, ay: number, fx: number, fy: number): Gesture => ({ kind: "scale", ax, ay, fx, fy });

  it("snaps the width to another prop's, marking both as the same width", () => {
    const index = guideIndex([box(0, 0, 4, 2)]);
    const start = box(10, 10, 12, 12);
    // East handle: anchored on the left edge (x 10), the right edge at 13.9: width 3.9.
    const r = snapResize(index, start, "e", scale(10, 11, 1.95, 1), { threshold: T, keepAspect: false });
    expect(r.gesture).toMatchObject({ kind: "scale", ax: 10, ay: 11, fy: 1 });
    expect((r.gesture as { fx: number }).fx).toBeCloseTo(2);
    expect(r.marks.sizes).toEqual([
      { dim: "width", box: box(10, 10, 14, 12), moving: true },
      { dim: "width", box: box(0, 0, 4, 2), moving: false },
    ]);
  });

  it("snaps the height from the bottom handle", () => {
    const index = guideIndex([box(0, 0, 4, 3)]);
    const start = box(10, 10, 12, 12);
    // South handle: anchored on the top (y 12), the bottom edge at 9.05: height 2.95.
    const r = snapResize(index, start, "s", scale(11, 12, 1, 1.475), { threshold: T, keepAspect: false });
    expect((r.gesture as { fy: number }).fy).toBeCloseTo(1.5);
    expect(r.marks.sizes.map((s) => [s.dim, s.moving])).toEqual([
      ["height", true],
      ["height", false],
    ]);
  });

  it("snaps a dragged edge to another prop's edge, with a guide", () => {
    const index = guideIndex([box(0, 5, 13, 6)]);
    const start = box(10, 10, 12, 12);
    const r = snapResize(index, start, "e", scale(10, 11, 1.45, 1), { threshold: T, keepAspect: false });
    expect((r.gesture as { fx: number }).fx).toBeCloseTo(1.5);
    expect(r.marks.guides).toEqual([{ axis: "x", at: 13, from: 5, to: 12 }]);
  });

  it("keeps proportions when asked, following whichever side snapped", () => {
    const index = guideIndex([box(0, 0, 4, 9)]);
    const start = box(10, 10, 12, 11);
    // NE corner, anchored at the bottom left (10, 10): 3.96 wide snaps to 4; height follows.
    const r = snapResize(index, start, "ne", scale(10, 10, 1.98, 1.98), { threshold: T, keepAspect: true });
    const g = r.gesture as { fx: number; fy: number };
    expect(g.fx).toBeCloseTo(2);
    expect(g.fy).toBeCloseTo(2);
  });

  it("leaves turned frames and far-off sizes alone", () => {
    const index = guideIndex([box(0, 0, 4, 2)]);
    const start = box(10, 10, 12, 12);
    const turned: Gesture = { kind: "scale", ax: 10, ay: 11, fx: 1.95, fy: 1, deg: 30 };
    expect(snapResize(index, start, "e", turned, { threshold: T, keepAspect: false }).gesture).toBe(turned);
    const far = scale(10, 11, 1.7, 1);
    expect(snapResize(index, start, "e", far, { threshold: T, keepAspect: false }).gesture).toEqual(far);
  });
});

describe("snapPointTo", () => {
  const index = guideIndex([box(0, 0, 2, 1)]);

  it("snaps a drawn point to other props' edges and centers, with guides", () => {
    const r = snapPointTo(index, { x: 2.1, y: 7 }, { threshold: T });
    expect(r.point).toEqual({ x: 2, y: 7 });
    expect(r.marks.guides).toEqual([{ axis: "x", at: 2, from: 0, to: 7 }]);
    expect(snapPointTo(index, { x: 5, y: 0.45 }, { threshold: T }).point).toEqual({ x: 5, y: 0.5 });
  });

  it("falls back to the grid point when nothing is near", () => {
    const r = snapPointTo(index, { x: 5.2, y: 7.1 }, { threshold: T, fallback: { x: 5, y: 7 } });
    expect(r.point).toEqual({ x: 5, y: 7 });
    expect(r.marks.guides).toEqual([]);
  });
});

describe("nearbyBoxes", () => {
  it("keeps the boxes in view, and only the nearest few when there are many", () => {
    const boxes = [box(0, 0, 1, 1), box(100, 100, 101, 101), box(5, 0, 6, 1), box(-9, 0, -8, 1)];
    const view = box(-10, -10, 10, 10);
    expect(nearbyBoxes(boxes, view, { x: 0, y: 0 })).toEqual([boxes[0], boxes[2], boxes[3]]);
    expect(nearbyBoxes(boxes, view, { x: 6, y: 0 }, 2)).toEqual([boxes[2], boxes[0]]);
  });
});

describe("formatGap", () => {
  it("shows a gap as a plain number, like the properties panel, to two decimals", () => {
    expect(formatGap(2)).toBe("2");
    expect(formatGap(1.2549)).toBe("1.25");
    expect(formatGap(0.3333333)).toBe("0.33");
  });
});

describe("speed", () => {
  it("snaps among hundreds of props without trying every pair", () => {
    const boxes: Box[] = [];
    for (let i = 0; i < 800; i++) boxes.push(box((i % 40) * 3, Math.floor(i / 40) * 3, (i % 40) * 3 + 2, Math.floor(i / 40) * 3 + 1));
    const index = guideIndex(boxes);
    const start = box(200, 200, 202, 201);
    const t0 = performance.now();
    for (let i = 0; i < 200; i++) snapMove(index, start, { dx: -100 - i * 0.37, dy: -150 + i * 0.11 }, { threshold: T });
    // Generous: about 1 ms a move on a slow machine.
    expect(performance.now() - t0).toBeLessThan(400);
  });
});
