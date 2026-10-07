// The edits the layout editor sends. Each user action becomes one batch (one undo step).

import type { Edit, PreviewProp, Prop, Show } from "../api/types";
import { frontView } from "./geometry";
import {
  type Align,
  type Box,
  type Gesture,
  type Size,
  type View,
  alignMoves,
  besideBox,
  boxCenter,
  boxOfPoints,
  toWorld,
  unionBox,
  copyName,
  distributeMoves,
  gestureTransform,
  isNoop,
  tidy,
} from "./layoutMath";

/** Each listed prop changed by the gesture, as one batch; nothing when it changes nothing. */
export function gestureEdits(show: Show, ids: string[], gesture: Gesture): Edit[] {
  if (isNoop(gesture)) return [];
  return show.props
    .filter((p) => ids.includes(p.id))
    .map((prop) => ({ type: "updateProp" as const, prop: { ...prop, transform: gestureTransform(gesture, prop.transform) } }));
}

/** The prop `id` changed by `change`, built from the show when the edit's turn comes (see `EditsFrom`). */
export function updateEdits(id: string, change: (prop: Prop) => Prop): (show: Show) => Edit[] {
  return (show) => {
    const prop = show.props.find((p) => p.id === id);
    return prop ? [{ type: "updateProp", prop: change(prop) }] : [];
  };
}

export function removeEdits(ids: string[]): Edit[] {
  return ids.map((id) => ({ type: "removeProp" as const, id }));
}

/** Copies of the props, a little to the right and below, unwired and named "… copy". */
export function duplicateEdits(show: Show, ids: string[], offset = 0.5): { edits: Edit[]; ids: string[] } {
  return pasteEdits(
    show,
    show.props.filter((p) => ids.includes(p.id)),
    offset,
  );
}

/**
 * New props copied from `props` (copied earlier, maybe from another show), `offset` to the right
 * and as far down, with new ids, named "… copy" where the name is taken. Props aren't wired, so
 * the copies aren't either.
 */
export function pasteEdits(show: Show, props: Prop[], offset: number): { edits: Edit[]; ids: string[] } {
  const taken = show.props.map((p) => p.name);
  const copies: Prop[] = props.map((prop) => {
    const name = taken.includes(prop.name) ? copyName(prop.name, taken) : prop.name;
    taken.push(name);
    const { position } = prop.transform;
    return {
      ...structuredClone(prop),
      id: crypto.randomUUID(),
      name,
      transform: { ...structuredClone(prop.transform), position: { ...position, x: tidy(position.x + offset), y: tidy(position.y - offset) } },
    };
  });
  return { edits: copies.map((prop) => ({ type: "addProp" as const, prop })), ids: copies.map((p) => p.id) };
}

/** Each selected prop's box from its pixels in the preview. */
export function selectedBoxes(preview: PreviewProp[], ids: string[]): { id: string; box: Box }[] {
  return preview
    .filter((p) => ids.includes(p.prop))
    .map((p) => ({ id: p.prop, box: boxOfPoints(p.points) }))
    .filter((b): b is { id: string; box: Box } => b.box !== null);
}

function movesToEdits(show: Show, moves: Map<string, { x: number; y: number }>): Edit[] {
  return show.props
    .filter((p) => {
      const d = moves.get(p.id);
      return d && (d.x !== 0 || d.y !== 0);
    })
    .map((prop) => {
      const d = moves.get(prop.id)!;
      return gestureEdits(show, [prop.id], { kind: "move", dx: d.x, dy: d.y })[0];
    })
    .filter(Boolean);
}

export function alignEdits(show: Show, preview: PreviewProp[], ids: string[], how: Align): Edit[] {
  return movesToEdits(show, alignMoves(selectedBoxes(preview, ids), how));
}

export function distributeEdits(show: Show, preview: PreviewProp[], ids: string[], axis: "horizontal" | "vertical"): Edit[] {
  return movesToEdits(show, distributeMoves(selectedBoxes(preview, ids), axis));
}

/**
 * A new prop moved just right of everything already in the show (instead of on top of it).
 * The other props' boxes come from the engine's `preview` where it has them, and are worked
 * out here otherwise.
 */
export function besideOthers(prop: Prop, show: Show, preview: PreviewProp[] = []): Prop {
  const drawn = new Map(preview.map((p) => [p.prop, p.points]));
  const existing = unionBox(show.props.map((p) => boxOfPoints(drawn.get(p.id) ?? frontView(p))));
  const at = besideBox(existing, boxOfPoints(frontView(prop)));
  return { ...prop, transform: { ...prop.transform, position: { ...prop.transform.position, ...at } } };
}

/** The part of the layout a view shows on a canvas of `size`; null before the canvas has a size. */
export function visibleBox(view: View, size: Size): Box | null {
  if (size.width <= 0 || size.height <= 0) return null;
  const a = toWorld(view, size, { x: 0, y: size.height });
  const b = toWorld(view, size, { x: size.width, y: 0 });
  return { minX: a.x, minY: a.y, maxX: b.x, maxY: b.y };
}

/**
 * A new prop centred in `visible` (what the canvas shows). When another prop is already centred
 * there (the last one added, say), it steps down and to the right until it's clear, so props added
 * one after another fan out instead of stacking. Without a visible area, it goes beside the others.
 */
export function placedInView(prop: Prop, show: Show, preview: PreviewProp[], visible: Box | null): Prop {
  if (!visible) return besideOthers(prop, show, preview);
  const own = boxOfPoints(frontView(prop));
  if (!own) return prop;
  const drawn = new Map(preview.map((p) => [p.prop, p.points]));
  const centres = show.props
    .map((p) => boxOfPoints(drawn.get(p.id) ?? frontView(p)))
    .filter((b): b is Box => b !== null)
    .map(boxCenter);
  const target = boxCenter(visible);
  const width = visible.maxX - visible.minX;
  const height = visible.maxY - visible.minY;
  const step = Math.max(Math.min(width, height) * 0.05, 1e-3);
  let { cx, cy } = target;
  // Fan out until clear, but never past the edge of what's shown (then start a new fan).
  for (let i = 0; i < 400 && centres.some((c) => Math.abs(c.cx - cx) < step / 2 && Math.abs(c.cy - cy) < step / 2); i++) {
    cx += step;
    cy -= step;
    if (cx > visible.maxX - width * 0.1 || cy < visible.minY + height * 0.1) {
      cx = target.cx + step * 0.5 * ((i % 7) + 1);
      cy = target.cy;
    }
  }
  const at = boxCenter(own);
  const { position } = prop.transform;
  return { ...prop, transform: { ...prop.transform, position: { ...position, x: tidy(position.x + cx - at.cx), y: tidy(position.y + cy - at.cy) } } };
}

/** "Port 2 on Falcon_F16V5_B9F5", one entry per place the prop is wired; empty when it isn't. */
/** The ports where other props come after `propId` (so their channels move when its pixel count
 * changes), as "Port 3", or "Port 3 on Garage" when the show has more than one controller. */
export function portsWithPropsAfter(show: Show, propId: string): string[] {
  const named = show.controllers.length > 1;
  const places: string[] = [];
  for (const controller of show.controllers) {
    for (const port of controller.ports) {
      const at = port.slots.findIndex((s) => s.prop === propId);
      if (at >= 0 && at < port.slots.length - 1) places.push(named ? `Port ${port.number} on ${controller.name}` : `Port ${port.number}`);
    }
  }
  return places;
}

export function wiringOf(show: Show, propId: string): string[] {
  const places: string[] = [];
  for (const controller of show.controllers) {
    for (const port of controller.ports) {
      if (port.slots.some((s) => s.prop === propId)) places.push(`Port ${port.number} on ${controller.name}`);
    }
  }
  return places;
}
