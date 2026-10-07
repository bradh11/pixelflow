// Pure logic for the Wiring screen: placing, moving, and removing props on controller ports
// (each gesture becomes one batch of edits: one undo step), how full each port is, which props
// aren't wired (or are wired twice), each port's channels, and the path its data takes through
// the layout. Nothing here touches the DOM, so it is all unit-tested.
//
// It follows the engine's channel mapping (crates/pf-mapping): a slot whose prop is gone, or
// whose pixel range doesn't fit the prop, carries nothing.

import type { ChannelMap, Controller, Edit, NodeRange, Port, PortSlot, PreviewProp, Show } from "../api/types";
import { FALCON_PIXELS_AT_40FPS } from "./controllerKinds";
import { plural, thousands } from "./format";
import type { Pt } from "./layoutMath";

/** A port on a controller. `at` (its place in the controller's port list) tells apart two ports
 * that share a number; without it, such a port is left alone. */
export interface PortRef {
  controller: string;
  port: number;
  at?: number;
}

/** A place on a port: where a dropped slot goes. */
export interface PlaceRef extends PortRef {
  index: number;
}

/**
 * A slot, by what it carries: the prop and its pixel range, with where it was seen (`index`) as a
 * hint. Edits find it again in the show as it is when their turn comes (an earlier edit may have
 * moved it) and do nothing when it's gone.
 */
export interface SlotRef extends PlaceRef {
  prop: string;
  segment: NodeRange | null;
}

/** Pixels in each prop, by id. */
export type NodeCounts = ReadonlyMap<string, number>;

export function nodeCounts(map: ChannelMap): Map<string, number> {
  return new Map(map.props.map((p) => [p.prop, p.nodes]));
}

/** Channels per pixel of each prop (3, or 4 for RGBW), by id. */
export function channelsPerPixel(map: ChannelMap): Map<string, number> {
  return new Map(map.props.map((p) => [p.prop, p.channelsPerPixel]));
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
export function slotLabel(name: string, slot: { segment: NodeRange | null }): string {
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

/** "ok"; "near" the limit; "slow": more than the port refreshes in time at the show's frame rate;
 * "over" the limit; "none": the limit isn't known. */
export type CapacityLevel = "none" | "ok" | "near" | "slow" | "over";

/** Pixels on one smart receiver of a port (or, with `receiver` null, wired to the port itself). */
export interface ReceiverLoad {
  /** The smart receiver (1 = A), or null for pixels wired straight to the port. */
  receiver: number | null;
  /** Pixels, counted as the boards count them (three channels to a pixel). */
  used: number;
}

/** How full a port is, counting every smart receiver on it together. */
export interface Capacity {
  /** Pixels, counted as the boards count them: three channels to a pixel, so RGBW counts 1⅓. */
  used: number;
  limit: number | null;
  /** About how many pixels the port refreshes in time at the show's frame rate, when that's below
   * `limit` (Falcon boards). */
  refresh: number | null;
  level: CapacityLevel;
  message: string | null;
  /** Each smart receiver's share of `used`, in wiring order; empty when the port feeds none. */
  receivers: ReceiverLoad[];
}

export interface CapacityOptions {
  /** Channels per pixel by prop; 3 when not given. */
  cpp?: ReadonlyMap<string, number>;
  adapter?: Controller["adapter"];
  /** The show's frame rate. */
  fps?: number;
}

/** Smart receivers are lettered on the boards: 1 is A, 2 is B, … */
export function receiverName(receiver: number): string {
  return receiver >= 1 && receiver <= 26 ? String.fromCharCode(64 + receiver) : String(receiver);
}

/**
 * How full the port is. Smart receivers on a port share its one limit, as in xLights: their
 * pixels are added up and checked together (each receiver's share is in `receivers`). Falcon
 * ports also say when they hold more than they refresh in time at the show's frame rate:
 * xLights' figure for V4/V5 boards is about 704 pixels at 40 fps, scaled here to the show's rate
 * (never above the board's limit). Over the limit is a warning: every channel is still sent.
 */
export function portCapacity(port: Port, nodes: NodeCounts, options: CapacityOptions = {}): Capacity {
  const loads: { receiver: number | null; channels: number }[] = [];
  let channels = 0;
  let wide = false;
  for (const slot of port.slots) {
    const range = slotRange(slot, nodes);
    if (!range) continue;
    const cpp = options.cpp?.get(slot.prop) ?? 3;
    const pixels = slot.nullPixels + range.end - range.start;
    let load = loads.find((o) => o.receiver === slot.smartReceiver);
    if (!load) loads.push((load = { receiver: slot.smartReceiver, channels: 0 }));
    load.channels += pixels * cpp;
    channels += pixels * cpp;
    wide ||= pixels > 0 && cpp > 3;
  }
  const receivers = loads.some((l) => l.receiver !== null) ? loads.map((l) => ({ receiver: l.receiver, used: Math.ceil(l.channels / 3) })) : [];
  const limit = port.maxPixels;
  const fps = options.fps ?? 40;
  const fast = options.adapter === "falcon" && limit !== null && fps > 0 ? Math.floor((FALCON_PIXELS_AT_40FPS * 40) / fps) : null;
  const refresh = fast !== null && limit !== null && fast < limit ? fast : null;
  const used = Math.ceil(channels / 3);
  const base = { used, limit, refresh, receivers };
  const shared = receivers.length > 1 ? `, shared by receivers ${receivers.map((r) => (r.receiver === null ? "the port" : receiverName(r.receiver))).join(", ")}` : "";
  if (limit === null) return { ...base, level: "none", message: null };
  if (used > limit) {
    const rgbw = wide ? "; RGBW pixels count as 1⅓" : "";
    return {
      ...base,
      level: "over",
      message: `${plural(used - limit, "pixel")} more than this port can drive (${thousands(used)} of ${thousands(limit)}${shared}${rgbw}). Move a prop to another port.`,
    };
  }
  if (refresh !== null && used > refresh) {
    return {
      ...base,
      level: "slow",
      message: `At ${fps} fps this port refreshes about ${thousands(refresh)} pixels in time; with ${thousands(used)} it will slow down. Move a prop to another port, or lower the show's frame rate.`,
    };
  }
  if (used >= limit * NEARLY_FULL) return { ...base, level: "near", message: `Nearly full: ${thousands(used)} of ${thousands(limit)} pixels${shared}.` };
  return { ...base, level: "ok", message: null };
}

/** Capacity options for a port on this controller of the show. */
export function capacityOptions(show: Show, controller: Controller, cpp?: ReadonlyMap<string, number>): CapacityOptions {
  return { cpp, adapter: controller.adapter, fps: show.settings.frameRate };
}

// ---- Edits --------------------------------------------------------------------------------

const sameRange = (a: NodeRange | null, b: NodeRange | null) => a === b || (!!a && !!b && a.start === b.start && a.end === b.end);

/** Where the port is in its controller's list; null when it's gone, or when two ports share its
 * number and `at` doesn't say which. */
function portIndex(controller: Controller, ref: PortRef): number | null {
  if (ref.at !== undefined && controller.ports[ref.at]?.number === ref.port) return ref.at;
  const matches = controller.ports.flatMap((p, i) => (p.number === ref.port ? [i] : []));
  return matches.length === 1 ? matches[0] : null;
}

/** The port, when it's there and unambiguous. */
export function findPort(show: Show, ref: PortRef): Port | null {
  const controller = show.controllers.find((c) => c.id === ref.controller);
  const i = controller ? portIndex(controller, ref) : null;
  return controller && i !== null ? controller.ports[i] : null;
}

/** The controller with one port changed; null when the controller or port isn't there. */
function changePort(show: Show, ref: PortRef, change: (port: Port) => Port): Controller | null {
  const controller = show.controllers.find((c) => c.id === ref.controller);
  const i = controller ? portIndex(controller, ref) : null;
  if (!controller || i === null) return null;
  return { ...controller, ports: controller.ports.map((p, j) => (j === i ? change(p) : p)) };
}

const update = (controller: Controller | null): Edit[] => (controller ? [{ type: "updateController", controller }] : []);

/** A reference to the slot at `index` on the port (null when there's none). */
export function slotRefAt(show: Show, port: PortRef, index: number): SlotRef | null {
  const slot = findPort(show, port)?.slots[index];
  return slot ? { ...port, index, prop: slot.prop, segment: slot.segment } : null;
}

/**
 * Where the slot is on `port` now: at its old place if it's still there; else the one slot with
 * the same prop and pixels; else, when its own pixel range was just changed, the same prop at its
 * old place (or the port's only slot for that prop). Null when it's gone or can't be told apart.
 */
export function resolveSlot(port: Port, ref: SlotRef): number | null {
  const here = port.slots[ref.index];
  const same = (s: PortSlot) => s.prop === ref.prop && sameRange(s.segment, ref.segment);
  if (here && same(here)) return ref.index;
  const exact = port.slots.flatMap((s, i) => (same(s) ? [i] : []));
  if (exact.length > 0) return exact.length === 1 ? exact[0] : null;
  if (here?.prop === ref.prop) return ref.index;
  const ofProp = port.slots.flatMap((s, i) => (s.prop === ref.prop ? [i] : []));
  return ofProp.length === 1 ? ofProp[0] : null;
}

/** The slot's place in the show as it is now, or null when it's gone. */
function locate(show: Show, ref: SlotRef): { ref: SlotRef; slot: PortSlot } | null {
  const port = findPort(show, ref);
  const index = port ? resolveSlot(port, ref) : null;
  return port && index !== null ? { ref: { ...ref, index }, slot: port.slots[index] } : null;
}

const inserted = (slots: PortSlot[], index: number, slot: PortSlot) => [...slots.slice(0, index), slot, ...slots.slice(index)];

/** Moves the slot to sit before position `to.index` (counted before the move). */
export function moveSlotEdits(show: Show, slotRef: SlotRef, to: PlaceRef): Edit[] {
  const found = locate(show, slotRef);
  if (!found) return [];
  const { ref: from, slot } = found;
  const fromController = show.controllers.find((c) => c.id === from.controller)!;
  const toController = show.controllers.find((c) => c.id === to.controller);
  const fromPort = portIndex(fromController, from);
  const toPort = toController ? portIndex(toController, to) : null;
  if (toPort === null) return [];
  if (from.controller === to.controller && fromPort === toPort) {
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
  const added = changePort(base, { ...to, at: toPort }, (p) => ({ ...p, slots: inserted(p.slots, Math.min(to.index, p.slots.length), slot) }));
  if (!added) return [];
  return from.controller === to.controller ? update(added) : [...update(removed), ...update(added)];
}

/** Moves the slot one place earlier (-1) or later (1) on its port, from wherever it is now. */
export function moveSlotByEdits(show: Show, ref: SlotRef, step: -1 | 1): Edit[] {
  const found = locate(show, ref);
  const port = found && findPort(show, found.ref);
  if (!found || !port) return [];
  const i = found.ref.index;
  if (step < 0 ? i === 0 : i === port.slots.length - 1) return [];
  return moveSlotEdits(show, found.ref, { ...found.ref, index: step < 0 ? i - 1 : i + 2 });
}

/**
 * Wires the prop at `to`, dragged from the props list: an unwired prop is wired whole; a partly
 * wired one gets a slot for its first pixels that aren't wired yet; a prop wired by one slot is
 * moved here. A prop already wired in several pieces is left alone (move its pieces instead).
 * An index past the end means the end of the port.
 */
export function wirePropEdits(show: Show, prop: string, to: PlaceRef, nodes: NodeCounts): Edit[] {
  const action = wireAction(show, prop, nodes);
  if (!action) return [];
  if (action.kind === "move") return moveSlotEdits(show, action.from, to);
  return update(changePort(show, to, (p) => ({ ...p, slots: inserted(p.slots, Math.min(to.index, p.slots.length), blankSlot(prop, action.segment)) })));
}

/** What adding the prop to a port does: wire it (whole, or the pixels not wired yet), or move its
 * one slot from where it is. Null when there's nothing to do. */
export type WireAction = { kind: "add"; segment: NodeRange | null } | { kind: "move"; from: SlotRef };

export function wireAction(show: Show, prop: string, nodes: NodeCounts): WireAction | null {
  const wiring = propWiring(show, nodes).get(prop);
  if (!wiring) return null;
  if (wiring.status === "unwired") return { kind: "add", segment: null };
  const gap = firstGap(
    wiring.places.map((place) => slotRange(place.slot, nodes)).filter((r): r is NodeRange => r !== null),
    wiring.nodes,
  );
  if (gap) return { kind: "add", segment: gap };
  if (wiring.places.length !== 1) return null;
  const [place] = wiring.places;
  return { kind: "move", from: { controller: place.controller, port: place.port, at: place.at, index: place.index, prop, segment: place.slot.segment } };
}

export function unwireEdits(show: Show, ref: SlotRef): Edit[] {
  const found = locate(show, ref);
  if (!found) return [];
  return update(changePort(show, found.ref, (p) => ({ ...p, slots: p.slots.filter((_, i) => i !== found.ref.index) })));
}

export function updateSlotEdits(show: Show, ref: SlotRef, change: (slot: PortSlot) => PortSlot): Edit[] {
  const found = locate(show, ref);
  if (!found) return [];
  return update(changePort(show, found.ref, (p) => ({ ...p, slots: p.slots.map((s, i) => (i === found.ref.index ? change(s) : s)) })));
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
  const i = controller ? portIndex(controller, ref) : null;
  if (!controller || i === null) return [];
  return update({ ...controller, ports: controller.ports.filter((_, j) => j !== i) });
}

/** Gives the port a new number (as printed on the controller); ports stay in number order. A
 * number another port already has is refused. */
export function renumberPortEdits(show: Show, ref: PortRef, number: number): Edit[] {
  const controller = show.controllers.find((c) => c.id === ref.controller);
  if (!controller || number === ref.port || controller.ports.some((p) => p.number === number)) return [];
  const renumbered = changePort(show, ref, (p) => ({ ...p, number }));
  return update(renumbered && { ...renumbered, ports: [...renumbered.ports].sort((a, b) => a.number - b.number) });
}

/** Appends the props, whole and in the given order, to the end of the port: those still unwired
 * when the edit's turn comes (the list may be older than the show). */
export function wireRemainingEdits(show: Show, ref: PortRef, props: string[], nodes: NodeCounts): Edit[] {
  const wiring = propWiring(show, nodes);
  const still = props.filter((id) => wiring.get(id)?.status === "unwired");
  if (still.length === 0) return [];
  return update(changePort(show, ref, (p) => ({ ...p, slots: [...p.slots, ...still.map((id) => blankSlot(id))] })));
}

// ---- Where each prop is wired ------------------------------------------------------------

export interface Place {
  controller: string;
  controllerName: string;
  port: number;
  /** The port's place in the controller's list. */
  at: number;
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
    c.ports.forEach((port, at) => {
      port.slots.forEach((slot, index) => places.get(slot.prop)?.push({ controller: c.id, controllerName: c.name, port: port.number, at, index, slot }));
    });
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
    // A prop with no pixels is wired as soon as it's on a port.
    const status: WiringStatus = count === 0 ? (here.length > 0 ? "wired" : "unwired") : covered === 0 ? "unwired" : twice ? "twice" : covered < count ? "partial" : "wired";
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

export function wiringProblems(show: Show, nodes: NodeCounts, cpp?: ReadonlyMap<string, number>): WiringProblem[] {
  const problems: WiringProblem[] = [];
  const name = (id: string) => show.props.find((p) => p.id === id)?.name ?? "A prop";
  for (const c of show.controllers) {
    for (const port of c.ports) {
      const capacity = portCapacity(port, nodes, capacityOptions(show, c, cpp));
      if (capacity.level === "over" && capacity.limit !== null) {
        const shared = capacity.receivers.length > 1 ? `, across its ${capacity.receivers.length} smart receivers` : "";
        problems.push({
          message: `Port ${port.number} on ${c.name} has ${plural(capacity.used - capacity.limit, "pixel")} more than it can drive (${thousands(capacity.used)} of ${thousands(capacity.limit)}${shared}).`,
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

/** Where one slot's lit pixels start on its controller (counted from 1), and the universes they
 * span (sACN). */
export interface SlotChannels {
  first: number;
  universes: [number, number] | null;
}

/** Each slot's channels, in slot order; null for a slot that carries nothing. The channel map's
 * spans for a port come in slot order, skipping slots with nothing to carry. */
export function slotChannels(map: ChannelMap, controller: string, port: Port, nodes: NodeCounts): (SlotChannels | null)[] {
  const output = map.controllers.find((c) => c.controller === controller);
  const spans = output?.spans.filter((s) => s.port === port.number) ?? [];
  let next = 0;
  return port.slots.map((slot) => {
    const range = slotRange(slot, nodes);
    if (!output || !range || range.end === range.start) return null;
    const i = spans.findIndex((s, j) => j >= next && s.prop === slot.prop && s.pixels === range.end - range.start);
    if (i < 0) return null;
    next = i + 1;
    const span = spans[i];
    const end = span.controllerChannel + span.pixels * span.channelsPerPixel;
    let universes: [number, number] | null = null;
    if (output.addressing.type === "sacn") {
      const touched = output.addressing.universes.filter((u) => u.controllerChannel < end && span.controllerChannel < u.controllerChannel + u.len).map((u) => u.universe);
      if (touched.length > 0) universes = [Math.min(...touched), Math.max(...touched)];
    }
    return { first: span.controllerChannel + 1, universes };
  });
}

/** "U1–4", "U3", or "" for none. */
export function universeText(universes: [number, number] | null): string {
  if (!universes) return "";
  return universes[0] === universes[1] ? `U${universes[0]}` : `U${universes[0]}–${universes[1]}`;
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
  /** Each prop's pixels in the order the data reaches them, with its slot's place on the port. */
  runs: { prop: string; slot: number; points: Pt[] }[];
  /** Wire between the controller and the first prop, and from each prop's last pixel to the next one's first. */
  jumps: { from: Pt; to: Pt }[];
}

/** The path a port's data takes: from the controller through each prop's first to last pixel. */
export function wiringPath(port: Port, preview: PreviewProp[]): WiringPath {
  const byId = new Map(preview.map((p) => [p.prop, p.points]));
  const runs: WiringPath["runs"] = [];
  for (const [index, slot] of port.slots.entries()) {
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
    runs.push({ prop: slot.prop, slot: index, points: order.map(at) });
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

/** Where a drop at height `y` goes among table rows (top to bottom): before the first row whose
 * middle is below it, else after the last. */
export function rowDropIndex(rows: Rect[], y: number): number {
  const i = rows.findIndex((r) => y < (r.top + r.bottom) / 2);
  return i < 0 ? rows.length : i;
}
