import { PerspectiveCamera, Vector3 } from "three";
import { describe, expect, it } from "vitest";
import type { PreviewProp3d, Transform } from "../api/types";
import {
  type Box3,
  type Orbit,
  type V3,
  EYE_HEIGHT,
  FOV_DEG,
  MAX_PITCH,
  MIN_PITCH,
  DOT_GAP,
  HALO_FROM,
  SPREAD_SIZE,
  backdropBox,
  boundsOfXyz,
  clipRange,
  composeGestures3d,
  coneFrom,
  dollyAt,
  dotOrder,
  dragDelta,
  fillColors,
  fitModelPlacement,
  fitOrbit,
  modelPoint,
  rotatedBox,
  focusOrbit,
  freeDragHandle,
  gizmoHit,
  gizmoLength,
  lightDim,
  lightHalo,
  lightWidth,
  moveGesture3,
  neighborShare,
  orbitBy,
  orbitEye,
  packFloods,
  packPositions,
  packSpread,
  panBy3,
  parseOrbit,
  pickPixel,
  pickProp,
  pixelsPerUnit,
  planeSquare,
  presetOrbit,
  project,
  propsInRect,
  screenRay,
  stepOrbit,
  stillDot,
  unionBox3,
  v3,
  viewProjection,
} from "./layout3d";
import { gestureTransform, isNoop } from "./layoutMath";
import { FILL, FINE_FROM, REACH, dotSize, pixelRoom, pixelSpacing, pixelSpread3d, pixelsAllRound } from "./pixelBatches";
import { coreDim, glowStep } from "./pixelGlow";

const size = { width: 800, height: 500 };
const orbit: Orbit = { target: v3(1, 2, -1), yaw: 0.6, pitch: 0.3, distance: 20 };

const prop = (id: string, xyz: number[], frameOffset = 0, channelsPerPixel = 3): PreviewProp3d => ({
  prop: id,
  frameOffset,
  channelsPerPixel,
  xyz: new Float32Array(xyz),
});

/** The same camera in three.js, for checking the math against the renderer's. */
function threeCamera(o: Orbit): PerspectiveCamera {
  const { near, far } = clipRange(o);
  const camera = new PerspectiveCamera(FOV_DEG, size.width / size.height, near, far);
  const eye = orbitEye(o);
  camera.position.set(eye.x, eye.y, eye.z);
  camera.lookAt(o.target.x, o.target.y, o.target.z);
  camera.updateMatrixWorld();
  return camera;
}

describe("the 3D camera", () => {
  it("projects points exactly where three.js draws them", () => {
    const camera = threeCamera(orbit);
    const m = viewProjection(orbit, size);
    for (const [x, y, z] of [
      [0, 0, 0],
      [3, 4, -2],
      [-5, 1, 6],
    ]) {
      const ndc = new Vector3(x, y, z).project(camera);
      const q = project(m, size, v3(x, y, z))!;
      expect(q.x).toBeCloseTo(((ndc.x + 1) / 2) * size.width, 6);
      expect(q.y).toBeCloseTo(((1 - ndc.y) / 2) * size.height, 6);
    }
  });

  it("puts its target in the middle of the view, and skips points behind it", () => {
    const m = viewProjection(orbit, size);
    const q = project(m, size, orbit.target)!;
    expect(q.x).toBeCloseTo(400);
    expect(q.y).toBeCloseTo(250);
    expect(q.depth).toBeCloseTo(20);
    const eye = orbitEye(orbit);
    const behind = v3(eye.x + (eye.x - orbit.target.x), eye.y + (eye.y - orbit.target.y), eye.z + (eye.z - orbit.target.z));
    expect(project(m, size, behind)).toBeNull();
  });

  it("casts a ray from the eye through a screen point", () => {
    const m = viewProjection(orbit, size);
    const p = v3(4, -1, 3);
    const s = project(m, size, p)!;
    const ray = screenRay(orbit, size, s);
    const along = (p.x - ray.origin.x) * ray.dir.x + (p.y - ray.origin.y) * ray.dir.y + (p.z - ray.origin.z) * ray.dir.z;
    const closest = v3(ray.origin.x + ray.dir.x * along, ray.origin.y + ray.dir.y * along, ray.origin.z + ray.dir.z * along);
    expect(Math.hypot(closest.x - p.x, closest.y - p.y, closest.z - p.z)).toBeLessThan(1e-6);
  });

  it("fits a box in view from any angle", () => {
    const box: Box3 = { min: v3(-12, 0, -3), max: v3(12, 9, 2) };
    for (const [yaw, pitch] of [
      [0, 0],
      [0.8, 0.4],
      [-2, 1.2],
    ]) {
      const o = fitOrbit(box, size, yaw, pitch);
      const m = viewProjection(o, size);
      for (const x of [box.min.x, box.max.x])
        for (const y of [box.min.y, box.max.y])
          for (const z of [box.min.z, box.max.z]) {
            const q = project(m, size, v3(x, y, z))!;
            expect(q.x).toBeGreaterThanOrEqual(0);
            expect(q.x).toBeLessThanOrEqual(size.width);
            expect(q.y).toBeGreaterThanOrEqual(0);
            expect(q.y).toBeLessThanOrEqual(size.height);
          }
    }
    expect(fitOrbit(null, size).distance).toBeGreaterThan(0);
  });

  it("fits snugly: a house front fills the view across or top to bottom", () => {
    const box: Box3 = { min: v3(-12, 0, 0), max: v3(12, 9, 0) };
    const o = fitOrbit(box, size, 0, 0);
    const m = viewProjection(o, size);
    const [a, b] = [project(m, size, box.min)!, project(m, size, box.max)!];
    const filled = Math.max(Math.abs(b.x - a.x) / size.width, Math.abs(b.y - a.y) / size.height);
    expect(filled).toBeGreaterThan(0.85);
    expect(filled).toBeLessThanOrEqual(1);
  });

  it("has views from the front, top, sides, and the street at eye height", () => {
    const box: Box3 = { min: v3(-10, 0, -2), max: v3(10, 8, 2) };
    const front = orbitEye(presetOrbit("front", box, size));
    expect(front.x).toBeCloseTo(0);
    expect(front.y).toBeCloseTo(4);
    expect(front.z).toBeGreaterThan(10);
    const top = orbitEye(presetOrbit("top", box, size));
    expect(top.y).toBeGreaterThan(20);
    expect(Math.abs(top.z)).toBeLessThan(1);
    expect(orbitEye(presetOrbit("left", box, size)).x).toBeLessThan(-10);
    expect(orbitEye(presetOrbit("right", box, size)).x).toBeGreaterThan(10);
    const street = presetOrbit("street", box, size);
    expect(orbitEye(street).y).toBeCloseTo(EYE_HEIGHT);
    expect(orbitEye(street).z).toBeGreaterThan(front.z);
  });

  it("orbits with a drag, never flipping over the top or under the ground", () => {
    const turned = orbitBy(orbit, 100, 0);
    expect(turned.yaw).toBeLessThan(orbit.yaw);
    expect(turned.pitch).toBe(orbit.pitch);
    expect(orbitBy(orbit, 0, 10_000).pitch).toBe(MAX_PITCH);
    expect(orbitBy(orbit, 0, -10_000).pitch).toBe(MIN_PITCH);
  });

  it("pans so the scene follows the pointer", () => {
    const panned = panBy3(orbit, size, 30, -20);
    const q = project(viewProjection(panned, size), size, orbit.target)!;
    expect(q.x).toBeCloseTo(430, 3);
    expect(q.y).toBeCloseTo(230, 3);
  });

  it("zooms toward the pointer, keeping what's under it in place", () => {
    const s = { x: 600, y: 120 };
    const zoomed = dollyAt(orbit, size, s, 0.5);
    expect(zoomed.distance).toBeCloseTo(10);
    // The point under the pointer on the plane through the target, facing the camera, stays under it.
    const r = screenRay(orbit, size, s);
    const eye = orbitEye(orbit);
    const back = v3(eye.x - orbit.target.x, eye.y - orbit.target.y, eye.z - orbit.target.z);
    const t =
      ((orbit.target.x - r.origin.x) * back.x + (orbit.target.y - r.origin.y) * back.y + (orbit.target.z - r.origin.z) * back.z) /
      (r.dir.x * back.x + r.dir.y * back.y + r.dir.z * back.z);
    const under = v3(r.origin.x + r.dir.x * t, r.origin.y + r.dir.y * t, r.origin.z + r.dir.z * t);
    const q = project(viewProjection(zoomed, size), size, under)!;
    expect(q.x).toBeCloseTo(s.x, 3);
    expect(q.y).toBeCloseTo(s.y, 3);
    expect(dollyAt(orbit, size, null, 1e-9).distance).toBeGreaterThan(0);
  });

  it("aims at a prop without changing the angle", () => {
    const focused = focusOrbit(orbit, { min: v3(5, 0, 0), max: v3(7, 2, 2) }, size);
    expect(focused.target).toEqual(v3(6, 1, 1));
    expect([focused.yaw, focused.pitch]).toEqual([orbit.yaw, orbit.pitch]);
    expect(focused.distance).toBeLessThan(orbit.distance);
  });

  it("glides toward a new view, the short way round, and arrives", () => {
    const from: Orbit = { target: v3(0, 0, 0), yaw: 3, pitch: 0, distance: 10 };
    const to: Orbit = { target: v3(10, 0, 0), yaw: -3, pitch: 0.5, distance: 40 };
    const next = stepOrbit(from, to, 1 / 60);
    expect(next.target.x).toBeGreaterThan(0);
    expect(next.target.x).toBeLessThan(10);
    expect(next.yaw).toBeGreaterThan(3); // across ±π, not back through 0
    let o = from;
    for (let i = 0; i < 200 && o !== to; i++) o = stepOrbit(o, to, 1 / 60);
    expect(o).toBe(to);
  });

  it("reads a remembered camera back only if it's usable", () => {
    expect(parseOrbit(JSON.parse(JSON.stringify(orbit)))).toEqual(orbit);
    expect(parseOrbit({ ...orbit, distance: -1 })).toBeNull();
    expect(parseOrbit({ ...orbit, yaw: "x" })).toBeNull();
    expect(parseOrbit(null)).toBeNull();
    expect(parseOrbit({ ...orbit, pitch: 9 })!.pitch).toBe(MAX_PITCH);
  });
});

describe("placing a house model", () => {
  it("scales it to the photo's width, stands it on the ground, and puts its front just behind the props", () => {
    const natural: Box3 = { min: v3(-5, -1, -4), max: v3(5, 6, 4) };
    const photo: Box3 = { min: v3(-12, 0, 0), max: v3(12, 12, 0) };
    const { position, scale } = fitModelPlacement(natural, photo);
    expect(scale).toBeCloseTo(2.4);
    expect(position.x).toBeCloseTo(0);
    expect(position.y).toBeCloseTo(2.4); // its bottom (-1 × 2.4) lifted to the ground
    expect(position.z).toBeCloseTo(-9.62); // its front (4 × 2.4) just behind z = 0
  });

  /** The placed model's box in the layout, from the 8 corners of its file's box. */
  const placedBox = (natural: Box3, rotationDeg: V3, placement: { position: V3; scale: number }): Box3 => {
    const corners = [natural.min.x, natural.max.x].flatMap((x) =>
      [natural.min.y, natural.max.y].flatMap((y) => [natural.min.z, natural.max.z].map((z) => modelPoint(v3(x, y, z), { rotationDeg, ...placement }))),
    );
    return boundsOfXyz([corners.flatMap((c) => [c.x, c.y, c.z])])!;
  };

  it("turns a model like props turn: tilt (X) first, then turn (Y), then Z, about the layout's axes", () => {
    // A Z-up model's "up" stood up by a tilt of -90 stays up when it's then turned by 30.
    const up = modelPoint(v3(0, 0, 1), { rotationDeg: v3(-90, 30, 0), position: v3(0, 0, 0), scale: 1 });
    expect(up.x).toBeCloseTo(0);
    expect(up.y).toBeCloseTo(1);
    expect(up.z).toBeCloseTo(0);
    const right = modelPoint(v3(1, 0, 0), { rotationDeg: v3(-90, 30, 0), position: v3(1, 2, 3), scale: 2 });
    expect(right.x).toBeCloseTo(1 + 2 * Math.cos(Math.PI / 6));
    expect(right.y).toBeCloseTo(2);
    expect(right.z).toBeCloseTo(3 - 2 * 0.5);
  });

  it("fits a model stood up from lying down by its turned box: on the ground, front just behind the props, as wide as the photo", () => {
    // Made Z-up: 10 wide (x), 8 deep (y), 6 tall (z).
    const natural: Box3 = { min: v3(-5, -4, 0), max: v3(5, 4, 6) };
    const photo: Box3 = { min: v3(-10, 0, 0), max: v3(10, 12, 0) };
    const rotationDeg = v3(-90, 0, 0);
    expect(rotatedBox(natural, rotationDeg)).toEqual({ min: v3(-5, 0, -4), max: v3(5, 6, 4) });
    const fit = fitModelPlacement(natural, photo, rotationDeg);
    expect(fit).toEqual({ scale: 2, position: v3(0, 0, -8.02) });
    const placed = placedBox(natural, rotationDeg, fit);
    expect(placed.min.y).toBeCloseTo(0);
    expect(placed.max.z).toBeCloseTo(-0.02);
    expect(placed.max.x - placed.min.x).toBeCloseTo(20);
  });

  it("matches the photo's width along the model's turned side", () => {
    const natural: Box3 = { min: v3(-5, -4, 0), max: v3(5, 4, 6) };
    const photo: Box3 = { min: v3(-6, 0, 0), max: v3(14, 12, 0) };
    // Stood up, then turned a quarter: its 8-deep side now faces the street.
    const rotationDeg = v3(-90, 90, 0);
    const fit = fitModelPlacement(natural, photo, rotationDeg);
    expect(fit.scale).toBe(2.5);
    const placed = placedBox(natural, rotationDeg, fit);
    expect(placed.min.x).toBeCloseTo(-6);
    expect(placed.max.x).toBeCloseTo(14);
    expect(placed.min.y).toBeCloseTo(0);
    expect(placed.max.z).toBeCloseTo(-0.02);
  });

  it("keeps a model's own size when there's nothing to match", () => {
    const { position, scale } = fitModelPlacement({ min: v3(2, 0, -1), max: v3(4, 3, 1) }, null);
    expect(scale).toBe(1);
    expect(position).toEqual(v3(-3, 0, -1.02));
  });
});

describe("boxes", () => {
  it("bounds pixels, skipping broken ones, and joins boxes", () => {
    const box = boundsOfXyz([new Float32Array([1, 2, 3, -1, 5, NaN, -2, 0, 4])])!;
    expect(box).toEqual({ min: v3(-2, 0, 3), max: v3(1, 2, 4) });
    expect(boundsOfXyz([])).toBeNull();
    expect(unionBox3([box, null, { min: v3(0, -1, 0), max: v3(9, 0, 0) }])).toEqual({ min: v3(-2, -1, 0), max: v3(9, 2, 4) });
    expect(backdropBox({ path: "p", x: -12, y: 12, width: 24, opacity: 1 }, 0.5, -0.5)).toEqual({ min: v3(-12, 0, -0.5), max: v3(12, 12, -0.5) });
  });
});

describe("picking pixels", () => {
  const front: Orbit = { target: v3(0, 0, 0), yaw: 0, pitch: 0, distance: 20 };
  const m = viewProjection(front, size);
  const center = { x: 400, y: 250 };

  it("picks the prop nearest the camera among pixels close to the pointer", () => {
    const props = [prop("back", [0, 0, -5]), prop("front", [0.05, 0, 3]), prop("aside", [6, 0, 0])];
    expect(pickProp(props, m, size, center, 8)).toBe("front");
    expect(pickProp([props[0], props[2]], m, size, center, 8)).toBe("back");
    expect(pickProp(props, m, size, { x: 10, y: 10 }, 8)).toBeNull();
  });

  it("tells where the picked pixel is", () => {
    expect(pickPixel([prop("a", [9, 9, 9, 0, 0, 1])], m, size, center, 8)).toEqual({ prop: "a", point: v3(0, 0, 1) });
  });

  it("ignores pixels behind the camera", () => {
    expect(pickProp([prop("behind", [0, 0, 30])], m, size, center, 50)).toBeNull();
  });

  it("finds props with pixels inside a dragged box", () => {
    const props = [prop("a", [0, 0, 0, 100, 100, 0]), prop("b", [8, 0, 0])];
    expect(propsInRect(props, m, size, { x: 390, y: 240 }, { x: 410, y: 260 })).toEqual(["a"]);
    expect(propsInRect(props, m, size, { x: 0, y: 0 }, { x: 800, y: 500 })).toEqual(["a", "b"]);
  });
});

describe("the move gizmo", () => {
  const o: Orbit = { target: v3(0, 0, 0), yaw: 0.7, pitch: 0.4, distance: 20 };
  const m = viewProjection(o, size);
  const origin = v3(1, 1, 0);
  const rayTo = (p: { x: number; y: number; z: number }) => screenRay(o, size, project(m, size, p)!);

  it("is grabbed by its arrows and plane squares", () => {
    const len = gizmoLength(o, size, origin);
    const tip = project(m, size, v3(origin.x, origin.y + len * 0.8, origin.z))!;
    expect(gizmoHit(o, size, origin, tip)).toBe("y");
    const square = planeSquare(origin, "xz", len).map((c) => project(m, size, c)!);
    const mid = { x: square.reduce((s, c) => s + c.x, 0) / 4, y: square.reduce((s, c) => s + c.y, 0) / 4 };
    expect(gizmoHit(o, size, origin, mid)).toBe("xz");
    expect(gizmoHit(o, size, origin, { x: 5, y: 5 })).toBeNull();
  });

  it("is always the same size on screen", () => {
    const near = gizmoLength(o, size, o.target);
    const far = gizmoLength({ ...o, distance: 40 }, size, o.target);
    expect(far).toBeCloseTo(near * 2);
  });

  it("moves along one axis however the pointer wanders off it", () => {
    const d = dragDelta("x", origin, rayTo(origin), rayTo(v3(origin.x + 2, origin.y, origin.z)))!;
    expect(d.x).toBeCloseTo(2, 2);
    expect([d.y, d.z]).toEqual([0, 0]);
    const z = dragDelta("z", origin, rayTo(origin), rayTo(v3(origin.x, origin.y + 0.3, origin.z - 1.5)))!;
    expect(z.x).toBe(0);
    expect(z.z).toBeLessThan(-0.5);
  });

  it("moves across a plane, snapping to the grid", () => {
    const d = dragDelta("xz", origin, rayTo(origin), rayTo(v3(origin.x + 1.2, origin.y, origin.z - 3.1)))!;
    expect(d.x).toBeCloseTo(1.2, 2);
    expect(d.y).toBeCloseTo(0, 6);
    expect(d.z).toBeCloseTo(-3.1, 2);
    expect(dragDelta("xz", origin, rayTo(origin), rayTo(v3(origin.x + 1.2, origin.y, origin.z - 3.1)), 0.5)).toEqual(v3(1, 0, -3));
  });

  it("can't follow a drag along an axis pointing at the camera", () => {
    const head: Orbit = { target: v3(0, 0, 0), yaw: 0, pitch: 0, distance: 20 };
    const ray = screenRay(head, size, { x: 400, y: 250 });
    expect(dragDelta("z", v3(0, 0, 0), ray, ray)).toBeNull();
  });

  it("won't throw a prop toward the horizon when dragged over the ground looking almost flat across it", () => {
    const low: Orbit = { target: v3(0, 0, 0), yaw: 0, pitch: 0.4, distance: 10 };
    const start = screenRay(low, size, project(viewProjection(low, size), size, v3(0, 0, 0))!);
    // A ray only a degree below the horizon meets the ground hundreds of units away.
    const grazing = { origin: start.origin, dir: { x: 0, y: -Math.sin(Math.PI / 180), z: -Math.cos(Math.PI / 180) } };
    expect(dragDelta("xz", v3(0, 0, 0), start, grazing)).toBeNull();
    const steep = { origin: start.origin, dir: { x: 0, y: -Math.sin(0.3), z: -Math.cos(0.3) } };
    expect(dragDelta("xz", v3(0, 0, 0), start, steep)).not.toBeNull();
  });

  it("drags props over the ground from above, and up the house from street level", () => {
    expect(freeDragHandle({ ...o, pitch: 0.8 })).toBe("xz");
    expect(freeDragHandle({ ...o, pitch: 0.1 })).toBe("xy");
  });
});

describe("gestures in depth", () => {
  it("moves a prop toward the street as one change to its position", () => {
    const t: Transform = { position: { x: 1, y: 2, z: 3 }, rotationDeg: { x: 0, y: 0, z: 0 }, scale: { x: 1, y: 1, z: 1 } };
    expect(gestureTransform(moveGesture3(v3(0.5, 0, -1.25)), t).position).toEqual({ x: 1.5, y: 2, z: 1.75 });
    expect(isNoop(moveGesture3(v3(0, 0, 0.1)))).toBe(false);
    expect(isNoop(moveGesture3(v3(0, 0, 0)))).toBe(true);
  });

  it("draws pixels where gestures still on their way will put them", () => {
    const props = [prop("a", [1, 0, 2]), prop("b", [0, 0, 0])];
    const out = composeGestures3d(props, [
      { ids: ["a"], gesture: moveGesture3(v3(1, 1, -1)) },
      { ids: ["a"], gesture: { kind: "rotate", cx: 0, cy: 0, deg: 90 } },
    ]);
    expect(Array.from(out[0].xyz).map((n) => Math.round(n * 1000) / 1000)).toEqual([-1, 2, 1]);
    expect(out[1]).toBe(props[1]);
    expect(composeGestures3d(props, [])).toBe(props);
  });

  it("adds up moves, with depth, in one pass over the pixels", () => {
    const props = [prop("a", [1, 0, 2, 3, 4, 5])];
    const out = composeGestures3d(props, [
      { ids: ["a"], gesture: moveGesture3(v3(1, 1, -1)) },
      { ids: ["a"], gesture: { kind: "move", dx: 0.5, dy: -2 } },
    ]);
    expect(Array.from(out[0].xyz)).toEqual([2.5, -1, 1, 4.5, 3, 4]);
  });
});

describe("pixel colors", () => {
  const picked = [200, 100, 250] as const;

  it("lights each pixel from the live frame, RGB or RGBW, and leaves missing ones off", () => {
    const props = [prop("rgb", [0, 0, 0, 1, 0, 0], 0, 3), prop("rgbw", [0, 0, 0, 1, 0, 0], 6, 4)];
    const frame = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 99, 10, 11]);
    const out = new Uint8Array(12).fill(77);
    fillColors(props, frame, new Set(), picked, out);
    expect(Array.from(out)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 0, 0]);
    // Playing, a selected prop's pixels show their own colors too.
    fillColors(props, frame, new Set(["rgb"]), picked, out);
    expect(Array.from(out.subarray(0, 6))).toEqual([1, 2, 3, 4, 5, 6]);
  });

  it("gives a selected prop's pixels the selection color when nothing plays, and the others none of their own", () => {
    const props = [prop("a", [0, 0, 0]), prop("b", [0, 0, 0])];
    const out = new Uint8Array(6).fill(77);
    fillColors(props, null, new Set(["b"]), picked, out);
    expect(Array.from(out)).toEqual([0, 0, 0, 200, 100, 250]);
  });

  it("packs every prop's pixels into one array for one draw", () => {
    const { xyz, starts } = packPositions([prop("a", [1, 2, 3]), prop("b", [4, 5, 6, 7, 8, 9])]);
    expect(Array.from(xyz)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9]);
    expect(starts.get("b")).toEqual({ start: 1, count: 2 });
  });

  it("marks the pixels of DMX floods, in the same order", () => {
    const flood = { ...prop("b", [4, 5, 6, 7, 8, 9]), flood: true };
    expect(Array.from(packFloods([prop("a", [1, 2, 3]), flood, prop("c", [0, 0, 0])]))).toEqual([0, 1, 1, 0]);
  });
});

describe("how every pixel's prop is spread out, for the renderer", () => {
  /** A matrix `columns` by `rows`, `step` apart both ways, facing the front at depth `z`. */
  const matrix = (columns: number, rows: number, step: number, z = 0) => Array.from({ length: columns * rows }, (_, i) => [(i % columns) * step, Math.floor(i / columns) * step, z]).flat();
  const perPixel = (packed: Float32Array) => Array.from({ length: packed.length / SPREAD_SIZE }, (_, i) => Array.from(packed.subarray(i * SPREAD_SIZE, (i + 1) * SPREAD_SIZE)).map((v) => Math.round(v * 1e4) / 1e4));

  it("is each prop's own spacing, room to the side and whether it's all round, for each of its pixels, with where the pixel is on the wire", () => {
    const string = prop("string", [0, 0, 3, 0.5, 0, 3, 1, 0, 3, 1.5, 0, 3]);
    const grid = prop("grid", matrix(4, 4, 0.2));
    const packed = perPixel(packSpread([string, grid, prop("one", [5, 5, 5])]));
    expect(packed).toHaveLength(21);
    // A string: its step; nothing to its side, as far as is looked (`REACH`); first, middle, middle, last.
    expect(packed.slice(0, 4)).toEqual([
      [0.5, REACH, 0, 1],
      [0.5, REACH, 0, 0],
      [0.5, REACH, 0, 0],
      [0.5, REACH, 0, 2],
    ]);
    // A matrix: its own, closer, spacing, the same to the side, and neighbors on every side.
    expect(packed[4]).toEqual([0.2, 0.2, 1, 1]);
    expect(packed[10]).toEqual([0.2, 0.2, 1, 0]);
    expect(packed[19]).toEqual([0.2, 0.2, 1, 2]);
    // A prop of one pixel has no neighbors to shrink for: it's its wire's first and last.
    expect(packed[20]).toEqual([1e6, REACH, 0, 3]);
  });

  it("is measured where the pixels stand, in depth too", () => {
    // A string running straight back from the camera: every pixel at one place, seen from the front.
    const back = prop("back", Array.from({ length: 20 }, (_, i) => [2, 1, -i * 0.3]).flat());
    expect(perPixel(packSpread([back]))[5]).toEqual([0.3, REACH, 0, 0]);
    // A matrix lying flat on the ground is as much a matrix as one on a wall.
    const lying = prop("lying", matrix(6, 6, 0.25).flatMap((v, i, all) => (i % 3 === 0 ? [v, 0, all[i + 1]] : [])));
    expect(perPixel(packSpread([lying]))[7]).toEqual([0.25, 0.25, 1, 0]);
  });

  it("measures again only the props whose pixels have moved since it was last packed", () => {
    const [string, grid] = [prop("string", [0, 0, 3, 0.5, 0, 3, 1, 0, 3]), prop("grid", matrix(4, 4, 0.2))];
    packSpread([string, grid]);
    const before = new Map([string, grid].map((p) => [p.prop, p.xyz]));
    // The layout as it comes after the string was stretched: every prop's pixels in a new array.
    const [stretched, same] = [prop("string", [0, 0, 3, 1, 0, 3, 2, 0, 3]), prop("grid", Array.from(grid.xyz))];
    const packed = perPixel(packSpread([stretched, same], before));
    expect(packed[0]).toEqual([1, REACH, 0, 1]);
    expect(packed[3]).toEqual([0.2, 0.2, 1, 1]);
    expect(pixelSpread3d(same.xyz)).toBe(pixelSpread3d(grid.xyz));
    expect(pixelSpread3d(stretched.xyz)).not.toBe(pixelSpread3d(string.xyz));
  });

  it("is the flat previews' measure for a prop that faces the front", () => {
    const flat = matrix(12, 5, 0.03).map((v, i) => (i % 3 === 1 ? v * 4 : v));
    const front = flat.filter((_, i) => i % 3 !== 2);
    const [spacing, room, round] = perPixel(packSpread([prop("m", flat)]))[0];
    expect([spacing, room, round]).toEqual([pixelSpacing(front), pixelRoom(front), pixelsAllRound(front) ? 1 : 0].map((v) => Math.round(v * 1e4) / 1e4));
    expect([spacing, room, round]).toEqual([0.03, 0.12, 0]);
  });
});

describe("how big a light is drawn", () => {
  const radius = [1.2, 4] as const;
  /** For pixels with as much room to the side as along the wire (a matrix): the dot is fitted to their spacing alone. */
  const fitted = (pitch: number, ratio: number) => lightWidth(1, 1, pitch, ratio, radius);

  it("turns a distance from the camera into device pixels to a layout unit, as the camera does", () => {
    const view = { width: 900, height: 600 };
    const front: Orbit = { target: v3(0, 0, 0), yaw: 0, pitch: 0, distance: 12 };
    const m = viewProjection(front, view);
    const [a, b] = [project(m, view, v3(0, 0, 0))!, project(m, view, v3(1, 0, 0))!];
    expect(b.x - a.x).toBeCloseTo(pixelsPerUnit(view.height, 1) / 12, 6);
    // Twice as far, half as many; twice the device pixels to a screen pixel, twice as many.
    expect(pixelsPerUnit(view.height, 2)).toBeCloseTo(2 * pixelsPerUnit(view.height, 1));
  });

  it("is the view's dot where a prop's pixels have room: a tenth of a layout unit across, bigger nearer the camera, within the smallest and largest", () => {
    const wide = (scale: number, ratio = 1) => Math.round(lightWidth(10, REACH, scale, ratio, radius) * 1e6) / 1e6;
    expect([5, 24, 30, 50, 79, 80, 400].map((scale) => wide(scale))).toEqual([2.4, 2.4, 3, 5, 7.9, 8, 8]);
    // The same on a screen with two device pixels to a screen pixel: twice as many of them.
    expect([10, 60, 100, 800].map((scale) => wide(scale, 2))).toEqual([4.8, 6, 10, 16]);
    // The Layout screen's dots are a little bigger.
    expect(lightWidth(10, REACH, 400, 1, [1.3, 4.5])).toBe(9);
  });

  it("stops short of the next pixel, so neighbors never touch however near the camera is", () => {
    for (const ratio of [1, 2]) {
      // From where the smallest dot there is fits.
      for (let pitch = Math.max(2 * ratio + DOT_GAP, (2 * ratio) / FILL); pitch < 80; pitch += 0.37) {
        const width = fitted(pitch, ratio);
        expect(width + DOT_GAP).toBeLessThanOrEqual(pitch + 1e-9);
        expect(width).toBeLessThanOrEqual(pitch * FILL + 1e-9);
      }
    }
    // The flat previews' dot for the same pixels is within a device pixel of it.
    for (const ratio of [1, 2]) {
      for (let pitch = 3 * ratio; pitch < 60; pitch++) {
        const [smallest, largest] = [2 * ratio, 8 * ratio];
        expect(Math.abs(lightWidth(1, 1, pitch, ratio, [4, 4]) - dotSize(pitch, smallest, largest))).toBeLessThanOrEqual(1);
      }
    }
  });

  it("keeps a string a bold line however close its pixels are: most of the view's dot wide, never thinner", () => {
    // The view's dot is 8 across here; pixels 30, 10, 4, 2 and a tenth of a device pixel apart.
    const widths = [0.3, 0.1, 0.04, 0.02, 0.001].map((spacing) => lightWidth(spacing, REACH, 100, 1, radius));
    expect(widths.map((w) => Math.round(w * 10) / 10)).toEqual([8, 7, 5.6, 5.6, 5.6]);
    expect(widths[2]).toBeCloseTo(8 * FILL);
    // Far off, where the view's dot is the smallest there is, the line is that.
    expect(lightWidth(0.02, REACH, 10, 1, radius)).toBe(2);
    expect(lightWidth(0.02, REACH, 10, 2, radius)).toBe(4);
  });

  it("draws a matrix's rows as thick as the room between them allows, and no thicker", () => {
    // Pixels 2 device pixels apart along a row, rows 7 apart: bars that leave the rows clear of each other.
    const bar = lightWidth(0.02, 0.07, 100, 1, radius);
    expect(bar).toBeCloseTo(7 * FILL);
    expect(bar + DOT_GAP).toBeLessThanOrEqual(7);
    // As close both ways, it's the smallest dot: the picture has no gaps.
    expect(lightWidth(0.02, 0.02, 100, 1, radius)).toBe(2);
  });

  it("never jumps as the camera moves: a little nearer is a little bigger", () => {
    for (const [spacing, room] of [
      [0.02, REACH],
      [0.07, 0.07],
      [0.3, REACH],
      [0.02, 0.07],
    ]) {
      let last = lightWidth(spacing, room, 2, 2, radius);
      for (let scale = 2; scale < 2000; scale *= 1.002) {
        const width = lightWidth(spacing, room, scale, 2, radius);
        expect(width).toBeGreaterThanOrEqual(last - 1e-9);
        expect(width - last).toBeLessThan(0.05);
        last = width;
      }
    }
  });

  it("sits on whole device pixels once the camera is still, when it's small, as the flat previews' small dots do", () => {
    // Two across: the two device pixels its middle is nearest to the join of.
    expect(stillDot(10.7, 2.3)).toEqual({ at: 11, width: 2 });
    expect(stillDot(10.2, 2.3)).toEqual({ at: 10, width: 2 });
    // Three across (from two and a half up): centered on a device pixel.
    expect(stillDot(10.7, 2.5)).toEqual({ at: 10.5, width: 3 });
    expect(stillDot(11.2, 3.4)).toEqual({ at: 11.5, width: 3 });
    // The same device pixels as a dot stamped flat: its left edge rounded to a whole one.
    for (const [x, size] of [
      [10.7, 2],
      [33.49, 3],
      [7.5, 3],
      [20.01, 2],
    ]) {
      expect(stillDot(x, size).at - size / 2).toBe(Math.round(x - size / 2));
    }
    // A wider one keeps its place: its soft edge falls where it falls.
    expect(stillDot(10.7, FINE_FROM)).toEqual({ at: 10.7, width: FINE_FROM });
    expect(stillDot(10.7, 6.3)).toEqual({ at: 10.7, width: 6.3 });
  });
});

describe("the color of a light among pixels closer together than a screen pixel", () => {
  it("is exactly its own while its prop's pixels are a screen pixel apart or more", () => {
    // A string with pixels 0.02 apart: 2, 1.2 and exactly 1 screen pixel.
    expect([100, 60, 50].map((scale) => neighborShare(0.02, scale, 1))).toEqual([0, 0, 0]);
    // Two device pixels to a screen pixel: it's the screen pixel that counts.
    expect(neighborShare(0.02, 100, 2)).toBe(0);
    expect(neighborShare(0.02, 50, 2)).toBeGreaterThan(0);
  });

  it("takes on more of the two wired next to it the closer they are, up to a third from each", () => {
    // Half a screen pixel apart: a sixth from each. On one spot: a third each, the three alike.
    expect(neighborShare(0.02, 25, 1)).toBeCloseTo(1 / 6);
    expect(neighborShare(0.02, 50, 2)).toBeCloseTo(1 / 6);
    expect(neighborShare(0, 25, 1)).toBeCloseTo(1 / 3);
    // Little by little as the camera backs off: nothing changes all at once.
    let last = 0;
    for (let scale = 60; scale > 1; scale *= 0.99) {
      const share = neighborShare(0.02, scale, 1);
      expect(share).toBeGreaterThanOrEqual(last);
      expect(share - last).toBeLessThan(0.005);
      last = share;
    }
    expect(last).toBeLessThanOrEqual(1 / 3);
  });
});

describe("which light a place on the screen shows", () => {
  const [near, far] = [0.05, 1000];
  const lit = { colored: true };
  const off = { colored: false };
  /** A dot's solid part, so far from the camera. */
  const solid = (light: { colored: boolean; beside?: boolean }, from: number) => dotOrder(light, 1, from, near, far);
  const edge = (light: { colored: boolean; beside?: boolean }, cover: number, from = 10) => dotOrder(light, cover, from, near, far);

  it("is a lit light before one that's off, wherever they stand: a light that's off never covers a lit one", () => {
    // Right against the camera and off, or at the far end of the view and lit.
    expect(solid(lit, 999)).toBeLessThan(solid(off, 0.06));
    expect(solid(lit, 999)).toBeLessThan(edge(off, 0.99, 0.06));
    // Even a lit light's thinnest edge comes before a solid light that's off.
    expect(edge(lit, 0.01, 999)).toBeLessThan(solid(off, 0.06));
    // Nothing playing: a selected prop's lights are the colored ones, the rest quiet.
    expect(Math.max(solid(lit, far), edge(lit, 0.001))).toBeLessThan(Math.min(solid(off, near), edge(off, 0.999)));
  });

  it("is something solid before any soft edge of its kind, however near the edge's dot is", () => {
    for (const light of [lit, off]) {
      // A string seen end on: the edge of a dot right by the camera, over the solid middle of one far behind it.
      expect(solid(light, 900)).toBeLessThan(edge(light, 0.99, 0.06));
      expect(solid(light, far)).toBeLessThan(edge(light, 0.999, near));
    }
  });

  it("is the nearest of the solid parts, told apart as finely near and far", () => {
    expect(solid(lit, 5)).toBeLessThan(solid(lit, 5.01));
    expect(solid(off, 400)).toBeLessThan(solid(off, 401));
    // A thousandth nearer is as much sooner at any distance, and more than a step of a 24-bit depth buffer.
    const sooner = [1, 30, 800].map((d) => solid(lit, d) - solid(lit, d * 0.999));
    for (const step of sooner) expect(step).toBeCloseTo(sooner[0], 10);
    expect(sooner[0]).toBeGreaterThan(2 ** -24);
    // Nothing is nearer than the nearest thing drawn, or past its kind's share of the depths.
    expect(solid(lit, 0.001)).toBe(0);
    expect(solid(lit, 1e9)).toBeLessThan(edge(lit, 1 - 1e-9));
  });

  it("is, of two dots as far away, the one whose middle the place is nearer: they meet half way between them", () => {
    // Dots a tenth of a unit across, 20 units off. A place 0.3 of a radius out from one's middle is 0.5 out from the other's.
    const at = (out: number) => solid(lit, coneFrom(20, 0.1, out));
    expect(at(0.3)).toBeLessThan(at(0.5));
    expect(at(0.4)).toBe(at(0.4));
    // The cone's tip is the dot's depth nearer than where the light is; by a round dot's rim, half of it is left.
    expect(coneFrom(20, 0.1, 0)).toBeCloseTo(19.9);
    expect(coneFrom(20, 0.1, 1)).toBeCloseTo(19.95);
    // A square dot's far corners (1.41 of its half-width out) are where the light is, never behind it.
    expect(coneFrom(20, 0.1, Math.SQRT2)).toBeCloseTo(20);
  });

  it("is the fullest of the soft edges", () => {
    expect(edge(lit, 0.8)).toBeLessThan(edge(lit, 0.3));
    expect(edge(off, 0.8, 500)).toBeLessThan(edge(off, 0.3, 1));
  });

  it("lets a light that's off keep its half of the way to a lit neighbor on its wire, and no more", () => {
    const beside = { colored: false, beside: true };
    // Its solid part meets the lit light's as an equal: the nearer middle has the place.
    expect(solid(beside, coneFrom(20, 0.1, 0.3))).toBeLessThan(solid(lit, coneFrom(20, 0.1, 0.5)));
    expect(solid(lit, coneFrom(20, 0.1, 0.3))).toBeLessThan(solid(beside, coneFrom(20, 0.1, 0.5)));
    // Its edge is a quiet light's still: it never covers anything solid.
    expect(edge(beside, 0.9)).toBe(edge(off, 0.9));
    expect(solid(off, 999)).toBeLessThan(edge(beside, 0.99));
  });
});

describe("a lit light's glow in 3D", () => {
  const radius = [1.2, 4] as const;

  it("is the flat previews' halo: sized from the dot a pixel has in the layout, as wide and as strong as the level says", () => {
    // A layout unit 40 screen pixels: a dot of radius 2. At half glow the bell is 1.8 of that, and 0.35 strong.
    expect(lightHalo(0.5, 40, 1, radius).bell).toBeCloseTo(2 * 1.8);
    expect(lightHalo(0.5, 40, 1, radius).strength).toBeCloseTo(0.35);
    expect(lightHalo(1, 40, 1, radius).bell).toBeCloseTo(2 * 2.4);
    expect(lightHalo(1, 40, 1, radius).strength).toBeCloseTo(0.7);
    // Twice the device pixels, twice the bell. Close up, it's sized from the view's largest dot.
    expect(lightHalo(0.5, 80, 2, radius).bell).toBeCloseTo(7.2);
    expect(lightHalo(0.5, 4000, 1, radius).bell).toBeCloseTo(4 * 1.8);
    // No glow, no halo; a level between the slider's steps is drawn at the nearest.
    expect(lightHalo(0, 40, 1, radius).strength).toBe(0);
    expect(lightHalo(0.52, 40, 1, radius)).toEqual(lightHalo(0.5, 40, 1, radius));
  });

  it("fades in as the camera nears, from a bell too narrow to see", () => {
    const strengths = [5, 8, 10, 12, 14, 16, 18, 40].map((scale) => lightHalo(0.5, scale, 1, radius).strength);
    // The bell is 0.09 of the scale here: none under three quarters of a device pixel, all of it from one and a half.
    expect(strengths[0]).toBe(0);
    expect(strengths[1]).toBe(0);
    expect(strengths.at(-1)).toBeCloseTo(0.35);
    expect(lightHalo(0.5, HALO_FROM[1] / 0.09 + 0.01, 1, radius).strength).toBeCloseTo(0.35);
    for (let i = 1; i < strengths.length; i++) expect(strengths[i]).toBeGreaterThanOrEqual(strengths[i - 1]);
    expect(strengths[3]).toBeGreaterThan(0);
    expect(strengths[3]).toBeLessThan(0.35);
  });

  it("dims the dot by what the halo adds back, as the flat previews do", () => {
    for (const [glow, scale, width] of [
      [0.5, 40, 4],
      [1, 40, 2.8],
      [0.25, 60, 6],
      [0.5, 300, 5.6],
    ]) {
      const around = Math.min(radius[1], 0.05 * scale);
      expect(lightDim(lightHalo(glow, scale, 1, radius), width)).toBeCloseTo(coreDim(glowStep(glow), (2 * around) / width), 12);
    }
    // No halo, no dimming.
    expect(lightDim(lightHalo(0, 40, 1, radius), 4)).toBe(1);
    expect(lightDim(lightHalo(0.5, 5, 1, radius), 2.4)).toBe(1);
  });
});
