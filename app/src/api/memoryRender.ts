// A rough, browser-only drawing of an authored sequence for the in-memory stand-in (tests and the
// `?demo` page). The real renderer is pf-render in the engine; this only has to look plausible:
// each effect kind gets a simple look of its own, layers mix bottom to top, later rows cover
// earlier ones, and fades dim.

import { frontView } from "../lib/geometry";
import { channelsPerPixel, nodeCount } from "../lib/shows";
import { facePartColor, faceParts, facesOf, phonemeAt, targetNodes } from "../lib/submodels";
import { effectAt } from "../lib/curves";
import type { DancerCharacter, Effect, Sequence, Sweep, TimingTrack } from "./sequence";
import type { FaceDefinition, Prop, Show } from "./types";

type Rgb = [number, number, number];

function parseColor(hex: string): Rgb {
  const n = parseInt(hex.replace("#", ""), 16);
  return Number.isFinite(n) ? [(n >> 16) & 255, (n >> 8) & 255, n & 255] : [255, 255, 255];
}

const frac = (x: number) => x - Math.floor(x);

/** A repeatable pseudo-random number in 0..1 for two integers. */
function hash(a: number, b: number): number {
  let h = Math.imul(a ^ 0x9e3779b9, 0x85ebca6b) ^ Math.imul(b + 0x632be5ab, 0xc2b2ae35);
  h ^= h >>> 15;
  h = Math.imul(h, 0x2c1b3c6d);
  h ^= h >>> 12;
  return (h >>> 0) / 4294967296;
}

interface Px {
  u: number;
  v: number;
  i: number;
  n: number;
  /** How wide the target is next to its height. */
  aspect: number;
}

function num(params: Record<string, unknown>, key: string, fallback: number): number {
  const v = params[key];
  return typeof v === "number" && Number.isFinite(v) ? v : fallback;
}

/** Where a pixel is along a sweep: 0 where it starts, 1 where it ends. */
function sweepAt(px: Px, sweep: Sweep | undefined): number {
  switch (sweep) {
    case "rightToLeft":
      return 1 - px.u;
    case "up":
      return px.v;
    case "down":
      return 1 - px.v;
    case "centerOut":
      return Math.abs(px.u - 0.5) * 2;
    case "edgesIn":
      return 1 - Math.abs(px.u - 0.5) * 2;
    case "diagonal":
      return (px.u + px.v) / 2;
    case "radial":
      return Math.hypot(px.u - 0.5, px.v - 0.5) / Math.SQRT1_2;
    default:
      return px.u;
  }
}

/** A dancer's body and head colors. */
const DANCERS: Record<DancerCharacter, [Rgb, Rgb]> = {
  skeleton: [[255, 255, 255], [255, 255, 255]],
  ghost: [[224, 240, 255], [224, 240, 255]],
  witch: [[153, 36, 255], [77, 255, 38]],
  santa: [[255, 15, 10], [255, 255, 255]],
  snowman: [[255, 255, 255], [255, 255, 255]],
  elf: [[26, 230, 31], [255, 178, 128]],
};

/** How far (x, y) is from the line a–b. */
function fromLine(x: number, y: number, ax: number, ay: number, bx: number, by: number): number {
  const [dx, dy] = [bx - ax, by - ay];
  const t = Math.max(0, Math.min(1, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy || 1)));
  return Math.hypot(ax + t * dx - x, ay + t * dy - y);
}

/** How far a mouth shape opens (0–1), as the engine's Sing effect opens it. */
const OPEN: Record<string, number> = { AI: 1, O: 0.9, E: 0.75, U: 0.6, WQ: 0.5, L: 0.5, ETC: 0.45, FV: 0.3, MBP: 0, REST: 0 };

/** The mark of `track` under `ms`, with its number. */
function markAt(track: TimingTrack | undefined, ms: number): { i: number; startMs: number; endMs: number } | null {
  const i = track ? track.marks.findIndex((m) => m.startMs <= ms && ms < m.endMs) : -1;
  return track && i >= 0 ? { i, startMs: track.marks[i].startMs, endMs: track.marks[i].endMs } : null;
}

/** One effect's color and coverage (0–1) at one pixel. */
function shade(effect: Effect, ms: number, px: Px, seed: number, tracks: TimingTrack[] = []): [Rgb, number] {
  const p = effect.params as Record<string, unknown> & { kind: string };
  const colors = effect.palette.colors.length > 0 ? effect.palette.colors.map(parseColor) : [[255, 255, 255] as Rgb];
  const get = (k: number) => colors[((Math.floor(k) % colors.length) + colors.length) % colors.length];
  const ramp = (x: number): Rgb => {
    if (colors.length === 1) return colors[0];
    const at = Math.min(0.9999, Math.max(0, x)) * (colors.length - 1);
    const [a, b, f] = [colors[Math.floor(at)], colors[Math.floor(at) + 1], frac(at)];
    return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f];
  };
  const length = Math.max(1, effect.endMs - effect.startMs);
  const t = (ms - effect.startMs) / length;
  const el = (ms - effect.startMs) / 1000;
  const reverse = p.direction === "reverse";
  const x = reverse ? 1 - px.i / Math.max(1, px.n - 1) : px.i / Math.max(1, px.n - 1);
  const scale = (c: Rgb, k: number): Rgb => [c[0] * k, c[1] * k, c[2] * k];
  const gradient = (g: unknown, fallback: Rgb) => (g === "horizontal" ? ramp(px.u) : g === "vertical" ? ramp(px.v) : fallback);
  switch (p.kind) {
    case "on": {
      const level = num(p, "startLevel", 1) + (num(p, "endLevel", 1) - num(p, "startLevel", 1)) * t;
      return [scale(gradient(p.gradient, get(0)), level), 1];
    }
    case "off":
      return [[0, 0, 0], 1];
    case "colorWash": {
      const pos = frac(t * num(p, "cycles", 1) * 0.5) * 2;
      const wave = pos > 1 ? 2 - pos : pos;
      const offset = p.gradient === "horizontal" ? px.u : p.gradient === "vertical" ? px.v : 0;
      return [ramp(frac(wave + offset * 0.5)), 1];
    }
    case "fade":
      return [scale(get(0), p.direction === "out" ? 1 - t : t), 1];
    case "chase": {
      const bands = Math.max(1, num(p, "bands", 1));
      let pos = el * num(p, "speed", 1);
      if (p.bounce) pos = 1 - Math.abs(1 - frac(pos / 2) * 2);
      const along = p.order === undefined || p.order === "wiring" ? x : reverse ? 1 - px.u : px.u;
      const s = (along - pos) * bands;
      return frac(s) < num(p, "width", 0.2) ? [get(s), 1] : [[0, 0, 0], 0];
    }
    case "bars": {
      const along = p.axis === "horizontal" ? px.u : px.v;
      const s = ((reverse ? 1 - along : along) - el * num(p, "speed", 0.5)) * Math.max(1, num(p, "count", 4));
      return [get(s), 1];
    }
    case "wave": {
      const phase = 2 * Math.PI * (px.u * num(p, "cycles", 1) - (reverse ? -1 : 1) * el * num(p, "speed", 1));
      const y = 0.5 + (num(p, "height", 0.8) / 2) * Math.sin(phase);
      return Math.abs(px.v - y) < num(p, "thickness", 0.2) / 2 ? [ramp(px.u), 1] : [[0, 0, 0], 0];
    }
    case "twinkle": {
      if (hash(px.i, seed) > num(p, "density", 0.3) * 1.6) return [[0, 0, 0], 0];
      const env = Math.max(0, Math.sin(2 * Math.PI * (el * num(p, "rate", 1) + hash(seed, px.i))));
      return [get(hash(px.i, seed + 1) * colors.length), env];
    }
    case "shimmer": {
      const cycles = el * num(p, "rate", 10);
      return frac(cycles) < num(p, "duty", 0.5) ? [get(cycles), 1] : [[0, 0, 0], 0];
    }
    case "strobe": {
      const cycles = el * num(p, "rate", 10);
      const lit = frac(cycles) < 0.5 && hash(px.i, Math.floor(cycles) + seed) < num(p, "density", 0.2);
      return lit ? [get(hash(Math.floor(cycles), px.i) * colors.length), 1] : [[0, 0, 0], 0];
    }
    case "spiral": {
      const turn = (reverse ? -1 : 1) * el * num(p, "speed", 0.5);
      const s = (px.u + num(p, "twist", 1) * px.v - turn) * Math.max(1, num(p, "count", 3));
      return frac(s) < num(p, "thickness", 0.5) ? [get(s), 1] : [[0, 0, 0], 0];
    }
    case "fire": {
      const flicker = 0.55 + 0.45 * Math.sin(px.u * 23 + el * 9 + Math.sin(px.u * 7 - el * 5) * 2);
      const heat = Math.max(0, 1 - px.v / Math.max(0.05, num(p, "height", 0.8))) * flicker;
      const rgb: Rgb = heat < 0.5 ? [heat * 2 * 255, 0, 0] : [255, (heat - 0.5) * 2 * 220, heat > 0.85 ? (heat - 0.85) * 600 : 0];
      return [rgb, heat > 0.04 ? 1 : 0];
    }
    case "meteors": {
      const count = Math.max(1, num(p, "count", 5));
      const sideways = p.direction === "left" || p.direction === "right";
      const across = sideways ? px.v : px.u;
      const along = p.direction === "up" || p.direction === "right" ? (sideways ? px.u : px.v) : 1 - (sideways ? px.u : px.v);
      const lane = Math.floor(across * count);
      const head = frac(el * num(p, "speed", 1) + hash(lane, seed)) * 1.3;
      const behind = head - along;
      const tail = num(p, "length", 0.25);
      return behind >= 0 && behind < tail ? [get(lane), 1 - behind / tail] : [[0, 0, 0], 0];
    }
    case "ripple": {
      const r = Math.hypot(px.u - 0.5, px.v - 0.5) / Math.SQRT1_2;
      const spacing = Math.max(0.02, num(p, "spacing", 0.4));
      const s = (r - el * num(p, "speed", 0.5)) / spacing;
      return frac(s) * spacing < num(p, "thickness", 0.12) ? [get(-Math.floor(s)), 1] : [[0, 0, 0], 0];
    }
    case "shape": {
      // Rings growing from random places, each fading over its life.
      const life = Math.max(0.01, num(p, "lifetime", 5) / 100);
      for (let k = 0; k < Math.min(20, num(p, "count", 5)); k++) {
        const age = t / life + hash(k, seed);
        const gen = Math.floor(age);
        const [cx, cy] = p.randomLocation === false ? [num(p, "centerX", 50) / 100, num(p, "centerY", 50) / 100] : [hash(k, gen + seed), hash(gen, k + seed)];
        const r = (num(p, "startSize", 1) + num(p, "growth", 10) * frac(age)) / 40;
        if (Math.abs(Math.hypot(px.u - cx, px.v - cy) - r) < 0.03) return [get(k), p.fade === false ? 1 : 1 - frac(age)];
      }
      return [[0, 0, 0], 0];
    }
    case "fan": {
      const [dx, dy] = [px.u - num(p, "centerX", 50) / 100, px.v - num(p, "centerY", 50) / 100];
      const r = Math.hypot(dx, dy) / 0.5;
      if (r < num(p, "startRadius", 1) / 100 || r > num(p, "endRadius", 50) / 100) return [[0, 0, 0], 0];
      const turn = (reverse ? -1 : 1) * t * num(p, "revolutions", 2);
      const a = frac(Math.atan2(dx, dy) / (2 * Math.PI) + (r * num(p, "bladeAngle", 90)) / 360 + turn) * Math.max(1, num(p, "blades", 3));
      const width = num(p, "bladeWidth", 50) / 100;
      return frac(a) < width ? [get((frac(a) / width) * colors.length), 1] : [[0, 0, 0], 0];
    }
    case "morph": {
      // A line sweeping up the prop: the head, then a tail fading behind it.
      const head = t / Math.max(0.01, num(p, "headDuration", 20) / 100);
      const behind = head - px.v;
      if (Math.abs(behind) < 0.04) return [get(0), 1];
      return behind > 0 && behind < head ? [get(1), Math.max(0, 1 - behind / 2)] : [[0, 0, 0], 0];
    }
    case "circles": {
      const size = num(p, "size", 5) / 40;
      for (let k = 0; k < Math.min(10, num(p, "count", 3)); k++) {
        const step = (el * num(p, "speed", 10)) / 20;
        const [x, y] = [frac(hash(k, seed) + step * (hash(seed, k) - 0.5)), frac(hash(k + 7, seed) + step * (hash(seed, k + 7) - 0.5))];
        if (Math.hypot(px.u - x, px.v - y) < size) return [get(k), 1];
      }
      return [[0, 0, 0], 0];
    }
    case "pinwheel": {
      // Arms turning around the middle, bending with twist.
      const arms = Math.max(1, num(p, "arms", 3));
      const [dx, dy] = [px.u - 0.5 - num(p, "centerX", 0) / 200, px.v - 0.5 - num(p, "centerY", 0) / 200];
      const r = Math.hypot(dx, dy) / Math.SQRT1_2;
      if (r > num(p, "armSize", 100) / 100) return [[0, 0, 0], 0];
      const turn = ((p.counterclockwise === false ? -1 : 1) * el * num(p, "speed", 10) * 20) / 360;
      const a = frac(Math.atan2(dy, dx) / (2 * Math.PI) - turn - (r * num(p, "twist", 0)) / 360) * arms;
      return frac(a) < Math.max(0.04, num(p, "thickness", 0) / 100) ? [get(a + 1), 1] : [[0, 0, 0], 0];
    }
    case "snowflakes": {
      // Flakes drifting down and across, wrapping around.
      const count = Math.min(100, num(p, "count", 5));
      const drift = (el * num(p, "speed", 10)) / 20;
      for (let k = 0; k < count; k++) {
        const [fx, fy] = [frac(hash(k, seed) + (p.motion === "blowing" ? drift / 2 : 0)), frac(hash(seed, k) - drift)];
        if (Math.abs(px.u - fx) < 0.025 && Math.abs(px.v - fy) < 0.04) return [get(0), 1];
      }
      return [[0, 0, 0], 0];
    }
    case "plasma":
    case "butterfly": {
      const time = (el * num(p, "speed", 10)) / 10;
      const v = Math.sin(px.u * 10 + time) + Math.sin(10 * (px.u * Math.sin(time / 2) + px.v * Math.cos(time / 3)) + time) + Math.sin(Math.hypot(px.u - 0.5, px.v - 0.5) * 8 + time);
      return [ramp((Math.sin(v) + 1) / 2), 1];
    }
    case "garlands": {
      // Swags stacking up from the bottom over the effect.
      const rows = 12;
      const filled = Math.floor(frac(t * Math.max(0.1, num(p, "cycles", 1))) * (rows + 1));
      const row = Math.floor(px.v * rows - Math.abs(Math.sin(px.u * Math.PI * 4)) * (p.shape === "straight" || p.shape === undefined ? 0 : 0.6));
      return row < filled && frac(px.v * rows) < 0.5 ? [ramp(1 - row / rows), 1] : [[0, 0, 0], 0];
    }
    case "lines": {
      // Lines between points bouncing around.
      const bounce = (x: number) => 1 - Math.abs(1 - frac(x / 2) * 2);
      for (let k = 0; k < Math.min(20, num(p, "count", 2)); k++) {
        const step = el * num(p, "speed", 1) * 0.3;
        const [ax, ay] = [bounce(hash(k, seed) * 2 + step * 0.7), bounce(hash(seed, k) * 2 + step * 0.9)];
        const [bx, by] = [bounce(hash(k + 9, seed) * 2 + step * 0.8), bounce(hash(seed, k + 9) * 2 + step * 0.6)];
        const [vx, vy] = [bx - ax, by - ay];
        const along = Math.max(0, Math.min(1, ((px.u - ax) * vx + (px.v - ay) * vy) / Math.max(1e-6, vx * vx + vy * vy)));
        if (Math.hypot(px.u - ax - vx * along, px.v - ay - vy * along) < 0.02) return [get(k), 1];
      }
      return [[0, 0, 0], 0];
    }
    case "life": {
      const generation = Math.floor(el * num(p, "speed", 10));
      return hash(px.i + generation * 7919, seed) < num(p, "density", 50) / 200 ? [ramp(hash(px.i, seed)), 1] : [[0, 0, 0], 0];
    }
    case "tendril": {
      // A trail following a point around a circle.
      for (let k = 0; k < 12; k++) {
        const a = el * 2 - k * 0.12;
        if (Math.hypot(px.u - 0.5 - 0.3 * Math.sin(a), px.v - 0.5 - 0.3 * Math.cos(a)) < 0.04) return [ramp(t), 1];
      }
      return [[0, 0, 0], 0];
    }
    case "text": {
      // A band where the text sits, scrolling if it moves.
      const text = typeof p.text === "string" ? p.text : "";
      const width = Math.min(1, text.length * 0.08);
      const move = p.movement === "left" ? -frac(el * num(p, "speed", 10) * 0.02) * 2 + 1 : 0;
      const left = 0.5 - width / 2 + move;
      const inside = px.u >= left && px.u < left + width && Math.abs(px.v - 0.5) < 0.15;
      return inside && hash(Math.floor((px.u - left) * 40), Math.floor(px.v * 10)) < 0.55 ? [get(0), 1] : [[0, 0, 0], 0];
    }
    case "vuMeter": {
      // A stand-in for the music (the engine reads the song): bars bouncing frame by frame.
      const bars = Math.max(1, Math.min(32, num(p, "bars", 6)));
      const bar = Math.min(bars - 1, Math.floor(px.u * bars));
      const level = 0.25 + 0.7 * hash(bar, Math.floor(el * 12));
      return px.v <= level ? [ramp(px.v), 1] : [[0, 0, 0], 0];
    }
    case "impact": {
      const hold = num(p, "hold", 0) / length;
      const x = Math.max(0, (t - hold) / Math.max(0.001, 1 - hold));
      const fade = p.decay === "linear" ? 1 - x : p.decay === "punch" ? (1 - x) ** 2 * (0.75 + 0.25 * Math.cos(x * 12)) : Math.exp(-4.6 * x);
      const bloom = num(p, "bloom", 0);
      const reach = bloom > 0 ? (ms - effect.startMs) / bloom : 2;
      const r = Math.hypot(px.u - num(p, "centerX", 50) / 100, px.v - num(p, "centerY", 50) / 100) / Math.SQRT1_2;
      const hit: Rgb = p.color === "palette" ? get(0) : [255, 255, 255];
      return r <= reach ? [p.colorShift ? (x < 0.5 ? hit : ramp(x)) : hit, fade] : [[0, 0, 0], 0];
    }
    case "wipe": {
      const d = Math.max(0.01, num(p, "duration", 50) / 100);
      const off = p.mode === "off" || (p.mode === "onOff" && t > 1 - Math.min(0.5, d));
      const progress = Math.min(1, p.mode === "onOff" && off ? (t - (1 - Math.min(0.5, d))) / Math.min(0.5, d) : t / (p.mode === "onOff" ? Math.min(0.5, d) : d));
      const s = sweepAt(px, p.direction as Sweep | undefined);
      const band = num(p, "band", 0);
      if (band > 0) {
        const lead = (off ? 1 - progress : progress) * (1 + band);
        return s <= lead && s > lead - band ? [ramp(s), 1] : [[0, 0, 0], 0];
      }
      return (off ? s > progress : s <= progress) ? [ramp(s), 1] : [[0, 0, 0], 0];
    }
    case "lightning": {
      // A strike a slot, flickering: the main stroke, a re-strike, then the tail.
      const slot = 1 / Math.max(0.1, num(p, "density", 1));
      const k = Math.floor(el / slot);
      const since = el - (k + hash(k, seed) * 0.8) * slot;
      if (since < 0) return [[0, 0, 0], 0];
      const level = Math.max(Math.exp(-since / 0.045), since > 0.09 ? 0.8 * Math.exp(-(since - 0.09) / 0.07) : 0);
      if (level < 0.01) return [[0, 0, 0], 0];
      if (p.flashOnly) return [get(0), level];
      const x = 0.2 + 0.6 * hash(seed, k) + 0.08 * Math.sin(px.v * 23 + k) + 0.04 * Math.sin(px.v * 61 + k * 3);
      return Math.abs(px.u - x) < 0.025 ? [get(0), level] : [get(0), level * num(p, "glow", 0.25)];
    }
    case "pulse": {
      const track = tracks.find((tr) => tr.id === p.timingTrack);
      const now = ms;
      let phase = frac((ms - effect.startMs) / 500);
      let k = Math.floor((ms - effect.startMs) / 500);
      if (p.source !== undefined && p.source !== "marks") {
        // A stand-in for the music: a beat twice a second, rising and falling.
        phase = frac(el * 2);
      } else if (track) {
        const i = track.marks.filter((m) => m.startMs <= now).length - 1;
        if (i < 0) return [get(0), num(p, "min", 0.1)];
        const next = track.marks[i + 1]?.startMs ?? track.marks[i].endMs;
        phase = (now - track.marks[i].startMs) / Math.max(1, next - track.marks[i].startMs);
        k = i;
      }
      const shape = p.shape ?? "sine";
      const f = phase >= 1 ? 0 : shape === "saw" ? 1 - phase : shape === "square" ? (phase < 0.5 ? 1 : 0) : shape === "heartbeat" ? Math.max(Math.exp(-((phase / 0.08) ** 2)), 0.6 * Math.exp(-(((phase - 0.28) / 0.08) ** 2))) : 0.5 + 0.5 * Math.cos(2 * Math.PI * phase);
      const [lo, hi] = [num(p, "min", 0.1), num(p, "max", 1)];
      return [get(k), lo + (hi - lo) * f];
    }
    case "sing": {
      const track = tracks.find((tr) => tr.id === p.timingTrack);
      const mark = markAt(track, ms);
      const lo = num(p, "min", 0);
      const open = mark ? (track?.kind === "phonemes" || track?.kind === "words" || track?.kind === "lyrics" ? OPEN[phonemeAt(track, ms).toUpperCase()] ?? 0.45 : 0.8) : 0;
      const x = mark ? (ms - mark.startMs) / Math.max(1, mark.endMs - mark.startMs) : 0;
      switch (p.mode) {
        case "wordPop":
          return mark ? [get(mark.i), lo + (1 - lo) * Math.exp(-3 * x)] : [get(0), lo];
        case "barMouth":
          return Math.abs(px.v - 0.5) <= open / 2 && open > 0 ? [get(0), 1] : [get(0), lo];
        case "karaoke":
          return mark ? (px.u <= x ? [get(0), 1] : [get(1), lo]) : [[0, 0, 0], 0];
        default:
          return [get(0), lo + (1 - lo) * open];
      }
    }
    case "colorShift": {
      const changes = Math.max(1, colors.length - 1);
      const slot = 1 / changes;
      const k = Math.min(changes - 1, Math.floor(t / slot));
      const window = Math.min(slot, num(p, "duration", 25) / 100);
      const stagger = num(p, "stagger", 0) / 100;
      const delay = stagger * window * sweepAt(px, p.direction as Sweep | undefined);
      const own = window * (1 - stagger);
      const x = t - k * slot - delay;
      let f = own <= 0 ? (x >= 0 ? 1 : 0) : Math.max(0, Math.min(1, x / own));
      if (p.ease === "instant") f = f > 0 ? 1 : 0;
      else if (p.ease !== "linear") f = f * f * (3 - 2 * f);
      const [a, b] = [get(k), get(k + 1)];
      return [[a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f], 1];
    }
    case "dancer": {
      // A stick figure in the character's colors: an arm up on each beat, turn about, and a
      // bob. The engine draws the characters themselves.
      const count = Math.max(1, Math.round(num(p, "count", 1)));
      const across = (p.mirror ? 1 - px.u : px.u) * count;
      const slot = Math.min(count - 1, Math.floor(across));
      const track = tracks.find((tr) => tr.id === p.timingTrack) ?? tracks.find((tr) => tr.kind === "beats");
      let beat = (ms - effect.startMs) / 500;
      if (track && track.marks.length > 1) {
        const i = Math.min(track.marks.length - 2, Math.max(0, track.marks.filter((m) => m.startMs <= ms).length - 1));
        beat = i + (ms - track.marks[i].startMs) / Math.max(1, track.marks[i + 1].startMs - track.marks[i].startMs);
      }
      beat = (beat - slot * num(p, "stagger", 0)) * (p.speed === "half" ? 0.5 : p.speed === "double" ? 2 : 1);
      // As tall as its size says, or as its share of the width allows.
      const wide = px.aspect / count;
      const tall = Math.min(num(p, "size", 90) / 100, wide * 3.6);
      const x = ((across - slot - num(p, "x", 50) / 100) * wide) / tall;
      const y = (px.v - num(p, "y", 0) / 100) / tall + 0.03 * (0.5 + 0.5 * Math.cos(2 * Math.PI * beat));
      const up = Math.floor(beat) % 2 === 0 ? -1 : 1;
      const [body, head] = p.usePalette ? [get(0), get(1)] : DANCERS[(p.character as DancerCharacter | undefined) ?? "skeleton"];
      if (Math.hypot(x, y - 0.87) < 0.11) return [head, 1];
      const limbs = [
        [0, 0.42, 0, 0.76],
        [0, 0.72, -0.2, up < 0 ? 0.98 : 0.46],
        [0, 0.72, 0.2, up > 0 ? 0.98 : 0.46],
        [0, 0.42, -0.1, 0],
        [0, 0.42, 0.1, 0],
      ];
      return limbs.some(([ax, ay, bx, by]) => fromLine(x, y, ax, ay, bx, by) < 0.04) ? [body, 1] : [[0, 0, 0], 0];
    }
    default:
      return [[0, 0, 0], 0];
  }
}

/** A repeatable number for an effect id, standing in for the engine's seed. */
function idSeed(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = Math.imul(h ^ id.charCodeAt(i), 0x01000193);
  return h >>> 0;
}

/** Blinking eyes: shut for 150 ms every 3–5 s, the first 1–3 s in (like the engine's, not the same times). */
function blinking(seed: number, startMs: number, ms: number): boolean {
  let at = startMs + 1000 + Math.floor(hash(seed, 0) * 2001);
  for (let k = 1; at + 150 <= ms; k++) at += 3000 + Math.floor(hash(seed, k) * 2001);
  return ms >= at;
}

/** A Faces effect's colors for one prop's nodes at `ms` (node → color). */
function faceColors(effect: Effect, doc: Sequence, prop: Prop, ms: number): Map<number, Rgb> {
  const p = effect.params as Extract<Effect["params"], { kind: "faces" }>;
  const lit = new Map<number, Rgb>();
  const wanted = (p.face ?? "").trim().toLowerCase();
  const region = facesOf(prop).find((r) => wanted === "" || r.name.trim().toLowerCase() === wanted);
  if (!region || region.kind !== "face") return lit;
  const face: FaceDefinition = region;
  const phoneme = phonemeAt(doc.timingTracks.find((t) => t.id === p.timingTrack), ms);
  const eyes = p.eyes ?? "auto";
  const closed = eyes === "closed" || (eyes === "auto" && blinking(idSeed(effect.id), effect.startMs, ms));
  for (const { part, ranges } of faceParts(face, phoneme, closed, p.outline ?? false)) {
    const color = parseColor(facePartColor(face, part, phoneme, closed, (p.colors ?? "face") === "face", effect.palette.colors));
    for (const r of ranges) for (let n = r.start; n < r.end; n++) lit.set(n, color);
  }
  return lit;
}

/** How much an effect's fades leave of it at `ms` (0–1). */
function fadeLevel(effect: Effect, ms: number): number {
  let level = 1;
  if (effect.fadeInMs > 0) level = Math.min(level, (ms - effect.startMs) / effect.fadeInMs);
  if (effect.fadeOutMs > 0) level = Math.min(level, (effect.endMs - ms) / effect.fadeOutMs);
  return Math.max(0, Math.min(1, level));
}

/** The sequence at `ms` as show frame bytes (prop order, like the engine's layout). */
export function renderSequenceFrame(doc: Sequence, show: Show, ms: number): Uint8Array {
  const layout = new Map<string, { prop: Prop; offset: number; nodes: number; cpp: number; points: number[] }>();
  let length = 0;
  for (const prop of show.props) {
    const nodes = nodeCount(prop.shape);
    const cpp = channelsPerPixel(prop);
    layout.set(prop.id, { prop, offset: length, nodes, cpp, points: frontView(prop) });
    length += nodes * cpp;
  }
  const frame = new Uint8Array(length);
  if (ms < 0 || ms >= doc.durationMs) return frame;
  for (const row of doc.rows) {
    const active = row.layers.map((layer) => {
      const effect = layer.effects.find((e) => e.startMs <= ms && ms < e.endMs);
      // Settings that change over the effect, at this moment.
      return effect ? effectAt(effect, ms) : null;
    });
    if (active.every((e) => e === null)) continue;
    // The pixels the row lights, prop by prop in order along the target (submodels light some of a prop's).
    const lights = targetNodes(show, row.target, (prop) => layout.get(prop.id)?.nodes ?? 0, (prop) => layout.get(prop.id)?.points ?? []);
    const props = lights
      .map(({ prop: id, nodes }) => {
        const p = layout.get(id);
        return p ? { ...p, list: nodes === "all" ? Array.from({ length: p.nodes }, (_, k) => k) : nodes } : undefined;
      })
      .filter((p) => p !== undefined);
    let [minX, minY, maxX, maxY] = [Infinity, Infinity, -Infinity, -Infinity];
    for (const p of props) {
      for (const k of p.list) {
        if (2 * k + 1 >= p.points.length) continue;
        minX = Math.min(minX, p.points[2 * k]);
        maxX = Math.max(maxX, p.points[2 * k]);
        minY = Math.min(minY, p.points[2 * k + 1]);
        maxY = Math.max(maxY, p.points[2 * k + 1]);
      }
    }
    const total = props.reduce((n, p) => n + p.list.length, 0);
    const faces = new Map(active.map((e) => [e, e?.params.kind === "faces" ? new Map(props.map((p) => [p.prop.id, faceColors(e, doc, p.prop, ms)])) : null]));
    let index = 0;
    for (const p of props) {
      for (const k of p.list) {
        index++;
        const [x, y] = [p.points[k * 2] ?? 0, p.points[k * 2 + 1] ?? 0];
        const px: Px = {
          u: maxX > minX ? (x - minX) / (maxX - minX) : 0.5,
          v: maxY > minY ? (y - minY) / (maxY - minY) : 0.5,
          i: index - 1,
          n: total,
          aspect: maxX > minX && maxY > minY ? (maxX - minX) / (maxY - minY) : 1,
        };
        let [r, g, b, a] = [0, 0, 0, 0];
        active.forEach((effect, layer) => {
          if (!effect) return;
          const face = faces.get(effect)?.get(p.prop.id);
          const lit = face?.get(k);
          const [rgb, coverage] = face ? (lit ? [lit, 1] : [[0, 0, 0] as Rgb, 0]) : shade(effect, ms, px, hash(layer, effect.id.length + effect.startMs), doc.timingTracks);
          const alpha = coverage * fadeLevel(effect, ms);
          if (alpha <= 0) return;
          r = r * (1 - alpha) + rgb[0] * alpha;
          g = g * (1 - alpha) + rgb[1] * alpha;
          b = b * (1 - alpha) + rgb[2] * alpha;
          a = a + alpha * (1 - a);
        });
        if (a <= 0) continue;
        const at = p.offset + k * p.cpp;
        frame[at] = Math.round(frame[at] * (1 - a) + r);
        frame[at + 1] = Math.round(frame[at + 1] * (1 - a) + g);
        frame[at + 2] = Math.round(frame[at + 2] * (1 - a) + b);
      }
    }
  }
  return frame;
}
