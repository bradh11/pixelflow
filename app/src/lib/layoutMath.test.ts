import { describe, expect, it } from "vitest";
import { emptyShow } from "../api/memory";
import type { Background, PreviewProp, Transform } from "../api/types";
import { applyTransform, frontView, localPositions } from "./geometry";
import {
  DRAWN_BY_ENDS,
  resizeView,
  alignMoves,
  backgroundBox,
  besideBox,
  boxFrom,
  boxOfPoints,
  copyName,
  defaultBackground,
  distributeMoves,
  drawnProp,
  fitView,
  gesturePoint,
  gestureTransform,
  handleAt,
  hitTest,
  isNoop,
  moveGesture,
  normalizeDeg,
  nudgeStep,
  panBy,
  placedProp,
  propsInBox,
  resizeBackground,
  rotateGesture,
  scaleGesture,
  snapPoint,
  toScreen,
  toWorld,
  unionBox,
  wheelIntent,
  wheelZoomFactor,
  composeGestures,
  constrainAngle,
  frameAngle,
  frameOfPoints,
  handleCursor,
  handlePositions,
  propAngles,
  visibleHandles,
  pinchFactor,
  zoomAt,
  type Gesture,
  type View,
} from "./layoutMath";
import { newProp } from "./shows";

const size = { width: 800, height: 600 };
const view: View = { cx: 0, cy: 0, zoom: 10 };
const preview = (id: string, points: number[]): PreviewProp => ({ prop: id, frameOffset: 0, channelsPerPixel: 3, points });

describe("the view", () => {
  it("maps world points to the screen (y up) and back", () => {
    expect(toScreen(view, size, { x: 0, y: 0 })).toEqual({ x: 400, y: 300 });
    expect(toScreen(view, size, { x: 1, y: 1 })).toEqual({ x: 410, y: 290 });
    expect(toWorld(view, size, { x: 410, y: 290 })).toEqual({ x: 1, y: 1 });
  });

  it("zooms about the cursor, keeping the point under it still", () => {
    const cursor = { x: 600, y: 100 };
    const before = toWorld(view, size, cursor);
    const zoomed = zoomAt(view, size, cursor, 2);
    expect(zoomed.zoom).toBe(20);
    const after = toWorld(zoomed, size, cursor);
    expect(after.x).toBeCloseTo(before.x);
    expect(after.y).toBeCloseTo(before.y);
    expect(zoomAt(view, size, cursor, 1e9).zoom).toBe(2000);
  });

  it("zooms by each step of a WebKit pinch, ignoring odd values", () => {
    expect(pinchFactor(1, 1.1)).toBeCloseTo(1.1);
    expect(pinchFactor(1.1, 1.21)).toBeCloseTo(1.1);
    expect(pinchFactor(1, 0.8)).toBeCloseTo(0.8);
    expect(pinchFactor(1, 10)).toBe(2);
    expect(pinchFactor(0, 1.5)).toBe(1);
    expect(pinchFactor(1, Number.NaN)).toBe(1);
  });

  it("pans so the picture follows the drag", () => {
    const moved = panBy(view, 50, -20);
    expect(toScreen(moved, size, { x: 0, y: 0 })).toEqual({ x: 450, y: 280 });
  });

  it("fits a box with padding, and copes with no size or nothing to show", () => {
    const fitted = fitView({ minX: -10, minY: 0, maxX: 10, maxY: 5 }, size, 40);
    expect(fitted).toEqual({ cx: 0, cy: 2.5, zoom: 36 });
    expect(fitView(null, size).zoom).toBeGreaterThan(0);
    expect(fitView({ minX: 2, minY: 3, maxX: 2, maxY: 3 }, size)).toMatchObject({ cx: 2, cy: 3 });
    expect(fitView({ minX: 2, minY: 3, maxX: 4, maxY: 5 }, { width: 0, height: 0 })).toMatchObject({ cx: 3, cy: 4 });
  });

  it("zooms for pinches, ⌘-scroll, and line-by-line mouse wheels, and pans for every other scroll", () => {
    const wheel = { deltaX: 0, deltaY: 0, deltaMode: 0, ctrlKey: false, metaKey: false };
    expect(wheelIntent({ ...wheel, deltaY: 3.5, ctrlKey: true })).toBe("zoom");
    expect(wheelIntent({ ...wheel, deltaY: 10, metaKey: true })).toBe("zoom");
    expect(wheelIntent({ ...wheel, deltaY: 3, deltaMode: 1 })).toBe("zoom");
    // A whole-pixel vertical step could be a mouse wheel or a fast trackpad flick: it pans.
    expect(wheelIntent({ ...wheel, deltaY: 100 })).toBe("pan");
    expect(wheelIntent({ ...wheel, deltaY: 12.5, deltaX: 2 })).toBe("pan");
    expect(wheelIntent({ ...wheel, deltaY: 4 })).toBe("pan");
    expect(wheelZoomFactor({ ...wheel, deltaY: -100 })).toBeGreaterThan(1);
    expect(wheelZoomFactor({ ...wheel, deltaY: 100 })).toBeLessThan(1);
  });
});

describe("boxes and picking", () => {
  it("bounds points and joins boxes", () => {
    expect(boxOfPoints([1, 2, -3, 5, 0, 0])).toEqual({ minX: -3, minY: 0, maxX: 1, maxY: 5 });
    expect(boxOfPoints([])).toBeNull();
    expect(unionBox([null, { minX: 0, minY: 0, maxX: 1, maxY: 1 }, { minX: -1, minY: 2, maxX: 0, maxY: 3 }])).toEqual({
      minX: -1,
      minY: 0,
      maxX: 1,
      maxY: 3,
    });
    expect(boxFrom({ x: 3, y: 0 }, { x: 1, y: 2 })).toEqual({ minX: 1, minY: 0, maxX: 3, maxY: 2 });
  });

  it("picks the prop with the nearest pixel within reach", () => {
    const props = [preview("a", [0, 0, 1, 0]), preview("b", [1.2, 0])];
    expect(hitTest(props, { x: 1.15, y: 0.05 }, 0.5)).toBe("b");
    expect(hitTest(props, { x: 0.1, y: 0 }, 0.5)).toBe("a");
    expect(hitTest(props, { x: 5, y: 5 }, 0.5)).toBeNull();
  });

  it("picks a prop by a click inside its outline, the smallest one when outlines overlap", () => {
    // A big ring of pixels with a small square of pixels inside it.
    const ring = preview("ring", [-5, 0, 0, 5, 5, 0, 0, -5]);
    const square = preview("square", [1, 1, 2, 1, 2, 2, 1, 2]);
    expect(hitTest([ring, square], { x: -2, y: -2 }, 0.2)).toBe("ring");
    expect(hitTest([ring, square], { x: 1.5, y: 1.5 }, 0.2)).toBe("square");
    expect(hitTest([square, ring], { x: 1.5, y: 1.5 }, 0.2)).toBe("square");
    expect(hitTest([ring, square], { x: 6, y: 6 }, 0.2)).toBeNull();
    // A pixel within reach still beats an outline.
    expect(hitTest([square, ring], { x: 0.1, y: 4.9 }, 0.2)).toBe("ring");

    // A line turned 45°: its outline is the line itself, not the square around it.
    const diagonal = preview("diagonal", [0, 0, 1, 1, 2, 2, 3, 3]);
    const angles = new Map([["diagonal", 45]]);
    expect(hitTest([diagonal], { x: 2.5, y: 0.5 }, 0.2, angles)).toBeNull();
    expect(hitTest([diagonal], { x: 2.5, y: 0.5 }, 0.2)).toBe("diagonal");
    expect(hitTest([diagonal], { x: 1.6, y: 1.5 }, 0.2, angles)).toBe("diagonal");
  });

  it("lists the angles of turned props", () => {
    const show = emptyShow("x");
    const [a, b, c] = [newProp("line", show), newProp("line", show), newProp("line", show)];
    b.transform.rotationDeg.z = 30;
    c.transform.rotationDeg = { x: 40, y: 0, z: 10 };
    expect([...propAngles([a, b, c])]).toEqual([[b.id, 30]]);
  });

  it("box-selects props with any pixel inside", () => {
    const props = [preview("a", [0, 0, 5, 5]), preview("b", [10, 10])];
    expect(propsInBox(props, { minX: 4, minY: 4, maxX: 6, maxY: 6 })).toEqual(["a"]);
    expect(propsInBox(props, { minX: -1, minY: -1, maxX: 11, maxY: 11 })).toEqual(["a", "b"]);
  });

  it("snaps to the grid", () => {
    expect(snapPoint({ x: 1.26, y: -0.74 }, 0.5)).toEqual({ x: 1.5, y: -0.5 });
    expect(snapPoint({ x: 0.3, y: 0.1 }, 0.25)).toEqual({ x: 0.25, y: 0 });
  });

  it("finds the corner and turn handles in screen pixels", () => {
    const box = { minX: 0, minY: 0, maxX: 10, maxY: 5 };
    expect(handleAt(box, view, size, { x: 402, y: 249 })).toBe("nw");
    expect(handleAt(box, view, size, { x: 500, y: 300 })).toBe("se");
    expect(handleAt(box, view, size, { x: 450, y: 222 })).toBe("rotate");
    expect(handleAt(box, view, size, { x: 450, y: 280 })).toBeNull();
  });
});

const transform = (x: number, y: number, rz: number, sx = 1, sy = 1): Transform => ({
  position: { x, y, z: 0 },
  rotationDeg: { x: 0, y: 0, z: rz },
  scale: { x: sx, y: sy, z: 1 },
});

/** The gesture moved every pixel of a prop exactly where it was drawn during the drag. */
function expectConsistent(g: Gesture, t: Transform, kind: "matrix" | "arch" = "matrix") {
  const prop = { ...newProp(kind, emptyShow("x")), transform: t };
  const before = frontView(prop);
  const after = frontView({ ...prop, transform: gestureTransform(g, t) });
  for (let i = 0; i < before.length; i += 2) {
    const want = gesturePoint(g, { x: before[i], y: before[i + 1] });
    expect(after[i]).toBeCloseTo(want.x, 2);
    expect(after[i + 1]).toBeCloseTo(want.y, 2);
  }
}

describe("gestures", () => {
  it("moves, turns about a point, and resizes from an anchor, matching what was drawn", () => {
    for (const t of [transform(0, 0, 0), transform(3, -2, 30, 2, 2), transform(-1, 4, -120, 0.5, 0.5)]) {
      expectConsistent({ kind: "move", dx: 2.5, dy: -1 }, t);
      expectConsistent({ kind: "rotate", cx: 1, cy: 2, deg: 45 }, t);
      expectConsistent({ kind: "scale", ax: -2, ay: 1, fx: 1.5, fy: 1.5 }, t);
    }
    // A stretch is exact for props that aren't turned, or are turned a quarter turn.
    expectConsistent({ kind: "scale", ax: 0, ay: 0, fx: 2, fy: 0.5 }, transform(1, 1, 0));
    expectConsistent({ kind: "scale", ax: 0, ay: 0, fx: 2, fy: 0.5 }, transform(1, 1, 90));
  });

  it("keeps angles between -180 and 180", () => {
    expect(normalizeDeg(190)).toBe(-170);
    expect(normalizeDeg(-540)).toBe(180);
    expect(gestureTransform({ kind: "rotate", cx: 0, cy: 0, deg: 100 }, transform(0, 0, 100)).rotationDeg.z).toBe(-160);
  });

  it("resizes from a corner with the opposite corner fixed, freely unless keeping proportions", () => {
    const box = { minX: 0, minY: 0, maxX: 4, maxY: 2 };
    // Dragging the top-right corner twice as far from the bottom-left corner doubles the size.
    expect(scaleGesture(box, "ne", { x: 4, y: 2 }, { x: 8, y: 4 }, true)).toEqual({ kind: "scale", ax: 0, ay: 0, fx: 2, fy: 2 });
    expect(scaleGesture(box, "ne", { x: 4, y: 2 }, { x: 8, y: 3 }, true)).toMatchObject({ fx: expect.closeTo(1.9, 5), fy: expect.closeTo(1.9, 5) });
    expect(scaleGesture(box, "ne", { x: 4, y: 2 }, { x: 8, y: 2 }, false)).toEqual({ kind: "scale", ax: 0, ay: 0, fx: 2, fy: 1 });
    expect(scaleGesture(box, "ne", { x: 4, y: 2 }, { x: 6, y: 5 }, false)).toEqual({ kind: "scale", ax: 0, ay: 0, fx: 1.5, fy: 2.5 });
    const flipped = scaleGesture(box, "sw", { x: 0, y: 0 }, { x: 10, y: 10 }, true);
    expect(flipped).toMatchObject({ ax: 4, ay: 2 });
    expect((flipped as { fx: number }).fx).toBeGreaterThan(0);
  });

  it("stretches one way from a side handle, the opposite side fixed", () => {
    const box = { minX: 0, minY: 0, maxX: 4, maxY: 2 };
    expect(scaleGesture(box, "n", { x: 2, y: 2 }, { x: 9, y: 3 }, false)).toEqual({ kind: "scale", ax: 2, ay: 0, fx: 1, fy: 1.5 });
    expect(scaleGesture(box, "w", { x: 0, y: 1 }, { x: 2, y: 7 }, true)).toEqual({ kind: "scale", ax: 4, ay: 1, fx: 0.5, fy: 1 });
    // A line's box has no height: only its length changes.
    expect(scaleGesture({ minX: 0, minY: 1, maxX: 4, maxY: 1 }, "ne", { x: 4, y: 1 }, { x: 6, y: 3 }, false)).toMatchObject({ fx: 1.5, fy: 1 });
  });

  it("resizes a turned prop along its own axes, exactly as drawn", () => {
    const t = transform(2, 1, 30, 1, 1);
    const prop = { ...newProp("arch", emptyShow("x")), transform: t };
    const frame = frameOfPoints([frontView(prop)], 30)!;
    // The arch's own width runs along its 30° axis; its frame is as snug as when it wasn't turned.
    const plain = frameOfPoints([frontView({ ...prop, transform: transform(2, 1, 0) })], 0)!;
    expect(frame.box.maxX - frame.box.minX).toBeCloseTo(plain.box.maxX - plain.box.minX, 4);
    expect(frame.box.maxY - frame.box.minY).toBeCloseTo(plain.box.maxY - plain.box.minY, 4);

    // Drag the top-right corner out by 2 along the arch's width and 1 along its height.
    const at = handlePositions(frame, view, size);
    const ne = toWorld(view, size, at.ne);
    const to = { x: ne.x + 2 * Math.cos(Math.PI / 6) - Math.sin(Math.PI / 6), y: ne.y + 2 * Math.sin(Math.PI / 6) + Math.cos(Math.PI / 6) };
    const g = scaleGesture(frame, "ne", ne, to, false);
    const w = frame.box.maxX - frame.box.minX;
    const h = frame.box.maxY - frame.box.minY;
    expect(g).toMatchObject({ deg: 30, fx: expect.closeTo((w + 2) / w, 3), fy: expect.closeTo((h + 1) / h, 3) });
    expectConsistent(g, t, "arch");
    const after = gestureTransform(g, t);
    expect(after.rotationDeg.z).toBe(30);
    expect(after.scale.x).toBeCloseTo((w + 2) / w, 3);
    expect(after.scale.y).toBeCloseTo((h + 1) / h, 3);

    // A side handle of a prop turned a quarter turn and a bit stretches the right way too.
    const quarter = transform(0, 0, 120);
    expectConsistent({ kind: "scale", ax: 1, ay: 2, fx: 1, fy: 1.7, deg: 30 }, quarter, "arch");
    expectConsistent({ kind: "scale", ax: 1, ay: 2, fx: 0.6, fy: 1.2, deg: -15 }, transform(0, 0, -105, -1, 2), "matrix");
  });

  it("knows which turned props can be stretched, and along which axes", () => {
    const turned = (z: number, x = 0) => {
      const prop = newProp("line", emptyShow("x"));
      prop.transform.rotationDeg = { x, y: 0, z };
      return prop;
    };
    expect(frameAngle([])).toEqual({ deg: 0, stretchable: true });
    expect(frameAngle([turned(0), turned(90), turned(-180), turned(270.0004)])).toEqual({ deg: 0, stretchable: true });
    expect(frameAngle([turned(30)])).toEqual({ deg: 30, stretchable: true });
    expect(frameAngle([turned(120), turned(-60), turned(30)])).toEqual({ deg: 30, stretchable: true });
    expect(frameAngle([turned(80)])).toEqual({ deg: -10, stretchable: true });
    expect(frameAngle([turned(0), turned(30)])).toEqual({ deg: 0, stretchable: false });
    expect(frameAngle([turned(0, 45)])).toEqual({ deg: 0, stretchable: false });
    expect(frameAngle([turned(10, 180)])).toEqual({ deg: 10, stretchable: true });
  });

  it("puts side handles on long enough edges of boxes that can stretch, and turns handles with the frame", () => {
    const box = { minX: 0, minY: 0, maxX: 10, maxY: 5 };
    expect(visibleHandles(box, view, true).sort()).toEqual(["e", "n", "ne", "nw", "rotate", "s", "se", "sw", "w"]);
    expect(visibleHandles(box, view, false).sort()).toEqual(["ne", "nw", "rotate", "se", "sw"]);
    // 10 × 1 units is 100 × 10 pixels: too short for side handles on the left and right.
    expect(visibleHandles({ minX: 0, minY: 0, maxX: 10, maxY: 1 }, view, true)).not.toContain("e");
    expect(visibleHandles({ minX: 0, minY: 0, maxX: 10, maxY: 0 }, view, true)).not.toContain("n");
    expect(handleAt(box, view, size, { x: 450, y: 251 }, visibleHandles(box, view, true))).toBe("n");
    expect(handleAt(box, view, size, { x: 500, y: 275 }, visibleHandles(box, view, true))).toBe("e");

    const frame = { box: { minX: -5, minY: -2, maxX: 5, maxY: 2 }, deg: 90 };
    const at = handlePositions(frame, view, size);
    // Turned a quarter turn, the frame's top edge is on the left of the screen.
    expect(at.n.x).toBeCloseTo(380);
    expect(at.n.y).toBeCloseTo(300);
    expect(at.rotate.x).toBeCloseTo(380 - 28);
    expect(handleCursor("n", 0)).toBe("ns-resize");
    expect(handleCursor("n", 90)).toBe("ew-resize");
    expect(handleCursor("ne", 0)).toBe("nesw-resize");
    expect(handleCursor("nw", 0)).toBe("nwse-resize");
    expect(handleCursor("e", 30)).toBe("nesw-resize");
  });

  it("turns by the swept angle, in 15° steps with shift", () => {
    const c = { x: 0, y: 0 };
    expect(rotateGesture(c, { x: 1, y: 0 }, { x: 0, y: 1 }, false)).toMatchObject({ deg: 90 });
    expect(rotateGesture(c, { x: 1, y: 0 }, { x: 1, y: 0.2 }, true)).toMatchObject({ deg: 15 });
  });

  it("moves freely, or so the first prop's origin lands on the grid", () => {
    expect(moveGesture({ x: 0, y: 0 }, { x: 1.23, y: 0.4 }, null, null)).toEqual({ kind: "move", dx: 1.23, dy: 0.4 });
    expect(moveGesture({ x: 0, y: 0 }, { x: 1.23, y: 0.4 }, { x: 0.1, y: 0 }, 0.5)).toEqual({ kind: "move", dx: 1.4, dy: 0.5 });
  });

  it("moves straight across or straight up and down with Shift, whichever the drag went further", () => {
    expect(moveGesture({ x: 0, y: 0 }, { x: 1.23, y: 0.4 }, null, null, true)).toEqual({ kind: "move", dx: 1.23, dy: 0 });
    expect(moveGesture({ x: 0, y: 0 }, { x: -0.3, y: -2 }, null, null, true)).toEqual({ kind: "move", dx: 0, dy: -2 });
    // With snap on, only the axis it moves along snaps: the other stays exactly where it was.
    expect(moveGesture({ x: 0, y: 0 }, { x: 1.23, y: 0.4 }, { x: 0.1, y: 0.13 }, 0.5, true)).toEqual({ kind: "move", dx: 1.4, dy: 0 });
  });

  it("keeps a drawn line to multiples of 45° with Shift", () => {
    const from = { x: 1, y: 1 };
    expect(constrainAngle(from, { x: 5, y: 1.4 })).toEqual({ x: 5, y: 1 });
    expect(constrainAngle(from, { x: 0.8, y: -3 })).toEqual({ x: 1, y: -3 });
    const diagonal = constrainAngle(from, { x: 4, y: 3.6 });
    expect(diagonal.x - from.x).toBeCloseTo(diagonal.y - from.y, 10);
    expect(diagonal.x - from.x).toBeCloseTo(2.8, 5);
    expect(constrainAngle(from, { x: -2, y: 4.2 })).toMatchObject({ x: expect.closeTo(-2.1, 5), y: expect.closeTo(4.1, 5) });
    expect(constrainAngle(from, from)).toEqual(from);
    // The line drawn there runs at exactly that angle.
    const line = drawnProp("line", from, constrainAngle(from, { x: 1.3, y: 6 }), newProp("line", emptyShow("x")));
    expect(line.transform.rotationDeg.z).toBe(90);
    const arch = drawnProp("arch", from, constrainAngle(from, { x: -3, y: -3.4 }), newProp("arch", emptyShow("x")));
    expect(arch.transform.rotationDeg.z).toBe(-135);
  });

  it("knows a gesture that changes nothing", () => {
    expect(isNoop({ kind: "move", dx: 0, dy: 0 })).toBe(true);
    expect(isNoop({ kind: "scale", ax: 0, ay: 0, fx: 1, fy: 1 })).toBe(true);
    expect(isNoop({ kind: "rotate", cx: 0, cy: 0, deg: 5 })).toBe(false);
  });
});

describe("drawing new props", () => {
  const show = emptyShow("x");
  const ends = (kind: "line" | "arch", a: [number, number], b: [number, number]) => {
    const prop = drawnProp(kind, { x: a[0], y: a[1] }, { x: b[0], y: b[1] }, newProp(kind, show));
    const pts = localPositions(prop.shape).map((p) => applyTransform(p, prop.transform));
    return { prop, first: pts[0], last: pts[pts.length - 1] };
  };

  it("runs a line or arch from where the drag started to where it ended", () => {
    for (const kind of ["line", "arch"] as const) {
      const { first, last } = ends(kind, [1, 1], [1, 5]);
      expect(first.x).toBeCloseTo(1);
      expect(first.y).toBeCloseTo(1);
      expect(last.x).toBeCloseTo(1);
      expect(last.y).toBeCloseTo(5);
    }
    const { prop } = ends("arch", [0, 0], [6, 0]);
    expect(prop.shape).toMatchObject({ width: 6, height: 3, nodes: 50 });
    expect(prop.transform.rotationDeg.z).toBe(0);
  });

  it("stands candy canes and hangs icicles between where the drag started and ended", () => {
    const [a, b] = [{ x: 1, y: 1 }, { x: 5, y: 4 }];
    expect(DRAWN_BY_ENDS).toEqual(expect.arrayContaining(["candyCanes", "icicles"]));
    for (const kind of ["candyCanes", "icicles"] as const) {
      const base = newProp(kind, show);
      const prop = drawnProp(kind, a, b, base);
      expect(prop.shape).toMatchObject({ width: 5 });
      expect(prop.transform.position).toMatchObject({ x: 3, y: 2.5 });
      expect(prop.transform.rotationDeg.z).toBeCloseTo((Math.atan2(3, 4) * 180) / Math.PI, 2);
      // The first cane's foot, or the first drop's top, is where the drag started.
      const [first] = localPositions(prop.shape).map((p) => applyTransform(p, prop.transform));
      expect(first.x).toBeCloseTo(1);
      expect(first.y).toBeCloseTo(1);
      if (kind === "icicles") expect(prop.shape).toMatchObject({ dropHeight: (base.shape as { dropHeight: number }).dropHeight });
    }
  });

  it("fills the drawn box with a matrix, tree, circle, or star", () => {
    const a = { x: 2, y: 1 };
    const b = { x: 6, y: 7 };
    const box = (kind: "matrix" | "tree" | "circle" | "star") => boxOfPoints(frontView(drawnProp(kind, a, b, newProp(kind, show))))!;
    const matrix = box("matrix");
    expect(matrix.minX).toBeCloseTo(2);
    expect(matrix.maxY).toBeCloseTo(7);
    const tree = box("tree");
    expect(tree.minY).toBeCloseTo(1);
    expect(tree.maxY).toBeCloseTo(7);
    expect(tree.maxX).toBeCloseTo(6, 1);
    const circle = box("circle");
    expect(circle.maxX - circle.minX).toBeCloseTo(4, 1);
    expect((circle.minY + circle.maxY) / 2).toBeCloseTo(4, 1);
    expect(box("star").maxY).toBeCloseTo(6);
  });

  it("fills the drawn box with a window frame, and fits a wreath or spinner in it", () => {
    const a = { x: 2, y: 1 };
    const b = { x: 6, y: 7 };
    const frame = boxOfPoints(frontView(drawnProp("windowFrame", a, b, newProp("windowFrame", show))))!;
    expect([frame.minX, frame.minY, frame.maxX, frame.maxY].map((v) => Math.round(v * 1e4) / 1e4)).toEqual([2, 1, 6, 7]);
    for (const kind of ["wreath", "spinner"] as const) {
      const prop = drawnProp(kind, a, b, newProp(kind, show));
      expect(prop.shape).toMatchObject({ radius: 2 });
      expect(prop.transform.position).toMatchObject({ x: 4, y: 4 });
    }
    const wreath = boxOfPoints(frontView(drawnProp("wreath", a, b, newProp("wreath", show))))!;
    expect(wreath.maxY).toBeCloseTo(6);
    expect(wreath.maxX - wreath.minX).toBeCloseTo(4, 1);
  });

  it("places a clicked prop at the click with its own size", () => {
    const prop = placedProp(newProp("matrix", show), { x: 3, y: 4 });
    expect(prop.transform.position).toEqual({ x: 3, y: 4, z: 0 });
  });

  it("puts an added prop just right of the others", () => {
    expect(besideBox({ minX: 0, minY: 0, maxX: 10, maxY: 5 }, { minX: -2, minY: 0, maxX: 2, maxY: 2 })).toEqual({ x: 13, y: 0 });
    expect(besideBox(null, null)).toEqual({ x: 0, y: 0 });
  });
});

describe("arranging", () => {
  const boxes = [
    { id: "a", box: { minX: 0, minY: 0, maxX: 2, maxY: 2 } },
    { id: "b", box: { minX: 5, minY: 3, maxX: 6, maxY: 4 } },
    { id: "c", box: { minX: 10, minY: 1, maxX: 14, maxY: 2 } },
  ];

  it("lines boxes up on any side or center", () => {
    expect(alignMoves(boxes, "left").get("c")).toEqual({ x: -10, y: 0 });
    expect(alignMoves(boxes, "right").get("a")).toEqual({ x: 12, y: 0 });
    expect(alignMoves(boxes, "center").get("b")).toEqual({ x: 1.5, y: 0 });
    expect(alignMoves(boxes, "top").get("a")).toEqual({ x: 0, y: 2 });
    expect(alignMoves(boxes, "bottom").get("b")).toEqual({ x: 0, y: -3 });
    expect(alignMoves(boxes, "middle").get("c")).toEqual({ x: 0, y: 0.5 });
  });

  it("spaces boxes with equal gaps, keeping the outer ones", () => {
    const moves = distributeMoves(boxes, "horizontal");
    expect(moves.get("a")).toEqual({ x: 0, y: 0 });
    expect(moves.get("c")).toEqual({ x: 0, y: 0 });
    // Gaps: (14 - 0 - (2 + 1 + 4)) / 2 = 3.5, so b starts at 5.5.
    expect(moves.get("b")).toEqual({ x: 0.5, y: 0 });
    expect(distributeMoves(boxes.slice(0, 2), "vertical").size).toBe(0);
  });

  it("nudges by a tenth of a unit, or a grid step, ten times with shift", () => {
    expect(nudgeStep(false, 0.5, false)).toBe(0.1);
    expect(nudgeStep(false, 0.5, true)).toBe(1);
    expect(nudgeStep(true, 0.5, true)).toBe(5);
  });

  it("names copies", () => {
    expect(copyName("Arch 1", ["Arch 1"])).toBe("Arch 1 copy");
    expect(copyName("Arch 1", ["Arch 1", "Arch 1 copy"])).toBe("Arch 1 copy 2");
  });
});

describe("the background photo", () => {
  const bg: Background = { path: "/house.jpg", x: -10, y: 8, width: 20, opacity: 0.7 };

  it("spans its width and the height its shape gives", () => {
    expect(backgroundBox(bg, 0.5)).toEqual({ minX: -10, maxX: 10, maxY: 8, minY: -2 });
  });

  it("resizes from a corner keeping the opposite corner and its shape", () => {
    const bigger = resizeBackground(bg, 0.5, "se", { x: 20, y: -5 });
    expect(backgroundBox(bigger, 0.5)).toEqual({ minX: -10, maxX: 20, maxY: 8, minY: -7 });
    const fromTopLeft = resizeBackground(bg, 0.5, "nw", { x: 0, y: 0 });
    expect(backgroundBox(fromTopLeft, 0.5)).toMatchObject({ maxX: 10, minY: -2, minX: 0 });
  });

  it("starts a little larger than the props, centered on them", () => {
    const placed = defaultBackground("/p.jpg", { minX: 0, minY: 0, maxX: 20, maxY: 5 }, 0.5);
    const box = backgroundBox(placed, 0.5);
    expect((box.minX + box.maxX) / 2).toBeCloseTo(10);
    expect((box.minY + box.maxY) / 2).toBeCloseTo(2.5);
    expect(box.maxX - box.minX).toBeCloseTo(28);
    expect(defaultBackground("/p.jpg", null, 0.5)).toMatchObject({ width: 20, opacity: 0.7 });
  });
});

describe("gestures on their way", () => {
  it("draws each prop moved by every gesture that lists it, in order", () => {
    const props = [preview("a", [0, 0, 1, 0]), preview("b", [5, 5])];
    const layers: { ids: string[]; gesture: Gesture }[] = [
      { ids: ["a"], gesture: { kind: "move", dx: 1, dy: 0 } },
      { ids: ["a", "b"], gesture: { kind: "rotate", cx: 0, cy: 0, deg: 90 } },
    ];
    const [a, b] = composeGestures(props, layers);
    const pts = Array.from(a.points);
    [0, 1, 0, 2].forEach((v, i) => expect(pts[i]).toBeCloseTo(v));
    const q = Array.from(b.points);
    expect(q[0]).toBeCloseTo(-5);
    expect(q[1]).toBeCloseTo(5);
    expect(composeGestures(props, [])).toBe(props);
    expect(composeGestures(props, [layers[0]])[1]).toBe(props[1]);
  });
});

describe("resizing the canvas", () => {
  it("keeps the same part of the layout in view, scaled to the new size", () => {
    const view = { cx: 3, cy: 2, zoom: 40 };
    const bigger = resizeView(view, { width: 800, height: 400 }, { width: 1600, height: 800 });
    expect(bigger).toEqual({ cx: 3, cy: 2, zoom: 80 });
    // Narrower than it is tall: the tighter direction decides, so nothing that was visible is cut off.
    const narrower = resizeView(view, { width: 800, height: 400 }, { width: 400, height: 400 });
    expect(narrower.zoom).toBe(20);
    // A zero-size moment (window minimized) leaves the view alone.
    expect(resizeView(view, { width: 0, height: 0 }, { width: 800, height: 400 })).toBe(view);
    expect(resizeView(view, { width: 800, height: 400 }, { width: 0, height: 300 })).toBe(view);
  });
});
