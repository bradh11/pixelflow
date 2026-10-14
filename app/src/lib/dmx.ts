// DMX fixtures in the window: the library of channel layouts (the engine's own, in
// `crates/pf-model/src/dmx.rs`, mirrored by `dmxLayouts.json` and checked against it by the
// shell's tests), making a fixture from one, and the colour its channels make, as the engine
// works it out (`emitted_colors`), for the browser stand-in and the Test screen.

import layouts from "../api/dmxLayouts.json";
import type { DmxChannel, DmxFixture, DmxRole, Prop, Show } from "../api/types";

/** A channel layout from the library: channels for the whole fixture, the same for each cell, then more. */
export interface DmxLayout {
  id: string;
  label: string;
  head: DmxRole[];
  cell: DmxRole[];
  tail: DmxRole[];
  /** Made for several cells (a bar). */
  multiCell: boolean;
}

/** The library of flood, par and bar layouts, named by their channels. */
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
};

/** Roles whose value comes from the colour drawn on the fixture. */
export function fromColor(role: DmxRole): boolean {
  return ["dimmer", "red", "green", "blue", "white", "amber"].includes(role);
}

/** Roles that belong to one cell of the fixture. */
export function perCell(role: DmxRole): boolean {
  return ["red", "green", "blue", "white", "amber"].includes(role);
}

/** A layout's channels for a fixture of `cells` cells (named with the cell's number when there are several). */
export function layoutChannels(layout: DmxLayout, cells: number): DmxChannel[] {
  const n = Math.max(1, Math.floor(cells));
  const out: DmxChannel[] = layout.head.map((role) => ({ name: DMX_ROLE_LABELS[role], role }));
  for (let cell = 0; cell < n; cell++) {
    for (const role of layout.cell) {
      const channel: DmxChannel = { name: n > 1 ? `${DMX_ROLE_LABELS[role]} ${cell + 1}` : DMX_ROLE_LABELS[role], role };
      if (perCell(role) && cell > 0) channel.cell = cell;
      out.push(channel);
    }
  }
  out.push(...layout.tail.map((role) => ({ name: DMX_ROLE_LABELS[role], role })));
  return out;
}

/** A fixture at `address` laid out as `layout` with `cells` cells. */
export function fixtureFromLayout(layout: DmxLayout, cells: number, address: number): DmxFixture {
  return { address, channels: layoutChannels(layout, cells), layout: layout.id };
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
  const same = (layout: DmxLayout) => JSON.stringify(layoutChannels(layout, cells)) === JSON.stringify(fixture.channels);
  return DMX_LAYOUTS.find((l) => l.id === fixture.layout && same(l)) ?? DMX_LAYOUTS.find(same) ?? null;
}

/** A new DMX flood for `show`: one cell, a dimmer then red, green and blue, at the first free address. */
export function newFixture(show: Show): Prop {
  const layout = DMX_LAYOUTS.find((l) => l.id === "dimmer-rgb") ?? DMX_LAYOUTS[0];
  const names = new Set(show.props.map((p) => p.name));
  let n = 1;
  while (names.has(`Flood ${n}`)) n++;
  const fixture = fixtureFromLayout(layout, 1, 1);
  fixture.address = nextFreeAddress(show, fixture.channels.length);
  return {
    id: crypto.randomUUID(),
    name: `Flood ${n}`,
    shape: { source: "generator", type: "line", nodes: 1, length: 0.5 },
    transform: { position: { x: 0, y: 0, z: 0 }, rotationDeg: { x: 0, y: 0, z: 0 }, scale: { x: 1, y: 1, z: 1 } },
    colorOrder: "RGB",
    regions: [],
    tags: [],
    dmx: fixture,
  };
}

/** Whether `prop` is a DMX fixture. */
export function isFixture(prop: Prop): boolean {
  return prop.dmx !== undefined;
}

const half = (a: number) => Math.ceil(a / 2);

/** What a fixture's channels hold for the colour drawn on its cells (`cells`: three bytes a cell), as the engine works it out. */
export function channelsForColors(channels: DmxChannel[], cells: ArrayLike<number>): number[] {
  const count = Math.floor(cells.length / 3);
  const rgb = (cell: number): [number, number, number] =>
    cell < count ? [cells[cell * 3], cells[cell * 3 + 1], cells[cell * 3 + 2]] : [0, 0, 0];
  const dimmed = channels.some((c) => c.role === "dimmer");
  let peak = 0;
  for (let c = 0; c < count; c++) peak = Math.max(peak, ...rgb(c));
  const [num, den] = dimmed && peak > 0 ? [255, peak] : [1, 1];
  const scale = (v: number) => Math.min(255, Math.floor((v * num + Math.floor(den / 2)) / den));
  return channels.map((channel) => {
    if (channel.role === "dimmer") return peak;
    if (!perCell(channel.role)) return channel.value ?? 0;
    const cell = channel.cell ?? 0;
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
  const out: [number, number, number][] = [];
  for (let cell = 0; cell < cells; cell++) {
    let [r, g, b, lit] = [0, 0, 0, false];
    channels.forEach((c, i) => {
      const v = value(i);
      const mine = (c.cell ?? 0) === cell;
      if (c.role === "red" && mine) [r, lit] = [r + v, true];
      else if (c.role === "green" && mine) [g, lit] = [g + v, true];
      else if (c.role === "blue" && mine) [b, lit] = [b + v, true];
      else if (c.role === "white" && mine) [r, g, b, lit] = [r + v, g + v, b + v, true];
      else if (c.role === "amber" && mine) [r, g, lit] = [r + v, g + half(v), true];
      else if (c.role === "uv") [r, b, lit] = [r + Math.floor(v / 4), b + Math.floor(v / 2), true];
    });
    if (!lit) [r, g, b] = [255, 255, 255];
    const out1 = (v: number) => Math.floor((Math.min(v, 255) * level + 127) / 255);
    out.push([out1(r), out1(g), out1(b)]);
  }
  return out;
}
