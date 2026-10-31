import { describe, expect, it } from "vitest";
import { emptyShow } from "../api/memory";
import type { ColorOrder, DeviceConfig, Prop, Show, StringConfig } from "../api/types";
import { newFixture } from "./dmx";
import { newController, newProp, nodeCount } from "./shows";
import { clearFits, unwiredFits } from "./unwiredFits";

/** A mega tree of `strings` × 50 pixels. */
function tree(name: string, strings = 16): Prop {
  const prop = { ...newProp("tree", emptyShow("t")), name };
  if (prop.shape.source === "generator" && prop.shape.type === "tree") prop.shape.strings = strings;
  return prop;
}

/** An arch of `nodes` pixels. */
function arch(name: string, nodes = 50): Prop {
  const prop = { ...newProp("arch", emptyShow("t")), name };
  if (prop.shape.source === "generator" && prop.shape.type === "arch") prop.shape.nodes = nodes;
  return prop;
}

const string = (name: string | null, pixels: number, colorOrder: ColorOrder = "RGB"): StringConfig => ({
  name,
  pixels,
  colorOrder,
  nullPixels: 0,
  reverse: false,
  brightness: 100,
  gamma: 1,
  smartReceiver: null,
});

/** A device with one string on each port, in order. */
const device = (...strings: StringConfig[]): DeviceConfig => ({
  input: { type: "ddp" },
  ports: strings.map((s, i) => ({ number: i + 1, strings: [s], maxPixels: null })),
  destinations: [],
  notes: [],
});

function showWith(...props: Prop[]): Show {
  return { ...emptyShow("Home"), props };
}

const names = (show: Show, ids: string[]) => ids.map((id) => show.props.find((p) => p.id === id)?.name);

describe("unwired props that fit a found string", () => {
  it("offers the show's unwired props with the string's pixel count", () => {
    const show = showWith(tree("Mega Tree 1"), arch("Garage Arch"), arch("Big Arch", 100));
    const fits = unwiredFits(show, device(string("Port 1", 800), string(null, 50)), {});
    expect(names(show, fits["port1/string1"].props)).toEqual(["Mega Tree 1"]);
    expect(names(show, fits["port2/string1"].props)).toEqual(["Garage Arch"]);
  });

  it("leaves out props that are wired, DMX fixtures, suggested by name, or would shift pixels", () => {
    const wired = tree("Wired Tree");
    const named = tree("Named Tree");
    const rgbw = { ...tree("RGBW Tree"), colorOrder: "RGBW" as const };
    const flood = newFixture(emptyShow("t"));
    const show = showWith(wired, named, rgbw, flood, tree("Free Tree"));
    const controller = newController("Main", "192.0.2.30", "ddp", 1);
    controller.ports[0].slots = [{ prop: wired.id, segment: null, nullPixels: 0, reverse: false, brightness: null, gamma: null, smartReceiver: null }];
    show.controllers = [controller];
    const config = device(string("Named Tree", 800), string("Tree", 800), string("Flood", nodeCount(flood.shape)));
    const fits = unwiredFits(show, config, { "port1/string1": { prop: named.id, reason: "sameName" } });
    // The string with a name match is left to it, and the matched prop isn't offered again.
    expect(fits["port1/string1"]).toBeUndefined();
    expect(names(show, fits["port2/string1"].props)).toEqual(["Free Tree"]);
    expect(fits["port3/string1"]).toBeUndefined();
  });

  it("keeps to the kind the string's name says, and offers any kind when it says none", () => {
    // A 50-pixel arch and a 50-pixel line.
    const line = { ...newProp("line", emptyShow("t")), name: "Fence" };
    const show = showWith(arch("Arch 1"), line);
    const config = device(string("Driveway Arches", 50), string("Port 2", 50), string("Roofline", 50));
    const fits = unwiredFits(show, config, {});
    expect(fits["port1/string1"]).toEqual({ props: [show.props[0].id], kind: "arch" });
    expect(names(show, fits["port2/string1"].props)).toEqual(["Arch 1", "Fence"]);
    expect(fits["port2/string1"].kind).toBeNull();
    expect(fits["port3/string1"]).toEqual({ props: [line.id], kind: "line" });
  });

  it("goes by the name for a prop whose shape doesn't say its kind", () => {
    const imported: Prop = {
      ...tree("Mega Tree"),
      shape: { source: "measured", provenance: "import", points: Array.from({ length: 800 }, (_, i) => ({ x: i, y: 0, z: 0 })) },
    };
    const show = showWith(imported);
    expect(unwiredFits(show, device(string("MegaTree", 800)), {})["port1/string1"].props).toEqual([imported.id]);
    expect(unwiredFits(show, device(string("Arch", 800)), {})["port1/string1"]).toBeUndefined();
  });

  it("puts the likeliest first: the most words in common, then by name", () => {
    const show = showWith(arch("Arch 10"), arch("Arch 2"), arch("Garage Arch 3"), arch("Arch 1"));
    const fits = unwiredFits(show, device(string("Garage arch", 50)), {});
    expect(names(show, fits["port1/string1"].props)).toEqual(["Garage Arch 3", "Arch 1", "Arch 2", "Arch 10"]);
  });

  it("starts a string on its only fit, never on a prop two strings could be", () => {
    const show = showWith(tree("Mega Tree 1"), arch("Arch 1"), arch("Arch 2"), { ...newProp("star", emptyShow("t")), name: "Star" });
    const fits = unwiredFits(show, device(string("Tree", 800), string("Arch", 50), string("Star left", 100), string("Star right", 100)), {});
    const start = clearFits(fits);
    // The tree is the one 800-pixel prop; the arches are two to pick from; one star fits two strings.
    expect(start).toEqual({ "port1/string1": show.props[0].id });
    expect(fits["port2/string1"].props).toHaveLength(2);
    expect(fits["port3/string1"].props).toEqual(fits["port4/string1"].props);
  });
});
