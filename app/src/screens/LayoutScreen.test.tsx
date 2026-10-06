import { Profiler, StrictMode } from "react";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryBackend, emptyShow } from "../api/memory";
import type { Edit, Prop, Show } from "../api/types";
import { newController } from "../lib/shows";
import { type Handle, type Pt, frameOfPoints, handlePositions, toScreen, toWorld } from "../lib/layoutMath";
import { newProp, nodeCount } from "../lib/shows";
import { formatGap } from "../lib/smartGuides";
import { GUIDE_COLORS } from "../components/layout/guideMarks";
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
    expect(canvas()).toHaveAccessibleDescription(/pick Line, Arch, Matrix, Tree, or a shape under More shapes/);
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

  it("box-selects everything with a pixel inside a Shift-dragged box", async () => {
    await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4), line("Far", 20, 0)));
    await drag({ x: -6, y: 6 }, { x: 3, y: -1 }, { shiftKey: true });
    const names = useLayoutEditor.getState().selected.map((id) => backend.show.props.find((p) => p.id === id)!.name);
    expect(names.sort()).toEqual(["Fence", "Gutter"]);
    expect(edits).toEqual([]);
  });

  it("moves the view by dragging empty space, keeping the selection; a click there clears it", async () => {
    await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)));
    await click({ x: 1, y: 0.02 });
    const gutter = backend.show.props[0].id;
    expect(useLayoutEditor.getState().selected).toEqual([gutter]);
    const before = useLayoutEditor.getState().view!;
    const [a, b] = [screenAt({ x: 10, y: -5 }), screenAt({ x: 12, y: -5 })];
    await act(async () => {
      fireEvent.pointerDown(canvas(), { clientX: a.x, clientY: a.y, button: 0, pointerId: 1 });
      fireEvent.pointerMove(canvas(), { clientX: b.x, clientY: b.y, pointerId: 1 });
      fireEvent.pointerUp(canvas(), { clientX: b.x, clientY: b.y, pointerId: 1 });
    });
    const after = useLayoutEditor.getState().view!;
    expect(after.zoom).toBe(before.zoom);
    expect(after.cx).toBeCloseTo(before.cx - 2, 5);
    expect(after.cy).toBeCloseTo(before.cy, 5);
    expect(useLayoutEditor.getState().selected).toEqual([gutter]);
    expect(edits).toEqual([]);
    await click({ x: 10, y: -5 });
    expect(useLayoutEditor.getState().selected).toEqual([]);
  });

  it("moves the view with a right-button drag, even over a prop", async () => {
    await setup(showWith(line("Gutter", 0, 0)));
    const before = useLayoutEditor.getState().view!;
    await drag({ x: 0, y: 0 }, { x: 1, y: 0 }, { button: 2 });
    expect(useLayoutEditor.getState().view!.cx).toBeCloseTo(before.cx - 1, 5);
    expect(position("Gutter")).toMatchObject({ x: 0, y: 0 });
    expect(useLayoutEditor.getState().selected).toEqual([]);
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
    for (const [label, value] of [
      ["Position Z", "1.5"],
      ["Tilt (X°)", "-10"],
      ["Turn (Y°)", "30"],
    ]) {
      const field = screen.getByLabelText(label);
      await user.clear(field);
      await user.type(field, `${value}{Enter}`);
    }
    expect(backend.show.props[0].transform.position).toEqual({ x: 7.5, y: 0, z: 1.5 });
    expect(backend.show.props[0].transform.rotationDeg).toEqual({ x: -10, y: 30, z: 45 });
    const before = edits.length;
    await user.clear(pixels);
    await user.type(pixels, "lots{Enter}");
    expect(pixels).toHaveValue("120");
    expect(edits).toHaveLength(before);
    await user.selectOptions(screen.getByLabelText("Color order"), "GRB");
    expect(backend.show.props[0].colorOrder).toBe("GRB");
  });

  it("sets how a matrix is wired from the properties panel", async () => {
    const user = await setup(showWith(placed("matrix", "Window", 0, 0)));
    act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
    await user.selectOptions(screen.getByLabelText("Strings run"), "vertical");
    await user.selectOptions(screen.getByLabelText("First pixel"), "topRight");
    await user.click(screen.getByLabelText(/Zig-zag/));
    expect(backend.show.props[0].shape).toMatchObject({ wiring: { start: "topRight", orientation: "vertical", serpentine: false } });
    expect(edits).toHaveLength(3);
  });

  it("makes a tree flat, or part of the way round, from the properties panel", async () => {
    const tree = { ...newProp("tree", emptyShow("x")), name: "Mega Tree" };
    const user = await setup(showWith(tree));
    act(() => useLayoutEditor.getState().select([tree.id]));
    await user.selectOptions(screen.getByLabelText("Style"), "flat");
    const round = screen.getByLabelText("Goes round (°)");
    await user.clear(round);
    await user.type(round, "180{Enter}");
    expect(backend.show.props[0].shape).toMatchObject({ style: "flat", degrees: 180 });
    expect(edits).toHaveLength(2);
  });

  it("numbers a custom grid's squares in order, empties them, clears and resizes it, one undo step each", async () => {
    const grid = { ...newProp("customGrid", emptyShow("x")), name: "Sign" };
    grid.shape = { source: "generator", type: "customGrid", columns: 3, rows: 2, cells: [0, 0, 0, 0, 0, 0] };
    const user = await setup(showWith(grid));
    act(() => useLayoutEditor.getState().select([grid.id]));
    const cells = () => backend.show.props[0].shape as Extract<Prop["shape"], { type: "customGrid" }>;
    await user.click(screen.getByRole("gridcell", { name: "Row 2, column 1: empty" }));
    await user.click(screen.getByRole("gridcell", { name: "Row 1, column 3: empty" }));
    await user.click(screen.getByRole("gridcell", { name: "Row 1, column 1: empty" }));
    expect(cells().cells).toEqual([3, 0, 2, 1, 0, 0]);
    await user.click(screen.getByRole("gridcell", { name: "Row 1, column 3: pixel 2" }));
    expect(cells().cells).toEqual([3, 0, 0, 1, 0, 0]);
    const columns = screen.getByLabelText("Columns");
    await user.clear(columns);
    await user.type(columns, "2{Enter}");
    expect(cells()).toMatchObject({ columns: 2, rows: 2, cells: [3, 0, 1, 0] });
    await user.click(screen.getByRole("button", { name: "Clear all" }));
    expect(cells().cells).toEqual([0, 0, 0, 0]);
    expect(edits).toHaveLength(6);
    await act(() => useApp.getState().undo());
    expect(cells().cells).toEqual([3, 0, 1, 0]);
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

  describe("copy, cut, paste, and delete", () => {
    it("copies and pastes the selection as offset copies, selected, one undo step each (⌘ or Ctrl)", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)));
      await click({ x: 1, y: 0 });
      expect(canvas()).toHaveFocus();
      await user.keyboard("{Meta>}c{/Meta}");
      expect(edits).toEqual([]);
      await user.keyboard("{Meta>}v{/Meta}");
      expect(edits).toHaveLength(1);
      expect(backend.show.props.map((p) => p.name)).toEqual(["Gutter", "Fence", "Gutter copy"]);
      expect(position("Gutter copy")).toMatchObject({ x: 0.5, y: -0.5 });
      await waitFor(() => expect(useLayoutEditor.getState().selected).toEqual([backend.show.props[2].id]));

      await user.keyboard("{Control>}v{/Control}");
      expect(edits).toHaveLength(2);
      expect(position("Gutter copy 2")).toMatchObject({ x: 1, y: -1 });
      await act(() => useApp.getState().undo());
      expect(backend.show.props.map((p) => p.name)).toEqual(["Gutter", "Fence", "Gutter copy"]);
    });

    it("cuts the selection, and pastes it back where it was", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)));
      await click({ x: 1, y: 4 });
      const fence = backend.show.props[1];
      await user.keyboard("{Meta>}x{/Meta}");
      expect(edits).toHaveLength(1);
      expect(backend.show.props.map((p) => p.name)).toEqual(["Gutter"]);
      await waitFor(() => expect(useLayoutEditor.getState().selected).toEqual([]));
      await user.keyboard("{Control>}v{/Control}");
      expect(edits).toHaveLength(2);
      const pasted = backend.show.props[1];
      expect(pasted).toMatchObject({ name: "Fence", transform: fence.transform, shape: fence.shape });
      expect(pasted.id).not.toBe(fence.id);
      await waitFor(() => expect(useLayoutEditor.getState().selected).toEqual([pasted.id]));
    });

    it("deletes the selection with Backspace (the Mac's delete key) right after clicking it, or from the props list", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)));
      await click({ x: 1, y: 0 });
      expect(canvas()).toHaveFocus();
      await user.keyboard("{Backspace}");
      expect(edits).toHaveLength(1);
      expect(backend.show.props.map((p) => p.name)).toEqual(["Fence"]);

      await user.click(screen.getByLabelText("Select Fence"));
      await user.keyboard("{Delete}");
      expect(edits).toHaveLength(2);
      expect(backend.show.props).toEqual([]);
      // Nothing selected: nothing more to delete.
      await user.keyboard("{Backspace}");
      expect(edits).toHaveLength(2);
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

  describe("poly lines", () => {
    const polyOf = (name: string) => backend.show.props.find((p) => p.name === name)!.shape as Extract<Prop["shape"], { type: "polyLine" }>;
    const worldPoints = (name: string) => {
      const prop = backend.show.props.find((p) => p.name === name)!;
      return polyOf(name).vertices.map((v) => ({ x: v.x + prop.transform.position.x, y: v.y + prop.transform.position.y }));
    };
    const doubleClick = () => act(async () => void fireEvent.doubleClick(canvas()));

    /** A poly line from (x, y) through each point given as offsets, 10 pixels a stretch. */
    function polyLine(name: string, x: number, y: number, ...offsets: [number, number][]): Prop {
      const prop = { ...newProp("polyLine", emptyShow("x")), name };
      prop.transform.position = { x, y, z: 0 };
      prop.shape = {
        source: "generator",
        type: "polyLine",
        vertices: offsets.map(([dx, dy]) => ({ x: dx, y: dy, z: 0 })),
        segments: offsets.slice(1).map(() => ({ nodes: 10 })),
      };
      return prop;
    }

    it("draws a line that bends, a click per point, finished with a double-click as one undo step", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      await user.click(screen.getByRole("button", { name: "Poly Line" }));
      expect(screen.getByText(/double-click or Enter to finish/)).toBeInTheDocument();
      await click({ x: 4, y: 1 });
      await click({ x: 7, y: 1 });
      await click({ x: 7, y: 3 });
      await click({ x: 7, y: 3 });
      await doubleClick();
      expect(edits).toHaveLength(1);
      const [add] = edits[0];
      const prop = (add as { prop: Prop }).prop;
      expect(prop.shape).toMatchObject({ type: "polyLine", segments: [{ nodes: 30 }, { nodes: 20 }] });
      const pts = worldPoints(prop.name);
      [{ x: 4, y: 1 }, { x: 7, y: 1 }, { x: 7, y: 3 }].forEach((p, i) => {
        expect(pts[i].x).toBeCloseTo(p.x, 1);
        expect(pts[i].y).toBeCloseTo(p.y, 1);
      });
      await waitFor(() => expect(useLayoutEditor.getState().selected).toEqual([prop.id]));
      expect(screen.getByRole("button", { name: "Select" })).toHaveAttribute("aria-pressed", "true");
      await act(() => useApp.getState().undo());
      expect(backend.show.props).toHaveLength(1);
    });

    it("finishes with Enter, takes the last point off with Backspace, and stops with Escape", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      await user.click(screen.getByRole("button", { name: "Poly Line" }));
      await click({ x: 4, y: 1 });
      await click({ x: 6, y: 1 });
      await click({ x: 6, y: 4 });
      await user.keyboard("{Backspace}");
      expect(backend.show.props).toHaveLength(1);
      await user.keyboard("{Enter}");
      expect(edits).toHaveLength(1);
      expect(polyOf((edits[0][0] as { prop: Prop }).prop.name).vertices).toHaveLength(2);

      await user.click(screen.getByRole("button", { name: "Poly Line" }));
      await click({ x: 4, y: 3 });
      await click({ x: 6, y: 3 });
      await user.keyboard("{Escape}");
      expect(screen.getByRole("button", { name: "Poly Line" })).toHaveAttribute("aria-pressed", "true");
      await user.keyboard("{Enter}");
      expect(edits).toHaveLength(1);
    });

    it("takes a point off with Backspace while drawing, never the selected props, even after a trip to 3D", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      // The canvas is mounted again on the way back from 3D, after the layout keys.
      await user.click(screen.getByRole("button", { name: "3D" }));
      await user.click(screen.getByRole("button", { name: "2D" }));
      await waitFor(() => expect(canvas()).toBeInTheDocument());
      act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
      await user.click(screen.getByRole("button", { name: "Poly Line" }));
      await click({ x: 4, y: 1 });
      await click({ x: 6, y: 1 });
      await click({ x: 6, y: 3 });
      await user.keyboard("{Backspace}");
      await user.keyboard("{Delete}");
      expect(backend.show.props.map((p) => p.name)).toEqual(["Gutter"]);
      expect(edits).toEqual([]);
      await click({ x: 8, y: 1 });
      await user.keyboard("{Enter}");
      expect(edits).toHaveLength(1);
      expect(polyOf((edits[0][0] as { prop: Prop }).prop.name).vertices).toHaveLength(2);
      expect(backend.show.props).toHaveLength(2);
    });

    it("ignores the second click of a double-click, even a pixel off", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      await user.click(screen.getByRole("button", { name: "Poly Line" }));
      await click({ x: 4, y: 1 });
      await click({ x: 7, y: 1 });
      const s = screenAt({ x: 7, y: 1 });
      await act(async () => {
        fireEvent.pointerDown(canvas(), { clientX: s.x + 1, clientY: s.y + 0.6, button: 0, pointerId: 1, detail: 2 });
        fireEvent.pointerUp(canvas(), { clientX: s.x + 1, clientY: s.y + 0.6, button: 0, pointerId: 1, detail: 2 });
      });
      await doubleClick();
      const prop = (edits[0][0] as { prop: Prop }).prop;
      expect(polyOf(prop.name).segments).toEqual([{ nodes: 30 }]);
    });

    it("treats a click right next to the last point as the same point", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      await user.click(screen.getByRole("button", { name: "Poly Line" }));
      await click({ x: 4, y: 1 });
      await click({ x: 7, y: 1 });
      const s = screenAt({ x: 7, y: 1 });
      await act(async () => {
        fireEvent.pointerDown(canvas(), { clientX: s.x + 2, clientY: s.y, button: 0, pointerId: 1 });
        fireEvent.pointerUp(canvas(), { clientX: s.x + 2, clientY: s.y, button: 0, pointerId: 1 });
      });
      await user.keyboard("{Enter}");
      expect(polyOf((edits[0][0] as { prop: Prop }).prop.name).vertices).toHaveLength(2);
    });

    it("keeps a stretch at 45° steps with Shift, and joins another line's end exactly", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      await user.click(screen.getByRole("button", { name: "Poly Line" }));
      // The gutter runs from (-2.5, 0) to (2.5, 0): start just off its right end.
      await click({ x: 2.55, y: 0.04 });
      await click({ x: 6, y: 0.4 }, { shiftKey: true });
      await user.keyboard("{Enter}");
      const prop = (edits[0][0] as { prop: Prop }).prop;
      expect(prop.transform.position).toEqual({ x: 2.5, y: 0, z: 0 });
      expect(polyOf(prop.name).vertices[1].y).toBeCloseTo(0, 5);
    });

    it("moves a point by dragging it, joining the end of another line, as one undo step", async () => {
      await setup(showWith(line("Gutter", 0, 0), polyLine("Roof", 4, 2, [0, 0], [2, 1], [4, 0])));
      act(() => useLayoutEditor.getState().select([backend.show.props[1].id]));
      await drag({ x: 4, y: 2 }, { x: 2.55, y: 0.05 });
      expect(edits).toHaveLength(1);
      const pts = worldPoints("Roof");
      expect(pts[0].x).toBeCloseTo(2.5, 5);
      expect(pts[0].y).toBeCloseTo(0, 5);
      expect(pts[1]).toEqual({ x: 6, y: 3 });
      await act(() => useApp.getState().undo());
      expect(worldPoints("Roof")[0]).toEqual({ x: 4, y: 2 });
    });

    it("adds a point with a click on a stretch's middle, removes one with Option-click or Delete, and bends a stretch by dragging its middle", async () => {
      const user = await setup(showWith(polyLine("Roof", 0, 0, [0, 0], [4, 0], [4, 4])));
      act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
      await click({ x: 2, y: 0 });
      expect(polyOf("Roof").vertices).toHaveLength(4);
      expect(polyOf("Roof").segments.map((s) => s.nodes)).toEqual([5, 5, 10]);
      expect(useLayoutEditor.getState().polyPoint).toMatchObject({ index: 1 });
      await user.keyboard("{Delete}");
      expect(polyOf("Roof").vertices).toHaveLength(3);
      expect(backend.show.props).toHaveLength(1);
      await click({ x: 4, y: 4 }, { altKey: true });
      expect(polyOf("Roof").vertices).toEqual([
        { x: 0, y: 0, z: 0 },
        { x: 4, y: 0, z: 0 },
      ]);
      await drag({ x: 2, y: 0 }, { x: 2, y: 1.5 });
      const curve = polyOf("Roof").segments[0].curve!;
      expect(curve[0].y).toBeCloseTo(2, 2);
      expect(edits).toHaveLength(4);
    });

    it("sets each stretch's pixels, spreads them evenly, and curves or straightens a stretch from the panel", async () => {
      const user = await setup(showWith(polyLine("Roof", 0, 0, [0, 0], [4, 0], [4, 4])));
      act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
      const stretches = within(screen.getByRole("list", { name: "Stretches" }));
      const second = stretches.getByLabelText(/Stretch 2 pixels \(4 long\)/);
      await user.clear(second);
      await user.type(second, "25{Enter}");
      expect(polyOf("Roof").segments.map((s) => s.nodes)).toEqual([10, 25]);
      await user.click(screen.getByRole("button", { name: "Curve stretch 1" }));
      expect(polyOf("Roof").segments[0].curve).toBeTruthy();
      await user.click(screen.getByRole("button", { name: "Straighten stretch 1" }));
      expect(polyOf("Roof").segments[0].curve).toBeUndefined();
      await user.click(screen.getByLabelText("Spread the pixels evenly along the whole line"));
      expect(polyOf("Roof").spreadNodes).toBe(35);
      expect(screen.getByLabelText("Pixels")).toHaveValue("35");
      expect(edits).toHaveLength(4);
    });

    it("splits a poly line at the picked point into two props, as one undo step", async () => {
      const user = await setup(showWith(polyLine("Roof", 0, 0, [0, 0], [4, 0], [4, 4])));
      act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
      await click({ x: 4, y: 0 });
      await user.click(screen.getByRole("button", { name: "Split here" }));
      expect(edits).toHaveLength(1);
      expect(backend.show.props.map((p) => p.name)).toEqual(["Roof", "Roof (2)"]);
      expect(polyOf("Roof").vertices).toHaveLength(2);
      expect(worldPoints("Roof (2)")).toEqual([
        { x: 4, y: 0 },
        { x: 4, y: 4 },
      ]);
      await act(() => useApp.getState().undo());
      expect(backend.show.props).toHaveLength(1);
    });

    it("adds a bend to a straight line, then drags the bend", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
      await user.click(screen.getByRole("button", { name: "Add bend" }));
      expect(polyOf("Gutter").vertices).toHaveLength(3);
      expect(nodeCount(backend.show.props[0].shape)).toBe(50);
      await drag({ x: 0, y: 0 }, { x: 0, y: 1 });
      expect(worldPoints("Gutter")[1]).toEqual({ x: 0, y: 1 });
    });

    it("joins two lines whose ends touch into one poly line, saying what happens to the second's wiring", async () => {
      const roof = polyLine("Roof", 2.5, 0, [0, 0], [2, 2]);
      const show = showWith(line("Gutter", 0, 0), roof);
      const controller = newController("Porch", "10.0.0.9", "ddp", 1);
      controller.ports[0].slots.push({ prop: roof.id, segment: null, nullPixels: 0, reverse: false, brightness: null, gamma: null, smartReceiver: null });
      show.controllers.push(controller);
      const user = await setup(show);
      act(() => useLayoutEditor.getState().select(backend.show.props.map((p) => p.id)));
      expect(screen.getByText(/Roof's own wiring is removed; the joined line keeps Gutter's/)).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Join into one poly line" }));
      expect(edits).toHaveLength(1);
      expect(backend.show.props.map((p) => p.name)).toEqual(["Gutter"]);
      expect(backend.show.controllers[0].ports[0].slots).toEqual([]);
      const pts = worldPoints("Gutter");
      [{ x: -2.5, y: 0 }, { x: 2.5, y: 0 }, { x: 4.5, y: 2 }].forEach((p, i) => {
        expect(pts[i].x).toBeCloseTo(p.x, 5);
        expect(pts[i].y).toBeCloseTo(p.y, 5);
      });
      expect(nodeCount(backend.show.props[0].shape)).toBe(60);
      await act(() => useApp.getState().undo());
      expect(backend.show.props).toHaveLength(2);
    });

    it("lets either line keep its wiring when joining, carries submodels, and warns which line runs backwards", async () => {
      // Both lines start where they meet, so one has to run backwards.
      const roof = polyLine("Roof", 2.5, 0, [0, 0], [2, 2]);
      const eave = polyLine("Eave", 2.5, 0, [0, 0], [3, 0]);
      eave.regions = [{ id: "e1", name: "Tip", kind: "nodes", lines: [[{ first: 0, last: 1 }]], layout: "horizontal", buffer: "default" }];
      const show = showWith(roof, eave);
      const controller = newController("Porch", "10.0.0.9", "ddp", 1);
      controller.ports[0].slots.push({ prop: roof.id, segment: null, nullPixels: 0, reverse: false, brightness: null, gamma: null, smartReceiver: null });
      show.controllers.push(controller);
      const user = await setup(show);
      act(() => useLayoutEditor.getState().select([roof.id, eave.id]));
      expect(screen.getByText(/Eave will run backwards, from its far end/)).toBeInTheDocument();
      await user.click(screen.getByRole("radio", { name: "Roof" }));
      expect(screen.getByText(/become one poly line named Roof/)).toBeInTheDocument();
      expect(screen.getByText(/Roof's controller port feeds it from there/)).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Join into one poly line" }));
      expect(edits).toHaveLength(1);
      expect(backend.show.props.map((p) => p.name)).toEqual(["Roof"]);
      expect(backend.show.controllers[0].ports[0].slots.map((s) => s.prop)).toEqual([roof.id]);
      // Eave's 10 pixels run backwards first: its pixels 0-1 are now 9-8.
      expect(backend.show.props[0].regions).toEqual([{ ...eave.regions[0], lines: [[{ first: 9, last: 8 }]] }]);
    });

    it("deletes a two-point line when its picked point is deleted (a line needs two points)", async () => {
      const user = await setup(showWith(polyLine("Roof", 0, 0, [0, 0], [4, 0])));
      act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
      await click({ x: 4, y: 0 });
      expect(useLayoutEditor.getState().polyPoint).toMatchObject({ index: 1 });
      await user.keyboard("{Delete}");
      expect(backend.show.props).toEqual([]);
      expect(edits).toHaveLength(1);
    });

    it("offers the less common shapes under More shapes", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      await user.click(screen.getByRole("button", { name: "More shapes" }));
      await user.click(screen.getByRole("menuitem", { name: "Star" }));
      expect(useLayoutEditor.getState().tool).toBe("star");
      expect(screen.getByRole("button", { name: "Star" })).toHaveAttribute("aria-pressed", "true");
      await user.click(screen.getByRole("button", { name: "Star" }));
      await user.keyboard("{Escape}");
      expect(screen.queryByRole("menu")).not.toBeInTheDocument();
      expect(useLayoutEditor.getState().tool).toBe("star");
    });
  });

  /** Picks `name` under More shapes, drags from `a` to `b`, and returns the prop added. */
  async function drawFromMore(user: ReturnType<typeof userEvent.setup>, name: string, a: Pt, b: Pt): Promise<Prop> {
    await user.click(screen.getByRole("button", { name: "More shapes" }));
    await user.click(screen.getByRole("menuitem", { name }));
    await drag(a, b);
    expect(edits).toHaveLength(1);
    const [add] = edits[0];
    expect(add.type).toBe("addProp");
    const prop = (add as { prop: Prop }).prop;
    await waitFor(() => expect(useLayoutEditor.getState().selected).toEqual([prop.id]));
    return prop;
  }

  describe("candy canes and icicles", () => {
    it("draws candy canes from one end to the other and turns their hooks from the panel", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      const prop = await drawFromMore(user, "Candy canes", { x: 2, y: 1 }, { x: 6, y: 1 });
      expect(prop.shape).toMatchObject({ type: "candyCanes", canes: 3, nodesPerCane: 18, width: expect.closeTo(4, 1) });
      expect(prop.transform.position).toMatchObject({ x: expect.closeTo(4, 1), y: expect.closeTo(1, 1) });
      expect(screen.getByLabelText("Pixels per cane")).toHaveValue("18");
      await user.click(screen.getByLabelText("Hooks point left"));
      expect(edits).toHaveLength(2);
      expect(edits[1]).toEqual([{ type: "updateProp", prop: expect.objectContaining({ shape: expect.objectContaining({ reverse: true }) }) }]);
    });

    it("draws icicles along a line and changes their drop pattern from the panel", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      const prop = await drawFromMore(user, "Icicles", { x: -2, y: 3 }, { x: 4, y: 3 });
      expect(prop.shape).toMatchObject({ type: "icicles", drops: [3, 4, 5, 4], width: expect.closeTo(6, 1) });
      const pattern = screen.getByLabelText("Drop pattern");
      expect(pattern).toHaveValue("3,4,5,4");
      await user.clear(pattern);
      await user.type(pattern, "2, 6,0{Enter}");
      expect(edits).toHaveLength(2);
      expect(edits[1]).toEqual([{ type: "updateProp", prop: expect.objectContaining({ shape: expect.objectContaining({ drops: [2, 6, 0] }) }) }]);
      expect(backend.show.props.at(-1)!.shape).toMatchObject({ drops: [2, 6, 0] });
    });
  });

  describe("window frames, wreaths and spinners", () => {
    /** The one edit after the prop was added: an update to its shape. */
    const shapeEdit = () => {
      expect(edits).toHaveLength(2);
      const [update] = edits[1];
      expect(update.type).toBe("updateProp");
      return (update as { prop: Prop }).prop.shape;
    };

    it("draws a window frame as a box and picks the corner its string starts at from the panel", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      const prop = await drawFromMore(user, "Window frame", { x: 1, y: 1 }, { x: 5, y: 4 });
      expect(prop.shape).toMatchObject({ type: "windowFrame", width: expect.closeTo(4, 1), height: expect.closeTo(3, 1) });
      expect(prop.transform.position).toMatchObject({ x: expect.closeTo(3, 1), y: expect.closeTo(2.5, 1) });
      expect(screen.getByLabelText("Pixels across the top")).toHaveValue("20");
      await user.selectOptions(screen.getByLabelText("First pixel"), "topRight");
      expect(shapeEdit()).toMatchObject({ start: "topRight" });
    });

    it("draws a wreath in a box and starts it at the bottom from the panel", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      const prop = await drawFromMore(user, "Wreath", { x: 0, y: 0 }, { x: 4, y: 4 });
      expect(prop.shape).toMatchObject({ type: "wreath", nodes: 50, radius: expect.closeTo(2, 1) });
      await user.click(screen.getByLabelText("Starts at the bottom"));
      expect(shapeEdit()).toMatchObject({ startAtBottom: true });
    });

    it("draws a spinner in a box and changes its arms from the panel", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      const prop = await drawFromMore(user, "Spinner", { x: 0, y: 0 }, { x: 4, y: 4 });
      expect(prop.shape).toMatchObject({ type: "spinner", arms: 6, radius: expect.closeTo(2, 1) });
      const arms = screen.getByLabelText("Arms");
      await user.clear(arms);
      await user.type(arms, "8{Enter}");
      expect(shapeEdit()).toMatchObject({ arms: 8 });
      expect(backend.show.props.at(-1)!.shape).toMatchObject({ arms: 8 });
    });

    it("draws a sphere in a box and sets how far round it goes from the panel", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      const prop = await drawFromMore(user, "Sphere", { x: 0, y: 0 }, { x: 4, y: 4 });
      expect(prop.shape).toMatchObject({ type: "sphere", columns: 16, rows: 20, radius: expect.closeTo(2, 1) });
      expect(screen.getByLabelText("Lowest pixels (latitude °)")).toHaveValue("-86");
      const round = screen.getByLabelText("Goes round (°)");
      await user.clear(round);
      await user.type(round, "180{Enter}");
      expect(shapeEdit()).toMatchObject({ degrees: 180 });
    });

    it("draws a cube in a box and picks its wiring style from the panel", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      const prop = await drawFromMore(user, "Cube", { x: 0, y: 0 }, { x: 4, y: 4 });
      expect(prop.shape).toMatchObject({ type: "cube", width: 5, spacing: expect.closeTo(1, 1) });
      await user.selectOptions(screen.getByLabelText("Strands run"), "stackedLeftRight");
      expect(shapeEdit()).toMatchObject({ style: "stackedLeftRight" });
    });
  });

  describe("xLights settings for arches, circles, stars and trees", () => {
    /** Selects the only prop and returns a reader for its shape in the backend. */
    async function selectOnly(prop: Prop) {
      const user = await setup(showWith(prop));
      act(() => useLayoutEditor.getState().select([prop.id]));
      return { user, shape: () => backend.show.props[0].shape as unknown as Record<string, unknown> };
    }

    it("sets an arch's count, curve and gap, and swaps to the layer settings once it has layers", async () => {
      const { user, shape } = await selectOnly(placed("arch", "Gate", 0, 0));
      expect(screen.getByLabelText("Curve (°)")).toHaveValue("180");
      expect(screen.queryByLabelText("Innermost layer (%)")).not.toBeInTheDocument();
      const arches = screen.getByLabelText("Arches");
      await user.clear(arches);
      await user.type(arches, "4{Enter}");
      const gap = screen.getByLabelText("Gap between arches");
      await user.clear(gap);
      await user.type(gap, "0.5{Enter}");
      const curve = screen.getByLabelText("Curve (°)");
      await user.clear(curve);
      await user.type(curve, "200{Enter}");
      expect(curve).toHaveValue("180");
      await user.clear(curve);
      await user.type(curve, "120{Enter}");
      expect(shape()).toMatchObject({ arches: 4, gap: 0.5, arc: 120 });
      await user.type(screen.getByLabelText("Layers (pixels each, inside first)"), "10,20,30{Enter}");
      // The arch's pixels become the layers' pixels.
      expect(shape()).toMatchObject({ layers: [10, 20, 30], nodes: 60 });
      expect(screen.queryByLabelText("Arches")).not.toBeInTheDocument();
      expect(screen.getByLabelText("Innermost layer (%)")).toHaveValue("70");
      await user.click(screen.getByLabelText("Starts on the innermost layer"));
      expect(shape()).toMatchObject({ startInside: true });
      const layers = screen.getByLabelText("Layers (pixels each, inside first)");
      await user.clear(layers);
      await user.type(layers, "{Enter}");
      expect(shape()).toMatchObject({ layers: [] });
      expect(screen.getByLabelText("Arches")).toHaveValue("4");
    });

    it("gives a circle rings inside each other and starts it at the bottom", async () => {
      const circle = { ...newProp("circle", emptyShow("x")), name: "Halo" };
      const { user, shape } = await selectOnly(circle);
      expect(screen.queryByLabelText("Innermost ring (%)")).not.toBeInTheDocument();
      await user.type(screen.getByLabelText("Rings (pixels each, inside first)"), "10,20{Enter}");
      const inner = screen.getByLabelText("Innermost ring (%)");
      await user.clear(inner);
      await user.type(inner, "40{Enter}");
      await user.click(screen.getByLabelText("Starts at the bottom"));
      expect(shape()).toMatchObject({ layers: [10, 20], nodes: 30, innerPercent: 40, startAtBottom: true });
    });

    it("wires a tree from the top, folds its zig-zag and winds it into a spiral", async () => {
      const tree = { ...newProp("tree", emptyShow("x")), name: "Mega" };
      const { user, shape } = await selectOnly(tree);
      await user.selectOptions(screen.getByLabelText("First pixel"), "topLeft");
      // The fold only shows with zig-zag on.
      const zigZag = screen.getByLabelText("Zig-zag (every other string runs back)");
      if ((zigZag as HTMLInputElement).checked) await user.click(zigZag);
      expect(screen.queryByLabelText("Zig-zag restarts every")).not.toBeInTheDocument();
      await user.click(zigZag);
      const fold = screen.getByLabelText("Zig-zag restarts every");
      await user.clear(fold);
      await user.type(fold, "3{Enter}");
      const spiral = screen.getByLabelText("Spiral turns");
      await user.clear(spiral);
      await user.type(spiral, "2.5{Enter}");
      expect(shape()).toMatchObject({ start: "topLeft", serpentine: true, strandsPerString: 3, spiralRotations: 2.5 });
      // Only round trees wind round.
      await user.selectOptions(screen.getByLabelText("Style"), "flat");
      expect(screen.queryByLabelText("Spiral turns")).not.toBeInTheDocument();
    });

    it("starts a star at a leg and runs it counter-clockwise", async () => {
      const star = { ...newProp("star", emptyShow("x")), name: "Topper" };
      const { user, shape } = await selectOnly(star);
      await user.selectOptions(screen.getByLabelText("First pixel"), "leftLeg");
      await user.click(screen.getByLabelText("Goes round counter-clockwise"));
      expect(shape()).toMatchObject({ start: "leftLeg", counterClockwise: true });
    });
  });

  describe("smart guides", () => {
    const zoom = () => useLayoutEditor.getState().view!.zoom;
    async function frameOf(name: string) {
      const id = backend.show.props.find((p) => p.name === name)!.id;
      return frameOfPoints([(await backend.previewProps()).props.find((p) => p.prop === id)!.points], 0)!.box;
    }
    /** A 3-wide matrix (the default is 4 wide). */
    function narrow(name: string, x: number, y: number): Prop {
      const prop = placed("matrix", name, x, y);
      prop.shape = { ...prop.shape, width: 3 } as Prop["shape"];
      return prop;
    }
    /**
     * Records the text drawn on the canvas, and the color of each line stroked, drawing at once
     * instead of on the next frame.
     */
    async function recordText() {
      const texts: string[] = [];
      const strokes: string[] = [];
      const context = new Proxy({} as Record<string | symbol, unknown>, {
        get: (target, key) => {
          if (key in target) return target[key];
          if (key === "fillText") return (text: string) => texts.push(text);
          if (key === "stroke") return () => strokes.push(String(target.strokeStyle));
          if (key === "measureText") return (text: string) => ({ width: text.length * 6 });
          return () => {};
        },
        set: (target, key, value) => {
          target[key] = value;
          return true;
        },
      });
      const getContext = HTMLCanvasElement.prototype.getContext;
      HTMLCanvasElement.prototype.getContext = (() => context) as unknown as typeof getContext;
      vi.stubGlobal("requestAnimationFrame", undefined);
      // Let a frame already asked for go by, so the next redraw draws at once.
      await act(() => new Promise((resolve) => setTimeout(resolve, 50)));
      return {
        texts,
        strokes,
        restore() {
          HTMLCanvasElement.prototype.getContext = getContext;
          vi.unstubAllGlobals();
        },
      };
    }

    it("are a tool bar toggle, on by default and remembered", async () => {
      const user = await setup(showWith(line("Gutter", 0, 0)));
      const toggle = screen.getByRole("button", { name: "Smart guides" });
      expect(toggle).toHaveAttribute("aria-pressed", "true");
      await user.click(toggle);
      expect(toggle).toHaveAttribute("aria-pressed", "false");
      expect(localStorage.getItem("pixelflow.smartGuides")).toBe("false");
    });

    it("line a dragged prop up with another's edges as one edit; Alt places it freely", async () => {
      await setup(showWith(placed("matrix", "A", 0, 0), placed("matrix", "B", 10, 3)));
      // Five pixels (within six) above lining up with A.
      const near = 5 / zoom();
      await drag({ x: 10, y: 3 }, { x: 10, y: near });
      expect(edits).toHaveLength(1);
      expect(edits[0].map((e) => e.type)).toEqual(["updateProp"]);
      expect(position("B")).toMatchObject({ x: expect.closeTo(10, 3), y: expect.closeTo(0, 3) });

      // Five pixels (more than a click) back up, with Option held.
      await drag({ x: 10, y: 0 }, { x: 10, y: near }, { altKey: true });
      expect(edits).toHaveLength(2);
      expect(position("B").y).toBeCloseTo(near, 2);
    });

    it("let go of a guide as soon as Option is pressed mid-drag", async () => {
      await setup(showWith(placed("matrix", "A", 0, 0), placed("matrix", "B", 10, 3)));
      const near = 5 / zoom();
      await act(async () => {
        pointer("pointerDown", { x: 10, y: 3 });
        pointer("pointerMove", { x: 10, y: near });
        fireEvent.keyDown(window, { key: "Alt", altKey: true });
        pointer("pointerUp", { x: 10, y: near }, { altKey: true });
      });
      expect(edits).toHaveLength(1);
      expect(position("B").y).toBeCloseTo(near, 2);
    });

    it("don't snap when turned off", async () => {
      const user = await setup(showWith(placed("matrix", "A", 0, 0), placed("matrix", "B", 10, 3)));
      await user.click(screen.getByRole("button", { name: "Smart guides" }));
      const near = 5 / zoom();
      await drag({ x: 10, y: 3 }, { x: 10, y: near });
      expect(position("B").y).toBeCloseTo(near, 2);
    });

    it("space a dragged prop as far from its neighbour as two others are apart, marking the gaps", async () => {
      await setup(showWith(placed("matrix", "A", 0, 0), placed("matrix", "B", 6, 0), placed("matrix", "C", 20, 0)));
      const [a, b] = [await frameOf("A"), await frameOf("B")];
      const gap = formatGap(b.minX - a.maxX);
      const canvasText = await recordText();
      try {
        const [from, to] = [screenAt({ x: 20, y: 0 }), screenAt({ x: 12 + 2 / zoom(), y: 0 })];
        await act(async () => {
          fireEvent.pointerDown(canvas(), { clientX: from.x, clientY: from.y, button: 0, pointerId: 1 });
          fireEvent.pointerMove(canvas(), { clientX: to.x, clientY: to.y, pointerId: 1 });
        });
        // Mid-drag: both equal gaps are marked.
        expect(canvasText.texts.filter((t) => t === gap)).toHaveLength(2);
        await act(async () => fireEvent.pointerUp(canvas(), { clientX: to.x, clientY: to.y, pointerId: 1 }));
        canvasText.texts.length = 0;
        act(() => useLayoutEditor.getState().setView({ ...useLayoutEditor.getState().view! }));
        expect(canvasText.texts).not.toContain(gap);
      } finally {
        canvasText.restore();
      }
      expect(edits).toHaveLength(1);
      expect(position("C").x).toBeCloseTo(12, 3);
    });

    it("go away when a drag is cancelled with Escape", async () => {
      const user = await setup(showWith(placed("matrix", "A", 0, 0), placed("matrix", "B", 6, 0), placed("matrix", "C", 20, 0)));
      const [a, b] = [await frameOf("A"), await frameOf("B")];
      const gap = formatGap(b.minX - a.maxX);
      const canvasText = await recordText();
      try {
        await act(async () => {
          pointer("pointerDown", { x: 20, y: 0 });
          pointer("pointerMove", { x: 12, y: 0 });
        });
        expect(canvasText.texts).toContain(gap);
        canvasText.texts.length = 0;
        await user.keyboard("{Escape}");
        expect(canvasText.texts).not.toContain(gap);
      } finally {
        canvasText.restore();
      }
      await act(async () => pointer("pointerUp", { x: 12, y: 0 }));
      expect(edits).toEqual([]);
    });

    it("resize a prop to the same width as another, marking both", async () => {
      await setup(showWith(placed("matrix", "A", 0, 0), narrow("B", 10, 5)));
      await click({ x: 10, y: 5 });
      const [a, b] = [await frameOf("A"), await frameOf("B")];
      const e = { x: b.maxX, y: (b.minY + b.maxY) / 2 };
      const to = { x: e.x + (a.maxX - a.minX) - (b.maxX - b.minX) + 2 / zoom(), y: e.y };
      const canvasText = await recordText();
      try {
        await act(async () => {
          pointer("pointerDown", e);
          pointer("pointerMove", to);
        });
        expect(canvasText.texts.filter((t) => t === "same width")).toHaveLength(2);
        await act(async () => pointer("pointerUp", to));
      } finally {
        canvasText.restore();
      }
      expect(edits).toHaveLength(1);
      const after = await frameOf("B");
      expect(after.maxX - after.minX).toBeCloseTo(a.maxX - a.minX, 2);
      expect(after.minX).toBeCloseTo(b.minX, 2);
    });

    it("snap a drawn prop's corner to another prop's edge", async () => {
      const user = await setup(showWith(placed("matrix", "A", 0, 0)));
      const a = await frameOf("A");
      await user.click(screen.getByRole("button", { name: "Matrix" }));
      await drag({ x: a.maxX + 2 / zoom(), y: 3 }, { x: a.maxX + 4, y: 5 });
      const prop = (edits[0][0] as { prop: Prop }).prop;
      expect(prop.transform.position.x).toBeCloseTo(a.maxX + 2, 2);
    });

    it("slide the end of a line held level with Shift to another prop's edge", async () => {
      const user = await setup(showWith(placed("matrix", "A", 0, 0)));
      const a = await frameOf("A");
      await user.click(screen.getByRole("button", { name: "Line" }));
      // From 8 left of A's right edge to just past it, a little uphill: Shift keeps it level.
      await drag({ x: a.maxX - 8, y: 5 }, { x: a.maxX + 2 / zoom(), y: 5.3 }, { shiftKey: true });
      const prop = (edits[0][0] as { prop: Prop }).prop;
      expect(prop.transform.rotationDeg.z).toBe(0);
      expect(prop.shape).toMatchObject({ length: expect.closeTo(8, 2) });
    });

    it("use upright boxes around turned props, both the moving one and the others", async () => {
      // Both turned upright: A covers y -2…2 and B y 4…8 (matrices are 4 wide, 2 tall).
      await setup(showWith(placed("matrix", "A", 0, 0, 90), placed("matrix", "B", 10, 6, 90)));
      expect((await frameOf("B")).minY).toBeCloseTo(4, 3);
      // B's bottom five pixels above A's bottom.
      await drag({ x: 10, y: 6 }, { x: 10, y: 5 / zoom() });
      expect(edits).toHaveLength(1);
      expect(position("B").y).toBeCloseTo(0, 3);
    });

    it("never guide a multi-selection by the props being moved with it", async () => {
      await setup(showWith(placed("matrix", "A", 0, 0), placed("matrix", "B", 10, 5), placed("matrix", "C", 10, 9)));
      act(() => useLayoutEditor.getState().select(backend.show.props.slice(1).map((p) => p.id)));
      // Five pixels right: C's old edges would pull B straight back (and the move would be lost).
      const near = 5 / zoom();
      await drag({ x: 10, y: 5 }, { x: 10 + near, y: 5 });
      expect(edits).toHaveLength(1);
      expect(position("B").x).toBeCloseTo(10 + near, 2);
      expect(position("C").x).toBeCloseTo(10 + near, 2);
    });

    it("keep a Shift-straight move straight, snapping only along it", async () => {
      await setup(showWith(placed("matrix", "A", 0, 0), placed("matrix", "B", 10, 3)));
      // Mostly downward with a little drift right: Shift keeps x, and y snaps level with A.
      await drag({ x: 10, y: 3 }, { x: 10.3, y: 5 / zoom() }, { shiftKey: true });
      expect(position("B").x).toBe(10);
      expect(position("B").y).toBeCloseTo(0, 3);
    });

    it("win over the grid when near, and leave the grid to it otherwise", async () => {
      const user = await setup(showWith(placed("matrix", "A", 0, 0.2), placed("matrix", "B", 10, 3)));
      await user.click(screen.getByRole("button", { name: "Snap to grid" }));
      // Five pixels above level with A (at 0.2, off the half-unit grid).
      await drag({ x: 10, y: 3 }, { x: 10, y: 0.2 + 5 / zoom() });
      expect(position("B").y).toBeCloseTo(0.2, 3);
      // Nowhere near a guide: the grid takes it to 3.5.
      await drag({ x: 10, y: 0.2 }, { x: 10, y: 3.3 });
      expect(position("B").y).toBe(3.5);
      expect(edits).toHaveLength(2);
    });

    describe("on poly lines", () => {
      /** A poly line from (x, y) through each point given as offsets, 10 pixels a stretch. */
      function polyLine(name: string, x: number, y: number, ...offsets: [number, number][]): Prop {
        const prop = { ...newProp("polyLine", emptyShow("x")), name };
        prop.transform.position = { x, y, z: 0 };
        prop.shape = {
          source: "generator",
          type: "polyLine",
          vertices: offsets.map(([dx, dy]) => ({ x: dx, y: dy, z: 0 })),
          segments: offsets.slice(1).map(() => ({ nodes: 10 })),
        };
        return prop;
      }
      /** The world points of the poly line `prop`. */
      function pointsOf(prop: Prop): Pt[] {
        const shape = prop.shape as Extract<Prop["shape"], { type: "polyLine" }>;
        return shape.vertices.map((v) => ({ x: v.x + prop.transform.position.x, y: v.y + prop.transform.position.y }));
      }
      const added = () => (edits[0][0] as { prop: Prop }).prop;
      const guideColor = () => GUIDE_COLORS[useApp.getState().theme].line;
      /** Zooms out so everything drawn here is on screen (only what's on screen guides). */
      const zoomOut = () => act(() => useLayoutEditor.getState().setView({ cx: 6, cy: 3, zoom: 30 }));
      function expectAt(p: Pt, want: Pt) {
        expect(p.x).toBeCloseTo(want.x, 3);
        expect(p.y).toBeCloseTo(want.y, 3);
      }

      it("line each point up with other props and the points placed before it; Option places one freely", async () => {
        const user = await setup(showWith(placed("matrix", "A", 0, 0)));
        zoomOut();
        const a = await frameOf("A");
        const px = 2 / zoom();
        await user.click(screen.getByRole("button", { name: "Poly Line" }));
        // Two pixels right of A's right edge.
        await click({ x: a.maxX + px, y: a.maxY + 3 });
        // Two pixels below level with A's top.
        await click({ x: a.maxX + 4, y: a.maxY - px });
        // Two pixels above level with the first point.
        await click({ x: a.maxX + 8, y: a.maxY + 3 + px });
        // The same, with Option held: where it was clicked.
        await click({ x: a.maxX + 12, y: a.maxY + 3 + px }, { altKey: true });
        await user.keyboard("{Enter}");
        expect(edits).toHaveLength(1);
        const pts = pointsOf(added());
        expectAt(pts[0], { x: a.maxX, y: a.maxY + 3 });
        expectAt(pts[1], { x: a.maxX + 4, y: a.maxY });
        expectAt(pts[2], { x: a.maxX + 8, y: a.maxY + 3 });
        expectAt(pts[3], { x: a.maxX + 12, y: a.maxY + 3 + px });
      });

      it("don't move points when turned off", async () => {
        const user = await setup(showWith(placed("matrix", "A", 0, 0)));
        const a = await frameOf("A");
        const px = 2 / zoom();
        await user.click(screen.getByRole("button", { name: "Smart guides" }));
        await user.click(screen.getByRole("button", { name: "Poly Line" }));
        await click({ x: a.maxX + px, y: a.maxY + 3 });
        await click({ x: a.maxX + 4, y: a.maxY - px });
        await user.keyboard("{Enter}");
        const pts = pointsOf(added());
        expectAt(pts[0], { x: a.maxX + px, y: a.maxY + 3 });
        expectAt(pts[1], { x: a.maxX + 4, y: a.maxY - px });
      });

      it("slide a point held at 45° with Shift along its line to a guide", async () => {
        const user = await setup(showWith(placed("matrix", "A", 0, 0)));
        const a = await frameOf("A");
        await user.click(screen.getByRole("button", { name: "Poly Line" }));
        await click({ x: a.maxX - 8, y: 5 });
        // Just past A's right edge, a little uphill: Shift keeps it level.
        await click({ x: a.maxX + 2 / zoom(), y: 5.3 }, { shiftKey: true });
        await user.keyboard("{Enter}");
        const pts = pointsOf(added());
        expectAt(pts[1], { x: a.maxX, y: 5 });
      });

      it("join another line's end before lining up with a guide nearer still", async () => {
        // The gutter ends at (2.5, 0); A's left edge is just right of that.
        const user = await setup(showWith(line("Gutter", 0, 0), placed("matrix", "A", 4.65, 3)));
        const a = await frameOf("A");
        const at = { x: 2.5 + (a.minX - 2.5) * 0.8, y: 0.02 };
        // In reach of both, and nearer A's edge than the line's end.
        expect(Math.abs(a.minX - at.x) * zoom()).toBeLessThan(6);
        expect(Math.abs(a.minX - at.x)).toBeLessThan(Math.abs(2.5 - at.x));
        await user.click(screen.getByRole("button", { name: "Poly Line" }));
        await click(at);
        await click({ x: 6, y: -3 });
        await user.keyboard("{Enter}");
        expect(added().transform.position).toEqual({ x: 2.5, y: 0, z: 0 });
      });

      it("draw the guides while placing a point, and none with Option held", async () => {
        const user = await setup(showWith(placed("matrix", "A", 0, 0)));
        const a = await frameOf("A");
        await user.click(screen.getByRole("button", { name: "Poly Line" }));
        const canvasText = await recordText();
        try {
          await act(async () => pointer("pointerMove", { x: a.maxX + 2 / zoom(), y: a.maxY + 3 }));
          expect(canvasText.strokes).toContain(guideColor());
          canvasText.strokes.length = 0;
          await act(async () => pointer("pointerMove", { x: a.maxX + 2 / zoom(), y: a.maxY + 3 }, { altKey: true }));
          expect(canvasText.strokes).not.toContain(guideColor());
        } finally {
          canvasText.restore();
        }
      });

      it("line a dragged point up with other props and the line's other points, drawing the guides; Option drags it freely", async () => {
        await setup(showWith(placed("matrix", "A", 0, 0), polyLine("Roof", 6, 4, [0, 0], [3, 2], [6, 0])));
        zoomOut();
        const a = await frameOf("A");
        const px = 2 / zoom();
        act(() => useLayoutEditor.getState().select([backend.show.props[1].id]));
        const canvasText = await recordText();
        try {
          // The middle point, to two pixels right of A's right edge and below level with the first point.
          await act(async () => {
            pointer("pointerDown", { x: 9, y: 6 });
            pointer("pointerMove", { x: a.maxX + px, y: 4 - px });
          });
          expect(canvasText.strokes).toContain(guideColor());
          await act(async () => pointer("pointerUp", { x: a.maxX + px, y: 4 - px }));
        } finally {
          canvasText.restore();
        }
        expect(edits).toHaveLength(1);
        const roof = () => backend.show.props.find((p) => p.name === "Roof")!;
        expectAt(pointsOf(roof())[1], { x: a.maxX, y: 4 });
        // Back out with Option held: where it was dropped.
        await drag({ x: a.maxX, y: 4 }, { x: a.maxX + px, y: 4 + 3 * px }, { altKey: true });
        expect(edits).toHaveLength(2);
        expectAt(pointsOf(roof())[1], { x: a.maxX + px, y: 4 + 3 * px });
      });
    });

    it.each(["Candy canes", "Icicles", "Window frame", "Wreath", "Spinner", "Sphere", "Cube"])(
      "snap the %s drawn from More shapes to other props' edges",
      async (name) => {
        const user = await setup(showWith(placed("matrix", "A", 0, 0)));
        act(() => useLayoutEditor.getState().setView({ cx: 4, cy: 4, zoom: 30 }));
        const a = await frameOf("A");
        const px = 2 / zoom();
        // Without the guide, the prop would sit half of that further right: far enough to tell.
        expect(px / 2).toBeGreaterThan(0.01);
        await user.click(screen.getByRole("button", { name: "More shapes" }));
        await user.click(screen.getByRole("menuitem", { name }));
        // From two pixels right of A's right edge: drawn by its ends or as a box, its middle is
        // 2.5 right of that edge.
        await drag({ x: a.maxX + px, y: a.maxY + 3 }, { x: a.maxX + 5, y: a.maxY + 8 });
        expect(edits).toHaveLength(1);
        const prop = (edits[0][0] as { prop: Prop }).prop;
        expect(prop.transform.position.x).toBeCloseTo(a.maxX + 2.5, 2);
      },
    );
  });
});
