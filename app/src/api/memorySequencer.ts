// An in-memory stand-in for the sequencer side of the engine, for tests and the plain-browser demo.
// It applies sequence edits with undo/redo and gesture merging like the engine, answers with the
// same light SequenceEditResult replies, and checks effect settings against the engine's catalog
// (effectCatalog.json, kept identical to the Rust table by a test in the desktop shell). It doesn't
// render, play, or check the sequence against the show.

import catalogJson from "./effectCatalog.json";
import type { MemoryBackend } from "./memory";
import { renderSequenceFrame } from "./memoryRender";
import type { PlaybackStatus, ShowSnapshot } from "./types";
import {
  noChanges,
  type Analysis,
  type EffectInfo,
  type ExportLayout,
  type ExportProgress,
  type ExportSummary,
  type Row,
  type Sequence,
  type SequenceChanges,
  type SequenceEdit,
  type SequenceEditResult,
  type SequenceSnapshot,
  type TimingTrack,
} from "./sequence";
import type { SequencerApi } from "./sequencer";

/** The engine's effect catalog (a copy of the Rust table). */
export const EFFECT_CATALOG = catalogJson as unknown as EffectInfo[];

const NO_SEQUENCE = "No sequence is open. Create or open one first.";

function fail(message: string): never {
  throw new Error(message);
}

/** Like the engine's format_ms: 1:02.500, or 1:02:03.000 past an hour. */
export function formatMs(ms: number): string {
  const h = Math.floor(ms / 3_600_000);
  const m = Math.floor((ms % 3_600_000) / 60_000);
  const s = Math.floor((ms % 60_000) / 1000);
  const milli = String(ms % 1000).padStart(3, "0");
  const ss = String(s).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}.${milli}` : `${m}:${ss}.${milli}`;
}

/** The first effect setting outside its catalog range, in the engine's words; null when all fit. */
export function settingProblem(doc: Sequence): string | null {
  for (const row of doc.rows) {
    for (const layer of row.layers) {
      for (const effect of layer.effects) {
        const info = EFFECT_CATALOG.find((e) => e.kind === effect.params.kind);
        if (!info) continue;
        for (const setting of info.settings) {
          if (setting.type !== "number" && setting.type !== "int") continue;
          const value = (effect.params as Record<string, unknown>)[setting.key];
          if (value === undefined) continue;
          let why: string | null = null;
          if (typeof value !== "number" || !Number.isFinite(value)) {
            why = `isn't a usable number; use ${setting.min} to ${setting.max}`;
          } else if (value < setting.min || value > setting.max) {
            why = `is ${value}; use ${setting.min} to ${setting.max}`;
          }
          if (why) {
            return `The ${info.label} effect at ${formatMs(effect.startMs)} has a setting PixelFlow can't use: ${setting.label} ${why}.`;
          }
        }
      }
    }
  }
  return null;
}

function checkTiming(startMs: number, endMs: number) {
  if (endMs <= startMs) fail("An effect must end after it starts.");
}

function gone(kind: string): never {
  return fail(`That ${kind} isn't in the sequence anymore.`);
}

function findRow(doc: Sequence, id: string): Row {
  return doc.rows.find((r) => r.id === id) ?? gone("row");
}

function locate(doc: Sequence, id: string): [number, number, number] | null {
  for (let r = 0; r < doc.rows.length; r++) {
    const layers = doc.rows[r].layers;
    for (let l = 0; l < layers.length; l++) {
      const e = layers[l].effects.findIndex((x) => x.id === id);
      if (e >= 0) return [r, l, e];
    }
  }
  return null;
}

function layerOf(row: Row, layer: number) {
  if (layer === row.layers.length) row.layers.push({ effects: [] });
  return row.layers[layer] ?? fail(`That row has no layer ${layer + 1}.`);
}

/** Applies one edit to `doc` in place, like the engine's SequenceEdit::apply. */
export function applySequenceEdit(doc: Sequence, edit: SequenceEdit) {
  switch (edit.type) {
    case "updateInfo":
      Object.assign(doc, { name: edit.name, audio: edit.audio, durationMs: edit.durationMs, frameMs: edit.frameMs });
      return;
    case "addRow": {
      if (doc.rows.some((r) => r.id === edit.row.id)) fail("A row with that id already exists.");
      const at = Math.min(edit.index ?? doc.rows.length, doc.rows.length);
      doc.rows.splice(at, 0, structuredClone(edit.row));
      return;
    }
    case "removeRow":
      findRow(doc, edit.id);
      doc.rows = doc.rows.filter((r) => r.id !== edit.id);
      return;
    case "moveRow": {
      const row = findRow(doc, edit.id);
      doc.rows = doc.rows.filter((r) => r.id !== edit.id);
      doc.rows.splice(Math.min(edit.index, doc.rows.length), 0, row);
      return;
    }
    case "addLayer": {
      const row = findRow(doc, edit.row);
      row.layers.splice(Math.min(edit.index ?? row.layers.length, row.layers.length), 0, { effects: [] });
      return;
    }
    case "removeLayer": {
      const row = findRow(doc, edit.row);
      if (edit.layer >= row.layers.length) fail(`That row has no layer ${edit.layer + 1}.`);
      row.layers.splice(edit.layer, 1);
      return;
    }
    case "addEffect": {
      checkTiming(edit.effect.startMs, edit.effect.endMs);
      if (locate(doc, edit.effect.id)) fail("An effect with that id already exists.");
      layerOf(findRow(doc, edit.row), edit.layer).effects.push(structuredClone(edit.effect));
      return;
    }
    case "updateEffect": {
      checkTiming(edit.effect.startMs, edit.effect.endMs);
      const [r, l, e] = locate(doc, edit.effect.id) ?? gone("effect");
      doc.rows[r].layers[l].effects[e] = structuredClone(edit.effect);
      return;
    }
    case "setEffectTiming": {
      checkTiming(edit.startMs, edit.endMs);
      const [r, l, e] = locate(doc, edit.id) ?? gone("effect");
      Object.assign(doc.rows[r].layers[l].effects[e], { startMs: edit.startMs, endMs: edit.endMs });
      return;
    }
    case "setEffectParams": {
      const [r, l, e] = locate(doc, edit.id) ?? gone("effect");
      doc.rows[r].layers[l].effects[e].params = structuredClone(edit.params);
      return;
    }
    case "moveEffect": {
      checkTiming(edit.startMs, edit.endMs);
      const target = findRow(doc, edit.row);
      if (edit.layer > target.layers.length) fail(`That row has no layer ${edit.layer + 1}.`);
      const [r, l, e] = locate(doc, edit.id) ?? gone("effect");
      const [effect] = doc.rows[r].layers[l].effects.splice(e, 1);
      layerOf(target, edit.layer).effects.push({ ...effect, startMs: edit.startMs, endMs: edit.endMs });
      return;
    }
    case "removeEffect": {
      const [r, l, e] = locate(doc, edit.id) ?? gone("effect");
      doc.rows[r].layers[l].effects.splice(e, 1);
      return;
    }
    case "addTimingTrack":
      if (doc.timingTracks.some((t) => t.id === edit.track.id)) fail("A timing track with that id already exists.");
      doc.timingTracks.push(structuredClone(edit.track));
      return;
    case "updateTimingTrack": {
      const at = doc.timingTracks.findIndex((t) => t.id === edit.track.id);
      if (at < 0) gone("timing track");
      doc.timingTracks[at] = structuredClone(edit.track);
      return;
    }
    case "removeTimingTrack":
      if (!doc.timingTracks.some((t) => t.id === edit.id)) gone("timing track");
      doc.timingTracks = doc.timingTracks.filter((t) => t.id !== edit.id);
      return;
  }
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

function listChanges<T extends { id: string }>(before: T[], after: T[]) {
  const was = new Map(before.map((x) => [x.id, x]));
  const now = new Set(after.map((x) => x.id));
  return {
    changed: after.filter((x) => !same(was.get(x.id), x)),
    removed: before.filter((x) => !now.has(x.id)).map((x) => x.id),
    order: same(
      before.map((x) => x.id),
      after.map((x) => x.id),
    )
      ? null
      : after.map((x) => x.id),
  };
}

/** What changed from `before` to `after`, as whole rows and tracks (always a valid reply). */
export function diffSequences(before: Sequence, after: Sequence): SequenceChanges {
  const changes = noChanges();
  if (
    before.name !== after.name ||
    before.audio !== after.audio ||
    before.durationMs !== after.durationMs ||
    before.frameMs !== after.frameMs
  ) {
    changes.info = { name: after.name, audio: after.audio, durationMs: after.durationMs, frameMs: after.frameMs };
  }
  const rows = listChanges(before.rows, after.rows);
  changes.rows = rows.changed;
  changes.removedRows = rows.removed;
  changes.rowOrder = rows.order;
  const tracks = listChanges<TimingTrack>(before.timingTracks, after.timingTracks);
  changes.timingTracks = tracks.changed;
  changes.removedTimingTracks = tracks.removed;
  changes.trackOrder = tracks.order;
  return changes;
}

function newSequence(name: string, durationMs: number): Sequence {
  return { schemaVersion: 1, name, audio: null, durationMs, frameMs: 25, timingTracks: [], rows: [] };
}

/** The sequencer in memory. Each instance holds one open sequence, like the engine. */
export class MemorySequencer implements SequencerApi {
  doc: Sequence | null = null;
  path: string | null = null;
  revision = 0;
  savedRevision = 0;
  undoStack: { before: Sequence; gesture: string | null }[] = [];
  redoStack: { after: Sequence; gesture: string | null }[] = [];
  /** Files "on disk", keyed by path. */
  files = new Map<string, Sequence>();
  /** What the file dialogs return. */
  nextOpenPath: string | null = null;
  nextSavePath: string | null = null;
  /** Calls made, for test assertions. */
  calls: string[] = [];
  private lastGesture: string | null = null;
  /** Changes with every new or opened document (like the engine's sequence_doc_id). */
  private docId = 0;
  private exportCancels = 0;
  /** Whether a playing sequence would go out to the controllers. */
  sendToControllers = true;
  /** How long edit, undo, and redo replies take to come back (tests of a slow engine). The edit
   * itself lands at once, as in the engine; only the answer is late. */
  replyDelayMs = 0;
  /** How long beat detection takes. */
  analysisDelayMs = 0;

  /** With a memory backend, frames are drawn (roughly) from its show and playback runs on its clock. */
  constructor(readonly backend: MemoryBackend | null = null) {}

  private async reply<T>(value: T, delayMs = this.replyDelayMs): Promise<T> {
    if (delayMs > 0) await new Promise((resolve) => setTimeout(resolve, delayMs));
    return value;
  }

  private open_(): Sequence {
    return this.doc ?? fail(NO_SEQUENCE);
  }

  private snapshot(): SequenceSnapshot {
    const sequence = this.open_();
    return {
      revision: this.revision,
      path: this.path,
      dirty: this.revision !== this.savedRevision,
      canUndo: this.undoStack.length > 0,
      canRedo: this.redoStack.length > 0,
      sequence: structuredClone(sequence),
      issues: [],
    };
  }

  private result(changes: SequenceChanges | null): SequenceEditResult {
    return {
      revision: this.revision,
      dirty: this.revision !== this.savedRevision,
      canUndo: this.undoStack.length > 0,
      canRedo: this.redoStack.length > 0,
      changed: changes !== null,
      changes: changes ?? noChanges(),
      issues: [],
    };
  }

  private replace(doc: Sequence, path: string | null) {
    this.docId++;
    this.doc = doc;
    this.path = path;
    this.revision++;
    this.savedRevision = this.revision;
    this.undoStack = [];
    this.redoStack = [];
    this.lastGesture = null;
  }

  async newSequenceDoc(name: string, durationMs: number) {
    this.calls.push("newSequenceDoc");
    if (durationMs > 4 * 60 * 60 * 1000) fail("PixelFlow sequences can be at most 4 hours.");
    this.replace(newSequence(name, durationMs), null);
    return this.snapshot();
  }

  async openSequenceDoc(path: string) {
    this.calls.push("openSequenceDoc");
    const doc = this.files.get(path) ?? fail(`Could not read ${path}: no such file`);
    this.replace(structuredClone(doc), path);
    return this.snapshot();
  }

  async saveSequenceDoc() {
    this.open_();
    if (!this.path) fail("This sequence has not been saved yet. Choose where to save it.");
    return this.saveSequenceDocAs(this.path);
  }

  async saveSequenceDocAs(path: string) {
    this.calls.push("saveSequenceDocAs");
    this.files.set(path, structuredClone(this.open_()));
    this.path = path;
    this.savedRevision = this.revision;
    return this.snapshot();
  }

  async closeSequenceDoc() {
    this.doc = null;
    this.path = null;
  }

  async getSequenceDoc() {
    return this.doc ? this.snapshot() : null;
  }

  async editSequence(edits: SequenceEdit[], gesture?: string) {
    this.calls.push("editSequence");
    const before = this.open_();
    const next = structuredClone(before);
    for (const edit of edits) applySequenceEdit(next, edit);
    const problem = settingProblem(next);
    if (problem) fail(problem);
    if (same(before, next)) return this.result(null);
    const changes = diffSequences(before, next);
    const top = this.undoStack.at(-1);
    const merge = gesture !== undefined && gesture === this.lastGesture && top?.gesture === gesture;
    if (!merge) this.undoStack.push({ before, gesture: gesture ?? null });
    this.lastGesture = gesture ?? null;
    this.redoStack = [];
    this.doc = next;
    this.revision++;
    return this.reply(this.result(changes));
  }

  async undoSequence() {
    this.calls.push("undoSequence");
    const now = this.open_();
    const step = this.undoStack.pop();
    if (!step) return this.result(null);
    this.redoStack.push({ after: now, gesture: step.gesture });
    this.doc = step.before;
    this.lastGesture = null;
    this.revision++;
    return this.reply(this.result(diffSequences(now, step.before)));
  }

  async redoSequence() {
    this.calls.push("redoSequence");
    const now = this.open_();
    const step = this.redoStack.pop();
    if (!step) return this.result(null);
    this.undoStack.push({ before: now, gesture: step.gesture });
    this.doc = step.after;
    this.lastGesture = null;
    this.revision++;
    return this.reply(this.result(diffSequences(now, step.after)));
  }

  async effectCatalog() {
    return structuredClone(EFFECT_CATALOG);
  }

  async sequenceDocFrame(positionMs: number) {
    const doc = this.open_();
    return this.backend ? renderSequenceFrame(doc, this.backend.show, positionMs) : new Uint8Array();
  }

  async playSequenceDoc(positionMs: number): Promise<PlaybackStatus> {
    const doc = this.open_();
    this.calls.push(`playSequenceDoc@${positionMs}`);
    const backend = this.backend ?? fail("Playing an authored sequence needs the PixelFlow desktop app.");
    const live = () => this.doc ?? doc;
    return backend.playAuthored(
      {
        path: this.path ?? "",
        music: doc.audio,
        durationMs: doc.durationMs,
        frameMs: doc.frameMs,
        frame: (ms) => renderSequenceFrame(live(), backend.show, ms),
      },
      positionMs,
    );
  }

  async setSequenceDocOutput(send: boolean) {
    this.calls.push(`setSequenceDocOutput:${send}`);
    this.sendToControllers = send;
    return (await this.backend?.playbackStatus()) ?? null;
  }

  async addSequenceDocToShow(path: string): Promise<ShowSnapshot> {
    const doc = this.open_();
    this.calls.push(`addSequenceDocToShow:${path}`);
    const backend = this.backend ?? fail("Adding to the show needs a show.");
    const base = doc.name.trim() || "Sequence";
    const taken = (n: string) => backend.show.sequences.some((s) => s.name === n);
    let name = base;
    for (let n = 2; taken(name); n++) name = `${base} (${n})`;
    return backend.applyEdits([
      { type: "addSequence", sequence: { id: crypto.randomUUID(), name, path, audio: doc.audio, offsetMs: 0 } },
    ]);
  }

  async sequenceExportLayout(): Promise<ExportLayout> {
    this.open_();
    return { channels: 0, blocks: [], notes: [] };
  }

  async exportSequenceDoc(path: string, onProgress?: (progress: ExportProgress) => void): Promise<ExportSummary> {
    this.calls.push("exportSequenceDoc");
    const doc = this.open_();
    const started = this.exportCancels;
    const frames = Math.ceil(doc.durationMs / doc.frameMs);
    let last = -1;
    for (let done = 1; done <= frames; done++) {
      const percent = Math.floor((done * 100) / frames);
      if (percent !== last || done === frames) {
        last = percent;
        onProgress?.({ path, framesDone: done, frames, percent });
        // Let a cancel from the UI (or the callback) land between steps.
        await Promise.resolve();
      }
      if (this.exportCancels !== started) fail("The export was cancelled.");
    }
    return {
      frames,
      frameMs: doc.frameMs,
      durationMs: doc.durationMs,
      channels: 0,
      media: doc.audio?.split(/[\\/]/).pop() ?? null,
      blocks: [],
      notes: [],
    };
  }

  async cancelSequenceExport() {
    this.exportCancels++;
  }

  async analyzeAudio(_path: string): Promise<Analysis> {
    const durationMs = this.doc?.durationMs ?? 60_000;
    const beats = Array.from({ length: Math.floor(durationMs / 500) }, (_, i) => i * 500);
    return { durationMs, tempoBpm: 120, beats, bars: beats.filter((_, i) => i % 4 === 0), onsets: beats };
  }

  async detectBeats() {
    const doc = this.open_();
    const docId = this.docId;
    if (!doc.audio) fail("This sequence has no music yet. Choose a song for it first.");
    const analysis = await this.reply(await this.analyzeAudio(doc.audio), this.analysisDelayMs);
    if (this.docId !== docId || this.doc?.audio !== doc.audio) {
      fail("The sequence or its music changed while the beats were being found. Run beat detection again.");
    }
    const latest = this.open_();
    const marks = (times: number[], label: (i: number) => string) =>
      times.map((t, i) => ({ startMs: t, endMs: times[i + 1] ?? doc.durationMs, label: label(i) }));
    const tracks: TimingTrack[] = [
      { id: crypto.randomUUID(), name: "Beats", kind: "beats", marks: marks(analysis.beats, (i) => String((i % 4) + 1)) },
      { id: crypto.randomUUID(), name: "Bars", kind: "bars", marks: marks(analysis.bars, (i) => String(i + 1)) },
    ];
    const edits: SequenceEdit[] = [
      ...latest.timingTracks
        .filter((t) => tracks.some((n) => n.name === t.name))
        .map((t) => ({ type: "removeTimingTrack" as const, id: t.id })),
      ...tracks.map((track) => ({ type: "addTimingTrack" as const, track })),
    ];
    return this.editSequence(edits);
  }

  async pickSequenceDocPath() {
    return this.nextOpenPath;
  }

  async pickSequenceDocSavePath(_defaultName: string) {
    return this.nextSavePath;
  }

  async pickExportPath(_defaultName: string) {
    return this.nextSavePath;
  }
}
