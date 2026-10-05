import { describe, expect, it } from "vitest";
import { MemoryBackend } from "../api/memory";
import type { Device, Discovery } from "../api/types";
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
      },
    });
    let answer!: (d: Discovery) => void;
    backend.discoverDevices = () => new Promise((resolve) => (answer = resolve));
    const scanning = useApp.getState().scan();
    useApp.getState().forgetDevice("10.0.0.20");
    answer({ devices: [device("10.0.0.20"), device("10.0.0.21"), device("10.0.0.22")], silent: [] });
    expect(await scanning).toBe(true);
    expect(useApp.getState().discovery?.devices.map((d) => d.address)).toEqual(["10.0.0.21", "10.0.0.22"]);
    localStorage.removeItem("pixelflow.devices");
  });
});
