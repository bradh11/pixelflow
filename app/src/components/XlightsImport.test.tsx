import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../App";
import { demoShow } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import { useApp } from "../state/store";

async function startApp() {
  const backend = new MemoryBackend();
  backend.nextShowFolder = "/Shows/Haas 2024";
  backend.xlightsImport = {
    show: { ...demoShow(), name: "Haas 2024" },
    summary: { props: 4, pixels: 1462, controllers: 2, wired: 3, groups: 1 },
    notes: ["Window Matrix: shown as a grid; pixel order may differ"],
  };
  await useApp.getState().connect(backend);
  const user = userEvent.setup();
  render(<App />);
  return { backend, user };
}

describe("xLights import", () => {
  it("imports a show folder from the welcome screen and reports what came in", async () => {
    const { user, backend } = await startApp();
    await user.click(screen.getByRole("button", { name: /import from xlights/i }));
    expect(backend.calls).toContain("importXlights:/Shows/Haas 2024");
    const report = await screen.findByRole("dialog", { name: "Imported Haas 2024" });
    expect(within(report).getByText(/4 props · 1,462 pixels · 2 controllers · 3 props wired · 1 group/)).toBeInTheDocument();
    expect(within(report).getByText(/Window Matrix: shown as a grid/)).toBeInTheDocument();
    await user.click(within(report).getByRole("button", { name: "Later" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(useApp.getState().snapshot!.show.name).toBe("Haas 2024");
    expect(useApp.getState().snapshot!.dirty).toBe(true);
  });

  it("asks before replacing unsaved work", async () => {
    const { user, backend } = await startApp();
    await user.click(screen.getByRole("button", { name: /^new show/i }));
    await useApp.getState().apply([{ type: "renameShow", name: "Changed" }]);
    await useApp.getState().importXlights();
    expect(await screen.findByRole("dialog", { name: /save changes to/i })).toBeInTheDocument();
    expect(backend.calls.some((c) => c.startsWith("importXlights"))).toBe(false);
  });

  it("shows every note, even repeated ones", async () => {
    const { user, backend } = await startApp();
    const repeated = "Tree: some light positions could not be computed and were placed at the origin";
    backend.xlightsImport!.notes = [repeated, repeated];
    const errors = vi.spyOn(console, "error").mockImplementation(() => {});
    await user.click(screen.getByRole("button", { name: /import from xlights/i }));
    const report = await screen.findByRole("dialog", { name: "Imported Haas 2024" });
    expect(within(report).getAllByText(repeated)).toHaveLength(2);
    expect(errors.mock.calls.some((c) => String(c[0]).includes("same key"))).toBe(false);
    errors.mockRestore();
  });
});
