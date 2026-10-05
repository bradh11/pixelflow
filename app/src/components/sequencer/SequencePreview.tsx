import { useEffect, useMemo, useRef, useState } from "react";
import type { Sequence } from "../../api/sequence";
import type { PreviewProp } from "../../api/types";
import { backgroundBox, boxOfPoints, fitView, toScreen, unionBox } from "../../lib/layoutMath";
import { batchPixels, drawBatches } from "../../lib/pixelBatches";
import { targetProp } from "../../lib/submodels";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { useBackgroundImage, usePreviewProps } from "../layout/useLayoutData";

const BACKDROP = "#0a0a0c";
const COLORS = { unlit: "rgba(200, 200, 200, 0.35)", selected: "#a78bfa", dark: "rgba(70, 70, 70, 0.55)" };
/** How often the preview picks up colors while the sequence plays. */
const PLAY_MS = 40;

/**
 * The show as it looks at the playhead, drawn like the Layout screen (view only): rendered by the
 * engine for the current moment while editing, and live while playing. "Selected row only" shows
 * just the props of the row being worked on.
 */
export function SequencePreview({ doc }: { doc: Sequence }) {
  const backend = useApp((s) => s.backend);
  const show = useApp((s) => s.snapshot?.show);
  const preview = usePreviewProps();
  const photo = useBackgroundImage(show?.background?.path);
  const api = useSequencer((s) => s.api);
  const playheadMs = useSequencer((s) => s.playheadMs);
  const revision = useSequencer((s) => s.revision);
  const playing = useSequencer((s) => s.status !== null);
  const activeRow = useSequencer((s) => s.activeRow);
  const [onlyRow, setOnlyRow] = useState(false);
  const [frame, setFrame] = useState<Uint8Array | null>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  /** The moment to draw next, while a frame is on its way: scrubbing asks for one frame at a time
   * and skips the moments it passed meanwhile. */
  const still = useRef<{ wanted: number | null; busy: boolean; live: boolean }>({ wanted: null, busy: false, live: true });

  // Still: render the moment at the playhead (the latest moment wins).
  useEffect(() => {
    if (!api || playing) return;
    const s = still.current;
    s.wanted = playheadMs;
    const next = () => {
      if (s.busy || s.wanted === null || !s.live) return;
      const ms = s.wanted;
      s.wanted = null;
      s.busy = true;
      api
        .sequenceDocFrame(ms)
        .then(
          (f) => s.live && !useSequencer.getState().status && setFrame(f.length ? f : null),
          () => s.live && setFrame(null),
        )
        .finally(() => {
          s.busy = false;
          next();
        });
    };
    next();
  }, [api, playheadMs, revision, playing, doc]);
  useEffect(() => {
    const s = still.current;
    s.live = true;
    return () => {
      s.live = false;
    };
  }, []);

  // Playing: follow the engine's live frame.
  useEffect(() => {
    if (!backend || !playing) return;
    let cancelled = false;
    let pending = false;
    const timer = setInterval(() => {
      if (pending) return;
      pending = true;
      backend.liveFrame().then(
        (f) => {
          pending = false;
          if (!cancelled && f.length) setFrame(f);
        },
        () => {
          pending = false;
        },
      );
    }, PLAY_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [backend, playing]);

  const props: PreviewProp[] = useMemo(() => {
    if (!onlyRow) return preview.props;
    const row = doc.rows.find((r) => r.id === activeRow);
    if (!row) return preview.props;
    const target = row.target;
    const group = "group" in target ? show?.groups.find((g) => g.id === target.group) : undefined;
    const ids = new Set(group ? [...group.members, ...(group.submodels ?? []).map((s) => s.prop)] : [targetProp(target)]);
    return preview.props.filter((p) => ids.has(p.prop));
  }, [onlyRow, preview.props, doc.rows, activeRow, show?.groups]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const measure = () => setSize({ width: canvas.clientWidth, height: canvas.clientHeight });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx || size.width === 0) return;
    const ratio = window.devicePixelRatio || 1;
    canvas.width = Math.round(size.width * ratio);
    canvas.height = Math.round(size.height * ratio);
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.fillStyle = BACKDROP;
    ctx.fillRect(0, 0, size.width, size.height);
    const bg = show?.background ?? null;
    const box = unionBox([...props.map((p) => boxOfPoints(p.points)), bg && !onlyRow ? backgroundBox(bg, photo.aspect) : null]);
    const view = fitView(box, size, 16);
    if (bg && photo.image && !onlyRow) {
      const b = backgroundBox(bg, photo.aspect);
      const tl = toScreen(view, size, { x: b.minX, y: b.maxY });
      const br = toScreen(view, size, { x: b.maxX, y: b.minY });
      // Dimmed, so the lights stand out as they would at night.
      ctx.globalAlpha = bg.opacity * 0.5;
      ctx.drawImage(photo.image, tl.x, tl.y, br.x - tl.x, br.y - tl.y);
      ctx.globalAlpha = 1;
    }
    const radius = Math.min(4, Math.max(1.2, view.zoom * 0.05));
    drawBatches(ctx, batchPixels(props, frame, view, size, new Set(), COLORS, radius), radius, ratio);
  }, [props, frame, size, show?.background, photo, onlyRow]);

  return (
    <div className="relative h-full w-full">
      <canvas ref={canvasRef} role="img" aria-label="Preview of the show at the playhead" className="h-full w-full rounded-md" />
      <label className="absolute top-2 right-2 flex items-center gap-1.5 rounded bg-black/50 px-2 py-1 text-xs text-white">
        <input type="checkbox" checked={onlyRow} onChange={(e) => setOnlyRow(e.target.checked)} />
        Selected row only
      </label>
      {preview.props.length === 0 && (
        <p className="pointer-events-none absolute inset-0 flex items-center justify-center text-sm text-neutral-400">
          Add props on the Layout screen to see them here.
        </p>
      )}
    </div>
  );
}
