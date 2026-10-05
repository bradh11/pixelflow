import { useEffect, useRef } from "react";
import type { Waveform } from "../api/types";

/** A song's loudness over time with a playhead; clicking jumps there. */
export function WaveformView({
  waveform,
  positionMs,
  durationMs,
  onSeek,
}: {
  waveform: Waveform | null;
  positionMs: number | null;
  durationMs: number;
  onSeek: (ms: number) => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const ratio = window.devicePixelRatio || 1;
    const { clientWidth: w, clientHeight: h } = canvas;
    canvas.width = Math.round(w * ratio);
    canvas.height = Math.round(h * ratio);
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const total = Math.max(durationMs, waveform?.durationMs ?? 0, 1);
    const played = positionMs === null ? 0 : (positionMs / total) * w;
    if (waveform && waveform.peaks.length > 0) {
      // The song may be a little longer or shorter than the lights; draw it to its own length.
      const songWidth = (waveform.durationMs / total) * w;
      const bar = songWidth / waveform.peaks.length;
      waveform.peaks.forEach((peak, i) => {
        const x = i * bar;
        const height = Math.max(1, peak * (h - 4));
        ctx.fillStyle = x < played ? "rgb(139, 92, 246)" : "rgba(140, 140, 150, 0.55)";
        ctx.fillRect(x, (h - height) / 2, Math.max(1, bar - 0.5), height);
      });
    } else {
      ctx.fillStyle = "rgba(140, 140, 150, 0.3)";
      ctx.fillRect(0, h / 2 - 1, w, 2);
    }
    if (positionMs !== null) {
      ctx.fillStyle = "rgb(250, 250, 250)";
      ctx.fillRect(Math.min(w - 2, played), 0, 2, h);
    }
  }, [waveform, positionMs, durationMs]);

  return (
    <canvas
      ref={canvasRef}
      role="img"
      aria-label={waveform ? "Music waveform (click to jump)" : "No music"}
      className="h-16 w-full cursor-pointer rounded bg-neutral-100 dark:bg-neutral-900"
      onClick={(e) => {
        const rect = e.currentTarget.getBoundingClientRect();
        const total = Math.max(durationMs, waveform?.durationMs ?? 0, 1);
        onSeek(Math.round(((e.clientX - rect.left) / Math.max(rect.width, 1)) * total));
      }}
    />
  );
}
