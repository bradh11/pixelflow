import { describe, expect, it } from "vitest";
import type { Effect, Row, Sequence } from "../api/sequence";
import {
  buildIndex,
  clampView,
  createSpan,
  effectsInView,
  fitInLane,
  fitView,
  followPlayhead,
  formatTime,
  freeLayer,
  hitEffect,
  laneAt,
  layoutLanes,
  marqueeSelect,
  moveDrag,
  moveEdits,
  pasteEffects,
  planDrop,
  resizeDrag,
  rulerTicks,
  snapTargets,
  snapTime,
  stepTime,
  timeToX,
  toggleSelection,
  xToTime,
  zoomAt,
} from "./timelineMath";

function fx(id: string, startMs: number, endMs: number): Effect {
  return { id, startMs, endMs, params: { kind: "on" }, palette: { colors: ["#ff0000"] }, blend: "normal", fadeInMs: 0, fadeOutMs: 0 };
}

function doc(rows: Row[], extra: Partial<Sequence> = {}): Sequence {
  return { schemaVersion: 1, name: "Song", audio: null, durationMs: 60_000, frameMs: 25, timingTracks: [], rows, ...extra };
}

const rowA: Row = { id: "A", target: { prop: "p1" }, layers: [{ effects: [fx("a1", 0, 1000), fx("a2", 2000, 3000)] }, { effects: [fx("a3", 500, 1500)] }] };
const rowB: Row = { id: "B", target: { prop: "p2" }, layers: [{ effects: [fx("b1", 4000, 6000)] }] };

describe("time and pixels", () => {
  it("convert both ways around the view's left edge", () => {
    const view = { startMs: 1000, pxPerMs: 0.1 };
    expect(timeToX(1000, view)).toBe(0);
    expect(timeToX(3000, view)).toBe(200);
    expect(xToTime(200, view)).toBe(3000);
    expect(xToTime(-50, view)).toBe(500);
  });

  it("fits the song to the width and keeps the view inside the song", () => {
    expect(fitView(60_000, 600)).toEqual({ startMs: 0, pxPerMs: 0.01 });
    expect(clampView({ startMs: -10, pxPerMs: 0.01 }, 60_000, 600).startMs).toBe(0);
    // 600 px at 0.1 px/ms shows 6 s; the latest start is 54 s.
    expect(clampView({ startMs: 59_000, pxPerMs: 0.1 }, 60_000, 600).startMs).toBe(54_000);
    // Zoom is limited both ways.
    expect(clampView({ startMs: 0, pxPerMs: 100 }, 60_000, 600).pxPerMs).toBe(2);
    expect(clampView({ startMs: 0, pxPerMs: 1e-9 }, 60_000, 600).pxPerMs).toBeCloseTo(0.01);
  });

  it("zooms around the pointer so the time under it stays put", () => {
    const view = { startMs: 10_000, pxPerMs: 0.05 };
    const under = xToTime(300, view);
    const zoomed = zoomAt(view, 2, 300, 60_000, 600);
    expect(zoomed.pxPerMs).toBe(0.1);
    expect(xToTime(300, zoomed)).toBeCloseTo(under);
  });

  it("follows the playhead a page at a time while playing", () => {
    const view = { startMs: 0, pxPerMs: 0.1 }; // shows 0–6 s in 600 px
    expect(followPlayhead(view, 3000, 600, 60_000)).toBe(view);
    expect(followPlayhead(view, 6500, 600, 60_000).startMs).toBe(6500 - 600 * 0.1 * 10);
    expect(followPlayhead({ startMs: 20_000, pxPerMs: 0.1 }, 1000, 600, 60_000).startMs).toBeLessThanOrEqual(1000);
  });

  it("formats times as m:ss.mmm", () => {
    expect(formatTime(0)).toBe("0:00.000");
    expect(formatTime(65_250)).toBe("1:05.250");
    expect(formatTime(3_725_004)).toBe("62:05.004");
    expect(formatTime(65_250, 1000)).toBe("1:05");
    expect(formatTime(65_250, 100)).toBe("1:05.3");
  });

  it("chooses ruler steps that stay readable at any zoom", () => {
    const ticks = rulerTicks({ startMs: 0, pxPerMs: 0.01 }, 600);
    // At 10 px per second, labels are 10 s apart (100 px).
    expect(ticks.major.slice(0, 3).map((t) => t.ms)).toEqual([0, 10_000, 20_000]);
    expect(ticks.major[1].label).toBe("0:10");
    expect(ticks.minor.length).toBeGreaterThan(ticks.major.length);
    const fine = rulerTicks({ startMs: 1000, pxPerMs: 1 }, 500);
    expect(fine.major[0].ms).toBe(1000);
    expect(fine.major[1].ms - fine.major[0].ms).toBe(100);
    expect(fine.major[1].label).toBe("0:01.1");
  });
});

describe("snapping", () => {
  const sequence = doc([rowA, rowB], {
    timingTracks: [{ id: "t", name: "Beats", kind: "beats", marks: [{ startMs: 500, endMs: 1000, label: "1" }, { startMs: 1000, endMs: 1500, label: "2" }] }],
  });

  it("collects timing marks and other effects' edges, leaving out the dragged ones", () => {
    const targets = snapTargets(sequence, new Set(["a2"]));
    expect(targets).toEqual([0, 500, 1000, 1500, 4000, 6000]);
  });

  it("snaps to the nearest target within the threshold", () => {
    const targets = [0, 500, 1000];
    expect(snapTime(980, targets, 30)).toEqual({ ms: 1000, snapped: true });
    expect(snapTime(940, targets, 30)).toEqual({ ms: 940, snapped: false });
    expect(snapTime(10, [], 30)).toEqual({ ms: 10, snapped: false });
  });
});

describe("rows, lanes, and effects", () => {
  const sequence = doc([rowA, rowB]);
  const index = buildIndex(sequence);

  it("lays rows out as one lane per layer, or one lane when collapsed", () => {
    const { lanes, height } = layoutLanes(sequence.rows, new Set(), 30);
    expect(lanes.map((l) => [l.rowId, l.layer, l.y])).toEqual([
      ["A", 0, 0],
      ["A", 1, 30],
      ["B", 0, 60],
    ]);
    expect(height).toBe(90);
    const collapsed = layoutLanes(sequence.rows, new Set(["A"]), 30);
    expect(collapsed.lanes.map((l) => [l.rowId, l.layer])).toEqual([
      ["A", -1],
      ["B", 0],
    ]);
    expect(laneAt(lanes, 45)?.layer).toBe(1);
    expect(laneAt(lanes, 95)).toBeNull();
  });

  it("finds the effects in view quickly, and which part of one the pointer is on", () => {
    const { lanes } = layoutLanes(sequence.rows, new Set(), 30);
    const view = { startMs: 0, pxPerMs: 0.1 };
    expect(effectsInView(index, lanes[0], 1500, 2500).map((e) => e.id)).toEqual(["a2"]);
    expect(effectsInView(index, lanes[0], 0, 60_000).map((e) => e.id)).toEqual(["a1", "a2"]);
    expect(hitEffect(index, lanes[0], 250, view)).toEqual({ id: "a2", part: "body" });
    expect(hitEffect(index, lanes[0], 201, view)).toEqual({ id: "a2", part: "start" });
    expect(hitEffect(index, lanes[0], 298, view)).toEqual({ id: "a2", part: "end" });
    expect(hitEffect(index, lanes[0], 150, view)).toBeNull();
    // A collapsed lane shows every layer; the top layer wins where they overlap.
    const collapsed = layoutLanes(sequence.rows, new Set(["A"]), 30).lanes[0];
    expect(hitEffect(index, collapsed, 80, view)?.id).toBe("a3");
  });

  it("selects with clicks, modifier clicks, and a marquee", () => {
    expect([...toggleSelection(new Set(["a1"]), "a2", false)]).toEqual(["a2"]);
    expect([...toggleSelection(new Set(["a1"]), "a2", true)]).toEqual(["a1", "a2"]);
    expect([...toggleSelection(new Set(["a1", "a2"]), "a2", true)]).toEqual(["a1"]);
    const { lanes } = layoutLanes(sequence.rows, new Set(), 30);
    const view = { startMs: 0, pxPerMs: 0.1 };
    // From 0.9 s to 4.5 s, over the first two lanes.
    expect(marqueeSelect(index, lanes, view, { x0: 90, y0: 5, x1: 450, y1: 50 }).sort()).toEqual(["a1", "a2", "a3"]);
    expect(marqueeSelect(index, lanes, view, { x0: 450, y0: 89, x1: 90, y1: 61 })).toEqual(["b1"]);
  });
});

describe("dragging", () => {
  const sequence = doc([rowA, rowB]);
  const { lanes } = layoutLanes(sequence.rows, new Set(), 30);

  it("moves selected effects together, snapping the grabbed one and staying in the song", () => {
    const items = [
      { id: "a1", startMs: 0, endMs: 1000, lane: 0 },
      { id: "a3", startMs: 500, endMs: 1500, lane: 1 },
    ];
    const moved = moveDrag({ items, primary: "a1", deltaMs: 1980, deltaLanes: 0, laneCount: 3, durationMs: 60_000, snap: { targets: [2000, 5000], thresholdMs: 50 } });
    expect(moved.items.map((i) => [i.startMs, i.endMs])).toEqual([
      [2000, 3000],
      [2500, 3500],
    ]);
    expect(moved.snappedAt).toBe(2000);
    // The end edge can snap too.
    const byEnd = moveDrag({ items, primary: "a1", deltaMs: 3990, deltaLanes: 0, laneCount: 3, durationMs: 60_000, snap: { targets: [5000], thresholdMs: 50 } });
    expect(byEnd.items[0]).toMatchObject({ startMs: 4000, endMs: 5000 });
    // Never before 0 or past the end, nor off the lanes.
    const clamped = moveDrag({ items, primary: "a1", deltaMs: -5000, deltaLanes: 5, laneCount: 3, durationMs: 60_000 });
    expect(clamped.items.map((i) => [i.startMs, i.lane])).toEqual([
      [0, 1],
      [500, 2],
    ]);
    expect(moveDrag({ items, primary: "a1", deltaMs: 70_000, deltaLanes: 0, laneCount: 3, durationMs: 60_000 }).items[1].endMs).toBe(60_000);
  });

  it("turns a move into timing edits, or moves to another row or layer", () => {
    const edits = moveEdits(
      [
        { id: "a1", startMs: 100, endMs: 1100, lane: 0 },
        { id: "a3", startMs: 600, endMs: 1600, lane: 2 },
      ],
      [
        { id: "a1", startMs: 0, endMs: 1000, lane: 0 },
        { id: "a3", startMs: 500, endMs: 1500, lane: 1 },
      ],
      lanes,
    );
    expect(edits).toEqual([
      { type: "setEffectTiming", id: "a1", startMs: 100, endMs: 1100 },
      { type: "moveEffect", id: "a3", row: "B", layer: 0, startMs: 600, endMs: 1600 },
    ]);
  });

  it("resizes from either edge, snapping and keeping at least one frame", () => {
    const item = { id: "a2", startMs: 2000, endMs: 3000, lane: 0 };
    expect(resizeDrag({ item, edge: "end", ms: 3490, minMs: 25, durationMs: 60_000, snap: { targets: [3500], thresholdMs: 20 } })).toEqual({ startMs: 2000, endMs: 3500, snappedAt: 3500 });
    expect(resizeDrag({ item, edge: "start", ms: 2990, minMs: 25, durationMs: 60_000 })).toEqual({ startMs: 2975, endMs: 3000, snappedAt: null });
    expect(resizeDrag({ item, edge: "start", ms: -100, minMs: 25, durationMs: 60_000 }).startMs).toBe(0);
    expect(resizeDrag({ item, edge: "end", ms: 99_999, minMs: 25, durationMs: 60_000 }).endMs).toBe(60_000);
  });

  it("creates a new effect one bar long, or two seconds without bars", () => {
    const bars = [
      { startMs: 0, endMs: 2000, label: "1" },
      { startMs: 2000, endMs: 4100, label: "2" },
    ];
    expect(createSpan({ ms: 2010, durationMs: 60_000, frameMs: 25, bars, snap: { targets: [2000], thresholdMs: 50 } })).toEqual({ startMs: 2000, endMs: 4100 });
    expect(createSpan({ ms: 9000, durationMs: 60_000, frameMs: 25, bars })).toEqual({ startMs: 9000, endMs: 11_000 });
    expect(createSpan({ ms: 59_000, durationMs: 60_000, frameMs: 25 })).toEqual({ startMs: 59_000, endMs: 60_000 });
    expect(createSpan({ ms: 60_000, durationMs: 60_000, frameMs: 25 })).toEqual({ startMs: 59_975, endMs: 60_000 });
  });

  it("fits a new effect into the gap it was dropped in", () => {
    const effects = [fx("x", 1000, 2000), fx("y", 3000, 4000)];
    expect(fitInLane(effects, 2200, 2800, 25)).toEqual({ startMs: 2200, endMs: 2800 });
    expect(fitInLane(effects, 2200, 4200, 25)).toEqual({ startMs: 2200, endMs: 3000 });
    expect(fitInLane(effects, 1500, 1800, 25)).toBeNull();
    expect(freeLayer(rowA, 500, 900)).toBe(2);
    expect(freeLayer(rowA, 1600, 1900)).toBe(0);
  });
});

describe("dropping from the palette", () => {
  const sequence = doc([rowA, rowB], {
    timingTracks: [{ id: "bars", name: "Bars", kind: "bars", marks: [{ startMs: 0, endMs: 4000, label: "1" }, { startMs: 4000, endMs: 8000, label: "2" }] }],
  });
  const index = buildIndex(sequence);
  const { lanes } = layoutLanes(sequence.rows, new Set(), 30);

  it("fills the gap it lands in, up to a bar", () => {
    // Row A, layer 0 has 0–1 s and 2–3 s: dropped at 1.2 s, it stops where the next one starts.
    expect(planDrop({ doc: sequence, index, lane: lanes[0], ms: 1200 })).toEqual({ rowId: "A", layer: 0, startMs: 1200, endMs: 2000 });
    expect(planDrop({ doc: sequence, index, lane: lanes[2], ms: 6500 })).toEqual({ rowId: "B", layer: 0, startMs: 6500, endMs: 10_500 });
  });

  it("goes on a layer with room when dropped on another effect or a collapsed row", () => {
    expect(planDrop({ doc: sequence, index, lane: lanes[0], ms: 500 })).toEqual({ rowId: "A", layer: 2, startMs: 500, endMs: 4500 });
    const collapsed = layoutLanes(sequence.rows, new Set(["B"]), 30).lanes[2];
    expect(planDrop({ doc: sequence, index, lane: collapsed, ms: 4100 })?.layer).toBe(1);
  });
});

describe("thousands of effects", () => {
  it("index, draw queries, and hit tests stay well inside a frame", () => {
    const rows: Row[] = Array.from({ length: 30 }, (_, r) => ({
      id: `r${r}`,
      target: { prop: `p${r}` },
      layers: [{ effects: Array.from({ length: 100 }, (_, i) => fx(`e${r}-${i}`, i * 3000, i * 3000 + 2500)) }],
    }));
    const big = doc(rows, { durationMs: 300_000 });
    let t = performance.now();
    const index = buildIndex(big);
    const indexMs = performance.now() - t;
    const { lanes } = layoutLanes(big.rows, new Set(), 30);
    const view = { startMs: 60_000, pxPerMs: 0.02 };
    t = performance.now();
    let drawn = 0;
    for (let k = 0; k < 10; k++) for (const lane of lanes) drawn += effectsInView(index, lane, view.startMs, view.startMs + 1200 / view.pxPerMs).length;
    const queryMs = (performance.now() - t) / 10;
    t = performance.now();
    for (let k = 0; k < 1000; k++) hitEffect(index, lanes[k % 30], (k * 7) % 1200, view);
    const hitMs = (performance.now() - t) / 1000;
    expect(drawn / 10).toBe(30 * 20);
    expect(indexMs).toBeLessThan(100);
    expect(queryMs).toBeLessThan(8);
    expect(hitMs).toBeLessThan(1);
  });
});

describe("keyboard and clipboard", () => {
  it("steps by a frame, or to the next or previous beat", () => {
    const beats = [0, 500, 1000, 1500];
    expect(stepTime(700, 1, { frameMs: 25 })).toBe(725);
    expect(stepTime(10, -1, { frameMs: 25 })).toBe(0);
    expect(stepTime(700, 1, { frameMs: 25, beats }, true)).toBe(1000);
    expect(stepTime(700, -1, { frameMs: 25, beats }, true)).toBe(500);
    expect(stepTime(1500, 1, { frameMs: 25, beats }, true)).toBe(2000);
  });

  it("pastes copies at the playhead on their own rows, on a free layer", () => {
    const sequence = doc([rowA, rowB]);
    const index = buildIndex(sequence);
    let n = 0;
    const copies = [
      { rowId: "A", effect: fx("a2", 2000, 3000) },
      { rowId: "B", effect: fx("b1", 4000, 6000) },
    ];
    const edits = pasteEffects(sequence, index, copies, 500, () => `new${++n}`);
    expect(edits).toEqual([
      { type: "addEffect", row: "A", layer: 2, effect: { ...fx("new1", 500, 1500) } },
      { type: "addEffect", row: "B", layer: 1, effect: { ...fx("new2", 2500, 4500) } },
    ]);
    expect(pasteEffects(sequence, index, [{ rowId: "gone", effect: fx("x", 0, 10) }], 0)).toEqual([]);
  });
});
