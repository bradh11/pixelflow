import { describe, expect, it } from "vitest";
import { demoShow } from "../api/demo";
import { demoSequence } from "../api/demoSequence";
import { newEffect } from "../api/sequence";
import type { Show } from "../api/types";
import { describeSequenceEdits, describeShowEdits } from "./describeChange";
import { newProp } from "./shows";

const show = (): Show => demoShow();
const prop = (s: Show, name: string) => s.props.find((p) => p.name === name)!;

describe("what a show change is called (for Undo and Redo)", () => {
  it("names a moved, turned, resized, renamed, or changed prop", () => {
    const s = show();
    const tree = prop(s, "Mega Tree");
    const moved = { ...tree, transform: { ...tree.transform, position: { ...tree.transform.position, x: 9 } } };
    expect(describeShowEdits([{ type: "updateProp", prop: moved }], s)).toBe("Move Mega Tree");
    const turned = { ...tree, transform: { ...tree.transform, rotationDeg: { ...tree.transform.rotationDeg, z: 30 } } };
    expect(describeShowEdits([{ type: "updateProp", prop: turned }], s)).toBe("Turn Mega Tree");
    const resized = { ...tree, transform: { ...tree.transform, scale: { ...tree.transform.scale, x: 2 } } };
    expect(describeShowEdits([{ type: "updateProp", prop: resized }], s)).toBe("Resize Mega Tree");
    expect(describeShowEdits([{ type: "updateProp", prop: { ...tree, name: "Big Tree" } }], s)).toBe("Rename Mega Tree to Big Tree");
    expect(describeShowEdits([{ type: "updateProp", prop: { ...tree, colorOrder: tree.colorOrder === "GRB" ? "RGB" : "GRB" } }], s)).toBe("Change Mega Tree");
  });

  it("counts several props", () => {
    const s = show();
    const moved = s.props.map((p) => ({ ...p, transform: { ...p.transform, position: { ...p.transform.position, x: p.transform.position.x + 1 } } }));
    expect(describeShowEdits(moved.map((p) => ({ type: "updateProp" as const, prop: p })), s)).toBe("Move 4 props");
    expect(describeShowEdits([{ type: "removeProp", id: prop(s, "Porch Star").id }], s)).toBe("Delete Porch Star");
    expect(describeShowEdits(s.props.slice(0, 2).map((p) => ({ type: "removeProp" as const, id: p.id })), s)).toBe("Delete 2 props");
    const arch = { ...newProp("arch", s), name: "Arch 1" };
    expect(describeShowEdits([{ type: "addProp", prop: arch }], s)).toBe("Add Arch 1");
  });

  it("names the props first when a change also touches wiring or groups", () => {
    const s = show();
    const star = prop(s, "Porch Star");
    const fpp = s.controllers[0];
    expect(describeShowEdits([{ type: "removeProp", id: star.id }, { type: "updateController", controller: fpp }], s)).toBe("Delete Porch Star");
  });

  it("names controllers, wiring, groups, the photo, the playlist, and the show", () => {
    const s = show();
    const fpp = s.controllers[0];
    const unwired = { ...fpp, ports: fpp.ports.map((p) => ({ ...p, slots: [] })) };
    expect(describeShowEdits([{ type: "updateController", controller: unwired }], s)).toBe(`Change wiring on ${fpp.name}`);
    expect(describeShowEdits([{ type: "updateController", controller: { ...fpp, address: "10.0.0.9" } }], s)).toBe(`Edit ${fpp.name}`);
    expect(describeShowEdits([{ type: "removeController", id: fpp.id }], s)).toBe(`Delete ${fpp.name}`);
    expect(describeShowEdits([{ type: "addGroup", group: { id: "g", name: "Arches", members: [] } }], s)).toBe("Make group Arches");
    expect(describeShowEdits([{ type: "setBackground", background: null }], s)).toBe("Remove the photo");
    expect(describeShowEdits([{ type: "renameShow", name: "Home" }], s)).toBe("Rename the show to Home");
    expect(describeShowEdits([{ type: "removeSequence", id: "x" }], { ...s, sequences: [{ id: "x", name: "Medley", path: "/m.fseq", audio: null, offsetMs: 0 }] })).toBe(
      "Take Medley off the playlist",
    );
  });
});

describe("what a sequence change is called", () => {
  const label = (kind: string) => kind[0].toUpperCase() + kind.slice(1);

  it("names effects added, deleted, moved, resized, and changed", () => {
    const s = show();
    const doc = demoSequence(s, 60_000);
    const row = doc.rows[0];
    const effect = row.layers[0].effects[0];
    const name = label(effect.params.kind);
    expect(describeSequenceEdits([{ type: "addEffect", row: row.id, layer: 0, effect: newEffect("fade", 0, 500) }], doc, label)).toBe("Add Fade");
    expect(describeSequenceEdits([{ type: "removeEffect", id: effect.id }], doc, label)).toBe(`Delete ${name}`);
    const length = effect.endMs - effect.startMs;
    expect(
      describeSequenceEdits([{ type: "moveEffect", id: effect.id, row: row.id, layer: 0, startMs: effect.startMs + 100, endMs: effect.startMs + 100 + length }], doc, label),
    ).toBe(`Move ${name}`);
    expect(describeSequenceEdits([{ type: "setEffectTiming", id: effect.id, startMs: effect.startMs, endMs: effect.endMs + 100 }], doc, label)).toBe(`Resize ${name}`);
    expect(describeSequenceEdits([{ type: "updateEffect", effect: { ...effect, fadeInMs: 100 } }], doc, label)).toBe(`Change ${name}`);
    const ids = row.layers[0].effects.slice(0, 3).map((e) => e.id);
    expect(describeSequenceEdits(ids.map((id) => ({ type: "removeEffect" as const, id })), doc, label)).toBe("Delete 3 effects");
  });

  it("names rows and timing tracks", () => {
    const doc = demoSequence(show(), 60_000);
    expect(describeSequenceEdits([{ type: "removeRow", id: doc.rows[0].id }], doc, label)).toBe("Delete a row");
    const track = doc.timingTracks[0];
    expect(describeSequenceEdits([{ type: "removeTimingTrack", id: track.id }], doc, label)).toBe(`Delete ${track.name}`);
    expect(describeSequenceEdits([{ type: "removeMarks", track: track.id, indices: [0] }], doc, label)).toBe(`Change marks on ${track.name}`);
  });
});
