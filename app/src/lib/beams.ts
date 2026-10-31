// Moving heads' beams and servos' arms in the previews. The 2D previews stamp them into their
// picture of dots (`stampPixels` in pixelBatches.ts), the video export the same way from the
// same numbers (crates/pf-video's beam.rs), held to each other by the scenes in
// api/contracts/pixels/; the 3D view draws them from these numbers too
// (components/layout3d/threeScene.ts).
//
// A head points as its pan and tilt say (motion.ts). Seen from the front, its beam runs from its
// cell along that direction, `length` layout units out, and widens to `length · tan(angle / 2)`
// either side, never narrower than `NARROWEST` screen pixels: at a picture pixel `along` of the
// way out and `r` of the way from the middle line to the edge there, it is
// `BEAM_STRENGTH × (1 - r²)² × (1 - BEAM_FADE × along)` solid, in the head's color, over what's
// under it and under every dot. Past its end it closes with a round cap as wide as the end, so a
// beam pointing at the viewer is a pool of light. Not playing, it shows where the head rests, in
// the unlit color, `UNLIT_BEAM` solid.
//
// A servo's arm turns from its cell in the fixture's front plane: up at Home, clockwise as seen
// from the street. It is `ARM_LENGTH` layout units long (never shorter than `SHORTEST_ARM`
// screen pixels) and `2 × ARM_HALF_WIDTH` screen pixels wide, in `ARM_COLOR` while playing, the
// unlit color when not.

import type { FixtureLayout, PreviewAxis, PreviewMotion, Prop, Transform } from "../api/types";
import { DEFAULT_BEAM, isHead } from "./dmx";
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
    motion.beam = { ...(pan ? { pan } : {}), ...(tilt ? { tilt } : {}), angleDeg: beam.angleDeg, length: beam.length };
  }
  const servos = found.filter((a) => a.role === "servo").map(preview);
  if (servos.length > 0) motion.servos = servos;
  return motion;
}

/** How solid a beam is at its start, playing. */
export const BEAM_STRENGTH = 0.5;
/** How much of its strength a beam has lost at its end. */
export const BEAM_FADE = 0.6;
/** How solid a beam is when nothing plays (where the head rests, in the unlit color). */
export const UNLIT_BEAM = 0.18;
/** The narrowest a beam is drawn, either side of its middle, in screen pixels. */
export const NARROWEST = 1.5;
/** A servo's arm: its length in layout units, its shortest in screen pixels, and half its width in screen pixels. */
export const ARM_LENGTH = 0.4;
export const SHORTEST_ARM = 10;
export const ARM_HALF_WIDTH = 1;
/** A servo's arm while something plays: red, green, blue. */
export const ARM_COLOR: [number, number, number] = [200, 200, 208];

/** Where an axis points, in degrees from Home, in `frame` (null: at rest). */
export function previewDegrees(axis: PreviewAxis, frame: ArrayLike<number> | null): number {
  const at = (i: number, rest: number) => (frame ? (i < frame.length ? frame[i] : 0) : rest);
  const coarse = at(axis.at, axis.rest[0]);
  const fine = axis.fineAt === undefined ? null : at(axis.fineAt, axis.rest[1]);
  return motorDegrees(axis.motor, coarse, fine);
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

/** A beam in a picture: where it starts, how far and which way it runs, and how wide its end is, in picture pixels. */
export interface BeamShape {
  ax: number;
  ay: number;
  ex: number;
  ey: number;
  w: number;
  narrow: number;
}

/**
 * A beam from the picture point (`ax`, `ay`) pointing `d` (layout), `length` long and `angleDeg`
 * wide, at a zoom of `zoom` screen pixels to a layout unit and `ratio` picture pixels to a
 * screen pixel.
 */
export function beamShape(ax: number, ay: number, d: [number, number, number], length: number, angleDeg: number, zoom: number, ratio: number): BeamShape {
  const scale = length * zoom * ratio;
  const narrow = NARROWEST * ratio;
  return { ax, ay, ex: d[0] * scale, ey: -d[1] * scale, w: Math.max(scale * spread(angleDeg), narrow), narrow };
}

/** The picture pixels a beam can touch: columns x0..x1, rows y0..y1, in a picture `width` by `height`. */
export function beamBox(b: BeamShape, width: number, height: number): [number, number, number, number] {
  const reach = b.w + 1;
  const x0 = Math.max(0, Math.floor(Math.min(b.ax, b.ax + b.ex) - reach));
  const x1 = Math.min(width, Math.ceil(Math.max(b.ax, b.ax + b.ex) + reach));
  const y0 = Math.max(0, Math.floor(Math.min(b.ay, b.ay + b.ey) - reach));
  const y1 = Math.min(height, Math.ceil(Math.max(b.ay, b.ay + b.ey) + reach));
  return [x0, x1, y0, y1];
}

/** How solid a beam is at the middle of the picture pixel (`px`, `py`), as a share of its strength (0 outside it). */
export function beamAt(b: BeamShape, px: number, py: number): number {
  const qx = px + 0.5 - b.ax;
  const qy = py + 0.5 - b.ay;
  const ee = b.ex * b.ex + b.ey * b.ey;
  const t = ee > 0 ? (qx * b.ex + qy * b.ey) / ee : 1;
  if (t < 0) return 0;
  let r2: number;
  let fade: number;
  if (t <= 1) {
    const rx = qx - t * b.ex;
    const ry = qy - t * b.ey;
    const half = Math.max(t * b.w, b.narrow);
    r2 = (rx * rx + ry * ry) / (half * half);
    fade = 1 - BEAM_FADE * t;
  } else {
    const rx = qx - b.ex;
    const ry = qy - b.ey;
    r2 = (rx * rx + ry * ry) / (b.w * b.w);
    fade = 1 - BEAM_FADE;
  }
  if (r2 >= 1) return 0;
  const s = 1 - r2;
  return s * s * fade;
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
export function armShape(ax: number, ay: number, d: [number, number, number], zoom: number, ratio: number): ArmShape {
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
