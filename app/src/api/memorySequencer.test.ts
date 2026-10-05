import { describe, expect, it } from "vitest";
import { newProp } from "../lib/shows";
import { MemoryBackend, emptyShow } from "./memory";
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

describe("MemorySequencer with a show", () => {
  async function withShow() {
    const show = emptyShow("Home");
    const strip = { ...newProp("line", show), name: "Strip" };
    show.props = [strip];
    const backend = new MemoryBackend(show);
    const seq = new MemorySequencer(backend);
    await seq.newSequenceDoc("Song", 10_000);
    const row = newRow({ prop: strip.id });
    await seq.editSequence([
      { type: "addRow", row },
      { type: "addEffect", row: row.id, layer: 0, effect: newEffect("on", 1000, 2000, ["#ff0000"]) },
    ]);
    return { backend, seq, strip };
  }

  it("draws frames of the sequence on the show's props", async () => {
    const { seq } = await withShow();
    const lit = await seq.sequenceDocFrame(1500);
    expect(Array.from(lit.slice(0, 3))).toEqual([255, 0, 0]);
    const dark = await seq.sequenceDocFrame(2500);
    expect(dark.every((v) => v === 0)).toBe(true);
  });

  it("plays on the backend's clock, and adds an export to the show's playlist", async () => {
    const { backend, seq } = await withShow();
    const status = await seq.playSequenceDoc(1200);
    expect(status).toMatchObject({ authored: true, state: "playing", durationMs: 10_000, frameMs: 25 });
    expect(Array.from((await backend.liveFrame()).slice(0, 3))).toEqual([255, 0, 0]);
    expect(await seq.setSequenceDocOutput(false)).toMatchObject({ authored: true });
    await backend.stopPlayback();
    const snap = await seq.addSequenceDocToShow("/Shows/Song.fseq");
    expect(snap.show.sequences[0]).toMatchObject({ name: "Song", path: "/Shows/Song.fseq" });
    // The same file again updates its entry; another file with the same name gets a number.
    const again = await seq.addSequenceDocToShow("/Shows/Song.fseq");
    expect(again.show.sequences).toHaveLength(1);
    expect(again.show.sequences[0].id).toBe(snap.show.sequences[0].id);
    const other = await seq.addSequenceDocToShow("/Shows/Other.fseq");
    expect(other.show.sequences[1].name).toBe("Song (2)");
  });

  it("starts a sequence with its music, and offers back kept unsaved work", async () => {
    const seq = new MemorySequencer();
    const snap = await seq.newSequenceDoc("Song", 10_000, "/music/song.mp3");
    expect(snap).toMatchObject({ dirty: false, canUndo: false, sequence: { audio: "/music/song.mp3" } });
    const kept = { ...snap.sequence, name: "Kept" };
    seq.recoveries = [{ id: "r1", name: "Kept", path: "/Shows/Kept.pfseq.json", savedAtMs: 5, doc: kept }];
    expect(await seq.sequenceRecoveries()).toEqual([{ id: "r1", name: "Kept", path: "/Shows/Kept.pfseq.json", savedAtMs: 5 }]);
    const recovered = await seq.recoverSequence("r1");
    expect(recovered).toMatchObject({ dirty: true, path: "/Shows/Kept.pfseq.json", sequence: { name: "Kept" } });
    expect(await seq.sequenceRecoveries()).toEqual([]);
    await expect(seq.recoverSequence("r1")).rejects.toThrow("That unsaved sequence isn't there anymore.");
  });
});
