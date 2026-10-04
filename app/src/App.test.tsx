import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "./App";
import { MemoryBackend, emptyShow } from "./api/memory";
import { useApp } from "./state/store";

let backend: MemoryBackend;

async function startApp() {
  backend = new MemoryBackend();
  await useApp.getState().connect(backend);
  const user = userEvent.setup();
  render(<App />);
  return user;
}

async function startFresh() {
  const user = await startApp();
  await user.click(screen.getByRole("button", { name: /start fresh/i }));
  return user;
}

describe("first run", () => {
  it("offers start, open, and upcoming import/discovery choices", async () => {
    await startApp();
    expect(screen.getByRole("heading", { name: "Welcome to PixelFlow" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /import from xlights/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /discover my devices/i })).toBeDisabled();
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
    await user.click(screen.getByRole("button", { name: /open a show/i }));
    expect(await screen.findByText("My House")).toBeInTheDocument();
  });

  it("cancelling the open dialog stays on the welcome screen", async () => {
    const user = await startApp();
    backend.nextOpenPath = null;
    await user.click(screen.getByRole("button", { name: /open a show/i }));
    expect(screen.getByRole("heading", { name: "Welcome to PixelFlow" })).toBeInTheDocument();
  });
});

describe("editing with undo and redo", () => {
  it("adds, renames, and deletes props, with undo and redo", async () => {
    const user = await startFresh();
    await user.selectOptions(screen.getByLabelText("Prop type"), "tree");
    await user.click(screen.getByRole("button", { name: /add prop/i }));
    expect(screen.getByDisplayValue("Mega Tree 1")).toBeInTheDocument();
    expect(screen.getByText("800")).toBeInTheDocument();
    expect(screen.getByText(/1 prop · 800 pixels/)).toBeInTheDocument();
    expect(screen.getByLabelText("Unsaved changes")).toBeInTheDocument();

    const name = screen.getByLabelText("Name of Mega Tree 1");
    await user.clear(name);
    await user.type(name, "Big Tree{Enter}");
    expect(screen.getByDisplayValue("Big Tree")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(screen.getByDisplayValue("Mega Tree 1")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Redo" }));
    expect(screen.getByDisplayValue("Big Tree")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Delete Big Tree" }));
    expect(screen.getByText("No props yet")).toBeInTheDocument();
    await user.keyboard("{Meta>}z{/Meta}");
    expect(screen.getByDisplayValue("Big Tree")).toBeInTheDocument();
  });

  it("shows engine errors in plain language", async () => {
    const user = await startFresh();
    backend.applyEdits = async () => {
      throw new Error("The prop 'Huge' has 2000000 pixels, but PixelFlow supports at most 1000000 per prop.");
    };
    await user.click(screen.getByRole("button", { name: /add prop/i }));
    expect(screen.getByRole("alert")).toHaveTextContent("supports at most 1000000 per prop");
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

describe("saving", () => {
  it("asks where to save the first time, then saves in place", async () => {
    const user = await startFresh();
    await user.click(screen.getByRole("button", { name: /add prop/i }));
    backend.nextSavePath = "/shows/new.pixelflow.json";
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(backend.calls).toContain("saveShowAs:/shows/new.pixelflow.json");
    expect(screen.queryByLabelText("Unsaved changes")).not.toBeInTheDocument();

    backend.nextSavePath = null;
    await user.click(screen.getByRole("button", { name: /add prop/i }));
    await user.keyboard("{Meta>}s{/Meta}");
    expect(backend.calls.filter((c) => c.startsWith("saveShowAs")).length).toBe(2);
    expect(screen.queryByLabelText("Unsaved changes")).not.toBeInTheDocument();
  });
});

describe("command palette", () => {
  it("opens with ⌘K and runs commands", async () => {
    const user = await startFresh();
    await user.keyboard("{Meta>}k{/Meta}");
    const input = screen.getByPlaceholderText("Type a command…");
    await user.type(input, "add prop: star");
    await user.keyboard("{Enter}");
    expect(screen.getByDisplayValue("Star 1")).toBeInTheDocument();
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
    await user.click(screen.getByRole("button", { name: /add prop/i }));
    await user.click(screen.getByRole("button", { name: "Wiring" }));
    await user.click(screen.getByRole("button", { name: /add controller/i }));
    await user.type(screen.getByPlaceholderText("192.168.1.50"), "10.0.0.20");
    await user.click(screen.getByRole("button", { name: "Add" }));
    expect(screen.getByRole("heading", { name: "Controller 1" })).toBeInTheDocument();

    await user.selectOptions(screen.getByLabelText("Add a prop to port 1"), "Arch 1");
    const chip = screen.getByRole("button", { name: "Unwire Arch 1 from port 1" });
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

    await user.click(screen.getByRole("button", { name: /stop/i }));
    expect(await screen.findByText("Output stopped")).toBeInTheDocument();
  });

  it("shows why output stopped on its own", async () => {
    const user = await startFresh();
    await user.click(screen.getByRole("button", { name: /add prop/i }));
    await user.click(screen.getByRole("button", { name: "Wiring" }));
    await user.click(screen.getByRole("button", { name: /add controller/i }));
    await user.type(screen.getByPlaceholderText("192.168.1.50"), "10.0.0.20");
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
