import { describe, expect, it } from "vitest";
import type { Mark, Sequence, TimingTrack } from "../api/sequence";
import { followEdits, lyricChildren, lyricFamily, withOnsets } from "./lyricEdits";
import { snapTargets, snapTime } from "./timelineMath";

const mark = (startMs: number, endMs: number, label = ""): Mark => ({ startMs, endMs, label });
const track = (id: string, name: string, kind: TimingTrack["kind"], marks: Mark[]): TimingTrack => ({ id, name, kind, marks });

function song(): Sequence {
  return {
    schemaVersion: 1,
    name: "Song",
    audio: null,
    durationMs: 60_000,
    frameMs: 25,
    rows: [],
    timingTracks: [
      track("beats", "Beats", "beats", [mark(0, 500)]),
      track("lines", "Lyrics", "lyrics", [mark(1_000, 3_000, "hello there")]),
      track("words", "Lyrics (words)", "words", [mark(1_000, 2_000, "hello"), mark(2_000, 3_000, "there")]),
      track("syl", "Lyrics (syllables)", "custom", [mark(1_000, 1_400, "hel"), mark(1_400, 2_000, "lo"), mark(2_000, 3_000, "there")]),
      track("ph", "Lyrics (phonemes)", "phonemes", [mark(1_000, 1_200, "E"), mark(1_200, 1_400, "L"), mark(1_400, 2_000, "O"), mark(2_000, 3_000, "AI")]),
      track("other", "Other (words)", "words", [mark(1_000, 2_000, "else")]),
    ],
  };
}

describe("a lyrics family", () => {
  it("is found by name, each level once, and not another lyrics' tracks", () => {
    const doc = song();
    const family = lyricFamily(doc, "syl");
    expect(Object.fromEntries(Object.entries(family).map(([k, t]) => [k, t!.id]))).toEqual({ lines: "lines", words: "words", syllables: "syl", phonemes: "ph" });
    expect(lyricChildren(doc, "words").map((t) => t.id)).toEqual(["syl", "ph"]);
    expect(lyricChildren(doc, "lines").map((t) => t.id)).toEqual(["words", "syl", "ph"]);
    expect(lyricChildren(doc, "ph")).toEqual([]);
    expect(lyricChildren(doc, "beats")).toEqual([]);
    expect(lyricChildren(doc, "other")).toEqual([]);
  });
});

describe("marks below follow a moved mark", () => {
  it("moving a word moves its syllables and mouth shapes with it", () => {
    const doc = song();
    const edits = followEdits(doc, "words", [{ from: { startMs: 1_000, endMs: 2_000 }, to: { startMs: 800, endMs: 1_800 } }]);
    const byId = Object.fromEntries(edits.map((e) => (e.type === "updateTimingTrack" ? [e.track.id, e.track.marks] : [])));
    expect(byId.syl).toEqual([mark(800, 1_200, "hel"), mark(1_200, 1_800, "lo"), mark(2_000, 3_000, "there")]);
    expect(byId.ph.map((m: Mark) => [m.startMs, m.endMs])).toEqual([
      [800, 1_000],
      [1_000, 1_200],
      [1_200, 1_800],
      [2_000, 3_000],
    ]);
    expect(Object.keys(byId).sort()).toEqual(["ph", "syl"]);
  });

  it("stretching a word stretches what's under it in proportion", () => {
    const doc = song();
    const edits = followEdits(doc, "words", [
      { from: { startMs: 1_000, endMs: 2_000 }, to: { startMs: 1_000, endMs: 1_500 } },
      { from: { startMs: 2_000, endMs: 3_000 }, to: { startMs: 1_500, endMs: 3_000 } },
    ]);
    const syl = edits.find((e) => e.type === "updateTimingTrack" && e.track.id === "syl");
    expect(syl?.type === "updateTimingTrack" && syl.track.marks).toEqual([mark(1_000, 1_200, "hel"), mark(1_200, 1_500, "lo"), mark(1_500, 3_000, "there")]);
  });

  it("changes nothing for an unmoved mark or a track without a family", () => {
    const doc = song();
    expect(followEdits(doc, "words", [{ from: { startMs: 1_000, endMs: 2_000 }, to: { startMs: 1_000, endMs: 2_000 } }])).toEqual([]);
    expect(followEdits(doc, "beats", [{ from: { startMs: 0, endMs: 500 }, to: { startMs: 100, endMs: 600 } }])).toEqual([]);
  });
});

describe("snapping to the voice", () => {
  it("adds the voice's onsets to the snap targets", () => {
    const doc = song();
    const targets = withOnsets(snapTargets(doc, new Set()), [1_037, 2_000]);
    expect(targets).toContain(1_037);
    expect(targets.filter((t) => t === 2_000)).toHaveLength(1);
    expect(snapTime(1_030, targets, 10)).toEqual({ ms: 1_037, snapped: true });
    expect(withOnsets([1, 2], null)).toEqual([1, 2]);
  });
});
