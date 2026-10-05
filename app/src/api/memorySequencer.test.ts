import { describe, expect, it } from "vitest";
import { EFFECT_CATALOG, MemorySequencer, formatMs } from "./memorySequencer";
import {
  applySequenceChanges,
  defaultParams,
  newEffect,
  newRow,
  type ExportProgress,
  type Sequence,
  type SequenceEdit,
} from "./sequence";

async function authored() {
  const seq = new MemorySequencer();
  const snap = await seq.newSequenceDoc("Song", 10_000);
  const row = newRow({ prop: "p1" });
  const effect = newEffect("chase", 0, 1000);
  await seq.editSequence([
    { type: "addRow", row },
    { type: "addEffect", row: row.id, layer: 0, effect },
  ]);
  return { seq, row, effect, start: snap.sequence };
}

describe("MemorySequencer", () => {
  it("answers edits with changes that bring a copy up to date, through undo and redo", async () => {
    const { seq, row, effect, start } = await authored();
    let copy: Sequence = applySequenceChanges(start, (await seq.undoSequence()).changes);
    copy = applySequenceChanges(start, (await seq.redoSequence()).changes);
    expect(copy).toEqual((await seq.getSequenceDoc())!.sequence);
    const second = newRow({ group: "g1" });
    const batches: SequenceEdit[][] = [
      [{ type: "addRow", row: second, index: 0 }],
      [{ type: "moveEffect", id: effect.id, row: second.id, layer: 1, startMs: 0, endMs: 50 }],
      [{ type: "addLayer", row: row.id }],
      [{ type: "updateInfo", name: "Renamed", audio: "song.mp3", durationMs: 9000, frameMs: 50 }],
      [{ type: "addTimingTrack", track: { id: "t1", name: "Beats", kind: "beats", marks: [] } }],
      [{ type: "removeRow", id: row.id }],
    ];
    for (const edits of batches) {
      const reply = await seq.editSequence(edits);
      expect(reply.changed).toBe(true);
      expect(reply).not.toHaveProperty("sequence");
      copy = applySequenceChanges(copy, reply.changes);
      expect(copy).toEqual(seq.doc);
    }
    for (;;) {
      const reply = await seq.undoSequence();
      if (!reply.changed) break;
      copy = applySequenceChanges(copy, reply.changes);
      expect(copy).toEqual(seq.doc);
    }
  });

  it("merges edits with the same gesture id into one undo step", async () => {
    const { seq, effect } = await authored();
    const move = (startMs: number, gesture?: string) =>
      seq.editSequence([{ type: "setEffectTiming", id: effect.id, startMs, endMs: startMs + 1000 }], gesture);
    const revision = seq.revision;
    for (const start of [10, 20, 30]) await move(start, "drag-1");
    expect(seq.revision).toBe(revision + 3);
    expect(seq.undoStack).toHaveLength(2);
    expect((await move(30, "drag-1")).changed).toBe(false);
    await move(40, "drag-2");
    expect(seq.undoStack).toHaveLength(3);
    await seq.undoSequence();
    // An undo ends the gesture.
    await move(50, "drag-1");
    expect(seq.undoStack).toHaveLength(3);
    await seq.undoSequence();
    await seq.undoSequence();
    expect(seq.doc!.rows[0].layers[0].effects[0].startMs).toBe(0);
  });

  it("refuses settings outside the catalog's ranges like the engine", async () => {
    const { seq, effect } = await authored();
    await expect(
      seq.editSequence([{ type: "setEffectParams", id: effect.id, params: { kind: "chase", speed: Infinity } }]),
    ).rejects.toThrow(
      "The Chase effect at 0:00.000 has a setting PixelFlow can't use: Speed isn't a usable number; use 0 to 50.",
    );
    await expect(
      seq.editSequence([{ type: "setEffectParams", id: effect.id, params: { kind: "chase", bands: 0 } }]),
    ).rejects.toThrow("Bands is 0; use 1 to 1000.");
    await expect(seq.editSequence([{ type: "removeEffect", id: "nope" }])).rejects.toThrow(
      "That effect isn't in the sequence anymore.",
    );
    expect(formatMs(3_723_004)).toBe("1:02:03.004");
  });

  it("serves the engine's effect catalog and builds default settings from it", async () => {
    const seq = new MemorySequencer();
    const catalog = await seq.effectCatalog();
    expect(catalog.map((e) => e.kind)).toHaveLength(14);
    const chase = catalog.find((e) => e.kind === "chase")!;
    expect(chase.settings.find((s) => s.key === "speed")).toMatchObject({ type: "number", min: 0, max: 50, default: 1 });
    expect(defaultParams(chase)).toEqual({
      kind: "chase",
      speed: 1,
      width: 0.2,
      bands: 1,
      direction: "forward",
      bounce: false,
    });
    expect(EFFECT_CATALOG.find((e) => e.kind === "off")!.settings).toEqual([]);
  });

  it("reports export progress and can cancel an export", async () => {
    const { seq } = await authored();
    const seen: ExportProgress[] = [];
    const summary = await seq.exportSequenceDoc("/shows/song.fseq", (p) => seen.push(p));
    expect(summary.frames).toBe(400);
    expect(seen.at(-1)).toEqual({ path: "/shows/song.fseq", framesDone: 400, frames: 400, percent: 100 });
    expect(seen.length).toBeLessThanOrEqual(101);
    await expect(
      seq.exportSequenceDoc("/shows/song.fseq", (p) => {
        if (p.percent === 10) void seq.cancelSequenceExport();
      }),
    ).rejects.toThrow("The export was cancelled.");
  });

  it("adds beat tracks once per name, as one undo step", async () => {
    const { seq } = await authored();
    await expect(seq.detectBeats()).rejects.toThrow("no music yet");
    await seq.editSequence([{ type: "updateInfo", name: "Song", audio: "song.mp3", durationMs: 10_000, frameMs: 25 }]);
    await seq.detectBeats();
    const reply = await seq.detectBeats();
    expect(reply.changes.removedTimingTracks).toHaveLength(2);
    expect(seq.doc!.timingTracks.map((t) => t.name)).toEqual(["Beats", "Bars"]);
  });
});
