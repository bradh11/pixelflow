// DMX floods in the 2D previews: each cell of a fixture lights a soft round wash of its color
// around it, under the dots, as a flood lights a wall. The video export draws the same wash from
// the same numbers (crates/pf-video's flood.rs): `FLOOD_STRENGTH` of the color at the middle,
// falling as (1 - r²)² to nothing at `FLOOD_RADIUS` layout units, never smaller than `SMALLEST`
// screen pixels across. Not playing, a flood's reach shows as a faint wash of the unlit color.

import type { PreviewProp } from "../api/types";
import { type Size, type View, toScreen } from "./layoutMath";

/** How far a flood's wash reaches, in layout units. */
export const FLOOD_RADIUS = 0.75;
/** How strong a wash is at its middle, as a share of its color. */
export const FLOOD_STRENGTH = 0.55;
/** The smallest a wash's radius is drawn, in screen pixels. */
const SMALLEST = 12;
/** How strong the wash showing a flood's reach is when nothing plays. */
const REACH = 0.25;
/** Where the gradient's stops are, from the middle out. */
const STOPS = [0, 0.15, 0.3, 0.45, 0.6, 0.75, 0.9, 1];

/** How much of a wash shows at `r` of its radius out. */
export function washAt(r: number): number {
  return r >= 1 ? 0 : (1 - r * r) ** 2;
}

/** A wash's radius in screen pixels, in `view`. */
export function washRadius(view: View): number {
  return Math.max(FLOOD_RADIUS * view.zoom, SMALLEST);
}

/**
 * Draws the floods among `props` on a canvas set up in screen pixels: each lit cell's wash of its
 * color in `frame`, added onto what's there; with no frame, each cell's reach in `unlit` (rgb).
 */
export function drawFloods(
  ctx: CanvasRenderingContext2D,
  props: PreviewProp[],
  frame: Uint8Array | null,
  view: View,
  size: Size,
  unlit: [number, number, number],
) {
  const floods = props.filter((p) => p.flood);
  if (floods.length === 0) return;
  const radius = washRadius(view);
  ctx.save();
  ctx.globalCompositeOperation = frame ? "lighter" : "source-over";
  for (const p of floods) {
    for (let i = 0, o = p.frameOffset; i + 1 < p.points.length; i += 2, o += p.channelsPerPixel) {
      const color: [number, number, number] = frame ? [frame[o] ?? 0, frame[o + 1] ?? 0, frame[o + 2] ?? 0] : unlit;
      if (color[0] + color[1] + color[2] === 0) continue;
      const strength = frame ? FLOOD_STRENGTH : REACH;
      const at = toScreen(view, size, { x: p.points[i], y: p.points[i + 1] });
      if (at.x < -radius || at.y < -radius || at.x > size.width + radius || at.y > size.height + radius) continue;
      const gradient = ctx.createRadialGradient(at.x, at.y, 0, at.x, at.y, radius);
      for (const t of STOPS) gradient.addColorStop(t, `rgba(${color[0]}, ${color[1]}, ${color[2]}, ${(strength * washAt(t)).toFixed(3)})`);
      ctx.fillStyle = gradient;
      ctx.beginPath();
      ctx.arc(at.x, at.y, radius, 0, Math.PI * 2);
      ctx.fill();
    }
  }
  ctx.restore();
}
