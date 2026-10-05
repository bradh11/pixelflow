// TypeScript mirrors of authored sequences (see crates/pf-sequence, crates/pf-render's export,
// crates/pf-analysis, and crates/pf-engine's SequenceEdit), plus helpers that build valid values.

import type { Uuid } from "./types";

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
  | "ripple";

export type Gradient = "none" | "horizontal" | "vertical";
export type Direction = "forward" | "reverse";
export type Axis = "horizontal" | "vertical";
export type Blend = "normal" | "add" | "max" | "multiply";

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
  | { kind: "ripple"; speed?: number; spacing?: number; thickness?: number };

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
}

/** Layers draw bottom (first) to top (last). */
export interface Layer {
  effects: Effect[];
}

export type SequenceTarget = { prop: Uuid } | { group: Uuid };

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

/** The open sequence after every change. */
export interface SequenceSnapshot {
  revision: number;
  path: string | null;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  sequence: Sequence;
  issues: SequenceIssue[];
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
  | { type: "removeTimingTrack"; id: Uuid };

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
