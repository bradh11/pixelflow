import { ChevronDown, ChevronRight, GripVertical, Layers, Maximize2, Plus, Trash2, ZoomIn, ZoomOut } from "lucide-react";
import { Fragment, type PointerEvent as ReactPointerEvent, memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { submodelsOf, targetKey, targetName } from "../../lib/submodels";
import { defaultParams, newEffect, newRow, type EffectKind, type Sequence, type SequenceEdit, type SequenceTarget, type TimingTrack } from "../../api/sequence";
import type { Show, Waveform } from "../../api/types";
import {
  type DragItem,
  type Lane,
  type MarkMove,
  type MarkSpan,
  type Placement,
  type View,
  SNAP_PX,
  autoScroll,
  autoScrollSpeed,
  buildIndex,
  clampView,
  effectBounds,
  fitView,
  followPlayhead,
  formatTime,
  hitEffect,
  hitMark,
  markBounds,
  markIndices,
  markMoveEdits,
  markMovesToSpans,
  moveMarksDrag,
  newMarkSpan,
  laneAt,
  laneOf,
  layoutLanes,
  marqueeSelect,
  moveDrag,
  moveEdits,
  placeMove,
  planDrop,
  resizeDrag,
  snapTargets,
  timeToX,
  toggleSelection,
  touchingEdge,
  xToTime,
  zoomAt,
} from "../../lib/timelineMath";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { GoToScreen } from "../GoToScreen";
import { usePaletteDrag } from "./EffectPalette";
import { TimingTrackHeaders } from "./TimingTrackHeaders";
import { LANE_H, RULER_H, TRACK_H, WAVE_H, drawTimeline, topHeight } from "./drawTimeline";
import { timelineMinHeight } from "../../lib/sequenceLayout";
import { resolveAudio } from "../../lib/showFiles";

/** Colors a new effect starts with. */
export const DEFAULT_COLORS = ["#ff0000", "#00c000", "#ffffff"];
/** Presses that move less than this (screen pixels) are clicks. */
const CLICK_PX = 3;
/** How often the timeline scrolls while something is held near its edge. */
const AUTO_SCROLL_MS = 16;

type Drag =
  | {
      kind: "move";
      primary: string;
      items: DragItem[];
      moved: Placement[];
      x: number;
      y: number;
      /** Where it was grabbed, in time and in the rows (which stay put when the view scrolls). */
      ms: number;
      rowsY: number;
      started: boolean;
      targets: number[];
    }
  | {
      kind: "resize";
      item: DragItem;
      edge: "start" | "end";
      result: { startMs: number; endMs: number };
      targets: number[];
      /** How far the edge may go before it runs into a neighbor. */
      bounds: { lo: number; hi: number };
    }
  | { kind: "marquee"; x0: number; y0: number; x1: number; y1: number; additive: string[] }
  | { kind: "scrub" }
  | {
      kind: "markMove";
      track: string;
      /** The grabbed mark, and every mark moving with it, as they were (which says which marks
       * they are, whatever comes and goes meanwhile), and where they are now. */
      primary: TimeSpan;
      from: TimeSpan[];
      spans: MarkSpan[];
      x: number;
      y: number;
      /** Where it was grabbed, in time. */
      ms: number;
      started: boolean;
      targets: number[];
    }
  | {
      kind: "markResize";
      track: string;
      /** The mark as it was (and the mark touching the dragged edge, which moves with it), and
       * where they are now. */
      from: TimeSpan[];
      spans: MarkSpan[];
      edge: "start" | "end";
      bounds: { lo: number; hi: number };
      targets: number[];
      changed: boolean;
    };

type TimeSpan = { startMs: number; endMs: number };

/** Where `from` marks go: to `spans` (the same marks, in the same order). */
const movesOf = (from: TimeSpan[], spans: TimeSpan[]): MarkMove[] =>
  from.map((f, k) => ({ fromStartMs: f.startMs, fromEndMs: f.endMs, startMs: spans[k].startMs, endMs: spans[k].endMs }));

/** Where a palette drop would land; `newLayer` when it would go on a new layer of the row. */
type Ghost = { lane: number; startMs: number; endMs: number; newLayer: boolean };

export { targetName };

export { resolveAudio };

function useSize(ref: React.RefObject<HTMLElement | null>) {
  const [size, setSize] = useState({ width: 0, height: 0 });
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => {
      const r = el.getBoundingClientRect();
      setSize((s) => (s.width === r.width && s.height === r.height ? s : { width: r.width, height: r.height }));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref]);
  return size;
}

/** The effects on their rows over time, with the ruler, the music, and the timing marks above. */
export function Timeline({ doc }: { doc: Sequence }) {
  const show = useApp((s) => s.snapshot?.show);
  const backend = useApp((s) => s.backend);
  const theme = useApp((s) => s.theme);
  const selection = useSequencer((s) => s.selection);
  const playheadMs = useSequencer((s) => s.playheadMs);
  const playing = useSequencer((s) => s.status?.state === "playing");
  const collapsed = useSequencer((s) => s.collapsed);
  const snapping = useSequencer((s) => s.snapping);
  const catalog = useSequencer((s) => s.catalog);
  const path = useSequencer((s) => s.path);
  const activeRow = useSequencer((s) => s.activeRow);
  const docKey = useSequencer((s) => s.docKey);
  const revealAt = useSequencer((s) => s.revealAt);
  const markSelection = useSequencer((s) => s.markSelection);
  const activeTrack = useSequencer((s) => s.activeTrack);
  const bodyRef = useRef<HTMLDivElement>(null);
  /** A mark's label being typed in place (and the mark as it was when the typing began). */
  const [labelEdit, setLabelEdit] = useState<{ track: string; startMs: number; endMs: number; value: string } | null>(null);
  /** Set once the label being typed is saved or cancelled, so the blur that follows doesn't save it
   * (again). */
  const labelDone = useRef(false);
  /** Dropped marks on their way to the engine, drawn where they were dropped meanwhile. */
  const pendingMarks = useRef<{ key: number; track: string; moves: MarkMove[] } | null>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const size = useSize(bodyRef);
  const top = topHeight(doc);
  const [view, setViewState] = useState<View | null>(null);
  const [scrollY, setScrollY] = useState(0);
  const [waveform, setWaveform] = useState<Waveform | null>(null);
  const drag = useRef<Drag | null>(null);
  /** Where the pointer is during a drag, and the timer scrolling the timeline while it's near an edge. */
  const pointer = useRef<{ x: number; y: number; alt: boolean } | null>(null);
  const scrollTimer = useRef<ReturnType<typeof setInterval> | null>(null);
  /** A finished move or resize on its way to the engine: drawn where it was dropped until the
   * engine has answered, so the effects don't jump back meanwhile. */
  const pending = useRef<{ key: number; items: DragItem[] } | null>(null);
  const pendingKey = useRef(0);
  const [, redraw] = useState(0);
  const [ghost, setGhost] = useState<Ghost | null>(null);

  const index = useMemo(() => buildIndex(doc), [doc]);
  const collapsedSet = useMemo(() => new Set(collapsed), [collapsed]);
  const { lanes, height: rowsHeight } = useMemo(() => layoutLanes(doc.rows, collapsedSet, LANE_H), [doc.rows, collapsedSet]);
  const labels = useMemo(() => new Map(catalog.map((c) => [c.kind as string, c.label])), [catalog]);
  const selectionSet = useMemo(() => new Set(selection), [selection]);
  const rowsViewport = Math.max(0, size.height - top);
  const maxScroll = Math.max(0, rowsHeight + LANE_H - rowsViewport);

  const width = Math.max(1, size.width);
  const current: View = view ? clampView(view, doc.durationMs, width) : fitView(doc.durationMs, width);
  const setView = useCallback((v: View) => setViewState(clampView(v, doc.durationMs, Math.max(1, width))), [doc.durationMs, width]);

  // Everything event handlers need, current as of the last render.
  const latest = useRef({ doc, index, lanes, view: current, top, scrollY, snapping, selection, maxScroll, catalog, activeRow, playheadMs, size });
  latest.current = { doc, index, lanes, view: current, top, scrollY, snapping, selection, maxScroll, catalog, activeRow, playheadMs, size };

  // Fit the song when a different sequence is opened (not when this one is saved somewhere new).
  useEffect(() => setViewState(null), [docKey]);
  useEffect(() => setScrollY((y) => Math.min(y, maxScroll)), [maxScroll]);

  // After a keyboard or problem-list pick: scroll the rows to the active row (or the selected
  // effect's lane) and the time to the selected effect, or else to the playhead.
  useEffect(() => {
    if (revealAt === 0 || size.width === 0) return;
    const s = useSequencer.getState();
    const playheadOnly = s.revealTarget === "playhead";
    const placed = !playheadOnly && s.selection.length > 0 ? index.byId.get(s.selection[0]) : undefined;
    const laneIndex = placed ? laneOf(lanes, placed.rowId, placed.layer) : lanes.findIndex((l) => l.rowId === s.activeRow);
    const lane = playheadOnly ? undefined : lanes[laneIndex];
    if (lane) {
      setScrollY((y) => {
        const next = lane.y < y ? lane.y : lane.y + lane.h > y + rowsViewport ? lane.y + lane.h - rowsViewport : y;
        return Math.max(0, Math.min(maxScroll, next));
      });
    }
    const visible = width / current.pxPerMs;
    if (placed) {
      const { startMs, endMs } = placed.effect;
      const inView = startMs < current.startMs + visible && endMs > current.startMs;
      if (!inView) setViewState(clampView({ ...current, startMs: startMs - visible * 0.1 }, doc.durationMs, width));
    } else {
      const next = followPlayhead(current, s.playheadMs, width, doc.durationMs);
      if (next !== current) setViewState(next);
    }
  }, [revealAt]); // eslint-disable-line react-hooks/exhaustive-deps

  // While playing, turn the page to keep the playhead in view.
  useEffect(() => {
    if (!playing || size.width === 0) return;
    const next = followPlayhead(current, playheadMs, width, doc.durationMs);
    if (next !== current) setViewState(next);
  }, [playheadMs, playing]); // eslint-disable-line react-hooks/exhaustive-deps

  // The music's loudness, about one value per 10 ms.
  const audio = resolveAudio(doc.audio, path);
  useEffect(() => {
    setWaveform(null);
    if (!backend || !audio) return;
    let cancelled = false;
    const slices = Math.max(100, Math.min(20_000, Math.round(doc.durationMs / 10)));
    backend.audioWaveform(audio, slices).then(
      (w) => !cancelled && setWaveform(w),
      () => !cancelled && setWaveform(null),
    );
    return () => {
      cancelled = true;
    };
  }, [backend, audio, doc.durationMs]);

  const snapFor = (exclude: string[], altKey: boolean) =>
    latest.current.snapping && !altKey ? snapTargets(latest.current.doc, new Set(exclude)) : [];

  const draw = () => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx || size.width === 0) return;
    const ratio = window.devicePixelRatio || 1;
    if (canvas.width !== Math.round(size.width * ratio) || canvas.height !== Math.round(size.height * ratio)) {
      canvas.width = Math.round(size.width * ratio);
      canvas.height = Math.round(size.height * ratio);
    }
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    const d = drag.current;
    drawTimeline(ctx, {
      width: size.width,
      height: size.height,
      theme,
      doc,
      index,
      lanes,
      view: current,
      scrollY,
      selection: selectionSet,
      playheadMs,
      waveform,
      labels,
      drag: d?.kind === "move" && d.started ? d.moved : d?.kind === "resize" ? [{ ...d.item, ...d.result }] : (pending.current?.items ?? null),
      snappedAt: d && d.kind !== "marquee" && d.kind !== "scrub" ? snappedOf(d) : null,
      marquee: d?.kind === "marquee" ? d : null,
      ghost,
      markSelection: markSelection ? { track: markSelection.track, starts: new Set(markSelection.starts) } : null,
      activeTrack,
      markDrag:
        d?.kind === "markMove" && d.started
          ? { track: d.track, spans: d.spans }
          : d?.kind === "markResize"
            ? { track: d.track, spans: d.spans }
            : pendingSpans(),
    });
  };
  useEffect(draw);

  /** Dropped marks still on their way, where they'll land (found by where they were). */
  function pendingSpans() {
    const p = pendingMarks.current;
    const track = p && doc.timingTracks.find((t) => t.id === p.track);
    const spans = track && markMovesToSpans(track, p.moves);
    return p && spans ? { track: p.track, spans } : null;
  }

  // Wheel: ⌘/Ctrl zooms around the pointer, sideways (or Shift) scrolls in time, otherwise rows.
  useEffect(() => {
    const el = canvasRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const { view: v, maxScroll: limit } = latest.current;
      const rect = el.getBoundingClientRect();
      if (e.ctrlKey || e.metaKey) {
        setViewState(zoomAt(v, Math.exp(-e.deltaY * 0.002), e.clientX - rect.left, latest.current.doc.durationMs, rect.width || 1));
      } else if (e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) {
        const delta = (e.shiftKey ? e.deltaY || e.deltaX : e.deltaX) / v.pxPerMs;
        setViewState(clampView({ ...v, startMs: v.startMs + delta }, latest.current.doc.durationMs, rect.width || 1));
      } else {
        setScrollY((y) => Math.max(0, Math.min(limit, y + e.deltaY)));
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  /** Places a new effect of `kind` where the palette dropped it (snapping unless Alt is held);
   * returns where it goes, or null when that's not over a row. */
  const dropAt = useCallback((kind: EffectKind, clientX: number, clientY: number, alt: boolean, place: boolean): Ghost | null => {
    const canvas = canvasRef.current;
    if (!canvas) return null;
    const rect = canvas.getBoundingClientRect();
    const { doc: d, index: idx, lanes: ls, view: v, top: tp, scrollY: sy } = latest.current;
    const [x, y] = [clientX - rect.left, clientY - rect.top];
    if (x < 0 || x > rect.width || y < tp || y > rect.height) return null;
    const lane = laneAt(ls, y - tp + sy);
    if (!lane) return null;
    const snap = latest.current.snapping && !alt ? { targets: snapTargets(d, new Set()), thresholdMs: SNAP_PX / v.pxPerMs } : undefined;
    const plan = planDrop({ doc: d, index: idx, lane, ms: xToTime(x, v), snap });
    if (!plan) return null;
    if (place) void addEffect(kind, plan);
    // Show it on the lane it will really go to; a new layer shows on the row's last lane.
    const shown = laneOf(ls, plan.rowId, plan.layer);
    const rowLanes = ls.filter((l) => l.rowId === plan.rowId);
    return { lane: shown >= 0 ? shown : ls.indexOf(rowLanes[rowLanes.length - 1]), startMs: plan.startMs, endMs: plan.endMs, newLayer: shown < 0 };
  }, []);

  async function addEffect(kind: EffectKind, plan: { rowId: string; layer: number; startMs: number; endMs: number }) {
    const info = latest.current.catalog.find((c) => c.kind === kind);
    const effect = { ...newEffect(kind, plan.startMs, plan.endMs, DEFAULT_COLORS), ...(info ? { params: defaultParams(info) } : {}) };
    const ok = await useSequencer.getState().edit([{ type: "addEffect", row: plan.rowId, layer: plan.layer, effect }]);
    if (ok) useSequencer.getState().select([effect.id], plan.rowId);
  }

  // The palette drops onto (and adds at the playhead on) this timeline.
  useEffect(() => {
    usePaletteDrag.setState({
      drop: (kind, x, y, alt) => dropAt(kind, x, y, alt, true) !== null,
      addAtPlayhead: (kind) => {
        const { doc: d, index: idx, lanes: ls, activeRow: row, playheadMs: at } = latest.current;
        const lane = ls.find((l) => l.rowId === row) ?? ls[0];
        if (!lane) {
          useApp.setState({ error: "Add a row first: effects go on a row for a prop or group." });
          return;
        }
        const plan = planDrop({ doc: d, index: idx, lane, ms: at });
        if (plan) void addEffect(kind, plan);
      },
    });
    return () => usePaletteDrag.setState({ drop: null, addAtPlayhead: null });
  }, [dropAt]);

  // While an effect is dragged from the palette, show where it would land.
  useEffect(
    () =>
      usePaletteDrag.subscribe((s) => {
        setGhost(s.kind ? dropAt(s.kind, s.x, s.y, s.alt, false) : null);
      }),
    [dropAt],
  );

  const point = (e: { clientX: number; clientY: number }) => {
    const rect = canvasRef.current!.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  };

  /** The timing track whose strip is at `y` (canvas coordinates), if any. */
  const trackAt = (y: number): TimingTrack | null => {
    const k = Math.floor((y - RULER_H - WAVE_H) / TRACK_H);
    return y >= RULER_H + WAVE_H && y < latest.current.top ? (latest.current.doc.timingTracks[k] ?? null) : null;
  };

  /** A press on a timing track's strip: pick the track, select marks, or start dragging them. */
  const pressTrack = (track: TimingTrack, x: number, y: number, e: ReactPointerEvent<HTMLCanvasElement>) => {
    const store = useSequencer.getState();
    const { view: v, doc: d } = latest.current;
    const hit = hitMark(track.marks, x, v);
    const additive = e.shiftKey || e.metaKey || e.ctrlKey;
    const current = store.markSelection?.track === track.id ? store.markSelection.starts : [];
    if (!hit) {
      if (!additive) store.selectMarks(track.id, []);
      else store.setActiveTrack(track.id);
      drag.current = null;
      return;
    }
    const start = track.marks[hit.index].startMs;
    if (additive) {
      store.selectMarks(track.id, current.includes(start) ? current.filter((s) => s !== start) : [...current, start]);
      drag.current = null;
      return;
    }
    if (track.kind === "phonemes") {
      // Phonemes from xLights can be looked at, not changed.
      store.selectMarks(track.id, [start]);
      drag.current = null;
      return;
    }
    const targets = (indices: number[]) =>
      latest.current.snapping && !e.altKey ? snapTargets(d, new Set(), { track: track.id, indices: new Set(indices) }) : [];
    const m = track.marks[hit.index];
    const span = (i: number) => ({ startMs: track.marks[i].startMs, endMs: track.marks[i].endMs });
    if (hit.part !== "body") {
      store.selectMarks(track.id, [start]);
      // Where two marks touch, the edge they share moves for both, as in xLights.
      const shortest = Math.min(m.endMs - m.startMs, ...[hit.index - 1, hit.index + 1].map((i) => (track.marks[i] ? track.marks[i].endMs - track.marks[i].startMs : Infinity)));
      const touching = touchingEdge(track.marks, hit.index, hit.part, Math.max(1, Math.min(d.frameMs, shortest)));
      const indices = touching ? [hit.index, touching.neighbor] : [hit.index];
      drag.current = {
        kind: "markResize",
        track: track.id,
        from: indices.map(span),
        spans: indices.map((i) => ({ index: i, ...span(i) })),
        edge: hit.part,
        bounds: touching?.bounds ?? markBounds(track.marks, hit.index, d.durationMs),
        targets: targets(indices),
        changed: false,
      };
      return;
    }
    const moving = current.includes(start) ? markIndices(track, current) : [hit.index];
    if (!current.includes(start)) store.selectMarks(track.id, [start]);
    const spans = moving.map((i) => ({ index: i, ...span(i) }));
    drag.current = { kind: "markMove", track: track.id, primary: span(hit.index), from: moving.map(span), spans, x, y, ms: xToTime(x, v), started: false, targets: targets(moving) };
  };

  const onPointerDown = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    if (e.button !== 0) return;
    canvasRef.current?.focus();
    const { x, y } = point(e);
    const { view: v, top: tp, scrollY: sy, lanes: ls, index: idx, selection: sel } = latest.current;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    const store = useSequencer.getState();
    if (y < tp) {
      const track = trackAt(y);
      if (track) {
        pressTrack(track, x, y, e);
        redraw((n) => n + 1);
        return;
      }
      drag.current = { kind: "scrub" };
      void store.seek(xToTime(x, v));
      return;
    }
    const rowsY = y - tp + sy;
    const lane = laneAt(ls, rowsY);
    if (lane) store.setActiveRow(lane.rowId);
    const hit = lane ? hitEffect(idx, lane, x, v) : null;
    const additive = e.shiftKey || e.metaKey || e.ctrlKey;
    if (!hit) {
      drag.current = { kind: "marquee", x0: x, y0: rowsY, x1: x, y1: rowsY, additive: additive ? sel : [] };
      return;
    }
    if (additive) {
      store.select([...toggleSelection(new Set(sel), hit.id, true)]);
      drag.current = null;
      return;
    }
    const placedLane = (id: string) => {
      const p = idx.byId.get(id)!;
      return laneOf(ls, p.rowId, p.layer);
    };
    const asItem = (id: string): DragItem => {
      const p = idx.byId.get(id)!;
      return { id, startMs: p.effect.startMs, endMs: p.effect.endMs, lane: placedLane(id) };
    };
    if (hit.part !== "body") {
      store.select([hit.id]);
      const item = asItem(hit.id);
      const bounds = effectBounds(latest.current.doc, hit.id) ?? { lo: 0, hi: latest.current.doc.durationMs };
      drag.current = { kind: "resize", item, edge: hit.part, result: { startMs: item.startMs, endMs: item.endMs }, targets: snapFor([hit.id], e.altKey), bounds };
      return;
    }
    const ids = sel.includes(hit.id) ? sel.filter((id) => idx.byId.has(id)) : [hit.id];
    if (!sel.includes(hit.id)) store.select(ids);
    const items = ids.map(asItem);
    drag.current = { kind: "move", primary: hit.id, items, moved: placeMove(idx, ls, items), x, y, ms: xToTime(x, v), rowsY, started: false, targets: snapFor(ids, e.altKey) };
  };

  const onPointerMove = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    const { x, y } = point(e);
    if (!drag.current) {
      // Show what a press here would do.
      const { view: v, top: tp, scrollY: sy, lanes: ls, index: idx } = latest.current;
      const track = trackAt(y);
      if (track) {
        const hit = track.kind === "phonemes" ? null : hitMark(track.marks, x, v);
        e.currentTarget.style.cursor = !hit ? "default" : hit.part === "body" ? "grab" : "ew-resize";
        return;
      }
      const lane = y >= tp ? laneAt(ls, y - tp + sy) : null;
      const hit = lane ? hitEffect(idx, lane, x, v) : null;
      e.currentTarget.style.cursor = y < tp ? "text" : !hit ? "default" : hit.part === "body" ? "grab" : "ew-resize";
      return;
    }
    pointer.current = { x, y, alt: e.altKey };
    dragTo(x, y, e.altKey);
    if (scrollsNow(drag.current)) keepScrolling();
  };

  /** Follows the pointer at `x`, `y` with the drag, against the view as it is now. */
  const dragTo = (x: number, y: number, altKey: boolean) => {
    const d = drag.current;
    const { view: v, top: tp, scrollY: sy, lanes: ls, index: idx, doc: dd } = latest.current;
    const threshold = SNAP_PX / v.pxPerMs;
    if (!d) return;
    if (d.kind === "markResize" || d.kind === "markMove") {
      // The marks as they are now: found by where they were, as other edits land meanwhile.
      const track = dd.timingTracks.find((t) => t.id === d.track);
      const found = track && markMovesToSpans(track, movesOf(d.from, d.from));
      if (!track || !found) return;
      const snap = altKey ? undefined : { targets: d.targets, thresholdMs: threshold };
      if (d.kind === "markResize") {
        const [grabbed, neighbor] = d.from;
        const item = { id: "", startMs: grabbed.startMs, endMs: grabbed.endMs, lane: 0 };
        const minMs = Math.max(1, Math.min(dd.frameMs, grabbed.endMs - grabbed.startMs));
        const r = resizeDrag({ item, edge: d.edge, ms: xToTime(x, v), minMs, durationMs: dd.durationMs, frameMs: dd.frameMs, bounds: d.bounds, snap });
        d.spans = [{ index: found[0].index, startMs: r.startMs, endMs: r.endMs }];
        if (neighbor) {
          d.spans.push(
            d.edge === "end"
              ? { index: found[1].index, startMs: r.endMs, endMs: neighbor.endMs }
              : { index: found[1].index, startMs: neighbor.startMs, endMs: r.startMs },
          );
        }
        d.changed = true;
        (d as Drag & { snappedAt?: number | null }).snappedAt = r.snappedAt;
      } else {
        if (!d.started && Math.hypot(x - d.x, y - d.y) < CLICK_PX) return;
        d.started = true;
        const moving = found.map((f) => f.index);
        const primary = found[d.from.findIndex((f) => f.startMs === d.primary.startMs)]?.index ?? moving[0];
        const r = moveMarksDrag({ marks: track.marks, moving, primary, deltaMs: xToTime(x, v) - d.ms, durationMs: dd.durationMs, frameMs: dd.frameMs, snap });
        d.spans = r.spans;
        (d as Drag & { snappedAt?: number | null }).snappedAt = r.snappedAt;
      }
      redraw((n) => n + 1);
      return;
    }
    if (d.kind === "scrub") {
      useSequencer.getState().setPlayhead(xToTime(x, v));
    } else if (d.kind === "marquee") {
      d.x1 = x;
      d.y1 = y - tp + sy;
    } else if (d.kind === "resize") {
      const snap = altKey ? undefined : { targets: d.targets, thresholdMs: threshold };
      const r = resizeDrag({ item: d.item, edge: d.edge, ms: xToTime(x, v), minMs: dd.frameMs, durationMs: dd.durationMs, frameMs: dd.frameMs, bounds: d.bounds, snap });
      d.result = r;
      (d as Drag & { snappedAt?: number | null }).snappedAt = r.snappedAt;
    } else {
      if (!d.started && Math.hypot(x - d.x, y - d.y) < CLICK_PX) return;
      d.started = true;
      const fromLane = laneAt(ls, d.rowsY);
      // Above the first row or below the last, the nearest row (not back to where it came from).
      const last = ls[ls.length - 1];
      const toLane = last ? laneAt(ls, Math.max(0, Math.min(last.y + last.h - 1, y - tp + sy))) : null;
      const deltaLanes = fromLane && toLane ? ls.indexOf(toLane) - ls.indexOf(fromLane) : 0;
      const snap = altKey ? undefined : { targets: d.targets, thresholdMs: threshold };
      const r = moveDrag({ items: d.items, primary: d.primary, deltaMs: xToTime(x, v) - d.ms, deltaLanes, laneCount: ls.length, durationMs: dd.durationMs, frameMs: dd.frameMs, snap });
      // Drawn where they'll really land: over another effect, that's a free layer.
      d.moved = placeMove(idx, ls, r.items);
      (d as Drag & { snappedAt?: number | null }).snappedAt = r.snappedAt;
    }
    redraw((n) => n + 1);
  };

  const stopScrolling = () => {
    if (scrollTimer.current !== null) clearInterval(scrollTimer.current);
    scrollTimer.current = null;
  };

  /**
   * While an effect or a mark is held near an edge, scrolls the timeline that way (faster the
   * further in), carrying the drag along; stops once the pointer leaves the edge, or the drag ends.
   */
  const keepScrolling = () => {
    const tick = () => {
      const d = drag.current;
      const p = pointer.current;
      const { size: sz, top: tp, view: v, scrollY: sy, maxScroll: limit, doc: dd } = latest.current;
      const scrolls = scrollsNow(d);
      const rows = d?.kind === "move";
      if (!scrolls || !p || (autoScrollSpeed(p.x, 0, sz.width) === 0 && (!rows || autoScrollSpeed(p.y, tp, sz.height) === 0))) {
        stopScrolling();
        return;
      }
      const step = autoScroll({ x: p.x, y: p.y, width: sz.width, rowsTop: tp, height: sz.height, view: v, scrollY: sy, maxScroll: limit, durationMs: dd.durationMs, rows });
      // At the song's end (or the last row) there's nowhere to go, but the pointer may come back.
      if (!step) return;
      latest.current.view = step.view;
      latest.current.scrollY = step.scrollY;
      setViewState(step.view);
      setScrollY(step.scrollY);
      dragTo(p.x, p.y, p.alt);
      redraw((n) => n + 1);
    };
    if (scrollTimer.current === null) scrollTimer.current = setInterval(tick, AUTO_SCROLL_MS);
  };
  useEffect(() => stopScrolling, []);

  /** Calls the drag off: nothing it did is kept. */
  const cancelDrag = () => {
    drag.current = null;
    pointer.current = null;
    stopScrolling();
    redraw((n) => n + 1);
  };

  // Escape calls off a drag in progress (and only that: the selection stays), and so does leaving
  // the window mid-drag (the release may never come back here).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || !drag.current) return;
      e.preventDefault();
      e.stopPropagation();
      cancelDrag();
    };
    const onBlur = () => drag.current && cancelDrag();
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", onBlur);
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  /** Sends a dropped move or resize, drawing `items` where they were dropped until it settles. */
  const sendDrop = (items: DragItem[], edits: SequenceEdit[]) => {
    const key = ++pendingKey.current;
    pending.current = { key, items };
    void useSequencer
      .getState()
      .edit(edits)
      .finally(() => {
        if (pending.current?.key !== key) return;
        pending.current = null;
        redraw((n) => n + 1);
      });
  };

  /** Sends dropped marks (moved or resized), drawing them where they were dropped until it settles,
   * and keeps them selected at their new times. The edits are worked out when their turn comes,
   * from where the marks are then (found by where they were); if one of them has gone or changed
   * meanwhile, the drop is called off and the user is told. The first `selected` marks stay selected
   * (a resized mark, not the neighbor whose edge moved with it). */
  const sendMarks = (trackId: string, moves: MarkMove[], selected = moves.length) => {
    if (moves.every((m) => m.startMs === m.fromStartMs && m.endMs === m.fromEndMs)) return;
    const key = ++pendingKey.current;
    pendingMarks.current = { key, track: trackId, moves };
    const store = useSequencer.getState();
    void store
      .edit((doc) => {
        const track = doc.timingTracks.find((t) => t.id === trackId);
        if (!track) return [];
        const spans = markMovesToSpans(track, moves);
        if (!spans) throw new Error("That mark changed before the move landed, so it stayed where it is. Drag it again.");
        return markMoveEdits(track, spans);
      })
      .then((ok) => ok && store.selectMarks(trackId, moves.slice(0, selected).map((m) => m.startMs)))
      .finally(() => {
        if (pendingMarks.current?.key !== key) return;
        pendingMarks.current = null;
        redraw((n) => n + 1);
      });
  };

  const onPointerUp = () => {
    const d = drag.current;
    drag.current = null;
    pointer.current = null;
    stopScrolling();
    if (!d) return;
    const store = useSequencer.getState();
    const { lanes: ls, index: idx, view: v } = latest.current;
    if (d.kind === "markResize") {
      if (d.changed) sendMarks(d.track, movesOf(d.from, d.spans), 1);
    } else if (d.kind === "markMove") {
      if (d.started) sendMarks(d.track, movesOf(d.from, d.spans));
      else store.selectMarks(d.track, [d.primary.startMs]);
    } else if (d.kind === "scrub") {
      void store.seek(store.playheadMs);
    } else if (d.kind === "marquee") {
      if (Math.abs(d.x1 - d.x0) < CLICK_PX && Math.abs(d.y1 - d.y0) < CLICK_PX) {
        if (d.additive.length === 0) store.select([]);
      } else {
        store.select([...new Set([...d.additive, ...marqueeSelect(idx, ls, v, d)])]);
      }
    } else if (d.kind === "resize") {
      if (d.result.startMs !== d.item.startMs || d.result.endMs !== d.item.endMs) {
        const { startMs, endMs } = d.result;
        sendDrop([{ ...d.item, startMs, endMs }], [{ type: "setEffectTiming", id: d.item.id, startMs, endMs }]);
      }
    } else if (d.started) {
      const edits = moveEdits(d.moved, idx);
      if (edits.length > 0) sendDrop(d.moved, edits);
    } else {
      store.select([d.primary]);
    }
    redraw((n) => n + 1);
  };

  /** Double-click on a timing track: a mark's label to type in place, or a new mark in empty space
   * (a beat long, or half a second). */
  const onDoubleClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const { x, y } = point(e);
    const track = trackAt(y);
    if (!track || track.kind === "phonemes") return;
    const { view: v, doc: d } = latest.current;
    const hit = hitMark(track.marks, x, v);
    const store = useSequencer.getState();
    if (hit) {
      const m = track.marks[hit.index];
      labelDone.current = false;
      setLabelEdit({ track: track.id, startMs: m.startMs, endMs: m.endMs, value: m.label });
      return;
    }
    const snap = latest.current.snapping && !e.altKey ? { targets: snapTargets(d, new Set()), thresholdMs: SNAP_PX / v.pxPerMs } : undefined;
    const span = newMarkSpan({ doc: d, track, ms: xToTime(x, v), snap });
    if (!span) return;
    void store.edit([{ type: "addMarks", track: track.id, marks: [{ ...span, label: "" }] }]).then((ok) => ok && store.selectMarks(track.id, [span.startMs]));
  };

  /** Saves (or, with `save` false, cancels) the label typed in place: once, whichever comes first
   * of Enter, Escape, and the blur that closing the box causes. Built when its turn comes, from
   * where the mark is then; if the mark has moved meanwhile, the user is told. */
  const commitLabel = (save: boolean) => {
    if (labelDone.current) return;
    labelDone.current = true;
    const edit = labelEdit;
    setLabelEdit(null);
    canvasRef.current?.focus();
    if (!edit || !save) return;
    const label = edit.value.trim();
    void useSequencer.getState().edit((latestDoc) => {
      const track = latestDoc.timingTracks.find((t) => t.id === edit.track);
      if (!track) return [];
      const index = track.marks.findIndex((m) => m.startMs === edit.startMs);
      if (index < 0) throw new Error("That mark moved before its label was saved. Double-click it to type the label again.");
      if (track.marks[index].label === label) return [];
      return [{ type: "setMark", track: track.id, index, mark: { ...track.marks[index], label } }];
    });
  };

  // The box sits where the mark was when the typing began.
  const labelBox = (() => {
    if (!labelEdit) return null;
    const k = doc.timingTracks.findIndex((t) => t.id === labelEdit.track);
    if (k < 0) return null;
    const left = Math.max(0, timeToX(labelEdit.startMs, current));
    const w = Math.max(120, timeToX(labelEdit.endMs, current) - left);
    return { left, top: RULER_H + WAVE_H + k * TRACK_H, width: Math.min(w, Math.max(120, width - left)) };
  })();

  const onPointerCancel = cancelDrag;

  const zoomBy = (factor: number) => setView(zoomAt(current, factor, width / 2, doc.durationMs, width));
  const visibleMs = width / current.pxPerMs;
  const thumb = Math.min(1, visibleMs / Math.max(1, doc.durationMs));

  return (
    <div className="flex flex-1 flex-col overflow-hidden" style={{ minHeight: timelineMinHeight(top) }}>
      <div className="flex min-h-0 flex-1">
        <RowHeaders doc={doc} show={show} lanes={lanes} top={top} scrollY={scrollY} rowsViewport={rowsViewport} />
        <div ref={bodyRef} className="relative min-w-0 flex-1">
          <canvas
            ref={canvasRef}
            tabIndex={0}
            role="application"
            aria-label="Timeline"
            aria-roledescription="timeline"
            aria-description="Drag effects to move them, drag their edges to change their length, drag across empty space to select several. Arrow keys move the playhead or the selected effects. On a timing track, drag marks or their edges, double-click a mark to type its label or empty space to add one, and press T as the music plays to tap marks in."
            className="absolute inset-0 h-full w-full touch-none outline-none"
            onDoubleClick={onDoubleClick}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={onPointerCancel}
            onPointerLeave={(e) => !drag.current && (e.currentTarget.style.cursor = "default")}
          />
          {labelEdit && labelBox && (
            <input
              autoFocus
              aria-label="Mark label"
              value={labelEdit.value}
              onChange={(e) => setLabelEdit({ ...labelEdit, value: e.target.value })}
              onFocus={(e) => e.currentTarget.select()}
              onBlur={() => commitLabel(true)}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitLabel(true);
                if (e.key === "Escape") {
                  e.preventDefault();
                  e.stopPropagation();
                  commitLabel(false);
                }
              }}
              className="absolute z-10 rounded border border-accent-500 bg-white px-1 text-xs text-neutral-900 outline-none dark:bg-neutral-950 dark:text-neutral-100"
              style={{ left: labelBox.left, top: labelBox.top, width: labelBox.width, height: TRACK_H }}
            />
          )}
          <p className="sr-only" aria-live="polite" data-testid="timeline-announcer">
            {markSelection ? describeMarks(doc, markSelection) : describeSelection(selection, index, show, labels)}
          </p>
          {maxScroll > 0 && (
            <input
              type="range"
              aria-label="Scroll rows"
              className="absolute right-0 w-3 opacity-60 [writing-mode:vertical-lr]"
              style={{ top, height: rowsViewport }}
              min={0}
              max={maxScroll}
              value={scrollY}
              onChange={(e) => setScrollY(Number(e.target.value))}
            />
          )}
        </div>
      </div>
      <div className="flex h-9 shrink-0 items-center gap-2 border-t border-neutral-200 px-2 dark:border-neutral-800">
        <span className="w-40 text-xs text-neutral-500 tabular-nums">
          {formatTime(current.startMs, 100)} – {formatTime(Math.min(doc.durationMs, current.startMs + visibleMs), 100)}
        </span>
        <input
          type="range"
          aria-label="Scroll in time"
          className="min-w-0 flex-1 accent-violet-600"
          min={0}
          max={Math.max(0, doc.durationMs - visibleMs)}
          step={Math.max(1, Math.round(visibleMs / 50))}
          value={current.startMs}
          disabled={thumb >= 1}
          onChange={(e) => setView({ ...current, startMs: Number(e.target.value) })}
        />
        <ToolButton label="Zoom out" onClick={() => zoomBy(1 / 1.6)}>
          <ZoomOut size={15} />
        </ToolButton>
        <ToolButton label="Zoom in" onClick={() => zoomBy(1.6)}>
          <ZoomIn size={15} />
        </ToolButton>
        <ToolButton label="Fit song" onClick={() => setViewState(null)}>
          <Maximize2 size={15} />
        </ToolButton>
      </div>
    </div>
  );
}

/** The selection in words, for screen readers. */
function describeSelection(selection: string[], index: ReturnType<typeof buildIndex>, show: Show | undefined, labels: Map<string, string>): string {
  if (selection.length === 0) return "No effect selected";
  if (selection.length > 1) return `${selection.length} effects selected`;
  const placed = index.byId.get(selection[0]);
  const row = placed && index.rows.get(placed.rowId);
  if (!placed || !row) return "No effect selected";
  const e = placed.effect;
  return `${labels.get(e.params.kind) ?? e.params.kind} on ${targetName(show, row.target)}, ${formatTime(e.startMs)} to ${formatTime(e.endMs)}, selected`;
}

/** Selected timing marks in words, for screen readers. */
function describeMarks(doc: Sequence, selection: { track: string; starts: number[] }): string {
  const track = doc.timingTracks.find((t) => t.id === selection.track);
  if (!track) return "No mark selected";
  if (selection.starts.length > 1) return `${selection.starts.length} marks selected on ${track.name}`;
  const m = track.marks.find((x) => x.startMs === selection.starts[0]);
  if (!m) return "No mark selected";
  return `Mark ${m.label ? `'${m.label}' ` : ""}on ${track.name}, ${formatTime(m.startMs)} to ${formatTime(m.endMs)}, selected`;
}

/** Whether a drag scrolls the timeline near its edges: resizes at once, moves once they've started
 * (a press that hasn't moved far enough to be a drag is still a click). */
function scrollsNow(d: Drag | null): boolean {
  if (!d) return false;
  if (d.kind === "move" || d.kind === "markMove") return d.started;
  return d.kind === "resize" || d.kind === "markResize";
}

function snappedOf(d: Drag): number | null {
  return (d as Drag & { snappedAt?: number | null }).snappedAt ?? null;
}

function ToolButton({ label, onClick, children }: { label: string; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className="rounded p-1.5 text-neutral-600 hover:bg-neutral-200/70 dark:text-neutral-300 dark:hover:bg-neutral-800"
    >
      {children}
    </button>
  );
}

/** Row names beside the lanes, with ways to collapse, reorder, add layers to, and remove rows.
 * (Memoized: the timeline redraws with the playhead many times a second; the names don't.) */
const RowHeaders = memo(function RowHeaders({
  doc,
  show,
  lanes,
  top,
  scrollY,
  rowsViewport,
}: {
  doc: Sequence;
  show: Show | undefined;
  lanes: Lane[];
  top: number;
  scrollY: number;
  rowsViewport: number;
}) {
  const collapsed = useSequencer((s) => s.collapsed);
  const activeRow = useSequencer((s) => s.activeRow);
  const { toggleCollapsed, edit, setActiveRow } = useSequencer.getState();
  const [adding, setAdding] = useState(false);
  const reorder = useRef<{ rowId: string; startY: number; to: number } | null>(null);
  const [dragTo, setDragTo] = useState<number | null>(null);
  const visible = lanes.filter((l) => l.y + l.h >= scrollY && l.y <= scrollY + rowsViewport);

  const rowIndexAt = (y: number) => {
    const lane = laneAt(lanes, Math.max(0, y));
    return lane ? lane.rowIndex : doc.rows.length - 1;
  };

  return (
    <div className="relative flex w-48 shrink-0 flex-col border-r border-neutral-200 text-xs dark:border-neutral-800">
      <div className="shrink-0 bg-neutral-100 dark:bg-[#141419]" style={{ height: top }}>
        <div className="flex items-center px-2 text-neutral-500" style={{ height: RULER_H }}>
          Time
        </div>
        <div className="flex items-center px-2 text-neutral-500" style={{ height: WAVE_H }}>
          {doc.audio ? "Music" : "No music"}
        </div>
        <TimingTrackHeaders doc={doc} />
      </div>
      <div className="relative min-h-0 flex-1 overflow-hidden" role="list" aria-label="Rows">
        {visible.map((lane) => {
          const row = doc.rows[lane.rowIndex];
          const name = targetName(show, row.target);
          const isCollapsed = collapsed.includes(row.id);
          const y = lane.y - scrollY;
          if (!lane.first) {
            return (
              <div key={`${row.id}:${lane.layer}`} className="absolute right-0 left-0 flex items-center pl-8 text-neutral-400" style={{ top: y, height: lane.h }}>
                Layer {lane.layer + 1}
              </div>
            );
          }
          return (
            <div
              key={row.id}
              role="listitem"
              aria-label={name}
              aria-current={activeRow === row.id || undefined}
              className={`group absolute right-0 left-0 flex items-center gap-0.5 border-t border-neutral-200 pr-1 dark:border-neutral-800 ${
                activeRow === row.id ? "bg-accent-50 dark:bg-accent-600/15" : ""
              } ${dragTo === lane.rowIndex ? "border-t-2 border-t-accent-500" : ""}`}
              style={{ top: y, height: lane.h }}
              onClick={() => setActiveRow(row.id)}
            >
              <span
                className="cursor-grab touch-none px-0.5 text-neutral-400"
                aria-hidden
                onPointerDown={(e) => {
                  e.currentTarget.setPointerCapture?.(e.pointerId);
                  reorder.current = { rowId: row.id, startY: e.clientY, to: lane.rowIndex };
                }}
                onPointerMove={(e) => {
                  const r = reorder.current;
                  if (!r) return;
                  const rect = e.currentTarget.parentElement!.parentElement!.getBoundingClientRect();
                  r.to = rowIndexAt(e.clientY - rect.top + scrollY);
                  setDragTo(r.to);
                }}
                onPointerUp={() => {
                  const r = reorder.current;
                  reorder.current = null;
                  setDragTo(null);
                  const from = doc.rows.findIndex((x) => x.id === r?.rowId);
                  if (r && r.to !== from) void edit([{ type: "moveRow", id: r.rowId, index: r.to }]);
                }}
              >
                <GripVertical size={13} />
              </span>
              <button
                type="button"
                aria-label={isCollapsed ? `Show ${name}'s layers` : `Fold ${name}'s layers into one line`}
                aria-expanded={!isCollapsed}
                title={isCollapsed ? "Show each layer" : "Fold the layers into one line"}
                className="rounded p-1 text-neutral-500 hover:bg-neutral-200/70 dark:hover:bg-neutral-800"
                onClick={(e) => {
                  e.stopPropagation();
                  toggleCollapsed(row.id);
                }}
              >
                {isCollapsed ? <ChevronRight size={13} /> : <ChevronDown size={13} />}
              </button>
              <span className="min-w-0 flex-1 truncate font-medium" title={name}>
                {name}
              </span>
              <span className="hidden items-center group-focus-within:flex group-hover:flex">
                <RowButton label={`Move ${name} up`} disabled={lane.rowIndex === 0} onClick={() => edit([{ type: "moveRow", id: row.id, index: lane.rowIndex - 1 }])}>
                  ↑
                </RowButton>
                <RowButton label={`Move ${name} down`} disabled={lane.rowIndex === doc.rows.length - 1} onClick={() => edit([{ type: "moveRow", id: row.id, index: lane.rowIndex + 1 }])}>
                  ↓
                </RowButton>
                <RowButton label={`Add a layer to ${name}`} onClick={() => edit([{ type: "addLayer", row: row.id }])}>
                  <Layers size={12} />
                </RowButton>
                <RowButton label={`Remove ${name}'s row`} onClick={() => edit([{ type: "removeRow", id: row.id }])}>
                  <Trash2 size={12} />
                </RowButton>
              </span>
            </div>
          );
        })}
        <div className="absolute right-0 left-0 px-2 py-1" style={{ top: lanes.length ? lanes[lanes.length - 1].y + LANE_H - scrollY : 0 }}>
          <button
            type="button"
            onClick={() => setAdding(true)}
            className="flex items-center gap-1 rounded px-1.5 py-1 text-accent-600 hover:bg-neutral-200/70 dark:text-accent-400 dark:hover:bg-neutral-800"
          >
            <Plus size={13} /> Add row
          </button>
        </div>
      </div>
      {adding && <AddRowMenu doc={doc} show={show} onClose={() => setAdding(false)} />}
    </div>
  );
});

function RowButton({ label, onClick, disabled, children }: { label: string; onClick: () => void; disabled?: boolean; children: React.ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      className="rounded px-1 py-0.5 text-neutral-500 hover:bg-neutral-200/70 disabled:opacity-30 dark:hover:bg-neutral-800"
    >
      {children}
    </button>
  );
}

/** Picks a prop, a submodel, or a group for a new row (or adds a row for every prop not on the timeline yet). */
export function AddRowMenu({ doc, show, onClose }: { doc: Sequence; show: Show | undefined; onClose: () => void }) {
  const edit = useSequencer((s) => s.edit);
  const used = new Set(doc.rows.map((r) => targetKey(r.target)));
  const groups = show?.groups ?? [];
  const props = show?.props ?? [];
  const missing = props.filter((p) => !used.has(p.id));
  const add = async (targets: SequenceTarget[]) => {
    onClose();
    await edit(targets.map((target) => ({ type: "addRow" as const, row: newRow(target) })));
  };
  const menu = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  // The menu takes the focus (and gives it back when it closes); Escape closes it.
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    menu.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      close.current();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      if (opener?.isConnected) opener.focus();
    };
  }, []);
  return (
    <div
      ref={menu}
      role="dialog"
      aria-label="Add a row"
      className="absolute bottom-2 left-2 z-30 flex max-h-96 w-64 flex-col rounded-lg border border-neutral-200 bg-white p-2 text-sm shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
    >
      <button
        type="button"
        disabled={missing.length === 0}
        className="mb-1 rounded-md bg-accent-50 px-2 py-1.5 text-left font-medium text-accent-700 hover:bg-accent-100 disabled:opacity-40 dark:bg-accent-600/15 dark:text-accent-300 dark:hover:bg-accent-600/25"
        onClick={() => add(missing.map((p) => ({ prop: p.id })))}
      >
        Add every prop ({missing.length})
      </button>
      <p className="px-1 pb-1 text-xs text-neutral-500">Or pick one: a row lights one prop, one of its submodels, or a group of props as one picture.</p>
      <div className="flex-1 overflow-auto">
        {groups.length > 0 && <p className="px-1 pt-1 text-xs font-semibold text-neutral-500">Groups</p>}
        {groups.map((g) => (
          <button key={g.id} type="button" className="block w-full truncate rounded px-2 py-1 text-left hover:bg-neutral-100 dark:hover:bg-neutral-800" onClick={() => add([{ group: g.id }])}>
            {g.name} {used.has(g.id) && <span className="text-xs text-neutral-400">(has a row)</span>}
          </button>
        ))}
        {props.length > 0 && <p className="px-1 pt-1 text-xs font-semibold text-neutral-500">Props</p>}
        {props.map((p) => (
          <Fragment key={p.id}>
            <button type="button" className="block w-full truncate rounded px-2 py-1 text-left hover:bg-neutral-100 dark:hover:bg-neutral-800" onClick={() => add([{ prop: p.id }])}>
              {p.name} {used.has(p.id) && <span className="text-xs text-neutral-400">(has a row)</span>}
            </button>
            {submodelsOf(p).map((r) => {
              const target = { region: { prop: p.id, region: r.id } };
              return (
                <button
                  key={r.id}
                  type="button"
                  aria-label={`${p.name} / ${r.name}`}
                  className="block w-full truncate rounded py-1 pr-2 pl-6 text-left text-neutral-600 hover:bg-neutral-100 dark:text-neutral-300 dark:hover:bg-neutral-800"
                  onClick={() => add([target])}
                >
                  <span aria-hidden className="text-neutral-400">└ </span>
                  {r.name} {used.has(targetKey(target)) && <span className="text-xs text-neutral-400">(has a row)</span>}
                </button>
              );
            })}
          </Fragment>
        ))}
        {props.length === 0 && groups.length === 0 && <p className="px-2 py-2 text-neutral-500">
            Your show has no props yet. <GoToScreen screen="layout">Add props on Layout</GoToScreen>
          </p>}
      </div>
      <div className="flex justify-end gap-2 border-t border-neutral-200 pt-2 dark:border-neutral-800">
        <button type="button" className="rounded px-2 py-1 hover:bg-neutral-100 dark:hover:bg-neutral-800" onClick={onClose}>
          Cancel
        </button>
      </div>
    </div>
  );
}
