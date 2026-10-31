// Moving heads' beams and servos' arms in the previews. The 2D previews stamp them into their
// picture of dots (`stampPixels` in pixelBatches.ts), the video export the same way from the
// same numbers (crates/pf-video's beam.rs), held to each other by the scenes in
// api/contracts/pixels/; the 3D view draws them from these numbers too
// (components/layout3d/threeScene.ts).
//
// A beam is drawn as the light would look at night: only while something plays, from the color
// the head shows then (its cell's, which is dark while its shutter is shut), as bright as that
// color is (its dimmer), in that color's hue.
//
// A head points as its pan and tilt say (motion.ts). Seen from the front, its beam runs from its
// cell along that direction, `length` layout units out, and widens to `length · tan(angle / 2)`
// either side (the angle its zoom channel sets, if it has one), never narrower than `NARROWEST`
// screen pixels. At a picture pixel `along` of the way out and `r` of the way from the middle line
// to the edge there, it is `(1 - r²)² × beamStrength(half width)` solid: a narrow beam is strong
// and a wide one soft, as a beam's light spreads, so it reads at a small preview and isn't blown
// out at a large one. Past `FADE_FROM` of the way out it fades to nothing at its end, as a beam
// does into the night, except as much as it points at the viewer (`toward²`): a beam pointing at
// you shows its end as a round glow (a pool of light round the head when it's straight at you).
//
// A beam pointing down that reaches the ground (y = 0) before its end stops there, at full, and
// lights a pool on the ground: the cone's footprint, stretched the more it slants, seen from the
// front as a flat ellipse (`POOL_FLAT` as deep as it is wide), `poolStrength(its width)` solid at
// its middle and soft to its edge.
//
// A servo's arm turns from its cell in the fixture's front plane: up at Home, clockwise as seen
// from the street. It is `ARM_LENGTH` layout units long (never shorter than `SHORTEST_ARM`
// screen pixels) and `2 × ARM_HALF_WIDTH` screen pixels wide, in `ARM_COLOR` while playing, the
// unlit color when not.

import type { FixtureLayout, PreviewAxis, PreviewMotion, PreviewZoom, Prop, Transform } from "../api/types";
import { DEFAULT_BEAM, DEFAULT_ZOOM, isHead } from "./dmx";
import { applyTransform } from "./geometry";
import { type Axis, armDirection, axes, beamDirection, cosDeg, motorDegrees, sinDeg } from "./motion";

/** A transform's rotation alone, as the engine's `rotation_basis` gives it: its columns are where the prop's right, up and front turn to. */
export function rotationBasis(t: Transform): number[] {
  const turn = (x: number, y: number, z: number) => applyTransform({ x, y, z }, { position: { x: 0, y: 0, z: 0 }, rotationDeg: t.rotationDeg, scale: { x: 1, y: 1, z: 1 } });
  const [a, b, c] = [turn(1, 0, 0), turn(0, 1, 0), turn(0, 0, 1)];
  return [a.x, b.x, c.x, a.y, b.y, c.y, a.z, b.z, c.z].map(Math.fround);
}

/** How a fixture with motors moves in the previews, as the engine's preview gives it (`PreviewProp::of`); undefined without motors. */
export function previewMotionOf(prop: Prop, fixture: FixtureLayout): PreviewMotion | undefined {
  const dmx = prop.dmx;
  const found = axes(fixture.channels);
  if (!dmx || found.length === 0) return undefined;
  const held = (i: number) => fixture.channels[i]?.value ?? 0;
  const preview = (a: Axis): PreviewAxis => ({
    at: fixture.dmxOffset + a.coarse,
    ...(a.fine !== null ? { fineAt: fixture.dmxOffset + a.fine } : {}),
    rest: [held(a.coarse), a.fine === null ? 0 : held(a.fine)],
    motor: a.motor,
  });
  const ofRole = (role: string) => {
    const a = found.find((x) => x.role === role);
    return a ? preview(a) : undefined;
  };
  const motion: PreviewMotion = { basis: rotationBasis(prop.transform) };
  if (isHead(dmx)) {
    const beam = dmx.beam ?? DEFAULT_BEAM;
    const [pan, tilt] = [ofRole("pan"), ofRole("tilt")];
    const z = fixture.channels.findIndex((c) => c.role === "zoom");
    const zoom: PreviewZoom | undefined =
      z < 0 ? undefined : { at: fixture.dmxOffset + z, rest: held(z), ...(fixture.channels[z].zoom ?? DEFAULT_ZOOM) };
    motion.beam = { ...(pan ? { pan } : {}), ...(tilt ? { tilt } : {}), angleDeg: beam.angleDeg, length: beam.length, ...(zoom ? { zoom } : {}) };
  }
  const servos = found.filter((a) => a.role === "servo").map(preview);
  if (servos.length > 0) motion.servos = servos;
  return motion;
}

/** How solid a beam is at its narrowest. */
export const BEAM_PEAK = 0.9;
/** A beam this wide either side of its middle (screen pixels) is half as solid as at its narrowest. */
export const BEAM_SOFT = 10;
/** How far out (of its drawn length) a beam that lands nowhere starts fading to nothing. */
export const FADE_FROM = 0.7;
/** How solid a pool of light on the ground is at its smallest. */
export const POOL_PEAK = 0.85;
/** A pool this wide either side of its middle (screen pixels) is half as solid as at its smallest. */
export const POOL_SOFT = 24;
/** A pool on the ground, seen from the front, is drawn this much as deep as it is wide. */
export const POOL_FLAT = 0.3;
/** The most a pool is stretched by a beam slanting onto the ground: as if it came down at least this steeply (its direction's downward part). */
export const GRAZING = 0.2;
/** The narrowest a beam is drawn, either side of its middle, in screen pixels. */
export const NARROWEST = 1.5;
/** A servo's arm: its length in layout units, its shortest in screen pixels, and half its width in screen pixels. */
export const ARM_LENGTH = 0.4;
export const SHORTEST_ARM = 10;
export const ARM_HALF_WIDTH = 1;
/** A servo's arm while something plays: red, green, blue. */
export const ARM_COLOR: [number, number, number] = [200, 200, 208];
/** A selected head's aim on the Layout screen, while nothing plays: dashes this long with gaps this long, in screen pixels. */
export const AIM_DASH = 4;
export const AIM_GAP = 3;

/** How solid a beam is on its middle line where it is `half` screen pixels wide either side. */
export function beamStrength(half: number): number {
  return (BEAM_PEAK * BEAM_SOFT) / (BEAM_SOFT + half);
}

/** How solid a pool of light is at its middle when it is `half` screen pixels wide either side. */
export function poolStrength(half: number): number {
  return (POOL_PEAK * POOL_SOFT) / (POOL_SOFT + half);
}

/** How much of a beam's light is left `t` of the way out (1 until `FADE_FROM`, then easing to 0 at its end). */
export function endFade(t: number): number {
  if (t <= FADE_FROM) return 1;
  const u = (t - FADE_FROM) / (1 - FADE_FROM);
  return 1 - u * u * (3 - 2 * u);
}

/** Where an axis points, in degrees from Home, in `frame` (null: at rest). */
export function previewDegrees(axis: PreviewAxis, frame: ArrayLike<number> | null): number {
  const at = (i: number, rest: number) => (frame ? (i < frame.length ? frame[i] : 0) : rest);
  const coarse = at(axis.at, axis.rest[0]);
  const fine = axis.fineAt === undefined ? null : at(axis.fineAt, axis.rest[1]);
  return motorDegrees(axis.motor, coarse, fine);
}

/** How wide a head's beam is, in degrees, with `frame` (null: at rest): its zoom's, or its own. */
export function beamWidth(motion: PreviewMotion, frame: ArrayLike<number> | null): number {
  const beam = motion.beam;
  if (!beam) return DEFAULT_BEAM.angleDeg;
  const zoom = beam.zoom;
  if (!zoom) return beam.angleDeg;
  const value = frame ? (zoom.at < frame.length ? frame[zoom.at] : 0) : zoom.rest;
  return zoom.fromDeg + ((zoom.toDeg - zoom.fromDeg) * value) / 255;
}

/** Which way a head points in the layout with `frame` (null: at rest). */
export function headDirection(motion: PreviewMotion, frame: ArrayLike<number> | null): [number, number, number] {
  const beam = motion.beam;
  const pan = beam?.pan ? previewDegrees(beam.pan, frame) : 0;
  const tilt = beam?.tilt ? previewDegrees(beam.tilt, frame) : 0;
  return beamDirection(pan, tilt, motion.basis);
}

/** Which way servo `k` points in the layout with `frame` (null: at rest). */
export function servoDirection(motion: PreviewMotion, k: number, frame: ArrayLike<number> | null): [number, number, number] {
  return armDirection(previewDegrees(motion.servos![k], frame), motion.basis);
}

/** How wide a beam spreads, either side of its middle, for each unit of its length. */
export function spread(angleDeg: number): number {
  return sinDeg(angleDeg / 2) / cosDeg(angleDeg / 2);
}

/** Where a beam pointing `d` from `headY` (layout) meets the ground, as how far along it (layout units), or null when it doesn't within `length`. */
export function groundReach(headY: number, d: ArrayLike<number>, length: number): number | null {
  if (!(d[1] < 0)) return null;
  const s = (headY > 0 ? headY : 0) / -d[1];
  return s < length ? s : null;
}

/** A pool of light in a picture: its middle and how far it reaches across and down, in picture pixels. */
export interface PoolShape {
  cx: number;
  cy: number;
  rx: number;
  ry: number;
}

/**
 * A beam in a picture: where it starts, how far and which way it runs, how wide its end is, how
 * narrow it may be, picture pixels to a screen pixel, how much it points at the viewer (squared),
 * and its pool on the ground when it lands on it, in picture pixels.
 */
export interface BeamShape {
  ax: number;
  ay: number;
  ex: number;
  ey: number;
  w: number;
  narrow: number;
  ratio: number;
  toward: number;
  pool: PoolShape | null;
}

/**
 * A beam from the picture point (`ax`, `ay`), a head `headY` above the ground (layout), pointing
 * `d` (layout), `length` long and `angleDeg` wide, at a zoom of `zoom` screen pixels to a layout
 * unit and `ratio` picture pixels to a screen pixel.
 */
export function beamShape(ax: number, ay: number, headY: number, d: ArrayLike<number>, length: number, angleDeg: number, zoom: number, ratio: number): BeamShape {
  const scale = zoom * ratio;
  const narrow = NARROWEST * ratio;
  const wide = spread(angleDeg);
  const toward = d[2] > 0 ? d[2] : 0;
  const landed = groundReach(headY, d, length);
  const reach = landed === null ? length : landed;
  const out = reach * scale;
  const ex = d[0] * out;
  const ey = -d[1] * out;
  let pool: PoolShape | null = null;
  if (landed !== null) {
    // The cone's footprint: as wide as the cone where it lands, stretched along its slant.
    const radius = reach * wide;
    const stretch = radius / Math.max(-d[1], GRAZING);
    const h = Math.sqrt(d[0] * d[0] + d[2] * d[2]);
    const hx = h > 0 ? d[0] / h : 1;
    const hz = h > 0 ? d[2] / h : 0;
    const [sx, sz, rx, rz] = [stretch * hx, stretch * hz, radius * hx, radius * hz];
    const across = Math.sqrt(sx * sx + rz * rz);
    const deep = Math.sqrt(sz * sz + rx * rx);
    pool = { cx: ax + ex, cy: ay + ey, rx: Math.max(across * scale, narrow), ry: Math.max(deep * scale * POOL_FLAT, ratio) };
  }
  return { ax, ay, ex, ey, w: Math.max(out * wide, narrow), narrow, ratio, toward: toward * toward, pool };
}

/** The picture pixels a beam can touch: columns x0..x1, rows y0..y1, in a picture `width` by `height`. */
export function beamBox(b: BeamShape, width: number, height: number): [number, number, number, number] {
  const reach = b.w + 1;
  let left = Math.min(b.ax, b.ax + b.ex) - reach;
  let right = Math.max(b.ax, b.ax + b.ex) + reach;
  let top = Math.min(b.ay, b.ay + b.ey) - reach;
  let bottom = Math.max(b.ay, b.ay + b.ey) + reach;
  if (b.pool) {
    left = Math.min(left, b.pool.cx - b.pool.rx - 1);
    right = Math.max(right, b.pool.cx + b.pool.rx + 1);
    top = Math.min(top, b.pool.cy - b.pool.ry - 1);
    bottom = Math.max(bottom, b.pool.cy + b.pool.ry + 1);
  }
  return [Math.max(0, Math.floor(left)), Math.min(width, Math.ceil(right)), Math.max(0, Math.floor(top)), Math.min(height, Math.ceil(bottom))];
}

/** How solid a beam (and its pool) is at the middle of the picture pixel (`px`, `py`), as a share of a full light's (0 outside it). */
export function beamAt(b: BeamShape, px: number, py: number): number {
  const qx = px + 0.5 - b.ax;
  const qy = py + 0.5 - b.ay;
  const ee = b.ex * b.ex + b.ey * b.ey;
  const t = ee > 0 ? (qx * b.ex + qy * b.ey) / ee : 1;
  let lit = 0;
  if (t >= 0 && t <= 1) {
    const rx = qx - t * b.ex;
    const ry = qy - t * b.ey;
    const half = Math.max(t * b.w, b.narrow);
    const r2 = (rx * rx + ry * ry) / (half * half);
    if (r2 < 1) {
      const s = 1 - r2;
      const along = b.pool ? 1 : b.toward + (1 - b.toward) * endFade(t);
      lit = s * s * along * beamStrength(half / b.ratio);
    }
  } else if (t > 1 && !b.pool && b.toward > 0) {
    const rx = qx - b.ex;
    const ry = qy - b.ey;
    const r2 = (rx * rx + ry * ry) / (b.w * b.w);
    if (r2 < 1) {
      const s = 1 - r2;
      lit = s * s * b.toward * beamStrength(b.w / b.ratio);
    }
  }
  if (b.pool) {
    const dx = (px + 0.5 - b.pool.cx) / b.pool.rx;
    const dy = (py + 0.5 - b.pool.cy) / b.pool.ry;
    const q = dx * dx + dy * dy;
    if (q < 1) {
      const s = 1 - q;
      const pool = s * s * poolStrength(b.pool.rx / b.ratio);
      lit = lit + pool - lit * pool;
    }
  }
  return lit;
}

/** A light's color as a beam draws it: its hue at full (red, green, blue packed as the pictures hold them) and how bright it is (0–1); null when it's dark. */
export function beamLight(r: number, g: number, b: number): { rgb: number; level: number } | null {
  const most = Math.max(r, g, b);
  if (most === 0) return null;
  const half = most >> 1;
  const full = (c: number) => Math.floor((c * 255 + half) / most);
  return { rgb: (full(b) << 16) | (full(g) << 8) | full(r), level: most / 255 };
}

/** An arm in a picture: its pivot, how far and which way it reaches, and half its width, in picture pixels. */
export interface ArmShape {
  ax: number;
  ay: number;
  ex: number;
  ey: number;
  half: number;
}

/** A servo's arm pivoting at the picture point (`ax`, `ay`), pointing `d` (layout). */
export function armShape(ax: number, ay: number, d: ArrayLike<number>, zoom: number, ratio: number): ArmShape {
  const len = Math.max(ARM_LENGTH * zoom, SHORTEST_ARM) * ratio;
  return { ax, ay, ex: d[0] * len, ey: -d[1] * len, half: ARM_HALF_WIDTH * ratio };
}

/** The picture pixels an arm can touch: columns x0..x1, rows y0..y1. */
export function armBox(a: ArmShape, width: number, height: number): [number, number, number, number] {
  const reach = a.half + 1;
  const x0 = Math.max(0, Math.floor(Math.min(a.ax, a.ax + a.ex) - reach));
  const x1 = Math.min(width, Math.ceil(Math.max(a.ax, a.ax + a.ex) + reach));
  const y0 = Math.max(0, Math.floor(Math.min(a.ay, a.ay + a.ey) - reach));
  const y1 = Math.min(height, Math.ceil(Math.max(a.ay, a.ay + a.ey) + reach));
  return [x0, x1, y0, y1];
}

/** How much of the picture pixel (`px`, `py`) an arm covers, 0 to 1. */
export function armAt(a: ArmShape, px: number, py: number): number {
  const qx = px + 0.5 - a.ax;
  const qy = py + 0.5 - a.ay;
  const ee = a.ex * a.ex + a.ey * a.ey;
  const t = ee > 0 ? Math.min(Math.max((qx * a.ex + qy * a.ey) / ee, 0), 1) : 0;
  const rx = qx - t * a.ex;
  const ry = qy - t * a.ey;
  const cover = a.half + 0.5 - Math.sqrt(rx * rx + ry * ry);
  return cover <= 0 ? 0 : cover >= 1 ? 1 : cover;
}

/**
 * The outline of where a beam points, as dashed lines one screen pixel wide (its two edges and
 * across its end, or round its end when it points at the viewer): segments of a path, each from
 * (x0, y0) to (x1, y1), in picture pixels. Never light: the Layout screen draws it for a selected
 * head while nothing plays.
 */
export function aimOutline(b: BeamShape): [number, number, number, number][] {
  const ex = b.ax + b.ex;
  const ey = b.ay + b.ey;
  const long = Math.sqrt(b.ex * b.ex + b.ey * b.ey);
  if (long < b.w) {
    const out: [number, number, number, number][] = [];
    const sides = 24;
    for (let i = 0; i < sides; i++) {
      const [a0, a1] = [(i * 360) / sides, ((i + 1) * 360) / sides];
      out.push([ex + b.w * cosDeg(a0), ey + b.w * sinDeg(a0), ex + b.w * cosDeg(a1), ey + b.w * sinDeg(a1)]);
    }
    return out;
  }
  const [nx, ny] = [-b.ey / long, b.ex / long];
  return [
    [b.ax, b.ay, ex + nx * b.w, ey + ny * b.w],
    [ex + nx * b.w, ey + ny * b.w, ex - nx * b.w, ey - ny * b.w],
    [ex - nx * b.w, ey - ny * b.w, b.ax, b.ay],
  ];
}

/** How much of the picture pixel (`px`, `py`) a dashed outline covers, 0 to 1 (see `aimOutline`), `ratio` picture pixels to a screen pixel. */
export function aimAt(path: [number, number, number, number][], px: number, py: number, ratio: number): number {
  const qx = px + 0.5;
  const qy = py + 0.5;
  const period = (AIM_DASH + AIM_GAP) * ratio;
  let walked = 0;
  let most = 0;
  for (const [x0, y0, x1, y1] of path) {
    const [dx, dy] = [x1 - x0, y1 - y0];
    const len = Math.sqrt(dx * dx + dy * dy);
    if (len > 0) {
      const t = Math.min(Math.max(((qx - x0) * dx + (qy - y0) * dy) / (len * len), 0), 1);
      const [rx, ry] = [qx - x0 - t * dx, qy - y0 - t * dy];
      const cover = 0.5 * ratio + 0.5 - Math.sqrt(rx * rx + ry * ry);
      if (cover > 0 && (walked + t * len) % period < AIM_DASH * ratio) most = Math.max(most, Math.min(cover, 1));
    }
    walked += len;
  }
  return most;
}
