import { PerspectiveCamera, Vector3 } from "three";
import { describe, expect, it } from "vitest";
import type { PreviewProp3d, Transform } from "../api/types";
import {
  type Box3,
  type Orbit,
  EYE_HEIGHT,
  FOV_DEG,
  MAX_PITCH,
  MIN_PITCH,
  backdropBox,
  boundsOfXyz,
  clipRange,
  composeGestures3d,
  dollyAt,
  dragDelta,
  fillColors,
  fitOrbit,
  focusOrbit,
  freeDragHandle,
  gizmoHit,
  gizmoLength,
  moveGesture3,
  orbitBy,
  orbitEye,
  packPositions,
  panBy3,
  parseOrbit,
  pickPixel,
  pickProp,
  planeSquare,
  presetOrbit,
  project,
  propsInRect,
  screenRay,
  stepOrbit,
  typicalSpacing,
  unionBox3,
  v3,
  viewProjection,
} from "./layout3d";
import { gestureTransform, isNoop } from "./layoutMath";

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
});

describe("bulb size", () => {
  it("follows how far apart neighboring pixels usually are", () => {
    const line = prop("a", [0, 0, 0, 0.1, 0, 0, 0.2, 0, 0, 0.3, 0, 0, 5, 0, 0]);
    expect(typicalSpacing([line])).toBeCloseTo(0.1);
    expect(typicalSpacing([prop("b", [1, 1, 1])])).toBeNull();
  });
});

describe("pixel colors", () => {
  const palette = { unlit: [10, 20, 30] as const, selected: [200, 100, 250] as const };

  it("lights each pixel from the live frame, RGB or RGBW, and leaves missing ones off", () => {
    const props = [prop("rgb", [0, 0, 0, 1, 0, 0], 0, 3), prop("rgbw", [0, 0, 0, 1, 0, 0], 6, 4)];
    const frame = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 99, 10, 11]);
    const out = new Uint8Array(12).fill(77);
    fillColors(props, frame, new Set(), palette, out);
    expect(Array.from(out)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 0, 0]);
  });

  it("shows bulbs unlit, or in the selection color, when nothing plays", () => {
    const props = [prop("a", [0, 0, 0]), prop("b", [0, 0, 0])];
    const out = new Uint8Array(6);
    fillColors(props, null, new Set(["b"]), palette, out);
    expect(Array.from(out)).toEqual([10, 20, 30, 200, 100, 250]);
  });

  it("packs every prop's pixels into one array for one draw", () => {
    const { xyz, starts } = packPositions([prop("a", [1, 2, 3]), prop("b", [4, 5, 6, 7, 8, 9])]);
    expect(Array.from(xyz)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9]);
    expect(starts.get("b")).toEqual({ start: 1, count: 2 });
  });
});
