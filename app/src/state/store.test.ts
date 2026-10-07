import { waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MemoryBackend, emptyShow } from "../api/memory";
import type { Device, Discovery, Show, ShowSnapshot } from "../api/types";
import { gestureEdits } from "../lib/layoutEdits";
import { newProp } from "../lib/shows";
import { useLayoutEditor } from "./layoutEditor";
import { initialThemeChoice, useApp } from "./store";
import { nextLabels, useUndoLabels } from "./undoLabels";

async function connected() {
  const backend = new MemoryBackend();
  await useApp.getState().connect(backend);
  return backend;
}

describe("app store", () => {
  it("loads the snapshot on connect", async () => {
    await connected();
    expect(useApp.getState().snapshot?.show.name).toBe("Untitled Show");
  });

  it("new show leaves the welcome screen", async () => {
    await connected();
    expect(await useApp.getState().newShow()).toBe(true);
    expect(useApp.getState().started).toBe(true);
  });

  it("save asks for a name and a path the first time and does nothing if cancelled", async () => {
    const backend = await connected();
    // Cancelling the name.
    let saving = useApp.getState().save();
    await waitFor(() => expect(useApp.getState().naming).toBe("Untitled Show"));
    useApp.getState().resolveNaming(null);
    expect(await saving).toBe(false);
    // Cancelling the file dialog.
    backend.nextSavePath = null;
    saving = useApp.getState().save();
    await waitFor(() => expect(useApp.getState().naming).not.toBeNull());
    useApp.getState().resolveNaming("Backyard");
    expect(await saving).toBe(false);
    expect(backend.calls.some((c) => c.startsWith("saveShowAs"))).toBe(false);

    backend.nextSavePath = "/shows/a.json";
    expect(await useApp.getState().save()).toBe(true);
    expect(useApp.getState().snapshot?.path).toBe("/shows/a.json");
    expect(useApp.getState().snapshot?.show.name).toBe("Backyard");
  });

  it("a named show's first save suggests its name as the file name, without asking", async () => {
    const backend = new MemoryBackend(emptyShow("Front Yard"));
    await useApp.getState().connect(backend);
    let suggested = "";
    backend.pickSavePath = async (name?: string) => {
      suggested = name ?? "";
      return "/shows/front.json";
    };
    expect(await useApp.getState().save()).toBe(true);
    expect(useApp.getState().naming).toBeNull();
    expect(suggested).toBe("Front Yard.pixelflow.json");
  });

  it("runs backend calls one at a time, in the order they were made", async () => {
    const backend = await connected();
    const base = await backend.getSnapshot();
    const started: string[] = [];
    let finishFirst!: (s: ShowSnapshot) => void;
    const first = useApp.getState().run(() => {
      started.push("first");
      return new Promise((resolve) => (finishFirst = resolve));
    });
    const second = useApp.getState().run(async () => {
      started.push("second");
      return { ...base, revision: base.revision + 2 };
    });
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(started).toEqual(["first"]);
    finishFirst({ ...base, revision: base.revision + 1 });
    expect(await first).toBe(true);
    expect(await second).toBe(true);
    expect(started).toEqual(["first", "second"]);
    expect(useApp.getState().snapshot?.revision).toBe(base.revision + 2);
  });

  it("builds edits from the show as it is when their turn comes, so none is lost", async () => {
    const prop = newProp("line", emptyShow("x"));
    const backend = new MemoryBackend({ ...emptyShow("Test"), props: [prop] });
    const applyEdits = backend.applyEdits.bind(backend);
    // A slow engine: the second edit is asked for while the first is still on its way.
    backend.applyEdits = async (edits) => {
      await new Promise((resolve) => setTimeout(resolve, 20));
      return applyEdits(edits);
    };
    await useApp.getState().connect(backend);
    const nudge = (show: Show) => gestureEdits(show, [prop.id], { kind: "move", dx: 0.1, dy: 0 });
    const results = await Promise.all([useApp.getState().apply(nudge), useApp.getState().apply(nudge)]);
    expect(results).toEqual([true, true]);
    expect(backend.show.props[0].transform.position.x).toBeCloseTo(0.2);
    expect(backend.undoStack).toHaveLength(2);
  });

  it("says which revision holds an edit, and skips the engine when there's nothing to change", async () => {
    const backend = await connected();
    const prop = newProp("line", backend.show);
    const revision = await useApp.getState().edit([{ type: "addProp", prop }]);
    expect(revision).toBe(backend.revision);
    const calls = backend.calls.length;
    expect(await useApp.getState().edit(() => [])).toBe(revision);
    expect(backend.calls).toHaveLength(calls);
    backend.applyEdits = async () => {
      throw new Error("Refused.");
    };
    expect(await useApp.getState().edit([{ type: "removeProp", id: prop.id }])).toBeNull();
  });

  it("leaves photo editing when the photo goes away", async () => {
    const backend = await connected();
    backend.images.set("/house.jpg", new Uint8Array([1]));
    const background = { path: "/house.jpg", x: 0, y: 5, width: 10, opacity: 0.7 };
    await useApp.getState().apply([{ type: "setBackground", background }]);
    useLayoutEditor.getState().setEditPhoto(true);
    await useApp.getState().undo();
    expect(useLayoutEditor.getState().editPhoto).toBe(false);
    await useApp.getState().redo();
    useLayoutEditor.getState().setEditPhoto(true);
    await useApp.getState().apply([{ type: "setBackground", background: null }]);
    expect(useLayoutEditor.getState().editPhoto).toBe(false);
  });

  it("turns backend failures into a dismissable message", async () => {
    const backend = await connected();
    backend.undo = async () => {
      throw new Error("Boom happened.");
    };
    expect(await useApp.getState().undo()).toBe(false);
    expect(useApp.getState().error).toBe("Boom happened.");
    useApp.getState().dismissError();
    expect(useApp.getState().error).toBeNull();
  });

  it("keeps an upgraded install dark, and lets a new one follow the computer", () => {
    expect(initialThemeChoice()).toBe("system");
    // Saved, so the next start (with other settings saved by then) still follows the computer.
    expect(localStorage.getItem("pixelflow.theme")).toBe("system");
    localStorage.setItem("pixelflow.devices", "[]");
    expect(initialThemeChoice()).toBe("system");
    // Someone who has used PixelFlow before and never chose: the dark they've always had, kept.
    localStorage.removeItem("pixelflow.theme");
    expect(initialThemeChoice()).toBe("dark");
    expect(localStorage.getItem("pixelflow.theme")).toBe("dark");
    localStorage.setItem("pixelflow.theme", "light");
    expect(initialThemeChoice()).toBe("light");
  });

  it("follows the computer's theme until one is chosen, and again when asked", () => {
    const light = { matches: true, addEventListener() {}, removeEventListener() {} };
    window.matchMedia = (() => light) as unknown as typeof window.matchMedia;
    try {
      useApp.getState().setTheme("system");
      expect(useApp.getState()).toMatchObject({ theme: "light", themeChoice: "system" });
      expect(localStorage.getItem("pixelflow.theme")).toBe("system");
      useApp.getState().setTheme("dark");
      expect(useApp.getState()).toMatchObject({ theme: "dark", themeChoice: "dark" });
      expect(localStorage.getItem("pixelflow.theme")).toBe("dark");
    } finally {
      delete (window as { matchMedia?: unknown }).matchMedia;
    }
  });

  it("doesn't name Undo after a change made elsewhere landed between an edit's sending and its reply", async () => {
    const backend = new MemoryBackend(emptyShow("House"));
    await useApp.getState().connect(backend);
    const show = backend.show;
    await useApp.getState().apply([{ type: "addProp", prop: { ...newProp("arch", show), name: "Arch 1" } }]);
    const label = () => nextLabels(useUndoLabels.getState().show, useApp.getState().snapshot?.revision).undo;
    expect(label()).toBe("Add Arch 1");
    // The assistant applies a change straight to the engine; the app hasn't heard of it yet.
    await backend.applyEdits([{ type: "renameShow", name: "Home" }]);
    await useApp.getState().apply([{ type: "addProp", prop: { ...newProp("tree", show), name: "Tree 1" } }]);
    expect(label()).toBeNull();
  });

  it("remembers the theme", async () => {
    useApp.getState().setTheme("light");
    expect(useApp.getState().theme).toBe("light");
    expect(localStorage.getItem("pixelflow.theme")).toBe("light");
  });

  it("ignores remembered controllers that aren't stored properly", async () => {
    const good = { address: "10.0.0.5", kind: "fpp", name: "FPP", model: "Pi", firmware: "9", mode: null, foundBy: ["ping"], responding: true, lastSeen: 1 };
    localStorage.setItem(
      "pixelflow.devices",
      JSON.stringify([good, null, 7, { address: "10.0.0.6" }, { ...good, address: "10.0.0.7", foundBy: "ping" }, { ...good, address: 8 }]),
    );
    await connected();
    expect(useApp.getState().discovery?.devices.map((d) => d.address)).toEqual(["10.0.0.5"]);
    localStorage.removeItem("pixelflow.devices");
  });

  it("doesn't bring back a controller forgotten while a scan runs", async () => {
    const backend = await connected();
    const device = (address: string): Device => ({ address, kind: "wled", name: address, model: "", firmware: "", mode: null, foundBy: ["ping"] });
    useApp.setState({
      discovery: {
        devices: [device("10.0.0.20"), device("10.0.0.21")].map((d) => ({ ...d, responding: true, lastSeen: 1 })),
        silent: [],
        locked: [],
      },
    });
    let answer!: (d: Discovery) => void;
    backend.discoverDevices = () => new Promise((resolve) => (answer = resolve));
    const scanning = useApp.getState().scan();
    useApp.getState().forgetDevice("10.0.0.20");
    answer({ devices: [device("10.0.0.20"), device("10.0.0.21"), device("10.0.0.22")], silent: [], locked: [] });
    expect(await scanning).toBe(true);
    expect(useApp.getState().discovery?.devices.map((d) => d.address)).toEqual(["10.0.0.21", "10.0.0.22"]);
    localStorage.removeItem("pixelflow.devices");
  });
});
