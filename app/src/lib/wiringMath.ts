// Pure logic for the Wiring screen: placing, moving, and removing props on controller ports
// (each gesture becomes one batch of edits: one undo step), how full each port is, which props
// aren't wired (or are wired twice), each port's channels, and the path its data takes through
// the layout. Nothing here touches the DOM, so it is all unit-tested.
//
// It follows the engine's channel mapping (crates/pf-mapping): a slot whose prop is gone, or
// whose pixel range doesn't fit the prop, carries nothing.

import type { ChannelMap, Controller, Edit, NodeRange, Port, PortSlot, PreviewProp, Show } from "../api/types";
import { plural, thousands } from "./format";
import type { Pt } from "./layoutMath";

/** A port on a controller. */
export interface PortRef {
  controller: string;
  port: number;
}

/** A position on a port: the slot there, or (for a drop) where a slot goes. */
export interface SlotRef extends PortRef {
  index: number;
}

/** Pixels in each prop, by id. */
export type NodeCounts = ReadonlyMap<string, number>;

export function nodeCounts(map: ChannelMap): Map<string, number> {
  return new Map(map.props.map((p) => [p.prop, p.nodes]));
}

/** A slot carrying the prop (or `segment` of it) with no overrides. */
export function blankSlot(prop: string, segment: NodeRange | null = null): PortSlot {
  return { prop, segment, nullPixels: 0, reverse: false, brightness: null, gamma: null, smartReceiver: null };
}

/** The prop pixels a slot carries; null when its prop is gone or its range doesn't fit. */
export function slotRange(slot: PortSlot, nodes: NodeCounts): NodeRange | null {
  const count = nodes.get(slot.prop);
  if (count === undefined) return null;
  const range = slot.segment ?? { start: 0, end: count };
  return range.start <= range.end && range.end <= count ? range : null;
}

/** "Arch", or "Arch · 1–25" when the slot carries part of the prop (pixels counted from 1). */
export function slotLabel(name: string, slot: PortSlot): string {
  return slot.segment ? `${name} · ${slot.segment.start + 1}–${slot.segment.end}` : name;
}

// ---- Capacity -----------------------------------------------------------------------------

/** From this share of a port's limit on, the port counts as nearly full. */
export const NEARLY_FULL = 0.9;

/** Physical pixels on the port, null pixels included. */
export function portPixels(port: Port, nodes: NodeCounts): number {
  let used = 0;
  for (const slot of port.slots) {
    const range = slotRange(slot, nodes);
    if (range) used += slot.nullPixels + range.end - range.start;
  }
  return used;
}

export type CapacityLevel = "none" | "ok" | "near" | "over";

export interface Capacity {
  used: number;
  limit: number | null;
  /** "none": the port's limit isn't known. */
  level: CapacityLevel;
  message: string | null;
}

export function portCapacity(port: Port, nodes: NodeCounts): Capacity {
  const used = portPixels(port, nodes);
  const limit = port.maxPixels;
  if (limit === null) return { used, limit, level: "none", message: null };
  if (used > limit) {
    return {
      used,
      limit,
      level: "over",
      message: `${plural(used - limit, "pixel")} more than this port can drive (${thousands(used)} of ${thousands(limit)}). Move a prop to another port.`,
    };
  }
  if (used >= limit * NEARLY_FULL) return { used, limit, level: "near", message: `Nearly full: ${thousands(used)} of ${thousands(limit)} pixels.` };
  return { used, limit, level: "ok", message: null };
}

// ---- Edits --------------------------------------------------------------------------------

/** The controller with one port changed; null when the controller or port isn't there. */
function changePort(show: Show, ref: PortRef, change: (port: Port) => Port): Controller | null {
  const controller = show.controllers.find((c) => c.id === ref.controller);
  if (!controller || !controller.ports.some((p) => p.number === ref.port)) return null;
  return { ...controller, ports: controller.ports.map((p) => (p.number === ref.port ? change(p) : p)) };
}

const update = (controller: Controller | null): Edit[] => (controller ? [{ type: "updateController", controller }] : []);

function slotAt(show: Show, ref: SlotRef): PortSlot | null {
  const port = show.controllers.find((c) => c.id === ref.controller)?.ports.find((p) => p.number === ref.port);
  return port?.slots[ref.index] ?? null;
}

const inserted = (slots: PortSlot[], index: number, slot: PortSlot) => [...slots.slice(0, index), slot, ...slots.slice(index)];

/** Moves the slot at `from` to sit before position `to.index` (counted before the move). */
export function moveSlotEdits(show: Show, from: SlotRef, to: SlotRef): Edit[] {
  const slot = slotAt(show, from);
  if (!slot) return [];
  if (from.controller === to.controller && from.port === to.port) {
    if (to.index === from.index || to.index === from.index + 1) return [];
    const index = to.index > from.index ? to.index - 1 : to.index;
    return update(
      changePort(show, from, (p) => {
        const rest = p.slots.filter((_, i) => i !== from.index);
        return { ...p, slots: inserted(rest, index, slot) };
      }),
    );
  }
  const removed = changePort(show, from, (p) => ({ ...p, slots: p.slots.filter((_, i) => i !== from.index) }));
  if (!removed) return [];
  // Within one controller both changes go in one update; across two, one each.
  const base = from.controller === to.controller ? { ...show, controllers: show.controllers.map((c) => (c.id === removed.id ? removed : c)) } : show;
  const added = changePort(base, to, (p) => ({ ...p, slots: inserted(p.slots, Math.min(to.index, p.slots.length), slot) }));
  if (!added) return [];
  return from.controller === to.controller ? update(added) : [...update(removed), ...update(added)];
}

/**
 * Wires the prop at `to`, dragged from the props list: an unwired prop is wired whole; a partly
 * wired one gets a slot for its first pixels that aren't wired yet; a prop wired by one slot is
 * moved here. A prop already wired in several pieces is left alone (move its pieces instead).
 */
export function wirePropEdits(show: Show, prop: string, to: SlotRef, nodes: NodeCounts): Edit[] {
  const wiring = propWiring(show, nodes).get(prop);
  if (!wiring) return [];
  if (wiring.status === "unwired") return update(changePort(show, to, (p) => ({ ...p, slots: inserted(p.slots, Math.min(to.index, p.slots.length), blankSlot(prop)) })));
  const gap = firstGap(
    wiring.places.map((place) => slotRange(place.slot, nodes)).filter((r): r is NodeRange => r !== null),
    wiring.nodes,
  );
  if (gap) return update(changePort(show, to, (p) => ({ ...p, slots: inserted(p.slots, Math.min(to.index, p.slots.length), blankSlot(prop, gap)) })));
  if (wiring.places.length === 1) {
    const [place] = wiring.places;
    return moveSlotEdits(show, { controller: place.controller, port: place.port, index: place.index }, to);
  }
  return [];
}

export function unwireEdits(show: Show, ref: SlotRef): Edit[] {
  if (!slotAt(show, ref)) return [];
  return update(changePort(show, ref, (p) => ({ ...p, slots: p.slots.filter((_, i) => i !== ref.index) })));
}

export function updateSlotEdits(show: Show, ref: SlotRef, change: (slot: PortSlot) => PortSlot): Edit[] {
  if (!slotAt(show, ref)) return [];
  return update(changePort(show, ref, (p) => ({ ...p, slots: p.slots.map((s, i) => (i === ref.index ? change(s) : s)) })));
}

export function updatePortEdits(show: Show, ref: PortRef, change: (port: Port) => Port): Edit[] {
  return update(changePort(show, ref, change));
}

/** A new empty port numbered after the last, with the pixel limit the other ports share (if any). */
export function addPortEdits(show: Show, controllerId: string): Edit[] {
  const controller = show.controllers.find((c) => c.id === controllerId);
  if (!controller) return [];
  const number = controller.ports.reduce((max, p) => Math.max(max, p.number), 0) + 1;
  const limits = new Set(controller.ports.map((p) => p.maxPixels));
  const maxPixels = limits.size === 1 ? [...limits][0] : null;
  return update({ ...controller, ports: [...controller.ports, { number, maxPixels, brightness: 100, gamma: 1, slots: [] }] });
}

export function removePortEdits(show: Show, ref: PortRef): Edit[] {
  const controller = show.controllers.find((c) => c.id === ref.controller);
  if (!controller?.ports.some((p) => p.number === ref.port)) return [];
  return update({ ...controller, ports: controller.ports.filter((p) => p.number !== ref.port) });
}

/** Gives the port a new number (as printed on the controller); ports stay in number order. A
 * number another port already has is refused. */
export function renumberPortEdits(show: Show, ref: PortRef, number: number): Edit[] {
  const controller = show.controllers.find((c) => c.id === ref.controller);
  if (!controller || number === ref.port || controller.ports.some((p) => p.number === number)) return [];
  const renumbered = changePort(show, ref, (p) => ({ ...p, number }));
  return update(renumbered && { ...renumbered, ports: [...renumbered.ports].sort((a, b) => a.number - b.number) });
}

/** Appends the props, whole and in the given order, to the end of the port. */
export function wireRemainingEdits(show: Show, ref: PortRef, props: string[]): Edit[] {
  if (props.length === 0) return [];
  return update(changePort(show, ref, (p) => ({ ...p, slots: [...p.slots, ...props.map((id) => blankSlot(id))] })));
}

// ---- Where each prop is wired ------------------------------------------------------------

export interface Place {
  controller: string;
  controllerName: string;
  port: number;
  index: number;
  slot: PortSlot;
}

export type WiringStatus = "unwired" | "wired" | "partial" | "twice";

export interface PropWiring {
  status: WiringStatus;
  places: Place[];
  /** Pixels at least one slot carries. */
  wiredPixels: number;
  nodes: number;
}

/** The first pixels (counting from the start) that none of `ranges` carries; null when all are. */
export function firstGap(ranges: NodeRange[], nodes: number): NodeRange | null {
  let at = 0;
  for (const r of [...ranges].sort((a, b) => a.start - b.start)) {
    if (r.start > at) return { start: at, end: r.start };
    at = Math.max(at, r.end);
  }
  return at < nodes ? { start: at, end: nodes } : null;
}

/** Every prop's wiring: where it's wired, how much of it, and whether any pixel is wired twice. */
export function propWiring(show: Show, nodes: NodeCounts): Map<string, PropWiring> {
  const places = new Map<string, Place[]>(show.props.map((p) => [p.id, []]));
  for (const c of show.controllers) {
    for (const port of c.ports) {
      port.slots.forEach((slot, index) => places.get(slot.prop)?.push({ controller: c.id, controllerName: c.name, port: port.number, index, slot }));
    }
  }
  const result = new Map<string, PropWiring>();
  for (const prop of show.props) {
    const count = nodes.get(prop.id) ?? 0;
    const here = places.get(prop.id) ?? [];
    const ranges = here
      .map((p) => slotRange(p.slot, nodes))
      .filter((r): r is NodeRange => r !== null && r.end > r.start)
      .sort((a, b) => a.start - b.start);
    let [covered, reach, twice] = [0, 0, false];
    for (const r of ranges) {
      if (r.start < reach) twice = true;
      covered += Math.max(0, r.end - Math.max(r.start, reach));
      reach = Math.max(reach, r.end);
    }
    const status: WiringStatus = covered === 0 ? "unwired" : twice ? "twice" : covered < count ? "partial" : "wired";
    if (!result.has(prop.id)) result.set(prop.id, { status, places: here, wiredPixels: covered, nodes: count });
  }
  return result;
}

/** Where a prop sits left to right: its leftmost pixel, or its position before it has pixels. */
function leftEdge(show: Show, preview: PreviewProp[], id: string): number {
  const points = preview.find((p) => p.prop === id)?.points;
  if (points && points.length >= 2) {
    let min = Infinity;
    for (let i = 0; i < points.length; i += 2) min = Math.min(min, points[i]);
    return min;
  }
  return show.props.find((p) => p.id === id)?.transform.position.x ?? 0;
}

/** The props not wired anywhere yet, left to right as they sit in the layout. */
export function unwiredInLayoutOrder(show: Show, preview: PreviewProp[], wiring: Map<string, PropWiring>): string[] {
  return show.props
    .filter((p) => wiring.get(p.id)?.status === "unwired")
    .map((p) => ({ id: p.id, x: leftEdge(show, preview, p.id) }))
    .sort((a, b) => a.x - b.x)
    .map((p) => p.id);
}

// ---- Problems ----------------------------------------------------------------------------

export interface WiringProblem {
  message: string;
  fix: string;
}

export function wiringProblems(show: Show, nodes: NodeCounts): WiringProblem[] {
  const problems: WiringProblem[] = [];
  const name = (id: string) => show.props.find((p) => p.id === id)?.name ?? "A prop";
  for (const c of show.controllers) {
    for (const port of c.ports) {
      const capacity = portCapacity(port, nodes);
      if (capacity.level === "over" && capacity.limit !== null) {
        problems.push({
          message: `Port ${port.number} on ${c.name} has ${plural(capacity.used - capacity.limit, "pixel")} more than it can drive (${thousands(capacity.used)} of ${thousands(capacity.limit)}).`,
          fix: "Move a prop to another port, or raise the port's pixel limit if the controller can drive more.",
        });
      }
      let missing = false;
      for (const slot of port.slots) {
        const count = nodes.get(slot.prop);
        if (count === undefined) {
          missing = true;
          continue;
        }
        if (!slotRange(slot, nodes) && slot.segment) {
          const n = name(slot.prop);
          problems.push({
            message: `${n} on port ${port.number} of ${c.name} uses pixels ${slot.segment.start + 1}–${slot.segment.end}, but ${n} only has ${thousands(count)}.`,
            fix: "Change its pixel range in the slot settings.",
          });
        }
      }
      if (missing) problems.push({ message: `Port ${port.number} on ${c.name} has a prop that no longer exists.`, fix: "Unwire it from the port." });
    }
    const numbers = c.ports.map((p) => p.number);
    const repeated = [...new Set(numbers.filter((n, i) => numbers.indexOf(n) !== i))];
    for (const n of repeated) problems.push({ message: `${c.name} has two ports numbered ${n}.`, fix: "Give one of them the number printed on the controller." });
  }
  for (const [id, wiring] of propWiring(show, nodes)) {
    if (wiring.status !== "twice") continue;
    const where = wiring.places.map((p) => `${p.controllerName} port ${p.port}`).join(", ");
    problems.push({
      message: `${name(id)} is wired more than once (${where}).`,
      fix: "Unwire one, or give each slot its own range of pixels in its settings.",
    });
  }
  return problems;
}

// ---- Channels ----------------------------------------------------------------------------

/** Channels (counted from 1) carrying the port's lit pixels, and the universes they're in (sACN). */
export interface PortChannels {
  first: number;
  last: number;
  universes: [number, number] | null;
}

export function portChannels(map: ChannelMap, controller: string, port: number): PortChannels | null {
  const output = map.controllers.find((c) => c.controller === controller);
  const spans = output?.spans.filter((s) => s.port === port) ?? [];
  if (!output || spans.length === 0) return null;
  const start = Math.min(...spans.map((s) => s.controllerChannel));
  const end = Math.max(...spans.map((s) => s.controllerChannel + s.pixels * s.channelsPerPixel));
  let universes: [number, number] | null = null;
  if (output.addressing.type === "sacn") {
    const touched = output.addressing.universes.filter((u) => u.controllerChannel < end && start < u.controllerChannel + u.len).map((u) => u.universe);
    if (touched.length > 0) universes = [Math.min(...touched), Math.max(...touched)];
  }
  return { first: start + 1, last: end, universes };
}

// ---- Wiring path -------------------------------------------------------------------------

/** Most points drawn along one prop of the path. */
const MAX_RUN_POINTS = 300;

export interface WiringPath {
  /** Where the controller is drawn: below the first pixel, under the layout. */
  start: Pt | null;
  /** The first pixel the port's data reaches. */
  firstPixel: Pt | null;
  /** Each prop's pixels in the order the data reaches them. */
  runs: { prop: string; points: Pt[] }[];
  /** Wire between the controller and the first prop, and from each prop's last pixel to the next one's first. */
  jumps: { from: Pt; to: Pt }[];
}

/** The path a port's data takes: from the controller through each prop's first to last pixel. */
export function wiringPath(port: Port, preview: PreviewProp[]): WiringPath {
  const byId = new Map(preview.map((p) => [p.prop, p.points]));
  const runs: WiringPath["runs"] = [];
  for (const slot of port.slots) {
    const points = byId.get(slot.prop);
    if (!points) continue;
    const count = Math.floor(points.length / 2);
    const range = slot.segment ?? { start: 0, end: count };
    const [start, end] = [Math.max(0, range.start), Math.min(count, range.end)];
    if (end <= start) continue;
    const step = Math.max(1, Math.ceil((end - start) / MAX_RUN_POINTS));
    const order: number[] = [];
    for (let i = 0; i < end - start; i += step) order.push(i);
    if (order[order.length - 1] !== end - start - 1) order.push(end - start - 1);
    const at = (i: number) => {
      const n = slot.reverse ? end - 1 - i : start + i;
      return { x: points[n * 2], y: points[n * 2 + 1] };
    };
    runs.push({ prop: slot.prop, points: order.map(at) });
  }
  if (runs.length === 0) return { start: null, firstPixel: null, runs: [], jumps: [] };
  const firstPixel = runs[0].points[0];
  let [minY, maxY] = [Infinity, -Infinity];
  for (const r of runs) for (const p of r.points) [minY, maxY] = [Math.min(minY, p.y), Math.max(maxY, p.y)];
  const start = { x: firstPixel.x, y: minY - Math.max(1, (maxY - minY) * 0.15) };
  const jumps = [{ from: start, to: firstPixel }];
  for (let i = 1; i < runs.length; i++) jumps.push({ from: runs[i - 1].points[runs[i - 1].points.length - 1], to: runs[i].points[0] });
  return { start, firstPixel, runs, jumps };
}

// ---- Dropping ----------------------------------------------------------------------------

export interface Rect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** Where a drop at `p` goes among chips laid out in lines (left to right, then down): before the
 * first chip on a later line, or on the same line whose middle is past the pointer. */
export function dropIndex(chips: Rect[], p: Pt): number {
  for (let i = 0; i < chips.length; i++) {
    const r = chips[i];
    if (p.y < r.top) return i;
    if (p.y <= r.bottom && p.x < (r.left + r.right) / 2) return i;
  }
  return chips.length;
}
