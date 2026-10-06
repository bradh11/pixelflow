import { describe, expect, it } from "vitest";
import type { Controller, Show } from "../api/types";
import { emptyShow } from "../api/memory";
import { addressProblem, checkDraft, controllerDraft, controllerEdits, type ControllerDraft } from "./controllerEdit";
import { newController } from "./shows";

function wired(): Controller {
  const c = newController("Garage", "10.0.0.20", "ddp", 2);
  c.adapter = "falcon";
  c.ports[0].slots.push({ prop: "p1", segment: null, nullPixels: 0, reverse: false, brightness: null, gamma: null, smartReceiver: null });
  c.sequenceChannels = { start: 1, count: 300 };
  return c;
}

function showWith(...controllers: Controller[]): Show {
  return { ...emptyShow("t"), controllers };
}

const draft = (c: Controller, change: Partial<ControllerDraft> = {}): ControllerDraft => ({ ...controllerDraft(c), ...change });

describe("addressProblem", () => {
  it("accepts what the engine sends to: an IPv4 address or host name, each with an optional :port", () => {
    expect(addressProblem("192.168.1.50")).toBeNull();
    expect(addressProblem("fpp.local")).toBeNull();
    expect(addressProblem("garage-falcon")).toBeNull();
    expect(addressProblem("10.0.0.5:4048")).toBeNull();
    expect(addressProblem("fpp.local:5568")).toBeNull();
    expect(addressProblem("MULTICAST")).toBeNull();
  });

  it("allows no address where none is needed (multicast sACN)", () => {
    expect(addressProblem("", { emptyOk: true })).toBeNull();
    expect(addressProblem("  ")).toBe("Enter the controller's IP address, like 192.168.1.50.");
  });

  it("explains what's wrong in plain words", () => {
    expect(addressProblem("192.168.1.300")).toBe("192.168.1.300 isn't a valid IP address: each of the four numbers must be 0 to 255.");
    expect(addressProblem("192.168.1")).toBe("192.168.1 isn't a complete IP address: it needs four numbers, like 192.168.1.50.");
    expect(addressProblem("my falcon")).toBe("An address can't have spaces. Enter an IP address like 192.168.1.50, or a name like fpp.local.");
    expect(addressProblem("http://10.0.0.5/")).toBe("Enter just the address (like 10.0.0.5), without http:// or a slash.");
    expect(addressProblem("10.0.0.5:")).toBe("The port after the colon must be a number from 1 to 65535, or leave the colon off.");
    expect(addressProblem("10.0.0.5:70000")).toBe("The port after the colon must be a number from 1 to 65535, or leave the colon off.");
    expect(addressProblem("fe80::1")).toBe("PixelFlow sends to IPv4 addresses. Enter one like 192.168.1.50.");
  });

  it("refuses numbers with a leading zero, which some systems read as octal", () => {
    expect(addressProblem("192.168.1.050")).toBe(
      "192.168.1.050 has a number that starts with 0, which some computers read differently. Write it as 192.168.1.50.",
    );
    expect(addressProblem("10.0.0.0")).toBeNull();
  });
});

describe("checkDraft", () => {
  it("opens clean on every controller an import can make, so Save works for any one change", () => {
    // Split xLights controllers share one IP; multicast ones may have no address or "MULTICAST".
    const front = { ...newController("Front (universes 1–4)", "10.0.0.5", "sacn", 1) };
    const back = { ...newController("Front (universes 10–12)", "10.0.0.5", "sacn", 1) };
    const multicast = newController("Yard", "", "sacn", 1);
    multicast.protocol = { type: "sacn", startUniverse: 70, universeSize: 510, allowPixelStraddle: false, multicast: true };
    const named = newController("Yard 2", "MULTICAST", "sacn", 1);
    const ported = newController("Bench", "127.0.0.2:4048", "ddp", 1);
    const show = showWith(front, back, multicast, named, ported);
    for (const c of show.controllers) {
      expect(checkDraft(draft(c), c, show)).toEqual({ problems: {}, warnings: {} });
      expect(checkDraft(draft(c, { name: `${c.name}!` }), c, show).problems).toEqual({});
    }
    // Only what changed is checked: a split controller's protocol can change.
    expect(checkDraft(draft(front, { protocol: "ddp" }), front, show).problems).toEqual({});
  });

  it("needs a name; another controller with the same name is only a warning", () => {
    const a = wired();
    const b = newController("Porch", "10.0.0.21", "ddp", 1);
    const show = showWith(a, b);
    expect(checkDraft(draft(a, { name: "  " }), a, show).problems.name).toBe("Give the controller a name.");
    const same = checkDraft(draft(a, { name: "porch" }), a, show);
    expect(same.problems).toEqual({});
    expect(same.warnings.name).toBe("Porch has that name too. That's allowed, but it's easy to mix them up.");
  });

  it("checks a changed address, and warns (doesn't refuse) when another controller has it", () => {
    const a = wired();
    const b = newController("Porch", "10.0.0.21", "ddp", 1);
    const show = showWith(a, b);
    expect(checkDraft(draft(a, { address: "10.0.0.300" }), a, show).problems.address).toMatch(/isn't a valid IP address/);
    const shared = checkDraft(draft(a, { address: "10.0.0.21" }), a, show);
    expect(shared.problems).toEqual({});
    expect(shared.warnings.address).toBe("Porch also uses 10.0.0.21. That's fine if it's the same controller.");
  });

  it("needs an address once multicast is turned off", () => {
    const yard = newController("Yard", "", "sacn", 1);
    yard.protocol = { type: "sacn", startUniverse: null, universeSize: 510, allowPixelStraddle: false, multicast: true };
    const show = showWith(yard);
    expect(checkDraft(draft(yard, { multicast: false }), yard, show).problems.address).toBe("Enter the controller's IP address, like 192.168.1.50.");
    expect(checkDraft(draft(yard, { protocol: "ddp" }), yard, show).problems.address).toBeDefined();
  });

  it("checks a changed start universe: whole digits from 1 to 63999, or empty", () => {
    const a = wired();
    const show = showWith(a);
    const universe = (text: string) => checkDraft(draft(a, { protocol: "sacn", startUniverse: text }), a, show).problems.startUniverse;
    expect(universe("")).toBeUndefined();
    expect(universe("20")).toBeUndefined();
    for (const bad of ["0", "1.5", "64000", "1e3", "0x10", " 7 x"]) {
      expect(universe(bad)).toBe("The start universe must be a whole number from 1 to 63999, or empty to let PixelFlow choose.");
    }
  });
});

describe("controllerEdits", () => {
  it("changes name, address, and protocol in one edit and keeps the wiring and the device link", () => {
    const a = wired();
    const show = showWith(a);
    const edits = controllerEdits(a.id, draft(a, { name: " Garage Falcon ", address: " 10.0.0.30 ", protocol: "sacn", startUniverse: "20", universeSize: 512, multicast: true }))(show);
    expect(edits).toHaveLength(1);
    const edit = edits[0];
    if (edit.type !== "updateController") throw new Error(edit.type);
    expect(edit.controller).toEqual({
      ...a,
      name: "Garage Falcon",
      address: "10.0.0.30",
      protocol: { type: "sacn", startUniverse: 20, universeSize: 512, allowPixelStraddle: false, multicast: true },
    });
    // Wiring, adapter, and the sequence channels found on the network are kept.
    expect(edit.controller.ports).toBe(a.ports);
    expect(edit.controller.sequenceChannels).toEqual({ start: 1, count: 300 });
    expect(edit.controller.adapter).toBe("falcon");
  });

  it("is built from the show as it is when its turn comes", () => {
    const a = wired();
    const later = { ...a, ports: [...a.ports, { number: 3, maxPixels: null, brightness: 100, gamma: 1, slots: [] }] };
    const edits = controllerEdits(a.id, draft(a, { name: "New" }))(showWith(later));
    expect(edits[0].type === "updateController" && edits[0].controller.ports).toHaveLength(3);
  });

  it("sends nothing when nothing changed or the controller is gone", () => {
    const a = wired();
    expect(controllerEdits(a.id, draft(a))(showWith(a))).toEqual([]);
    expect(controllerEdits(a.id, draft(a, { name: "X" }))(showWith())).toEqual([]);
  });

  it("an empty start universe lets PixelFlow choose; DDP drops the sACN settings", () => {
    const a = wired();
    const sacn = controllerEdits(a.id, draft(a, { protocol: "sacn" }))(showWith(a))[0];
    expect(sacn.type === "updateController" && sacn.controller.protocol).toEqual({ type: "sacn", startUniverse: null, universeSize: 510, allowPixelStraddle: false, multicast: false });
    const back = { ...a, protocol: { type: "sacn" as const, startUniverse: 4, universeSize: 510 as const, allowPixelStraddle: false, multicast: false } };
    const auto = controllerEdits(a.id, draft(back, { startUniverse: "" }))(showWith(back))[0];
    expect(auto.type === "updateController" && auto.controller.protocol).toMatchObject({ startUniverse: null });
    const ddp = controllerEdits(a.id, draft(back, { protocol: "ddp" }))(showWith(back))[0];
    expect(ddp.type === "updateController" && ddp.controller.protocol).toEqual({ type: "ddp" });
  });
});
