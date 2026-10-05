import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { demoShow } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import type { Edit, Show } from "../api/types";
import { useApp } from "../state/store";
import { useWiring } from "../state/wiring";
import { WiringScreen } from "./WiringScreen";

// Where things sit on screen (jsdom doesn't lay anything out): the props list on the left; each
// port row 50 px tall from y = 100, in page order; chips 100 px apart from x = 400.
const ROW_TOP = 100;
const ROW_H = 50;
const CHIP_X = 400;
const CHIP_STEP = 100;

function rect(left: number, top: number, width: number, height: number): DOMRect {
  return { left, top, width, height, right: left + width, bottom: top + height, x: left, y: top, toJSON: () => ({}) } as DOMRect;
}

beforeEach(() => {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    const el = this as HTMLElement;
    if (el.dataset.wiringDrop === "props") return rect(0, 0, 250, 2000);
    const rows = [...document.querySelectorAll("[data-wiring-drop='port']")];
    if (el.dataset.wiringDrop === "port") return rect(300, ROW_TOP + rows.indexOf(el) * ROW_H, 800, ROW_H - 10);
    if (el.dataset.wiringChip !== undefined) {
      const row = rows.indexOf(el.closest("[data-wiring-drop='port']")!);
      return rect(CHIP_X + Number(el.dataset.index) * CHIP_STEP, ROW_TOP + row * ROW_H + 10, 80, 20);
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

/** The point to drop at on row `row` (page order), before chip `index` (or past the last one). */
const at = (row: number, index: number): [number, number] => [CHIP_X + index * CHIP_STEP + 10, ROW_TOP + row * ROW_H + 20];

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

const chip = (name: string) => screen.getByRole("button", { name });
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
    await act(async () => fireEvent.keyDown(window, { key: "Escape" }));
    expect(useWiring.getState().drag).toBeNull();
    fireEvent.pointerUp(propItem("Porch Star"), { clientX: at(4, 0)[0], clientY: at(4, 0)[1], pointerId: 1 });
    expect(edits).toHaveLength(0);
  });

  it("reorders a chip within its port, moves it to another controller, and unwires it on the list", async () => {
    await setup();
    await act(async () => drag(chip("Garage Arch on Main FPP port 1"), at(0, 2)));
    expect(names(0, 0)).toEqual(["Window Matrix", "Garage Arch"]);

    await act(async () => drag(chip("Garage Arch on Main FPP port 1"), at(4, 0)));
    expect(names(0, 0)).toEqual(["Window Matrix"]);
    expect(names(1, 0)).toEqual(["Garage Arch"]);
    // One step, though two controllers changed.
    expect(edits.at(-1)).toHaveLength(2);

    await act(async () => drag(chip("Garage Arch on Porch WLED port 1"), [100, 500]));
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

  it("moves and unwires chips from the keyboard", async () => {
    const user = await setup();
    chip("Garage Arch on Main FPP port 1").focus();
    await user.keyboard("{ArrowRight}");
    expect(chip("Window Matrix on Main FPP port 1")).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(chip("Mega Tree on Main FPP port 2")).toHaveFocus();
    await user.keyboard("{ArrowUp}");
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

  it("shows each port's universes and channels", async () => {
    await setup();
    // Arch (50) and Matrix (512) on universe 1 up: 562 pixels × 3 channels.
    expect(screen.getByTestId("channels-1")).toHaveTextContent("Universe 1–4 · Ch 1–1,686");
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
    await user.selectOptions(screen.getByLabelText("Add a prop to port 4 of Main FPP"), "All 2 unwired props, left to right…");
    expect(screen.getByText(/Wire 2 props onto the end of port 4/)).toHaveTextContent("Porch Star, Mega Tree?");
    expect(edits).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Wire them" }));
    expect(names(0, 3)).toEqual(["Porch Star", "Mega Tree"]);
    expect(edits).toHaveLength(1);
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
    await user.type(screen.getByPlaceholderText("192.168.1.50"), "10.0.0.20");
    await user.click(screen.getByRole("button", { name: "Add" }));
    const falcon = backend.show.controllers[2];
    expect(falcon.adapter).toBe("falcon");
    expect(falcon.ports).toHaveLength(16);
    expect(falcon.ports.every((p) => p.maxPixels === 1024)).toBe(true);
    expect(screen.getAllByRole("meter")).toHaveLength(16);
  });

  it("folds a controller away", async () => {
    const user = await setup();
    await user.click(screen.getByRole("button", { name: "Main FPP" }));
    expect(screen.queryByRole("button", { name: "Garage Arch on Main FPP port 1" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Main FPP" }));
    expect(chip("Garage Arch on Main FPP port 1")).toBeInTheDocument();
  });

  it("describes the hovered port's wiring under the preview", async () => {
    await setup();
    expect(screen.getByTestId("wiring-preview-caption")).toHaveTextContent("Point at a port to see its wiring here.");
    fireEvent.pointerEnter(screen.getAllByRole("button", { name: "Port 1 settings" })[0].closest("li")!);
    expect(screen.getByTestId("wiring-preview-caption")).toHaveTextContent("Port 1 on Main FPP: Garage Arch → Window Matrix.");
  });
});
