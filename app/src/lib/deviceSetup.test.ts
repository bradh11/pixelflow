import { describe, expect, it } from "vitest";
import type { ColorOrder, Controller, DeviceConfig, Prop, Show, StringConfig } from "../api/types";
import { demoShow, demoShowDevices } from "../api/demo";
import { emptyShow } from "../api/memory";
import { applySetup, compareSetup, deviceSetup, diffPorts, showSetup, takeFromDevice } from "./deviceSetup";
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
    const rows = compareSetup(show, controller, "fpp", config).changes.map((c) => [c.id, c.subject, c.what, c.before, c.after, c.canTake]);
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
    expect(ids).toEqual(["input/receives", "port1/string1/colorOrder", "port1/string2/pixels", "port2/string1/pixels", "port3/string1"]);
    const sent = diffPorts(deviceSetup(main.config), showSetup(show, controller, false), "toDevice").map((c) => c.id);
    expect(sent).toEqual(["port1/string2/pixels", "port2/string1/pixels", "port3/string1"]);
  });
});
