// Where a test pattern goes: the controllers a target's pixels are wired to, and the channels
// (and universes) it uses on each.

import sample from "../api/sampleShow.json";
import type { ChannelMap, Controller, OutputSpan, Show, TargetSpec } from "../api/types";
import { thousands } from "./format";

/** One controller a test pattern reaches. */
export interface TestDestination {
  controller: Controller;
  protocol: "sACN" | "DDP";
  /** The first and last controller channels used (from 1). */
  first: number;
  last: number;
  /** Channels the target's pixels use on this controller. */
  channels: number;
  /** sACN only: the first and last universes used. */
  universes: [number, number] | null;
}

/** The props a target covers (a group's submodels count as their whole prop); null for "every prop". */
function targetProps(show: Show, target: TargetSpec): Set<string> | null {
  switch (target.type) {
    case "show":
      return null;
    case "prop":
      return new Set([target.id]);
    case "group": {
      const group = show.groups.find((g) => g.id === target.id);
      return new Set((group?.members ?? []).map((m) => (typeof m === "string" ? m : m.prop)));
    }
    default:
      return null;
  }
}

/** The controllers a target sends to, in the show's order, with what it uses on each. */
export function testDestinations(show: Show, map: ChannelMap, target: TargetSpec): TestDestination[] {
  const props = targetProps(show, target);
  const out: TestDestination[] = [];
  for (const controller of show.controllers) {
    if ((target.type === "controller" || target.type === "port") && (target.type === "controller" ? target.id : target.controller) !== controller.id) continue;
    const output = map.controllers.find((c) => c.controller === controller.id);
    const spans = (output?.spans ?? []).filter(
      (s: OutputSpan) => s.pixels > 0 && (props === null || props.has(s.prop)) && (target.type !== "port" || s.port === target.port),
    );
    if (!output || spans.length === 0) continue;
    const start = Math.min(...spans.map((s) => s.controllerChannel));
    const end = Math.max(...spans.map((s) => s.controllerChannel + s.pixels * s.channelsPerPixel));
    let universes: [number, number] | null = null;
    if (output.addressing.type === "sacn") {
      const touched = output.addressing.universes
        .filter((u) => spans.some((s) => u.controllerChannel < s.controllerChannel + s.pixels * s.channelsPerPixel && s.controllerChannel < u.controllerChannel + u.len))
        .map((u) => u.universe);
      if (touched.length > 0) universes = [Math.min(...touched), Math.max(...touched)];
    }
    out.push({
      controller,
      protocol: controller.protocol.type === "sacn" ? "sACN" : "DDP",
      first: start + 1,
      last: end,
      channels: spans.reduce((sum, s) => sum + s.pixels * s.channelsPerPixel, 0),
      universes,
    });
  }
  return out;
}

const range = ([a, b]: [number, number]) => (a === b ? `${a}` : `${a}–${b}`);

/** "sACN · universes 1–4 · ch 1–1,686" or "DDP · ch 1–300". */
export function describeUse(d: TestDestination): string {
  const universes = d.universes ? ` · ${d.universes[0] === d.universes[1] ? "universe" : "universes"} ${range(d.universes)}` : "";
  return `${d.protocol}${universes} · ch ${thousands(d.first)}–${thousands(d.last)}`;
}

/** Whether this is the demo show built into the app (or the browser's `?demo` house): every
 * controller is one of the demo's, by id or by name and address. */
export function isDemoShow(show: Show): boolean {
  const demo = (sample as unknown as Show).controllers;
  return (
    show.controllers.length > 0 &&
    show.controllers.every((c) => demo.some((d) => d.id === c.id || (d.name === c.name && d.address === c.address)))
  );
}

const COLOR_NAMES: Record<string, string> = {
  "#ff0000": "red",
  "#00ff00": "green",
  "#0000ff": "blue",
  "#ffffff": "white",
};

/** "white", or the hex when the color has no plain name. */
export function colorName(hex: string): string {
  return COLOR_NAMES[hex.toLowerCase()] ?? hex.toUpperCase();
}

/** "Chase, white → Main FPP (192.168.1.50), Porch WLED (192.168.1.60)". */
export function sendingSummary(pattern: string, color: string | null, destinations: TestDestination[]): string {
  const what = color ? `${pattern}, ${colorName(color)}` : pattern;
  const where = destinations.map((d) => `${d.controller.name} (${d.controller.address})`).join(", ");
  return where ? `${what} → ${where}` : what;
}
