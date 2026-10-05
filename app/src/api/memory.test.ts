import { describe, expect, it } from "vitest";
import { MemoryBackend, emptyShow } from "./memory";
import { newController, newProp } from "../lib/shows";

describe("MemoryBackend", () => {
  it("applies edits with undo and redo, tracking dirty state", async () => {
    const backend = new MemoryBackend();
    const prop = newProp("line", backend.show);
    let snap = await backend.applyEdits([{ type: "addProp", prop }]);
    expect(snap.summary).toMatchObject({ props: 1, pixels: 50 });
    expect(snap.dirty && snap.canUndo).toBe(true);
    snap = await backend.undo();
    expect(snap.summary.props).toBe(0);
    snap = await backend.redo();
    expect(snap.show.props[0].id).toBe(prop.id);
  });

  it("removing a prop unwires it and drops it from groups", async () => {
    const backend = new MemoryBackend();
    const prop = newProp("arch", backend.show);
    const controller = newController("C", "10.0.0.1", "ddp", 1);
    controller.ports[0].slots.push({ prop: prop.id, segment: null, nullPixels: 0, reverse: false, brightness: null, gamma: null, smartReceiver: null });
    await backend.applyEdits([
      { type: "addProp", prop },
      { type: "addController", controller },
      { type: "addGroup", group: { id: crypto.randomUUID(), name: "G", members: [prop.id] } },
    ]);
    const snap = await backend.applyEdits([{ type: "removeProp", id: prop.id }]);
    expect(snap.show.controllers[0].ports[0].slots).toEqual([]);
    expect(snap.show.groups[0].members).toEqual([]);
  });

  it("rejects duplicate and unknown ids without changing the show", async () => {
    const backend = new MemoryBackend();
    const prop = newProp("arch", backend.show);
    await backend.applyEdits([{ type: "addProp", prop }]);
    await expect(backend.applyEdits([{ type: "addProp", prop }])).rejects.toThrow("A prop with that id already exists.");
    await expect(backend.applyEdits([{ type: "removeController", id: "nope" }])).rejects.toThrow("There is no controller with that id.");
    expect((await backend.getSnapshot()).summary.props).toBe(1);
  });

  it("saves and opens files, and needs a path before plain save", async () => {
    const backend = new MemoryBackend(emptyShow("House"));
    await expect(backend.saveShow()).rejects.toThrow("not been saved yet");
    let snap = await backend.saveShowAs("/shows/house.json");
    expect(snap.path).toBe("/shows/house.json");
    expect(snap.dirty).toBe(false);
    await backend.newShow("Other");
    snap = await backend.openShow("/shows/house.json");
    expect(snap.show.name).toBe("House");
    await expect(backend.openShow("/missing.json")).rejects.toThrow("Could not read");
  });

  it("starts and stops output with a generation counter", async () => {
    const backend = new MemoryBackend();
    const status = await backend.startOutput({ kind: "chase", color: "ffffff" }, { type: "show" });
    expect(status.running).toBe(true);
    expect(status.generation).toBe(1);
    expect((await backend.stopOutput()).running).toBe(false);
  });
});
