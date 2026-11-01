import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { VIEW_DEFAULTS } from "../lib/viewOptions";
import { loadShowView, loadViewOptions, saveShowView, useView3d } from "./view3d";

const orbit = { target: { x: 1, y: 2, z: 3 }, yaw: 0.5, pitch: 0.2, distance: 12 };

describe("a show's 3D view settings", () => {
  beforeEach(() => {
    localStorage.clear();
    useView3d.setState({ showKey: null, carried: null });
  });
  afterEach(() => vi.restoreAllMocks());

  it("follow the show when it's saved under a new name", () => {
    saveShowView("unsaved:House", { orbit, photoDepth: 2 });
    useView3d.getState().openShow("unsaved:House");
    useView3d.getState().carryShow("unsaved:House", "/shows/house.pixelflow.json");
    expect(loadShowView("/shows/house.pixelflow.json")).toEqual({ orbit, photoDepth: 2 });
    expect(useView3d.getState()).toMatchObject({ showKey: "/shows/house.pixelflow.json", photoDepth: 2 });
    // Changing the depth now remembers it under the new name.
    useView3d.getState().setPhotoDepth(4);
    expect(loadShowView("/shows/house.pixelflow.json").photoDepth).toBe(4);
  });

  it("don't replace settings already remembered under the new name", () => {
    saveShowView("unsaved:House", { orbit, photoDepth: 2 });
    saveShowView("/shows/other.pixelflow.json", { photoDepth: 7 });
    useView3d.getState().carryShow("unsaved:House", "/shows/other.pixelflow.json");
    expect(loadShowView("/shows/other.pixelflow.json")).toEqual({ orbit: null, photoDepth: 7 });
  });

  it("still move along when this computer won't store them", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    useView3d.setState({ showKey: "unsaved:House", photoDepth: 3 });
    expect(() => useView3d.getState().carryShow("unsaved:House", "/shows/house.pixelflow.json")).not.toThrow();
    expect(useView3d.getState()).toMatchObject({ showKey: "/shows/house.pixelflow.json", photoDepth: 3 });
  });
});

describe("the previews' glow", () => {
  const KEY = "pixelflow.view3dOptions";
  beforeEach(() => localStorage.clear());

  it("starts at none, and is remembered on this computer once set", () => {
    expect(loadViewOptions()).toEqual({ glow: 0, ground: true, ...VIEW_DEFAULTS });
    expect(useView3d.getState().glow).toBe(0);
    useView3d.getState().setGlow(0.35);
    expect(useView3d.getState().glow).toBe(0.35);
    expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual({ glow: 0.35, ground: true, ...VIEW_DEFAULTS });
    expect(loadViewOptions().glow).toBe(0.35);
    // Hiding the ground keeps it.
    useView3d.getState().setGround(false);
    expect(loadViewOptions()).toMatchObject({ glow: 0.35, ground: false });
    useView3d.getState().setGround(true);
  });

  it("stays between none and full", () => {
    const set = (level: number) => {
      useView3d.getState().setGlow(level);
      return useView3d.getState().glow;
    };
    expect([set(7), set(-1), set(NaN), set(1), set(0)]).toEqual([1, 0, 0, 1, 0]);
    localStorage.setItem(KEY, JSON.stringify({ glow: 5 }));
    expect(loadViewOptions().glow).toBe(1);
  });

  it("reads settings saved as the 3D view's Glow button: on as half way, off or never chosen as none", () => {
    const saved = (options: unknown) => {
      localStorage.setItem(KEY, JSON.stringify(options));
      return loadViewOptions();
    };
    expect(saved({ bloom: true, ground: false })).toEqual({ glow: 0.5, ground: false, ...VIEW_DEFAULTS });
    expect(saved({ bloom: false, ground: true })).toMatchObject({ glow: 0, ground: true });
    expect(saved({ ground: false })).toMatchObject({ glow: 0, ground: false });
    expect(saved({})).toMatchObject({ glow: 0, ground: true });
    // A saved level counts, whatever the button was.
    expect(saved({ bloom: true, glow: 0.2 }).glow).toBe(0.2);
    expect(saved({ bloom: true, glow: 0 }).glow).toBe(0);
    expect(saved({ bloom: true, glow: "lots" }).glow).toBe(0.5);
    localStorage.setItem(KEY, "not json");
    expect(loadViewOptions()).toEqual({ glow: 0, ground: true, ...VIEW_DEFAULTS });
    localStorage.setItem(KEY, "null");
    expect(loadViewOptions()).toEqual({ glow: 0, ground: true, ...VIEW_DEFAULTS });
    // Setting a level saves it in the button's place.
    saved({ bloom: true, ground: true });
    useView3d.getState().setGlow(0.8);
    expect(JSON.parse(localStorage.getItem(KEY)!)).toMatchObject({ glow: 0.8, ground: true });
    expect(JSON.parse(localStorage.getItem(KEY)!)).not.toHaveProperty("bloom");
  });

  it("still applies when this computer won't store it", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    expect(() => useView3d.getState().setGlow(0.6)).not.toThrow();
    expect(useView3d.getState().glow).toBe(0.6);
  });
});

describe("the other view options", () => {
  const KEY = "pixelflow.view3dOptions";
  beforeEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
    useView3d.setState({ glow: 0, ground: true, ...VIEW_DEFAULTS });
  });

  it("are remembered on this computer beside the glow, never in the show", () => {
    useView3d.getState().setGlow(0.25);
    useView3d.getState().setViewOptions({ names: true, unlit: "hidden" });
    useView3d.getState().setViewOptions({ dotSize: "large", boost: 2, grid: false });
    expect(useView3d.getState()).toMatchObject({ glow: 0.25, grid: false, names: true, unlit: "hidden", dotSize: "large", boost: 2 });
    expect(JSON.parse(localStorage.getItem(KEY)!)).toEqual({ glow: 0.25, ground: true, grid: false, names: true, unlit: "hidden", dotSize: "large", boost: 2 });
    expect(loadViewOptions()).toEqual({ glow: 0.25, ground: true, grid: false, names: true, unlit: "hidden", dotSize: "large", boost: 2 });
    // Changing the glow or the ground keeps them.
    useView3d.getState().setGround(false);
    expect(loadViewOptions()).toMatchObject({ ground: false, names: true, boost: 2 });
  });

  it("keep to the choices there are", () => {
    useView3d.getState().setViewOptions({ boost: 7, unlit: "sometimes" as never, dotSize: "tiny" as never });
    expect(useView3d.getState()).toMatchObject({ boost: 0, unlit: "dim", dotSize: "medium" });
  });
});
