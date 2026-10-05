import { describe, expect, it } from "vitest";
import type { PortSlot, Prop, Show } from "./types";
import { emptyShow } from "./memory";
import { mapControllers } from "./memoryMapping";
import { newController, newProp } from "../lib/shows";

const slot = (prop: Prop, overrides: Partial<PortSlot> = {}): PortSlot => ({
  prop: prop.id,
  segment: null,
  nullPixels: 0,
  reverse: false,
  brightness: null,
  gamma: null,
  smartReceiver: null,
  ...overrides,
});

function show(): { show: Show; arch: Prop; line: Prop } {
  const s = emptyShow("t");
  const arch = { ...newProp("arch", s), name: "Arch" }; // 50 pixels
  const line = { ...newProp("line", s), name: "Line", colorOrder: "RGBW" as const }; // 50 pixels, 4 channels each
  s.props = [arch, line];
  return { show: s, arch, line };
}

describe("memory channel mapping (like the engine's)", () => {
  it("places slots on controller channels in wiring order, after null pixels", () => {
    const { show: s, arch, line } = show();
    const c = newController("C", "10.0.0.1", "ddp", 2);
    c.ports[0].slots = [slot(arch, { nullPixels: 2, reverse: true }), slot(line, { segment: { start: 10, end: 20 } })];
    c.ports[1].slots = [slot(arch, { segment: { start: 0, end: 5 } })];
    s.controllers = [c];
    const [out] = mapControllers(s);
    expect(out.controller).toBe(c.id);
    expect(out.addressing).toEqual({ type: "ddp" });
    expect(out.spans.map((sp) => [sp.port, sp.controllerChannel, sp.pixels, sp.channelsPerPixel, sp.frameOffset, sp.reverse])).toEqual([
      [1, 6, 50, 3, 0, true],
      [1, 156, 10, 4, 150 + 40, false],
      [2, 196, 5, 3, 0, false],
    ]);
    expect(out.channelCount).toBe(211);
  });

  it("skips slots whose prop is gone or whose pixels don't fit", () => {
    const { show: s, arch } = show();
    const c = newController("C", "10.0.0.1", "ddp", 1);
    c.ports[0].slots = [{ ...slot(arch), prop: "missing" }, slot(arch, { segment: { start: 40, end: 60 } }), slot(arch)];
    s.controllers = [c];
    expect(mapControllers(s)[0].spans.map((sp) => sp.controllerChannel)).toEqual([0]);
  });

  it("packs sACN universes without splitting pixels and numbers them from 1", () => {
    const { show: s, arch } = show();
    const big = { ...newProp("matrix", s), name: "Matrix" }; // 512 pixels
    s.props.push(big);
    const a = newController("A", "10.0.0.1", "sacn", 1);
    a.ports[0].slots = [slot(big)];
    const b = newController("B", "10.0.0.2", "sacn", 1);
    b.ports[0].slots = [slot(arch)];
    s.controllers = [a, b];
    const [outA, outB] = mapControllers(s);
    expect(outA.addressing).toEqual({
      type: "sacn",
      multicast: false,
      universes: [
        { universe: 1, controllerChannel: 0, len: 510 },
        { universe: 2, controllerChannel: 510, len: 510 },
        { universe: 3, controllerChannel: 1020, len: 510 },
        { universe: 4, controllerChannel: 1530, len: 6 },
      ],
    });
    expect(outB.addressing).toEqual({ type: "sacn", multicast: false, universes: [{ universe: 5, controllerChannel: 0, len: 150 }] });
  });

  it("keeps a pinned start universe", () => {
    const { show: s, arch } = show();
    const a = newController("A", "10.0.0.1", "sacn", 1);
    a.protocol = { type: "sacn", startUniverse: 40, universeSize: 512, allowPixelStraddle: true, multicast: true };
    a.ports[0].slots = [slot(arch)];
    s.controllers = [a];
    expect(mapControllers(s)[0].addressing).toEqual({ type: "sacn", multicast: true, universes: [{ universe: 40, controllerChannel: 0, len: 150 }] });
  });

  it("never numbers a universe past 65,535, like the engine", () => {
    const { show: s, arch } = show();
    const a = newController("A", "10.0.0.1", "sacn", 1);
    a.protocol = { type: "sacn", startUniverse: 65535, universeSize: 510, allowPixelStraddle: false, multicast: false };
    const big = { ...newProp("matrix", s), name: "Matrix" }; // 512 pixels: 4 universes
    s.props.push(big);
    a.ports[0].slots = [slot(big), slot(arch)];
    s.controllers = [a];
    const out = mapControllers(s)[0].addressing;
    expect(out.type === "sacn" && out.universes.map((u) => u.universe)).toEqual([65535, 65535, 65535, 65535]);
  });
});
