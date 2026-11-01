// Moving heads' beams and servos' arms in the previews. The 2D previews stamp them into their
// picture of dots (`stampPixels` in pixelBatches.ts), the video export the same way from the
// same numbers (crates/pf-video's beam.rs), held to each other by the scenes in
// api/contracts/pixels/; the 3D view draws the same cone as a volume, by the same numbers
// (components/layout3d/threeScene.ts).
//
// A beam is drawn as its light would look at night in a little haze: only while something plays,
// from the color the head shows then (its cell's, which is dark while its shutter is shut or
// between a strobe's flashes), as bright as that color is (its dimmer), in that color's hue.
// Beams' light adds up where they cross, keeping their hues (`shine` in pixelBatches.ts): light
// too bright to show is drawn as bright as its hue goes, not washed to white.
//
// A head points as its pan and tilt say (motion.ts). Its light reaches `BEAM_REACH` times as far as
// its beam is set to be drawn (its `length`): at full strength that far, then fading gently to
// `FAR` of it at the end of its reach (`alongBeam`), where it ends softly. It widens by
// `tan(angle / 2)` either side for each unit out (the angle its zoom channel sets, if it has one),
// never narrower than `NARROWEST` screen pixels. Seen from the front, at a picture pixel `r` of the
// way from its middle line to its edge, it is `(1 - r²)²` as strong as across its middle: a
// filled cone, brightest along its core and soft to its edges, as a cone of lit haze looks from
// the side. Across its middle it is `beamStrength(half width)`: strong where it is narrow and
// softer where it has spread, so it reads in a small preview and isn't blown out in a large one.
// The more it points toward or away from the viewer, the more of it you look along, and the
// brighter it is (`look`); its far end then shows as a soft disc (`cap`).
//
// A beam pointing down that reaches the ground (y = 0) before the end of its reach stops there
// and lights a pool on the ground: the cone's footprint, stretched the more it slants, seen from
// the front as a flat ellipse (`POOL_FLAT` as deep as it is wide), `poolStrength(its width)` strong
// at its middle (times what is left of the beam's light there) and soft to its edge.
//
// At the lens, while its light is on, a head shows a flare: a bright spot that is larger and
// whiter the brighter the head is, and flashes as its strobe does (`flareShape`).
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

/** A beam's light reaches this many times as far as it is set to be drawn (its `length`). */
export const BEAM_REACH = 4;
/** It is at full strength for this much of its reach (as far as it is set to be drawn), then fades gently. */
export const FULL = 0.25;
/** How much of its light is left at the end of its reach. */
export const FAR = 0.35;
/** A beam that lands nowhere eases out to nothing over the last this much of its reach. */
export const SOFT_END = 0.06;
/** How strong a beam is across its middle at its narrowest. */
export const BEAM_PEAK = 0.8;
/** A beam this wide either side of its middle (screen pixels) is 1/√2 as strong across its middle as at its narrowest; three times this, half. */
export const BEAM_SOFT = 12;
/** Seen along more of its length, a beam is brighter: up to 1 / LEAST_SEEN as bright, pointing at or away from the viewer. */
export const LEAST_SEEN = 0.35;
/** How strong a beam's far end is, seen face on, against the beam itself just before it. */
export const CAP = 1.5;
/** How strong a pool of light on the ground is at its smallest, before what's left of the beam's light there. */
export const POOL_PEAK = 0.9;
/** A pool this wide either side of its middle (screen pixels) is 1/√2 as strong as at its smallest; three times this, half. */
export const POOL_SOFT = 24;
/** A pool on the ground, seen from the front, is drawn this much as deep as it is wide. */
export const POOL_FLAT = 0.3;
/** The most a pool is stretched by a beam slanting onto the ground: as if it came down at least this steeply (its direction's downward part). */
export const GRAZING = 0.2;
/** The narrowest a beam is drawn, either side of its middle, in screen pixels. */
export const NARROWEST = 1.5;
/** A lens flare's radius in layout units at full brightness, never under FLARE_SMALLEST or over FLARE_LARGEST screen pixels; at its dimmest, FLARE_FROM of that. */
export const FLARE_SIZE = 0.4;
export const FLARE_SMALLEST = 5;
export const FLARE_LARGEST = 28;
export const FLARE_FROM = 0.35;
/** How strong a flare is at its middle at full brightness: over 1, so its middle is as bright as its color goes. */
export const FLARE_PEAK = 1.5;
/** How white a flare is at full brightness (0: the light's hue, 1: white). */
export const FLARE_WHITE = 0.7;
/** A servo's arm: its length in layout units, its shortest in screen pixels, and half its width in screen pixels. */
export const ARM_LENGTH = 0.4;
export const SHORTEST_ARM = 10;
export const ARM_HALF_WIDTH = 1;
/** A servo's arm while something plays: red, green, blue. */
export const ARM_COLOR: [number, number, number] = [200, 200, 208];
/** A selected head's aim on the Layout screen, while nothing plays: dashes this long with gaps this long, in screen pixels. */
export const AIM_DASH = 4;
export const AIM_GAP = 3;

/** How strong a beam is across its middle where it is `half` screen pixels wide either side. */
export function beamStrength(half: number): number {
  return BEAM_PEAK * Math.sqrt(BEAM_SOFT / (BEAM_SOFT + half));
}

/** How strong a pool of light is at its middle when it is `half` screen pixels wide either side. */
export function poolStrength(half: number): number {
  return POOL_PEAK * Math.sqrt(POOL_SOFT / (POOL_SOFT + half));
}

/** How much of a beam's light is left `t` of the way along its reach: 1 until `FULL`, then easing down to `FAR` at its end. */
export function alongBeam(t: number): number {
  if (t <= FULL) return 1;
  const u = Math.min((t - FULL) / (1 - FULL), 1);
  return 1 - (1 - FAR) * u * u * (3 - 2 * u);
}

/** How much of a beam that lands nowhere shows `t` of the way along its reach: all of it, easing out to nothing over the last `SOFT_END`. */
export function softEnd(t: number): number {
  if (t <= 1 - SOFT_END) return 1;
  if (t >= 1) return 0;
  const v = (1 - t) / SOFT_END;
  return v * v * (3 - 2 * v);
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

/** A pool of light in a picture: its middle, how far it reaches across and down (picture pixels), and how strong it is at its middle. */
export interface PoolShape {
  cx: number;
  cy: number;
  rx: number;
  ry: number;
  strength: number;
}

/**
 * A beam's far end in a picture, seen from the front: a disc across the beam, an ellipse as wide
 * as the beam there and `depth` deep along its line (`ux`, `uy`, picture pixels), as strong at its
 * middle as `strength`.
 */
export interface CapShape {
  ux: number;
  uy: number;
  depth: number;
  strength: number;
}

/**
 * A beam in a picture: where it starts, how far and which way it runs, how wide its end is, how
 * narrow it may be, picture pixels to a screen pixel, how much of its reach it runs (less than 1
 * where it lands on the ground), how much brighter it is for being seen along its length, and
 * its far end or (where it lands) its pool on the ground, in picture pixels.
 */
export interface BeamShape {
  ax: number;
  ay: number;
  ex: number;
  ey: number;
  w: number;
  narrow: number;
  ratio: number;
  ends: number;
  look: number;
  cap: CapShape | null;
  pool: PoolShape | null;
}

/**
 * A beam from the picture point (`ax`, `ay`), a head `headY` above the ground (layout), pointing
 * `d` (layout), its light reaching `reach` and `angleDeg` wide, at a zoom of `zoom` screen pixels
 * to a layout unit and `ratio` picture pixels to a screen pixel.
 */
export function beamShape(ax: number, ay: number, headY: number, d: ArrayLike<number>, reach: number, angleDeg: number, zoom: number, ratio: number): BeamShape {
  const scale = zoom * ratio;
  const narrow = NARROWEST * ratio;
  const wide = spread(angleDeg);
  const seen = Math.sqrt(d[0] * d[0] + d[1] * d[1]);
  const facing = Math.abs(d[2]);
  const landed = groundReach(headY, d, reach);
  const drawn = landed === null ? reach : landed;
  const out = drawn * scale;
  const ex = d[0] * out;
  const ey = -d[1] * out;
  const w = Math.max(out * wide, narrow);
  let pool: PoolShape | null = null;
  let cap: CapShape | null = null;
  if (landed !== null) {
    // The cone's footprint: as wide as the cone where it lands, stretched along its slant.
    const radius = drawn * wide;
    const stretch = radius / Math.max(-d[1], GRAZING);
    const h = Math.sqrt(d[0] * d[0] + d[2] * d[2]);
    const hx = h > 0 ? d[0] / h : 1;
    const hz = h > 0 ? d[2] / h : 0;
    const [sx, sz, rx, rz] = [stretch * hx, stretch * hz, radius * hx, radius * hz];
    const across = Math.max(Math.sqrt(sx * sx + rz * rz) * scale, narrow);
    const deep = Math.sqrt(sz * sz + rx * rx);
    pool = { cx: ax + ex, cy: ay + ey, rx: across, ry: Math.max(deep * scale * POOL_FLAT, ratio), strength: poolStrength(across / ratio) * alongBeam(landed / reach) };
  } else if (facing > 0) {
    const long = Math.sqrt(ex * ex + ey * ey);
    cap = { ux: long > 0 ? ex / long : 1, uy: long > 0 ? ey / long : 0, depth: Math.max(w * facing, ratio), strength: CAP * facing * FAR * beamStrength(w / ratio) };
  }
  return { ax, ay, ex, ey, w, narrow, ratio, ends: drawn / reach, look: 1 / Math.max(seen, LEAST_SEEN), cap, pool };
}

/** The rows of a picture `height` high a beam (its far end and its pool too) can touch: rows y0..y1. */
export function beamRows(b: BeamShape, height: number): [number, number] {
  const reach = b.w + 1;
  let top = Math.min(b.ay, b.ay + b.ey) - reach;
  let bottom = Math.max(b.ay, b.ay + b.ey) + reach;
  if (b.pool) {
    top = Math.min(top, b.pool.cy - b.pool.ry - 1);
    bottom = Math.max(bottom, b.pool.cy + b.pool.ry + 1);
  }
  return [Math.max(0, Math.floor(top)), Math.min(height, Math.ceil(bottom))];
}

/**
 * The columns of row `py` of a picture `width` wide a beam (its far end and its pool too) can
 * touch: x0..x1, none when x0 ≥ x1. Its cone lies within the four-sided shape from its start,
 * `narrow` either side, to its end, `w` either side; its end within a circle `w` (or its cap's
 * depth) round it; its pool within its ellipse.
 */
export function beamColumns(b: BeamShape, py: number, width: number): [number, number] {
  const y = py + 0.5;
  let left = Infinity;
  let right = -Infinity;
  const ex = b.ax + b.ex;
  const ey = b.ay + b.ey;
  const long = Math.sqrt(b.ex * b.ex + b.ey * b.ey);
  if (long > 0) {
    const [nx, ny] = [-b.ey / long, b.ex / long];
    const corners = [b.ax + nx * b.narrow, b.ay + ny * b.narrow, ex + nx * b.w, ey + ny * b.w, ex - nx * b.w, ey - ny * b.w, b.ax - nx * b.narrow, b.ay - ny * b.narrow];
    for (let i = 0; i < 8; i += 2) {
      const [x0, y0, x1, y1] = [corners[i], corners[i + 1], corners[(i + 2) % 8], corners[(i + 3) % 8]];
      if ((y0 - y) * (y1 - y) > 0) continue;
      if (y0 === y1) {
        left = Math.min(left, x0, x1);
        right = Math.max(right, x0, x1);
      } else {
        const x = x0 + ((y - y0) * (x1 - x0)) / (y1 - y0);
        left = Math.min(left, x);
        right = Math.max(right, x);
      }
    }
  }
  const round = (cx: number, cy: number, rx: number, ry: number) => {
    const dy = (y - cy) / ry;
    if (dy * dy >= 1) return;
    const across = rx * Math.sqrt(1 - dy * dy);
    left = Math.min(left, cx - across);
    right = Math.max(right, cx + across);
  };
  if (b.cap) {
    const r = Math.max(b.w, b.cap.depth);
    round(ex, ey, r, r);
  }
  if (b.pool) round(b.pool.cx, b.pool.cy, b.pool.rx, b.pool.ry);
  if (left > right) return [0, 0];
  return [Math.max(0, Math.floor(left) - 1), Math.min(width, Math.ceil(right) + 1)];
}

/** How much light a beam (with its far end and its pool) gives at the middle of the picture pixel (`px`, `py`), as a share of a full light's (0 outside it; more than 1 where it is that bright). */
export function beamAt(b: BeamShape, px: number, py: number): number {
  const qx = px + 0.5 - b.ax;
  const qy = py + 0.5 - b.ay;
  const ee = b.ex * b.ex + b.ey * b.ey;
  let lit = 0;
  if (ee > 0) {
    const t = (qx * b.ex + qy * b.ey) / ee;
    if (t >= 0 && t <= 1) {
      const rx = qx - t * b.ex;
      const ry = qy - t * b.ey;
      const half = Math.max(t * b.w, b.narrow);
      const r2 = (rx * rx + ry * ry) / (half * half);
      if (r2 < 1) {
        const s = 1 - r2;
        const along = t * b.ends;
        const left = b.pool ? alongBeam(along) : alongBeam(along) * softEnd(along);
        lit = s * s * left * beamStrength(half / b.ratio) * b.look;
      }
    }
  }
  if (b.cap) {
    const cx = qx - b.ex;
    const cy = qy - b.ey;
    const along = (cx * b.cap.ux + cy * b.cap.uy) / b.cap.depth;
    const across = (cy * b.cap.ux - cx * b.cap.uy) / b.w;
    const q = along * along + across * across;
    if (q < 1) {
      const s = 1 - q;
      lit += s * s * b.cap.strength;
    }
  }
  if (b.pool) {
    const dx = (px + 0.5 - b.pool.cx) / b.pool.rx;
    const dy = (py + 0.5 - b.pool.cy) / b.pool.ry;
    const q = dx * dx + dy * dy;
    if (q < 1) {
      const s = 1 - q;
      lit += s * s * b.pool.strength;
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

/** A lens flare in a picture: its middle and radius (picture pixels), how strong it is at its middle, and its color (packed as the pictures hold them). */
export interface FlareShape {
  x: number;
  y: number;
  r: number;
  strength: number;
  rgb: number;
}

/** The flare at a head's lens, at the picture point (`x`, `y`), for its light (see `beamLight`), at a zoom of `zoom` screen pixels to a layout unit and `ratio` picture pixels to a screen pixel. */
export function flareShape(x: number, y: number, light: { rgb: number; level: number }, zoom: number, ratio: number): FlareShape {
  const size = Math.min(Math.max(FLARE_SIZE * zoom, FLARE_SMALLEST), FLARE_LARGEST);
  const tint = (shift: number) => flareTint((light.rgb >> shift) & 255, light.level) << shift;
  return { x, y, r: size * (FLARE_FROM + (1 - FLARE_FROM) * light.level) * ratio, strength: FLARE_PEAK * light.level, rgb: tint(0) | tint(8) | tint(16) };
}

/** One of red, green or blue (`c`, of a light's hue at full) in its flare at brightness `level`: whiter the brighter it is. */
export function flareTint(c: number, level: number): number {
  return Math.floor(c + (255 - c) * (FLARE_WHITE * level) + 0.5);
}

/** The picture pixels a flare can touch: columns x0..x1, rows y0..y1, in a picture `width` by `height`. */
export function flareBox(f: FlareShape, width: number, height: number): [number, number, number, number] {
  return [Math.max(0, Math.floor(f.x - f.r)), Math.min(width, Math.ceil(f.x + f.r)), Math.max(0, Math.floor(f.y - f.r)), Math.min(height, Math.ceil(f.y + f.r))];
}

/** How much light a flare gives at the middle of the picture pixel (`px`, `py`), as a share of a full light's (0 outside it). */
export function flareAt(f: FlareShape, px: number, py: number): number {
  const dx = (px + 0.5 - f.x) / f.r;
  const dy = (py + 0.5 - f.y) / f.r;
  const q = dx * dx + dy * dy;
  if (q >= 1) return 0;
  const s = 1 - q;
  return s * s * s * f.strength;
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
