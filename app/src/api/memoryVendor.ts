import { nodeCount } from "../lib/shows";
import type { Effect, EffectKind, Row, Sequence, SequenceTarget } from "./sequence";
import {
  VENDOR_AUTO_MAP,
  type SequenceImportSummary,
  type Show,
  type VendorInspection,
  type VendorItem,
  type VendorItemKind,
  type VendorMapping,
  type VendorPropType,
  type VendorSuggestion,
  type VendorTarget,
} from "./types";

/** A vendor package "on disk" for the memory sequencer: its sequences and what's in each. */
export interface MemoryVendorPackage {
  sequences: { name: string; song: string; durationMs: number; items: VendorItem[] }[];
  hasLayout: boolean;
  music: string | null;
  key: string;
}

/** The demo's vendor package (`?demo`): a song made for someone else's props. */
export const DEMO_VENDOR_PATH = "/Users/demo/Downloads/Holiday Mashup.zip";

function item(name: string, kind: VendorItemKind, type: VendorPropType, effects: number, pixels: number, displayAs: string | null = null): VendorItem {
  const at = kind === "submodel" || kind === "strand" ? name.indexOf("/") : -1;
  return {
    name,
    label: at >= 0 ? name.slice(at + 1) : name,
    parent: at >= 0 ? name.slice(0, at) : null,
    kind,
    type,
    displayAs,
    effects,
    pixels,
  };
}

export function demoVendorPackage(): MemoryVendorPackage {
  const items = [
    item("Whole House GRP", "group", "other", 64, 9800, "ModelGroup"),
    item("MegaTree 16x50", "model", "tree", 120, 800, "Tree 360"),
    item("MegaTree 16x50/Star Topper", "submodel", "star", 14, 0),
    item("P10 Matrix", "model", "matrix", 92, 6144, "Horiz Matrix"),
    item("Arches GRP", "group", "arch", 26, 300, "ModelGroup"),
    item("Arch 1", "model", "arch", 40, 50, "Arches"),
    item("Arch 1/Strand 1", "strand", "arch", 6, 0),
    item("Arch 2", "model", "arch", 38, 50, "Arches"),
    item("Roofline Left", "model", "line", 30, 150, "Single Line"),
    item("Mini Star", "model", "star", 18, 40, "Star"),
    item("Spinner 1", "model", "spinner", 22, 120, "Spinner"),
    item("Flood Left", "model", "flood", 9, 1, "Single Line"),
  ];
  return {
    sequences: [
      { name: "Holiday Mashup/Sequences/Holiday Mashup.xsq", song: "Holiday Mashup", durationMs: 60_000, items },
      { name: "Holiday Mashup/Sequences/Holiday Mashup (short).xsq", song: "Holiday Mashup (short)", durationMs: 30_000, items: items.slice(0, 6) },
    ],
    hasLayout: true,
    music: "Holiday Mashup.mp3",
    key: "layout:demo-vendor",
  };
}

const SHAPE_TYPES: Record<string, VendorPropType> = {
  line: "line",
  polyLine: "line",
  arch: "arch",
  circle: "circle",
  matrix: "matrix",
  tree: "tree",
  star: "star",
  candyCanes: "canes",
  windowFrame: "window",
  icicles: "icicles",
  wreath: "wreath",
  spinner: "spinner",
  sphere: "sphere",
  cube: "cube",
};

/** The words of a name, lowercased, numbers apart, plurals singular, "group" left out. */
function words(name: string): string[] {
  return (name.toLowerCase().match(/[a-z]+|[0-9]+/g) ?? [])
    .map((w) => (w.length > 3 && w.endsWith("s") && !w.endsWith("ss") ? w.slice(0, -1) : w))
    .filter((w) => !["group", "grp"].includes(w));
}

function letters(name: string): string {
  return words(name)
    .filter((w) => !/^[0-9]+$/.test(w))
    .join("");
}

/** The show's props, groups, and submodels, as a mapping can name them. */
export function vendorTargets(show: Show): VendorTarget[] {
  const typeOf = new Map(
    show.props.map((p) => [p.id, (p.shape.source === "generator" ? SHAPE_TYPES[p.shape.type] : undefined) ?? "other"] as const),
  );
  const pixelsOf = new Map(show.props.map((p) => [p.id, nodeCount(p.shape)] as const));
  const out: VendorTarget[] = show.groups.map((g) => {
    const props = [...new Set(g.members.map((m) => (typeof m === "string" ? m : m.prop)))];
    const counts = new Map<VendorPropType, number>();
    for (const id of props) {
      const t = typeOf.get(id) ?? "other";
      if (t !== "other") counts.set(t, (counts.get(t) ?? 0) + 1);
    }
    const type = [...counts.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] ?? "other";
    return { name: g.name, label: g.name, parent: null, kind: "group", type, pixels: props.reduce((n, id) => n + (pixelsOf.get(id) ?? 0), 0) };
  });
  for (const p of show.props) {
    const type = typeOf.get(p.id) ?? "other";
    out.push({ name: p.name, label: p.name, parent: null, kind: "model", type, pixels: pixelsOf.get(p.id) ?? 0 });
    for (const r of p.regions.filter((r) => r.kind !== "face")) {
      out.push({ name: `${p.name}/${r.name}`, label: r.name, parent: p.name, kind: "submodel", type, pixels: 0 });
    }
  }
  return out;
}

/**
 * Suggestions as the engine makes them, simplified: a saved mapping, the same name, a similar
 * name, then the same kind of prop (each of the show's props going to one item only).
 */
export function suggestVendorMapping(items: VendorItem[], targets: VendorTarget[], saved: VendorMapping | null): { suggestions: VendorSuggestion[]; mapping: VendorMapping } {
  const byName = new Map(targets.map((t) => [t.name, t]));
  const best = new Map<string, VendorSuggestion>();
  const claimed = new Set<string>();
  const suggest = (i: VendorItem, target: string[], confidence: number, reason: VendorSuggestion["reason"]) => {
    best.set(i.name, { item: i.name, targets: target, confidence, reason });
    target.forEach((t) => claimed.add(t));
  };
  for (const i of items) {
    const kept = saved?.items[i.name]?.filter((t) => byName.has(t));
    if (kept && (kept.length > 0 || saved?.items[i.name]?.length === 0)) suggest(i, kept, 1, "saved");
    else if (byName.has(i.name)) suggest(i, [i.name], 1, "exact");
    else {
      const same = targets.find((t) => t.name.toLowerCase() === i.name.toLowerCase());
      if (same) suggest(i, [same.name], 0.95, "exact");
    }
  }
  const candidates: { i: VendorItem; t: VendorTarget; confidence: number; reason: VendorSuggestion["reason"] }[] = [];
  for (const i of items) {
    if (best.has(i.name) || (i.kind !== "model" && i.kind !== "group")) continue;
    for (const t of targets.filter((t) => t.kind !== "submodel")) {
      const sameSide = (i.kind === "group") === (t.kind === "group");
      const pixels = i.pixels && t.pixels ? Math.min(i.pixels, t.pixels) / Math.max(i.pixels, t.pixels) : 0;
      if (letters(i.name) && letters(i.name) === letters(t.name)) candidates.push({ i, t, confidence: sameSide ? 0.8 : 0.7, reason: "name" });
      else if (i.type !== "other" && i.type === t.type) {
        const only = targets.filter((x) => x.kind === t.kind && x.type === t.type).length === 1;
        candidates.push({ i, t, confidence: Math.min(0.79, 0.42 + 0.2 * pixels + (sameSide ? 0.05 : -0.12) + (only ? 0.1 : 0)), reason: "type" });
      }
    }
  }
  candidates.sort((a, b) => b.confidence - a.confidence || b.i.effects - a.i.effects);
  const hints = new Map<string, VendorSuggestion>();
  for (const c of candidates) {
    if (best.has(c.i.name)) continue;
    if (c.confidence >= VENDOR_AUTO_MAP && !claimed.has(c.t.name)) suggest(c.i, [c.t.name], c.confidence, c.reason);
    else if (!hints.has(c.i.name)) hints.set(c.i.name, { item: c.i.name, targets: [c.t.name], confidence: Math.min(c.confidence, VENDOR_AUTO_MAP - 0.01), reason: c.reason });
  }
  for (const i of items.filter((i) => i.kind === "submodel" && !best.has(i.name))) {
    const prop = best.get(i.parent ?? "")?.targets[0];
    const region = targets.find((t) => t.kind === "submodel" && t.parent === prop && letters(t.label) === letters(i.label));
    if (region) suggest(i, [region.name], 0.9, "name");
  }
  const mapping: VendorMapping = { items: {} };
  const suggestions = items.map((i) => {
    const s = best.get(i.name);
    if (s) {
      if (s.reason === "saved" || s.confidence >= VENDOR_AUTO_MAP) mapping.items[i.name] = [...s.targets];
      return s;
    }
    return hints.get(i.name) ?? { item: i.name, targets: [], confidence: 0, reason: "none" as const };
  });
  return { suggestions, mapping };
}

/** Inspects `pkg`'s sequence `sequence` (or its first) against `show`. */
export function inspectVendor(pkg: MemoryVendorPackage, show: Show, sequence: string | undefined, saved: VendorMapping | null, musicFolder: string | null): VendorInspection {
  const seq = pkg.sequences.find((s) => s.name === sequence) ?? pkg.sequences[0];
  const targets = vendorTargets(show);
  const { suggestions, mapping } = suggestVendorMapping(seq.items, targets, saved);
  return {
    sequences: pkg.sequences.map((s) => s.name),
    sequence: seq.name,
    song: seq.song,
    hasLayout: pkg.hasLayout,
    items: structuredClone(seq.items),
    targets,
    suggestions,
    mapping,
    key: pkg.key,
    allExact: seq.items.every((i, n) => i.effects === 0 || (suggestions[n].reason === "exact" && suggestions[n].confidence >= 1)),
    music: pkg.music,
    musicFolder,
  };
}

const KINDS: EffectKind[] = ["colorWash", "bars", "spiral", "twinkle", "wave", "chase", "meteors", "shimmer"];
const COLORS = ["#ff1a1a", "#16c60c", "#ffffff", "#ffb300", "#2a6bff"];

function targetOf(show: Show, name: string): SequenceTarget | null {
  const prop = show.props.find((p) => p.name === name);
  if (prop) return { prop: prop.id };
  const group = show.groups.find((g) => g.name === name);
  if (group) return { group: group.id };
  const at = name.indexOf("/");
  const parent = at > 0 ? show.props.find((p) => p.name === name.slice(0, at)) : undefined;
  const region = parent?.regions.find((r) => r.name === name.slice(at + 1));
  return parent && region ? { region: { prop: parent.id, region: region.id } } : null;
}

/** Builds the sequence `mapping` makes of `pkg`'s sequence: each mapped item's effects on each
 * of its targets, items sharing a target layered on it in order (the first on top). */
export function importVendor(
  pkg: MemoryVendorPackage,
  show: Show,
  sequence: string,
  mapping: VendorMapping,
  musicFolder: string | null,
): { sequence: Sequence; summary: SequenceImportSummary; notes: string[] } {
  const seq = pkg.sequences.find((s) => s.name === sequence) ?? pkg.sequences[0];
  const rows: Row[] = [];
  const rowFor = new Map<string, Row>();
  const unmapped: string[] = [];
  let effects = 0;
  let skipped = 0;
  seq.items.forEach((item, n) => {
    if (item.effects === 0) return;
    const targets = (mapping.items[item.name] ?? []).map((t) => targetOf(show, t)).filter((t): t is SequenceTarget => t !== null);
    if (targets.length === 0) {
      unmapped.push(`${item.name} (${item.effects} effect${item.effects === 1 ? "" : "s"})`);
      skipped += item.effects;
      return;
    }
    const length = Math.max(50, Math.floor(seq.durationMs / item.effects / 50) * 50);
    for (const target of targets) {
      const layer: Effect[] = Array.from({ length: item.effects }, (_, i): Effect => ({
        id: crypto.randomUUID(),
        startMs: i * length,
        endMs: Math.min(seq.durationMs, (i + 1) * length),
        params: { kind: KINDS[(n + i) % KINDS.length] } as Effect["params"],
        palette: { colors: [COLORS[(n + i) % COLORS.length], COLORS[(n + i + 2) % COLORS.length]] },
        blend: "normal",
        fadeInMs: 0,
        fadeOutMs: 0,
      })).filter((e) => e.startMs < e.endMs);
      effects += layer.length;
      const key = JSON.stringify(target);
      const row = rowFor.get(key);
      if (row) row.layers.unshift({ effects: layer });
      else {
        const made: Row = { id: crypto.randomUUID(), target, layers: [{ effects: layer }] };
        rowFor.set(key, made);
        rows.push(made);
      }
    }
  });
  const notes: string[] = [];
  if (unmapped.length) notes.push(`These xLights models weren't mapped to anything, so their effects weren't imported: ${unmapped.join(", ")}.`);
  let audio: string | null = null;
  if (pkg.music && musicFolder) audio = `${musicFolder}/${pkg.music}`;
  else if (pkg.music) notes.unshift(`The package's music (${pkg.music}) wasn't saved, because there's no folder for it yet. Save the show, then import again, or choose the music in the sequence's settings.`);
  return {
    sequence: { schemaVersion: 1, name: seq.song, audio, durationMs: seq.durationMs, frameMs: 25, timingTracks: [], rows },
    summary: { rows: rows.length, effects, exact: effects, approximate: 0, placeholders: 0, skipped, timingTracks: 0, marks: 0, lyricMarks: 0, marksSkipped: 0 },
    notes,
  };
}
