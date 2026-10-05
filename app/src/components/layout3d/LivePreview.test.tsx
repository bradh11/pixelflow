import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryBackend, emptyShow } from "../../api/memory";
import { newProp } from "../../lib/shows";
import { useLayoutEditor } from "../../state/layoutEditor";
import { useApp } from "../../state/store";
import { LivePreview } from "./LivePreview";
import type { Scene3d } from "./scene";

const fake = vi.hoisted(() => ({ colors: [] as { rgb: number[]; lit: boolean }[], gizmo: [] as unknown[] }));

vi.mock("./threeScene", () => ({
  createThreeScene: (): Scene3d => ({
    resize: () => {},
    setPixels: () => {},
    updatePixels: () => {},
    setColors: (rgb, lit) => fake.colors.push({ rgb: Array.from(rgb), lit }),
    setBulbSize: () => {},
    setBackdrop: () => {},
    setModel: async () => null,
    placeModel: () => null,
    surfaceAt: () => null,
    setSelectionBox: () => {},
    setGizmo: (g) => fake.gizmo.push(g),
    setOptions: () => {},
    render: () => {},
    dispose: () => {},
  }),
}));

describe("the live preview", () => {
  const descriptors = {
    clientWidth: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth"),
    clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight"),
  };
  beforeEach(() => {
    fake.colors.length = 0;
    fake.gizmo.length = 0;
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => 600 });
    Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 400 });
  });
  afterEach(() => {
    for (const [key, d] of Object.entries(descriptors)) if (d) Object.defineProperty(HTMLElement.prototype, key, d);
  });

  it("switches to a look-only 3D view lit by the screen's live colors, and remembers it", async () => {
    const show = emptyShow("Home");
    const line = newProp("line", show);
    line.shape = { source: "generator", type: "line", nodes: 2, length: 1 };
    show.props = [line];
    await useApp.getState().connect(new MemoryBackend(show));
    useLayoutEditor.getState().select([line.id]);
    const props = [{ prop: line.id, frameOffset: 0, channelsPerPixel: 3, points: [0, 0, 1, 0] }];
    const user = userEvent.setup();
    const { rerender } = render(<LivePreview props={props} frame={null} />);
    expect(screen.getByRole("img", { name: "Preview" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "3D" }));
    expect(screen.getByRole("application", { name: "3D preview" })).toBeInTheDocument();
    expect(localStorage.getItem("pixelflow.playMode")).toBe("3d");
    await waitFor(() => expect(fake.colors.at(-1)?.lit).toBe(false));
    rerender(<LivePreview props={props} frame={new Uint8Array([9, 8, 7, 6, 5, 4])} />);
    await waitFor(() => expect(fake.colors.at(-1)).toEqual({ rgb: [9, 8, 7, 6, 5, 4], lit: true }));
    // Look only: no gizmo, even with a prop selected in the editor.
    expect(fake.gizmo.every((g) => g === null)).toBe(true);
    await user.click(screen.getByRole("button", { name: "2D" }));
    expect(screen.getByRole("img", { name: "Preview" })).toBeInTheDocument();
  });
});
