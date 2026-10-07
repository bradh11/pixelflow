import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { demoShow } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import { useApp } from "../state/store";
import { useToasts } from "../state/toast";
import { HistoryScreen } from "./HistoryScreen";

describe("history", () => {
  it("says backups aren't the saved file, and restoring one says so and can be undone", async () => {
    const backend = new MemoryBackend(demoShow());
    const older = { ...structuredClone(backend.show), name: "Older" };
    backend.history = [{ entry: { id: "h1", savedAtMs: Date.UTC(2026, 9, 1, 18), sizeBytes: 2048 }, show: older }];
    await useApp.getState().connect(backend);
    const user = userEvent.setup();
    render(<HistoryScreen />);
    expect(screen.getByText(/They aren't your saved file/)).toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: /Restore/ }));
    expect(backend.show.name).toBe("Older");
    expect(useToasts.getState().toasts.at(-1)?.text).toMatch(/^Restored the show from /);
    expect(useApp.getState().snapshot?.canUndo).toBe(true);
  });
});
