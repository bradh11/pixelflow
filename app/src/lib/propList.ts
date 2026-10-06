// The Layout screen's props list: which props it shows, in what order, and range selection.

import type { Prop, Show } from "../api/types";
import { shapeLabel } from "./shows";

export type PropSort = "layout" | "name" | "pixels" | "type" | "unwired";

export const PROP_SORTS: { sort: PropSort; label: string }[] = [
  { sort: "layout", label: "Layout order" },
  { sort: "name", label: "Name" },
  { sort: "pixels", label: "Most pixels" },
  { sort: "type", label: "Type" },
  { sort: "unwired", label: "Not wired first" },
];

export interface ListOptions {
  sort: PropSort;
  /** Part of a name or type to look for; empty lists every prop. */
  query: string;
  unwiredOnly: boolean;
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** Every prop wired to some port. */
export function wiredProps(show: Show): Set<string> {
  const wired = new Set<string>();
  for (const c of show.controllers) for (const port of c.ports) for (const slot of port.slots) wired.add(slot.prop);
  return wired;
}

/** The props to list: those matching the search (and wiring filter), in the chosen order. */
export function listedProps(props: Prop[], options: ListOptions, pixels: ReadonlyMap<string, number>, wired: ReadonlySet<string>): Prop[] {
  const query = options.query.trim().toLowerCase();
  const labels = new Map<Prop, string>();
  const label = (p: Prop) => {
    let l = labels.get(p);
    if (l === undefined) labels.set(p, (l = shapeLabel(p.shape)));
    return l;
  };
  const shown = props.filter(
    (p) => (!options.unwiredOnly || !wired.has(p.id)) && (!query || p.name.toLowerCase().includes(query) || label(p).toLowerCase().includes(query)),
  );
  const byName = (a: Prop, b: Prop) => collator.compare(a.name, b.name);
  switch (options.sort) {
    case "layout":
      return shown;
    case "name":
      return shown.sort(byName);
    case "pixels":
      return shown.sort((a, b) => (pixels.get(b.id) ?? 0) - (pixels.get(a.id) ?? 0) || byName(a, b));
    case "type":
      return shown.sort((a, b) => collator.compare(label(a), label(b)) || byName(a, b));
    case "unwired":
      return shown.sort((a, b) => Number(wired.has(a.id)) - Number(wired.has(b.id)));
  }
}

/** Shift-click: everything from the anchor to `to`, in list order (just `to` when the anchor isn't listed). */
export function rangeSelect(order: string[], anchor: string, to: string): string[] {
  const a = order.indexOf(anchor);
  const b = order.indexOf(to);
  if (a < 0 || b < 0) return [to];
  return order.slice(Math.min(a, b), Math.max(a, b) + 1);
}
