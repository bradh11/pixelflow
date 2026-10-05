import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { demoShow } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import type { XlightsSequenceImported } from "../api/types";
import { useApp } from "../state/store";

function imported(): XlightsSequenceImported {
  return {
    snapshot: {
      revision: 1,
      path: null,
      dirty: true,
      canUndo: false,
      canRedo: false,
      sequence: {
        schemaVersion: 1,
        name: "Carol of the Bells",
        audio: null,
        durationMs: 180_000,
        frameMs: 25,
        timingTracks: [],
        rows: [],
      },
      issues: [],
    },
    summary: {
      rows: 12,
      effects: 840,
      exact: 600,
      approximate: 200,
      placeholders: 40,
      skipped: 5,
      timingTracks: 3,
      marks: 1234,
      lyricMarks: 300,
    },
    notes: ["PixelFlow has no matching effect yet for this xLights effect: Faces (40)."],
  };
}

async function startApp() {
  const backend = new MemoryBackend(demoShow());
  backend.nextXlightsSequencePath = "/Shows/Carol.xsq";
  backend.xlightsSequenceImport = imported();
  await useApp.getState().connect(backend);
  useApp.setState({ started: true, sequenceImportReport: null, error: null });
  const user = userEvent.setup();
  render(<App />);
  return { backend, user };
}

describe("xLights sequence import", () => {
  it("runs from the command palette and reports what came in", async () => {
    const { user, backend } = await startApp();
    act(() => useApp.getState().setPaletteOpen(true));
    await user.click(await screen.findByRole("option", { name: "Import xLights sequence…" }));
    expect(backend.calls).toContain("importXlightsSequence:/Shows/Carol.xsq");
    const report = await screen.findByRole("dialog", { name: "Imported Carol of the Bells" });
    expect(within(report).getByText(/12 rows · 840 effects · 3 timing tracks · 1,234 marks/)).toBeInTheDocument();
    expect(within(report).getByText(/600 exact · 200 approximated · 40 placeholders · 5 not imported/)).toBeInTheDocument();
    expect(within(report).getByText(/Faces \(40\)/)).toBeInTheDocument();
    await user.click(within(report).getByRole("button", { name: "Done" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows a failed import as an error and does nothing when cancelled", async () => {
    const { backend } = await startApp();
    backend.xlightsSequenceImport = null;
    expect(await act(() => useApp.getState().importXlightsSequence())).toBe(false);
    expect(useApp.getState().error).toBe("Could not read /Shows/Carol.xsq.");
    backend.nextXlightsSequencePath = null;
    const calls = backend.calls.length;
    expect(await act(() => useApp.getState().importXlightsSequence())).toBe(false);
    expect(backend.calls.length).toBe(calls);
    expect(useApp.getState().sequenceImportReport).toBeNull();
  });
});
