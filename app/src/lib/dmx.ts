// DMX fixtures in the window: the library of channel layouts (the engine's own, in
// `crates/pf-model/src/dmx.rs`, mirrored by `dmxLayouts.json` and checked against it by the
// shell's tests), making a fixture from one, the colour its channels make, as the engine works
// it out (`emitted_colors`), and whether its shutter lets it show at a moment
// (`crates/pf-model/src/shutter.rs`), for the browser stand-in and the Test screen.

import layouts from "../api/dmxLayouts.json";
import type { DmxChannel, DmxFixture, DmxRole, DmxShutterRange, DmxSlot, DmxZoom, Prop, Show } from "../api/types";
import { axes, motorProblem, restMotorsAt } from "./motion";

/** What kind of fixture a library layout is for. */
export type DmxKind = "flood" | "head" | "servo";

/** A channel layout from the library: channels for the whole fixture, the same for each cell, then more. */
export interface DmxLayout {
  id: string;
  label: string;
  head: DmxRole[];
  cell: DmxRole[];
  tail: DmxRole[];
  /** Made for several cells (a bar, or several servos). */
  multiCell: boolean;
  kind: DmxKind;
}

/** The library of flood, par, bar, moving-head and servo layouts, named by their channels. */
export const DMX_LAYOUTS: DmxLayout[] = layouts as DmxLayout[];

/** The name people see for each role. */
export const DMX_ROLE_LABELS: Record<DmxRole, string> = {
  dimmer: "Dimmer",
  red: "Red",
  green: "Green",
  blue: "Blue",
  white: "White",
  amber: "Amber",
  uv: "UV",
  strobe: "Strobe",
  fixed: "Mode",
  pan: "Pan",
  panFine: "Pan fine",
  tilt: "Tilt",
  tiltFine: "Tilt fine",
  colorWheel: "Color wheel",
  gobo: "Gobo",
  other: "Other",
  cyan: "Cyan",
  magenta: "Magenta",
  yellow: "Yellow",
  servo: "Servo",
  servoFine: "Servo fine",
  speed: "Speed",
  zoom: "Zoom",
};

const PER_CELL: DmxRole[] = ["red", "green", "blue", "white", "amber", "cyan", "magenta", "yellow"];

/** Roles whose value comes from the colour drawn on the fixture. */
export function fromColor(role: DmxRole): boolean {
  return role === "dimmer" || PER_CELL.includes(role);
}

/** Roles that belong to one cell of the fixture. */
export function perCell(role: DmxRole): boolean {
  return PER_CELL.includes(role);
}

/** A colour wheel with its colours listed: it follows the colour drawn. */
export function isColorWheel(channel: DmxChannel): boolean {
  return channel.role === "colorWheel" && (channel.slots ?? []).some((s) => s.color !== undefined);
}

/** Whether a channel's value comes from the colour drawn (a colour or dimmer channel, or a colour wheel with its colours). */
export function channelFromColor(channel: DmxChannel): boolean {
  return fromColor(channel.role) || isColorWheel(channel);
}

/** A layout's channels for a fixture of `cells` cells (named with the cell's number when there are several), motors at Home. */
export function layoutChannels(layout: DmxLayout, cells: number): DmxChannel[] {
  const n = Math.max(1, Math.floor(cells));
  const channel = (role: DmxRole, name: string): DmxChannel => {
    const c: DmxChannel = { name, role };
    if (role === "pan" || role === "tilt" || role === "servo") {
      const range = role === "pan" ? 540 : role === "tilt" ? 270 : 180;
      c.motor = { rangeDeg: range, homeDeg: range / 2 };
    }
    return c;
  };
  const out: DmxChannel[] = layout.head.map((role) => channel(role, DMX_ROLE_LABELS[role]));
  for (let cell = 0; cell < n; cell++) {
    for (const role of layout.cell) {
      const c = channel(role, n > 1 ? `${DMX_ROLE_LABELS[role]} ${cell + 1}` : DMX_ROLE_LABELS[role]);
      if (perCell(role) && cell > 0) c.cell = cell;
      out.push(c);
    }
  }
  out.push(...layout.tail.map((role) => channel(role, DMX_ROLE_LABELS[role])));
  // Held values of 0 aren't written, as the engine leaves them out.
  return restMotorsAt(out, 0).map((c) => {
    if (c.value === 0) delete c.value;
    return c;
  });
}

/** Whether a fixture is a moving head: it has a pan or a tilt channel. */
export function isHead(fixture: DmxFixture): boolean {
  return fixture.channels.some((c) => c.role === "pan" || c.role === "tilt");
}

/** Whether a fixture is servos: it has a servo channel and no pan or tilt. */
export function isServos(fixture: DmxFixture): boolean {
  return !isHead(fixture) && fixture.channels.some((c) => c.role === "servo");
}

/** What a fixture is, in a few words: "DMX fixture · 1 cell", "Moving head · 16-bit", "Servos · 2". */
export function fixtureSummary(fixture: DmxFixture, cells: number): string {
  if (isHead(fixture)) {
    const sixteen = axes(fixture.channels).some((a) => a.fine !== null);
    return `Moving head · ${sixteen ? "16-bit" : "8-bit"}`;
  }
  if (isServos(fixture)) {
    const n = axes(fixture.channels).length;
    return n === 1 ? "Servo" : `Servos · ${n}`;
  }
  return `DMX fixture · ${cells === 1 ? "1 cell" : `${cells} cells`}`;
}

/** The beam a head is drawn with when it doesn't say. */
export const DEFAULT_BEAM = { angleDeg: 15, length: 6 } as const;

/** How a zoom channel widens the beam when it doesn't say: narrow at 0, wide at 255. */
export const DEFAULT_ZOOM: DmxZoom = { fromDeg: 5, toDeg: 45 };

/** What a strobe channel that lists nothing does: open from 0 to 9, then a strobe from slow to fast. */
export const USUAL_SHUTTER: DmxShutterRange[] = [
  { from: 0, does: "open" },
  { from: 10, does: "strobe" },
];

/** Flashes a second at the first value of a strobe range, and at its last. */
export const STROBE_SLOWEST_HZ = 1;
export const STROBE_FASTEST_HZ = 20;

/** What a shutter does at `value`: closed, open, or flashing `hz` times a second. */
export type ShutterState = { kind: "closed" } | { kind: "open" } | { kind: "strobe"; hz: number };

/** What a shutter whose values do as `ranges` say does at `value` (the engine's `shutter_at`). */
export function shutterAt(ranges: DmxShutterRange[], value: number): ShutterState {
  let i = -1;
  ranges.forEach((r, k) => {
    if (r.from <= value) i = k;
  });
  if (i < 0) return { kind: "open" };
  const range = ranges[i];
  if (range.does !== "strobe") return { kind: range.does };
  const next = ranges[i + 1];
  const last = next ? Math.max(Math.max(next.from - 1, 0), range.from) : 255;
  const span = Math.max(last - range.from, 1);
  const share = (value - range.from) / span;
  return { kind: "strobe", hz: STROBE_SLOWEST_HZ + (STROBE_FASTEST_HZ - STROBE_SLOWEST_HZ) * share };
}

/** Whether a strobe of `hz` flashes a second shows a flash in the frame shown at `tMs` for `frameMs` (the engine's `flashes`). */
export function flashes(hz: number, tMs: number, frameMs: number): boolean {
  const frame = Math.max(frameMs, 1);
  const rate = Math.min(hz, 500 / frame);
  if (!(rate > 0)) return false;
  const first = Math.ceil((tMs * rate) / 1000);
  return first < ((tMs + frame) * rate) / 1000;
}

/** What a strobe channel's values do: its own ranges, or the usual ones. */
export function shutterOf(channel: DmxChannel): DmxShutterRange[] {
  return channel.shutter && channel.shutter.length > 0 ? channel.shutter : USUAL_SHUTTER;
}

/** Whether a fixture's light shows at `tMs` (a frame shown for `frameMs`) with its channels at `dmx`: its first strobe channel's shutter open, or flashing then. */
export function shutterOpen(channels: DmxChannel[], dmx: ArrayLike<number>, tMs: number, frameMs: number): boolean {
  const i = channels.findIndex((c) => c.role === "strobe");
  if (i < 0) return true;
  const state = shutterAt(shutterOf(channels[i]), i < dmx.length ? dmx[i] : 0);
  return state.kind === "open" || (state.kind === "strobe" && flashes(state.hz, tMs, frameMs));
}

/** Why a strobe channel's ranges can't be used, in the engine's words, or null. */
export function shutterProblem(ranges: DmxShutterRange[]): string | null {
  if (ranges.length > 32) return `A strobe channel lists at most 32 ranges of values; this one lists ${ranges.length}.`;
  if (ranges.some((r, i) => i > 0 && ranges[i - 1].from >= r.from)) {
    return "A strobe channel's ranges of values go from the lowest up, each starting after the one before.";
  }
  return null;
}

/** A fixture at `address` laid out as `layout` with `cells` cells. */
export function fixtureFromLayout(layout: DmxLayout, cells: number, address: number): DmxFixture {
  const fixture: DmxFixture = { address, channels: layoutChannels(layout, cells), layout: layout.id };
  if (isHead(fixture)) fixture.beam = { ...DEFAULT_BEAM };
  return fixture;
}

/** The last address a fixture uses. */
export function lastAddress(fixture: DmxFixture): number {
  return fixture.address + Math.max(fixture.channels.length, 1) - 1;
}

/** Why a fixture can't be used on a prop of `cells` cells, in the engine's words, or null. */
export function fixtureProblem(fixture: DmxFixture, cells: number): string | null {
  if (fixture.channels.length === 0) return "A DMX fixture needs at least one channel.";
  if (fixture.channels.length > 512) return `A DMX fixture has at most 512 channels; this one has ${fixture.channels.length}.`;
  if (!(fixture.address >= 1 && fixture.address <= 512)) return `A DMX address is 1 to 512, so ${fixture.address} won't work.`;
  if (lastAddress(fixture) > 512) {
    return `Its ${fixture.channels.length} channels from address ${fixture.address} run past 512, the end of the universe.`;
  }
  if (cells < 1 || cells > 64) return "A DMX fixture has 1 to 64 cells (light positions).";
  const stray = fixture.channels.find((c) => perCell(c.role) && (c.cell ?? 0) >= cells);
  if (stray) return `The channel '${stray.name}' is for cell ${(stray.cell ?? 0) + 1}, but the fixture has ${cells}.`;
  for (const c of fixture.channels) {
    const problem = c.motor ? motorProblem(c.motor) : null;
    if (problem) return `The channel '${c.name}': ${problem}`;
    if ((c.slots?.length ?? 0) > 64) return `The channel '${c.name}' lists ${c.slots!.length} slots; a wheel has at most 64.`;
    const shutter = shutterProblem(c.shutter ?? []);
    if (shutter) return `The channel '${c.name}': ${shutter}`;
    const zoom = c.zoom;
    const angle = (a: number) => Number.isFinite(a) && a >= 1 && a <= 120;
    if (zoom && !(angle(zoom.fromDeg) && angle(zoom.toDeg))) return `The channel '${c.name}': A zoom widens a beam to 1° to 120°.`;
  }
  const beam = fixture.beam;
  if (beam && !(beam.angleDeg >= 1 && beam.angleDeg <= 120)) return "A beam is 1° to 120° wide.";
  if (beam && !(beam.length >= 0.5 && beam.length <= 100)) return "A beam is drawn 0.5 to 100 long.";
  return null;
}

/** The first address from which `channels` channels overlap no fixture already in `show` (1 when none is free). */
export function nextFreeAddress(show: Show, channels: number): number {
  const used = show.props.flatMap((p) => (p.dmx ? [[p.dmx.address, lastAddress(p.dmx)] as const] : []));
  let start = 1;
  while (start + channels - 1 <= 512) {
    const clash = used.find(([a, b]) => start <= b && a <= start + channels - 1);
    if (!clash) return start;
    start = clash[1] + 1;
  }
  return 1;
}

/** The library layout a fixture's channels are exactly, for `cells` cells (null: custom). */
export function layoutOf(fixture: DmxFixture, cells: number): DmxLayout | null {
  // Only what says which channel is which: a motor's settings or a held value don't make it another layout.
  const shape = (channels: DmxChannel[]) => JSON.stringify(channels.map((c) => [c.name, c.role, c.cell ?? 0]));
  const same = (layout: DmxLayout) => shape(layoutChannels(layout, cells)) === shape(fixture.channels);
  return DMX_LAYOUTS.find((l) => l.id === fixture.layout && same(l)) ?? DMX_LAYOUTS.find(same) ?? null;
}

/** A name not yet used in `show`: `base 1`, `base 2`, … */
function freeName(show: Show, base: string): string {
  const names = new Set(show.props.map((p) => p.name));
  let n = 1;
  while (names.has(`${base} ${n}`)) n++;
  return `${base} ${n}`;
}

/** A new fixture for `show` laid out as the library's `layoutId`, with `cells` cells, at the first free address. */
function newFromLayout(show: Show, layoutId: string, cells: number, base: string): Prop {
  const layout = DMX_LAYOUTS.find((l) => l.id === layoutId) ?? DMX_LAYOUTS[0];
  const fixture = fixtureFromLayout(layout, cells, 1);
  fixture.address = nextFreeAddress(show, fixture.channels.length);
  return {
    id: crypto.randomUUID(),
    name: freeName(show, base),
    shape: { source: "generator", type: "line", nodes: cells, length: Math.max(0.5, cells * 0.25) },
    transform: { position: { x: 0, y: 0, z: 0 }, rotationDeg: { x: 0, y: 0, z: 0 }, scale: { x: 1, y: 1, z: 1 } },
    colorOrder: "RGB",
    regions: [],
    tags: [],
    dmx: fixture,
  };
}

/** A new DMX flood for `show`: one cell, a dimmer then red, green and blue, at the first free address. */
export function newFixture(show: Show): Prop {
  return newFromLayout(show, "dimmer-rgb", 1, "Flood");
}

/** A new moving head for `show`: a common 16-bit head with a dimmer and red, green, blue and white, at Home. */
export function newMovingHead(show: Show): Prop {
  return newFromLayout(show, "head-rgbw-16", 1, "Moving Head");
}

/** A new servo for `show`: one 16-bit servo, at Home. */
export function newServo(show: Show): Prop {
  return newFromLayout(show, "servo-16", 1, "Servo");
}

/** Whether `prop` is a DMX fixture. */
export function isFixture(prop: Prop): boolean {
  return prop.dmx !== undefined;
}

const half = (a: number) => Math.ceil(a / 2);

/** The slot with a colour that `distance` puts nearest (the first of equals), if any has one. */
function nearestSlot(slots: DmxSlot[] | undefined, distance: (s: DmxSlot) => number): DmxSlot | undefined {
  let best: DmxSlot | undefined;
  let bestAt = Infinity;
  for (const s of slots ?? []) {
    if (s.color === undefined) continue;
    const d = distance(s);
    if (d < bestAt) [best, bestAt] = [s, d];
  }
  return best;
}

const hexRgb = (hex: string): [number, number, number] => {
  const n = parseInt(hex.replace("#", ""), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
};

/** What a fixture's channels hold for the colour drawn on its cells (`cells`: three bytes a cell), as the engine works it out. */
export function channelsForColors(channels: DmxChannel[], cells: ArrayLike<number>): number[] {
  const count = Math.floor(cells.length / 3);
  const rgb = (cell: number): [number, number, number] =>
    cell < count ? [cells[cell * 3], cells[cell * 3 + 1], cells[cell * 3 + 2]] : [0, 0, 0];
  const dimmed = channels.some((c) => c.role === "dimmer");
  let peak = 0;
  for (let c = 0; c < count; c++) peak = Math.max(peak, ...rgb(c));
  const [num, den] = dimmed && peak > 0 ? [255, peak] : [1, 1];
  const scaleBy = (v: number, n: number, d: number) => Math.min(255, Math.floor((v * n + Math.floor(d / 2)) / d));
  const scale = (v: number) => scaleBy(v, num, den);
  return channels.map((channel) => {
    if (channel.role === "dimmer") return peak;
    const cell = channel.cell ?? 0;
    if (channel.role === "cyan") return 255 - scale(rgb(cell)[0]);
    if (channel.role === "magenta") return 255 - scale(rgb(cell)[1]);
    if (channel.role === "yellow") return 255 - scale(rgb(cell)[2]);
    if (isColorWheel(channel)) {
      if (peak === 0) return channel.value ?? 0;
      const drawn = rgb(0).map((v) => scaleBy(v, 255, peak));
      const slot = nearestSlot(channel.slots, (s) => {
        const c = hexRgb(s.color!);
        return (c[0] - drawn[0]) ** 2 + (c[1] - drawn[1]) ** 2 + (c[2] - drawn[2]) ** 2;
      });
      return slot?.value ?? channel.value ?? 0;
    }
    if (!perCell(channel.role)) return channel.value ?? 0;
    let [r, g, b] = rgb(cell).map(scale);
    const inCell = (role: DmxRole) => channels.some((c) => c.role === role && (c.cell ?? 0) === cell);
    const hasRgb = inCell("red") || inCell("green") || inCell("blue");
    const white = !inCell("white") ? 0 : hasRgb ? Math.min(r, g, b) : Math.max(r, g, b);
    if (hasRgb) [r, g, b] = [r - white, g - white, b - white];
    const amber = inCell("amber") ? Math.min(r, Math.min(255, g * 2)) : 0;
    switch (channel.role) {
      case "red":
        return r - amber;
      case "green":
        return g - Math.min(half(amber), g);
      case "blue":
        return b;
      case "white":
        return white;
      default:
        return amber;
    }
  });
}

/** The colour each of `cells` cells shows for a fixture's channel values, as the engine works it out. */
export function emittedColors(channels: DmxChannel[], dmx: ArrayLike<number>, cells: number): [number, number, number][] {
  const value = (i: number) => (i < dmx.length ? dmx[i] : 0);
  const dimmerAt = channels.findIndex((c) => c.role === "dimmer");
  const level = dimmerAt >= 0 ? value(dimmerAt) : 255;
  // Servos with no dimmer make no light of their own.
  const dark = dimmerAt < 0 && channels.some((c) => c.role === "servo");
  const out: [number, number, number][] = [];
  for (let cell = 0; cell < cells; cell++) {
    let [r, g, b, lit] = [0, 0, 0, false];
    let cmy = false;
    const taken = [0, 0, 0];
    channels.forEach((c, i) => {
      const v = value(i);
      const mine = (c.cell ?? 0) === cell;
      if (c.role === "red" && mine) [r, lit] = [r + v, true];
      else if (c.role === "green" && mine) [g, lit] = [g + v, true];
      else if (c.role === "blue" && mine) [b, lit] = [b + v, true];
      else if (c.role === "white" && mine) [r, g, b, lit] = [r + v, g + v, b + v, true];
      else if (c.role === "amber" && mine) [r, g, lit] = [r + v, g + half(v), true];
      else if (c.role === "cyan" && mine) [cmy, taken[0]] = [true, v];
      else if (c.role === "magenta" && mine) [cmy, taken[1]] = [true, v];
      else if (c.role === "yellow" && mine) [cmy, taken[2]] = [true, v];
      else if (c.role === "uv") [r, b, lit] = [r + Math.floor(v / 4), b + Math.floor(v / 2), true];
      else if (isColorWheel(c)) {
        const slot = nearestSlot(c.slots, (s) => Math.abs(s.value - v));
        if (slot?.color) {
          const [sr, sg, sb] = hexRgb(slot.color);
          [r, g, b, lit] = [r + sr, g + sg, b + sb, true];
        }
      }
    });
    if (cmy) [r, g, b, lit] = [r + 255 - taken[0], g + 255 - taken[1], b + 255 - taken[2], true];
    if (!lit) [r, g, b] = dark ? [0, 0, 0] : [255, 255, 255];
    const out1 = (v: number) => Math.floor((Math.min(v, 255) * level + 127) / 255);
    out.push([out1(r), out1(g), out1(b)]);
  }
  return out;
}

/** A fixture's motors (see `axes` in motion.ts). */
export function fixtureAxes(fixture: DmxFixture) {
  return axes(fixture.channels);
}
