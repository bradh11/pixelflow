// Every control the app shows has a name, and every button showing only an icon has a tooltip.
// The app is opened on the demo show and walked through each screen (and the panels and menus
// that open on them), checking whatever is on screen at each stop.

import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { DEMO_MUSIC, DEMO_SEQUENCE_PATH, demoSequence } from "./api/demoSequence";
import { demoDevices, demoFppFiles, demoFppSchedules, demoPlayers, demoShow } from "./api/demo";
import { MemoryBackend } from "./api/memory";
import { FakeAssistant } from "./api/memoryAssistant";
import { MemorySequencer } from "./api/memorySequencer";
import { useAssistant } from "./state/assistant";
import { useLayoutEditor } from "./state/layoutEditor";
import { useSequencer } from "./state/sequencer";
import { type Screen, useApp } from "./state/store";
import { useCloseGuard } from "./state/closeGuard";
import { toast } from "./state/toast";
import { useView3d } from "./state/view3d";
import type { Scene3d } from "./components/layout3d/scene";

// The 3D view draws with WebGL, which jsdom hasn't: a stand-in scene.
vi.mock("./components/layout3d/threeScene", () => ({
  measureModel: async () => ({ min: { x: 0, y: 0, z: 0 }, max: { x: 1, y: 1, z: 1 } }),
  createThreeScene: (): Scene3d =>
    ({
      resize() {},
      setPixels() {},
      updatePixels() {},
      setColors() {},
      setBulbSize() {},
      setBackdrop() {},
      setModel: async () => null,
      placeModel: () => null,
      surfaceAt: () => null,
      setSelectionBox() {},
      setGizmo() {},
      setOptions() {},
      render() {},
      dispose() {},
    }) as Scene3d,
}));

const ROLES = ["button", "link", "tab", "menuitem", "menuitemradio", "checkbox", "radio", "switch", "textbox", "combobox", "slider", "spinbutton", "option"] as const;

/**
 * The text a sighted user can count on seeing on the element: its text, leaving out what's only
 * for screen readers, what's hidden at some widths (a `hidden` class: shown only from a
 * breakpoint, which jsdom doesn't apply), and decoration (aria-hidden, like a badge's count).
 */
function visibleText(el: Element): string {
  let text = "";
  const walk = (node: Node) => {
    if (node.nodeType === Node.TEXT_NODE) text += node.textContent ?? "";
    if (!(node instanceof Element)) return;
    if (node.classList.contains("sr-only") || node.classList.contains("hidden") || node.getAttribute("aria-hidden") === "true") return;
    if (node.tagName.toLowerCase() === "svg") return;
    node.childNodes.forEach(walk);
  };
  walk(el);
  return text.trim();
}

const hasTooltip = (el: Element) => el.hasAttribute("data-tip") || el.hasAttribute("title") || el.hasAttribute("data-tip-title");

/** What's wrong with the controls on screen now, as "<where>: <what>" lines. */
function problems(where: string): string[] {
  const found: string[] = [];
  for (const role of ROLES) {
    for (const el of screen.queryAllByRole(role, { name: (name) => name.trim() === "" })) {
      found.push(`${where}: a ${role} with no name: ${el.outerHTML.slice(0, 160)}`);
    }
  }
  for (const el of screen.queryAllByRole("button")) {
    const text = visibleText(el);
    if (text === "" && !hasTooltip(el)) found.push(`${where}: an icon-only button with no tooltip: ${el.outerHTML.slice(0, 160)}`);
    // A name given in words must start from the words shown, so a spoken "click Open" finds it.
    const label = el.getAttribute("aria-label");
    const first = text.toLowerCase().match(/[a-z]+/)?.[0];
    if (label && first && !label.toLowerCase().includes(first)) found.push(`${where}: "${label}" doesn't include its text "${text}"`);
  }
  return found;
}

async function openApp() {
  const show = demoShow();
  const backend = new MemoryBackend(show);
  backend.nextAudioPath = DEMO_MUSIC;
  backend.deviceNetwork = demoDevices();
  backend.fppPlayers = demoPlayers();
  backend.fppFiles = demoFppFiles();
  backend.fppSchedules = demoFppSchedules();
  const seq = new MemorySequencer(backend);
  seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(show, 60_000));
  await seq.openSequenceDoc(DEMO_SEQUENCE_PATH);
  const assistant = new FakeAssistant(backend);
  assistant.keys.set("anthropic", "keychain");
  useAssistant.getState().setModel("claude-opus-5-5");
  await useApp.getState().connect(backend);
  useApp.setState({ started: true });
  await useSequencer.getState().connect(seq);
  await useAssistant.getState().connect(assistant);
  const user = userEvent.setup();
  render(<App />);
  return { user, backend };
}

const go = (screenName: Screen) => act(() => useApp.getState().setScreen(screenName));

describe("every control has a name, and icon-only buttons have tooltips", () => {
  beforeEach(() => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({
      x: 0,
      y: 0,
      left: 0,
      top: 0,
      width: 1000,
      height: 600,
      right: 1000,
      bottom: 600,
      toJSON: () => ({}),
    });
  });
  afterEach(() => vi.restoreAllMocks());

  it("on every screen, with the assistant open", async () => {
    const { user } = await openApp();
    const found: string[] = [];
    for (const name of ["layout", "devices", "wiring", "test", "sequence", "play", "history", "settings"] as const) {
      go(name);
      found.push(...problems(name));
    }
    await user.click(screen.getByRole("button", { name: /^Assistant/ }));
    found.push(...problems("assistant"));
    expect(found).toEqual([]);
  });

  it("with the sidebar showing only icons, and its setup checklist open", async () => {
    window.innerWidth = 1100;
    const { user } = await openApp();
    const found = problems("icons-only sidebar");
    await user.click(screen.getByRole("button", { name: /^Set up your show/ }));
    found.push(...problems("setup checklist"));
    expect(found).toEqual([]);
  });

  it("on the Layout screen with props selected, its menus, and the groups tab", async () => {
    const { user, backend } = await openApp();
    go("layout");
    const found: string[] = [];
    act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
    found.push(...problems("layout, one prop selected"));
    act(() => useLayoutEditor.getState().select(backend.show.props.map((p) => p.id)));
    found.push(...problems("layout, every prop selected"));
    await user.click(screen.getByRole("button", { name: "Add prop" }));
    found.push(...problems("add prop menu"));
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "More shapes" }));
    found.push(...problems("more shapes menu"));
    await user.keyboard("{Escape}");
    found.push(...problems("props list"));
    await user.click(screen.getByRole("button", { name: /^Photo/ }));
    found.push(...problems("photo panel"));
    await user.click(screen.getByRole("button", { name: "Tips" }));
    found.push(...problems("tips"));
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("tab", { name: /Groups/ }));
    found.push(...problems("groups tab"));
    expect(found).toEqual([]);
  });

  it("on the Sequence screen with an effect selected", async () => {
    await openApp();
    go("sequence");
    const timeline = screen.getByRole("application", { name: "Timeline" });
    // The Garage Arch's first effect (see the sequence screen's tests for the layout).
    fireEvent.pointerDown(timeline, { clientX: 1000 / 60, clientY: 179, button: 0, pointerId: 1 });
    fireEvent.pointerUp(timeline, { clientX: 1000 / 60, clientY: 179, pointerId: 1 });
    const panel = screen.getByRole("complementary", { name: "Effect settings" });
    expect(within(panel).getAllByRole("heading").length).toBeGreaterThan(0);
    expect(problems("sequence, an effect selected")).toEqual([]);
  });

  it("in the Wiring, Devices, Play and Sequence dialogs and menus", async () => {
    const { user, backend } = await openApp();
    const found: string[] = [];
    go("wiring");
    await user.click(screen.getByRole("button", { name: "Edit Main FPP" }));
    found.push(...problems("wiring, editing a controller"));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await user.click(screen.getAllByRole("button", { name: "Port 1 settings" })[0]);
    found.push(...problems("wiring, port settings"));
    await user.keyboard("{Escape}");
    screen.getByRole("button", { name: "Garage Arch on Main FPP port 1" }).focus();
    await user.keyboard("{Enter}");
    found.push(...problems("wiring, a prop's settings"));
    await user.click(screen.getByRole("button", { name: "Add a prop to port 2 of Main FPP" }));
    found.push(...problems("wiring, add a prop"));
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "Add controller" }));
    found.push(...problems("wiring, add a controller"));
    await user.click(screen.getByRole("button", { name: "Wire port 2 of Main FPP on the layout" }));
    await user.click(within(screen.getByRole("list", { name: "Props to add, in the order you pick them" })).getByRole("button", { name: /Garage Arch/ }));
    found.push(...problems("wiring, on the layout, asking"));
    await user.keyboard("{Escape}{Escape}");

    go("devices");
    await user.click(screen.getByRole("button", { name: "Scan network" }));
    await screen.findAllByRole("row");
    found.push(...problems("devices, after a scan"));

    go("play");
    backend.nextSequencePath = "/Shows/Medley.fseq";
    await user.click(screen.getByRole("button", { name: "Add sequence" }));
    await screen.findByRole("region", { name: "Transport" });
    found.push(...problems("play, with a sequence"));

    go("sequence");
    await user.click(screen.getByRole("button", { name: "Beats menu" }));
    found.push(...problems("sequence, a timing track's menu"));
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "Add timing track" }));
    found.push(...problems("sequence, add a timing track"));
    await user.keyboard("{Escape}");

    await user.click(screen.getByRole("button", { name: /Demo House/ }));
    found.push(...problems("show menu"));
    await user.keyboard("{Escape}");
    act(() => useAssistant.getState().setSettingsOpen(true));
    found.push(...problems("AI settings"));
    expect(found).toEqual([]);
  });

  it("on the Welcome screen", async () => {
    await useApp.getState().connect(new MemoryBackend());
    render(<App />);
    expect(await screen.findByRole("heading", { name: "Welcome to PixelFlow" })).toBeInTheDocument();
    expect(problems("welcome")).toEqual([]);
  });

  it("on the Layout screen in 3D, with its photo and model settings", async () => {
    const { user } = await openApp();
    act(() => useView3d.getState().setMode("3d"));
    go("layout");
    const found = problems("layout in 3D");
    await user.click(screen.getByRole("button", { name: /^Photo and model/ }));
    found.push(...problems("3D photo and model"));
    expect(found).toEqual([]);
  });

  it("in a narrow window: the icons-only sidebar, the floating assistant, and the panels over the canvas and timeline", async () => {
    window.innerWidth = 1100;
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
      const narrow = this instanceof HTMLElement && (this.dataset.layoutRow !== undefined || this.dataset.sequenceWorkspace !== undefined);
      const width = narrow ? 700 : 1000;
      return { x: 0, y: 0, left: 0, top: 0, width, height: 600, right: width, bottom: 600, toJSON: () => ({}) } as DOMRect;
    });
    const { user, backend } = await openApp();
    await user.click(screen.getByRole("button", { name: /^Assistant/ }));
    expect(screen.getByRole("complementary", { name: "Assistant" })).toHaveAttribute("data-overlay", "true");
    go("layout");
    const found = problems("narrow layout");
    await user.click(screen.getByRole("button", { name: "Show the props and groups list" }));
    act(() => useLayoutEditor.getState().select([backend.show.props[0].id]));
    found.push(...problems("narrow layout, list and properties floating"));
    go("sequence");
    expect(screen.getByRole("complementary", { name: "Effects" })).toHaveAttribute("data-compact", "true");
    found.push(...problems("narrow sequence, effects as icons"));
    const timeline = screen.getByRole("application", { name: "Timeline" });
    fireEvent.pointerDown(timeline, { clientX: 1000 / 60, clientY: 179, button: 0, pointerId: 1 });
    fireEvent.pointerUp(timeline, { clientX: 1000 / 60, clientY: 179, pointerId: 1 });
    found.push(...problems("narrow sequence, settings floating"));
    expect(found).toEqual([]);
  });

  it("with the preview beside the timeline", async () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
      const width = this instanceof HTMLElement && this.dataset.sequenceWorkspace !== undefined ? 1400 : 1000;
      return { x: 0, y: 0, left: 0, top: 0, width, height: 600, right: width, bottom: 600, toJSON: () => ({}) } as DOMRect;
    });
    await openApp();
    go("sequence");
    expect(document.querySelector("[data-sequence-workspace]")).toHaveAttribute("data-preview", "side");
    expect(problems("sequence, preview beside")).toEqual([]);
  });

  it("in the sequence dialogs, the Start page, and the Add a row picker", async () => {
    const { user } = await openApp();
    go("sequence");
    const found: string[] = [];
    await user.click(screen.getByRole("button", { name: "Add row" }));
    found.push(...problems("add a row"));
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "New sequence" }));
    found.push(...problems("new sequence dialog"));
    await user.click(within(screen.getByRole("dialog", { name: "New sequence" })).getByRole("button", { name: "Cancel" }));
    await act(() => useSequencer.getState().closeDocument());
    found.push(...problems("sequence start page"));
    expect(found).toEqual([]);
  });

  it("in the app's dialogs, reports, problems list, toasts, and History with backups", async () => {
    const { user, backend } = await openApp();
    const found: string[] = [];
    const look = (where: string) => found.push(...problems(where));
    act(() => useApp.setState({ naming: "Untitled Show" }));
    look("name your show");
    act(() => useApp.setState({ naming: null, renaming: true }));
    look("rename show");
    act(() => useApp.setState({ renaming: false, pendingReplace: "new" }));
    look("unsaved changes");
    act(() => useApp.setState({ pendingReplace: null }));
    act(() => useCloseGuard.setState({ asking: true }));
    look("close window");
    act(() => useCloseGuard.setState({ asking: false }));
    act(() => useApp.setState({ importReport: { name: "House", summary: { props: 4, pixels: 1462, controllers: 2, wired: 3, groups: 1 }, notes: ["A note"] } }));
    look("import report");
    act(() => useApp.setState({ importReport: null, filesReport: { found: [], stillMissing: [], gaveUp: false } }));
    look("files report");
    act(() => useApp.setState({ filesReport: null }));
    const snapshot = useApp.getState().snapshot!;
    act(() => useApp.setState({ snapshot: { ...snapshot, issues: [{ severity: "warning", code: "x", message: "Something to fix", fix: "Fix it" }] } }));
    await user.click(screen.getByRole("button", { name: "1 warning" }));
    look("problems list");
    act(() => void toast("Deleted Arch 1", { label: "Undo", run: () => undefined }));
    look("toast with Undo");
    backend.history = [{ entry: { id: "h1", savedAtMs: Date.UTC(2026, 9, 1, 18), sizeBytes: 2048 }, show: structuredClone(backend.show) }];
    go("history");
    await screen.findByRole("button", { name: /Restore/ });
    look("history with backups");
    expect(found).toEqual([]);
  });

  it("in the assistant's proposal, its draft preview, and an FPP's page", async () => {
    const { user } = await openApp();
    await user.click(screen.getByRole("button", { name: /^Assistant/ }));
    const panel = screen.getByRole("complementary", { name: "Assistant" });
    await user.type(within(panel).getByRole("textbox", { name: "Message the assistant" }), "Add two arches beside the garage{Enter}");
    const card = await within(panel).findByRole("region", { name: "Proposed changes" });
    const found = problems("proposal card");
    await user.click(within(card).getByRole("button", { name: "Preview" }));
    await screen.findByRole("dialog", { name: "Preview: not applied yet" });
    found.push(...problems("draft preview"));
    await user.click(screen.getByRole("button", { name: "Close preview" }));
    await user.click(screen.getByRole("button", { name: "Close assistant" }));
    go("devices");
    await user.click(screen.getByRole("button", { name: "Scan network" }));
    await screen.findAllByRole("row");
    await user.click(screen.getAllByRole("button", { name: /^Open FPP/ })[0]);
    await screen.findByRole("button", { name: "Play Christmas Medley 2017" });
    found.push(...problems("FPP page"));
    await user.click(screen.getByRole("button", { name: /^Send a sequence/ }));
    found.push(...problems("FPP page, send menu"));
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("tab", { name: /^Playlists/ }));
    await screen.findByRole("button", { name: "Play Christmas Show" });
    found.push(...problems("FPP page, playlists"));
    await user.click(screen.getByRole("button", { name: "Set up my show from this FPP" }));
    await screen.findByRole("group", { name: "What will be added" });
    found.push(...problems("FPP page, setting up the show"));
    expect(found).toEqual([]);
  });

  it("in the command palette", async () => {
    await openApp();
    act(() => useApp.getState().setPaletteOpen(true));
    expect(problems("command palette")).toEqual([]);
  });
});
