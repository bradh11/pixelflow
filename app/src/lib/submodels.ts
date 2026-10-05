// Submodels and singing faces: reading and writing xLights-style pixel lists ("1-10,15"),
// working out which pixels a submodel or a face part lights (a mirror of pf-model's
// `Region::node_list` and pf-render's sub-buffer crop), the Faces effect's mouth shapes (a
// mirror of pf-render's `faces.rs`), and names for rows on submodels.

import type { SequenceTarget } from "../api/sequence";
import type { FaceDefinition, NodeRange, NodeRun, Phoneme, Prop, Region, Show, SubmodelLine } from "../api/types";

/** The mouth shapes in menu order, with the names xLights and Papagayo use. */
export const PHONEMES: { value: Phoneme; label: string }[] = [
  { value: "AI", label: "AI" },
  { value: "E", label: "E" },
  { value: "ETC", label: "etc" },
  { value: "FV", label: "FV" },
  { value: "L", label: "L" },
  { value: "MBP", label: "MBP" },
  { value: "O", label: "O" },
  { value: "REST", label: "rest" },
  { value: "U", label: "U" },
  { value: "WQ", label: "WQ" },
];

/**
 * Reads one line of a pixel list: pixel numbers from 1 and ranges either way (`1-10,15,20-12`),
 * with a blank or `0` entry for an empty spot. A blank line has no spots.
 */
export function parseLine(text: string): { line: SubmodelLine } | { error: string } {
  if (text.trim() === "") return { line: [] };
  const line: SubmodelLine = [];
  for (const raw of text.split(",")) {
    const part = raw.trim();
    if (part === "" || part === "0") {
      line.push(null);
      continue;
    }
    const numbers = part.split("-");
    const parsed = numbers.map((n) => (/^\s*\d+\s*$/.test(n) ? Number(n) : NaN));
    if (numbers.length > 2 || parsed.some((n) => !Number.isSafeInteger(n) || n < 1)) {
      return { error: `"${part}" isn't a pixel number or range; use numbers from 1, like 1-10 or 15.` };
    }
    const [first, last = first] = parsed;
    line.push({ first: first - 1, last: last - 1 });
  }
  return { line };
}

/** Writes a line the way {@link parseLine} reads it (from 1; `0` for an empty spot). */
export function formatLine(line: SubmodelLine): string {
  return line.map((run) => (run === null ? "0" : run.first === run.last ? String(run.first + 1) : `${run.first + 1}-${run.last + 1}`)).join(",");
}

/** Node ranges (0-based, half-open) as pixel numbers from 1: `1-4, 9`. */
export function formatRanges(ranges: NodeRange[]): string {
  return ranges
    .filter((r) => r.end > r.start)
    .map((r) => (r.end - r.start === 1 ? String(r.start + 1) : `${r.start + 1}-${r.end}`))
    .join(", ");
}

/** A run's nodes in order, leaving out any at or past `count`. */
function runNodes(run: NodeRun, count: number, out: (n: number) => void) {
  const lo = Math.min(run.first, run.last);
  const hi = Math.min(Math.max(run.first, run.last), count - 1);
  if (run.first <= run.last) for (let n = lo; n <= hi; n++) out(n);
  else for (let n = hi; n >= lo; n--) out(n);
}

/** Every node of a face, each once. */
function faceNodes(face: FaceDefinition, count: number): number[] {
  const parts = [...Object.values(face.mouths).flat(), ...face.eyesOpen, ...face.eyesClosed, ...face.outline];
  return uniqueNodes(parts.flatMap((r) => rangeNodes(r, count)));
}

function rangeNodes(range: NodeRange | undefined, count: number): number[] {
  if (!range) return [];
  const out: number[] = [];
  for (let n = range.start; n < Math.min(range.end, count); n++) out.push(n);
  return out;
}

function uniqueNodes(nodes: number[]): number[] {
  return [...new Set(nodes)];
}

/**
 * The prop's pixels a region lights, each once, in the region's order. A rectangle (sub-buffer)
 * takes the pixels inside it, judged by their positions (`points`: x, y pairs in wiring order)
 * across the prop's bounding box.
 */
export function regionNodes(region: Region, count: number, points?: ArrayLike<number>): number[] {
  if (region.kind === "face") return faceNodes(region, count);
  if (region.kind === "subBuffer") {
    if (!points) return [];
    const box = bounds(points, count);
    const out: number[] = [];
    for (let n = 0; n < count && 2 * n + 1 < points.length; n++) {
      const u = box.w > 1e-9 ? (points[2 * n] - box.minX) / box.w : 0.5;
      const v = box.h > 1e-9 ? (points[2 * n + 1] - box.minY) / box.h : 0.5;
      const [lo, hi] = [Math.min(region.x1, region.x2) - 1e-3, Math.max(region.x1, region.x2) + 1e-3];
      const [bottom, top] = [Math.min(region.y1, region.y2) - 1e-3, Math.max(region.y1, region.y2) + 1e-3];
      if (u * 100 >= lo && u * 100 <= hi && v * 100 >= bottom && v * 100 <= top) out.push(n);
    }
    return out;
  }
  const out: number[] = [];
  const seen = new Set<number>();
  for (const line of region.lines) {
    for (const run of line) {
      if (run)
        runNodes(run, count, (n) => {
          if (!seen.has(n)) {
            seen.add(n);
            out.push(n);
          }
        });
    }
  }
  return out;
}

function bounds(points: ArrayLike<number>, count: number) {
  let [minX, minY, maxX, maxY] = [Infinity, Infinity, -Infinity, -Infinity];
  for (let n = 0; n < count && 2 * n + 1 < points.length; n++) {
    minX = Math.min(minX, points[2 * n]);
    maxX = Math.max(maxX, points[2 * n]);
    minY = Math.min(minY, points[2 * n + 1]);
    maxY = Math.max(maxY, points[2 * n + 1]);
  }
  if (minX > maxX) [minX, minY, maxX, maxY] = [0, 0, 0, 0];
  return { minX, minY, w: maxX - minX, h: maxY - minY };
}

/** A region's kind in a few words. */
export function regionKindLabel(region: Region): string {
  if (region.kind === "face") return "Face";
  if (region.kind === "subBuffer") return "Rectangle";
  return region.lines.length === 1 ? "1 line" : `${region.lines.length} lines`;
}

/**
 * Why `name` can't be a new name for a region on `prop` (ignoring the region `except`), or null.
 * Names follow the engine's check: not blank, and different from the prop's other submodels and
 * faces, ignoring case.
 */
export function regionNameProblem(prop: Prop, name: string, except?: string): string | null {
  const wanted = name.trim().toLowerCase();
  if (!wanted) return "Give it a name.";
  if (prop.regions.some((r) => r.id !== except && r.name.trim().toLowerCase() === wanted)) {
    return `"${name.trim()}" is taken on ${prop.name}; names must be different on each prop.`;
  }
  return null;
}

/** A new, empty submodel with the first free "Submodel N" name. */
export function newSubmodel(prop: Prop): Region {
  const taken = new Set(prop.regions.map((r) => r.name.toLowerCase()));
  let n = 1;
  while (taken.has(`submodel ${n}`)) n++;
  return { id: crypto.randomUUID(), name: `Submodel ${n}`, kind: "nodes", lines: [[]], layout: "horizontal", buffer: "default" };
}

/** The prop's submodels (not its faces), for row and group pickers. */
export function submodelsOf(prop: Prop): Region[] {
  return prop.regions.filter((r) => r.kind !== "face");
}

/** The prop's faces. */
export function facesOf(prop: Prop): Region[] {
  return prop.regions.filter((r) => r.kind === "face");
}

/** The prop a target lights, for a prop or submodel row. */
export function targetProp(target: SequenceTarget): string | null {
  if ("prop" in target) return target.prop;
  if ("region" in target) return target.region.prop;
  return null;
}

/** One key per target (rows on the same target share it). */
export function targetKey(target: SequenceTarget): string {
  if ("prop" in target) return target.prop;
  if ("group" in target) return target.group;
  return `${target.region.prop}/${target.region.region}`;
}

/** A target's name: "Arch", "Outline" (a group), or "Arch / Left" (a submodel). */
export function targetName(show: Show | undefined, target: SequenceTarget): string {
  if ("prop" in target) return show?.props.find((p) => p.id === target.prop)?.name ?? "Missing prop";
  if ("group" in target) return show?.groups.find((g) => g.id === target.group)?.name ?? "Missing group";
  const prop = show?.props.find((p) => p.id === target.region.prop);
  const region = prop?.regions.find((r) => r.id === target.region.region);
  return prop && region ? `${prop.name} / ${region.name}` : "Missing submodel";
}

/** Some of the pixels a target lights: one prop's (all of them, or a list). */
export interface TargetSegment {
  prop: string;
  nodes: number[] | "all";
}

/**
 * The pixels a target lights, in order along the target: a whole prop, a submodel's pixels, or a
 * group's members in their listed order (whole props and submodels mixed, as xLights lists them).
 * A pixel already in the group keeps its first place.
 */
export function targetNodes(show: Show, target: SequenceTarget, count: (prop: Prop) => number, points: (prop: Prop) => ArrayLike<number>): TargetSegment[] {
  const out: TargetSegment[] = [];
  const seen = new Map<string, Set<number> | "all">();
  const add = (propId: string, regionId: string | null) => {
    const prop = show.props.find((p) => p.id === propId);
    const before = seen.get(propId);
    if (!prop || before === "all") return;
    if (regionId === null) {
      const nodes: number[] | "all" = before ? Array.from({ length: count(prop) }, (_, k) => k).filter((k) => !before.has(k)) : "all";
      if (nodes === "all" || nodes.length > 0) out.push({ prop: propId, nodes });
      seen.set(propId, "all");
      return;
    }
    const region = prop.regions.find((r) => r.id === regionId);
    if (!region) return;
    const lit = before ?? new Set<number>();
    const nodes = regionNodes(region, count(prop), points(prop)).filter((n) => !lit.has(n));
    nodes.forEach((n) => lit.add(n));
    seen.set(propId, lit);
    if (nodes.length > 0) out.push({ prop: propId, nodes });
  };
  if ("prop" in target) add(target.prop, null);
  else if ("region" in target) add(target.region.prop, target.region.region);
  else {
    const group = show.groups.find((g) => g.id === target.group);
    for (const m of group?.members ?? []) {
      if (typeof m === "string") add(m, null);
      else add(m.prop, m.region);
    }
  }
  return out;
}

/** The pixels lit for each part of a face showing `phoneme` with eyes open or closed, outline first, eyes last (later parts win). */
export function faceParts(face: FaceDefinition, phoneme: Phoneme, eyesClosed: boolean, outline: boolean): { part: "outline" | "mouth" | "eyes"; ranges: NodeRange[] }[] {
  const parts: { part: "outline" | "mouth" | "eyes"; ranges: NodeRange[] }[] = [];
  if (outline) parts.push({ part: "outline", ranges: face.outline });
  parts.push({ part: "mouth", ranges: face.mouths[phoneme] ?? [] });
  parts.push({ part: "eyes", ranges: eyesClosed ? face.eyesClosed : face.eyesOpen });
  return parts;
}

/**
 * The color of a face part: the face's own (white where it has none) when `own` and the face has
 * colors, else the palette's (mouth, eyes, outline; a short palette repeats its last color).
 */
export function facePartColor(face: FaceDefinition, part: "outline" | "mouth" | "eyes", phoneme: Phoneme, eyesClosed: boolean, own: boolean, palette: string[]): string {
  if (own && face.colors) {
    const c = face.colors;
    return (part === "outline" ? c.outline : part === "mouth" ? c.mouths?.[phoneme] : eyesClosed ? c.eyesClosed : c.eyesOpen) ?? "#ffffff";
  }
  const index = part === "mouth" ? 0 : part === "eyes" ? 1 : 2;
  return palette.length === 0 ? "#ffffff" : palette[Math.min(index, palette.length - 1)];
}

/** The mouth shapes a word makes, guessed from its letters (an approximation; see pf-render's `faces.rs`). */
export function wordPhonemes(word: string): Phoneme[] {
  const letters = [...word.toLowerCase()].filter((c) => /\p{L}/u.test(c));
  const out: Phoneme[] = [];
  for (let i = 0; i < letters.length; i++) {
    const c = letters[i];
    let p: Phoneme;
    if (c === "o" && letters[i + 1] === "o") {
      p = "U";
      i++;
    } else if (c === "a" || c === "i") p = "AI";
    else if (c === "e") p = "E";
    else if (c === "o") p = "O";
    else if (c === "u") p = "U";
    else if (c === "y" && i === letters.length - 1 && i > 0) p = "E";
    else if ("mbp".includes(c)) p = "MBP";
    else if ("fv".includes(c)) p = "FV";
    else if (c === "l") p = "L";
    else if ("wq".includes(c)) p = "WQ";
    else p = "ETC";
    if (out[out.length - 1] !== p) out.push(p);
  }
  return out;
}

/**
 * The mouth shape at `ms` on a timing track: the mark's phoneme on a phonemes track, shapes
 * guessed from the word's letters (spread over the mark) on any other, and rest between marks.
 */
export function phonemeAt(track: { kind: string; marks: { startMs: number; endMs: number; label: string }[] } | undefined, ms: number): Phoneme {
  const mark = track?.marks.find((m) => m.startMs <= ms && ms < m.endMs);
  if (!track || !mark) return "REST";
  if (track.kind === "phonemes") return phonemeFromName(mark.label) ?? "REST";
  const shapes = wordPhonemes(mark.label);
  if (shapes.length === 0) return "REST";
  const k = Math.floor(((ms - mark.startMs) * shapes.length) / Math.max(1, mark.endMs - mark.startMs));
  return shapes[Math.min(k, shapes.length - 1)];
}

/** How a highlighted submodel's pixels look on the layout canvas. */
export const HIGHLIGHT = "#ffd23f";

/**
 * The pixels to draw over a prop for a highlighted region: their positions (x, y pairs) and
 * colors (r, g, b per pixel). A submodel's pixels are all bright; a face with `phoneme` shows
 * that mouth with open eyes and its outline in the face's colors, its other pixels dark.
 */
export function highlightPixels(region: Region, count: number, points: ArrayLike<number>, phoneme: Phoneme | null): { points: number[]; rgb: Uint8Array } {
  const nodes = regionNodes(region, count, points).filter((n) => 2 * n + 1 < points.length);
  const rgb = new Uint8Array(nodes.length * 3);
  const hex = (color: string) => [1, 3, 5].map((i) => parseInt(color.slice(i, i + 2), 16) || 0);
  const lit = new Map<number, number[]>();
  if (region.kind === "face" && phoneme) {
    for (const { part, ranges } of faceParts(region, phoneme, false, true)) {
      const color = hex(facePartColor(region, part, phoneme, false, true, []));
      for (const r of ranges) for (let n = r.start; n < r.end; n++) lit.set(n, color);
    }
  }
  const bright = hex(HIGHLIGHT);
  nodes.forEach((n, i) => rgb.set(region.kind === "face" && phoneme ? (lit.get(n) ?? [0, 0, 0]) : bright, i * 3));
  return { points: nodes.flatMap((n) => [points[2 * n], points[2 * n + 1]]), rgb };
}

/** A phoneme name as xLights writes it (`AI`, `etc`, `rest`), any case. */
export function phonemeFromName(name: string): Phoneme | null {
  const wanted = name.trim().toUpperCase();
  return PHONEMES.find((p) => p.value === wanted)?.value ?? null;
}
