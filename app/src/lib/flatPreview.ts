// How the flat previews (the Sequence screen's, the Play screen's, and the preview's own window)
// show the display: what's behind it, the colors of pixels with no lit color of their own, how
// it's fitted in the canvas, and how big its dots are. The video export draws the same picture
// (crates/pf-video's raster.rs): the scenes in api/contracts/pixels/ are drawn from what's here,
// and the export's tests fail unless it draws them alike.

import { type Box, type Size, type View, fitView } from "./layoutMath";
import type { PixelColors } from "./pixelBatches";
import { DOT_RADIUS } from "./pixelGlow";

/** The night sky behind the display. */
export const BACKDROP = "#0a0a0c";
export const COLORS = { unlit: "rgba(200, 200, 200, 0.35)", selected: "#a78bfa", dark: "rgba(70, 70, 70, 0.55)" } satisfies PixelColors;

/**
 * The view that shows all of `box` (the display) in a canvas of `size`, with a little room around
 * it, and the largest radius a dot is drawn with there (screen pixels): the dot a pixel has in
 * the layout, but never too small to see or so big it's a blob.
 */
export function fitDisplay(box: Box | null, size: Size): { view: View; radius: number } {
  const view = fitView(box, size, 16);
  return { view, radius: Math.min(4, Math.max(1.2, view.zoom * DOT_RADIUS)) };
}
