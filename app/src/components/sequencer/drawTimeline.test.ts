import { describe, expect, it } from "vitest";
import type { Effect, Sequence } from "../../api/sequence";
import { buildIndex, layoutLanes, rulerTicks } from "../../lib/timelineMath";
import { LANE_H, RULER_H, TRACK_H, VOCALS_H, WAVE_H, drawTimeline, topHeight } from "./drawTimeline";

/** A 2D context stand-in that remembers the text and rectangles drawn. */
function recorder() {
  const texts: { text: string; x: number }[] = [];
  const rects: { x: number; y: number; w: number; h: number }[] = [];
  const ctx = new Proxy({} as Record<string | symbol, unknown>, {
    get(target, prop) {
      if (prop === "fillText") return (text: string, x: number) => texts.push({ text, x });
      if (prop === "fillRect") return (x: number, y: number, w: number, h: number) => rects.push({ x, y, w, h });
      return prop in target ? target[prop] : () => undefined;
    },
    set(target, prop, value) {
      target[prop] = value;
      return true;
    },
  });
  return { ctx: ctx as unknown as CanvasRenderingContext2D, texts, rects };
}

const effect = (startMs: number, endMs: number): Effect => ({
  id: "e1",
  startMs,
  endMs,
  params: { kind: "chase" } as Effect["params"],
  palette: { colors: [] },
  blend: "normal",
  fadeInMs: 0,
  fadeOutMs: 0,
});

function scene(doc: Sequence, view: { startMs: number; pxPerMs: number }, waveform: { durationMs: number; peaks: number[] } | null = null) {
  const { lanes } = layoutLanes(doc.rows, new Set(), LANE_H);
  return {
    width: 1000,
    height: 300,
    theme: "dark" as const,
    doc,
    index: buildIndex(doc),
    lanes,
    view,
    scrollY: 0,
    selection: new Set<string>(),
    playheadMs: 0,
    waveform,
    labels: new Map([["chase", "Chase"]]),
    drag: null,
    snappedAt: null,
    marquee: null,
    ghost: null,
  };
}

const doc = (effects: Effect[], durationMs = 60_000): Sequence => ({
  schemaVersion: 1,
  name: "Song",
  audio: null,
  durationMs,
  frameMs: 25,
  timingTracks: [],
  rows: [{ id: "r1", target: { prop: "p1" }, layers: [{ effects }] }],
});

describe("drawing the timeline", () => {
  it("keeps an effect's name in view when the effect starts left of it", () => {
    const { ctx, texts } = recorder();
    // The effect runs 0–20 s; the view starts at 10 s.
    drawTimeline(ctx, scene(doc([effect(0, 20_000)]), { startMs: 10_000, pxPerMs: 0.05 }));
    expect(texts.find((t) => t.text === "Chase")?.x).toBe(5);
  });

  it("fills the music between values when zoomed in past one value per pixel", () => {
    const { ctx, rects } = recorder();
    // One value per 10 ms at 1 px per ms: each value is 10 px wide, with no gaps.
    const peaks = Array.from({ length: 6000 }, () => 0.5);
    drawTimeline(ctx, scene(doc([]), { startMs: 0, pxPerMs: 1 }, { durationMs: 60_000, peaks }));
    const bars = rects.filter((r) => r.y > RULER_H && r.y + r.h < RULER_H + WAVE_H && r.h > 2);
    expect(bars.length).toBeGreaterThan(90);
    expect(bars.every((b) => b.w === 10)).toBe(true);
    // Zoomed out, one bar per pixel.
    const out = recorder();
    drawTimeline(out.ctx, scene(doc([]), { startMs: 0, pxPerMs: 1000 / 60_000 }, { durationMs: 60_000, peaks }));
    expect(out.rects.filter((r) => r.y > RULER_H && r.y + r.h < RULER_H + WAVE_H && r.h > 2).every((b) => b.w === 1)).toBe(true);
  });

  it("draws the timing marks in view as spans with their labels, the selected and dragged ones lit", () => {
    const { ctx, texts, rects } = recorder();
    const marks = Array.from({ length: 600 }, (_, i) => ({ startMs: i * 100, endMs: i * 100 + 100, label: `w${i}` }));
    const sequence: Sequence = { ...doc([]), timingTracks: [{ id: "t", name: "Words", kind: "words", marks }] };
    // 1 px per ms: 1000 px show 1–2 s, marks 10 through 19.
    drawTimeline(ctx, {
      ...scene(sequence, { startMs: 1000, pxPerMs: 1 }),
      markSelection: { track: "t", starts: new Set([1200]) },
      markDrag: { track: "t", spans: [{ index: 15, startMs: 1550, endMs: 1650 }] },
      activeTrack: "t",
    });
    const labels = texts.map((t) => t.text).filter((t) => t.startsWith("w"));
    expect(labels).toEqual(["w10", "w11", "w12", "w13", "w14", "w15", "w16", "w17", "w18", "w19", "w15"]);
    // Each mark's span fills its strip (2 px in, under the music band).
    const strip = rects.filter((r) => r.y === RULER_H + WAVE_H + 2 && r.w === 100);
    expect(strip.map((r) => r.x)).toEqual([0, 100, 200, 300, 400, 500, 600, 700, 800, 900, 550]);
  });

  it("draws the vocals lane under the timing tracks, with a tick at each onset", () => {
    const { ctx, rects } = recorder();
    const sequence: Sequence = { ...doc([]), timingTracks: [{ id: "t", name: "Words", kind: "words", marks: [] }] };
    expect(topHeight(sequence, true)).toBe(topHeight(sequence) + VOCALS_H);
    // One value per 10 ms, loud from 1.0 to 1.2 s; at 1 px per ms each is 10 px wide.
    const levels = Array.from({ length: 6000 }, (_, i) => (i >= 100 && i < 120 ? 255 : 0));
    drawTimeline(ctx, { ...scene(sequence, { startMs: 1000, pxPerMs: 1 }), vocals: { lane: { hopMs: 10, offsetMs: 0, levels, onsets: [1000, 1500] } } });
    const laneTop = RULER_H + WAVE_H + TRACK_H;
    const inLane = rects.filter((r) => r.y > laneTop && r.y + r.h <= laneTop + VOCALS_H);
    const bars = inLane.filter((r) => r.h > 10);
    expect(bars.map((b) => b.x)).toEqual(Array.from({ length: 20 }, (_, i) => i * 10));
    expect(bars.every((b) => b.w === 10)).toBe(true);
    const ticks = inLane.filter((r) => r.w === 1 && r.h === 5);
    expect(ticks.map((r) => r.x)).toEqual([0, 500]);
    // Shown before it's worked out: an empty lane, nothing in it.
    const empty = recorder();
    drawTimeline(empty.ctx, { ...scene(sequence, { startMs: 1000, pxPerMs: 1 }), vocals: { lane: null } });
    expect(empty.rects.filter((r) => r.y > laneTop + 1 && r.y + r.h <= laneTop + VOCALS_H && r.h > 2)).toEqual([]);
  });

  it("labels the ruler readably for long sequences", () => {
    // Four hours in 1000 px.
    const ticks = rulerTicks({ startMs: 0, pxPerMs: 1000 / 14_400_000 }, 1000);
    expect(ticks.major.length).toBeGreaterThan(1);
    const gaps = ticks.major.slice(1).map((t, i) => t.x - ticks.major[i].x);
    expect(Math.min(...gaps)).toBeGreaterThanOrEqual(80);
    expect(ticks.major.length).toBeLessThanOrEqual(13);
  });
});
