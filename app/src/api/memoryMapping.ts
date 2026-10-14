// Controller channels and sACN universes for the in-memory backend, following the engine's
// pf-mapping closely enough for the Wiring screen (and the demo) to show real channel ranges.
// Wiring problems aren't reported here; the engine does that.

import type { Controller, ControllerOutput, FixtureLayout, OutputSpan, Prop, Show, UniverseSpan } from "./types";
import { channelsPerPixel, nodeCount } from "../lib/shows";
import { fixtureProblem } from "../lib/dmx";

/** The largest sACN universe number. */
const MAX_UNIVERSE = 65535;
/** Channels in a DMX universe. */
const DMX_UNIVERSE = 512;

/** Whether a controller's ports are DMX universes (a DMX node, or any USB DMX interface). */
export function isDmxController(controller: Controller): boolean {
  return controller.kind === "dmx" || controller.protocol.type === "dmxUsb";
}

/** Every fixture's channels in the frame, after every prop's pixels (from `pixelsLen`), in show order. A fixture that can't be used takes none. */
export function mapFixtures(show: Show, pixelsLen: number): FixtureLayout[] {
  const out: FixtureLayout[] = [];
  let pixels = 0;
  let dmx = pixelsLen;
  const seen = new Set<string>();
  for (const prop of show.props) {
    const nodes = nodeCount(prop.shape);
    const cellsOffset = pixels;
    pixels += nodes * channelsPerPixel(prop);
    if (!prop.dmx || fixtureProblem(prop.dmx, nodes) !== null || seen.has(prop.id)) continue;
    seen.add(prop.id);
    out.push({ prop: prop.id, cellsOffset, cells: nodes, dmxOffset: dmx, address: prop.dmx.address, channels: prop.dmx.channels });
    dmx += prop.dmx.channels.length;
  }
  return out;
}

interface Run {
  pixels: number;
  cpp: number;
}

/** Universe-sized chunks `[firstChannel, len]` of a controller's pixels (never splitting a pixel
 * unless `straddle`, or a universe is too small for one of the pixels: then channels run straight
 * through, as xLights numbers them). */
export function universeChunks(runs: Run[], size: number, straddle: boolean): [number, number][] {
  const total = runs.reduce((sum, r) => sum + r.pixels * r.cpp, 0);
  const out: [number, number][] = [];
  if (size < 1) return out;
  if (straddle || runs.some((r) => r.pixels > 0 && r.cpp > size)) {
    for (let start = 0; start < total; start += size) out.push([start, Math.min(size, total - start)]);
    return out;
  }
  let [start, len] = [0, 0];
  for (const run of runs) {
    let remaining = run.pixels;
    while (remaining > 0) {
      const fit = Math.floor((size - len) / run.cpp);
      if (fit === 0) {
        out.push([start, len]);
        start += len;
        len = 0;
        continue;
      }
      const take = Math.min(fit, remaining);
      len += take * run.cpp;
      remaining -= take;
    }
  }
  if (len > 0) out.push([start, len]);
  return out;
}

/** Every controller's output: spans in wiring order, and its universes when it uses sACN. */
export function mapControllers(show: Show): ControllerOutput[] {
  // First occurrence of an id wins (the offset still advances), like the engine.
  const layouts = new Map<string, { prop: Prop; frameOffset: number; nodes: number; cpp: number }>();
  let offset = 0;
  for (const prop of show.props) {
    const nodes = nodeCount(prop.shape);
    const cpp = channelsPerPixel(prop);
    if (!layouts.has(prop.id)) layouts.set(prop.id, { prop, frameOffset: offset, nodes, cpp });
    offset += nodes * cpp;
  }

  const pixelsLen = offset;
  const fixtures = new Map(mapFixtures(show, pixelsLen).map((f) => [f.prop, f]));

  const wired = show.controllers.map((controller) => {
    const spans: OutputSpan[] = [];
    const runs: Run[] = [];
    let channel = 0;
    if (isDmxController(controller)) {
      // Each port one whole universe; each fixture at its address (a USB interface sends one).
      const ports = controller.protocol.type === "dmxUsb" ? controller.ports.slice(0, 1) : controller.ports;
      ports.forEach((port, n) => {
        for (const slot of port.slots) {
          const fixture = fixtures.get(slot.prop);
          const prop = layouts.get(slot.prop)?.prop;
          if (!fixture || !prop) continue;
          spans.push({
            prop: prop.id,
            port: port.number,
            controllerChannel: n * DMX_UNIVERSE + fixture.address - 1,
            frameOffset: fixture.dmxOffset,
            pixels: fixture.channels.length,
            channelsPerPixel: 1,
            reverse: false,
            colorOrder: "RGB",
            brightness: 100,
            gamma: 1,
          });
        }
      });
      spans.sort((a, b) => a.controllerChannel - b.controllerChannel);
      return { spans, runs: [{ pixels: ports.length * DMX_UNIVERSE, cpp: 1 }], channelCount: ports.length * DMX_UNIVERSE, dmx: ports.length };
    }
    for (const port of controller.ports) {
      for (const slot of port.slots) {
        const layout = layouts.get(slot.prop);
        if (!layout) continue;
        const prop = layout.prop;
        if (prop.dmx) {
          // A fixture on a pixel controller takes its channels where it falls, in one piece
          // (never split between universes), after its null channels.
          const fixture = fixtures.get(prop.id);
          if (!fixture) continue;
          if (slot.nullPixels > 0) {
            runs.push({ pixels: slot.nullPixels, cpp: 1 });
            channel += slot.nullPixels;
          }
          spans.push({
            prop: prop.id,
            port: port.number,
            controllerChannel: channel,
            frameOffset: fixture.dmxOffset,
            pixels: fixture.channels.length,
            channelsPerPixel: 1,
            reverse: false,
            colorOrder: "RGB",
            brightness: 100,
            gamma: 1,
          });
          runs.push({ pixels: 1, cpp: fixture.channels.length });
          channel += fixture.channels.length;
          continue;
        }
        const range = slot.segment ?? { start: 0, end: layout.nodes };
        if (range.start > range.end || range.end > layout.nodes) continue;
        if (slot.nullPixels > 0) {
          runs.push({ pixels: slot.nullPixels, cpp: layout.cpp });
          channel += slot.nullPixels * layout.cpp;
        }
        const pixels = range.end - range.start;
        if (pixels === 0) continue;
        spans.push({
          prop: prop.id,
          port: port.number,
          controllerChannel: channel,
          frameOffset: layout.frameOffset + range.start * layout.cpp,
          pixels,
          channelsPerPixel: layout.cpp,
          reverse: slot.reverse,
          colorOrder: prop.colorOrder,
          brightness: slot.brightness ?? port.brightness,
          gamma: slot.gamma ?? port.gamma,
        });
        runs.push({ pixels, cpp: layout.cpp });
        channel += pixels * layout.cpp;
      }
    }
    return { spans, runs, channelCount: channel, dmx: null };
  });

  // sACN and Art-Net number their universes apart: pinned starts stay, the rest are numbered up
  // from 1 (sACN) or 0 (Art-Net), skipping pinned ranges.
  const chunked = show.controllers.map((c, i) => {
    const p = c.protocol;
    if (p.type !== "sacn" && p.type !== "artNet") return [];
    const dmx = wired[i].dmx;
    if (dmx !== null) return Array.from({ length: dmx }, (_, n): [number, number] => [n * DMX_UNIVERSE, DMX_UNIVERSE]);
    return universeChunks(wired[i].runs, p.universeSize, p.allowPixelStraddle);
  });
  const starts = (type: "sacn" | "artNet", first: number) => {
    const pinned = show.controllers.flatMap((c, i) =>
      c.protocol.type === type && c.protocol.startUniverse !== null
        ? [[c.protocol.startUniverse, c.protocol.startUniverse + chunked[i].length] as const]
        : [],
    );
    let next = first;
    return show.controllers.map((c, i) => {
      if (c.protocol.type !== type) return 0;
      if (c.protocol.startUniverse !== null) return c.protocol.startUniverse;
      const count = chunked[i].length;
      for (let clash = pinned.find(([s, e]) => count > 0 && next < e && s < next + count); clash; ) {
        next = clash[1];
        clash = pinned.find(([s, e]) => count > 0 && next < e && s < next + count);
      }
      const start = next;
      next += count;
      return start;
    });
  };
  const sacnStarts = starts("sacn", 1);
  const artNetStarts = starts("artNet", 0);
  return show.controllers.map((controller, i) => {
    const { spans, channelCount } = wired[i];
    const protocol = controller.protocol;
    if (protocol.type === "ddp") return { controller: controller.id, channelCount, addressing: { type: "ddp" as const }, spans };
    if (protocol.type === "dmxUsb") return { controller: controller.id, channelCount, addressing: { type: "dmxUsb" as const }, spans };
    const first = protocol.type === "sacn" ? sacnStarts[i] : artNetStarts[i];
    // Universe numbers stop at 65,535 (the engine clamps them the same way; it reports the problem).
    const universes: UniverseSpan[] = chunked[i].map(([controllerChannel, len], n) => ({ universe: Math.min(first + n, MAX_UNIVERSE), controllerChannel, len }));
    if (protocol.type === "artNet") return { controller: controller.id, channelCount, addressing: { type: "artNet" as const, universes }, spans };
    return { controller: controller.id, channelCount, addressing: { type: "sacn" as const, universes, multicast: protocol.multicast }, spans };
  });
}
