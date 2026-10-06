import { describe, expect, it } from "vitest";
import type { Controller, Show } from "../api/types";
import { emptyShow } from "../api/memory";
import { addressProblem, controllerDraft, controllerEdits, draftProblems, type ControllerDraft } from "./controllerEdit";
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
  it("accepts IPv4 addresses and host names", () => {
    expect(addressProblem("192.168.1.50")).toBeNull();
    expect(addressProblem("fpp.local")).toBeNull();
    expect(addressProblem("garage-falcon")).toBeNull();
  });

  it("explains what's wrong in plain words", () => {
    expect(addressProblem("")).toBe("Enter the controller's IP address, like 192.168.1.50.");
    expect(addressProblem("192.168.1.300")).toBe("192.168.1.300 isn't a valid IP address: each of the four numbers must be 0 to 255.");
    expect(addressProblem("192.168.1")).toBe("192.168.1 isn't a complete IP address: it needs four numbers, like 192.168.1.50.");
    expect(addressProblem("my falcon")).toBe("An address can't have spaces. Enter an IP address like 192.168.1.50, or a name like fpp.local.");
    expect(addressProblem("http://10.0.0.5/")).toBe("Enter just the address (like 10.0.0.5), without http:// or a slash.");
  });
});

describe("draftProblems", () => {
  it("needs a name not used by another controller", () => {
    const a = wired();
    const b = newController("Porch", "10.0.0.21", "ddp", 1);
    const show = showWith(a, b);
    expect(draftProblems(draft(a, { name: "  " }), show, a.id).name).toBe("Give the controller a name.");
    expect(draftProblems(draft(a, { name: "porch" }), show, a.id).name).toBe("Another controller is already called porch.");
    expect(draftProblems(draft(a), show, a.id)).toEqual({});
  });

  it("warns when another controller has the address", () => {
    const a = wired();
    const b = newController("Porch", "10.0.0.21", "ddp", 1);
    expect(draftProblems(draft(a, { address: "10.0.0.21" }), showWith(a, b), a.id).address).toBe("Porch already uses 10.0.0.21.");
  });

  it("checks the start universe only for sACN with one pinned", () => {
    const a = wired();
    const show = showWith(a);
    expect(draftProblems(draft(a, { protocol: "sacn", startUniverse: "" }), show, a.id)).toEqual({});
    expect(draftProblems(draft(a, { protocol: "sacn", startUniverse: "0" }), show, a.id).startUniverse).toBe(
      "The start universe must be a whole number from 1 to 63999, or empty to let PixelFlow choose.",
    );
    expect(draftProblems(draft(a, { protocol: "sacn", startUniverse: "1.5" }), show, a.id).startUniverse).toBeDefined();
    expect(draftProblems(draft(a, { protocol: "sacn", startUniverse: "64000" }), show, a.id).startUniverse).toBeDefined();
    expect(draftProblems(draft(a, { protocol: "ddp", startUniverse: "0" }), show, a.id)).toEqual({});
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
    const ddp = controllerEdits(a.id, draft(back, { protocol: "ddp" }))(showWith(back))[0];
    expect(ddp.type === "updateController" && ddp.controller.protocol).toEqual({ type: "ddp" });
  });
});
