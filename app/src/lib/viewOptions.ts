// How this viewer likes to look at the lights, beside Glow (state/view3d.ts): the 3D view's grid
// on the ground, prop names over the display, whether pixels that are off show, how big dots are
// drawn, and a boost that lifts dim levels. None of it is part of the show: every preview, flat
// and 3D, follows it, and nothing changes what the show's colors are or what goes to the lights.
//
// The video export draws the same dots (crates/pf-video): its own Dot size, Off pixels and Boost
// dim start from these, and boost a frame with the same numbers (`boostTable`), so a video looks
// as the preview does.

import type { PreviewProp, PreviewProp3d } from "../api/types";

/** Pixels that are off: drawn dim (as they always were), hidden (only lit pixels show, as at night), or hidden only while the show plays. */
export type Unlit = "dim" | "hidden" | "auto";
export type DotSize = "small" | "medium" | "large";

export interface ViewOptions {
  /** The 3D view's measured grid on the ground. */
  grid: boolean;
  /** Prop names over the display. */
  names: boolean;
  unlit: Unlit;
  dotSize: DotSize;
  /** How much dim levels are lifted: 0 (off) to `BOOST_STEPS`. */
  boost: number;
}

/** As the previews have always looked, with nothing added: the grid shows as the ground does. */
export const VIEW_DEFAULTS: ViewOptions = { grid: true, names: false, unlit: "dim", dotSize: "medium", boost: 0 };

export const UNLIT_CHOICES: { value: Unlit; label: string }[] = [
  { value: "dim", label: "Dim" },
  { value: "hidden", label: "Hidden" },
  { value: "auto", label: "Auto" },
];

/**
 * How big dots are drawn against the usual: the view's dot and the dot a pixel has in the layout
 * both scale by this (dots still stop short of their neighbors, so only props with room between
 * their pixels get bigger dots). The video export's Dot size has the same three.
 */
export const DOT_SCALES: Record<DotSize, number> = { small: 0.6, medium: 1, large: 1.5 };

export const DOT_CHOICES: { value: DotSize; label: string }[] = [
  { value: "small", label: "Small" },
  { value: "medium", label: "Medium" },
  { value: "large", label: "Large" },
];

/** The steps of Boost dim, after off. */
export const BOOST_STEPS = 3;
export const BOOST_CHOICES: { value: number; label: string }[] = [
  { value: 0, label: "Off" },
  { value: 1, label: "Low" },
  { value: 2, label: "Medium" },
  { value: 3, label: "High" },
];

const isUnlit = (v: unknown): v is Unlit => v === "dim" || v === "hidden" || v === "auto";
const isDotSize = (v: unknown): v is DotSize => v === "small" || v === "medium" || v === "large";
const boostStep = (v: unknown) => (typeof v === "number" && Number.isInteger(v) && v >= 0 && v <= BOOST_STEPS ? v : 0);

/** The options in `value` (as remembered, or as the other window's note has them), the usual for anything missing or not understood. */
export function readViewOptions(value: unknown): ViewOptions {
  const o = typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
  return {
    grid: o.grid !== false,
    names: o.names === true,
    unlit: isUnlit(o.unlit) ? o.unlit : VIEW_DEFAULTS.unlit,
    dotSize: isDotSize(o.dotSize) ? o.dotSize : VIEW_DEFAULTS.dotSize,
    boost: boostStep(o.boost),
  };
}

/**
 * Whether pixels that are off are left out of a view that's `playing` (the show's time moving):
 * always when hidden, never when dim, and with Auto while it plays (as the lights look at night),
 * not while it's paused or stopped or the layout is being edited (so every prop can be seen).
 */
export const hidesUnlit = (unlit: Unlit, playing: boolean) => unlit === "hidden" || (unlit === "auto" && playing);

const tables: Uint8Array[] = [];

/**
 * What Boost dim makes of a light's brightest channel at each level (0–255) at a step. Low lifts
 * a level `x` (0–1) half way to √x, Medium to √x, High half way on from there to ∜x: full stays
 * full and off stays off, and a dim fade shows on a screen as it does to an eye in the dark. Only
 * square roots, sums and halves, which every computer works out to the same last bit, so the
 * video export's table is this one.
 */
export function boostTable(step: number): Uint8Array {
  let table = tables[step];
  if (!table) {
    table = new Uint8Array(256);
    for (let m = 0; m < 256; m++) {
      const x = m / 255;
      const root = Math.sqrt(x);
      const lifted = step <= 0 ? x : step === 1 ? (x + root) / 2 : step === 2 ? root : (root + Math.sqrt(root)) / 2;
      table[m] = Math.floor(lifted * 255 + 0.5);
    }
    tables[step] = table;
  }
  return table;
}

/**
 * A copy of `frame` with every prop's pixels lifted by Boost dim at `step`, each keeping its hue:
 * its brightest channel goes where `boostTable` says, the others in proportion. Only what's drawn
 * changes (the frame given is left as it is); at step 0 it's the frame itself.
 */
export function boostFrame(frame: Uint8Array, props: readonly (PreviewProp | PreviewProp3d)[], step: number): Uint8Array {
  if (step <= 0) return frame;
  const table = boostTable(Math.min(step, BOOST_STEPS));
  const out = frame.slice();
  for (const p of props) {
    const cpp = p.channelsPerPixel;
    // A prop whose pixels aren't three colors each has nothing here to lift.
    if (cpp < 3) continue;
    const count = "xyz" in p ? Math.floor(p.xyz.length / 3) : p.points.length >> 1;
    for (let i = 0, o = p.frameOffset; i < count && o + 2 < frame.length; i++, o += cpp) {
      // Read from the frame as it was, so a pixel in two props is lifted once.
      const [r, g, b] = [frame[o], frame[o + 1], frame[o + 2]];
      const most = Math.max(r, g, b);
      if (most === 0) continue;
      const to = table[most];
      const half = most >> 1;
      out[o] = Math.floor((r * to + half) / most);
      out[o + 1] = Math.floor((g * to + half) / most);
      out[o + 2] = Math.floor((b * to + half) / most);
    }
  }
  return out;
}
