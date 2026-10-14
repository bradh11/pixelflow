// DMX floods in the previews: each cell of a fixture lights a soft round wash of its color around
// it, under the dots, as a flood lights a wall. The 2D previews stamp it into their picture of
// dots (`stampPixels` in pixelBatches.ts), the video export the same way from the same numbers
// (crates/pf-video's dots.rs), held to each other by the scenes in api/contracts/pixels/, and the
// 3D view draws it from these numbers too (components/layout3d/threeScene.ts).
//
// A wash reaches `FLOOD_RADIUS` layout units (never less than `SMALLEST` screen pixels), and at
// a place r of the way out (as a share of that radius) it is `FLOOD_STRENGTH` × (1 - r²)² solid,
// in the flood's color, over what's under it. Not playing, a flood's reach shows as a faint wash
// (`UNLIT_WASH` solid) of the unlit color.

/** How far a flood's wash reaches, in layout units. */
export const FLOOD_RADIUS = 0.75;
/** How solid a wash is at its middle. */
export const FLOOD_STRENGTH = 0.55;
/** The smallest a wash's radius is drawn, in screen pixels. */
export const SMALLEST_WASH = 12;
/** How solid the wash showing a flood's reach is when nothing plays. */
export const UNLIT_WASH = 0.25;

/** How much of a wash shows `r2` of the way out, squared (the distance squared over the radius squared). */
export function washAt(r2: number): number {
  if (r2 >= 1) return 0;
  const t = 1 - r2;
  return t * t;
}

/** A wash's radius in screen pixels, at a zoom of `zoom` screen pixels to a layout unit. */
export function washRadius(zoom: number): number {
  return Math.max(FLOOD_RADIUS * zoom, SMALLEST_WASH);
}
