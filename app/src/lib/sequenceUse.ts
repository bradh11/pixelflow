// What in the open sequence would be stranded by deleting a group or props: their rows keep their
// effects but light nothing once what they point at is gone.

import type { Sequence } from "../api/sequence";
import { plural } from "./format";

export type UseTarget = { group: string } | { props: string[] };

/** Rows in `doc` that light the group, or any of the props (or their submodels), and their effects. */
export function rowsUsing(doc: Sequence | null, what: UseTarget): { rows: number; effects: number } {
  const props = "props" in what ? new Set(what.props) : null;
  let rows = 0;
  let effects = 0;
  for (const row of doc?.rows ?? []) {
    const t = row.target;
    const hit = props
      ? ("prop" in t && props.has(t.prop)) || ("region" in t && props.has(t.region.prop))
      : "group" in t && "group" in what && t.group === what.group;
    if (!hit) continue;
    rows++;
    for (const layer of row.layers) effects += layer.effects.length;
  }
  return { rows, effects };
}

/** "Group “X” lights 2 rows with 143 effects in Medley. Delete it anyway? …", or null when nothing uses it. */
export function deleteUseWarning(doc: Sequence | null, subject: string, what: UseTarget, many = false): string | null {
  const { rows, effects } = rowsUsing(doc, what);
  if (!doc || rows === 0) return null;
  return `${subject} ${many ? "light" : "lights"} ${plural(rows, "row")} with ${plural(effects, "effect")} in ${doc.name}. Delete ${many ? "them" : "it"} anyway? Those rows will show nothing until you point them at something else.`;
}
