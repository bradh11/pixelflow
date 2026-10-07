import { describe, expect, it } from "vitest";
import { emptyShow } from "../api/memory";
import { mapControllers } from "../api/memoryMapping";
import type { ChannelMap, Controller, Edit, PortSlot, PreviewProp, Prop, Show } from "../api/types";
import { newController, newProp } from "./shows";
import {
  addPortEdits,
  blankSlot,
  dropIndex,
  firstGap,
  moveSlotByEdits,
  moveSlotEdits,
  portCapacity,
  portChannels,
  portPixels,
  propWiring,
  removePortEdits,
  renumberPortEdits,
  resolveSlot,
  slotLabel,
  slotRefAt,
  unwireEdits,
  unwiredInLayoutOrder,
  updatePortEdits,
  updateSlotEdits,
  wirePropEdits,
  wireRemainingEdits,
  wiringPath,
  wiringProblems,
} from "./wiringMath";

function prop(kind: "arch" | "line" | "star", name: string, x = 0): Prop {
  const p = { ...newProp(kind, emptyShow("x")), name };
  p.transform.position = { x, y: 0, z: 0 };
  return p;
}

/** Arch (50 px), Line (50 px), Star (100 px); controller A has 2 ports, B has 1. */
function fixture() {
  const show = emptyShow("t");
  const arch = prop("arch", "Arch", 5);
  const line = prop("line", "Line", -5);
  const star = prop("star", "Star", 0);
  show.props = [arch, line, star];
  const a = newController("A", "10.0.0.1", "ddp", 2);
  const b = newController("B", "10.0.0.2", "ddp", 1);
  show.controllers = [a, b];
  const nodes = new Map(show.props.map((p) => [p.id, p.id === star.id ? 100 : 50]));
  return { show, arch, line, star, a, b, nodes };
}

/** Applies updateController edits to a copy of the show. */
function applied(show: Show, edits: Edit[]): Show {
  const next = structuredClone(show);
  for (const e of edits) {
    if (e.type !== "updateController") throw new Error(`unexpected ${e.type}`);
    next.controllers = next.controllers.map((c) => (c.id === e.controller.id ? e.controller : c));
  }
  return next;
}

const props = (c: Controller, port: number) => c.ports.find((p) => p.number === port)!.slots.map((s) => s.prop);

/** The slot at `index` on the port, by identity. */
const at = (show: Show, controller: string, port: number, index: number) => slotRefAt(show, { controller, port }, index)!;

describe("moving slots", () => {
  it("wires an unwired prop where it is dropped, as one edit", () => {
    const { show, arch, line, a } = fixture();
    a.ports[0].slots = [blankSlot(arch.id)];
    const edits = wirePropEdits(show, line.id, { controller: a.id, port: 1, index: 0 }, fixture().nodes);
    expect(edits).toHaveLength(1);
    expect(props(applied(show, edits).controllers[0], 1)).toEqual([line.id, arch.id]);
  });

  it("reorders within a port, wherever it is dropped", () => {
    const { show, arch, line, star, a } = fixture();
    a.ports[0].slots = [blankSlot(arch.id), blankSlot(line.id), blankSlot(star.id)];
    const after = (from: number, to: number) => props(applied(show, moveSlotEdits(show, at(show, a.id, 1, from), { controller: a.id, port: 1, index: to })).controllers[0], 1);
    expect(after(0, 3)).toEqual([line.id, star.id, arch.id]);
    expect(after(0, 2)).toEqual([line.id, arch.id, star.id]);
    expect(after(2, 0)).toEqual([star.id, arch.id, line.id]);
    // Dropping right where it is changes nothing.
    expect(moveSlotEdits(show, at(show, a.id, 1, 1), { controller: a.id, port: 1, index: 1 })).toEqual([]);
    expect(moveSlotEdits(show, at(show, a.id, 1, 1), { controller: a.id, port: 1, index: 2 })).toEqual([]);
  });

  it("moves to another port, or another controller, keeping the slot's settings", () => {
    const { show, arch, line, a, b } = fixture();
    a.ports[0].slots = [{ ...blankSlot(arch.id), reverse: true, nullPixels: 3 }, blankSlot(line.id)];
    const sameController = moveSlotEdits(show, at(show, a.id, 1, 0), { controller: a.id, port: 2, index: 0 });
    expect(sameController).toHaveLength(1);
    const one = applied(show, sameController).controllers[0];
    expect(props(one, 1)).toEqual([line.id]);
    expect(one.ports[1].slots[0]).toEqual({ ...blankSlot(arch.id), reverse: true, nullPixels: 3 });

    const across = moveSlotEdits(show, at(show, a.id, 1, 1), { controller: b.id, port: 1, index: 0 });
    expect(across).toHaveLength(2);
    const moved = applied(show, across);
    expect(props(moved.controllers[0], 1)).toEqual([arch.id]);
    expect(props(moved.controllers[1], 1)).toEqual([line.id]);
  });

  it("dragging an already wired prop from the list moves its one slot", () => {
    const { show, arch, a, b, nodes } = fixture();
    a.ports[0].slots = [blankSlot(arch.id)];
    const moved = applied(show, wirePropEdits(show, arch.id, { controller: b.id, port: 1, index: 0 }, nodes));
    expect(props(moved.controllers[0], 1)).toEqual([]);
    expect(props(moved.controllers[1], 1)).toEqual([arch.id]);
  });

  it("a partly wired prop gets a slot for the pixels that aren't wired yet", () => {
    const { show, arch, a, nodes } = fixture();
    a.ports[0].slots = [{ ...blankSlot(arch.id), segment: { start: 0, end: 20 } }];
    const next = applied(show, wirePropEdits(show, arch.id, { controller: a.id, port: 2, index: 0 }, nodes));
    expect(next.controllers[0].ports[1].slots[0].segment).toEqual({ start: 20, end: 50 });
  });

  it("a prop wired in pieces everywhere can't be dropped again from the list", () => {
    const { show, arch, a, nodes } = fixture();
    a.ports[0].slots = [{ ...blankSlot(arch.id), segment: { start: 0, end: 20 } }, { ...blankSlot(arch.id), segment: { start: 20, end: 50 } }];
    expect(wirePropEdits(show, arch.id, { controller: a.id, port: 2, index: 0 }, nodes)).toEqual([]);
  });

  it("unwires, and edits a slot or a port", () => {
    const { show, arch, line, a } = fixture();
    a.ports[0].slots = [blankSlot(arch.id), blankSlot(line.id)];
    expect(props(applied(show, unwireEdits(show, at(show, a.id, 1, 0))).controllers[0], 1)).toEqual([line.id]);
    const reversed = applied(show, updateSlotEdits(show, at(show, a.id, 1, 1), (s) => ({ ...s, reverse: true })));
    expect(reversed.controllers[0].ports[0].slots[1].reverse).toBe(true);
    const limited = applied(show, updatePortEdits(show, { controller: a.id, port: 2 }, (p) => ({ ...p, maxPixels: 100 })));
    expect(limited.controllers[0].ports[1].maxPixels).toBe(100);
    // Gone since: nothing to send.
    expect(unwireEdits(show, { controller: a.id, port: 1, index: 5, prop: "gone", segment: null })).toEqual([]);
    expect(updateSlotEdits(show, { controller: "nope", port: 1, index: 0, prop: arch.id, segment: null }, (s) => s)).toEqual([]);
  });

  it("adds, removes, and renumbers ports", () => {
    const { show, a } = fixture();
    a.ports[1].maxPixels = 1024;
    const added = applied(show, addPortEdits(show, a.id)).controllers[0];
    expect(added.ports.map((p) => p.number)).toEqual([1, 2, 3]);
    // A new port takes the limit the others share.
    expect(added.ports[2].maxPixels).toBeNull();
    a.ports[0].maxPixels = 1024;
    expect(applied(show, addPortEdits(show, a.id)).controllers[0].ports[2].maxPixels).toBe(1024);

    expect(applied(show, removePortEdits(show, { controller: a.id, port: 1 })).controllers[0].ports.map((p) => p.number)).toEqual([2]);
    expect(applied(show, renumberPortEdits(show, { controller: a.id, port: 2 }, 8)).controllers[0].ports.map((p) => p.number)).toEqual([1, 8]);
    // Ports are kept in number order, and a number already used is refused.
    expect(applied(show, renumberPortEdits(show, { controller: a.id, port: 1 }, 5)).controllers[0].ports.map((p) => p.number)).toEqual([2, 5]);
    expect(renumberPortEdits(show, { controller: a.id, port: 1 }, 2)).toEqual([]);
  });

  it("wires the remaining props onto the end of a port, skipping any wired since", () => {
    const { show, arch, line, star, a, nodes } = fixture();
    a.ports[0].slots = [blankSlot(arch.id)];
    expect(props(applied(show, wireRemainingEdits(show, { controller: a.id, port: 1 }, [line.id, star.id], nodes)).controllers[0], 1)).toEqual([arch.id, line.id, star.id]);
    expect(wireRemainingEdits(show, { controller: a.id, port: 1 }, [], nodes)).toEqual([]);
    // The list was made before the line got wired elsewhere: it isn't wired twice.
    a.ports[1].slots = [blankSlot(line.id)];
    expect(props(applied(show, wireRemainingEdits(show, { controller: a.id, port: 1 }, [line.id, star.id], nodes)).controllers[0], 1)).toEqual([arch.id, star.id]);
  });

  it("finds a slot by its prop after the port changed, and does nothing once it's gone", () => {
    const { show, arch, line, star, a } = fixture();
    a.ports[0].slots = [blankSlot(arch.id), blankSlot(line.id), blankSlot(star.id)];
    const line1 = at(show, a.id, 1, 1);
    // An earlier edit (a drop, or another key press) put the star first: the line is now 3rd.
    a.ports[0].slots = [blankSlot(star.id), blankSlot(arch.id), blankSlot(line.id)];
    expect(resolveSlot(a.ports[0], line1)).toBe(2);
    const reversed = applied(show, updateSlotEdits(show, line1, (s) => ({ ...s, reverse: true })));
    expect(reversed.controllers[0].ports[0].slots.map((s) => s.reverse)).toEqual([false, false, true]);
    expect(props(applied(show, unwireEdits(show, line1)).controllers[0], 1)).toEqual([star.id, arch.id]);
    // Gone (undone, or moved to another port): nothing happens.
    a.ports[0].slots = [blankSlot(star.id), blankSlot(arch.id)];
    expect(resolveSlot(a.ports[0], line1)).toBeNull();
    expect(unwireEdits(show, line1)).toEqual([]);
    expect(moveSlotEdits(show, line1, { controller: a.id, port: 2, index: 0 })).toEqual([]);
    expect(updateSlotEdits(show, line1, (s) => s)).toEqual([]);
  });

  it("tells pieces of one prop apart by their pixels, and follows a piece whose range was just changed", () => {
    const { show, arch, a } = fixture();
    const [first, second] = [{ ...blankSlot(arch.id), segment: { start: 0, end: 25 } }, { ...blankSlot(arch.id), segment: { start: 25, end: 50 } }];
    a.ports[0].slots = [first, second];
    const ref = at(show, a.id, 1, 1);
    a.ports[0].slots = [blankSlot("x"), first, second];
    expect(resolveSlot(a.ports[0], ref)).toBe(2);
    // Its own range changed in place (from its settings): still the same slot.
    a.ports[0].slots = [first, { ...second, segment: { start: 25, end: 40 } }];
    expect(resolveSlot(a.ports[0], ref)).toBe(1);
    // Two identical slots and neither where it was: can't tell which, so nothing.
    a.ports[0].slots = [blankSlot("x"), blankSlot("y"), blankSlot("z"), second, second];
    expect(resolveSlot(a.ports[0], { ...ref, index: 0 })).toBeNull();
  });

  it("a held key moves the same prop each time, even when the presses were read from an old show", () => {
    const { show, arch, line, star, a } = fixture();
    a.ports[0].slots = [blankSlot(arch.id), blankSlot(line.id), blankSlot(star.id)];
    const arch0 = at(show, a.id, 1, 0);
    const once = applied(show, moveSlotByEdits(show, arch0, 1));
    expect(props(once.controllers[0], 1)).toEqual([line.id, arch.id, star.id]);
    // The repeat was built from the same render (index 0), but runs on the new show.
    const twice = applied(once, moveSlotByEdits(once, arch0, 1));
    expect(props(twice.controllers[0], 1)).toEqual([line.id, star.id, arch.id]);
    expect(moveSlotByEdits(twice, arch0, 1)).toEqual([]);
    expect(props(applied(twice, moveSlotByEdits(twice, arch0, -1)).controllers[0], 1)).toEqual([line.id, arch.id, star.id]);
  });

  it("edits only the port it was given when two share a number", () => {
    const { show, arch, line, star, a, nodes } = fixture();
    a.ports[1].number = 1;
    a.ports[0].slots = [blankSlot(arch.id)];
    a.ports[1].slots = [blankSlot(line.id)];
    const second = slotRefAt(show, { controller: a.id, port: 1, at: 1 }, 0)!;
    expect(second.prop).toBe(line.id);
    const reversed = applied(show, updateSlotEdits(show, second, (s) => ({ ...s, reverse: true })));
    expect(reversed.controllers[0].ports.map((p) => p.slots[0].reverse)).toEqual([false, true]);
    const removed = applied(show, removePortEdits(show, { controller: a.id, port: 1, at: 1 })).controllers[0];
    expect(removed.ports.map((p) => p.slots.map((s) => s.prop))).toEqual([[arch.id]]);
    // Without saying which, an ambiguous port is left alone.
    expect(removePortEdits(show, { controller: a.id, port: 1 })).toEqual([]);
    expect(wirePropEdits(show, star.id, { controller: a.id, port: 1, index: 0 }, nodes)).toEqual([]);
    expect(wirePropEdits(show, star.id, { controller: a.id, port: 1, at: 0, index: 0 }, nodes)).toHaveLength(1);
    // Renumbering the second one fixes it.
    expect(applied(show, renumberPortEdits(show, { controller: a.id, port: 1, at: 1 }, 2)).controllers[0].ports.map((p) => p.number)).toEqual([1, 2]);
  });
});

describe("capacity", () => {
  it("counts null pixels and segments", () => {
    const { arch, line, nodes } = fixture();
    const port = { number: 1, maxPixels: null, brightness: 100, gamma: 1, slots: [{ ...blankSlot(arch.id), nullPixels: 2 }, { ...blankSlot(line.id), segment: { start: 0, end: 10 } }] };
    expect(portPixels(port, nodes)).toBe(62);
    expect(portCapacity(port, nodes)).toEqual({ used: 62, limit: null, refresh: null, level: "none", message: null, receivers: [] });
  });

  it("counts the smart receivers on a port against its one limit, as xLights does", () => {
    const { arch, line, star, nodes } = fixture();
    const on = (prop: string, smartReceiver: number | null) => ({ ...blankSlot(prop), smartReceiver });
    const port = { number: 17, maxPixels: 220, brightness: 100, gamma: 1, slots: [on(arch.id, 1), on(line.id, 2), on(star.id, 3)] };
    // 50 + 50 + 100 = 200 of 220: nearly full, with each receiver's share.
    expect(portCapacity(port, nodes)).toEqual({
      used: 200,
      limit: 220,
      refresh: null,
      level: "near",
      message: "Nearly full: 200 of 220 pixels, shared by receivers A, B, C.",
      receivers: [
        { receiver: 1, used: 50 },
        { receiver: 2, used: 50 },
        { receiver: 3, used: 100 },
      ],
    });
    // Each receiver alone fits 150, but together they're over: one warning for the port.
    port.maxPixels = 150;
    expect(portCapacity(port, nodes)).toMatchObject({
      used: 200,
      level: "over",
      message: "50 pixels more than this port can drive (200 of 150, shared by receivers A, B, C). Move a prop to another port.",
    });
    // A receiver used twice adds up; pixels wired straight to the port count too.
    port.slots = [on(arch.id, 1), on(line.id, null), on(star.id, 1)];
    expect(portCapacity(port, nodes).receivers).toEqual([
      { receiver: 1, used: 150 },
      { receiver: null, used: 50 },
    ]);
    // One receiver: no "shared by".
    port.slots = [on(arch.id, 2)];
    port.maxPixels = 50;
    expect(portCapacity(port, nodes)).toMatchObject({ level: "near", message: "Nearly full: 50 of 50 pixels.", receivers: [{ receiver: 2, used: 50 }] });
  });

  it("counts RGBW pixels by their channels, as the boards do", () => {
    const { arch, nodes } = fixture();
    const port = { number: 1, maxPixels: 60, brightness: 100, gamma: 1, slots: [blankSlot(arch.id)] };
    const cpp = new Map([[arch.id, 4]]);
    // 50 RGBW pixels carry 200 channels: the time of 67 RGB pixels.
    expect(portCapacity(port, nodes, { cpp })).toMatchObject({
      used: 67,
      level: "over",
      message: "7 pixels more than this port can drive (67 of 60; RGBW pixels count as 1⅓). Move a prop to another port.",
    });
  });

  it("warns when a Falcon port has more pixels than it refreshes in time at the show's frame rate", () => {
    const { star, nodes } = fixture();
    const big = new Map([...nodes, [star.id, 900]]);
    const port = { number: 1, maxPixels: 1024, brightness: 100, gamma: 1, slots: [blankSlot(star.id)] };
    // xLights: a Falcon V4/V5 port refreshes about 704 pixels at 40 fps.
    expect(portCapacity(port, big, { adapter: "falcon", fps: 40 })).toMatchObject({
      used: 900,
      refresh: 704,
      level: "slow",
      message: "At 40 fps this port refreshes about 704 pixels in time; with 900 it will slow down. Move a prop to another port, or lower the show's frame rate.",
    });
    // At 20 fps it drives its full 1,024 in time.
    expect(portCapacity(port, big, { adapter: "falcon", fps: 20 })).toMatchObject({ refresh: null, level: "ok" });
    // At 50 fps, fewer.
    expect(portCapacity(port, big, { adapter: "falcon", fps: 50 })).toMatchObject({ refresh: 563, level: "slow" });
    // Other controllers: only the limit they were given.
    expect(portCapacity(port, big, { adapter: "fpp", fps: 40 })).toMatchObject({ refresh: null, level: "ok" });
  });

  it("is amber when nearly full and red when over, saying so plainly", () => {
    const { arch, star, nodes } = fixture();
    const port = (limit: number) => ({ number: 1, maxPixels: limit, brightness: 100, gamma: 1, slots: [blankSlot(arch.id), blankSlot(star.id)] });
    expect(portCapacity(port(1024), nodes)).toMatchObject({ used: 150, limit: 1024, level: "ok", message: null });
    expect(portCapacity(port(160), nodes)).toMatchObject({ level: "near", message: "Nearly full: 150 of 160 pixels." });
    expect(portCapacity(port(150), nodes)).toMatchObject({ level: "near" });
    expect(portCapacity(port(100), nodes)).toMatchObject({
      level: "over",
      message: "50 pixels more than this port can drive (150 of 100). Move a prop to another port.",
    });
  });
});

describe("prop status", () => {
  it("tells unwired, wired, partly wired, and wired twice apart", () => {
    const { show, arch, line, star, a, b, nodes } = fixture();
    a.ports[0].slots = [blankSlot(arch.id), { ...blankSlot(line.id), segment: { start: 0, end: 30 } }];
    b.ports[0].slots = [blankSlot(arch.id)];
    const wiring = propWiring(show, nodes);
    expect(wiring.get(star.id)).toMatchObject({ status: "unwired", places: [], wiredPixels: 0 });
    expect(wiring.get(line.id)).toMatchObject({ status: "partial", wiredPixels: 30, nodes: 50 });
    expect(wiring.get(arch.id)!.status).toBe("twice");
    expect(wiring.get(arch.id)!.places.map((p) => [p.controllerName, p.port, p.index])).toEqual([
      ["A", 1, 0],
      ["B", 1, 0],
    ]);
    b.ports[0].slots = [];
    expect(propWiring(show, nodes).get(arch.id)!.status).toBe("wired");
  });

  it("a prop with no pixels counts as wired once it's on a port", () => {
    const { show, star, a, nodes } = fixture();
    const empty = new Map([...nodes, [star.id, 0]]);
    expect(propWiring(show, empty).get(star.id)!.status).toBe("unwired");
    a.ports[0].slots = [blankSlot(star.id)];
    expect(propWiring(show, empty).get(star.id)!.status).toBe("wired");
  });

  it("segments that meet but don't overlap are fine", () => {
    const { show, arch, a, nodes } = fixture();
    a.ports[0].slots = [{ ...blankSlot(arch.id), segment: { start: 0, end: 25 } }];
    a.ports[1].slots = [{ ...blankSlot(arch.id), segment: { start: 25, end: 50 } }];
    expect(propWiring(show, nodes).get(arch.id)!.status).toBe("wired");
  });

  it("finds the first pixels nothing carries", () => {
    expect(firstGap([], 50)).toEqual({ start: 0, end: 50 });
    expect(firstGap([{ start: 0, end: 20 }], 50)).toEqual({ start: 20, end: 50 });
    expect(firstGap([{ start: 10, end: 50 }], 50)).toEqual({ start: 0, end: 10 });
    expect(firstGap([{ start: 20, end: 50 }, { start: 0, end: 10 }], 50)).toEqual({ start: 10, end: 20 });
    expect(firstGap([{ start: 0, end: 50 }], 50)).toBeNull();
  });

  it("labels a chip with its pixel range when it carries part of a prop", () => {
    const { arch } = fixture();
    expect(slotLabel("Arch", blankSlot(arch.id))).toBe("Arch");
    expect(slotLabel("Arch", { ...blankSlot(arch.id), segment: { start: 0, end: 25 } })).toBe("Arch · 1–25");
  });

  it("lists the unwired props left to right", () => {
    const { show, arch, line, star, a, nodes } = fixture();
    const preview: PreviewProp[] = [
      { prop: arch.id, frameOffset: 0, channelsPerPixel: 3, points: [5, 0, 6, 0] },
      { prop: line.id, frameOffset: 0, channelsPerPixel: 3, points: [-5, 0, -4, 0] },
    ];
    // The star has no preview yet: its position decides.
    expect(unwiredInLayoutOrder(show, preview, propWiring(show, nodes))).toEqual([line.id, star.id, arch.id]);
    a.ports[0].slots = [blankSlot(line.id)];
    expect(unwiredInLayoutOrder(show, preview, propWiring(show, nodes))).toEqual([star.id, arch.id]);
  });
});

describe("problems", () => {
  it("names each problem and how to fix it", () => {
    const { show, arch, line, a, b, nodes } = fixture();
    a.ports[0].maxPixels = 60;
    a.ports[0].slots = [blankSlot(arch.id), blankSlot(line.id), { ...blankSlot(line.id), segment: { start: 40, end: 60 } }];
    b.ports[0].slots = [blankSlot("gone"), blankSlot(arch.id)];
    a.ports[1].number = 1;
    const problems = wiringProblems(show, nodes);
    // Like the engine, a slot whose pixels don't fit (or whose prop is gone) carries nothing.
    expect(problems.map((p) => p.message)).toEqual([
      "Port 1 on A has 40 pixels more than it can drive (100 of 60).",
      "Line on port 1 of A uses pixels 41–60, but Line only has 50.",
      "A has two ports numbered 1.",
      "Port 1 on B has a prop that no longer exists.",
      "Arch is wired more than once (A port 1, B port 1).",
    ]);
    expect(problems.every((p) => p.fix.length > 0)).toBe(true);
  });

  it("finds nothing wrong with tidy wiring", () => {
    const { show, arch, a, nodes } = fixture();
    a.ports[0].slots = [blankSlot(arch.id)];
    expect(wiringProblems(show, nodes)).toEqual([]);
  });
});

describe("channels", () => {
  it("gives each port's channel range and universes from the channel map", () => {
    const { show, arch, line, star, a } = fixture();
    a.protocol = { type: "sacn", startUniverse: null, universeSize: 510, allowPixelStraddle: false, multicast: false };
    a.ports[0].slots = [blankSlot(arch.id), blankSlot(line.id)];
    a.ports[1].slots = [{ ...blankSlot(star.id), nullPixels: 1 }];
    const map: ChannelMap = { frameLen: 0, props: [], controllers: mapControllers(show) };
    expect(portChannels(map, a.id, 1)).toEqual({ first: 1, last: 300, universes: [1, 1] });
    // Null pixels stay dark: the range starts at the first lit pixel.
    expect(portChannels(map, a.id, 2)).toEqual({ first: 304, last: 603, universes: [1, 2] });
    expect(portChannels(map, a.id, 3)).toBeNull();
    expect(portChannels(map, "nope", 1)).toBeNull();
  });
});

describe("wiring path", () => {
  const preview: PreviewProp[] = [
    // A, left to right at y = 0; B, left to right at y = 4.
    { prop: "A", frameOffset: 0, channelsPerPixel: 3, points: [0, 0, 1, 0, 2, 0, 3, 0] },
    { prop: "B", frameOffset: 0, channelsPerPixel: 3, points: [0, 4, 1, 4, 2, 4, 3, 4] },
  ];
  const port = (slots: PortSlot[]) => ({ number: 1, maxPixels: null, brightness: 100, gamma: 1, slots });

  it("runs from the controller through each prop's first to last pixel", () => {
    const path = wiringPath(port([blankSlot("A"), blankSlot("B")]), preview);
    expect(path.runs.map((r) => [r.prop, r.points[0], r.points.at(-1)])).toEqual([
      ["A", { x: 0, y: 0 }, { x: 3, y: 0 }],
      ["B", { x: 0, y: 4 }, { x: 3, y: 4 }],
    ]);
    expect(path.firstPixel).toEqual({ x: 0, y: 0 });
    // The controller sits below the first pixel.
    expect(path.start!.x).toBe(0);
    expect(path.start!.y).toBeLessThan(0);
    expect(path.jumps).toEqual([
      { from: path.start, to: { x: 0, y: 0 } },
      { from: { x: 3, y: 0 }, to: { x: 0, y: 4 } },
    ]);
  });

  it("starts at the other end when reversed, and follows a segment", () => {
    const path = wiringPath(port([{ ...blankSlot("A"), reverse: true, segment: { start: 1, end: 3 } }]), preview);
    expect(path.runs[0].points).toEqual([
      { x: 2, y: 0 },
      { x: 1, y: 0 },
    ]);
    expect(path.firstPixel).toEqual({ x: 2, y: 0 });
  });

  it("skips props it has no pixels for, and is empty for an empty port", () => {
    expect(wiringPath(port([blankSlot("missing"), blankSlot("B")]), preview).runs.map((r) => r.prop)).toEqual(["B"]);
    expect(wiringPath(port([]), preview)).toEqual({ start: null, firstPixel: null, runs: [], jumps: [] });
  });

  it("keeps long props to a few hundred points", () => {
    const long = Array.from({ length: 20_000 }, (_, i) => (i % 2 === 0 ? i / 2 : 0));
    const path = wiringPath(port([blankSlot("L")]), [{ prop: "L", frameOffset: 0, channelsPerPixel: 3, points: long }]);
    expect(path.runs[0].points.length).toBeLessThanOrEqual(301);
    expect(path.runs[0].points.at(-1)).toEqual({ x: 9999, y: 0 });
  });
});

describe("drop position", () => {
  const rect = (left: number, top: number) => ({ left, top, right: left + 50, bottom: top + 20 });
  // Two lines of chips: three on the first, one on the second.
  const chips = [rect(0, 0), rect(60, 0), rect(120, 0), rect(0, 30)];

  it("goes before the first chip whose middle is past the pointer", () => {
    expect(dropIndex(chips, { x: 10, y: 10 })).toBe(0);
    expect(dropIndex(chips, { x: 40, y: 10 })).toBe(1);
    expect(dropIndex(chips, { x: 200, y: 10 })).toBe(3);
    expect(dropIndex(chips, { x: 10, y: 40 })).toBe(3);
    expect(dropIndex(chips, { x: 200, y: 40 })).toBe(4);
    expect(dropIndex([], { x: 0, y: 0 })).toBe(0);
  });

  it("past the last line means the end", () => {
    expect(dropIndex(chips, { x: 0, y: 200 })).toBe(4);
  });
});
