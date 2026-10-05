import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryBackend, emptyShow } from "../api/memory";
import type { Edit, Prop, Show } from "../api/types";
import { newController } from "../lib/shows";
import { type Pt, toScreen } from "../lib/layoutMath";
import { newProp } from "../lib/shows";
import { useLayoutEditor } from "../state/layoutEditor";
import { useApp } from "../state/store";
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

function showWith(...props: Prop[]): Show {
  return { ...emptyShow("Test House"), props };
}

let backend: MemoryBackend;
let edits: Edit[][];

async function setup(show: Show) {
  backend = new MemoryBackend(show);
  edits = [];
  const applyEdits = backend.applyEdits.bind(backend);
  backend.applyEdits = (batch: Edit[]) => {
    edits.push(batch);
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
    await click({ x: -1, y: 4 }, { shiftKey: true });
    expect(screen.getByText("2 props selected")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(useLayoutEditor.getState().selected).toEqual([]);
    expect(screen.getByText("Background photo")).toBeInTheDocument();
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
});
