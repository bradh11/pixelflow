import { describe, expect, it } from "vitest";
import type { Mark } from "../api/sequence";
import { HOLD_MS, finishTapSession, nextMark, pressTap, releaseTap, startTapSession } from "./tapTiming";

const mark = (startMs: number, endMs: number, label: string): Mark => ({ startMs, endMs, label });
const words = [mark(500, 900, "oh"), mark(1_000, 1_400, "say"), mark(1_400, 1_800, "can"), mark(2_000, 2_500, "you"), mark(3_000, 3_400, "see")];

describe("tap timing", () => {
  it("times the marks from where it starts, in order", () => {
    let s = startTapSession(words, 1_000);
    expect(nextMark(s)).toBe(1);
    s = releaseTap(pressTap(s, 1_050), 1_080);
    expect(nextMark(s)).toBe(2);
    s = releaseTap(pressTap(s, 1_500), 1_520);
    const done = finishTapSession(s, words, 10_000)!;
    // "say" ends where "can" starts; "can" keeps its length; the rest stay put.
    expect(done.marks.map((m) => [m.label, m.startMs, m.endMs])).toEqual([
      ["oh", 500, 900],
      ["say", 1_050, 1_500],
      ["can", 1_500, 1_900],
      ["you", 2_000, 2_500],
      ["see", 3_000, 3_400],
    ]);
    expect(done.moves).toEqual([
      { from: { startMs: 1_000, endMs: 1_400 }, to: { startMs: 1_050, endMs: 1_500 } },
      { from: { startMs: 1_400, endMs: 1_800 }, to: { startMs: 1_500, endMs: 1_900 } },
    ]);
  });

  it("holding the key sets the end where it's let go", () => {
    let s = startTapSession(words, 1_000);
    s = releaseTap(pressTap(s, 1_000), 1_000 + HOLD_MS + 20);
    s = releaseTap(pressTap(s, 1_600), 1_610);
    const done = finishTapSession(s, words, 10_000)!;
    expect(done.marks[1]).toEqual(mark(1_000, 1_200, "say"));
    expect(done.marks[2]).toEqual(mark(1_600, 2_000, "can"));
  });

  it("a key held down takes no second press, and nothing comes after the last mark", () => {
    let s = startTapSession(words, 3_000);
    s = pressTap(s, 3_100);
    expect(pressTap(s, 3_150)).toBe(s);
    s = releaseTap(s, 3_120);
    expect(nextMark(s)).toBeNull();
    expect(pressTap(s, 3_300)).toBe(s);
    expect(finishTapSession(s, words, 10_000)!.marks[4]).toEqual(mark(3_100, 3_500, "see"));
  });

  it("moves the rest later when the taps run into them, and ends the mark before by the first", () => {
    let s = startTapSession(words, 1_000);
    s = releaseTap(pressTap(s, 850), 860);
    s = releaseTap(pressTap(s, 2_100), 2_110);
    const done = finishTapSession(s, words, 10_000)!;
    expect(done.marks.map((m) => [m.startMs, m.endMs])).toEqual([
      [500, 850],
      [850, 2_100],
      [2_100, 2_101],
      [2_101, 2_601],
      [3_101, 3_501],
    ]);
  });

  it("finishing without a tap changes nothing", () => {
    expect(finishTapSession(startTapSession(words, 0), words, 10_000)).toBeNull();
  });
});
