import { Profiler, StrictMode } from "react";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryBackend, emptyShow } from "../api/memory";
import type { Edit, Prop, Show } from "../api/types";
import { newController } from "../lib/shows";
import { type Handle, type Pt, frameOfPoints, handlePositions, toScreen, toWorld } from "../lib/layoutMath";
import { newProp } from "../lib/shows";
import { useLayoutEditor } from "../state/layoutEditor";
import { useApp } from "../state/store";
import { DesktopLikeBackend } from "../test/desktopBackend";
import { LayoutScreen } from "./LayoutScreen";

vi.mock("../components/layout/useLayoutData", async (original) => ({
  ...(await original<typeof import("../components/layout/useLayoutData")>()),
  // jsdom can't decode images; pretend every photo is twice as wide as it is tall.
  imageAspect: async () => 0.5,
}));

const SIZE = { width: 800, height: 600 };

/** A line prop 5 units long (50 pixels), centered at (x, y). */
function line(name: string, x: number, y: number): Prop {
  const show = emptyShow("x");
  const prop = { ...newProp("line", show), name };
  prop.transform.position = { x, y, z: 0 };
  return prop;
}

/** A prop of the kind with its default size, its origin at (x, y), turned `deg`. */
function placed(kind: "arch" | "matrix", name: string, x: number, y: number, deg = 0): Prop {
  const prop = { ...newProp(kind, emptyShow("x")), name };
  prop.transform.position = { x, y, z: 0 };
  prop.transform.rotationDeg.z = deg;
  return prop;
}

function showWith(...props: Prop[]): Show {
  return { ...emptyShow("Test House"), props };
}

let backend: MemoryBackend;
let edits: Edit[][];

/** `delayMs`: how long the engine takes to apply each batch of edits. */
async function setup(show: Show, delayMs = 0, Engine: typeof MemoryBackend = MemoryBackend) {
  backend = new Engine(show);
  edits = [];
  const applyEdits = backend.applyEdits.bind(backend);
  backend.applyEdits = async (batch: Edit[]) => {
    edits.push(batch);
    if (delayMs) await new Promise((resolve) => setTimeout(resolve, delayMs));
    return applyEdits(batch);
  };
  await useApp.getState().connect(backend);
  useApp.setState({ started: true });
  const user = userEvent.setup();
  render(<LayoutScreen />);
  // The canvas fits the props in once their positions arrive.
  await waitFor(() => expect(useLayoutEditor.getState().view).not.toBeNull());
  return user;
}

const canvas = () => screen.getByRole("application", { name: "Layout canvas" });

function screenAt(world: Pt): Pt {
  return toScreen(useLayoutEditor.getState().view!, SIZE, world);
}

function pointer(kind: "pointerDown" | "pointerMove" | "pointerUp", world: Pt, init: Record<string, unknown> = {}) {
  const s = screenAt(world);
  fireEvent[kind](canvas(), { clientX: s.x, clientY: s.y, button: 0, pointerId: 1, ...init });
}

async function drag(from: Pt, to: Pt, init: Record<string, unknown> = {}) {
  // Work out both screen points first: the view doesn't change during a drag.
  const [a, b] = [screenAt(from), screenAt(to)];
  const c = canvas();
  await act(async () => {
    fireEvent.pointerDown(c, { clientX: a.x, clientY: a.y, button: 0, pointerId: 1, ...init });
    fireEvent.pointerMove(c, { clientX: (a.x + b.x) / 2, clientY: (a.y + b.y) / 2, pointerId: 1, ...init });
    fireEvent.pointerMove(c, { clientX: b.x, clientY: b.y, pointerId: 1, ...init });
    fireEvent.pointerUp(c, { clientX: b.x, clientY: b.y, pointerId: 1, ...init });
  });
}

async function click(world: Pt, init: Record<string, unknown> = {}) {
  await act(async () => {
    pointer("pointerDown", world, init);
    pointer("pointerUp", world, init);
  });
}

const position = (name: string) => backend.show.props.find((p) => p.name === name)!.transform.position;

describe("LayoutScreen", () => {
  const descriptors = {
    clientWidth: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth"),
    clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight"),
  };
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, "clientWidth", {
      configurable: true,
      get() {
        return this.tagName === "CANVAS" ? SIZE.width : 0;
      },
    });
    Object.defineProperty(HTMLElement.prototype, "clientHeight", {
      configurable: true,
      get() {
        return this.tagName === "CANVAS" ? SIZE.height : 0;
      },
    });
  });
  afterEach(() => {
    for (const [key, d] of Object.entries(descriptors)) if (d) Object.defineProperty(HTMLElement.prototype, key, d);
  });

  it("is a labelled canvas with instructions, a tool bar, and the props list", async () => {
    await setup(showWith(line("Gutter", 0, 0)));
    expect(canvas()).toHaveAccessibleDescription(/Click a prop to select it/);
    expect(canvas()).toHaveAccessibleDescription(/Command-A to select every prop/);
    expect(canvas()).toHaveAccessibleDescription(/Command-D duplicates it/);
    expect(canvas()).toHaveAccessibleDescription(/pick Line, Arch, Matrix, Tree, Circle, or Star in the tool bar/);
    const tools = screen.getByRole("toolbar", { name: "Layout tools" });
    expect(within(tools).getByRole("button", { name: "Select" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByLabelText("Name of Gutter")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Props list/ }));
    expect(screen.queryByLabelText("Name of Gutter")).not.toBeInTheDocument();
  });

  it("selects a prop by clicking near its pixels, adds with shift-click, and clears with Escape", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)));
    await click({ x: 1, y: 0.02 });
    expect(screen.getByLabelText("Name")).toHaveValue("Gutter");
    expect(screen.getByTestId("selection-announcer")).toHaveTextContent("Gutter selected");
    await click({ x: -1, y: 4 }, { shiftKey: true });
    expect(within(screen.getByRole("complementary", { name: "Properties" })).getByText("2 props selected")).toBeInTheDocument();
    expect(screen.getByTestId("selection-announcer")).toHaveTextContent("2 props selected");
    await click({ x: 1, y: 0.02 }, { shiftKey: true });
    expect(useLayoutEditor.getState().selected).toEqual([backend.show.props[1].id]);
    expect(edits).toEqual([]);
    await user.keyboard("{Escape}");
    expect(useLayoutEditor.getState().selected).toEqual([]);
    expect(screen.getByText("Background photo")).toBeInTheDocument();
    expect(screen.getByTestId("selection-announcer")).toHaveTextContent("Nothing selected");
  });

  it("box-selects everything with a pixel inside the dragged box", async () => {
    await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4), line("Far", 20, 0)));
    await drag({ x: -6, y: 6 }, { x: 3, y: -1 });
    const names = useLayoutEditor.getState().selected.map((id) => backend.show.props.find((p) => p.id === id)!.name);
    expect(names.sort()).toEqual(["Fence", "Gutter"]);
    expect(edits).toEqual([]);
  });

  it("moves selected props with one undoable edit per drag, snapping to the grid when on", async () => {
    await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)));
    await drag({ x: 0, y: 0 }, { x: 2.13, y: 1 });
    expect(edits).toHaveLength(1);
    expect(edits[0].map((e) => e.type)).toEqual(["updateProp"]);
    expect(position("Gutter").x).toBeCloseTo(2.13, 1);
    expect(position("Gutter").y).toBeCloseTo(1, 1);
    expect(position("Fence")).toMatchObject({ x: 0, y: 4 });

    await userEvent.click(screen.getByRole("button", { name: "Snap to grid" }));
    await waitFor(() => expect(edits).toHaveLength(1));
    const start = position("Gutter");
    await drag(start, { x: start.x + 1.1, y: start.y });
    expect(edits).toHaveLength(2);
    expect(position("Gutter").x % 0.5).toBeCloseTo(0, 5);

    await act(() => useApp.getState().undo());
    expect(position("Gutter").x).toBeCloseTo(2.13, 1);
  });

  it("draws a new prop where it was dragged, selects it, and goes back to Select", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    await user.click(screen.getByRole("button", { name: "Matrix" }));
    expect(screen.getByRole("button", { name: "Matrix" })).toHaveAttribute("aria-pressed", "true");
    await drag({ x: 4, y: 1 }, { x: 8, y: 3 });
    expect(edits).toHaveLength(1);
    const [add] = edits[0];
    expect(add.type).toBe("addProp");
    const prop = (add as { prop: Prop }).prop;
    expect(prop.shape).toMatchObject({ type: "matrix", columns: 32, rows: 16 });
    expect(prop.shape).toMatchObject({ width: expect.closeTo(4, 1), height: expect.closeTo(2, 1) });
    expect(prop.transform.position).toMatchObject({ x: expect.closeTo(6, 1), y: expect.closeTo(2, 1) });
    await waitFor(() => expect(useLayoutEditor.getState().selected).toEqual([prop.id]));
    expect(screen.getByRole("button", { name: "Select" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByLabelText("Columns")).toHaveValue("32");
  });

  it("cancels a drawing with Escape", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    await user.click(screen.getByRole("button", { name: "Arch" }));
    await act(async () => {
      pointer("pointerDown", { x: 4, y: 0 });
      pointer("pointerMove", { x: 8, y: 0 });
    });
    await user.keyboard("{Escape}");
    await act(async () => pointer("pointerUp", { x: 8, y: 0 }));
    expect(edits).toEqual([]);
    expect(screen.getByRole("button", { name: "Arch" })).toHaveAttribute("aria-pressed", "true");
    await user.keyboard("{Escape}");
    expect(screen.getByRole("button", { name: "Select" })).toHaveAttribute("aria-pressed", "true");
  });

  it("edits a prop's pixels, size, and placement from the properties panel", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
    const pixels = screen.getByLabelText("Pixels");
    await user.clear(pixels);
    await user.type(pixels, "120{Enter}");
    expect(edits.at(-1)).toEqual([{ type: "updateProp", prop: expect.objectContaining({ shape: expect.objectContaining({ nodes: 120 }) }) }]);
    const x = screen.getByLabelText("Position X");
    await user.clear(x);
    await user.type(x, "7.5{Enter}");
    expect(position("Gutter").x).toBe(7.5);
    const rotation = screen.getByLabelText("Rotation (degrees)");
    await user.clear(rotation);
    await user.type(rotation, "45{Enter}");
    expect(backend.show.props[0].transform.rotationDeg.z).toBe(45);
    const before = edits.length;
    await user.clear(pixels);
    await user.type(pixels, "lots{Enter}");
    expect(pixels).toHaveValue("120");
    expect(edits).toHaveLength(before);
    await user.selectOptions(screen.getByLabelText("Color order"), "GRB");
    expect(backend.show.props[0].colorOrder).toBe("GRB");
  });

  it("says where a prop is wired, or that it isn't", async () => {
    const wired = line("Gutter", 0, 0);
    const show = showWith(wired, line("Fence", 0, 4));
    const controller = newController("Falcon_F16V5_B9F5", "192.0.2.20", "ddp", 2);
    controller.ports[1].slots = [{ prop: wired.id, segment: null, nullPixels: 0, reverse: false, brightness: null, gamma: null, smartReceiver: null }];
    show.controllers = [controller];
    await setup(show);
    act(() => useLayoutEditor.getState().select([wired.id]));
    expect(screen.getByText("Port 2 on Falcon_F16V5_B9F5")).toBeInTheDocument();
    act(() => useLayoutEditor.getState().select([show.props[1].id]));
    expect(screen.getByText(/Not wired/)).toBeInTheDocument();
  });

  it("selects all, duplicates, nudges, and deletes from the keyboard, each as one undo step", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)));
    canvas().focus();
    await user.keyboard("{Meta>}a{/Meta}");
    expect(useLayoutEditor.getState().selected).toHaveLength(2);

    await user.keyboard("{Meta>}d{/Meta}");
    expect(edits).toHaveLength(1);
    expect(backend.show.props.map((p) => p.name)).toEqual(["Gutter", "Fence", "Gutter copy", "Fence copy"]);
    await waitFor(() => expect(useLayoutEditor.getState().selected).toHaveLength(2));

    await user.keyboard("{Shift>}{ArrowRight}{/Shift}");
    expect(edits).toHaveLength(2);
    expect(position("Gutter copy").x).toBeCloseTo(1.5);
    await user.keyboard("{ArrowUp}");
    expect(position("Fence copy").y).toBeCloseTo(3.6);

    await user.keyboard("{Delete}");
    expect(edits).toHaveLength(4);
    expect(backend.show.props.map((p) => p.name)).toEqual(["Gutter", "Fence"]);
    await act(() => useApp.getState().undo());
    expect(backend.show.props).toHaveLength(4);
  });

  it("leaves typing in a field alone", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
    const name = screen.getByLabelText("Name");
    await user.click(name);
    await user.keyboard("{Backspace}{ArrowLeft}");
    expect(edits).toEqual([]);
    expect(backend.show.props).toHaveLength(1);
  });

  it("lines up and spaces out several props", async () => {
    const user = await setup(showWith(line("A", 0, 0), line("B", 10, 2), line("C", 30, 5)));
    act(() => useLayoutEditor.getState().select(backend.show.props.map((p) => p.id)));
    await user.click(screen.getByRole("button", { name: "Align bottom edges" }));
    expect(edits).toHaveLength(1);
    expect(backend.show.props.map((p) => p.transform.position.y)).toEqual([0, 0, 0]);
    await user.click(screen.getByRole("button", { name: "Space evenly left to right" }));
    expect(position("B").x).toBeCloseTo(15);
    act(() => useLayoutEditor.getState().select(backend.show.props.slice(0, 2).map((p) => p.id)));
    expect(screen.getByRole("button", { name: "Space evenly left to right" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Delete 2 props" }));
    expect(edits.at(-1)?.map((e) => e.type)).toEqual(["removeProp", "removeProp"]);
  });

  it("turns and resizes the selection with its handles", async () => {
    await setup(showWith(line("Gutter", 0, 0)));
    act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
    // The box around a flat line is the line itself; its top-right corner is the right end.
    await drag({ x: 2.5, y: 0 }, { x: 5, y: 0 });
    expect(edits).toHaveLength(1);
    const t = backend.show.props[0].transform;
    expect(t.scale.x).toBeCloseTo(1.5, 1);
    expect(t.position.x).toBeCloseTo(1.25, 1);
  });

  it("turns the selection with its round handle, in 15° steps with Shift held from the start", async () => {
    await setup(showWith(line("Gutter", 0, 0)));
    act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
    // The turn handle sits 28 px above the middle of the selection box's top edge.
    const center = screenAt({ x: 0, y: 0 });
    const c = canvas();
    await act(async () => {
      fireEvent.pointerDown(c, { clientX: center.x, clientY: center.y - 28, button: 0, pointerId: 1, shiftKey: true });
      fireEvent.pointerMove(c, { clientX: center.x + 100, clientY: center.y - 90, pointerId: 1, shiftKey: true });
      fireEvent.pointerUp(c, { clientX: center.x + 100, clientY: center.y - 90, pointerId: 1, shiftKey: true });
    });
    expect(edits).toHaveLength(1);
    expect(backend.show.props[0].transform.rotationDeg.z).toBe(-45);
    expect(useLayoutEditor.getState().selected).toHaveLength(1);
  });

  it("adds a background photo placed behind the props, dims it, and removes it", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    backend.images.set("/photos/house.jpg", new Uint8Array([1, 2, 3]));
    backend.nextImagePath = "/photos/house.jpg";
    await user.click(screen.getByRole("button", { name: "Choose photo…" }));
    await waitFor(() => expect(backend.show.background).toBeTruthy());
    const bg = backend.show.background!;
    expect(bg).toMatchObject({ path: "/photos/house.jpg", opacity: 0.7 });
    // Centered on the props, and wider than them.
    expect(bg.x + bg.width / 2).toBeCloseTo(0);
    expect(bg.width).toBeGreaterThan(5);
    expect(edits).toHaveLength(1);

    const slider = screen.getByLabelText("Photo strength");
    fireEvent.change(slider, { target: { value: "40" } });
    fireEvent.change(slider, { target: { value: "30" } });
    expect(edits).toHaveLength(1);
    await act(async () => fireEvent.pointerUp(slider));
    expect(edits).toHaveLength(2);
    expect(backend.show.background?.opacity).toBeCloseTo(0.3);

    await user.click(screen.getByRole("button", { name: "Move or resize photo" }));
    expect(screen.getByRole("button", { name: "Edit photo" })).toHaveAttribute("aria-pressed", "true");
    const box = { x: bg.x + bg.width / 2, y: bg.y - 1 };
    await drag(box, { x: box.x + 2, y: box.y });
    expect(edits).toHaveLength(3);
    expect(backend.show.background?.x).toBeCloseTo(bg.x + 2);

    await user.click(screen.getByRole("button", { name: "Done moving photo" }));
    await user.click(screen.getByRole("button", { name: "Remove photo" }));
    expect(backend.show.background).toBeNull();
  });

  it("adds a prop from the menu beside the others", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    await user.selectOptions(screen.getByLabelText("Prop type"), "line");
    await user.click(screen.getByRole("button", { name: /add prop/i }));
    const added = backend.show.props[1];
    expect(added.transform.position.x).toBeCloseTo(6);
  });

  describe("while edits are on their way", () => {
    it("never loses a nudge: quick taps are one step each, a held key is one step", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)), 30);
      act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
      canvas().focus();
      await user.keyboard("{ArrowRight}{ArrowRight}");
      await waitFor(() => expect(position("Gutter").x).toBeCloseTo(0.2));
      expect(backend.undoStack).toHaveLength(2);

      await user.keyboard("{ArrowUp>3/}");
      await waitFor(() => expect(position("Gutter").y).toBeCloseTo(0.3));
      expect(backend.undoStack).toHaveLength(3);
      await act(() => useApp.getState().undo());
      expect(position("Gutter")).toMatchObject({ x: expect.closeTo(0.2), y: 0 });
    });

    it("sends a held key's move after a pause even if its release is missed", async () => {
      await setup(showWith(line("Gutter", 0, 0)));
      act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
      canvas().focus();
      fireEvent.keyDown(canvas(), { key: "ArrowLeft" });
      fireEvent.keyDown(canvas(), { key: "ArrowLeft", repeat: true });
      expect(edits).toHaveLength(0);
      await waitFor(() => expect(position("Gutter").x).toBeCloseTo(-0.2), { timeout: 2000 });
      expect(edits).toHaveLength(1);
    });

    it("keeps a quick second drag on top of the first", async () => {
      await setup(showWith(line("Gutter", 0, 0)), 30);
      await drag({ x: 0, y: 0 }, { x: 1, y: 0 });
      await drag({ x: 1, y: 0 }, { x: 3, y: 1 });
      await waitFor(() => expect(edits).toHaveLength(2));
      await waitFor(() => expect(position("Gutter")).toMatchObject({ x: expect.closeTo(3, 1), y: expect.closeTo(1, 1) }));
      expect(useLayoutEditor.getState().pending.every((p) => p.revision !== null)).toBe(true);
      await waitFor(() => expect(useLayoutEditor.getState().pending).toEqual([]));
    });
  });

  describe("in the desktop app (positions as raw bytes, engine revisions)", () => {
    // WebKit sends the pressed buttons and pointer type with every pointer event.
    const mouse = { pointerType: "mouse", buttons: 1 };

    it("selects an existing prop by clicking it, and moves it by dragging", async () => {
      await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)), 0, DesktopLikeBackend);
      await click({ x: 1, y: 4.02 }, mouse);
      expect(useLayoutEditor.getState().selected).toEqual([backend.show.props[1].id]);
      expect(screen.getByLabelText("Name")).toHaveValue("Fence");

      await drag({ x: 1, y: 4 }, { x: 3, y: 5 }, mouse);
      expect(edits).toHaveLength(1);
      expect(position("Fence")).toMatchObject({ x: expect.closeTo(2, 1), y: expect.closeTo(5, 1) });
      await waitFor(() => expect(useLayoutEditor.getState().pending).toEqual([]));

      // Again, from where it is now, with the engine's positions for the new revision.
      await drag({ x: 2, y: 5 }, { x: 2, y: 2 }, mouse);
      expect(edits).toHaveLength(2);
      expect(position("Fence")).toMatchObject({ x: expect.closeTo(2, 1), y: expect.closeTo(2, 1) });
      expect(useLayoutEditor.getState().selected).toEqual([backend.show.props[1].id]);
    });

    it("selects a prop by clicking inside it, not only on its pixels", async () => {
      await setup(showWith(placed("arch", "Garage Arch", 0, 0), line("Fence", 0, 4)), 0, DesktopLikeBackend);
      // Under the arch's curve, well away from any of its pixels.
      await click({ x: 0, y: 0.8 }, mouse);
      expect(useLayoutEditor.getState().selected).toEqual([backend.show.props[0].id]);
      await drag({ x: 0.3, y: 0.8 }, { x: 1.3, y: 0.8 }, mouse);
      expect(position("Garage Arch").x).toBeCloseTo(1, 1);
      // Outside everything: clears the selection.
      await click({ x: 6, y: -3 }, mouse);
      expect(useLayoutEditor.getState().selected).toEqual([]);
    });

    it("never gets stuck moving the view after Space was let go out of sight", async () => {
      await setup(showWith(line("Gutter", 0, 0)), 0, DesktopLikeBackend);
      canvas().focus();
      // Space held, then the window loses focus (its release goes elsewhere).
      fireEvent.keyDown(canvas(), { key: " " });
      fireEvent.blur(window);
      await click({ x: 1, y: 0 }, mouse);
      expect(useLayoutEditor.getState().selected).toEqual([backend.show.props[0].id]);
      // ⌘-Space belongs to the system (Spotlight), which keeps the release to itself.
      act(() => useLayoutEditor.getState().clear());
      fireEvent.keyDown(canvas(), { key: " ", metaKey: true });
      await click({ x: 1, y: 0 }, mouse);
      expect(useLayoutEditor.getState().selected).toEqual([backend.show.props[0].id]);
    });

    it("selects props that were already in the show, with a slow engine", async () => {
      await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)), 30, DesktopLikeBackend);
      await click({ x: -2, y: 0 }, mouse);
      expect(useLayoutEditor.getState().selected).toEqual([backend.show.props[0].id]);
      await drag({ x: -2, y: 0 }, { x: -2, y: -3 }, mouse);
      await waitFor(() => expect(position("Gutter").y).toBeCloseTo(-3, 1));
      await waitFor(() => expect(useLayoutEditor.getState().pending).toEqual([]));
      await click({ x: 0, y: -3 }, mouse);
      expect(useLayoutEditor.getState().selected).toEqual([backend.show.props[0].id]);
    });
  });

  describe("Shift keeps things straight", () => {
    it("draws a line level and an arch at 45°", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      await user.click(screen.getByRole("button", { name: "Line" }));
      await drag({ x: 5, y: 0 }, { x: 9, y: 0.7 }, { shiftKey: true });
      const added = () => (edits.at(-1)![0] as { prop: Prop }).prop;
      expect(added().transform.rotationDeg.z).toBe(0);
      expect(added().shape).toMatchObject({ length: expect.closeTo(4, 1) });
      await waitFor(() => expect(screen.getByRole("button", { name: "Select" })).toHaveAttribute("aria-pressed", "true"));

      await user.click(screen.getByRole("button", { name: "Arch" }));
      await drag({ x: 5, y: 0 }, { x: 8, y: 3.4 }, { shiftKey: true });
      expect(added().transform.rotationDeg.z).toBe(45);
      await user.click(screen.getByRole("button", { name: "Line" }));
      await drag({ x: 5, y: 0 }, { x: 5.3, y: -3 }, { shiftKey: true });
      expect(added().transform.rotationDeg.z).toBe(-90);
    });

    it("moves props straight across or up and down, even when Shift is pressed mid-drag", async () => {
      await setup(showWith(line("Gutter", 0, 0)));
      await click({ x: 0, y: 0 });
      await drag({ x: 0, y: 0 }, { x: 2, y: 0.6 }, { shiftKey: true });
      expect(position("Gutter")).toMatchObject({ x: expect.closeTo(2, 1), y: 0 });

      const [a, b] = [screenAt({ x: 2, y: 0 }), screenAt({ x: 2.4, y: 3 })];
      await act(async () => {
        fireEvent.pointerDown(canvas(), { clientX: a.x, clientY: a.y, button: 0, pointerId: 1 });
        fireEvent.pointerMove(canvas(), { clientX: b.x, clientY: b.y, pointerId: 1 });
        fireEvent.keyDown(window, { key: "Shift", shiftKey: true });
        fireEvent.pointerUp(canvas(), { clientX: b.x, clientY: b.y, pointerId: 1, shiftKey: true });
      });
      expect(edits).toHaveLength(2);
      expect(position("Gutter")).toMatchObject({ x: expect.closeTo(2, 1), y: expect.closeTo(3, 1) });
    });
  });

  describe("resizing", () => {
    /** Where a handle of the prop's frame (along axes turned `deg`) is, in world units. */
    async function handle(name: string, h: Handle, deg = 0): Promise<Pt> {
      const id = backend.show.props.find((p) => p.name === name)!.id;
      const points = (await backend.previewProps()).props.find((p) => p.prop === id)!.points;
      const view = useLayoutEditor.getState().view!;
      return toWorld(view, SIZE, handlePositions(frameOfPoints([points], deg)!, view, SIZE)[h]);
    }
    async function frameOf(name: string, deg = 0) {
      const id = backend.show.props.find((p) => p.name === name)!.id;
      return frameOfPoints([(await backend.previewProps()).props.find((p) => p.prop === id)!.points], deg)!.box;
    }
    const scaleOf = (name: string) => backend.show.props.find((p) => p.name === name)!.transform.scale;

    it("stretches width and height separately from a corner, or in proportion with Shift", async () => {
      await setup(showWith(placed("matrix", "Window", 0, 0)));
      await click({ x: 0, y: 0 });
      const box = await frameOf("Window");
      const [w, h] = [box.maxX - box.minX, box.maxY - box.minY];
      const ne = await handle("Window", "ne");
      await drag(ne, { x: ne.x + 1, y: ne.y + 1 });
      expect(edits).toHaveLength(1);
      expect(scaleOf("Window").x).toBeCloseTo((w + 1) / w, 2);
      expect(scaleOf("Window").y).toBeCloseTo((h + 1) / h, 2);
      // The opposite corner stays put.
      const after = await frameOf("Window");
      expect(after.minX).toBeCloseTo(box.minX, 2);
      expect(after.minY).toBeCloseTo(box.minY, 2);

      await act(() => useApp.getState().undo());
      const corner = await handle("Window", "ne");
      await drag(corner, { x: corner.x + 1, y: corner.y + 0.1 }, { shiftKey: true });
      expect(scaleOf("Window").x).toBeCloseTo(scaleOf("Window").y, 5);
      expect(scaleOf("Window").x).toBeGreaterThan(1.1);
    });

    it("stretches one way from a side handle", async () => {
      await setup(showWith(placed("matrix", "Window", 0, 0)));
      await click({ x: 0, y: 0 });
      const box = await frameOf("Window");
      const n = await handle("Window", "n");
      await drag(n, { x: n.x + 3, y: n.y + 1 });
      expect(edits).toHaveLength(1);
      expect(scaleOf("Window").x).toBe(1);
      expect(scaleOf("Window").y).toBeCloseTo((box.maxY - box.minY + 1) / (box.maxY - box.minY), 2);
      expect((await frameOf("Window")).minY).toBeCloseTo(box.minY, 2);
    });

    it("stretches a turned arch along its own width, exactly where the drag showed it", async () => {
      await setup(showWith(placed("arch", "Garage Arch", 1, 1, 30)));
      act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
      const box = await frameOf("Garage Arch", 30);
      const e = await handle("Garage Arch", "e", 30);
      const [c, s] = [Math.cos(Math.PI / 6), Math.sin(Math.PI / 6)];
      // One unit out along the arch's width, with a little wobble across it.
      await drag(e, { x: e.x + c - 0.2 * s, y: e.y + s + 0.2 * c });
      expect(edits).toHaveLength(1);
      const t = backend.show.props[0].transform;
      expect(t.rotationDeg.z).toBe(30);
      expect(t.scale.x).toBeCloseTo((box.maxX - box.minX + 1) / (box.maxX - box.minX), 2);
      expect(t.scale.y).toBe(1);
      const after = await frameOf("Garage Arch", 30);
      expect(after.minX).toBeCloseTo(box.minX, 2);
      expect(after.maxX).toBeCloseTo(box.maxX + 1, 2);
      expect(after.maxY - after.minY).toBeCloseTo(box.maxY - box.minY, 2);
    });
  });

  it("narrows a selection to the prop clicked without dragging", async () => {
    await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)));
    act(() => useLayoutEditor.getState().select(backend.show.props.map((p) => p.id)));
    await click({ x: 1, y: 4 });
    expect(useLayoutEditor.getState().selected).toEqual([backend.show.props[1].id]);
    expect(edits).toEqual([]);
  });

  it("keeps a prop turned at an angle in proportion when stretched with Shift", async () => {
    const turned = line("Gutter", 0, 0);
    turned.transform.rotationDeg.z = 30;
    await setup(showWith(turned));
    act(() => useLayoutEditor.getState().select([turned.id]));
    // The selection box's top-right corner: the line's upper end.
    const end = { x: 2.5 * Math.cos(Math.PI / 6), y: 2.5 * Math.sin(Math.PI / 6) };
    await drag(end, { x: end.x * 2, y: end.y * 1.2 }, { shiftKey: true });
    expect(edits).toHaveLength(1);
    const { scale } = backend.show.props[0].transform;
    expect(scale.x).toBeCloseTo(scale.y, 5);
  });

  it("moves the selection with arrow keys while its props-list checkbox has focus", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    const box = screen.getByLabelText("Select Gutter");
    await user.click(box);
    expect(box).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    await waitFor(() => expect(position("Gutter").x).toBeCloseTo(0.1));
  });

  it("allows a negative scale (mirrored) but not zero", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
    const sx = screen.getByLabelText("Scale X");
    await user.clear(sx);
    await user.type(sx, "-1{Enter}");
    expect(backend.show.props[0].transform.scale.x).toBe(-1);
    await user.clear(sx);
    await user.type(sx, "0{Enter}");
    expect(sx).toHaveValue("-1");
    expect(backend.show.props[0].transform.scale.x).toBe(-1);
  });

  it("zooms with a Safari-style pinch and pans with a plain scroll", async () => {
    await setup(showWith(line("Gutter", 0, 0)));
    const before = useLayoutEditor.getState().view!;
    const pinch = (type: string, scale: number) =>
      act(() => {
        canvas().dispatchEvent(Object.assign(new Event(type, { cancelable: true }), { scale, clientX: 400, clientY: 300 }));
      });
    pinch("gesturestart", 1);
    pinch("gesturechange", 1.5);
    pinch("gesturechange", 2);
    pinch("gestureend", 2);
    expect(useLayoutEditor.getState().view!.zoom).toBeCloseTo(before.zoom * 2);

    const zoomed = useLayoutEditor.getState().view!;
    act(() => {
      fireEvent.wheel(canvas(), { deltaY: 100, deltaMode: 0 });
    });
    const after = useLayoutEditor.getState().view!;
    expect(after.zoom).toBe(zoomed.zoom);
    expect(after.cy).not.toBe(zoomed.cy);
  });

  it("doesn't re-render the screen for live colors, panning, or zooming", async () => {
    backend = new MemoryBackend(showWith(line("Gutter", 0, 0)));
    backend.liveFrame = async () => new Uint8Array(150).fill(200);
    await useApp.getState().connect(backend);
    useApp.setState({ started: true });
    let commits = 0;
    render(
      <Profiler id="layout" onRender={() => commits++}>
        <LayoutScreen />
      </Profiler>,
    );
    await waitFor(() => expect(useLayoutEditor.getState().view).not.toBeNull());
    await act(() => new Promise((resolve) => setTimeout(resolve, 150)));
    const settled = commits;
    await act(() => new Promise((resolve) => setTimeout(resolve, 350)));
    act(() => {
      fireEvent.wheel(canvas(), { deltaY: 40 });
      fireEvent.wheel(canvas(), { deltaY: -10, metaKey: true });
      useLayoutEditor.getState().setView({ cx: 3, cy: 1, zoom: 50 });
    });
    expect(commits).toBe(settled);
  });

  it("stops photo editing when the photo is removed or its adding undone, and sends each strength change once", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    backend.images.set("/photos/house.jpg", new Uint8Array([1, 2, 3]));
    backend.nextImagePath = "/photos/house.jpg";
    await user.click(screen.getByRole("button", { name: "Choose photo…" }));
    await waitFor(() => expect(backend.show.background).toBeTruthy());

    const slider = screen.getByLabelText("Photo strength");
    fireEvent.change(slider, { target: { value: "40" } });
    await act(async () => {
      fireEvent.pointerUp(slider);
      fireEvent.keyUp(slider, { key: "Shift" });
      fireEvent.blur(slider);
    });
    await waitFor(() => expect(backend.show.background?.opacity).toBeCloseTo(0.4));
    expect(edits).toHaveLength(2);

    await user.click(screen.getByRole("button", { name: "Move or resize photo" }));
    expect(screen.getByRole("button", { name: "Edit photo" })).toHaveAttribute("aria-pressed", "true");
    await user.click(screen.getByRole("button", { name: "Remove photo" }));
    expect(useLayoutEditor.getState().editPhoto).toBe(false);
    expect(screen.getByRole("button", { name: "Add photo…" })).not.toHaveAttribute("aria-pressed", "true");

    await act(() => useApp.getState().undo());
    await user.click(screen.getByRole("button", { name: "Edit photo" }));
    expect(useLayoutEditor.getState().editPhoto).toBe(true);
    await act(() => useApp.getState().undo());
    await act(() => useApp.getState().undo());
    expect(backend.show.background).toBeNull();
    expect(useLayoutEditor.getState().editPhoto).toBe(false);
  });

  it("reads the photo again when the same file is chosen again, or on Try again", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    backend.images.set("/photos/house.jpg", new Uint8Array([1, 2, 3]));
    backend.nextImagePath = "/photos/house.jpg";
    await user.click(screen.getByRole("button", { name: "Choose photo…" }));
    await waitFor(() => expect(backend.show.background).toBeTruthy());
    const reads = vi.spyOn(backend, "readImage");
    await user.click(screen.getByRole("button", { name: "Replace…" }));
    await waitFor(() => expect(reads.mock.calls.filter(([p]) => p === "/photos/house.jpg").length).toBeGreaterThanOrEqual(2));

    // A photo that has gone missing, then comes back.
    const background = { ...backend.show.background!, path: "/photos/porch.jpg" };
    await act(() => useApp.getState().apply([{ type: "setBackground", background }]));
    expect(await screen.findByText("This photo was moved or deleted. Choose it again with Replace…")).toBeInTheDocument();
    backend.images.set("/photos/porch.jpg", new Uint8Array([1, 2, 3]));
    reads.mockClear();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(screen.queryByText(/moved or deleted/)).not.toBeInTheDocument());
    expect(reads).toHaveBeenCalledWith("/photos/porch.jpg");
  });

  it("keeps drawing after mounting twice in strict mode", async () => {
    // A context that only counts fills of the whole canvas (one per drawn frame).
    let frames = 0;
    const context = new Proxy({} as Record<string | symbol, unknown>, {
      get: (target, key) => {
        if (key in target) return target[key];
        if (key === "fillRect") return (_x: number, _y: number, w: number) => w === SIZE.width && frames++;
        return () => {};
      },
      set: (target, key, value) => {
        target[key] = value;
        return true;
      },
    });
    const getContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = (() => context) as unknown as typeof getContext;
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => setTimeout(() => cb(0), 0) as unknown as number);
    vi.stubGlobal("cancelAnimationFrame", (id: number) => clearTimeout(id));
    try {
      backend = new MemoryBackend(showWith(line("Gutter", 0, 0)));
      await useApp.getState().connect(backend);
      useApp.setState({ started: true });
      render(
        <StrictMode>
          <LayoutScreen />
        </StrictMode>,
      );
      await waitFor(() => expect(frames).toBeGreaterThan(0));
      const before = frames;
      act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
      await waitFor(() => expect(frames).toBeGreaterThan(before));
    } finally {
      HTMLCanvasElement.prototype.getContext = getContext;
      vi.unstubAllGlobals();
    }
  });
});
