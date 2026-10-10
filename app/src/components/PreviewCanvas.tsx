import { useCallback, useEffect, useMemo, useRef } from "react";
import type { PreviewProp } from "../api/types";
import { boxOfPoints, fitView, unionBox } from "../lib/layoutMath";
import { DOT_RADIUS, drawPixels } from "../lib/pixelBatches";

const BACKDROP = "#050505";
/** Pixels with no live data are gray; while something plays, pixels that are off aren't drawn. */
const COLORS = { unlit: "rgba(128, 128, 128, 0.25)", selected: "rgba(128, 128, 128, 0.25)" };
const NONE: ReadonlySet<string> = new Set();

/**
 * Draws every prop's pixels (front view) in their current colors from `frame` (a show frame), as
 * crisp dots like the Sequence screen's preview, with as much glow around the lit ones as `glow`
 * says (0–1; none unless given).
 */
export function PreviewCanvas({ props, frame, glow = 0 }: { props: PreviewProp[]; frame: Uint8Array | null; glow?: number }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const box = useMemo(() => unionBox(props.map((p) => boxOfPoints(p.points))), [props]);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const ratio = window.devicePixelRatio || 1;
    const size = { width: canvas.clientWidth, height: canvas.clientHeight };
    if (canvas.width !== Math.round(size.width * ratio) || canvas.height !== Math.round(size.height * ratio)) {
      canvas.width = Math.round(size.width * ratio);
      canvas.height = Math.round(size.height * ratio);
    }
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.fillStyle = BACKDROP;
    ctx.fillRect(0, 0, size.width, size.height);
    if (!box) return;
    const view = fitView(box, size, 24);
    const radius = Math.min(4, Math.max(1.2, view.zoom * DOT_RADIUS));
    drawPixels(ctx, props, frame, view, size, NONE, COLORS, radius, ratio, glow);
  }, [props, frame, box, glow]);

  useEffect(draw, [draw]);

  // Redraw when the canvas changes size (window resize, side panels opening).
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const observer = new ResizeObserver(() => draw());
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [draw]);

  return <canvas ref={canvasRef} role="img" aria-label="Preview" className="h-full w-full rounded-lg" />;
}
