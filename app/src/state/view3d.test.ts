import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { loadShowView, saveShowView, useView3d } from "./view3d";

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
