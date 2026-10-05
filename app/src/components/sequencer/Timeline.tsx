import { ChevronDown, ChevronRight, GripVertical, Layers, Maximize2, Plus, Trash2, ZoomIn, ZoomOut } from "lucide-react";
import { type PointerEvent as ReactPointerEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { defaultParams, newEffect, newRow, type EffectKind, type Sequence, type SequenceEdit, type SequenceTarget } from "../../api/sequence";
import type { Show, Waveform } from "../../api/types";
import {
  type DragItem,
  type Lane,
  type Placement,
  type View,
  SNAP_PX,
  buildIndex,
  clampView,
  effectBounds,
  fitView,
  followPlayhead,
  formatTime,
  hitEffect,
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
  toggleSelection,
  xToTime,
  zoomAt,
} from "../../lib/timelineMath";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { usePaletteDrag } from "./EffectPalette";
import { LANE_H, RULER_H, TRACK_H, WAVE_H, drawTimeline, topHeight } from "./drawTimeline";

/** Colors a new effect starts with. */
export const DEFAULT_COLORS = ["#ff0000", "#00c000", "#ffffff"];
/** Presses that move less than this (screen pixels) are clicks. */
const CLICK_PX = 3;

type Drag =
  | { kind: "move"; primary: string; items: DragItem[]; moved: Placement[]; x: number; y: number; started: boolean; targets: number[] }
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
  | { kind: "scrub" };

/** Where a palette drop would land; `newLayer` when it would go on a new layer of the row. */
type Ghost = { lane: number; startMs: number; endMs: number; newLayer: boolean };

/** A target's name, from the show. */
export function targetName(show: Show | undefined, target: SequenceTarget): string {
  if ("prop" in target) return show?.props.find((p) => p.id === target.prop)?.name ?? "Missing prop";
  return show?.groups.find((g) => g.id === target.group)?.name ?? "Missing group";
}

/** Music paths relative to the sequence file are found next to it. */
export function resolveAudio(audio: string | null, docPath: string | null): string | null {
  if (!audio) return null;
  if (/^([a-zA-Z]:[\\/]|[\\/])/.test(audio) || !docPath) return audio;
  const folder = docPath.replace(/[\\/][^\\/]*$/, "");
  return `${folder}/${audio}`;
}

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
  const { selection, playheadMs, status, collapsed, snapping, catalog, path, activeRow, docKey, revealAt } = useSequencer();
  const bodyRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const size = useSize(bodyRef);
  const top = topHeight(doc);
  const [view, setViewState] = useState<View | null>(null);
  const [scrollY, setScrollY] = useState(0);
  const [waveform, setWaveform] = useState<Waveform | null>(null);
  const drag = useRef<Drag | null>(null);
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
  const latest = useRef({ doc, index, lanes, view: current, top, scrollY, snapping, selection, maxScroll, catalog, activeRow, playheadMs });
  latest.current = { doc, index, lanes, view: current, top, scrollY, snapping, selection, maxScroll, catalog, activeRow, playheadMs };

  // Fit the song when a different sequence is opened (not when this one is saved somewhere new).
  useEffect(() => setViewState(null), [docKey]);
  useEffect(() => setScrollY((y) => Math.min(y, maxScroll)), [maxScroll]);

  // After a keyboard or problem-list pick: scroll the rows to the active row (or the selected
  // effect's lane) and the time to the selected effect, or else to the playhead.
  useEffect(() => {
    if (revealAt === 0 || size.width === 0) return;
    const s = useSequencer.getState();
    const placed = s.selection.length > 0 ? index.byId.get(s.selection[0]) : undefined;
    const laneIndex = placed ? laneOf(lanes, placed.rowId, placed.layer) : lanes.findIndex((l) => l.rowId === s.activeRow);
    const lane = lanes[laneIndex];
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
    if (status?.state !== "playing" || size.width === 0) return;
    const next = followPlayhead(current, playheadMs, width, doc.durationMs);
    if (next !== current) setViewState(next);
  }, [playheadMs, status?.state]); // eslint-disable-line react-hooks/exhaustive-deps

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
      snappedAt: d && (d.kind === "move" || d.kind === "resize") ? snappedOf(d) : null,
      marquee: d?.kind === "marquee" ? d : null,
      ghost,
    });
  };
  useEffect(draw);

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

  const onPointerDown = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    if (e.button !== 0) return;
    canvasRef.current?.focus();
    const { x, y } = point(e);
    const { view: v, top: tp, scrollY: sy, lanes: ls, index: idx, selection: sel } = latest.current;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    const store = useSequencer.getState();
    if (y < tp) {
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
    drag.current = { kind: "move", primary: hit.id, items, moved: placeMove(idx, ls, items), x, y, started: false, targets: snapFor(ids, e.altKey) };
  };

  const onPointerMove = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    const { x, y } = point(e);
    const d = drag.current;
    const { view: v, top: tp, scrollY: sy, lanes: ls, index: idx, doc: dd } = latest.current;
    const threshold = SNAP_PX / v.pxPerMs;
    if (!d) {
      // Show what a press here would do.
      const lane = y >= tp ? laneAt(ls, y - tp + sy) : null;
      const hit = lane ? hitEffect(idx, lane, x, v) : null;
      e.currentTarget.style.cursor = y < tp ? "text" : !hit ? "default" : hit.part === "body" ? "grab" : "ew-resize";
      return;
    }
    if (d.kind === "scrub") {
      useSequencer.getState().setPlayhead(xToTime(x, v));
    } else if (d.kind === "marquee") {
      d.x1 = x;
      d.y1 = y - tp + sy;
    } else if (d.kind === "resize") {
      const snap = e.altKey ? undefined : { targets: d.targets, thresholdMs: threshold };
      const r = resizeDrag({ item: d.item, edge: d.edge, ms: xToTime(x, v), minMs: dd.frameMs, durationMs: dd.durationMs, frameMs: dd.frameMs, bounds: d.bounds, snap });
      d.result = r;
      (d as Drag & { snappedAt?: number | null }).snappedAt = r.snappedAt;
    } else {
      if (!d.started && Math.hypot(x - d.x, y - d.y) < CLICK_PX) return;
      d.started = true;
      const fromLane = laneAt(ls, d.y - tp + sy);
      const toLane = laneAt(ls, y - tp + sy);
      const deltaLanes = fromLane && toLane ? ls.indexOf(toLane) - ls.indexOf(fromLane) : 0;
      const snap = e.altKey ? undefined : { targets: d.targets, thresholdMs: threshold };
      const r = moveDrag({ items: d.items, primary: d.primary, deltaMs: (x - d.x) / v.pxPerMs, deltaLanes, laneCount: ls.length, durationMs: dd.durationMs, frameMs: dd.frameMs, snap });
      // Drawn where they'll really land: over another effect, that's a free layer.
      d.moved = placeMove(idx, ls, r.items);
      (d as Drag & { snappedAt?: number | null }).snappedAt = r.snappedAt;
    }
    redraw((n) => n + 1);
  };

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

  const onPointerUp = () => {
    const d = drag.current;
    drag.current = null;
    if (!d) return;
    const store = useSequencer.getState();
    const { lanes: ls, index: idx, view: v } = latest.current;
    if (d.kind === "scrub") {
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

  const onPointerCancel = () => {
    drag.current = null;
    redraw((n) => n + 1);
  };

  const zoomBy = (factor: number) => setView(zoomAt(current, factor, width / 2, doc.durationMs, width));
  const visibleMs = width / current.pxPerMs;
  const thumb = Math.min(1, visibleMs / Math.max(1, doc.durationMs));

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex min-h-0 flex-1">
        <RowHeaders doc={doc} show={show} lanes={lanes} top={top} scrollY={scrollY} rowsViewport={rowsViewport} />
        <div ref={bodyRef} className="relative min-w-0 flex-1">
          <canvas
            ref={canvasRef}
            tabIndex={0}
            role="application"
            aria-label="Timeline"
            aria-roledescription="timeline"
            aria-description="Drag effects to move them, drag their edges to change their length, drag across empty space to select several. Arrow keys move the playhead or the selected effects."
            className="absolute inset-0 h-full w-full touch-none outline-none"
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={onPointerCancel}
            onPointerLeave={(e) => !drag.current && (e.currentTarget.style.cursor = "default")}
          />
          <p className="sr-only" aria-live="polite" data-testid="timeline-announcer">
            {describeSelection(selection, index, show, labels)}
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

/** Row names beside the lanes, with ways to collapse, reorder, add layers to, and remove rows. */
function RowHeaders({
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
  const { collapsed, toggleCollapsed, edit, activeRow, setActiveRow } = useSequencer();
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
        {doc.timingTracks.map((t) => (
          <div key={t.id} className="flex items-center truncate px-2 text-neutral-500" style={{ height: TRACK_H }} title={t.name}>
            {t.name}
          </div>
        ))}
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
                className="rounded p-0.5 text-neutral-500 hover:bg-neutral-200/70 dark:hover:bg-neutral-800"
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
}

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

/** Picks a prop or group for a new row (or adds a row for every prop not on the timeline yet). */
export function AddRowMenu({ doc, show, onClose }: { doc: Sequence; show: Show | undefined; onClose: () => void }) {
  const edit = useSequencer((s) => s.edit);
  const used = new Set(doc.rows.map((r) => ("prop" in r.target ? r.target.prop : r.target.group)));
  const groups = show?.groups ?? [];
  const props = show?.props ?? [];
  const missing = props.filter((p) => !used.has(p.id));
  const add = async (targets: SequenceTarget[]) => {
    onClose();
    await edit(targets.map((target) => ({ type: "addRow" as const, row: newRow(target) })));
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div
      role="dialog"
      aria-label="Add a row"
      className="absolute bottom-2 left-2 z-30 flex max-h-96 w-64 flex-col rounded-lg border border-neutral-200 bg-white p-2 text-sm shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
    >
      <p className="px-1 pb-1 text-xs text-neutral-500">A row lights one prop, or a group of props as one picture.</p>
      <div className="flex-1 overflow-auto">
        {groups.length > 0 && <p className="px-1 pt-1 text-xs font-semibold text-neutral-500">Groups</p>}
        {groups.map((g) => (
          <button key={g.id} type="button" className="block w-full truncate rounded px-2 py-1 text-left hover:bg-neutral-100 dark:hover:bg-neutral-800" onClick={() => add([{ group: g.id }])}>
            {g.name} {used.has(g.id) && <span className="text-xs text-neutral-400">(has a row)</span>}
          </button>
        ))}
        {props.length > 0 && <p className="px-1 pt-1 text-xs font-semibold text-neutral-500">Props</p>}
        {props.map((p) => (
          <button key={p.id} type="button" className="block w-full truncate rounded px-2 py-1 text-left hover:bg-neutral-100 dark:hover:bg-neutral-800" onClick={() => add([{ prop: p.id }])}>
            {p.name} {used.has(p.id) && <span className="text-xs text-neutral-400">(has a row)</span>}
          </button>
        ))}
        {props.length === 0 && groups.length === 0 && <p className="px-2 py-2 text-neutral-500">Your show has no props yet. Add some on the Layout screen.</p>}
      </div>
      <div className="flex justify-between gap-2 border-t border-neutral-200 pt-2 dark:border-neutral-800">
        <button type="button" disabled={missing.length === 0} className="rounded px-2 py-1 text-accent-600 hover:bg-neutral-100 disabled:opacity-40 dark:text-accent-400 dark:hover:bg-neutral-800" onClick={() => add(missing.map((p) => ({ prop: p.id })))}>
          Add every prop ({missing.length})
        </button>
        <button type="button" className="rounded px-2 py-1 hover:bg-neutral-100 dark:hover:bg-neutral-800" onClick={onClose}>
          Cancel
        </button>
      </div>
    </div>
  );
}
