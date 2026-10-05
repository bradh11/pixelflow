import { describe, expect, it } from "vitest";
import type { Mark, TimingTrack } from "./sequence";
import { addMarks, overlapWith, splitWords } from "./timingMarks";

describe("timing marks", () => {
  it("adds many marks quickly, in order", () => {
    // 100k marks there and 100k new ones in the gaps, back to front: one at a time took minutes.
    const marks: Mark[] = Array.from({ length: 100_000 }, (_, i) => ({ startMs: i * 20, endMs: i * 20 + 10, label: "" }));
    const track: TimingTrack = { id: "t", name: "Beats", kind: "beats", marks };
    const added: Mark[] = Array.from({ length: 100_000 }, (_, i) => ({ startMs: (99_999 - i) * 20 + 10, endMs: (99_999 - i) * 20 + 20, label: "x" }));
    const started = performance.now();
    addMarks(track, added);
    expect(performance.now() - started).toBeLessThan(2000);
    expect(track.marks).toHaveLength(200_000);
    expect(track.marks.every((m, i) => i === 0 || track.marks[i - 1].endMs <= m.startMs)).toBe(true);
    expect(track.marks[1]).toEqual({ startMs: 10, endMs: 20, label: "x" });
    // Only a moved mark's neighbors are looked at.
    expect(overlapWith(track.marks, { startMs: 15, endMs: 25 }, [1])).toBe(2);
    expect(overlapWith(track.marks, { startMs: 12, endMs: 18 }, [1])).toBe(-1);
  });

  it("keeps punctuation with a word, taking no time of its own", () => {
    const split = (label: string) => splitWords({ startMs: 0, endMs: 1600, label }).map((m) => [m.startMs, m.endMs, m.label]);
    expect(split("— Deck the halls - fa la,")).toEqual([
      [0, 400, "— Deck"],
      [400, 700, "the"],
      [700, 1200, "halls -"],
      [1200, 1400, "fa"],
      [1400, 1600, "la,"],
    ]);
    expect(split("- … !")).toEqual([]);
    // Letters as the engine counts them (vowel signs too), so both split alike.
    expect(split("नमस्ते a")).toEqual([
      [0, 1333, "नमस्ते"],
      [1333, 1600, "a"],
    ]);
  });
});
