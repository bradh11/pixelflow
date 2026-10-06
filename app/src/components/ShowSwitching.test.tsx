import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { demoShow } from "../api/demo";
import { DEMO_SEQUENCE_PATH, demoSequence } from "../api/demoSequence";
import { MemoryBackend, emptyShow, layoutThumbnail } from "../api/memory";
import { MemorySequencer } from "../api/memorySequencer";
import type { RecentShow, Show } from "../api/types";
import { useSequencer } from "../state/sequencer";
import { useApp } from "../state/store";

const HOUSE = "/Shows/House/house.pixelflow.json";
const SHED = "/Shows/Shed/shed.pixelflow.json";
const GONE = "/Shows/Old/gone.pixelflow.json";

function entry(path: string, show: Show, hoursAgo: number): RecentShow {
  return {
    path,
    name: show.name,
    openedAt: Date.now() - hoursAgo * 3_600_000,
    props: show.props.length,
    pixels: 1234,
    controllers: show.controllers.length,
    thumbnail: layoutThumbnail(show),
    status: "here",
  };
}

/** The app on its start page, with House, Shed, and a show that's gone in its recent list. */
async function start({ sequence = false }: { sequence?: boolean } = {}) {
  const house = { ...demoShow(), name: "Demo House" };
  const shed = emptyShow("Shed");
  const backend = new MemoryBackend();
  backend.files.set(HOUSE, house);
  backend.files.set(SHED, shed);
  backend.recent = [entry(HOUSE, house, 2), entry(SHED, shed, 30), entry(GONE, emptyShow("Old Show"), 80)];
  const seq = new MemorySequencer(backend);
  if (sequence) seq.files.set(DEMO_SEQUENCE_PATH, demoSequence(house, 60_000));
  await useApp.getState().connect(backend);
  await useSequencer.getState().connect(seq);
  const user = userEvent.setup();
  render(<App />);
  await screen.findByRole("heading", { name: "Recent shows" });
  return { backend, seq, user, house };
}

const welcome = () => screen.queryByRole("heading", { name: "Welcome to PixelFlow" });
const showMenuButton = () => document.querySelector<HTMLElement>('[aria-haspopup="menu"]')!;

/** Opens House from the start page. */
async function openHouse(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("button", { name: /^Open Demo House/ }));
  await waitFor(() => expect(welcome()).not.toBeInTheDocument());
}

describe("the start page", () => {
  it("lists recent shows with their folder, when they were opened, counts, and a picture", async () => {
    await start();
    const list = screen.getByRole("heading", { name: "Recent shows" }).closest("section")!;
    const house = within(list).getByRole("button", { name: /^Open Demo House/ });
    expect(house).toHaveTextContent("/Shows/House");
    expect(house).toHaveTextContent("Opened 2 h ago");
    expect(house).toHaveTextContent("4 props · 1,234 pixels · 2 controllers");
    expect(house.querySelector("img")?.getAttribute("src")).toMatch(/^data:image\/svg\+xml/);
    // The other ways to start are still there.
    for (const name of [/^new show/i, /^open…/i, /^import from xlights/i, /^try the demo show/i, /^discover my devices/i]) {
      expect(screen.getByRole("button", { name })).toBeEnabled();
    }
  });

  it("opens a recent show with a click, through the same open as the Open dialog", async () => {
    const { backend, user } = await start();
    await openHouse(user);
    expect(backend.calls).toContain(`openShow:${HOUSE}`);
    expect(useApp.getState().snapshot?.show.name).toBe("Demo House");
  });

  it("opens a recent show with the keyboard", async () => {
    const { backend, user } = await start();
    screen.getByRole("button", { name: /^Open Shed/ }).focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(backend.calls).toContain(`openShow:${SHED}`));
  });

  it("keeps a show that's gone, greyed, with Locate… and Remove from list", async () => {
    const { backend, user } = await start();
    const gone = screen.getByRole("listitem", { name: "Old Show (moved or deleted)" });
    expect(gone).toHaveTextContent("Moved or deleted");
    expect(within(gone).queryByRole("button", { name: /^Open/ })).not.toBeInTheDocument();
    backend.files.set("/Shows/New/old.pixelflow.json", emptyShow("Old Show"));
    backend.nextRecentLocatePath = "/Shows/New/old.pixelflow.json";
    await user.click(within(gone).getByRole("button", { name: "Locate Old Show…" }));
    await waitFor(() => expect(useApp.getState().snapshot?.path).toBe("/Shows/New/old.pixelflow.json"));
    expect(backend.recent.map((r) => r.path)).not.toContain(GONE);
  });

  it("removes a show from the list, or clears it, only when asked", async () => {
    const { backend, user } = await start();
    await user.click(screen.getByRole("button", { name: "Remove Old Show from the list" }));
    await waitFor(() => expect(screen.queryByText("Old Show")).not.toBeInTheDocument());
    expect(backend.calls).toContain(`forgetRecentShow:${GONE}`);
    await user.click(screen.getByRole("button", { name: "Clear recent shows" }));
    expect(await screen.findByText(/Shows you open or save will be listed here/)).toBeInTheDocument();
    expect(backend.recent).toEqual([]);
  });

  it("says a recent show couldn't be opened, and keeps it", async () => {
    const { backend, user } = await start();
    backend.files.delete(SHED);
    await user.click(screen.getByRole("button", { name: /^Open Shed/ }));
    await waitFor(() => expect(useApp.getState().error).toMatch(/still in your recent shows: use Locate…/));
    expect(welcome()).toBeInTheDocument();
    expect(await screen.findByRole("listitem", { name: "Shed (moved or deleted)" })).toBeInTheDocument();
  });

  it("says what's opening while it opens", async () => {
    const { backend, user } = await start();
    let finish!: () => void;
    const open = backend.openShow.bind(backend);
    backend.openShow = async (path) => {
      await new Promise<void>((resolve) => (finish = resolve));
      return open(path);
    };
    await user.click(screen.getByRole("button", { name: /^Open Shed/ }));
    expect(await screen.findByRole("status")).toHaveTextContent("Opening Shed…");
    act(() => finish());
    await waitFor(() => expect(screen.queryByRole("status")).not.toBeInTheDocument());
  });

  it("opens the demo show as an unsaved copy", async () => {
    const { backend, user } = await start();
    await user.click(screen.getByRole("button", { name: /^try the demo show/i }));
    await waitFor(() => expect(welcome()).not.toBeInTheDocument());
    const snapshot = useApp.getState().snapshot!;
    expect(snapshot.show.name).toBe("Demo House");
    expect(snapshot.path).toBeNull();
    expect(snapshot.dirty).toBe(true);
    expect(backend.calls).toContain("openSampleShow");
  });
});

describe("the show menu", () => {
  it("lists the recent shows with this one marked, and switches to another", async () => {
    const { backend, user } = await start();
    await openHouse(user);
    await user.click(showMenuButton());
    const menu = screen.getByRole("menu", { name: "Show" });
    expect(within(menu).getByText(HOUSE)).toBeInTheDocument();
    const recent = within(menu).getByRole("group", { name: "Recent shows" });
    const items = within(recent).getAllByRole("menuitem");
    expect(items[0]).toHaveAttribute("aria-current", "true");
    expect(items[0]).toHaveTextContent("Demo House");
    for (const name of ["Open…", "New show", "Import from xLights…", "Rename…", "Save", "Save As…", "Close show"]) {
      expect(within(menu).getByRole("menuitem", { name: new RegExp(`^${name}(⌘|⇧|$)`) })).toBeInTheDocument();
    }
    await user.click(within(recent).getByRole("menuitem", { name: /Shed/ }));
    await waitFor(() => expect(useApp.getState().snapshot?.show.name).toBe("Shed"));
    expect(backend.calls).toContain(`openShow:${SHED}`);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("moves through its items with the arrow keys and closes with Escape", async () => {
    const { user } = await start();
    await openHouse(user);
    await user.click(showMenuButton());
    const items = within(screen.getByRole("menu")).getAllByRole("menuitem");
    expect(items[0]).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(items[1]).toHaveFocus();
    await user.keyboard("{ArrowUp}{ArrowUp}");
    expect(items[items.length - 1]).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(showMenuButton()).toHaveFocus();
  });

  it("Close show, and the PixelFlow brand, go back to the start page", async () => {
    const { user } = await start();
    await openHouse(user);
    await user.click(showMenuButton());
    await user.click(screen.getByRole("menuitem", { name: /^Close show/ }));
    expect(await screen.findByRole("heading", { name: "Welcome to PixelFlow" })).toBeInTheDocument();
    await openHouse(user);
    await user.click(screen.getByRole("button", { name: "PixelFlow home (closes the show)" }));
    expect(await screen.findByRole("heading", { name: "Welcome to PixelFlow" })).toBeInTheDocument();
  });

  it("says an imported show isn't saved yet", async () => {
    const { backend, user } = await start();
    backend.nextShowFolder = "/xLights/Show";
    backend.xlightsImport = { show: emptyShow("My xLights Show"), summary: { props: 0, pixels: 0, controllers: 0, wired: 0, groups: 0 }, notes: [] };
    await user.click(screen.getByRole("button", { name: /^import from xlights/i }));
    await waitFor(() => expect(welcome()).not.toBeInTheDocument());
    act(() => useApp.getState().dismissImportReport());
    await user.click(showMenuButton());
    expect(within(screen.getByRole("menu")).getByText("(not saved yet)")).toBeInTheDocument();
  });

  it("renames the show from the menu, as one undo step", async () => {
    const { backend, user } = await start();
    await openHouse(user);
    await user.click(showMenuButton());
    await user.click(screen.getByRole("menuitem", { name: "Rename…" }));
    const field = screen.getByRole("textbox", { name: "Show name" });
    await user.clear(field);
    await user.type(field, "Our House{Enter}");
    await waitFor(() => expect(useApp.getState().snapshot?.show.name).toBe("Our House"));
    expect(backend.undoStack).toHaveLength(1);
    await user.keyboard("{Meta>}z{/Meta}");
    await waitFor(() => expect(useApp.getState().snapshot?.show.name).toBe("Demo House"));
  });

  it("renames the show with a double-click on its name; Escape keeps the old name", async () => {
    const { user } = await start();
    await openHouse(user);
    await user.dblClick(showMenuButton());
    const field = screen.getByRole("textbox", { name: "Show name" });
    await user.type(field, "Nope{Escape}");
    expect(useApp.getState().snapshot?.show.name).toBe("Demo House");
    await user.dblClick(showMenuButton());
    await user.clear(screen.getByRole("textbox", { name: "Show name" }));
    await user.type(screen.getByRole("textbox", { name: "Show name" }), "Front Yard{Enter}");
    await waitFor(() => expect(screen.getByRole("button", { name: /Front Yard/ })).toBeInTheDocument());
  });
});

describe("leaving a show with unsaved work", () => {
  it("asks before closing; Cancel stays, Don't save leaves", async () => {
    const { user } = await start();
    await openHouse(user);
    await act(() => useApp.getState().apply([{ type: "renameShow", name: "Changed" }]));
    await user.keyboard("{Meta>}w{/Meta}");
    const dialog = await screen.findByRole("dialog", { name: "Save changes to Changed?" });
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(welcome()).not.toBeInTheDocument();
    await user.keyboard("{Meta>}w{/Meta}");
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Don't save" }));
    expect(await screen.findByRole("heading", { name: "Welcome to PixelFlow" })).toBeInTheDocument();
  });

  it("asks once about the show and its open sequence, and Save all saves both", async () => {
    const { backend, seq, user } = await start({ sequence: true });
    await openHouse(user);
    await act(() => useSequencer.getState().open(DEMO_SEQUENCE_PATH));
    await act(() => useApp.getState().apply([{ type: "renameShow", name: "Changed" }]));
    act(() => useSequencer.setState({ dirty: true }));
    await user.click(showMenuButton());
    await user.click(screen.getByRole("menuitem", { name: /Shed/ }));
    const dialog = await screen.findByRole("dialog", { name: `Save changes to Changed and ${useSequencer.getState().doc!.name}?` });
    await user.click(within(dialog).getByRole("button", { name: "Save all" }));
    await waitFor(() => expect(useApp.getState().snapshot?.show.name).toBe("Shed"));
    expect(seq.calls).toContain("saveSequenceDocAs");
    expect(backend.calls).toContain(`saveShowAs:${HOUSE}`);
    // The sequence went with its show.
    expect(useSequencer.getState().doc).toBeNull();
  });

  it("Don't save drops the open sequence's changes too", async () => {
    const { user } = await start({ sequence: true });
    await openHouse(user);
    await act(() => useSequencer.getState().open(DEMO_SEQUENCE_PATH));
    act(() => useSequencer.setState({ dirty: true }));
    await user.keyboard("{Meta>}n{/Meta}");
    const dialog = await screen.findByRole("dialog", { name: /^Save changes to / });
    expect(within(dialog).getByText(/open sequence closes with the show/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Don't save" }));
    await waitFor(() => expect(useApp.getState().snapshot?.show.name).toBe("Untitled Show"));
    expect(useSequencer.getState().doc).toBeNull();
  });
});

describe("shortcuts and the menu bar", () => {
  it("⌘O opens, ⌘N starts a new show, ⌘W closes the show", async () => {
    const { backend, user } = await start();
    backend.nextOpenPath = SHED;
    await user.keyboard("{Meta>}o{/Meta}");
    await waitFor(() => expect(useApp.getState().snapshot?.show.name).toBe("Shed"));
    await user.keyboard("{Meta>}n{/Meta}");
    await waitFor(() => expect(useApp.getState().snapshot?.show.name).toBe("Untitled Show"));
    await user.keyboard("{Meta>}w{/Meta}");
    expect(await screen.findByRole("heading", { name: "Welcome to PixelFlow" })).toBeInTheDocument();
  });

  it("⇧⌘O opens the show menu at its recent shows", async () => {
    const { user } = await start();
    await openHouse(user);
    await user.keyboard("{Meta>}{Shift>}o{/Shift}{/Meta}");
    const recent = within(await screen.findByRole("menu")).getByRole("group", { name: "Recent shows" });
    expect(within(recent).getAllByRole("menuitem")[0]).toHaveFocus();
  });

  it("⌘W with no show open is left to the window", async () => {
    const { backend } = await start();
    // The menu bar's Close Show (⌘W) closes the window when there's no show.
    await act(() => backend.chooseMenu({ action: "closeShow" }));
    await waitFor(() => expect(backend.calls).toContain("closeWindow"));
  });

  it("File menu items run the same actions", async () => {
    const { backend } = await start();
    await act(() => backend.chooseMenu({ action: "openRecent", path: SHED }));
    await waitFor(() => expect(useApp.getState().snapshot?.show.name).toBe("Shed"));
    await act(() => backend.chooseMenu({ action: "closeShow" }));
    expect(await screen.findByRole("heading", { name: "Welcome to PixelFlow" })).toBeInTheDocument();
    await act(() => backend.chooseMenu({ action: "clearRecent" }));
    await waitFor(() => expect(backend.recent).toEqual([]));
    expect(backend.calls).not.toContain("closeWindow");
  });

  it("the command palette lists the recent shows and the show shortcuts", async () => {
    const { user } = await start();
    await openHouse(user);
    await user.keyboard("{Meta>}k{/Meta}");
    const group = await screen.findByRole("group", { name: "Open Recent" });
    expect(within(group).getByText(/Open recent: Shed/)).toBeInTheDocument();
    expect(within(group).getByText(/Open recent: Old Show .*moved, Locate…/)).toBeInTheDocument();
    for (const [label, key] of [["New show", "⌘N"], ["Open show…", "⌘O"], ["Open recent show…", "⇧⌘O"], ["Close show", "⌘W"]]) {
      expect(screen.getByRole("option", { name: new RegExp(`^${label}\\s*${key}`) })).toBeInTheDocument();
    }
    await user.type(screen.getByPlaceholderText("Type a command…"), "open recent: shed");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(useApp.getState().snapshot?.show.name).toBe("Shed"));
  });
});
