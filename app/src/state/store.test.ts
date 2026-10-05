import { describe, expect, it } from "vitest";
import { MemoryBackend } from "../api/memory";
import { useApp } from "./store";

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

  it("save asks for a path the first time and does nothing if cancelled", async () => {
    const backend = await connected();
    backend.nextSavePath = null;
    expect(await useApp.getState().save()).toBe(false);
    expect(backend.calls.some((c) => c.startsWith("saveShowAs"))).toBe(false);

    backend.nextSavePath = "/shows/a.json";
    expect(await useApp.getState().save()).toBe(true);
    expect(useApp.getState().snapshot?.path).toBe("/shows/a.json");
  });

  it("keeps the newer snapshot when an older call resolves last", async () => {
    const backend = await connected();
    const base = await backend.getSnapshot();
    const older = { ...base, revision: base.revision + 1 };
    const newer = { ...base, revision: base.revision + 2 };
    let finishFirst!: (s: typeof older) => void;
    const first = useApp.getState().run(() => new Promise((resolve) => (finishFirst = resolve)));
    expect(await useApp.getState().run(async () => newer)).toBe(true);
    finishFirst(older);
    expect(await first).toBe(true);
    expect(useApp.getState().snapshot?.revision).toBe(newer.revision);
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

  it("remembers the theme", async () => {
    useApp.getState().setTheme("light");
    expect(useApp.getState().theme).toBe("light");
    expect(localStorage.getItem("pixelflow.theme")).toBe("light");
  });
});
