import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { demoShow } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import type { Edit, Show } from "../api/types";
import { wiringBox, wiringView } from "../components/wiring/WiringPreview";
import { toScreen } from "../lib/layoutMath";
import { newController } from "../lib/shows";
import { useApp } from "../state/store";
import { useToasts } from "../state/toast";
import { useWiring } from "../state/wiring";
import { WiringScreen } from "./WiringScreen";

// Where things sit on screen (jsdom doesn't lay anything out): the props list on the left; each
// port 200 px tall from y = 100, in page order, its table rows 20 px tall from 30 px down; the
// click-to-wire canvas 1000 × 600 at the top left.
const PORT_TOP = 100;
const PORT_H = 200;
const ROWS_FROM = 30;
const ROW_H = 20;
const CANVAS = { width: 1000, height: 600 };

function rect(left: number, top: number, width: number, height: number): DOMRect {
  return { left, top, width, height, right: left + width, bottom: top + height, x: left, y: top, toJSON: () => ({}) } as DOMRect;
}

beforeEach(() => {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    const el = this as HTMLElement;
    if (el.dataset.wiringDrop === "props") return rect(0, 0, 250, 2000);
    if (el.dataset.wireCanvas !== undefined) return rect(0, 0, CANVAS.width, CANVAS.height);
    const ports = [...document.querySelectorAll("[data-wiring-drop='port']")];
    if (el.dataset.wiringDrop === "port") return rect(300, PORT_TOP + ports.indexOf(el) * PORT_H, 800, PORT_H - 10);
    if (el.dataset.wiringRow !== undefined) {
      const zone = el.closest("[data-wiring-drop='port']")!;
      const index = [...zone.querySelectorAll("[data-wiring-row]")].indexOf(el);
      return rect(300, PORT_TOP + ports.indexOf(zone) * PORT_H + ROWS_FROM + index * ROW_H, 800, ROW_H);
    }
    return rect(0, 0, 0, 0);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

let backend: MemoryBackend;
let edits: Edit[][];

async function setup(show: Show = demoShow()) {
  backend = new MemoryBackend(show);
  edits = [];
  const applyEdits = backend.applyEdits.bind(backend);
  backend.applyEdits = async (batch: Edit[]) => {
    edits.push(batch);
    return applyEdits(batch);
  };
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, screen: "wiring" });
  const user = userEvent.setup();
  render(<WiringScreen />);
  return user;
}

/** The point to drop at on port `port` (page order), before row `index` (or past the last one). */
const at = (port: number, index: number): [number, number] => [500, PORT_TOP + port * PORT_H + ROWS_FROM + index * ROW_H + 5];

function drag(el: Element, to: [number, number], end = true) {
  fireEvent.pointerDown(el, { clientX: 5, clientY: 5, button: 0, pointerId: 1 });
  fireEvent.pointerMove(el, { clientX: (5 + to[0]) / 2, clientY: (5 + to[1]) / 2, pointerId: 1 });
  fireEvent.pointerMove(el, { clientX: to[0], clientY: to[1], pointerId: 1 });
  if (end) fireEvent.pointerUp(el, { clientX: to[0], clientY: to[1], pointerId: 1 });
}

const names = (controller: number, port: number) => {
  const show = backend.show;
  return show.controllers[controller].ports[port].slots.map((s) => show.props.find((p) => p.id === s.prop)?.name);
};

/** A prop's row on a port, by the name of the button that opens its settings. */
const chip = (name: string) => screen.getByRole("button", { name });
/** The drag handle of a prop's row. */
const handle = (label: string, n = 0) => screen.getAllByRole("button", { name: `Drag ${label} to reorder` })[n];
const propItem = (name: string) => within(screen.getByRole("complementary", { name: "Props" })).getByRole("button", { name: new RegExp(`^${name},`) });

describe("wiring screen", () => {
  it("lists unwired props first, counts them, and finds props by name", async () => {
    const user = await setup();
    expect(screen.getByTestId("unwired-count")).toHaveTextContent("1 prop isn't wired yet");
    const items = within(screen.getByRole("complementary", { name: "Props" })).getAllByRole("button");
    expect(items[0]).toHaveAccessibleName(/^Porch Star, 100 pixels\. Not wired$/);
    expect(items[1]).toHaveAccessibleName(/Main FPP · Port 1/);
    await user.type(screen.getByPlaceholderText("Find a prop"), "tree");
    expect(within(screen.getByRole("complementary", { name: "Props" })).getAllByRole("button")).toHaveLength(1);
  });

  it("drags a prop from the list onto a port, where it's dropped, as one undo step", async () => {
    await setup();
    await act(async () => drag(propItem("Porch Star"), at(0, 1)));
    expect(names(0, 0)).toEqual(["Garage Arch", "Porch Star", "Window Matrix"]);
    expect(edits).toHaveLength(1);
    expect(screen.getByTestId("unwired-count")).toHaveTextContent("Every prop is wired.");
    await act(() => useApp.getState().undo());
    expect(names(0, 0)).toEqual(["Garage Arch", "Window Matrix"]);
  });

  it("shows where a drop will land while dragging, and Escape lets go without changing anything", async () => {
    await setup();
    await act(async () => drag(propItem("Porch Star"), at(4, 0), false));
    expect(useWiring.getState().drag?.over).toMatchObject({ kind: "port", port: 1, index: 0 });
    expect(screen.getByText("Let go to wire it here")).toBeInTheDocument();
    // The preview follows the port under the drag.
    expect(screen.getByTestId("wiring-preview-caption")).toHaveTextContent("Port 1 on Porch WLED has nothing wired yet.");
    await act(async () => fireEvent.keyDown(window, { key: "Escape" }));
    expect(useWiring.getState().drag).toBeNull();
    fireEvent.pointerUp(propItem("Porch Star"), { clientX: at(4, 0)[0], clientY: at(4, 0)[1], pointerId: 1 });
    expect(edits).toHaveLength(0);
  });

  it("reorders a row by its handle, moves it to another controller, and unwires it on the list", async () => {
    await setup();
    await act(async () => drag(handle("Garage Arch"), at(0, 2)));
    expect(names(0, 0)).toEqual(["Window Matrix", "Garage Arch"]);
    // The table follows: Garage Arch is now the 2nd row.
    expect(handle("Garage Arch").closest("tr")).toHaveTextContent(/^2Garage Arch/);

    await act(async () => drag(handle("Garage Arch"), at(4, 0)));
    expect(names(0, 0)).toEqual(["Window Matrix"]);
    expect(names(1, 0)).toEqual(["Garage Arch"]);
    // One step, though two controllers changed.
    expect(edits.at(-1)).toHaveLength(2);

    await act(async () => drag(handle("Garage Arch"), [100, 500]));
    expect(names(1, 0)).toEqual([]);
    expect(screen.getByTestId("unwired-count")).toHaveTextContent("2 props aren't wired yet");
    expect(edits).toHaveLength(3);
  });

  it("a click (no drag) opens the chip's settings", async () => {
    const user = await setup();
    await user.click(chip("Window Matrix on Main FPP port 1"));
    const settings = screen.getByRole("region", { name: "Window Matrix settings" });
    expect(settings).toHaveTextContent("Main FPP · Port 1 · 2nd on the port");
    expect(edits).toHaveLength(0);
  });

  it("moves and unwires rows from the keyboard", async () => {
    const user = await setup();
    chip("Garage Arch on Main FPP port 1").focus();
    await user.keyboard("{ArrowDown}");
    expect(chip("Window Matrix on Main FPP port 1")).toHaveFocus();
    // On to the next port's rows, and back.
    await user.keyboard("{ArrowDown}");
    expect(chip("Mega Tree on Main FPP port 2")).toHaveFocus();
    await user.keyboard("{ArrowUp}{ArrowUp}");
    expect(chip("Garage Arch on Main FPP port 1")).toHaveFocus();

    await user.keyboard("{Alt>}{ArrowDown}{/Alt}");
    expect(names(0, 0)).toEqual(["Window Matrix", "Garage Arch"]);
    // Focus follows the chip to its new place.
    await waitFor(() => expect(document.activeElement).toHaveAttribute("data-index", "1"));
    expect(document.activeElement).toHaveAccessibleName("Garage Arch on Main FPP port 1");
    await user.keyboard("{Alt>}{ArrowUp}{/Alt}");
    expect(names(0, 0)).toEqual(["Garage Arch", "Window Matrix"]);

    await user.keyboard("{Delete}");
    expect(names(0, 0)).toEqual(["Window Matrix"]);
    await waitFor(() => expect(chip("Window Matrix on Main FPP port 1")).toHaveFocus());

    await user.keyboard("{Enter}");
    expect(screen.getByRole("region", { name: "Window Matrix settings" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("region", { name: "Window Matrix settings" })).not.toBeInTheDocument();
  });

  it("changes a slot's settings, checking the numbers", async () => {
    const user = await setup();
    await user.click(chip("Garage Arch on Main FPP port 1"));
    const settings = screen.getByRole("region", { name: "Garage Arch settings" });
    await user.click(within(settings).getByLabelText(/Starts at the other end/));
    expect(backend.show.controllers[0].ports[0].slots[0].reverse).toBe(true);

    const nulls = within(settings).getByLabelText("Empty pixels before it");
    await user.clear(nulls);
    await user.type(nulls, "5000{Enter}");
    expect(nulls).toHaveValue("0");
    await user.clear(nulls);
    await user.type(nulls, "3{Enter}");
    expect(backend.show.controllers[0].ports[0].slots[0].nullPixels).toBe(3);

    const brightness = within(settings).getByLabelText("Brightness (%)");
    await user.type(brightness, "60{Enter}");
    expect(backend.show.controllers[0].ports[0].slots[0].brightness).toBe(60);
    await user.clear(brightness);
    await user.tab();
    expect(backend.show.controllers[0].ports[0].slots[0].brightness).toBeNull();

    await user.click(within(settings).getByLabelText(/The whole prop/));
    const to = within(settings).getByLabelText("To pixel");
    await user.clear(to);
    await user.type(to, "25{Enter}");
    expect(backend.show.controllers[0].ports[0].slots[0].segment).toEqual({ start: 0, end: 25 });
    expect(chip("Garage Arch · 1–25 on Main FPP port 1")).toBeInTheDocument();
    expect(screen.getByTestId("unwired-count")).toHaveTextContent("1 prop isn't wired yet");
    expect(propItem("Garage Arch")).toHaveAccessibleName(/25 of 50 pixels wired/);

    // Each change was its own undo step.
    expect(edits.every((batch) => batch.length === 1 && batch[0].type === "updateController")).toBe(true);
  });

  it("moves a slot to another port from its settings, and unwires it there", async () => {
    const user = await setup();
    await user.click(chip("Mega Tree on Main FPP port 2"));
    const settings = screen.getByRole("region", { name: "Mega Tree settings" });
    await user.selectOptions(within(settings).getByLabelText("Port"), "Porch WLED · Port 1");
    expect(names(1, 0)).toEqual(["Mega Tree"]);
    const moved = screen.getByRole("region", { name: "Mega Tree settings" });
    expect(moved).toHaveTextContent("Porch WLED · Port 1");
    await user.click(within(moved).getByRole("button", { name: /Unwire/ }));
    expect(names(1, 0)).toEqual([]);
  });

  it("fills a port's capacity bar, warning when it's nearly full and over", async () => {
    const show = demoShow();
    show.controllers[0].ports[0].maxPixels = 600; // Arch 50 + Matrix 512 = 562
    const user = await setup(show);
    expect(screen.getByRole("meter", { name: "Port 1 pixels used" })).toHaveAttribute("aria-valuetext", "562 of 600 pixels");
    expect(screen.getByText("Nearly full: 562 of 600 pixels.")).toBeInTheDocument();

    await act(async () => drag(propItem("Porch Star"), at(0, 2)));
    expect(screen.getByRole("alert")).toHaveTextContent("62 pixels more than this port can drive (662 of 600). Move a prop to another port.");
    expect(screen.getByRole("region", { name: "Wiring problems" })).toHaveTextContent("Port 1 on Main FPP has 62 pixels more than it can drive");

    // Raise the limit in the port's settings.
    await user.click(screen.getAllByRole("button", { name: "Port 1 settings" })[0]);
    const limit = screen.getByLabelText("Pixel limit");
    await user.clear(limit);
    await user.type(limit, "1024{Enter}");
    expect(backend.show.controllers[0].ports[0].maxPixels).toBe(1024);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows each port's universes and channels, and each row's", async () => {
    await setup();
    // Arch (50) and Matrix (512) on universe 1 up: 562 pixels × 3 channels.
    expect(screen.getByTestId("channels-1")).toHaveTextContent("Ch 1–1,686");
    expect(screen.getAllByTestId("summary-1")[0]).toHaveTextContent("2 props · 562 px · U1–4");
    const matrix = chip("Window Matrix on Main FPP port 1").closest("tr")!;
    // 2nd on the port: from channel 151 (after the arch's 150), in universes 1 to 4.
    expect([...matrix.querySelectorAll("td")].map((td) => td.textContent)).toEqual(["2", "Window Matrix", "512", "151", "U1–4", "", "—", ""]);
  });

  it("folds a port to a one-line summary, and a drop on it goes to its end", async () => {
    const user = await setup();
    await user.click(screen.getAllByRole("button", { name: "Hide the props on port 1" })[0]);
    expect(screen.getAllByTestId("summary-1")[0]).toHaveTextContent("Garage Arch → Window Matrix · 562 px · U1–4");
    expect(screen.queryByRole("button", { name: "Garage Arch on Main FPP port 1" })).not.toBeInTheDocument();
    await act(async () => drag(propItem("Porch Star"), at(0, 0)));
    expect(names(0, 0)).toEqual(["Garage Arch", "Window Matrix", "Porch Star"]);
    await user.click(screen.getAllByRole("button", { name: "Show the props on port 1" })[0]);
    expect(chip("Porch Star on Main FPP port 1")).toBeInTheDocument();
  });

  it("toggles a row's direction and unwires it from its row", async () => {
    const user = await setup();
    await user.click(screen.getByRole("checkbox", { name: "Window Matrix starts at the other end" }));
    expect(backend.show.controllers[0].ports[0].slots[1].reverse).toBe(true);
    await user.click(screen.getByRole("button", { name: "Settings for Window Matrix" }));
    expect(screen.getByRole("region", { name: "Window Matrix settings" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Unwire Garage Arch" }));
    expect(names(0, 0)).toEqual(["Window Matrix"]);
    expect(edits).toHaveLength(2);
  });

  it("lights the pointed-at row's prop and its port in the preview", async () => {
    await setup();
    fireEvent.pointerEnter(chip("Mega Tree on Main FPP port 2").closest("tr")!);
    expect(useWiring.getState().hoveredProp).toBe(backend.show.props.find((p) => p.name === "Mega Tree")!.id);
    expect(screen.getByTestId("wiring-preview-caption")).toHaveTextContent("Port 2 on Main FPP: Mega Tree.");
    fireEvent.pointerLeave(chip("Mega Tree on Main FPP port 2").closest("tr")!);
    expect(useWiring.getState().hoveredProp).toBeNull();
  });

  it("flags a prop wired twice", async () => {
    const show = demoShow();
    show.controllers[1].ports[0].slots = [{ ...show.controllers[0].ports[0].slots[0] }];
    await setup(show);
    expect(propItem("Garage Arch")).toHaveAccessibleName(/Wired twice: Main FPP · Port 1, Porch WLED · Port 1/);
    expect(screen.getByRole("region", { name: "Wiring problems" })).toHaveTextContent("Garage Arch is wired more than once");
  });

  it("wires every remaining prop onto a port, left to right, after asking", async () => {
    const show = demoShow();
    show.controllers[0].ports[1].slots = [];
    const user = await setup(show);
    await user.click(screen.getByRole("button", { name: "Add a prop to port 4 of Main FPP" }));
    await user.click(screen.getByRole("button", { name: "All 2 unwired props, left to right…" }));
    expect(screen.getByText(/Wire 2 props onto the end of port 4/)).toHaveTextContent("Porch Star, Mega Tree?");
    expect(edits).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Wire them" }));
    expect(names(0, 3)).toEqual(["Porch Star", "Mega Tree"]);
    expect(edits).toHaveLength(1);
  });

  it("adds a prop from one shared picker, built only while it's open", async () => {
    const user = await setup();
    // No list of props in any port row until a picker opens.
    expect(document.querySelectorAll("option")).toHaveLength(0);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    const add = screen.getByRole("button", { name: "Add a prop to port 2 of Main FPP" });
    await user.click(add);
    const picker = screen.getByRole("dialog", { name: "Add a prop to port 2 of Main FPP" });
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    // Unwired first, then props wired on another port, to move here.
    expect(within(picker).getByText("Not wired").nextElementSibling).toHaveTextContent("Porch Star");
    expect(within(picker).getByText("Move here").nextElementSibling).toHaveTextContent(/Garage Arch.*from Main FPP · Port 1/);
    // The prop already on this port isn't offered.
    expect(within(picker).queryByRole("button", { name: /^Mega Tree/ })).not.toBeInTheDocument();
    await user.type(within(picker).getByLabelText("Find a prop to add"), "arch");
    expect(within(picker).getAllByRole("button").map((b) => b.textContent)).toEqual(["Garage Archfrom Main FPP · Port 1"]);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(add).toHaveFocus();

    await user.click(add);
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Porch Star" }));
    expect(names(0, 1)).toEqual(["Mega Tree", "Porch Star"]);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("keeps the settings on their prop when another lands before it, and closes them when it's gone", async () => {
    const user = await setup();
    await user.click(chip("Window Matrix on Main FPP port 1"));
    expect(screen.getByRole("region", { name: "Window Matrix settings" })).toHaveTextContent("Port 1 · 2nd on the port");
    await act(async () => drag(propItem("Porch Star"), at(0, 0)));
    expect(names(0, 0)).toEqual(["Porch Star", "Garage Arch", "Window Matrix"]);
    const settings = screen.getByRole("region", { name: "Window Matrix settings" });
    expect(settings).toHaveTextContent("Port 1 · 3rd on the port");
    // Its change goes to the Window Matrix, not to the prop now in its old place.
    await user.click(within(settings).getByLabelText(/Starts at the other end/));
    expect(backend.show.controllers[0].ports[0].slots.map((s) => s.reverse)).toEqual([false, false, true]);

    // Undo back past the drop: still there (2nd again). Undo its own wiring away: closed.
    await act(() => useApp.getState().undo());
    await act(() => useApp.getState().undo());
    expect(screen.getByRole("region", { name: "Window Matrix settings" })).toHaveTextContent("2nd on the port");
    await act(async () => drag(handle("Window Matrix"), [100, 500]));
    expect(screen.queryByRole("region", { name: "Window Matrix settings" })).not.toBeInTheDocument();
    await act(() => useApp.getState().undo());
    expect(screen.queryByRole("region", { name: /settings$/ })).not.toBeInTheDocument();
  });

  it("acts on the same prop when key presses arrive before the show catches up", async () => {
    const show = demoShow();
    const star = show.props.find((p) => p.name === "Porch Star")!;
    show.controllers[0].ports[0].slots.push({ prop: star.id, segment: null, nullPixels: 0, reverse: false, brightness: null, gamma: null, smartReceiver: null });
    await setup(show);
    // A slow engine: every press below is read from the same (old) screen.
    const applyEdits = backend.applyEdits;
    backend.applyEdits = async (batch) => {
      await new Promise((r) => setTimeout(r, 20));
      return applyEdits(batch);
    };
    const arch = chip("Garage Arch on Main FPP port 1");
    await act(async () => {
      fireEvent.keyDown(arch, { key: "ArrowDown", altKey: true });
      fireEvent.keyDown(arch, { key: "ArrowDown", altKey: true });
      await new Promise((r) => setTimeout(r, 80));
    });
    expect(names(0, 0)).toEqual(["Window Matrix", "Porch Star", "Garage Arch"]);

    const matrix = chip("Window Matrix on Main FPP port 1");
    await act(async () => {
      fireEvent.keyDown(matrix, { key: "Delete" });
      fireEvent.keyDown(matrix, { key: "Delete" });
      await new Promise((r) => setTimeout(r, 80));
    });
    // Held Delete unwires that prop once, not its neighbour as well.
    expect(names(0, 0)).toEqual(["Porch Star", "Garage Arch"]);
  });

  it("opening settings from the keyboard moves focus into them, and closing gives it back", async () => {
    const user = await setup();
    chip("Window Matrix on Main FPP port 1").focus();
    await user.keyboard("{Enter}");
    expect(screen.getByRole("heading", { name: "Window Matrix" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("region", { name: "Window Matrix settings" })).not.toBeInTheDocument();
    await waitFor(() => expect(chip("Window Matrix on Main FPP port 1")).toHaveFocus());
  });

  it("a press let go with Escape doesn't also open settings, and a row removed mid-drag lets go", async () => {
    await setup();
    // A wired prop in the list: a click opens its settings, a drag moves it.
    const arch = propItem("Garage Arch");
    await act(async () => drag(arch, at(4, 0), false));
    await act(async () => fireEvent.keyDown(window, { key: "Escape" }));
    fireEvent.pointerUp(arch, { clientX: at(4, 0)[0], clientY: at(4, 0)[1], pointerId: 1 });
    fireEvent.click(arch);
    expect(screen.queryByRole("region", { name: /settings$/ })).not.toBeInTheDocument();
    expect(edits).toHaveLength(0);

    await act(async () => drag(handle("Window Matrix"), at(4, 0), false));
    expect(useWiring.getState().drag).not.toBeNull();
    // Undo-like: the port loses its chips while the drag is on.
    await act(async () => {
      await useApp.getState().apply((show) => [{ type: "updateController", controller: { ...show.controllers[0], ports: show.controllers[0].ports.map((p, i) => (i === 0 ? { ...p, slots: [] } : p)) } }]);
    });
    expect(useWiring.getState().drag).toBeNull();
  });

  it("counts the smart receivers on a port against its one limit, showing each one's share", async () => {
    const show = demoShow();
    const port = show.controllers[0].ports[0];
    port.maxPixels = 600;
    port.slots[0].smartReceiver = 1; // Garage Arch, 50
    port.slots[1].smartReceiver = 2; // Window Matrix, 512
    port.slots.length = 2;
    await setup(show);
    // One bar for the port: 562 of 600 together (as xLights counts it), split A 50 · B 512.
    expect(screen.getAllByRole("meter", { name: "Port 1 pixels used" })).toHaveLength(1);
    expect(screen.getByRole("meter", { name: "Port 1 pixels used" })).toHaveAttribute("aria-valuetext", "562 of 600 pixels (A 50 · B 512)");
    expect(screen.getByTestId("receivers-1")).toHaveTextContent("A 50 · B 512");
    expect(screen.getByText("Nearly full: 562 of 600 pixels, shared by receivers A, B.")).toBeInTheDocument();

    // Each receiver alone fits a 520 limit, but together they don't: a warning for the port.
    await act(async () => {
      await useApp.getState().apply((s) => [
        { type: "updateController", controller: { ...s.controllers[0], ports: s.controllers[0].ports.map((p, i) => (i === 0 ? { ...p, maxPixels: 520 } : p)) } },
      ]);
    });
    expect(screen.getByRole("alert")).toHaveTextContent("42 pixels more than this port can drive (562 of 520, shared by receivers A, B).");
  });

  it("warns when a Falcon port holds more than it refreshes at the show's frame rate", async () => {
    const show = demoShow();
    const falcon = show.controllers[0];
    falcon.adapter = "falcon";
    falcon.ports[0].maxPixels = 1024; // Arch 50 + Matrix 512 + Star 100 = 662 is fine at 40 fps…
    await setup(show);
    expect(screen.queryByText(/refreshes about/)).not.toBeInTheDocument();
    // …but not at 60 fps (about 469).
    await act(() => useApp.getState().apply([{ type: "setFrameRate", fps: 60 }]));
    expect(screen.getByText("At 60 fps this port refreshes about 469 pixels in time; with 562 it will slow down. Move a prop to another port, or lower the show's frame rate.")).toBeInTheDocument();
  });

  it("adds, renumbers, and removes ports", async () => {
    const user = await setup();
    await user.click(screen.getByRole("button", { name: "Add a port to Porch WLED" }));
    expect(backend.show.controllers[1].ports.map((p) => p.number)).toEqual([1, 2]);
    const portButtons = () => within(screen.getByRole("region", { name: "Porch WLED" })).getAllByRole("button", { name: /^Port \d+ settings$/ });
    await user.click(portButtons()[1]);
    const number = screen.getByLabelText("Port number");
    await user.clear(number);
    await user.type(number, "1{Enter}");
    expect(screen.getByText("Another port is already number 1.")).toBeInTheDocument();
    await user.clear(number);
    await user.type(number, "5{Enter}");
    expect(backend.show.controllers[1].ports.map((p) => p.number)).toEqual([1, 5]);
    await user.click(portButtons()[1]);
    await user.click(screen.getByRole("button", { name: "Remove port" }));
    expect(backend.show.controllers[1].ports.map((p) => p.number)).toEqual([1]);
  });

  it("adds a known controller with its ports and pixel limit", async () => {
    const user = await setup();
    await user.click(screen.getByRole("button", { name: /add controller/i }));
    await user.selectOptions(screen.getByLabelText("Controller type"), "Falcon F16V5");
    expect(screen.getByLabelText("Ports")).toHaveValue(16);
    expect(screen.getByRole("button", { name: "Add" })).toBeDisabled();
    expect(screen.getByText("Enter the controller's IP address, like 192.168.1.50.")).toBeInTheDocument();
    await user.type(screen.getByPlaceholderText("e.g. 192.168.1.50"), "10.0.0.20");
    await user.click(screen.getByRole("button", { name: "Add" }));
    const falcon = backend.show.controllers[2];
    expect(falcon.adapter).toBe("falcon");
    expect(falcon.ports).toHaveLength(16);
    expect(falcon.ports.every((p) => p.maxPixels === 1024)).toBe(true);
    expect(screen.getAllByRole("meter")).toHaveLength(16);
  });

  it("edits a controller in place as one undo step, keeping its wiring and what was found on the network", async () => {
    const user = await setup();
    const before = structuredClone(backend.show.controllers[0]);
    await user.click(screen.getByRole("button", { name: "Edit Main FPP" }));
    const form = screen.getByRole("form", { name: "Edit Main FPP" });
    const name = within(form).getByLabelText("Name");
    await user.clear(name);
    await user.type(name, "House FPP");
    const address = within(form).getByLabelText("IP address");
    await user.clear(address);
    await user.type(address, "192.168.1.51");
    await user.type(within(form).getByLabelText("Start universe"), "20");
    await user.click(within(form).getByRole("button", { name: "Save" }));
    expect(edits).toHaveLength(1);
    const c = backend.show.controllers[0];
    expect(c).toMatchObject({ id: before.id, name: "House FPP", address: "192.168.1.51", ports: before.ports, sequenceChannels: before.sequenceChannels });
    expect(c.protocol).toMatchObject({ type: "sacn", startUniverse: 20 });
    expect(screen.queryByRole("form", { name: "Edit Main FPP" })).not.toBeInTheDocument();
    expect(chip("Garage Arch on House FPP port 1")).toBeInTheDocument();
    await act(() => useApp.getState().undo());
    expect(backend.show.controllers[0]).toEqual(before);
  });

  it("explains what's wrong before saving a controller, and Cancel leaves it as it was", async () => {
    const user = await setup();
    await user.click(screen.getByRole("button", { name: "Edit Porch WLED" }));
    const form = screen.getByRole("form", { name: "Edit Porch WLED" });
    const address = within(form).getByLabelText("IP address");
    await user.clear(address);
    await user.type(address, "192.168.1.600");
    expect(within(form).getByText("192.168.1.600 isn't a valid IP address: each of the four numbers must be 0 to 255.")).toBeInTheDocument();
    expect(within(form).getByRole("button", { name: "Save" })).toBeDisabled();
    // Save says why it's off.
    expect(within(form).getByRole("status")).toHaveTextContent("To save, fix the address above.");
    await user.clear(address);
    await user.type(address, "192.168.1.50");
    // Sharing an address is allowed (one controller can be split in two), with a word about it.
    expect(within(form).getByText("Main FPP also uses 192.168.1.50. That's fine if it's the same controller.")).toBeInTheDocument();
    expect(within(form).getByRole("button", { name: "Save" })).toBeEnabled();
    await user.click(within(form).getByRole("button", { name: "Cancel" }));
    expect(edits).toHaveLength(0);
    expect(backend.show.controllers[1].address).toBe("192.168.1.60");
  });

  it("edits controllers an xLights import made: split ones sharing an address, multicast ones with none, and host:port", async () => {
    const show = demoShow();
    const split = (name: string) => newController(name, "10.0.0.5", "sacn", 1);
    const yard = newController("Yard", "", "sacn", 1);
    yard.protocol = { type: "sacn", startUniverse: 70, universeSize: 510, allowPixelStraddle: false, multicast: true };
    show.controllers = [split("Front (universes 1–4)"), split("Front (universes 10–12)"), yard, newController("Bench", "127.0.0.2:4048", "ddp", 1)];
    const user = await setup(show);
    for (const [name, change] of [
      ["Front (universes 10–12)", "Protocol"],
      ["Yard", "Name"],
      ["Bench", "Name"],
    ] as const) {
      await user.click(screen.getByRole("button", { name: `Edit ${name}` }));
      const form = screen.getByRole("form", { name: `Edit ${name}` });
      if (change === "Protocol") await user.selectOptions(within(form).getByLabelText("Protocol"), "ddp");
      else await user.type(within(form).getByLabelText("Name"), " 2");
      expect(within(form).queryByRole("status")).not.toBeInTheDocument();
      await user.click(within(form).getByRole("button", { name: "Save" }));
      expect(screen.queryByRole("form", { name: `Edit ${name}` })).not.toBeInTheDocument();
    }
    expect(edits).toHaveLength(3);
    expect(backend.show.controllers.map((c) => c.name)).toEqual(["Front (universes 1–4)", "Front (universes 10–12)", "Yard 2", "Bench 2"]);
    expect(backend.show.controllers[1].protocol).toEqual({ type: "ddp" });
    expect(backend.show.controllers.map((c) => c.address)).toEqual(["10.0.0.5", "10.0.0.5", "", "127.0.0.2:4048"]);
  });

  it("adds a controller at host:port", async () => {
    const user = await setup();
    await user.click(screen.getByRole("button", { name: /add controller/i }));
    await user.type(screen.getByPlaceholderText("e.g. 192.168.1.50"), "10.0.0.20:4048");
    await user.click(screen.getByRole("button", { name: "Add" }));
    expect(backend.show.controllers[2].address).toBe("10.0.0.20:4048");
  });

  it("keeps the rename field open while the name is empty", async () => {
    const user = await setup();
    await user.dblClick(screen.getByRole("button", { name: "Porch WLED" }));
    const rename = screen.getByRole("textbox", { name: "Name of Porch WLED" });
    await user.clear(rename);
    await user.keyboard("{Enter}");
    expect(screen.getByRole("textbox", { name: "Name of Porch WLED" })).toBeInTheDocument();
    expect(screen.getByText("Give the controller a name.")).toBeInTheDocument();
    expect(edits).toHaveLength(0);
  });

  it("switches a controller between DDP and sACN, and renames it with a double-click", async () => {
    const user = await setup();
    await user.click(screen.getByRole("button", { name: "Edit Porch WLED" }));
    const form = screen.getByRole("form", { name: "Edit Porch WLED" });
    expect(within(form).queryByLabelText("Start universe")).not.toBeInTheDocument();
    await user.selectOptions(within(form).getByLabelText("Protocol"), "sacn");
    await user.click(within(form).getByRole("button", { name: /^More: universe size/ }));
    await user.selectOptions(within(form).getByLabelText("Channels per universe"), "512");
    await user.click(within(form).getByRole("button", { name: "Save" }));
    expect(backend.show.controllers[1].protocol).toEqual({ type: "sacn", startUniverse: null, universeSize: 512, allowPixelStraddle: false, multicast: false });

    await user.dblClick(screen.getByRole("button", { name: "Porch WLED" }));
    const rename = screen.getByRole("textbox", { name: "Name of Porch WLED" });
    await user.clear(rename);
    await user.type(rename, "Porch{Enter}");
    expect(backend.show.controllers[1].name).toBe("Porch");
    expect(screen.getByRole("region", { name: "Porch" })).toBeInTheDocument();
  });

  it("deleting a controller says what it unwired, and its Undo brings it all back", async () => {
    const user = await setup();
    const before = structuredClone(backend.show.controllers);
    await user.click(screen.getByRole("button", { name: "Delete Main FPP" }));
    expect(backend.show.controllers.map((c) => c.name)).toEqual(["Porch WLED"]);
    const toast = useToasts.getState().toasts.at(-1)!;
    expect(toast.text).toBe("Deleted Main FPP and unwired 3 props");
    await act(async () => void (await toast.action!.run()));
    expect(backend.show.controllers).toEqual(before);
  });

  it("empty, it points to where props and controllers come from with buttons", async () => {
    const user = await setup({ ...demoShow(), props: [], controllers: [] });
    await user.click(screen.getByRole("button", { name: "Add a controller" }));
    expect(screen.getByPlaceholderText("e.g. 192.168.1.50")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await user.click(screen.getByRole("button", { name: /find the controllers on your network/ }));
    expect(useApp.getState().screen).toBe("devices");
    await user.click(screen.getByRole("button", { name: /Add props on Layout/ }));
    expect(useApp.getState().screen).toBe("layout");
  });

  it("folds a controller away", async () => {
    const user = await setup();
    await user.click(screen.getByRole("button", { name: "Main FPP" }));
    expect(screen.queryByRole("button", { name: "Garage Arch on Main FPP port 1" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Main FPP" }));
    expect(chip("Garage Arch on Main FPP port 1")).toBeInTheDocument();
  });

  describe("wiring on the layout", () => {
    /** The demo show without its photo, so the canvas shows just the props. */
    const plain = () => ({ ...demoShow(), background: null });

    /** Clicks the prop's middle pixel on the click-to-wire canvas. */
    async function clickOn(name: string) {
      const preview = (await backend.previewProps()).props;
      const id = backend.show.props.find((p) => p.name === name)!.id;
      const points = preview.find((p) => p.prop === id)!.points;
      const mid = Math.floor(points.length / 4) * 2;
      const s = toScreen(wiringView(wiringBox(preview, null, 0), CANVAS), CANVAS, { x: points[mid], y: points[mid + 1] });
      const canvas = document.querySelector("[data-wire-canvas]")!;
      await act(async () => fireEvent.click(canvas, { clientX: s.x, clientY: s.y }));
    }
    const chain = () => within(screen.getByRole("list", { name: /in wiring order/ })).getAllByRole("listitem").map((li) => li.textContent);
    const wire = (port: number, controller: string) => screen.getByRole("button", { name: `Wire port ${port} of ${controller} on the layout` });

    it("adds props in the order they're clicked, as one undo step when done", async () => {
      const user = await setup(plain());
      await act(async () => {}); // the props' positions arrive
      await user.click(wire(1, "Porch WLED"));
      expect(screen.getByRole("heading", { name: "Wiring Porch WLED · Port 1" })).toHaveFocus();
      await clickOn("Porch Star");
      await clickOn("Garage Arch");
      // Wired on another port: it asks first.
      expect(screen.getByRole("group", { name: "What to do with this prop" })).toHaveTextContent("Garage Arch is on Main FPP · Port 1. Move it here?");
      await user.click(screen.getByRole("button", { name: "Move it here" }));
      await clickOn("Mega Tree");
      await user.click(screen.getByRole("button", { name: "Move it here" }));
      expect(chain()).toEqual(["1Porch Star", "2Garage Arch", "3Mega Tree"]);
      // Nothing changes until Done.
      expect(edits).toHaveLength(0);
      // The running count: Star 100 + Arch 50 + Tree 800.
      expect(screen.getByRole("complementary", { name: "Wiring this port" })).toHaveTextContent("950 px");

      await user.click(screen.getByRole("button", { name: "Done" }));
      expect(names(1, 0)).toEqual(["Porch Star", "Garage Arch", "Mega Tree"]);
      expect(names(0, 0)).toEqual(["Window Matrix"]);
      expect(names(0, 1)).toEqual([]);
      expect(edits).toHaveLength(1);
      await waitFor(() => expect(wire(1, "Porch WLED")).toHaveFocus());
      await act(() => useApp.getState().undo());
      expect(names(1, 0)).toEqual([]);
      expect(names(0, 0)).toEqual(["Garage Arch", "Window Matrix"]);
    });

    it("clicking a prop already on the port removes it or moves it to the end", async () => {
      const user = await setup(plain());
      await act(async () => {});
      await user.click(wire(1, "Main FPP"));
      await clickOn("Garage Arch");
      expect(screen.getByRole("group", { name: "What to do with this prop" })).toHaveTextContent("Garage Arch is already on this port.");
      await user.click(screen.getByRole("button", { name: "Move to end" }));
      expect(chain()).toEqual(["1Window Matrix", "2Garage Arch"]);
      await clickOn("Window Matrix");
      await user.click(screen.getByRole("button", { name: "Remove" }));
      expect(chain()).toEqual(["1Garage Arch"]);
      await user.click(screen.getByRole("button", { name: "Done" }));
      expect(names(0, 0)).toEqual(["Garage Arch"]);
      expect(edits).toHaveLength(1);
    });

    it("Escape closes the question, then asks before throwing changes away", async () => {
      const user = await setup(plain());
      await act(async () => {});
      await user.click(wire(1, "Porch WLED"));
      await clickOn("Porch Star");
      await clickOn("Mega Tree");
      await user.keyboard("{Escape}");
      expect(screen.queryByRole("group", { name: "What to do with this prop" })).not.toBeInTheDocument();
      expect(chain()).toEqual(["1Porch Star"]);

      // With a change made, Escape asks, starting on Keep.
      await user.keyboard("{Escape}");
      const ask = screen.getByRole("alertdialog", { name: "Keep the 1 change to Port 1?" });
      expect(within(ask).getByRole("button", { name: "Keep" })).toHaveFocus();
      await user.click(within(ask).getByRole("button", { name: "Back to wiring" }));
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
      expect(chain()).toEqual(["1Porch Star"]);

      await user.keyboard("{Escape}");
      await user.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Discard" }));
      expect(screen.queryByRole("heading", { name: /^Wiring Porch WLED/ })).not.toBeInTheDocument();
      expect(edits).toHaveLength(0);
      expect(names(1, 0)).toEqual([]);
      await waitFor(() => expect(wire(1, "Porch WLED")).toHaveFocus());

      // Keep is Done: one undo step.
      await user.click(wire(1, "Porch WLED"));
      await clickOn("Porch Star");
      await user.keyboard("{Escape}");
      await user.keyboard("{Enter}");
      expect(names(1, 0)).toEqual(["Porch Star"]);
      expect(edits).toHaveLength(1);

      // With no changes, Escape just leaves.
      await user.click(wire(1, "Main FPP"));
      await user.keyboard("{Escape}");
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
      expect(screen.queryByRole("heading", { name: /^Wiring Main FPP/ })).not.toBeInTheDocument();
      expect(edits).toHaveLength(1);
    });

    it("works from the keyboard: pick props from a list in order", async () => {
      const user = await setup(plain());
      wire(1, "Porch WLED").focus();
      await user.keyboard("{Enter}");
      const next = screen.getByRole("list", { name: "Props to add, in the order you pick them" });
      // Unwired first.
      expect(within(next).getAllByRole("button")[0]).toHaveTextContent("Porch Star");
      await user.type(screen.getByLabelText("Find a prop to wire next"), "star");
      within(next).getByRole("button", { name: /Porch Star/ }).focus();
      await user.keyboard("{Enter}");
      await user.clear(screen.getByLabelText("Find a prop to wire next"));
      within(next).getByRole("button", { name: /Mega Tree/ }).focus();
      await user.keyboard("{Enter}");
      screen.getByRole("button", { name: "Move it here" }).focus();
      await user.keyboard("{Enter}");
      expect(chain()).toEqual(["1Porch Star", "2Mega Tree"]);
      screen.getByRole("button", { name: "Move Porch Star to the end" }).focus();
      await user.keyboard("{Enter}");
      expect(chain()).toEqual(["1Mega Tree", "2Porch Star"]);
      screen.getByRole("button", { name: "Done" }).focus();
      await user.keyboard("{Enter}");
      expect(names(1, 0)).toEqual(["Mega Tree", "Porch Star"]);
      expect(edits).toHaveLength(1);
    });

    it("wires onto the smart receiver picked", async () => {
      const show = plain();
      show.controllers[0].ports[0].slots[0].smartReceiver = 1;
      show.controllers[0].ports[0].slots[1].smartReceiver = 2;
      const user = await setup(show);
      await user.click(wire(1, "Main FPP"));
      expect(screen.getByRole("radio", { name: "Receiver B" })).toBeChecked();
      await user.click(screen.getByRole("radio", { name: "Receiver A" }));
      await user.click(within(screen.getByRole("list", { name: "Props to add, in the order you pick them" })).getByRole("button", { name: /Porch Star/ }));
      await user.click(screen.getByRole("button", { name: "Done" }));
      const slots = backend.show.controllers[0].ports[0].slots;
      expect(names(0, 0)).toEqual(["Garage Arch", "Porch Star", "Window Matrix"]);
      expect(slots.map((s) => s.smartReceiver)).toEqual([1, 1, 2]);
    });
  });

  it("describes the hovered port's wiring under the preview", async () => {
    await setup();
    expect(screen.getByTestId("wiring-preview-caption")).toHaveTextContent("Point at a port to see its wiring here.");
    fireEvent.pointerEnter(screen.getAllByRole("button", { name: "Port 1 settings" })[0].closest("li")!);
    expect(screen.getByTestId("wiring-preview-caption")).toHaveTextContent("Port 1 on Main FPP: Garage Arch → Window Matrix.");
  });
});
