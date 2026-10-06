// Moving a prop's submodels and faces when its pixels move: lines joined into one (a line's
// pixels shift along, or run backwards when it's turned round) or cut in two.

import type { NodeRange, NodeRun, Region, SubmodelLine } from "../api/types";

/**
 * Which pixels move where: those from `from` up to (not including) `to` keep their order
 * (or run backwards with `reverse`) and start `offset` further on (`offset` may be negative);
 * the rest are left out.
 */
export interface PixelMove {
  from: number;
  to: number;
  offset: number;
  reverse: boolean;
}

/** Where pixel `i` (inside the move) goes. */
const place = (m: PixelMove, i: number) => (m.reverse ? m.offset + (m.to - 1 - i) + m.from : m.offset + i);

/** True when the move leaves every pixel where it was. */
const still = (m: PixelMove) => m.offset === 0 && !m.reverse;

function moveRun(run: NodeRun, m: PixelMove): NodeRun | null {
  const [lo, hi] = [Math.min(run.first, run.last), Math.max(run.first, run.last)];
  const [a, b] = [Math.max(lo, m.from), Math.min(hi, m.to - 1)];
  if (a > b) return null;
  const [first, last] = run.first <= run.last ? [a, b] : [b, a];
  return { first: place(m, first), last: place(m, last) };
}

function moveRanges(ranges: NodeRange[], m: PixelMove): NodeRange[] {
  const out: NodeRange[] = [];
  for (const r of ranges) {
    const [a, b] = [Math.max(r.start, m.from), Math.min(r.end, m.to)];
    if (a >= b) continue;
    const [p, q] = [place(m, a), place(m, b - 1)];
    out.push({ start: Math.min(p, q), end: Math.max(p, q) + 1 });
  }
  return out;
}

/**
 * The submodel or face with its pixels moved, or null when none of its pixels are in the move.
 * A rectangle of the prop (a sub-buffer) only stays when the pixels stay put.
 */
export function moveRegion(region: Region, m: PixelMove): Region | null {
  switch (region.kind) {
    case "nodes": {
      let any = false;
      const lines: SubmodelLine[] = region.lines.map((line) =>
        line.flatMap((run) => {
          if (run === null) return [null];
          const moved = moveRun(run, m);
          if (moved) any = true;
          return moved ? [moved] : [];
        }),
      );
      return any ? { ...region, lines } : null;
    }
    case "subBuffer":
      return still(m) && m.from === 0 ? region : null;
    case "face": {
      const mouths = Object.fromEntries(Object.entries(region.mouths).map(([k, v]) => [k, moveRanges(v ?? [], m)]));
      const moved = {
        ...region,
        mouths,
        eyesOpen: moveRanges(region.eyesOpen, m),
        eyesClosed: moveRanges(region.eyesClosed, m),
        outline: moveRanges(region.outline, m),
      };
      const any = [moved.eyesOpen, moved.eyesClosed, moved.outline, ...Object.values(mouths)].some((r) => r.length > 0);
      return any ? moved : null;
    }
  }
}

/** `name`, or "name (2)", "name (3)", … when it's taken (names are compared as the panel does). */
export function freeRegionName(name: string, taken: Set<string>): string {
  const key = (s: string) => s.trim().toLowerCase();
  if (!taken.has(key(name))) return name;
  for (let n = 2; ; n++) if (!taken.has(key(`${name} (${n})`))) return `${name} (${n})`;
}
