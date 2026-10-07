// Camera mapping for the in-memory backend: which node each sequence index lights, a made-up
// capture to read (the demo's stand-in for a phone video), and a simple plan from it. The real
// decoding and planning are in crates/pf-camera-map.

import type { CameraMapAnomaly, CameraMapPlan, CodeBase, DecodedCapture, FoundPixel, Prop, PropPlan, Show, TargetSpec } from "./types";
import type { FrameSource } from "../lib/captureFrames";
import { correctedColorOrder, sequenceSeconds, slotCount, symbolAt, SLOT_SECONDS } from "../lib/cameraMap";
import { frontView } from "../lib/geometry";
import { memberProp, nodeCount } from "../lib/shows";

export interface Owner {
  prop: Prop;
  node: number;
}

/** The nodes `target` lights, in the order the camera-mapping pattern numbers them. */
export function cameraOwners(show: Show, target: TargetSpec): Owner[] {
  const whole = (prop: Prop | undefined): Owner[] =>
    prop ? Array.from({ length: nodeCount(prop.shape) }, (_, node) => ({ prop, node })) : [];
  const byId = (id: string) => show.props.find((p) => p.id === id);
  switch (target.type) {
    case "show":
      return show.props.flatMap(whole);
    case "prop":
      return whole(byId(target.id));
    case "group":
      return (show.groups.find((g) => g.id === target.id)?.members ?? []).flatMap((m) => whole(byId(memberProp(m))));
    case "controller":
    case "port": {
      const controller = show.controllers.find((c) => c.id === (target.type === "port" ? target.controller : target.id));
      const ports = (controller?.ports ?? []).filter((p) => target.type !== "port" || p.number === target.port);
      return ports.flatMap((p) =>
        p.slots.flatMap((slot) => {
          const owners = whole(byId(slot.prop));
          return slot.reverse ? owners.reverse() : owners;
        }),
      );
    }
  }
}

/** The props `owners` cover, in order of first appearance. */
export function ownerProps(owners: Owner[]): Prop[] {
  return [...new Set(owners.map((o) => o.prop))];
}

/** The demo's capture: a video to read, and what decoding it gives. */
export interface SampleCapture {
  source: FrameSource;
  /** Where the sequence starts in the video, seconds. */
  start: number;
  decoded: DecodedCapture;
  /** From the decoded (analysis-size) frame to layout units. */
  toLayout: (x: number, y: number) => [number, number];
}

const PRE_ROLL = 1.4;
const SOURCE = { width: 1920, height: 1080 };
const ANALYSIS = 960 / SOURCE.width;
const COLORS: Record<string, [number, number, number]> = {
  red: [255, 50, 35],
  green: [60, 255, 90],
  blue: [40, 90, 255],
  white: [255, 250, 240],
};

/**
 * A made-up phone video of `owners` flashing the sequence, filmed slightly tilted, with a few
 * things to find: a dead pixel and a reflection on the first prop, the second prop's red and
 * green swapped, and a last prop that isn't the shape the layout says.
 */
export function sampleCapture(owners: Owner[], base: CodeBase): SampleCapture {
  const pixels = owners.length;
  const props = ownerProps(owners);
  const layout = owners.map((o) => {
    const view = frontView(o.prop);
    return [view[2 * o.node] ?? 0, view[2 * o.node + 1] ?? 0] as [number, number];
  });
  const xs = layout.map((p) => p[0]);
  const ys = layout.map((p) => p[1]);
  const [cx, cy] = [(Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2];
  const scale = Math.min(1500 / Math.max(1e-6, Math.max(...xs) - Math.min(...xs)), 760 / Math.max(1e-6, Math.max(...ys) - Math.min(...ys)));
  const tilt = (2 * Math.PI) / 180;
  const [s, c] = [Math.sin(tilt), Math.cos(tilt)];
  const toImage = ([x, y]: [number, number]): [number, number] => {
    const [dx, dy] = [x - cx, y - cy];
    return [960 + scale * (c * dx - s * dy), 560 - scale * (s * dx + c * dy)];
  };
  const toLayout = (ax: number, ay: number): [number, number] => {
    const [dx, dy] = [(ax / ANALYSIS - 960) / scale, -(ay / ANALYSIS - 560) / scale];
    return [cx + c * dx + s * dy, cy - s * dx + c * dy];
  };
  // The last prop (of three or more) isn't quite the shape the layout has: it sags.
  const sagging = props.length > 2 ? props[props.length - 1] : null;
  const truth = layout.map((p, i): [number, number] => {
    const [x, y] = toImage(p);
    return owners[i].prop === sagging ? [x, y + 14 * Math.sin(owners[i].node * 0.7)] : [x, y];
  });
  const first = owners.filter((o) => o.prop === props[0]).map((_, i) => i);
  const dead = first.length > 12 ? first[10] : -1;
  const reflected = first.slice(0, 3);
  const swapped = new Set(owners.flatMap((o, i) => (props.length > 1 && o.prop === props[1] ? [i] : [])));

  const duration = PRE_ROLL + sequenceSeconds(pixels, base) * 1.3 + 1;
  const slots = slotCount(pixels, base);
  const source: FrameSource = {
    duration,
    ...SOURCE,
    grab: async (t, width, height) => {
      const k = width / SOURCE.width;
      const out = new Uint8ClampedArray(width * height * 4);
      for (let y = 0; y < height; y++) {
        const shade = 6 + (14 * y) / height;
        for (let x = 0; x < width; x++) {
          const p = (y * width + x) * 4;
          out[p] = shade;
          out[p + 1] = shade + 2;
          out[p + 2] = shade + 8;
          out[p + 3] = 255;
        }
      }
      const since = t - PRE_ROLL;
      if (since < 0) return out;
      const slot = Math.floor(since / SLOT_SECONDS) % slots;
      const splat = (x: number, y: number, rgb: [number, number, number], gain: number) => {
        const [px, py, r] = [x * k, y * k, Math.max(0.8, 2.4 * k)];
        const reach = Math.ceil(r * 3);
        for (let yy = Math.max(0, Math.round(py) - reach); yy <= Math.min(height - 1, Math.round(py) + reach); yy++) {
          for (let xx = Math.max(0, Math.round(px) - reach); xx <= Math.min(width - 1, Math.round(px) + reach); xx++) {
            const f = gain * Math.exp(-((xx - px) ** 2 + (yy - py) ** 2) / (2 * r * r));
            const p = (yy * width + xx) * 4;
            out[p] += rgb[0] * f;
            out[p + 1] += rgb[1] * f;
            out[p + 2] += rgb[2] * f;
          }
        }
      };
      truth.forEach(([x, y], i) => {
        if (i === dead) return;
        let symbol = symbolAt(pixels, base, slot, i);
        if (swapped.has(i) && (symbol === "red" || symbol === "green")) symbol = symbol === "red" ? "green" : "red";
        if (symbol === "off") return;
        splat(x, y, COLORS[symbol], 1.4);
        if (reflected.includes(i)) splat(x + 4, y + 150, COLORS[symbol], 0.35);
      });
      return out;
    },
    dispose: () => undefined,
  };

  const found = (i: number, [x, y]: [number, number], seen: FoundPixel["seen"]): FoundPixel => ({
    index: i,
    x: x * ANALYSIS,
    y: y * ANALYSIS,
    confidence: 0.75 + 0.2 * (((i * 7919) % 13) / 13),
    brightness: 180,
    seen,
  });
  const decoded: DecodedCapture = {
    width: SOURCE.width * ANALYSIS,
    height: SOURCE.height * ANALYSIS,
    pixels: truth.flatMap((p, i) => (i === dead ? [] : [found(i, p, swapped.has(i) ? [1, 0, 2] : [0, 1, 2])])),
    duplicates: reflected.map((i) => found(i, [truth[i][0] + 4, truth[i][1] + 150], [0, 1, 2])),
    unreadable: [
      { x: 140, y: 420, brightness: 30 },
      { x: 820, y: 80, brightness: 25 },
    ],
  };
  return { source, start: PRE_ROLL, decoded, toLayout };
}

/** A simple plan in the browser: the known camera (or the video scaled to the layout) instead of
 * a fit, and the anomalies that are easy to see. */
export function planInMemory(
  owners: Owner[],
  decoded: DecodedCapture,
  toLayout: ((x: number, y: number) => [number, number]) | null,
): CameraMapPlan {
  const props = ownerProps(owners);
  const place =
    toLayout ??
    ((x: number, y: number): [number, number] => {
      const k = 10 / Math.max(1, decoded.height);
      return [k * (x - decoded.width / 2), k * (decoded.height / 2 - y)];
    });
  const anomalies: CameraMapAnomaly[] = [];
  const plans: PropPlan[] = props.map((prop, k) => {
    const nodes = nodeCount(prop.shape);
    const placed: ([number, number] | null)[] = Array.from({ length: nodes }, () => null);
    for (const f of decoded.pixels) {
      const o = owners[f.index];
      if (o?.prop === prop) placed[o.node] = place(f.x, f.y);
    }
    const missing: [number, number][] = [];
    placed.forEach((p, i) => {
      if (p) return;
      const last = missing[missing.length - 1];
      if (last && last[1] === i - 1) last[1] = i;
      else missing.push([i, i]);
    });
    if (missing.length) anomalies.push({ kind: "missing", prop: k, ranges: missing });
    const known = placed.flatMap((p, i) => (p ? [[i, p] as const] : []));
    const points = known.length
      ? placed.map((p, i): [number, number] => {
          if (p) return p;
          const before = [...known].reverse().find(([j]) => j < i);
          const after = known.find(([j]) => j > i);
          if (before && after) {
            const f = (i - before[0]) / (after[0] - before[0]);
            return [before[1][0] + (after[1][0] - before[1][0]) * f, before[1][1] + (after[1][1] - before[1][1]) * f];
          }
          return (before ?? after)![1];
        })
      : [];
    const view = frontView(prop);
    const shift = known.reduce((s, [i, p]) => [s[0] + p[0] - view[2 * i], s[1] + p[1] - view[2 * i + 1]], [0, 0]).map((v) => v / Math.max(1, known.length));
    const size = Math.max(1e-6, ...known.map(([, p]) => Math.hypot(p[0] - points[0][0], p[1] - points[0][1])));
    const error = Math.sqrt(known.reduce((s, [i, p]) => s + (p[0] - view[2 * i] - shift[0]) ** 2 + (p[1] - view[2 * i + 1] - shift[1]) ** 2, 0) / Math.max(1, known.length)) / size;
    return {
      nodes,
      found: known.length,
      points,
      measured: placed.map((p) => p !== null),
      fit: known.length >= 3 ? { scale: 1, rotationDeg: 0, tx: shift[0], ty: shift[1], error, fits: error < 0.06 } : null,
    };
  });
  for (const f of decoded.duplicates) {
    const o = owners[f.index];
    if (o) anomalies.push({ kind: "duplicate", prop: props.indexOf(o.prop), node: o.node, x: f.x, y: f.y });
  }
  props.forEach((prop, k) => {
    const seen = decoded.pixels.filter((f) => owners[f.index]?.prop === prop && f.seen && f.seen.join() !== "0,1,2");
    const total = decoded.pixels.filter((f) => owners[f.index]?.prop === prop).length;
    if (seen.length >= 2 && seen.length * 5 >= total * 3) {
      const suggested = correctedColorOrder(prop.colorOrder, seen[0].seen!);
      if (suggested !== prop.colorOrder) anomalies.push({ kind: "colorOrder", prop: k, configured: prop.colorOrder, suggested });
    }
  });
  if (decoded.unreadable.length) anomalies.push({ kind: "unreadable", count: decoded.unreadable.length });
  return {
    props: props.map((p) => ({ prop: p.id, name: p.name, nodes: nodeCount(p.shape) })),
    plan: { alignment: null, alignmentError: 0, props: plans, anomalies },
  };
}
