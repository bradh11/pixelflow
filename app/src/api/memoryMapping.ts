// Controller channels and sACN universes for the in-memory backend, following the engine's
// pf-mapping closely enough for the Wiring screen (and the demo) to show real channel ranges.
// Wiring problems aren't reported here; the engine does that.

import type { ControllerOutput, OutputSpan, Show, UniverseSpan } from "./types";
import { channelsPerPixel, nodeCount } from "../lib/shows";

interface Run {
  pixels: number;
  cpp: number;
}

/** Universe-sized chunks `[firstChannel, len]` of a controller's pixels (never splitting a pixel
 * unless `straddle`). */
function chunks(runs: Run[], size: number, straddle: boolean): [number, number][] {
  const total = runs.reduce((sum, r) => sum + r.pixels * r.cpp, 0);
  const out: [number, number][] = [];
  if (straddle) {
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
  const layouts = new Map<string, { frameOffset: number; nodes: number; cpp: number }>();
  let offset = 0;
  for (const prop of show.props) {
    const nodes = nodeCount(prop.shape);
    const cpp = channelsPerPixel(prop);
    if (!layouts.has(prop.id)) layouts.set(prop.id, { frameOffset: offset, nodes, cpp });
    offset += nodes * cpp;
  }

  const wired = show.controllers.map((controller) => {
    const spans: OutputSpan[] = [];
    const runs: Run[] = [];
    let channel = 0;
    for (const port of controller.ports) {
      for (const slot of port.slots) {
        const layout = layouts.get(slot.prop);
        const prop = show.props.find((p) => p.id === slot.prop);
        if (!layout || !prop) continue;
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
    return { spans, runs, channelCount: channel };
  });

  // Pinned start universes stay; the rest are numbered from 1 up, skipping pinned ranges.
  const universeChunks = show.controllers.map((c, i) =>
    c.protocol.type === "sacn" ? chunks(wired[i].runs, c.protocol.universeSize, c.protocol.allowPixelStraddle) : [],
  );
  const pinned = show.controllers.flatMap((c, i) =>
    c.protocol.type === "sacn" && c.protocol.startUniverse !== null
      ? [[c.protocol.startUniverse, c.protocol.startUniverse + universeChunks[i].length] as const]
      : [],
  );
  let next = 1;
  return show.controllers.map((controller, i) => {
    const { spans, channelCount } = wired[i];
    const protocol = controller.protocol;
    if (protocol.type === "ddp") return { controller: controller.id, channelCount, addressing: { type: "ddp" as const }, spans };
    const count = universeChunks[i].length;
    let start = protocol.startUniverse;
    if (start === null) {
      for (let clash = pinned.find(([s, e]) => count > 0 && next < e && s < next + count); clash; ) {
        next = clash[1];
        clash = pinned.find(([s, e]) => count > 0 && next < e && s < next + count);
      }
      start = next;
      next += count;
    }
    const first = start;
    const universes: UniverseSpan[] = universeChunks[i].map(([controllerChannel, len], n) => ({ universe: first + n, controllerChannel, len }));
    return { controller: controller.id, channelCount, addressing: { type: "sacn" as const, universes, multicast: protocol.multicast }, spans };
  });
}
