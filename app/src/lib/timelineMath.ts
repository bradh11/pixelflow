// Pure timeline math: time ↔ pixels, zoom, ruler ticks, snapping, lanes, hit testing, selection,
// and the edits that drags, pastes, and the keyboard turn into. No React, no engine calls.

import type { Effect, Mark, Row, Sequence, SequenceEdit, TimingTrack } from "../api/sequence";
import { insertIndex } from "../api/timingMarks";

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

const NICE_STEPS = [
  10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10_000, 15_000, 30_000, 60_000, 120_000, 300_000, 600_000, 900_000, 1_800_000, 3_600_000,
];
/** Minor ticks per major tick, by the major step's leading digit. */
const MINOR_DIVISIONS: Record<string, number> = { "1": 5, "2": 4, "5": 5, "1.5": 3, "3": 3, "6": 6, "9": 3, "1.8": 6, "3.6": 4 };

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

/** Every time an edge may snap to: timing marks, other effects' edges, and the start. Effects in
 * `exclude` and the marks `skipMarks` (being dragged) don't count. */
export function snapTargets(doc: Sequence, exclude: ReadonlySet<string>, skipMarks?: { track: string; indices: ReadonlySet<number> }): number[] {
  const times = new Set<number>([0]);
  for (const track of doc.timingTracks) {
    const skip = skipMarks?.track === track.id ? skipMarks.indices : null;
    track.marks.forEach((mark, i) => {
      if (skip?.has(i)) return;
      times.add(mark.startMs);
      times.add(mark.endMs);
    });
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

/** `ms` on the frame grid (whole milliseconds without one). */
export function onGrid(ms: number, frameMs = 1): number {
  return Math.round(ms / Math.max(1, frameMs)) * Math.max(1, frameMs);
}

/** Moves every dragged effect by the same amount: the grabbed one (`primary`) snaps by either
 * edge (or, not snapping, starts on the frame grid), and none goes before the start, past the end,
 * or off the lanes. */
export function moveDrag(args: {
  items: DragItem[];
  primary: string;
  deltaMs: number;
  deltaLanes: number;
  laneCount: number;
  durationMs: number;
  frameMs?: number;
  snap?: Snap;
}): { items: DragItem[]; snappedAt: number | null } {
  const { items, primary, laneCount, durationMs, snap } = args;
  const lo = -Math.min(...items.map((i) => i.startMs));
  const hi = durationMs - Math.max(...items.map((i) => i.endMs));
  const clamp = (d: number) => Math.max(lo, Math.min(hi, d));
  const grabbed = items.find((i) => i.id === primary);
  const start = grabbed?.startMs ?? 0;
  let delta = clamp(onGrid(start + args.deltaMs, args.frameMs) - start);
  let snappedAt: number | null = null;
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

/** Where a moved effect lands: its row and layer (and the lane showing it, to draw it there). */
export interface Placement extends DragItem {
  rowId: string;
  layer: number;
}

/**
 * Where dragged effects land: on the layer they were dropped on, unless that would overlap an
 * effect that isn't moving (or one placed before it); then on the row's first layer with room,
 * or a new layer on top, like a drop from the palette. A collapsed row keeps an effect on its own
 * layer when it can. `lane` is the lane showing the layer (the dropped-on lane for a new layer).
 */
export function placeMove(index: EffectIndex, lanes: Lane[], moved: DragItem[]): Placement[] {
  const moving = new Set(moved.map((m) => m.id));
  const placed: Placement[] = [];
  for (const item of moved) {
    const lane = lanes[item.lane];
    const row = lane && index.rows.get(lane.rowId);
    const from = index.byId.get(item.id);
    if (!lane || !row || !from) continue;
    const overlaps = (s: number, e: number) => s < item.endMs && item.startMs < e;
    const busy = (layer: number) =>
      (row.layers[layer]?.effects ?? []).some((e) => !moving.has(e.id) && overlaps(e.startMs, e.endMs)) ||
      placed.some((p) => p.rowId === row.id && p.layer === layer && overlaps(p.startMs, p.endMs));
    let layer = lane.layer >= 0 ? lane.layer : from.rowId === row.id ? from.layer : 0;
    if (busy(layer)) {
      layer = 0;
      while (busy(layer)) layer++;
    }
    const shown = laneOf(lanes, row.id, layer);
    placed.push({ ...item, rowId: row.id, layer, lane: shown >= 0 ? shown : item.lane });
  }
  return placed;
}

/** The edits for a finished move: new times, plus a new row or layer for effects that changed
 * place (new layers in order, so each one's layer exists when it's reached). */
export function moveEdits(placements: Placement[], index: EffectIndex): SequenceEdit[] {
  const edits: SequenceEdit[] = [];
  for (const p of [...placements].sort((a, b) => a.layer - b.layer)) {
    const from = index.byId.get(p.id);
    if (!from) continue;
    if (from.rowId === p.rowId && from.layer === p.layer) {
      if (p.startMs !== from.effect.startMs || p.endMs !== from.effect.endMs) {
        edits.push({ type: "setEffectTiming", id: p.id, startMs: p.startMs, endMs: p.endMs });
      }
    } else {
      edits.push({ type: "moveEffect", id: p.id, row: p.rowId, layer: p.layer, startMs: p.startMs, endMs: p.endMs });
    }
  }
  return edits;
}

/** Drags one edge of an effect to `ms` (snapped, or else on the frame grid), keeping at least
 * `minMs`, staying in the song, and stopping at the neighbors (`bounds`, see effectBounds). */
export function resizeDrag(args: {
  item: DragItem;
  edge: "start" | "end";
  ms: number;
  minMs: number;
  durationMs: number;
  frameMs?: number;
  bounds?: { lo: number; hi: number };
  snap?: Snap;
}): { startMs: number; endMs: number; snappedAt: number | null } {
  const { item, edge, minMs, durationMs, snap } = args;
  const snapped = snap ? snapTime(args.ms, snap.targets, snap.thresholdMs) : { ms: args.ms, snapped: false };
  const t = snapped.snapped ? Math.round(snapped.ms) : onGrid(snapped.ms, args.frameMs);
  const lo = Math.max(0, args.bounds?.lo ?? 0);
  const hi = Math.min(durationMs, args.bounds?.hi ?? durationMs);
  if (edge === "end") {
    const endMs = Math.max(item.startMs + minMs, Math.min(hi, t));
    return { startMs: item.startMs, endMs, snappedAt: snapped.snapped && endMs === t ? t : null };
  }
  const startMs = Math.min(item.endMs - minMs, Math.max(lo, t));
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
  const snapped = snap ? snapTime(args.ms, snap.targets, snap.thresholdMs) : { ms: args.ms, snapped: false };
  const at = snapped.snapped ? Math.round(snapped.ms) : onGrid(snapped.ms, frameMs);
  const startMs = Math.max(0, Math.min(at, durationMs - frameMs));
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

/**
 * How far an effect's edges may go without running into its neighbors on the same layer (effects
 * in `ignore` don't count: they're moving too): from the end of the one before it to the start of
 * the one after it, or the song's ends. Neighbors that already overlap it never make it smaller.
 */
export function effectBounds(doc: Sequence, id: string, ignore: ReadonlySet<string> = new Set()): { lo: number; hi: number } | null {
  for (const row of doc.rows) {
    for (const layer of row.layers) {
      const effect = layer.effects.find((e) => e.id === id);
      if (!effect) continue;
      let lo = 0;
      let hi = doc.durationMs;
      for (const other of layer.effects) {
        if (other.id === id || ignore.has(other.id)) continue;
        if (other.startMs < effect.startMs) lo = Math.max(lo, other.endMs);
        else hi = Math.min(hi, other.startMs);
      }
      return { lo: Math.min(lo, effect.startMs), hi: Math.max(hi, effect.endMs) };
    }
  }
  return null;
}

// --- Keyboard and clipboard -------------------------------------------------------------------

/**
 * The edits that move effects `ids` one step (a frame, or with `byBeat` to the next or previous
 * beat), built from `doc` as it is now: every one moves by the same amount, and none goes past the
 * song's ends or into a neighbor that isn't moving. Empty when they can't move that way.
 */
export function nudgeEdits(doc: Sequence, ids: string[], direction: 1 | -1, byBeat: boolean): SequenceEdit[] {
  const chosen = new Set(ids);
  const starts: number[] = [];
  for (const row of doc.rows) for (const layer of row.layers) for (const e of layer.effects) if (chosen.has(e.id)) starts.push(e.startMs);
  if (starts.length === 0) return [];
  const track = doc.timingTracks.find((t) => t.kind === "beats") ?? doc.timingTracks[0];
  const grid = { frameMs: doc.frameMs, beats: track?.marks.map((m) => m.startMs) ?? [] };
  const first = Math.min(...starts);
  return shiftEdits(doc, ids, stepTime(first, direction, grid, byBeat) - first).edits;
}

/**
 * Moves every effect `ids` by the same amount, as near to `deltaMs` as they can go: none goes past
 * the song's ends or into an effect that isn't moving. Gives the amount they move by.
 */
export function shiftEdits(doc: Sequence, ids: readonly string[], deltaMs: number): { deltaMs: number; edits: SequenceEdit[] } {
  const chosen = new Set(ids);
  const placed: Effect[] = [];
  for (const row of doc.rows) for (const layer of row.layers) for (const e of layer.effects) if (chosen.has(e.id)) placed.push(e);
  let delta = Math.round(deltaMs);
  for (const e of placed) {
    const bounds = effectBounds(doc, e.id, chosen) ?? { lo: 0, hi: doc.durationMs };
    delta = Math.max(bounds.lo - e.startMs, Math.min(bounds.hi - e.endMs, delta));
  }
  // Pulled back past nothing (they're already against something): don't move the other way.
  if (delta === 0 || Math.sign(delta) !== Math.sign(deltaMs)) return { deltaMs: 0, edits: [] };
  return { deltaMs: delta, edits: placed.map((e) => ({ type: "setEffectTiming", id: e.id, startMs: e.startMs + delta, endMs: e.endMs + delta })) };
}

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

// --- Timing marks -----------------------------------------------------------------------------

/** The marks of a sorted, non-overlapping track that show between `t0` and `t1`: [first, end). */
export function marksInView(marks: Mark[], t0: number, t1: number): [number, number] {
  let lo = 0;
  let hi = marks.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (marks[mid].endMs <= t0) lo = mid + 1;
    else hi = mid;
  }
  let end = lo;
  while (end < marks.length && marks[end].startMs < t1) end++;
  return [lo, end];
}

/** The mark at `x` on a track (and which part: an edge, to resize, or the body, to move). Where two
 * marks touch, the edge nearer the pointer wins. */
export function hitMark(marks: Mark[], x: number, view: View, edgePx = 5): { index: number; part: EffectPart } | null {
  const t = xToTime(x, view);
  const slack = (edgePx + 1) / view.pxPerMs;
  const [first, end] = marksInView(marks, t - slack, t + slack);
  let best: { index: number; part: EffectPart; gap: number } | null = null;
  for (let i = first; i < end; i++) {
    const x0 = timeToX(marks[i].startMs, view);
    const x1 = timeToX(marks[i].endMs, view);
    const edge = Math.max(2, Math.min(edgePx, (x1 - x0) / 3));
    const toStart = Math.abs(x - x0);
    const toEnd = Math.abs(x1 - x);
    // An edge of the mark the pointer is over beats an equally near edge of its neighbor.
    const outside = x < x0 || x > x1 ? 0.5 : 0;
    let hit: { index: number; part: EffectPart; gap: number } | null = null;
    if (toStart <= edge && toStart <= toEnd) hit = { index: i, part: "start", gap: toStart + outside };
    else if (toEnd <= edge) hit = { index: i, part: "end", gap: toEnd + outside };
    else if (x >= x0 && x <= x1) hit = { index: i, part: "body", gap: Infinity };
    if (hit && (!best || hit.gap < best.gap)) best = hit;
  }
  return best ? { index: best.index, part: best.part } : null;
}

/** Indices of the marks that start at `starts` (marks are picked by start time, which stays put
 * while other marks come and go). */
export function markIndices(track: TimingTrack, starts: readonly number[]): number[] {
  const wanted = new Set(starts);
  const out: number[] = [];
  track.marks.forEach((m, i) => {
    if (wanted.has(m.startMs)) out.push(i);
  });
  return out;
}

/** How far a mark's edges may go: from the end of the mark before it to the start of the one
 * after it (marks in `moving` don't count), or the song's ends. */
export function markBounds(marks: Mark[], index: number, durationMs: number, moving: ReadonlySet<number> = new Set()): { lo: number; hi: number } {
  let lo = 0;
  let hi = durationMs;
  for (let i = index - 1; i >= 0; i--) {
    if (moving.has(i)) continue;
    lo = marks[i].endMs;
    break;
  }
  for (let i = index + 1; i < marks.length; i++) {
    if (moving.has(i)) continue;
    hi = marks[i].startMs;
    break;
  }
  return { lo: Math.min(lo, marks[index].startMs), hi: Math.max(hi, marks[index].endMs) };
}

/** A mark where it is (or would go) during a drag. */
export interface MarkSpan {
  index: number;
  startMs: number;
  endMs: number;
}

/** Moves the marks `moving` of a track by the same amount: the grabbed one (`primary`) snaps by
 * either edge (or, not snapping, starts on the frame grid), and none runs into a mark that isn't
 * moving or off the song. */
export function moveMarksDrag(args: {
  marks: Mark[];
  moving: number[];
  primary: number;
  deltaMs: number;
  durationMs: number;
  frameMs?: number;
  snap?: Snap;
}): { spans: MarkSpan[]; snappedAt: number | null } {
  const { marks, moving, primary, durationMs, snap } = args;
  const set = new Set(moving);
  let lo = -Infinity;
  let hi = Infinity;
  for (const i of moving) {
    const b = markBounds(marks, i, durationMs, set);
    lo = Math.max(lo, b.lo - marks[i].startMs);
    hi = Math.min(hi, b.hi - marks[i].endMs);
  }
  const clamp = (d: number) => Math.max(lo, Math.min(hi, d));
  const grabbed = marks[primary];
  let delta = clamp(onGrid(grabbed.startMs + args.deltaMs, args.frameMs) - grabbed.startMs);
  let snappedAt: number | null = null;
  if (snap) {
    const byStart = snapTime(grabbed.startMs + delta, snap.targets, snap.thresholdMs);
    const byEnd = snapTime(grabbed.endMs + delta, snap.targets, snap.thresholdMs);
    const startGap = Math.abs(byStart.ms - (grabbed.startMs + delta));
    const endGap = Math.abs(byEnd.ms - (grabbed.endMs + delta));
    const pick = byStart.snapped && (!byEnd.snapped || startGap <= endGap) ? "start" : byEnd.snapped ? "end" : null;
    if (pick) {
      const target = pick === "start" ? byStart.ms : byEnd.ms;
      const snappedDelta = Math.round(target - (pick === "start" ? grabbed.startMs : grabbed.endMs));
      if (clamp(snappedDelta) === snappedDelta) {
        delta = snappedDelta;
        snappedAt = target;
      }
    }
  }
  return { spans: moving.map((i) => ({ index: i, startMs: marks[i].startMs + delta, endMs: marks[i].endMs + delta })), snappedAt };
}

/** A dropped mark: where it was (which says which mark it is, as marks come and go) and where it
 * goes. */
export interface MarkMove {
  fromStartMs: number;
  fromEndMs: number;
  startMs: number;
  endMs: number;
}

/** Where `moves` put the marks of `track` as it is now, found by where each mark was; null when one
 * of them has gone or changed since the drag began. */
export function markMovesToSpans(track: TimingTrack, moves: MarkMove[]): MarkSpan[] | null {
  const spans: MarkSpan[] = [];
  for (const move of moves) {
    const index = insertIndex(track.marks, move.fromStartMs) - 1;
    const m = track.marks[index];
    if (!m || m.startMs !== move.fromStartMs || m.endMs !== move.fromEndMs) return null;
    spans.push({ index, startMs: move.startMs, endMs: move.endMs });
  }
  return spans;
}

/** Where two marks touch at `edge` of the mark at `index`: the neighbor, and how far the shared edge
 * may go (each keeps at least `minMs`); null when the edge touches nothing. */
export function touchingEdge(marks: Mark[], index: number, edge: "start" | "end", minMs: number): { neighbor: number; bounds: { lo: number; hi: number } } | null {
  const m = marks[index];
  const neighbor = edge === "start" ? index - 1 : index + 1;
  const n = marks[neighbor];
  if (!m || !n || (edge === "start" ? n.endMs !== m.startMs : n.startMs !== m.endMs)) return null;
  const [first, second] = edge === "start" ? [n, m] : [m, n];
  return { neighbor, bounds: { lo: first.startMs + minMs, hi: second.endMs - minMs } };
}

/** The edits for moved or resized marks. Each mark lands before the next one moves, so marks moved
 * together go in the order that never overlaps a neighbor on the way (the leading one first). */
export function markMoveEdits(track: TimingTrack, spans: MarkSpan[]): SequenceEdit[] {
  const changed = spans.filter((s) => {
    const m = track.marks[s.index];
    return m && (m.startMs !== s.startMs || m.endMs !== s.endMs);
  });
  if (changed.length === 0) return [];
  // Which way they go: by the middle, so a shared edge dragged right counts as right too.
  const first = track.marks[changed[0].index];
  const right = changed[0].startMs + changed[0].endMs > first.startMs + first.endMs;
  const ordered = [...changed].sort((a, b) => (right ? b.index - a.index : a.index - b.index));
  return ordered.map((s) => ({
    type: "setMark" as const,
    track: track.id,
    index: s.index,
    mark: { ...track.marks[s.index], startMs: s.startMs, endMs: s.endMs },
  }));
}

/** Where a mark added at `ms` (snapped) goes: one beat long (the beat it starts in), or `defaultMs`
 * without beats, shortened to the gap it starts in; null on top of another mark. */
export function newMarkSpan(args: { doc: Sequence; track: TimingTrack; ms: number; defaultMs?: number; snap?: Snap }): { startMs: number; endMs: number } | null {
  const { doc, track, defaultMs = 500, snap } = args;
  const snapped = snap ? snapTime(args.ms, snap.targets, snap.thresholdMs) : { ms: args.ms, snapped: false };
  const startMs = Math.max(0, snapped.snapped ? Math.round(snapped.ms) : onGrid(snapped.ms, doc.frameMs));
  if (startMs >= doc.durationMs) return null;
  const beats = doc.timingTracks.find((t) => t.kind === "beats" && t.id !== track.id)?.marks;
  const beat = beats?.find((b) => b.startMs <= startMs && startMs < b.endMs);
  const length = beat ? beat.endMs - beat.startMs : defaultMs;
  let endMs = Math.min(doc.durationMs, startMs + Math.max(1, length));
  for (const m of track.marks) {
    if (m.startMs <= startMs && startMs < m.endMs) return null;
    if (m.startMs > startMs && m.startMs < endMs) endMs = m.startMs;
  }
  return endMs > startMs ? { startMs, endMs } : null;
}

/**
 * Tap to time: the edits for a tap at `atMs` on `track` (the playhead while the music plays). The
 * mark the last tap started (`lastStart`) ends here, and a new one starts here, `lengthMs` long
 * until the next tap ends it (shortened to the gap it's in); a tap inside an existing mark splits
 * it instead. Null when there's nothing to do (past the end, or a mark already starts here).
 */
export function tapEdits(track: TimingTrack, atMs: number, lastStart: number | null, durationMs: number, lengthMs = 500): { edits: SequenceEdit[]; startMs: number } | null {
  const at = Math.round(atMs);
  if (at < 0 || at >= durationMs) return null;
  const marks = track.marks.map((m) => ({ ...m }));
  const edits: SequenceEdit[] = [];
  const last = lastStart === null ? -1 : marks.findIndex((m) => m.startMs === lastStart);
  if (last >= 0 && at > marks[last].startMs) {
    const next = marks[last + 1];
    const endMs = Math.min(at, next ? next.startMs : durationMs);
    if (endMs !== marks[last].endMs) {
      marks[last].endMs = endMs;
      edits.push({ type: "setMark", track: track.id, index: last, mark: { ...marks[last] } });
    }
  }
  const inside = marks.findIndex((m) => m.startMs < at && at < m.endMs);
  if (inside >= 0) {
    edits.push({ type: "splitMark", track: track.id, index: inside, atMs: at });
  } else if (!marks.some((m) => m.startMs === at)) {
    const next = marks.find((m) => m.startMs > at);
    const endMs = Math.min(at + lengthMs, next ? next.startMs : durationMs, durationMs);
    edits.push({ type: "addMarks", track: track.id, marks: [{ startMs: at, endMs, label: "" }] });
  }
  return edits.length > 0 ? { edits, startMs: at } : null;
}

/** "1:05.250", "65.25", or "65" → milliseconds; null when it isn't a time. */
export function parseTime(text: string): number | null {
  const m = /^(?:(\d+):)?(\d+(?:\.\d*)?)$/.exec(text.trim());
  if (!m) return null;
  const seconds = Number(m[2]);
  if (m[1] && seconds >= 60) return null;
  return Math.round(((m[1] ? Number(m[1]) : 0) * 60 + seconds) * 1000);
}

/** The track a lyrics track's words go on: the one named "<name> (words)" (a words track first),
 * or null when there's none yet. Never another track, which could be another lyrics track's words. */
export function wordsTrackFor(doc: Sequence, track: TimingTrack): TimingTrack | null {
  const named = doc.timingTracks.filter((t) => t.name === `${track.name} (words)` && t.kind !== "phonemes" && t.id !== track.id);
  return named.find((t) => t.kind === "words") ?? named[0] ?? null;
}
