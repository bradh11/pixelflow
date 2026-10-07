import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryBackend, emptyShow } from "../api/memory";
import type { Prop, Show } from "../api/types";
import { type Pt, toScreen } from "../lib/layoutMath";
import { newProp } from "../lib/shows";
import { LayoutScreen } from "../screens/LayoutScreen";
import { useContextMenu } from "../state/contextMenu";
import { useLayoutEditor } from "../state/layoutEditor";
import { effectMenuItems } from "../state/sequenceActions";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";
import { useWiring } from "../state/wiring";
import { ContextMenuLayer } from "./ContextMenu";
import { demoShow } from "../api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "../api/demoSequence";
import { MemorySequencer } from "../api/memorySequencer";
import { WiringScreen } from "../screens/WiringScreen";

vi.mock("../components/layout/useLayoutData", async (original) => ({
  ...(await original<typeof import("../components/layout/useLayoutData")>()),
  imageAspect: async () => 0.5,
}));

const SIZE = { width: 800, height: 600 };

function line(name: string, x: number, y: number): Prop {
  const prop = { ...newProp("line", emptyShow("x")), name };
  prop.transform.position = { x, y, z: 0 };
  return prop;
}

const showWith = (...props: Prop[]): Show => ({ ...emptyShow("Test House"), props });

let backend: MemoryBackend;

async function setup(show: Show) {
  backend = new MemoryBackend(show);
  await useApp.getState().connect(backend);
  useApp.setState({ started: true });
  const user = userEvent.setup();
  render(
    <>
      <LayoutScreen />
      <ContextMenuLayer />
    </>,
  );
  await waitFor(() => expect(useLayoutEditor.getState().view).not.toBeNull());
  return user;
}

const canvas = () => screen.getByRole("application", { name: "Layout canvas" });
const menu = () => screen.queryByRole("menu");
const names = () => within(menu()!).getAllByRole("menuitem").map((i) => i.textContent);

function rightClick(world: Pt) {
  const s = toScreen(useLayoutEditor.getState().view!, SIZE, world);
  act(() => {
    fireEvent.contextMenu(canvas(), { clientX: s.x, clientY: s.y, button: 2 });
  });
}

const descriptors = {
  clientWidth: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth"),
  clientHeight: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight"),
};
beforeEach(() => {
  Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => SIZE.width });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => SIZE.height });
});
afterEach(() => {
  for (const [key, d] of Object.entries(descriptors)) if (d) Object.defineProperty(HTMLElement.prototype, key, d);
});

describe("the layout's right-click menu", () => {
  it("picks the prop under the pointer and offers what can be done to it, with its shortcuts", async () => {
    await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)));
    const gutter = backend.show.props[0].id;
    rightClick({ x: 0, y: 0 });
    expect(useLayoutEditor.getState().selected).toEqual([gutter]);
    expect(menu()).toHaveAccessibleName("Gutter");
    expect(names()).toEqual(["Cut⌘X", "Copy⌘C", "Paste⌘V", "Duplicate⌘D", "Delete⌫", "Group⌘G", "Wire…", "Rename", "Bring to view"]);
    expect(within(menu()!).getByRole("menuitem", { name: /Paste/ })).toBeDisabled();
    // The keyboard is in the menu.
    expect(within(menu()!).getByRole("menuitem", { name: /Cut/ })).toHaveFocus();
  });

  it("duplicates through the same edit as ⌘D (one undo step)", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    rightClick({ x: 0, y: 0 });
    await user.click(within(menu()!).getByRole("menuitem", { name: /Duplicate/ }));
    await waitFor(() => expect(backend.show.props).toHaveLength(2));
    expect(menu()).not.toBeInTheDocument();
    await act(async () => void (await useApp.getState().undo()));
    expect(backend.show.props).toHaveLength(1);
  });

  it("copies, then pastes from the menu on empty space", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    rightClick({ x: 0, y: 0 });
    await user.click(within(menu()!).getByRole("menuitem", { name: /Copy/ }));
    rightClick({ x: 30, y: 30 });
    expect(names()).toEqual(["Paste⌘V", "Select all⌘A"]);
    await user.click(within(menu()!).getByRole("menuitem", { name: /Paste/ }));
    await waitFor(() => expect(backend.show.props).toHaveLength(2));
  });

  it("acts on the whole selection when the prop clicked is in it", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)));
    const ids = backend.show.props.map((p) => p.id);
    act(() => useLayoutEditor.getState().select(ids));
    rightClick({ x: 0, y: 4 });
    expect(menu()).toHaveAccessibleName("2 props");
    expect(within(menu()!).getByRole("menuitem", { name: "Rename" })).toBeDisabled();
    await user.click(within(menu()!).getByRole("menuitem", { name: /Delete/ }));
    await waitFor(() => expect(backend.show.props).toHaveLength(0));
  });

  it("opens from the keyboard with Shift+F10, moves with the arrows, and Escape gives the focus back", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
    canvas().focus();
    await user.keyboard("{Shift>}{F10}{/Shift}");
    expect(menu()).toBeInTheDocument();
    await user.keyboard("{ArrowDown}");
    expect(within(menu()!).getByRole("menuitem", { name: /Copy/ })).toHaveFocus();
    await user.keyboard("{End}");
    expect(within(menu()!).getByRole("menuitem", { name: "Bring to view" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(menu()).not.toBeInTheDocument();
    expect(canvas()).toHaveFocus();
    // Escape closed the menu only: the selection stays.
    expect(useLayoutEditor.getState().selected).toHaveLength(1);
  });

  it("also opens with the keyboard's menu key, and Delete in the menu doesn't delete the props", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
    canvas().focus();
    await user.keyboard("{ContextMenu}");
    expect(menu()).toBeInTheDocument();
    await user.keyboard("{Delete}");
    expect(backend.show.props).toHaveLength(1);
  });

  it("doesn't open after a right-button drag (that pans the view)", async () => {
    await setup(showWith(line("Gutter", 0, 0)));
    const c = canvas();
    act(() => {
      fireEvent.pointerDown(c, { clientX: 400, clientY: 300, button: 2, pointerId: 1 });
      fireEvent.contextMenu(c, { clientX: 400, clientY: 300, button: 2 });
      fireEvent.pointerMove(c, { clientX: 460, clientY: 300, pointerId: 1 });
      fireEvent.pointerUp(c, { clientX: 460, clientY: 300, button: 2, pointerId: 1 });
    });
    expect(menu()).not.toBeInTheDocument();
    // A press without a move opens it on release.
    act(() => {
      fireEvent.pointerDown(c, { clientX: 400, clientY: 300, button: 2, pointerId: 1 });
      fireEvent.contextMenu(c, { clientX: 400, clientY: 300, button: 2 });
      fireEvent.pointerUp(c, { clientX: 400, clientY: 300, button: 2, pointerId: 1 });
    });
    expect(menu()).toBeInTheDocument();
  });

  it("renames in the props list, and Wire… opens Wiring narrowed to the prop", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0)));
    rightClick({ x: 0, y: 0 });
    await user.click(within(menu()!).getByRole("menuitem", { name: "Rename" }));
    const field = await screen.findByRole("textbox", { name: "Name of Gutter" });
    expect(field).toHaveFocus();
    await user.clear(field);
    await user.type(field, "Eaves{Enter}");
    await waitFor(() => expect(backend.show.props[0].name).toBe("Eaves"));
    rightClick({ x: 0, y: 0 });
    await user.click(within(menu()!).getByRole("menuitem", { name: "Wire…" }));
    expect(useApp.getState().screen).toBe("wiring");
    expect(useWiring.getState().query).toBe("Eaves");
  });

  it("brings the props into view", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0), line("Fence", 40, 0)));
    act(() => useLayoutEditor.getState().setView({ cx: 0, cy: 0, zoom: 20 }));
    rightClick({ x: 0, y: 0 });
    act(() => useLayoutEditor.getState().select([backend.show.props[1].id]));
    useContextMenu.getState().close();
    const row = screen.getByRole("option", { name: /Fence/ });
    fireEvent.contextMenu(row, { clientX: 10, clientY: 10 });
    await user.click(within(menu()!).getByRole("menuitem", { name: "Bring to view" }));
    await waitFor(() => expect(useLayoutEditor.getState().view!.cx).toBeCloseTo(40, 0));
  });
});

describe("the props list's right-click menu", () => {
  it("opens on a row, picking it, and from the keyboard on the active row", async () => {
    const user = await setup(showWith(line("Gutter", 0, 0), line("Fence", 0, 4)));
    const fence = backend.show.props[1].id;
    fireEvent.contextMenu(screen.getByRole("option", { name: /Fence/ }), { clientX: 10, clientY: 10 });
    expect(useLayoutEditor.getState().selected).toEqual([fence]);
    expect(menu()).toHaveAccessibleName("Fence");
    await user.keyboard("{Escape}");
    screen.getByRole("listbox", { name: "Props" }).focus();
    await user.keyboard("{Shift>}{F10}{/Shift}");
    expect(menu()).toBeInTheDocument();
    await user.click(within(menu()!).getByRole("menuitem", { name: /Copy/ }));
    expect(useLayoutEditor.getState().clipboard?.props).toHaveLength(1);
  });
});

describe("the timeline's right-click menu", () => {
  it("offers Copy, Paste, Duplicate, Delete and Edit settings, through the same edits as the keys", () => {
    const items = effectMenuItems(["e1"]);
    expect(items.map((i) => i.label)).toEqual(["Copy", "Paste", "Duplicate", "Delete", "Edit settings"]);
    expect(items.map((i) => i.shortcut)).toEqual(["seq-copy", "seq-paste", "seq-duplicate", "seq-delete", undefined]);
    // Nothing copied yet: Paste waits.
    expect(items[1].disabled).toBe(true);
    useSequencer.setState({ clipboard: [{ rowId: "r", effect: {} as never }] });
    expect(effectMenuItems([])[0]).toMatchObject({ label: "Paste", disabled: false });
  });

  it("duplicates and deletes as one undo step each", async () => {
    const show = demoShow();
    const engine = new MemoryBackend(show);
    const seq = new MemorySequencer(engine);
    seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000));
    await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
    await useApp.getState().connect(engine);
    await useSequencer.getState().connect(seq);
    const count = () => useSequencer.getState().doc!.rows.flatMap((r) => r.layers.flatMap((l) => l.effects)).length;
    const first = useSequencer.getState().doc!.rows.flatMap((r) => r.layers.flatMap((l) => l.effects))[0].id;
    const before = count();
    effectMenuItems([first]).find((i) => i.label === "Duplicate")!.run();
    await waitFor(() => expect(count()).toBe(before + 1));
    await act(async () => void (await useSequencer.getState().undo()));
    expect(count()).toBe(before);
    effectMenuItems([first]).find((i) => i.label === "Delete")!.run();
    await waitFor(() => expect(count()).toBe(before - 1));
  });
});

describe("the wiring table's right-click menu", () => {
  it("opens on a row or with Shift+F10 on its name, and unwires through the same edit as the Unwire button", async () => {
    backend = new MemoryBackend(demoShow());
    await useApp.getState().connect(backend);
    useApp.setState({ started: true, screen: "wiring" });
    const user = userEvent.setup();
    render(
      <>
        <WiringScreen />
        <ContextMenuLayer />
      </>,
    );
    const chip = (await screen.findAllByRole("button", { name: / on .* port \d+$/ }))[0];
    fireEvent.contextMenu(chip.closest("tr")!, { clientX: 20, clientY: 20 });
    expect(names()).toEqual(["Settings", "Move up", "Move down", "Start at the other end", "Unwire"]);
    expect(within(menu()!).getByRole("menuitem", { name: "Move up" })).toBeDisabled();
    await user.keyboard("{Escape}");
    expect(chip).toHaveFocus();
    await user.keyboard("{Shift>}{F10}{/Shift}");
    expect(menu()).toBeInTheDocument();
    const wired = () => backend.show.controllers.flatMap((c) => c.ports.flatMap((p) => p.slots)).length;
    const before = wired();
    await user.click(within(menu()!).getByRole("menuitem", { name: "Unwire" }));
    await waitFor(() => expect(wired()).toBe(before - 1));
  });
});
