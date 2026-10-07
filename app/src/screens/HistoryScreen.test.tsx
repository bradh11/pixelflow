import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { demoShow } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import { useApp } from "../state/store";
import { useToasts } from "../state/toast";
import { HISTORY_REFRESH_MS, HistoryScreen } from "./HistoryScreen";

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

  it("lists backups made in the background, and after a save, without an edit", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      const backend = new MemoryBackend(demoShow());
      await useApp.getState().connect(backend);
      render(<HistoryScreen />);
      expect(await screen.findByText("No backups yet")).toBeInTheDocument();
      // The autosave every 30 seconds changes nothing the screen watches.
      const copy = { ...structuredClone(backend.show), name: "Copy" };
      backend.history = [{ entry: { id: "h1", savedAtMs: Date.UTC(2026, 9, 1, 18), sizeBytes: 2048 }, show: copy }];
      await act(() => vi.advanceTimersByTimeAsync(HISTORY_REFRESH_MS));
      expect(screen.getAllByRole("button", { name: /Restore/ })).toHaveLength(1);
      // A save moves the backups to the saved file's folder.
      backend.history = [...backend.history, { entry: { id: "h2", savedAtMs: Date.UTC(2026, 9, 1, 19), sizeBytes: 2048 }, show: copy }];
      await act(() => useApp.getState().run((b) => b.saveShowAs("/Shows/house.pixelflow.json")));
      await waitFor(() => expect(screen.getAllByRole("button", { name: /Restore/ })).toHaveLength(2));
    } finally {
      vi.useRealTimers();
    }
  });

  it("with no backups yet, a button goes back to work", async () => {
    await useApp.getState().connect(new MemoryBackend(demoShow()));
    const user = userEvent.setup();
    render(<HistoryScreen />);
    await user.click(await screen.findByRole("button", { name: "Go to Layout" }));
    expect(useApp.getState().screen).toBe("layout");
  });
});
