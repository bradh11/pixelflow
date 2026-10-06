import { afterEach, describe, expect, it, vi } from "vitest";
import { useLayoutEditor } from "./layoutEditor";

const fresh = async () => {
  vi.resetModules();
  return (await import("./layoutEditor")).useLayoutEditor;
};

describe("the smart guides setting", () => {
  afterEach(() => vi.restoreAllMocks());

  it("is on by default", async () => {
    expect((await fresh()).getState().smartGuides).toBe(true);
  });

  it("is remembered on this computer", async () => {
    useLayoutEditor.getState().setSmartGuides(false);
    expect(useLayoutEditor.getState().smartGuides).toBe(false);
    expect((await fresh()).getState().smartGuides).toBe(false);
    useLayoutEditor.getState().setSmartGuides(true);
    expect((await fresh()).getState().smartGuides).toBe(true);
  });

  it("still works when this computer won't store it", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    const store = await fresh();
    expect(store.getState().smartGuides).toBe(true);
    expect(() => store.getState().setSmartGuides(false)).not.toThrow();
    expect(store.getState().smartGuides).toBe(false);
  });
});
