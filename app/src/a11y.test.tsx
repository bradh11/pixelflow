// Every control the app shows has a name, and every button showing only an icon has a tooltip.
// The app is opened on the demo show and walked through each screen (and the panels and menus
// that open on them), checking whatever is on screen at each stop.

import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { DEMO_MUSIC, DEMO_SEQUENCE_PATH, demoSequence } from "./api/demoSequence";
import { demoDevices, demoPlayers, demoShow } from "./api/demo";
import { MemoryBackend } from "./api/memory";
import { FakeAssistant } from "./api/memoryAssistant";
import { MemorySequencer } from "./api/memorySequencer";
import { useAssistant } from "./state/assistant";
import { useLayoutEditor } from "./state/layoutEditor";
import { useSequencer } from "./state/sequencer";
import { type Screen, useApp } from "./state/store";

const ROLES = ["button", "link", "tab", "menuitem", "menuitemradio", "checkbox", "radio", "switch", "textbox", "combobox", "slider", "spinbutton", "option"] as const;

/** The text a sighted user sees on the element: its text, leaving out what's only for screen readers. */
function visibleText(el: Element): string {
  let text = "";
  const walk = (node: Node) => {
    if (node.nodeType === Node.TEXT_NODE) text += node.textContent ?? "";
    if (!(node instanceof Element)) return;
    if (node.classList.contains("sr-only") || node.tagName.toLowerCase() === "svg") return;
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
    if (visibleText(el) === "" && !hasTooltip(el)) found.push(`${where}: an icon-only button with no tooltip: ${el.outerHTML.slice(0, 160)}`);
  }
  return found;
}

async function openApp() {
  const show = demoShow();
  const backend = new MemoryBackend(show);
  backend.nextAudioPath = DEMO_MUSIC;
  backend.deviceNetwork = demoDevices();
  backend.fppPlayers = demoPlayers();
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
    for (const name of ["layout", "devices", "wiring", "test", "sequence", "play", "history"] as const) {
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

  it("in the command palette", async () => {
    await openApp();
    act(() => useApp.getState().setPaletteOpen(true));
    expect(problems("command palette")).toEqual([]);
  });
});
