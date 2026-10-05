import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { demoShow } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import { MemorySequencer } from "../api/memorySequencer";
import { useApp } from "../state/store";

function sequencer(): MemorySequencer {
  const sequencer = new MemorySequencer();
  sequencer.nextXlightsSequencePath = "/Shows/Carol.xsq";
  sequencer.xlightsSequenceImport = {
    sequence: {
      schemaVersion: 1,
      name: "Carol of the Bells",
      audio: null,
      durationMs: 180_000,
      frameMs: 25,
      timingTracks: [],
      rows: [],
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
      marksSkipped: 2,
    },
    notes: ["PixelFlow has no matching effect yet for this xLights effect: Faces (40)."],
  };
  return sequencer;
}

async function startApp() {
  await useApp.getState().connect(new MemoryBackend(demoShow()));
  const seq = sequencer();
  useApp.getState().connectSequencer(seq);
  useApp.setState({ started: true, sequenceImportReport: null, error: null });
  const user = userEvent.setup();
  render(<App />);
  return { seq, user };
}

describe("xLights sequence import", () => {
  it("runs from the command palette, opens the sequence unsaved, and reports what came in", async () => {
    const { user, seq } = await startApp();
    act(() => useApp.getState().setPaletteOpen(true));
    await user.click(await screen.findByRole("option", { name: "Import xLights sequence…" }));
    expect(seq.calls).toContain("importXlightsSequence:/Shows/Carol.xsq");
    const report = await screen.findByRole("dialog", { name: "Imported Carol of the Bells" });
    expect(
      within(report).getByText(/12 rows · 840 effects · 3 timing tracks · 1,234 marks \(2 not imported\)/),
    ).toBeInTheDocument();
    expect(within(report).getByText(/It's open as an unsaved sequence/)).toBeInTheDocument();
    expect(within(report).getByText(/600 exact · 200 approximated · 40 placeholders · 5 not imported/)).toBeInTheDocument();
    expect(within(report).getByText(/Faces \(40\)/)).toBeInTheDocument();
    const open = await seq.getSequenceDoc();
    expect(open).toMatchObject({ path: null, dirty: true, canUndo: false, sequence: { name: "Carol of the Bells" } });
    await user.click(within(report).getByRole("button", { name: "Done" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("asks before a second import replaces an unsaved one, and can save it first", async () => {
    const { user, seq } = await startApp();
    expect(await act(() => useApp.getState().importXlightsSequence())).toBe(true);
    act(() => useApp.getState().dismissSequenceImportReport());
    const imports = () => seq.calls.filter((c) => c.startsWith("importXlightsSequence")).length;

    // Cancel: nothing changes.
    expect(await act(() => useApp.getState().importXlightsSequence())).toBe(false);
    let ask = await screen.findByRole("dialog", { name: "Save changes to Carol of the Bells?" });
    await user.click(within(ask).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(imports()).toBe(1);

    // Don't save: the import goes ahead.
    await act(() => useApp.getState().importXlightsSequence());
    ask = await screen.findByRole("dialog", { name: "Save changes to Carol of the Bells?" });
    await user.click(within(ask).getByRole("button", { name: "Don't save" }));
    await screen.findByRole("dialog", { name: "Imported Carol of the Bells" });
    expect(imports()).toBe(2);
    act(() => useApp.getState().dismissSequenceImportReport());

    // Save: asks where (it has no file yet), saves, then imports.
    seq.nextSavePath = "/Shows/Carol.pfseq.json";
    await act(() => useApp.getState().importXlightsSequence());
    ask = await screen.findByRole("dialog", { name: "Save changes to Carol of the Bells?" });
    await user.click(within(ask).getByRole("button", { name: "Save" }));
    await screen.findByRole("dialog", { name: "Imported Carol of the Bells" });
    expect(seq.calls).toContain("saveSequenceDocAs");
    expect(seq.files.has("/Shows/Carol.pfseq.json")).toBe(true);
    expect(imports()).toBe(3);
  });

  it("shows a failed import as an error and does nothing when cancelled", async () => {
    const { seq } = await startApp();
    seq.xlightsSequenceImport = null;
    expect(await act(() => useApp.getState().importXlightsSequence())).toBe(false);
    expect(useApp.getState().error).toBe("Could not read /Shows/Carol.xsq: no such file");
    expect(await seq.getSequenceDoc()).toBeNull();
    seq.nextXlightsSequencePath = null;
    const calls = seq.calls.length;
    expect(await act(() => useApp.getState().importXlightsSequence())).toBe(false);
    expect(seq.calls.length).toBe(calls);
    expect(useApp.getState().sequenceImportReport).toBeNull();
  });
});
