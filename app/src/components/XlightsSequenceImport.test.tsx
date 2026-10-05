import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { demoShow } from "../api/demo";
import { MemoryBackend } from "../api/memory";
import { MemorySequencer } from "../api/memorySequencer";
import { demoSequence } from "../api/demoSequence";
import { unsavedWork } from "../state/closeGuard";
import { useSequencer } from "../state/sequencer";
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

/** The one "Save changes to …?" question (New, Open, Recover, and the import all use it). */
async function askedAbout(name: string) {
  const ask = await screen.findByRole("dialog", { name: "Unsaved changes" });
  expect(ask).toHaveTextContent(`Save changes to ${name}?`);
  expect(screen.getAllByRole("dialog")).toHaveLength(1);
  return ask;
}

async function startApp() {
  await useApp.getState().connect(new MemoryBackend(demoShow()));
  const seq = sequencer();
  await useSequencer.getState().connect(seq);
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
    expect(within(report).getByText(/It's open on the Sequence screen as an unsaved sequence/)).toBeInTheDocument();
    expect(within(report).getByText(/600 exact · 200 approximated · 40 placeholders · 5 not imported/)).toBeInTheDocument();
    expect(within(report).getByText(/Faces \(40\)/)).toBeInTheDocument();
    const open = await seq.getSequenceDoc();
    expect(open).toMatchObject({ path: null, dirty: true, canUndo: false, sequence: { name: "Carol of the Bells" } });
    // Open in the timeline's state, on the Sequence screen.
    expect(useSequencer.getState()).toMatchObject({ path: null, dirty: true, doc: { name: "Carol of the Bells" } });
    expect(useApp.getState().screen).toBe("sequence");
    await user.click(within(report).getByRole("button", { name: "Done" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("runs from the Sequence screen", async () => {
    const { user, seq } = await startApp();
    act(() => useApp.getState().setScreen("sequence"));
    await user.click(await screen.findByRole("button", { name: /Import an xLights sequence/ }));
    await screen.findByRole("dialog", { name: "Imported Carol of the Bells" });
    await user.click(screen.getByRole("button", { name: "Done" }));
    // Now the toolbar has the action too; the imported sequence is unsaved, so it asks first.
    await user.click(screen.getByRole("button", { name: "Import xLights sequence…" }));
    await askedAbout("Carol of the Bells");
    expect(seq.calls.filter((c) => c.startsWith("importXlightsSequence"))).toHaveLength(1);
  });

  it("asks once when a sequence with unsaved changes is open, then treats the import as the open unsaved work", async () => {
    const { user, seq } = await startApp();
    // A saved sequence, then an unsaved change to it.
    const show = useApp.getState().snapshot!.show;
    seq.files.set("/Shows/Medley.pfseq.json", demoSequence(show, 60_000));
    await act(() => useSequencer.getState().open("/Shows/Medley.pfseq.json"));
    await act(() => useSequencer.getState().edit((doc) => [{ type: "removeRow", id: doc.rows[0].id }]));
    const medley = useSequencer.getState().doc!.name;
    expect(unsavedWork().sequence).toBe(medley);
    act(() => useApp.getState().setScreen("sequence"));
    const imports = () => seq.calls.filter((c) => c.startsWith("importXlightsSequence")).length;

    // From the toolbar: one question (not a second, app-wide one), and nothing imported yet.
    await user.click(await screen.findByRole("button", { name: "Import xLights sequence…" }));
    const ask = await askedAbout(medley);
    expect(imports()).toBe(0);
    expect(useApp.getState().pendingReplace).toBeNull();
    await user.click(within(ask).getByRole("button", { name: "Don't save" }));
    await screen.findByRole("dialog", { name: "Imported Carol of the Bells" });
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    expect(imports()).toBe(1);
    expect(seq.files.get("/Shows/Medley.pfseq.json")!.rows.length).toBe(demoSequence(show, 60_000).rows.length);

    // The import is now the open, unsaved sequence: closing asks about it, and the engine keeps it
    // (autosaves it) like any unsaved sequence until it's saved.
    expect(useSequencer.getState()).toMatchObject({ path: null, dirty: true, doc: { name: "Carol of the Bells" } });
    expect(unsavedWork().sequence).toBe("Carol of the Bells");
    expect(await seq.getSequenceDoc()).toMatchObject({ dirty: true, path: null });
  });

  it("asks before a second import replaces an unsaved one, and can save it first", async () => {
    const { user, seq } = await startApp();
    expect(await act(() => useApp.getState().importXlightsSequence())).toBe(true);
    act(() => useApp.getState().dismissSequenceImportReport());
    const imports = () => seq.calls.filter((c) => c.startsWith("importXlightsSequence")).length;

    // Cancel: nothing changes.
    expect(await act(() => useApp.getState().importXlightsSequence())).toBe(false);
    let ask = await askedAbout("Carol of the Bells");
    await user.click(within(ask).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(imports()).toBe(1);

    // Don't save: the import goes ahead.
    await act(() => useApp.getState().importXlightsSequence());
    ask = await askedAbout("Carol of the Bells");
    await user.click(within(ask).getByRole("button", { name: "Don't save" }));
    await screen.findByRole("dialog", { name: "Imported Carol of the Bells" });
    expect(imports()).toBe(2);
    act(() => useApp.getState().dismissSequenceImportReport());

    // Save: asks where (it has no file yet), saves, then imports.
    seq.nextSavePath = "/Shows/Carol.pfseq.json";
    await act(() => useApp.getState().importXlightsSequence());
    ask = await askedAbout("Carol of the Bells");
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
