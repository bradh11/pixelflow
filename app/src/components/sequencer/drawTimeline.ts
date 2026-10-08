// Draws the timeline's canvas: the ruler, the music, the timing tracks, and the effects on every
// visible lane. Only what's on screen is drawn, found through the effect index, so thousands of
// effects cost no more than the few dozen in view.

import type { EffectKind, Sequence } from "../../api/sequence";
import type { Waveform } from "../../api/types";
import { type DragItem, type EffectIndex, type Lane, type MarkSpan, type View, effectsInView, marksInView, rulerTicks, timeToX } from "../../lib/timelineMath";

export const RULER_H = 24;
export const WAVE_H = 44;
export const TRACK_H = 18;
export const LANE_H = 30;

/** Height of the band above the rows: ruler, music, and one strip per timing track. */
export function topHeight(doc: Sequence): number {
  return RULER_H + WAVE_H + TRACK_H * doc.timingTracks.length;
}

/** A hue per effect kind, so kinds can be told apart at a glance. */
const HUES: Record<EffectKind, number> = {
  on: 48,
  off: 0,
  colorWash: 290,
  fade: 30,
  chase: 200,
  bars: 170,
  wave: 220,
  twinkle: 60,
  shimmer: 320,
  strobe: 0,
  spiral: 260,
  fire: 15,
  meteors: 190,
  ripple: 140,
  shape: 100,
  fan: 240,
  morph: 80,
  circles: 160,
  faces: 340,
};

interface Theme {
  bg: string;
  band: string;
  laneA: string;
  laneB: string;
  line: string;
  text: string;
  muted: string;
  wave: string;
  wavePlayed: string;
  playhead: string;
  accent: string;
  mark: string;
  markSpanA: string;
  markSpanB: string;
  markSelected: string;
  activeTrack: string;
}

const DARK: Theme = {
  bg: "#0b0b0e",
  band: "#141419",
  laneA: "#111115",
  laneB: "#16161b",
  line: "rgba(255,255,255,0.08)",
  text: "#e5e5e5",
  muted: "#8a8a94",
  wave: "rgba(150,150,165,0.55)",
  wavePlayed: "rgb(139,92,246)",
  playhead: "#f5f5f5",
  accent: "#a78bfa",
  mark: "rgba(167,139,250,0.75)",
  markSpanA: "rgba(167,139,250,0.10)",
  markSpanB: "rgba(167,139,250,0.18)",
  markSelected: "rgba(167,139,250,0.45)",
  activeTrack: "rgba(167,139,250,0.08)",
};

const LIGHT: Theme = {
  bg: "#ffffff",
  band: "#f4f4f6",
  laneA: "#fafafa",
  laneB: "#f1f1f4",
  line: "rgba(0,0,0,0.08)",
  text: "#18181b",
  muted: "#71717a",
  wave: "rgba(110,110,125,0.55)",
  wavePlayed: "rgb(124,58,237)",
  playhead: "#18181b",
  accent: "#7c3aed",
  mark: "rgba(124,58,237,0.7)",
  markSpanA: "rgba(124,58,237,0.07)",
  markSpanB: "rgba(124,58,237,0.14)",
  markSelected: "rgba(124,58,237,0.35)",
  activeTrack: "rgba(124,58,237,0.06)",
};

export interface TimelineScene {
  width: number;
  height: number;
  theme: "dark" | "light";
  doc: Sequence;
  index: EffectIndex;
  lanes: Lane[];
  view: View;
  scrollY: number;
  selection: ReadonlySet<string>;
  playheadMs: number;
  waveform: Waveform | null;
  labels: Map<string, string>;
  /** Effects being dragged, drawn where they'd land. */
  drag: DragItem[] | null;
  /** A time the drag snapped to, marked with a line. */
  snappedAt: number | null;
  /** A marquee being drawn (rows area coordinates). */
  marquee: { x0: number; y0: number; x1: number; y1: number } | null;
  /** Where a palette drop would land (`newLayer`: on a new layer of that lane's row). */
  ghost: { lane: number; startMs: number; endMs: number; newLayer?: boolean } | null;
  /** Selected timing marks (by start time) on one track. */
  markSelection?: { track: string; starts: ReadonlySet<number> } | null;
  /** The timing track taps go to, shaded. */
  activeTrack?: string | null;
  /** Timing marks being dragged, drawn where they'd land. */
  markDrag?: { track: string; spans: MarkSpan[] } | null;
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  const radius = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + radius, y);
  ctx.arcTo(x + w, y, x + w, y + h, radius);
  ctx.arcTo(x + w, y + h, x, y + h, radius);
  ctx.arcTo(x, y + h, x, y, radius);
  ctx.arcTo(x, y, x + w, y, radius);
  ctx.closePath();
}

export function drawTimeline(ctx: CanvasRenderingContext2D, s: TimelineScene) {
  const t = s.theme === "dark" ? DARK : LIGHT;
  const { width, height, view, doc } = s;
  const top = topHeight(doc);
  const t0 = view.startMs;
  const t1 = view.startMs + width / view.pxPerMs;
  ctx.fillStyle = t.bg;
  ctx.fillRect(0, 0, width, height);

  // Rows.
  ctx.save();
  ctx.beginPath();
  ctx.rect(0, top, width, height - top);
  ctx.clip();
  const end = timeToX(doc.durationMs, view);
  const dragged = new Set(s.drag?.map((d) => d.id) ?? []);
  for (const lane of s.lanes) {
    const y = top + lane.y - s.scrollY;
    if (y + lane.h < top || y > height) continue;
    ctx.fillStyle = lane.rowIndex % 2 === 0 ? t.laneA : t.laneB;
    ctx.fillRect(0, y, Math.min(width, end), lane.h);
    if (lane.first) {
      ctx.fillStyle = t.line;
      ctx.fillRect(0, y, width, 1);
    }
  }
  // Bars as faint guides across the rows.
  const bars = doc.timingTracks.find((tr) => tr.kind === "bars") ?? doc.timingTracks[0];
  if (bars && bars.marks.length * 3 < width) {
    ctx.fillStyle = t.line;
    for (const m of bars.marks) {
      if (m.startMs < t0 || m.startMs > t1) continue;
      ctx.fillRect(Math.round(timeToX(m.startMs, view)), top, 1, height - top);
    }
  }
  ctx.font = "11px Inter, ui-sans-serif, system-ui, sans-serif";
  ctx.textBaseline = "middle";
  for (const lane of s.lanes) {
    const y = top + lane.y - s.scrollY;
    if (y + lane.h < top || y > height) continue;
    for (const e of effectsInView(s.index, lane, t0, t1)) {
      const x0 = timeToX(e.startMs, view);
      const x1 = timeToX(e.endMs, view);
      drawEffect(ctx, t, e.params.kind, e.palette.colors, s.labels.get(e.params.kind) ?? e.params.kind, x0, x1, y, lane.h, {
        selected: s.selection.has(e.id),
        faded: dragged.has(e.id),
        fadeIn: e.fadeInMs * view.pxPerMs,
        fadeOut: e.fadeOutMs * view.pxPerMs,
      });
    }
  }
  if (s.drag) {
    for (const item of s.drag) {
      const lane = s.lanes[item.lane];
      const placed = s.index.byId.get(item.id);
      if (!lane || !placed) continue;
      const e = placed.effect;
      const y = top + lane.y - s.scrollY;
      drawEffect(ctx, t, e.params.kind, e.palette.colors, s.labels.get(e.params.kind) ?? "", timeToX(item.startMs, view), timeToX(item.endMs, view), y, lane.h, {
        selected: true,
        faded: false,
        fadeIn: 0,
        fadeOut: 0,
      });
    }
  }
  if (s.ghost) {
    const lane = s.lanes[s.ghost.lane];
    if (lane) {
      const y = top + lane.y - s.scrollY + 2;
      ctx.setLineDash([4, 3]);
      ctx.strokeStyle = t.accent;
      ctx.lineWidth = 1.5;
      const x0 = timeToX(s.ghost.startMs, view);
      const w = Math.max(2, timeToX(s.ghost.endMs, view) - x0 - 1);
      if (s.ghost.newLayer) {
        // Over the row's busy last lane: a solid label, so it reads above the effects there.
        const label = "+ New layer";
        const labelW = Math.max(w, 72);
        ctx.fillStyle = t.bg;
        ctx.fillRect(Math.max(0, x0), y, labelW, lane.h - 4);
        ctx.fillStyle = t.accent;
        ctx.textBaseline = "middle";
        ctx.fillText(label, Math.max(0, x0) + 5, y + (lane.h - 5) / 2);
      }
      ctx.strokeRect(x0 + 0.5, y + 0.5, w, lane.h - 5);
      ctx.setLineDash([]);
    }
  }
  if (s.marquee) {
    const { x0, y0, x1, y1 } = s.marquee;
    ctx.fillStyle = s.theme === "dark" ? "rgba(167,139,250,0.12)" : "rgba(124,58,237,0.1)";
    ctx.strokeStyle = t.accent;
    ctx.lineWidth = 1;
    const [x, y, w, h] = [Math.min(x0, x1), top + Math.min(y0, y1) - s.scrollY, Math.abs(x1 - x0), Math.abs(y1 - y0)];
    ctx.fillRect(x, y, w, h);
    ctx.strokeRect(x + 0.5, y + 0.5, w, h);
  }
  ctx.restore();

  // The band above the rows.
  ctx.fillStyle = t.band;
  ctx.fillRect(0, 0, width, top);
  ctx.fillStyle = t.line;
  ctx.fillRect(0, RULER_H, width, 1);
  ctx.fillRect(0, RULER_H + WAVE_H, width, 1);
  ctx.fillRect(0, top - 1, width, 1);
  const ticks = rulerTicks(view, width);
  ctx.fillStyle = t.muted;
  for (const m of ticks.minor) ctx.fillRect(Math.round(m.x), RULER_H - 5, 1, 5);
  ctx.textBaseline = "top";
  for (const m of ticks.major) {
    ctx.fillRect(Math.round(m.x), RULER_H - 10, 1, 10);
    ctx.fillStyle = t.text;
    ctx.fillText(m.label, Math.round(m.x) + 3, 4);
    ctx.fillStyle = t.muted;
  }
  if (end < width) {
    ctx.fillStyle = s.theme === "dark" ? "rgba(0,0,0,0.45)" : "rgba(0,0,0,0.06)";
    ctx.fillRect(end, 0, width - end, height);
  }

  // The music.
  const w = s.waveform;
  if (w && w.peaks.length > 0 && w.durationMs > 0) {
    const mid = RULER_H + WAVE_H / 2;
    const msPerPeak = w.durationMs / w.peaks.length;
    const playX = timeToX(s.playheadMs, view);
    const first = Math.max(0, Math.floor(t0 / msPerPeak));
    const last = Math.min(w.peaks.length, Math.ceil(t1 / msPerPeak) + 1);
    const bar = (x: number, width: number, peak: number) => {
      const h = Math.max(1, peak * (WAVE_H - 6));
      ctx.fillStyle = x < playX ? t.wavePlayed : t.wave;
      ctx.fillRect(x, mid - h / 2, width, h);
    };
    if (msPerPeak * view.pxPerMs > 1) {
      // Zoomed in past one value per pixel: each value fills the width up to the next one.
      for (let i = first; i < last; i++) {
        const x0 = Math.floor(timeToX(i * msPerPeak, view));
        const x1 = Math.floor(timeToX((i + 1) * msPerPeak, view));
        bar(x0, Math.max(1, x1 - x0), w.peaks[i]);
      }
    } else {
      // One bar per screen pixel (the loudest value in it), however far out the view is.
      let px = -1;
      let peak = 0;
      for (let i = first; i < last; i++) {
        const x = Math.floor(timeToX(i * msPerPeak, view));
        if (x !== px) {
          if (px >= 0) bar(px, 1, peak);
          px = x;
          peak = 0;
        }
        peak = Math.max(peak, w.peaks[i]);
      }
      if (px >= 0) bar(px, 1, peak);
    }
  }

  // Timing tracks: each mark a span with a tick at its start and its label inside.
  ctx.textBaseline = "middle";
  doc.timingTracks.forEach((track, k) => {
    const y = RULER_H + WAVE_H + k * TRACK_H;
    if (track.id === s.activeTrack) {
      ctx.fillStyle = t.activeTrack;
      ctx.fillRect(0, y, width, TRACK_H);
    }
    const selected = s.markSelection?.track === track.id ? s.markSelection.starts : null;
    const dragging = s.markDrag?.track === track.id ? s.markDrag.spans : null;
    const moving = new Set(dragging?.map((d) => d.index) ?? []);
    const dense = track.marks.length > 0 && (track.marks.length * 4) / Math.max(1, doc.durationMs * view.pxPerMs) > 1;
    const [first, end] = marksInView(track.marks, t0, t1);
    let lastLabel = -Infinity;
    const drawMark = (i: number, startMs: number, endMs: number, label: string, highlighted: boolean) => {
      const x0 = Math.round(timeToX(startMs, view));
      const x1 = Math.round(timeToX(endMs, view));
      const w = Math.max(1, x1 - x0);
      if (!dense || highlighted) {
        ctx.fillStyle = highlighted ? t.markSelected : i % 2 === 0 ? t.markSpanA : t.markSpanB;
        ctx.fillRect(x0, y + 2, w, TRACK_H - 4);
      }
      ctx.fillStyle = highlighted ? t.accent : t.mark;
      ctx.fillRect(x0, y + 2, 1, TRACK_H - 4);
      if (!dense && label && x0 - lastLabel > 24 && w > 14) {
        ctx.save();
        ctx.beginPath();
        ctx.rect(x0, y, w, TRACK_H);
        ctx.clip();
        ctx.fillStyle = highlighted ? t.text : t.muted;
        ctx.fillText(label, Math.max(x0, 0) + 3, y + TRACK_H / 2);
        ctx.restore();
        lastLabel = x0;
      }
    };
    for (let i = first; i < end; i++) {
      const m = track.marks[i];
      if (moving.has(i)) {
        ctx.globalAlpha = 0.35;
        drawMark(i, m.startMs, m.endMs, m.label, false);
        ctx.globalAlpha = 1;
        continue;
      }
      drawMark(i, m.startMs, m.endMs, m.label, selected?.has(m.startMs) ?? false);
    }
    lastLabel = -Infinity;
    for (const d of dragging ?? []) {
      const m = track.marks[d.index];
      if (m) drawMark(d.index, d.startMs, d.endMs, m.label, true);
    }
  });

  // Snap line and playhead over everything.
  if (s.snappedAt !== null) {
    ctx.fillStyle = t.accent;
    ctx.fillRect(Math.round(timeToX(s.snappedAt, view)), 0, 1, height);
  }
  const x = Math.round(timeToX(s.playheadMs, view));
  if (x >= -1 && x <= width + 1) {
    ctx.fillStyle = t.playhead;
    ctx.fillRect(x - 0.5, 0, 1.5, height);
    ctx.beginPath();
    ctx.moveTo(x - 5, 0);
    ctx.lineTo(x + 5, 0);
    ctx.lineTo(x, 7);
    ctx.closePath();
    ctx.fill();
  }
}

function drawEffect(
  ctx: CanvasRenderingContext2D,
  t: Theme,
  kind: EffectKind,
  colors: string[],
  label: string,
  x0: number,
  x1: number,
  laneY: number,
  laneH: number,
  o: { selected: boolean; faded: boolean; fadeIn: number; fadeOut: number },
) {
  const y = laneY + 3;
  const h = laneH - 6;
  const w = Math.max(2, x1 - x0 - 1);
  const hue = HUES[kind] ?? 0;
  const dark = t === DARK;
  ctx.globalAlpha = o.faded ? 0.35 : 1;
  ctx.fillStyle = kind === "off" ? (dark ? "#2a2a30" : "#d4d4d8") : `hsl(${hue} ${dark ? 45 : 60}% ${dark ? 28 : 82}%)`;
  roundRect(ctx, x0, y, w, h, 4);
  ctx.fill();
  if (o.fadeIn > 1 || o.fadeOut > 1) {
    ctx.fillStyle = dark ? "rgba(0,0,0,0.35)" : "rgba(255,255,255,0.55)";
    if (o.fadeIn > 1) {
      ctx.beginPath();
      ctx.moveTo(x0, y);
      ctx.lineTo(x0 + Math.min(o.fadeIn, w), y);
      ctx.lineTo(x0, y + h);
      ctx.closePath();
      ctx.fill();
    }
    if (o.fadeOut > 1) {
      ctx.beginPath();
      ctx.moveTo(x0 + w, y);
      ctx.lineTo(x0 + w - Math.min(o.fadeOut, w), y);
      ctx.lineTo(x0 + w, y + h);
      ctx.closePath();
      ctx.fill();
    }
  }
  // Palette swatches along the bottom edge.
  if (w > 10 && colors.length > 0) {
    const sw = Math.min(10, (w - 4) / colors.length);
    colors.forEach((c, i) => {
      ctx.fillStyle = c;
      ctx.fillRect(x0 + 2 + i * sw, y + h - 4, Math.max(1, sw - 1), 3);
    });
  }
  // The name stays in view when the effect starts left of the view.
  const textX = Math.max(x0, 0);
  if (x0 + w - textX > 34) {
    ctx.save();
    ctx.beginPath();
    ctx.rect(x0, y, w, h);
    ctx.clip();
    ctx.fillStyle = dark ? "#f4f4f5" : "#18181b";
    ctx.fillText(label, textX + 5, y + (h - 3) / 2);
    ctx.restore();
  }
  if (o.selected) {
    ctx.strokeStyle = t.accent;
    ctx.lineWidth = 2;
    roundRect(ctx, x0 + 1, y + 1, w - 2, h - 2, 3);
    ctx.stroke();
  }
  ctx.globalAlpha = 1;
}
