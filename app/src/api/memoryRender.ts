// A rough, browser-only drawing of an authored sequence for the in-memory stand-in (tests and the
// `?demo` page). The real renderer is pf-render in the engine; this only has to look plausible:
// each effect kind gets a simple look of its own, layers mix bottom to top, later rows cover
// earlier ones, and fades dim.

import { frontView } from "../lib/geometry";
import { channelsPerPixel, nodeCount } from "../lib/shows";
import { facePartColor, faceParts, facesOf, phonemeAt, targetNodes } from "../lib/submodels";
import { effectAt } from "../lib/curves";
import type { Effect, Sequence } from "./sequence";
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
}

function num(params: Record<string, unknown>, key: string, fallback: number): number {
  const v = params[key];
  return typeof v === "number" && Number.isFinite(v) ? v : fallback;
}

/** One effect's color and coverage (0–1) at one pixel. */
function shade(effect: Effect, ms: number, px: Px, seed: number): [Rgb, number] {
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
      const s = (x - pos) * bands;
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
        };
        let [r, g, b, a] = [0, 0, 0, 0];
        active.forEach((effect, layer) => {
          if (!effect) return;
          const face = faces.get(effect)?.get(p.prop.id);
          const lit = face?.get(k);
          const [rgb, coverage] = face ? (lit ? [lit, 1] : [[0, 0, 0] as Rgb, 0]) : shade(effect, ms, px, hash(layer, effect.id.length + effect.startMs));
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
