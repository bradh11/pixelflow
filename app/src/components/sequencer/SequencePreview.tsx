import { Maximize2, PanelRight, PanelTop } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Sequence } from "../../api/sequence";
import type { PreviewProp, PreviewSet3d, Show } from "../../api/types";
import { backgroundBox, boxOfPoints, fitView, toScreen, unionBox } from "../../lib/layoutMath";
import { DOT_RADIUS, drawPixels } from "../../lib/pixelBatches";
import { targetNodes, targetPreview } from "../../lib/submodels";
import { useSequencer } from "../../state/sequencer";
import { useApp } from "../../state/store";
import { showViewKey, useView3d } from "../../state/view3d";
import { type PhotoImage, useBackgroundImage, usePreviewProps, usePreviewProps3d } from "../layout/useLayoutData";
import { GlowControl } from "../layout3d/GlowControl";
import { Layout3dView } from "../layout3d/Layout3dView";
import { ModeSwitch } from "../layout3d/ModeSwitch";
import { IconButton } from "../ui";
import { everyFrame, frameTarget } from "../../lib/avSync";
import { pipeline, playClock, usePreviewSync } from "../../state/previewSync";

/** The display's width over its height (props and photo), for sizing the preview to it. */
export function useDisplayAspect(): number {
  const show = useApp((s) => s.snapshot?.show);
  const preview = usePreviewProps();
  const photo = useBackgroundImage(show?.background?.path);
  return useMemo(() => {
    const bg = show?.background ?? null;
    const box = unionBox([...preview.props.map((p) => boxOfPoints(p.points)), bg ? backgroundBox(bg, photo.aspect) : null]);
    if (!box) return 16 / 9;
    const w = box.maxX - box.minX;
    const h = box.maxY - box.minY;
    return w > 0 && h > 0 ? Math.min(4, Math.max(0.5, w / h)) : 16 / 9;
  }, [show?.background, preview.props, photo.aspect]);
}

const BACKDROP = "#0a0a0c";
const COLORS = { unlit: "rgba(200, 200, 200, 0.35)", selected: "#a78bfa", dark: "rgba(70, 70, 70, 0.55)" };
const NONE: ReadonlySet<string> = new Set();

/**
 * The show as it looks at the playhead, flat like the Layout screen or in 3D (view only, with the
 * show's camera from the Layout and Play screens): rendered by the engine for the current moment
 * while editing, and live while playing. "Selected row only" shows just the pixels of the row
 * being worked on (only a submodel's own pixels, for a row on a submodel).
 */
export function SequencePreview({
  doc,
  expanded,
  onExpand,
  place,
  onPlace,
}: {
  doc: Sequence;
  expanded?: boolean;
  onExpand?: (expanded: boolean) => void;
  /** Where the preview sits, and how to move it (only where it can go beside the timeline). */
  place?: "side" | "top";
  onPlace?: (place: "side" | "top") => void;
}) {
  const snapshot = useApp((s) => s.snapshot);
  const show = snapshot?.show;
  const preview = usePreviewProps();
  const photo = useBackgroundImage(show?.background?.path);
  const api = useSequencer((s) => s.api);
  const playheadMs = useSequencer((s) => s.playheadMs);
  const revision = useSequencer((s) => s.revision);
  const running = useSequencer((s) => s.status?.state === "playing");
  const activeRow = useSequencer((s) => s.activeRow);
  const mode = useView3d((s) => s.sequenceMode);
  const setMode = useView3d((s) => s.setSequenceMode);
  const in3d = mode === "3d";
  const preview3d = usePreviewProps3d(in3d);
  const [onlyRow, setOnlyRow] = useState(false);
  const aspect = useDisplayAspect();
  const [frame, setFrame] = useState<Uint8Array | null>(null);
  /** The moment to draw next, while a frame is on its way: scrubbing asks for one frame at a time
   * and skips the moments it passed meanwhile. */
  const still = useRef<{ wanted: number | null; busy: boolean; live: boolean }>({ wanted: null, busy: false, live: true });

  // Still: render the moment at the playhead (the latest moment wins).
  useEffect(() => {
    if (!api || running) return;
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
          (f) => s.live && useSequencer.getState().status?.state !== "playing" && setFrame(f.length ? f : null),
          () => s.live && setFrame(null),
        )
        .finally(() => {
          s.busy = false;
          next();
        });
    };
    next();
  }, [api, playheadMs, revision, running, doc]);
  useEffect(() => {
    const s = still.current;
    s.live = true;
    return () => {
      s.live = false;
    };
  }, []);

  // Playing: the moment that will be heard when the frame reaches the screen (see lib/avSync),
  // one frame asked for at a time. Controllers get the engine's own frames, untouched.
  useEffect(() => {
    if (!api || !running) return;
    let pending = false;
    let live = true;
    const stop = everyFrame(() => {
      const reading = playClock.reading();
      if (pending || !reading) return;
      const sentAt = performance.now();
      const { offsetMs } = usePreviewSync.getState();
      const ms = frameTarget(reading, sentAt, pipeline.fetchMs ?? 0, pipeline.refreshMs, offsetMs, doc.durationMs);
      pending = true;
      api.sequenceDocFrame(Math.round(ms)).then(
        (f) => {
          pending = false;
          pipeline.noteFetch(performance.now() - sentAt);
          if (live && useSequencer.getState().status?.state === "playing" && f.length) setFrame(f);
        },
        () => {
          pending = false;
        },
      );
    });
    return () => {
      live = false;
      stop();
    };
  }, [api, running, doc.durationMs]);

  // The selected row's pixels, by prop: whole props, a submodel's pixels, or a group's members.
  const segments = useMemo(() => {
    const row = onlyRow ? doc.rows.find((r) => r.id === activeRow) : undefined;
    if (!row || !show) return null;
    const flat = new Map(preview.props.map((p) => [p.prop, p.points]));
    return targetNodes(
      show,
      row.target,
      (prop) => (flat.get(prop.id)?.length ?? 0) >> 1,
      (prop) => flat.get(prop.id) ?? [],
    );
  }, [onlyRow, doc.rows, activeRow, show, preview.props]);
  const props: PreviewProp[] = useMemo(() => (segments ? targetPreview(preview.props, segments, "2d") : preview.props), [segments, preview.props]);
  const shown3d: PreviewSet3d = useMemo(
    () => (segments ? { revision: preview3d.revision, props: targetPreview(preview3d.props, segments, "3d") } : preview3d),
    [segments, preview3d],
  );
  // Just the row: without the photo, as in 2D.
  const photo3d: PhotoImage = useMemo(() => (onlyRow ? { ...photo, image: null } : photo), [onlyRow, photo]);

  return (
    <section aria-label="Preview" className="flex h-full w-full flex-col gap-1.5">
      <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1">
        <span className="inline-flex items-center gap-0.5">
          <ModeSwitch mode={mode} onChange={setMode} />
          {/* Beside the timeline the column is narrow, at the window's edge. */}
          <GlowControl iconOnly={place === "side"} align={place === "side" ? "right" : "left"} />
        </span>
        <label className="flex items-center gap-1.5 text-xs text-neutral-600 dark:text-neutral-300" title="Show only the pixels of the row being worked on">
          <input type="checkbox" checked={onlyRow} onChange={(e) => setOnlyRow(e.target.checked)} />
          {place === "side" ? "Row only" : "Selected row only"}
        </label>
        {onExpand && (
          <button
            type="button"
            aria-pressed={expanded}
            aria-label="Bigger preview"
            title={expanded ? "Give the timeline its room back" : "Give the preview most of the screen"}
            onClick={() => onExpand(!expanded)}
            className={`ml-auto inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs text-neutral-600 hover:bg-neutral-200/70 dark:text-neutral-300 dark:hover:bg-neutral-800 ${
              expanded ? "bg-accent-50 text-accent-600 dark:bg-accent-600/15 dark:text-accent-400" : ""
            }`}
          >
            {/* Beside the timeline the column is narrow: the icon alone, named on hover. */}
            <Maximize2 size={13} aria-hidden /> <span className={place === "side" ? "sr-only" : undefined}>Bigger preview</span>
          </button>
        )}
        {onPlace && (
          <IconButton
            label={place === "side" ? "Preview above the timeline" : "Preview beside the timeline"}
            className={`${onExpand ? "" : "ml-auto"} rounded-md p-1 text-neutral-600 hover:bg-neutral-200/70 dark:text-neutral-300 dark:hover:bg-neutral-800`}
            onClick={() => onPlace(place === "side" ? "top" : "side")}
          >
            {place === "side" ? <PanelTop size={14} aria-hidden /> : <PanelRight size={14} aria-hidden />}
          </IconButton>
        )}
      </div>
      <div
        className={`relative min-h-0 ${place === "side" ? "w-full" : "flex-1"}`}
        // Beside the timeline: as tall as the display needs at the column's width, within reason.
        style={place === "side" ? { aspectRatio: `${aspect} / 1`, maxHeight: expanded ? "75vh" : "50vh", minHeight: 120 } : undefined}
      >
        {in3d && snapshot ? (
          <Layout3dView preview={shown3d} show={snapshot.show} photo={photo3d} storageKey={showViewKey(snapshot.path, snapshot.show.name)} frame={frame} />
        ) : (
          <FlatPreview props={props} frame={frame} show={show} photo={photo} onlyRow={onlyRow} />
        )}
        {preview.props.length === 0 && (
          <p className="pointer-events-none absolute inset-0 flex items-center justify-center text-sm text-neutral-400">Add props on the Layout screen to see them here.</p>
        )}
      </div>
    </section>
  );
}

/** The props drawn flat, front on, as on the Layout screen (with the photo behind, dimmed). */
function FlatPreview({ props, frame, show, photo, onlyRow }: { props: PreviewProp[]; frame: Uint8Array | null; show: Show | undefined; photo: PhotoImage; onlyRow: boolean }) {
  const glow = useView3d((s) => s.glow);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });

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
    ctx.clearRect(0, 0, size.width, size.height);
    const bg = show?.background ?? null;
    const box = unionBox([...props.map((p) => boxOfPoints(p.points)), bg && !onlyRow ? backgroundBox(bg, photo.aspect) : null]);
    const view = fitView(box, size, 16);
    // The night sky only behind the display itself (around it, the panel shows through).
    ctx.fillStyle = BACKDROP;
    if (box) {
      const a = toScreen(view, size, { x: box.minX, y: box.maxY });
      const b = toScreen(view, size, { x: box.maxX, y: box.minY });
      const pad = 8;
      ctx.beginPath();
      ctx.roundRect(a.x - pad, a.y - pad, b.x - a.x + 2 * pad, b.y - a.y + 2 * pad, 6);
      ctx.fill();
    } else {
      ctx.fillRect(0, 0, size.width, size.height);
    }
    if (bg && photo.image && !onlyRow) {
      const b = backgroundBox(bg, photo.aspect);
      const tl = toScreen(view, size, { x: b.minX, y: b.maxY });
      const br = toScreen(view, size, { x: b.maxX, y: b.minY });
      // Dimmed, so the lights stand out as they would at night.
      ctx.globalAlpha = bg.opacity * 0.5;
      ctx.drawImage(photo.image, tl.x, tl.y, br.x - tl.x, br.y - tl.y);
      ctx.globalAlpha = 1;
    }
    const radius = Math.min(4, Math.max(1.2, view.zoom * DOT_RADIUS));
    drawPixels(ctx, props, frame, view, size, NONE, COLORS, radius, ratio, glow);
  }, [props, frame, size, show?.background, photo, onlyRow, glow]);

  return <canvas ref={canvasRef} role="img" aria-label="Preview of the show at the playhead" className="h-full w-full rounded-md" />;
}
