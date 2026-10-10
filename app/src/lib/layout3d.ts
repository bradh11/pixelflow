// Pure math for the 3D layout view: the orbiting camera (presets, fit, damping, panning and
// zooming toward the pointer), projecting pixels to the screen and picking them, the move
// gizmo and dragging along its axes and planes, pixel colors, and gestures still on their way.
// Nothing here touches the DOM or WebGL, so it is all unit-tested; the renderer draws what this
// works out.
//
// World units are PixelFlow layout units: x to the right, y up, z toward the street (the front
// of the house faces +z). Screen units are CSS pixels from the view's top-left corner, y down.

import type { Background, PreviewProp3d } from "../api/types";
import { applyTransform } from "./geometry";
import { type Gesture, type Pt, type Size, backgroundBox, gesturePoint, tidy } from "./layoutMath";

export interface V3 {
  x: number;
  y: number;
  z: number;
}

export interface Box3 {
  min: V3;
  max: V3;
}

/**
 * The camera, circling `target`: `yaw` turns it around the vertical axis (0 looks at the front
 * of the house from the street, positive swings it to the right), `pitch` raises it (positive
 * looks down), and `distance` is how far it is from the target. Angles are in radians.
 */
export interface Orbit {
  target: V3;
  yaw: number;
  pitch: number;
  distance: number;
}

/** Column-major 4×4 matrix (like three.js's `Matrix4.elements`). */
export type Mat4 = Float64Array;

export const FOV_DEG = 45;
export const MIN_DISTANCE = 0.3;
export const MAX_DISTANCE = 5000;
export const MAX_PITCH = (89 * Math.PI) / 180;
export const MIN_PITCH = (-80 * Math.PI) / 180;
/** Eye height for the street view (layout units: about a person's eye height in meters). */
export const EYE_HEIGHT = 1.7;
/** The angle a new 3D view starts at: a little to the right and above, so depth shows. */
export const DEFAULT_YAW = 0.35;
export const DEFAULT_PITCH = 0.18;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const rad = (deg: number) => (deg * Math.PI) / 180;

/**
 * How strongly the 3D view's bloom spreads the lights' glow, for a glow level (0–1): none at 0,
 * and 0.9 at half way (the level a saved "Glow on" reads as: see state/view3d.ts).
 */
export const bloomStrength = (glow: number) => 1.8 * (Number.isFinite(glow) ? clamp(glow, 0, 1) : 0);

export const v3 = (x: number, y: number, z: number): V3 => ({ x, y, z });
export const add = (a: V3, b: V3): V3 => v3(a.x + b.x, a.y + b.y, a.z + b.z);
export const sub = (a: V3, b: V3): V3 => v3(a.x - b.x, a.y - b.y, a.z - b.z);
export const scale = (a: V3, k: number): V3 => v3(a.x * k, a.y * k, a.z * k);
export const dot = (a: V3, b: V3) => a.x * b.x + a.y * b.y + a.z * b.z;
export const cross = (a: V3, b: V3): V3 => v3(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x);
export const length = (a: V3) => Math.hypot(a.x, a.y, a.z);
export function normalize(a: V3): V3 {
  const l = length(a);
  return l > 0 ? scale(a, 1 / l) : v3(0, 0, 0);
}

// ---- Camera ----------------------------------------------------------------------------

/** Where the camera is. */
export function orbitEye(o: Orbit): V3 {
  const c = Math.cos(o.pitch);
  return add(o.target, scale(v3(Math.sin(o.yaw) * c, Math.sin(o.pitch), Math.cos(o.yaw) * c), o.distance));
}

/** The camera's right, up, and backward directions (backward points from the target to the eye). */
export function orbitBasis(o: Orbit): { right: V3; up: V3; back: V3 } {
  const back = normalize(sub(orbitEye(o), o.target));
  const right = normalize(cross(v3(0, 1, 0), back));
  return { right, up: cross(back, right), back };
}

/** Near and far clipping distances that suit the camera's distance. */
export function clipRange(o: Orbit): { near: number; far: number } {
  return { near: clamp(o.distance * 0.002, 0.01, 1), far: Math.max(1000, o.distance * 50) };
}

/** World to camera space (the camera's inverse world matrix). */
export function viewMatrix(o: Orbit): Mat4 {
  const eye = orbitEye(o);
  const { right: x, up: y, back: z } = orbitBasis(o);
  const m = new Float64Array(16);
  m[0] = x.x;
  m[4] = x.y;
  m[8] = x.z;
  m[12] = -dot(x, eye);
  m[1] = y.x;
  m[5] = y.y;
  m[9] = y.z;
  m[13] = -dot(y, eye);
  m[2] = z.x;
  m[6] = z.y;
  m[10] = z.z;
  m[14] = -dot(z, eye);
  m[15] = 1;
  return m;
}

/** Perspective projection, as three.js's `PerspectiveCamera` builds it. */
export function perspectiveMatrix(fovDeg: number, aspect: number, near: number, far: number): Mat4 {
  const top = near * Math.tan(rad(fovDeg) / 2);
  const m = new Float64Array(16);
  m[0] = near / (top * aspect);
  m[5] = near / top;
  m[10] = -(far + near) / (far - near);
  m[11] = -1;
  m[14] = (-2 * far * near) / (far - near);
  return m;
}

export function multiply(a: Mat4, b: Mat4): Mat4 {
  const m = new Float64Array(16);
  for (let col = 0; col < 4; col++) {
    for (let row = 0; row < 4; row++) {
      let s = 0;
      for (let k = 0; k < 4; k++) s += a[k * 4 + row] * b[col * 4 + k];
      m[col * 4 + row] = s;
    }
  }
  return m;
}

const aspectOf = (size: Size) => (size.height > 0 ? size.width / size.height : 1);

/** World to clip space for the camera on a view of `size`. */
export function viewProjection(o: Orbit, size: Size): Mat4 {
  const { near, far } = clipRange(o);
  return multiply(perspectiveMatrix(FOV_DEG, aspectOf(size), near, far), viewMatrix(o));
}

/** Where a world point appears on screen, and how far in front of the camera it is; null behind it. */
export function project(m: Mat4, size: Size, p: V3): { x: number; y: number; depth: number } | null {
  const w = m[3] * p.x + m[7] * p.y + m[11] * p.z + m[15];
  if (w <= 1e-9) return null;
  const x = (m[0] * p.x + m[4] * p.y + m[8] * p.z + m[12]) / w;
  const y = (m[1] * p.x + m[5] * p.y + m[9] * p.z + m[13]) / w;
  return { x: ((x + 1) / 2) * size.width, y: ((1 - y) / 2) * size.height, depth: w };
}

export interface Ray {
  origin: V3;
  dir: V3;
}

/** The ray from the camera through a screen point. */
export function screenRay(o: Orbit, size: Size, s: Pt): Ray {
  const t = Math.tan(rad(FOV_DEG) / 2);
  const nx = size.width > 0 ? (s.x / size.width) * 2 - 1 : 0;
  const ny = size.height > 0 ? 1 - (s.y / size.height) * 2 : 0;
  const { right, up, back } = orbitBasis(o);
  const dir = normalize(add(add(scale(right, nx * t * aspectOf(size)), scale(up, ny * t)), scale(back, -1)));
  return { origin: orbitEye(o), dir };
}

/** World units across one screen pixel at `p` (for sizing handles and pick tolerances). */
export function worldPerPixel(o: Orbit, size: Size, p: V3): number {
  const { back } = orbitBasis(o);
  const depth = Math.max(1e-6, -dot(sub(p, orbitEye(o)), back));
  return (2 * depth * Math.tan(rad(FOV_DEG) / 2)) / Math.max(1, size.height);
}

/** Turns the camera around its target by a drag of (dx, dy) screen pixels. */
export function orbitBy(o: Orbit, dx: number, dy: number): Orbit {
  return { ...o, yaw: o.yaw - dx * 0.006, pitch: clamp(o.pitch + dy * 0.006, MIN_PITCH, MAX_PITCH) };
}

/** Slides the camera and its target so the scene follows a drag of (dx, dy) screen pixels. */
export function panBy3(o: Orbit, size: Size, dx: number, dy: number): Orbit {
  const k = worldPerPixel(o, size, o.target);
  const { right, up } = orbitBasis(o);
  return { ...o, target: add(o.target, add(scale(right, -dx * k), scale(up, dy * k))) };
}

/**
 * Moves the camera `factor` times as far from its target (below 1: closer), keeping the point
 * under the screen point `s` where it is, so zooming goes toward the pointer.
 */
export function dollyAt(o: Orbit, size: Size, s: Pt | null, factor: number): Orbit {
  const distance = clamp(o.distance * factor, MIN_DISTANCE, MAX_DISTANCE);
  const k = distance / o.distance;
  if (!s) return { ...o, distance };
  // The point under the pointer on the plane through the target, facing the camera.
  const ray = screenRay(o, size, s);
  const { back } = orbitBasis(o);
  const hit = rayPlane(ray, o.target, back);
  if (!hit) return { ...o, distance };
  // Scaling the camera about that point keeps it under the pointer.
  return { ...o, distance, target: add(hit, scale(sub(o.target, hit), k)) };
}

/** Smallest box around every pixel (x, y, z triples); null when there are none. */
export function boundsOfXyz(arrays: ArrayLike<number>[]): Box3 | null {
  let [minX, minY, minZ, maxX, maxY, maxZ] = [Infinity, Infinity, Infinity, -Infinity, -Infinity, -Infinity];
  for (const a of arrays) {
    for (let i = 0; i + 2 < a.length; i += 3) {
      const [x, y, z] = [a[i], a[i + 1], a[i + 2]];
      if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(z)) continue;
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
      if (z < minZ) minZ = z;
      if (z > maxZ) maxZ = z;
    }
  }
  return minX === Infinity ? null : { min: v3(minX, minY, minZ), max: v3(maxX, maxY, maxZ) };
}

export function unionBox3(boxes: (Box3 | null | undefined)[]): Box3 | null {
  const real = boxes.filter((b): b is Box3 => !!b);
  if (real.length === 0) return null;
  return {
    min: v3(Math.min(...real.map((b) => b.min.x)), Math.min(...real.map((b) => b.min.y)), Math.min(...real.map((b) => b.min.z))),
    max: v3(Math.max(...real.map((b) => b.max.x)), Math.max(...real.map((b) => b.max.y)), Math.max(...real.map((b) => b.max.z))),
  };
}

export const boxCenter3 = (b: Box3): V3 => scale(add(b.min, b.max), 0.5);

/** The background photo as a flat box standing at depth `z`. */
export function backdropBox(bg: Background, aspect: number, z = 0): Box3 {
  const b = backgroundBox(bg, aspect);
  return { min: v3(b.minX, b.minY, z), max: v3(b.maxX, b.maxY, z) };
}

/**
 * How far back a camera at angle (yaw, pitch), aimed at the box's center, must be for every
 * corner of the box to be in view, with a small margin. Boxes are taken as at least a unit
 * across, so a single pixel isn't filled in.
 */
function fitDistance(box: Box3, yaw: number, pitch: number, size: Size): number {
  const half = v3(Math.max(0.5, (box.max.x - box.min.x) / 2), Math.max(0.5, (box.max.y - box.min.y) / 2), Math.max(0.5, (box.max.z - box.min.z) / 2));
  const { right, up, back } = orbitBasis({ target: v3(0, 0, 0), yaw, pitch, distance: 1 });
  const tanV = Math.tan(rad(FOV_DEG) / 2);
  const tanH = tanV * aspectOf(size);
  let distance = 0;
  for (const sx of [-1, 1])
    for (const sy of [-1, 1])
      for (const sz of [-1, 1]) {
        const corner = v3(sx * half.x, sy * half.y, sz * half.z);
        // A corner `toward` the camera is closer, so it needs the camera farther back to fit.
        const toward = dot(corner, back);
        const need = Math.max(Math.abs(dot(corner, right)) / tanH, Math.abs(dot(corner, up)) / tanV) + toward;
        distance = Math.max(distance, need);
      }
  return clamp(distance * 1.08, MIN_DISTANCE, MAX_DISTANCE);
}

/** The camera at the same angle, moved so the whole box fits the view. */
export function fitOrbit(box: Box3 | null, size: Size, yaw = DEFAULT_YAW, pitch = DEFAULT_PITCH): Orbit {
  if (!box) return { target: v3(0, 2, 0), yaw, pitch, distance: 25 };
  return { target: boxCenter3(box), yaw, pitch, distance: fitDistance(box, yaw, pitch, size) };
}

export type Preset = "front" | "top" | "left" | "right" | "street";

export const PRESETS: { preset: Preset; label: string; key: string }[] = [
  { preset: "front", label: "Front", key: "1" },
  { preset: "top", label: "Top", key: "2" },
  { preset: "left", label: "Left", key: "3" },
  { preset: "right", label: "Right", key: "4" },
  { preset: "street", label: "Street view", key: "5" },
];

/** The camera for a named view of the box: straight on from a side or above, or from the street at eye height. */
export function presetOrbit(preset: Preset, box: Box3 | null, size: Size): Orbit {
  switch (preset) {
    case "front":
      return fitOrbit(box, size, 0, 0);
    case "top":
      return fitOrbit(box, size, 0, MAX_PITCH);
    case "left":
      return fitOrbit(box, size, -Math.PI / 2, 0);
    case "right":
      return fitOrbit(box, size, Math.PI / 2, 0);
    case "street": {
      // Standing across the street, a little farther back than a fit, eyes at a person's height.
      const fit = fitOrbit(box, size, 0, 0);
      const distance = fit.distance * 1.15;
      const pitch = clamp(Math.asin(clamp((EYE_HEIGHT - fit.target.y) / distance, -1, 1)), MIN_PITCH, MAX_PITCH);
      return { ...fit, distance, pitch };
    }
  }
}

/** The camera at the same angle, aimed at the box and close enough to fill the view with it. */
export function focusOrbit(o: Orbit, box: Box3, size: Size): Orbit {
  return { ...o, target: boxCenter3(box), distance: fitDistance(box, o.yaw, o.pitch, size) };
}

/** The shortest turn from angle `a` to angle `b` (radians). */
function angleDelta(a: number, b: number): number {
  const d = (b - a) % (2 * Math.PI);
  return d > Math.PI ? d - 2 * Math.PI : d < -Math.PI ? d + 2 * Math.PI : d;
}

/**
 * The camera `dt` seconds along its way from `current` to `goal`: it eases in (fast at first,
 * then slower) so moves and presets glide instead of jumping. Returns the goal once it's
 * close enough to stop.
 */
export function stepOrbit(current: Orbit, goal: Orbit, dt: number, rate = 14): Orbit {
  const k = 1 - Math.exp(-rate * Math.max(0, dt));
  const next: Orbit = {
    target: add(current.target, scale(sub(goal.target, current.target), k)),
    yaw: current.yaw + angleDelta(current.yaw, goal.yaw) * k,
    pitch: current.pitch + (goal.pitch - current.pitch) * k,
    // Distance eases on a log scale, so zooming in and out feels the same.
    distance: current.distance * Math.pow(goal.distance / current.distance, k),
  };
  return orbitsClose(next, goal) ? goal : next;
}

/** True when two cameras look the same (within a hair). */
export function orbitsClose(a: Orbit, b: Orbit): boolean {
  const scaleOf = Math.max(a.distance, b.distance);
  return (
    length(sub(a.target, b.target)) < scaleOf * 1e-4 &&
    Math.abs(angleDelta(a.yaw, b.yaw)) < 1e-4 &&
    Math.abs(a.pitch - b.pitch) < 1e-4 &&
    Math.abs(a.distance - b.distance) < scaleOf * 1e-4
  );
}

/** The camera read back from storage, or null when it isn't usable. */
export function parseOrbit(value: unknown): Orbit | null {
  if (typeof value !== "object" || value === null) return null;
  const o = value as Record<string, unknown>;
  const t = o.target as Record<string, unknown> | undefined;
  const nums = [o.yaw, o.pitch, o.distance, t?.x, t?.y, t?.z];
  if (!nums.every((n) => typeof n === "number" && Number.isFinite(n))) return null;
  const [yaw, pitch, distance, x, y, z] = nums as number[];
  if (distance <= 0) return null;
  return { target: v3(x, y, z), yaw, pitch: clamp(pitch, MIN_PITCH, MAX_PITCH), distance: clamp(distance, MIN_DISTANCE, MAX_DISTANCE) };
}

/** Where a house model's placement puts a point of its file: scaled, turned like props are, then moved. */
export function modelPoint(p: V3, placement: { position: V3; rotationDeg: V3; scale: number }): V3 {
  const { position, rotationDeg, scale: s } = placement;
  // Props turn about X, then Y, then Z (layout axes); the model turns the same way.
  return applyTransform(p, { position, rotationDeg, scale: v3(s, s, s) });
}

/** The box around a model's file box once it's turned by `rotationDeg` (not moved or scaled). */
export function rotatedBox(box: Box3, rotationDeg: V3): Box3 {
  const corners: number[] = [];
  for (const x of [box.min.x, box.max.x])
    for (const y of [box.min.y, box.max.y])
      for (const z of [box.min.z, box.max.z]) {
        const c = modelPoint(v3(x, y, z), { position: v3(0, 0, 0), rotationDeg, scale: 1 });
        // `|| 0` keeps -0 (from turning a zero) out.
        corners.push(tidy(c.x) || 0, tidy(c.y) || 0, tidy(c.z) || 0);
      }
  return boundsOfXyz([corners])!;
}

/**
 * Where to put a house model so it stands on the ground under the display: scaled so it's as
 * wide as `target` (the photo, or the props) when there is one, centered on it left to right,
 * and with its front face just behind the props (z = 0). `natural` is the model's box as its
 * file has it, and `rotationDeg` how it's turned, so a model made lying down (Z up) and stood
 * up with a tilt is measured standing.
 */
export function fitModelPlacement(natural: Box3, target: Box3 | null, rotationDeg: V3 = v3(0, 0, 0)): { position: V3; scale: number } {
  const turned = rotatedBox(natural, rotationDeg);
  const width = turned.max.x - turned.min.x;
  const scaled = target && width > 0 ? (target.max.x - target.min.x) / width : 1;
  const s = Number.isFinite(scaled) && scaled > 0 ? scaled : 1;
  const cx = target ? (target.min.x + target.max.x) / 2 : 0;
  const round = (v: number) => tidy(v) || 0;
  return {
    scale: Number(s.toPrecision(4)),
    position: v3(round(cx - ((turned.min.x + turned.max.x) / 2) * s), round(-turned.min.y * s), round(-turned.max.z * s - 0.02)),
  };
}

// ---- Picking ---------------------------------------------------------------------------

/**
 * The pixel under the screen point `s`: of the pixels within `tolerance` screen pixels of it,
 * the one nearest the camera (what you see is what you click). Gives its prop and where it is.
 */
export function pickPixel(props: PreviewProp3d[], m: Mat4, size: Size, s: Pt, tolerance: number): { prop: string; point: V3 } | null {
  let best: { prop: string; point: V3 } | null = null;
  let bestDepth = Infinity;
  const tol2 = tolerance * tolerance;
  const [hw, hh] = [size.width / 2, size.height / 2];
  for (const p of props) {
    const a = p.xyz;
    for (let i = 0; i + 2 < a.length; i += 3) {
      const [x, y, z] = [a[i], a[i + 1], a[i + 2]];
      const w = m[3] * x + m[7] * y + m[11] * z + m[15];
      if (w <= 1e-9 || w >= bestDepth) continue;
      const sx = ((m[0] * x + m[4] * y + m[8] * z + m[12]) / w + 1) * hw;
      const sy = (1 - (m[1] * x + m[5] * y + m[9] * z + m[13]) / w) * hh;
      const dx = sx - s.x;
      const dy = sy - s.y;
      if (dx * dx + dy * dy <= tol2) {
        best = { prop: p.prop, point: v3(x, y, z) };
        bestDepth = w;
      }
    }
  }
  return best;
}

/** The prop whose pixel is under the screen point `s` (see `pickPixel`). */
export function pickProp(props: PreviewProp3d[], m: Mat4, size: Size, s: Pt, tolerance: number): string | null {
  return pickPixel(props, m, size, s, tolerance)?.prop ?? null;
}

/**
 * How far apart neighboring pixels usually are (the median gap between pixels next to each
 * other on the wire), to size the bulbs; null when there are too few pixels to tell.
 */
export function typicalSpacing(props: PreviewProp3d[], samples = 4000): number | null {
  const gaps: number[] = [];
  const total = props.reduce((n, p) => n + Math.max(0, p.xyz.length / 3 - 1), 0);
  const every = Math.max(1, Math.floor(total / samples));
  let k = 0;
  for (const p of props) {
    const a = p.xyz;
    for (let i = 3; i + 2 < a.length; i += 3, k++) {
      if (k % every !== 0) continue;
      const d = Math.hypot(a[i] - a[i - 3], a[i + 1] - a[i - 2], a[i + 2] - a[i - 1]);
      if (d > 1e-6 && Number.isFinite(d)) gaps.push(d);
    }
  }
  if (gaps.length === 0) return null;
  gaps.sort((x, y) => x - y);
  return gaps[Math.floor(gaps.length / 2)];
}

/** Props with any pixel inside the screen rectangle between `a` and `b` (in front of the camera). */
export function propsInRect(props: PreviewProp3d[], m: Mat4, size: Size, a: Pt, b: Pt): string[] {
  const [minX, maxX, minY, maxY] = [Math.min(a.x, b.x), Math.max(a.x, b.x), Math.min(a.y, b.y), Math.max(a.y, b.y)];
  const out: string[] = [];
  for (const p of props) {
    const xyz = p.xyz;
    for (let i = 0; i + 2 < xyz.length; i += 3) {
      const q = project(m, size, v3(xyz[i], xyz[i + 1], xyz[i + 2]));
      if (q && q.x >= minX && q.x <= maxX && q.y >= minY && q.y <= maxY) {
        out.push(p.prop);
        break;
      }
    }
  }
  return out;
}

// ---- Rays, planes, and the move gizmo --------------------------------------------------

/** Where the ray meets the plane through `point` facing `normal`; null if it never does. */
export function rayPlane(ray: Ray, point: V3, normal: V3): V3 | null {
  const denom = dot(ray.dir, normal);
  if (Math.abs(denom) < 1e-9) return null;
  const t = dot(sub(point, ray.origin), normal) / denom;
  return t < 0 ? null : add(ray.origin, scale(ray.dir, t));
}

/** How far along the line through `origin` in direction `axis` (unit) is the point nearest the ray; null when they're parallel. */
export function rayAxisParam(ray: Ray, origin: V3, axis: V3): number | null {
  // Closest points between two lines.
  const w = sub(origin, ray.origin);
  const b = dot(axis, ray.dir);
  const denom = 1 - b * b;
  if (denom < 1e-6) return null;
  return (b * dot(w, ray.dir) - dot(w, axis)) / denom;
}

export type GizmoHandle = "x" | "y" | "z" | "xy" | "xz" | "yz";

export const AXES: Record<"x" | "y" | "z", V3> = { x: v3(1, 0, 0), y: v3(0, 1, 0), z: v3(0, 0, 1) };
const PLANE_NORMALS: Record<"xy" | "xz" | "yz", V3> = { xy: v3(0, 0, 1), xz: v3(0, 1, 0), yz: v3(1, 0, 0) };

/** Gizmo arrows are this many screen pixels long, wherever the selection is. */
export const GIZMO_PX = 90;
/** The plane squares sit this far along their two axes (fraction of an arrow), and are this big. */
export const PLANE_AT = 0.28;
export const PLANE_SIZE = 0.22;

/** The gizmo's arrow length in world units, so it's always the same size on screen. */
export function gizmoLength(o: Orbit, size: Size, origin: V3): number {
  return worldPerPixel(o, size, origin) * GIZMO_PX;
}

/** The four corners of a plane handle's square. */
export function planeSquare(origin: V3, handle: "xy" | "xz" | "yz", len: number): V3[] {
  const [a, b] = [AXES[handle[0] as "x" | "y" | "z"], AXES[handle[1] as "x" | "y" | "z"]];
  const [lo, hi] = [len * PLANE_AT, len * (PLANE_AT + PLANE_SIZE)];
  return [
    add(origin, add(scale(a, lo), scale(b, lo))),
    add(origin, add(scale(a, hi), scale(b, lo))),
    add(origin, add(scale(a, hi), scale(b, hi))),
    add(origin, add(scale(a, lo), scale(b, hi))),
  ];
}

function distanceToSegment(p: Pt, a: Pt, b: Pt): number {
  const [dx, dy] = [b.x - a.x, b.y - a.y];
  const l2 = dx * dx + dy * dy;
  const t = l2 > 0 ? clamp(((p.x - a.x) * dx + (p.y - a.y) * dy) / l2, 0, 1) : 0;
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

function insidePolygon(p: Pt, poly: Pt[]): boolean {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [a, b] = [poly[i], poly[j]];
    if (a.y > p.y !== b.y > p.y && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) inside = !inside;
  }
  return inside;
}

/** The gizmo handle under the screen point `s`, if any: plane squares first, then arrows. */
export function gizmoHit(o: Orbit, size: Size, origin: V3, s: Pt, tolerance = 7): GizmoHandle | null {
  const m = viewProjection(o, size);
  const len = gizmoLength(o, size, origin);
  for (const handle of ["xy", "xz", "yz"] as const) {
    const corners = planeSquare(origin, handle, len).map((c) => project(m, size, c));
    if (corners.every((c) => c !== null) && insidePolygon(s, corners as Pt[])) return handle;
  }
  const from = project(m, size, origin);
  if (!from) return null;
  let best: GizmoHandle | null = null;
  let bestDistance = tolerance;
  for (const axis of ["x", "y", "z"] as const) {
    const to = project(m, size, add(origin, scale(AXES[axis], len)));
    if (!to) continue;
    // An arrow pointing straight at the camera is a dot: too small to grab reliably.
    if (Math.hypot(to.x - from.x, to.y - from.y) < 12) continue;
    const d = distanceToSegment(s, from, to);
    if (d <= bestDistance) {
      best = axis;
      bestDistance = d;
    }
  }
  return best;
}

/** Drags on a plane follow rays at least this steep to it (sine of the angle: about 3°). */
const MIN_PLANE_ANGLE = 0.05;

/**
 * How far a drag on a gizmo handle (or on the ground, "xz") moves the selection, from where it
 * was grabbed (`start`) to where the pointer is now (`now`), both rays from the camera. Moves
 * snap to `grid` when it's given. Null when the drag can't be followed (looking along the axis
 * or edge-on at the plane).
 */
export function dragDelta(handle: GizmoHandle, origin: V3, start: Ray, now: Ray, grid: number | null = null): V3 | null {
  let delta: V3;
  if (handle === "x" || handle === "y" || handle === "z") {
    const axis = AXES[handle];
    const [t0, t1] = [rayAxisParam(start, origin, axis), rayAxisParam(now, origin, axis)];
    if (t0 === null || t1 === null) return null;
    delta = scale(axis, t1 - t0);
  } else {
    const normal = PLANE_NORMALS[handle];
    // A ray almost along the plane meets it very far away: following it would throw the
    // selection toward the horizon, so such a drag waits for a steadier angle.
    if (Math.abs(dot(start.dir, normal)) < MIN_PLANE_ANGLE || Math.abs(dot(now.dir, normal)) < MIN_PLANE_ANGLE) return null;
    const [a, b] = [rayPlane(start, origin, normal), rayPlane(now, origin, normal)];
    if (!a || !b) return null;
    delta = sub(b, a);
  }
  // `|| 0` keeps -0 out of saved positions.
  const snap = (v: number) => tidy(grid && grid > 0 ? Math.round(v / grid) * grid : v) || 0;
  return v3(snap(delta.x), snap(delta.y), snap(delta.z));
}

/** The plane to drag a prop along when it's grabbed by its pixels: the ground, unless looking nearly flat across it. */
export function freeDragHandle(o: Orbit): "xz" | "xy" {
  return Math.abs(o.pitch) > rad(20) ? "xz" : "xy";
}

// ---- Gestures and colors ---------------------------------------------------------------

/** A move gesture in 3D (a 2D move with depth). */
export function moveGesture3(d: V3): Gesture {
  return { kind: "move", dx: d.x, dy: d.y, dz: d.z };
}

/** Where a pixel ends up after a gesture: moves carry depth; turns and resizes happen in the front view. */
export function gesturePoint3(g: Gesture, p: V3): V3 {
  const q = gesturePoint(g, p);
  return v3(q.x, q.y, p.z + (g.kind === "move" ? (g.dz ?? 0) : 0));
}

/** Each prop's pixels moved by every gesture in `layers` that lists it, in order. */
export function composeGestures3d(props: PreviewProp3d[], layers: { ids: string[]; gesture: Gesture }[]): PreviewProp3d[] {
  if (layers.length === 0) return props;
  const sets = layers.map((l) => new Set(l.ids));
  return props.map((p) => {
    const mine = layers.filter((_, i) => sets[i].has(p.prop));
    if (mine.length === 0) return p;
    const a = p.xyz;
    const out = new Float32Array(a.length);
    if (mine.every(({ gesture }) => gesture.kind === "move")) {
      // Only moves (dragging, nudging): one offset for every pixel, with nothing made per pixel.
      let [dx, dy, dz] = [0, 0, 0];
      for (const { gesture: g } of mine) {
        if (g.kind !== "move") continue;
        dx += g.dx;
        dy += g.dy;
        dz += g.dz ?? 0;
      }
      for (let i = 0; i + 2 < a.length; i += 3) {
        out[i] = a[i] + dx;
        out[i + 1] = a[i + 1] + dy;
        out[i + 2] = a[i + 2] + dz;
      }
      return { ...p, xyz: out };
    }
    for (let i = 0; i + 2 < a.length; i += 3) {
      let q = v3(a[i], a[i + 1], a[i + 2]);
      for (const { gesture } of mine) q = gesturePoint3(gesture, q);
      out[i] = q.x;
      out[i + 1] = q.y;
      out[i + 2] = q.z;
    }
    return { ...p, xyz: out };
  });
}

export type Rgb = readonly [number, number, number];

/** Colors for pixels that don't show a lit color of their own. */
export interface PixelPalette {
  /** Nothing playing: a bulb that's off, but visible. */
  unlit: Rgb;
  /** Nothing playing: a selected prop's bulbs. */
  selected: Rgb;
}

/**
 * Fills `out` (RGB bytes, one triple per pixel, props in order) with each pixel's color: from
 * `frame` while something plays (missing channels are off), otherwise unlit or selected.
 */
export function fillColors(props: PreviewProp3d[], frame: Uint8Array | null, selected: ReadonlySet<string>, palette: PixelPalette, out: Uint8Array) {
  let at = 0;
  for (const p of props) {
    const n = Math.floor(p.xyz.length / 3);
    if (!frame) {
      const [r, g, b] = selected.has(p.prop) ? palette.selected : palette.unlit;
      for (let i = 0; i < n && at + 2 < out.length; i++, at += 3) {
        out[at] = r;
        out[at + 1] = g;
        out[at + 2] = b;
      }
      continue;
    }
    const step = p.channelsPerPixel;
    for (let i = 0, o = p.frameOffset; i < n && at + 2 < out.length; i++, at += 3, o += step) {
      if (o + 2 < frame.length) {
        out[at] = frame[o];
        out[at + 1] = frame[o + 1];
        out[at + 2] = frame[o + 2];
      } else {
        out[at] = out[at + 1] = out[at + 2] = 0;
      }
    }
  }
}

/** Every prop's pixels in one array (x, y, z triples, props in order), and where each prop starts in it (pixels). */
export function packPositions(props: PreviewProp3d[]): { xyz: Float32Array; starts: Map<string, { start: number; count: number }> } {
  const total = props.reduce((n, p) => n + Math.floor(p.xyz.length / 3), 0);
  const xyz = new Float32Array(total * 3);
  const starts = new Map<string, { start: number; count: number }>();
  let at = 0;
  for (const p of props) {
    const count = Math.floor(p.xyz.length / 3);
    xyz.set(p.xyz.subarray(0, count * 3), at * 3);
    starts.set(p.prop, { start: at, count });
    at += count;
  }
  return { xyz, starts };
}
