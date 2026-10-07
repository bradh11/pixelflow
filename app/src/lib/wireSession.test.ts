import { describe, expect, it } from "vitest";
import { emptyShow } from "../api/memory";
import type { Edit, Prop, Show } from "../api/types";
import { newController, newProp } from "./shows";
import { answer, clickProp, doOp, draftPort, insertIndex, sessionEdits, setReceiver, startSession } from "./wireSession";
import { blankSlot } from "./wiringMath";

function prop(kind: "arch" | "line" | "star", name: string): Prop {
  return { ...newProp(kind, emptyShow("x")), name };
}

/** Arch (50 px), Line (50 px), Star (100 px); controller A has 2 ports, B has 1. */
function fixture() {
  const show = emptyShow("t");
  const arch = prop("arch", "Arch");
  const line = prop("line", "Line");
  const star = prop("star", "Star");
  show.props = [arch, line, star];
  const a = newController("A", "10.0.0.1", "ddp", 2);
  const b = newController("B", "10.0.0.2", "ddp", 1);
  show.controllers = [a, b];
  const nodes = new Map(show.props.map((p) => [p.id, p.id === star.id ? 100 : 50]));
  const port1 = { controller: a.id, port: 1, at: 0 };
  return { show, arch, line, star, a, b, nodes, port1 };
}

function applied(show: Show, edits: Edit[]): Show {
  const next = structuredClone(show);
  for (const e of edits) if (e.type === "updateController") next.controllers = next.controllers.map((c) => (c.id === e.controller.id ? e.controller : c));
  return next;
}

describe("click-to-wire session", () => {
  it("adds props in the order they are clicked, as one batch against the show", () => {
    const { show, arch, line, star, nodes, port1 } = fixture();
    let s = startSession(show, port1);
    for (const p of [star, arch, line]) s = clickProp(show, s, nodes, p.id);
    expect(draftPort(show, s, nodes)?.slots.map((x) => x.prop)).toEqual([star.id, arch.id, line.id]);
    // The show itself is untouched until the edits are sent.
    expect(show.controllers[0].ports[0].slots).toEqual([]);
    const edits = sessionEdits(show, s, nodes);
    expect(edits).toHaveLength(1);
    expect(applied(show, edits).controllers[0].ports[0].slots.map((x) => x.prop)).toEqual([star.id, arch.id, line.id]);
  });

  it("asks before taking a prop off the chain or moving it to the end, keeping its settings", () => {
    const { show, arch, line, star, a, nodes, port1 } = fixture();
    a.ports[0].slots = [{ ...blankSlot(arch.id), reverse: true }, blankSlot(line.id)];
    let s = startSession(show, port1);
    s = clickProp(show, s, nodes, arch.id);
    expect(s.prompt).toEqual({ kind: "inChain", prop: arch.id, last: false });
    expect(s.ops).toEqual([]);
    expect(answer(s, "cancel").prompt).toBeNull();
    s = answer(s, "toEnd");
    expect(draftPort(show, s, nodes)?.slots.map((x) => x.prop)).toEqual([line.id, arch.id]);
    expect(clickProp(show, s, nodes, arch.id).prompt).toMatchObject({ last: true });

    s = answer(clickProp(show, s, nodes, arch.id), "remove");
    expect(draftPort(show, s, nodes)?.slots.map((x) => x.prop)).toEqual([line.id]);
    // Added back in the same session, it keeps the settings it had.
    s = clickProp(show, s, nodes, arch.id);
    s = clickProp(show, s, nodes, star.id);
    expect(draftPort(show, s, nodes)?.slots).toEqual([blankSlot(line.id), { ...blankSlot(arch.id), reverse: true }, blankSlot(star.id)]);
  });

  it("asks before moving a prop here from another port, then moves it in the same batch", () => {
    const { show, arch, star, a, b, nodes, port1 } = fixture();
    b.ports[0].slots = [{ ...blankSlot(star.id), nullPixels: 2 }];
    let s = clickProp(show, startSession(show, port1), nodes, star.id);
    expect(s.prompt).toEqual({ kind: "elsewhere", prop: star.id, controller: b.id, port: 1 });
    s = answer(s, "move");
    s = clickProp(show, s, nodes, arch.id);
    const edits = sessionEdits(show, s, nodes);
    // Two controllers changed, one batch.
    expect(edits).toHaveLength(2);
    const after = applied(show, edits);
    expect(after.controllers[0].ports[0].slots).toEqual([{ ...blankSlot(star.id), nullPixels: 2 }, blankSlot(arch.id)]);
    expect(after.controllers[1].ports[0].slots).toEqual([]);
    expect(a.ports[0].slots).toEqual([]);
  });

  it("adds the rest of a prop wired in part, and refuses one wired in several pieces", () => {
    const { show, arch, star, b, nodes, port1 } = fixture();
    b.ports[0].slots = [{ ...blankSlot(arch.id), segment: { start: 0, end: 20 } }, { ...blankSlot(star.id), segment: { start: 0, end: 50 } }];
    show.controllers[0].ports[1].slots = [{ ...blankSlot(star.id), segment: { start: 50, end: 100 } }];
    let s = clickProp(show, startSession(show, port1), nodes, arch.id);
    expect(draftPort(show, s, nodes)?.slots).toEqual([{ ...blankSlot(arch.id), segment: { start: 20, end: 50 } }]);
    s = clickProp(show, s, nodes, star.id);
    expect(s.refused).toBe(star.id);
    expect(s.ops).toHaveLength(1);
  });

  it("adds to the chosen smart receiver, keeping each receiver's props together", () => {
    const { show, arch, line, star, a, nodes, port1 } = fixture();
    a.ports[0].slots = [{ ...blankSlot(arch.id), smartReceiver: 1 }, { ...blankSlot(line.id), smartReceiver: 2 }];
    let s = startSession(show, port1);
    // It starts on the receiver the port's last prop is on.
    expect(s.receiver).toBe(2);
    s = setReceiver(s, 1);
    s = clickProp(show, s, nodes, star.id);
    expect(draftPort(show, s, nodes)?.slots.map((x) => [x.prop, x.smartReceiver])).toEqual([
      [arch.id, 1],
      [star.id, 1],
      [line.id, 2],
    ]);
    expect(insertIndex(a.ports[0], 3)).toBe(2);
    expect(insertIndex({ ...a.ports[0], slots: [{ ...blankSlot(arch.id), smartReceiver: 3 }] }, 2)).toBe(0);
    expect(insertIndex(a.ports[0], null)).toBe(2);
  });

  it("sends nothing when the gestures add up to no change, and skips gestures that no longer apply", () => {
    const { show, arch, nodes, port1 } = fixture();
    let s = doOp(startSession(show, port1), { kind: "add", prop: arch.id, receiver: null });
    s = doOp(s, { kind: "remove", prop: arch.id });
    expect(sessionEdits(show, s, nodes)).toEqual([]);
    // Gone from the show by the time Done is pressed: nothing to do.
    const gone = doOp(startSession(show, port1), { kind: "add", prop: "deleted", receiver: null });
    expect(sessionEdits(show, gone, nodes)).toEqual([]);
    expect(clickProp(show, startSession(show, port1), nodes, "deleted").ops).toEqual([]);
  });
});
