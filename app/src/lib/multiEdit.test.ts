import { describe, expect, it } from "vitest";
import { type Effect, type Sequence, newEffect } from "../api/sequence";
import { effectsById, lengthEdits, shared, sharedKind, updateEach } from "./multiEdit";
import { shiftEdits } from "./timelineMath";

function doc(layers: Effect[][], durationMs = 10_000): Sequence {
  return {
    schemaVersion: 2,
    name: "s",
    audio: null,
    durationMs,
    frameMs: 25,
    timingTracks: [],
    rows: [{ id: "row", target: { prop: "p" }, layers: layers.map((effects) => ({ effects })) }],
  };
}

const fx = (id: string, kind: Effect["params"]["kind"], startMs: number, endMs: number, extra: Partial<Effect> = {}): Effect => ({
  ...newEffect(kind, startMs, endMs),
  id,
  ...extra,
});

describe("editing several effects at once", () => {
  it("finds the selected effects in the document's order", () => {
    const d = doc([[fx("a", "on", 0, 100), fx("b", "chase", 200, 300)], [fx("c", "chase", 0, 50)]]);
    expect(effectsById(d, ["c", "a", "gone"]).map((e) => e.id)).toEqual(["a", "c"]);
  });

  it("says whether a value is the same on all of them, or mixed", () => {
    expect(shared([3, 3, 3])).toEqual({ mixed: false, value: 3 });
    expect(shared([3, 4])).toEqual({ mixed: true, value: 3 });
    expect(shared([["#ff0000"], ["#ff0000"]])).toEqual({ mixed: false, value: ["#ff0000"] });
    expect(shared([["#ff0000"], ["#00ff00"]]).mixed).toBe(true);
  });

  it("knows their kind only when they're all the same kind", () => {
    expect(sharedKind([fx("a", "chase", 0, 1), fx("b", "chase", 1, 2)])).toBe("chase");
    expect(sharedKind([fx("a", "chase", 0, 1), fx("b", "wave", 1, 2)])).toBeNull();
    expect(sharedKind([])).toBeNull();
  });

  it("changes each effect as it is now, sending only the ones that change", () => {
    const d = doc([[fx("a", "on", 0, 100, { blend: "add" }), fx("b", "chase", 200, 300)]]);
    const edits = updateEach(d, ["a", "b"], (e) => ({ ...e, blend: "add" }));
    expect(edits).toEqual([{ type: "updateEffect", effect: { ...d.rows[0].layers[0].effects[1], blend: "add" } }]);
    expect(updateEach(d, ["a"], () => null)).toEqual([]);
  });

  it("moves them all by the same amount, stopping at the song's ends and at effects that aren't moving", () => {
    const d = doc([[fx("a", "on", 1000, 2000), fx("b", "on", 3000, 4000), fx("wall", "on", 5000, 6000)]]);
    // Together they move freely past each other's places.
    expect(shiftEdits(d, ["a", "b"], 500)).toEqual({
      deltaMs: 500,
      edits: [
        { type: "setEffectTiming", id: "a", startMs: 1500, endMs: 2500 },
        { type: "setEffectTiming", id: "b", startMs: 3500, endMs: 4500 },
      ],
    });
    // b stops at the wall, and a moves no further than b can.
    expect(shiftEdits(d, ["a", "b"], 5000).deltaMs).toBe(1000);
    expect(shiftEdits(d, ["a", "b"], -5000).deltaMs).toBe(-1000);
    // Already against the wall: nothing to do.
    const stuck = doc([[fx("a", "on", 0, 1000)]]);
    expect(shiftEdits(stuck, ["a"], -200)).toEqual({ deltaMs: 0, edits: [] });
    expect(shiftEdits(d, ["a"], 12.4).deltaMs, "whole milliseconds").toBe(12);
  });

  it("sets every length, stopping at the next effect or the song's end and keeping fades inside", () => {
    const d = doc(
      [[fx("a", "on", 0, 1000, { fadeInMs: 800, fadeOutMs: 200 }), fx("b", "on", 1500, 2000), fx("c", "on", 9000, 9500)]],
      10_000,
    );
    const edits = lengthEdits(d, ["a", "b", "c"], 600);
    expect(edits.map((e) => (e.type === "updateEffect" ? [e.effect.id, e.effect.startMs, e.effect.endMs, e.effect.fadeInMs, e.effect.fadeOutMs] : null))).toEqual([
      ["a", 0, 600, 600, 200],
      ["b", 1500, 2100, 0, 0],
      ["c", 9000, 9600, 0, 0],
    ]);
    // Longer: a stops at b, c at the end of the song.
    const longer = lengthEdits(d, ["a", "c"], 4000);
    expect(longer.map((e) => (e.type === "updateEffect" ? [e.effect.startMs, e.effect.endMs] : null))).toEqual([
      [0, 1500],
      [9000, 10_000],
    ]);
    // Never shorter than a frame.
    expect(lengthEdits(d, ["b"], 0).map((e) => (e.type === "updateEffect" ? e.effect.endMs : null))).toEqual([1525]);
    // Less than a frame of room: it stops at the next effect or the song's end all the same.
    const tight = doc([[fx("x", "on", 0, 10), fx("wall", "on", 20, 100), fx("last", "on", 9990, 9995)]], 10_000);
    expect(lengthEdits(tight, ["x", "last"], 0).map((e) => (e.type === "updateEffect" ? [e.effect.id, e.effect.endMs] : null))).toEqual([
      ["x", 20],
      ["last", 10_000],
    ]);
    // Already that long: nothing to send.
    expect(lengthEdits(d, ["b"], 500)).toEqual([]);
  });
});
