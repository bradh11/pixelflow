import { useEffect, useRef, useState } from "react";
import type { Waveform } from "../api/types";
import { useAudioProgress } from "../state/audioProgress";
import { ProgressBar } from "./ProgressBar";

/**
 * A song's loudness over time with a playhead at the music's position; clicking jumps there.
 * Positions given and reported are lights (sequence) time: music time is lights time minus the
 * offset (how far the lights run ahead of the music).
 */
export function WaveformView({
  waveform,
  audio = null,
  loading = false,
  positionMs,
  durationMs,
  offsetMs = 0,
  onSeek,
}: {
  waveform: Waveform | null;
  /** The music file, for how far reading it has got while `loading`. */
  audio?: string | null;
  loading?: boolean;
  positionMs: number | null;
  durationMs: number;
  offsetMs?: number;
  onSeek: (ms: number) => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(0);
  // Redraw when the canvas changes size (the window or the panel next to it).
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => setWidth(canvas.clientWidth));
    observer.observe(canvas);
    return () => observer.disconnect();
  }, []);

  const total = Math.max(durationMs, waveform?.durationMs ?? 0, 1);

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
    const music = positionMs === null ? null : Math.max(0, positionMs - offsetMs);
    const played = music === null ? 0 : (music / total) * w;
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
    if (music !== null) {
      // The canvas's text color follows the theme (dark playhead on light, light on dark).
      ctx.fillStyle = getComputedStyle(canvas).color || "rgb(120, 120, 130)";
      ctx.fillRect(Math.min(w - 2, played), 0, 2, h);
    }
  }, [waveform, positionMs, total, offsetMs, width]);

  return (
    <div className="relative">
      <canvas
        ref={canvasRef}
        role="img"
        aria-label={loading ? "Loading the music waveform" : waveform ? "Music waveform (click to jump)" : "No music"}
        aria-busy={loading || undefined}
        className="h-16 w-full cursor-pointer rounded bg-neutral-100 text-neutral-800 dark:bg-neutral-900 dark:text-neutral-100"
        onClick={(e) => {
          const rect = e.currentTarget.getBoundingClientRect();
          const music = ((e.clientX - rect.left) / Math.max(rect.width, 1)) * total;
          onSeek(Math.max(0, Math.round(music + offsetMs)));
        }}
      />
      {loading && <Reading audio={audio} />}
    </div>
  );
}

/** Over the waveform while the music is read for it, with how far it has got once the reading
 * says. */
function Reading({ audio }: { audio: string | null }) {
  const progress = useAudioProgress("waveform", audio ?? "");
  return (
    <div className="pointer-events-none absolute inset-0 flex items-center justify-center text-xs text-neutral-500">
      {progress ? <ProgressBar label={progress.stage} fraction={progress.fraction} className="w-56 max-w-[60%]" /> : "Reading the music…"}
    </div>
  );
}
