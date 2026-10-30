// Moving heads and servos, as the engine works them out (crates/pf-model/src/motion.rs and the
// motors of dmx.rs): a position in degrees from Home to a motor's channel values and back, a
// fixture's motors, and which way a head's beam or a servo's arm points.
//
// A motor turns `rangeDeg` as its value goes from 0 to its highest (255, or 65535 with a fine
// channel), Home is `homeDeg` from the value-0 end, limits clamp a position first, and a reversed
// motor turns the other way. A head points along (sin tilt·sin pan, cos tilt, sin tilt·cos pan) in
// its own frame (x right, y up, z toward the street): up at Home, tilting toward the street,
// panning to the right as seen from the street. `sinDeg` and `cosDeg` take the engine's own steps,
// so the previews and the video export draw a beam to the same bits.

import type { DmxChannel, DmxMotor, DmxRole } from "../api/types";

/** The usual motor for a channel of `role`: pan 540°, tilt 270°, a servo 180°, Home in the middle. */
export function defaultMotor(role: DmxRole): DmxMotor {
  const range = role === "pan" || role === "panFine" ? 540 : role === "tilt" || role === "tiltFine" ? 270 : 180;
  return { rangeDeg: range, homeDeg: range / 2 };
}

/** A motor's channel: a head's pan or tilt, or a servo. */
export function isMotor(role: DmxRole): boolean {
  return role === "pan" || role === "tilt" || role === "servo";
}

/** The fine channel's role for a motor's role. */
export function fineRole(role: DmxRole): DmxRole | null {
  return role === "pan" ? "panFine" : role === "tilt" ? "tiltFine" : role === "servo" ? "servoFine" : null;
}

/** A channel's motor, or the usual one for its role (a motor's channel only). */
export function motorOf(channel: DmxChannel): DmxMotor | null {
  return isMotor(channel.role) ? (channel.motor ?? defaultMotor(channel.role)) : null;
}

/** Where a motor's travel ends each way, in degrees from Home. */
export function travel(m: DmxMotor): [number, number] {
  return m.reverse ? [m.homeDeg - m.rangeDeg, m.homeDeg] : [-m.homeDeg, m.rangeDeg - m.homeDeg];
}

/** The farthest a motor may turn each way, in degrees from Home: its limits within its travel. */
export function limits(m: DmxMotor): [number, number] {
  const [low, high] = travel(m);
  const lo = m.minDeg === undefined ? low : Math.max(m.minDeg, low);
  const hi = m.maxDeg === undefined ? high : Math.min(m.maxDeg, high);
  return lo <= hi ? [lo, hi] : [hi, hi];
}

/** The coarse and fine bytes a motor takes for `degrees` from Home (16-bit when `fine`; the fine byte is 0 otherwise). */
export function motorValues(m: DmxMotor, degrees: number, fine: boolean): [number, number] {
  const [lo, hi] = limits(m);
  const p = Number.isNaN(degrees) ? 0 : Math.min(Math.max(degrees, lo), hi);
  const x = m.rangeDeg > 0 ? (m.homeDeg + (m.reverse ? -1 : 1) * p) / m.rangeDeg : 0;
  const full = fine ? 65535 : 255;
  const v = Math.min(Math.max(Math.floor(x * full + 0.5), 0), full);
  return fine ? [v >> 8, v & 255] : [v, 0];
}

/** Where a motor is, in degrees from Home, at the value `coarse` (and `fine`, for a 16-bit motor). */
export function motorDegrees(m: DmxMotor, coarse: number, fine: number | null): number {
  const x = fine === null ? coarse / 255 : (coarse * 256 + fine) / 65535;
  return (m.reverse ? -1 : 1) * (x * m.rangeDeg - m.homeDeg);
}

/** Why a motor's settings can't be used, in the engine's words, or null. */
export function motorProblem(m: DmxMotor): string | null {
  const numbers = [m.rangeDeg, m.homeDeg, m.minDeg, m.maxDeg].filter((v) => v !== undefined);
  if (!numbers.every(Number.isFinite)) return "A motor's degrees must be numbers.";
  if (!(m.rangeDeg > 0 && m.rangeDeg <= 1080)) return `A motor turns more than 0 and at most 1080°, so ${m.rangeDeg}° won't work.`;
  if (!(m.homeDeg >= 0 && m.homeDeg <= m.rangeDeg)) return `Home is within the motor's ${m.rangeDeg}°, so ${m.homeDeg}° won't work.`;
  if ((m.minDeg !== undefined && m.minDeg > 0) || (m.maxDeg !== undefined && m.maxDeg < 0)) {
    return "A motor's limits are on either side of Home: the lower 0° or less, the upper 0° or more.";
  }
  return null;
}

/** One motor of a fixture: its channel, its fine channel (indices into the fixture's), and its motor. */
export interface Axis {
  role: DmxRole;
  coarse: number;
  fine: number | null;
  motor: DmxMotor;
}

/**
 * A fixture's motors in the order of their channels, each with its fine channel: the k-th fine
 * channel of a kind belongs to the k-th motor of that kind, wherever it is.
 */
export function axes(channels: DmxChannel[]): Axis[] {
  const out: Axis[] = [];
  channels.forEach((c, i) => {
    const motor = motorOf(c);
    if (!motor) return;
    const nth = channels.slice(0, i).filter((o) => o.role === c.role).length;
    const fine = fineRole(c.role);
    const fines = channels.flatMap((o, j) => (o.role === fine ? [j] : []));
    out.push({ role: c.role, coarse: i, fine: fines[nth] ?? null, motor });
  });
  return out;
}

/** The channel values an axis takes for `degrees` from Home: its own, and its fine channel's (or null). */
export function axisValues(axis: Axis, degrees: number): [number, number | null] {
  const [coarse, fine] = motorValues(axis.motor, degrees, axis.fine !== null);
  return [coarse, axis.fine === null ? null : fine];
}

/** Where an axis is, in degrees from Home, with the fixture's channels at `dmx`. */
export function axisDegrees(axis: Axis, dmx: ArrayLike<number>): number {
  const at = (i: number) => (i < dmx.length ? dmx[i] : 0);
  return motorDegrees(axis.motor, at(axis.coarse), axis.fine === null ? null : at(axis.fine));
}

/** `channels` with each motor resting (its held values) at `degrees` from Home. */
export function restMotorsAt(channels: DmxChannel[], degrees: number): DmxChannel[] {
  const out = channels.map((c) => ({ ...c }));
  for (const axis of axes(out)) {
    const [coarse, fine] = axisValues(axis, degrees);
    out[axis.coarse].value = coarse;
    if (axis.fine !== null && fine !== null) out[axis.fine].value = fine;
  }
  return out;
}

/** The nearest quarter turn to `degrees` (0-3) and the radians left over (within ±45°). */
function quarter(degrees: number): [number, number] {
  if (!Number.isFinite(degrees)) return [0, 0];
  const turns = Math.floor(degrees / 90 + 0.5);
  const rest = degrees - turns * 90;
  return [turns - Math.floor(turns / 4) * 4, rest * (Math.PI / 180)];
}

function sinSeries(t: number): number {
  const t2 = t * t;
  return t * (1 - (t2 / 6) * (1 - (t2 / 20) * (1 - (t2 / 42) * (1 - (t2 / 72) * (1 - t2 / 110)))));
}

function cosSeries(t: number): number {
  const t2 = t * t;
  return 1 - (t2 / 2) * (1 - (t2 / 12) * (1 - (t2 / 30) * (1 - (t2 / 56) * (1 - (t2 / 90) * (1 - t2 / 132)))));
}

/** The sine of `degrees`, by the engine's own steps (`sin_deg`). */
export function sinDeg(degrees: number): number {
  const [q, t] = quarter(degrees);
  return q === 0 ? sinSeries(t) : q === 1 ? cosSeries(t) : q === 2 ? -sinSeries(t) : -cosSeries(t);
}

/** The cosine of `degrees`, by the engine's own steps (`cos_deg`). */
export function cosDeg(degrees: number): number {
  const [q, t] = quarter(degrees);
  return q === 0 ? cosSeries(t) : q === 1 ? -sinSeries(t) : q === 2 ? -cosSeries(t) : sinSeries(t);
}

/** A fixture's own frame in the layout: its right, up and front as the columns of a row-major 3 × 3 matrix. */
export type Basis = ArrayLike<number>;

/** The frame of a fixture that isn't turned. */
export const UPRIGHT: Basis = [1, 0, 0, 0, 1, 0, 0, 0, 1];

function turn(b: Basis, x: number, y: number, z: number): [number, number, number] {
  return [b[0] * x + b[1] * y + b[2] * z, b[3] * x + b[4] * y + b[5] * z, b[6] * x + b[7] * y + b[8] * z];
}

/** Which way a head points in the layout for `pan` and `tilt` (degrees from Home), its frame being `basis`. */
export function beamDirection(pan: number, tilt: number, basis: Basis): [number, number, number] {
  const [sp, cp, st, ct] = [sinDeg(pan), cosDeg(pan), sinDeg(tilt), cosDeg(tilt)];
  return turn(basis, st * sp, ct, st * cp);
}

/** Which way a servo's arm points in the layout at `degrees` from Home, its frame being `basis`. */
export function armDirection(degrees: number, basis: Basis): [number, number, number] {
  return turn(basis, sinDeg(degrees), cosDeg(degrees), 0);
}
