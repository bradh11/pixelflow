import { useEffect, useMemo, useRef } from "react";
import type { PreviewProp } from "../api/types";

/** Color for pixels that have no live data. */
const UNLIT = "rgba(128, 128, 128, 0.25)";

interface Bounds {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

function boundsOf(props: PreviewProp[]): Bounds | null {
  let b: Bounds | null = null;
  for (const p of props) {
    for (let i = 0; i + 1 < p.points.length; i += 2) {
      const [x, y] = [p.points[i], p.points[i + 1]];
      b = b
        ? { minX: Math.min(b.minX, x), minY: Math.min(b.minY, y), maxX: Math.max(b.maxX, x), maxY: Math.max(b.maxY, y) }
        : { minX: x, minY: y, maxX: x, maxY: y };
    }
  }
  return b;
}

/** Draws every prop's pixels (front view) in their current colors from `frame` (a show frame). */
export function PreviewCanvas({ props, frame }: { props: PreviewProp[]; frame: Uint8Array | null }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const bounds = useMemo(() => boundsOf(props), [props]);
  const pixelCount = useMemo(() => props.reduce((n, p) => n + p.points.length / 2, 0), [props]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const ratio = window.devicePixelRatio || 1;
    const { clientWidth: w, clientHeight: h } = canvas;
    if (canvas.width !== Math.round(w * ratio) || canvas.height !== Math.round(h * ratio)) {
      canvas.width = Math.round(w * ratio);
      canvas.height = Math.round(h * ratio);
    }
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.fillStyle = "#050505";
    ctx.fillRect(0, 0, w, h);
    if (!bounds) return;
    const pad = 24;
    const spanX = Math.max(bounds.maxX - bounds.minX, 1e-6);
    const spanY = Math.max(bounds.maxY - bounds.minY, 1e-6);
    const scale = Math.min((w - 2 * pad) / spanX, (h - 2 * pad) / spanY);
    const offsetX = (w - spanX * scale) / 2;
    const offsetY = (h - spanY * scale) / 2;
    // Dot size shrinks as the display gets denser, within readable limits.
    const radius = Math.max(1.2, Math.min(4, Math.sqrt((w * h) / Math.max(pixelCount, 1)) / 4));
    ctx.globalCompositeOperation = "lighter";
    for (const p of props) {
      for (let i = 0, n = 0; i + 1 < p.points.length; i += 2, n++) {
        const x = offsetX + (p.points[i] - bounds.minX) * scale;
        const y = offsetY + (bounds.maxY - p.points[i + 1]) * scale;
        const at = p.frameOffset + n * p.channelsPerPixel;
        if (frame && at + 2 < frame.length) {
          const [r, g, b] = [frame[at], frame[at + 1], frame[at + 2]];
          if (r + g + b === 0) continue;
          ctx.fillStyle = `rgb(${r}, ${g}, ${b})`;
        } else {
          ctx.fillStyle = UNLIT;
        }
        ctx.beginPath();
        ctx.arc(x, y, radius, 0, Math.PI * 2);
        ctx.fill();
      }
    }
    ctx.globalCompositeOperation = "source-over";
  }, [props, frame, bounds, pixelCount]);

  return <canvas ref={canvasRef} aria-label="Preview" className="h-full w-full rounded-lg" />;
}
