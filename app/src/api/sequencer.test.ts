import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

import { EFFECT_KINDS, newEffect, newRow, type SequenceEdit } from "./sequence";
import { tauriSequencer } from "./sequencer";

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
    expect(EFFECT_KINDS).toHaveLength(14);
    expect(new Set(EFFECT_KINDS.map((k) => k.kind)).size).toBe(14);
  });
});

describe("tauriSequencer", () => {
  beforeEach(() => invoke.mockReset());

  it("calls the shell's commands with camelCase arguments", async () => {
    invoke.mockResolvedValue(null);
    const edits: SequenceEdit[] = [{ type: "setEffectTiming", id: "e1", startMs: 0, endMs: 500 }];
    await tauriSequencer.newSequenceDoc("Song", 180_000);
    await tauriSequencer.editSequence(edits);
    await tauriSequencer.playSequenceDoc(2500);
    await tauriSequencer.exportSequenceDoc("/shows/song.fseq");
    await tauriSequencer.detectBeats();
    expect(invoke.mock.calls).toEqual([
      ["new_sequence_doc", { name: "Song", durationMs: 180_000 }],
      ["edit_sequence", { edits }],
      ["play_sequence_doc", { positionMs: 2500 }],
      ["export_sequence_doc", { path: "/shows/song.fseq" }],
      ["detect_beats"],
    ]);
  });

  it("returns scrub frames as bytes", async () => {
    invoke.mockResolvedValue(new Uint8Array([1, 2, 3]).buffer);
    const frame = await tauriSequencer.sequenceDocFrame(100);
    expect(Array.from(frame)).toEqual([1, 2, 3]);
    expect(invoke).toHaveBeenCalledWith("sequence_doc_frame", { positionMs: 100 });
  });
});
