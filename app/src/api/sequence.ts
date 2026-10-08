// TypeScript mirrors of authored sequences (see crates/pf-sequence, crates/pf-render's export,
// crates/pf-analysis, and crates/pf-engine's SequenceEdit), plus helpers that build valid values.

import type { Show, Uuid } from "./types";

/** A color as `#rrggbb`. */
export type Rgb = string;

export type EffectKind =
  | "on"
  | "off"
  | "colorWash"
  | "fade"
  | "chase"
  | "bars"
  | "wave"
  | "twinkle"
  | "shimmer"
  | "strobe"
  | "spiral"
  | "fire"
  | "meteors"
  | "ripple"
  | "faces";

export type Gradient = "none" | "horizontal" | "vertical";
export type Direction = "forward" | "reverse";
export type Axis = "horizontal" | "vertical";
/** How an effect mixes with the layers below it (see `Blend` in crates/pf-sequence/src/effect.rs). */
export type Blend =
  | "normal"
  | "add"
  | "subtract"
  | "max"
  | "min"
  | "multiply"
  | "average"
  | "over"
  | "behind"
  | "mask"
  | "reveal"
  | "revealBrightness"
  | "cutOut"
  | "clip"
  | "clipBrightness"
  | "shadow"
  | "shadowBelow"
  | "highlight"
  | "highlightAdd"
  | "bottomHalf"
  | "leftHalf";

/** Settings for each kind of effect; missing settings take the engine's defaults. */
export type EffectParams =
  | { kind: "on"; gradient?: Gradient; startLevel?: number; endLevel?: number }
  | { kind: "off" }
  | { kind: "colorWash"; cycles?: number; gradient?: Gradient }
  | { kind: "fade"; direction?: "in" | "out" }
  | { kind: "chase"; speed?: number; width?: number; bands?: number; direction?: Direction; bounce?: boolean }
  | { kind: "bars"; count?: number; speed?: number; axis?: Axis; direction?: Direction }
  | { kind: "wave"; cycles?: number; speed?: number; height?: number; thickness?: number; direction?: Direction }
  | { kind: "twinkle"; density?: number; rate?: number }
  | { kind: "shimmer"; rate?: number; duty?: number }
  | { kind: "strobe"; rate?: number; density?: number }
  | { kind: "spiral"; count?: number; speed?: number; thickness?: number; twist?: number; direction?: Direction }
  | { kind: "fire"; height?: number; sparks?: number }
  | { kind: "meteors"; count?: number; speed?: number; length?: number; direction?: "down" | "up" | "left" | "right" }
  | { kind: "ripple"; speed?: number; spacing?: number; thickness?: number }
  | {
      kind: "faces";
      /** One of the prop's faces by name; blank: its first face. */
      face?: string;
      timingTrack?: Uuid | null;
      eyes?: "open" | "auto" | "closed";
      colors?: "face" | "palette";
      outline?: boolean;
    };

export interface Palette {
  colors: Rgb[];
}

export interface Effect {
  id: Uuid;
  startMs: number;
  /** Exclusive. */
  endMs: number;
  params: EffectParams;
  palette: Palette;
  blend: Blend;
  fadeInMs: number;
  fadeOutMs: number;
  /** Sparkles on the lit pixels, 0 (none, when missing) to 200 (most). */
  sparkles?: number;
  /** The sparkles' color (white when missing). */
  sparkleColor?: Rgb;
  /** Softening, 0 (none, when missing) to 14. */
  blur?: number;
}

/**
 * Layers draw bottom (first, index 0) to top (last): the opposite of xLights, where layer 1 is on
 * top. An effect's `blend` mixes only with the layers below it on the same row (the lowest effect
 * drawn covers, whatever its blend); rows don't blend with each other — a later row covers an earlier one where it's lit.
 */
export interface Layer {
  effects: Effect[];
}

export type SequenceTarget = { prop: Uuid } | { group: Uuid } | { region: { prop: Uuid; region: Uuid } };

export interface Row {
  id: Uuid;
  target: SequenceTarget;
  layers: Layer[];
}

export type TimingKind = "beats" | "bars" | "sections" | "lyrics" | "words" | "phonemes" | "custom";

export interface Mark {
  startMs: number;
  endMs: number;
  label: string;
}

export interface TimingTrack {
  id: Uuid;
  name: string;
  kind: TimingKind;
  marks: Mark[];
}

/** An authored sequence (`*.pfseq.json`). Later rows draw over earlier ones. */
export interface Sequence {
  schemaVersion: number;
  name: string;
  audio: string | null;
  durationMs: number;
  frameMs: number;
  timingTracks: TimingTrack[];
  rows: Row[];
}

export interface SequenceIssue {
  severity: "warning" | "error";
  message: string;
  row?: Uuid;
  effect?: Uuid;
}

/** A sequence that wasn't saved when PixelFlow last closed, kept so it can be recovered. */
export interface SequenceRecovery {
  /** Pass back to recover or discard it. */
  id: string;
  name: string;
  /** The file it was opened from or last saved to; null when it was never saved. */
  path: string | null;
  /** When it was last kept (milliseconds since 1970). */
  savedAtMs: number;
}

/** The whole open sequence: when one is opened, created, or saved, and from getSequenceDoc (resync). */
export interface SequenceSnapshot {
  revision: number;
  path: string | null;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  sequence: Sequence;
  issues: SequenceIssue[];
}

/** The sequence's name, music, length, and frame time. */
export interface SequenceInfo {
  name: string;
  audio: string | null;
  durationMs: number;
  frameMs: number;
}

/** An effect and where it now sits: `index` within `layer` of row `row`. */
export interface PlacedEffect {
  row: Uuid;
  layer: number;
  index: number;
  effect: Effect;
}

/**
 * What an edit, undo, or redo changed; bring a copy of the document up to date with
 * `applySequenceChanges`. Edits list single effects where they can; rows whose layers changed (and
 * every row an undo or redo touched) come whole in `rows`.
 */
export interface SequenceChanges {
  info: SequenceInfo | null;
  rows: Row[];
  removedRows: Uuid[];
  rowOrder: Uuid[] | null;
  effects: PlacedEffect[];
  removedEffects: Uuid[];
  timingTracks: TimingTrack[];
  removedTimingTracks: Uuid[];
  trackOrder: Uuid[] | null;
}

/** The reply to an edit, undo, or redo: what changed, not the whole document. */
export interface SequenceEditResult {
  /** Increases on every change (also when an edit merges into its gesture's undo step). */
  revision: number;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  /** False when nothing changed. */
  changed: boolean;
  changes: SequenceChanges;
  /** Problems in the whole sequence now, errors first. */
  issues: SequenceIssue[];
  /** The show's revision now: it changes when undo or redo also took back (or brought back) a show
   * change made together with this one. Absent where there's no such pairing. */
  showRevision?: number;
}

/** No changes (for building replies). */
export function noChanges(): SequenceChanges {
  return {
    info: null,
    rows: [],
    removedRows: [],
    rowOrder: null,
    effects: [],
    removedEffects: [],
    timingTracks: [],
    removedTimingTracks: [],
    trackOrder: null,
  };
}

function byOrder<T extends { id: Uuid }>(items: T[], order: Uuid[]): T[] {
  const rank = new Map(order.map((id, i) => [id, i]));
  return [...items].sort((a, b) => (rank.get(a.id) ?? Infinity) - (rank.get(b.id) ?? Infinity));
}

function upsert<T extends { id: Uuid }>(items: T[], updates: T[]): T[] {
  const out = [...items];
  for (const item of updates) {
    const at = out.findIndex((x) => x.id === item.id);
    if (at >= 0) out[at] = item;
    else out.push(item);
  }
  return out;
}

/** `doc` with `changes` applied (a new object; `doc` is left as it was). */
export function applySequenceChanges(doc: Sequence, changes: SequenceChanges): Sequence {
  const next: Sequence = { ...doc };
  if (changes.info) Object.assign(next, changes.info);
  let rows = upsert(
    doc.rows.filter((r) => !changes.removedRows.includes(r.id)),
    changes.rows,
  );
  if (changes.rowOrder) rows = byOrder(rows, changes.rowOrder);
  if (changes.effects.length > 0 || changes.removedEffects.length > 0) {
    const gone = new Set([...changes.removedEffects, ...changes.effects.map((p) => p.effect.id)]);
    const whole = new Set(changes.rows.map((r) => r.id));
    const placed = [...changes.effects].sort((a, b) => a.index - b.index);
    rows = rows.map((row) => {
      const mine = placed.filter((p) => p.row === row.id);
      if (whole.has(row.id) || (mine.length === 0 && !row.layers.some((l) => l.effects.some((e) => gone.has(e.id))))) {
        return row;
      }
      const layers = row.layers.map((layer) => ({ effects: layer.effects.filter((e) => !gone.has(e.id)) }));
      for (const p of mine) layers[p.layer].effects.splice(p.index, 0, p.effect);
      return { ...row, layers };
    });
  }
  let tracks = upsert(
    doc.timingTracks.filter((t) => !changes.removedTimingTracks.includes(t.id)),
    changes.timingTracks,
  );
  if (changes.trackOrder) tracks = byOrder(tracks, changes.trackOrder);
  next.rows = rows;
  next.timingTracks = tracks;
  return next;
}

/** One choice in a list setting. */
export interface ChoiceOption {
  value: string;
  label: string;
}

interface SettingBase {
  /** The key in the effect's `params`. */
  key: string;
  label: string;
  description: string;
}

/** One effect setting, with the control that edits it (`type`), its range, and its default. */
export type EffectSetting = SettingBase &
  (
    | { type: "number"; min: number; max: number; step: number; default: number; unit?: string }
    | { type: "int"; min: number; max: number; step: number; default: number; unit?: string }
    | { type: "bool"; default: boolean }
    | { type: "choice"; default: string; options: ChoiceOption[] }
    /** One of the row's prop's faces, by name ("" = its first face). */
    | { type: "face"; default: string }
    /** One of the sequence's timing tracks, by id. */
    | { type: "timingTrack"; default: Uuid | null }
  );

/**
 * An effect kind for the settings panel, from the engine's effect catalog: the same table the
 * engine clamps files to and checks edits against, so a value inside these ranges is accepted.
 */
export interface EffectInfo {
  kind: EffectKind;
  label: string;
  description: string;
  settings: EffectSetting[];
}

/** Every setting of `info` at its default, as effect params. */
export function defaultParams(info: EffectInfo): EffectParams {
  const params: Record<string, unknown> = { kind: info.kind };
  for (const setting of info.settings) params[setting.key] = setting.default;
  return params as EffectParams;
}

/** How far an export has got (sent about once per percent). */
export interface ExportProgress {
  /** The file being written. */
  path: string;
  framesDone: number;
  frames: number;
  /** 0–100. */
  percent: number;
}

/** One change to the open sequence; a batch is one undo step. */
export type SequenceEdit =
  | { type: "updateInfo"; name: string; audio: string | null; durationMs: number; frameMs: number }
  | { type: "addRow"; row: Row; index?: number }
  | { type: "removeRow"; id: Uuid }
  | { type: "moveRow"; id: Uuid; index: number }
  | { type: "addLayer"; row: Uuid; index?: number }
  | { type: "removeLayer"; row: Uuid; layer: number }
  /** `layer` may be one past the top to start a new layer. */
  | { type: "addEffect"; row: Uuid; layer: number; effect: Effect }
  | { type: "updateEffect"; effect: Effect }
  | { type: "setEffectTiming"; id: Uuid; startMs: number; endMs: number }
  | { type: "setEffectParams"; id: Uuid; params: EffectParams }
  | { type: "moveEffect"; id: Uuid; row: Uuid; layer: number; startMs: number; endMs: number }
  | { type: "removeEffect"; id: Uuid }
  | { type: "addTimingTrack"; track: TimingTrack }
  | { type: "updateTimingTrack"; track: TimingTrack }
  | { type: "removeTimingTrack"; id: Uuid }
  | { type: "renameTimingTrack"; id: Uuid; name: string }
  /** Moves a timing track to `index` (clamped to the end). */
  | { type: "moveTimingTrack"; id: Uuid; index: number }
  /** Adds marks, each where it belongs in time; marks on a track never overlap. */
  | { type: "addMarks"; track: Uuid; marks: Mark[] }
  /** Replaces the mark at `index`: moves, resizes, or relabels it. */
  | { type: "setMark"; track: Uuid; index: number; mark: Mark }
  | { type: "removeMarks"; track: Uuid; indices: number[] }
  /** Splits the mark at `index` in two at `atMs`; the first part keeps the label. */
  | { type: "splitMark"; track: Uuid; index: number; atMs: number }
  /** Joins the mark at `index` with the next one (labels joined with a space). */
  | { type: "mergeMarks"; track: Uuid; index: number }
  /** A mark every `everyMs` from `fromMs` to `toMs`, replacing the marks there. */
  | { type: "generateMarks"; track: Uuid; everyMs: number; fromMs: number; toMs: number }
  /** Replaces `to`'s marks with every `every`th mark of `from` (1 = a copy). */
  | { type: "copyMarks"; from: Uuid; to: Uuid; every: number }
  /** One mark per line, spread over `fromMs..toMs` by letter count, replacing the marks there. */
  | { type: "spreadLyrics"; track: Uuid; lines: string[]; fromMs: number; toMs: number }
  /** Labels the marks at `indices` with `labels`, in order (as many of each). */
  | { type: "labelMarks"; track: Uuid; indices: number[]; labels: string[] }
  /** Breaks each phrase mark at `indices` of `track` into word marks on `words`, replacing the marks there. */
  | { type: "breakIntoWords"; track: Uuid; indices: number[]; words: Uuid };

/** What importing a timing file did: the edit's reply, the tracks added (as named in the
 * sequence), and what didn't come across. */
export interface TimingImported {
  result: SequenceEditResult;
  tracks: string[];
  notes: string[];
}

/** Where one controller's channels sit in an exported `.fseq` file. */
export interface ExportBlock {
  controller: Uuid;
  name: string;
  /** First channel, counting from 1. */
  start: number;
  count: number;
  fromSequenceChannels: boolean;
}

export interface ExportLayout {
  channels: number;
  blocks: ExportBlock[];
  notes: string[];
}

export interface ExportSummary {
  frames: number;
  frameMs: number;
  durationMs: number;
  channels: number;
  media: string | null;
  blocks: ExportBlock[];
  notes: string[];
}

/** What beat detection found in a song (times in ms). */
export interface Analysis {
  durationMs: number;
  tempoBpm: number | null;
  beats: number[];
  bars: number[];
  onsets: number[];
}

/** Every effect kind with the name people see, in menu order. */
export const EFFECT_KINDS: { kind: EffectKind; label: string }[] = [
  { kind: "on", label: "On" },
  { kind: "off", label: "Off" },
  { kind: "colorWash", label: "Color Wash" },
  { kind: "fade", label: "Fade" },
  { kind: "chase", label: "Chase" },
  { kind: "bars", label: "Bars" },
  { kind: "wave", label: "Wave" },
  { kind: "twinkle", label: "Twinkle" },
  { kind: "shimmer", label: "Shimmer" },
  { kind: "strobe", label: "Strobe" },
  { kind: "spiral", label: "Spiral" },
  { kind: "fire", label: "Fire" },
  { kind: "meteors", label: "Meteors" },
  { kind: "ripple", label: "Ripple" },
];

/** A new effect of `kind` (engine-default settings, white) from `startMs` to `endMs`. */
export function newEffect(kind: EffectKind, startMs: number, endMs: number, colors: Rgb[] = ["#ffffff"]): Effect {
  return {
    id: crypto.randomUUID(),
    startMs,
    endMs,
    params: { kind } as EffectParams,
    palette: { colors },
    blend: "normal",
    fadeInMs: 0,
    fadeOutMs: 0,
  };
}

/** A new row with one empty layer. */
export function newRow(target: SequenceTarget): Row {
  return { id: crypto.randomUUID(), target, layers: [{ effects: [] }] };
}

/** The most rows a sequence can have (the engine's MAX_ROWS, crates/pf-sequence/src/limits.rs). */
export const MAX_ROWS = 10_000;

/**
 * A row for every group (that has members) and then every prop, each in the show's (layout) order:
 * what a new sequence starts with, as in xLights. At most `limit` rows (the first ones).
 */
export function rowsForShow(show: Pick<Show, "groups" | "props">, limit = MAX_ROWS): Row[] {
  const targets: SequenceTarget[] = [
    ...show.groups.filter((g) => g.members.length > 0).map((g) => ({ group: g.id })),
    ...show.props.map((p) => ({ prop: p.id })),
  ];
  return targets.slice(0, limit).map(newRow);
}
