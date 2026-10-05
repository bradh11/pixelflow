// Pure timeline math: time ↔ pixels, zoom, ruler ticks, snapping, lanes, hit testing, selection,
// and the edits that drags, pastes, and the keyboard turn into. No React, no engine calls.

import type { Effect, Mark, Row, Sequence, SequenceEdit } from "../api/sequence";

/** What part of the song the timeline shows: the time at its left edge and the zoom. */
export interface View {
  startMs: number;
  pxPerMs: number;
}

/** Most zoomed in: 2 px per millisecond (a 25 ms frame is 50 px wide). */
export const MAX_PX_PER_MS = 2;
/** How close (in pixels) an edge has to come to a mark to snap to it. */
export const SNAP_PX = 8;

export function timeToX(ms: number, view: View): number {
  return (ms - view.startMs) * view.pxPerMs;
}

export function xToTime(x: number, view: View): number {
  return view.startMs + x / view.pxPerMs;
}

/** The whole song in `width` pixels. */
export function fitView(durationMs: number, width: number): View {
  return { startMs: 0, pxPerMs: Math.max(width, 1) / Math.max(durationMs, 1) };
}

/** Keeps the zoom between "whole song" and the maximum, and the view inside the song. */
export function clampView(view: View, durationMs: number, width: number): View {
  const fit = fitView(durationMs, width).pxPerMs;
  const pxPerMs = Math.min(MAX_PX_PER_MS, Math.max(fit, Number.isFinite(view.pxPerMs) ? view.pxPerMs : fit));
  const visible = Math.max(width, 1) / pxPerMs;
  const startMs = Math.max(0, Math.min(view.startMs, Math.max(0, durationMs - visible)));
  return { startMs, pxPerMs };
}

/** Zooms by `factor` keeping the time under `anchorX` where it is. */
export function zoomAt(view: View, factor: number, anchorX: number, durationMs: number, width: number): View {
  const anchor = xToTime(anchorX, view);
  const pxPerMs = clampView({ startMs: 0, pxPerMs: view.pxPerMs * factor }, durationMs, width).pxPerMs;
  return clampView({ startMs: anchor - anchorX / pxPerMs, pxPerMs }, durationMs, width);
}

/** While playing: when the playhead leaves the view, turn the page so it's near the left again. */
export function followPlayhead(view: View, ms: number, width: number, durationMs: number): View {
  const visible = Math.max(width, 1) / view.pxPerMs;
  if (ms >= view.startMs && ms <= view.startMs + visible) return view;
  return clampView({ ...view, startMs: ms - visible * 0.1 }, durationMs, width);
}

/** 65250 → "1:05.250"; with a coarser `stepMs` (the ruler's spacing), fewer decimals. */
export function formatTime(ms: number, stepMs = 1): string {
  const decimals = stepMs >= 1000 ? 0 : stepMs >= 100 ? 1 : stepMs >= 10 ? 2 : 3;
  const unit = 10 ** (3 - decimals);
  const units = Math.round(Math.max(0, ms) / unit);
  const perSecond = 1000 / unit;
  const totalSeconds = Math.floor(units / perSecond);
  const fraction = units % perSecond;
  const text = `${Math.floor(totalSeconds / 60)}:${String(totalSeconds % 60).padStart(2, "0")}`;
  return decimals === 0 ? text : `${text}.${String(fraction).padStart(decimals, "0")}`;
}

const NICE_STEPS = [10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10_000, 15_000, 30_000, 60_000, 120_000, 300_000, 600_000];
/** Minor ticks per major tick, by the major step's leading digit. */
const MINOR_DIVISIONS: Record<string, number> = { "1": 5, "2": 4, "5": 5, "1.5": 3, "3": 3, "6": 6 };

export interface RulerTicks {
  major: { ms: number; x: number; label: string }[];
  minor: { ms: number; x: number }[];
}

/** Labelled ticks at least ~80 px apart, with minor ticks between. */
export function rulerTicks(view: View, width: number): RulerTicks {
  const major = NICE_STEPS.find((s) => s * view.pxPerMs >= 80) ?? NICE_STEPS[NICE_STEPS.length - 1];
  const leading = major / 10 ** Math.floor(Math.log10(major));
  const minor = major / (MINOR_DIVISIONS[String(leading)] ?? 5);
  const end = xToTime(width, view);
  const ticks: RulerTicks = { major: [], minor: [] };
  for (let ms = Math.ceil(view.startMs / minor) * minor; ms <= end; ms += minor) {
    const at = Math.round(ms);
    if (at % major === 0) ticks.major.push({ ms: at, x: timeToX(at, view), label: formatTime(at, major) });
    else ticks.minor.push({ ms: at, x: timeToX(at, view) });
  }
  return ticks;
}

// --- Snapping ---------------------------------------------------------------------------------

/** Every time an edge may snap to: timing marks, other effects' edges, and the start. */
export function snapTargets(doc: Sequence, exclude: ReadonlySet<string>): number[] {
  const times = new Set<number>([0]);
  for (const track of doc.timingTracks) {
    for (const mark of track.marks) {
      times.add(mark.startMs);
      times.add(mark.endMs);
    }
  }
  for (const row of doc.rows) {
    for (const layer of row.layers) {
      for (const e of layer.effects) {
        if (exclude.has(e.id)) continue;
        times.add(e.startMs);
        times.add(e.endMs);
      }
    }
  }
  return [...times].sort((a, b) => a - b);
}

/** First index in sorted `values` whose value is >= `x`. */
function lowerBound(values: ArrayLike<number>, x: number): number {
  let lo = 0;
  let hi = values.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (values[mid] < x) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

export interface Snap {
  targets: number[];
  thresholdMs: number;
}

/** The nearest target within `thresholdMs`, or `ms` itself. */
export function snapTime(ms: number, targets: number[], thresholdMs: number): { ms: number; snapped: boolean } {
  const i = lowerBound(targets, ms);
  let best: number | null = null;
  for (const j of [i - 1, i]) {
    if (j < 0 || j >= targets.length) continue;
    if (best === null || Math.abs(targets[j] - ms) < Math.abs(best - ms)) best = targets[j];
  }
  return best !== null && Math.abs(best - ms) <= thresholdMs ? { ms: best, snapped: true } : { ms, snapped: false };
}

// --- Rows, lanes, and the effect index --------------------------------------------------------

/** One horizontal strip of the timeline: a layer of a row, or a whole collapsed row (`layer` -1). */
export interface Lane {
  rowId: string;
  rowIndex: number;
  layer: number;
  /** Top, from the top of the rows area. */
  y: number;
  h: number;
  /** The row's first lane (where its name goes). */
  first: boolean;
  layerCount: number;
}

export function layoutLanes(rows: Row[], collapsed: ReadonlySet<string>, laneHeight: number): { lanes: Lane[]; height: number } {
  const lanes: Lane[] = [];
  let y = 0;
  rows.forEach((row, rowIndex) => {
    const layerCount = row.layers.length;
    const layers = collapsed.has(row.id) ? [-1] : Array.from({ length: Math.max(1, layerCount) }, (_, i) => i);
    layers.forEach((layer, i) => {
      lanes.push({ rowId: row.id, rowIndex, layer, y, h: laneHeight, first: i === 0, layerCount });
      y += laneHeight;
    });
  });
  return { lanes, height: y };
}

/** The lane at `y` (rows area coordinates), or null. */
export function laneAt(lanes: Lane[], y: number): Lane | null {
  let lo = 0;
  let hi = lanes.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const lane = lanes[mid];
    if (y < lane.y) hi = mid - 1;
    else if (y >= lane.y + lane.h) lo = mid + 1;
    else return lane;
  }
  return null;
}

/** The lane showing a row's layer (its collapsed lane when the row is collapsed). */
export function laneOf(lanes: Lane[], rowId: string, layer: number): number {
  return lanes.findIndex((l) => l.rowId === rowId && (l.layer === layer || l.layer === -1));
}

export interface Placed {
  effect: Effect;
  rowId: string;
  rowIndex: number;
  layer: number;
}

interface LayerIndex {
  /** Effects by start time. */
  effects: Effect[];
  starts: Float64Array;
  /** The longest effect, to find ones that started before the view but still show. */
  maxLength: number;
}

/** Fast lookups for drawing and hit testing; rebuilt when the document changes. */
export interface EffectIndex {
  byId: Map<string, Placed>;
  rows: Map<string, Row>;
  layers: Map<string, LayerIndex>;
}

export function buildIndex(doc: Sequence): EffectIndex {
  const index: EffectIndex = { byId: new Map(), rows: new Map(), layers: new Map() };
  doc.rows.forEach((row, rowIndex) => {
    index.rows.set(row.id, row);
    row.layers.forEach((layer, l) => {
      const effects = [...layer.effects].sort((a, b) => a.startMs - b.startMs);
      let maxLength = 0;
      for (const effect of effects) {
        index.byId.set(effect.id, { effect, rowId: row.id, rowIndex, layer: l });
        maxLength = Math.max(maxLength, effect.endMs - effect.startMs);
      }
      index.layers.set(`${row.id}:${l}`, { effects, starts: Float64Array.from(effects, (e) => e.startMs), maxLength });
    });
  });
  return index;
}

function visibleInLayer(layer: LayerIndex | undefined, t0: number, t1: number, out: Effect[]) {
  if (!layer) return;
  for (let i = lowerBound(layer.starts, t0 - layer.maxLength); i < layer.effects.length; i++) {
    const e = layer.effects[i];
    if (e.startMs >= t1) break;
    if (e.endMs > t0) out.push(e);
  }
}

/** Effects in a lane that show between `t0` and `t1`; for a collapsed lane, bottom layer first. */
export function effectsInView(index: EffectIndex, lane: Lane, t0: number, t1: number): Effect[] {
  const out: Effect[] = [];
  if (lane.layer >= 0) {
    visibleInLayer(index.layers.get(`${lane.rowId}:${lane.layer}`), t0, t1, out);
  } else {
    for (let l = 0; l < lane.layerCount; l++) visibleInLayer(index.layers.get(`${lane.rowId}:${l}`), t0, t1, out);
  }
  return out;
}

export type EffectPart = "body" | "start" | "end";

/** The effect (and which part: an edge, to resize, or the body, to move) at `x` in a lane. */
export function hitEffect(index: EffectIndex, lane: Lane, x: number, view: View, edgePx = 6): { id: string; part: EffectPart } | null {
  const t = xToTime(x, view);
  const candidates = effectsInView(index, lane, t - 1 / view.pxPerMs, t + 1 / view.pxPerMs);
  for (let i = candidates.length - 1; i >= 0; i--) {
    const e = candidates[i];
    const x0 = timeToX(e.startMs, view);
    const x1 = timeToX(e.endMs, view);
    if (x < x0 || x > x1) continue;
    const edge = Math.min(edgePx, (x1 - x0) / 3);
    if (x - x0 <= edge) return { id: e.id, part: "start" };
    if (x1 - x <= edge) return { id: e.id, part: "end" };
    return { id: e.id, part: "body" };
  }
  return null;
}

// --- Selection --------------------------------------------------------------------------------

/** A click selects one effect; with Shift or ⌘ (`additive`) it adds or removes it. */
export function toggleSelection(selection: ReadonlySet<string>, id: string, additive: boolean): Set<string> {
  if (!additive) return new Set([id]);
  const next = new Set(selection);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}

/** Effects touched by a marquee (rows area coordinates; any corner order). */
export function marqueeSelect(
  index: EffectIndex,
  lanes: Lane[],
  view: View,
  rect: { x0: number; y0: number; x1: number; y1: number },
): string[] {
  const [y0, y1] = [Math.min(rect.y0, rect.y1), Math.max(rect.y0, rect.y1)];
  const t0 = xToTime(Math.min(rect.x0, rect.x1), view);
  const t1 = xToTime(Math.max(rect.x0, rect.x1), view);
  const ids: string[] = [];
  for (const lane of lanes) {
    if (lane.y + lane.h <= y0 || lane.y >= y1) continue;
    for (const e of effectsInView(index, lane, t0, t1)) ids.push(e.id);
  }
  return ids;
}

// --- Dragging ---------------------------------------------------------------------------------

/** An effect being dragged: where it is (or would go) in time and lane. */
export interface DragItem {
  id: string;
  startMs: number;
  endMs: number;
  lane: number;
}

/** Moves every dragged effect by the same amount: the grabbed one (`primary`) snaps by either
 * edge, and none goes before the start, past the end, or off the lanes. */
export function moveDrag(args: {
  items: DragItem[];
  primary: string;
  deltaMs: number;
  deltaLanes: number;
  laneCount: number;
  durationMs: number;
  snap?: Snap;
}): { items: DragItem[]; snappedAt: number | null } {
  const { items, primary, laneCount, durationMs, snap } = args;
  const lo = -Math.min(...items.map((i) => i.startMs));
  const hi = durationMs - Math.max(...items.map((i) => i.endMs));
  const clamp = (d: number) => Math.max(lo, Math.min(hi, d));
  let delta = clamp(Math.round(args.deltaMs));
  let snappedAt: number | null = null;
  const grabbed = items.find((i) => i.id === primary);
  if (snap && grabbed) {
    const byStart = snapTime(grabbed.startMs + delta, snap.targets, snap.thresholdMs);
    const byEnd = snapTime(grabbed.endMs + delta, snap.targets, snap.thresholdMs);
    const startGap = Math.abs(byStart.ms - (grabbed.startMs + delta));
    const endGap = Math.abs(byEnd.ms - (grabbed.endMs + delta));
    const pick = byStart.snapped && (!byEnd.snapped || startGap <= endGap) ? "start" : byEnd.snapped ? "end" : null;
    if (pick) {
      const target = pick === "start" ? byStart.ms : byEnd.ms;
      const snappedDelta = target - (pick === "start" ? grabbed.startMs : grabbed.endMs);
      if (clamp(snappedDelta) === snappedDelta) {
        delta = snappedDelta;
        snappedAt = target;
      }
    }
  }
  const minLane = Math.min(...items.map((i) => i.lane));
  const maxLane = Math.max(...items.map((i) => i.lane));
  const dl = Math.max(-minLane, Math.min(laneCount - 1 - maxLane, args.deltaLanes));
  return {
    items: items.map((i) => ({ ...i, startMs: i.startMs + delta, endMs: i.endMs + delta, lane: i.lane + dl })),
    snappedAt,
  };
}

/** The edits for a finished move: new times, plus a new row or layer for effects that changed lane. */
export function moveEdits(moved: DragItem[], original: DragItem[], lanes: Lane[]): SequenceEdit[] {
  const edits: SequenceEdit[] = [];
  moved.forEach((item, i) => {
    const before = original[i];
    if (item.lane === before.lane) {
      if (item.startMs !== before.startMs || item.endMs !== before.endMs) {
        edits.push({ type: "setEffectTiming", id: item.id, startMs: item.startMs, endMs: item.endMs });
      }
      return;
    }
    const lane = lanes[item.lane];
    edits.push({ type: "moveEffect", id: item.id, row: lane.rowId, layer: Math.max(0, lane.layer), startMs: item.startMs, endMs: item.endMs });
  });
  return edits;
}

/** Drags one edge of an effect to `ms` (snapped), keeping at least `minMs` and staying in the song. */
export function resizeDrag(args: {
  item: DragItem;
  edge: "start" | "end";
  ms: number;
  minMs: number;
  durationMs: number;
  snap?: Snap;
}): { startMs: number; endMs: number; snappedAt: number | null } {
  const { item, edge, minMs, durationMs, snap } = args;
  const snapped = snap ? snapTime(args.ms, snap.targets, snap.thresholdMs) : { ms: args.ms, snapped: false };
  const t = Math.round(snapped.ms);
  if (edge === "end") {
    const endMs = Math.max(item.startMs + minMs, Math.min(durationMs, t));
    return { startMs: item.startMs, endMs, snappedAt: snapped.snapped && endMs === t ? t : null };
  }
  const startMs = Math.min(item.endMs - minMs, Math.max(0, t));
  return { startMs, endMs: item.endMs, snappedAt: snapped.snapped && startMs === t ? t : null };
}

/** Where a new effect dropped at `ms` goes: it starts there (snapped) and lasts the bar it starts in,
 * or `defaultMs` without bars, ending by the end of the song. */
export function createSpan(args: {
  ms: number;
  durationMs: number;
  frameMs: number;
  bars?: Mark[];
  defaultMs?: number;
  snap?: Snap;
}): { startMs: number; endMs: number } {
  const { durationMs, frameMs, bars, defaultMs = 2000, snap } = args;
  const at = snap ? snapTime(args.ms, snap.targets, snap.thresholdMs).ms : args.ms;
  const startMs = Math.max(0, Math.min(Math.round(at), durationMs - frameMs));
  const bar = bars?.find((b) => b.startMs <= startMs && startMs < b.endMs);
  const length = bar ? bar.endMs - bar.startMs : defaultMs;
  return { startMs, endMs: Math.min(durationMs, startMs + Math.max(length, frameMs)) };
}

/** Shortens a new effect to the gap it starts in (effects sorted by start), or null when it starts
 * on top of another effect. */
export function fitInLane(effects: Effect[], startMs: number, endMs: number, minMs: number): { startMs: number; endMs: number } | null {
  let end = endMs;
  for (const e of effects) {
    if (e.startMs <= startMs && startMs < e.endMs) return null;
    if (e.startMs > startMs && e.startMs < end) end = e.startMs;
  }
  return end - startMs >= minMs ? { startMs, endMs: end } : null;
}

/** The first layer of `row` with nothing between `startMs` and `endMs` (counting `extra` spans
 * about to be added), or a new layer on top. */
export function freeLayer(row: Row, startMs: number, endMs: number, extra: { layer: number; startMs: number; endMs: number }[] = []): number {
  const overlaps = (s: number, e: number) => s < endMs && startMs < e;
  for (let l = 0; ; l++) {
    const existing = row.layers[l]?.effects ?? [];
    const busy = existing.some((e) => overlaps(e.startMs, e.endMs)) || extra.some((x) => x.layer === l && overlaps(x.startMs, x.endMs));
    if (!busy) return l;
    if (l >= row.layers.length && !extra.some((x) => x.layer > l)) return l + 1;
  }
}

/** Where an effect dropped from the palette at `ms` on `lane` goes: the bar it lands in (or two
 * seconds), shortened to fit the gap it's dropped in; dropped on top of another effect (or on a
 * collapsed row), it goes on the first layer with room instead. */
export function planDrop(args: {
  doc: Sequence;
  index: EffectIndex;
  lane: Lane;
  ms: number;
  snap?: Snap;
}): { rowId: string; layer: number; startMs: number; endMs: number } | null {
  const { doc, index, lane, ms, snap } = args;
  const row = index.rows.get(lane.rowId);
  if (!row || doc.durationMs <= 0) return null;
  const bars = doc.timingTracks.find((t) => t.kind === "bars")?.marks;
  const span = createSpan({ ms, durationMs: doc.durationMs, frameMs: doc.frameMs, bars, snap });
  if (lane.layer >= 0) {
    const effects = index.layers.get(`${lane.rowId}:${lane.layer}`)?.effects ?? [];
    const fit = fitInLane(effects, span.startMs, span.endMs, doc.frameMs);
    if (fit) return { rowId: lane.rowId, layer: lane.layer, ...fit };
  }
  return { rowId: lane.rowId, layer: freeLayer(row, span.startMs, span.endMs), ...span };
}

// --- Keyboard and clipboard -------------------------------------------------------------------

/** One step from `ms`: a frame, or (with `byBeat`) to the next or previous beat. */
export function stepTime(ms: number, direction: 1 | -1, grid: { frameMs: number; beats?: number[] }, byBeat = false): number {
  const beats = grid.beats ?? [];
  if (byBeat && beats.length > 0) {
    const interval = beats.length > 1 ? beats[beats.length - 1] - beats[beats.length - 2] : grid.frameMs;
    if (direction > 0) {
      const next = beats.find((b) => b > ms);
      return next ?? ms + interval;
    }
    const previous = [...beats].reverse().find((b) => b < ms);
    return previous ?? Math.max(0, ms - interval);
  }
  return Math.max(0, ms + direction * grid.frameMs);
}

/** Copies of effects pasted so the earliest starts at `atMs`, each on its own row (if it's still
 * there), on the first layer with room. */
export function pasteEffects(
  doc: Sequence,
  index: EffectIndex,
  copies: { rowId: string; effect: Effect }[],
  atMs: number,
  newId: () => string = () => crypto.randomUUID(),
): SequenceEdit[] {
  const placed = copies.filter((c) => index.rows.has(c.rowId));
  if (placed.length === 0) return [];
  const offset = Math.round(atMs) - Math.min(...placed.map((p) => p.effect.startMs));
  const added = new Map<string, { layer: number; startMs: number; endMs: number }[]>();
  const edits: SequenceEdit[] = [];
  for (const p of placed) {
    const row = index.rows.get(p.rowId);
    if (!row) continue;
    const length = p.effect.endMs - p.effect.startMs;
    const startMs = Math.max(0, Math.min(p.effect.startMs + offset, doc.durationMs - length));
    const endMs = Math.min(doc.durationMs, startMs + length);
    if (endMs <= startMs) continue;
    const extra = added.get(p.rowId) ?? [];
    const layer = freeLayer(row, startMs, endMs, extra);
    extra.push({ layer, startMs, endMs });
    added.set(p.rowId, extra);
    edits.push({ type: "addEffect", row: p.rowId, layer, effect: { ...structuredClone(p.effect), id: newId(), startMs, endMs } });
  }
  return edits;
}
