import { describe, expect, it } from "vitest";
import type { ColorOrder, Controller, DeviceConfig, Prop, Show, StringConfig } from "../api/types";
import { demoShow, demoShowDevices } from "../api/demo";
import setupCases from "../api/setupCases.json";
import { emptyShow } from "../api/memory";
import { applySetup, compareSetup, deviceSetup, diffPorts, matchProps, showSetup, takeFromDevice } from "./deviceSetup";
import { newController, nodeCount } from "./shows";

const line = (name: string, nodes: number): Prop => ({
  id: crypto.randomUUID(),
  name,
  shape: { source: "generator", type: "line", nodes, length: nodes * 0.1 },
  transform: { position: { x: 0, y: 0, z: 0 }, rotationDeg: { x: 0, y: 0, z: 0 }, scale: { x: 1, y: 1, z: 1 } },
  colorOrder: "RGB",
  regions: [],
  tags: [],
});

const slot = (prop: Prop, controllerColorOrder: ColorOrder | null = null) => ({
  prop: prop.id,
  segment: null,
  nullPixels: 0,
  reverse: false,
  brightness: null,
  gamma: null,
  smartReceiver: null,
  controllerColorOrder,
});

const string = (name: string, pixels: number, colorOrder: ColorOrder = "RGB"): StringConfig => ({
  name,
  pixels,
  colorOrder,
  nullPixels: 0,
  reverse: false,
  brightness: 100,
  gamma: 1,
  smartReceiver: null,
});

/** "Arch" (50) and "Gutter" (100) on port 1; "Roof" (200, GRB on the controller) on port 2. */
function setup(): { show: Show; controller: Controller } {
  const show = emptyShow("t");
  const [arch, gutter, roof] = [line("Arch", 50), line("Gutter", 100), line("Roof", 200)];
  const controller = newController("Garage FPP", "192.0.2.30", "ddp", 2);
  controller.ports[0].slots = [slot(arch), slot(gutter)];
  controller.ports[1].slots = [slot(roof, "GRB")];
  show.props = [arch, gutter, roof];
  show.controllers = [controller];
  return { show, controller };
}

const matching = (): DeviceConfig => ({
  input: { type: "ddp" },
  ports: [
    { number: 1, strings: [string("Arch", 50), string("Gutter", 100)], maxPixels: null },
    { number: 2, strings: [string("Roof", 200, "GRB")], maxPixels: null },
  ],
  destinations: [],
  notes: [],
});

describe("device setup", () => {
  it("places the show's strings back to back from channel 1, or adds them up per output", () => {
    const { show, controller } = setup();
    const starts = showSetup(show, controller, false).ports.flatMap((p) => p.strings.map((s) => [p.number, s.name, s.pixels, s.start]));
    expect(starts).toEqual([
      [1, "Arch", 50, 1],
      [1, "Gutter", 100, 151],
      [2, "Roof", 200, 451],
    ]);
    const merged = showSetup(show, controller, true).ports[0].strings;
    expect(merged.map((s) => [s.name, s.pixels, s.slots])).toEqual([["Arch + Gutter", 150, [0, 1]]]);
  });

  it("finds nothing to change on a matching device", () => {
    const { show, controller } = setup();
    expect(compareSetup(show, controller, "fpp", matching()).changes).toEqual([]);
  });

  it("reads differences show → device in plain rows, the same as the engine", () => {
    const { show, controller } = setup();
    const config = matching();
    config.ports[0].strings[1].pixels = 80;
    config.ports[0].strings[0].colorOrder = "GRB";
    config.ports[1].strings = [];
    config.ports.push({ number: 3, strings: [string("Porch", 30, "BGR")], maxPixels: null });
    config.input = { type: "sacn", startUniverse: 7, channelsPerUniverse: 510, universeCount: 2 };
    const rows = compareSetup(show, controller, "falcon", config).changes.map((c) => [c.id, c.subject, c.what, c.before, c.after, c.canTake]);
    expect(rows).toEqual([
      ["input/receives", "", "Receives", "DDP", "sACN (E1.31)", true],
      ["port1/string1/colorOrder", "String 1 · Arch", "Color order", "Not set", "GRB", true],
      ["port1/string2/pixels", "String 2 · Gutter", "Pixels", "100", "80", true],
      ["port2/string1", "String 1 · Roof", "Removed string", "200 pixels", "None", true],
      ["port3/string1", "String 1 · Porch", "New string", "None", "30 pixels, BGR", true],
    ]);
  });

  it("warns about what goes dark when sending", () => {
    const { show, controller } = setup();
    const device = deviceSetup(matching());
    device.ports[0].strings[1].pixels = 120;
    device.ports.push({ number: 4, strings: [device.ports[1].strings[0]] });
    const changes = diffPorts(device, showSetup(show, controller, false), "toDevice");
    expect(changes.map((c) => c.warning)).toEqual(["20 pixels fewer: the last 20 on this string go dark.", "Its 200 pixels go dark."]);
  });

  it("takes only the picked differences, and can wire an existing prop to a new string", () => {
    const { show, controller } = setup();
    const star = line("Star", 30);
    show.props.push(star);
    const config = matching();
    config.ports[0].strings[1].pixels = 80;
    config.ports[0].strings[0].colorOrder = "GRB";
    config.ports.push({ number: 3, strings: [string("Porch", 30, "BGR")], maxPixels: null });
    const taken = takeFromDevice(show, controller, "fpp", config, ["port1/string2/pixels", "port3/string1"], { "port3/string1": star.id });
    expect(taken.changedProps.map((p) => [p.name, nodeCount(p.shape)])).toEqual([["Gutter", 80]]);
    expect(taken.newProps).toEqual([]);
    expect(taken.controller.ports[2].number).toBe(3);
    expect(taken.controller.ports[2].slots[0]).toMatchObject({ prop: star.id, controllerColorOrder: "BGR" });
    expect(taken.controller.ports[0].slots[0].controllerColorOrder).toBeNull();
    expect(() => takeFromDevice(show, controller, "fpp", matching(), ["port1/string2/pixels"])).toThrow(/changed since you compared/);
  });

  it("a sent setup reads back as the show's", () => {
    const { show, controller } = setup();
    const config = matching();
    config.ports[0].strings[1].pixels = 80;
    const target = showSetup(show, controller, false);
    expect(diffPorts(deviceSetup(applySetup(config, target)), target, "toDevice")).toEqual([]);
  });

  it("the demo show's own FPP has differences to compare and send", () => {
    const show = demoShow();
    const [main] = demoShowDevices(show).details;
    const controller = show.controllers.find((c) => c.address === main.device.address)!;
    const ids = compareSetup(show, controller, "fpp", main.config).changes.map((c) => c.id);
    expect(ids).toEqual(["port1/string1/colorOrder", "port1/string2/pixels", "port2/string1/pixels", "port3/string1"]);
    const sent = diffPorts(deviceSetup(main.config), showSetup(show, controller, false), "toDevice").map((c) => c.id);
    expect(sent).toEqual(["port1/string2/pixels", "port2/string1/pixels", "port3/string1"]);
  });

  // The same cases run against the engine's comparison (crates/pf-devices/src/setup.rs).
  type CaseSlot = { name: string; nodes: number; shape?: string; colorOrder?: ColorOrder; controllerColorOrder?: ColorOrder; nullPixels?: number; smartReceiver?: number };
  type CaseString = { name: string | null; pixels: number; colorOrder?: ColorOrder; smartReceiver?: number };
  it.each(setupCases as unknown as { name: string; kind: "fpp" | "falcon" | "wled"; protocol: Controller["protocol"] | { type: "sacn"; startUniverse: number | null; universeSize: number }; show: { port: number; slots: CaseSlot[] }[]; device: { input: DeviceConfig["input"]; ports: { number: number; strings: CaseString[] }[] }; address?: string; props?: CaseSlot[]; expect: { rows?: unknown[][]; notes?: string[]; suggested?: Record<string, [string, string]> } }[])(
    "shared case: $name",
    ({ kind, protocol, show: ports, device, address, props, expect: wanted }) => {
      const show = emptyShow("t");
      const controller = newController("C", "192.0.2.1", "ddp", 0);
      controller.protocol = protocol.type === "sacn" ? { allowPixelStraddle: false, multicast: false, ...protocol } : { type: "ddp" };
      for (const entry of ports) {
        const port = { number: entry.port, maxPixels: null, brightness: 100, gamma: 1, slots: [] as Controller["ports"][number]["slots"] };
        for (const s of entry.slots) {
          const prop = line(s.name, s.nodes);
          if (s.shape === "arch") prop.shape = { source: "generator", type: "arch", nodes: s.nodes, width: 2, height: 1, arches: 1 } as Prop["shape"];
          prop.colorOrder = s.colorOrder ?? "RGB";
          port.slots.push({ ...slot(prop, s.controllerColorOrder ?? null), nullPixels: s.nullPixels ?? 0, smartReceiver: s.smartReceiver ?? null });
          show.props.push(prop);
        }
        controller.ports.push(port);
      }
      show.controllers = [controller];
      // Props in the show that no controller wires.
      for (const p of props ?? []) show.props.push({ ...line(p.name, p.nodes), colorOrder: p.colorOrder ?? "RGB" });
      const config: DeviceConfig = {
        input: device.input,
        ports: device.ports.map((p) => ({
          number: p.number,
          maxPixels: null,
          strings: p.strings.map((s) => ({ ...string(s.name ?? "", s.pixels, s.colorOrder ?? "RGB"), name: s.name, smartReceiver: s.smartReceiver ?? null })),
        })),
        destinations: [],
        notes: [],
      };
      if (wanted.suggested) {
        const found = matchProps(show, { address: address ?? "192.0.2.1", kind }, config);
        const named = Object.fromEntries(Object.entries(found).map(([key, m]) => [key, [show.props.find((p) => p.id === m.prop)!.name, m.reason]]));
        expect(named).toEqual(wanted.suggested);
      }
      if (!wanted.rows) return;
      const { changes, notes } = compareSetup(show, controller, kind, config);
      expect(changes.map((c) => [c.id, c.subject, c.what, c.before, c.after, c.canTake, c.warning])).toEqual(wanted.rows);
      expect(notes).toEqual(wanted.notes);
    },
  );
});
