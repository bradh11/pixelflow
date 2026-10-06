// The sample show offered on the start page ("Try the demo show"): the `?demo` house, kept as a
// file so the desktop app opens the very same show. It always opens as a new, unsaved copy.

import type { Show } from "./types";
import sample from "./sampleShow.json";

/** A fresh copy of the sample show. */
export function sampleShow(): Show {
  return structuredClone(sample) as unknown as Show;
}

/** `text` with every id replaced by its order of first appearance, to compare shows by content. */
export function withNumberedIds(text: string): string {
  const ids = new Map<string, string>();
  return text.replace(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/g, (id) => {
    if (!ids.has(id)) ids.set(id, `00000000-0000-4000-8000-${String(ids.size + 1).padStart(12, "0")}`);
    return ids.get(id)!;
  });
}
