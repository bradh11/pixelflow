// Prop names over the display, when the viewer's Names option is on (lib/viewOptions.ts): each in
// a small dark tag under its prop, or over it, or across its middle, wherever there's room. Where
// props crowd together their names would pile up, so a name that would overlap one already placed
// is left out: the biggest props on the screen are named first, and zooming in names the rest.
// The flat previews and the 3D view place them alike, from each prop's box on the screen.

import type { PreviewProp } from "../api/types";
import { type Box3, type Mat4, project, v3 } from "./layout3d";
import { type Size, type View, boxOfPoints, toScreen } from "./layoutMath";

/** A prop to name, as it is on the screen (CSS pixels): its middle across, the top and bottom of its box, and how much of the screen it takes (the more, the sooner it's named). */
export interface LabelSpot {
  text: string;
  x: number;
  top: number;
  bottom: number;
  weight: number;
}

/** A name placed on the screen: its text (shortened to fit) and its tag's box (CSS pixels). */
export interface PlacedLabel {
  text: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export const LABEL_FONT = "500 11px system-ui, -apple-system, 'Segoe UI', sans-serif";
/** A tag's height, the room either side of its text, how far it stands off its prop, and the widest it gets (CSS pixels). */
const HEIGHT = 16;
const PAD = 4;
const GAP = 3;
const WIDEST = 160;
/** Tags keep this far from each other and from the screen's edges. */
const APART = 2;

/** `text` shortened with an ellipsis until it's no wider than `most`. */
export function fitText(text: string, most: number, widthOf: (text: string) => number): string {
  if (widthOf(text) <= most) return text;
  let keep = text.length;
  while (keep > 1 && widthOf(`${text.slice(0, keep).trimEnd()}…`) > most) keep--;
  return `${text.slice(0, keep).trimEnd()}…`;
}

/**
 * Where each prop's name goes on a screen of `size`: under its prop, or else over it, or else
 * across its middle, inside the screen and clear of every name placed before it; a name with no
 * such place is left out. The props taking the most of the screen are named first.
 */
export function placeLabels(spots: readonly LabelSpot[], size: Size, widthOf: (text: string) => number): PlacedLabel[] {
  const order = spots.map((s, i) => ({ s, i })).sort((a, b) => b.s.weight - a.s.weight || a.i - b.i);
  const placed: PlacedLabel[] = [];
  const clear = (x: number, y: number, w: number) =>
    y >= APART &&
    y + HEIGHT <= size.height - APART &&
    placed.every((p) => x + w + APART <= p.x || p.x + p.width + APART <= x || y + HEIGHT + APART <= p.y || p.y + p.height + APART <= y);
  for (const { s } of order) {
    if (!s.text || s.x < 0 || s.x > size.width || s.bottom < 0 || s.top > size.height) continue;
    const text = fitText(s.text, WIDEST - 2 * PAD, widthOf);
    const width = Math.ceil(widthOf(text)) + 2 * PAD;
    if (width + 2 * APART > size.width) continue;
    const x = Math.min(Math.max(s.x - width / 2, APART), size.width - width - APART);
    const y = [s.bottom + GAP, s.top - GAP - HEIGHT, (s.top + s.bottom - HEIGHT) / 2].find((at) => clear(x, at, width));
    if (y !== undefined) placed.push({ text, x, y, width, height: HEIGHT });
  }
  return placed;
}

/** The props' names as the flat previews show them: each from its pixels' box in `view`. */
export function flatSpots(props: readonly PreviewProp[], names: ReadonlyMap<string, string>, view: View, size: Size): LabelSpot[] {
  const spots: LabelSpot[] = [];
  for (const p of props) {
    const text = names.get(p.prop);
    const box = text ? boxOfPoints(p.points) : null;
    if (!text || !box) continue;
    const a = toScreen(view, size, { x: box.minX, y: box.maxY });
    const b = toScreen(view, size, { x: box.maxX, y: box.minY });
    spots.push({ text, x: (a.x + b.x) / 2, top: a.y, bottom: b.y, weight: (b.x - a.x + 1) * (b.y - a.y + 1) });
  }
  return spots;
}

/**
 * The props' names as the 3D view shows them, through the camera `m` on a view of `size`: each
 * from where the corners of its box land on the screen. A prop reaching behind the camera isn't
 * named.
 */
export function deepSpots(boxes: readonly { prop: string; box: Box3 }[], names: ReadonlyMap<string, string>, m: Mat4, size: Size): LabelSpot[] {
  const spots: LabelSpot[] = [];
  for (const { prop, box } of boxes) {
    const text = names.get(prop);
    if (!text) continue;
    let [left, right, top, bottom] = [Infinity, -Infinity, Infinity, -Infinity];
    let seen = true;
    for (const x of [box.min.x, box.max.x])
      for (const y of [box.min.y, box.max.y])
        for (const z of [box.min.z, box.max.z]) {
          const q = seen ? project(m, size, v3(x, y, z)) : null;
          if (!q) {
            seen = false;
            continue;
          }
          [left, right, top, bottom] = [Math.min(left, q.x), Math.max(right, q.x), Math.min(top, q.y), Math.max(bottom, q.y)];
        }
    if (seen) spots.push({ text, x: (left + right) / 2, top, bottom, weight: (right - left + 1) * (bottom - top + 1) });
  }
  return spots;
}

const widths = new Map<string, number>();

/** Draws the props' names on `ctx` (set to CSS pixels), placed so none overlap (see `placeLabels`). */
export function drawLabels(ctx: CanvasRenderingContext2D, spots: readonly LabelSpot[], size: Size) {
  if (spots.length === 0) return;
  ctx.save();
  ctx.font = LABEL_FONT;
  const widthOf = (text: string) => {
    let w = widths.get(text);
    if (w === undefined) {
      w = ctx.measureText(text).width;
      if (widths.size > 5000) widths.clear();
      widths.set(text, w);
    }
    return w;
  };
  ctx.textBaseline = "middle";
  ctx.textAlign = "center";
  for (const label of placeLabels(spots, size, widthOf)) {
    ctx.fillStyle = "rgba(10, 12, 20, 0.72)";
    ctx.beginPath();
    ctx.roundRect(label.x, label.y, label.width, label.height, 4);
    ctx.fill();
    ctx.fillStyle = "rgba(255, 255, 255, 0.92)";
    ctx.fillText(label.text, label.x + label.width / 2, label.y + label.height / 2 + 0.5);
  }
  ctx.restore();
}
