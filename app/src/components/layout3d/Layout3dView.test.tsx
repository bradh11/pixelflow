import { act, render, waitFor } from "@testing-library/react";
import { StrictMode } from "react";
import { describe, expect, it } from "vitest";
import { emptyShow } from "../../api/memory";
import { Layout3dView } from "./Layout3dView";
import type { Scene3d, SceneFactory } from "./scene";

const stub: Scene3d = {
  resize: () => {},
  setPixels: () => {},
  updatePixels: () => {},
  setColors: () => {},
  setBulbSize: () => {},
  setBackdrop: () => {},
  setModel: async () => null,
  placeModel: () => null,
  surfaceAt: () => null,
  setSelectionBox: () => {},
  setGizmo: () => {},
  setOptions: () => {},
  render: () => {},
  dispose: () => {},
};

describe("the 3D view's renderer", () => {
  it("is made once per canvas even when React mounts the view twice (as it does in development), and let go of once the view is gone", async () => {
    // A real renderer on a canvas takes the canvas's one WebGL context: a second one made on the
    // same canvas shares it, and throwing the first away would lose the context for both.
    const made: { canvas: HTMLCanvasElement; disposed: number }[] = [];
    const factory: SceneFactory = async (canvas) => {
      await Promise.resolve();
      const record = { canvas, disposed: 0 };
      made.push(record);
      return { ...stub, dispose: () => void record.disposed++ };
    };
    const view = (
      <StrictMode>
        <Layout3dView preview={{ revision: 0, props: [] }} show={emptyShow("Home")} photo={{ image: null, aspect: 0.75, problem: null, reload: () => {} }} storageKey="k" frame={null} sceneFactory={factory} />
      </StrictMode>
    );
    const { unmount } = render(view);
    await waitFor(() => expect(made).toHaveLength(1));
    await act(async () => {});
    expect(made).toHaveLength(1);
    expect(made[0].disposed, "still in use").toBe(0);
    unmount();
    await waitFor(() => expect(made[0].disposed).toBe(1));
  });
});
