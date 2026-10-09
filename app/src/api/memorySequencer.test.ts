import { describe, expect, it, vi } from "vitest";
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
  type TimingTrack,
} from "./sequence";
import timingCases from "./timingEditCases.json";

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
    expect(catalog.map((e) => e.kind)).toHaveLength(29);
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

  it("adds beat tracks once per name, keeping sections, accents, and moments already there, as one undo step", async () => {
    const { seq } = await authored();
    await expect(seq.detectBeats()).rejects.toThrow("no music yet");
    await seq.editSequence([{ type: "updateInfo", name: "Song", audio: "song.mp3", durationMs: 10_000, frameMs: 25 }]);
    await seq.detectBeats();
    expect(seq.doc!.timingTracks.map((t) => t.name)).toEqual(["Beats", "Bars", "Sections", "Accents", "Moments", "Drums"]);
    const moments = seq.doc!.timingTracks[4];
    expect(moments.marks.map((m) => m.label)).toEqual(["Drop: Chorus", "Impact"]);
    expect(seq.doc!.timingTracks[5].marks.map((m) => m.label)).toEqual(["Crash"]);
    const sections = seq.doc!.timingTracks[2];
    const reply = await seq.detectBeats();
    expect(reply.changes.removedTimingTracks).toHaveLength(3);
    expect(seq.doc!.timingTracks.map((t) => t.name)).toEqual(["Sections", "Accents", "Moments", "Beats", "Bars", "Drums"]);
    expect(seq.doc!.timingTracks[0]).toEqual(sections);
    expect(seq.doc!.timingTracks[2]).toEqual(moments);
  });
});

interface TimingCase {
  name: string;
  tracks: TimingTrack[];
  edits: SequenceEdit[];
  expect: { tracks: TimingTrack[] } | { error: string };
}

describe("MemorySequencer timing tracks", () => {
  // The same table is run against the engine by the desktop shell's tests.
  it.each((timingCases as unknown as TimingCase[]).map((c) => [c.name, c] as const))("matches the engine: %s", async (_name, c) => {
    const seq = new MemorySequencer();
    await seq.newSequenceDoc("Song", 60_000);
    if (c.tracks.length > 0) await seq.editSequence(c.tracks.map((track) => ({ type: "addTimingTrack" as const, track })));
    const before = structuredClone(seq.doc);
    if ("error" in c.expect) {
      await expect(seq.editSequence(c.edits)).rejects.toThrow(c.expect.error);
      expect(seq.doc).toEqual(before);
    } else {
      await seq.editSequence(c.edits);
      expect(seq.doc!.timingTracks).toEqual(c.expect.tracks);
      // One undo step.
      await seq.undoSequence();
      expect(seq.doc).toEqual(before);
    }
  });

  it("imports timing files with names of their own, and exports a lyrics track with its words", async () => {
    const seq = new MemorySequencer();
    await seq.newSequenceDoc("Song", 60_000);
    const lyrics: TimingTrack = { id: "l", name: "Vocals", kind: "lyrics", marks: [{ startMs: 0, endMs: 900, label: "Hi there" }] };
    const words: TimingTrack = { id: "w", name: "Vocals (words)", kind: "words", marks: [{ startMs: 0, endMs: 400, label: "Hi" }] };
    seq.timingFiles.set("/t/Vocals.xtiming", { tracks: [lyrics, words], notes: ["1 mark was left out."] });
    const first = await seq.importTimingFile("/t/Vocals.xtiming");
    expect(first).toMatchObject({ tracks: ["Vocals", "Vocals (words)"], notes: ["1 mark was left out."] });
    expect(first.result.changes.timingTracks).toHaveLength(2);
    const again = await seq.importTimingFile("/t/Vocals.xtiming");
    // The number goes on the shared name, so the layers stay paired.
    expect(again.tracks).toEqual(["Vocals 2", "Vocals 2 (words)"]);
    await seq.undoSequence();
    await expect(seq.importTimingFile("/t/missing.xtiming")).rejects.toThrow("Could not read /t/missing.xtiming");

    const id = seq.doc!.timingTracks[0].id;
    expect(await seq.exportTimingTrack(id, "/t/out.xtiming")).toBe(1);
    expect(seq.exportedTimingFiles.get("/t/out.xtiming")!.map((t) => t.name)).toEqual(["Vocals", "Vocals (words)"]);
    await seq.exportTimingTrack(id, "/t/out.txt");
    expect(seq.exportedTimingFiles.get("/t/out.txt")!.map((t) => t.name)).toEqual(["Vocals"]);
    await expect(seq.exportTimingTrack("gone", "/t/x.txt")).rejects.toThrow("That timing track isn't in the sequence anymore.");
    await expect(seq.exportTimingTrack(id, "/t/evil.sh")).rejects.toThrow("Timing tracks are saved as xLights timing files (.xtiming) or Audacity labels (.txt).");
  });

  it("doesn't add a file's tracks to a sequence opened while it was read", async () => {
    const seq = new MemorySequencer();
    await seq.newSequenceDoc("Song", 60_000);
    seq.timingFiles.set("/t/a.txt", { tracks: [{ id: "a", name: "a", kind: "custom", marks: [] }], notes: [] });
    seq.analysisDelayMs = 5;
    const reading = seq.importTimingFile("/t/a.txt");
    await seq.newSequenceDoc("Other", 1000);
    await expect(reading).rejects.toThrow("Another sequence was opened while the timing file was being read. Import it again.");
    expect(seq.doc!.timingTracks).toEqual([]);
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

  it("loops on the backend's clock when asked, from the top again at the end", async () => {
    const { backend, seq } = await withShow();
    const now = vi.spyOn(Date, "now").mockReturnValue(100_000);
    expect(await seq.setSequenceDocLoop(true)).toBeNull();
    const status = await seq.playSequenceDoc(9_000);
    expect(status).toMatchObject({ state: "playing", looping: true });
    now.mockReturnValue(101_500);
    expect(await backend.playbackStatus()).toMatchObject({ state: "playing", positionMs: 500, looping: true });
    expect(Array.from((await backend.liveFrame()).slice(0, 3))).toEqual([0, 0, 0]);
    now.mockReturnValue(102_000);
    expect(Array.from((await backend.liveFrame()).slice(0, 3))).toEqual([255, 0, 0]);
    // Turned off while playing: it plays out to the end this time round.
    expect(await seq.setSequenceDocLoop(false)).toMatchObject({ looping: false, positionMs: 1000 });
    now.mockReturnValue(111_000);
    expect(await backend.playbackStatus()).toMatchObject({ state: "ended", positionMs: 10_000 });
    expect(seq.calls).toContain("setSequenceDocLoop:true");
    now.mockRestore();
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
