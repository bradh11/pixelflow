import { describe, expect, it } from "vitest";
import { demoDevices } from "./demo";
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

  it("a test pattern stops playback, like the engine", async () => {
    const backend = new MemoryBackend();
    const controller = { ...newController("C", "10.0.0.1", "ddp", 0), sequenceChannels: { start: 1, count: 30 } };
    await backend.applyEdits([{ type: "addController", controller }]);
    await backend.startPlayback("/Shows/a.fseq", 0);
    expect(await backend.playbackStatus()).not.toBeNull();
    await backend.startOutput({ kind: "chase", color: "ffffff" }, { type: "show" });
    expect(await backend.playbackStatus()).toBeNull();
  });

  it("adding an FPP destination picks it by protocol and never doubles an address", async () => {
    const backend = new MemoryBackend();
    backend.deviceNetwork = demoDevices();
    await expect(backend.importFppDestination("192.0.2.10", "192.0.2.20", "sACN unicast")).rejects.toThrow("doesn't send to");
    const snap = await backend.importFppDestination("192.0.2.10", "192.0.2.20", "DDP");
    expect(snap.show.controllers).toHaveLength(1);
    await expect(backend.importFppDestination("192.0.2.10", "192.0.2.20", "DDP")).rejects.toThrow("already in your show");
    // Importing the controller itself fills in the placeholder and says so first.
    const details = await backend.inspectDevice("192.0.2.20");
    expect(details.plan.alreadyInShow).toBe(false);
    expect(details.plan.notes).toContain("Fills in Falcon_F16V5_B9F5, added from your FPP's output list.");
  });

  it("previews each prop's real shape where its transform puts it", async () => {
    const backend = new MemoryBackend();
    const prop = newProp("line", backend.show);
    prop.shape = { source: "generator", type: "line", nodes: 3, length: 2 };
    prop.transform = { position: { x: 10, y: 2, z: 0 }, rotationDeg: { x: 0, y: 0, z: 90 }, scale: { x: 2, y: 2, z: 1 } };
    const arch = newProp("arch", backend.show);
    await backend.applyEdits([{ type: "addProp", prop }, { type: "addProp", prop: arch }]);
    const [line, second] = await backend.previewProps();
    const expected = [10, 0, 10, 2, 10, 4];
    line.points.forEach((v, i) => expect(v).toBeCloseTo(expected[i]));
    expect(second.frameOffset).toBe(9);
    expect(second.points).toHaveLength(100);
  });

  it("sets and removes the background photo like the engine, refusing bad values", async () => {
    const backend = new MemoryBackend();
    expect(backend.show.background).toBeNull();
    const photo = { path: "/house.jpg", x: -10, y: 8, width: 20, opacity: 0.7 };
    let snap = await backend.applyEdits([{ type: "setBackground", background: photo }]);
    expect(snap.show.background).toEqual(photo);
    await expect(backend.applyEdits([{ type: "setBackground", background: { ...photo, width: 0 } }])).rejects.toThrow(
      "The background photo must be wider than zero.",
    );
    snap = await backend.undo();
    expect(snap.show.background).toBeNull();
    backend.images.set("/house.jpg", new Uint8Array([7]));
    expect(await backend.readImage("/house.jpg")).toEqual(new Uint8Array([7]));
    await expect(backend.readImage("/missing.png")).rejects.toThrow("Could not read /missing.png");
  });
});
