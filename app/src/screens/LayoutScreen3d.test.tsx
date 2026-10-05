import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryBackend, emptyShow } from "../api/memory";
import type { Edit, Prop, Show } from "../api/types";
import type { Scene3d } from "../components/layout3d/scene";
import { type Orbit, type V3, boundsOfXyz, boxCenter3, gizmoLength, project, v3, viewProjection } from "../lib/layout3d";
import { newProp } from "../lib/shows";
import { useLayoutEditor } from "../state/layoutEditor";
import { useApp } from "../state/store";
import { LayoutScreen } from "./LayoutScreen";

/** A stand-in for the three.js renderer: records what the view asks it to draw. */
const fake = vi.hoisted(() => {
  const made: {
    pixels: Float32Array | null;
    colors: { rgb: Uint8Array; lit: boolean } | null;
    orbit: unknown;
    gizmo: unknown;
    selectionBox: unknown;
    calls: string[];
  }[] = [];
  return { made };
});

vi.mock("../components/layout3d/threeScene", () => ({
  // A model 10 wide, 6 tall, and 8 deep, standing on its base.
  measureModel: async () => ({ min: { x: -5, y: 0, z: -4 }, max: { x: 5, y: 6, z: 4 } }),
  createThreeScene: (): Scene3d => {
    const record = { pixels: null as Float32Array | null, colors: null as { rgb: Uint8Array; lit: boolean } | null, orbit: null as unknown, gizmo: null as unknown, selectionBox: null as unknown, calls: [] as string[] };
    fake.made.push(record);
    return {
      resize: () => record.calls.push("resize"),
      setPixels: (xyz) => {
        record.calls.push("setPixels");
        record.pixels = xyz.slice();
      },
      updatePixels: (start, xyz) => {
        record.calls.push("updatePixels");
        record.pixels?.set(xyz, start * 3);
      },
      setColors: (rgb, lit) => (record.colors = { rgb: rgb.slice(), lit }),
      setBulbSize: () => {},
      setBackdrop: () => {},
      setModel: async (model) => {
        record.calls.push(model ? `setModel:${model.name}` : "setModel:none");
        return model ? { min: { x: -5, y: 0, z: -4 }, max: { x: 5, y: 6, z: 4 } } : null;
      },
      placeModel: (p) => {
        record.calls.push(`placeModel:${p.scale}`);
        return null;
      },
      surfaceAt: () => null,
      setSelectionBox: (box) => (record.selectionBox = box),
      setGizmo: (gizmo) => (record.gizmo = gizmo),
      setOptions: () => {},
      render: (orbit) => (record.orbit = orbit),
      dispose: () => record.calls.push("dispose"),
    };
  },
}));

const SIZE = { width: 800, height: 600 };

function line(name: string, x: number, y: number, z = 0): Prop {
  const prop = { ...newProp("line", emptyShow("x")), name };
  prop.transform.position = { x, y, z };
  return prop;
}

function showWith(...props: Prop[]): Show {
  return { ...emptyShow("Test House"), props };
}

let backend: MemoryBackend;
let edits: Edit[][];

async function setup(show: Show) {
  backend = new MemoryBackend(show);
  edits = [];
  const applyEdits = backend.applyEdits.bind(backend);
  backend.applyEdits = async (batch: Edit[]) => {
    edits.push(batch);
    return applyEdits(batch);
  };
  await useApp.getState().connect(backend);
  useApp.setState({ started: true });
  const user = userEvent.setup();
  render(<LayoutScreen />);
  return user;
}

const scene = () => fake.made.at(-1)!;
const view3d = () => screen.getByRole("application", { name: "3D layout" });
const orbit = () => scene().orbit as Orbit;

/** Switches to 3D and waits for the camera to settle on the props. */
async function open3d(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "3D" }));
  await waitFor(() => expect(scene()?.orbit).toBeTruthy());
}

/** Where a world point is on the 3D view now. */
function screenOf(p: V3) {
  return project(viewProjection(orbit(), SIZE), SIZE, p)!;
}

function pointer(kind: "pointerDown" | "pointerMove" | "pointerUp", s: { x: number; y: number }, init: Record<string, unknown> = {}) {
  fireEvent[kind](view3d(), { clientX: s.x, clientY: s.y, button: 0, pointerId: 1, ...init });
}

async function clickAt(p: V3, init: Record<string, unknown> = {}) {
  const s = screenOf(p);
  await act(async () => {
    pointer("pointerDown", s, init);
    pointer("pointerUp", s, init);
  });
}

describe("the 3D layout", () => {
  const descriptors = {
    clientWidth: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth"),
    clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight"),
  };
  beforeEach(() => {
    fake.made.length = 0;
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => SIZE.width });
    Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => SIZE.height });
  });
  afterEach(() => {
    for (const [key, d] of Object.entries(descriptors)) if (d) Object.defineProperty(HTMLElement.prototype, key, d);
  });

  it("switches between 2D and 3D from the tool bar or with V, and remembers the choice", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    expect(screen.getByRole("application", { name: "Layout canvas" })).toBeInTheDocument();
    await open3d(user);
    expect(screen.queryByRole("application", { name: "Layout canvas" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "3D" })).toHaveAttribute("aria-pressed", "true");
    expect(localStorage.getItem("pixelflow.layoutMode")).toBe("3d");
    await user.keyboard("v");
    expect(screen.getByRole("application", { name: "Layout canvas" })).toBeInTheDocument();
    expect(scene().calls).toContain("dispose");
    await user.keyboard("v");
    expect(view3d()).toBeInTheDocument();
  });

  it("keeps drawing tools for 2D, saying so, and puts one down when switching to 3D", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    const tools = screen.getByRole("toolbar", { name: "Layout tools" });
    await user.click(within(tools).getByRole("button", { name: "Arch" }));
    await open3d(user);
    const arch = within(tools).getByRole("button", { name: "Arch" });
    expect(arch).toHaveAttribute("aria-disabled", "true");
    expect(arch).toHaveAttribute("title", expect.stringMatching(/2D view/));
    expect(within(tools).getByRole("button", { name: "Select" })).toHaveAttribute("aria-pressed", "true");
    await user.click(arch);
    expect(useLayoutEditor.getState().tool).toBe("select");
    expect(within(tools).queryByRole("button", { name: "Move view" })).not.toBeInTheDocument();
  });

  it("draws every pixel at its 3D position in one batch, unlit until something plays, then in live colors", async () => {
    const tree = { ...newProp("tree", emptyShow("x")), name: "Mega Tree" };
    const user = await setup(showWith(line("Gutter", 0, 0), tree));
    let frame = new Uint8Array(0);
    backend.liveFrame = async () => frame;
    await open3d(user);
    const pixels = scene().pixels!;
    expect(pixels.length).toBe((50 + 800) * 3);
    // The tree has depth.
    const zs = Array.from(pixels.subarray(50 * 3)).filter((_, i) => i % 3 === 2);
    expect(Math.max(...zs)).toBeGreaterThan(1);
    expect(scene().colors!.lit).toBe(false);
    frame = new Uint8Array(850 * 3).map((_, i) => i % 251);
    await waitFor(() => expect(scene().colors!.lit).toBe(true), { timeout: 3000 });
    expect(Array.from(scene().colors!.rgb.subarray(0, 6))).toEqual([0, 1, 2, 3, 4, 5]);
  });

  it("selects a prop by clicking its pixels, shift-click adds, and clicking empty space clears", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)));
    await open3d(user);
    const [gutter, fence] = backend.show.props.map((p) => p.id);
    await clickAt(v3(1, 0, 0));
    expect(useLayoutEditor.getState().selected).toEqual([gutter]);
    expect(screen.getByLabelText("Name")).toHaveValue("Gutter");
    await waitFor(() => expect(scene().selectionBox).toBeTruthy());
    await clickAt(v3(-1, 4, 0), { shiftKey: true });
    expect(useLayoutEditor.getState().selected).toEqual([gutter, fence]);
    await act(async () => {
      pointer("pointerDown", { x: 5, y: 5 });
      pointer("pointerUp", { x: 5, y: 5 });
    });
    expect(useLayoutEditor.getState().selected).toEqual([]);
  });

  it("moves the selection toward the street with the gizmo's Z arrow, as one undo step", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    await open3d(user);
    await clickAt(v3(1, 0, 0));
    await waitFor(() => expect(scene().gizmo).toBeTruthy());
    const box = boundsOfXyz([scene().pixels!])!;
    const origin = boxCenter3(box);
    const len = gizmoLength(orbit(), SIZE, origin);
    const from = screenOf(v3(origin.x, origin.y, origin.z + len * 0.75));
    const to = screenOf(v3(origin.x, origin.y, origin.z + 2));
    await act(async () => {
      pointer("pointerDown", from);
      pointer("pointerMove", { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 });
      pointer("pointerMove", to);
    });
    // While dragging, the pixels are drawn where the move puts them.
    expect(scene().calls).toContain("updatePixels");
    await act(async () => pointer("pointerUp", to));
    await waitFor(() => expect(edits).toHaveLength(1));
    const moved = backend.show.props[0].transform.position;
    expect(moved.x).toBe(0);
    expect(moved.y).toBe(0);
    // Grabbed three quarters along the arrow and let go at z = 2.
    expect(moved.z).toBeCloseTo(2 - len * 0.75, 1);
    expect(moved.z).toBeGreaterThan(1);
    await act(async () => void (await useApp.getState().undo()));
    expect(backend.show.props[0].transform.position.z).toBe(0);
  });

  it("zooms to a prop on double-click, and fits everything in on a double-click elsewhere", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0), line("Fence", 20, 6)));
    await open3d(user);
    const fitted = orbit().distance;
    fireEvent.doubleClick(view3d(), { clientX: screenOf(v3(1, 0, 0)).x, clientY: screenOf(v3(1, 0, 0)).y });
    await waitFor(() => expect(orbit().target.x).toBeCloseTo(0, 2), { timeout: 3000 });
    expect(orbit().distance).toBeLessThan(fitted);
    fireEvent.doubleClick(view3d(), { clientX: 2, clientY: 2 });
    await waitFor(() => expect(orbit().distance).toBeCloseTo(fitted, 2), { timeout: 3000 });
  });

  it("says what's selected, for screen readers", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    await open3d(user);
    await clickAt(v3(1, 0, 0));
    expect(screen.getByTestId("selection-announcer")).toHaveTextContent("Gutter selected");
  });

  it("drops a drag in progress with Escape", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    await open3d(user);
    await clickAt(v3(1, 0, 0));
    const from = screenOf(v3(1, 0, 0));
    await act(async () => {
      pointer("pointerDown", from);
      pointer("pointerMove", { x: from.x + 60, y: from.y + 30 });
    });
    await user.keyboard("{Escape}");
    await act(async () => pointer("pointerUp", { x: from.x + 60, y: from.y + 30 }));
    expect(edits).toEqual([]);
  });

  it("removes, duplicates, and copies the selection with the same keys as 2D", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)));
    await open3d(user);
    await clickAt(v3(1, 0, 0));
    await user.keyboard("{Meta>}d{/Meta}");
    await waitFor(() => expect(backend.show.props.map((p) => p.name)).toEqual(["Gutter", "Fence", "Gutter copy"]));
    await user.keyboard("{Delete}");
    await waitFor(() => expect(backend.show.props.map((p) => p.name)).toEqual(["Gutter", "Fence"]));
  });

  it("picks views with 1–5 and fits everything in with F", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0), line("Fence", 3, 4, -2)));
    await open3d(user);
    await user.keyboard("1");
    await waitFor(() => expect(Math.abs(orbit().yaw) + Math.abs(orbit().pitch)).toBeLessThan(1e-3), { timeout: 3000 });
    await user.keyboard("2");
    await waitFor(() => expect(orbit().pitch).toBeGreaterThan(1.5), { timeout: 3000 });
    await user.click(within(screen.getByRole("toolbar", { name: "3D view" })).getByRole("button", { name: "Right" }));
    await waitFor(() => expect(orbit().yaw).toBeCloseTo(Math.PI / 2, 3), { timeout: 3000 });
    const before = orbit().distance;
    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    await waitFor(() => expect(orbit().distance).toBeLessThan(before * 0.9), { timeout: 3000 });
    await user.keyboard("f");
    await waitFor(() => expect(orbit().distance).toBeCloseTo(before, 3), { timeout: 3000 });
  });

  it("remembers the camera for the show", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    await open3d(user);
    await user.keyboard("4");
    await waitFor(() => expect(JSON.parse(localStorage.getItem("pixelflow.view3d:unsaved:Test House") ?? "{}").orbit?.yaw).toBeCloseTo(Math.PI / 2), {
      timeout: 3000,
    });
  });

  it("stands the photo a chosen depth behind the props, remembered for the show", async () => {
    const show = showWith(line("Gutter", 0, 0));
    show.background = { path: "/house.jpg", x: -10, y: 8, width: 20, opacity: 0.7 };
    const user = await setup(show);
    expect(screen.queryByLabelText("Photo depth")).not.toBeInTheDocument();
    await open3d(user);
    expect(screen.getByText(/Drag to orbit/, { selector: "li" })).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Photo depth"), { target: { value: "3.5" } });
    expect(screen.getByText(/Photo depth in 3D: 3.5 behind/)).toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem("pixelflow.view3d:unsaved:Test House")!).photoDepth).toBe(3.5);
  });

  it("adds a house model sized to the photo and standing behind the props, then places and removes it", async () => {
    const show = showWith(line("Gutter", 0, 0));
    show.background = { path: "/house.jpg", x: -10, y: 8, width: 20, opacity: 0.7 };
    const user = await setup(show);
    backend.models.set("/models/house.obj", new TextEncoder().encode("v 0 0 0"));
    backend.nextModelPath = "/models/house.obj";
    await open3d(user);
    await user.click(screen.getByRole("button", { name: "Add house model…" }));
    await waitFor(() => expect(backend.show.houseModel).toBeTruthy());
    expect(backend.show.houseModel).toEqual({
      path: "/models/house.obj",
      position: { x: 0, y: 0, z: -8.02 },
      rotationDeg: { x: 0, y: 0, z: 0 },
      scale: 2,
      opacity: 1,
    });
    expect(edits).toHaveLength(1);
    await waitFor(() => expect(scene().calls).toContain("setModel:/models/house.obj"));
    expect(scene().calls).toContain("placeModel:2");

    const turn = screen.getByLabelText("Model turn (Y°)");
    await user.clear(turn);
    await user.type(turn, "15{Enter}");
    expect(backend.show.houseModel!.rotationDeg.y).toBe(15);
    await user.click(screen.getByRole("button", { name: "Remove model" }));
    await waitFor(() => expect(backend.show.houseModel ?? null).toBeNull());
    await waitFor(() => expect(scene().calls).toContain("setModel:none"));
  });

  it("explains when 3D can't be shown", async () => {
    const three = await import("../components/layout3d/threeScene");
    const spy = vi.spyOn(three, "createThreeScene").mockImplementation(() => {
      throw new Error("no WebGL");
    });
    const user = await setup(showWith(line("Gutter", 0, 0)));
    await user.click(screen.getByRole("button", { name: "3D" }));
    expect(await screen.findByText(/needs WebGL/)).toBeInTheDocument();
    spy.mockRestore();
  });
});
