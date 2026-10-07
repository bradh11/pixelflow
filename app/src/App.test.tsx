import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "./App";
import { MemoryBackend, emptyShow } from "./api/memory";
import { useApp } from "./state/store";
import { useLayoutEditor } from "./state/layoutEditor";

let backend: MemoryBackend;

async function startApp() {
  backend = new MemoryBackend();
  await useApp.getState().connect(backend);
  const user = userEvent.setup();
  render(<App />);
  return user;
}

/** Adds a prop of the kind (an arch unless said) from the Add prop menu. */
async function addProp(user: ReturnType<typeof userEvent.setup>, kind = "Arch") {
  await user.click(screen.getByRole("button", { name: "Add prop" }));
  await user.click(screen.getByRole("menuitem", { name: kind }));
}

/** Answers "Name your show" (asked before a new show's first save). */
async function nameShow(user: ReturnType<typeof userEvent.setup>, name = "My House") {
  const dialog = await screen.findByRole("dialog", { name: "Name your show" });
  const field = within(dialog).getByLabelText("Show name");
  await user.clear(field);
  await user.type(field, name);
  await user.click(within(dialog).getByRole("button", { name: "Save…" }));
}

async function startFresh() {
  const user = await startApp();
  await user.click(screen.getByRole("button", { name: /^new show/i }));
  return user;
}

describe("first run", () => {
  it("offers start, open, xLights import, and discovery", async () => {
    await startApp();
    expect(screen.getByRole("heading", { name: "Welcome to PixelFlow" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /import from xlights/i })).toBeEnabled();
    expect(screen.getByRole("button", { name: /discover my devices/i })).toBeEnabled();
  });

  it("start fresh opens an empty show", async () => {
    await startFresh();
    expect(screen.getByText("Untitled Show")).toBeInTheDocument();
    expect(screen.getByText("No props yet")).toBeInTheDocument();
    expect(backend.calls).toContain("newShow");
  });

  it("open a show uses the file dialog and loads the file", async () => {
    const user = await startApp();
    backend.files.set("/shows/house.pixelflow.json", emptyShow("My House"));
    backend.nextOpenPath = "/shows/house.pixelflow.json";
    await user.click(screen.getByRole("button", { name: /^open…/i }));
    expect(await screen.findByText("My House")).toBeInTheDocument();
  });

  it("cancelling the open dialog stays on the welcome screen", async () => {
    const user = await startApp();
    backend.nextOpenPath = null;
    await user.click(screen.getByRole("button", { name: /^open…/i }));
    expect(screen.getByRole("heading", { name: "Welcome to PixelFlow" })).toBeInTheDocument();
  });
});

describe("editing with undo and redo", () => {
  it("adds, renames, and deletes props, with undo and redo", async () => {
    const user = await startFresh();
    await addProp(user, "Mega tree");
    expect((await screen.findAllByDisplayValue("Mega Tree 1")).length).toBeGreaterThan(0);
    expect(screen.getByTestId("toast")).toHaveTextContent("Added Mega Tree 1 — drag it into place");
    expect(screen.getByText("800")).toBeInTheDocument();
    expect(screen.getByText(/1 prop · 800 pixels/)).toBeInTheDocument();
    expect(screen.getByText("Show not saved")).toBeInTheDocument();

    await user.dblClick(screen.getByRole("option", { name: /^Mega Tree 1/ }));
    const name = screen.getByLabelText("Name of Mega Tree 1");
    await user.clear(name);
    await user.type(name, "Big Tree{Enter}");
    expect(screen.getAllByDisplayValue("Big Tree")[0]).toBeInTheDocument();

    // Undo and Redo say what they'd take back or bring back.
    expect(screen.getByRole("button", { name: "Undo" })).toHaveAttribute("data-tip", "Undo: Rename Mega Tree 1 to Big Tree");
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(screen.getAllByDisplayValue("Mega Tree 1")[0]).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Undo" })).toHaveAttribute("data-tip", "Undo: Add Mega Tree 1");
    expect(screen.getByRole("button", { name: "Redo" })).toHaveAttribute("data-tip", "Redo: Rename Mega Tree 1 to Big Tree");
    await user.click(screen.getByRole("button", { name: "Redo" }));
    expect(screen.getAllByDisplayValue("Big Tree")[0]).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Delete Big Tree" }));
    expect(screen.getByText("No props yet")).toBeInTheDocument();
    expect(screen.getAllByTestId("toast").at(-1)).toHaveTextContent("Deleted Big Tree");
    await user.keyboard("{Meta>}z{/Meta}");
    expect(screen.getByRole("option", { name: /^Big Tree/ })).toBeInTheDocument();

    // The toast's Undo takes a delete back while it's still the latest change.
    await user.click(screen.getByRole("button", { name: "Delete Big Tree" }));
    await user.click(within(screen.getAllByTestId("toast").at(-1)!).getByRole("button", { name: "Undo" }));
    expect(screen.getByRole("option", { name: /^Big Tree/ })).toBeInTheDocument();
  });

  it("shows engine errors in plain language", async () => {
    const user = await startFresh();
    backend.applyEdits = async () => {
      throw new Error("The prop 'Huge' has 2000000 pixels, but PixelFlow supports at most 1000000 per prop.");
    };
    await addProp(user);
    expect(screen.getByRole("alert")).toHaveTextContent("supports at most 1000000 per prop");
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

describe("saving", () => {
  it("asks where to save the first time, then saves in place", async () => {
    const user = await startFresh();
    await addProp(user);
    backend.nextSavePath = "/shows/new.pixelflow.json";
    await user.click(screen.getByRole("button", { name: "Save" }));
    await nameShow(user, "Backyard");
    await waitFor(() => expect(backend.calls).toContain("saveShowAs:/shows/new.pixelflow.json"));
    expect(backend.show.name).toBe("Backyard");
    expect(screen.queryByText("Show not saved")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("toast").at(-1)).toHaveTextContent("Saved Backyard");

    backend.nextSavePath = null;
    await addProp(user);
    await user.keyboard("{Meta>}s{/Meta}");
    expect(backend.calls.filter((c) => c.startsWith("saveShowAs")).length).toBe(2);
    expect(screen.queryByText("Show not saved")).not.toBeInTheDocument();
  });
});

describe("command palette", () => {
  it("adds a prop from another screen in the middle of the Layout canvas, not off to the side", async () => {
    const size = { clientWidth: 800, clientHeight: 600 } as const;
    const saved = Object.fromEntries(Object.keys(size).map((k) => [k, Object.getOwnPropertyDescriptor(HTMLElement.prototype, k)]));
    for (const [k, v] of Object.entries(size)) {
      Object.defineProperty(HTMLElement.prototype, k, {
        configurable: true,
        get() {
          return this.tagName === "CANVAS" ? v : 0;
        },
      });
    }
    try {
      const user = await startFresh();
      await addProp(user, "Line / string");
      await waitFor(() => expect(useLayoutEditor.getState().view).not.toBeNull());
      await user.click(screen.getByRole("button", { name: "Wiring" }));
      await user.keyboard("{Meta>}k{/Meta}");
      await user.type(screen.getByPlaceholderText("Type a command…"), "add prop: line");
      await user.keyboard("{Enter}");
      await waitFor(() => expect(backend.show.props).toHaveLength(2));
      expect(useApp.getState().screen).toBe("layout");
      const [first, second] = backend.show.props.map((p) => p.transform.position.x);
      // Beside the first line would be 6 units over; in the view it's one small step aside.
      expect(Math.abs(second - first)).toBeLessThan(2);
    } finally {
      for (const [k, d] of Object.entries(saved)) if (d) Object.defineProperty(HTMLElement.prototype, k, d);
    }
  });

  it("reaches every screen, and the main action of each", async () => {
    const user = await startFresh();
    for (const [command, heading] of [
      ["go to sequence", "Sequence"],
      ["go to play", "Play"],
      ["go to devices", "Devices"],
    ]) {
      await user.keyboard("{Meta>}k{/Meta}");
      await user.type(screen.getByPlaceholderText("Type a command…"), command);
      await user.keyboard("{Enter}");
      expect(screen.getByRole("heading", { name: heading })).toBeInTheDocument();
    }
    await user.keyboard("{Meta>}k{/Meta}");
    await user.type(screen.getByPlaceholderText("Type a command…"), "sequence");
    expect(screen.getByRole("option", { name: "Open sequence…" })).toBeInTheDocument();
    await user.clear(screen.getByPlaceholderText("Type a command…"));
    await user.type(screen.getByPlaceholderText("Type a command…"), "scan the network");
    await user.keyboard("{Enter}");
    expect(useApp.getState().screen).toBe("devices");
    expect(backend.calls).toContain("discoverDevices::network");
  });

  it("opens with ⌘K and runs commands", async () => {
    const user = await startFresh();
    await user.keyboard("{Meta>}k{/Meta}");
    const input = screen.getByPlaceholderText("Type a command…");
    await user.type(input, "add prop: star");
    await user.keyboard("{Enter}");
    expect(screen.getAllByDisplayValue("Star 1")[0]).toBeInTheDocument();
    expect(screen.queryByPlaceholderText("Type a command…")).not.toBeInTheDocument();

    await user.keyboard("{Meta>}k{/Meta}");
    await user.type(screen.getByPlaceholderText("Type a command…"), "go to wiring");
    await user.keyboard("{Enter}");
    expect(screen.getByRole("heading", { name: "Wiring" })).toBeInTheDocument();
  });
});

describe("wiring and test output", () => {
  it("adds a controller, wires a prop, and starts and stops a test pattern", async () => {
    const user = await startFresh();
    await addProp(user);
    await user.click(screen.getByRole("button", { name: "Wiring" }));
    await user.click(screen.getByRole("button", { name: /add controller/i }));
    await user.type(screen.getByPlaceholderText("e.g. 192.168.1.50"), "10.0.0.20");
    await user.click(screen.getByRole("button", { name: "Add" }));
    expect(screen.getByRole("heading", { name: "Controller 1" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Add a prop to port 1 of Controller 1" }));
    await user.click(within(screen.getByRole("dialog", { name: "Add a prop to port 1 of Controller 1" })).getByRole("button", { name: "Arch 1" }));
    const chip = screen.getByRole("button", { name: "Arch 1 on Controller 1 port 1" });
    expect(chip).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Test" }));
    await user.selectOptions(screen.getByLabelText("Target"), "Controller 1 · port 1");
    await user.selectOptions(screen.getByLabelText("Pattern"), "solid");
    await user.click(screen.getByRole("button", { name: /start/i }));
    expect(await screen.findByText(/Sending/)).toBeInTheDocument();
    const row = screen.getByRole("cell", { name: "Controller 1" }).closest("tr")!;
    expect(within(row).getByText("ok")).toBeInTheDocument();
    expect(backend.output.target).toEqual({ type: "port", controller: backend.show.controllers[0].id, port: 1 });
    expect(backend.output.pattern).toEqual({ kind: "solid", color: "ffffff" });

    // While it runs, changes reach the lights without pressing Restart.
    await user.click(screen.getByRole("button", { name: "Red" }));
    await waitFor(() => expect(backend.output.pattern).toEqual({ kind: "solid", color: "ff0000" }));
    expect(screen.getByLabelText("Color as hex")).toHaveValue("#ff0000");
    await user.selectOptions(screen.getByLabelText("Pattern"), "chase");
    await waitFor(() => expect(backend.output.pattern).toEqual({ kind: "chase", color: "ff0000" }));
    const hex = screen.getByLabelText("Color as hex");
    await user.clear(hex);
    await user.type(hex, "00ff00");
    await waitFor(() => expect(backend.output.pattern).toEqual({ kind: "chase", color: "00ff00" }));

    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    expect(await screen.findByText("Output stopped")).toBeInTheDocument();
  });

  it("shows why output stopped on its own", async () => {
    const user = await startFresh();
    await addProp(user);
    await user.click(screen.getByRole("button", { name: "Wiring" }));
    await user.click(screen.getByRole("button", { name: /add controller/i }));
    await user.type(screen.getByPlaceholderText("e.g. 192.168.1.50"), "10.0.0.20");
    await user.click(screen.getByRole("button", { name: "Add" }));
    const reason = "Output stopped because the show now has errors: Port 1 is over capacity.";
    backend.outputStatus = async () => ({ ...(await backend.stopOutput()), stopReason: reason });
    await user.click(screen.getByRole("button", { name: "Test" }));
    expect(await screen.findByText(reason)).toBeInTheDocument();
  });
});

describe("theme", () => {
  it("toggles and remembers the theme", async () => {
    const user = await startFresh();
    expect(document.documentElement.dataset.theme).toBe("dark");
    await user.click(screen.getByRole("button", { name: "Light theme" }));
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(localStorage.getItem("pixelflow.theme")).toBe("light");
  });
});

async function addController(user: ReturnType<typeof userEvent.setup>, ip: string) {
  await user.click(screen.getByRole("button", { name: /add controller/i }));
  await user.type(screen.getByPlaceholderText("e.g. 192.168.1.50"), ip);
  await user.click(screen.getByRole("button", { name: "Add" }));
}

describe("unsaved changes on new and open", () => {
  it("asks first; Cancel keeps the show and its undo history", async () => {
    const user = await startFresh();
    await addProp(user);
    await user.keyboard("{Meta>}n{/Meta}");
    const dialog = screen.getByRole("dialog", { name: "Save changes to Untitled Show?" });
    expect(dialog).toHaveTextContent("Your changes will be lost if you don't save them.");
    expect(backend.calls.filter((c) => c === "newShow").length).toBe(1);
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog", { name: /save changes/i })).not.toBeInTheDocument();
    expect(screen.getByText(/^1 prop · .*pixels/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Undo" })).toBeEnabled();
  });

  it("Don't save creates the new show", async () => {
    const user = await startFresh();
    await addProp(user);
    await user.keyboard("{Meta>}n{/Meta}");
    await user.click(screen.getByRole("button", { name: "Don't save" }));
    expect(backend.calls.filter((c) => c === "newShow").length).toBe(2);
    expect(await screen.findByText("No props yet")).toBeInTheDocument();
  });

  it("Save asks for a path on a never-saved show, saves, then creates the new show", async () => {
    const user = await startFresh();
    await addProp(user);
    backend.nextSavePath = "/shows/keep.pixelflow.json";
    await user.keyboard("{Meta>}n{/Meta}");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Save" }));
    await nameShow(user);
    await waitFor(() => expect(backend.calls).toContain("saveShowAs:/shows/keep.pixelflow.json"));
    expect(backend.calls.filter((c) => c === "newShow").length).toBe(2);
    expect(await screen.findByText("No props yet")).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: /save changes/i })).not.toBeInTheDocument();
  });

  it("Save that is cancelled keeps the dialog open", async () => {
    const user = await startFresh();
    await addProp(user);
    backend.nextSavePath = null;
    await user.keyboard("{Meta>}n{/Meta}");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Save" }));
    expect(screen.getByRole("dialog", { name: /save changes/i })).toBeInTheDocument();
    expect(backend.calls.filter((c) => c === "newShow").length).toBe(1);
  });

  it("Escape cancels", async () => {
    const user = await startFresh();
    await addProp(user);
    await user.keyboard("{Meta>}n{/Meta}");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: /save changes/i })).not.toBeInTheDocument();
  });

  it("open asks too, and a clean show is replaced without asking", async () => {
    const user = await startFresh();
    await user.keyboard("{Meta>}n{/Meta}");
    expect(screen.queryByRole("dialog", { name: /save changes/i })).not.toBeInTheDocument();
    expect(backend.calls.filter((c) => c === "newShow").length).toBe(2);

    await addProp(user);
    backend.files.set("/shows/house.pixelflow.json", emptyShow("My House"));
    backend.nextOpenPath = "/shows/house.pixelflow.json";
    await user.keyboard("{Meta>}o{/Meta}");
    await user.click(screen.getByRole("button", { name: "Don't save" }));
    expect(await screen.findByText("My House")).toBeInTheDocument();
  });
});

describe("test output safety", () => {
  async function wired() {
    const user = await startFresh();
    await addProp(user);
    await user.click(screen.getByRole("button", { name: "Wiring" }));
    await addController(user, "10.0.0.20");
    return user;
  }

  it("shows an error when stopping fails", async () => {
    const user = await wired();
    await user.click(screen.getByRole("button", { name: "Test" }));
    await user.click(screen.getByRole("button", { name: /^start/i }));
    expect(await screen.findByText(/Sending/)).toBeInTheDocument();
    backend.stopOutput = async () => {
      throw new Error("Could not stop output.");
    };
    await user.click(screen.getByRole("button", { name: /^stop$/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not stop output.");
  });

  it("shows live output in the status bar and can stop it from any screen", async () => {
    const user = await wired();
    await user.click(screen.getByRole("button", { name: "Test" }));
    await user.click(screen.getByRole("button", { name: /^start/i }));
    expect(await screen.findByText(/Sending/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Layout" }));
    expect(await screen.findByText("Live output")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Stop live output" }));
    expect(backend.output.running).toBe(false);
    await waitFor(() => expect(screen.queryByText("Live output")).not.toBeInTheDocument());
  });

  it("falls back to the whole show with a notice when the chosen target disappears", async () => {
    const user = await wired();
    await addController(user, "10.0.0.21");
    await user.click(screen.getByRole("button", { name: "Test" }));
    await user.selectOptions(screen.getByLabelText("Target"), "Controller 1 · port 1");
    await user.click(screen.getByRole("button", { name: "Wiring" }));
    await user.click(screen.getByRole("button", { name: "Delete Controller 1" }));
    await user.click(screen.getByRole("button", { name: "Test" }));
    expect(await screen.findByRole("status")).toHaveTextContent(
      "The chosen target was removed; testing the whole show.",
    );
    expect(screen.getByLabelText("Target")).toHaveValue("show");
    await user.click(screen.getByRole("button", { name: /^start/i }));
    await waitFor(() => expect(backend.calls).toContain("startOutput"));
    expect(backend.lastTarget).toEqual({ type: "show" });
  });
});

describe("confirm dialog safety", () => {
  it("ignores shortcuts while the dialog is open", async () => {
    const user = await startFresh();
    await addProp(user);
    await user.keyboard("{Meta>}n{/Meta}");
    expect(screen.getByRole("dialog", { name: /save changes/i })).toBeInTheDocument();
    await user.keyboard("{Meta>}z{/Meta}");
    expect(backend.calls).not.toContain("undo");
  });

  it("a double-click on Save creates the new show once", async () => {
    const user = await startFresh();
    await addProp(user);
    await user.keyboard("{Meta>}n{/Meta}");
    const dialog = screen.getByRole("dialog");
    backend.nextSavePath = "/shows/x.pixelflow.json";
    const originalSave = backend.saveShowAs.bind(backend);
    backend.saveShowAs = async (path: string) => {
      await new Promise((r) => setTimeout(r, 50));
      return originalSave(path);
    };
    const newShowsBefore = backend.calls.filter((c) => c === "newShow").length;
    const save = within(dialog).getByRole("button", { name: "Save" });
    await Promise.all([user.click(save), user.click(save)]);
    await nameShow(user);
    await waitFor(() => expect(backend.calls.filter((c) => c === "newShow").length).toBe(newShowsBefore + 1));
    await new Promise((r) => setTimeout(r, 150));
    expect(backend.calls.filter((c) => c.startsWith("saveShowAs:")).length).toBe(1);
    expect(backend.calls.filter((c) => c === "newShow").length).toBe(newShowsBefore + 1);
  });
});

describe("problems popover", () => {
  it("toggles aria-expanded, closes on Escape and when problems clear", async () => {
    const user = await startFresh();
    const base = useApp.getState().snapshot!;
    const withIssues = (issues: typeof base.issues) => act(() => useApp.setState({ snapshot: { ...base, issues } }));
    withIssues([{ severity: "error", message: "Port 1 is over capacity.", fix: "Move a prop." } as (typeof base.issues)[number]]);
    const btn = screen.getByRole("button", { name: /1 error/i });
    expect(btn).toHaveAttribute("aria-expanded", "false");
    await user.click(btn);
    expect(btn).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("Port 1 is over capacity.")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(btn).toHaveAttribute("aria-expanded", "false");
    await user.click(btn);
    withIssues([]);
    expect(screen.getByRole("button", { name: /no problems/i })).toHaveAttribute("aria-expanded", "false");
    withIssues([{ severity: "error", message: "Again.", fix: null } as (typeof base.issues)[number]]);
    expect(screen.getByRole("button", { name: /1 error/i })).toHaveAttribute("aria-expanded", "false");
  });
});
