// The Moving Head and Servo effects, as the engine works them out (crates/pf-render/src/heads.rs
// and the moving-head and servo parts of dmx.rs), step for step, for the browser stand-in.
//
// Each position comes from the settings and the moment alone. The movement's pace is the speed
// added up from the effect's start (in 64 steps when the speed has a curve), a head running
// behind reads the settings at an earlier moment (never before the effect's start), and every
// position goes through the motor, which keeps it within its limits. Curves that follow the music
// or a timing track sit halfway, as everywhere in the stand-in.

import type { Effect, HeadColor, HeadPattern, ServoPattern } from "../api/sequence";
import type { DmxChannel } from "../api/types";
import { curveValue } from "./curves";
import { channelFromColor, channelsForColors, flashes, openValue, shutterOf, strobeValue } from "./dmx";
import { type Axis, axisValues, cosDeg, sinDeg } from "./motion";

/** The fractional part of `x` (0 to 1). */
const frac = (x: number) => x - Math.floor(x);

/** Where a head sits in its fan. */
export interface FanPlace {
  /** Heads before it in its fan. */
  index: number;
  /** Its place from the fan's middle: -1.5, -0.5, 0.5, 1.5 for four. */
  place: number;
  /** It is in the fan's right half. */
  right: boolean;
}

/**
 * Where the `index`-th of `count` heads sits in its fan when they are split into `fans` fans side
 * by side (the last may have fewer heads).
 */
export function fanPlace(index: number, count: number, fans: number): FanPlace {
  const n = Math.max(1, count);
  const size = Math.ceil(n / Math.min(Math.max(1, fans), n));
  const [fan, i] = [Math.floor(index / size), index % size];
  const inFan = Math.max(1, Math.min(size, Math.max(0, n - fan * size)));
  const middle = (inFan - 1) / 2;
  return { index: i, place: i - middle, right: i > middle };
}

/** A small, well-mixed hash (the engine's `hash32`). */
export function hash32(value: number): number {
  let x = value >>> 0;
  x ^= x >>> 16;
  x = Math.imul(x, 0x7feb352d);
  x ^= x >>> 15;
  x = Math.imul(x, 0x846ca68b);
  x ^= x >>> 16;
  return x >>> 0;
}

/** The `n`-th of the numbers (0 to 1) that seed `seed` gives head `head`. */
export function chance(seed: number, head: number, n: number): number {
  const key = Math.imul(seed, 0x9e3779b9) ^ (Math.imul(head, 16) + n);
  return hash32(key) / 4294967296;
}

/**
 * How far a head is from its centre `cycles` times round its movement, in degrees of pan and
 * tilt: `width` across and `height` up and down, end to end. Wander's paths are picked by `seed`,
 * each head (`head`) its own.
 */
export function headOffset(pattern: HeadPattern, cycles: number, width: number, height: number, seed: number, head: number): [number, number] {
  const [w, h] = [width / 2, height / 2];
  const turn = 360 * frac(cycles);
  switch (pattern) {
    case "sweep": {
      const s = sinDeg(turn);
      return [w * s, h * s];
    }
    case "circle":
      return [w * sinDeg(turn), h * cosDeg(turn)];
    case "eight":
      return [w * sinDeg(turn), h * sinDeg(2 * turn)];
    case "wave":
      return [w * sinDeg(turn), h * sinDeg(3 * turn)];
    case "wander": {
      // Two waves on each axis at chance paces and starting points: never quite repeats.
      const r = (n: number) => chance(seed, head, n);
      const wave = (pace: number, start: number) => sinDeg(360 * frac(pace * cycles + start));
      const pan = 0.65 * wave(0.6 + 0.8 * r(0), r(1)) + 0.35 * wave(1.3 + 1.4 * r(2), r(3));
      const tilt = 0.65 * wave(0.6 + 0.8 * r(4), r(5)) + 0.35 * wave(1.3 + 1.4 * r(6), r(7));
      return [w * pan, h * tilt];
    }
    default:
      return [0, 0];
  }
}

/** How far a servo is from its position `cycles` times through its movement, in degrees: `size` end to end, `steps` places for Steps. */
export function servoOffset(pattern: ServoPattern, cycles: number, size: number, steps: number): number {
  const half = size / 2;
  const f = frac(cycles);
  switch (pattern) {
    // Out to one end, back through the middle to the other, and home: a steady pace.
    case "sweep":
      return half * (f < 0.25 ? 4 * f : f < 0.75 ? 2 - 4 * f : 4 * f - 4);
    case "wave":
      return half * sinDeg(360 * f);
    case "steps": {
      const n = Math.max(2, steps);
      const k = Math.min(Math.floor(f * n), n - 1);
      return -half + (size * k) / (n - 1);
    }
    default:
      return 0;
  }
}

/** A Moving Head effect's settings, each filled in and kept within its range, as the engine reads them. */
export interface HeadSettings {
  pan: number;
  tilt: number;
  pattern: HeadPattern;
  width: number;
  height: number;
  speed: number;
  fanPan: number;
  fanTilt: number;
  delay: number;
  fans: number;
  mirror: boolean;
  color: HeadColor;
  dimmer: number;
  strobe: number;
  seed: number;
}

/** A Servo effect's settings, each filled in and kept within its range. */
export interface ServoSettings {
  position: number;
  pattern: ServoPattern;
  size: number;
  speed: number;
  steps: number;
  fan: number;
  delay: number;
  servo: number;
}

/** `v` within `lo` and `hi`, or `fallback` when it isn't a number. */
function within(v: number | undefined, fallback: number, lo: number, hi: number): number {
  return typeof v === "number" && Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : fallback;
}

/** A Moving Head effect's settings as the engine reads them (missing ones at its defaults). */
export function headSettings(effect: Effect): HeadSettings {
  const p = effect.params.kind === "movingHead" ? effect.params : { kind: "movingHead" as const };
  const whole = (v: number | undefined, fallback: number, lo: number, hi: number) => Math.round(within(v, fallback, lo, hi));
  return {
    pan: within(p.pan, 0, -360, 360),
    tilt: within(p.tilt, 0, -180, 180),
    pattern: p.pattern ?? "still",
    width: within(p.width, 90, 0, 360),
    height: within(p.height, 45, 0, 180),
    speed: within(p.speed, 0.25, 0, 10),
    fanPan: within(p.fanPan, 0, -180, 180),
    fanTilt: within(p.fanTilt, 0, -90, 90),
    delay: within(p.delay, 0, 0, 5),
    fans: whole(p.fans, 1, 1, 16),
    mirror: p.mirror ?? false,
    color: p.color ?? "first",
    dimmer: within(p.dimmer, 100, 0, 100),
    strobe: within(p.strobe, 0, 0, 20),
    seed: whole(p.seed, 1, 0, 9999),
  };
}

/** A Servo effect's settings as the engine reads them (missing ones at its defaults). */
export function servoSettings(effect: Effect): ServoSettings {
  const p = effect.params.kind === "servo" ? effect.params : { kind: "servo" as const };
  return {
    position: within(p.position, 0, -180, 180),
    pattern: p.pattern ?? "still",
    size: within(p.size, 90, 0, 360),
    speed: within(p.speed, 0.5, 0, 10),
    steps: Math.round(within(p.steps, 4, 2, 32)),
    fan: within(p.fan, 0, -180, 180),
    delay: within(p.delay, 0, 0, 5),
    servo: Math.round(within(p.servo, 0, 0, 64)),
  };
}

/** The steps a speed curve is added up in, from the effect's start to the moment drawn. */
const SPEED_STEPS = 64;

/** Where `ms` is in the effect: 0 at its start, 1 at its end. */
function progress(effect: Effect, ms: number): number {
  const length = Math.max(1, effect.endMs - effect.startMs);
  return Math.min(1, Math.max(0, (ms - effect.startMs) / length));
}

/** The number setting `key` at `ms`: its curve's value there, or the setting. */
function number(effect: Effect, key: string, setting: number, ms: number): number {
  const curve = effect.curves?.[key];
  return curve ? curveValue(curve, progress(effect, ms)) : setting;
}

/** The whole-number setting `key` at `ms`: its curve's value there, rounded and kept within `lo` and `hi`, or the setting. */
function whole(effect: Effect, key: string, setting: number, ms: number, lo: number, hi: number): number {
  const curve = effect.curves?.[key];
  return curve ? Math.min(hi, Math.max(lo, Math.round(curveValue(curve, progress(effect, ms))))) : setting;
}

/** The moment `seconds` before `ms`, but not before the effect's start: a head running behind waits at the start. */
export function behind(effect: Effect, ms: number, seconds: number): number {
  const back = Math.round(Math.max(0, seconds) * 1000);
  return Math.max(ms - back, effect.startMs);
}

/** Times round its movement by `ms`: the speed (times a second) added up from the effect's start, following its curve where it has one. */
export function cycles(effect: Effect, speed: number, ms: number): number {
  const elapsed = Math.max(0, ms - effect.startMs);
  const curve = effect.curves?.speed;
  if (!curve) return (speed * elapsed) / 1000;
  const step = elapsed / SPEED_STEPS;
  let sum = 0;
  for (let k = 0; k < SPEED_STEPS; k++) {
    const at = Math.round(effect.startMs + (k + 0.5) * step);
    sum += Math.min(10, Math.max(0, curveValue(curve, progress(effect, at))));
  }
  return (sum * step) / 1000;
}

/** How much of its light an effect's fades leave at `ms` (0 to 1). */
export function fadeLevel(effect: Effect, ms: number): number {
  let level = 1;
  if (effect.fadeInMs > 0) level = Math.min(level, (ms - effect.startMs) / effect.fadeInMs);
  if (effect.fadeOutMs > 0) level = Math.min(level, (effect.endMs - ms) / effect.fadeOutMs);
  return Math.max(0, Math.min(1, level));
}

/** One fixture a channel effect plays on, and its channels set so far this frame (null: from its color). */
export interface FixtureValues {
  channels: DmxChannel[];
  /** Its motors (see `axes` in motion.ts). */
  axes: Axis[];
  /** Its cells (light positions). */
  cells: number;
  values: (number | null)[];
}

type Color = [number, number, number];

/** The palette's colors (0 to 1 each), white when it has none, as the engine's `Colors`. */
function paletteColors(effect: Effect): Color[] {
  const out = effect.palette.colors.map((hex): Color => {
    const n = parseInt(hex.replace("#", ""), 16);
    return Number.isFinite(n) ? [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255] : [1, 1, 1];
  });
  return out.length > 0 ? out : [[1, 1, 1]];
}

/** The palette as a smooth ramp: 0 the first color, 1 the last. */
function ramp(colors: Color[], x: number): Color {
  if (colors.length === 1) return colors[0];
  const pos = Math.min(1, Math.max(0, x)) * (colors.length - 1);
  const i = Math.min(Math.floor(pos), colors.length - 2);
  const f = pos - i;
  const [a, b] = [colors[i], colors[i + 1]];
  return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f];
}

/** A motor's channels set for `degrees` from Home (its limits keep it within them). */
function setAxis(values: (number | null)[], axis: Axis, degrees: number) {
  const [coarse, fine] = axisValues(axis, degrees);
  if (axis.coarse < values.length) values[axis.coarse] = coarse;
  if (axis.fine !== null && fine !== null && axis.fine < values.length) values[axis.fine] = fine;
}

const isHead = (f: FixtureValues) => f.axes.some((a) => a.role === "pan" || a.role === "tilt");

/** Sets a fixture's color and dimmer channels (or its color wheel) to what `color` at `level` makes, by the rule a color drawn on it follows. */
function light(f: FixtureValues, color: Color, level: number) {
  const byte = (v: number) => Math.min(255, Math.max(0, Math.round(v * level * 255)));
  const cells = new Uint8Array(f.cells * 3);
  for (let c = 0; c < f.cells; c++) cells.set(color.map(byte), c * 3);
  const channels = channelsForColors(f.channels, cells);
  f.channels.forEach((channel, i) => {
    if (channelFromColor(channel) && i < f.values.length) f.values[i] = channels[i];
  });
}

/**
 * Sets a fixture's strobe channel (its first, the one its shutter is) to flash `strobe` times a
 * second, or (0) to open for a steady light. True when its shutter does what is asked; false
 * when the light has to flash by itself: it has no strobe channel, or none of its values strobe.
 */
function shutter(f: FixtureValues, strobe: number): boolean {
  const i = f.channels.findIndex((c) => c.role === "strobe");
  if (i < 0) return strobe <= 0;
  const ranges = shutterOf(f.channels[i]);
  const flashing = strobe > 0 ? strobeValue(ranges, strobe) : null;
  const value = flashing ?? openValue(ranges, f.channels[i].value ?? 0);
  if (value !== null && i < f.values.length) f.values[i] = value;
  return flashing !== null || strobe <= 0;
}

/** A Moving Head effect at `ms` (a frame shown for `frameMs`): each head among `fixtures` (the row's, in member order) pointed, and lit. */
export function pointHeads(fixtures: FixtureValues[], effect: Effect, ms: number, frameMs: number) {
  const s = headSettings(effect);
  const fans = whole(effect, "fans", s.fans, ms, 1, 16);
  const seed = whole(effect, "seed", s.seed, ms, 0, 9999);
  const count = fixtures.filter(isHead).length;
  const colors = paletteColors(effect);
  const fade = fadeLevel(effect, ms);
  let head = 0;
  for (const f of fixtures) {
    if (!isHead(f)) continue;
    const { index, place, right } = fanPlace(head, count, fans);
    // Each head runs `delay` behind the one before it in its fan.
    const delay = number(effect, "delay", s.delay, ms);
    const at = behind(effect, ms, index * delay);
    const pan = number(effect, "pan", s.pan, at);
    const tilt = number(effect, "tilt", s.tilt, at);
    const width = number(effect, "width", s.width, at);
    const height = number(effect, "height", s.height, at);
    const fanPan = number(effect, "fanPan", s.fanPan, at);
    const fanTilt = number(effect, "fanTilt", s.fanTilt, at);
    const [dp, dt] = headOffset(s.pattern, cycles(effect, s.speed, at), width, height, seed, index);
    // Mirrored, the right half of a fan points and moves the other way across.
    const across = s.mirror && right ? -1 : 1;
    const pointing = { pan: across * (pan + dp) + place * fanPan, tilt: tilt + place * fanTilt + dt };
    for (const axis of f.axes) {
      if (axis.role === "pan" || axis.role === "tilt") setAxis(f.values, axis, pointing[axis.role]);
    }
    if (s.color !== "drawn") {
      const color = s.color === "wash" ? ramp(colors, progress(effect, at)) : s.color === "perHead" ? colors[head % colors.length] : colors[0];
      const strobe = number(effect, "strobe", s.strobe, ms);
      // A head that can't strobe itself flashes its light when the previews would.
      const shows = shutter(f, strobe) || flashes(strobe, ms, frameMs);
      const level = shows ? (Math.min(100, Math.max(0, number(effect, "dimmer", s.dimmer, at))) / 100) * fade : 0;
      light(f, color, level);
    }
    head++;
  }
}

/** A Servo effect at `ms`: each servo among `fixtures` (or only the one it names, of each fixture) turned, fanned out in order. */
export function turnServos(fixtures: FixtureValues[], effect: Effect, ms: number) {
  const s = servoSettings(effect);
  const servo = whole(effect, "servo", s.servo, ms, 0, 64);
  const steps = whole(effect, "steps", s.steps, ms, 2, 32);
  const chosen = (f: FixtureValues) => f.axes.filter((a) => a.role === "servo").filter((_, nth) => servo === 0 || nth + 1 === servo);
  const count = fixtures.reduce((n, f) => n + chosen(f).length, 0);
  const middle = (Math.max(1, count) - 1) / 2;
  let index = 0;
  for (const f of fixtures) {
    for (const axis of chosen(f)) {
      const delay = number(effect, "delay", s.delay, ms);
      const at = behind(effect, ms, index * delay);
      const position = number(effect, "position", s.position, at);
      const size = number(effect, "size", s.size, at);
      const fan = number(effect, "fan", s.fan, at);
      const degrees = position + (index - middle) * fan + servoOffset(s.pattern, cycles(effect, s.speed, at), size, steps);
      setAxis(f.values, axis, degrees);
      index++;
    }
  }
}
