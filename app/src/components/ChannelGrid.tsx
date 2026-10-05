import { useEffect, useRef } from "react";

/** Pixels per row in the grid. */
const COLUMNS = 64;

/** Draws a block of sequence channels as a grid of RGB pixels: what a controller receives when
 * PixelFlow doesn't know its strings yet. `start` counts from 1. */
export function ChannelGrid({
  frame,
  start,
  count,
  label,
}: {
  frame: Uint8Array | null;
  start: number;
  count: number;
  label: string;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const pixels = Math.ceil(count / 3);
  const rows = Math.max(1, Math.ceil(pixels / COLUMNS));

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    canvas.width = COLUMNS;
    canvas.height = rows;
    const image = ctx.createImageData(COLUMNS, rows);
    for (let p = 0; p < pixels; p++) {
      const at = start - 1 + p * 3;
      const o = p * 4;
      image.data[o] = frame?.[at] ?? 0;
      image.data[o + 1] = frame?.[at + 1] ?? 0;
      image.data[o + 2] = frame?.[at + 2] ?? 0;
      image.data[o + 3] = 255;
    }
    ctx.putImageData(image, 0, 0);
  }, [frame, start, pixels, rows]);

  return (
    <figure className="flex flex-col gap-2">
      <figcaption className="text-sm text-neutral-500">{label}</figcaption>
      <canvas
        ref={canvasRef}
        aria-label={label}
        className="w-full max-w-2xl rounded bg-black [image-rendering:pixelated]"
        style={{ aspectRatio: `${COLUMNS} / ${rows}` }}
      />
    </figure>
  );
}
