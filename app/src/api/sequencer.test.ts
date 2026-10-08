import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
const listen = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: (...args: unknown[]) => listen(...args) }));

import {
  EFFECT_KINDS,
  applySequenceChanges,
  newEffect,
  newRow,
  noChanges,
  MAX_ROWS,
  rowsForShow,
  type ExportProgress,
  type Sequence,
  type SequenceEdit,
} from "./sequence";
import { EXPORT_PROGRESS_EVENT, tauriSequencer } from "./sequencer";

describe("sequence helpers", () => {
  it("build effects and rows in the engine's JSON shape", () => {
    const effect = newEffect("chase", 1000, 2000, ["#ff0000", "#0000ff"]);
    expect(effect).toMatchObject({
      startMs: 1000,
      endMs: 2000,
      params: { kind: "chase" },
      palette: { colors: ["#ff0000", "#0000ff"] },
      blend: "normal",
      fadeInMs: 0,
      fadeOutMs: 0,
    });
    expect(effect.id).toMatch(/^[0-9a-f-]{36}$/);
    const row = newRow({ group: "g1" });
    expect(row.layers).toEqual([{ effects: [] }]);
    expect(EFFECT_KINDS).toHaveLength(18);
    expect(new Set(EFFECT_KINDS.map((k) => k.kind)).size).toBe(18);
  });
});

describe("applySequenceChanges", () => {
  it("applies the engine's single-effect changes without touching the rest", () => {
    const [a, b, c] = [newEffect("on", 0, 10), newEffect("chase", 10, 20), newEffect("fire", 20, 30)];
    const first = { ...newRow({ prop: "p1" }), layers: [{ effects: [a, b, c] }] };
    const second = newRow({ prop: "p2" });
    const doc: Sequence = {
      schemaVersion: 1, name: "s", audio: null, durationMs: 1000, frameMs: 25, timingTracks: [], rows: [first, second],
    };
    // Engine reply for: move `a` to the end of its layer later in time, `c` to the other row, remove `b`.
    const moved = { ...a, startMs: 40, endMs: 50 };
    const next = applySequenceChanges(doc, {
      ...noChanges(),
      effects: [
        { row: first.id, layer: 0, index: 0, effect: moved },
        { row: second.id, layer: 0, index: 0, effect: c },
      ],
      removedEffects: [b.id],
    });
    expect(next.rows[0].layers[0].effects).toEqual([moved]);
    expect(next.rows[1].layers[0].effects).toEqual([c]);
    expect(doc.rows[0].layers[0].effects).toHaveLength(3);
    expect(next.rows[1]).not.toBe(doc.rows[1]);
    // Order and info changes.
    const reordered = applySequenceChanges(next, {
      ...noChanges(),
      rowOrder: [second.id, first.id],
      info: { name: "t", audio: "a.mp3", durationMs: 5, frameMs: 50 },
    });
    expect(reordered.rows.map((r) => r.id)).toEqual([second.id, first.id]);
    expect(reordered).toMatchObject({ name: "t", audio: "a.mp3", durationMs: 5, frameMs: 50 });
  });
});

describe("tauriSequencer", () => {
  beforeEach(() => {
    invoke.mockReset();
    listen.mockReset();
  });

  it("starts a new sequence with its rows when given", async () => {
    invoke.mockResolvedValue(null);
    const rows = rowsForShow({ groups: [{ id: "g", name: "G", members: ["p"] }], props: [] });
    await tauriSequencer.newSequenceDoc("Song", 1000, null, rows);
    expect(invoke.mock.calls).toEqual([["new_sequence_doc", { name: "Song", durationMs: 1000, audio: null, rows }]]);
    expect(rows).toEqual([{ id: expect.any(String), target: { group: "g" }, layers: [{ effects: [] }] }]);
  });

  it("rows for a show skip empty groups and stop at the most a sequence can have", () => {
    const props = Array.from({ length: 12 }, (_, i) => ({ id: `p${i}` }) as never);
    const groups = [
      { id: "empty", name: "Nothing", members: [] },
      { id: "g", name: "G", members: ["p0"] },
    ];
    expect(rowsForShow({ groups, props }).map((r) => r.target)).toEqual([{ group: "g" }, ...Array.from({ length: 12 }, (_, i) => ({ prop: `p${i}` }))]);
    expect(rowsForShow({ groups, props }, 5)).toHaveLength(5);
    expect(MAX_ROWS).toBe(10_000);
  });

  it("calls the shell's commands with camelCase arguments", async () => {
    invoke.mockResolvedValue(null);
    const edits: SequenceEdit[] = [{ type: "setEffectTiming", id: "e1", startMs: 0, endMs: 500 }];
    await tauriSequencer.newSequenceDoc("Song", 180_000, "/music/song.mp3");
    await tauriSequencer.sequenceRecoveries();
    await tauriSequencer.recoverSequence("123-4-0");
    await tauriSequencer.discardSequenceRecovery("123-4-0");
    await tauriSequencer.editSequence(edits);
    await tauriSequencer.editSequence(edits, "drag-3");
    await tauriSequencer.playSequenceDoc(2500);
    await tauriSequencer.exportSequenceDoc("/shows/song.fseq");
    await tauriSequencer.cancelSequenceExport();
    await tauriSequencer.effectCatalog();
    await tauriSequencer.detectBeats();
    await tauriSequencer.setSequenceDocOutput(false);
    await tauriSequencer.setSequenceDocLoop(true);
    await tauriSequencer.addSequenceDocToShow("/shows/song.fseq");
    await tauriSequencer.importTimingFile("/shows/Lyrics.xtiming");
    await tauriSequencer.exportTimingTrack("t1", "/shows/Lyrics.txt");
    expect(invoke.mock.calls).toEqual([
      ["new_sequence_doc", { name: "Song", durationMs: 180_000, audio: "/music/song.mp3" }],
      ["sequence_recoveries"],
      ["recover_sequence", { id: "123-4-0" }],
      ["discard_sequence_recovery", { id: "123-4-0" }],
      ["edit_sequence", { edits }],
      ["edit_sequence", { edits, gesture: "drag-3" }],
      ["play_sequence_doc", { positionMs: 2500 }],
      ["export_sequence_doc", { path: "/shows/song.fseq" }],
      ["cancel_sequence_export"],
      ["effect_catalog"],
      ["detect_beats"],
      ["set_sequence_doc_output", { send: false }],
      ["set_sequence_doc_loop", { looping: true }],
      ["add_sequence_doc_to_show", { path: "/shows/song.fseq" }],
      ["import_timing_file", { path: "/shows/Lyrics.xtiming" }],
      ["export_timing_track", { id: "t1", path: "/shows/Lyrics.txt" }],
    ]);
    expect(listen).not.toHaveBeenCalled();
  });

  it("passes export progress for this file to the callback and stops listening after", async () => {
    const unlisten = vi.fn();
    let handler: (event: { payload: ExportProgress }) => void = () => {};
    listen.mockImplementation(async (_name: string, h: typeof handler) => {
      handler = h;
      return unlisten;
    });
    invoke.mockImplementation(async () => {
      handler({ payload: { path: "/other.fseq", framesDone: 1, frames: 2, percent: 50 } });
      handler({ payload: { path: "/shows/song.fseq", framesDone: 2, frames: 2, percent: 100 } });
      return { frames: 2 };
    });
    const seen: ExportProgress[] = [];
    await tauriSequencer.exportSequenceDoc("/shows/song.fseq", (p) => seen.push(p));
    expect(listen.mock.calls[0][0]).toBe(EXPORT_PROGRESS_EVENT);
    expect(seen).toEqual([{ path: "/shows/song.fseq", framesDone: 2, frames: 2, percent: 100 }]);
    expect(unlisten).toHaveBeenCalledOnce();

    invoke.mockRejectedValue("The export was cancelled.");
    await expect(tauriSequencer.exportSequenceDoc("/shows/song.fseq", () => {})).rejects.toBe(
      "The export was cancelled.",
    );
    expect(unlisten).toHaveBeenCalledTimes(2);
  });

  it("returns scrub frames as bytes", async () => {
    invoke.mockResolvedValue(new Uint8Array([1, 2, 3]).buffer);
    const frame = await tauriSequencer.sequenceDocFrame(100);
    expect(Array.from(frame)).toEqual([1, 2, 3]);
    expect(invoke).toHaveBeenCalledWith("sequence_doc_frame", { positionMs: 100 });
  });

  it("asks the shell for every file dialog, by kind", async () => {
    invoke.mockResolvedValue("/shows/Caf\u0000e9.pfseq.json");
    expect(await tauriSequencer.pickSequenceDocSavePath("Song.pfseq.json")).toBe("/shows/Caf\u0000e9.pfseq.json");
    expect(invoke).toHaveBeenCalledWith("pick_path", { kind: "sequenceDocSave", name: "Song.pfseq.json" });
    invoke.mockResolvedValue(null);
    expect(await tauriSequencer.pickXlightsSequencePath()).toBeNull();
    expect(invoke).toHaveBeenLastCalledWith("pick_path", { kind: "xlightsSequence" });
  });
});
