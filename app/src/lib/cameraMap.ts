// Camera mapping in the window: the sequence's shape (mirroring crates/pf-camera-map, for the
// demo capture and progress), plain-language anomalies, and turning a plan into show edits.

import type { CameraMapAnomaly, CameraMapPlan, CameraMapProp, CodeBase, Edit, PropPlan, Prop, Show } from "../api/types";

/** Slot length in seconds (pf_camera_map::DEFAULT_SLOT_SECONDS). */
export const SLOT_SECONDS = 0.5;
/** Dark/white sync slots at the start of every pass. */
const PREAMBLE = [false, true, false, true, true, false];
const CHECKS = 2;

export type Symbol = "off" | "red" | "green" | "blue" | "white";

const radix = (base: CodeBase) => (base === "four" ? 4 : 2);

/** Digits needed for every pixel's number (index + 1, up to `pixels`). */
export function codeDigits(pixels: number, base: CodeBase): number {
  let digits = 1;
  for (let reach = radix(base); reach <= pixels; reach *= radix(base)) digits++;
  return digits;
}

/** Slots in one pass: preamble, three colour references, the digits, check digits, a dark tail. */
export function slotCount(pixels: number, base: CodeBase): number {
  return PREAMBLE.length + 3 + codeDigits(pixels, base) + CHECKS + 1;
}

/** One pass of the sequence, in seconds. */
export function sequenceSeconds(pixels: number, base: CodeBase): number {
  return slotCount(pixels, base) * SLOT_SECONDS;
}

/** What pixel `index` shows during slot `slot` of a pass. */
export function symbolAt(pixels: number, base: CodeBase, slot: number, index: number): Symbol {
  const r = radix(base);
  const n = codeDigits(pixels, base);
  const number = Array.from({ length: n }, (_, i) => Math.floor((index + 1) / r ** i) % r);
  const checks = [number.reduce((a, d) => a + d, 0) % r, number.reduce((a, d, i) => a + (i + 1) * d, 0) % r];
  const show = (d: number): Symbol => (d === 0 ? "off" : base === "two" ? "white" : (["red", "green", "blue"] as const)[d - 1]);
  if (slot < PREAMBLE.length) return PREAMBLE[slot] ? "white" : "off";
  slot -= PREAMBLE.length;
  if (slot < 3) return (["red", "green", "blue"] as const)[slot];
  slot -= 3;
  if (slot < n) return show(number[slot]);
  slot -= n;
  if (slot < CHECKS) return show(checks[slot]);
  return "off";
}

/** The colour order a prop should have, given the colours the camera saw for red, green, blue. */
export function correctedColorOrder(configured: string, seen: [number, number, number]): string {
  const rgb = "RGB";
  return [...configured].map((ch) => (rgb.includes(ch) ? rgb[seen[rgb.indexOf(ch)] % 3] : ch)).join("");
}

/** Node ranges as people count them (from 1): "4", "10–12". */
function nodes(ranges: [number, number][]): string {
  const shown = ranges.slice(0, 4).map(([a, b]) => (a === b ? `${a + 1}` : `${a + 1}–${b + 1}`));
  return shown.join(", ") + (ranges.length > 4 ? `, and ${ranges.length - 4} more` : "");
}

/** A sentence about something worth checking in a capture. */
export function describeAnomaly(anomaly: CameraMapAnomaly, names: string[]): string {
  const name = "prop" in anomaly ? (names[anomaly.prop] ?? "A prop") : "";
  switch (anomaly.kind) {
    case "missing": {
      const count = anomaly.ranges.reduce((n, [a, b]) => n + b - a + 1, 0);
      return `${name}: ${count === 1 ? "pixel" : `${count} pixels`} ${nodes(anomaly.ranges)} never lit up in the video. They may be dead, hidden from the camera, or wired somewhere else. Their places are filled in between their neighbours.`;
    }
    case "duplicate":
      return `${name}: pixel ${anomaly.node + 1} was seen twice; the dimmer one (a reflection?) is ignored.`;
    case "reversed":
      return `${name}: the pixels run the other way from the layout — the string may start at the other end. The measured shape follows the real wiring.`;
    case "jump":
      return `${name}: pixel ${anomaly.node + 1} is far from the pixel before it. It may be wired out of order, or misread.`;
    case "colorOrder":
      return `${name}: colours came out wrong (red showed as another colour). Its colour order looks like ${anomaly.suggested}, not ${anomaly.configured}.`;
    case "unreadable":
      return `${anomaly.count} lit ${anomaly.count === 1 ? "spot" : "spots"} didn't read as any pixel (a light that flickered, or two pixels too close together to tell apart).`;
  }
}

/** What to do with a prop's capture. */
export type ApplyChoice = "measured" | "fit" | "skip";

/** Whether the prop's own shape can be kept and just moved onto the measured pixels. */
export function canFit(prop: Prop, plan: PropPlan): boolean {
  return (
    prop.shape.source === "generator" &&
    !!plan.fit?.fits &&
    prop.transform.rotationDeg.x === 0 &&
    prop.transform.rotationDeg.y === 0
  );
}

/** Whether the capture lit every node of the prop, so its measured shape is complete. */
export function canMeasure(entry: CameraMapProp, plan: PropPlan): boolean {
  return plan.found > 0 && entry.covered >= entry.nodes;
}

/** The choice offered first: keep the shape when it fits, else the measured points. */
export function defaultChoice(prop: Prop, entry: CameraMapProp, plan: PropPlan): ApplyChoice {
  if (canFit(prop, plan)) return "fit";
  return canMeasure(entry, plan) ? "measured" : "skip";
}

/**
 * The edits that place each chosen prop (one undo step when applied together). "measured" sets a
 * camera-measured shape centred on the prop's position; "fit" keeps the shape and moves, turns,
 * and scales it onto the pixels.
 */
export function placementEdits(show: Show, result: CameraMapPlan, choices: Record<string, ApplyChoice>): Edit[] {
  const edits: Edit[] = [];
  result.props.forEach((entry, i) => {
    const prop = show.props.find((p) => p.id === entry.prop);
    const plan = result.plan.props[i];
    const choice = choices[entry.prop] ?? "skip";
    if (!prop || !plan || plan.points.length !== plan.nodes || choice === "skip") return;
    if (choice === "measured" && !canMeasure(entry, plan)) return;
    if (choice === "fit" && plan.fit && canFit(prop, plan)) {
      const { scale, rotationDeg, tx, ty } = plan.fit;
      const a = (rotationDeg * Math.PI) / 180;
      const { x, y, z } = prop.transform.position;
      edits.push({
        type: "updateProp",
        prop: {
          ...prop,
          transform: {
            position: { x: scale * (Math.cos(a) * x - Math.sin(a) * y) + tx, y: scale * (Math.sin(a) * x + Math.cos(a) * y) + ty, z },
            rotationDeg: { ...prop.transform.rotationDeg, z: prop.transform.rotationDeg.z + rotationDeg },
            scale: { x: prop.transform.scale.x * scale, y: prop.transform.scale.y * scale, z: prop.transform.scale.z * scale },
          },
        },
      });
      return;
    }
    const cx = plan.points.reduce((s, p) => s + p[0], 0) / plan.points.length;
    const cy = plan.points.reduce((s, p) => s + p[1], 0) / plan.points.length;
    edits.push({
      type: "updateProp",
      prop: {
        ...prop,
        shape: { source: "measured", provenance: "cameraMap", points: plan.points.map(([x, y]) => ({ x: x - cx, y: y - cy, z: 0 })) },
        transform: { position: { x: cx, y: cy, z: prop.transform.position.z }, rotationDeg: { x: 0, y: 0, z: 0 }, scale: { x: 1, y: 1, z: 1 } },
      },
    });
  });
  return edits;
}

