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
  moveSlotEdits,
  portCapacity,
  portChannels,
  portPixels,
  propWiring,
  removePortEdits,
  renumberPortEdits,
  slotLabel,
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
    const after = (from: number, to: number) => props(applied(show, moveSlotEdits(show, { controller: a.id, port: 1, index: from }, { controller: a.id, port: 1, index: to })).controllers[0], 1);
    expect(after(0, 3)).toEqual([line.id, star.id, arch.id]);
    expect(after(0, 2)).toEqual([line.id, arch.id, star.id]);
    expect(after(2, 0)).toEqual([star.id, arch.id, line.id]);
    // Dropping right where it is changes nothing.
    expect(moveSlotEdits(show, { controller: a.id, port: 1, index: 1 }, { controller: a.id, port: 1, index: 1 })).toEqual([]);
    expect(moveSlotEdits(show, { controller: a.id, port: 1, index: 1 }, { controller: a.id, port: 1, index: 2 })).toEqual([]);
  });

  it("moves to another port, or another controller, keeping the slot's settings", () => {
    const { show, arch, line, a, b } = fixture();
    a.ports[0].slots = [{ ...blankSlot(arch.id), reverse: true, nullPixels: 3 }, blankSlot(line.id)];
    const sameController = moveSlotEdits(show, { controller: a.id, port: 1, index: 0 }, { controller: a.id, port: 2, index: 0 });
    expect(sameController).toHaveLength(1);
    const one = applied(show, sameController).controllers[0];
    expect(props(one, 1)).toEqual([line.id]);
    expect(one.ports[1].slots[0]).toEqual({ ...blankSlot(arch.id), reverse: true, nullPixels: 3 });

    const across = moveSlotEdits(show, { controller: a.id, port: 1, index: 1 }, { controller: b.id, port: 1, index: 0 });
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
    expect(props(applied(show, unwireEdits(show, { controller: a.id, port: 1, index: 0 })).controllers[0], 1)).toEqual([line.id]);
    const reversed = applied(show, updateSlotEdits(show, { controller: a.id, port: 1, index: 1 }, (s) => ({ ...s, reverse: true })));
    expect(reversed.controllers[0].ports[0].slots[1].reverse).toBe(true);
    const limited = applied(show, updatePortEdits(show, { controller: a.id, port: 2 }, (p) => ({ ...p, maxPixels: 100 })));
    expect(limited.controllers[0].ports[1].maxPixels).toBe(100);
    // Gone since: nothing to send.
    expect(unwireEdits(show, { controller: a.id, port: 1, index: 5 })).toEqual([]);
    expect(updateSlotEdits(show, { controller: "nope", port: 1, index: 0 }, (s) => s)).toEqual([]);
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

  it("wires the remaining props onto the end of a port", () => {
    const { show, arch, line, star, a } = fixture();
    a.ports[0].slots = [blankSlot(arch.id)];
    expect(props(applied(show, wireRemainingEdits(show, { controller: a.id, port: 1 }, [line.id, star.id])).controllers[0], 1)).toEqual([arch.id, line.id, star.id]);
    expect(wireRemainingEdits(show, { controller: a.id, port: 1 }, [])).toEqual([]);
  });
});

describe("capacity", () => {
  it("counts null pixels and segments", () => {
    const { arch, line, nodes } = fixture();
    const port = { number: 1, maxPixels: null, brightness: 100, gamma: 1, slots: [{ ...blankSlot(arch.id), nullPixels: 2 }, { ...blankSlot(line.id), segment: { start: 0, end: 10 } }] };
    expect(portPixels(port, nodes)).toBe(62);
    expect(portCapacity(port, nodes)).toEqual({ used: 62, limit: null, level: "none", message: null });
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
