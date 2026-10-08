// Settings that change over an effect: the same reading of a curve as the engine's (see
// crates/pf-sequence/src/curve.rs), and the edits the settings panel makes to curves.

import type { Curve, CurveShape, Effect } from "../api/sequence";

export const MIN_CYCLES = 0.1;
export const MAX_CYCLES = 100;
export const MAX_POINTS = 512;

/** Where a custom curve is at `t`: straight between points, held before the first and after the
 * last; at a step (points sharing a time) the later point counts from that time on. */
function customLevel(points: [number, number][], t: number): number {
  if (points.length === 0) return 0;
  let after = 0;
  while (after < points.length && points[after][0] <= t) after++;
  if (after === 0) return points[0][1];
  const a = points[after - 1];
  const b = points[after];
  if (!b) return a[1];
  const span = b[0] - a[0];
  return span <= 0 ? a[1] : a[1] + (b[1] - a[1]) * ((t - a[0]) / span);
}

/** Where the curve is between `from` (0) and `to` (1) at time `t` (0–1 over the effect). */
export function curveLevel(curve: Curve, t: number): number {
  const at = Number.isFinite(t) ? Math.min(1, Math.max(0, t)) : 0;
  const cycles = Math.min(MAX_CYCLES, Math.max(MIN_CYCLES, curve.cycles ?? 1));
  const phase = at * cycles - Math.floor(at * cycles);
  switch (curve.shape) {
    case "ramp":
      return at;
    case "sine":
      return 0.5 - 0.5 * Math.cos(at * cycles * 2 * Math.PI);
    case "square":
      return phase < 0.5 ? 0 : 1;
    case "saw":
      return phase;
    case "custom":
      return customLevel(curve.points ?? [], at);
  }
}

/** The setting's value at time `t` (0–1 over the effect). */
export function curveValue(curve: Curve, t: number): number {
  return curve.from + (curve.to - curve.from) * curveLevel(curve, t);
}

/** The effect's settings as they are `ms` into the sequence, each curve's value in place of its
 * setting (whole numbers rounded, as the engine does). */
export function effectAt(effect: Effect, ms: number): Effect {
  const curves = effect.curves;
  if (!curves || Object.keys(curves).length === 0) return effect;
  const length = Math.max(1, effect.endMs - effect.startMs);
  const t = Math.min(1, Math.max(0, (ms - effect.startMs) / length));
  const params = { ...effect.params } as Record<string, unknown>;
  const now: Effect = { ...effect, params: params as Effect["params"] };
  for (const [key, curve] of Object.entries(curves)) {
    const value = curveValue(curve, t);
    if (key === "sparkles" || key === "blur") now[key] = Math.round(value);
    else if (typeof params[key] === "number" || params[key] === undefined) params[key] = value;
  }
  return now;
}

/** The shapes people pick from: a ramp is "up" or "down" by which end is higher. */
export type ShapeChoice = "rampUp" | "rampDown" | "sine" | "square" | "saw" | "custom";

export const SHAPE_CHOICES: { value: ShapeChoice; label: string }[] = [
  { value: "rampUp", label: "Ramp up" },
  { value: "rampDown", label: "Ramp down" },
  { value: "sine", label: "Sine" },
  { value: "square", label: "Square" },
  { value: "saw", label: "Saw" },
  { value: "custom", label: "Custom" },
];

export function shapeChoice(curve: Curve): ShapeChoice {
  if (curve.shape === "ramp") return curve.from <= curve.to ? "rampUp" : "rampDown";
  return curve.shape;
}

/** Points tracing a curve's shape, for turning it into a custom one that starts out the same. */
function tracePoints(curve: Curve): [number, number][] {
  if (curve.shape === "ramp") return [[0, 0], [1, 1]];
  if (curve.shape === "custom") return curve.points ?? [[0, 0], [1, 1]];
  const n = 16;
  return Array.from({ length: n + 1 }, (_, i) => [i / n, Math.round(curveLevel(curve, i / n) * 1000) / 1000] as [number, number]);
}

/** The curve with another shape picked, keeping its two values. */
export function withShape(curve: Curve, choice: ShapeChoice): Curve {
  const lo = Math.min(curve.from, curve.to);
  const hi = Math.max(curve.from, curve.to);
  switch (choice) {
    case "rampUp":
      return { shape: "ramp", from: lo, to: hi };
    case "rampDown":
      return { shape: "ramp", from: hi, to: lo };
    case "custom":
      return { shape: "custom", from: curve.from, to: curve.to, points: tracePoints(curve) };
    default: {
      const shape: CurveShape = choice;
      return curve.cycles === undefined ? { shape, from: curve.from, to: curve.to } : { shape, from: curve.from, to: curve.to, cycles: curve.cycles };
    }
  }
}

/** A new curve for a setting turned to change over the effect: from its value toward the far end
 * of its range. */
export function startCurve(value: number, min: number, max: number): Curve {
  return { shape: "ramp", from: value, to: value - min < max - value ? max : min };
}

/** `curves` with `key` set to `curve` (removed when null); undefined when none are left. */
export function withCurve(curves: Effect["curves"], key: string, curve: Curve | null): Effect["curves"] {
  const next = { ...(curves ?? {}) };
  if (curve) next[key] = curve;
  else delete next[key];
  return Object.keys(next).length > 0 ? next : undefined;
}

/** Puts a custom curve's points in time order, inside 0–1. */
export function tidyPoints(points: [number, number][]): [number, number][] {
  const fit = (v: number) => Math.min(1, Math.max(0, v));
  return points
    .map(([t, v]) => [fit(t), fit(v)] as [number, number])
    .sort((a, b) => a[0] - b[0])
    .slice(0, MAX_POINTS);
}
